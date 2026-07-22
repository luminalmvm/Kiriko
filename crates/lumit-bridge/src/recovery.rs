//! The bridge v0.5 recovery and boot ops: the dedicated autosave, listing the
//! autosaves beside a project, replaying a crash journal, and the splash boot
//! log.
//!
//! # In plain terms
//!
//! Four small, mostly-pure surfaces the shell needs:
//!
//! - **Autosave** — write a rotating copy beside the project *without* changing
//!   the loaded path, so the periodic autosave never silently re-points Save to
//!   the autosave file (the known bridge drift gap). Mirrors
//!   `lumit_project::autosave` exactly, rebasing against the project folder the
//!   same way the egui `autosave_tick` does.
//! - **List autosaves** — scan the `autosaves/` folder beside a project for its
//!   rotating slots, newest first (pure file-system read, no document touched).
//! - **Restore journal** — open a project and replay its crash journal
//!   (`lumit_project::JournalFile`) on top, the egui recovery path. The bridge
//!   now *writes* the journal on every commit (v0.9, `crate::state::commit` /
//!   `journal_append`), so this recovers work THIS frontend left unsaved, not
//!   only a journal a prior egui session wrote.
//! - **Boot log** — the honest lines the engine can report for the splash:
//!   library version, ABI, the compiled feature set. No fabricated module lines.

use crate::err_json;
use crate::state::{snapshot, Bridge};
use lumit_core::store::DocumentStore;
use lumit_project::JournalFile;
use serde_json::json;
use std::path::PathBuf;

/// Resolve the target project path: the given `path`, or (when empty) the loaded
/// path. `Err` when neither is available.
fn target_path(bridge: &Bridge, path: &str, ctx: &str) -> Result<PathBuf, String> {
    if !path.trim().is_empty() {
        Ok(PathBuf::from(path))
    } else {
        bridge
            .path
            .clone()
            .ok_or_else(|| format!("{ctx}: no project path yet — save the project first"))
    }
}

/// Write a rotating autosave beside the project **without** re-pointing the
/// loaded path (the dedicated `lumit_bridge_autosave`, closing the drift gap the
/// `saveProject`-based autosave had). `keep` is the number of rotating slots
/// (clamped to at least 1). The document is rebased against the project folder,
/// exactly as the egui `autosave_tick` does, so no machine-specific path is
/// written. The reply carries the written autosave `path`; `bridge.path` is
/// untouched, so the next Save still writes the main project file.
pub(crate) fn autosave(bridge: &mut Bridge, path: &str, keep: usize) -> String {
    let ctx = "autosave";
    let project = match target_path(bridge, path, ctx) {
        Ok(p) => p,
        Err(e) => return err_json(e),
    };
    let dir = project.parent().unwrap_or_else(|| std::path::Path::new(""));
    let doc = lumit_project::rebase_for_save(&bridge.store.snapshot(), dir);
    match lumit_project::autosave(&doc, &project, keep.max(1)) {
        // Deliberately do NOT set bridge.path — that is the whole point.
        Ok(written) => json!({
            "ok": true,
            "path": written.to_string_lossy(),
        })
        .to_string(),
        Err(e) => err_json(format!("{ctx}: {e}")),
    }
}

/// List the rotating autosaves beside a project, newest first — the recovery
/// modal's "open an autosave" list. Pure: it scans the `autosaves/` folder for
/// `<stem>.autosave-N.lum` and reports each `{slot, path}` (slot 1 is newest).
/// `path` is the project path, or empty to use the loaded one. An empty list is
/// a clean `{ok:true, autosaves:[]}` (never an error), so "no autosaves yet" is
/// an ordinary answer.
pub(crate) fn list_autosaves(bridge: &Bridge, path: &str) -> String {
    let ctx = "list autosaves";
    let project = match target_path(bridge, path, ctx) {
        Ok(p) => p,
        Err(e) => return err_json(e),
    };
    let dir = project
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""))
        .join("autosaves");
    let stem = project
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    // Slots are `<stem>.autosave-N.lum`, N = 1 newest. Probe upward until a slot
    // is missing (rotation keeps them contiguous from 1).
    let mut autosaves = Vec::new();
    let mut n = 1usize;
    loop {
        let candidate = dir.join(format!("{stem}.autosave-{n}.lum"));
        if !candidate.is_file() {
            break;
        }
        autosaves.push(json!({
            "slot": n,
            "path": candidate.to_string_lossy(),
        }));
        n += 1;
        if n > 999 {
            break; // belt and braces against a pathological folder
        }
    }
    json!({ "ok": true, "autosaves": autosaves }).to_string()
}

