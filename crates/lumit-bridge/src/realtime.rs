//! The realtime preview-tier controller, wired into the Viewer render path
//! (K-030/K-171).
//!
//! # In plain terms
//!
//! During playback the machine may not keep up at full resolution. The realtime
//! controller watches how long frames actually take and, when they get too slow
//! for the frame budget, drops the preview to a coarser resolution (Full → Half
//! → Third → Quarter); when frames are comfortably fast for a sustained stretch
//! it earns the resolution back — quick to worsen, slow to improve, so the
//! picture never flickers between qualities. That decision core is
//! [`lumit_eval::schedule::RealtimeController`], already built and tested; it was
//! historically unwired (K-171). This module runs one instance for the session
//! and connects it to the bridge's pull-model rendering.
//!
//! The fit to the pull model: the Viewer render path measures the wall-clock
//! cost of each *genuine* GPU render (a cache hit is not a render, so it is not
//! measured) and reports it here via [`observe`]. The controller updates its
//! smoothed cost and picks the tier. Dart reads the tier back with
//! [`current`] (exposed as the `playback_tier` op) to show the Viewer readout
//! and, in **Auto** resolution mode, to choose the scale for the next frame.
//!
//! Manual override (the resolution picker set to Half/Third/Quarter rather than
//! Auto) is a Dart-side decision: Dart simply passes its chosen scale to the
//! render and ignores the tier. So it does not corrupt the controller, [`observe`]
//! only feeds a cost when the render was issued at the controller's *own* tier
//! scale (an Auto render). A manual render at a different scale is measured for
//! nothing — the controller keeps modelling the Auto tier. [`reset`] restarts the
//! controller (Dart calls it when playback stops, the comp changes, or the user
//! switches back to Auto), so a fresh run starts optimistic at Full.

use lumit_eval::schedule::{RealtimeController, COARSEST_TIER, FINEST_TIER};
use serde_json::json;
use std::sync::{Mutex, OnceLock};

/// The session-lifetime controller behind its own lock (independent of the
/// document and renderer locks — reading the tier never blocks an edit).
static CONTROLLER: OnceLock<Mutex<RealtimeController>> = OnceLock::new();

fn with_controller<R>(f: impl FnOnce(&mut RealtimeController) -> R) -> R {
    let mutex = CONTROLLER.get_or_init(|| Mutex::new(RealtimeController::new()));
    let mut guard = mutex.lock().unwrap_or_else(|poison| poison.into_inner());
    f(&mut guard)
}

/// The preview divisor for a tier (1 = Full, 2 = Half, 3 = Third, 4 = Quarter)
/// as a render scale (`1.0 / tier`). The one mapping Dart and the render path
/// share, so "am I rendering at the controller's tier?" is one comparison.
pub(crate) fn tier_scale(tier: u32) -> f32 {
    1.0 / tier.clamp(FINEST_TIER, COARSEST_TIER) as f32
}

/// Report one genuine render's measured `cost_secs` at frame rate `fps`, but
/// only when it was issued at the controller's own tier scale (an Auto render) —
/// a manual render at a different `scale` is not the controller's business and
/// is ignored, so it cannot mislead the model. Returns the tier in force after
/// the report (unchanged on an ignored cost). Called only from the render path
/// (the `render` feature); the tier read-back ops compile in every build.
#[cfg_attr(not(feature = "render"), allow(dead_code))]
pub(crate) fn observe(cost_secs: f64, fps: f64, scale: f32) -> u32 {
    with_controller(|c| {
        let expected = tier_scale(c.tier());
        // A small tolerance: Dart's Auto scale is derived from the same
        // `tier_scale`, so an exact match is expected, but float equality is
        // fragile — accept anything within half a tier step.
        if (scale - expected).abs() <= 0.01 {
            c.record(cost_secs, fps)
        } else {
            c.tier()
        }
    })
}

/// The tier currently in force (1..=4).
pub(crate) fn tier() -> u32 {
    with_controller(|c| c.tier())
}

/// Restart the controller — optimistic at Full again. Called when playback
/// stops, the composition changes, or the user switches back to Auto, so a fresh
/// run does not inherit a stale tier.
pub(crate) fn reset() {
    with_controller(|c| *c = RealtimeController::new());
}

/// The `playback_tier` reply: the current `tier` (1..=4) and its `scale`
/// (`1.0 / tier`), so the Viewer can show "Half", "Third" etc. and an Auto-mode
/// client can render the next frame at that scale.
pub(crate) fn playback_tier() -> String {
    let t = tier();
    json!({
        "ok": true,
        "tier": t,
        "scale": tier_scale(t),
    })
    .to_string()
}

/// Reset and return the fresh tier (the `reset_realtime` reply).
pub(crate) fn reset_reply() -> String {
    reset();
    playback_tier()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).expect("valid JSON")
    }

    /// The tier/scale mapping is the shared contract Dart relies on.
    #[test]
    fn tier_scale_maps_divisor_to_scale() {
        assert!((tier_scale(1) - 1.0).abs() < 1e-6);
        assert!((tier_scale(2) - 0.5).abs() < 1e-6);
        assert!((tier_scale(3) - 1.0 / 3.0).abs() < 1e-6);
        assert!((tier_scale(4) - 0.25).abs() < 1e-6);
        // Out-of-range tiers clamp to the valid divisor band.
        assert!((tier_scale(0) - 1.0).abs() < 1e-6);
        assert!((tier_scale(9) - 0.25).abs() < 1e-6);
    }

    /// The global-controller behaviours, in one test so the shared static is not
    /// raced by cargo's parallel runner: Auto-tier costs drop the tier and the
    /// `playback_tier` reply reports it; a manual-scale cost is ignored; reset
    /// restores Full.
    #[test]
    fn controller_wiring_end_to_end() {
        let fps = 60.0;
        reset();
        assert_eq!(tier(), 1, "starts optimistic at Full");

        // A manual render at a scale that is not the controller's tier (Quarter
        // while it sits at Full) is ignored — it cannot move the Auto model.
        for _ in 0..10 {
            observe(0.2, fps, 0.25);
        }
        assert_eq!(tier(), 1, "a manual-scale cost cannot move the tier");

        // Brutal 200 ms renders at the current Auto tier drop the tier fast.
        for _ in 0..10 {
            let scale = tier_scale(tier());
            observe(0.2, fps, scale);
        }
        assert!(
            tier() > 1,
            "slow Auto renders coarsen the preview, tier {}",
            tier()
        );
        let reply = parse(&playback_tier());
        assert_eq!(reply["ok"], json!(true));
        assert_eq!(reply["tier"].as_u64().unwrap(), tier() as u64);
        let scale = reply["scale"].as_f64().unwrap();
        assert!((scale - tier_scale(tier()) as f64).abs() < 1e-6);

        // Reset restores Full, and the reply reflects it.
        let reply = parse(&reset_reply());
        assert_eq!(reply["tier"], json!(1));
        assert_eq!(tier(), 1);
    }
}
