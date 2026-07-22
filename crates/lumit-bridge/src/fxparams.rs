//! The bridge v0.5 effect-parameter setters for the non-scalar/non-colour kinds
//! (enum/bool/seed/point), effect reordering, and the linked-keyframe batch op.
//!
//! # In plain terms
//!
//! Bridge v0.3 could set only the scalar and colour effect parameters. The
//! Effect Controls panel also shows *enum* dropdowns, *bool* checkboxes, *seed*
//! fields and *point* pickers, and the read-back already carries their values —
//! these are the matching setters, so those controls become live. Alongside them:
//! reordering an effect in the stack (drag-to-reorder), and one batch op that
//! commits several transform-keyframe edits as a *single* undo step — the linked
//! x/y pair the panel previously cost one undo per axis.
//!
//! Every effect edit routes through [`crate::edits::with_effects`]
//! (`SetLayerEffects`); the keyframe batch commits one [`Op::Batch`] of
//! `SetTransformProperty` ops. Undo stays one clean step and the two frontends
//! cannot drift.

use crate::edits::{layer_local_seconds, rational_at, with_effects};
use crate::err_json;
use crate::state::{commit, parse_comp_layer, parse_transform_prop, Bridge};
use lumit_core::anim::{Animation, Keyframe, Property, SideInterp};
use lumit_core::model::EffectValue;
use lumit_core::ops::Op;
use serde_json::Value;
use uuid::Uuid;

/// Resolve an effect id (a calm error when malformed), shared by the setters.
fn parse_effect(effect_id: &str, ctx: &str) -> Result<Uuid, String> {
    Uuid::parse_str(effect_id).map_err(|_| format!("{ctx}: effect id is not a valid UUID"))
}

/// Set an enum (`Choice`) effect parameter to an option `index`.
pub(crate) fn set_effect_param_choice(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    effect_id: &str,
    param_name: &str,
    index: u32,
) -> String {
    let ctx = "set effect choice";
    let id = match parse_effect(effect_id, ctx) {
        Ok(id) => id,
        Err(e) => return err_json(e),
    };
    let param = param_name.to_owned();
    with_effects(bridge, comp_id, layer_id, ctx, move |effects| {
        let p = find_param(effects, id, &param, ctx)?;
        match p {
            EffectValue::Choice(c) => {
                *c = index;
                Ok(())
            }
            _ => Err(err_json(format!("{ctx}: parameter is not an enum"))),
        }
    })
}

/// Set a `Bool` effect parameter.
pub(crate) fn set_effect_param_bool(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    effect_id: &str,
    param_name: &str,
    value: bool,
) -> String {
    let ctx = "set effect bool";
    let id = match parse_effect(effect_id, ctx) {
        Ok(id) => id,
        Err(e) => return err_json(e),
    };
    let param = param_name.to_owned();
    with_effects(bridge, comp_id, layer_id, ctx, move |effects| {
        let p = find_param(effects, id, &param, ctx)?;
        match p {
            EffectValue::Bool(b) => {
                *b = value;
                Ok(())
            }
            _ => Err(err_json(format!("{ctx}: parameter is not a boolean"))),
        }
    })
}

/// Set a `Seed` effect parameter.
pub(crate) fn set_effect_param_seed(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    effect_id: &str,
    param_name: &str,
    seed: u32,
) -> String {
    let ctx = "set effect seed";
    let id = match parse_effect(effect_id, ctx) {
        Ok(id) => id,
        Err(e) => return err_json(e),
    };
    let param = param_name.to_owned();
    with_effects(bridge, comp_id, layer_id, ctx, move |effects| {
        let p = find_param(effects, id, &param, ctx)?;
        match p {
            EffectValue::Seed(s) => {
                *s = seed;
                Ok(())
            }
            _ => Err(err_json(format!("{ctx}: parameter is not a seed"))),
        }
    })
}

/// Set a `Point` effect parameter to a static `(x, y)` (both channels static).
pub(crate) fn set_effect_param_point(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    effect_id: &str,
    param_name: &str,
    x: f64,
    y: f64,
) -> String {
    let ctx = "set effect point";
    let id = match parse_effect(effect_id, ctx) {
        Ok(id) => id,
        Err(e) => return err_json(e),
    };
    let param = param_name.to_owned();
    with_effects(bridge, comp_id, layer_id, ctx, move |effects| {
        let p = find_param(effects, id, &param, ctx)?;
        match p {
            EffectValue::Point(px, py) => {
                *px = Property::fixed(x);
                *py = Property::fixed(y);
                Ok(())
            }
            _ => Err(err_json(format!("{ctx}: parameter is not a point"))),
        }
    })
}

