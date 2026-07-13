//! Retime: the map from a clip's local time to source time — the evaluation
//! core of docs/04-RETIMING.md §2–§5 (binding), with the cubic solve from
//! docs/impl/keyframe-eval.md §2/§4.
//!
//! In plain terms: a Retime answers one question — "when the clip's own clock
//! reads t, which moment of the source footage is on screen?". The answer is a
//! curve made of segments. A *Rate* segment speaks Vegas: "play at 200%,
//! easing down to 50%". A *Map* segment speaks After Effects: "be at source
//! second 3.2 by clip second 1.0", shaped by tangent handles. Both kinds meet
//! at *boundaries*, and every boundary stores its exact source position as a
//! fraction — never a rounded decimal — so cutting and re-editing a ramp can
//! never nudge a frame off a beat. Rendering evaluates the curve in fast
//! floating point; the exact fractions are the durable truth the floats are
//! recomputed from.
//!
//! Scope note: this module is the maths only. Overrun clamping (§7), the two
//! graph-editor lenses (§9), cutting (§8) and the flow interpolation engine
//! (§10) build on top of it and live elsewhere.

use crate::time::{Rational, TimeError};
use serde::{Deserialize, Serialize};

/// What can go wrong inside retime maths or structure checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RetimeError {
    /// Rational arithmetic failed even after the §4.1 fallback (only
    /// reachable with astronomically out-of-range times).
    #[error("retime arithmetic failed: {0}")]
    Arithmetic(#[from] TimeError),
    /// The store's shape breaks an invariant (docs/04-RETIMING.md §3).
    #[error("invalid retime structure: {0}")]
    InvalidStructure(&'static str),
}

/// The shape of a speed transition inside a [`RateSegment`] — deliberately
/// the Vegas fade-type vocabulary (docs/04-RETIMING.md §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ease {
    /// Straight speed ramp.
    Linear,
    /// Lingers at the starting speed, transitions late.
    Slow,
    /// Transitions early, settles at the ending speed.
    Fast,
    /// S-curve: gentle at both ends.
    Smooth,
    /// Inverse S-curve: brisk at both ends.
    Sharp,
}

impl Ease {
    /// E(1), the exact integral of the ease over the whole segment
    /// (docs/04-RETIMING.md §4.1 table). This is the number that makes
    /// boundary source positions exact: how much of the speed *change*
    /// contributes to the total source advance.
    pub fn e_at_1(self) -> Rational {
        let (num, den) = match self {
            Ease::Linear | Ease::Smooth | Ease::Sharp => (1, 2),
            Ease::Slow => (1, 3),
            Ease::Fast => (2, 3),
        };
        // Fixed single-digit fractions cannot fail to construct; the
        // fallback value is unreachable.
        Rational::new(num, den).unwrap_or(Rational::ZERO)
    }

    /// E(u) = ∫₀ᵘ e(w) dw in f64, for per-sample rendering
    /// (docs/04-RETIMING.md §4.1 table, including the piecewise Smooth and
    /// Sharp forms).
    pub fn big_e(self, u: f64) -> f64 {
        match self {
            Ease::Linear => u * u / 2.0,
            Ease::Slow => u * u * u / 3.0,
            Ease::Fast => u * u - u * u * u / 3.0,
            Ease::Smooth => {
                if u <= 0.5 {
                    2.0 * u * u * u / 3.0
                } else {
                    let w = 1.0 - u;
                    u + 2.0 * w * w * w / 3.0 - 0.5
                }
            }
            Ease::Sharp => {
                if u <= 0.5 {
                    u * u - 2.0 * u * u * u / 3.0
                } else {
                    2.0 * u * u * u / 3.0 - u * u + u - 1.0 / 6.0
                }
            }
        }
    }

    /// e(u), the speed-profile shape itself (0 at the segment start, 1 at
    /// the end). Used for the instantaneous speed readout.
    pub fn small_e(self, u: f64) -> f64 {
        match self {
            Ease::Linear => u,
            Ease::Slow => u * u,
            Ease::Fast => 2.0 * u - u * u,
            Ease::Smooth => {
                if u <= 0.5 {
                    2.0 * u * u
                } else {
                    let w = 1.0 - u;
                    1.0 - 2.0 * w * w
                }
            }
            Ease::Sharp => {
                if u <= 0.5 {
                    2.0 * u - 2.0 * u * u
                } else {
                    2.0 * u * u - 2.0 * u + 1.0
                }
            }
        }
    }
}

/// How fractional source positions become pixels (docs/04-RETIMING.md §10) —
/// a per-clip render policy, orthogonal to the retime map itself.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum Interpolation {
    /// Round to the nearest source frame — crisp, deterministic, the
    /// gaming-footage default.
    #[default]
    Nearest,
    /// Crossfade the two neighbouring frames.
    Blend,
    /// Optical-flow synthesis of the in-between frame.
    Flow(FlowParams),
}

