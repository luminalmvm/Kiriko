# 06 — Remaining work (delete-on-done ledger)

Every partially finished (◐/◑) or not-started (☐) item extracted from
05-PARITY-CHECKLIST.md on 2026-07-22 (owner request). **Rows are deleted as they
land** — an empty section gets deleted too; an empty file means the transfer is
complete. 05 stays the permanent record; this file is the burn-down.

Excluded on purpose (not parity work): flutter_rust_bridge codegen (deferred by
design until the API stabilises), the macOS pass, the post-parity design changes
in 05 §post-parity, and the two recorded behavioural deviations (export
queue-snapshot timing; share-export VBR cap).

## A — bridge ops (Rust + Dart plumbing) — LANDED (bridge v0.7, ABI 6→7)

All section-A bridge ops shipped. Rust ops (in-crate tested, three feature
configs green) + FFI exports + typed Dart plumbing on the additive
`EditOpsBridge` capability interface (kept off `DocumentBridge` so the existing
fakes need no change) + `AppStateStub` pass-throughs (errors → `errorNotice`) +
`edit_ops_test.dart`. One caveat recorded, not a stub:

- **Beat detection** runs **synchronously** in the bridge (`detect_beats` mixes
  the comp audio through the headless input builder and analyses in one blocking
  call the Dart side awaits off its UI isolate), where egui runs it off-thread
  (`detect_beats`/`poll_beats`). If long-audio latency bites, a start/poll pair
  like the export ops is the follow-up — the maths is identical, only the
  threading differs. `clear_beat_markers` is always available (a plain marker
  edit). Detection needs the `media` + `render` features; a feature-less build
  reports that calmly.
- **Recovery `restore_journal`** replays whatever on-disk crash journal exists
  for the opened project's document id (the engine's `JournalFile` read+replay,
  the egui recovery path). The bridge does **not yet write** the journal on every
  commit, so today it recovers a journal a prior session (e.g. the egui app) left
  rather than one this bridge wrote — a named follow-up (wire journal-append into
  the bridge commit path, matching egui's `AppState::commit`). `list_autosaves`
  is a pure folder scan and is complete.

## B — performance follow-ups (K-176/K-177 remainders)

- Bridge-side rendered-frame cache keyed like egui's `comp_frame_cache`
  (comp+frame+scale → frame) — the highest-leverage scrub fix still open
- Engine-side render cancellation (a superseded render still runs to
  completion in the worker, blocking the lock)
- Settings cache controls: "Clear cache" / "Choose cache root folder" land on
  the new cache (stubs today)
- Fence/keyed-mutex handshake for the shared texture — only if the owner's
  live run shows tearing (verify first)
- Footage probing off-thread + Project-panel thumbnails (probe is synchronous;
  no thumbnails)

## C — timeline and graph UI

- Keyframe right-click interpolation menu: Easy ease / Linear / Hold / Unify
  handles / Delete (lane keys remove-only today; graph keys have no menu)
- Empty-lane context menu: Composition settings · Reveal in project · Show
  time grid · Beat sensitivity slider + Detect · Clear beat markers
- Comp-tab-strip right-click: Pop out timeline (routes to the multi-window
  notice until E lands)
- Graph editor: the Retime **Time**/value lens + transform value/speed graph;
  RATE/MAP type chips + ease labels; kink badges; overrun hatching; numeric
  % and t·s entry fields; boundary beat/frame snapping; Vegas default-lens
  preference; speed-keyframe drag handles
- Timeline remainder: beat markers + cache bar; sequence sub-bars; overrun
  HOLD hatch on clip bars; resizable outline column; keyframe copy/paste
  (Ctrl+C/V); move the MB master into the top row
- Transport: loop the **work area** when one is set (loops the whole comp
  today; `work_area` is in the snapshot now)
- Layer context menu: wire Rename (in-place editor), Add effect (categorised
  picker), Convert to sequenced, Trim to source end (ops from A)

## D — editors, viewer and panels

- Property editors beyond Transform: **text content** (the sharp one), solid
  colour/size, camera properties (ops from A)
- Viewer toolbar: the tool row (select/hand/shape/pen tools per the egui
  toolbar) with the Shape tool's right-click mask-shape picker
- Viewer transform overlays/gizmos for the selected layer; the eyedropper
  magnifier (sample the shown pixels, commit through the colour ops)
- Resolution picker + realtime tier readout (engine ladder; scale plumbing
  exists in the bridge render call)
- Project panel: thumbnails, missing-footage badge, rename, and the four
  context-menu ops wiring (ops from A)
- Hierarchy: adopt `source_comp_id` (id-based nesting instead of by-name);
  comp-scoped selection of nested layers
- Effects & presets: `.lumfx` preset save/load; category grouping (registry
  categories from A); drag-an-effect-onto-a-layer application
- Effect controls: per-parameter stopwatch/navigator on effect params

## E — chrome and shell

- Value-field context menu: Reset / Copy / Paste on every DragValueField
- UI-scale setting actually applied to the window (persisted but inert today)
- Tooltip breadth pass: layer switches, transport, ruler, scopes header —
  every egui `on_hover_text` surface
- Splash shows the engine's real boot log (op from A)
- Recovery modal: restore journal / last save / open an autosave (ops from A)
- Pop out a panel into its own OS window (multi-window — the one item with
  real platform risk; attempt last, record the outcome either way)

## Stale rows to reconcile in 05 while burning down

- RECONCILED (2026-07-22, with the section-A burn-down): the graph-lens "→Rate
  drift figure dropped by BridgeReply" remainder was stale — `driftSeconds` is
  threaded and the notice reads "fitted, N ms drift"; 05's F3 graph-lens
  named-remainder has been updated to drop the drift-figure caveat.
