use std::path::Path;
use std::time::Duration;

use gpui::{Context, Window};

use crate::presence;
use crate::vault;
use crate::views::save_status;
use crate::views::vault_cache::Fingerprint;

use super::navigation::Screen;
use super::Shell;

/// How often the vault is checked for edits made outside DockCV, **while the
/// window is in front**.
///
/// The work is genuinely small — a directory of nine TOML files fingerprints in
/// 18 µs on a background thread, which at two ticks a second is 0.004% of one
/// core and 130 ms of CPU an hour. What is not small is doing it forever: a
/// timer that fires twice a second keeps waking a process that has nothing to
/// do, and a wakeup costs more than the work in it. Almost every one of those
/// ticks finds nothing, because almost nobody edits the vault from outside.
///
/// So it runs while somebody is looking at the app and not otherwise, which is
/// the honest reading of what it is for: nobody needs a preview refreshed on a
/// window they are not in front of. Coming back to the window checks once,
/// immediately — so switching from the editor you made the change in to DockCV
/// always shows the change, whatever the tick was doing.
const VAULT_WATCH_INTERVAL: Duration = Duration::from_millis(500);

impl Shell {
    /// Mark `dir` as open in this DockCV, and say so if another one has it
    /// open too.
    ///
    /// Every route to a vault comes through here — the one restored at launch
    /// and the one picked or created — because a vault opened by any of them
    /// is one whose files the other DockCV may be writing. See `presence`.
    pub(crate) fn announce_presence(&mut self, dir: &Path, cx: &mut Context<Self>) {
        let others = presence::announce(cx, dir);
        save_status::report_shared_vault(cx, &others);
    }

    /// Notice edits made to the vault by anything that is not DockCV.
    ///
    /// The README's promise is that the vault is plain text you can edit in any
    /// editor, and half of keeping it is not overwriting those edits
    /// (`vault::save`). This is the other half: seeing them. Without it the
    /// cache only re-read on the next frame — which happens when somebody moves
    /// the mouse, not when the file changes — so a document rewritten by
    /// another editor, or by an assistant working on the TOML directly, sat
    /// there stale until the window was touched.
    ///
    /// Polling rather than a filesystem-event API, and deliberately. A vault is
    /// a handful of small files, so the poll is free; events are not delivered
    /// reliably on the network and cloud-synced folders a vault is most likely
    /// to live in, which is exactly where somebody else's edit comes from; and
    /// every backend would have to be bridged onto GPUI's executor anyway. The
    /// cheap thing that always works beats the clever thing that usually does.
    fn watch_vault(&mut self, cx: &mut Context<Self>) {
        if self.vault_watch.is_some() {
            return;
        }
        let executor = cx.background_executor().clone();
        self.vault_watch = Some(cx.spawn(async move |this, cx| {
            // The first reading is a baseline, not a change: everything on disk
            // at that moment is what we just loaded.
            let mut seen: Option<Fingerprint> = None;
            loop {
                executor.timer(VAULT_WATCH_INTERVAL).await;
                let Ok(Some(dir)) = this.read_with(cx, |shell, _| shell.vault.clone()) else {
                    // No vault yet, or the window has gone. Only the second is
                    // a reason to stop.
                    if this.read_with(cx, |_, _| ()).is_err() {
                        return;
                    }
                    continue;
                };
                let taken = executor.spawn(async move { Fingerprint::of(&dir) }).await;
                if seen.as_ref() == Some(&taken) {
                    continue;
                }
                let first = seen.is_none();
                seen = Some(taken);
                if first {
                    continue;
                }
                if this
                    .update(cx, |this, cx| this.vault_changed_on_disk(cx))
                    .is_err()
                {
                    return;
                }
            }
        }));
    }

    /// Follow the window: watch while it is in front, stop when it is not, and
    /// look once on the way back in.
    ///
    /// Registered from `render`, which is the only place with a `Window`. The
    /// same reason `ensure_inputs` lives there.
    pub(crate) fn ensure_vault_watch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault_watch_activation.is_some() {
            return;
        }
        self.vault_watch_activation =
            Some(cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    // A DockCV started while this one was in the background
                    // is said once, here, rather than left for the first
                    // clash to reveal.
                    let newcomers = presence::newcomers(cx);
                    save_status::report_shared_vault(cx, &newcomers);
                    // Whatever happened while we were away, happened. Read it
                    // now rather than up to one interval later: this is the
                    // moment somebody looks.
                    this.vault_changed_on_disk(cx);
                    this.watch_vault(cx);
                } else {
                    this.vault_watch = None;
                }
            }));
    }

    /// Something in the vault is not what it was.
    ///
    /// Two things follow, and only one of them is this method's own work. The
    /// notify makes a frame happen, and `Shell::render` re-reads the vault into
    /// the cache as it always has — so the gallery, library, diary and
    /// applications all update by the path they already used. The editor is
    /// separate because it holds a document of its own; it is told to look, and
    /// looks on its next frame, where it has the `Window` a recompile needs.
    pub(crate) fn vault_changed_on_disk(&mut self, cx: &mut Context<Self>) {
        self.cache_stale = true;
        if let Some(draft) = self.drafting.as_mut() {
            draft.external_change_pending = vault::OnDisk::read(&draft.path) != draft.on_disk;
        }
        if let Screen::Editor(editor) = &self.screen {
            editor.update(cx, |editor, cx| {
                editor.external_change_pending = true;
                cx.notify();
            });
        }
        cx.notify();
    }
}
