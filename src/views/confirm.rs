//! Ask before doing something that cannot be undone.
//!
//! DockCV has no document-level undo: deleting a diary entry, an application, a
//! library block or a section variant rewrites the file and the previous
//! contents are gone. Deleting a *document* is different — it moves to
//! `.trash` — which is why that one still goes straight through, and why
//! emptying the trash is the most dangerous button in the app.
//!
//! The prompt is the platform's own alert rather than a widget of ours, for the
//! reason a native alert exists: it is modal, it steals the keyboard, and it
//! looks like every other "are you sure" the user has ever dismissed. A custom
//! sheet would be prettier and easier to click through by accident.
//!
//! Copy rules, applied to every caller here:
//!
//! * The title names **what** is being deleted, not "Are you sure?".
//! * The detail says plainly that it cannot be undone, and then says what is
//!   *not* affected — that is the question the user actually has, and answering
//!   it is what makes the dialog worth reading rather than worth clicking past.
//! * The confirm button repeats the verb ("Delete", "Empty Trash"), so the
//!   button says what happens rather than "OK".

use gpui::{Context, PromptLevel, Window};

/// Put a warning alert in front of `then`, and run it only on confirmation.
///
/// The confirm button is first in the list because that is the index this reads
/// back as `0`, and the platform draws it as the default. Cancel is second, and
/// is also what Escape and a dismissed dialog produce — every path that is not
/// an explicit yes leaves the data alone.
pub(super) fn destructive<T: 'static>(
    title: String,
    detail: String,
    confirm_label: &'static str,
    window: &mut Window,
    cx: &mut Context<T>,
    then: impl FnOnce(&mut T, &mut Window, &mut Context<T>) + 'static,
) {
    caution(title, detail, confirm_label, window, cx, then)
}

/// The same alert for something that is not a deletion but is still worth
/// stopping for — an export that may carry a confidential note outward
/// (US-36). Same mechanism, different name, because calling that call site
/// `destructive` would be a lie in the code.
pub(super) fn caution<T: 'static>(
    title: String,
    detail: String,
    confirm_label: &'static str,
    window: &mut Window,
    cx: &mut Context<T>,
    then: impl FnOnce(&mut T, &mut Window, &mut Context<T>) + 'static,
) {
    let answer = window.prompt(
        PromptLevel::Warning,
        &title,
        Some(&detail),
        &[confirm_label, "Cancel"],
        cx,
    );
    cx.spawn_in(window, async move |this, cx| {
        if answer.await == Ok(0) {
            let _ = this.update_in(cx, |this, window, cx| then(this, window, cx));
        }
    })
    .detach();
}

/// The sentence every one of these dialogs ends up needing, so it reads the
/// same everywhere and nobody has to invent it again.
pub(super) const CANNOT_UNDO: &str = "This can't be undone.";
