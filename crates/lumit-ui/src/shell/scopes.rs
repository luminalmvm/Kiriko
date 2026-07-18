//! The Scopes panel (docs/07-UI-SPEC.md §8, K-096): waveform, vectorscope and
//! histogram over the composited frame.
//!
//! In plain terms: a scope is a little instrument that plots the picture's
//! brightness and colour instead of showing the picture — the same tools a
//! colourist reads a shot with. The waveform shows how bright each column is,
//! the histogram counts how many pixels sit at each brightness, and the
//! vectorscope plots colour (hue as angle, saturation as distance from the
//! centre).
//!
//! Each Scopes panel shows one scope, chosen in its header, so opening a few
//! side by side gives a colourist's bench (UI-SPEC §8). The scope is drawn from
//! the composited frame Lumit banks in RAM under the playhead — the frame you
//! see in the Viewer. It reads that frame from the cache every paint and, while
//! playing, requests a repaint at the playback cadence, so the trace tracks the
//! live frame for every frame the cache holds (K-130). When a playback frame
//! isn't banked yet — one the frame budget skipped, or still rendering — the
//! scope holds the last frame it showed rather than blanking, and catches up as
//! soon as the current frame is banked. Guaranteed every-frame tracing under
//! all conditions still waits on a GPU-side scope pass (the K-096 v1 note).
//!
//! All colours come from `theme.scope`; the trace textures are built as raw
//! RGBA byte buffers so this module constructs no `Color32` of its own (the
//! no-hex-outside-theme rule, docs/15-DESIGN.md).

use super::*;
use egui::Color32;

/// Which scope one panel instance shows (chosen in its header). Carried in
/// [`Panel::Scopes`] so each pane keeps its own choice and it persists with
/// the workspace, and so two Scopes panels can show different scopes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum ScopeKind {
    /// Brightness (luma) waveform — the default.
    #[default]
    WaveformLuma,
    /// Red/green/blue waveforms overlaid.
    WaveformRgb,
    /// Chroma plotted on a circle (hue = angle, saturation = radius).
    Vectorscope,
    /// Per-channel pixel counts by brightness.
    Histogram,
}

impl ScopeKind {
    pub(crate) const ALL: [ScopeKind; 4] = [
        ScopeKind::WaveformLuma,
        ScopeKind::WaveformRgb,
        ScopeKind::Vectorscope,
        ScopeKind::Histogram,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            ScopeKind::WaveformLuma => "Waveform",
            ScopeKind::WaveformRgb => "RGB waveform",
            ScopeKind::Vectorscope => "Vectorscope",
            ScopeKind::Histogram => "Histogram",
        }
    }
}

/// The trace texture resolution (columns × value levels for waveforms, the
/// square grid for the vectorscope, bins × levels for the histogram).
const GRID: usize = 256;
/// Cap on pixels actually sampled per paint: scopes participate in graceful
/// degradation (UI-SPEC §8), and a full 1080p frame is far more than a
/// 256-wide plot resolves. Subsampling to roughly this many keeps the paint
/// cheap without changing the shape of the distribution.
const MAX_SAMPLES: usize = 240_000;

/// Which banked frame key a scope reads this paint: the frame under the
/// playhead when it is banked, otherwise the last key the pane showed (so a
/// not-yet-banked playback frame holds the last real trace rather than
/// blanking). Both candidates must still be present, tested via `is_banked`.
/// Pure, so the choice is unit-tested without a cache or a GPU.
fn shown_frame_key(
    current: Option<u128>,
    last: Option<u128>,
    is_banked: impl Fn(u128) -> bool,
) -> Option<u128> {
    match current {
        Some(k) if is_banked(k) => Some(k),
        _ => last.filter(|&k| is_banked(k)),
    }
}

