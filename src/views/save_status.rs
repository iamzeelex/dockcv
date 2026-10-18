//! One place the app records "something went wrong touching the vault", and one
//! banner that says so.
//!
//! Before this, every one of the twenty call sites that writes to the vault
//! spelled the outcome `let _ = vault::save(…)`. A full disk, a permissions
//! change, a vault on a volume that has since been unmounted, a folder that has
//! gone read-only — the user kept typing and nothing reached the disk, with no
//! indication anywhere. For a product whose whole promise is that the files on
//! disk *are* the product, that is the failure that ends trust, so it is the one
//! failure that must be impossible to miss.
//!
//! The same banner carries the read half ([`report_unreadable`]), because a
//! document that will not parse now *stays* unplaced rather than being replaced
//! by a fresh sample — and refusing to open something is only defensible if the
//! refusal is visible.
//!
//! Held as a GPUI `Global` rather than a field, for the same reason
//! [`crate::theme`] is one: the writes happen in `Shell`, in `Root`, and in a
//! background task belonging to neither, while the banner is drawn exactly once
//! at the top of `Shell::render` — where it covers every screen, the editor
//! included. A field would have to be threaded through all three and kept in
//! sync.

use std::path::Path;

use gpui::prelude::*;
use gpui::{div, px, AnyElement, App, ClickEvent, Global, SharedString, Window};

use dockcv_ui_components::{Button, ButtonExt, StyledText, TextStyle};

use crate::presence;
use crate::theme::ActiveTheme;
use crate::vault;

/// The most recent vault problem, app-wide. At most one is shown: the banner
/// names one thing, and it should name the thing that just went wrong.
#[derive(Default)]
pub struct SaveStatus {
    notice: Option<Notice>,
    /// Bumped on every successful write, and read by
    /// [`super::vault_cache::VaultCache::refresh`].
    ///
    /// The cache notices other people's edits by fingerprinting the directory;
    /// this covers the one case a fingerprint cannot see — *our* write landing
    /// inside the same filesystem timestamp tick as the previous one and
    /// leaving the file exactly as long. It rides along here rather than in a
    /// global of its own because this function is already the single point
    /// every vault write passes through, and a second one would be a second
    /// thing to forget.
    revision: u64,
}

/// Whether a notice is a problem or a fact.
///
/// The banner was written for failures and is drawn in `danger` throughout,
/// which is the wrong voice for "your file was reloaded" — that is not a
/// problem, it is the app doing what it should and saying so. One field rather
/// than a second banner: it is the same shape in the same place, and two
/// banners would eventually disagree about which one wins.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tone {
    /// Something went wrong and nothing will fix itself.
    Alarm,
    /// Something happened that the user should know about.
    Note,
}

struct Notice {
    tone: Tone,
    /// What a later success clears. A successful write of the *same* thing is
    /// evidence this particular failure is over; a successful write of
    /// something else is not — a read-only `library.toml` must not be cleared
    /// by the diary saving fine.
    key: &'static str,
    title: String,
    detail: String,
    /// What to do about it. Static because there are only two situations and
    /// the words for each are fixed.
    hint: &'static str,
}

impl Global for SaveStatus {}

/// Shown under a failed write. Says the edits are not gone, because the user's
/// first question is whether they have to retype the last minute of work.
const WRITE_HINT: &str = "Your edits are still here on screen. Check the vault folder is \
     reachable and writable, then make one more edit to retry.";

/// Shown when the file changed under a document DockCV was holding. Says which
/// of the two versions is where, because both still exist and the user is the
/// only one who can say which is wanted.
const CONFLICT_HINT: &str = "Nothing was written — the file on disk is exactly as the other \
     editor left it, and the version on screen is still here. Leave this document to keep \
     the file's version, or copy what you need off the screen first.";

/// Shown under a failed open. Says the file was left alone, because the
/// previous behaviour was to overwrite it.
const READ_HINT: &str = "The file has been left exactly as it is — nothing was overwritten. \
     Open it in a text editor to fix it, or move it out of the vault.";

