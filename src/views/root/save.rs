use std::time::Duration;

use gpui::{Context, Window};

use crate::vault;
use crate::views::save_status;

use super::Root;

/// Idle time after the last change before the document is written to its vault
/// file. Debounced so rapid edits coalesce into one write.
pub(super) const SAVE_DEBOUNCE: Duration = Duration::from_millis(600);

impl Root {
    /// Debounced background write of the document to its cvault file.
    ///
    /// The outcome is recorded rather than discarded: this is the write that
    /// runs on every keystroke, so it is also the one whose silent failure cost
    /// the most — a read-only vault meant an afternoon of edits existed only on
    /// screen.
    pub(super) fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let doc = self.doc.clone();
        let path = self.doc_path.clone();
        let executor = cx.background_executor().clone();

        let seen = self.on_disk;
        self.unsaved = true;
        self.save_task = Some(cx.spawn(async move |this, cx| {
            executor.timer(SAVE_DEBOUNCE).await;
            let result = executor
                .spawn({
                    let path = path.clone();
                    async move { vault::save(&doc, &path, seen) }
                })
                .await;
            let written = result.is_ok();
            cx.update(|cx| {
                let now = save_status::record_document(cx, &path, seen, result);
                let _ = this.update(cx, |this, _| {
                    this.on_disk = now;
                    // A refused or failed write leaves this set: the banner
                    // says what went wrong, this keeps saying that the file
                    // does not have your edit.
                    this.unsaved = !written;
                });
                // The banner lives on `Shell`'s frame, which nothing else here
                // touches, so this write needs its own repaint request.
                cx.refresh_windows();
            });
        }));
    }

    /// Write the document out now, ignoring the debounce.
    ///
    /// Called when the editor is about to stop existing — the app is quitting,
    /// the window is closing, or `Shell` is swapping the screen out from under
    /// it. Dropping the entity cancels [`Root::save_task`], so without this the
    /// last 600 ms of typing goes nowhere.
    pub fn flush_save(&self) -> Result<vault::OnDisk, vault::SaveError> {
        vault::save(&self.doc, &self.doc_path, self.on_disk)
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
        let profiles = vault::load_profiles(&self.vault_dir);
        if profiles != self.profiles {
            self.profiles = profiles;
            self.reset_layout_sliders();
            self.schedule_recompile(window, cx);
            cx.notify();
        }
        match vault::external_change(&self.doc, &self.doc_path, self.on_disk) {
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
        self.doc = doc;
        self.on_disk = seen;
        self.fields_stale = true;
        self.schedule_recompile(window, cx);
        save_status::report_reloaded(cx, &self.doc_path);
        cx.notify();
    }

    /// What this editor believes is on disk, for whoever flushes on its behalf.
    pub fn seen_on_disk(&self) -> vault::OnDisk {
        self.on_disk
    }
}
