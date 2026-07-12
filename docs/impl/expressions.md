# Embedding QuickJS-ng deterministically

Expressions ([12-PLUGINS.md](../12-PLUGINS.md) §scripting) via the `rquickjs` crate
(quickjs-ng bindings). The hard requirements: determinism (same project → same pixels on
any machine) and isolation (an expression can never stall or crash a render).

## 1. Runtime topology

- One `Runtime` + `Context` **per worker thread**, lazily created, kept for the thread's
  life (contexts are not Send; never share). Memory limit 32 MB, stack 256 KB via
  `Runtime::set_memory_limit`/`set_max_stack_size`.
- **Interrupt handler is the budget**: `set_interrupt_handler` checks both the epoch token
  ([playback-scheduler.md](playback-scheduler.md)) and a per-evaluation deadline
  (default 5 ms). Trip → evaluation error → property falls back to pre-expression value
  with badge ([03-DATA-MODEL.md](../03-DATA-MODEL.md) §6.4). No expression can hang a
  render, by construction.
- Compile once: on expression edit, compile to bytecode
  (`Module::write_object`/`Context::compile`), cache bytecode per (source hash); threads
  instantiate from bytecode. Compile errors surface at edit time in the UI, not at render.

## 2. Determinism (binding rules)

Strip/replace in every context before user code runs:

- `Date` → shimmed: `Date.now()` and `new Date()` throw a helpful error naming
  `time`/`thisComp.frameDuration` as the alternative.
- `Math.random` → seeded xorshift128+ keyed by `hash(property_id, frame_time, user_seed)`
  — same call sequence, same values, everywhere; `seedRandom(seed, timeless)` follows AE
  semantics (timeless ⇒ drop frame_time from the key).
- No `eval`/`Function` from strings (QuickJS flag), no dynamic `import`, no host globals
  beyond the documented API. There is no IO to remove if you never add it — expressions
  get **zero** filesystem/network/process surface.
- Numbers: QuickJS is pure-software IEEE754 double — bit-identical across platforms, which
  is exactly why QuickJS-ng over V8/JIT engines (K-063). Do not introduce host-side fast
  paths that shortcut through f32.

## 3. The property graph bridge

Expose `thisComp`/`thisLayer`/`effect(...)` as lightweight opaque objects holding
`(snapshot Arc, object id)`; property reads resolve through the **snapshot captured for
this render job**, so an expression mid-playback never sees a half-edit. Reads are
`valueAtTime`-shaped internally; plain `.value` = valueAtTime(current). Cycle handling:
evaluation carries a visit stack of property ids; revisits → error-with-badge (AE
behaves the same). The dependency set collected during evaluation feeds the node's cache
key ([06-RENDER-PIPELINE.md](../06-RENDER-PIPELINE.md)): hash of (expression bytecode,
read property values) — constant expressions therefore cache once per comp automatically.

## 4. The v1 API surface

Implement exactly the subset in [12-PLUGINS.md](../12-PLUGINS.md) (time, value,
valueAtTime, wiggle, loopIn/Out(+Duration), seedRandom/random/gaussRandom, linear, ease,
easeIn/Out, clamp, length, normalize, thisComp/thisLayer/thisProperty, comp(), layer(),
effect(), marker access, posterizeTime). `wiggle` must match AE's fractal-sum behaviour
closely enough that imported motion looks right: sum of 1 + `octaves` value-noise layers,
each `amp_mult^i · noise(freq · 2^i · t)`, value-noise from the seeded PRNG on integer
lattice with smoothstep — golden-test against Bridge-exported AE samples and tune
constants once, then lock.

## 5. Test plan

1. Determinism: 10⁴ random expressions from a generator (arithmetic, wiggle, property
   reads) evaluated on x86 Windows and ARM macOS — byte-identical f64 outputs.
2. Budget: `while(true){}` trips the interrupt in ≤ 6 ms, render completes with fallback
   value, badge set.
3. Cycles: a→b→a property reads error cleanly, no stack overflow (depth-limit test to
   1000 chained properties).
4. AE goldens: Bridge-exported comps using wiggle/loopOut/valueAtTime match AE's sampled
   values within visual tolerance (wiggle: statistical match — same amplitude spectrum,
   locked goldens thereafter).
5. Memory: leak test — 10⁶ evaluations, per-thread context RSS flat.
