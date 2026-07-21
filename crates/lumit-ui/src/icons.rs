//! Lumit's icons: the Iconoir set, rendered from an embedded icon font
//! (K-085, via the `iconflow` crate; Iconoir is MIT-licensed).
//!
//! In plain terms: instead of drawing every icon by hand from lines and curves
//! (the old way), Lumit now ships a professionally drawn icon family as a
//! small font file baked into the program. Each icon is a character in that
//! font, so it stays razor-sharp at any size and always takes the exact theme
//! colour we ask for — dimming on hover, turning accent when active — just
//! like text does. Emoji are still banned: every glyph here is a real icon
//! from one consistent set, never a character we hope the user's fonts carry.
//!
//! [`install`] must run once at startup (Theme::install_fonts does this)
//! before anything paints, or the icon font family won't exist.

use egui::{Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, RichText, Vec2};
use iconflow::{Pack, Size, Style};

/// One icon. Add a variant here and its Iconoir name in [`Icon::name`]; the
/// `every_icon_resolves` test fails on any name the pack doesn't carry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    /// Selection arrow (the Select tool).
    Pointer,
    /// Pan the view (the Hand tool).
    Move,
    /// Mask/shape tool — rectangle.
    Rectangle,
    /// Mask/shape tool — ellipse.
    Ellipse,
    /// Mask/shape tool — star.
    Star,
    /// Pen (nib) tool.
    Pen,
    /// Transport: play.
    Play,
    /// Transport: pause.
    Pause,
    /// Closed padlock (aspect ratio locked).
    Lock,
    /// Open padlock (aspect ratio free).
    Unlock,
    /// Chain link (linked values, e.g. linked scale).
    Link,
    /// A link with a cross — footage whose file cannot be found (docs/07 §3.3).
    Unlink,
    /// Folder (a project folder).
    Folder,
    /// Film (the "new composition" button).
    Film,
    /// Graph editor view: an animation curve with control points.
    GraphCurve,
    /// Layer/timeline view: stacked bars.
    TimelineBars,
    /// Node graph view (the future node system).
    Nodes,
    /// Footage item: a media clip.
    Footage,
    /// Composition item.
    Comp,
    /// Solid item: a filled block of colour.
    Solid,
    /// Sequence layer: clips cut back-to-back on a row.
    Sequence,
    /// Text layer.
    Text,
    /// Camera layer.
    Camera,
    /// Layer visibility switch: an eye.
    Eye,
    /// Hidden layer: an eye, closed (the visibility switch when off).
    EyeClosed,
    /// Audible layer: a speaker.
    Audio,
    /// Muted layer: a speaker, off.
    Mute,
    /// Pop a panel out into its own window.
    PopOut,
    /// Jump to the previous keyframe.
    PrevKeyframe,
    /// Jump to the next keyframe.
    NextKeyframe,
    /// Add a keyframe at the playhead.
    KeyframeAdd,
    /// A keyframe (outline diamond) — the navigator's add button when the
    /// playhead is between keys.
    Keyframe,
    /// A keyframe, filled — shown on the navigator's add/remove button when the
    /// playhead sits on a key (note 2.4).
    KeyframeFilled,
    /// The animate toggle on a property row.
    Stopwatch,
    /// Disclosure twirl, closed (points right).
    TwirlClosed,
    /// Disclosure twirl, open (points down).
    TwirlOpen,
    /// Collapse transformations (Precomp layers): the AE-style sunburst.
    Collapse,
    /// The Flow option on footage layers (K-088): optical-flow interpolation.
    Flow,
    /// The 3D column: a cube (the header glyph over the per-layer 3D switch).
    Cube3d,
    /// Snapping toggle in the timeline's bottom bar: a magnet.
    Magnet,
    /// Sample a colour from the Viewer: an eyedropper/pipette.
    Eyedropper,
    /// Reset a control to its default (EC4): a circular refresh arrow.
    Reset,
    /// Motion blur (the comp master and the per-layer switch): an arrow with
    /// speed lines — the closest Iconoir glyph to a motion smear (owner).
    MotionBlur,
    /// The per-layer fx switch (TL2): bypasses the layer's whole effect stack.
    Fx,
}