/// Find a parameter's value inside a stack clone, or a calm error.
fn find_param<'a>(
    effects: &'a mut [lumit_core::model::EffectInstance],
    effect_id: Uuid,
    param_name: &str,
    ctx: &str,
) -> Result<&'a mut EffectValue, String> {
    let inst = effects
        .iter_mut()
        .find(|e| e.id == effect_id)
        .ok_or_else(|| err_json(format!("{ctx}: unknown effect")))?;
    let param = inst
        .params
        .iter_mut()
        .find(|p| p.id == param_name)
        .ok_or_else(|| err_json(format!("{ctx}: unknown parameter")))?;
    Ok(&mut param.value)
}

/// Reorder an effect within a layer's stack to `new_index` — the Effect Controls
/// drag-to-reorder ([`Op::SetLayerEffects`] with the reordered list). The index
/// clamps into range (a value past the end lands the effect at the bottom). One
/// undo step.
pub(crate) fn reorder_effect(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    effect_id: &str,
    new_index: i64,
) -> String {
    let ctx = "reorder effect";
    let id = match parse_effect(effect_id, ctx) {
        Ok(id) => id,
        Err(e) => return err_json(e),
    };
    with_effects(bridge, comp_id, layer_id, ctx, move |effects| {
        let Some(from) = effects.iter().position(|e| e.id == id) else {
            return Err(err_json(format!("{ctx}: unknown effect")));
        };
        let inst = effects.remove(from);
        let to = usize::try_from(new_index).unwrap_or(0).min(effects.len());
        effects.insert(to, inst);
        Ok(())
    })
}

/// Apply several transform-keyframe edits as one undo step — the linked x/y pair
/// (and any multi-property gesture) committed as a single [`Op::Batch`]. `ops_json`
/// is a JSON array of `{property, action, frame, value?}` objects, all on one
/// layer: `action` is `add` (insert/replace a key at `frame` with `value`),
/// `remove` (delete the key at `frame`, collapsing to static when it was the
/// last), or `toggle` (the stopwatch — seed a key on enable, collapse on
/// disable). Edits on the *same* property chain in order; edits on *different*
/// properties are independent, so a linked pair is two `add`s that undo together.
/// An empty array is a calm no-op that still refreshes.
pub(crate) fn apply_keyframe_batch(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    ops_json: &str,
) -> String {
    let ctx = "apply keyframe batch";
    let (comp, layer) = match parse_comp_layer(comp_id, layer_id) {
        Ok(pair) => pair,
        Err(e) => return err_json(format!("{ctx}: {e}")),
    };
    let entries: Vec<KeyOp> = match serde_json::from_str::<Vec<Value>>(ops_json) {
        Ok(v) => match v.iter().map(KeyOp::parse).collect::<Result<_, _>>() {
            Ok(e) => e,
            Err(e) => return err_json(format!("{ctx}: {e}")),
        },
        Err(_) => return err_json(format!("{ctx}: expected a JSON array of keyframe ops")),
    };
    let doc = bridge.store.snapshot();
    let Some(c) = doc.comp(comp) else {
        return err_json(format!("{ctx}: unknown composition"));
    };
    let Some(l) = c.layers.iter().find(|l| l.id == layer) else {
        return err_json(format!("{ctx}: unknown layer"));
    };
    let fps = c.frame_rate.fps().max(1.0);
    // Per-property working animation, in first-touched order, so repeated edits
    // on one property chain and a linked pair keeps a stable op order.
    let mut working: Vec<(lumit_core::model::TransformProp, Animation)> = Vec::new();
    for entry in &entries {
        let Some(prop) = parse_transform_prop(&entry.property) else {
            return err_json(format!("{ctx}: unknown property '{}'", entry.property));
        };
        let pos = match working.iter().position(|(p, _)| *p == prop) {
            Some(i) => i,
            None => {
                working.push((prop, l.transform.get(prop).animation.clone()));
                working.len() - 1
            }
        };
        let anim = &mut working[pos].1;
        let lt = layer_local_seconds(c, l, entry.frame);
        match entry.action.as_str() {
            "add" => keyframe_upsert(anim, lt, entry.value),
            "remove" => keyframe_remove(anim, lt, fps),
            "toggle" => keyframe_toggle(anim, lt),
            other => return err_json(format!("{ctx}: unknown action '{other}'")),
        }
    }
    let ops: Vec<Op> = working
        .into_iter()
        .map(|(prop, animation)| Op::SetTransformProperty {
            comp,
            layer,
            prop,
            animation,
        })
        .collect();
    if ops.is_empty() {
        return crate::state::snapshot(bridge);
    }
    commit(bridge, Op::Batch { ops }, ctx)
}