/// Optical-flow parameters (docs/08-EFFECTS.md). Placeholder for now: the
/// flow engine is future work, but the policy must already round-trip
/// project files, so the shape exists with only the forward-compat map.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FlowParams {
    /// Unknown fields from newer Kiriko versions (docs/10-FILE-FORMAT.md §1.1).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// One point where two segments meet (or the curve starts/ends). Stores the
/// exact local time and the exact source position — the "frame on the beat
/// stays on the beat" guarantee lives in these two fractions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Boundary {
    /// Local time (seconds into the clip).
    pub t: Rational,
    /// Source time — exact, shared by both adjacent segments (C0).
    pub s: Rational,
    /// When true, edits keep the speed equal on both sides of this boundary
    /// (docs/04-RETIMING.md §6.1). Evaluation ignores it.
    #[serde(default)]
    pub smooth: bool,
    /// Unknown fields from newer Kiriko versions (docs/10-FILE-FORMAT.md §1.1).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Boundary {
    pub fn new(t: Rational, s: Rational) -> Self {
        Self {
            t,
            s,
            smooth: false,
            extra: serde_json::Map::new(),
        }
    }
}

/// One span of the retime curve, in one of the two native vocabularies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RetimeSegment {
    /// Speed-native: constant or eased speed (Vegas semantics).
    Rate(RateSegment),
    /// Value-native: cubic source-time curve (After Effects semantics).
    Map(MapSegment),
}

/// Speed-defined segment. Source advance is a closed-form integral
/// (docs/04-RETIMING.md §4.1): speed runs from `v0` to `v1` along the ease.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateSegment {
    /// Speed at the segment start (1 = 100%, 0 = freeze).
    pub v0: Rational,
    /// Speed at the segment end.
    pub v1: Rational,
    /// Shape of the speed transition between them.
    pub ease: Ease,
    /// Unknown fields from newer Kiriko versions (docs/10-FILE-FORMAT.md §1.1).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl RateSegment {
    pub fn new(v0: Rational, v1: Rational, ease: Ease) -> Self {
        Self {
            v0,
            v1,
            ease,
            extra: serde_json::Map::new(),
        }
    }
}

/// Value-defined segment: an x-monotone parametric cubic bezier in (t, s),
/// AE-compatible (docs/04-RETIMING.md §4.2, K-025). Endpoint positions come
/// from the two boundaries; this stores only the handle description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapSegment {
    /// Outgoing speed (source seconds per local second) at the start.
    pub m0: Rational,
    /// Incoming speed at the end.
    pub m1: Rational,
    /// Outgoing influence — how far the start handle reaches, in (0, 1].
    pub b0: Rational,
    /// Incoming influence, in (0, 1].
    pub b1: Rational,
    /// Unknown fields from newer Kiriko versions (docs/10-FILE-FORMAT.md §1.1).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl MapSegment {
    pub fn new(m0: Rational, m1: Rational, b0: Rational, b1: Rational) -> Self {
        Self {
            m0,
            m1,
            b0,
            b1,
            extra: serde_json::Map::new(),
        }
    }
}

