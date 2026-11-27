//! The version constructor: what a draft is, and the four things you can do to it.
//!
//! Between "there is a job" and "there is a version for it" the product used to
//! put the Preset Matrix — a section × variant grid. That is the right surface
//! for auditing a document with five readings in it and the wrong one for
//! making the first: it asks which *variant* of each section, when the question
//! in the person's head is which *changes* to make.
//!
//! So this screen asks that instead. The answers are not generated: a change is
//! a cut of a section that already exists, under the name and the one-line
//! description its author gave it (`front_door/changes.rs`). The matrix is still
//! there, one click away, for when three named changes are not enough.
//!
//! **Nothing is written until `Save version`.** The draft is a `ResumeDoc` in
//! memory with the source preset applied and the toggles on top; discarding it
//! leaves the vault exactly as it was. That is a change from the flow this
//! replaces, which created the preset and an application card the moment you
//! typed a company name — and would have left a card pinned to a preset that
//! no longer existed the first time somebody changed their mind.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{Context, Task};

use crate::render::Rendered;
use crate::resume::model::{Application, ResumeDoc, SectionKind, SentCv, VariantId};
use crate::resume::posting::{
    coverage, deciding_terms, gaps, unused, Coverage, Unused, UnusedSource,
};
use crate::typst_engine::PageGeometry;
use crate::vault;

use super::changes::{self, Change, ChangeKind};
use crate::views::save_status;
use crate::views::shell::Shell;

/// A version being built, and nothing on disk yet.
pub(crate) struct VersionDraft {
    pub path: PathBuf,
    /// The exact file version this in-memory copy was based on. Save must use
    /// this marker so an assistant's filesystem edit cannot be overwritten by
    /// a stale draft.
    pub on_disk: vault::OnDisk,
    /// The document as this version would read: its chosen source reading
    /// applied, then whatever has been toggled since.
    pub doc: ResumeDoc,
    pub company: String,
    pub role: String,
    /// The posting's text, on its way to the application card.
    ///
    /// Carried rather than re-asked: the sheet is where it was pasted, the card
    /// is where it belongs, and the draft is the only thing that exists between
    /// the two. Empty when the job was named without one.
    pub posting: String,
    /// The baseline preset index in `doc`.
    pub source: usize,
    /// The source was the document as saved, so this in-memory preset exists
    /// only to describe and toggle changes relative to that starting point.
    /// It is removed again before a version is written.
    pub document_source: Option<usize>,
    /// The compiled page, once there is one. `None` is "not yet", never "no
    /// page" — a blank sheet would be a claim about the document.
    pub page: Option<Rendered>,
    pub geometry: Option<PageGeometry>,
    pub compiling: bool,
    /// The visible preview was successfully compiled from the current draft.
    /// Save is disabled while a new layout is pending or after a failed build.
    pub preview_current: bool,
    pub preview_error: Option<String>,
    pub task: Option<Task<()>>,
    /// The posting read against the page as it currently stands.
    ///
    /// `None` when the job was named without a posting. Recomputed on every
    /// toggle, which is the point: the number says what *this* page answers,
    /// so leaving a section out moves it, and the answer arrives while the
    /// decision is being made rather than one screen earlier.
    pub read: Option<DraftRead>,
    /// What an assistant picked from the menu, resolved to stable document and
    /// block identities so a refreshed list cannot point at the wrong row.
    pub advice_changes: Vec<(SectionKind, super::changes::ChangeKind, String)>,
    /// Source identity plus the exact text the assistant saw; an external
    /// library edit must not move an old recommendation onto a different row.
    pub advice_components: Vec<(UnusedSource, String, String)>,
    /// Candidate ids in the last prompt, held until its answer is pasted.
    pub pending_advice_changes: Vec<(SectionKind, super::changes::ChangeKind)>,
    pub pending_advice_components: Vec<(UnusedSource, String)>,
    pub advice_feedback: Option<String>,
    pub advice_running: bool,
    /// Library blocks added to this draft, kept distinct so they can be
    /// reviewed and so repeat clicks do not duplicate them.
    pub added_components: Vec<(UnusedSource, String)>,
    /// One working variant per section; several selected blocks in the same
    /// section are grouped into a single job-specific cut.
    pub component_variants: HashMap<SectionKind, VariantId>,
    /// The file changed since this draft was opened. Keep the unsaved draft on
    /// screen and offer a deliberate reload rather than replacing it silently.
    pub external_change_pending: bool,
}

/// What the posting asks of the page in front of you.
pub(crate) struct DraftRead {
    pub terms: usize,
    pub coverage: Coverage,
    /// Entries in the vault that answer what this page does not say.
    pub unused: Vec<Unused>,
}

