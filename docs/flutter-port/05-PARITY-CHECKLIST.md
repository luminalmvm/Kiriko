# 05 — Parity checklist (living)

The tick-list for the one-for-one port. Updated in the same commit as the work,
newest state wins. ☐ to do · ◐ partial (remainder named) · ☑ done (with tests
where the row is logic).

## Phase F0 — scaffold and chrome

- ☑ Flutter project scaffolded (`flutter_ui/`, package `lumit_flutter`), analyzer clean
- ☑ Theme port: all 7 colour schemes digit-for-digit, ShapeTokens SHARP/ROUND,
  `with_accent` hover shift by mode, `label_colour`, `document_colour`,
  AnimationLevel durations — unit-tested against the Rust values
- ☑ Inter Medium bundled; type scale 16/12/12/11/12
- ☑ Icon set: 44 variants mapped to Iconoir, motion-blur mark as a CustomPainter
- ☑ Settings model: Performance / Autosave / Interface / Export defaults matching
  the engine constants — unit-tested
- ☑ Workspace persistence to JSON (schemes, shape, accent, animation, settings,
  dock layout)
- ☑ Dock: tree model (splits/tabs/panes) + default workspace byte-matching
  `default_layout()` shares — unit-tested; resizable dividers; tab pills with
  the three-state styling; solo panes bare
- ☑ Dock: drag a tab / bare-pane grip to re-dock — model + interaction, unit-
  and widget-tested (movePanel/simplify with the every-panel-once invariant; a
  ghost pill, drop-zone previews and commit-on-release over both shapes)
- ☑ Bare-pane affordances: right-click "Pop out into its own window" (surfaces
  the multi-window notice for now); the corner drag grip built (2×3 dot grid,
  drags the pane exactly like a tab)
- ☐ Pop out a panel into its own OS window (multi-window; deferred, see 04)
- ☑ Menu bar: File / Edit / Composition / Window with the full shipped item set
  (engine-backed items dispatch to the stub state and surface a notice)
- ☑ Status line: notices, error tint rule, export-progress slot
- ☑ Settings window: all five pages, every control, fixed geometry, opens on
  Appearance; scheme/shape/accent/motion apply live
- ☑ Accent colour picker (HSV square + hue strip + hex), reusable for the
  future editor-colour submenu
- ☑ Command palette: modal, fuzzy filter, keyboard selection, the shipped
  command list (incl. the hidden export alias)
- ☑ Shortcut routing: the §5 inventory table wired to the stub state, with the
  text-field focus gate
- ☑ Panel stubs: all seven panels with real chrome (Viewer surround, scope
  graticule placeholder, timeline strip skeleton) so the workspace reads right