impl Icon {
    /// Every variant, for exhaustive iteration (tests, palettes).
    pub const ALL: [Icon; 44] = [
        Icon::Pointer,
        Icon::Move,
        Icon::Rectangle,
        Icon::Ellipse,
        Icon::Star,
        Icon::Pen,
        Icon::Play,
        Icon::Pause,
        Icon::Lock,
        Icon::Unlock,
        Icon::Link,
        Icon::Unlink,
        Icon::Folder,
        Icon::Film,
        Icon::GraphCurve,
        Icon::TimelineBars,
        Icon::Nodes,
        Icon::Footage,
        Icon::Comp,
        Icon::Solid,
        Icon::Sequence,
        Icon::Text,
        Icon::Camera,
        Icon::Eye,
        Icon::EyeClosed,
        Icon::Audio,
        Icon::Mute,
        Icon::PopOut,
        Icon::PrevKeyframe,
        Icon::NextKeyframe,
        Icon::KeyframeAdd,
        Icon::Keyframe,
        Icon::KeyframeFilled,
        Icon::Stopwatch,
        Icon::TwirlClosed,
        Icon::TwirlOpen,
        Icon::Collapse,
        Icon::Flow,
        Icon::Cube3d,
        Icon::Magnet,
        Icon::Eyedropper,
        Icon::Reset,
        Icon::MotionBlur,
        Icon::Fx,
    ];

    /// The Iconoir icon this variant renders.
    fn name(self) -> &'static str {
        match self {
            Icon::Pointer => "cursor-pointer",
            Icon::Move => "drag-hand-gesture",
            Icon::Rectangle => "square",
            Icon::Ellipse => "circle",
            Icon::Star => "star",
            Icon::Pen => "design-nib",
            Icon::Play => "play",
            Icon::Pause => "pause",
            Icon::Lock => "lock",
            Icon::Unlock => "lock-slash",
            Icon::Link => "link",
            Icon::Unlink => "link-xmark",
            Icon::Folder => "folder",
            Icon::Film => "movie",
            Icon::GraphCurve => "ease-curve-control-points",
            Icon::TimelineBars => "align-left",
            Icon::Nodes => "network",
            Icon::Footage => "media-video",
            Icon::Comp => "frame",
            Icon::Solid => "fill-color",
            Icon::Sequence => "view-columns-3",
            Icon::Text => "text",
            Icon::Camera => "video-camera",
            Icon::Eye => "eye",
            Icon::EyeClosed => "eye-closed",
            Icon::Audio => "sound-high",
            Icon::Mute => "sound-off",
            Icon::PopOut => "open-new-window",
            Icon::PrevKeyframe => "nav-arrow-left",
            Icon::NextKeyframe => "nav-arrow-right",
            Icon::KeyframeAdd => "keyframe-plus",
            Icon::Keyframe => "keyframe",
            Icon::KeyframeFilled => "keyframe",
            Icon::Stopwatch => "timer",
            Icon::TwirlClosed => "nav-arrow-right",
            Icon::TwirlOpen => "nav-arrow-down",
            Icon::Collapse => "flare",
            Icon::Flow => "wind",
            Icon::Cube3d => "cube",
            Icon::Magnet => "magnet",
            Icon::Eyedropper => "color-picker",
            Icon::Reset => "refresh-double",
            // Drawn, not looked up (see `drawn`); the name is kept only so
            // debug output and the icon gallery have something to print.
            Icon::MotionBlur => "motion-blur (drawn)",
            Icon::Fx => "fx",
        }
    }

    /// The drawing style for this icon. Almost every glyph is the Regular
    /// (outline) weight; a few name a Filled variant of the same Iconoir icon so
    /// a toggle can swap outline↔filled to show state (e.g. a keyframe that is
    /// hollow between keys and solid on a key — note 2.4).
    fn style(self) -> Style {
        match self {
            Icon::KeyframeFilled => Style::Filled,
            _ => Style::Regular,
        }
    }

    /// Icons Lumit draws itself rather than taking from the pack. Iconoir has
    /// no motion-blur glyph — the nearest was a plain fast-arrow, which reads
    /// as "next", not "blur" — so this one is drawn from the owner's own
    /// artwork. Drawn icons are exempt from the pack-resolution test and have
    /// no inline-text form (see [`text`]).
    fn drawn(self) -> bool {
        matches!(self, Icon::MotionBlur)
    }

    /// The glyph and font family for this icon, or None if the pack lacks it
    /// (guarded by `every_icon_resolves`, so None never happens in practice —
    /// and the UI degrades to painting nothing rather than faulting).
    fn glyph(self) -> Option<(char, &'static str)> {
        let r = iconflow::try_icon(Pack::Iconoir, self.name(), self.style(), Size::Regular).ok()?;
        Some((char::from_u32(r.codepoint)?, r.family))
    }
}

