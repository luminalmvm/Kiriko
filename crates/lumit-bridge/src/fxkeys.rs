//! The bridge v0.9 effect-parameter keyframe ops: the stopwatch and keyframe
//! navigator for an animatable effect parameter.
//!
//! # In plain terms
//!
//! Bridge v0.5 could only *set the value* of an effect parameter. The Effect
//! controls also show a stopwatch and a keyframe navigator on each animatable
//! parameter (egui's `effect_rows.rs` per-parameter navigator) — turning
//! keyframing on, adding and removing keys at the playhead, shifting selected
//! keys, and setting a key's interpolation. These are those ops, mirroring the
//! transform keyframe ops (`crate::edits`) exactly, but driving an effect
//! parameter's [`Property`] instead of a transform property's.
//!
//! An animatable parameter is a `Float` (one property), a `Point` (two: x, y) or
//! a `Colour` (four: r, g, b, a). A `channel` argument selects which — 0 for a
//! Float, 0/1 for a Point, 0..3 for a Colour — so one uniform op surface serves
//! all three. Bool/Choice/Seed/File/Layer parameters are not animatable and the
//! ops refuse them calmly.
//!
//! Every op commits the whole edited stack as one [`Op::SetLayerEffects`] (the
//! coarse, exactly-invertible shape every effect edit takes), so undo is one
//! clean step and the two frontends cannot drift.

use crate::edits::{layer_local_seconds, parse_side_interp, rational_at};
use crate::err_json;
use crate::state::{commit, parse_comp_layer, Bridge};
use lumit_core::anim::{Animation, Keyframe, Property, SideInterp};
use lumit_core::model::{Composition, EffectInstance, EffectValue, Layer};
use lumit_core::ops::Op;
use uuid::Uuid;

/// Resolve the target of an effect-param keyframe op: the comp id, a cloned comp
/// and layer, and the layer's cloned effect stack. Shared by every op below.
fn resolve(
    bridge: &Bridge,
    comp_id: &str,
    layer_id: &str,
    ctx: &str,
) -> Result<(Uuid, Composition, Layer, Vec<EffectInstance>), String> {
    let (comp, layer) = parse_comp_layer(comp_id, layer_id).map_err(|e| format!("{ctx}: {e}"))?;
    let doc = bridge.store.snapshot();
    let c = doc
        .comp(comp)
        .cloned()
        .ok_or_else(|| format!("{ctx}: unknown composition"))?;
    let l = c
        .layers
        .iter()
        .find(|l| l.id == layer)
        .cloned()
        .ok_or_else(|| format!("{ctx}: unknown layer"))?;
    let effects = l.effects.clone();
    Ok((comp, c, l, effects))
}

/// Locate the animatable [`Property`] for `(effect_id, param, channel)` inside a
/// stack, for mutation. `channel` selects the axis/channel of a Point/Colour (0
/// for a Float). A non-animatable kind is a calm error.
fn param_property<'a>(
    effects: &'a mut [EffectInstance],
    effect_id: Uuid,
    param: &str,
    channel: usize,
    ctx: &str,
) -> Result<&'a mut Property, String> {
    let e = effects
        .iter_mut()
        .find(|e| e.id == effect_id)
        .ok_or_else(|| format!("{ctx}: unknown effect"))?;
    let p = e
        .params
        .iter_mut()
        .find(|p| p.id == param)
        .ok_or_else(|| format!("{ctx}: unknown parameter '{param}'"))?;
    match &mut p.value {
        EffectValue::Float(prop) => Ok(prop),
        EffectValue::Point(x, y) => match channel {
            0 => Ok(x),
            1 => Ok(y),
            _ => Err(format!("{ctx}: a point has channels 0 (x) and 1 (y)")),
        },
        EffectValue::Colour(ch) => ch
            .get_mut(channel)
            .ok_or_else(|| format!("{ctx}: a colour has channels 0..3 (r, g, b, a)")),
        _ => Err(format!("{ctx}: parameter '{param}' is not animatable")),
    }
}