- ☑ Splash boot card (K-008: centred card, boot lines, click to skip; the
  engine's real boot log streams in at F1) — widget-tested
- ☑ Active-panel accent edge (last click wins, one pane at a time) —
  widget-tested
- ☐ Sharp/Round: Round cards ☑ (fill, radius, padding, gap, shadow); window
  inset ☑; resize-gap hover/drag tinting ☑

## Phase F1 — bridge (in progress)

- ◐ `lumit-bridge` crate ☑ (bridge v0: hand-rolled JSON over a C ABI, loaded
  over `dart:ffi`; catch_unwind on every export, Rust-owned strings freed via
  `lumit_bridge_free_string`, single-client `Mutex` state; in-crate tests).
  flutter_rust_bridge codegen ☐ — deferred until the API surface stabilises
  (bridge v0 is hand-rolled JSON over C ABI, see 03-ARCHITECTURE §Bridge v0)
- ☑ Project open/save/snapshot/ops. Live: new project, open (`.lum`), save (to
  the loaded path, or a save dialogue when the project has no path yet — the
  egui Save behaviour, no separate Save As), document snapshot (item tree +
  can_undo/can_redo + path), new composition through the real op/undo path,
  undo/redo
- ☑ File dialogues (the `file_selector` plugin): Open project (`.lum` filter),
  Save (falls through to a save dialogue with `untitled.lum` when unsaved),
  Import footage (multi-select, filter mirrors the egui import list). Dialogue
  calls route through injectable seams so widget tests stub paths without
  plugin channels
- ☑ Import footage → `lumit_bridge_import_footage`: one footage item per file
  through the real AddItem/undo path (no auto-folder, mirroring the egui
  frontend); one calm notice for N imported, failures in the error tint. No
  media probing yet (probing/thumbnails are F2)
- ☑ Reopen the last project on launch: the workspace persists `lastProjectPath`
  (set on open/save-with-path), and a live bridge reopens it when the file is
  still on disk — a missing or unreadable file degrades to a calm status-line
  notice, never a crash
- ◐ Project panel live: item tree + type icons (footage/folder/composition/
  solid) with layer-colour tints and nesting, empty-document hint, hover fill.
  Not yet: thumbnails, relink, missing badge, selection/drag
- ☑ Session restore *within* a project (open comps, playhead, selection) — the
  Flutter counterpart of the egui shell's `SavedSession`, keyed by project path
  in `Workspace.sessions` (persisted beside `lastProjectPath` in the workspace
  JSON). `AppStateStub` persists it on change (front-comp / selection / playhead
  seams, via the additive `rememberSession` seam) and re-applies it after a
  project opens/reopens (`sessionFor` → `_applySessionFor`, each id validated
  against the fresh document so a stale comp/layer id falls back to the default,
  never a crash) — round-trip + re-apply + stale-fallback tested. INTEGRATOR:
  the shell wires `rememberSession`/`sessionFor` at `AppStateStub` construction
  (one-line each, alongside the existing `rememberProject`).
- ☑ Autosave: a periodic rotating copy beside the project mirroring
  `lumit_project::autosave` — `autosaves/<stem>.autosave-N.lum`, N = 1 newest,
  keep-N rotation (`AutosaveScheme`, pure + tested). `AppStateStub.autosaveTick`
  writes only when the document is dirty and has a path (silent, like egui),
  never touching the main file; `startAutosave` drives it on a Timer (opt-in so
  tests own no pending timers). BRIDGE GAP CLOSED (v0.7): `_writeAutosave` now
  routes through the dedicated `lumit_bridge_autosave` op (rotating copy beside
  the project, WITHOUT re-pointing the engine's loaded path) when the loaded
  library carries the `EditOpsBridge` capability; an older library falls back to
  the previous rotate-then-`saveProject` path. INTEGRATOR: call
  `app.startAutosave()` once a bridge is live and point
  `autosaveInterval`/`autosaveKeep` at Settings → General.

## Phase F2 — Viewer (in progress)

- ☑ Composited-comp preview (all layers, transforms, effects), CPU path — K-175.
  The compositor is reached WITHOUT extracting it from `crates/lumit-ui` first:
  `export.rs`'s window-free `Renderer` is wrapped in a new `lumit_ui::headless`
  seam (`HeadlessRenderer`), and `lumit-bridge` (default-on `render` feature)
  holds one session-lifetime renderer and exposes
  `lumit_bridge_render_comp_frame`. Dart's `preview_source.dart` renders the
  WHOLE comp via `renderCompFrame` (a separate `CompRenderBridge` capability,
  probed once) and falls back per frame to the single-layer decode when a render
  returns null. Same compositor as export ⇒ preview == export == Flutter (K-031).
  A missing layer is slated as colour bars INSIDE the composited frame (engine
  side), so no separate Viewer slate on the comp path. `scale` downsamples the
  output only (full-res internal render — noted). Rust headless + bridge tests
  and Dart fake-bridge selection tests. `headless.rs`, `render.rs`, `ffi.rs`,
  `bridge.dart`, `preview_source.dart`, `viewer_panel.dart`
- ☑ Shared-texture path (zero-copy, K-177) — **built on Windows**, opt-in
  `shared-texture` feature. The engine renders into a **D3D12 shared texture**
  (shared heap + NT handle via `CreateSharedHandle`, wrapped back into wgpu with
  `create_texture_from_hal`), the bridge hands the handle across
  (`lumit_bridge_render_to_shared`/`_shared_supported`, no bytes), the Windows
  runner registers it as a `kFlutterDesktopGpuSurfaceTypeDxgiSharedHandle`
  external texture (`windows/runner/viewer_texture_bridge.{h,cpp}`), and the
  Viewer shows a `Texture` widget (`ViewerTextureController` owns the channel
  lifecycle). Verified end to end on the dev adapter (`lumit-ui` headless test:
  non-zero, stable handle across frames). **What remains / stays open:** the
  read-back path stays as the airtight automatic fallback (old dll / non-Windows
  / no D3D12 adapter / unwired runner — every seam tested with fakes) and feeds
  the **Scopes** via a throttled ~10 Hz CPU render (the texture path moves no
  pixels to the CPU); a keyed-mutex / shared-fence handshake is the named
  follow-up (only if tearing shows); the rendered-frame cache and engine-side
  render cancellation (below) are still open. The C++ runner plugin compiles only
  under `flutter build windows` on the owner's machine (the docs-first sandbox
  cannot run the Windows app toolchain).
- ☑ CPU RGBA fallback — **single-layer footage preview** (the fallback when comp
  render is unavailable: an old library, or no GPU adapter). The Viewer resolves
  the front comp's topmost visible footage layer whose span covers the playhead,
  maps comp-frame → source-frame by straight offset (Retime is not in the
  snapshot yet — noted in code), decodes one frame via `decodeFrame` through a
  shared `PreviewSource` (throttled to one decode per painted frame, an 8-entry
  `ui.Image` LRU), and blits it fit-to-panel on the neutral surround. NB the
  snapshot layer carries no source-item id, so the layer is matched to its
  footage item by name (documented limitation — the comp path has neither, the
  engine resolves everything). Unit- and widget-tested. `preview_source.dart`,
  `viewer_panel.dart`
- ◐ Transport: play/pause on a Ticker at the comp's rational fps, looping the
  composition (mirrors the egui transport, which loops the work area); frame +
  SMPTE timecode readout; `Full` resolution label as-is. Remaining: the
  resolution ladder is engine-side (a later phase), so the label is static

## Bridge v0.2 data + ops (done, feeds F2/F3/F4)

- ☑ Snapshot v2 (additive): comps carry `{width,height,fps,frame_count,layers,
  markers}`; footage carries `status` (ok/missing/unprobed/failed) and, once
  probed, `media` `{duration_frames,fps,width,height,audio}`. Frames are integers
  from the comp's own rate (rational time). Layer `kind`/`switches` mirror the
  model names. Typed Dart classes (`BridgeComp`/`BridgeLayer`/`BridgeSwitches`/
  `BridgeMedia`/`BridgeFps`) parse it; `AppStateStub.frontComp` resolves the
  active comp
- ☑ Layer/transform/marker ops through the real undo path: `set_layer_switch`,
  `edit_layer_span`, `set_transform`, `add_marker` (each one `lumit-core` op, one
  undo step, full snapshot back). Dart pass-throughs refresh the snapshot and
  surface failures on the error tint

## Bridge v0.3 read-back + ops (done, feeds F3/F4)

- ☑ Snapshot v3 (additive, ABI 2→3): each layer carries a `transform` read-back
  (`{value,animated,keys?}` per property; keys are `{frame,value,interp_in,
  interp_out}` with the `SideInterp` variant names), its identity link
  (`source_item_id`/`source_comp_id`/`colour`) and an `effects` array
  (`{id,name,enabled,params:[{name,kind,value}]}`); each comp carries `work_area`
  (`[in,out]` frames or null). Typed Dart classes (`BridgeTransform`/
  `BridgeTransformProperty`/`BridgeKeyframe`/`BridgeEffect`/`BridgeEffectParam`)
  parse it. `AppStateStub.transformValueFor` reads it, falling back to the
  session edit map
- ☑ Layer lifecycle ops: `add_solid_layer`/`add_text_layer`/`add_camera_layer`/
  `add_adjustment_layer`/`add_sequence_layer`, `delete_layer`, `duplicate_layer`
  — each mirrors the egui add/duplicate/delete defaults exactly, through
  `AddLayer`/`RemoveLayer` (solid as one `Batch`)
- ☑ `set_comp_settings` (one `SetCompSettings`, one undo step, background kept)
- ☑ Keyframe ops: `toggle_property_animated` (stopwatch), `add_keyframe`,
  `remove_keyframe`, `shift_keyframes` — mirroring `upsert_key`, delete-collapse
  and the lane's `shift_keys_time`, all via `SetTransformProperty`
- ☑ `set_work_area_edge` (the B/N keys, `SetWorkArea`)
- ☑ Effects: `list_effects` (the `BUILTINS` registry), `add_effect`,
  `remove_effect`, `set_effect_enabled`, `set_effect_param_scalar`/`_colour`
  (all `SetLayerEffects`). Point/file/layer param kinds are read-back only
- ◑ Footage probing on import/open (`media` feature): resolution, rate, frame
  count and status carried in the snapshot; missing files probe to `missing`,
  never an error. Synchronous at this phase; not yet off-thread, no thumbnails
- ◐ Transport ☑ (play/pause, frame + timecode, `Full` label); resolution picker
  + realtime tier readout ☐ (the ladder is engine-side, a later phase)
- ☑ Missing-footage slate: generated colour bars (band-for-band from
  `lumit-media/src/slate.rs`, drawn from `documentColour`, never a bundled
  asset) with the item path overlaid; a present-but-unreadable file shows a
  dark "unreadable" slate (docs/07 §3.3). `slate.dart`
- ☑ Scopes over the shown frame: Waveform (luma), Waveform (RGB), Vectorscope,
  Histogram — chosen in a `BareDropdown`, drawn on the fixed `ScopeColours`
  (never the theme), reading the same decoded pixels as the Viewer through the
  shared `PreviewSource`; the trace is built off the build path (256×256 image,
  rebuilt only when the shown frame or the scope changes) and the last trace is
  held when a frame is momentarily unavailable (K-130). Scope maths ported
  one-for-one from `scopes.rs` and unit-tested. `scopes_panel.dart`,
  `scope_maths.dart`
- ☐ Eyedropper magnifier; transform overlays

## Bridge v0.4 export + Retime + last columns (done, feeds F3/F4)

- ☑ Export over the headless seam (K-175, K-017): `start_export`/`export_poll`/
  `export_cancel` reuse `lumit_ui::export` on its own thread; the seam
  (`HeadlessRenderer::export_inputs`) builds the `ItemInfo` map + audio jobs +
  a shared GPU context. One export at a time; a second start returns
  `ok:false "an export is already running"` (Dart queues). `BridgeExportState`
  parses the poll reply
- ☑ Export preset resolver (`export_preset`, pure + always compiled): stamps the
  preset (codec/size/bitrate), applies the VBR-peak-preserved-while-unedited
  rule + 1.5× fallback, and renders the `{comp}`/`{preset}`/`{date}` filename
  template (Windows-sanitised, `.mp4` forced; blank template = the preset's own
  default byte-for-byte, K-119). Faithful port of `ExportDialogState::apply`/
  `spec` + `export_default_file_name`/`render_filename_template`/
  `sanitise_windows_filename`, with an unconditional round-trip test table.
  `BridgeExportPreset` parses it
- ☑ Full-run test: a tiny solid comp exports to a temp `.mp4` (gated behind
  `LUMIT_BRIDGE_EXPORT_TEST`; the spec/plumbing tests are unconditional). Passes
  on the dev box (the encoder ladder picks software x264 headless)
- ☑ Keyframe interpolation: snapshot keys carry `bezier_in`/`bezier_out`
  (`{speed,influence}`) on a `Bezier` side; `set_keyframe_interp` sets a key's
  interp via `SetTransformProperty`. `BridgeBezier` parses it
- ☑ Retime read-back: a footage layer carries `retime`
  (`{reverse,interpolation,boundaries,segments}`; boundaries as comp frames +
  seconds, segments tagged `rate`/`map`). Ops (all `SetLayerRetime`):
  `set_retime_enabled`, `set_retime_speed`, `set_segment_preset` (Lin/Slow/Fast/
  Smth/Shrp), `segment_to_rate` (→Rate, drift in the reply), `drag_boundary`.
  `BridgeRetime`/`BridgeRetimeBoundary`/`BridgeRetimeSegment` parse it
- ☑ Last columns: each layer carries `blend_mode`, `matte`
  (`{source,channel,inverted,source_mode}`) and `parent`; each comp carries
  `motion_blur`. Ops: `list_blend_modes`, `set_blend_mode`, `set_matte`,
  `set_parent`, `set_motion_blur`, `add_mask` (rectangle/ellipse/star).
  `BridgeMatte`/`BridgeMotionBlur`/`BridgeBlendMode` parse them; `AppStateStub`
  pass-throughs + an `pollExport` timer seam wired
- ☑ Session restore: nothing engine-side — open comps + playhead are Dart state,
  so `SavedSession` stays a frontend concern (confirmed)

## Phase F3 — Timeline (in progress)

- ☑ Comp-tab strip: one pill per composition in the snapshot (three-state fill,
  a local copy of the dock tab styling), clicking fronts that comp
  (`AppStateStub.frontCompId` + `frontCompSelect`); the current-time readout
  (frame + seconds at comp fps) sits at the strip's right
- ☑ Two-row time ruler over the lane: the full-height band (owner design change,
  see post-parity item 2), zoom-adaptive frame/second ticks + labels, markers as
  small flags, click/drag anywhere scrubs the playhead (`goToFrame`); the
  playhead is an accent line over ruler and lanes — unit-tested (tick density,
  scrub) + widget-tested
- ☑ Layer rows (22 px): outline column (fixed 260 px) with layer index, type
  glyph + 3 px colour tab, ellipsised name, and the switch cluster (eye, speaker
  — footage/sequence/precomp only, solo, lock, fx, motion blur, 3D, collapse),
  each toggling through `setLayerSwitch`; muted/hidden rows dim their name
- ☑ Outline degradation order (owner design change, see post-parity item 3):
  switches drop collapse/3D → fx/MB → solo → speaker → index as the column
  narrows, never overlapping; glyph + name + eye survive longest — unit-tested
- ☑ Clip bars on the lane: type-colour wash + 3 px tab + hairline edge (accent
  when selected); click a bar or outline row selects (`selectedLayer`, now a
  layer-id String); drag the body to move (one `move_in` op — see the SpanEdit
  note below), drag the 6 px edge handles to trim (`trim_in`/`trim_out`);
  snapping rounds drags to whole seconds and marker frames — widget-tested
- ☑ Bottom bar: zoom − / + / Fit + percentage readout, magnet snap toggle, graph
  lens toggle (kept from the F0 skeleton, zoom readout corrected to `zoom×100`).
  Wave 3 adds the **composition motion-blur master** here (`_MotionBlurMaster`,
  toggling `setMotionBlur` while preserving the shutter angle/phase/samples). The
  egui frontend puts it in the Timeline top row; that strip is shared, so it sits
  in the bottom bar for now — INTEGRATOR to move it up once the top row is owned
  in one place. The leading cluster scrolls horizontally so it never overflows a
  narrow panel — widget-tested
- ☑ Outline twirls + Transform property rows (wave 2): each layer row gains a
  disclosure twirl; open reveals a Transform group header and one 22 px row per
  transform property (Anchor point, Position, Scale, Rotation, Opacity — x/y
  pairs on one row with two readouts, plus the 3D rows when 3D). Each property
  row carries the drawn stopwatch (accent when animated, toggling through
  `togglePropertyAnimated` at the playhead), the read-back value(s) via
  `transformValueFor` (display-only; editing lives in Effect controls), and the
  shared ◄ ◆ ► navigator (prev/next jump the playhead, ◆ adds a key when between
  keys and removes when on one — egui note 2.4) — widget-tested
- ☑ Keyframe lanes (wave 2): an open property row draws its keys as
  interpolation-coded glyphs (diamond = Linear, square = a Hold side, circle = a
  Bezier side — ported from `graph.rs::key_shape`); click selects (Ctrl/Shift
  additive), drag slides the selected keys and commits **one** `shift_keyframes`
  per channel on release (live preview while dragging), right-click removes a key
  — unit-tested (glyph table, selection, grouping) + widget-tested
- ☑ Work area (wave 2): `comp.work_area` renders as the egui success strip on
  the ruler (dimmed outside, edge brackets); dragging an edge moves it through
  `setWorkAreaEdge`. B/N gain additive `workAreaInAtPlayhead`/`workAreaOutAtPlayhead`
  helpers on `AppStateStub` (the shell's B/N handlers still need repointing to
  these — see the deviation note) — geometry unit-tested + edge-drag widget-tested
- ☑ Layer context menu (wave 2 + wave 3): right-click a layer row → Duplicate
  (Ctrl+D), Delete and the Solo/Enabled/Motion-blur switch toggles wired to the
  real ops. Wave 3 adds the last-columns pickers and the mask shapes: **Add
  mask** ▸ Rectangle/Ellipse/Star (→ `addMask`), **Blend mode** ▸ the
  `listBlendModes` registry (→ `setBlendMode`), **Matte** ▸ None / another
  layer / Luma / Inverted (→ `setMatte`), **Parent** ▸ None / a non-cycling
  layer (→ `setParent`). egui shows blend/matte/parent as inline outline
  dropdowns in a wide row; the Flutter outline is narrow and column-degraded, so
  — taking the option the port allows — they live in the context menu instead
  (`panels/timeline/columns.dart`, the pure `parentingWouldCycle` unit-tested).
  Rename, Add effect and Convert still route to `app.engine(...)` honestly —
  widget-tested. egui's **Trim to source end** entry (offered on a retimed footage
  layer, `menu.rs:174-184`) now has a bridge op + `AppStateStub.trimToSourceEnd`
  (v0.7); the layer-menu entry itself is the section-C repoint
