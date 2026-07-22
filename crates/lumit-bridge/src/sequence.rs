//! The bridge v0.5 razor ops: cutting and deleting clips in a Sequence layer.
//!
//! # In plain terms
//!
//! A Sequence layer holds a row of clips (the Vegas-style surface). The razor
//! is the blade: cut the clip under the playhead into two, or delete the clip
//! under the playhead outright (leaving a gap). Both mirror `lumit-ui`'s
//! `cut_sequence_at_playhead` / `delete_clip_at_playhead` exactly — the clip
//! *places* never move (the beat-sync covenant, K-071) — and each commits one
//! [`Op::SetSequenceClips`], so undo is one clean step and the two frontends
//! cannot drift.
//!
//! The razor only bites a Sequence layer. Asked to cut a footage, solid or any
//! other kind, it refuses calmly with a note rather than doing nothing — the
//! same "the razor needs a sequence layer" message the egui frontend gives.

use crate::err_json;
use crate::state::{parse_comp_layer, Bridge};
use lumit_core::model::{Composition, Layer, LayerKind};
use lumit_core::ops::Op;
use lumit_core::sequence::Clip;
use uuid::Uuid;

/// Resolve a comp id and layer id to the comp (cloned), the layer (cloned) and
/// its clip list — refusing calmly when the layer is not a Sequence. A `not_seq`
/// message tailors the refusal to the caller (cut vs delete).
fn resolve_sequence(
    bridge: &Bridge,
    comp_id: &str,
    layer_id: &str,
    ctx: &str,
    not_seq: &str,
) -> Result<(Uuid, Composition, Layer, Vec<Clip>), String> {
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
    let clips = match &l.kind {
        LayerKind::Sequence { clips } => clips.clone(),
        _ => return Err(format!("{ctx}: {not_seq}")),
    };
    Ok((comp, c, l, clips))
}

/// The clip index under the playhead `frame`, and the layer-local cut time `tau`
/// as a rational — the exact `comp_time(frame) − start_offset` the egui razor
/// uses. `Err` when the playhead maps to no valid time or sits over no clip.
fn clip_under_playhead(
    c: &Composition,
    l: &Layer,
    clips: &[Clip],
    frame: i64,
    ctx: &str,
) -> Result<(usize, lumit_core::time::Rational), String> {
    let comp_t = c
        .frame_rate
        .time_of_frame(frame)
        .map_err(|e| format!("{ctx}: {e}"))?;
    let tau = comp_t
        .0
        .checked_sub(l.start_offset.0)
        .map_err(|e| format!("{ctx}: {e}"))?;
    let idx = clips
        .iter()
        .position(|clip| clip.contains(tau.to_f64()))
        .ok_or_else(|| format!("{ctx}: no clip under the playhead"))?;
    Ok((idx, tau))
}

/// Razor: cut the selected Sequence layer's clip at the playhead into two — one
/// undo step (`SetSequenceClips`). The clip places don't move (the beat-sync
/// covenant, K-071). An eased ramp that cannot be split cleanly at this time is
/// a calm error, exactly as the egui razor reports.
pub(crate) fn cut_clip_at_playhead(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    frame: i64,
) -> String {
    let ctx = "cut clip at playhead";
    let (comp, c, l, clips) = match resolve_sequence(
        bridge,
        comp_id,
        layer_id,
        ctx,
        "the razor needs a sequence layer",
    ) {
        Ok(t) => t,
        Err(e) => return err_json(e),
    };
    let (idx, tau) = match clip_under_playhead(&c, &l, &clips, frame, ctx) {
        Ok(t) => t,
        Err(e) => return err_json(e),
    };
    let Some((left, right)) = clips[idx].cut(tau) else {
        return err_json(format!("{ctx}: can't cut an eased ramp here yet"));
    };
    let mut new_clips = clips;
    new_clips.splice(idx..=idx, [left, right]);
    crate::state::commit(
        bridge,
        Op::SetSequenceClips {
            comp,
            layer: l.id,
            clips: new_clips,
        },
        ctx,
    )
}

