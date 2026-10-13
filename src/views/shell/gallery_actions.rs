use std::path::PathBuf;

use gpui::{Context, PathPromptOptions, Window};

use crate::config;
use crate::resume::model::ResumeDoc;
use crate::theme::{ActiveTheme, ThemeMode};
use crate::typst_engine::PageGeometry;
use crate::vault;
use crate::views::confirm;
use crate::views::import::panels::ImportStep;
use crate::views::save_status;

use super::navigation::Screen;
use super::Shell;

/// Options for a single-folder picker.
pub(crate) fn pick_dir() -> PathPromptOptions {
    PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: None,
    }
}

/// Extract the first chosen path from a picker result.
pub(crate) fn first_path<E>(result: Result<gpui::Result<Option<Vec<PathBuf>>>, E>) -> Option<PathBuf> {
    match result {
        Ok(Ok(Some(mut paths))) if !paths.is_empty() => Some(paths.remove(0)),
        _ => None,
    }
}

impl Shell {
    /// Switch palettes. The theme is a `Global`, so setting it repaints every
    /// screen at once — nothing has to be pushed into the open editor.
    pub(crate) fn set_theme(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        if cx.theme().mode == mode {
            return;
        }
        crate::theme::set_theme_mode(cx, mode);
        config::set_theme(mode);
        cx.notify();
    }