- ☑ Layer search (wave 2): a search box in the outline header filters rows by
  case-insensitive name substring — unit-tested + widget-tested
- ☑ Horizontal pan (wave 2): shift-wheel + a scrollbar when zoomed past fit,
  ruler and lanes locked through a session-persisted `viewStartFrame` on the
  `LaneScale` — clamp unit-tested
- ◐ Graph lens — the Retime **speed** lens (docs/04-RETIMING.md §9.2), ported
  from `graph.rs::graph_plot_retime`: when the graph toggle is on the lane area
  becomes the speed-over-time curve for the selected footage layer (`panels/
  timeline/graph_editor.dart` + the pure `graph_maths.dart`; one switch in
  `timeline_panel.dart`). Rate segments draw their native ease shape (v₀ →
  v₁ along e(u), the five `Ease` profiles ported exactly), Map segments draw
  their derived speed y′(u)/x′(u); the header carries the Retime enable toggle,
  the Lin/Slow/Fast/Smth/Shrp ramp presets (stamping the segment under the
  playhead via `setSegmentPreset`) and →Rate (`convertSegmentToRate`, surfacing
  a conversion notice); boundaries draw as verticals, the interior ones
  draggable (live curve preview, committing `dragBoundary` on release); 0%/100%
  reference lines, a labelled y-grid, and the playhead with a speed readout,
  all on the ruler's shared `LaneScale`. Unit-tested (ease shapes, sampling, the
  map derivative, segment-at-frame, boundary hit-test/clamp) + widget-tested
  (appears with the lens + a footage selection; preset click stamps the playhead
  frame; →Rate surfaces the notice; boundary drag commits `dragBoundary`).
  **Named remainder** (deliberately out of this slice — the egui graph editor's
  own gaps or bridge-plumbing limits): the Retime **Time**/value (source-
  position) lens and the transform value/speed graph (egui offers both, but the
  bridge's graph ops are speed-lens only, so a non-footage or un-retimed
  selection shows a calm hint rather than a value graph); the RATE/MAP
  **type chips** and ease-name label (§9.4), **kink badges** (§6.1) and overrun
  **hatching** (§7.2), per-boundary/per-segment **numeric % / t·s entry** fields
  (§9.3), the boundary-drag **beat/frame snapping**, the Vegas default-lens
  preference, and speed-keyframe (plain-store) drag handles
