# Lumit data model

**Status: canonical.** The object model every other document builds on. Terminology per
[01-GLOSSARY.md](01-GLOSSARY.md); decisions per [02-DECISIONS.md](02-DECISIONS.md).
Serialisation of this model is specified in [10-FILE-FORMAT.md](10-FILE-FORMAT.md); how it
compiles into the evaluation graph is specified in [06-RENDER-PIPELINE.md](06-RENDER-PIPELINE.md).

Sketches below use Rust-flavoured pseudocode. They describe shape and invariants, not final
field names.

---

## 1. Foundations

### 1.1 Identity

Every model object carries a stable **UUIDv7 id**, assigned at creation and never reused.
All cross-references (layer parenting, mattes, clip sources, expression links) are by id.
Names are display strings only; renaming MUST never break a reference.

### 1.2 Time is rational

Authoritative time is never floating point (see [14-ENGINEERING-RULES.md](14-ENGINEERING-RULES.md)).

```rust
struct RationalTime { num: i64, den: u32 }   // seconds = num / den
struct FrameRate    { num: u32, den: u32 }   // e.g. 60000/1001
```

The four timebases (source, clip, layer, comp — [01-GLOSSARY.md](01-GLOSSARY.md) §4) are
distinct newtypes over `RationalTime`. Conversions between them are explicit functions, and
the Retime map ([04-RETIMING.md](04-RETIMING.md)) is the only nontrivial conversion.

### 1.3 Non-destructive rule

Per K-024: no operation on this model modifies source media, and no operation is
irreversible within a session (everything goes through the operation journal, §10).
Baking exists only inside the export pipeline and produces no model mutations.

---

## 2. Project

v1 ships this as lumit-core's `Document` — a flat item store. The richer
`Project`/`ProjectSettings` shape is the intended direction; the settings it
would hold (a display-transform default, an expression-engine version) arrive
with those features (colour management, expressions), so v1 has no
`ProjectSettings` yet.

```rust
struct Document {
    id: Uuid,
    items: Vec<ProjectItem>,   // flat storage; Project-panel order = Vec order; folders hold children by id
    auto_folders: AutoFolders, // where new solids / comps are auto-filed (K-068)
}
```

