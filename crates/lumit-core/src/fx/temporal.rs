use super::*;
use crate::model::{EffectInstance, EffectNamespace, EffectValue};

/// Which layers a Posterize Time effect (docs/08 §3.25) holds in time — the
/// owner's Scope choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosterizeScope {
    /// Adjustment behaviour: the composite of everything below the effect's
    /// layer renders at the held time (the owner's global stop-motion pass).
    EverythingBelow,
    /// Only the layer's own source and effect stack sample the held time.
    ThisLayer,
}

/// A Posterize Time effect resolved at a layer time (docs/08 §3.25,
/// docs/impl/temporal-rerender.md): the coarse grid it snaps time to and the
/// scope it covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PosterizeParams {
    /// Posterised frame rate in fps — the grid the current time snaps down to.
    pub rate: f64,
    /// Grid phase offset in comp seconds (shifts where the steps land).
    pub phase: f64,
    pub scope: PosterizeScope,
}

/// The held comp time for Posterize Time (docs/08 §3.25): the current comp time
/// `t` snapped down to the coarser `rate`-fps grid, offset by `phase` —
/// `floor((t − phase)·rate)/rate + phase`. A degenerate grid (`rate <= 0`)
/// holds nothing and returns `t` unchanged, never dividing by zero (the engine
/// no-panic rule, docs/14). Pure and deterministic, so the two comp times that
/// share a held frame re-render to the same pixels (docs/impl/temporal-rerender
/// §6).
pub fn posterize_held_time(t: f64, rate: f64, phase: f64) -> f64 {
    if rate <= 0.0 {
        return t;
    }
    ((t - phase) * rate).floor() / rate + phase
}

/// The first enabled built-in Posterize Time effect in a live stack, resolved
/// at layer time `lt`. None when the stack is bypassed or carries none — so a
/// layer with no Posterize pays nothing and renders normally. A stack with more
/// than one takes the first in order (a single time-hold per layer in v1).
pub fn stack_posterize(
    effects: &[EffectInstance],
    fx_on: bool,
    lt: f64,
) -> Option<PosterizeParams> {
    if !fx_on {
        return None;
    }
    effects
        .iter()
        .filter(|e| e.enabled && e.effect.namespace == EffectNamespace::Builtin)
        .find(|e| e.effect.match_name == "posterize_time")
        .map(|e| {
            let rate = e.float_at("rate", lt).unwrap_or(12.0);
            let phase = e.float_at("phase", lt).unwrap_or(0.0);
            let scope = match e.param("scope") {
                Some(EffectValue::Choice(1)) => PosterizeScope::ThisLayer,
                _ => PosterizeScope::EverythingBelow,
            };
            PosterizeParams { rate, phase, scope }
        })
}

/// The union of source-relative frame offsets a layer's live effect stack
/// needs (docs/08 §1.3 `temporal`), always sorted and always containing 0
/// (the current frame). `&[0]` when the stack is bypassed, empty, or every
/// effect is a plain single-frame one — so a layer with no temporal effect
/// pays nothing. The render pipeline decodes the layer's source at each of
/// these offsets so a temporal effect (echo, flow motion blur, datamosh)
/// can read its neighbours.
pub fn stack_temporal_window(effects: &[EffectInstance], fx_on: bool) -> Vec<i32> {
    let mut offsets = vec![0i32];
    if fx_on {
        for e in effects.iter().filter(|e| e.enabled) {
            if e.effect.namespace != EffectNamespace::Builtin {
                continue;
            }
            if let Some(s) = schema(&e.effect.match_name) {
                offsets.extend_from_slice(s.traits.temporal);
            }
        }
    }
    offsets.sort_unstable();
    offsets.dedup();
    offsets
}

/// True when any live effect in the stack reads frames other than the
/// current one — the cheap gate the render/cache paths check before doing
/// any neighbour-frame work.
pub fn stack_is_temporal(effects: &[EffectInstance], fx_on: bool) -> bool {
    fx_on
        && effects
            .iter()
            .filter(|e| e.enabled && e.effect.namespace == EffectNamespace::Builtin)
            .any(|e| {
                schema(&e.effect.match_name)
                    .is_some_and(|s| s.traits.temporal.iter().any(|&o| o != 0))
            })
}

/// The neighbour offset a live effect wants a dense **flow field** measured
/// against (per-pixel motion vectors between the current source frame and
/// that neighbour), computed in the decode worker and handed to the kernel
/// as a texture — the gate mirroring [`stack_is_temporal`] that the render/
/// decode paths check before doing any flow work. Flow motion blur (docs/08
/// §3.2) wants `1` (the +1 neighbour); Datamosh (§3.12, K-107) wants `-1` —
/// both purely static reads of the schema's own match name now (K-107
/// dropped the dynamic per-instance check a combined Glitch effect used to
/// need). Both effects are also temporal (their windows reach that same
/// offset), so the neighbour machinery already fetches the source frame the
/// flow is measured against.
///
/// A layer can carry only one flow field per frame in v1
/// ([`crate`]-external callers store it in a single `Option` slot) — if a
/// stack somehow has both a live Motion blur and a live Datamosh, the first
/// one encountered in stack order wins and the other's flow-dependent
/// behaviour degrades to its own missing-field passthrough (never a fault;
/// pinned by test, K-104).
pub fn stack_flow_neighbour(effects: &[EffectInstance], fx_on: bool) -> Option<i32> {
    if !fx_on {
        return None;
    }
    for e in effects
        .iter()
        .filter(|e| e.enabled && e.effect.namespace == EffectNamespace::Builtin)
    {
        if e.effect.match_name == "motion_blur" {
            return Some(1);
        }
        if e.effect.match_name == "datamosh" {
            return Some(-1);
        }
    }
    None
}