- ☐ Remainder (still open): matte/blend/parent columns, the top-row MB-master
  toggle, beat markers/cache bar, sequence sub-bars and the overrun HOLD hatch,
  resizable outline column, keyframe copy/paste

## Phase F4 — editors (in progress)

Effect controls rows · keyframe navigators · channel picker · Effects & presets
(.lumfx) · Scopes (waveform/vectorscope/histogram) · Hierarchy · Export
dialogue + queue · Comp settings · Add mask · Recovery modal

First slice (2026-07-21):

- ☑ Hierarchy: the front comp's layer tree — comp header (accent glyph + name),
  layers indented with the layer-type glyph/colour, precomp rows twirl open to
  reveal the nested comp's layers, click selects a layer by its stable id.
  `hierarchy_panel.dart`, widget-tested. Nesting is resolved by matching the
  precomp layer's *name* against the project's compositions, because snapshot v2
  tagged a precomp only as `kind:"precomp"`. Snapshot v3 now carries
  `source_comp_id` (and `source_item_id`/`colour`), so the panel can match by id
  and comp-scoped selection becomes possible — a panel adoption still to land.
  The full project flowchart / node graph is later.
- ◐ Effect controls: the selected layer's **Transform** rows and its **effect
  stack**, in the settings-card style. `effect_controls_panel.dart`, widget-
  tested (`f4_effects_test.dart`).
  - **Transform values now live**: each row seeds its value boxes from
    `AppStateStub.transformValueFor(layerId, property)` (snapshot v3 read-back
    first, session-edit fallback) — the em-dash placeholder and its hint are
    gone. Commits still route through `app.setTransform` (one undo step). Linked
    Scale is now ratio-preserving (a zero base falls back to matching the edited
    axis).
  - **Stopwatch + keyframe navigator** on every transform row: the stopwatch
    (accent when animated) toggles animation at `previewFrame` through
    `togglePropertyAnimated`; the shared ◄ ◆ ► navigator (ported from egui
    `keyframe_nav.rs` note 2.4) shows once a row is animated — ◄/► jump the
    playhead to the previous/next key via `app.goToFrame`, and ◆ adds a key at
    the playhead (`addKeyframe`) or removes the one already there
    (`removeKeyframe`). Multi-axis rows (Anchor/Position/Scale) drive their
    stopwatch and navigator across every axis; because the bridge keyframe ops
    are per-property (no batch op), a linked add/remove issues one op per axis
    (so it is more than one undo step — a named remainder until a batch op
    lands).
  - **Effect stack**: one card per effect in stack order — an enabled checkbox
    (`setEffectEnabled`), the registry label, and a quiet remove ×
    (`removeEffect`); parameter rows by kind — scalar as a `DragValueField`
    (`setEffectParamScalar`, unclamped drag since ranges are not in the
    snapshot), colour as a swatch opening `showColourPicker`
    (`setEffectParamColour`), and the three `channel_colour_1..3` params folded
    into one channel-picker row (K-143). enum/bool/seed/point/file/layer show
    their value read-only with an "edits arrive with the matching bridge op"
    tooltip — no faked edits.
  - Named remainder: the bridge now exposes (v0.7) the enum/bool/seed/point
    parameter setters (`set_effect_param_choice`/`_bool`/`_seed`/`_point`), the
    param **ranges** in the snapshot (`range: {min, max, slider_min, slider_max}`
    or `{options}`; `BridgeParamRange` parses them), effect **reorder**
    (`reorder_effect`), and the single-undo **keyframe batch**
    (`apply_keyframe_batch`, one `Op::Batch` for a linked x/y pair) — with matching
    `AppStateStub` pass-throughs. Still to wire in the panel (section D): the
    enum/bool/seed/point controls, the range-clamped drags, drag-to-reorder, and
    the linked-pair batch on Anchor/Position/Scale. Still no bridge surface:
    file/layer parameter edits, per-parameter keyframe stopwatch/navigator, and
    the eyedropper.
