# 17 - Bridge contract (front/back boundary)

**Status: canonical.** This is the single source of truth for how the Flutter
frontend and the Rust engine talk to each other. It supersedes the scattered
descriptions that previously lived in the flutter-port notes (now archived
under [archive/flutter-port/](archive/flutter-port/)). If this doc and the code
disagree, fix one of them in the same commit.

## In plain terms

The application is two halves. The **frontend** is written in Dart and drawn by
Flutter: windows, panels, the timeline, dialogs, input. The **engine** is the
Rust workspace: the document, undo history, decoding, compositing, caching,
export. Dart cannot call Rust functions directly, so a **bridge** sits between
them. The bridge is one Rust crate (`lumit-bridge`) that compiles to a single
shared library (a `.dll` on Windows) which the Flutter runner loads at start-up.

Two kinds of information cross the boundary:

**Commands and state** travel as **JSON text over a C ABI**. Dart calls a plain
C function, passes a JSON string in, and gets a JSON string back. Every reply
is either `{"ok": true, ... }` or `{"ok":false, "error":" ... "}`.
**Video frames** are far too large to encode as text, so they travel as **raw
pixel buffers** - or, on the fast path, are never copied at all and are shared
as GPU memory the frontend displays directly.

## The layering

```
flutter_ui/ (Dart)              widgets, layout, theme, input, dialogs
    |   dart: ffi
crates/lumit-bridge (Rust)      the C ABI surface: commands in, JSON/pixels out
    |   plain Rust calls
crates/lumit-core, -project,    the engine (unchanged by the bridge)
        -media, -eval, -gpu,
        -audio, -cache, -ui
```

- `lumit-bridge` is a leaf crate. Engine crates never depend on it, and nothing
    in the engine depends on the frontend. The rule from
    [05-ARCHITECTURE.md](05-ARCHITECTURE.md) - engine crates never know the UI
    exists - is unbroken. The bridge is not an engine crate; it is the seam.
- The one deliberate, emporary exception is the Viewer render path: the bridge
    borrows `lumit-ui`'s headless compositor through the `lumit_ui::headless` seam
    to composite a frame (logged as K-175). This is the bridge reaching into the UI
    crate, not an engine crate doing so, so the dependency rule still holds.
- Long-running work (decode, export, beat detection) runs on worker threads with
    channels inside the engine; the bridge exposes progress through poll functions
    the frontend calls on a cadence.

## The transport: JSON over a C ABI ("bridge v0")

The current seam is a hand-written `extern "C"` surface, not generated code. This
is a deliberate interim choice ("bridge v0"): it keeps the toolchain simple - no
code generation, no build step - while the command surface is still changing.
`flutter_rust_bridge` remains the intended target once the API stabilises; see
[TODO.md](TODO.md) and the migration note below.

The exported functions live in [`ffi.rs`](../crates/lumit-bridge/src/ffi.rs) and
are named `lumit_bridge_*`. Dart binds them in
[bridge.dart ](../flutter_ui/lib/bridge/bridge.dart).

### The four binding rules

These are the contract. They do not change when generated code eventually
replaces the hand-written seam.

1. **No panic crosses the boundary.** Every exported function body runs inside
    `std::panic::catch_unwind`. A panic becomes an ordinary
    `{"ok": false, "error":"..."}` reply, never an unwind into Dart
    ([14-ENGINEERING-RULES.md](14-ENGINEERING-RULES.md)). The error string is a
    calm sentence fit for the status line.

2. **Rust owns the strings.** Each JSON function returns a Rust-allocated,
    NUL-terminated UTF-8 pointer. Dart copies the bytes out and immediately hands
    the pointer back to `lumit_bridge_free_string` so Rust frees it. Dart never
    frees Rust memory itself; Rust never reads a freed pointer.

3. **One client, one lock.** The engine-side document and its undo store live
    behind a single process-wide `Mutex` (there is exactly one Flutter window
    driving it). The lock is held only for the duration of one state transition,
    never across re-entry, an await, or a GPU/FFI call
    ([14-ENGINEERING-RULES.md](14-ENGINEERING-RULES.md)).

4. **An absent library degrades, it does not crash.** Dart's
    `LumitBridge.tryLoad()` returns null when the `.dll` cannot be found or bound,
    and the whole frontend keeps its placeholder behaviour. The bridge is an
    enhancement, never a hard dependency of the chrome. Every Flutter widget test
    runs without the library present, and stays green.

### Commands down, state up

The engine owns the document; the frontend never mutates it directly.

- **Commands down.** Every user action becomes one bridge call. Each edit maps
    onto a real, unit-tested `lumit_core` op (`AddLayer`, `SetTransformProperty`,
    `SetLayerEffects`, and so on), so undo/redo journalling is one clean step and
    is untouched by the existence of the bridge.
