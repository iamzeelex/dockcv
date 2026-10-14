use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gpui::{prelude::*, App, Context, Window};

use dockcv_ui_components::{TextFieldEvent, TextFieldState};

use crate::resume::template;
use crate::typst_engine::{PageGeometry, TypstEngine};
use crate::vault;

use super::navigation::Screen;
use super::Shell;

impl Shell {
    /// Build the screens' text boxes on the first frame and keep their changes
    /// flowing back into the shell.
    pub(crate) fn ensure_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(crate) fn library_hits(&self, query: &str) -> usize {
        if query.is_empty() {
            return 0;
        }
        let library = self.cache.library();
        crate::views::library::POOLS
            .iter()
            .flat_map(|&(section, _)| crate::views::library::cards_for(library, section))
            .filter(|card| card.haystack().contains(query))
            .count()
    }

    /// Open the Library with `query` already in its box.
    ///
    /// Carrying the query across is the whole gesture: the gallery said blocks
    /// match, and arriving at an unfiltered Library would make the user type it
    /// again to see the thing they were just told about.
    pub(crate) fn open_library_with(
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

    pub(crate) fn search_query(&self, cx: &App) -> String {
        self.search
            .as_ref()
            .map(|f| f.read(cx).value(cx).trim().to_lowercase())
            .unwrap_or_default()
    }

    /// The library's current search query, lowercased and trimmed.
    pub(crate) fn library_query(&self, cx: &App) -> String {
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
    pub(crate) fn ensure_reading_pages(&mut self, cx: &mut Context<Self>) {
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
                            let mut engine = engine.lock().unwrap_or_else(|e| e.into_inner());
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
}