- ☑ Comp settings: the composition-settings / new-composition dialogue in the
  Settings-window visual style (name, size, frame-rate preset dropdown,
  duration), shown through the app Overlay. `dialogs.dart`, wired from the
  Composition menu, widget-tested. **Both commit for real now**: editing seeds
  the fields from the front comp and Apply commits the whole set through
  `app.setCompSettings(comp, name, w, h, fps_num, fps_den, duration_frames)` as
  one undo step (`fpsRational` maps the NTSC presets to their 1001 rationals).
  New composition creates the comp through `app.newComposition`, then applies
  size/rate/duration to it with `setCompSettings` as one visible flow (the
  bridge's `newComposition` takes only a name) — the stubbed `app.engine(…)`
  notice is gone.
- ◐ Effects & presets: a search field over the built-in effect registry
  (`app.listEffects()`, label substring, case-insensitive) and the matching
  effects listed, applied to the selected layer of the front comp
  (`app.addEffect`) by double-clicking a row or the Add button that appears on
  a hovered row; no selected layer shows a quiet hint. `effects_presets_panel.
  dart`, widget-tested (`f4_effects_test.dart`). The egui `effects_panel` groups
  the built-ins by `FxCategory` and lists user presets above them; the Dart
  registry now carries the category (v0.7: `BridgeEffectInfo {name, label,
  category, categoryLabel}` from `list_effects`), so the panel **can** group —
  the grouping UI itself is the section-D wave. Named remainder: the `.lumfx`
  **preset save/load** (needs the file + preset bridge ops — a placeholder row at
  the bottom says exactly that), the category-grouping UI, and drag-onto-a-layer
  application.
- ☑ Add mask ▸ Rectangle/Ellipse/Star: wired from the **layer context menu**
  (`addMask`, wave 3) and from the **Composition ▸ Add mask** menu bar
  (`shell/menu_bar.dart:90-94` → `addMaskToSelected`, corrected 2026-07-22 audit —
  the earlier "INTEGRATOR: repoint left for after the wave" note was stale; the
  menu-bar repoint is done). `AppStateStub.addMaskToSelected(kind)` exposes the
  selected-layer path (quiet error when none). The command palette has no Add-mask
  entry (egui's palette has none either), so there is nothing to repoint there.
- ☑ Export dialogue + queue + live progress: the Settings-window-style modal
  (`export_dialog.dart`) — preset dropdown (stamps codec/size/bitrate/name via
  `app.exportPreset`, the engine-side `ExportDialogState::apply` resolver), codec
  dropdown, size (comp size when Custom, with a "Use comp size" reset), a Mbps
  bitrate box (blank = encoder default; the 1.5× peak / preset-peak switch is
  resolved engine-side off the preset name we send), include-audio checkbox, the
  resolver's suggested file name, a Save-location picker (new
  `pickExportSaveLocation` seam in `file_dialogs.dart`), and Queue/Export +
  Cancel. Confirming calls `app.queueExport`; a Dart-side one-at-a-time queue
  (`AppStateStub`, a `VecDeque` mirror of `export_actions.rs`) starts the next on
  each done/failed. A shell `Timer` polls `app.exportPollTick` at ~4 Hz while one
  runs; the status line reads `exporting {name} {frame}/{total} · {enc} · {n}
  queued` with a × cancel, completion drops the quiet `exported {path} — encoded
  with {enc}` notice and failures take the error tint (app_update.rs wording).
  File → Export comp… / Export preset ▸ / the palette Export command open it;
  Export for sharing ▸ Discord 50 MB / Small 10 MB run the K-037 share maths
  (`shareExportBitRate`, a pure port of `start_share_export`) and start directly.
  Widget + app-state tested (`export_test.dart`). Without a bridge every entry
  keeps its F0 `engine` notice (pinned).
  - **Known behavioural difference (honest):** egui's queue snapshots the whole
    document at *queue* time (docs/06 §7.1); the bridge can only snapshot at
    *start* time (`start_export` reads the store then), so a Dart queue item
    holds the call arguments and the document is snapshotted when its turn comes
    — a later document edit reaches a still-queued export where egui froze it.
  - Named remainder: the share export's video peak (egui sets `max_rate: None`;
    the bridge's custom-preset resolver applies the customary 1.5× peak to any
    explicit bitrate, so a share export gains a VBR cap egui did not have), and
    the share `has_audio` budget test is a snapshot approximation (any audible
    footage layer whose source probed with audio) rather than the renderer's
    exact `comp_audio_jobs`.

## Performance (perf pass, K-176)

The owner reported the UI as "laggy af". Three causes were fixed; the evidence is
in `test/timeline_test.dart` (Playhead notifier split), `test/viewer_scopes_test.
dart` (off-thread renderer) and `test/timeline_columns_session_test.dart`
(debounced session).

- ☑ **Render isolate.** `PreviewSource` no longer calls `renderCompFrame` /
  `decodeFrame` synchronously on the UI isolate (K-017 / docs/14: the UI thread
  must never render a frame). The heavy render/decode rides a long-lived worker
  isolate (`panels/preview_isolate.dart`, `IsolateFrameRenderer`) that opens its
  OWN `DynamicLibrary.open` of the same `lumit_bridge.dll` — same process, so the
  same engine state behind the bridge's process-wide `Mutex`
  (`crates/lumit-bridge/src/state.rs`: `static BRIDGE: OnceLock<Mutex<Bridge>>`,
  held only for one call, never across a re-entrant call — so the render on the
  worker and document ops on the UI isolate serialise through the lock rather
  than race). Request/response over `SendPort`s carries `{compId/itemId, frame,
  scale, generation}` → `{TransferableTypedData rgba, w, h}`. **Latest-wins**: at
  most one render in flight, a newer wanted frame supersedes the queued one (the
  K-170 pattern applied to the Viewer); the last real picture stays on screen
  while a newer frame is in flight (never blank — the K-130 hold idea). The inline
  `SynchronousFrameRenderer` remains the fallback when isolates are unavailable
  (tests, the placeholder build, a machine where the worker cannot open the
  library).
  - **Remainder (◐):** the read-back path is a full-resolution CPU pixel readback
    per frame. The zero-copy end-state **is now built** (K-177): on Windows, with
    the `shared-texture` feature, the render stays on the GPU (a D3D12 shared NT
    handle Flutter samples directly) and the readback path is the fallback. What
    is still not done on top of that: the render is not cancelled mid-flight
    engine-side —
    latest-wins only drops the *reply* on the Dart side; a superseded comp render
    still runs to completion in the worker. The worker path is unverified in this
    sandbox (no native build / no `.dll`, and an isolate cannot bind a Dart fake),
    so it is exercised only by a deferred-reply fake renderer standing in for the
    worker; the inline path carries the shipped tests.
