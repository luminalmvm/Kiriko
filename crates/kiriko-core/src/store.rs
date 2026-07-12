//! The document store: immutable snapshots + operation journal
//! (docs/05-ARCHITECTURE.md; docs/impl/playback-scheduler.md §3).
//!
//! The UI thread is the single writer (by convention); readers grab an
//! `Arc<Document>` snapshot at any time, lock-free, and never observe a
//! half-applied edit.

use crate::model::Document;
use crate::ops::{apply, Op, OpError};
use arc_swap::ArcSwap;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// One journal entry: the op as applied, and its exact inverse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub op: Op,
    pub inverse: Op,
}

#[derive(Default)]
struct Journal {
    undo: Vec<JournalEntry>,
    redo: Vec<JournalEntry>,
}

pub struct DocumentStore {
    current: ArcSwap<Document>,
    journal: Mutex<Journal>,
}

impl DocumentStore {
    pub fn new(doc: Document) -> Self {
        Self {
            current: ArcSwap::from_pointee(doc),
            journal: Mutex::new(Journal::default()),
        }
    }

    /// Lock-free snapshot for readers (render jobs capture this at schedule time).
    pub fn snapshot(&self) -> Arc<Document> {
        self.current.load_full()
    }

    /// Apply an operation, journal it, publish the new snapshot.
    pub fn commit(&self, op: Op) -> Result<Arc<Document>, OpError> {
        let mut journal = self.journal.lock();
        let mut doc = Document::clone(&self.snapshot());
        let inverse = apply(&mut doc, &op)?;
        journal.undo.push(JournalEntry { op, inverse });
        journal.redo.clear();
        let arc = Arc::new(doc);
        self.current.store(arc.clone());
        Ok(arc)
    }

    /// Undo the most recent operation. Ok(None) when there is nothing to undo.
    pub fn undo(&self) -> Result<Option<Arc<Document>>, OpError> {
        let mut journal = self.journal.lock();
        let Some(entry) = journal.undo.pop() else {
            return Ok(None);
        };
        let mut doc = Document::clone(&self.snapshot());
        // Applying the inverse yields the original op again — symmetry by construction.
        let op = apply(&mut doc, &entry.inverse)?;
        journal.redo.push(JournalEntry { op, inverse: entry.inverse.clone() });
        let arc = Arc::new(doc);
        self.current.store(arc.clone());
        Ok(Some(arc))
    }

    /// Redo the most recently undone operation. Ok(None) when nothing to redo.
    pub fn redo(&self) -> Result<Option<Arc<Document>>, OpError> {
        let mut journal = self.journal.lock();
        let Some(entry) = journal.redo.pop() else {
            return Ok(None);
        };
        let mut doc = Document::clone(&self.snapshot());
        let inverse = apply(&mut doc, &entry.op)?;
        journal.undo.push(JournalEntry { op: entry.op, inverse });
        let arc = Arc::new(doc);
        self.current.store(arc.clone());
        Ok(Some(arc))
    }

    /// Ops applied since the store was created (or last save), oldest first —
    /// the crash-recovery log persisted by kiriko-project (slice 3).
    pub fn journal_ops(&self) -> Vec<Op> {
        self.journal.lock().undo.iter().map(|e| e.op.clone()).collect()
    }

