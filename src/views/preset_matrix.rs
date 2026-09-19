//! The Preset Matrix: section × preset, with the working copy as its first
//! column (US-02 / P-01).
//!
//! Not a comparison screen. The first pass drew preset A against preset B
//! because the mockup did, and a `+ compare a third` prompt that was never
//! built sat beside it for two releases — which is the shape telling on
//! itself. A document has two or three readings (the design doc's §2 numbers),
//! so showing *all* of them is simpler than making somebody pick two, and
//! comparison is then what a grid does for free.
//!
//! Rendering lives in `preset_matrix_grid.rs`; this file is the state and the
//! questions the grid asks of it.

use gpui::{Entity, Subscription};
use std::collections::HashMap;
use std::path::PathBuf;

use dockcv_ui_components::TextFieldState;

use crate::resume::model::{ResumeDoc, SectionKind, VariantId};
use crate::resume::outcomes::PresetRecord;
use crate::typst_engine::PageGeometry;

pub struct PresetMatrix {
    pub path: PathBuf,
    pub doc: ResumeDoc,
    /// What `path` held when this screen took its copy — the same guard the
    /// editor carries, for the same reason: this screen holds a whole document
    /// in memory and writes all of it back. See [`crate::vault::OnDisk`].
    pub on_disk: crate::vault::OnDisk,
    /// Hide the rows every column agrees on.
    ///
    /// Defaulted on past three presets rather than always: at two or three
    /// columns the agreeing rows are the context that makes the differing ones
    /// legible, and at five they are the noise a comparison table is told to
    /// drop (NN/g, Baymard — the design doc's §2).
    pub differences_only: bool,
    /// Whichever preset is mid-rename. One at a time, like the editor's
    /// section rename — the gesture this deliberately copies, so a preset and
    /// a section heading are renamed the same way in the same product.
    pub renaming_preset: Option<PresetRename>,
    /// How many pages each column lays out into, keyed by [`Column::preset`].
    ///
    /// Measured, never guessed: a column with no entry here prints no page
    /// line at all rather than a number that might be wrong. Cleared whenever
    /// a pin changes, because a pin is exactly what decides a column's length.
    pub pages: HashMap<Option<usize>, PageGeometry>,
    /// True while a measuring pass is in flight, so a second one does not
    /// stack behind it.
    pub measuring: bool,
    /// What the board says each reading has done, keyed the same way.
    ///
    /// Read once when the screen opens rather than per frame: the board cannot
    /// change while the matrix is in front of it.
    pub records: HashMap<usize, PresetRecord>,
}

/// Live state for a preset rename. `FieldId::PresetName` was addressable from
/// the day presets existed and no view drew it, so a preset created as
/// `Preset 2` kept that name for life (G-14).
pub struct PresetRename {
    pub idx: usize,
    pub field: Entity<TextFieldState>,
    /// Keeps the `TextFieldEvent` → commit translation alive for the rename.
    pub _subscription: Subscription,
}

/// What one column says about one section.
///
/// Three states, and the third is the one ids bought us (C0): a preset can
/// pin a variant that has since been deleted, which is a different fact from
/// "this preset says nothing about this section" and from "this preset leaves
/// the section out". Before ids, all three looked alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Pin(VariantId),
    Hidden,
    Unpinned,
}

/// A column of the grid: the working copy, or one preset.
pub struct Column {
    /// `None` is the working copy — always first, never editable here.
    pub preset: Option<usize>,
    pub name: String,
    /// `ACTIVE` or `EDITED`, from [`PresetMatrix::working_copy_mark`].
    pub mark: Option<&'static str>,
}

impl PresetMatrix {
    pub fn new(path: PathBuf, doc: ResumeDoc) -> Self {
        let on_disk = crate::vault::OnDisk::read(&path);
        let differences_only = doc.presets.len() > 3;

        Self {
            path,
            doc,
            on_disk,
            differences_only,
            renaming_preset: None,
            pages: HashMap::new(),
            measuring: false,
            records: HashMap::new(),
        }
    }

    /// Forget every page measurement.
    ///
    /// Called when a pin moves. Clearing all of them rather than the one
    /// column that changed is deliberate: two presets can pin the same
    /// variants, and a measurement is cheap enough that being right is worth
    /// more than being clever about which columns to keep.
    pub fn forget_measurements(&mut self) {
        self.pages.clear();
    }

    /// The Typst source a column would compile to, for measuring.
    pub fn source_for(&self, preset: Option<usize>) -> String {
        let mut doc = self.doc.clone();
        if let Some(index) = preset {
            doc.apply_preset(index);
        }
        crate::resume::template::generate_for(&doc)
    }

