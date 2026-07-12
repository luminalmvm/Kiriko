# Playback scheduler: epochs, the job pool, and the audio clock

The scheduler is where "the UI thread never evaluates anything" (K-017) and "degrade,
never crash" (K-018) become code. Its three load-bearing ideas: **epoch cancellation**,
**bounded everything**, and **the audio clock as the only clock**.

## 1. Epochs

One global `AtomicU64` per *interaction context* (scrub epoch, playback epoch, device
epoch). Any event that invalidates in-flight work — playhead move, param drag tick, stop,
device loss — increments the relevant epoch.

```rust
pub struct Epoch(Arc<AtomicU64>);
pub struct EpochToken { epoch: Epoch, seen: u64 }
impl EpochToken {
    #[inline] pub fn cancelled(&self) -> bool {
        self.epoch.0.load(Ordering::Relaxed) != self.seen
    }
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.cancelled() { Err(Cancelled) } else { Ok(()) }
    }
}
```

- Every job closure receives a token; **every loop over frames, rows, nodes, or dispatches
  calls `token.check()?`** ([14-ENGINEERING-RULES.md](../14-ENGINEERING-RULES.md) makes
  this a review rule). Target granularity ≤ ~10 ms of work between checks.
- Cancelled jobs return `Err(Cancelled)` up their stack; the executor swallows it silently.
  Completed-but-stale results still enter the cache (the work is paid for —
  [06-RENDER-PIPELINE.md](../06-RENDER-PIPELINE.md) §6.3) but are not presented.
- There are no "kill thread" or force-abort paths anywhere. GPU dispatches are bounded
  short instead ([gpu-foundation.md](gpu-foundation.md) §4).

## 2. Thread topology

| Thread(s) | Owns | Never does |
|---|---|---|
| UI (main) | egui, input, document edits (single writer), snapshot publication | evaluation, decode, blocking waits |
| Worker pool (N = cores − 3, min 2) | graph evaluation, CPU effects, cache (de)serialisation | GPU submits, UI state |
| Decode threads (per active clip, pooled ≤ 16) | libav decode, frame index lookups | evaluation |
| GPU-submit (1) | queue submissions, presents, pool trim, device-loss recovery | waiting on locks held elsewhere |
| Audio (cpal callback, realtime) | mixing decoded PCM into the device buffer | allocation, locks, logging — **nothing blocking, ever** |
| IO (1) | project save/autosave/journal, disk cache reads/writes | anything latency-sensitive |

Pool: use **rayon** scoped into a dedicated `ThreadPool` (not the global one) — its
work-stealing is right for DAG fan-out, and cancellation is our epoch tokens, not rayon's.
Channels: `crossbeam::channel::bounded` everywhere; every `send` on a full queue is
back-pressure by design. Choose capacities once, in one constants module, documented.

## 3. The document snapshot handoff

UI thread applies operations → new immutable snapshot (`Arc<Document>`) → publish with
`arc_swap::ArcSwap`. Jobs capture the snapshot Arc at schedule time; a job never sees a
half-edited document, and two jobs disagreeing about the document is impossible by
construction. Renders are keyed by (snapshot content hashes), so an edit during playback
simply means the next scheduled frame uses the new snapshot — no locks, no waiting.

## 4. The audio clock (master)

- cpal output stream at the device's native rate (resample media to it, §media-io). In the
  callback: copy from a lock-free SPSC ring of pre-mixed samples; on underrun, output
  silence and set an atomic flag (the scheduler reacts, audio never blocks).
- The clock: `samples_consumed: AtomicU64`, incremented in the callback. Playback position
  = `start_comp_time + samples_consumed / rate − output_latency`, where `output_latency`
  is estimated once at stream start (cpal cannot report it reliably: measure buffer size ×
  periods, allow a user nudge setting, and don't obsess — A/V sync tolerance is ±half a
  frame, [13-PERFORMANCE-RULES.md](../13-PERFORMANCE-RULES.md)).
- Video presents chase this clock: each vsync, present the newest ring-buffer frame whose
  comp time ≤ clock. Hold last frame if the ring is late (audio keeps going); drop frames
  the clock has passed.

## 5. Playback pipeline

On play: capture snapshot, bump playback epoch, then run a **frame scheduler loop** on a
worker:

```
target = clock + lookahead(ring capacity)
while playing:
    n = next comp frame ≤ target not yet scheduled
    if ring.free() == 0 or governor says pause: park until a slot frees (bounded wait)
    schedule render(n) on pool  → on completion (and epoch still valid) push to ring
    adapt: measure p95 render cost; lookahead = clamp(2 × cost × fps, 8, 16 frames)
```

- **Cached mode**: render at full chosen quality; ring underruns engage the degradation
  ladder in order ([13-PERFORMANCE-RULES.md](../13-PERFORMANCE-RULES.md)) — the governor
  owns that decision, not the loop.
- **Realtime mode (K-030)**: the loop instead picks resolution tier per frame from a
  controller: EWMA of render cost per tier; drop a tier when `cost_ewma > 0.9/fps`, rise
  when `< 0.4/fps` sustained 12 frames (hysteresis — numbers are starting points, tune on
  reference hardware). Tier goes into the cache key as usual, so realtime playback still
  warms caches.
- Scrub is the same machinery with a 1-deep "ring" (mailbox) and the scrub epoch bumping
  per mouse event; pre-roll on play start = fill ring before starting the audio stream
  (≤ 150 ms budget).

## 6. Test plan

1. Cancellation latency: start a deliberately slow 4 s render, bump epoch — all workers
   idle within 15 ms (checkpoint granularity proof).
2. Snapshot isolation: fuzz 10⁴ concurrent edit/render interleavings — every rendered
   frame's inputs hash-match exactly one published snapshot.
3. A/V drift: 10-minute playback with induced GPU stalls — audio glitch count 0, measured
   drift ≤ ±½ frame at every second (log clock vs presented frame).
4. Underrun ladder: throttle the pool artificially — assert ladder engages in documented
   order and UI frame time stays ≤ 8 ms throughout.
5. Realtime-mode controller: synthetic comp with cost cliff (heavy effect appears at t=5s)
   — tier drops within 3 frames, no flapping (≤ 1 tier change per second after settle).