/// One retime store. Owned by a Clip or by a Footage/Precomp layer
/// (docs/03-DATA-MODEL.md); this module is only the curve and its maths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Retime {
    /// n + 1 boundaries for n segments. `boundaries[0].t == 0`, the last sits
    /// at the clip duration. Strictly increasing in t. Authoritative for
    /// evaluation and cutting.
    pub boundaries: Vec<Boundary>,
    /// n segments; `segments[i]` spans `boundaries[i] .. boundaries[i + 1]`.
    pub segments: Vec<RetimeSegment>,
    /// Reverse gate (docs/04-RETIMING.md §6.2), default off. While off,
    /// evaluation clamps RateSegment speeds to ≥ 0, so the curve never runs
    /// backwards; MapSegment monotonicity is an editing-time invariant and
    /// is not re-checked per sample.
    #[serde(default)]
    pub allow_reverse: bool,
    /// Frame interpolation policy (§10). Default Nearest.
    #[serde(default)]
    pub interpolation: Interpolation,
    /// Unknown fields from newer Kiriko versions (docs/10-FILE-FORMAT.md §1.1).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Retime {
    /// The "no retiming" store (docs/04-RETIMING.md §3 default state): one
    /// 100%-speed segment across `[0, duration]`, source running from
    /// `source_in`. Evaluates as `f(t) = source_in + t` — a pure pass-through
    /// that must render identically to Retime being absent.
    pub fn identity(duration: Rational, source_in: Rational) -> Self {
        // source_in + duration is exact for any real media; the fallback
        // chain below only degrades past ~400 years of source time, where a
        // frozen tail beats a panic (engine crates never panic, K-011).
        let s_end = add_with_flick_fallback(source_in, duration).unwrap_or(source_in);
        Self {
            boundaries: vec![
                Boundary::new(Rational::ZERO, source_in),
                Boundary::new(duration, s_end),
            ],
            segments: vec![RetimeSegment::Rate(RateSegment::new(
                Rational::ONE,
                Rational::ONE,
                Ease::Linear,
            ))],
            allow_reverse: false,
            interpolation: Interpolation::default(),
            extra: serde_json::Map::new(),
        }
    }

    /// A single constant-speed retime over [0, `duration`] (local time),
    /// source running from `source_in` at `speed` (1 = 100%). This is the
    /// simple "play this clip faster/slower" case the timeline speed control
    /// produces; the graph-editor lenses build richer stores later.
    pub fn constant_speed(duration: Rational, source_in: Rational, speed: Rational) -> Self {
        let mut r = Self {
            boundaries: vec![
                Boundary::new(Rational::ZERO, source_in),
                Boundary::new(duration, source_in),
            ],
            segments: vec![RetimeSegment::Rate(RateSegment::new(
                speed,
                speed,
                Ease::Linear,
            ))],
            allow_reverse: speed.is_negative(),
            interpolation: Interpolation::default(),
            extra: serde_json::Map::new(),
        };
        // Fill the end boundary's source position exactly from the rate.
        let _ = r.recompute_boundaries();
        r
    }

    /// Structural sanity (docs/04-RETIMING.md §3 invariants): n + 1
    /// boundaries for n segments, first boundary at local time zero,
    /// boundary times strictly increasing.
    pub fn validate(&self) -> Result<(), RetimeError> {
        if self.segments.is_empty() {
            return Err(RetimeError::InvalidStructure(
                "a retime needs at least one segment",
            ));
        }
        if self.boundaries.len() != self.segments.len() + 1 {
            return Err(RetimeError::InvalidStructure(
                "boundary count must be segment count plus one",
            ));
        }
        if self.boundaries[0].t != Rational::ZERO {
            return Err(RetimeError::InvalidStructure(
                "the first boundary must sit at local time zero",
            ));
        }
        if self
            .boundaries
            .windows(2)
            .any(|pair| pair[1].t <= pair[0].t)
        {
            return Err(RetimeError::InvalidStructure(
                "boundary times must strictly increase",
            ));
        }
        Ok(())
    }

    /// Re-derive every boundary source position downstream of a RateSegment,
    /// exactly (docs/04-RETIMING.md §4.1 boundary consistency):
    ///
    /// ```text
    /// s[i+1] = s[i] + d · [ v0 + (v1 − v0) · E(1) ]
    /// ```
    ///
    /// all in rational arithmetic, so repeated edits never accumulate drift.
    /// MapSegment boundaries are left untouched — a map's endpoints *are*
    /// its boundaries, so the stored `s` is already authoritative. Speeds
    /// respect the reverse gate (negative speeds count as zero while
    /// `allow_reverse` is off), keeping boundaries consistent with what
    /// `evaluate` renders.
    pub fn recompute_boundaries(&mut self) -> Result<(), RetimeError> {
        self.validate()?;
        for i in 0..self.segments.len() {
            if let RetimeSegment::Rate(seg) = &self.segments[i] {
                let (v0, v1) = clamped_speeds(seg, self.allow_reverse);
                let d = self.boundaries[i + 1].t.checked_sub(self.boundaries[i].t)?;
                let advance = rate_advance(d, v0, v1, seg.ease)?;
                self.boundaries[i + 1].s = add_with_flick_fallback(self.boundaries[i].s, advance)?;
            }
        }
        Ok(())
    }

    /// Resolve local time `t` (seconds) to a source time (docs/04-RETIMING.md
    /// §4.3). `t` is clamped into the local domain `[0, D]`; the result is
    /// deliberately *not* clamped to the source extent — that clamp is what
    /// defines overrun (§7) and happens at a later stage.
    ///
    /// Per-sample evaluation is f64 by design; the rational boundaries are
    /// the exact anchors it works from.
    pub fn evaluate(&self, t: f64) -> f64 {
        let Some((i, t)) = self.locate(t) else {
            // Structurally unusable store: hold the first known source
            // position rather than fault (engine crates never panic).
            return self.boundaries.first().map_or(0.0, |b| b.s.to_f64());
        };
        let (lo, hi) = (&self.boundaries[i], &self.boundaries[i + 1]);
        let (t0, t1) = (lo.t.to_f64(), hi.t.to_f64());
        let d = t1 - t0;
        if d <= 0.0 {
            return lo.s.to_f64();
        }
        match &self.segments[i] {
            RetimeSegment::Rate(seg) => {
                let u = ((t - t0) / d).clamp(0.0, 1.0);
                let (v0, v1) = clamped_speeds(seg, self.allow_reverse);
                let (v0, v1) = (v0.to_f64(), v1.to_f64());
                // f(t) = s_i + d·[v0·u + (v1 − v0)·E(u)]  (§4.1)
                lo.s.to_f64() + d * (v0 * u + (v1 - v0) * seg.ease.big_e(u))
            }
            RetimeSegment::Map(seg) => {
                let (x, y) = map_control_points(seg, lo, hi);
                bezier(&y, map_param_at(seg, &x, t))
            }
        }
    }

    /// Instantaneous speed df/dt at local time `t` (1.0 = 100%). For a
    /// RateSegment this is the speed profile itself, v0 + (v1 − v0)·e(u);
    /// for a MapSegment it is y′(u)/x′(u) (docs/04-RETIMING.md §4.2).
    pub fn speed_at(&self, t: f64) -> f64 {
        let Some((i, t)) = self.locate(t) else {
            return 0.0;
        };
        let (lo, hi) = (&self.boundaries[i], &self.boundaries[i + 1]);
        let (t0, t1) = (lo.t.to_f64(), hi.t.to_f64());
        let d = t1 - t0;
        if d <= 0.0 {
            return 0.0;
        }
        match &self.segments[i] {
            RetimeSegment::Rate(seg) => {
                let u = ((t - t0) / d).clamp(0.0, 1.0);
                let (v0, v1) = clamped_speeds(seg, self.allow_reverse);
                let (v0, v1) = (v0.to_f64(), v1.to_f64());
                v0 + (v1 - v0) * seg.ease.small_e(u)
            }
            RetimeSegment::Map(seg) => {
                let (x, y) = map_control_points(seg, lo, hi);
                let u = map_param_at(seg, &x, t);
                // x′ can legitimately touch zero at a 100%-influence handle;
                // the floor keeps the readout finite (speed is then simply
                // "very large", which is the truth of the curve there).
                bezier_deriv(&y, u) / bezier_deriv(&x, u).max(1e-12)
            }
        }
    }

    /// Clamp `t` into the local domain and find the segment containing it
    /// (binary search over the boundary list, §4.3). `None` means the store
    /// is structurally unusable and evaluation should degrade gracefully.
    fn locate(&self, t: f64) -> Option<(usize, f64)> {
        let first = self.boundaries.first()?;
        let last = self.boundaries.last()?;
        if self.segments.is_empty() || self.boundaries.len() != self.segments.len() + 1 {
            return None;
        }
        let t = t.clamp(first.t.to_f64(), last.t.to_f64());
        // Largest segment whose start boundary is ≤ t.
        let idx = self.boundaries.partition_point(|b| b.t.to_f64() <= t);
        Some((idx.saturating_sub(1).min(self.segments.len() - 1), t))
    }
}

