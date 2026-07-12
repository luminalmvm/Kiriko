//! Application state behind the shell: the document store, project path,
//! journal, dirty tracking, autosave. Slice 3 of docs/impl/phase-0-kickoff.md.

use kiriko_core::model::{Composition, Document, FootageItem, LinearColour, MediaRef, ProjectItem};
use kiriko_core::ops::Op;
use kiriko_core::time::{Duration, FrameRate, Rational};
use kiriko_core::DocumentStore;
use kiriko_project::JournalFile;
use std::path::{Path, PathBuf};
use std::time::Instant;
use uuid::Uuid;

pub const AUTOSAVE_INTERVAL_SECS: u64 = 300;
pub const AUTOSAVE_KEEP: usize = 5;

/// Infallible constructor for small literal rationals.
fn rat(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap_or(Rational::ZERO)
}

/// A recovery offer: the saved document plus the journal ops beyond it.
pub struct PendingRecovery {
    pub doc: Document,
    pub path: PathBuf,
    pub ops: Vec<Op>,
}

pub struct AppState {
    pub store: DocumentStore,
    pub path: Option<PathBuf>,
    journal: Option<JournalFile>,
    pub dirty: bool,
    pub selected_comp: Option<Uuid>,
    pub pending_recovery: Option<PendingRecovery>,
    pub error: Option<String>,
    last_autosave: Instant,
    comp_counter: usize,
}

impl Default for AppState {
    fn default() -> Self {
        let doc = Document::new();
        let journal = JournalFile::for_document(doc.id);
        Self {
            store: DocumentStore::new(doc),
            path: None,
            journal,
            dirty: false,
            selected_comp: None,
            pending_recovery: None,
            error: None,
            last_autosave: Instant::now(),
            comp_counter: 0,
        }
    }
}

