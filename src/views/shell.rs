//! The top-level navigation shell.
//!
//! `Shell` owns which [`Screen`] is showing and routes to it. It is what the
//! window opens; the editor, setup, gallery, etc. are screens hosted inside it.
//! On first launch it shows the welcome → setup flow; once a vault is chosen it
//! remembers it (via `config`) and opens straight into the editor next time.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gpui::{AnyWindowHandle, Context, Entity, Subscription, Task};

use dockcv_ui_components::TextFieldState;

use crate::config;
use crate::resume::model::{ResumeDoc, SectionKind};
use crate::typst_engine::{PageGeometry, TypstEngine};
use crate::vault;

use super::applications::data::{ApplicationSort, ApplicationsView};
use super::applications::pin::PinPick;
use super::diary::capture::DiaryPaste;
use super::diary::use_in_cv::DiaryUse;
use super::gallery_sort::GallerySort;
use super::import::panels::ImportStep;
use super::library::edit::LibraryEdit;
use super::library::link::PushReview;
use super::library::LibrarySort;
use super::update_notice::UpdateState;
use super::vault_cache::VaultCache;

pub(crate) mod diary_actions;
pub(crate) mod gallery_actions;
pub(crate) mod git;
pub(crate) mod inputs;
pub(crate) mod leave;
pub(crate) mod matrix_actions;
pub(crate) mod navigation;
pub(crate) mod render;
pub(crate) mod watch;

#[cfg(test)]
pub(crate) use diary_actions::parse_tags;
pub(crate) use diary_actions::remove_at;
pub(crate) use gallery_actions::pick_dir;
pub(crate) use navigation::Screen;
pub use navigation::VaultScreen;

pub struct Shell {
    pub(super) screen: Screen,
    /// The batch export sheet (`preset_matrix/export.rs`): `Some` between
    /// choosing a folder and confirming the list of files it will receive.
    pub(super) batch_export: Option<super::preset_matrix::export::BatchExportSheet>,
    /// The active vault directory once chosen.
    pub(super) vault: Option<PathBuf>,
    /// Watches the vault for edits made outside DockCV, while the window is in
    /// front. See [`Shell::watch_vault`].
    pub(super) vault_watch: Option<Task<()>>,
    /// Starts and stops that watch as the window comes and goes. Registered on
    /// the first frame, because it needs a `Window`.
    pub(super) vault_watch_activation: Option<Subscription>,
    /// Which document the gallery is renaming inline, if any.
    pub(super) renaming_doc: Option<PathBuf>,
    /// The rename box. One field reused across cards — only one rename can be
    /// open at a time, and a field per card would be a field per document.
    pub(super) rename_field: Option<Entity<TextFieldState>>,
    /// Whether the gallery is showing the "new document" template chooser.
    pub(super) gallery_creating: bool,
    /// A version being renamed from its row on the front door.
    pub(super) renaming_version: Option<super::front_door::version::VersionRename>,
    /// Which note is open in each of the import rail's two lists.
    pub(super) import_notes: super::import::ImportNotes,
    /// A local assistant reading a scanned CV right now.
    pub(super) import_run: Option<super::assistant::LocalRun>,
    /// A local assistant tailoring a CV from its posting.
    pub(super) advice_run: Option<super::assistant::LocalRun>,
    /// A file picker is already on screen. See `import_existing_resume`.
    pub(super) import_picking: bool,
    /// A version being built, in memory and not on disk. See
    /// `front_door/draft.rs` — discarding it leaves the vault untouched.
    pub(super) drafting: Option<Box<super::front_door::draft::VersionDraft>>,
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
    pub(super) applications_period: super::applications::funnel::Period,
    /// The open detail panel, if any — the fields it is editing live with it,
    /// so closing the panel drops them.
    pub(super) applications_detail: Option<super::applications::detail::ApplicationDetail>,
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
    /// Whether the cache needs to be rechecked against disk (e.g. after external
    /// modification or window focus).
    pub(super) cache_stale: bool,
    /// The main window, set by `app.rs` once it exists. What the alert in front
    /// of leaving a CV in conflict is drawn on (`leave.rs`) — the exits that
    /// need it arrive as app actions, which bring no window of their own.
    pub(crate) window: Option<AnyWindowHandle>,
    /// That alert is up. A second exit while it is waiting (⌘Q twice) is
    /// dropped rather than stacking a second alert on the first.
    pub(super) asking_to_leave: bool,
}

impl Shell {
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
            import_notes: Default::default(),
            import_run: None,
            advice_run: None,
            import_picking: false,
            drafting: None,
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
            cache_stale: true,
            window: None,
            asking_to_leave: false,
        }
    }
}
