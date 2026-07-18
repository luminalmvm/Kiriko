# Layer-input effect parameters + completing DoF — impl note

Feeds docs/08-EFFECTS.md §3.9 (DoF) and docs/03-DATA-MODEL.md (a new effect
parameter type). The GPU kernel already exists (`lumit_gpu::fx::dof` +
`upload_depth_map` + `fx_dof.wgsl`, oracle green); this note is the *how* for the
two things left: an effect parameter that references **another layer** as an
input, and wiring the DoF effect on top of it.

## In plain terms
Some effects need a second picture, not a number — depth-of-field needs a **depth
map** saying how far each pixel is. The cleanest source is *another layer* in the
comp (a depth pass). Lumit already does exactly this for **track mattes**: a layer
names another layer, and the compositor renders that other layer alone and hands
its texture in. A layer-input parameter is the same idea, but the referenced
layer's texture is handed to an **effect** instead of the matte stage.

## 1. The parameter (mirror `MatteRef`)
- `ParamKind::Layer { }` (fx.rs) — declares the effect wants a reference to
  another layer (a depth/aux input).
- `EffectValue::Layer(Option<Uuid>)` (model.rs) — the referenced layer's id, or
  None when unset. Exactly the shape of `MatteRef.layer`, minus channel/invert.
- Inspector: a **Layer picker** arm — a dropdown of the comp's layers by name
  (plus "None"), like the Parent picker in the Effect Controls panel (K-103).
  Selecting sets the id through an undoable op.

## 2. Threading the referenced layer's texture (mirror mattes + the LUT §8)
`run_ops` takes only `&[Resolved]` (Copy scalars), so — exactly as the LUT
texture and the flow field are threaded — the referenced layer's **rendered
texture** travels beside the ops:
- The referenced layer is rendered **alone at comp size** (linear fp16), the same
  render the matte stage already produces (`MatteInput.texture` is "the matte
  layer rendered alone at comp size" — reuse that path / helper).
- `run_ops` gains a parallel input `layer_inputs: &[Option<Tex>]`, one slot per
  effect op that declares a Layer parameter; the k-th such op binds the k-th slot
  (the LUT counter pattern). A `None` slot (unset / missing / cyclic reference) is
  a passthrough — never a fault.
- Preview (`build_comp_draws` → `gpu.rs`) and **export** must render the
  referenced layer and thread it **identically** (K-031) — factor "render layer
  X alone at comp size" into one shared helper both call, as the matte path does.
- Guard cycles: an effect on layer A referencing layer B that (transitively)
  references A must not infinitely recurse — a visited-set like the cache key's,
  or simply "a layer-input renders its target with that target's own layer-input
  effects disabled" for v1. Flag the choice.

## 3. The DoF effect (docs/08 §3.9) on top
- Schema `dof` (Blur & sharpen or a new Camera category — check §3.9): params a
  `Layer` param `depth` + Float `focus` (0–1), `range` (0–1), `aperture` (px@comp)
  and Mix. Traits: cost Moderate, roi Padded(aperture), temporal `{0}`,
  premultiplied true (the gather is over premultiplied colour — confirm against
  `fx_dof.wgsl`).
- `Resolved::Dof { focus, range, aperture, mix }` (scalars only — the depth
  texture is threaded per §2, not carried in Resolved).
- resolve arm reads the floats; the depth layer id is read separately by the
  caller (like the LUT path) to render + thread the depth texture.
- `run_ops` Dof arm: if its `layer_inputs` slot is `Some(depth)`, call
  `fx.dof(ctx, &tex, w, h, depth, focus, range, aperture, mix)`; else passthrough
  (no depth = no blur, a labelled no-op).
- `cpu::apply` Dof arm = passthrough (GPU-only, like the LUT); the §1.6 oracle
  reference is the existing `wgsl_dof_matches_the_cpu_oracle` in lumit-gpu (its
  `dof_reference`), not `cpu::apply` — the depth is a texture, not a number.
- Depth encoding: the depth layer's pixels are read as a single channel (its
  luminance or R) mapped to 0..1 by `upload_depth_map`; document that a brighter
  pixel = nearer/farther (pick one, note it). A pre-rendered comp-size texture is
  already fp16 working format, so extract depth as luma in the DoF kernel's read,
  or upload a converted R32Float — reuse `upload_depth_map`'s contract.

## 4. Cache key (lumit-eval)
The referenced layer's *content* feeds the effect, so the consumer frame key must
change when the depth layer changes: in `feed_layer`, when an effect has a
`Layer` param, recurse `feed_layer` on the referenced layer (guarded by the same
visited set) so its evaluated transform/effects/source join the key. v1 minimum:
at least hash the referenced layer id; full recursive content-hashing is the
correct form — do it if the visited-set makes it clean.

## 5. Test plan
- The existing `wgsl_dof_matches_the_cpu_oracle` covers the kernel (done).
- A resolve test: a `dof` instance resolves its floats and its `Layer` param
  round-trips a layer id (serde).
- A no-op test: `dof` with an unset depth layer is a passthrough.
- Preview==export: the referenced-layer render + threading go through one shared
  helper (asserted by construction / reviewed by hand, as for the LUT).

## Status / follow-ups
This unblocks **DoF v1** (a depth layer + focus/aperture/mix). The fuller
"DOF PRO" second effect, shaped bokeh highlights, and the deferred bright-rim
"Highlight bloom" param remain post-v1. Log a K-decision for the Layer-input
parameter kind and the DoF effect when they land.