/// How large the preview page is rasterized. Fixed rather than
/// resolution-matched: this is a reading of the shape of a page, not a proof
/// sheet, and the editor is where somebody goes to read the words.
const PREVIEW_SCALE: f32 = 2.0;

impl VersionDraft {
    /// The changes available against the source, with the working copy's
    /// answers already marked.
    pub(crate) fn changes(&self) -> Vec<Change> {
        changes::available(&self.doc, self.source)
    }

    /// What the source preset is called.
    pub(crate) fn source_name(&self) -> String {
        self.doc
            .presets
            .get(self.source)
            .map(|p| p.name.clone())
            .unwrap_or_default()
    }

    /// The file stem, which is how the applications board keys a send.
    pub(crate) fn stem(&self) -> String {
        self.path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

impl Shell {
    /// Start building a version of `path` from a saved preset, or from the
    /// document as it is saved when there is no preset to choose.
    pub(crate) fn open_version_draft(
        &mut self,
        path: PathBuf,
        source: Option<usize>,
        company: String,
        role: String,
        posting: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let (mut doc, on_disk) = match vault::load_seen(&path) {
            Ok(loaded) => loaded,
            Err(_) => {
                save_status::record(
                    cx,
                    "this CV",
                    Err(format!("{} could not be read", path.display())),
                );
                cx.notify();
                return false;
            }
        };
        let (source, document_source) = match source {
            Some(source) if doc.presets.get(source).is_some() => {
                // Stand the working copy in the source's reading, so every
                // change offered is a change from the thing it names.
                doc.apply_preset(source);
                (source, None)
            }
            Some(_) => {
                save_status::record(
                    cx,
                    "this CV",
                    Err("The selected starting version is no longer available.".to_string()),
                );
                cx.notify();
                return false;
            }
            None => {
                // `available` compares changes with a preset. Snapshot the
                // file's current reading in memory, then discard this helper
                // preset when saving the finished version.
                let name = crate::views::tailor::unique_preset_name(&doc, "Current CV");
                doc.add_preset(name);
                let source = doc.presets.len() - 1;
                (source, Some(source))
            }
        };

        self.drafting = Some(Box::new(VersionDraft {
            path,
            on_disk,
            doc,
            company,
            role,
            posting,
            source,
            document_source,
            page: None,
            geometry: None,
            compiling: false,
            preview_current: false,
            preview_error: None,
            task: None,
            read: None,
            advice_changes: Vec::new(),
            advice_components: Vec::new(),
            pending_advice_changes: Vec::new(),
            pending_advice_components: Vec::new(),
            advice_feedback: None,
            advice_running: false,
            added_components: Vec::new(),
            component_variants: HashMap::new(),
            external_change_pending: false,
        }));
        self.refresh_draft_read();
        self.compile_draft(cx);
        cx.notify();
        true
    }

    /// The page, the diary and the library together — what the vault knows.
    fn draft_vault_corpus(&self, page: &str) -> String {
        let mut corpus = page.to_string();
        for entry in &self.cache.diary().entries {
            corpus.push(' ');
            corpus.push_str(&entry.text);
        }
        let library = self.cache.library();
        for block in &library.work {
            corpus.push_str(&format!(
                " {} {}",
                block.position,
                block.highlights.join(" ")
            ));
        }
        for block in &library.skills {
            corpus.push_str(&format!(" {} {}", block.name, block.keywords.join(" ")));
        }
        for block in &library.education {
            corpus.push_str(&format!(
                " {} {} {}",
                block.institution,
                block.study_type,
                block.highlights.join(" ")
            ));
        }
        for block in &library.certificates {
            corpus.push_str(&format!(" {} {}", block.name, block.issuer));
        }
        for block in &library.volunteer {
            corpus.push_str(&format!(
                " {} {} {}",
                block.position,
                block.organization,
                block.highlights.join(" ")
            ));
        }
        corpus
    }

    /// Read the draft's posting against the page the draft currently makes.
    ///
    /// Pure arithmetic over text the app already produces, same as the sheet's
    /// read — and deliberately recomputed rather than carried across, because
    /// the page changes under it. A coverage number copied from the previous
    /// screen would be a claim about a document that no longer exists.
    pub(crate) fn refresh_draft_read(&mut self) {
        let Some(draft) = self.drafting.as_mut() else {
            return;
        };
        let posting = draft.posting.clone();
        if posting.trim().is_empty() {
            draft.read = None;
            return;
        }
        let page = crate::resume::export_plain_text(&draft.doc.compose());

        // One version here, so `deciding_terms` reads as "how much of the
        // posting's own vocabulary this page already shares" — which is the
        // question on this screen, where there is nothing to choose between.
        let terms = deciding_terms(&posting, std::slice::from_ref(&page));
        let read = coverage(&terms, &page);
        let vault = self.draft_vault_corpus(&page);
        let missing = gaps(&posting, &page, &vault);
        let found = unused(
            &missing,
            &self.cache.diary().entries,
            self.cache.library(),
            &page,
        );

        // Reborrowed: `self.cache` and `self.drafting` cannot be held at once.
        if let Some(draft) = self.drafting.as_mut() {
            draft.read = Some(DraftRead {
                terms: terms.len(),
                coverage: read,
                unused: found,
            });
        }
    }

    /// The prompt for this draft: the posting, the menu, and the rules.
    pub(crate) fn draft_advice_prompt(&self) -> Option<String> {
        let draft = self.drafting.as_ref()?;
        if draft.posting.trim().is_empty() {
            return None;
        }
        let unused = draft
            .read
            .as_ref()
            .map(|r| r.unused.clone())
            .unwrap_or_default();
        let changes = draft.changes();
        let alternatives = changes
            .iter()
            .enumerate()
            .filter_map(|(index, change)| {
                let ChangeKind::Variant(id) = change.kind else {
                    return None;
                };
                let mut candidate = draft.doc.clone();
                if !candidate.set_active_variant_by_id(change.section, id) {
                    return None;
                }
                candidate
                    .hidden_sections
                    .retain(|hidden| *hidden != change.section);
                Some((index, assistant_section_text(&candidate, change.section)))
            })
            .collect::<Vec<_>>();
        Some(super::advice::advice_prompt(
            &draft.posting,
            &assistant_cv_text(&draft.doc),
            &changes,
            &unused,
            &alternatives,
        ))
    }

    /// Instructions for an assistant that can access the local file system.
    /// Only local routes get this prompt; browser routes receive a list of
    /// choices and return IDs for the user to review.
    pub(crate) fn draft_file_advice_prompt(&self) -> Option<String> {
        let draft = self.drafting.as_ref()?;
        if draft.posting.trim().is_empty() {
            return None;
        }
        let source = if draft.document_source == Some(draft.source) {
            "the current saved reading".to_string()
        } else {
            draft.source_name()
        };
        Some(super::advice::file_advice_prompt(
            &draft.posting,
            &draft.path,
            &draft.company,
            &draft.role,
            &source,
        ))
    }

    /// Put the prompt where the person can use it whatever the route did.
    ///
    /// Every time, not only when the link cannot carry it: none of these links
    /// can report back, and somebody looking at an empty composer should
    /// already have the prompt in their hand.
    pub(crate) fn stage_draft_advice(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.draft_advice_prompt() else {
            return;
        };
        self.stage_draft_prompt(prompt, true, cx);
    }

    /// Stage a file-edit prompt for a local assistant. The route will get the
    /// document's folder; the file remains unchanged until that assistant is
    /// explicitly opened and allowed to make the edit.
    pub(crate) fn stage_draft_file_advice(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.draft_file_advice_prompt() else {
            return;
        };
        self.stage_draft_prompt(prompt, false, cx);
    }

    fn stage_draft_prompt(&mut self, prompt: String, expects_picks: bool, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(prompt));
        if let Some(draft) = self.drafting.as_mut() {
            draft.advice_feedback = Some(if expects_picks {
                "Prompt copied. Paste the assistant's JSON answer here to review its picks.".into()
            } else {
                "Instructions copied. DockCV will flag a change to this CV file when you return."
                    .into()
            });
            if expects_picks {
                let changes = draft.changes();
                draft.pending_advice_changes = changes
                    .iter()
                    .map(|change| (change.section, change.kind))
                    .collect();
                draft.pending_advice_components = draft
                    .read
                    .as_ref()
                    .map(|read| {
                        super::advice::component_candidates(&read.unused)
                            .into_iter()
                            .map(|(_, item)| {
                                (item.source.clone(), item.text.clone().unwrap_or_default())
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            } else {
                draft.pending_advice_changes.clear();
                draft.pending_advice_components.clear();
            }
        }
        cx.notify();
    }

    /// Read the answer off the clipboard and mark what it picked.
    ///
    /// **Marks, does not apply.** The rows it names get a line of the
    /// assistant's reasoning beside them and stay unticked; the person is
    /// still the one who decides what the CV says.
    pub(crate) fn paste_draft_advice(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            if let Some(draft) = self.drafting.as_mut() {
                draft.advice_feedback = Some("The clipboard is empty. Copy the prompt, ask your assistant, then copy its answer.".into());
            }
            cx.notify();
            return;
        };
        let (changes, notes) = match self.drafting.as_ref() {
            Some(draft) => (
                draft.pending_advice_changes.len(),
                draft.pending_advice_components.len(),
            ),
            None => return,
        };
        self.accept_draft_advice(&text, changes, notes, cx);
    }

    /// Run a local assistant against this CV file. This route is explicit on
    /// screen and gets only the CV path plus the job prompt; the assistant may
    /// add variants and a preset, while DockCV's watcher reports and reloads
    /// that on-disk change.
    pub(crate) fn run_local_draft_assistant(
        &mut self,
        program: &'static str,
        argv: &'static [&'static str],
        cx: &mut Context<Self>,
    ) {
        let (prompt, path) = match (
            self.draft_file_advice_prompt(),
            self.drafting.as_ref().map(|draft| draft.path.clone()),
        ) {
            (Some(prompt), Some(path)) => (prompt, path),
            _ => return,
        };
        let command = format!(
            "{program} {} <job-specific CV instructions>",
            argv.join(" ")
        );
        let cwd = path.parent().map(PathBuf::from);
        let executor = cx.background_executor().clone();
        self.stage_draft_prompt(prompt.clone(), false, cx);
        if let Some(draft) = self.drafting.as_mut() {
            draft.advice_running = true;
            draft.advice_feedback = Some(format!("Running {program} with access to this CV…"));
        }

        let task = cx.spawn(async move |this, cx| {
            let outcome = executor
                .spawn({
                    let executor = executor.clone();
                    async move {
                        let mut child = std::process::Command::new(program);
                        child.args(argv).arg(&prompt);
                        if let Some(cwd) = cwd {
                            child.current_dir(cwd);
                        }
                        let mut child = child
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .spawn()
                            .map_err(|e| format!("{program} would not start: {e}"))?;

                        let deadline = Instant::now() + super::super::assistant::LOCAL_TIMEOUT;
                        loop {
                            match child.try_wait() {
                                Ok(Some(_)) => break,
                                Ok(None) if Instant::now() < deadline => {
                                    executor.timer(Duration::from_millis(250)).await;
                                }
                                Ok(None) => {
                                    let _ = child.kill();
                                    return Err(format!("{program} was still running after four minutes and was stopped."));
                                }
                                Err(e) => return Err(format!("{program} failed: {e}")),
                            }
                        }
                        let out = child
                            .wait_with_output()
                            .map_err(|e| format!("{program} failed: {e}"))?;
                        if !out.status.success() {
                            let why = String::from_utf8_lossy(&out.stderr).trim().chars().take(400).collect::<String>();
                            return Err(if why.is_empty() {
                                format!("{program} exited without a result")
                            } else {
                                why
                            });
                        }
                        Ok(String::from_utf8_lossy(&out.stdout).trim().chars().take(500).collect::<String>())
                    }
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.advice_run = None;
                let Some(draft) = this.drafting.as_mut() else {
                    return;
                };
                draft.advice_running = false;
                match outcome {
                    Ok(summary) => {
                        draft.advice_feedback = Some(if summary.is_empty() {
                            "Assistant finished. If it changed the CV file, DockCV will show a reload option.".into()
                        } else {
                            format!("Assistant finished: {summary}")
                        });
                        this.vault_changed_on_disk(cx);
                    }
                    Err(error) => {
                        draft.advice_feedback = Some(format!("{program} could not finish: {error}"));
                    }
                }
                cx.notify();
            });
        });
        self.advice_run = Some(super::super::assistant::LocalRun {
            tool: program,
            command,
            _task: task,
        });
        cx.notify();
    }

    /// Validate a returned JSON selection before any suggestion reaches the
    /// draft. Unknown IDs are ignored with a visible note; malformed answers
    /// stay out of the document and explain how to retry.
    fn accept_draft_advice(
        &mut self,
        answer: &str,
        change_count: usize,
        component_count: usize,
        cx: &mut Context<Self>,
    ) {
        match super::advice::parse_advice_checked(answer, change_count, component_count) {
            Err(message) => {
                if let Some(draft) = self.drafting.as_mut() {
                    draft.advice_feedback = Some(message);
                }
            }
            Ok((advice, ignored)) => {
                if let Some(draft) = self.drafting.as_mut() {
                    draft.advice_changes = advice
                        .changes
                        .iter()
                        .filter_map(|(index, why)| {
                            let (section, kind) = draft.pending_advice_changes.get(*index)?;
                            Some((*section, *kind, why.clone()))
                        })
                        .collect();
                    let mut stale_components = 0;
                    draft.advice_components = advice
                        .notes
                        .iter()
                        .filter_map(|(index, why)| {
                            let (source, snapshot) = draft.pending_advice_components.get(*index)?;
                            let still_current = draft
                                .read
                                .as_ref()
                                .and_then(|read| {
                                    read.unused.iter().find(|item| item.source == *source)
                                })
                                .is_some_and(|item| {
                                    item.text.as_deref() == Some(snapshot.as_str())
                                });
                            if !still_current {
                                stale_components += 1;
                                return None;
                            }
                            Some((source.clone(), snapshot.clone(), why.clone()))
                        })
                        .collect();
                    let count = draft.advice_changes.len() + draft.advice_components.len();
                    let ignored = ignored + stale_components;
                    draft.advice_feedback = Some(if count == 0 {
                        if ignored > 0 {
                            "Those recommendations no longer match the current draft. Copy a fresh prompt before asking again.".into()
                        } else {
                            "The assistant found no existing change or saved block to recommend. You can still choose manually.".into()
                        }
                    } else if ignored > 0 {
                        format!("{} suggestion{} matched this draft; {ignored} unknown item{} ignored. Review each suggestion before adding it.", count, if count == 1 { "" } else { "s" }, if ignored == 1 { " was" } else { "s were" })
                    } else {
                        format!(
                            "{} suggestion{} marked below. Review each one before adding it.",
                            count,
                            if count == 1 { " is" } else { "s are" }
                        )
                    });
                    draft.pending_advice_changes.clear();
                    draft.pending_advice_components.clear();
                }
            }
        }
        cx.notify();
    }

    /// Add a saved Library block to this in-memory version, as an exact copy.
    /// The source preset continues to pin its original section variant; a new
    /// variant carries the added block, so the preview makes the change visible
    /// and the original CV stays intact until Save version.
    pub(crate) fn add_draft_component(&mut self, source: UnusedSource, cx: &mut Context<Self>) {
        let UnusedSource::Library { section, index } = source.clone() else {
            return;
        };
        let library = self.cache.library().clone();
        let block_text = self
            .drafting
            .as_ref()
            .and_then(|draft| draft.read.as_ref())
            .and_then(|read| read.unused.iter().find(|item| item.source == source))
            .and_then(|item| item.text.clone());
        let Some(block_text) = block_text else {
            if let Some(draft) = self.drafting.as_mut() {
                draft.advice_feedback = Some("That saved block is no longer a match for this posting. DockCV has refreshed the list; choose from the current results.".into());
            }
            cx.notify();
            return;
        };
        let found = match section {
            SectionKind::Work => library.work.get(index).is_some(),
            SectionKind::Education => library.education.get(index).is_some(),
            SectionKind::Skills => library.skills.get(index).is_some(),
            SectionKind::Certificates => library.certificates.get(index).is_some(),
            SectionKind::Organizations => library.volunteer.get(index).is_some(),
            SectionKind::Profile | SectionKind::Custom(_) => false,
        };
        if !found {
            if let Some(draft) = self.drafting.as_mut() {
                draft.advice_feedback = Some("That saved block changed on disk. DockCV has refreshed the list; choose the block again.".into());
            }
            cx.notify();
            return;
        }
        let Some(draft) = self.drafting.as_mut() else {
            return;
        };
        if draft
            .added_components
            .contains(&(source.clone(), block_text.clone()))
        {
            return;
        }

        if !draft.component_variants.contains_key(&section) {
            let base = format!("{} selection", draft.company);
            let label = unique_variant_name(&draft.doc, section, &base);
            draft.doc.add_variant(section);
            if let Some(name) = draft.doc.variant_name_mut(section) {
                *name = label;
            }
            if let Some(id) = draft.doc.active_variant_id(section) {
                draft.component_variants.insert(section, id);
            }
        } else if let Some(id) = draft.component_variants.get(&section).copied() {
            // The person may have toggled this variant off since the previous
            // block was added. Select the job-specific cut before appending so
            // the source version can never be mutated by a second add.
            draft.doc.set_active_variant_by_id(section, id);
        }
        draft
            .doc
            .hidden_sections
            .retain(|hidden| *hidden != section);
        match section {
            SectionKind::Work => draft
                .doc
                .work
                .active_mut()
                .push(library.work[index].clone()),
            SectionKind::Education => draft
                .doc
                .education
                .active_mut()
                .push(library.education[index].clone()),
            SectionKind::Skills => draft
                .doc
                .skills
                .active_mut()
                .push(library.skills[index].clone()),
            SectionKind::Certificates => draft
                .doc
                .certificates
                .active_mut()
                .push(library.certificates[index].clone()),
            SectionKind::Organizations => draft
                .doc
                .volunteer
                .active_mut()
                .push(library.volunteer[index].clone()),
            SectionKind::Profile | SectionKind::Custom(_) => return,
        }
        draft.added_components.push((source, block_text));
        draft.advice_changes.clear();
        draft.advice_feedback = Some("Added to the draft as written. Check the preview and the new section choice before saving.".into());
        self.refresh_draft_read();
        self.compile_draft(cx);
        cx.notify();
    }

    /// Refresh this draft from a file changed outside DockCV, then rebuild its
    /// baseline against the same named source preset where possible.
    pub(crate) fn reload_draft_from_disk(&mut self, cx: &mut Context<Self>) {
        let Some(old) = self.drafting.as_ref() else {
            return;
        };
        let path = old.path.clone();
        let old_source_name = old.source_name();
        let company = old.company.clone();
        let role = old.role.clone();
        let posting = old.posting.clone();
        let (mut doc, on_disk) = match vault::load_seen(&path) {
            Ok(loaded) => loaded,
            Err(message) => {
                if let Some(draft) = self.drafting.as_mut() {
                    draft.advice_feedback = Some(format!(
                        "The file is still being edited or is not valid TOML: {message}"
                    ));
                }
                cx.notify();
                return;
            }
        };
        let (source, document_source) =
            if let Some(index) = doc.presets.iter().position(|p| p.name == old_source_name) {
                doc.apply_preset(index);
                (index, None)
            } else {
                let name = crate::views::tailor::unique_preset_name(&doc, "Current CV");
                doc.add_preset(name);
                (doc.presets.len() - 1, Some(doc.presets.len() - 1))
            };
        self.drafting = Some(Box::new(VersionDraft {
            path,
            on_disk,
            doc,
            company,
            role,
            posting,
            source,
            document_source,
            page: None,
            geometry: None,
            compiling: false,
            preview_current: false,
            preview_error: None,
            task: None,
            read: None,
            advice_changes: Vec::new(),
            advice_components: Vec::new(),
            pending_advice_changes: Vec::new(),
            pending_advice_components: Vec::new(),
            advice_feedback: Some(
                "Reloaded from disk. Choose the new version or saved blocks you want to use."
                    .into(),
            ),
            advice_running: false,
            added_components: Vec::new(),
            component_variants: HashMap::new(),
            external_change_pending: false,
        }));
        self.refresh_draft_read();
        self.compile_draft(cx);
        cx.notify();
    }

    /// Throw the draft away. Nothing was written, so nothing is undone.
    pub(crate) fn discard_version_draft(&mut self, cx: &mut Context<Self>) {
        self.drafting = None;
        self.advice_run = None;
        cx.notify();
    }

    /// Turn one change on or off.
    ///
    /// Off does not mean "the document's default" — it means *what the source
    /// reads*, which is the only answer that makes the count honest. A change
    /// you switched off is not a change.
    pub(crate) fn toggle_draft_change(&mut self, change: &Change, cx: &mut Context<Self>) {
        let Some(draft) = self.drafting.as_mut() else {
            return;
        };
        let Some(source) = draft.doc.presets.get(draft.source).cloned() else {
            return;
        };
        changes::toggle(&mut draft.doc, &source, change);
        // The page just changed, so what it answers just changed with it.
        self.refresh_draft_read();
        self.compile_draft(cx);
        cx.notify();
    }

    /// Start from a different version instead.
    ///
    /// Every toggle is dropped, and deliberately: a change is defined against
    /// the source, so carrying them over would carry names that mean something
    /// else now.
    pub(crate) fn set_draft_source(&mut self, source: usize, cx: &mut Context<Self>) {
        let Some(draft) = self.drafting.as_mut() else {
            return;
        };
        if draft.source == source || draft.doc.presets.get(source).is_none() {
            return;
        }
        draft.source = source;
        draft.doc.apply_preset(source);
        draft.advice_changes.clear();
        draft.advice_components.clear();
        draft.pending_advice_changes.clear();
        draft.pending_advice_components.clear();
        draft.added_components.clear();
        draft.component_variants.clear();
        draft.advice_feedback = None;
        self.refresh_draft_read();
        self.compile_draft(cx);
        cx.notify();
    }

    /// Write the version, file the application, and go back to the list.
    ///
    /// The preset is captured by [`ResumeDoc::add_preset`] rather than
    /// assembled here: it records the whole reading — selection, hidden, order,
    /// headings, language, profile — and a call site that lists the fields it
    /// knows about is how a new one gets silently dropped.
    pub(crate) fn save_version_draft(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.drafting.as_ref() else {
            return;
        };
        if !draft.preview_current
            || draft.compiling
            || draft.advice_running
            || draft.external_change_pending
        {
            return;
        }
        let Some(vault_dir) = self.vault.clone() else {
            return;
        };
        let stem = draft.stem();
        let path = draft.path.clone();
        let baseline = draft.on_disk;
        let mut doc = draft.doc.clone();
        let based_on = (draft.document_source != Some(draft.source))
            .then(|| doc.presets.get(draft.source).map(|p| p.name.clone()))
            .flatten();
        let company = draft.company.clone();
        let role = draft.role.clone();
        let posting = draft.posting.clone();

        if let Some(index) = draft.document_source {
            if index < doc.presets.len() {
                doc.presets.remove(index);
            }
        }

        let name = crate::views::tailor::unique_preset_name(&doc, &draft.company);
        doc.add_preset(name.clone());
        if let Some(preset) = doc.presets.last_mut() {
            preset.based_on = based_on;
        }

        // Save against the version this draft actually read. Reading the
        // latest hash here would let an assistant's new variants be silently
        // replaced by this older in-memory copy.
        let result = vault::save(&doc, &path, baseline);
        let failed = result.is_err();
        // Always recorded, not only on failure: the banner is one mechanism for
        // every vault write, and the revision it bumps is what makes the cache
        // — and so the list this returns to — show the new version.
        save_status::record_document(cx, &path, baseline, result);
        if failed {
            if let Some(draft) = self.drafting.as_mut() {
                draft.external_change_pending = vault::OnDisk::read(&draft.path) != draft.on_disk;
                draft.advice_feedback = Some(if draft.external_change_pending {
                    "The CV file changed while this draft was open. Reload it to include the assistant's updates, then save again.".into()
                } else {
                    "DockCV could not save this version. Your draft is still here; check the save message and try again.".into()
                });
            }
            cx.notify();
            return;
        }

        let _ = self.drafting.take();

        // The card arrives with the version rather than before it. It goes to
        // the wishlist, so nothing counts as sent until it moves — which is
        // what `furthest` is for.
        let (mut applications, seen) = vault::load_applications_seen(&vault_dir);
        applications.entries.push(Application {
            company,
            role,
            posting,
            created: vault::today_iso(),
            sent_as: Some(SentCv {
                document: stem,
                preset: name,
            }),
            ..Default::default()
        });
        let now = save_status::record_notebook(
            cx,
            "applications board",
            &vault::applications_path(&vault_dir),
            seen,
            vault::save_applications_seen(&vault_dir, &applications, seen),
        );
        self.follow_board_write(seen, now);

        self.reading_pages.retain(|(p, _), _| *p != path);
        self.load_matrix_records();
        cx.notify();
    }

    /// Save it, then open the matrix on it.
    ///
    /// Named for both halves. The matrix reads a document from disk, so there
    /// is no version of this that adjusts a draft — and silently writing one
    /// behind a button called `Adjust sections` is the kind of surprise this
    /// screen exists to remove.
    pub(crate) fn save_draft_and_open_matrix(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.drafting.as_ref().map(|d| d.path.clone()) else {
            return;
        };
        self.save_version_draft(cx);
        if self.drafting.is_none() {
            self.open_preset_matrix(path, cx);
        }
    }

    /// Lay the draft out and rasterize one page, off the UI thread.
    pub(crate) fn compile_draft(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.drafting.as_mut() else {
            return;
        };
        let source =
            crate::resume::template::generate_for_with_profiles(&draft.doc, self.cache.profiles());
        draft.compiling = true;
        draft.preview_current = false;
        draft.preview_error = None;

        let engine = self
            .thumb_engine
            .get_or_insert_with(|| {
                std::sync::Arc::new(std::sync::Mutex::new(
                    crate::typst_engine::TypstEngine::new(String::new()),
                ))
            })
            .clone();
        let executor = cx.background_executor().clone();

        let task = cx.spawn(async move |this, cx| {
            let compiled = executor
                .spawn(async move {
                    let mut engine = engine.lock().unwrap_or_else(|e| e.into_inner());
                    engine.set_source(source);
                    engine.compile_to_pixels(PREVIEW_SCALE).ok()
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                let Some(draft) = this.drafting.as_mut() else {
                    return;
                };
                draft.compiling = false;
                // A reading that will not compile keeps the last page it had
                // rather than blanking: the change you just made is the thing
                // to look at, and an empty sheet says the document is empty.
                if let Some((pixels, geometry)) = compiled {
                    match crate::render::pixels_to_render_image(pixels, PREVIEW_SCALE) {
                        Ok(page) => {
                            draft.page = Some(page);
                            draft.geometry = Some(geometry);
                            draft.preview_current = true;
                        }
                        Err(error) => {
                            draft.preview_error = Some(format!("Could not draw the preview: {error}"));
                        }
                    }
                } else {
                    draft.preview_error = Some("Could not compile this version's layout. Check the sections, then try again.".into());
                }
                cx.notify();
            });
        });

        if let Some(draft) = self.drafting.as_mut() {
            draft.task = Some(task);
        }
    }

    /// Why this source, in evidence rather than opinion.
    ///
    /// The board knows how many applications went out under each reading and
    /// how many came back. That is a fact about this person's own history, and
    /// it is the whole of what this panel is allowed to say until there is a
    /// job description to read — see the note in `tailor.rs`.
    pub(crate) fn draft_source_evidence(&self) -> Option<(String, String)> {
        let draft = self.drafting.as_ref()?;
        let name = draft.source_name();
        let lookup_name = if draft.document_source == Some(draft.source) {
            ""
        } else {
            &name
        };
        let record = self
            .cache
            .applications()
            .record_for(&draft.stem(), lookup_name);

        Some(match (record.sent, record.interviewed) {
            (0, _) => (
                format!("{name} is where you left this CV"),
                "Nothing has gone out under any version of it yet, so there is no record to \
                 choose by. Start from the one that is closest and change what does not fit."
                    .to_string(),
            ),
            (sent, 0) => (
                format!("{name} is the one you send"),
                format!(
                    "{sent} application{} went out under it. None have come back as an \
                     interview yet.",
                    if sent == 1 { "" } else { "s" }
                ),
            ),
            (sent, interviewed) => (
                format!("{name} is the one that has been working"),
                format!(
                    "{sent} application{} went out under it, and {interviewed} came back as \
                     an interview.",
                    if sent == 1 { "" } else { "s" }
                ),
            ),
        })
    }
}

/// How many of the offered changes are on.
pub(crate) fn selected(changes: &[Change]) -> usize {
    changes::applied_count(changes)
}

/// The prompt only needs work history and qualifications. Contact details are
/// not useful for matching a posting, so redact them before they leave DockCV.
fn assistant_cv_text(doc: &ResumeDoc) -> String {
    let mut resume = doc.compose();
    resume.basics.name.clear();
    resume.basics.email.clear();
    resume.basics.phone.clear();
    resume.basics.location.clear();
    resume.basics.url.clear();
    resume.basics.profiles.clear();
    crate::resume::export_plain_text(&resume)
}

/// The exact existing text behind each alternative, so a browser assistant
/// can compare the contents as well as the variant's title and description.
fn assistant_section_text(doc: &ResumeDoc, section: SectionKind) -> String {
    let resume = doc.compose();
    match section {
        SectionKind::Profile => format!("{} {}", resume.basics.label, resume.basics.summary),
        SectionKind::Work => resume
            .work
            .iter()
            .map(|item| {
                format!(
                    "{} · {} · {} · {}",
                    item.name,
                    item.position,
                    item.summary,
                    item.highlights.join("; ")
                )
            })
            .collect::<Vec<_>>()
            .join("; "),
        SectionKind::Education => resume
            .education
            .iter()
            .map(|item| {
                format!(
                    "{} · {} · {}",
                    item.institution,
                    item.study_type,
                    item.highlights.join("; ")
                )
            })
            .collect::<Vec<_>>()
            .join("; "),
        SectionKind::Skills => resume
            .skills
            .iter()
            .map(|item| format!("{}: {}", item.name, item.keywords.join(", ")))
            .collect::<Vec<_>>()
            .join("; "),
        SectionKind::Certificates => resume
            .certificates
            .iter()
            .map(|item| format!("{} · {}", item.name, item.issuer))
            .collect::<Vec<_>>()
            .join("; "),
        SectionKind::Organizations => resume
            .volunteer
            .iter()
            .map(|item| {
                format!(
                    "{} · {} · {}",
                    item.organization,
                    item.position,
                    item.highlights.join("; ")
                )
            })
            .collect::<Vec<_>>()
            .join("; "),
        SectionKind::Custom(id) => resume
            .custom_sections
            .iter()
            .find(|item| item.id == id)
            .map(|section| {
                section
                    .entries
                    .iter()
                    .map(|item| {
                        format!(
                            "{} · {} · {}",
                            item.title,
                            item.subtitle,
                            item.highlights.join("; ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default(),
    }
}

/// Keep the generated selection label readable and unique inside a section.
fn unique_variant_name(doc: &ResumeDoc, section: SectionKind, base: &str) -> String {
    let base: String = base.chars().take(48).collect();
    let existing = doc.variant_names(section);
    if !existing.iter().any(|name| name == &base) {
        return base;
    }
    for suffix in 2..=999 {
        let candidate = format!("{base} {suffix}");
        if !existing.iter().any(|name| name == &candidate) {
            return candidate;
        }
    }
    format!("{base} copy")
}

/// What the row of sections a change touches is called, for the row's eyebrow.
pub(crate) fn section_label(doc: &ResumeDoc, section: SectionKind) -> String {
    doc.section_title(section)
}