    /// `1 page`, or `2 pages · 6 lines over` when a reading does not fit.
    ///
    /// `None` until the column has actually been measured. The page count is
    /// the same integer the preview toolbar shows, and the overflow is said in
    /// lines because that is the unit a person trims in — both come from the
    /// compiler's own measurement of the laid-out pages, not from arithmetic
    /// on the settings.
    pub fn pages_line(&self, preset: Option<usize>) -> Option<String> {
        let geometry = self.pages.get(&preset)?;
        let pages = geometry.page_count.max(1);
        let noun = if pages == 1 { "page" } else { "pages" };
        if geometry.overflow_pt <= 0.0 {
            return Some(format!("{pages} {noun}"));
        }
        match geometry.line_advance_pt {
            Some(advance) if advance > 0.0 => {
                let lines = (geometry.overflow_pt / advance).ceil() as i64;
                let line_noun = if lines == 1 { "line" } else { "lines" };
                Some(format!("{pages} {noun} · {lines} {line_noun} over"))
            }
            // Measured as overflowing, but the page holds too little text to
            // average a line height from. Saying how many pages is still true.
            _ => Some(format!("{pages} {noun}")),
        }
    }

    /// Whether a column's measurement says it runs past one page.
    pub fn overflows(&self, preset: Option<usize>) -> bool {
        self.pages
            .get(&preset)
            .is_some_and(|geometry| geometry.overflow_pt > 0.0)
    }

    /// `sent 11 · 4 interviews`, or nothing for a reading nothing went out
    /// under. Counts only — never a rate (see `PresetRecord`).
    pub fn record_line(&self, preset: Option<usize>) -> Option<String> {
        let record = self.records.get(&preset?)?;
        if record.is_empty() {
            return None;
        }
        if record.interviewed == 0 {
            return Some(format!("sent {}", record.sent));
        }
        let noun = if record.interviewed == 1 {
            "interview"
        } else {
            "interviews"
        };
        Some(format!(
            "sent {} · {} {noun}",
            record.sent, record.interviewed
        ))
    }

