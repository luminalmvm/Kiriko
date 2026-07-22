//! The bridge v0.5 beat ops: detecting beat markers from a composition's audio,
//! and clearing them.
//!
//! # In plain terms
//!
//! "Detect beats" listens to a composition's audio, finds the onsets and the
//! tempo, and drops a Beat marker on each — the rhythm you snap edits to.
//! "Clear beat markers" removes just those (leaving your own user and chapter
//! markers). Clearing is a plain marker-list edit, always available; detecting
//! needs the audio pipeline (the `media` feature) and the compositor's input
//! builder (the `render` feature), so it is gated on both — without them it
//! reports the calm "needs the media build" note rather than pretending.
//!
//! egui runs detection **off the UI thread** (`detect_beats` spawns, `poll_beats`
//! drains) because the mixdown + onset analysis can be slow on long audio. The
//! bridge runs it **synchronously** here: it is a single blocking call the Dart
//! side awaits off its own UI isolate (the same choice the media probe makes at
//! this phase). If long-audio latency bites, a start/poll pair like the export
//! ops is the follow-up — the maths is identical, only the threading differs.
//! Both commit one [`Op::SetCompMarkers`] (via `with_regenerated_beats`), so the
//! two frontends produce the same markers and undo is one clean step.

use crate::err_json;
use crate::state::{commit, Bridge};
use lumit_core::ops::Op;
use uuid::Uuid;

/// Remove every detected Beat marker from a composition, keeping user and
/// chapter markers — `lumit-ui`'s `clear_beat_markers` ([`Op::SetCompMarkers`]).
/// A comp with no beat markers is a calm no-op that still refreshes. Always
/// available (no media feature needed).
pub(crate) fn clear_beat_markers(bridge: &mut Bridge, comp_id: &str) -> String {
    let ctx = "clear beat markers";
    let comp = match Uuid::parse_str(comp_id) {
        Ok(id) => id,
        Err(_) => return err_json(format!("{ctx}: composition id is not a valid UUID")),
    };
    let doc = bridge.store.snapshot();
    let Some(c) = doc.comp(comp) else {
        return err_json(format!("{ctx}: unknown composition"));
    };
    if !c.markers.iter().any(|m| m.is_beat()) {
        // Nothing to clear — still return a fresh snapshot (a calm no-op).
        return crate::state::snapshot(bridge);
    }
    let markers = c.markers.iter().filter(|m| !m.is_beat()).cloned().collect();
    commit(bridge, Op::SetCompMarkers { comp, markers }, ctx)
}

/// Detect beat markers for a composition from its audio — `lumit-ui`'s
/// `detect_beats` + `poll_beats` collapsed into one synchronous call.
/// `sensitivity_percent` is 0..100 (higher = more beats, 50 = Standard); it is
/// turned into the detector's δ exactly as the egui menu does
/// (`delta_from_sensitivity`). Mixes the comp's audio, runs onset + tempo
/// detection, snaps near-grid onsets onto the tempo grid, and commits the new
/// Beat markers (replacing only the prior Beat markers) as one
/// [`Op::SetCompMarkers`]. A silent comp is a calm note. Needs the `media` and
/// `render` features; without them it reports that calmly.
#[cfg(all(feature = "media", feature = "render"))]
pub(crate) fn detect_beats(bridge: &mut Bridge, comp_id: &str, sensitivity_percent: i64) -> String {
    let ctx = "detect beats";
    let comp = match Uuid::parse_str(comp_id) {
        Ok(id) => id,
        Err(_) => return err_json(format!("{ctx}: composition id is not a valid UUID")),
    };
    let doc = bridge.store.snapshot();
    let Some(c) = doc.comp(comp).cloned() else {
        return err_json(format!("{ctx}: unknown composition"));
    };
    // The audio jobs for this comp, built through the same headless input path
    // the exporter uses. No adapter / no audio ⇒ a calm note, never a crash.
    let Some(inputs) = crate::render::with_export_inputs(&doc, comp) else {
        return err_json(format!(
            "{ctx}: could not build the audio for this composition"
        ));
    };
    if inputs.audio.is_empty() {
        return err_json(format!(
            "{ctx}: no audio in this composition to detect beats from"
        ));
    }
    let rate = 48_000u32;
    let duration_s = c.duration.0.to_f64();
    let samples = lumit_ui::export::mixdown(&inputs.audio, rate, duration_s);
    let percent = sensitivity_percent.clamp(0, 100) as u8;
    let delta = lumit_audio::beat::delta_from_sensitivity(percent);
    let analysis = lumit_audio::beat::analyse_stereo(&samples, rate, delta);
    let times: Vec<f64> = analysis.onsets.iter().map(|o| o.time).collect();
    let snapped = lumit_audio::beat::snap_to_grid(&times, analysis.bpm, 0.045);
    let new_beats: Vec<lumit_core::markers::Marker> = snapped
        .iter()
        .zip(&analysis.onsets)
        .filter_map(|(t, o)| {
            let time = lumit_core::Rational::from_f64_on_grid(t.max(0.0), 1000).ok()?;
            Some(lumit_core::markers::Marker::beat(
                Uuid::now_v7(),
                time,
                o.confidence,
            ))
        })
        .collect();
    let markers = lumit_core::markers::with_regenerated_beats(&c.markers, new_beats);
    commit(bridge, Op::SetCompMarkers { comp, markers }, ctx)
}

