//! Card chrome for the Applications board: the per-status tint each column
//! wears, and the meta lines a card draws under its title.
//!
//! Split out of `applications.rs` to keep that file under the ~800-line rule
//! (CLAUDE.md, "Conventions"). These are free functions rather than `Shell`
//! methods because none of them touches shell state — they take a theme and an
//! application and return elements.

use gpui::prelude::*;
use gpui::{div, px, AnyElement};

use dockcv_ui_components::{Icon, IconName, Sizable, StatusTint};

use crate::resume::model::{Application, ApplicationStatus, Snapshot};
use crate::theme::{StyledText, TextStyle, Theme};

use super::applications_data::{relative_from_iso, short_date};

/// The four status-tinted shades for one column. Wishlist has no colour of
/// its own; Applied and Interviewing share `accent`; Offer is `success`;
/// Rejected is `status_closed` (never `danger` — a rejection is the ordinary
/// outcome, not an error, per the design doc's own `Theme::status_closed`
/// doc comment).
pub(super) fn column_tint(theme: &Theme, status: ApplicationStatus) -> StatusTint {
    match status {
        ApplicationStatus::Wishlist => StatusTint::neutral(theme.border, theme.text_subtle),
        ApplicationStatus::Applied | ApplicationStatus::Interviewing => {
            StatusTint::of(theme.accent)
        }
        ApplicationStatus::Offer => StatusTint::of(theme.success),
        ApplicationStatus::Closed => StatusTint::of(theme.status_closed),
    }
}

/// The per-column meta line(s) below the chip: `saved/applied N ago`, the
/// Offer salary caption, the Rejected reason line, and — on every status that
/// implies a document was actually sent — the snapshot line, when one exists.
/// Never draws `★ N wins attached` (see this module's doc comment).
pub(super) fn card_meta(
    theme: &Theme,
    app: &Application,
    status: ApplicationStatus,
    now_secs: u64,
) -> Vec<AnyElement> {
    let mut meta: Vec<AnyElement> = Vec::new();

    match status {
        ApplicationStatus::Wishlist => {
            if let Some(rel) = relative_from_iso(&app.created, now_secs) {
                meta.push(meta_line(theme, format!("saved {rel}")));
            }
        }
        ApplicationStatus::Applied => {
            let date = app.applied.as_deref().unwrap_or(&app.created);
            if let Some(rel) = relative_from_iso(date, now_secs) {
                meta.push(meta_line(theme, format!("applied {rel}")));
            }
        }
        // Interviewing's status chip already carries this column's one line
        // of drawn content (the design doc's `★ N wins attached` third line
        // is deliberately not built — see this module's doc comment).
        ApplicationStatus::Interviewing => {}
        ApplicationStatus::Offer => {
            if !app.compensation.is_empty() {
                meta.push(prose_line(theme, app.compensation.clone()));
            }
        }
        ApplicationStatus::Closed => {
            // Lead with *which* ending. This column holds rejections,
            // ghostings and withdrawals alike, and "no reason given" under a
            // card you withdrew from reads as though someone turned you down.
            let ending = app
                .closed_as
                .map(|c| c.label().to_lowercase())
                .unwrap_or_else(|| "closed".to_string());
            let text = match &app.closure_note {
                Some(note) if !note.trim().is_empty() => format!("{ending}: {note}"),
                _ => ending,
            };
            meta.push(prose_line(theme, text));
        }
    }

    // Every status that implies a document was actually sent draws the
    // snapshot line, once one exists — the design doc's own ruling (§3) on
    // the mockup's Applied/Interviewing inconsistency.
    if matches!(
        status,
        ApplicationStatus::Applied | ApplicationStatus::Interviewing | ApplicationStatus::Offer
    ) {
        match app.snapshots.last() {
            Some(snapshot) => meta.push(snapshot_line(theme, snapshot)),
            // A sent card with no snapshot says so. The whole promise of this
            // surface is that it knows what the company received (US-04); when
            // it does not, the honest move is to show the hole and name the
            // reason, not to leave a blank where evidence should be.
            None if app.sent_as.is_none() => {
                meta.push(meta_line(theme, "no CV pinned".to_string()))
            }
            // O-19: "yet" was the whole problem. A capture is attempted the
            // moment a card is sent, so by the time this card is drawn the
            // attempt has already happened and failed — the banner said so
            // once and then went away, leaving a line that reads as "not yet"
            // forever. It says what is true instead, and the `···` menu
            // carries the retry.
            None => meta.push(meta_line(theme, "CV pinned, no snapshot".to_string())),
        }
    }

    // A word this build does not know (L-8). The card sits in Wishlist because
    // that is where an unreadable status lands, so without this line the file
    // says one thing and the board shows another with nothing to connect them.
    // The word is quoted rather than corrected: it is the user's, it is still
    // in the file, and only they know what they meant.
    if !app.status_is_recognised() {
        meta.push(meta_line(
            theme,
            format!("status “{}” not recognised", app.status_word),
        ));
    }

    meta
}

fn meta_line(theme: &Theme, text: String) -> AnyElement {
    div()
        .mt(px(2.0))
        .text_style(TextStyle::meta())
        .text_color(theme.text_subtle)
        .child(text)
        .into_any_element()
}

fn prose_line(theme: &Theme, text: String) -> AnyElement {
    div()
        .mt(px(2.0))
        .text_style(TextStyle::body())
        .text_color(theme.text_subtle)
        .child(text)
        .into_any_element()
}

fn snapshot_line(theme: &Theme, snapshot: &Snapshot) -> AnyElement {
    div()
        .mt(px(4.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .child(
            Icon::new(IconName::File)
                .with_size(theme.icon_sm())
                .text_color(theme.text_subtle),
        )
        .child(
            div()
                .text_style(TextStyle::meta())
                .text_color(theme.text_subtle)
                .child(format!(
                    "snapshot v{} · {}",
                    snapshot.version,
                    short_date(&snapshot.date)
                )),
        )
        .into_any_element()
}
