//! The bridge v0.9 effect-preset ops: save a layer's whole effect stack to a
//! `.lumfx` JSON string, and load one onto a layer.
//!
//! # In plain terms
//!
//! An effect preset is the list of effects on a layer, with their settings,
//! written to a small `.lumfx` JSON document so it can be reused or shared.
//! `save_effect_preset` returns that JSON text for the Dart side to write to a
//! file (Dart owns the file dialog); `load_effect_preset` takes the JSON text a
//! file held and appends its effects — each with a *fresh* instance id (K-065),
//! so applying one preset to two layers never makes them share an instance — to
//! the target layer's stack as one undo step.
//!
//! The on-disk shape is **byte-compatible** with `crates/lumit-ui/src/preset.rs`
//! (the egui frontend's preset file): the same `{format, name, effects}` object,
//! the same `format` number, serialised with the same pretty printer. So a
//! preset saved by egui loads here and vice versa. The type is re-declared here
//! (rather than borrowed from `lumit-ui`) because presets are pure JSON over the
//! `lumit-core` model and must work in every build — including
//! `--no-default-features`, where `lumit-ui` is not even linked. A round-trip
//! test pins the two shapes together so they cannot silently drift.

use crate::edits::with_effects;
use crate::err_json;
use crate::state::Bridge;
use lumit_core::model::EffectInstance;
use serde_json::json;
use uuid::Uuid;

/// The current on-disk preset format version — kept equal to `lumit-ui`'s
/// `PRESET_FORMAT` so files interchange.
pub(crate) const PRESET_FORMAT: u32 = 1;

/// A saved effect stack, byte-compatible with `lumit_ui::preset::EffectPreset`.
/// Field order and names match exactly (`format`, `name`, `effects`), so
/// `serde_json` produces identical bytes for identical stacks.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct EffectPreset {
    format: u32,
    name: String,
    effects: Vec<EffectInstance>,
}

/// Serialise a layer's whole effect stack to the `.lumfx` JSON text (v0.9). The
/// reply carries the text as `preset`; the Dart side writes it to a file it
/// picked. `name` is the preset's display name (the egui frontend uses the file
/// stem or a typed name). A layer with no effects still saves — an empty preset
/// is a valid, if unexciting, file.
pub(crate) fn save_effect_preset(
    bridge: &Bridge,
    comp_id: &str,
    layer_id: &str,
    name: &str,
) -> String {
    let ctx = "save effect preset";
    let effects = match layer_effects(bridge, comp_id, layer_id, ctx) {
        Ok(e) => e,
        Err(e) => return err_json(e),
    };
    let preset = EffectPreset {
        format: PRESET_FORMAT,
        name: name.to_owned(),
        effects,
    };
    match serde_json::to_string_pretty(&preset) {
        Ok(text) => json!({ "ok": true, "preset": text }).to_string(),
        Err(e) => err_json(format!("{ctx}: {e}")),
    }
}

/// Load a `.lumfx` preset (its JSON `text`, read from a file by the Dart side)
/// onto a layer, appending its effects — each with a fresh instance id (K-065) —
/// to the layer's stack as one undo step (v0.9). A malformed document is a calm
/// error; a newer `format` still loads (unknown fields ride along in each
/// effect's `extra` map, matching how the project file tolerates additions).
pub(crate) fn load_effect_preset(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    text: &str,
) -> String {
    let ctx = "load effect preset";
    let preset: EffectPreset = match serde_json::from_str(text) {
        Ok(p) => p,
        Err(e) => return err_json(format!("{ctx}: not a valid preset — {e}")),
    };
    // Fresh instance ids so applying one preset to several layers never shares
    // an instance id (ids are instance identity only; they never feed a cache
    // key). Mirrors `lumit_ui::preset::instantiated`.
    let fresh: Vec<EffectInstance> = preset
        .effects
        .into_iter()
        .map(|mut e| {
            e.id = Uuid::now_v7();
            e
        })
        .collect();
    with_effects(bridge, comp_id, layer_id, ctx, move |effects| {
        effects.extend(fresh);
        Ok(())
    })
}

