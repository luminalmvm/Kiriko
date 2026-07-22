//! The bridge v0.5 asset-property ops: a text layer's content, a solid's colour
//! and size, and a camera's zoom.
//!
//! # In plain terms
//!
//! These are the property editors the panels need for the non-transform layer
//! kinds — changing what a text layer *says* (and its size and fill colour),
//! recolouring or resizing a solid, and setting a camera's zoom. Each routes
//! through the real [`lumit_core::ops::Op`] the egui inspector commits
//! (`SetTextDocument`, `SetSolidDef`, `SetCameraZoom`), so undo is one clean
//! step. A solid edit goes through the shared `SolidDef` asset, so every layer
//! using that solid updates together — exactly as the egui frontend behaves.

use crate::err_json;
use crate::state::{commit, parse_comp_layer, Bridge};
use lumit_core::anim::Animation;
use lumit_core::model::{LayerKind, LinearColour, ProjectItem, TextDocument};
use lumit_core::ops::Op;

/// Set a text layer's document — its `text`, `size` (points) and `fill` (scene-
/// linear RGBA) — one [`Op::SetTextDocument`]. Only a text layer accepts this;
/// anything else is a calm error. The layer's `extra` map on the document is
/// preserved.
#[allow(clippy::too_many_arguments)]
pub(crate) fn set_text_content(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    text: &str,
    size: f64,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
) -> String {
    let ctx = "set text content";
    let (comp, layer) = match parse_comp_layer(comp_id, layer_id) {
        Ok(pair) => pair,
        Err(e) => return err_json(format!("{ctx}: {e}")),
    };
    let doc = bridge.store.snapshot();
    let Some(c) = doc.comp(comp) else {
        return err_json(format!("{ctx}: unknown composition"));
    };
    let Some(l) = c.layers.iter().find(|l| l.id == layer) else {
        return err_json(format!("{ctx}: unknown layer"));
    };
    let LayerKind::Text { document } = &l.kind else {
        return err_json(format!("{ctx}: this is not a text layer"));
    };
    let document = TextDocument {
        text: text.to_owned(),
        size,
        fill: LinearColour([r as f32, g as f32, b as f32, a as f32]),
        extra: document.extra.clone(),
    };
    commit(
        bridge,
        Op::SetTextDocument {
            comp,
            layer,
            document,
        },
        ctx,
    )
}

/// Recolour and resize a solid layer's backing `SolidDef` asset — one
/// [`Op::SetSolidDef`]. `colour` is scene-linear RGBA; width/height clamp to
/// 16..16384 (the comp-size range). Every layer using this solid updates. Only
/// a solid layer accepts this; anything else is a calm error.
#[allow(clippy::too_many_arguments)]
pub(crate) fn set_solid(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
    width: u32,
    height: u32,
) -> String {
    let ctx = "set solid";
    let (comp, layer) = match parse_comp_layer(comp_id, layer_id) {
        Ok(pair) => pair,
        Err(e) => return err_json(format!("{ctx}: {e}")),
    };
    let doc = bridge.store.snapshot();
    let Some(c) = doc.comp(comp) else {
        return err_json(format!("{ctx}: unknown composition"));
    };
    let Some(l) = c.layers.iter().find(|l| l.id == layer) else {
        return err_json(format!("{ctx}: unknown layer"));
    };
    let LayerKind::Solid { def } = &l.kind else {
        return err_json(format!("{ctx}: this is not a solid layer"));
    };
    let Some(ProjectItem::Solid(solid)) = doc.item(*def) else {
        return err_json(format!("{ctx}: the solid asset is missing"));
    };
    commit(
        bridge,
        Op::SetSolidDef {
            def: *def,
            name: solid.name.clone(),
            colour: LinearColour([r as f32, g as f32, b as f32, a as f32]),
            width: width.clamp(16, 16384),
            height: height.clamp(16, 16384),
        },
        ctx,
    )
}