/// One Scopes panel. Reads the banked composited frame for the previewed
/// comp and draws the chosen scope; the header switches the scope in place.
pub(crate) fn scopes_panel(ui: &mut egui::Ui, theme: &Theme, app: &AppState, kind: &mut ScopeKind) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        for k in ScopeKind::ALL {
            if ui.selectable_label(*kind == k, k.label()).clicked() {
                *kind = k;
            }
        }
    });
    ui.add_space(2.0);
    ui.separator();

    // The frame the Viewer is showing right now: the key under the playhead
    // (`preview_frame` advances every playback tick), read from the RAM cache
    // each paint so the scope tracks the live frame during playback wherever
    // that frame is banked — the same peek the eyedropper reads. `frame_key_for`
    // is None while a plain footage item (not a comp) is previewed.
    let current = app
        .preview_comp
        .and_then(|comp| app.frame_key_for(comp, app.preview_frame));
    // Hold the last frame this pane showed (per-pane, in egui temp memory) so a
    // playback frame that isn't banked yet — one the frame budget skipped, or
    // still rendering — keeps the last real trace on screen instead of blanking
    // mid-play. A cached current frame always wins and refreshes the held key.
    let shown_id = ui.id().with("scopes-shown-frame");
    let last = ui.data(|d| d.get_temp::<u128>(shown_id));
    let shown_key = shown_frame_key(current, last, |k| app.comp_frame_cache.contains_key(&k));
    if let Some(k) = shown_key {
        ui.data_mut(|d| d.insert_temp(shown_id, k));
    }
    // Keep re-sampling while the playhead moves. `request_repaint_after` at the
    // playback cadence (rather than a bare `request_repaint`) tracks live at
    // ~60 fps without shortening the frame delay to zero, so it never busies the
    // idle-paused UI (the guard) nor spins faster than playback itself.
    if app.is_playing() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));
    }
    let Some(frame) = shown_key.and_then(|k| app.comp_frame_cache.peek(&k)) else {
        empty_hint(
            ui,
            theme,
            "No frame yet",
            "Scopes read the frame shown in the Viewer. Open a composition and \
             scrub to a frame.",
        );
        return;
    };

    let avail = ui.available_size();
    let (rect, _) = ui.allocate_exact_size(avail, egui::Sense::hover());
    match *kind {
        ScopeKind::WaveformLuma => {
            let img = waveform_image(
                &frame.rgba,
                frame.width,
                frame.height,
                WaveMode::Luma,
                theme,
            );
            paint_texture(ui, rect, "scope-wave-luma", img);
            paint_graticule_lines(ui, theme, rect);
        }
        ScopeKind::WaveformRgb => {
            let img = waveform_image(&frame.rgba, frame.width, frame.height, WaveMode::Rgb, theme);
            paint_texture(ui, rect, "scope-wave-rgb", img);
            paint_graticule_lines(ui, theme, rect);
        }
        ScopeKind::Vectorscope => {
            // A vectorscope reads square: fit the biggest centred square.
            let side = rect.width().min(rect.height());
            let square = egui::Rect::from_center_size(rect.center(), egui::vec2(side, side));
            let img = vectorscope_image(&frame.rgba, frame.width, frame.height, theme);
            paint_texture(ui, square, "scope-vector", img);
            paint_vectorscope_graticule(ui, theme, square);
        }
        ScopeKind::Histogram => {
            let img = histogram_image(&frame.rgba, frame.width, frame.height, theme);
            paint_texture(ui, rect, "scope-histogram", img);
            paint_graticule_lines(ui, theme, rect);
        }
    }
}

// ---------------------------------------------------------------------------
// Sampling + pure counting (unit-tested).
// ---------------------------------------------------------------------------

/// Pixel strides (x, y) that keep the sampled pixel count near [`MAX_SAMPLES`].
fn strides(width: usize, height: usize) -> (usize, usize) {
    let total = width.saturating_mul(height).max(1);
    if total <= MAX_SAMPLES {
        return (1, 1);
    }
    // Scale both axes by the same factor so coverage stays even.
    let factor = ((total as f64 / MAX_SAMPLES as f64).sqrt()).max(1.0);
    let s = factor.ceil() as usize;
    (s.max(1), s.max(1))
}

/// Rec.709 luma of an sRGB (gamma) pixel, 0..=255 → 0.0..=1.0. Scopes read the
/// displayed (gamma-encoded) signal, as video scopes do, so no linearisation.
fn luma8(r: u8, g: u8, b: u8) -> f32 {
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
}

/// Which channels a waveform plots.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WaveMode {
    Luma,
    Rgb,
}