/// Endpoint speeds with the reverse gate applied (docs/04-RETIMING.md §6.2):
/// while `allow_reverse` is off, negative speeds evaluate as zero. Every ease
/// is monotone, so clamping the endpoints clamps the whole profile (§4.1).
fn clamped_speeds(seg: &RateSegment, allow_reverse: bool) -> (Rational, Rational) {
    if allow_reverse {
        (seg.v0, seg.v1)
    } else {
        let floor = |v: Rational| if v.is_negative() { Rational::ZERO } else { v };
        (floor(seg.v0), floor(seg.v1))
    }
}

/// `a + b`, exact. On i64 overflow, follow the §4.1 precision policy: redo
/// the sum in i128 and round to the flick grid (1/705 600 000 s) — a
/// sub-nanosecond rounding reachable only under pathological editing.
fn add_with_flick_fallback(a: Rational, b: Rational) -> Result<Rational, RetimeError> {
    match a.checked_add(b) {
        Ok(v) => Ok(v),
        Err(TimeError::Overflow) => {
            let num = i128::from(a.num()) * i128::from(b.den())
                + i128::from(b.num()) * i128::from(a.den());
            let den = i128::from(a.den()) * i128::from(b.den());
            Rational::from_f64_on_grid(num as f64 / den as f64, Rational::FLICK_DEN)
                .map_err(RetimeError::from)
        }
        Err(e) => Err(RetimeError::from(e)),
    }
}