    pub fn can_undo(&self) -> bool {
        !self.journal.lock().undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.journal.lock().redo.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::ops::Op;
    use crate::time::{CompTime, Duration, FrameRate, Rational};
    use uuid::Uuid;

    fn t(n: i64, d: i64) -> CompTime {
        CompTime(Rational::new(n, d).unwrap())
    }

    fn test_comp() -> Composition {
        Composition {
            id: Uuid::now_v7(),
            name: "Comp 1".into(),
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(60, 1).unwrap(),
            duration: Duration(Rational::new(30, 1).unwrap()),
            background: LinearColour::BLACK,
            layers: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }

    fn test_layer(item: Uuid) -> Layer {
        Layer {
            id: Uuid::now_v7(),
            name: "clip.mp4".into(),
            kind: LayerKind::Footage { item },
            in_point: t(0, 1),
            out_point: t(10, 1),
            start_offset: t(0, 1),
            switches: Switches::default(),
            extra: serde_json::Map::new(),
        }
    }

    fn json(doc: &Document) -> String {
        serde_json::to_string(doc).unwrap()
    }

    /// Build a scripted edit sequence against a fresh store.
    fn scripted_ops(doc: &Document) -> (Vec<Op>, Uuid) {
        let comp = test_comp();
        let comp_id = comp.id;
        let footage = FootageItem {
            id: Uuid::now_v7(),
            name: "capture.mp4".into(),
            extra: serde_json::Map::new(),
            media: MediaRef {
                relative_path: "footage/capture.mp4".into(),
                absolute_path: "/tmp/capture.mp4".into(),
                extra: serde_json::Map::new(),
            },
        };
        let layer = test_layer(footage.id);
        let layer_id = layer.id;
        let _ = doc;
        (
            vec![
                Op::AddItem { index: 0, item: Box::new(ProjectItem::Footage(footage)) },
                Op::AddItem { index: 1, item: Box::new(ProjectItem::Composition(comp)) },
                Op::AddLayer { comp: comp_id, index: 0, layer: Box::new(layer) },
                Op::SetLayerSpan {
                    comp: comp_id,
                    layer: layer_id,
                    in_point: t(1, 2),
                    out_point: t(19, 2),
                    start_offset: t(1, 2),
                },
                Op::RenameLayer { comp: comp_id, layer: layer_id, name: "hero shot".into() },
                Op::RenameItem { id: comp_id, name: "Main edit".into() },
            ],
            comp_id,
        )
    }

    #[test]
    fn undo_all_restores_initial_redo_all_restores_final() {
        let initial = Document::new();
        let initial_json = json(&initial);
        let store = DocumentStore::new(initial);
        let (ops, _) = scripted_ops(&store.snapshot());
        for op in ops {
            store.commit(op).unwrap();
        }
        let final_json = json(&store.snapshot());

        while store.undo().unwrap().is_some() {}
        assert_eq!(json(&store.snapshot()), initial_json, "undo-all == initial");

        while store.redo().unwrap().is_some() {}
        assert_eq!(json(&store.snapshot()), final_json, "redo-all == final");
    }

    #[test]
    fn journal_replay_reproduces_final_state() {
        let initial = Document::new();
        let mut replayed = initial.clone();
        let store = DocumentStore::new(initial);
        let (ops, _) = scripted_ops(&store.snapshot());
        for op in ops {
            store.commit(op).unwrap();
        }
        for op in store.journal_ops() {
            crate::ops::apply(&mut replayed, &op).unwrap();
        }
        assert_eq!(json(&replayed), json(&store.snapshot()));
    }

    #[test]
    fn snapshots_are_isolated_from_later_edits() {
        let store = DocumentStore::new(Document::new());
        let before = store.snapshot();
        let (ops, _) = scripted_ops(&before);
        for op in ops {
            store.commit(op).unwrap();
        }
        assert!(before.items.is_empty(), "old snapshot unchanged");
        assert_eq!(store.snapshot().items.len(), 2);
    }

    #[test]
    fn commit_clears_redo() {
        let store = DocumentStore::new(Document::new());
        let (ops, comp_id) = scripted_ops(&store.snapshot());
        for op in ops {
            store.commit(op).unwrap();
        }
        store.undo().unwrap();
        assert!(store.can_redo());
        store
            .commit(Op::RenameItem { id: comp_id, name: "diverged".into() })
            .unwrap();
        assert!(!store.can_redo(), "new edit invalidates the redo branch");
    }

    #[test]
    fn invalid_ops_leave_document_untouched() {
        let store = DocumentStore::new(Document::new());
        let before = json(&store.snapshot());
        let bogus = Op::RemoveItem { id: Uuid::now_v7() };
        assert!(store.commit(bogus).is_err());
        assert_eq!(json(&store.snapshot()), before);
        assert!(!store.can_undo());
    }
}