/// Column-vs-value counts for a waveform. Returned as `GRID` rows of `GRID`
/// columns, row 0 = brightest (top). One grid per plotted channel: `[luma]`
/// for [`WaveMode::Luma`], `[r, g, b]` for [`WaveMode::Rgb`].
fn waveform_counts(rgba: &[u8], width: usize, height: usize, mode: WaveMode) -> Vec<Vec<u32>> {
    let channels = if mode == WaveMode::Rgb { 3 } else { 1 };
    let mut grids = vec![vec![0u32; GRID * GRID]; channels];
    if width == 0 || height == 0 || rgba.len() < width * height * 4 {
        return grids;
    }
    let (sx, sy) = strides(width, height);
    let mut y = 0;
    while y < height {
        let mut x = 0;
        while x < width {
            let i = (y * width + x) * 4;
            let (r, g, b) = (rgba[i], rgba[i + 1], rgba[i + 2]);
            let bx = (x * GRID / width).min(GRID - 1);
            match mode {
                WaveMode::Luma => {
                    let v = luma8(r, g, b);
                    let by = value_row(v);
                    grids[0][by * GRID + bx] += 1;
                }
                WaveMode::Rgb => {
                    for (c, ch) in [r, g, b].into_iter().enumerate() {
                        let by = value_row(ch as f32 / 255.0);
                        grids[c][by * GRID + bx] += 1;
                    }
                }
            }
            x += sx;
        }
        y += sy;
    }
    grids
}

/// Map a 0..=1 value to a grid row: 1.0 (bright) at the top (row 0), 0.0 at
/// the bottom.
fn value_row(v: f32) -> usize {
    let clamped = v.clamp(0.0, 1.0);
    (((1.0 - clamped) * (GRID as f32 - 1.0)).round() as usize).min(GRID - 1)
}

/// Per-channel brightness counts: `[r, g, b]`, each `GRID` bins.
fn histogram_counts(rgba: &[u8], width: usize, height: usize) -> [Vec<u32>; 3] {
    let mut bins = [vec![0u32; GRID], vec![0u32; GRID], vec![0u32; GRID]];
    if width == 0 || height == 0 || rgba.len() < width * height * 4 {
        return bins;
    }
    let (sx, sy) = strides(width, height);
    let mut y = 0;
    while y < height {
        let mut x = 0;
        while x < width {
            let i = (y * width + x) * 4;
            for (channel, bin_row) in bins.iter_mut().enumerate() {
                let bin = (rgba[i + channel] as usize * (GRID - 1)) / 255;
                bin_row[bin] += 1;
            }
            x += sx;
        }
        y += sy;
    }
    bins
}

/// Chroma counts on the vectorscope's square grid (row 0 = top). Uses Rec.601
/// Cb/Cr, the broadcast vectorscope's axes, centred at the grid's middle.
fn vectorscope_counts(rgba: &[u8], width: usize, height: usize) -> Vec<u32> {
    let mut grid = vec![0u32; GRID * GRID];
    if width == 0 || height == 0 || rgba.len() < width * height * 4 {
        return grid;
    }
    let (sx, sy) = strides(width, height);
    let centre = (GRID as f32 - 1.0) / 2.0;
    // Cb/Cr span roughly -0.5..0.5; leave a margin so full-saturation points
    // land inside the grid, not on its edge.
    let scale = (GRID as f32) * 0.9;
    let mut y = 0;
    while y < height {
        let mut x = 0;
        while x < width {
            let i = (y * width + x) * 4;
            let (r, g, b) = (
                rgba[i] as f32 / 255.0,
                rgba[i + 1] as f32 / 255.0,
                rgba[i + 2] as f32 / 255.0,
            );
            let cb = -0.168_736 * r - 0.331_264 * g + 0.5 * b;
            let cr = 0.5 * r - 0.418_688 * g - 0.081_312 * b;
            // Screen y grows downward; Cr up, so negate it.
            let px = centre + cb * scale;
            let py = centre - cr * scale;
            if px >= 0.0 && px < GRID as f32 && py >= 0.0 && py < GRID as f32 {
                grid[py as usize * GRID + px as usize] += 1;
            }
            x += sx;
        }
        y += sy;
    }
    grid
}

// ---------------------------------------------------------------------------
// Colourising (counts → RGBA texture) + painting.
// ---------------------------------------------------------------------------

/// A soft, saturating map from a count to a 0..=1 trace intensity. The square
/// root lifts faint traces into view without blowing out the dense ones — the
/// filmic falloff a hardware scope's phosphor gives.
fn intensity(count: u32, peak: u32) -> f32 {
    if peak == 0 {
        return 0.0;
    }
    (count as f32 / peak as f32).sqrt().min(1.0)
}

/// Start an opaque RGBA buffer filled with the scope backdrop.
fn backdrop(theme: &Theme) -> Vec<u8> {
    let bg = theme.scope.bg;
    let mut buf = vec![0u8; GRID * GRID * 4];
    for px in buf.chunks_exact_mut(4) {
        px[0] = bg.r();
        px[1] = bg.g();
        px[2] = bg.b();
        px[3] = 0xff;
    }
    buf
}