- ☑ **Playhead notifier split.** `AppStateStub` gained `ValueNotifier<int>
  playheadFrame`. Pure playhead motion (a scrub via `goToFrame`, a playback tick
  via `advancePlayback`) fires only that notifier, never the big `notifyListeners`
  — so layer rows, the Project/Hierarchy panels and the effect-controls body no
  longer rebuild at frame rate. Only the widgets that genuinely track the playhead
  per frame watch it: the Viewer transport frame/timecode, the Timeline playhead
  line and comp-tab clock, the Scopes/`PreviewSource` frame source, the graph
  readout, and the ◄◆► keyframe-navigator clusters (whose add-vs-remove sense
  depends on the live playhead). Guard: `advancePlayback`/`goToFrame` fire the app
  notifier zero times, and the `LayerRow` widgets keep their identity across 10
  advances (`test/timeline_test.dart`).
- ☑ **Debounced session persistence.** Session `remember` → `Workspace.save()`
  (JSON to disk) no longer fires per frame during a scrub. It coalesces on a
  ~500 ms trailing debounce, flushed on dispose and on project close (open/new).
  Autosave is untouched. Guard: a 30-frame scrub writes 0 times mid-scrub, one
  time after the flush (`test/timeline_columns_session_test.dart`).

- ◐ **Playhead-drag scrub smoothness (owner, desk-test round 2, 2026-07-22).**
  Scrubbing is "much better" after the perf pass above but still short of egui.
  Causal analysis (the mechanism, named): egui serves a scrub from an in-RAM
  rendered-frame cache — `AppState::comp_frame_cache` in
  `crates/lumit-ui/src/app_state/previewing.rs` (a per-frame map; a re-visited
  frame is a hash-map hit with no render, and it is also what feeds the timeline
  cache bar via `cache_bar()` at previewing.rs:201). The Flutter path renders
  **every** scrub frame fresh through the bridge (`renderCompFrame` on the worker
  isolate, `panels/preview_isolate.dart`) with a full-resolution GPU→CPU RGBA
  readback each time; the only reuse is the 8-entry `ui.Image` LRU in
  `preview_source.dart`, which is keyed on already-*decoded* single frames, not on
  rendered comp frames — so a re-scrubbed frame re-renders end to end. Fixes, in
  order of leverage:
  1. a **bridge-side rendered-frame cache** keyed exactly like egui's
     `comp_frame_cache` (comp+frame+scale → RGBA), so a re-scrubbed frame skips
     the render entirely — the highest-leverage change and the one that closes
     most of the gap;
  2. **engine-side render cancellation** — today latest-wins only drops the
     Dart-side *reply* (F2 render-isolate remainder); a superseded comp render
     still runs to completion in the worker, stealing the lock the next frame
     wants;
  3. the **shared-texture zero-copy path** (no readback at all — the F2
     "shared-texture path" row). **Done (K-177)**, shipped as a D3D12 shared NT
     handle (not the D3D11 route the earlier note guessed at); a keyed-mutex
     handshake is its own named follow-up.
  Items 1 and 2 not started; item 3 done.

## Desk-test round 2 findings (owner, 2026-07-22)

Three items the owner raised on the second desk-test. Item 3 (playhead-drag
smoothness) is recorded in full under **Performance (K-176)** above; items 1 and
2 are below.

### 1. "Right clicking menu items" — context-menu coverage

The owner wants right-click menus everywhere egui offers one. Per-surface audit
(egui call sites are `.context_menu(…)` in `crates/lumit-ui/src/**`; Flutter is
`onSecondaryTap*` handlers in `flutter_ui/lib/**`):

| Surface | egui offers (source) | Flutter has (source) | Gap |
|---|---|---|---|
| Bare pane | Pop out into its own window (`shell/dock.rs:231`) | Yes — Pop out, surfaces the multi-window notice (`shell/dock_widget.dart:642`) | **parity** (pop-out itself still deferred) |
| Layer row (outline name) | Rename · Add effect ▸ (categorised) · Add mask ▸ · Duplicate · Delete · Solo · Enabled · Motion blur · Convert to sequenced · **Trim to source end** (retimed footage) (`shell/timeline/menu.rs:27`, opened at `timeline/panel.rs:816`) | Rename\* · Add effect\* · Add mask ▸ · **Blend mode ▸ · Matte ▸ · Parent ▸** · Duplicate · Delete · Solo · Enabled · Motion blur · Convert\* (`panels/timeline/layer_menu.dart`) | **partial** — Rename / Add effect / Convert are stubs (`app.engine(…)`, layer_menu.dart:152-156); **Trim to source end missing**; Add effect not categorised. Blend/Matte/Parent added deliberately (narrow column, K-note) |
| Lane / property keyframe | Timeline lane key: right-click removes; graph-editor key: Easy ease · Linear · Hold · Unify handles · Delete key (`shell/graph.rs:1676`) | Property-row key right-click **removes only** (`panels/timeline/property_row.dart:137`) | **partial** — interpolation-set menu (Easy ease/Linear/Hold/Unify) missing; the graph-editor's own key right-click is entirely absent (`graph_editor.dart` has no `onSecondaryTap`) |
| Empty timeline lane | Composition settings · Reveal in project · Show time grid · Beat sensitivity slider + Detect beats · Clear beat markers (`timeline/panel.rs:384`) | none | **whole menu missing** |
| Comp-tab strip (empty space) | Pop out timeline (`shell/panels.rs:1139`) | none (`panels/timeline/comp_tabs.dart` is tap-only) | **missing** |
| Project row | Composition settings · Relink… (missing footage) · Find missing footage · Move to root · Delete (`shell/panels.rs:909`) | Composition settings… · Relink… · Find missing footage · Move to root · Delete (`panels/project_panel.dart` `showProjectContextMenu`) | **parity of items** — Comp settings wired to the dialogue; Relink / Find missing / Move to root / Delete are honest notice stubs (no bridge ops yet) |
| Viewer toolbar Shape tool | Rectangle / Ellipse / Star mask shape (`shell/app_update.rs:971`) | none — the Viewer has no draw-tool row | **missing** |
| Value field (DragValue) | egui built-in Reset / Copy / Paste on every drag box | none (`DragValueField`) | **missing** (minor) |
| Dock tab pill (inside a tab group) | none (egui gives only bare panes a menu) | none | parity |

### 2. "The layer area being in the wrong order" — investigated

Fact established from the code (topmost-first convention, all four in agreement):

- egui timeline draws rows **forward** over the model Vec — `for (layer_no, layer)
  in comp.layers.iter().enumerate()` (`shell/timeline/panel.rs:489`), so
  `layers[0]` is the **top** row, numbered "1" (`panel.rs:777-780`, `layer_no + 1`).