/// One parsed keyframe-batch entry.
struct KeyOp {
    property: String,
    action: String,
    frame: i64,
    value: f64,
}

impl KeyOp {
    fn parse(v: &Value) -> Result<Self, String> {
        let property = v
            .get("property")
            .and_then(Value::as_str)
            .ok_or("each op needs a 'property' string")?
            .to_owned();
        let action = v
            .get("action")
            .and_then(Value::as_str)
            .ok_or("each op needs an 'action' string")?
            .to_owned();
        let frame = v.get("frame").and_then(Value::as_i64).unwrap_or(0);
        let value = v.get("value").and_then(Value::as_f64).unwrap_or(0.0);
        Ok(KeyOp {
            property,
            action,
            frame,
            value,
        })
    }
}

/// The value an animation evaluates to at layer-local time `lt` (via a scratch
/// [`Property`], since [`Animation`] itself carries no evaluator).
fn value_of(anim: &Animation, lt: f64) -> f64 {
    Property {
        animation: anim.clone(),
        extra: serde_json::Map::new(),
    }
    .value_at(lt)
}

/// Insert or replace a key at layer-local time `lt` with `value` (half-frame
/// tolerance, Linear sides), promoting a static property to keyframed — the same
/// upsert `crate::edits::add_keyframe` performs.
fn keyframe_upsert(anim: &mut Animation, lt: f64, value: f64) {
    let mut keys = match anim {
        Animation::Keyframed(k) => std::mem::take(k),
        Animation::Static(v) => vec![Keyframe {
            time: rational_at(0.0),
            value: *v,
            interp_in: SideInterp::Linear,
            interp_out: SideInterp::Linear,
        }],
    };
    const EPS: f64 = 1.0 / 240.0;
    if let Some(existing) = keys.iter_mut().find(|k| (k.time.to_f64() - lt).abs() < EPS) {
        existing.value = value;
    } else {
        keys.push(Keyframe {
            time: rational_at(lt),
            value,
            interp_in: SideInterp::Linear,
            interp_out: SideInterp::Linear,
        });
        keys.sort_by_key(|k| k.time);
    }
    *anim = Animation::Keyframed(keys);
}

/// Remove the key at `lt`, collapsing to static at that value when it was the
/// last — the same collapse `crate::edits::remove_keyframe` performs. A no-op
/// on a static property.
fn keyframe_remove(anim: &mut Animation, lt: f64, fps: f64) {
    let value_here = value_of(anim, lt);
    let Animation::Keyframed(keys) = anim else {
        return;
    };
    let tol = 0.5 / fps;
    let kept: Vec<Keyframe> = keys
        .iter()
        .copied()
        .filter(|k| (k.time.to_f64() - lt).abs() >= tol)
        .collect();
    *anim = if kept.is_empty() {
        Animation::Static(value_here)
    } else {
        Animation::Keyframed(kept)
    };
}

