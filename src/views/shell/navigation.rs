use std::path::PathBuf;

use gpui::{prelude::*, Context, Entity, Window};

use crate::config;
use crate::vault;
use crate::views::save_status;
use crate::views::{EditorEvent, Root};

use super::Shell;

/// The rail's four destinations, as a type `app.rs` can name.
///
/// [`Screen`] itself stays crate-private — it carries entities and half its
/// variants are not places you can navigate *to*. This is the subset the
/// keyboard and the rail actually address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultScreen {
    Cvs,
    Library,
    Diary,
    Applications,
}

pub(crate) enum Screen {
    /// Before the vault has been looked at.
    ///
    /// Exists so `Shell::new` can touch no filesystem: it runs *before*
    /// `open_window`, so anything it blocks on delays the first frame — and on
    /// macOS a vault under `~/Documents` means the OS raises its own consent
    /// dialog, which then appears with no application behind it (L-16). The
    /// backdrop only, for the frame or two it takes; showing `Welcome` here
    /// would flash the wrong screen at everyone who already has a vault.
    Opening,
    Welcome,
    Setup,
    Gallery,
    Library,
    Diary,
    Applications,
    Editor(Entity<Root>),
    /// Boxed: the matrix carries a whole `ResumeDoc`, and an inline variant
    /// would make every `Screen` that large.
    PresetMatrix(Box<crate::views::preset_matrix::PresetMatrix>),
}

impl Shell {
    /// Land the vault the startup task went looking for.
    ///
    /// The counterpart to [`Shell::new`] deferring it: `Some` means the
    /// remembered directory is still a vault and the gallery can open, `None`
    /// means it is gone, was never set, or could not be read — all of which
    /// lead to Welcome, because from here they are the same situation for the
    /// user even though the log tells them apart.
    pub(crate) fn finish_opening(&mut self, resolved: Option<(PathBuf, usize)>, cx: &mut Context<Self>) {
        match resolved {
            Some((dir, documents)) => {
                // The line that says which vault a report is about.
                log::info!("vault restored: {} ({documents} documents)", dir.display());
                self.vault = Some(dir);
                self.cache_stale = true;
                self.screen = Screen::Gallery;
                // Open into the CV the app was last in, not into a list of
                // one. The reading needs no second field: applying a preset
                // writes the document's active variants, so the file already
                // is the reading it was left in (C6). A path that has since
                // been deleted or renamed simply does not open, and the front
                // door is what the person sees — which is the right answer to
                // "the file I had open is gone".
                if let Some(last) = config::load().last_document {
                    if last.is_file() {
                        self.open_doc(last, cx);
                    }
                }
            }
            None => {
                log::info!("no usable vault recorded — starting at Welcome");
                self.screen = Screen::Welcome;
            }
        }
        cx.notify();
    }

    /// Adopt `vault_dir` as the active vault: remember it and show the gallery.
    pub(crate) fn open_vault(&mut self, vault_dir: PathBuf, cx: &mut Context<Self>) {
        config::set_vault(vault_dir.clone());
        let is_empty = vault::list_documents(&vault_dir).is_empty();
        // The first line of any useful report: which vault, and how much is in
        // it. A count, never a name — see `logging`'s content rule.
        log::info!(
            "vault opened: {} ({} documents)",
            vault_dir.display(),
            vault::list_documents(&vault_dir).len()
        );
        self.vault = Some(vault_dir);
        self.gallery_creating = is_empty;
        self.screen = Screen::Gallery;
        cx.notify();
    }

    /// Open a specific document in the editor, returning to the gallery when
    /// the editor asks to.
    ///
    /// The document is loaded **here**, before the editor entity exists, and a
    /// failure leaves the user where they are. `Root` used to load its own file
    /// and answer a parse failure by seeding the bundled AltaCV sample *and
    /// writing it to that path* — so a typo in a document the product
    /// advertises as hand-editable was replaced by sample data on one click,
    /// and the gallery happily routed a card marked "unreadable file" straight
    /// into it. A document that will not parse is a document to leave alone.
    /// Open a document, showing it at `preset` when the caller named one.
    ///
    /// The gallery's preset chips are the only caller that names one: the card
    /// itself opens the document as it was left, which is what a card has
    /// always done.
    pub(crate) fn open_doc_at(
        &mut self,
        doc_path: PathBuf,
        preset: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_doc(doc_path, cx);
        if let (Some(index), Screen::Editor(editor)) = (preset, &self.screen) {
            let editor = editor.clone();
            editor.update(cx, |root, cx| root.open_at_preset(index, window, cx));
        }
    }

    pub(crate) fn open_doc(&mut self, doc_path: PathBuf, cx: &mut Context<Self>) {
        let doc = match vault::load(&doc_path) {
            Ok(doc) => doc,
            Err(message) => {
                save_status::report_unreadable(cx, &doc_path, message);
                cx.notify();
                return;
            }
        };
        save_status::clear_open_failure(cx);

        let history = self.undo_histories.get(&doc_path).cloned();
        self.editing_path = Some(doc_path.clone());
        config::set_last_document(Some(doc_path.clone()));
        self.last_opened = Some(doc_path.clone());
        let editor = cx.new(move |cx| {
            let mut root = Root::new(doc_path, doc, cx);
            if let Some((undo, redo)) = history {
                root.undo_stack = undo;
                root.redo_stack = redo;
            }
            root
        });
        cx.subscribe(&editor, |this, editor, event, cx| match event {
            EditorEvent::BackToGallery => {
                // Flush before leaving, exactly as the matrix arm below does.
                // Saves are debounced by 600 ms on a `Task` the editor entity
                // owns, and switching `self.screen` drops that entity — so
                // without this the last keystrokes before clicking back were
                // simply cancelled. The sibling arm has always done this and
                // documented why; this one did not, which made "back" the one
                // exit that quietly lost work.
                this.flush_editor(&editor, cx);

                // Editing changes what the document's readings cost, so its
                // measurements go and the front door takes them again.
                if let Some(path) = this.editing_path.take() {
                    this.reading_pages.retain(|(p, _), _| *p != path);
                }
                this.screen = Screen::Gallery;
                cx.notify();
            }
            // P-01: the toolbar's preset menu is a door into the Preset
            // Matrix, scoped to the document currently open in the editor.
            EditorEvent::OpenPresetMatrix => {
                // The matrix re-reads the document from disk, so an unflushed
                // write would show it the state before the last keystrokes.
                this.flush_editor(&editor, cx);
                if let Some(path) = this.editing_path.clone() {
                    this.open_preset_matrix(path, cx);
                }
            }
        })
        .detach();
        self.screen = Screen::Editor(editor);
        cx.notify();
    }