/// Edit an effect parameter's property in the stack (via `edit`) and commit the
/// whole stack as one [`Op::SetLayerEffects`]. The closure gets the property to
/// mutate plus the comp and layer (for layer-local time), returning a calm error
/// to abort the edit.
#[allow(clippy::too_many_arguments)]
fn with_param(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    effect_id: &str,
    param: &str,
    channel: i64,
    ctx: &str,
    edit: impl FnOnce(&mut Property, &Composition, &Layer) -> Result<(), String>,
) -> String {
    let id = match Uuid::parse_str(effect_id) {
        Ok(id) => id,
        Err(_) => return err_json(format!("{ctx}: effect id is not a valid UUID")),
    };
    let channel = channel.max(0) as usize;
    let (comp, c, l, mut effects) = match resolve(bridge, comp_id, layer_id, ctx) {
        Ok(t) => t,
        Err(e) => return err_json(e),
    };
    {
        let prop = match param_property(&mut effects, id, param, channel, ctx) {
            Ok(p) => p,
            Err(e) => return err_json(e),
        };
        if let Err(e) = edit(prop, &c, &l) {
            return err_json(e);
        }
    }
    commit(
        bridge,
        Op::SetLayerEffects {
            comp,
            layer: l.id,
            effects,
        },
        ctx,
    )
}

/// The stopwatch on an effect parameter: on enable seed a key at the playhead
/// holding the current value; on disable collapse to a static at the current
/// evaluated value — the effect-param twin of `toggle_property_animated`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn toggle_effect_param_animated(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    effect_id: &str,
    param: &str,
    channel: i64,
    frame: i64,
) -> String {
    let ctx = "toggle effect keyframing";
    with_param(
        bridge,
        comp_id,
        layer_id,
        effect_id,
        param,
        channel,
        ctx,
        move |prop, c, l| {
            let lt = layer_local_seconds(c, l, frame);
            prop.animation = if prop.is_animated() {
                Animation::Static(prop.value_at(lt))
            } else {
                Animation::Keyframed(vec![Keyframe {
                    time: rational_at(lt),
                    value: prop.value_at(lt),
                    interp_in: SideInterp::Linear,
                    interp_out: SideInterp::Linear,
                }])
            };
            Ok(())
        },
    )
}

/// Insert or replace an effect-param keyframe at the playhead `frame` with
/// `value` — the effect-param twin of `add_keyframe`. A static parameter becomes
/// keyframed with this one key.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_effect_param_keyframe(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    effect_id: &str,
    param: &str,
    channel: i64,
    frame: i64,
    value: f64,
) -> String {
    let ctx = "add effect keyframe";
    with_param(
        bridge,
        comp_id,
        layer_id,
        effect_id,
        param,
        channel,
        ctx,
        move |prop, c, l| {
            let lt = layer_local_seconds(c, l, frame);
            let mut keys = match &prop.animation {
                Animation::Keyframed(k) => k.clone(),
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
            prop.animation = Animation::Keyframed(keys);
            Ok(())
        },
    )
}

/// Remove the effect-param keyframe at the playhead `frame`. When it was the
/// last key the parameter collapses to a static at the value there — the
/// effect-param twin of `remove_keyframe`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn remove_effect_param_keyframe(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    effect_id: &str,
    param: &str,
    channel: i64,
    frame: i64,
) -> String {
    let ctx = "remove effect keyframe";
    with_param(
        bridge,
        comp_id,
        layer_id,
        effect_id,
        param,
        channel,
        ctx,
        move |prop, c, l| {
            let lt = layer_local_seconds(c, l, frame);
            let Animation::Keyframed(keys) = &prop.animation else {
                return Err(format!("{ctx}: parameter is not animated"));
            };
            let fps = c.frame_rate.fps().max(1.0);
            let tol = 0.5 / fps;
            let kept: Vec<Keyframe> = keys
                .iter()
                .copied()
                .filter(|k| (k.time.to_f64() - lt).abs() >= tol)
                .collect();
            prop.animation = if kept.is_empty() {
                Animation::Static(prop.value_at(lt))
            } else {
                Animation::Keyframed(kept)
            };
            Ok(())
        },
    )
}

