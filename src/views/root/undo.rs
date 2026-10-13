//! Undo and redo for the **document**, as distinct from the text a field holds.
//!
//! Two stacks of whole `ResumeDoc` snapshots. A snapshot rather than a diff
//! because a CV is kilobytes of TOML and the document type is already `Clone` —
//! `schedule_save` clones it on every keystroke — so the cheap thing and the
//! simple thing are the same thing here, and an inverse-operation scheme would
//! be a new way for a delete and its undo to disagree.
//!
//! ## What this does and does not cover
//!
//! **Structural** changes: deleting an entry, a variant, a preset or a custom
//! section; adding one; reordering sections; applying a preset; pulling in a
//! library block or a diary line. These are the changes that lost data with no
//! way back, which is what [`super::confirm`] guards and what this makes
//! recoverable.
//!
//! **Typing does not checkpoint.** Every keystroke already runs through
//! `schedule_save`, and snapshotting there would bury the structural entries
//! under a character-by-character history — while the text field it came from
//! has had its own undo the whole time (`gpui-component` binds `cmd-z` inside a
//! focused input). So the two live side by side: while the caret is in a field,
//! `cmd-z` is that field's; otherwise it is the document's.
//!
//! That split resolves itself in the case that matters. Every structural change
//! sets `fields_stale`, which rebuilds `Root::fields` on the next frame and
//! drops the entity that held focus — so immediately after the delete you just
//! regretted, focus is on the editor and `cmd-z` means the document.

use gpui::{Context, Window};

#[cfg(test)]
use crate::resume::model::ResumeDoc;

use super::Root;

/// How many structural steps back the editor can go.
///
/// Bounded because each entry is a whole document. Sixty is far past the point
/// a person is still reconstructing what they did — and at CV scale it is a
/// couple of megabytes at worst, which is less than one gallery thumbnail used
/// to cost before that bug was fixed.
const UNDO_DEPTH: usize = 60;

impl Root {
    /// Record the document as it is *right now*, before changing it.
    ///
    /// Call this immediately **before** a structural mutation, not after: the
    /// stack holds the state to come back to.
    pub(super) fn checkpoint(&mut self) {
        self.undo_stack.push(self.doc.clone());
        if self.undo_stack.len() > UNDO_DEPTH {
            // Drop the oldest. `remove(0)` on a 60-element vector of documents
            // is nothing beside the clone that just happened.
            self.undo_stack.remove(0);
        }
        // A new edit invalidates the forward history, the same way it does in
        // every editor: you cannot redo into a future that no longer follows.
        self.redo_stack.clear();
    }

    pub(super) fn undo_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(previous) = self.undo_stack.pop() else {
            return;
        };
        let current = std::mem::replace(&mut self.doc, previous);
        self.redo_stack.push(current);
        self.after_history_move(window, cx);
    }

    pub(super) fn redo_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(next) = self.redo_stack.pop() else {
            return;
        };
        let current = std::mem::replace(&mut self.doc, next);
        self.undo_stack.push(current);
        self.after_history_move(window, cx);
    }

    /// Everything a document swap has to bring back into line.
    ///
    /// Preset state needs no repair here: the active or nearest preset is
    /// derived from `doc` after the swap, which is why undoing `Update` and
    /// `Revert` also restores the correct toolbar label.
    fn after_history_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The fields are bound to the old document; rebuilding them is what
        // makes the restored values appear in the boxes rather than only in
        // the preview.
        self.fields_stale = true;
        self.profile_detachment = None;
        self.profile_fork = None;
        self.reset_layout_sliders();
        self.focus_handle.focus(window, cx);
        cx.notify();
        self.schedule_recompile(window, cx);
        self.schedule_save(cx);
    }

    /// Whether there is a structural change to undo.
    pub(super) fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Whether there is an undone change to step forward into.
    pub(super) fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}

