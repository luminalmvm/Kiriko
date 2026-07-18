# Temporal re-render effects: accumulation motion blur + Posterize Time

Feeds docs/08-EFFECTS.md (two Temporal effects) and docs/06-RENDER-PIPELINE.md. These are
NOT per-pixel effects — they change *what time the layers below them render at*, so they live
at the frame-orchestration layer, not in `run_ops`. Owner spec: [[accumulation-mb-design]],
[[posterize-time-design]].

## In plain terms
Per-layer motion blur (K-120) smears one layer along its own transform. **Accumulation motion
blur** is the expensive, correct one: it renders the *whole scene below it* several times at
in-between moments and averages the finished frames — so footage motion, animated effects,
depth passes and everything else are all correct per sample (no blurred-depth artefact).
**Posterize Time** renders the scene below at a *held* time on a coarse grid, for the choppy
stop-motion look. Both re-run the render at a changed time; that is the whole trick.

## 1. Why it cannot be a `run_ops` effect
`fxops::run_ops` receives finished textures (`&[Resolved]` + the composite-below as one
texture). To render the below-stack at a *different* time you must rebuild its draw list
(`draws::build_comp_draws(doc, comp, τ, …)`) and `realise` it — and only the frame
orchestration (which holds `doc`/`comp` and the decoded `pixels_by_layer`) can do that.
`realise` only has textures. So these effects are detected and executed **where
`build_comp_draws` + `realise` are called** (preview: the comp-render entry in `gpu.rs`/
`app_state`; export: `render_comp_linear`), not in the compositor's per-op loop.

Both are **adjustment-layer** effects (docs/08 §1.5): they process everything below. "Apply
to all layers" (the owner's global toggle) is simply putting the effect on a top adjustment
layer spanning the comp — no separate global flag needed.

## 2. The shared capability: render the below-stack at time τ
Factor one helper both effects and both paths (preview + export) use:
`render_below_at(doc, comp, below_layers, τ, …) -> Texture` = `build_comp_draws` at `τ` (reuse
the *same* decoded `pixels_by_layer` — footage frames are held; only transforms/effects/camera
re-resolve) → `realise`. Sub-frame footage *motion* is out of scope (that is the flow
`motion_blur` effect); this blurs comp-driven animation. Preview and export MUST call the one
helper so a preview frame equals an export frame (K-031), exactly as `render_layer_input` and
`motion_blur_average` are shared.

## 3. Accumulation MB
- Params: **Samples** N, **Shutter angle**, **Shutter phase** (reuse `MotionBlur::sample_offsets`
  maths for the centred offsets), Mix. Category Temporal, cost Heavy (≈ N× a full comp render).
- Render below at `τ_k = t + off_k·dt` for each k; **average** the N textures with the
  pure-additive-at-`1/N` accumulation pipeline `motion_blur_average` already added (colour AND
  alpha additive, so a static scene is unchanged). Composite the average in place of the plain
  below-composite.

## 4. Posterize Time
- Params: **Rate** (fps, e.g. 12), optional **Phase**; **Scope** = *Everything below* | *This
  layer's effects*. Category Temporal, cost cheap (one render at the held time — often the
  SAME held time across many frames, so the frame cache key collapses them: a big win).
- Held time `τ = floor((t − phase)·rate)/rate + phase`.
- **Everything below**: `render_below_at(…, τ)` — the adjustment path.
- **This layer's effects**: no re-render of others; the layer the effect sits on evaluates its
  own source + effect stack at `τ` instead of `t` (a per-layer time substitution feeding its
  own stack). Simpler; no orchestration re-entry.

## 5. Per-effect "don't sample" flag (owner request)
`EffectInstance` gains `#[serde(default = "default_true")] sample_temporally: bool`. During a
sub-frame/held render, an effect with it **false** is evaluated at the *frame* time `t`, not
`τ_k` — for particle systems and other stochastic/costly effects the user does not want re-run
per sample. Implementation: `resolve_stack` grows an optional per-effect time override (or the
caller resolves excluded effects at `t` and the rest at `τ_k` and merges). Held effects render
once at `t`; only sampled ones move.

## 6. Cache key + determinism (lumit-eval)
The sampled/held times are a pure function of `t`, the rate/shutter, and `sample_temporally`
flags, so `feed_*` just reflects them: accumulation MB feeds N + the shutter (only when the
effect is present and enabled); Posterize Time feeds the *held* time (so identical held frames
share a key — the deduplication that makes it cheap). No new non-determinism.

## 7. Build order (incremental, each green)
1. `render_below_at` shared helper (preview + export) — the risky orchestration re-entry;
   prove preview == export on a still scene first (a re-render at the same `t` must be
   bit-identical to no re-render).
2. Posterize Time, *Everything below* scope (one render at `τ`) — the simplest consumer.
3. `EffectInstance.sample_temporally` + Posterize Time *this-layer* scope.
4. Accumulation MB (N samples + the additive average) on top of (1).
Each step is a K-decision + docs/08 section + oracle/parity test where one applies (these have
no per-pixel oracle; the test is a still-scene identity + a moving-scene coverage check, as
per-layer MB used).

## Traps
- Re-rendering re-decodes nothing — pass the SAME `pixels_by_layer`; re-resolving at `τ` must
  not re-request media (that would thrash the decode planner).
- Recursion: an adjustment layer's below-set excludes itself; nested comps already cycle-guard
  via `visited`.
- Cost: N× render is Heavy — respect docs/13 budgets; degrade N under the preview-draft/scrub
  path (fewer samples while interacting), full N on export.
- Preview == export is the whole risk surface — one shared `render_below_at`, reviewed by hand.
