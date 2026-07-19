//! `shell::timeline` — the Timeline panel: the comp-tab strip, the ruler
//! and lane geometry, the layer/row loop, keyframe-lane editing and the
//! bottom bar. Split out of a single large file (mechanical, no behaviour
//! change): this module keeps the `LayerMap` overlay mapping and the mod
//! declarations and glob re-exports so every existing `shell::…` path
//! still resolves. Shared shell names reach the submodules through
//! `use super::*` and these glob re-exports.

use super::*;

mod bottom_bar;
mod lane;
mod menu;
mod panel;

pub(crate) use bottom_bar::*;
pub(crate) use lane::*;
pub(crate) use menu::*;
pub(crate) use panel::*;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;

/// Footage preview: the frame fit to the surround, scrub bar, resolution picker.
#[cfg(feature = "media")]
/// The layer↔screen mapping the Viewer overlays share: the layer's evaluated
/// 2D transform at the playhead, then the view placement.
#[cfg(feature = "media")]
pub(crate) struct LayerMap {
    px: f64,
    py: f64,
    ax: f64,
    ay: f64,
    sx: f64,
    sy: f64,
    sin: f64,
    cos: f64,
    origin: egui::Pos2,
    view_scale: f32,
}

#[cfg(feature = "media")]
impl LayerMap {
    pub(crate) fn of(
        layer: &lumit_core::model::Layer,
        lt: f64,
        draw: egui::Rect,
        scale: f32,
    ) -> Self {
        let tr = &layer.transform;
        let rot = tr.rotation.value_at(lt).to_radians();
        let (sin, cos) = rot.sin_cos();
        Self {
            px: tr.position_x.value_at(lt),
            py: tr.position_y.value_at(lt),
            ax: tr.anchor_x.value_at(lt),
            ay: tr.anchor_y.value_at(lt),
            sx: (tr.scale_x.value_at(lt) / 100.0).max(1e-6),
            sy: (tr.scale_y.value_at(lt) / 100.0).max(1e-6),
            sin,
            cos,
            origin: draw.min,
            view_scale: scale,
        }
    }

    /// Layer space → screen.
    pub(crate) fn to_screen(&self, p: (f64, f64)) -> egui::Pos2 {
        let (dx, dy) = ((p.0 - self.ax) * self.sx, (p.1 - self.ay) * self.sy);
        let (rx, ry) = (dx * self.cos - dy * self.sin, dx * self.sin + dy * self.cos);
        self.origin + egui::vec2((self.px + rx) as f32, (self.py + ry) as f32) * self.view_scale
    }

    /// Screen → layer space (drag and pen positions come back through this).
    pub(crate) fn layer_of(&self, pos: egui::Pos2) -> (f64, f64) {
        let c = (pos - self.origin) / self.view_scale;
        let (dx, dy) = (f64::from(c.x) - self.px, f64::from(c.y) - self.py);
        let (rx, ry) = (
            dx * self.cos + dy * self.sin,
            -dx * self.sin + dy * self.cos,
        );
        (rx / self.sx + self.ax, ry / self.sy + self.ay)
    }
}