/// The exact source advance of one RateSegment: `d · [v0 + (v1 − v0)·E(1)]`
/// (docs/04-RETIMING.md §4.1). On i64 overflow, fall back to wide floating
/// point rounded onto the flick grid, per the same precision policy.
fn rate_advance(
    d: Rational,
    v0: Rational,
    v1: Rational,
    ease: Ease,
) -> Result<Rational, RetimeError> {
    let e1 = ease.e_at_1();
    let exact = v1
        .checked_sub(v0)
        .and_then(|dv| dv.checked_mul(e1))
        .and_then(|weighted| v0.checked_add(weighted))
        .and_then(|inner| d.checked_mul(inner));
    match exact {
        Ok(v) => Ok(v),
        Err(TimeError::Overflow) => {
            let approx = d.to_f64() * (v0.to_f64() + (v1.to_f64() - v0.to_f64()) * e1.to_f64());
            Rational::from_f64_on_grid(approx, Rational::FLICK_DEN).map_err(RetimeError::from)
        }
        Err(e) => Err(RetimeError::from(e)),
    }
}

/// The §4.2 control points for one MapSegment between its two boundaries,
/// in f64 for evaluation:
///
/// ```text
/// P0 = (t0,          s0)
/// P1 = (t0 + b0·d,   s0 + m0·b0·d)
/// P2 = (t1 − b1·d,   s1 − m1·b1·d)
/// P3 = (t1,          s1)
/// ```
fn map_control_points(seg: &MapSegment, lo: &Boundary, hi: &Boundary) -> ([f64; 4], [f64; 4]) {
    let (t0, s0) = (lo.t.to_f64(), lo.s.to_f64());
    let (t1, s1) = (hi.t.to_f64(), hi.s.to_f64());
    let d = t1 - t0;
    let (m0, m1) = (seg.m0.to_f64(), seg.m1.to_f64());
    let (b0, b1) = (seg.b0.to_f64(), seg.b1.to_f64());
    (
        [t0, t0 + b0 * d, t1 - b1 * d, t1],
        [s0, s0 + m0 * b0 * d, s1 - m1 * b1 * d, s1],
    )
}

/// Find the bezier parameter u with x(u) = t. The polynomial subclass
/// (b0 = b1 = 1/3, §4.2) makes x(u) linear, so u falls straight out; the
/// general case root-solves.
fn map_param_at(seg: &MapSegment, x: &[f64; 4], t: f64) -> f64 {
    if is_one_third(seg.b0) && is_one_third(seg.b1) {
        ((t - x[0]) / (x[3] - x[0])).clamp(0.0, 1.0)
    } else {
        solve_u(x, t)
    }
}

fn is_one_third(r: Rational) -> bool {
    r.num() == 1 && r.den() == 3
}

/// Cubic bezier over four scalar control points (Bernstein form).
fn bezier(p: &[f64; 4], u: f64) -> f64 {
    let w = 1.0 - u;
    w * w * w * p[0] + 3.0 * w * w * u * p[1] + 3.0 * w * u * u * p[2] + u * u * u * p[3]
}

fn bezier_deriv(p: &[f64; 4], u: f64) -> f64 {
    let w = 1.0 - u;
    3.0 * w * w * (p[1] - p[0]) + 6.0 * w * u * (p[2] - p[1]) + 3.0 * u * u * (p[3] - p[2])
}