impl SaveStatus {
    /// Fold one write's outcome in. Split out from [`record`] so the clearing
    /// rule — the only real logic here — is testable without an `App`.
    fn apply(&mut self, what: &'static str, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.revision = self.revision.wrapping_add(1);
                if self.notice.as_ref().is_some_and(|n| n.key == what) {
                    self.notice = None;
                }
            }
            Err(message) => {
                self.notice = Some(Notice {
                    key: what,
                    tone: Tone::Alarm,
                    title: format!("Couldn't save your {what}"),
                    detail: message,
                    hint: WRITE_HINT,
                })
            }
        }
    }
}

/// Record the outcome of a vault write.
///
/// `what` is the noun the user recognises, not the file name: they know they
/// have a diary, they have never thought about `diary.toml`.
///
/// Callers pass the `Result` they used to discard. Nothing else about a call
/// site needs to change, which is what made converting all twenty of them a
/// mechanical edit rather than a redesign.
pub fn record(cx: &mut App, what: &'static str, result: Result<(), String>) {
    // Also to stderr: a user who hits this and reports it should have something
    // to paste, and the banner deliberately does not carry the whole OS error.
    // Debug, not info: writes are debounced every 600 ms while typing, so at
    // info this one line would bury the session. `DOCKCV_LOG=debug` turns the
    // full write trace on when a corruption report needs it.
    if result.is_ok() {
        log::debug!("wrote {what}");
    }
    if let Err(message) = &result {
        log::error!("could not save {what}: {message}");
    }
    cx.default_global::<SaveStatus>().apply(what, result);
}

/// Record a document write, and hand back the file state to hold until the
/// next one.
///
/// Every path that writes a document goes through here, so a conflict is
/// reported as a conflict rather than as a failure — the two need different
/// words. A failed write says "make one more edit to retry", which for a
/// conflict is exactly wrong: the retry would refuse again, and telling
/// somebody to repeat an action that cannot work is worse than saying nothing.
///
/// On a refusal the caller keeps the state it had. It has not agreed with the
/// file, and pretending otherwise would let the next write clobber it.
pub fn record_document(
    cx: &mut App,
    path: &Path,
    seen: vault::OnDisk,
    result: Result<vault::OnDisk, vault::SaveError>,
) -> vault::OnDisk {
    match result {
        Ok(now) => {
            record(cx, "document", Ok(()));
            now
        }
        Err(vault::SaveError::Conflict) => {
            report_conflict(cx, path);
            seen
        }
        Err(error) => {
            record(cx, "document", Err(error.message()));
            seen
        }
    }
}

/// Record a notebook write (`library`, `diary`, `applications board`, `profiles`),
/// reporting external file conflicts identically to documents.
pub fn record_notebook(
    cx: &mut App,
    what: &'static str,
    path: &Path,
    seen: vault::OnDisk,
    result: Result<vault::OnDisk, vault::SaveError>,
) -> vault::OnDisk {
    match result {
        Ok(now) => {
            record(cx, what, Ok(()));
            now
        }
        Err(vault::SaveError::Conflict) => {
            report_conflict(cx, path);
            seen
        }
        Err(error) => {
            record(cx, what, Err(error.message()));
            seen
        }
    }
}

/// The conflict hint when another DockCV has the vault open — the same facts,
/// plus the one thing that stops it happening again.
const SHARED_CONFLICT_HINT: &str = "Nothing was written — the file on disk is exactly as \
     the other DockCV left it, and the version on screen is still here. Leave this document \
     to keep the file's version, or copy what you need off the screen first. Quit one DockCV \
     to work in one.";

/// Say that a file changed under a document we are holding, and that both
/// versions are still here.
///
/// Reported from two places: a write that was refused, and the watcher finding
/// the change while the editor has unsaved edits of its own. Same situation,
/// same words.
pub fn report_conflict(cx: &mut App, path: &Path) {
    let others = presence::others(cx);
    log::warn!(
        "{} changed on disk under an edited document{}",
        path.display(),
        if others.is_empty() {
            ""
        } else {
            " — another DockCV has this vault open"
        }
    );
    cx.default_global::<SaveStatus>().notice =
        Some(conflict_notice(stem(path, "this document"), &others));
}

