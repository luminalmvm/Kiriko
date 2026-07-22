//! Engine-side render cancellation (K-176, follow-up 2).
//!
//! # In plain terms
//!
//! The Dart worker is "latest-wins": when you scrub, it only wants the newest
//! frame and drops the reply to any older request. But dropping the *reply* does
//! not stop the *work* — a superseded comp render still ran to completion on the
//! worker, holding the renderer lock the next frame needed. This module lets the
//! engine skip that wasted work.
//!
//! Every render request carries a monotonic `generation` (the worker's
//! latest-wins counter). We keep the highest generation seen so far. A request
//! whose generation is *below* that high-water mark has already been superseded
//! by a newer one, so it aborts before starting the render rather than
//! completing.
//!
//! ## Granularity, honestly
//!
//! The headless renderer's `render_rgba` is one monolithic call (composite →
//! display-encode → GPU→CPU read-back); it exposes no seam to poll between its
//! internal wgpu submissions without reaching into `lumit-ui`. So the achievable
//! granularity is: **check once, after the renderer lock is acquired and before
//! the render begins.** That is exactly where the win is — the stale request
//! queued behind the lock is skipped instead of stealing a full GPU render from
//! the frame the user actually wants. A cache *hit* is always served regardless
//! of generation (it is cheap and correct); only a genuine miss consults the
//! high-water mark before rendering.

use std::sync::atomic::{AtomicU64, Ordering};

/// The highest render generation seen. A request at or above this may proceed;
/// one below it has been superseded and should abort.
static LATEST_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Whether a render at `generation` is still wanted (`true` = proceed, `false` =
/// superseded, skip). Called by the render path once it holds the renderer lock.
/// A render **reads** the high-water mark but never raises it — only an explicit
/// [`cancel_stale`] from the UI isolate does — so a low-priority render (e.g. the
/// throttled scope read-back) can never supersede a primary one, and equal or
/// higher generations always proceed.
#[cfg(feature = "render")]
pub(crate) fn should_render(generation: u64) -> bool {
    generation >= LATEST_GENERATION.load(Ordering::Acquire)
}

/// Mark every generation below `generation` as stale (Dart's
/// `render_cancel_stale`): the UI isolate calls this as it issues a new primary
/// render, so a stale render already queued behind the renderer lock is skipped
/// when it wakes. Monotonic (`fetch_max`), so an out-of-order call never lowers
/// the mark.
pub(crate) fn cancel_stale(generation: u64) {
    LATEST_GENERATION.fetch_max(generation, Ordering::AcqRel);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Publishing a newer generation (as the UI isolate does when it issues a new
    /// primary render) supersedes an older queued render, but equal-or-newer
    /// renders still proceed. A render itself never raises the mark.
    #[cfg(feature = "render")]
    #[test]
    fn a_superseded_generation_is_skipped() {
        // Use the HIGHEST base of any test touching the shared high-water mark
        // (fetch_max only ever raises it): this test asserts that `base + 10`
        // still proceeds, which a parallel test with a higher base would break
        // — the flake the old 1_000_000 base allowed when the sibling test's
        // 2_000_000 publish interleaved.
        let base = 3_000_000;
        cancel_stale(base + 10);
        assert!(
            should_render(base + 10),
            "the published generation proceeds"
        );
        assert!(
            !should_render(base + 5),
            "an older render queued behind it is skipped"
        );
        // A render reading the mark does not raise it, so a still-newer render
        // proceeds without any explicit publish.
        assert!(
            should_render(base + 11),
            "a newer render proceeds and does not lower the mark for others"
        );
        assert!(
            should_render(base + 10),
            "the mark was not raised by a read"
        );
    }

    /// `cancel_stale` raises the high-water mark so a later older render aborts.
    #[cfg(feature = "render")]
    #[test]
    fn cancel_stale_supersedes_lower_generations() {
        let base = 2_000_000;
        cancel_stale(base + 100);
        assert!(
            !should_render(base + 50),
            "a request below a cancelled generation is skipped"
        );
    }

    /// Without the render feature `cancel_stale` is still callable (a no-op path
    /// the FFI keeps in every build), never a panic.
    #[test]
    fn cancel_stale_is_always_callable() {
        cancel_stale(1);
    }
}