/// Solve x(u) = t by Newton inside a shrinking bracket — the binding
/// algorithm of docs/impl/keyframe-eval.md §2 (the same solver as
/// `anim::CubicSpan::solve_u`): fast like Newton, and mathematically unable
/// to escape [0, 1] because a bisection bracket always backs it up. Run to
/// the ≤ 2⁻⁴⁸ relative tolerance of docs/04-RETIMING.md §4.3; 48 iterations
/// guarantee that even in the pure-bisection worst case (x′ = 0 flat spots
/// at 100%-influence handles), and Newton normally exits in a handful.
fn solve_u(x: &[f64; 4], t: f64) -> f64 {
    let (x0, x3) = (x[0], x[3]);
    if x3 <= x0 {
        return 0.0;
    }
    let tol = (x3 - x0) * 2.0_f64.powi(-48);
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    let mut u = ((t - x0) / (x3 - x0)).clamp(0.0, 1.0); // x ≈ identity guess
    for _ in 0..48 {
        let xu = bezier(x, u);
        if (xu - t).abs() <= tol {
            break;
        }
        if xu < t {
            lo = u;
        } else {
            hi = u;
        }
        let dxu = bezier_deriv(x, u);
        let newton = u - (xu - t) / dxu;
        u = if dxu > 1e-12 && newton > lo && newton < hi {
            newton
        } else {
            0.5 * (lo + hi)
        };
    }
    u
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn rat(n: i64, d: i64) -> Rational {
        Rational::new(n, d).unwrap()
    }

    /// Build a store from (t, s) boundary pairs and segments.
    fn store(bounds: &[(Rational, Rational)], segments: Vec<RetimeSegment>) -> Retime {
        Retime {
            boundaries: bounds.iter().map(|&(t, s)| Boundary::new(t, s)).collect(),
            segments,
            allow_reverse: false,
            interpolation: Interpolation::default(),
            extra: serde_json::Map::new(),
        }
    }

    fn rate(v0: Rational, v1: Rational, ease: Ease) -> RetimeSegment {
        RetimeSegment::Rate(RateSegment::new(v0, v1, ease))
    }

    #[test]
    fn identity_is_a_straight_pass_through() {
        let r = Retime::identity(rat(4, 1), rat(3, 2));
        r.validate().unwrap();
        for t in [0.0, 0.25, 1.0, 2.5, 4.0] {
            assert!((r.evaluate(t) - (1.5 + t)).abs() < 1e-9, "t = {t}");
            assert!((r.speed_at(t) - 1.0).abs() < 1e-9, "speed at {t}");
        }
        // Out-of-domain requests clamp to the ends of the local domain.
        assert!((r.evaluate(-1.0) - 1.5).abs() < 1e-9);
        assert!((r.evaluate(10.0) - 5.5).abs() < 1e-9);
        // The end boundary is the exact rational sum.
        assert_eq!(r.boundaries[1].s, rat(11, 2));
    }

    #[test]
    fn double_speed_covers_twice_the_source() {
        let mut r = store(
            &[(rat(0, 1), rat(5, 1)), (rat(2, 1), rat(0, 1))],
            vec![rate(rat(2, 1), rat(2, 1), Ease::Linear)],
        );
        r.recompute_boundaries().unwrap();
        assert_eq!(r.boundaries[1].s, rat(9, 1)); // 5 + 2·2, exactly
        assert!((r.evaluate(0.5) - 6.0).abs() < 1e-9);
        assert!((r.evaluate(1.5) - 8.0).abs() < 1e-9);
        assert!((r.speed_at(1.0) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn constant_speed_evaluates_as_linear_from_source_in() {
        // Half speed from source 2s over a 4s layer: at lt the source is
        // 2 + 0.5·lt; the end boundary is exactly 2 + 0.5·4 = 4.
        let r = Retime::constant_speed(rat(4, 1), rat(2, 1), rat(1, 2));
        assert_eq!(r.boundaries[1].s, rat(4, 1));
        assert!((r.evaluate(0.0) - 2.0).abs() < 1e-9);
        assert!((r.evaluate(2.0) - 3.0).abs() < 1e-9);
        assert!((r.evaluate(4.0) - 4.0).abs() < 1e-9);
        assert!((r.speed_at(1.0) - 0.5).abs() < 1e-9);
        r.validate().unwrap();
    }

    #[test]
    fn freeze_holds_one_source_position() {
        let mut r = store(
            &[(rat(0, 1), rat(7, 2)), (rat(3, 1), rat(99, 1))],
            vec![rate(rat(0, 1), rat(0, 1), Ease::Linear)],
        );
        r.recompute_boundaries().unwrap();
        assert_eq!(r.boundaries[1].s, rat(7, 2)); // frozen: no advance at all
        for t in [0.0, 0.6, 1.5, 2.9, 3.0] {
            assert!((r.evaluate(t) - 3.5).abs() < 1e-12, "t = {t}");
        }
        assert!(r.speed_at(1.0).abs() < 1e-12);
    }

    #[test]
    fn linear_ramp_matches_the_hand_integral() {
        // v: 1 → 3 over d = 1 with Linear ease. E(1) = 1/2, so
        // s_end = s0 + d·(v0 + (v1 − v0)/2) = 0 + 1·(1 + 1) = 2, exactly.
        let mut r = store(
            &[(rat(0, 1), rat(0, 1)), (rat(1, 1), rat(0, 1))],
            vec![rate(rat(1, 1), rat(3, 1), Ease::Linear)],
        );
        r.recompute_boundaries().unwrap();
        assert_eq!(r.boundaries[1].s, rat(2, 1));
        // Midpoint by hand: f(½) = v0·u + Δv·u²/2 = ½ + 2·⅛ = ¾.
        assert!((r.evaluate(0.5) - 0.75).abs() < 1e-12);
        assert!((r.evaluate(1.0) - 2.0).abs() < 1e-12);
        // And the speed profile itself: v(½) = 1 + 2·½ = 2.
        assert!((r.speed_at(0.5) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn boundary_consistency_matches_f64_evaluation_for_every_ease() {
        for ease in [
            Ease::Linear,
            Ease::Slow,
            Ease::Fast,
            Ease::Smooth,
            Ease::Sharp,
        ] {
            let mut r = store(
                &[(rat(0, 1), rat(1, 4)), (rat(2, 1), rat(0, 1))],
                vec![rate(rat(1, 2), rat(5, 2), ease)],
            );
            r.recompute_boundaries().unwrap();
            let exact_end = r.boundaries[1].s.to_f64();
            let evaluated_end = r.evaluate(2.0);
            assert!(
                (exact_end - evaluated_end).abs() < 1e-9,
                "{ease:?}: rational end {exact_end} vs evaluated {evaluated_end}"
            );
        }
    }

    #[test]
    fn ease_integrals_agree_with_their_exact_endpoints() {
        for ease in [
            Ease::Linear,
            Ease::Slow,
            Ease::Fast,
            Ease::Smooth,
            Ease::Sharp,
        ] {
            assert_eq!(ease.big_e(0.0), 0.0, "{ease:?} at 0");
            let exact = ease.e_at_1().to_f64();
            assert!((ease.big_e(1.0) - exact).abs() < 1e-12, "{ease:?} at 1");
            // The piecewise Smooth/Sharp forms join without a step at u = ½.
            let below = ease.big_e(0.5 - 1e-9);
            let above = ease.big_e(0.5 + 1e-9);
            assert!(
                (below - above).abs() < 1e-8,
                "{ease:?} join: {below} vs {above}"
            );
            // e(u) ≥ 0 on [0, 1] for every ease, so E must never decrease.
            let mut prev = 0.0;
            for i in 0..=100 {
                let e = ease.big_e(f64::from(i) / 100.0);
                assert!(e >= prev - 1e-12, "{ease:?} decreasing at {i}");
                prev = e;
            }
        }
    }

    #[test]
    fn reverse_speeds_clamp_when_reverse_is_off() {
        // Authored with negative speed but the gate off: behaves as a freeze
        // (speeds clamp to zero — §6.2 monotone clamp) in both the exact
        // boundary maths and evaluation.
        let mut r = store(
            &[(rat(0, 1), rat(5, 1)), (rat(2, 1), rat(0, 1))],
            vec![rate(rat(-1, 1), rat(-1, 1), Ease::Linear)],
        );
        r.recompute_boundaries().unwrap();
        assert_eq!(r.boundaries[1].s, rat(5, 1));
        assert!((r.evaluate(1.0) - 5.0).abs() < 1e-12);
        assert!(r.speed_at(1.0).abs() < 1e-12);

        // Same store with the gate on: genuine reverse.
        r.allow_reverse = true;
        r.recompute_boundaries().unwrap();
        assert_eq!(r.boundaries[1].s, rat(3, 1)); // 5 + 2·(−1)
        assert!((r.evaluate(1.0) - 4.0).abs() < 1e-12);
        assert!((r.speed_at(1.0) + 1.0).abs() < 1e-12);
    }

    #[test]
    fn worked_example_from_the_spec_is_bit_exact() {
        // docs/04-RETIMING.md §12.4: 100% / 850% / 20% / 100% over
        // t = 0, 1.2, 1.45, 2.8, 4 — the boundary rationals are given in the
        // spec and must reproduce bit-for-bit.
        let mut r = store(
            &[
                (rat(0, 1), rat(0, 1)),
                (rat(6, 5), rat(0, 1)),
                (rat(29, 20), rat(0, 1)),
                (rat(14, 5), rat(0, 1)),
                (rat(4, 1), rat(0, 1)),
            ],
            vec![
                rate(rat(1, 1), rat(1, 1), Ease::Linear),
                rate(rat(17, 2), rat(17, 2), Ease::Linear),
                rate(rat(1, 5), rat(1, 5), Ease::Linear),
                rate(rat(1, 1), rat(1, 1), Ease::Linear),
            ],
        );
        r.recompute_boundaries().unwrap();
        let expected = [
            rat(0, 1),
            rat(6, 5),
            rat(133, 40),
            rat(719, 200),
            rat(959, 200),
        ];
        for (i, want) in expected.iter().enumerate() {
            assert_eq!(r.boundaries[i].s, *want, "boundary {i}");
        }
        // Binary search lands mid-ramp correctly: halfway through the 850%
        // segment, f = 6/5 + 0.125·8.5.
        assert!((r.evaluate(1.325) - 2.2625).abs() < 1e-9);
    }

    #[test]
    fn general_influence_map_hits_its_endpoints() {
        // m0 = m1 = 0 with b0 = b1 = 1/4: an S-shaped ease-in/out in the
        // general-influence class (solver path, not the 1/3 fast path).
        let seg = MapSegment::new(rat(0, 1), rat(0, 1), rat(1, 4), rat(1, 4));
        let r = store(
            &[(rat(0, 1), rat(0, 1)), (rat(2, 1), rat(1, 1))],
            vec![RetimeSegment::Map(seg)],
        );
        assert!(r.evaluate(0.0).abs() < 1e-9);
        assert!((r.evaluate(2.0) - 1.0).abs() < 1e-9);
        // Symmetric handles: the midpoint maps to the middle of the source span.
        assert!((r.evaluate(1.0) - 0.5).abs() < 1e-9);
        // x-monotone curve: output is non-decreasing and inside the span.
        let mut prev = -1e-9;
        for i in 0..=200 {
            let v = r.evaluate(f64::from(i) / 100.0);
            assert!(v >= prev - 1e-9, "not monotone at sample {i}");
            assert!(
                (-1e-9..=1.0 + 1e-9).contains(&v),
                "out of range at {i}: {v}"
            );
            prev = v;
        }
    }

    #[test]
    fn full_influence_spike_stays_bracketed() {
        // b0 = b1 = 1: x′(u) = 0 at u = ½ — the case that diverges under
        // plain Newton. The bracketed solver must stay finite and in-range.
        let seg = MapSegment::new(rat(0, 1), rat(0, 1), rat(1, 1), rat(1, 1));
        let r = store(
            &[(rat(0, 1), rat(0, 1)), (rat(1, 1), rat(1, 1))],
            vec![RetimeSegment::Map(seg)],
        );
        for i in 0..=100 {
            let t = f64::from(i) / 100.0;
            let v = r.evaluate(t);
            assert!(v.is_finite());
            assert!((-1e-9..=1.0 + 1e-9).contains(&v), "t = {t}, v = {v}");
        }
        assert!((r.evaluate(0.5) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn linear_rate_and_its_polynomial_map_agree() {
        // §5.1 exactness, checked from both sides without a conversion API:
        // a Linear rate 1 → 3 equals the hand-built polynomial-subclass map
        // (m0 = v0, m1 = v1, b0 = b1 = 1/3) over the same boundaries.
        let bounds = [(rat(0, 1), rat(0, 1)), (rat(1, 1), rat(2, 1))];
        let as_rate = store(&bounds, vec![rate(rat(1, 1), rat(3, 1), Ease::Linear)]);
        let as_map = store(
            &bounds,
            vec![RetimeSegment::Map(MapSegment::new(
                rat(1, 1),
                rat(3, 1),
                rat(1, 3),
                rat(1, 3),
            ))],
        );
        for i in 0..=100 {
            let t = f64::from(i) / 100.0;
            let (a, b) = (as_rate.evaluate(t), as_map.evaluate(t));
            assert!((a - b).abs() < 1e-12, "t = {t}: rate {a} vs map {b}");
        }
    }

    #[test]
    fn validate_rejects_malformed_stores() {
        let good = Retime::identity(rat(1, 1), rat(0, 1));

        let mut no_segments = good.clone();
        no_segments.segments.clear();
        assert!(no_segments.validate().is_err());

        let mut miscounted = good.clone();
        miscounted
            .boundaries
            .push(Boundary::new(rat(2, 1), rat(2, 1)));
        assert!(miscounted.validate().is_err());

        let mut late_start = good.clone();
        late_start.boundaries[0].t = rat(1, 2);
        assert!(late_start.validate().is_err());

        let mut not_increasing = good.clone();
        not_increasing.boundaries[1].t = rat(0, 1);
        assert!(not_increasing.validate().is_err());

        assert!(good.validate().is_ok());
    }

    #[test]
    fn overflow_falls_back_to_the_flick_grid() {
        // Coprime denominators near 2^32 (nothing cancels): the exact sum's
        // denominator would pass 2^63, so the §4.1 fallback rounds onto the
        // flick grid instead of failing.
        let a = rat(1_000_003, 4_294_967_297); // prime num, den = 2^32 + 1
        let b = rat(1_000_003, 8_589_934_591); // prime num, den = 2^33 − 1
        assert!(a.checked_add(b).is_err(), "expected raw overflow");
        let sum = add_with_flick_fallback(a, b).unwrap();
        let exact = 1_000_003.0 / 4_294_967_297.0 + 1_000_003.0 / 8_589_934_591.0;
        assert!(
            (sum.to_f64() - exact).abs() < 2.0 / Rational::FLICK_DEN as f64,
            "grid-rounded {sum:?} too far from {exact}"
        );
    }

    #[test]
    fn retime_round_trips_through_serde() {
        let mut r = Retime::identity(rat(4, 1), rat(0, 1));
        r.segments.push(RetimeSegment::Map(MapSegment::new(
            rat(1, 1),
            rat(0, 1),
            rat(1, 4),
            rat(1, 2),
        )));
        r.boundaries.push(Boundary::new(rat(6, 1), rat(9, 2)));
        r.interpolation = Interpolation::Flow(FlowParams::default());
        r.allow_reverse = true;
        let json = serde_json::to_value(&r).unwrap();
        let back: Retime = serde_json::from_value(json).unwrap();
        assert_eq!(back, r);
    }
}