A `ProjectItem` (the intended **Asset**) is one of the following. **v1 ships
`Footage`, `Folder`, `Composition`, `Solid`**; the audio/still/sequence kinds
are future (audio is currently only a footage layer's stream, §5.2):

| Asset | v1? | Contents |
|---|---|---|
| `Footage` (`FootageItem`) | yes | Media reference (§3); interpretation and proxy state are future |
| `AudioItem` | future | Audio-only media reference |
| `StillItem` | future | Single image |
| `SequenceItem` | future | Image sequence (pattern, fps) |
| `Composition` | yes | §4 |
| `Folder` | yes | Ordered children ids |
| `Solid` (`SolidDef`) | yes | Shared solid definition (colour, size) — solids are items so they dedupe |

### 3. Media references and interpretation

```rust
struct MediaRef {
    relative_path: String,     // relative to project file where possible
    absolute_path: String,     // last known absolute location
    fingerprint: Fingerprint,  // size + mtime + head/tail content hash, for relinking
}

struct FootageInterpretation {
    frame_rate_override: Option<FrameRate>,
    alpha: AlphaMode,               // straight | premultiplied(colour) | ignore | guess
    colour_space: ColourSpaceTag,   // default: Rec.709/sRGB assumption for game captures
    loop_count: u32,
    start_timecode_policy: TcPolicy,
}
```

A footage item whose file cannot be found enters a **missing** state: it keeps all metadata,
renders as a labelled placeholder slate, and never blocks project open. Relink flow in
[07-UI-SPEC.md](07-UI-SPEC.md).

---

## 4. Composition

```rust
struct Composition {
    id: Uuid,
    name: String,
    width: u32, height: u32,            // hard cap 16384×16384 in v1
    pixel_aspect: Rational,             // 1:1 default
    frame_rate: FrameRate,
    duration: CompTime,
    background: LinearColour,
    depth: CompDepth,                   // Fp16 (default) | Fp32   (K-026, per-comp)
    motion_blur: MotionBlurSettings,    // shutter_angle (deg), shutter_phase, max_samples
    work_area: (CompTime, CompTime),
    markers: Vec<Marker>,
    layers: Vec<Layer>,                 // index 0 = top of the stack
}
```

Comp frame rate is presentational (it defines frame boundaries for snapping and export);
evaluation is defined at arbitrary rational times so nested comps of differing rates stay exact.

---

## 5. Layers

### 5.1 Common layer core

Every layer, regardless of type:

```rust
struct Layer {
    id: Uuid,
    name: String,                      // defaults from source; user-renameable
    kind: LayerKind,                   // one of §5.2
    in_point: CompTime,                // may be negative — the layer may start before comp 0 (K-153)
    out_point: CompTime,               // exclusive; out > in; may exceed the comp duration (K-153)
    start_offset: CompTime,            // where layer time 0 sits on the comp timeline; may be negative
    parent: Option<Uuid>,              // transform parenting (K-103); a missing/cyclic parent degrades to none
    label: u8,                         // index into the theme label palette (TL2); organisational, never rendered
    blend: BlendMode,
    matte: Option<MatteRef>,           // { layer, channel: Alpha|Luma, inverted, source } (K-142)
    transform: TransformGroup,         // §6
    masks: Vec<Mask>,                  // §7
    effects: Vec<EffectInstance>,      // §8, ordered top-to-bottom
    switches: Switches,
}
// Future (not in v1): `stretch` (uniform rate multiplier), per-layer `markers`,
// and `audio: AudioProps` (animatable level / mute). v1 mutes via the `audible`
// switch, and audio comes only from a footage layer's own stream (§5.2, docs/09).

struct Switches {
    visible: bool, audible: bool, locked: bool,
    solo: bool,                        // K-105: while any layer is soloed, only soloed layers render
    fx: bool,                          // docs/08 §1.5: off bypasses the layer's whole effect stack (default on)
    motion_blur: bool,                 // K-120: per-layer shutter smear (needs the comp master on)
    three_d: bool,                     // 2.5D: position in z, honour the active camera
    collapse: bool,                    // Precomp layers: transform concatenation (docs/06 §1.4)
}
// Future switches (K-168, deferred): `shy` (needs an outline filter row) and
// `quality` (Draft|Full — needs a bicubic sampler choice). `adjustment` is not a
// switch — an adjustment layer is a LayerKind (§5.2).
```

Invariants:
- A layer sits freely across the comp boundaries (K-153): `in_point` may be **negative**
  (the layer starts before comp time 0) and `out_point` may exceed the comp **duration**.
  Only `out > in` is enforced. The engine renders and plays a layer solely where its span
  `[in_point, out_point)` **intersects the comp window `[0, comp_end)`** — frames outside the
  window are simply never sampled — so an over-hanging head or tail is carried without data
  loss and is recoverable by sliding the layer. Import never trims a long clip to fit: a
  footage/precomp layer keeps its full source/nested duration, positioned from the comp start.
- A matte reference to a missing/deleted layer degrades to "no matte" with a badge, never an error.
- Any layer can serve as a matte for any number of consumers; the engine evaluates it once
  ([06-RENDER-PIPELINE.md](06-RENDER-PIPELINE.md)).
- `source: LayerInputSource` (default `EffectsAndMasks`, K-142, revising K-125's `after_effects`
  bool — the most complete source is the sensible default for a new matte/depth input):
  **None** gates by the matte layer's **raw** pixels (no masks, no effects); **Masks** gates
  by the source plus its own masks; **EffectsAndMasks** runs the matte layer's effect stack
  into the matte first (a keyed or blurred matte). v1 skips the source's *temporal* effects
  through a matte (echo/flow degrade to a still — [docs/impl/layer-input.md](impl/layer-input.md)).
  A project saved with K-125's `after_effects` bool migrates on load (`true` →
  `EffectsAndMasks`, `false` → `Masks` so no masks are dropped, absent → the default
  `EffectsAndMasks`).

### 5.2 Layer kinds

| Kind | v1? | Source payload | Notes |
|---|---|---|---|
| `Footage { item: Uuid, retime: Option<Retime> }` | yes | One footage item | The AE-style default. `None` = source rate. Retime per [04-RETIMING.md](04-RETIMING.md). |
| `Sequence { clips: Vec<Clip> }` | yes | Its clips | §5.3. |
| `Precomp { comp: Uuid }` | yes | Another composition | `collapse` switch defers rasterisation. Cycles invalid. **Precomp-level retime is future** — the `retime` field is not on the kind yet; nest through a Sequence clip to retime a comp for now. |
| `Solid { def: Uuid }` | yes | A SolidDef | |
| `Text { document: TextDocument }` | yes | §9.1 | v1: one run. |
| `Camera { zoom: Property }` | yes | — | AE camera: `zoom` is focal distance in comp pixels (z=0 maps 1:1). Only affects 3D-switch layers; the topmost visible camera is active. |
| `Adjustment` | yes | — | No source of its own; its masks + effect stack apply to the composite of every layer beneath it, within its span. (There is no `adjustment` switch — it is this kind.) |
| `Shape { contents: Vec<ShapeElement> }` | future | §9.2 | |
| `Null` | future | — | Transform-only, invisible. |
| `Audio { item: Uuid }` | future | An audio item | v1 audio is only a footage layer's own stream (§5.2, docs/09). |
| `Light` | future | — | Paired with Camera; not in v1. |
| `Light { light: LightProps }` | §9.3 | 3D only. |