/// Open a project and replay its crash journal on top — the egui recovery
/// "restore journal" path (`open_path` + `resolve_recovery(true)`). Opens the
/// `.lum`, reads the sidecar journal for its document id, and applies each op in
/// order, stopping at the first that no longer applies. Installs the recovered
/// document as the current one and points the bridge at the project path. The
/// reply is the refreshed snapshot with two added fields: `replayed` (ops
/// applied) and `journal_total` (ops found). `path` empty uses the loaded path.
/// A project with no journal replays zero and simply opens cleanly.
pub(crate) fn restore_journal(bridge: &mut Bridge, path: &str) -> String {
    let ctx = "restore journal";
    let project = match target_path(bridge, path, ctx) {
        Ok(p) => p,
        Err(e) => return err_json(e),
    };
    let (mut doc, _manifest) = match lumit_project::open(&project) {
        Ok(pair) => pair,
        Err(e) => return err_json(format!("{ctx}: {e}")),
    };
    let ops = JournalFile::for_document(doc.id)
        .and_then(|j| j.read().ok())
        .unwrap_or_default();
    let total = ops.len();
    let mut replayed = 0usize;
    for op in &ops {
        if lumit_core::ops::apply(&mut doc, op).is_err() {
            break;
        }
        replayed += 1;
    }
    bridge.store = DocumentStore::new(doc);
    bridge.path = Some(project);
    bridge.media.clear();
    // Arm the journal on the recovered document so further edits are captured.
    crate::state::set_journal_for_current_doc(bridge);
    crate::state::refresh_media(bridge);
    let mut v: serde_json::Value = match serde_json::from_str(&snapshot(bridge)) {
        Ok(v) => v,
        Err(_) => return snapshot(bridge),
    };
    v["replayed"] = json!(replayed);
    v["journal_total"] = json!(total);
    v.to_string()
}