/// Add `frac` of a trace colour onto one pixel, clamped — additive over the
/// backdrop so overlapping channels brighten toward white, like a real scope.
fn add_trace(px: &mut [u8], colour: Color32, frac: f32) {
    let f = frac.clamp(0.0, 1.0);
    for (c, chan) in [colour.r(), colour.g(), colour.b()].into_iter().enumerate() {
        let add = (chan as f32 * f) as u32;
        px[c] = (px[c] as u32 + add).min(255) as u8;
    }
}

fn waveform_image(
    rgba: &[u8],
    width: u32,
    height: u32,
    mode: WaveMode,
    theme: &Theme,
) -> egui::ColorImage {
    let grids = waveform_counts(rgba, width as usize, height as usize, mode);
    let colours = match mode {
        WaveMode::Luma => vec![theme.scope.trace],
        WaveMode::Rgb => vec![theme.scope.red, theme.scope.green, theme.scope.blue],
    };
    let peak = grids
        .iter()
        .flat_map(|g| g.iter())
        .copied()
        .max()
        .unwrap_or(0);
    let mut buf = backdrop(theme);
    for cell in 0..GRID * GRID {
        let px = &mut buf[cell * 4..cell * 4 + 4];
        for (grid, colour) in grids.iter().zip(&colours) {
            let f = intensity(grid[cell], peak);
            if f > 0.0 {
                add_trace(px, *colour, f);
            }
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([GRID, GRID], &buf)
}

fn histogram_image(rgba: &[u8], width: u32, height: u32, theme: &Theme) -> egui::ColorImage {
    let bins = histogram_counts(rgba, width as usize, height as usize);
    let colours = [theme.scope.red, theme.scope.green, theme.scope.blue];
    let peak = bins
        .iter()
        .flat_map(|b| b.iter())
        .copied()
        .max()
        .unwrap_or(0);
    let mut buf = backdrop(theme);
    for (chan, colour) in bins.iter().zip(colours) {
        for (bin, &count) in chan.iter().enumerate() {
            // Column height ∝ count; fill from the bottom up.
            let h = (intensity(count, peak) * (GRID as f32 - 1.0)).round() as usize;
            for row in (GRID - 1).saturating_sub(h)..GRID {
                let cell = row * GRID + bin;
                add_trace(&mut buf[cell * 4..cell * 4 + 4], colour, 0.7);
            }
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([GRID, GRID], &buf)
}

fn vectorscope_image(rgba: &[u8], width: u32, height: u32, theme: &Theme) -> egui::ColorImage {
    let grid = vectorscope_counts(rgba, width as usize, height as usize);
    let peak = grid.iter().copied().max().unwrap_or(0);
    let mut buf = backdrop(theme);
    for cell in 0..GRID * GRID {
        let f = intensity(grid[cell], peak);
        if f > 0.0 {
            add_trace(&mut buf[cell * 4..cell * 4 + 4], theme.scope.trace, f);
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([GRID, GRID], &buf)
}

/// Upload the built image and paint it stretched to `rect`. The handle is
/// stashed in the frame's temporary memory so it outlives this call and the
/// texture is still registered when egui renders at the end of the frame.
fn paint_texture(ui: &mut egui::Ui, rect: egui::Rect, name: &str, image: egui::ColorImage) {
    let handle = ui
        .ctx()
        .load_texture(name, image, egui::TextureOptions::LINEAR);
    let id = handle.id();
    ui.memory_mut(|m| m.data.insert_temp(egui::Id::new(name), handle));
    ui.painter().image(
        id,
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}

/// Faint horizontal reference lines at the quarter marks (0, 25, 50, 75,
/// 100 % — the levels a waveform/histogram is read against).
fn paint_graticule_lines(ui: &egui::Ui, theme: &Theme, rect: egui::Rect) {
    let painter = ui.painter_at(rect);
    let stroke = egui::Stroke::new(1.0_f32, theme.scope.graticule);
    for i in 0..=4 {
        let y = rect.top() + rect.height() * (i as f32 / 4.0);
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            stroke,
        );
    }
}

/// The vectorscope's circle and cross-hair.
fn paint_vectorscope_graticule(ui: &egui::Ui, theme: &Theme, rect: egui::Rect) {
    let painter = ui.painter_at(rect);
    let stroke = egui::Stroke::new(1.0_f32, theme.scope.graticule);
    let centre = rect.center();
    let radius = rect.width().min(rect.height()) * 0.45;
    painter.circle_stroke(centre, radius, stroke);
    painter.line_segment(
        [
            egui::pos2(centre.x - radius, centre.y),
            egui::pos2(centre.x + radius, centre.y),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(centre.x, centre.y - radius),
            egui::pos2(centre.x, centre.y + radius),
        ],
        stroke,
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// A `w`×`h` frame of one solid colour, opaque.
    fn solid(w: usize, h: usize, r: u8, g: u8, b: u8) -> Vec<u8> {
        let mut v = Vec::with_capacity(w * h * 4);
        for _ in 0..w * h {
            v.extend_from_slice(&[r, g, b, 0xff]);
        }
        v
    }

    #[test]
    fn shown_frame_key_prefers_current_then_holds_last() {
        let banked = |k: u128| k == 10 || k == 20; // frames 10 and 20 are cached
                                                   // Current frame cached: it wins, whatever the held key was.
        assert_eq!(shown_frame_key(Some(10), Some(20), banked), Some(10));
        // Current frame not banked yet: hold the last shown frame instead.
        assert_eq!(shown_frame_key(Some(30), Some(20), banked), Some(20));
        // Current unkeyable (footage preview): still hold the last shown frame.
        assert_eq!(shown_frame_key(None, Some(10), banked), Some(10));
        // The held key was evicted (no longer banked): show nothing, don't blank
        // onto a dangling key.
        assert_eq!(shown_frame_key(Some(30), Some(40), banked), None);
        // Nothing to show at all.
        assert_eq!(shown_frame_key(None, None, banked), None);
    }

    #[test]
    fn value_row_maps_bright_to_top_dark_to_bottom() {
        assert_eq!(value_row(1.0), 0);
        assert_eq!(value_row(0.0), GRID - 1);
        assert!(value_row(0.5) > 0 && value_row(0.5) < GRID - 1);
    }

    #[test]
    fn a_solid_grey_makes_one_waveform_row() {
        // Mid-grey everywhere: every column's luma lands on the same row, and
        // that row holds the whole frame's sample count.
        let frame = solid(16, 16, 128, 128, 128);
        let grids = waveform_counts(&frame, 16, 16, WaveMode::Luma);
        let row = value_row(luma8(128, 128, 128));
        let in_row: u32 = (0..GRID).map(|x| grids[0][row * GRID + x]).sum();
        let total: u32 = grids[0].iter().sum();
        assert_eq!(in_row, total, "all energy sits on the grey's own row");
        assert_eq!(total, 16 * 16);
    }

    #[test]
    fn histogram_puts_a_solid_in_one_bin_per_channel() {
        let frame = solid(10, 10, 255, 0, 64);
        let bins = histogram_counts(&frame, 10, 10);
        // Red maxed → top bin; green zero → bottom bin.
        assert_eq!(bins[0][GRID - 1], 100);
        assert_eq!(bins[1][0], 100);
        // Every channel counts every sampled pixel exactly once.
        for chan in &bins {
            assert_eq!(chan.iter().sum::<u32>(), 100);
        }
    }

    #[test]
    fn neutral_grey_sits_at_the_vectorscope_centre() {
        // A neutral colour has zero chroma, so it lands on the centre cell.
        let frame = solid(8, 8, 128, 128, 128);
        let grid = vectorscope_counts(&frame, 8, 8);
        let mid = (GRID - 1) / 2;
        let peak_cell = (0..GRID * GRID).max_by_key(|&c| grid[c]).unwrap();
        let (px, py) = (peak_cell % GRID, peak_cell / GRID);
        assert!(px.abs_diff(mid) <= 1 && py.abs_diff(mid) <= 1);
    }

    #[test]
    fn strides_cap_the_sample_count() {
        // Below the cap: no subsampling.
        assert_eq!(strides(100, 100), (1, 1));
        // Well above: strides bring the sampled count near the cap, not over.
        let (sx, sy) = strides(4000, 4000);
        assert!(sx > 1 && sy > 1);
        let sampled = (4000usize.div_ceil(sx)) * (4000usize.div_ceil(sy));
        assert!(sampled <= MAX_SAMPLES, "sampled {sampled} exceeds cap");
    }

    #[test]
    fn empty_or_short_buffers_do_not_panic() {
        assert!(waveform_counts(&[], 0, 0, WaveMode::Luma)[0]
            .iter()
            .all(|&c| c == 0));
        assert!(histogram_counts(&[1, 2, 3], 4, 4)[0]
            .iter()
            .all(|&c| c == 0));
        assert!(vectorscope_counts(&[], 2, 2).iter().all(|&c| c == 0));
    }
}