### 5.3 Clips (Sequence layers only)

```rust
struct Clip {
    id: Uuid,
    source: ClipSource,            // FootageItem | Composition
    source_in: SourceTime,         // trim into the source
    source_out: SourceTime,        // exclusive
    place: ClipTimeSpan,           // start + duration in layer time; derived from edits, stored explicitly
    retime: Retime,                // exact rational boundaries — see 04-RETIMING.md
    interpolation: FrameInterp,    // Nearest | Blend | Flow  (render policy, not part of the map)
    label: LabelColour,
}
```

Invariants (binding, per K-020/K-022):
- Clips on one Sequence layer MUST NOT overlap. Gaps are allowed and render transparent.
- An **edit point** is the shared boundary of two adjacent clips. Retime edits MUST NOT move
  `place` of any clip (the beat-sync covenant).
- Cutting a clip produces two clips whose retimes are exact partitions of the original
  ([04-RETIMING.md](04-RETIMING.md) §cutting).
- Layer-level properties (transform, effects, masks, matte, blend) apply to the Sequence
  layer's assembled output, after clip retiming — a glow keyframed on the layer is unaffected
  by where cuts fall.

---

## 6. Properties, keyframes, animation

### 6.1 Property

A **property** is an animatable slot. Properties live in **property groups** forming a stable
tree (transform group, each effect's parameters, each mask's geometry, retime).

```rust
struct Property<T: PropValue> {
    id: Uuid,
    animation: Animation<T>,
    expression: Option<Expression>,   // §6.4 — applied after keyframe evaluation
}

enum Animation<T> {
    Static(T),
    Keyframed(Vec<Keyframe<T>>),      // sorted by time, unique times
}
```

`PropValue` types: `f64`, `Vec2`, `Vec3`, `LinearColour`, `bool`, `enum` (hold-only
interpolation for the last two), `BezierPath` (mask/shape geometry), `TextDocument`.

Property addressing is by stable path of ids (not display names), so expressions and the AE
Bridge survive renames: `layer(id).effect(id).param(id)`.

### 6.2 Keyframes — AE-compatible maths (K-025)

```rust
struct Keyframe<T> {
    time: OwnerTime,                  // timebase of the owning object
    value: T,
    interp_in:  SideInterp,           // approaching this key
    interp_out: SideInterp,           // leaving this key
    spatial: Option<SpatialTangents>, // Vec2/Vec3 positional properties only
    roving: bool,                     // spatial properties only
    label: Option<LabelColour>,
}

enum SideInterp {
    Hold,
    Linear,
    Bezier { speed: f64, influence: f64 },   // speed: units/sec; influence: 0.1..=100.0 (%)
}
```

Between two keys `(t1,v1) → (t2,v2)` with bezier sides, the value curve is the cubic bezier
with control points at

```
P1 = (t1 + influence_out·Δt, v1 + speed_out·influence_out·Δt)
P2 = (t2 − influence_in·Δt,  v2 − speed_in·influence_in·Δt)      where Δt = t2 − t1
```

— exactly AE's model, so Bridge import ([11-AE-IMPORT.md](11-AE-IMPORT.md)) is lossless and
the speed graph in the graph editor is the true derivative. Easy-ease presets are speed 0,
influence 33.33%.

Spatial properties additionally carry in/out tangents in value space defining the motion
path; **roving** keyframes surrender their time and are repositioned to equalise speed along
the path (recomputed whenever neighbours change).

### 6.3 Evaluation order of one property

```
keyframe/static evaluation → expression (may read the pre-expression value) → clamp/validate
```

A property's evaluated value at a time is pure: same project, same time, same value —
no wall clock, no external state ([14-ENGINEERING-RULES.md](14-ENGINEERING-RULES.md)).

### 6.4 Expressions

```rust
struct Expression {
    source: String,          // JavaScript, ES2018 surface — see 12-PLUGINS.md
    enabled: bool,
    last_error: Option<ExprError>,   // runtime state, not serialised as authority
}
```

An expression failure disables that expression with a badge and falls back to the
pre-expression value. It never fails the render.

---

## 7. Masks