/// Read a layer's effect stack (cloned), resolving the comp and layer with a
/// calm message on any miss.
fn layer_effects(
    bridge: &Bridge,
    comp_id: &str,
    layer_id: &str,
    ctx: &str,
) -> Result<Vec<EffectInstance>, String> {
    let (comp, layer) =
        crate::state::parse_comp_layer(comp_id, layer_id).map_err(|e| format!("{ctx}: {e}"))?;
    let doc = bridge.store.snapshot();
    let c = doc
        .comp(comp)
        .ok_or_else(|| format!("{ctx}: unknown composition"))?;
    let l = c
        .layers
        .iter()
        .find(|l| l.id == layer)
        .ok_or_else(|| format!("{ctx}: unknown layer"))?;
    Ok(l.effects.clone())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::state::new_composition;
    use lumit_core::model::ProjectItem;
    use serde_json::Value;

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).expect("reply is valid JSON")
    }

    /// A bridge with one comp holding a single footage layer carrying two
    /// effects (one with an edited param), returning the bridge, comp id and
    /// layer id.
    fn bridge_with_two_effects() -> (Bridge, String, String) {
        let mut b = Bridge::new();
        new_composition(&mut b, "Scene");
        let comp = b
            .store
            .snapshot()
            .items
            .iter()
            .find_map(|i| match i {
                ProjectItem::Composition(c) => Some(c.id),
                _ => None,
            })
            .expect("a comp exists");
        // Add a footage layer to carry effects.
        let footage = crate::state::import_footage(&mut b, "/media/clip.mp4");
        let _ = footage;
        let item = b
            .store
            .snapshot()
            .items
            .iter()
            .find_map(|i| match i {
                ProjectItem::Footage(f) => Some(f.id),
                _ => None,
            })
            .expect("a footage item exists");
        // Place the footage as a layer.
        crate::edits::add_footage_layer(&mut b, &comp.to_string(), &item.to_string());
        let layer = b
            .store
            .snapshot()
            .comp(comp)
            .unwrap()
            .layers
            .first()
            .unwrap()
            .id;
        let (comp_s, layer_s) = (comp.to_string(), layer.to_string());
        // Add two effects to the layer.
        crate::edits::add_effect(&mut b, &comp_s, &layer_s, "blur");
        crate::edits::add_effect(&mut b, &comp_s, &layer_s, "glow");
        (b, comp_s, layer_s)
    }

    fn layer_effect_count(b: &Bridge, comp: &str, layer: &str) -> usize {
        let doc = b.store.snapshot();
        let c = doc.comp(Uuid::parse_str(comp).unwrap()).unwrap();
        c.layers
            .iter()
            .find(|l| l.id == Uuid::parse_str(layer).unwrap())
            .unwrap()
            .effects
            .len()
    }

    #[test]
    fn save_returns_a_lumfx_document_of_the_stack() {
        let (b, comp, layer) = bridge_with_two_effects();
        let reply = parse(&save_effect_preset(&b, &comp, &layer, "My look"));
        assert_eq!(reply["ok"], json!(true));
        let text = reply["preset"].as_str().unwrap();
        // The document parses back to the same shape with our two effects.
        let preset: EffectPreset = serde_json::from_str(text).unwrap();
        assert_eq!(preset.format, PRESET_FORMAT);
        assert_eq!(preset.name, "My look");
        assert_eq!(preset.effects.len(), 2);
        assert_eq!(preset.effects[0].effect.match_name, "blur");
        assert_eq!(preset.effects[1].effect.match_name, "glow");
    }

    #[test]
    fn load_appends_with_fresh_ids_as_one_undo_step() {
        let (mut b, comp, layer) = bridge_with_two_effects();
        let text = parse(&save_effect_preset(&b, &comp, &layer, "look"))["preset"]
            .as_str()
            .unwrap()
            .to_owned();
        let before = layer_effect_count(&b, &comp, &layer);
        // Capture the original ids to prove the loaded ones are fresh.
        let original_ids: Vec<Uuid> = b
            .store
            .snapshot()
            .comp(Uuid::parse_str(&comp).unwrap())
            .unwrap()
            .layers
            .iter()
            .find(|l| l.id == Uuid::parse_str(&layer).unwrap())
            .unwrap()
            .effects
            .iter()
            .map(|e| e.id)
            .collect();
        let reply = parse(&load_effect_preset(&mut b, &comp, &layer, &text));
        assert_eq!(reply["ok"], json!(true));
        assert_eq!(
            layer_effect_count(&b, &comp, &layer),
            before + 2,
            "the preset's two effects appended"
        );
        // One undo removes the whole load (SetLayerEffects is one op).
        crate::state::undo(&mut b);
        assert_eq!(layer_effect_count(&b, &comp, &layer), before);
        // Re-load and check the appended ids are all new.
        load_effect_preset(&mut b, &comp, &layer, &text);
        let ids: Vec<Uuid> = b
            .store
            .snapshot()
            .comp(Uuid::parse_str(&comp).unwrap())
            .unwrap()
            .layers
            .iter()
            .find(|l| l.id == Uuid::parse_str(&layer).unwrap())
            .unwrap()
            .effects
            .iter()
            .map(|e| e.id)
            .collect();
        for appended in &ids[before..] {
            assert!(!original_ids.contains(appended), "loaded ids are fresh");
        }
    }

    #[test]
    fn load_of_a_malformed_preset_is_a_calm_error() {
        let (mut b, comp, layer) = bridge_with_two_effects();
        let reply = parse(&load_effect_preset(&mut b, &comp, &layer, "{ not json"));
        assert_eq!(reply["ok"], json!(false));
        assert!(reply["error"].as_str().unwrap().contains("preset"));
    }

    /// The bridge preset shape is byte-identical to `lumit-ui`'s: a document the
    /// bridge writes parses under `lumit_ui::preset::from_json`, and one egui
    /// writes parses here. (Compiled only in the `render` build, where
    /// `lumit-ui` is linked; the shape is pinned there so the two cannot drift.)
    #[cfg(feature = "render")]
    #[test]
    fn preset_shape_matches_lumit_ui_byte_for_byte() {
        let (b, comp, layer) = bridge_with_two_effects();
        let text = parse(&save_effect_preset(&b, &comp, &layer, "shared"))["preset"]
            .as_str()
            .unwrap()
            .to_owned();
        // lumit-ui parses what the bridge wrote.
        let ui_preset =
            lumit_ui::preset::from_json(&text).expect("lumit-ui parses the bridge file");
        assert_eq!(ui_preset.name, "shared");
        assert_eq!(ui_preset.effects.len(), 2);
        // And the bridge parses what lumit-ui writes, byte-for-byte identical.
        let ui_text = lumit_ui::preset::to_json("shared", &ui_preset.effects).unwrap();
        assert_eq!(ui_text, text, "the two frontends serialise identically");
    }
}