/// Register the icon font into a set of font definitions. Must run before
/// anything paints an icon; `Theme::install_fonts` calls it at startup.
pub fn install(defs: &mut egui::FontDefinitions) {
    for font in iconflow::fonts() {
        defs.font_data.insert(
            font.family.to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(font.bytes)),
        );
        defs.families
            .entry(FontFamily::Name(font.family.into()))
            .or_default()
            .insert(0, font.family.to_owned());
    }
}

/// Paint `icon` centred in `rect` in `color`. The glyph fills the smaller side
/// of the rect (Iconoir's own padding keeps strokes off the edge). `_width` is
/// kept for call-site compatibility with the old stroke-drawn icons.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32, _width: f32) {
    if icon.drawn() {
        draw_motion_blur(painter, rect, color);
        return;
    }
    let Some((glyph, family)) = icon.glyph() else {
        return;
    };
    let size = rect.width().min(rect.height());
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::new(size, FontFamily::Name(family.into())),
        color,
    );
}

/// The icon as inline rich text at `size` px, for `egui::Button`/`ui.label`.
/// No colour is set, so the widget's own state colouring applies (disabled
/// buttons dim their icon like they dim their text).
pub fn text(icon: Icon, size: f32) -> RichText {
    // A drawn icon has no character to typeset; callers that need one inline
    // allocate a rect and call `paint` instead (the timeline's motion-blur
    // master does exactly that).
    let Some((glyph, family)) = icon.glyph().filter(|_| !icon.drawn()) else {
        return RichText::new("");
    };
    RichText::new(glyph.to_string()).font(FontId::new(size, FontFamily::Name(family.into())))
}

/// A disclosure twirl: points right when closed, down when open.
pub fn disclosure(painter: &Painter, rect: Rect, open: bool, color: Color32) {
    let icon = if open {
        Icon::TwirlOpen
    } else {
        Icon::TwirlClosed
    };
    paint(painter, rect, icon, color, 1.0);
}

/// The animate toggle on a property row: Iconoir's timer, sized from the old
/// stopwatch's radius. `animated` state is carried by the colour the caller
/// picks (accent when animated), so the glyph itself doesn't change.
pub fn stopwatch(painter: &Painter, center: Pos2, radius: f32, _animated: bool, color: Color32) {
    let side = radius * 2.0 + 4.0;
    paint(
        painter,
        Rect::from_center_size(center, Vec2::splat(side)),
        Icon::Stopwatch,
        color,
        1.0,
    );
}