```rust
struct Mask {
    id: Uuid,
    path: Property<BezierPath>,       // closed or open; animatable
    mode: MaskMode,                   // None|Add|Subtract|Intersect|Lighten|Darken|Difference
    inverted: bool,
    opacity: Property<f64>,           // 0..100
    feather: Property<Vec2>,          // px at layer scale
    expansion: Property<f64>,         // px, signed
}
```

Masks apply in order before the effect stack ([06-RENDER-PIPELINE.md](06-RENDER-PIPELINE.md)).
Variable-width feather is post-v1; the model reserves per-vertex feather data.

## 8. Effects

```rust
struct EffectInstance {
    id: Uuid,
    effect: EffectKey,        // { namespace: Builtin|Ofx|Lfx|Placeholder, match_name, version }
    enabled: bool,
    params: PropertyGroup,    // declared by the effect; all animatable, expression-visible
}
```

**Placeholder** effects (from AE import, or a missing plugin) keep `match_name` and the full
parameter dump, render as identity with a badge, and round-trip through save untouched
([11-AE-IMPORT.md](11-AE-IMPORT.md)).

An effect parameter may also **reference another layer** as an auxiliary input (a
Layer-reference parameter, [08-EFFECTS.md](08-EFFECTS.md) §1.2 — a depth pass for Depth of
field): the stored value is an optional layer id, the same by-id cross-reference §5.1's matte
uses, and a dangling reference degrades to a no-op exactly as a dangling matte does. A
companion `<id>_source` Choice holds its `LayerInputSource` sampling mode (None / Masks /
Effects and masks, K-142), the same three-way source a matte carries in §5.1.

## 9. Rich layer payloads

### 9.1 Text

v1 `TextDocument`: styled runs (font family/weight, size, fill, stroke, tracking, leading),
point vs paragraph text, alignment. Per-character animators are post-v1; the document model
keeps text as structured runs (never rasterised into the project) so animators bolt on later.

### 9.2 Shape

v1 `ShapeElement` tree: groups; parametric rectangle/ellipse/polystar; bezier path; fill
(solid, linear/radial gradient); stroke (width, caps, joins, dashes); trim paths. Repeater,
offset, wiggle-path are tier 2 ([08-EFFECTS.md](08-EFFECTS.md) keeps the list).

### 9.3 2.5D (K-023)

All transforms are 4×4 internally from day one; the `three_d` switch exposes z and full
rotation. The Phase 1 camera is the seed of `CameraProps`: `Camera { zoom: Property }` —
a one-node camera whose zoom is the AE model (focal distance in comp pixels; the z=0
plane maps 1:1, a layer at depth z scales by zoom/(z+zoom)), positioned and rotated by
the layer's own transform group, with the topmost visible camera active. `CameraProps`
v1 grows from there: one-node/two-node, focal length presets, depth of field (focus
distance, aperture, blur level). `LightProps` v1: ambient/point/spot/directional with
intensity, colour, cone; shadows post-v1. 2D layers ignore cameras (render in a fixed
orthographic pass), matching AE's mental model.

## 10. Undo, journal, dirty state

All mutations go through **operations** — small, serialisable, invertible commands
(`SetKeyframe`, `MoveClip`, `AddLayer`, …) applied to the document behind a single writer.
The **operation journal** is the undo/redo stack and the autosave crash-recovery log
([10-FILE-FORMAT.md](10-FILE-FORMAT.md) §autosave). The UI renders from immutable snapshots;
workers render from the snapshot current when their job was scheduled
([05-ARCHITECTURE.md](05-ARCHITECTURE.md)).

## 11. Markers

```rust
struct Marker {
    id: Uuid,
    time: OwnerTime,
    duration: Option<RationalTime>,
    label: String,
    colour: LabelColour,
    kind: MarkerKind,        // User | Beat { confidence: f32 } | Chapter
}
```

Beat markers are ordinary markers with provenance; regenerating beats replaces only
`Beat`-kind markers ([09-AUDIO.md](09-AUDIO.md)).

## 12. Schema evolution

The model is versioned (`schema_version` in the project file). Rules, binding:
- Additive changes only where possible; unknown fields MUST be preserved on load/save
  (forward compatibility for shared projects, K-065).
- Any breaking change ships with a migration and a decision-log entry.
- Pre-1.0, migrations may be dropped after six months; post-1.0, never.

## Open questions

- Maximum comp size: 16384² is the common GPU texture limit; do we macro-tile to exceed it
  (AE allows 30000²) or cap and revisit?
- Should `stretch` survive long-term, or is it sugar the UI lowers into Retime? (It rescales
  keyframes, which Retime deliberately does not — kept for AE compatibility for now.)
- Per-vertex mask feather data reserved but unspecified — spec when variable-width feather lands.
- Gradient model for text stroke/fill v1 or tier 2?