/// The words for a conflict, split out so they are testable without an `App`.
///
/// Another DockCV on the vault is by far the likeliest writer when there is
/// one, and the general list — an editor, a sync client, git — is a list of
/// things the user goes looking for that are not there. Still not stated as
/// certain: the detail says the other one is open, not that it wrote.
fn conflict_notice(name: &str, others: &[presence::Instance]) -> Notice {
    match others {
        [] => Notice {
            tone: Tone::Alarm,
            key: "document",
            title: format!("{name} was changed outside DockCV"),
            detail: "Something else — another editor, a sync client, a `git checkout` — wrote \
                 to this file after DockCV opened it."
                .to_string(),
            hint: CONFLICT_HINT,
        },
        _ => Notice {
            tone: Tone::Alarm,
            key: "document",
            title: format!("{name} was changed by {}", the_other(others)),
            detail: format!(
                "{} {} this vault open too, and saves to the same files as this window.",
                describe(others),
                if others.len() == 1 { "has" } else { "have" }
            ),
            hint: SHARED_CONFLICT_HINT,
        },
    }
}

/// Shown after a document was re-read because its file changed underneath it.
const RELOADED_HINT: &str = "DockCV had no unsaved changes of its own, so it took the \
     file's version. Undo puts back what was on screen before.";

/// [`RELOADED_HINT`] when "DockCV" alone would not say which of the two.
const SHARED_RELOADED_HINT: &str = "This window had no unsaved changes of its own, so it \
     took the file's version. Undo puts back what was on screen before.";

/// Say that a document on screen has been replaced by the file's version.
///
/// The app watches the vault, so an edit made in another editor — or by an
/// assistant working on the TOML directly — appears here without anyone
/// reopening anything. It still has to be *said*: a document that changes
/// while you are looking at it, with no explanation, is indistinguishable from
/// one that changed by itself.
pub fn report_reloaded(cx: &mut App, path: &Path) {
    let others = presence::others(cx);
    log::info!(
        "reloaded {} after an edit {}",
        path.display(),
        if others.is_empty() {
            "outside DockCV"
        } else {
            "with another DockCV on this vault"
        }
    );
    cx.default_global::<SaveStatus>().notice =
        Some(reloaded_notice(stem(path, "This document"), &others));
}

fn reloaded_notice(name: &str, others: &[presence::Instance]) -> Notice {
    match others {
        [] => Notice {
            tone: Tone::Note,
            key: "document",
            title: format!("{name} was edited outside DockCV"),
            detail: "The version on disk is now the one on screen.".to_string(),
            hint: RELOADED_HINT,
        },
        _ => Notice {
            tone: Tone::Note,
            key: "document",
            title: format!("{name} was edited in {}", the_other(others)),
            detail: format!(
                "{} {} this vault open too. The version on disk is now the one on screen.",
                describe(others),
                if others.len() == 1 { "has" } else { "have" }
            ),
            hint: SHARED_RELOADED_HINT,
        },
    }
}

/// Shown when another DockCV is found on the vault this one just opened.
const SHARED_HINT: &str = "Saves from either one are a change the other has to pick up. \
     Quit one to work in one.";

/// Key for the "also open in another DockCV" note, so nothing else clears it.
const SHARED: &str = "shared vault";

/// Say that another DockCV has this vault open — on opening it, and when one
/// turns up while this window was in the background.
///
/// A note, not an alarm: nothing has gone wrong yet, and both can be used.
/// It also does not push an alarm off the banner — a failed save is still the
/// thing to read first.
pub fn report_shared_vault(cx: &mut App, others: &[presence::Instance]) {
    let Some(notice) = shared_notice(others) else {
        return;
    };
    log::warn!("this vault is also open in {}", describe(others));
    let status = cx.default_global::<SaveStatus>();
    if status
        .notice
        .as_ref()
        .is_some_and(|n| n.tone == Tone::Alarm)
    {
        return;
    }
    status.notice = Some(notice);
}

fn shared_notice(others: &[presence::Instance]) -> Option<Notice> {
    let title = match others {
        [] => return None,
        [one] => format!("DockCV {} also has this vault open", one.version),
        several => format!("{} other DockCVs also have this vault open", several.len()),
    };
    Some(Notice {
        tone: Tone::Note,
        key: SHARED,
        title,
        detail: describe(others),
        hint: SHARED_HINT,
    })
}

