use std::rc::Rc;
use std::time::Duration;

use gpui::{App, Context, Entity, Window};

use crate::resume::model::ResumeDoc;
use crate::vault;
use crate::views::{confirm, save_status};

use super::Root;

/// Idle time after the last change before the document is written to its vault
/// file. Debounced so rapid edits coalesce into one write.
pub(super) const SAVE_DEBOUNCE: Duration = Duration::from_millis(600);

impl Root {
    /// Write the document to its cvault file once the typing stops.
    ///
    /// Only the *wait* is a task. The write happens in [`Root::save_now`], on
    /// this thread, against the document and the file state as they are when
    /// the wait ends — not as they were when it began. Both halves used to live
    /// in the task, with `seen` captured at the keystroke and the write on a
    /// background thread, and that broke saving for good in two ordinary ways:
    ///
    /// * A keystroke landing while the write was in flight replaced this task,
    ///   and dropping a task does not stop a write that has started. The file
    ///   changed, the line that records it never ran, and every later save —
    ///   the flush on leaving included — compared the file against a state from
    ///   before DockCV's own write and refused: "changed outside DockCV".
    /// * Anything else that wrote meanwhile (the export recording itself) left
    ///   this task holding a `seen` that was no longer true, and its refusal
    ///   then put that stale value back over the right one.
    ///
    /// A CV is ~12 KB of TOML and the write measured 0.4 ms at p50 and 0.6 ms
    /// at worst; doing it here costs nothing a frame would notice, and it puts
    /// every write this editor makes in one order.
    pub(super) fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        self.unsaved = true;
        self.save_task = Some(cx.spawn(async move |this, cx| {
            executor.timer(SAVE_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                this.save_now(cx);
                // The banner lives on `Shell`'s frame, which nothing else here
                // touches, so this write needs its own repaint request.
                cx.refresh_windows();
            });
        }));
    }

    /// Write the document out now, ignoring the debounce. The one way this
    /// editor writes its file.
    ///
    /// Also called when the editor is about to stop existing — the app is
    /// quitting, the window is closing, or `Shell` is swapping the screen out
    /// from under it. Dropping the entity cancels [`Root::save_task`], so
    /// without this the last 600 ms of typing goes nowhere.
    ///
    /// The outcome is recorded rather than discarded: this is the write that
    /// runs on every pause in typing, so it is also the one whose silent
    /// failure cost the most — a read-only vault meant an afternoon of edits
    /// existed only on screen.
    pub fn save_now(&mut self, cx: &mut Context<Self>) {
        let seen = self.on_disk;
        let result = vault::save(&self.doc, &self.doc_path, seen);
        let written = result.is_ok();
        self.on_disk = save_status::record_document(cx, &self.doc_path, seen, result);
        if written {
            // What was written is this document's own serialization, so the
            // file's hash and the document's are one value again.
            self.agreed = self.on_disk;
        }
        // A refused or failed write leaves this set: the banner says what went
        // wrong, this keeps saying that the file does not have your edit.
        self.unsaved = !written;
    }

    /// Take the file's version of this document, when there is nothing of ours
    /// to lose by it.
    ///
    /// The vault is plain text and the README says so, which means somebody —
    /// a person in another editor, an assistant working on the TOML, a `git
    /// checkout` — will change a file while it is open here. Refusing to
    /// overwrite it (`vault::save`) keeps their work; this is the other half,
    /// which puts it on screen.
    ///
    /// Only when the editor has nothing unsaved. Then adopting the file is
    /// lossless and needs no decision from anyone. When both have changed it is
    /// a real conflict, and a conflict is reported rather than resolved: only a
    /// person knows which version they want.
    pub(super) fn adopt_external_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The directory watcher reports the vault, not one file. Profiles are
        // live shared data, so re-resolve them before deciding whether the CV
        // itself changed. A profiles-only edit should re-render immediately
        // and must not masquerade as an external document conflict.
        let (profiles, profiles_on_disk) = vault::load_profiles_seen(&self.vault_dir);
        // Whatever the content, the file is now known to hold these bytes.
        // Taking the profiles and keeping the old hash is what made the next
        // layout write from this editor refuse as "changed outside DockCV".
        self.profiles_on_disk = profiles_on_disk;
        if profiles != self.profiles {
            self.profiles = profiles;
            self.reset_layout_sliders();
            self.schedule_recompile(window, cx);
            cx.notify();
        }
        match vault::external_change(&self.doc, &self.doc_path, self.on_disk, self.agreed) {
            // Some other file in the vault moved, or this one was written by us.
            vault::ExternalChange::None => return,
            vault::ExternalChange::Conflict => {
                save_status::report_conflict(cx, &self.doc_path);
                return;
            }
            vault::ExternalChange::Adopt => {}
        }

        let (doc, seen) = match vault::load_seen(&self.doc_path) {
            Ok(loaded) => loaded,
            // Half-written, or being rewritten as we looked. Nothing is lost by
            // waiting: the watcher fires again on the next change, and until
            // then the version on screen is still the one we last agreed with.
            Err(message) => {
                log::debug!(
                    "{} changed but would not parse: {message}",
                    self.doc_path.display()
                );
                return;
            }
        };

        self.take_files_version(doc, seen, window, cx);
        save_status::report_reloaded(cx, &self.doc_path);
    }

    /// The banner's two ways out of a conflict, when the conflict is over the
    /// document this editor holds.
    ///
    /// A conflict is reported rather than resolved because only a person knows
    /// which version they want — and until now the person had no way to say.
    /// Every later write was refused against the old file state, and leaving
    /// the document dropped the version on screen — so the only way out was
    /// the file's version, whichever one the person wanted.
    ///
    /// Weak handles: the banner is drawn by `Shell`, and a click arriving after
    /// the editor has gone has nothing to act on.
    pub fn conflict_choices(this: &Entity<Self>, cx: &App) -> Option<save_status::ConflictChoices> {
        let path = save_status::conflict_path(cx)?;
        if path != this.read(cx).doc_path {
            return None;
        }
        let keep = this.downgrade();
        let take = this.downgrade();
        Some(save_status::ConflictChoices {
            keep_mine: Rc::new(move |window, cx| {
                let _ = keep.update(cx, |root, cx| root.keep_my_version(window, cx));
            }),
            use_file: Rc::new(move |window, cx| {
                let _ = take.update(cx, |root, cx| root.use_files_version(window, cx));
            }),
        })
    }

    /// Write the version on screen over the file, because the person chose it.
    ///
    /// `vault::save` refuses a file that is not the one this editor read, so
    /// that nothing is replaced without somebody deciding to replace it. This
    /// is the decision, so it does the one thing the refusal asks for: agree
    /// with what the file holds, then make the ordinary write.
    ///
    /// The file is read again here, after the confirmation, rather than
    /// remembered from when the conflict was reported: the choice is "mine over
    /// the file's", and the file's is whatever is there now. Only its hash —
    /// the other version may be half-written or not TOML at all, and neither
    /// is a reason to refuse to replace it.
    ///
    /// Behind a confirmation, as `confirm` puts everything that cannot be
    /// undone: the file's version was never on screen, so Undo has nothing to
    /// bring back.
    pub(super) fn keep_my_version(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = document_name(&self.doc_path);
        confirm::destructive(
            format!("Replace the file's version of “{name}”?"),
            format!(
                "The changes made to it outside DockCV will be gone. {} The version on \
                 screen is the one that will be saved.",
                confirm::CANNOT_UNDO,
            ),
            "Replace",
            window,
            cx,
            |this, _window, cx| {
                this.on_disk = vault::OnDisk::read(&this.doc_path);
                this.save_now(cx);
                // The banner is on `Shell`'s frame, not this one.
                cx.refresh_windows();
                cx.notify();
            },
        );
    }

    /// Put the file's version on screen in place of this one, because the
    /// person chose it.
    ///
    /// The same step the editor takes on its own when it has nothing unsaved,
    /// checkpoint included: here there *is* something unsaved, and Undo is what
    /// keeps choosing the file from costing it.
    pub(super) fn use_files_version(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match vault::load_seen(&self.doc_path) {
            Ok((doc, seen)) => {
                self.take_files_version(doc, seen, window, cx);
                save_status::report_file_chosen(cx, &self.doc_path);
            }
            // Nothing changes on screen, and the conflict stands — with its
            // buttons, since keeping the screen's version is still possible.
            Err(message) => {
                save_status::report_conflict_unreadable(cx, &self.doc_path, message);
            }
        }
        cx.notify();
    }

    /// Replace the document on screen with the file's, the one way this editor
    /// takes a version it did not write.
    fn take_files_version(
        &mut self,
        doc: ResumeDoc,
        seen: vault::OnDisk,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A checkpoint first, so ⌘Z puts back what was on screen. The document
        // changing under you is exactly the moment you might want that.
        self.checkpoint();
        self.agreed = vault::OnDisk::of_document(&doc);
        self.doc = doc;
        self.on_disk = seen;
        // The screen is the file now. A debounced write still waiting would
        // find a file DockCV did not write and rewrite it in DockCV's own
        // spelling — a change to it nobody made.
        self.save_task = None;
        self.unsaved = false;
        // Everything else a swapped document has to bring into line, as an
        // Undo does: the sliders and the profile prompts describe the old one.
        self.profile_detachment = None;
        self.profile_fork = None;
        self.reset_layout_sliders();
        self.fields_stale = true;
        self.schedule_recompile(window, cx);
        cx.notify();
    }
}

/// A document as the gallery names it: its file stem.
fn document_name(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("this document")
        .to_string()
}
