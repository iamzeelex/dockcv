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
//! document that will not parse now *stays* unparsed rather than being replaced
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

use dockcv_ui_components::{StyledText, TextStyle};

use crate::theme::ActiveTheme;

/// The most recent vault problem, app-wide. At most one is shown: the banner
/// names one thing, and it should name the thing that just went wrong.
#[derive(Default)]
pub struct SaveStatus {
    notice: Option<Notice>,
}

struct Notice {
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
                if self.notice.as_ref().is_some_and(|n| n.key == what) {
                    self.notice = None;
                }
            }
            Err(message) => {
                self.notice = Some(Notice {
                    key: what,
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
    if let Err(message) = &result {
        eprintln!("DockCV: could not save {what}: {message}");
    }
    cx.default_global::<SaveStatus>().apply(what, result);
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
    eprintln!("DockCV: could not open {}: {message}", path.display());
    cx.default_global::<SaveStatus>().notice = Some(Notice {
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
    let theme = cx.theme().clone();
    let title: SharedString = notice.title.clone().into();
    let detail: SharedString = notice.detail.clone().into();
    let hint = notice.hint;

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
                    .rounded_lg()
                    .bg(theme.elevated)
                    .border_1()
                    .border_color(theme.danger)
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
                                    .text_color(theme.danger)
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
                        div()
                            .id("vault-notice-dismiss")
                            .flex_none()
                            .px_2()
                            .py(px(1.0))
                            .rounded_md()
                            .text_style(TextStyle::control())
                            .text_color(theme.text_muted)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.hover).text_color(theme.text))
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
            notice: Some(Notice {
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
}
