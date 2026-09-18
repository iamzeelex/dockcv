//! Preset Matrix Screen (US-02 / P-01): Section × Variant grid and two-preset diff view.
//!
//! From the mockup's preset-matrix row.
//!
//! Features:
//! 1. Header Toolbar with a `<person_name> / Presets` breadcrumb and `+ Save current as new preset` action.
//! 2. `COMPARING` toolbar with interactive Preset A vs Preset B pills, `vs` divider, `+ compare a third` prompt, and `■ Differs between presets` legend indicator.
//! 3. 3-Column Table Grid (`Section | Preset A | Preset B`) with clean cell agreement vs amber-highlighted diff cells (`border-left: 2px solid theme.warning` + translucent amber background).

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, Div, Entity, FontWeight, SharedString, Subscription};
use std::collections::HashMap;
use std::path::PathBuf;

use dockcv_ui_components::{
    Button, ButtonExt, ButtonVariants, DockIcon, Icon, IconName, ScrollableElement, Sizable,
    TextField, TextFieldState, CHROME_HEIGHT, MONO, SANS,
};

use crate::resume::model::{ResumeDoc, SectionKind, VariantId};
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::shell::Shell;

pub struct PresetMatrix {
    pub path: PathBuf,
    pub doc: ResumeDoc,
    /// What `path` held when this screen took its copy — the same guard the
    /// editor carries, for the same reason: this screen holds a whole document
    /// in memory and writes all of it back. See [`crate::vault::OnDisk`].
    pub on_disk: crate::vault::OnDisk,
    pub active_preset_idx: usize,
    pub compare_preset_idx: Option<usize>,
    /// Whichever preset is mid-rename. One at a time, like the editor's
    /// section rename — the gesture this deliberately copies, so a preset and
    /// a section heading are renamed the same way in the same product.
    pub renaming_preset: Option<PresetRename>,
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

impl PresetMatrix {
    pub fn new(path: PathBuf, doc: ResumeDoc) -> Self {
        let on_disk = crate::vault::OnDisk::read(&path);
        let compare_preset_idx = if doc.presets.len() > 1 {
            Some(1)
        } else {
            Some(0)
        };

        Self {
            path,
            doc,
            on_disk,
            active_preset_idx: 0,
            compare_preset_idx,
            renaming_preset: None,
        }
    }