/// Standalone so the stack behaviour is testable without a `Window`, an `App`
/// or a running editor — which is most of what is worth testing here.
#[cfg(test)]
pub(super) fn push(stack: &mut Vec<ResumeDoc>, doc: &ResumeDoc) {
    stack.push(doc.clone());
    if stack.len() > UNDO_DEPTH {
        stack.remove(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::{Resume, SectionKind};

    fn doc_named(name: &str) -> ResumeDoc {
        let mut resume = Resume::default();
        resume.basics.name = name.into();
        ResumeDoc::from_resume(resume, "Base")
    }

    /// The stack is bounded, and it drops the *oldest* entry — losing the most
    /// recent one would make undo skip the step you just took.
    #[test]
    fn the_stack_is_bounded_and_forgets_the_oldest_first() {
        let mut stack: Vec<ResumeDoc> = Vec::new();
        for i in 0..(UNDO_DEPTH + 5) {
            push(&mut stack, &doc_named(&format!("step {i}")));
        }

        assert_eq!(stack.len(), UNDO_DEPTH);
        assert_eq!(
            stack.last().unwrap().profile.active().name,
            format!("step {}", UNDO_DEPTH + 4),
            "the newest step must survive"
        );
        assert_eq!(
            stack.first().unwrap().profile.active().name,
            "step 5",
            "the oldest five are the ones that went"
        );
    }

    /// A snapshot has to be a real copy: if undo handed back something sharing
    /// state with the live document, it would restore nothing.
    #[test]
    fn a_snapshot_is_independent_of_the_document_it_came_from() {
        let mut doc = doc_named("Albert Einstein");
        doc.add_variant(SectionKind::Work);
        let snapshot = doc.clone();

        doc.add_variant(SectionKind::Work);
        doc.profile.active_mut().name = "Someone else".into();
        doc.add_preset("Tailored");

        assert_eq!(snapshot.profile.active().name, "Albert Einstein");
        assert!(snapshot.presets.is_empty());
        assert_eq!(
            snapshot.variant_names(SectionKind::Work).len(),
            doc.variant_names(SectionKind::Work).len() - 1
        );
    }

    #[test]
    fn can_undo_and_can_redo_reflect_stack_emptiness() {
        let mut undo_stack: Vec<ResumeDoc> = Vec::new();
        let mut redo_stack: Vec<ResumeDoc> = Vec::new();

        assert!(undo_stack.is_empty());
        assert!(redo_stack.is_empty());

        let doc = doc_named("Doc 1");
        undo_stack.push(doc);

        assert!(!undo_stack.is_empty());
        assert!(redo_stack.is_empty());

        let popped = undo_stack.pop().unwrap();
        redo_stack.push(popped);

        assert!(undo_stack.is_empty());
        assert!(!redo_stack.is_empty());
    }

    #[test]
    fn session_history_round_trips_across_views() {
        let mut session_store: std::collections::HashMap<
            std::path::PathBuf,
            (Vec<ResumeDoc>, Vec<ResumeDoc>),
        > = std::collections::HashMap::new();

        let doc_path = std::path::PathBuf::from("/vault/resume.toml");
        let undo_stack = vec![doc_named("Initial"), doc_named("Step 1")];
        let redo_stack = vec![doc_named("Redo 1")];

        // "Exit editor" -> store in session
        session_store.insert(doc_path.clone(), (undo_stack.clone(), redo_stack.clone()));

        // "Reopen editor" -> restore from session
        let (restored_undo, restored_redo) = session_store.get(&doc_path).cloned().unwrap();
        assert_eq!(restored_undo.len(), 2);
        assert_eq!(restored_redo.len(), 1);
        assert_eq!(
            restored_undo.last().unwrap().profile.active().name,
            "Step 1"
        );
    }

    /// Undoing `Update <preset>` puts the old pins back, and the toolbar
    /// follows without being told.
    ///
    /// The whole reason C6 could delete `after_history_move`'s preset repair:
    /// the label is derived from the document, so restoring the document
    /// restores the label. This test is the checkpoint-then-restore that
    /// `Root::update_preset` performs, with the `Window` taken out of it.
    #[test]
    fn undoing_an_update_restores_the_presets_old_reading() {
        let mut doc = doc_named("Albert Einstein");
        doc.add_preset("Infra-heavy");
        let pinned_before = doc.presets[0].selection.clone();
        assert_eq!(doc.active_preset_index(), Some(0));

        // What `Root::update_preset` does: checkpoint, then rewrite the pins.
        let mut undo_stack: Vec<ResumeDoc> = Vec::new();
        push(&mut undo_stack, &doc);
        doc.add_variant(SectionKind::Work);
        assert_eq!(doc.active_preset_index(), None, "the working copy moved");
        assert!(doc.update_preset(0));
        assert_eq!(doc.active_preset_index(), Some(0), "the preset caught up");
        assert_ne!(doc.presets[0].selection, pinned_before);

        // What Undo does: swap the whole document back.
        doc = undo_stack.pop().expect("a checkpoint was pushed");

        assert_eq!(doc.presets[0].selection, pinned_before);
        assert_eq!(
            doc.active_preset_index(),
            Some(0),
            "restoring the document restores the label, with no view state to repair"
        );
    }

    /// Reverting is `apply_preset`, and it is undoable the same way — back to
    /// the edited working copy, not to the preset.
    #[test]
    fn undoing_a_revert_returns_to_the_edited_working_copy() {
        let mut doc = doc_named("Albert Einstein");
        doc.add_preset("Infra-heavy");
        doc.add_variant(SectionKind::Work);
        let edited = doc.variant_ids(SectionKind::Work);
        assert_eq!(doc.active_preset_index(), None);

        let mut undo_stack: Vec<ResumeDoc> = Vec::new();
        push(&mut undo_stack, &doc);
        doc.apply_preset(0);
        assert_eq!(doc.active_preset_index(), Some(0));

        doc = undo_stack.pop().expect("a checkpoint was pushed");

        assert_eq!(doc.active_preset_index(), None, "back to EDITED");
        assert_eq!(
            doc.variant_ids(SectionKind::Work),
            edited,
            "and the variant the revert switched away from is still there"
        );
    }
}