- **State up.** A successful edit returns the full refreshed **snapshot** - the
    document as the panels need to read it (project tree, comp outlines, layers,
    transforms, effects, retime, work area). The frontend holds it in
    `ChangeNotifier`s the widgets watch.
- **Rational time crosses as integers.** Frame counts and rates cross as exact
    `{num, den}` pairs or integer frame indices derived from a composition's own
    frame rate, never as floating-point seconds
    ([04-RETIMING.md](04-RETIMING.md), [14-ENGINEERING-RULES.md](14-ENGINEERING-RULES.md)).

### The ABI version and additive evolution

The bridge reports an integer `abi` from `lumit_bridge_version`. The snapshot has
grown additively across versions - each revision keeps every field the previous
one had, so an older reader never breaks:

ABI | Adds |
|---|---|
| v0 | Project lifecycle, ops dispatch, the snapshot, the JSON contract |
| v0.2 | Per-comp comp' block, footage `status`/`media`, the binary frame buffer|
| v0.3 | Transform read-back, identity links, effects, work area, layer lifecycle ops |
| v0.4 | Export, keyframe interpolation, Retime read-back and ops, the last timeline columns |
| v0.9 | Beat markers, sequence clips, overrun data, asset read-back, effect-param animation, `.lumfx` presets, mask geometry, the realtime (Auto) tier |





When adding surface: keep it additive, bump `abi`, and never remove a field a
shipped snapshot promised.

## The frame paths (pixels, not JSON)

A video frame is too large to encode as text, so frames have their own ownership
contract, documented beside the functions in
[`ffi.rs`](../crates/lumit-bridge/src/ffi.rs).

- **CPU buffer path.** `lumit_bridge_decode_frame` and
    lumit_bridge_render_comp_frame' return a Rust-owned block of tightly packed
    RGBA8 bytes (null on failure, with the out-pointers zeroed), writing the
    frame's width, height, and length into out-pointers. Dart copies the pixels out
    and hands the pointer **and its exact length** back to
    `lumit_bridge_free_buffer` - the mirror of the string contract, one boxed slice
    freed as a whole. The length must be exactly the `out_len` the decode wrote.
- **Zero-copy shared-texture path (Windows, opt-in, K-177).** The per-frame CPU
round trip (render on the GPU, read pixels down to the CPU, copy across FFI,
upload back to the GPU) is the recorded top performance cost (K-176). The
shared-texture path removes it: the engine renders into a shared D3D12 texture
and hands the frontend an NT handle (`lumit_bridge_render_to_shared`), which the
Windows runner registers as a Flutter external texture - no pixels copied. It is
an opt-in `shared-texture` cargo feature, off by default so every existing build
and CI gate is unchanged. See [06-RENDER-PIPELINE.md](06-RENDER-PIPELINE.md) and
[lumit-gpu/src/shared.rs ](../crates/lumit-gpu/src/shared.rs).

## Feature gates

- **`media`** (default on) pulls `lumit-media` (FFmpeg) for probing and decoding.
    `--no-default-features` drops it: the crate still builds and tests without
    FFmpeg (CI parity), footage reports `unprobed`, and the frame functions return
    null.
- **`render`** (default on) enables the composited-comp Viewer path and export
through the headless seam.
- **`shared-texture`** (default off) enables the zero-copy path above; the shipped
Windows .dll is built with it.

## Threading and long-running work

- **Export** runs on its own encode thread inside `lumit-ui::export` (K-017). The
    bridge holds the handle and drains progress on `lumit_bridge_export_poll`.
- **Playback / realtime tier.** A genuine render reports its measured cost to
    `lumit-eval`'s realtime controller (K-171); the frontend reads the current tier
    and scale back through `lumit_bridge_playback_tier` to drive the Auto
    resolution setting.
- **Known synchronous seams** (probing on import, beat detection) still run on the
calling thread and are honest follow-ups in [TODO.md](TODO.md); they function
today, the conversion is a threading refactor, not a missing capability.

## The migration to generated bindings

`flutter_rust_bridge` is purpose-built for this seam and is the intended endpoint.
It is deliberately deferred until the command surface stops changing, because
code generation mid-flux means constant regeneration churn. The four binding
rules above are written so the contract survives the switch unchanged: only the
mechanism that marshals a call changes, not the ownership, the degradation rule,
the version gate, or the rational-time convention. Track this under
[TODO.md](TODO.md) -> "Bridge and platform".

## See also

- [05-ARCHITECTURE.md](05-ARCHITECTURE.md) - crates, threads, the dependency rule.
- [06-RENDER-PIPELINE.md](06-RENDER-PIPELINE.md) - how a frame is produced.
- [GUIDE.md](GUIDE.md) - the plain-English tour of the codebase.
- [archive/flutter-port/](archive/flutter-port/) - the historical record of the
    egui-to-Flutter port that produced this seam (frozen; not maintained).