/// Without the media + render features there is no audio pipeline, so detection
/// reports that calmly rather than pretending.
#[cfg(not(all(feature = "media", feature = "render")))]
pub(crate) fn detect_beats(
    _bridge: &mut Bridge,
    _comp_id: &str,
    _sensitivity_percent: i64,
) -> String {
    err_json("detect beats: this build has no audio pipeline (needs the media + render features)")
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

    #[test]
    fn clear_beat_markers_on_a_comp_with_none_is_a_calm_no_op() {
        let mut b = Bridge::new();
        new_composition(&mut b, "Scene");
        let comp = comp_id(&b);
        let reply = parse(&clear_beat_markers(&mut b, &comp));
        assert_eq!(reply["ok"], json!(true));
    }

    #[test]
    fn clear_beat_markers_removes_only_beats() {
        use lumit_core::markers::Marker;
        use lumit_core::ops::Op;
        let mut b = Bridge::new();
        new_composition(&mut b, "Scene");
        let comp = comp_id(&b);
        let cid = Uuid::parse_str(&comp).unwrap();
        // One user marker + two beat markers.
        let markers = vec![
            Marker::user(Uuid::now_v7(), lumit_core::Rational::new(1, 1).unwrap()),
            Marker::beat(
                Uuid::now_v7(),
                lumit_core::Rational::new(2, 1).unwrap(),
                0.9,
            ),
            Marker::beat(
                Uuid::now_v7(),
                lumit_core::Rational::new(3, 1).unwrap(),
                0.8,
            ),
        ];
        b.store
            .commit(Op::SetCompMarkers { comp: cid, markers })
            .unwrap();
        let before = parse(&snapshot(&b));
        // The comp block markers count the user + beats (3 total frames). The
        // comp is nested one level under the auto-folder, so search children too.
        let comp_markers = |snap: &Value| -> usize {
            for item in snap["items"].as_array().unwrap() {
                let found = if item["kind"] == json!("composition") {
                    Some(item)
                } else {
                    item["children"]
                        .as_array()
                        .and_then(|ch| ch.iter().find(|c| c["kind"] == json!("composition")))
                };
                if let Some(c) = found {
                    return c["comp"]["markers"].as_array().unwrap().len();
                }
            }
            0
        };
        assert_eq!(comp_markers(&before), 3);
        let after = parse(&clear_beat_markers(&mut b, &comp));
        assert_eq!(comp_markers(&after), 1, "only the user marker remains");
    }

    /// Without an audio track (or a GPU adapter on a headless CI box) detection
    /// is a calm note, never a crash. This holds in every feature configuration:
    /// with the features, `with_export_inputs`/no-audio returns the note; without
    /// them, the feature-less stub returns its own note.
    #[test]
    fn detect_beats_on_a_silent_comp_is_a_calm_note() {
        let mut b = Bridge::new();
        new_composition(&mut b, "Scene");
        let comp = comp_id(&b);
        let reply = parse(&detect_beats(&mut b, &comp, 50));
        assert_eq!(reply["ok"], json!(false));
        assert!(reply["error"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("beat"));
    }
}