impl AppState {
    fn report<T>(&mut self, r: Result<T, impl std::fmt::Display>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.error = Some(e.to_string());
                None
            }
        }
    }

    /// All document mutation funnels through here: commit, journal, dirty.
    pub fn commit(&mut self, op: Op) {
        match self.store.commit(op.clone()) {
            Ok(_) => {
                self.dirty = true;
                if let Some(journal) = &self.journal {
                    if let Err(e) = journal.append(&op) {
                        self.error = Some(format!("journal: {e}"));
                    }
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn undo(&mut self) {
        match self.store.undo() {
            Ok(Some(_)) => self.dirty = true,
            Ok(None) => {}
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn redo(&mut self) {
        match self.store.redo() {
            Ok(Some(_)) => self.dirty = true,
            Ok(None) => {}
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn install(&mut self, doc: Document, path: Option<PathBuf>, dirty: bool) {
        self.journal = JournalFile::for_document(doc.id);
        self.selected_comp = doc.items.iter().find_map(|i| match i {
            ProjectItem::Composition(c) => Some(c.id),
            _ => None,
        });
        self.store = DocumentStore::new(doc);
        self.path = path;
        self.dirty = dirty;
        self.comp_counter = 0;
    }

    pub fn new_project(&mut self) {
        if let Some(journal) = &self.journal {
            let _ = journal.clear();
        }
        self.install(Document::new(), None, false);
    }

    pub fn open_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter("Kiriko project", &["kir"])
            .pick_file();
        if let Some(path) = picked {
            self.open_path(&path);
        }
    }

    pub fn open_path(&mut self, path: &Path) {
        let Some((doc, _manifest)) = self.report(kiriko_project::open(path)) else {
            return;
        };
        // Crash recovery: a non-empty journal for this document means the last
        // session ended without a save (docs/10-FILE-FORMAT.md §4).
        let ops = JournalFile::for_document(doc.id)
            .and_then(|j| j.read().ok())
            .unwrap_or_default();
        if ops.is_empty() {
            self.install(doc, Some(path.to_owned()), false);
        } else {
            self.pending_recovery = Some(PendingRecovery {
                doc,
                path: path.to_owned(),
                ops,
            });
        }
    }

    pub fn resolve_recovery(&mut self, recover: bool) {
        let Some(pending) = self.pending_recovery.take() else {
            return;
        };
        let mut doc = pending.doc;
        if recover {
            let mut replayed = 0usize;
            for op in &pending.ops {
                if kiriko_core::ops::apply(&mut doc, op).is_err() {
                    break;
                }
                replayed += 1;
            }
            self.install(doc, Some(pending.path), true);
            if replayed < pending.ops.len() {
                self.error = Some(format!(
                    "recovered {replayed} of {} changes; the rest could not be replayed",
                    pending.ops.len()
                ));
            }
            // Journal stays until the user saves.
        } else {
            if let Some(journal) = JournalFile::for_document(doc.id) {
                let _ = journal.clear();
            }
            self.install(doc, Some(pending.path), false);
        }
    }

    pub fn save(&mut self) {
        let path = match &self.path {
            Some(p) => Some(p.clone()),
            None => rfd::FileDialog::new()
                .add_filter("Kiriko project", &["kir"])
                .set_file_name("untitled.kir")
                .save_file(),
        };
        let Some(path) = path else { return };
        let doc = self.store.snapshot();
        if self.report(kiriko_project::save(&doc, &path)).is_some() {
            if let Some(journal) = &self.journal {
                let _ = journal.clear();
            }
            self.path = Some(path);
            self.dirty = false;
        }
    }

    pub fn autosave_tick(&mut self) {
        if self.dirty
            && self.path.is_some()
            && self.last_autosave.elapsed().as_secs() >= AUTOSAVE_INTERVAL_SECS
        {
            self.last_autosave = Instant::now();
            if let Some(path) = self.path.clone() {
                let doc = self.store.snapshot();
                let _ = self.report(kiriko_project::autosave(&doc, &path, AUTOSAVE_KEEP));
            }
        }
    }

    pub fn import_footage_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter(
                "Media",
                &[
                    "mp4", "mov", "mkv", "avi", "webm", "png", "jpg", "jpeg", "wav", "mp3", "flac",
                ],
            )
            .pick_files();
        let Some(files) = picked else { return };
        let base = self.store.snapshot().items.len();
        for (i, file) in files.into_iter().enumerate() {
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "footage".into());
            let item = FootageItem {
                id: Uuid::now_v7(),
                name: name.clone(),
                extra: serde_json::Map::new(),
                media: MediaRef {
                    relative_path: name,
                    absolute_path: file.to_string_lossy().into_owned(),
                    extra: serde_json::Map::new(),
                },
            };
            self.commit(Op::AddItem {
                index: base + i,
                item: Box::new(ProjectItem::Footage(item)),
            });
        }
    }

    pub fn new_composition(&mut self) {
        self.comp_counter += 1;
        let comp = Composition {
            id: Uuid::now_v7(),
            name: format!("Comp {}", self.comp_counter),
            width: 1920,
            height: 1080,
            frame_rate: match FrameRate::new(60, 1) {
                Ok(fr) => fr,
                Err(_) => return,
            },
            duration: Duration(rat(30, 1)),
            background: LinearColour::BLACK,
            layers: Vec::new(),
            extra: serde_json::Map::new(),
        };
        let id = comp.id;
        let index = self.store.snapshot().items.len();
        self.commit(Op::AddItem {
            index,
            item: Box::new(ProjectItem::Composition(comp)),
        });
        self.selected_comp = Some(id);
    }

    pub fn project_title(&self) -> String {
        let name = self
            .path
            .as_deref()
            .and_then(Path::file_stem)
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into());
        if self.dirty {
            format!("{name} • Kiriko")
        } else {
            format!("{name} — Kiriko")
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The slice 3 drill: save, edit past the save, crash (drop without
    /// saving), reopen — the journal restores every post-save change.
    #[test]
    fn kill_and_recover_drill() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drill.kir");

        let doc_id;
        let final_json;
        {
            let mut app = AppState::default();
            doc_id = app.store.snapshot().id;
            app.new_composition();
            app.path = Some(path.clone());
            app.save();
            assert!(!app.dirty);

            // Edits after the save — journalled, never saved.
            app.new_composition();
            app.new_composition();
            assert!(app.dirty);
            final_json = serde_json::to_string(&*app.store.snapshot()).unwrap();
            // "kill -9": app dropped here with dirty state.
        }

        let mut app2 = AppState::default();
        app2.open_path(&path);
        let pending = app2.pending_recovery.as_ref().expect("recovery offered");
        assert_eq!(pending.ops.len(), 2);
        app2.resolve_recovery(true);
        assert_eq!(
            serde_json::to_string(&*app2.store.snapshot()).unwrap(),
            final_json,
            "recovered document equals the pre-crash document"
        );
        assert!(app2.dirty, "recovered state needs a save");

        // Saving clears the journal: a fresh open offers no recovery.
        app2.save();
        let mut app3 = AppState::default();
        app3.open_path(&path);
        assert!(app3.pending_recovery.is_none());

        let _ = JournalFile::for_document(doc_id).map(|j| j.clear());
    }

    #[test]
    fn discarding_recovery_opens_last_save_and_clears_journal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drill2.kir");
        let saved_json;
        {
            let mut app = AppState::default();
            app.new_composition();
            app.path = Some(path.clone());
            app.save();
            saved_json = serde_json::to_string(&*app.store.snapshot()).unwrap();
            app.new_composition(); // journalled, then "crash"
        }
        let mut app2 = AppState::default();
        app2.open_path(&path);
        assert!(app2.pending_recovery.is_some());
        app2.resolve_recovery(false);
        assert_eq!(
            serde_json::to_string(&*app2.store.snapshot()).unwrap(),
            saved_json
        );
        let mut app3 = AppState::default();
        app3.open_path(&path);
        assert!(
            app3.pending_recovery.is_none(),
            "journal cleared on discard"
        );
    }
}
