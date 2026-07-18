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
    /// A keyframe is here (clicking removes it).
    Keyframe,
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
}

impl Icon {
    /// Every variant, for exhaustive iteration (tests, palettes).
    pub const ALL: [Icon; 37] = [
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
        Icon::Audio,
        Icon::Mute,
        Icon::PopOut,
        Icon::PrevKeyframe,
        Icon::NextKeyframe,
        Icon::KeyframeAdd,
        Icon::Keyframe,
        Icon::Stopwatch,
        Icon::TwirlClosed,
        Icon::TwirlOpen,
        Icon::Collapse,
        Icon::Flow,
        Icon::Cube3d,
        Icon::Magnet,
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
            Icon::Audio => "sound-high",
            Icon::Mute => "sound-off",
            Icon::PopOut => "open-new-window",
            Icon::PrevKeyframe => "nav-arrow-left",
            Icon::NextKeyframe => "nav-arrow-right",
            Icon::KeyframeAdd => "keyframe-plus",
            Icon::Keyframe => "keyframe",
            Icon::Stopwatch => "timer",
            Icon::TwirlClosed => "nav-arrow-right",
            Icon::TwirlOpen => "nav-arrow-down",
            Icon::Collapse => "flare",
            Icon::Flow => "wind",
            Icon::Cube3d => "cube",
            Icon::Magnet => "magnet",
        }
    }

    /// The glyph and font family for this icon, or None if the pack lacks it
    /// (guarded by `every_icon_resolves`, so None never happens in practice —
    /// and the UI degrades to painting nothing rather than faulting).
    fn glyph(self) -> Option<(char, &'static str)> {
        let r =
            iconflow::try_icon(Pack::Iconoir, self.name(), Style::Regular, Size::Regular).ok()?;
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
    let Some((glyph, family)) = icon.glyph() else {
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
        for icon in Icon::ALL {
            let r = iconflow::try_icon(Pack::Iconoir, icon.name(), Style::Regular, Size::Regular);
            assert!(r.is_ok(), "{icon:?} → {:?} does not resolve", icon.name());
            let r = r.unwrap();
            assert!(
                char::from_u32(r.codepoint).is_some(),
                "{icon:?} has an invalid codepoint"
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
