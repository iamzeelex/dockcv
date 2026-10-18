//! Leaving the editor while its CV and its file disagree.
//!
//! Every exit from the editor writes on the way out (`flush_editor`), so
//! leaving normally costs nothing. A real conflict is the exception: the file
//! changed outside DockCV while there were edits here, `vault::save` refuses
//! the flush, and the version on screen — the only copy of it — went with the
//! editor when the screen swapped. The banner offers both ways out, but a
//! banner can be dismissed or never read, and ⌘1 or ⌘Q does not stop to look
//! at it. So every exit asks first, and only then.
//!
//! The exits: the editor's own back control, the Preset Matrix, ⌘1–⌘4, ⌘N,
//! ⌘Q, ⌘W and the window's close button. Not covered: a quit the platform does
//! not let an app refuse — the Dock's Quit, logging out — where the flush in
//! `on_app_quit` stays the last word, as before.

use gpui::{AnyWindowHandle, Context, Entity, PromptLevel};

use crate::views::{save_status, Root};

use super::navigation::Screen;
use super::Shell;

/// The alert's buttons, in the order the platform reads them: the first is
/// Return's and `Cancel` is Escape's. Keeping the version that was being typed
/// is the reflex answer, and staying put — the one choice that changes nothing
/// — is the one a dismissed alert falls back to.
const ANSWERS: [&str; 3] = ["Keep My Version", "Use the File's Version", "Cancel"];

const DETAIL: &str = "The version on screen hasn't been saved. Keeping it replaces the \
     changes made outside DockCV; using the file's version sets yours aside, and Undo \
     brings it back until DockCV quits.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    KeepMine,
    UseFile,
    Stay,
}

impl Choice {
    /// Anything but an explicit choice stays — a dismissed alert as much as
    /// Cancel. Leaving is the thing that loses work, so it is the thing that
    /// needs a yes.
    fn from_answer(answer: Option<usize>) -> Self {
        match answer {
            Some(0) => Self::KeepMine,
            Some(1) => Self::UseFile,
            _ => Self::Stay,
        }
    }
}

impl Shell {
    /// Leave the editor, then do `then`.
    ///
    /// When the CV on screen is in conflict with its file, ask which version to
    /// keep first, and do `then` only once one is kept; Cancel stays in the
    /// editor and does nothing. Off the editor, `then` runs straight away.
    pub(crate) fn leave_editor(
        &mut self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        if self.asking_to_leave {
            return;
        }
        let Screen::Editor(editor) = &self.screen else {
            then(self, cx);
            return;
        };
        let editor = editor.clone();
        self.flush_editor(&editor, cx);
        if !editor.read(cx).in_conflict() {
            then(self, cx);
            return;
        }
        let Some(window) = self.window else {
            // Only before the window exists, when there is no editor either.
            // Leaving as before beats refusing to leave at all.
            log::warn!("leaving a CV in conflict with no window to ask in");
            then(self, cx);
            return;
        };
        self.ask_before_leaving(editor, window, cx, then);
    }

    /// Flush, and say whether leaving now would drop the version on screen.
    ///
    /// For the window's close button, which has to answer before anything can
    /// be asked: it refuses, and the alert comes up after.
    pub(crate) fn must_ask_before_leaving(&mut self, cx: &mut Context<Self>) -> bool {
        if self.asking_to_leave {
            return true;
        }
        let Screen::Editor(editor) = &self.screen else {
            return false;
        };
        let editor = editor.clone();
        self.flush_editor(&editor, cx);
        editor.read(cx).in_conflict()
    }

    /// Close the main window, which quits (`app.rs`'s `on_window_closed`).
    pub(crate) fn close_main_window(&mut self, cx: &mut Context<Self>) {
        if let Some(window) = self.window {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
    }

    fn ask_before_leaving(
        &mut self,
        editor: Entity<Root>,
        window: AnyWindowHandle,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        let name = editor
            .read(cx)
            .doc_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("this CV")
            .to_string();
        let title = format!("“{name}” was changed outside DockCV");
        let answer = window.update(cx, |_, window, cx| {
            window.prompt(PromptLevel::Warning, &title, Some(DETAIL), &ANSWERS, cx)
        });
        let Ok(answer) = answer else {
            log::warn!("could not ask before leaving a CV in conflict");
            then(self, cx);
            return;
        };
        self.asking_to_leave = true;
        cx.spawn(async move |this, cx| {
            let choice = Choice::from_answer(answer.await.ok());
            let _ = this.update(cx, |this, cx| {
                this.asking_to_leave = false;
                this.settle_and_leave(editor, window, choice, cx, then);
            });
        })
        .detach();
    }

    fn settle_and_leave(
        &mut self,
        editor: Entity<Root>,
        window: AnyWindowHandle,
        choice: Choice,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        // The window kept drawing while the alert was up; if something else
        // has already taken the editor away, the question is moot.
        if !matches!(&self.screen, Screen::Editor(open) if *open == editor) {
            return;
        }
        let settled = match choice {
            Choice::Stay => false,
            Choice::KeepMine => editor.update(cx, |root, cx| root.write_over_file(cx)),
            Choice::UseFile => window
                .update(cx, |_, window, cx| {
                    editor.update(cx, |root, cx| root.use_files_version(window, cx))
                })
                .unwrap_or(false),
        };
        // Not settled — cancelled, a write that failed, a file that would not
        // open: stay, with the banner saying which.
        if !settled {
            cx.notify();
            return;
        }
        // The checkpoint "Use the file's version" just took is the version
        // that was on screen; this is what keeps it for the next visit.
        self.remember_history(&editor, cx);
        if choice == Choice::UseFile {
            let path = editor.read(cx).doc_path.clone();
            save_status::report_set_aside(cx, &path);
        }
        then(self, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The order is what the platform keys on: Return takes the first answer,
    /// Escape takes the one labelled `Cancel`.
    #[test]
    fn only_an_explicit_answer_leaves() {
        assert_eq!(Choice::from_answer(Some(0)), Choice::KeepMine);
        assert_eq!(Choice::from_answer(Some(1)), Choice::UseFile);
        assert_eq!(Choice::from_answer(Some(2)), Choice::Stay);
        assert_eq!(ANSWERS[2], "Cancel");
        assert_eq!(
            Choice::from_answer(None),
            Choice::Stay,
            "an alert that went away without an answer"
        );
        assert_eq!(Choice::from_answer(Some(7)), Choice::Stay);
    }
}
