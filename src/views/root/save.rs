use std::time::Duration;

use gpui::{Context, Window};

use crate::vault;
use crate::views::save_status;

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

        // A checkpoint first, so ⌘Z puts back what was on screen. The document
        // changing under you is exactly the moment you might want that.
        self.checkpoint();
        self.agreed = vault::OnDisk::of_document(&doc);
        self.doc = doc;
        self.on_disk = seen;
        self.fields_stale = true;
        self.schedule_recompile(window, cx);
        save_status::report_reloaded(cx, &self.doc_path);
        cx.notify();
    }
}