    /// The left pill: the preset's name, or the live input while it is being
    /// renamed. The pen beside it is the same trigger the editor puts on a
    /// section header — one gesture for renaming anything the user named.
    pub fn render_preset_a_pill(&self, cx: &mut Context<Shell>, name: String) -> impl IntoElement {
        let renaming = self
            .renaming_preset
            .as_ref()
            .filter(|r| r.idx == self.active_preset_idx);

        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(match renaming {
                Some(rename) => div()
                    .w(px(150.0))
                    .child(TextField::new(&rename.field).small())
                    .into_any_element(),
                None => Button::new("preset-a-pill")
                    .chip(false, cx.theme())
                    .primary()
                    .tooltip("Cycle to the next preset")
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.cycle_matrix_preset_a(cx);
                    }))
                    .child(name)
                    .into_any_element(),
            })
            .child(
                Button::new("preset-a-rename")
                    .icon_only()
                    .icon(DockIcon::Pen)
                    .tooltip("Rename this preset")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.start_preset_rename(window, cx);
                    })),
            )
    }

    /// Cycle active Preset A selection.
    pub fn cycle_preset_a(&mut self) {
        if !self.doc.presets.is_empty() {
            self.active_preset_idx = (self.active_preset_idx + 1) % self.doc.presets.len();
        }
    }

    /// Cycle active Preset B selection.
    pub fn cycle_preset_b(&mut self) {
        if !self.doc.presets.is_empty() {
            let current = self.compare_preset_idx.unwrap_or(0);
            self.compare_preset_idx = Some((current + 1) % self.doc.presets.len());
        }
    }

    /// What each of the two presets pins for each section, by id.
    ///
    /// Ids rather than names since C0: a cell has to be able to say "this
    /// preset selected a cut of Skills that has since been deleted", and a
    /// name cannot tell that apart from a variant that was merely renamed.
    /// [`Self::variant_label`] is what turns an id back into something to read.
    pub fn compute_diff(&self) -> HashMap<SectionKind, (VariantId, Option<VariantId>)> {
        let mut map = HashMap::new();

        let preset_a = self.doc.presets.get(self.active_preset_idx);
        let preset_b = self
            .compare_preset_idx
            .and_then(|idx| self.doc.presets.get(idx));

        for section in self.doc.sections() {
            let active_a = preset_a
                .and_then(|p| p.variant_for(section))
                .or_else(|| self.doc.active_variant_id(section))
                .unwrap_or_default();

            let active_b = preset_b.and_then(|p| p.variant_for(section));

            map.insert(section, (active_a, active_b));
        }

        map
    }

    /// Whether preset `index` leaves `section` out of the document.
    pub fn preset_hides(&self, index: usize, section: SectionKind) -> bool {
        self.doc
            .presets
            .get(index)
            .is_some_and(|p| p.hidden.contains(&section))
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

    fn identity(&self) -> String {
        let name = self.doc.profile.active().name.trim().to_string();
        if !name.is_empty() {
            return name;
        }
        self.path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    pub fn render_matrix(&self, cx: &mut Context<Shell>) -> Div {
        let theme = *cx.theme();
        let diff_map = self.compute_diff();

        let preset_a_name = self
            .doc
            .presets
            .get(self.active_preset_idx)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Preset A".to_string());

        let preset_b_name = self
            .compare_preset_idx
            .and_then(|idx| self.doc.presets.get(idx))
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Preset B".to_string());

        // Header Toolbar matching mockup lines 1381-1392
        let header_toolbar = div()
            .flex()
            .items_center()
            .justify_between()
            .h(CHROME_HEIGHT)
            .pl(px(80.0))
            .pr(px(20.0))
            .bg(theme.chrome)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .child(
                        Button::new("matrix-back")
                            .quiet()
                            .gap(px(4.0))
                            .icon(IconName::ChevronLeft)
                            .tooltip("Back to the document")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.leave_preset_matrix(cx);
                            }))
                            .child(self.identity()),
                    )
                    .child(
                        div()
                            .font_family(SANS)
                            .text_size(px(13.5))
                            .text_color(theme.border_strong)
                            .child("/"),
                    )
                    .child(
                        div()
                            .font_family(SANS)
                            .text_size(px(13.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child("Presets"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Button::new("export-all-presets-btn")
                            .toolbar()
                            .icon(IconName::ArrowDown)
                            .tooltip("Export every preset to PDF into a folder")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.export_all_matrix_presets(cx);
                            }))
                            .child("Export all presets"),
                    )
                    .child(
                        Button::new("save-new-preset-btn")
                            .quiet()
                            .text_color(theme.accent)
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.save_matrix_as_preset(cx);
                            }))
                            .icon(IconName::Plus)
                            .child("Save current as new preset"),
                    ),
            );

        // Comparing Bar matching mockup lines 1394-1402
        let comparing_bar = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .mb(px(22.0))
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_subtle)
                    .child("COMPARING"),
            )
            .child(self.render_preset_a_pill(cx, preset_a_name.clone()))
            .child(
                div()
                    .font_family(SANS)
                    .text_size(px(12.0))
                    .text_color(theme.text_subtle)
                    .child("vs"),
            )
            .child(
                Button::new("preset-b-pill")
                    .chip(false, cx.theme())
                    .outline()
                    .tooltip("Compare against a different preset")
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.cycle_matrix_preset_b(cx);
                    }))
                    .child(preset_b_name.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_style(TextStyle::label())
                    .text_color(theme.text_subtle)
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(cx.theme().radius_sm())
                    .border_1()
                    .border_dashed()
                    .border_color(theme.border)
                    .child(Icon::new(IconName::Plus).with_size(theme.icon_sm()))
                    .child("compare a third"),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .font_family(SANS)
                    .text_size(px(12.0))
                    .text_color(theme.accent)
                    .child(
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded(px(2.0))
                            .bg(theme.warning),
                    )
                    .child("Differs between presets"),
            );

        // 3-Column Table Grid matching mockup lines 1403-1427
        let mut table_rows = Vec::new();

        // Header Row
        table_rows.push(
            div()
                .flex()
                .w_full()
                .gap(px(1.0))
                .child(
                    div()
                        .w(px(170.0))
                        .flex_none()
                        .bg(theme.surface)
                        .px(px(16.0))
                        .py(px(12.0))
                        .font_family(MONO)
                        .text_size(px(11.0))
                        .text_color(theme.text_subtle)
                        .child("SECTION"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .bg(theme.surface)
                        .px(px(16.0))
                        .py(px(12.0))
                        .font_family(SANS)
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .child(preset_a_name),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .bg(theme.surface)
                        .px(px(16.0))
                        .py(px(12.0))
                        .font_family(SANS)
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .child(preset_b_name),
                ),
        );

        // Data Rows — from the document's own section order, so custom
        // sections (D-9) appear and a renamed heading (O-14) shows the user's
        // word rather than the enum's.
        for section in self.doc.sections() {
            let section_label = self.doc.section_title(section);

            let (pin_a, pin_b) = diff_map.get(&section).copied().unwrap_or_default();
            let name_a = self.variant_label(section, pin_a);
            let name_b = pin_b.map(|b| self.variant_label(section, b));
            // O-13 is modelled now, so the design's `— hidden —` is real: it
            // says this preset leaves the section out of the document, which
            // `ResumeDoc::compose` honours all the way to the PDF.
            let hidden_a = self.preset_hides(self.active_preset_idx, section);
            let hidden_b = self
                .compare_preset_idx
                .is_some_and(|i| self.preset_hides(i, section));
            let is_diff = hidden_a != hidden_b || pin_b.is_some_and(|b| b != pin_a);

            let detail_a = name_a
                .as_deref()
                .and_then(|n| self.variant_detail(section, n));
            let detail_b = name_b
                .as_ref()
                .and_then(|b| b.as_deref())
                .and_then(|n| self.variant_detail(section, n));

            // Three different things a cell can say, and the third is new with
            // C0. `— hidden —` is section visibility (O-13). `not pinned` is a
            // preset that names no variant here at all, which
            // `reconcile_presets` now prevents but a hand-edited file can still
            // produce. `— deleted variant —` is a pin that resolves to nothing:
            // the preset selected a cut of this section that no longer exists,
            // which used to be indistinguishable from the document simply being
            // on something else. Clicking the cell repairs it.
            let label = |hidden: bool, name: Option<Option<String>>| match (hidden, name) {
                (true, _) => "— hidden —".to_string(),
                (_, None) => "not pinned".to_string(),
                (_, Some(None)) => "— deleted variant —".to_string(),
                (_, Some(Some(name))) => name,
            };
            let cell_a_text = label(hidden_a, Some(name_a.clone()));
            let cell_b_text = label(hidden_b, name_b.clone());

            let row = div()
                .flex()
                .w_full()
                .gap(px(1.0))
                // Col 1: Section
                .child(
                    div()
                        .w(px(170.0))
                        .flex_none()
                        .bg(theme.surface)
                        .px(px(16.0))
                        .py(px(15.0))
                        .font_family(SANS)
                        .text_size(px(13.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .child(section_label),
                )
                // Col 2: Preset A Cell. Clicking cycles this preset's pin for
                // the row's section — US-02 asks for a matrix you can *edit*,
                // not just read, and cycling is the whole interaction a cell
                // needs when a section has two or three variants.
                .child(
                    div()
                        .id(SharedString::from(format!("cell-a-{section:?}")))
                        .cursor_pointer()
                        // Clicking a cell cycles that section's variant, and
                        // nothing but the cursor used to say so.
                        .hover(|s| s.border_color(theme.accent))
                        .border_1()
                        .border_color(gpui::Hsla::transparent_black())
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            this.cycle_matrix_cell(0, section, cx);
                        }))
                        .flex_1()
                        .min_w_0()
                        .px(px(16.0))
                        .py(px(15.0))
                        .text_style(TextStyle::body())
                        .when(is_diff, |s| {
                            s.bg(theme.hover)
                                .border_l_2()
                                .border_color(theme.warning)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text)
                        })
                        .when(!is_diff, |s| {
                            s.bg(theme.elevated).text_color(theme.text_muted)
                        })
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .gap(px(6.0))
                                .child(cell_a_text.clone())
                                .when_some(detail_a, |s, d| {
                                    s.child(
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(if is_diff {
                                                theme.warning
                                            } else {
                                                theme.text_subtle
                                            })
                                            .child(d),
                                    )
                                }),
                        ),
                )
                // Col 3: Preset B Cell
                .child(
                    div()
                        .id(SharedString::from(format!("cell-b-{section:?}")))
                        .cursor_pointer()
                        // Clicking a cell cycles that section's variant, and
                        // nothing but the cursor used to say so.
                        .hover(|s| s.border_color(theme.accent))
                        .border_1()
                        .border_color(gpui::Hsla::transparent_black())
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            this.cycle_matrix_cell(1, section, cx);
                        }))
                        .flex_1()
                        .min_w_0()
                        .px(px(16.0))
                        .py(px(15.0))
                        .text_style(TextStyle::body())
                        .when(is_diff, |s| {
                            s.bg(theme.hover)
                                .border_l_2()
                                .border_color(theme.warning)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text)
                        })
                        .when(!is_diff, |s| {
                            s.bg(theme.elevated).text_color(theme.text_muted)
                        })
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .gap(px(6.0))
                                .child(cell_b_text)
                                .when_some(detail_b, |s, d| {
                                    s.child(
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(if is_diff {
                                                theme.warning
                                            } else {
                                                theme.text_subtle
                                            })
                                            .child(d),
                                    )
                                }),
                        ),
                );

            table_rows.push(row);
        }

        let matrix_table = div()
            .w_full()
            .max_w(px(1180.0))
            .flex()
            .flex_col()
            .gap(px(1.0))
            .bg(theme.border)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md())
            .overflow_hidden()
            .children(table_rows);

        let matrix_body = div()
            .id("preset-matrix-grid")
            .flex_1()
            .p(px(26.0))
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .child(comparing_bar)
            .child(matrix_table);

        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .child(header_toolbar)
            .child(matrix_body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::{Preset, ResumeDoc, SectionKind};

    /// Two presets that differ on Work and agree on Profile, pinned by id and
    /// read back as names.
    #[test]
    fn test_preset_matrix_diff_computation() {
        let mut doc = ResumeDoc::default();
        doc.work.variants[0].name = "FAANG".into();
        let faang = doc.work.active_id();
        doc.add_variant(SectionKind::Work); // "FAANG copy", now active
        doc.work.variants[1].name = "Startup".into();
        let startup = doc.work.active_id();
        let base = doc.profile.active_id();

        doc.presets = vec![
            Preset {
                name: "Preset A".into(),
                selection: vec![(SectionKind::Profile, base), (SectionKind::Work, faang)],
                hidden: Vec::new(),
            },
            Preset {
                name: "Preset B".into(),
                selection: vec![(SectionKind::Profile, base), (SectionKind::Work, startup)],
                hidden: Vec::new(),
            },
        ];

        let mut matrix = PresetMatrix::new(PathBuf::from("/dummy/path"), doc);
        matrix.active_preset_idx = 0;
        matrix.compare_preset_idx = Some(1);

        let diff = matrix.compute_diff();
        let (prof_a, prof_b) = diff.get(&SectionKind::Profile).copied().unwrap();
        assert_eq!(prof_a, base);
        assert_eq!(prof_b, Some(base));

        let (work_a, work_b) = diff.get(&SectionKind::Work).copied().unwrap();
        assert_eq!(work_a, faang);
        assert_eq!(work_b, Some(startup));
        assert_eq!(
            matrix.variant_label(SectionKind::Work, work_a).as_deref(),
            Some("FAANG")
        );
    }

    /// A preset pinning a variant that has been deleted is a cell that says so.
    ///
    /// The whole reason pins are ids: before C0 this cell showed whichever
    /// variant the *document* happened to be on, which is a different fact
    /// about a different object, and the user had no way to tell.
    #[test]
    fn a_pin_to_a_deleted_variant_is_labelled_rather_than_guessed_at() {
        let mut doc = ResumeDoc::default();
        doc.add_variant(SectionKind::Work); // "Base copy", now active
        doc.work.variants[1].name = "Infra".into();
        let infra = doc.work.active_id();
        doc.add_preset("Infra-heavy");

        doc.remove_variant(SectionKind::Work, 1);
        assert_eq!(doc.work.variants.len(), 1);

        let matrix = PresetMatrix::new(PathBuf::from("/dummy/path"), doc);
        assert_eq!(matrix.variant_label(SectionKind::Work, infra), None);
        assert_eq!(
            matrix.doc.unresolved_pins(0),
            vec![SectionKind::Work],
            "the preset still names the cut that was deleted, and says which"
        );
    }
}
