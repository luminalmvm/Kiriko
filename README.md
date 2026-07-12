# Kiriko

A native motion-graphics and compositing editor — After Effects' depth, Vegas' retiming
soul, one application. Built first for gaming-edit and montage editors; growing into a full
After Effects replacement. Rust · wgpu · egui · GPLv3.

**Status: design phase.** The complete system is specified in [docs/](docs/) before the
first line of application code; the specs are canonical and implementation follows them.

## Why

The montage scene edits in After Effects plus an expensive third-party plugin stack, and
lives with preview lag, crashes, and a retiming workflow many of them fight. Kiriko's
promises: playback at speed, degrade-never-crash, retiming as a first-class citizen with a
beat-sync covenant, and the genre's staple effects in the box. The full pitch:
[docs/00-VISION.md](docs/00-VISION.md).

## The documentation set

| Doc | What it specifies |
|---|---|
| [00-VISION](docs/00-VISION.md) | Why Kiriko exists, pillars, non-goals, the v1 milestone |
| [01-GLOSSARY](docs/01-GLOSSARY.md) | Canonical terminology — binding on all docs, UI, and code |
| [02-DECISIONS](docs/02-DECISIONS.md) | Numbered decision log with rationale |
| [03-DATA-MODEL](docs/03-DATA-MODEL.md) | Project/comp/layer/clip/property/keyframe object model |
| [04-RETIMING](docs/04-RETIMING.md) | The Retime system: segments, two graph lenses, the covenant |
| [05-ARCHITECTURE](docs/05-ARCHITECTURE.md) | Crates, threads, document snapshots, evaluation graph, GPU |
| [06-RENDER-PIPELINE](docs/06-RENDER-PIPELINE.md) | Render order, colour, caching, preview, export |
| [07-UI-SPEC](docs/07-UI-SPEC.md) | Panels, workspaces, Viewer, Timeline, graph editor, keymap |
| [08-EFFECTS](docs/08-EFFECTS.md) | Built-in effect suite (the montage staples in-box) |
| [09-AUDIO](docs/09-AUDIO.md) | v1 sync toolkit; the future Composer |
| [10-FILE-FORMAT](docs/10-FILE-FORMAT.md) | The .kir container, sidecar caches, autosave |
| [11-AE-IMPORT](docs/11-AE-IMPORT.md) | After Effects project import and the fidelity matrix |
| [12-PLUGINS](docs/12-PLUGINS.md) | OFX hosting, the KFX native API, expressions |
| [13-PERFORMANCE-RULES](docs/13-PERFORMANCE-RULES.md) | Budgets, resource governor, degradation ladder |
| [14-ENGINEERING-RULES](docs/14-ENGINEERING-RULES.md) | Binding rules for all code |
| [15-DESIGN](docs/15-DESIGN.md) | Dark-first Aizome design language |
| [16-ROADMAP](docs/16-ROADMAP.md) | Phases and their gates |

Two companion sets:
- [docs/impl/](docs/impl/) — implementation notes for the genuinely hard, low-level parts
  (rational time, cubic solving, wgpu patterns, hardware decode interop, the scheduler,
  optical flow, OFX hosting, beat detection, expression embedding): exact algorithms,
  reference code, traps, and test plans.
- [docs/research/](docs/research/) — the research notes that informed the specs.

## Licence

[GPLv3](LICENSE). Forks stay open; contributions welcome once implementation begins.