/// Set a camera layer's zoom to a static `zoom` (pixels, the AE model) — one
/// [`Op::SetCameraZoom`] replacing the whole animation (the coarse-grained,
/// exactly-invertible shape every property edit uses). Only a camera layer
/// accepts this; anything else is a calm error.
pub(crate) fn set_camera_zoom(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    zoom: f64,
) -> String {
    let ctx = "set camera zoom";
    let (comp, layer) = match parse_comp_layer(comp_id, layer_id) {
        Ok(pair) => pair,
        Err(e) => return err_json(format!("{ctx}: {e}")),
    };
    let doc = bridge.store.snapshot();
    let Some(c) = doc.comp(comp) else {
        return err_json(format!("{ctx}: unknown composition"));
    };
    let Some(l) = c.layers.iter().find(|l| l.id == layer) else {
        return err_json(format!("{ctx}: unknown layer"));
    };
    if !matches!(l.kind, LayerKind::Camera { .. }) {
        return err_json(format!("{ctx}: this is not a camera layer"));
    }
    commit(
        bridge,
        Op::SetCameraZoom {
            comp,
            layer,
            animation: Animation::Static(zoom),
        },
        ctx,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::edits::{add_camera_layer, add_solid_layer, add_text_layer};
    use crate::state::{new_composition, snapshot, undo};
    use serde_json::{json, Value};
    use uuid::Uuid;

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).expect("reply is valid JSON")
    }

    fn comp_id(b: &Bridge) -> String {
        b.store
            .snapshot()
            .items
            .iter()
            .find_map(|i| match i {
                ProjectItem::Composition(c) => Some(c.id),
                _ => None,
            })
            .expect("a comp exists")
            .to_string()
    }

    fn find_comp(snap: &Value) -> Value {
        for item in snap["items"].as_array().unwrap() {
            if item["kind"] == json!("composition") {
                return item.clone();
            }
            for child in item["children"].as_array().unwrap() {
                if child["kind"] == json!("composition") {
                    return child.clone();
                }
            }
        }
        panic!("no composition in snapshot");
    }

    fn first_layer_id(b: &Bridge) -> String {
        let snap = parse(&snapshot(b));
        find_comp(&snap)["comp"]["layers"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn set_text_content_replaces_the_document_and_undoes() {
        let mut b = Bridge::new();
        new_composition(&mut b, "Scene");
        let comp = comp_id(&b);
        add_text_layer(&mut b, &comp);
        let layer = first_layer_id(&b);
        set_text_content(&mut b, &comp, &layer, "Hello", 48.0, 1.0, 0.0, 0.0, 1.0);
        // Read the document back through the store.
        let doc = b.store.snapshot();
        let l = doc
            .comp(Uuid::parse_str(&comp).unwrap())
            .unwrap()
            .layers
            .iter()
            .find(|l| l.id == Uuid::parse_str(&layer).unwrap())
            .unwrap();
        let LayerKind::Text { document } = &l.kind else {
            panic!("not text");
        };
        assert_eq!(document.text, "Hello");
        assert_eq!(document.size, 48.0);
        assert_eq!(document.fill.0, [1.0, 0.0, 0.0, 1.0]);
        // Undo restores the starter "Text".
        undo(&mut b);
        let doc = b.store.snapshot();
        let l = doc
            .comp(Uuid::parse_str(&comp).unwrap())
            .unwrap()
            .layers
            .iter()
            .find(|l| l.id == Uuid::parse_str(&layer).unwrap())
            .unwrap();
        let LayerKind::Text { document } = &l.kind else {
            panic!("not text");
        };
        assert_eq!(document.text, "Text");
    }

    #[test]
    fn set_text_on_a_non_text_layer_is_a_calm_error() {
        let mut b = Bridge::new();
        new_composition(&mut b, "Scene");
        let comp = comp_id(&b);
        add_camera_layer(&mut b, &comp);
        let layer = first_layer_id(&b);
        let reply = parse(&set_text_content(
            &mut b, &comp, &layer, "x", 12.0, 1.0, 1.0, 1.0, 1.0,
        ));
        assert_eq!(reply["ok"], json!(false));
        assert!(reply["error"].as_str().unwrap().contains("not a text"));
    }

    #[test]
    fn set_solid_recolours_the_asset_and_reads_back() {
        let mut b = Bridge::new();
        new_composition(&mut b, "Scene");
        let comp = comp_id(&b);
        add_solid_layer(&mut b, &comp);
        let layer = first_layer_id(&b);
        // Use exactly-f32-representable values so the read-back compares cleanly.
        let snap = parse(&set_solid(
            &mut b, &comp, &layer, 0.25, 0.5, 0.75, 1.0, 640, 480,
        ));
        assert_eq!(snap["ok"], json!(true));
        // The layer read-back carries the solid's colour.
        assert_eq!(
            find_comp(&snap)["comp"]["layers"][0]["colour"],
            json!([0.25, 0.5, 0.75, 1.0])
        );
    }

    #[test]
    fn set_camera_zoom_round_trips() {
        let mut b = Bridge::new();
        new_composition(&mut b, "Scene");
        let comp = comp_id(&b);
        add_camera_layer(&mut b, &comp);
        let layer = first_layer_id(&b);
        let snap = parse(&set_camera_zoom(&mut b, &comp, &layer, 1234.0));
        assert_eq!(snap["ok"], json!(true));
        let doc = b.store.snapshot();
        let l = doc
            .comp(Uuid::parse_str(&comp).unwrap())
            .unwrap()
            .layers
            .iter()
            .find(|l| l.id == Uuid::parse_str(&layer).unwrap())
            .unwrap();
        let LayerKind::Camera { zoom } = &l.kind else {
            panic!("not a camera");
        };
        assert_eq!(zoom.value_at(0.0), 1234.0);
    }
}