- The model's own contract agrees: `Op::AddLayer` / `Op::MoveLayer` document
  **"index 0 = top"** (`crates/lumit-core/src/ops.rs:50,60`); the bridge inserts
  new layers at index 0 (`crates/lumit-bridge/src/edits.rs:81-88`).
- The compositor renders `layers[0]` **topmost**: `render_comp_linear` builds the
  draw list with `comp.layers.iter().enumerate().rev()` (`export.rs:1463`), so
  `layers[0]` is pushed last, and `composite_seeded` paints the list front-to-back
  (`crates/lumit-gpu/src/composite.rs:852` — painter's order, comment at 248), so
  the last-pushed (`layers[0]`) lands on top.
- The bridge serialises layers **forward** (`crates/lumit-bridge/src/snapshot.rs:130`,
  `.iter().enumerate()`), and Flutter draws them **forward** — `for (final l in
  widget.comp.layers)` (`flutter_ui/lib/panels/timeline_panel.dart:224`, comment
  "top-first"), numbering `displayIndex + 1` (`panels/timeline/layer_row.dart:333`).

**Conclusion (honest):** the Flutter layer list is **not inverted** — row order,
1-based numbering and compositor stacking all agree with egui (`layers[0]` = top
row = topmost render). No code-level order defect was reproducible from static
reading. Should a live repro confirm a visible inversion, the single-line fix
would be to reverse the iteration at `timeline_panel.dart:224` — but the code as
written is correct, so this is left recorded, not fixed. **Most plausible real
cause of the owner's impression** (both to verify live): the Project panel cannot
open or reorder comps and no menu path adds a layer (see the sweep gaps), so what
the owner sees is whatever fixed order the loaded `.lum` already holds, with no
way to move a row — i.e. an *inability to reorder*, not a wrong render order.
Layer reorder-by-drag exists in egui (`panel.rs:799-814`, `Op::ReorderLayer`).
**Now shipped in the port** (see "Layer & footage placement" above): the bridge
`reorder_layer` op and a vertical-drag-on-outline reorder in the Flutter timeline
with a live insertion indicator — so the owner can move a row, not just view a
fixed order.

## Parity sweep gaps (2026-07-22 audit — newly recorded)

Found by the full sweep and **not** already tracked elsewhere in this file. Each
is a genuine miss (fires even with a live bridge) unless marked otherwise.

### Layer & footage placement (the biggest hole)

- ☑ **Add-layer menu + palette wired.** The Composition menu ("Add solid / text /
  camera / adjustment / sequence layer") and the matching command-palette entries
  now route to the real `AppStateStub.addSolidLayer … addSequenceLayer` ops for
  the front comp; with no composition open they surface a calm notice rather than
  the F0 stub (`shell/menu_bar.dart` `_addLayer`, `shell/command_palette.dart`
  `addLayer`). The palette's "Add marker at playhead" is wired too. Widget-tested
  (menu add-solid → `add_solid:<front comp>`).
- ☑ **Footage can be placed into a comp.** New bridge export
  `add_footage_layer(comp_id, item_id)` (ABI 5) mirrors egui's
  `add_footage_to_comp` (`crates/lumit-ui/src/app_state/layers.rs:60`): a Footage
  layer at index 0, the media's own duration/size when probed (else the full
  comp), anchored on the media centre placed at the comp centre. Journal-backed,
  full-snapshot reply, in-crate tests (add → footage layer with `source_item_id`;
  undo removes). The Flutter UI places it two ways: **double-click** a Project
  footage row (`addFootageToFrontComp`), and **drag** a Project footage row onto
  the Timeline lane (`Draggable`/`DragTarget`, `FootageDragData`). Both go to the
  top of the stack, mirroring egui (in-point at comp 0, not the drop frame).
- ☑ **Layer reorder-by-drag shipped.** New bridge export
  `reorder_layer(comp_id, layer_id, new_index)` (ABI 5) commits the real
  `Op::ReorderLayer` (`crates/lumit-core/src/ops.rs:64`; 0 = top, clamps
  out-of-range). Rust tests: bottom→top round-trips through undo; an out-of-range
  index clamps to the end. Flutter: a **vertical drag on a layer row's outline**
  reorders (live insertion-line indicator, commit on release), the target index
  computed from the other rows' centres exactly as egui's `layer_row_centers`
  (`panel.rs:1770`). The lane's horizontal grip/trim drags stay distinct.
  Widget-tested for both up and down moves.
- ◐ **Razor / clip editing — bridge ops landed (v0.7).** `cut_clip_at_playhead` /
  `delete_clip_at_playhead` (sequence layers, `SetSequenceClips`, undoable) +
  `AppStateStub.cutClipAtPlayhead`/`deleteClipAtPlayhead` (resolve the front comp,
  selected layer and playhead; a non-sequence layer is refused calmly). Still
  open: the menu-bar wiring (`shell/menu_bar.dart:76-77`) to these pass-throughs
  (section C).
- ◐ **Beat detection — bridge ops landed (v0.7).** `detect_beats(comp,
  sensitivity)` (synchronous — mixes the comp audio through the headless input
  builder and analyses, `media`+`render` features) / `clear_beat_markers` (always
  available) + `AppStateStub.detectBeats`/`clearBeatMarkers` on the front comp.
  Still open: the empty-lane / Composition-menu wiring
  (`shell/menu_bar.dart:87-89`, sections B/C) and, if long-audio latency bites, an
  off-thread start/poll pair (the egui `detect_beats`/`poll_beats` split).

### Project panel (interactive)

- ◐ **Project panel is now interactive** (`panels/project_panel.dart`): a click
  **selects** (highlight, `selectedProjectItem`), a **double-click** opens a
  composition (`frontCompSelect`) or places a footage item into the front comp as
  a layer (`addFootageToFrontComp`), a footage row is **Draggable** onto the
  Timeline, and a **right-click** raises the egui project menu (Composition
  settings…, Relink…, Find missing footage, Move to root, Delete). Composition
  settings opens the existing dialogue (fronting that comp first); the other four
  are honest notice stubs — **Delete** has no delete-item bridge op yet (state.rs
  / edits.rs carry only `delete_layer`), and Relink / Find missing footage /
  Move to root await their engine ops. Widget-tested. Still open: thumbnails, the
  missing-footage badge, rename, and the four stubbed menu ops.

### Settings & window

- ☐ **UI-scale persisted but never applied.** The Interface page writes
  `ws.interface.uiScale` (`shell/settings_window.dart:276-284`) and it round-trips
  to the workspace JSON, but **nothing consumes it** — no `MediaQuery` /
  `TextScaler` / window-metrics wiring exists in `flutter_ui/lib`, so the slider is
  inert. egui applies UI scale on slider release (K-117, 02-UI-INVENTORY §10).
- ☐ **Cache controls stubbed.** Performance page "Clear cache"
  (`settings_window.dart:334`) and "Choose cache root folder"
  (`settings_window.dart:367`) call `app.engine(…)`; no bridge cache op backs them.
- ◐ **Tooltip coverage is partial.** `LumitTooltip` exists and honours "Show
  tooltips", but it is applied to only a subset of controls (effects list, effect-
  controls rows, dock grip, Viewer play, a few timeline buttons, graph header).
  Many egui `on_hover_text` surfaces have none yet — layer switches, transport
  step/loop buttons, the ruler, the scopes header. Breadth gap, not a defect.