/// "the other DockCV", or "another DockCV" when there is more than one and the
/// definite article would be a guess.
fn the_other(others: &[presence::Instance]) -> &'static str {
    if others.len() == 1 {
        "the other DockCV"
    } else {
        "another DockCV"
    }
}

fn describe(others: &[presence::Instance]) -> String {
    others
        .iter()
        .map(presence::Instance::describe)
        .collect::<Vec<_>>()
        .join("; ")
}

/// A file's name as the gallery shows it.
fn stem<'a>(path: &'a Path, fallback: &'a str) -> &'a str {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(fallback)
}

/// How many successful vault writes this process has made.
///
/// [`super::vault_cache::VaultCache`] compares it against its own copy so a
/// write DockCV made itself is never missed, whatever the filesystem's
/// timestamp granularity happens to be. Wraps rather than saturates: the cache
/// only ever asks "is this the same number as last time".
pub fn vault_revision(cx: &App) -> u64 {
    cx.try_global::<SaveStatus>().map_or(0, |s| s.revision)
}

/// Key used by the open/read half, so a later successful open clears it.
const OPEN: &str = "open";

/// Report that a document could not be read, naming it the way the gallery
/// does — by its file stem, which is what the user sees on the card.
pub fn report_unreadable(cx: &mut App, path: &Path, message: String) {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("this document");
    log::error!("could not open {}: {message}", path.display());
    cx.default_global::<SaveStatus>().notice = Some(Notice {
        tone: Tone::Alarm,
        key: OPEN,
        title: format!("Couldn't open “{name}”"),
        detail: message,
        hint: READ_HINT,
    });
}

/// Clear a stale "couldn't open" notice — called when a document does open, so
/// the message about the last broken one does not follow the user around.
pub fn clear_open_failure(cx: &mut App) {
    let status = cx.default_global::<SaveStatus>();
    if status.notice.as_ref().is_some_and(|n| n.key == OPEN) {
        status.notice = None;
    }
}