/// The motion-blur mark, drawn rather than typeset: a ring with speed lines
/// running into it, from the owner's own artwork. Authored on a 24×24 grid —
/// the SVG's coordinates are used verbatim below, so the two stay comparable
/// — and mapped onto whatever rect the caller gives, keeping it square and
/// centred. The original is a filled outline; the same shape falls out of a
/// stroked ring plus capped bars, which also keeps its weight consistent with
/// the stroked Iconoir set around it.
fn draw_motion_blur(painter: &Painter, rect: Rect, color: Color32) {
    let s = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - Vec2::splat(12.0 * s);
    let at = |x: f32, y: f32| origin + Vec2::new(x * s, y * s);
    // The artwork's bar thickness (and so the ring's stroke): 2 grid units.
    let stroke = egui::Stroke::new(2.0 * s, color);
    // The ring: outer edge 5 units from centre, inner 3 — a 2-unit stroke on
    // a 4-unit radius.
    painter.circle_stroke(at(17.0, 12.0), 4.0 * s, stroke);
    // The streaks, each running in until the ring swallows it. Two of the
    // three rows are broken by a second, shorter dash further left, which is
    // what makes the mark read as motion rather than as a plain arrow.
    for (x1, x2, y) in [
        (4.0, 14.0, 8.0),
        (10.0, 13.0, 12.0),
        (8.0, 14.0, 16.0),
        (3.0, 7.0, 12.0),
        (4.0, 5.0, 16.0),
    ] {
        painter.line_segment([at(x1, y), at(x2, y)], stroke);
    }
}

/// A small downward caret marking a control as a dropdown.
pub fn caret_down(painter: &Painter, center: Pos2, color: Color32) {
    paint(
        painter,
        Rect::from_center_size(center, Vec2::splat(9.0)),
        Icon::TwirlOpen,
        color,
        1.0,
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Every variant's Iconoir name resolves in the embedded pack — a typo'd
    /// or removed icon name fails here, not silently in the UI.
    #[test]
    fn every_icon_resolves() {
        for icon in Icon::ALL.into_iter().filter(|i| !i.drawn()) {
            let r = iconflow::try_icon(Pack::Iconoir, icon.name(), Style::Regular, Size::Regular);
            assert!(r.is_ok(), "{icon:?} → {:?} does not resolve", icon.name());
            let r = r.unwrap();
            assert!(
                char::from_u32(r.codepoint).is_some(),
                "{icon:?} has an invalid codepoint"
            );
        }
    }

    /// The drawn icons really are drawn: `paint` must put ink on the canvas
    /// for one without consulting the pack, and `text` must decline to give
    /// it a glyph (an empty RichText, not a wrong character).
    #[test]
    fn drawn_icons_paint_without_the_pack_and_have_no_text_form() {
        let ctx = egui::Context::default();
        let drawn: Vec<Icon> = Icon::ALL.into_iter().filter(|i| i.drawn()).collect();
        assert!(!drawn.is_empty(), "MotionBlur is drawn");
        for icon in drawn {
            assert!(icon.glyph().is_none() || text(icon, 12.0).text().is_empty());
            // Count what `paint` adds by bracketing it with two markers: no
            // fonts are installed here, so a glyph icon would add nothing and
            // the count would be zero.
            let added = std::cell::Cell::new(0i64);
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let painter = ui.painter().clone();
                    let before = painter.add(egui::Shape::Noop);
                    paint(
                        &painter,
                        Rect::from_min_size(Pos2::ZERO, Vec2::splat(16.0)),
                        icon,
                        Color32::WHITE,
                        1.4,
                    );
                    let after = painter.add(egui::Shape::Noop);
                    added.set(after.0 as i64 - before.0 as i64 - 1);
                });
            });
            assert!(
                added.get() >= 6,
                "{icon:?} added {} shapes — the mark is a ring plus five                  streaks, so anything less means it did not draw",
                added.get()
            );
        }
    }

    /// Every icon paints without panicking once the fonts are installed
    /// (unknown font families make egui's layouter fault, so this also guards
    /// the install path).
    #[test]
    fn every_icon_paints() {
        let ctx = egui::Context::default();
        let mut defs = egui::FontDefinitions::default();
        install(&mut defs);
        ctx.set_fonts(defs);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let painter = ui.painter().clone();
                for icon in Icon::ALL {
                    paint(
                        &painter,
                        Rect::from_min_size(Pos2::ZERO, Vec2::splat(16.0)),
                        icon,
                        Color32::WHITE,
                        1.5,
                    );
                }
            });
        });
    }
}