/// Delete the clip under the playhead in the selected Sequence layer, leaving a
/// gap (the Vegas surface allows gaps, K-071) — one undo step. Refuses calmly
/// on a non-sequence layer or when no clip sits under the playhead.
pub(crate) fn delete_clip_at_playhead(
    bridge: &mut Bridge,
    comp_id: &str,
    layer_id: &str,
    frame: i64,
) -> String {
    let ctx = "delete clip at playhead";
    let (comp, c, l, clips) =
        match resolve_sequence(bridge, comp_id, layer_id, ctx, "not a sequence layer") {
            Ok(t) => t,
            Err(e) => return err_json(e),
        };
    let (idx, _tau) = match clip_under_playhead(&c, &l, &clips, frame, ctx) {
        Ok(t) => t,
        Err(e) => return err_json(e),
    };
    let mut new_clips = clips;
    new_clips.remove(idx);
    crate::state::commit(
        bridge,
        Op::SetSequenceClips {
            comp,
            layer: l.id,
            clips: new_clips,
        },
        ctx,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::edits::add_camera_layer;
    use crate::state::{new_composition, snapshot, undo};
    use lumit_core::model::ProjectItem;
    use lumit_core::sequence::{Clip, ClipSource};
    use lumit_core::time::{CompTime, Rational};
    use serde_json::{json, Value};

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).expect("reply is valid JSON")
    }

    /// Locate the single composition (nested one level under the auto-folder).
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

    /// A bridge with one 60 fps comp holding a single Sequence layer whose one
    /// clip spans layer-local [0, 5] s. Returns the bridge, comp id, layer id.
    fn bridge_with_sequence() -> (Bridge, String, String) {
        let mut b = Bridge::new();
        new_composition(&mut b, "Scene");
        let comp_id = b
            .store
            .snapshot()
            .items
            .iter()
            .find_map(|i| match i {
                ProjectItem::Composition(c) => Some(c.id),
                _ => None,
            })
            .expect("a comp exists");
        let dur = Rational::new(5, 1).unwrap();
        let clip = Clip {
            id: Uuid::now_v7(),
            source: ClipSource::Footage(Uuid::now_v7()),
            source_in: Rational::ZERO,
            source_out: dur,
            place_start: Rational::ZERO,
            place_duration: dur,
            retime: lumit_core::retime::Retime::identity(dur, Rational::ZERO),
            interpolation: Default::default(),
            extra: serde_json::Map::new(),
        };
        let layer = lumit_core::model::Layer {
            id: Uuid::now_v7(),
            name: "Sequence".into(),
            kind: LayerKind::Sequence { clips: vec![clip] },
            in_point: CompTime(Rational::ZERO),
            out_point: CompTime(dur),
            start_offset: CompTime(Rational::ZERO),
            transform: lumit_core::model::TransformGroup::default(),
            matte: None,
            parent: None,
            label: 0,
            volume_db: lumit_core::anim::Property::zero(),
            blend: Default::default(),
            masks: Vec::new(),
            effects: Vec::new(),
            switches: lumit_core::model::Switches::default(),
            extra: serde_json::Map::new(),
        };
        let layer_id = layer.id.to_string();
        b.store
            .commit(Op::AddLayer {
                comp: comp_id,
                index: 0,
                layer: Box::new(layer),
            })
            .unwrap();
        (b, comp_id.to_string(), layer_id)
    }

    fn seq_clip_count(b: &Bridge, comp: &str, layer: &str) -> usize {
        let doc = b.store.snapshot();
        let c = doc.comp(Uuid::parse_str(comp).unwrap()).unwrap();
        let l = c
            .layers
            .iter()
            .find(|l| l.id == Uuid::parse_str(layer).unwrap())
            .unwrap();
        match &l.kind {
            LayerKind::Sequence { clips } => clips.len(),
            _ => panic!("not a sequence"),
        }
    }

    #[test]
    fn cut_splits_the_clip_under_the_playhead_and_undoes() {
        let (mut b, comp, layer) = bridge_with_sequence();
        // Cut at frame 120 (2 s, inside the [0,5] clip).
        let snap = parse(&cut_clip_at_playhead(&mut b, &comp, &layer, 120));
        assert_eq!(snap["ok"], json!(true));
        assert_eq!(seq_clip_count(&b, &comp, &layer), 2);
        undo(&mut b);
        assert_eq!(seq_clip_count(&b, &comp, &layer), 1);
    }

    #[test]
    fn delete_removes_the_clip_under_the_playhead() {
        let (mut b, comp, layer) = bridge_with_sequence();
        let snap = parse(&delete_clip_at_playhead(&mut b, &comp, &layer, 60));
        assert_eq!(snap["ok"], json!(true));
        assert_eq!(seq_clip_count(&b, &comp, &layer), 0);
    }

    #[test]
    fn cut_with_no_clip_under_the_playhead_is_a_calm_error() {
        let (mut b, comp, layer) = bridge_with_sequence();
        // Frame 600 is 10 s — past the 5 s clip, so nothing sits under it.
        let reply = parse(&cut_clip_at_playhead(&mut b, &comp, &layer, 600));
        assert_eq!(reply["ok"], json!(false));
        assert!(reply["error"].as_str().unwrap().contains("no clip"));
    }

    #[test]
    fn razor_refuses_a_non_sequence_layer_calmly() {
        let (mut b, comp, _layer) = bridge_with_sequence();
        add_camera_layer(&mut b, &comp);
        let snap = parse(&snapshot(&b));
        let comp_item = find_comp(&snap);
        let cam = comp_item["comp"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["kind"] == json!("camera"))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let reply = parse(&cut_clip_at_playhead(&mut b, &comp, &cam, 60));
        assert_eq!(reply["ok"], json!(false));
        assert!(reply["error"].as_str().unwrap().contains("sequence layer"));
    }
}