    /// Switch to one of the rail's destinations, from anywhere.
    ///
    /// Routed through here rather than by assigning `self.screen` because
    /// leaving the editor has to flush first: writes are debounced on a task
    /// the editor entity owns, and dropping that entity cancels it. ⌘1–⌘4 must
    /// not be the one exit that loses the last keystrokes.
    pub fn go_to(&mut self, screen: VaultScreen, cx: &mut Context<Self>) {
        // Not from Welcome or Setup: there is no vault yet, and the rail those
        // chords belong to is not on screen.
        if matches!(
            self.screen,
            Screen::Opening | Screen::Welcome | Screen::Setup
        ) {
            return;
        }
        if let Screen::Editor(editor) = &self.screen {
            let editor = editor.clone();
            self.flush_editor(&editor, cx);
            if let Some(path) = self.editing_path.take() {
                self.reading_pages.retain(|(p, _), _| *p != path);
            }
        }
        self.screen = match screen {
            VaultScreen::Cvs => Screen::Gallery,
            VaultScreen::Library => Screen::Library,
            VaultScreen::Diary => Screen::Diary,
            VaultScreen::Applications => Screen::Applications,
        };
        cx.notify();
    }

    /// Write the editor's document out now, cancelling nothing and waiting for
    /// nothing. Every exit from the editor goes through here.
    ///
    /// Synchronous on purpose: the alternative is to await the pending task,
    /// and the pending task is a 600 ms timer we are trying to get *ahead* of.
    /// A CV is kilobytes of TOML — this is one small write, not a reason to
    /// build a handshake.
    pub(crate) fn flush_editor(&mut self, editor: &Entity<Root>, cx: &mut Context<Self>) {
        let (path, undo, redo) = {
            let root = editor.read(cx);
            (
                root.doc_path.clone(),
                root.undo_stack.clone(),
                root.redo_stack.clone(),
            )
        };
        self.undo_histories.insert(path, (undo, redo));
        // Two statements, not one: `record` needs `cx` mutably and `read` holds
        // it immutably, and `flush_save` returning an owned `Result` is what
        // lets the first borrow end before the second begins.
        let (path, seen, result) = {
            let editor = editor.read(cx);
            (
                editor.doc_path.clone(),
                editor.seen_on_disk(),
                editor.flush_save(),
            )
        };
        let now = save_status::record_document(cx, &path, seen, result);
        editor.update(cx, |editor, _| editor.on_disk = now);
    }

    /// Write out whatever is open, whoever is asking.
    ///
    /// The quit and window-close hooks call this: they know the app is going
    /// away, not which screen happens to be up. Both the editor and the Preset
    /// Matrix hold a document that a debounce may not have written yet.
    pub fn flush_open_document(&mut self, cx: &mut Context<Self>) {
        match &self.screen {
            Screen::Editor(editor) => {
                let editor = editor.clone();
                self.flush_editor(&editor, cx);
            }
            Screen::PresetMatrix(pm) => {
                let result = vault::save(&pm.doc, &pm.path, pm.on_disk);
                let (path, seen) = (pm.path.clone(), pm.on_disk);
                let now = save_status::record_document(cx, &path, seen, result);
                if let Screen::PresetMatrix(pm) = &mut self.screen {
                    pm.on_disk = now;
                }
            }
            // Every other screen writes synchronously as it edits; there is no
            // pending state to lose.
            _ => {}
        }
    }

    /// Leave the Preset Matrix, back the way the user came in: to the editor
    /// if a document is open behind it, otherwise to the gallery whose badge
    /// opened it. `editing_path` is `Some` exactly while the editor holds a
    /// document (`BackToGallery` takes it), which is what makes it a reliable
    /// answer here rather than a guess.
    pub(crate) fn leave_preset_matrix(&mut self, cx: &mut Context<Self>) {
        match self.editing_path.clone() {
            Some(path) => self.open_doc(path, cx),
            None => {
                self.screen = Screen::Gallery;
                cx.notify();
            }
        }
    }

    /// Open the Preset Matrix view for a document.
    pub(crate) fn open_preset_matrix(&mut self, doc_path: PathBuf, cx: &mut Context<Self>) {
        if let Ok(doc) = vault::load(&doc_path) {
            let pm = crate::views::preset_matrix::PresetMatrix::new(doc_path, doc);
            self.screen = Screen::PresetMatrix(Box::new(pm));
            // The evidence in the column headers, in the order it can be had:
            // the board is on disk and answers immediately, the page counts
            // need a compile each and arrive when they arrive.
            self.load_matrix_records();
            self.measure_matrix_pages(cx);
            cx.notify();
        }
    }
}