/// The engine's honest boot lines for the splash (K-008). Each is a fact the
/// library can truthfully report at load time — its version, the ABI it speaks,
/// and the feature set it was compiled with. No fabricated module lines (the GPU
/// adapter is only known on the first render, so it is named as "probed on first
/// render" rather than asserted). Stateless.
pub(crate) fn boot_log() -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("lumit-bridge {}", env!("CARGO_PKG_VERSION")));
    lines.push(format!("ABI v{}", crate::ABI_VERSION));
    lines.push(format!(
        "media (decode/probe): {}",
        if cfg!(feature = "media") {
            "on — FFmpeg linked"
        } else {
            "off"
        }
    ));
    lines.push(format!(
        "compositor: {}",
        if cfg!(feature = "render") {
            "linked — GPU adapter probed on first render"
        } else {
            "off"
        }
    ));
    lines.push(format!(
        "zero-copy Viewer (shared texture): {}",
        if cfg!(all(windows, feature = "shared-texture")) {
            "available"
        } else {
            "unavailable — read-back path"
        }
    ));
    json!({ "ok": true, "lines": lines }).to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::state::{import_footage, new_composition};
    use serde_json::Value;

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).expect("reply is valid JSON")
    }

    #[test]
    fn autosave_writes_a_copy_without_repointing_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("scene.lum");
        let mut b = Bridge::new();
        new_composition(&mut b, "Scene");
        // Save once so a real project file exists and the path is set.
        crate::state::save_project(&mut b, &project.to_string_lossy());
        assert_eq!(b.path.as_deref(), Some(project.as_path()));
        // Make an edit, then autosave.
        import_footage(&mut b, "/media/clip.mp4");
        let reply = parse(&autosave(&mut b, "", 3));
        assert_eq!(reply["ok"], json!(true));
        // The autosave file exists…
        let written = PathBuf::from(reply["path"].as_str().unwrap());
        assert!(written.is_file());
        assert!(written.to_string_lossy().contains("autosave-1"));
        // …and the loaded path is UNCHANGED (the whole point of the op).
        assert_eq!(b.path.as_deref(), Some(project.as_path()));
    }

    #[test]
    fn list_autosaves_reports_slots_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("scene.lum");
        let mut b = Bridge::new();
        new_composition(&mut b, "Scene");
        crate::state::save_project(&mut b, &project.to_string_lossy());
        // Two autosaves → two rotating slots.
        autosave(&mut b, "", 5);
        autosave(&mut b, "", 5);
        let reply = parse(&list_autosaves(&b, ""));
        assert_eq!(reply["ok"], json!(true));
        let slots = reply["autosaves"].as_array().unwrap();
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0]["slot"], json!(1));
        assert_eq!(slots[1]["slot"], json!(2));
    }

    #[test]
    fn list_autosaves_with_no_folder_is_a_clean_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("fresh.lum");
        let b = Bridge::new();
        let reply = parse(&list_autosaves(&b, &project.to_string_lossy()));
        assert_eq!(reply["ok"], json!(true));
        assert_eq!(reply["autosaves"], json!([]));
    }

    #[test]
    fn restore_journal_opens_and_replays_zero_without_a_journal() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("scene.lum");
        let mut b = Bridge::new();
        new_composition(&mut b, "Scene");
        crate::state::save_project(&mut b, &project.to_string_lossy());
        // A fresh save has no crash journal, so restore opens cleanly, replaying 0.
        let mut b2 = Bridge::new();
        let reply = parse(&restore_journal(&mut b2, &project.to_string_lossy()));
        assert_eq!(reply["ok"], json!(true));
        assert_eq!(reply["replayed"], json!(0));
        assert_eq!(reply["journal_total"], json!(0));
        // The project loaded and the path is set.
        assert!(reply["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| { i["kind"] == json!("folder") || i["kind"] == json!("composition") }));
    }

    /// Journal-append on commit (v0.9): once `new_project` arms the journal,
    /// every bridge commit records its op, so `restore_journal` recovers *this*
    /// frontend's unsaved work. The journal is keyed by the document id, so the
    /// test reads it back and clears it (never leaving a cache-dir orphan).
    #[test]
    fn commit_appends_to_the_crash_journal() {
        use crate::state::{commit, new_project};
        use lumit_core::markers::Marker;
        use lumit_core::model::ProjectItem;
        use lumit_core::ops::Op;
        // new_project arms the journal on the fresh document.
        let mut b = Bridge::new();
        new_project(&mut b);
        // The journal starts empty for the fresh document.
        assert!(b.journal.is_some(), "new_project arms the journal");
        let _ = b.journal.as_ref().unwrap().clear();
        // A commit records its op.
        new_composition(&mut b, "Scene");
        let comp = b
            .store
            .snapshot()
            .items
            .iter()
            .find_map(|i| match i {
                ProjectItem::Composition(c) => Some(c.id),
                _ => None,
            })
            .expect("a comp exists");
        commit(
            &mut b,
            Op::SetCompMarkers {
                comp,
                markers: vec![Marker::user(
                    uuid::Uuid::now_v7(),
                    lumit_core::Rational::new(1, 1).unwrap(),
                )],
            },
            "test marker",
        );
        let ops = b.journal.as_ref().unwrap().read().expect("journal reads");
        assert!(
            ops.len() >= 2,
            "the new comp and the marker op are journalled, got {}",
            ops.len()
        );
        // A save clears the journal (it covers work between saves).
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("scene.lum");
        crate::state::save_project(&mut b, &project.to_string_lossy());
        let after_save = b.journal.as_ref().unwrap().read().expect("journal reads");
        assert!(
            after_save.is_empty(),
            "a successful save clears the journal"
        );
        // Tidy up the cache-dir sidecar for this unique document id.
        let _ = b.journal.as_ref().unwrap().clear();
    }

    #[test]
    fn boot_log_reports_version_and_features() {
        let reply = parse(&boot_log());
        assert_eq!(reply["ok"], json!(true));
        let lines = reply["lines"].as_array().unwrap();
        assert!(!lines.is_empty());
        assert!(lines
            .iter()
            .any(|l| l.as_str().unwrap().contains("lumit-bridge")));
        assert!(lines.iter().any(|l| l
            .as_str()
            .unwrap()
            .contains(&format!("ABI v{}", crate::ABI_VERSION))));
    }
}