    /// The mark carried by a preset column relative to the working copy.
    ///
    /// Exact equality wins and only the first identical preset is active. If
    /// none is exact, the nearest preset receives `EDITED`; distance and ties
    /// are defined by `ResumeDoc`, so editor and matrix cannot disagree.
    pub fn working_copy_mark(&self, index: usize) -> Option<&'static str> {
        match self.doc.active_preset_index() {
            Some(active) if active == index => Some("ACTIVE"),
            Some(_) => None,
            None if self.doc.nearest_preset_index() == Some(index) => Some("EDITED"),
            None => None,
        }
    }

    /// Every column, working copy first, presets in document order.
    pub fn columns(&self) -> Vec<Column> {
        let mut columns = vec![Column {
            preset: None,
            name: "Now".to_string(),
            mark: None,
        }];
        columns.extend(
            self.doc
                .presets
                .iter()
                .enumerate()
                .map(|(index, preset)| Column {
                    preset: Some(index),
                    name: preset.name.clone(),
                    mark: self.working_copy_mark(index),
                }),
        );
        columns
    }

    /// What `column` says about `section`.
    pub fn choice(&self, column: &Column, section: SectionKind) -> Choice {
        match column.preset {
            // The working copy reads the document itself, which is the point
            // of drawing it: the other columns are claims about what would
            // happen, this one is what is on the page right now.
            None => {
                if self.doc.is_hidden(section) {
                    Choice::Hidden
                } else {
                    match self.doc.active_variant_id(section) {
                        Some(id) => Choice::Pin(id),
                        None => Choice::Unpinned,
                    }
                }
            }
            Some(index) => match self.doc.presets.get(index) {
                Some(preset) if preset.hidden.contains(&section) => Choice::Hidden,
                Some(preset) => match preset.variant_for(section) {
                    Some(id) => Choice::Pin(id),
                    None => Choice::Unpinned,
                },
                None => Choice::Unpinned,
            },
        }
    }

    /// What the working copy says — the reference every other column is read
    /// against, because "how does this preset differ from what I am looking
    /// at" is the question somebody standing in front of the grid has.
    pub fn working_copy_choice(&self, section: SectionKind) -> Choice {
        self.choice(
            &Column {
                preset: None,
                name: String::new(),
                mark: None,
            },
            section,
        )
    }

    /// The heading one column prints for `section`.
    pub fn section_heading(&self, column: &Column, section: SectionKind) -> Option<String> {
        match column.preset {
            None => Some(self.doc.section_title(section)),
            Some(index) => self.doc.section_title_for_preset(index, section),
        }
    }

    /// The one-based place one column gives `section` in the rendered order.
    pub fn section_number(&self, column: &Column, section: SectionKind) -> Option<usize> {
        let order = match column.preset {
            None => self.doc.sections(),
            Some(index) => self.doc.sections_for_preset(index)?,
        };
        order
            .iter()
            .position(|kind| *kind == section)
            .map(|i| i + 1)
    }

    /// Whether `column` departs from the working copy anywhere on this row.
    pub fn cell_differs(&self, column: &Column, section: SectionKind) -> bool {
        let working = Column {
            preset: None,
            name: String::new(),
            mark: None,
        };
        self.choice(column, section) != self.working_copy_choice(section)
            || self.section_heading(column, section) != self.section_heading(&working, section)
            || self.section_number(column, section) != self.section_number(&working, section)
    }

    /// Whether headings differ anywhere across this row.
    pub fn heading_differs(&self, section: SectionKind) -> bool {
        let working = self.doc.section_title(section);
        self.columns().iter().any(|column| {
            self.section_heading(column, section).as_deref() != Some(working.as_str())
        })
    }

    /// Whether the section occupies different positions across the columns.
    pub fn order_differs(&self, section: SectionKind) -> bool {
        let working = self
            .doc
            .sections()
            .iter()
            .position(|kind| *kind == section)
            .map(|i| i + 1);
        self.columns()
            .iter()
            .any(|column| self.section_number(column, section) != working)
    }

    /// Whether any column disagrees with the working copy on this section.
    pub fn row_differs(&self, section: SectionKind) -> bool {
        self.columns()
            .iter()
            .any(|column| self.cell_differs(column, section))
    }

    /// The rows the grid draws, after `differences_only`.
    pub fn rows(&self) -> Vec<SectionKind> {
        self.doc
            .sections()
            .into_iter()
            .filter(|section| !self.differences_only || self.row_differs(*section))
            .collect()
    }

    /// What a cell prints.
    ///
    /// `— deleted variant —` is the loud half of C0: the preset selected a cut
    /// of this section that no longer exists. Clicking the cell repairs it.
    pub fn cell_text(&self, section: SectionKind, choice: Choice) -> String {
        match choice {
            Choice::Hidden => "— hidden —".to_string(),
            Choice::Unpinned => "not pinned".to_string(),
            Choice::Pin(id) => self
                .variant_label(section, id)
                .unwrap_or_else(|| "— deleted variant —".to_string()),
        }
    }

    /// The secondary line on a cell — `· 4 roles`, and nothing for a choice
    /// that names no variant.
    pub fn cell_detail(&self, section: SectionKind, choice: Choice) -> Option<String> {
        let Choice::Pin(id) = choice else {
            return None;
        };
        let name = self.variant_label(section, id)?;
        self.variant_detail(section, &name)
    }

    /// The name to print for a pinned id.
    ///
    /// `None` when nothing in the section carries it, which is a preset
    /// pinning a variant that has been deleted. The cell says so rather than
    /// showing the variant the document happens to be on, because those are
    /// different facts and only one of them is about the preset.
    pub fn variant_label(&self, section: SectionKind, id: VariantId) -> Option<String> {
        let index = self
            .doc
            .variant_ids(section)
            .iter()
            .position(|v| *v == id)?;
        self.doc.variant_names(section).get(index).cloned()
    }

    fn variant_detail(&self, section: SectionKind, variant_name: &str) -> Option<String> {
        match section {
            SectionKind::Profile => {
                if let Some(v) = self
                    .doc
                    .profile
                    .variants
                    .iter()
                    .find(|v| v.name == variant_name)
                {
                    let lines = v.data.summary.lines().count();
                    Some(format!("· {lines} lines"))
                } else {
                    None
                }
            }
            SectionKind::Work => {
                if let Some(v) = self
                    .doc
                    .work
                    .variants
                    .iter()
                    .find(|v| v.name == variant_name)
                {
                    let count = v.data.len();
                    Some(format!("· {count} roles"))
                } else {
                    None
                }
            }
            SectionKind::Education => {
                if let Some(v) = self
                    .doc
                    .education
                    .variants
                    .iter()
                    .find(|v| v.name == variant_name)
                {
                    let count = v.data.len();
                    Some(format!("· {count} entries"))
                } else {
                    None
                }
            }
            SectionKind::Skills => {
                if let Some(v) = self
                    .doc
                    .skills
                    .variants
                    .iter()
                    .find(|v| v.name == variant_name)
                {
                    let count = v.data.len();
                    Some(format!("· {count} groups"))
                } else {
                    None
                }
            }
            // Counts, like every other section. The design draws `· shown` /
            // `— hidden —` here, but that is **section visibility** (O-13),
            // which the model does not have — and an *empty* section is not a
            // hidden one. A CV with no certificates yet would have been
            // labelled "hidden from this preset", which is a claim about the
            // user's intent derived from the absence of data.
            SectionKind::Certificates => self
                .doc
                .certificates
                .variants
                .iter()
                .find(|v| v.name == variant_name)
                .map(|v| format!("· {} entries", v.data.len())),
            SectionKind::Organizations => self
                .doc
                .volunteer
                .variants
                .iter()
                .find(|v| v.name == variant_name)
                .map(|v| format!("· {} entries", v.data.len())),
            SectionKind::Custom(id) => self
                .doc
                .custom_section(id)
                .and_then(|s| s.content.variants.iter().find(|v| v.name == variant_name))
                .map(|v| format!("· {} entries", v.data.len())),
        }
    }

    pub(super) fn identity(&self) -> String {
        let name = self.doc.profile.active().name.trim().to_string();
        if !name.is_empty() {
            return name;
        }
        self.path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    }
}