/// Slide the effect-param keyframes at comp `frames` by `delta` frames — the
/// effect-param twin of `shift_keyframes`. `frames_json` is a JSON array of comp
/// frame indices; matched keys move by `delta / fps` seconds (interp preserved),
/// the rest stay, sorted and deduped.
#[allow(clippy::too_many_arguments)]
pub(crate) fn shift_effect_param_keyframes(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    effect_id: &str,
    param: &str,
    channel: i64,
    frames_json: &str,
    delta: i64,
) -> String {
    let ctx = "shift effect keyframes";
    let frames: Vec<i64> = match serde_json::from_str(frames_json) {
        Ok(v) => v,
        Err(_) => {
            return err_json(format!("{ctx}: frames must be a JSON array of integers"));
        }
    };
    with_param(
        bridge,
        comp_id,
        layer_id,
        effect_id,
        param,
        channel,
        ctx,
        move |prop, c, l| {
            let Animation::Keyframed(keys) = &prop.animation else {
                return Err(format!("{ctx}: parameter is not animated"));
            };
            let fps = c.frame_rate.fps().max(1.0);
            let tol = 0.5 / fps;
            let delta_secs = delta as f64 / fps;
            let move_times: Vec<f64> = frames
                .iter()
                .map(|f| layer_local_seconds(c, l, *f))
                .collect();
            let mut out: Vec<Keyframe> = keys
                .iter()
                .map(|k| {
                    let moved = move_times.iter().any(|t| (t - k.time.to_f64()).abs() < tol);
                    if moved {
                        Keyframe {
                            time: rational_at(k.time.to_f64() + delta_secs),
                            ..*k
                        }
                    } else {
                        *k
                    }
                })
                .collect();
            out.sort_by_key(|k| k.time);
            out.dedup_by(|a, b| a.time == b.time);
            prop.animation = Animation::Keyframed(out);
            Ok(())
        },
    )
}