### Editors & viewer

- ◐ **Property editors beyond Transform — bridge ops landed (v0.7).** The setters
  now exist: `set_text_content(text, size, fill)`, `set_solid(colour, size)` (via
  the shared `SolidDef`, every layer using it updates), `set_camera_zoom`, with
  `AppStateStub.setTextContent`/`setSolid`/`setCameraZoom` pass-throughs. Still
  open: the editors themselves in `panels/effect_controls_panel.dart` (section D —
  text content is the sharp one).
- ◐ **Retime reverse toggle and interpolation switch — bridge setters landed
  (v0.7).** `set_retime_reverse` and `set_retime_interpolation`
  (`nearest`/`blend`/`flow`) edit the store fields (seed identity when the layer
  has none; a store that reverts to a pure identity clears the Retime, mirroring
  the egui Flow toggle) + `AppStateStub.setRetimeReverse`/`setRetimeInterpolation`.
  Still open: the graph-header toggle/switch UI (`panels/timeline/graph_editor.dart`,
  section C).
- ◐ **Recovery modal — bridge ops landed (v0.7), modal still to build.**
  `list_autosaves` (pure folder scan, `[{slot, path}]`) and `restore_journal`
  (open + replay the crash journal, reply carries `replayed`/`journal_total`) +
  `AppStateStub.listAutosaves`/`restoreJournal`. The modal surface itself
  (`shell/dialogs.dart`) is section E. NB the bridge does not yet *write* the
  journal on commit, so restore currently finds a journal a prior session left
  (named follow-up in 06 §A).
- ◐ **Splash boot log — bridge op landed (v0.7), splash still to wire.**
  `lumit_bridge_boot_log()` returns the engine's honest lines (library version,
  ABI, compiled feature set — no fabricated module lines) + `AppStateStub.bootLog`.
  Still open: streaming them into the splash (section E; the splash still shows
  canned lines).

### Unwired `app.engine(…)` action strings (full inventory)

Every distinct string, where it fires, and what it awaits. "no-bridge fallback"
= only shown when `bridge == null` (the placeholder build behaves as before, so
not a true gap); "**stub**" = fires even with a live bridge.

| String | Fires from | Status / awaits |
|---|---|---|
| New project / New composition / Undo / Redo / Save / Open project / Import footage | `state/app_state.dart:327-410` | no-bridge fallback (the real op runs when a bridge is present) |
| ~~Add solid / text / camera / adjustment / sequence layer~~ | menu bar + palette | **WIRED** — routed to `app.addSolidLayer` … `addSequenceLayer` on the front comp (no comp → notice) |
| ~~Add marker at playhead~~ (palette) | palette | **WIRED** — the palette copy now calls `app.addMarker` on the front comp (no comp → notice) |
| Export comp | palette default / `shell/shell.dart:120` | fallback when no export opener is wired in that context |
| Cut clip at playhead / Delete clip at playhead | menu bar (`menu_bar.dart:76-77`) | bridge op **landed** (v0.7, `AppStateStub.cutClipAtPlayhead`/`deleteClipAtPlayhead`); menu-bar repoint pending (section C) |
| Detect beats (sensitivity N) / Clear beat markers | menu bar (`menu_bar.dart:87-89`) | bridge op **landed** (v0.7, `AppStateStub.detectBeats`/`clearBeatMarkers`); menu repoint pending (sections B/C) |
| Clear cache / Choose cache root folder | settings (`settings_window.dart:334,367`) | **stub** — awaits a cache bridge op (section B) |
| Rename layer / Convert to sequenced layer | layer context menu (`layer_menu.dart:152-156`) | bridge op **landed** (v0.7, `AppStateStub.renameLayer`/`convertToSequenced`; `trimToSourceEnd` too); the in-place rename editor + Convert/Trim menu repoint pending (section C) |
| Add effect (categorised) | layer context menu | bridge exposes the category now (v0.7 `list_effects` carries it); the categorised layer-menu picker is section C |

## Post-parity fixes (owner's known rough edges — do NOT fix during the port)

Collected here as they come up so parity stays honest. Behavioural changes wait
until parity; the pure *defects* in this list (marked "defect") are different —
the port should simply not have them, and their absence must not be read as a
deviation when the two frontends are compared side by side.

Recorded 2026-07-21, from the owner:

1. **Settings dialogue z-order (defect).** In the egui Settings window (and
   similar surfaces), text can get stuck rendering behind input boxes. The
   Flutter port must keep labels and controls in one layout flow so nothing
   can overlap; do not reproduce.
2. **Timeline ruler height (design change, post-parity or at F3 build time).**
   The time ruler above the lane area should span the full two-row band —
   the egui frontend draws it one row tall even though the mouse-interaction
   region already covers both rows. Decide at F3: build the Flutter ruler
   two rows tall from the start (the owner's stated intent) and note the
   deliberate deviation here when done.
   → Built into the F3 timeline from the start (recorded deviation): the ruler
   band spans the full two-row height, labels up top, ticks below.
3. **Layer-area overcrowding (design change, F3).** The timeline's layer
   outline handles small panel sizes badly — switches, buttons and labels
   start overlapping as the column shrinks. The Flutter layer area needs a
   real degradation order (truncate/hide columns before anything overlaps).
   Design it during F3 rather than copying the egui behaviour.
   → Built into the F3 timeline from the start (recorded deviation): the outline
   measures its width and drops switches collapse/3D → fx/MB → solo → speaker →
   index, never overlapping; glyph + name + eye survive longest.
4. **Value boxes clip their row (defect).** DragValue-style value boxes
   across most of the egui UI clip at the bottom of the row they sit on.
   The Flutter `DragValueField` must size rows to fit their controls; do
   not reproduce. Owner notes there are probably more of this kind — add
   them here as they surface.

5. **Full editor-colour customisation (design change, post-parity).** The
   accent picker should eventually live in a more advanced appearance
   submenu where every editor colour can be edited, not just the accent
   (owner, 2026-07-21; matches the long-standing theme-customisation wish).
   The single accent picker stays until then.

## Known deliberate deviations

- No migration of the eframe-persisted workspace; the Flutter frontend starts
  from defaults (03-ARCHITECTURE §Persistence).
- The Settings window opens on General, not Appearance (owner request,
  2026-07-21).
- The splash is not skippable by click (matches egui, where the boot card is
  the window; an early click-to-skip experiment was removed on owner
  feedback, 2026-07-21).
- Menus animate per AnimationLevel (egui's could not animate at all).
- macOS native menu bar deferred with the rest of the macOS pass.