    /// Permanently remove everything in the vault's `.trash`.
    ///
    /// The one button in DockCV that destroys data outright — deleting a
    /// document only moves it here — so it is the one that asks first, and the
    /// dialog says how many files are about to go.
    pub(crate) fn empty_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let count = vault::trash_count(&vault);
        if count == 0 {
            return;
        }
        confirm::destructive(
            format!(
                "Permanently delete {count} deleted {}?",
                if count == 1 { "CV" } else { "CVs" }
            ),
            format!(
                "{} Everything in the vault's .trash folder is removed from disk. \
                 To keep any of it, move it out in Finder first.",
                confirm::CANNOT_UNDO
            ),
            "Empty Trash",
            window,
            cx,
            move |_this, _window, cx| {
                save_status::record(cx, "trash", vault::empty_trash(&vault));
                cx.notify();
            },
        );
    }

    /// Start a new CV — the gallery's template chooser, reached from ⌘N and
    /// the File menu as well as the gallery's own button.
    pub fn start_new_cv(&mut self, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        self.screen = Screen::Gallery;
        self.gallery_creating = true;
        cx.notify();
    }

    /// Show the vault folder in Finder.
    ///
    /// US-09: the user must always be able to see the real path *and* open it.
    /// Settings has shown the path since O-21; this is the other half, and it
    /// is the answer to "where are my files" that does not involve retyping a
    /// path into a Go-to-Folder box.
    pub fn reveal_vault(&mut self, cx: &mut Context<Self>) {
        if let Some(vault) = self.vault.clone() {
            cx.open_with_system(&vault);
        }
    }

    /// Forget every page measurement, so the front door re-measures.
    pub(crate) fn rebuild_thumbnails(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.reading_pages.clear();
        cx.notify();
    }

    pub(crate) fn duplicate_doc(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if vault::duplicate_document(&path).is_ok() {
            cx.notify();
        }
    }

    pub(crate) fn delete_doc(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if vault::delete_document(&path).is_ok() {
            self.reading_pages.retain(|(p, _), _| *p != path);
            cx.notify();
        }
    }

    /// Start renaming `path`, seeding the box with its current file name.
    pub(crate) fn start_rename(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if let Some(field) = self.rename_field.clone() {
            field.update(cx, |state, cx| state.seed(&stem, window, cx));
        }
        self.renaming_doc = Some(path);
        cx.notify();
    }

    pub(crate) fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.renaming_doc = None;
        cx.notify();
    }

    /// Apply the rename. A failure (name taken, empty) leaves the box open
    /// with the reason showing, rather than closing and losing what was typed.
    pub(crate) fn commit_rename(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let (Some(path), Some(field)) = (self.renaming_doc.clone(), self.rename_field.clone())
        else {
            return;
        };
        let name = field.read(cx).value(cx).trim().to_string();
        match vault::rename_document(&path, &name) {
            Ok(new_path) => {
                // Measurements are keyed by path, so they move across rather
                // than the renamed document's rows going briefly blank.
                let moved: Vec<((PathBuf, Option<String>), PageGeometry)> = self
                    .reading_pages
                    .keys()
                    .filter(|(p, _)| *p == path)
                    .cloned()
                    .filter_map(|key| {
                        let geometry = *self.reading_pages.get(&key)?;
                        Some(((new_path.clone(), key.1.clone()), geometry))
                    })
                    .collect();
                self.reading_pages.retain(|(p, _), _| *p != path);
                self.reading_pages.extend(moved);
                self.renaming_doc = None;
                self.setup_error = None;
            }
            Err(message) => self.setup_error = Some(message),
        }
        cx.notify();
    }

    /// Create a new document from a template and open it.
    pub(crate) fn create_doc(&mut self, doc: ResumeDoc, base: &str, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        match vault::create_document(&vault, &doc, base) {
            Ok(path) => {
                self.gallery_creating = false;
                self.open_doc(path, cx);
            }
            Err(message) => {
                self.setup_error = Some(message);
                cx.notify();
            }
        }
    }

    /// Prompt for a file (PDF, DOCX, JSON, TXT) and import it as a new CV.
    pub(crate) fn import_existing_resume(&mut self, cx: &mut Context<Self>) {
        // One panel at a time.
        //
        // The drop zone is clickable and so is the `Choose file` button inside
        // it, which is two targets for one intent — and a double-click on the
        // zone is two more. Each opened its own `NSOpenPanel`, stacked, so
        // choosing a file dismissed the top one and revealed the next: the
        // dialog looked like it would not close.
        if self.import_picking {
            return;
        }
        self.import_picking = true;
        self.setup_error = None;
        let prompt = PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Resume File (PDF, DOCX, JSON, TXT)".into()),
        };
        let receiver = cx.prompt_for_paths(prompt);
        let executor = cx.background_executor().clone();

        cx.spawn(async move |this, cx| {
            let picked = first_path(receiver.await);
            let _ = this.update(cx, |this, _cx| this.import_picking = false);
            let Some(file_path) = picked else {
                return;
            };
            let filename = file_path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "resume".to_string());
            let failed_name = filename.clone();
            let failed_path = file_path.clone();
            let _ = this.update(cx, |this, cx| {
                this.import_step = ImportStep::Parsing { filename };
                cx.notify();
            });
            let result = executor
                .spawn(async move { crate::import::import_file(&file_path) })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(imported) => {
                    this.import_step = ImportStep::Step2Review {
                        imported: Box::new(imported),
                    };
                    cx.notify();
                }
                // A step, not a string dropped on the drop zone: the drop zone
                // never rendered `setup_error`, so a failed import bounced back
                // to the start with nothing said at all.
                Err(error) => {
                    this.import_step = ImportStep::CouldNotRead {
                        filename: failed_name,
                        path: Some(failed_path),
                        error: Box::new(error),
                    };
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Create a new `cvault` inside a user-chosen folder.
    pub(crate) fn create_new_vault(&mut self, cx: &mut Context<Self>) {
        self.setup_error = None;
        let receiver = cx.prompt_for_paths(pick_dir());
        cx.spawn(async move |this, cx| {
            let Some(parent) = first_path(receiver.await) else {
                return;
            };
            let _ = this.update(cx, |this, cx| match vault::create_vault(&parent) {
                Ok(dir) => this.open_vault(dir, cx),
                Err(message) => {
                    this.setup_error = Some(message);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Open an existing vault folder.
    pub(crate) fn open_existing_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.setup_error = None;
        let receiver = cx.prompt_for_paths(pick_dir());
        cx.spawn_in(window, async move |this, cx| {
            let Some(dir) = first_path(receiver.await) else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                if !vault::is_vault(&dir) {
                    this.setup_error = Some("That is not a folder.".into());
                    cx.notify();
                    return;
                }
                // `is_vault` only asks whether it is a directory, which is why
                // picking `~/` used to be accepted in silence — after which the
                // gallery parsed every unrelated `.toml` on the machine and drew
                // the failures as cards reading "unreadable file".
                match vault::vault_shape(&dir) {
                    vault::VaultShape::Unrecognized { stray_toml } => {
                        let detail = if stray_toml == 0 {
                            "Nothing in it looks like a CV, a block library or a diary.".to_string()
                        } else {
                            format!(
                                "It holds {stray_toml} .toml file{} and none of them is a CV. \
                                 DockCV would list every one of them as an unreadable document.",
                                if stray_toml == 1 { "" } else { "s" }
                            )
                        };
                        // Not refused outright: it is the user's disk, and a
                        // vault restored from a backup can look like anything.
                        // But it is asked about, and the answer is No by
                        // default.
                        confirm::destructive(
                            "That folder doesn't look like a vault.".into(),
                            detail,
                            "Use It Anyway",
                            window,
                            cx,
                            move |this, _window, cx| this.adopt_vault(dir, cx),
                        );
                    }
                    _ => this.adopt_vault(dir, cx),
                }
            });
        })
        .detach();
    }

    /// Take a folder as the vault, and leave a marker saying so.
    ///
    /// The marker is what lets the folder answer for itself next time instead
    /// of being re-classified by its contents — and what makes an intentionally
    /// odd vault stop asking after the first time.
    pub(crate) fn adopt_vault(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        vault::mark_as_vault(&dir);
        self.open_vault(dir, cx);
    }
}