/// The stopwatch: enable seeds one key holding the current value at `lt`;
/// disable collapses to a static at the current value — the same toggle
/// `crate::edits::toggle_property_animated` performs.
fn keyframe_toggle(anim: &mut Animation, lt: f64) {
    let value_here = value_of(anim, lt);
    *anim = match anim {
        Animation::Keyframed(_) => Animation::Static(value_here),
        Animation::Static(_) => Animation::Keyframed(vec![Keyframe {
            time: rational_at(lt),
            value: value_here,
            interp_in: SideInterp::Linear,
            interp_out: SideInterp::Linear,
        }]),
    };
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::edits::{add_camera_layer, add_effect};
    use crate::state::{new_composition, snapshot, undo};
    use lumit_core::model::ProjectItem;
    use serde_json::json;

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).expect("reply is valid JSON")
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

    fn first_layer(snap: &Value) -> Value {
        find_comp(snap)["comp"]["layers"][0].clone()
    }

    fn comp_with_camera() -> (Bridge, String, String) {
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
            .unwrap()
            .to_string();
        add_camera_layer(&mut b, &comp);
        let layer = first_layer(&parse(&snapshot(&b)))["id"]
            .as_str()
            .unwrap()
            .to_owned();
        (b, comp, layer)
    }

    /// A builtin with an enum (Choice) parameter, its name and the param id.
    fn choice_builtin() -> (&'static str, &'static str) {
        lumit_core::fx::BUILTINS
            .iter()
            .find_map(|s| {
                s.params.iter().find_map(|p| match p.kind {
                    lumit_core::fx::ParamKind::Choice { .. } => Some((s.match_name, p.id)),
                    _ => None,
                })
            })
            .expect("a builtin has a choice param")
    }

    #[test]
    fn set_choice_param_round_trips() {
        let (mut b, comp, layer) = comp_with_camera();
        let (name, param) = choice_builtin();
        add_effect(&mut b, &comp, &layer, name);
        let snap = parse(&snapshot(&b));
        let effect = first_layer(&snap)["effects"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let snap = parse(&set_effect_param_choice(
            &mut b, &comp, &layer, &effect, param, 1,
        ));
        let params = first_layer(&snap)["effects"][0]["params"]
            .as_array()
            .unwrap()
            .clone();
        let got = params.iter().find(|p| p["name"] == json!(param)).unwrap();
        assert_eq!(got["kind"], json!("enum"));
        assert_eq!(got["value"], json!(1));
    }

    #[test]
    fn reorder_effect_moves_within_the_stack_and_undoes() {
        let (mut b, comp, layer) = comp_with_camera();
        let a = lumit_core::fx::BUILTINS[0].match_name;
        let c = lumit_core::fx::BUILTINS[1].match_name;
        add_effect(&mut b, &comp, &layer, a);
        add_effect(&mut b, &comp, &layer, c);
        let snap = parse(&snapshot(&b));
        let effects = first_layer(&snap)["effects"].as_array().unwrap().clone();
        // Move the second (index 1) to the top (index 0).
        let second = effects[1]["id"].as_str().unwrap().to_owned();
        let snap = parse(&reorder_effect(&mut b, &comp, &layer, &second, 0));
        assert_eq!(snap["ok"], json!(true));
        assert_eq!(
            first_layer(&snap)["effects"][0]["id"].as_str().unwrap(),
            second
        );
        // Undo restores the original order.
        let after = parse(&undo(&mut b));
        assert_eq!(
            first_layer(&after)["effects"][1]["id"].as_str().unwrap(),
            second
        );
    }

    #[test]
    fn keyframe_batch_adds_a_linked_pair_in_one_undo_step() {
        let (mut b, comp, layer) = comp_with_camera();
        let ops = json!([
            {"property": "position_x", "action": "add", "frame": 0, "value": 100.0},
            {"property": "position_y", "action": "add", "frame": 0, "value": 200.0},
        ])
        .to_string();
        let snap = parse(&apply_keyframe_batch(&mut b, &comp, &layer, &ops));
        assert_eq!(snap["ok"], json!(true));
        let tr = first_layer(&snap)["transform"].clone();
        assert_eq!(tr["position_x"]["animated"], json!(true));
        assert_eq!(tr["position_y"]["animated"], json!(true));
        // One undo removes BOTH keys (a single batch step).
        let after = parse(&undo(&mut b));
        let tr = first_layer(&after)["transform"].clone();
        assert_eq!(tr["position_x"]["animated"], json!(false));
        assert_eq!(tr["position_y"]["animated"], json!(false));
    }

    #[test]
    fn keyframe_batch_rejects_a_bad_action() {
        let (mut b, comp, layer) = comp_with_camera();
        let ops = json!([{"property": "opacity", "action": "wobble", "frame": 0}]).to_string();
        let reply = parse(&apply_keyframe_batch(&mut b, &comp, &layer, &ops));
        assert_eq!(reply["ok"], json!(false));
        assert!(reply["error"].as_str().unwrap().contains("unknown action"));
    }
}