/// The banner, or nothing when there is nothing wrong.
///
/// Absolutely positioned by the caller's container so appearing and
/// disappearing never reflows the screen underneath — the user is mid-sentence
/// when this shows up, and content jumping under the caret would be its own
/// small betrayal.
pub fn banner(cx: &mut App) -> Option<AnyElement> {
    let notice = cx.try_global::<SaveStatus>()?.notice.as_ref()?;
    let theme = *cx.theme();
    let title: SharedString = notice.title.clone().into();
    let detail: SharedString = notice.detail.clone().into();
    let hint = notice.hint;
    // A fact is not an alarm. Tokens either way — see `CLAUDE.md`'s theme rule.
    let accent = match notice.tone {
        Tone::Alarm => theme.danger,
        Tone::Note => theme.accent,
    };

    Some(
        div()
            .absolute()
            .bottom(px(20.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            // The banner is an overlay; without this the full-width row would
            // swallow clicks meant for the screen behind it.
            .occlude()
            .child(
                div()
                    .max_w(px(560.0))
                    .flex()
                    .items_start()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .rounded(theme.radius_md())
                    .bg(theme.elevated)
                    .border_1()
                    .border_color(accent)
                    .shadow_lg()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_style(TextStyle::control())
                                    .text_color(accent)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::meta())
                                    .text_color(theme.text_muted)
                                    .child(detail),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::meta())
                                    .text_color(theme.text_subtle)
                                    .child(hint),
                            ),
                    )
                    .child(
                        Button::new("vault-notice-dismiss")
                            .quiet()
                            .flex_none()
                            .on_click(|_: &ClickEvent, window: &mut Window, cx: &mut App| {
                                // Clearing rather than a `dismissed` flag: if
                                // the condition persists, the next debounced
                                // write puts the banner straight back, which
                                // is the honest behaviour. Dismissing a
                                // still-failing save into silence is not.
                                cx.default_global::<SaveStatus>().notice = None;
                                window.refresh();
                            })
                            .child("×"),
                    ),
            )
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clearing rule is the whole subtlety here: a write that succeeds is
    /// only evidence about the thing it wrote.
    #[test]
    fn a_success_clears_only_its_own_failure() {
        let mut status = SaveStatus::default();

        status.apply("library", Err("read-only file system".into()));
        assert!(status.notice.is_some());

        // The diary saving fine says nothing about the library.
        status.apply("diary", Ok(()));
        assert!(
            status.notice.is_some(),
            "an unrelated success must not clear a live failure"
        );

        status.apply("library", Ok(()));
        assert!(status.notice.is_none(), "the same thing writing clears it");
    }

    /// The newest failure wins rather than being dropped because an older one
    /// is still on screen.
    #[test]
    fn a_later_failure_replaces_an_earlier_one() {
        let mut status = SaveStatus::default();
        status.apply("document", Err("no space left on device".into()));
        status.apply("diary", Err("permission denied".into()));

        let notice = status.notice.expect("still failing");
        assert_eq!(notice.key, "diary");
        assert_eq!(notice.detail, "permission denied");
        assert_eq!(notice.title, "Couldn't save your diary");
    }

    /// A write failure and a read failure must not clear each other — they are
    /// different problems, and the read half's whole point is that it says
    /// nothing was lost.
    #[test]
    fn a_successful_write_does_not_clear_an_unreadable_document() {
        let mut status = SaveStatus {
            revision: 0,
            notice: Some(Notice {
                tone: Tone::Alarm,
                key: OPEN,
                title: "Couldn't open “broken”".into(),
                detail: "parse error".into(),
                hint: READ_HINT,
            }),
        };

        status.apply("document", Ok(()));
        assert!(status.notice.is_some(), "a save says nothing about a read");
        assert_eq!(status.notice.as_ref().map(|n| n.hint), Some(READ_HINT));
    }

    fn installed() -> presence::Instance {
        presence::Instance {
            pid: 4121,
            version: "0.4.0".into(),
            executable: "/Applications/DockCV.app/Contents/MacOS/dockcv".into(),
            vault: "/vault".into(),
        }
    }

    /// The bug this wording exists for: with a second DockCV on the vault, the
    /// banner sent people looking for an editor, a sync client and a git
    /// checkout that were not there.
    #[test]
    fn a_conflict_with_another_dockcv_open_names_it() {
        let notice = conflict_notice("lilia", &[installed()]);
        assert_eq!(notice.title, "lilia was changed by the other DockCV");
        assert!(notice
            .detail
            .contains("DockCV 0.4.0 at /Applications/DockCV.app (pid 4121)"));
        assert!(!notice.detail.contains("sync client"));
        assert_eq!(notice.hint, SHARED_CONFLICT_HINT);
    }

    /// Alone on the vault, the words are what they always were.
    #[test]
    fn a_conflict_with_no_other_dockcv_keeps_the_general_wording() {
        let notice = conflict_notice("lilia", &[]);
        assert_eq!(notice.title, "lilia was changed outside DockCV");
        assert_eq!(notice.hint, CONFLICT_HINT);

        let reloaded = reloaded_notice("lilia", &[]);
        assert_eq!(reloaded.title, "lilia was edited outside DockCV");
        assert_eq!(reloaded.hint, RELOADED_HINT);
    }

    #[test]
    fn a_reload_from_another_dockcv_names_it() {
        let notice = reloaded_notice("lilia", &[installed()]);
        assert_eq!(notice.title, "lilia was edited in the other DockCV");
        assert!(notice.tone == Tone::Note);
        assert_eq!(notice.hint, SHARED_RELOADED_HINT);
    }

    #[test]
    fn the_open_note_names_one_or_counts_several() {
        assert!(shared_notice(&[]).is_none());

        let one = shared_notice(&[installed()]).expect("one other");
        assert_eq!(one.title, "DockCV 0.4.0 also has this vault open");
        assert_eq!(
            one.detail,
            "DockCV 0.4.0 at /Applications/DockCV.app (pid 4121)"
        );

        let mut dev = installed();
        dev.pid = 5000;
        let two = shared_notice(&[installed(), dev]).expect("two others");
        assert_eq!(two.title, "2 other DockCVs also have this vault open");
        assert_eq!(
            conflict_notice("lilia", &[installed(), installed()]).title,
            "lilia was changed by another DockCV"
        );
    }
}
