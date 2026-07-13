//! Sequence layers: clips cut back-to-back on one row (docs/03-DATA-MODEL.md
//! §5.3, docs/04-RETIMING.md §1.3). This is Kiriko's Vegas-style editing
//! surface.
//!
//! In plain terms: a Sequence layer is one timeline row holding a run of
//! **clips** laid end to end. Each clip points at a source (a footage item or
//! a comp), carries its own trim and its own [`Retime`] ramp, and sits at an
//! exact place on the row. Clips never overlap; a gap between them shows
//! through as transparent. To draw the layer at a given moment you ask "which
//! clip is under the playhead, and which moment of its source does that map
//! to?" — that resolution is all this module does. Turning that source moment
//! into pixels, and the layer's own masks/effects/transform, happen above.
//!
//! Scope note: this is the resolution model and its invariants only. Wiring
//! it into `LayerKind` and the render paths is the next step and lives
//! elsewhere; cutting (§8) and the graph lenses (§9) build on top.

use crate::retime::{Interpolation, Retime};
use crate::time::Rational;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What a clip plays: one footage item or one nested composition
/// (docs/03-DATA-MODEL.md §5.3 ClipSource).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipSource {
    Footage(Uuid),
    Comp(Uuid),
}

/// One clip on a Sequence layer (docs/03-DATA-MODEL.md §5.3). Times are exact
/// rationals in seconds; `place_*` are on the layer's timeline, `source_*`
/// index into the clip's source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: Uuid,
    pub source: ClipSource,
    /// Trim into the source (seconds).
    pub source_in: Rational,
    /// Exclusive trim end (seconds).
    pub source_out: Rational,
    /// Where the clip starts on the layer's timeline (seconds).
    pub place_start: Rational,
    /// How long the clip occupies on the layer's timeline (seconds).
    pub place_duration: Rational,
    /// The clip's retime map: clip-local time → source time. Its first
    /// boundary's source position is the clip's effective source in.
    pub retime: Retime,
    /// How fractional source moments become pixels (render policy).
    #[serde(default)]
    pub interpolation: Interpolation,
    /// Unknown fields from newer Kiriko versions (docs/10-FILE-FORMAT.md §1.1).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Clip {
    /// A plain (un-retimed) clip of `source` placed at `place_start` for
    /// `place_duration`, playing its source from `source_in` at natural rate.
    pub fn new(
        source: ClipSource,
        source_in: Rational,
        source_out: Rational,
        place_start: Rational,
        place_duration: Rational,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            source,
            source_in,
            source_out,
            place_start,
            place_duration,
            retime: Retime::identity(place_duration, source_in),
            interpolation: Interpolation::default(),
            extra: serde_json::Map::new(),
        }
    }

    /// Where the clip ends on the layer timeline (exclusive).
    pub fn place_end(&self) -> Rational {
        self.place_start
            .checked_add(self.place_duration)
            .unwrap_or(self.place_start)
    }

    /// True when layer-local time `lt` (seconds) falls within this clip.
    pub fn contains(&self, lt: f64) -> bool {
        lt >= self.place_start.to_f64() && lt < self.place_end().to_f64()
    }

    /// The source time (seconds) shown at layer-local time `lt`, via the
    /// clip's retime (which maps clip-local time → source time). Only
    /// meaningful when [`Self::contains`] is true.
    pub fn source_time(&self, lt: f64) -> f64 {
        let clip_time = lt - self.place_start.to_f64();
        self.retime.evaluate(clip_time)
    }
}

/// The clip active at layer-local time `lt`, or None if `lt` is in a gap
/// (transparent) or past the end. Clips must not overlap, so at most one
/// matches; the first match wins defensively.
pub fn active_clip(clips: &[Clip], lt: f64) -> Option<&Clip> {
    clips.iter().find(|c| c.contains(lt))
}

/// Resolve layer-local time `lt` to `(active clip id, source, source time)`,
/// or None in a gap. The one query the renderer needs.
pub fn resolve(clips: &[Clip], lt: f64) -> Option<(Uuid, ClipSource, f64)> {
    active_clip(clips, lt).map(|c| (c.id, c.source, c.source_time(lt)))
}

/// Do any two clips overlap on the layer timeline? (docs/03-DATA-MODEL.md
/// §5.3 invariant: clips MUST NOT overlap — this is the check editors run
/// after a move before committing.)
pub fn has_overlap(clips: &[Clip]) -> bool {
    let mut spans: Vec<(f64, f64)> = clips
        .iter()
        .map(|c| (c.place_start.to_f64(), c.place_end().to_f64()))
        .collect();
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    spans.windows(2).any(|w| w[1].0 < w[0].1)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn rat(n: i64, d: i64) -> Rational {
        Rational::new(n, d).unwrap()
    }

    fn clip(src: Uuid, place_start: i64, place_dur: i64) -> Clip {
        Clip::new(
            ClipSource::Footage(src),
            rat(0, 1),
            rat(place_dur, 1),
            rat(place_start, 1),
            rat(place_dur, 1),
        )
    }

    #[test]
    fn resolution_picks_the_clip_under_the_playhead() {
        let (a, b) = (Uuid::now_v7(), Uuid::now_v7());
        // Clip A [0,2), then a gap [2,3), then clip B [3,5).
        let clips = vec![clip(a, 0, 2), clip(b, 3, 2)];
        assert_eq!(resolve(&clips, 1.0).unwrap().1, ClipSource::Footage(a));
        assert_eq!(resolve(&clips, 4.0).unwrap().1, ClipSource::Footage(b));
        // The gap and past-the-end render transparent (None).
        assert!(resolve(&clips, 2.5).is_none());
        assert!(resolve(&clips, 5.0).is_none());
        // Boundaries: start inclusive, end exclusive.
        assert!(resolve(&clips, 0.0).is_some());
        assert!(resolve(&clips, 2.0).is_none());
        assert!(resolve(&clips, 3.0).is_some());
    }

    #[test]
    fn source_time_runs_through_the_clip_retime() {
        // A clip at layer [2,6) whose source starts at 10s, played at half
        // speed: at layer time 4 (clip-local 2) the source is 10 + 0.5·2 = 11.
        let src = Uuid::now_v7();
        let mut c = clip(src, 2, 4);
        c.retime = Retime::constant_speed(rat(4, 1), rat(10, 1), rat(1, 2));
        assert!((c.source_time(2.0) - 10.0).abs() < 1e-9); // clip start
        assert!((c.source_time(4.0) - 11.0).abs() < 1e-9); // half speed
        assert!((c.source_time(6.0) - 12.0).abs() < 1e-9); // clip end
    }

    #[test]
    fn overlap_detection() {
        let s = Uuid::now_v7();
        // Back-to-back is fine (end-exclusive touching).
        assert!(!has_overlap(&[clip(s, 0, 2), clip(s, 2, 2)]));
        // A gap is fine.
        assert!(!has_overlap(&[clip(s, 0, 2), clip(s, 5, 2)]));
        // Genuine overlap is caught.
        assert!(has_overlap(&[clip(s, 0, 3), clip(s, 2, 2)]));
    }

    #[test]
    fn clip_round_trips_through_serde() {
        let c = clip(Uuid::now_v7(), 1, 4);
        let json = serde_json::to_string(&c).unwrap();
        let back: Clip = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}