/// Set the interpolation of the effect-param keyframe nearest the playhead
/// `frame` — the effect-param twin of `set_keyframe_interp`. Each side takes a
/// `Hold`/`Linear`/`Bezier` name; a `Bezier` side reads its `(speed, influence)`
/// from the matching pair. A no-op (not animated, or no key there) is a calm
/// error.
#[allow(clippy::too_many_arguments)]
pub(crate) fn set_effect_param_keyframe_interp(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    effect_id: &str,
    param: &str,
    channel: i64,
    frame: i64,
    interp_in: &str,
    interp_out: &str,
    speed_in: f64,
    influence_in: f64,
    speed_out: f64,
    influence_out: f64,
) -> String {
    let ctx = "set effect keyframe interp";
    let side_in = match parse_side_interp(interp_in, speed_in, influence_in) {
        Ok(s) => s,
        Err(e) => return err_json(format!("{ctx}: {e}")),
    };
    let side_out = match parse_side_interp(interp_out, speed_out, influence_out) {
        Ok(s) => s,
        Err(e) => return err_json(format!("{ctx}: {e}")),
    };
    with_param(
        bridge,
        comp_id,
        layer_id,
        effect_id,
        param,
        channel,
        ctx,
        move |prop, c, l| {
            let lt = layer_local_seconds(c, l, frame);
            let Animation::Keyframed(keys) = &mut prop.animation else {
                return Err(format!("{ctx}: parameter is not animated"));
            };
            let fps = c.frame_rate.fps().max(1.0);
            let tol = 0.5 / fps;
            let Some(k) = keys.iter_mut().find(|k| (k.time.to_f64() - lt).abs() < tol) else {
                return Err(format!("{ctx}: no keyframe at the playhead"));
            };
            k.interp_in = side_in;
            k.interp_out = side_out;
            Ok(())
        },
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::state::{new_composition, snapshot};
    use lumit_core::model::ProjectItem;
    use serde_json::{json, Value};

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).expect("reply is valid JSON")
    }

    /// A bridge with one comp, one footage layer, and one `blur` effect; returns
    /// bridge, comp id, layer id, effect id and the first Float param's name.
    fn bridge_with_effect() -> (Bridge, String, String, String, String) {
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
        crate::state::import_footage(&mut b, "/media/clip.mp4");
        let item = b
            .store
            .snapshot()
            .items
            .iter()
            .find_map(|i| match i {
                ProjectItem::Footage(f) => Some(f.id),
                _ => None,
            })
            .unwrap();
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
        crate::edits::add_effect(&mut b, &comp.to_string(), &layer.to_string(), "blur");
        let doc = b.store.snapshot();
        let l = doc
            .comp(comp)
            .unwrap()
            .layers
            .iter()
            .find(|l| l.id == layer)
            .unwrap();
        let e = &l.effects[0];
        let param = e
            .params
            .iter()
            .find(|p| matches!(p.value, EffectValue::Float(_)))
            .expect("blur has a float param")
            .id
            .clone();
        (
            b,
            comp.to_string(),
            layer.to_string(),
            e.id.to_string(),
            param,
        )
    }

    /// Read the effect param's read-back object from the snapshot.
    fn param_readback(b: &Bridge, comp: &str, layer: &str, effect: &str, param: &str) -> Value {
        let snap = parse(&snapshot(b));
        fn find_comp<'a>(items: &'a [Value], comp: &str) -> Option<&'a Value> {
            for item in items {
                if item["kind"] == json!("composition") && item["id"].as_str() == Some(comp) {
                    return Some(item);
                }
                if let Some(ch) = item["children"].as_array() {
                    if let Some(found) = find_comp(ch, comp) {
                        return Some(found);
                    }
                }
            }
            None
        }
        let c = find_comp(snap["items"].as_array().unwrap(), comp).expect("comp");
        let l = c["comp"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["id"] == json!(layer))
            .unwrap();
        let e = l["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == json!(effect))
            .unwrap();
        e["params"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == json!(param))
            .unwrap()
            .clone()
    }

    #[test]
    fn stopwatch_toggles_effect_param_keyframing() {
        let (mut b, comp, layer, effect, param) = bridge_with_effect();
        // Enable at frame 0: the param becomes animated with one key.
        let reply = parse(&toggle_effect_param_animated(
            &mut b, &comp, &layer, &effect, &param, 0, 0,
        ));
        assert_eq!(reply["ok"], json!(true));
        let p = param_readback(&b, &comp, &layer, &effect, &param);
        assert_eq!(p["animated"], json!(true));
        assert_eq!(p["keys"].as_array().unwrap().len(), 1);
        // Disable: it collapses back to static.
        toggle_effect_param_animated(&mut b, &comp, &layer, &effect, &param, 0, 0);
        let p = param_readback(&b, &comp, &layer, &effect, &param);
        assert_eq!(p["animated"], json!(false));
    }

    #[test]
    fn add_remove_and_shift_effect_param_keys() {
        let (mut b, comp, layer, effect, param) = bridge_with_effect();
        // Add two keys (at 60 fps default: frames 0 and 60).
        add_effect_param_keyframe(&mut b, &comp, &layer, &effect, &param, 0, 0, 5.0);
        add_effect_param_keyframe(&mut b, &comp, &layer, &effect, &param, 0, 60, 20.0);
        let p = param_readback(&b, &comp, &layer, &effect, &param);
        assert_eq!(p["animated"], json!(true));
        assert_eq!(p["keys"].as_array().unwrap().len(), 2);
        // Shift the second key (frame 60) by +30 frames → frame 90.
        shift_effect_param_keyframes(&mut b, &comp, &layer, &effect, &param, 0, "[60]", 30);
        let p = param_readback(&b, &comp, &layer, &effect, &param);
        let frames: Vec<i64> = p["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k["frame"].as_i64().unwrap())
            .collect();
        assert!(
            frames.contains(&0) && frames.contains(&90),
            "got {frames:?}"
        );
        // Remove the key at frame 0.
        remove_effect_param_keyframe(&mut b, &comp, &layer, &effect, &param, 0, 0);
        let p = param_readback(&b, &comp, &layer, &effect, &param);
        assert_eq!(p["keys"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn set_effect_param_key_interp_and_undo() {
        let (mut b, comp, layer, effect, param) = bridge_with_effect();
        add_effect_param_keyframe(&mut b, &comp, &layer, &effect, &param, 0, 0, 5.0);
        let reply = parse(&set_effect_param_keyframe_interp(
            &mut b, &comp, &layer, &effect, &param, 0, 0, "Hold", "Linear", 0.0, 0.0, 0.0, 0.0,
        ));
        assert_eq!(reply["ok"], json!(true));
        let p = param_readback(&b, &comp, &layer, &effect, &param);
        assert_eq!(p["keys"][0]["interp_in"], json!("Hold"));
        // Each edit is one undo step.
        crate::state::undo(&mut b);
        let p = param_readback(&b, &comp, &layer, &effect, &param);
        assert_eq!(p["keys"][0]["interp_in"], json!("Linear"));
    }

    #[test]
    fn a_non_animatable_param_is_a_calm_error() {
        let (mut b, comp, layer, effect, _param) = bridge_with_effect();
        // `quality` on blur is a Choice — not animatable.
        let reply = parse(&toggle_effect_param_animated(
            &mut b, &comp, &layer, &effect, "quality", 0, 0,
        ));
        // Either the param is absent or not animatable — both are calm errors.
        assert_eq!(reply["ok"], json!(false));
    }
}
