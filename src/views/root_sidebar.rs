//! Content mode: the document navigator and the inspector beside it.
//!
//! The editor used to mount every section as a card and let you open several at
//! once, which meant the panel held as many forms as you had opened and the
//! thing you were actually editing was somewhere in the middle of them. The
//! navigator on the left lists sections and, for the selected one, its entries;
//! the inspector on the right mounts **only the selected entry**. Every
//! `render_*_section` below therefore filters its loop to
//! `self.selection.item`.
//!
//! Selection is ephemeral and lives in `root_editor_state.rs`, which also
//! normalizes it after a delete, a variant change or an undo — an index into a
//! list that just got shorter is the one way this model can lie.
//!
//! Built-in sections live here; a custom section's own anatomy (D-9) is in
//! `root_custom_sections.rs`, which reuses the `card`/`field`/`entry_header`/
//! `add_button` building blocks defined below.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, IntoElement, Pixels, SharedString};

use dockcv_ui_components::{
    Button, ButtonExt, DockIcon, Field, Form, Icon, IconName, ScrollableElement, SelectableRow,
    Sizable, TextField, SANS,
};

use crate::resume::edit::{FieldId, ListId};
use crate::resume::model::SectionKind;
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::Root;

/// C-1 (the editor-comfort notes): upstream's `Field` label inherits
/// `TextStyle::label()`'s 1.45 leading — generous for a heading, wasteful for
/// a one-word field label that never wraps. Tightened at the call site, not
/// in the shared type scale: the 12px `label` step itself is untouched, only
/// the line box around it shrinks. A spacing fix, not a type-size one.
pub(super) const FIELD_LABEL_LINE_HEIGHT: Pixels = px(14.0);

impl Root {
    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let section = self.selection.section;
        let title = super::root_editor_state::section_label(&self.doc, section);
        let rows: Vec<AnyElement> = self
            .doc
            .sections()
            .into_iter()
            .map(|kind| self.render_nav_section(cx, kind))
            .collect();
        let selected_entry = self
            .selection
            .item
            .map(|i| super::root_editor_state::item_label(&self.doc, section, i));

        div()
            .flex()
            .h_full()
            .w_full()
            .min_w_0()
            .bg(theme.background)
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    // 168 truncated "Work Experience" and "Organizations" —
                    // two of the six built-in sections could not print their
                    // own names.
                    .w(px(190.0))
                    .h_full()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .border_r_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px(px(12.0))
                            .pt(px(18.0))
                            .pb(px(12.0))
                            .child(
                                div()
                                    .font_family(SANS)
                                    .text_size(px(11.0))
                                    .text_color(theme.text_subtle)
                                    .child("SECTIONS"),
                            )
                            .child(self.add_section_button(cx)),
                    )
                    .child(
                        div()
                            .id("section-navigator")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .px(px(6.0))
                            .pb(px(18.0))
                            .children(rows),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .px(px(20.0))
                            .pt(px(16.0))
                            .pb(px(13.0))
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(self.render_section_heading(cx, section, title.into()))
                                    .child(self.rename_button(cx, section))
                                    .child(self.section_layout_button(cx, section))
                                    .child(self.visibility_button(cx, section))
                                    .when_some(self.render_trim_chip(cx, section), |el, chip| {
                                        el.child(chip)
                                    })
                                    .when_some(
                                        match section {
                                            SectionKind::Custom(id) => {
                                                Some(self.section_menu_button(cx, id))
                                            }
                                            _ => None,
                                        },
                                        |el, menu| el.child(menu),
                                    )
                                    .child(div().flex_1()),
                            )
                            .when_some(selected_entry, |el, entry| {
                                // The entry's name and the two things you can
                                // do to the entry, on one line. They used to be
                                // an `Entry 1` header inside the form — a
                                // positional label in a panel that shows one
                                // entry, naming nothing, while the name itself
                                // was printed twice above it.
                                el.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .text_style(TextStyle::meta())
                                                .text_color(theme.text_muted)
                                                .child(entry),
                                        )
                                        .children(self.entry_actions(cx, section)),
                                )
                            })
                            .children(self.render_ats_chip(cx, section)),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "section-inspector-{section:?}-{:?}",
                                self.selection.item
                            )))
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .px(px(20.0))
                            .py(px(18.0))
                            .child(self.render_section(cx, section)),
                    ),
            )
    }

    fn render_nav_section(&self, cx: &mut Context<Self>, section: SectionKind) -> AnyElement {
        let theme = *cx.theme();
        let selected = self.selection.section == section;
        let expanded = self.expanded.contains(&section);
        let count = super::root_editor_state::item_count(&self.doc, section);
        let title = super::root_editor_state::section_label(&self.doc, section);
        let mut status = Vec::new();
        if self.doc.variant_names(section).len() > 1 {
            status.push(self.doc.variant_name(section).clone());
        }
        if self.doc.is_hidden(section) {
            status.push("Hidden".to_string());
        }
        let ats_count = self
            .ats_findings
            .iter()
            .filter(|finding| finding.section == section)
            .count();
        if ats_count > 0 {
            status.push(format!("ATS {ats_count}"));
        }
        let status = status.join(" · ");
        let title_for_drag: SharedString = title.clone().into();
        let row = SelectableRow::new(SharedString::from(format!("nav-section-{section:?}")))
            .selected(selected)
            .aria_label(title.clone())
            .leading(self.render_drag_handle(cx, section, title_for_drag))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().truncate().child(title))
                    .when(!status.is_empty(), |el| {
                        el.child(
                            div()
                                .truncate()
                                .text_style(TextStyle::meta())
                                .text_color(theme.text_subtle)
                                .child(status),
                        )
                    }),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.selection =
                    super::root_editor_state::EditorSelection::for_section(&this.doc, section);
                this.focused_section = section;
                this.expanded.insert(section);
                cx.notify();
            }))
            .trailing(
                div()
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_subtle)
                    .child(count.to_string()),
            )
            .trailing(
                Button::new(SharedString::from(format!("expand-section-{section:?}")))
                    .icon_only()
                    .icon(if selected && expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .tooltip(if selected && expanded {
                        "Collapse entries"
                    } else {
                        "Show entries"
                    })
                    // Only the selected section draws its entries, so on any
                    // other row this has to select as well — a chevron that
                    // sets a flag nothing reads is a control that does nothing.
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        if this.selection.section == section {
                            if !this.expanded.insert(section) {
                                this.expanded.remove(&section);
                            }
                        } else {
                            this.selection = super::root_editor_state::EditorSelection::for_section(
                                &this.doc, section,
                            );
                            this.focused_section = section;
                            this.expanded.insert(section);
                        }
                        cx.notify();
                    })),
            );
        // `section_drop_target` needs an interactive element to hang the drop
        // on, and a `SelectableRow` is a `RenderOnce`; the wrapper is what
        // reorder aims at, the row is what it looks like.
        let row = self.section_drop_target(
            cx,
            section,
            div()
                .id(SharedString::from(format!("nav-drop-{section:?}")))
                .w_full()
                .child(row),
        );
        let mut group = div()
            .flex()
            .flex_col()
            .gap(px(1.0))
            .mb(px(10.0))
            .child(row);
        if selected && expanded {
            if section == SectionKind::Profile {
                group =
                    group.child(self.render_nav_item(cx, section, None, "Identity".to_string()));
            }
            for index in 0..count {
                group = group.child(self.render_nav_item(
                    cx,
                    section,
                    Some(index),
                    super::root_editor_state::item_label(&self.doc, section, index),
                ));
            }
            if let Some(list) = Self::section_list(section) {
                group = group.child(self.nav_add_button(cx, section, list));
            }
        }
        group.into_any_element()
    }

    fn render_nav_item(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
        item: Option<usize>,
        label: String,
    ) -> AnyElement {
        let selected = self.selection.section == section && self.selection.item == item;
        SelectableRow::new(SharedString::from(format!("nav-item-{section:?}-{item:?}")))
            .selected(selected)
            // Past the drag handle the section rows carry, or an entry lines
            // up with its own parent instead of under it.
            .indent(px(26.0))
            .aria_label(label.clone())
            .child(div().w_full().min_w_0().truncate().child(label))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.selection = super::root_editor_state::EditorSelection { section, item };
                this.focused_section = section;
                cx.notify();
            }))
            .into_any_element()
    }

    pub(super) fn section_list(section: SectionKind) -> Option<ListId> {
        Some(match section {
            SectionKind::Profile => ListId::Profiles,
            SectionKind::Work => ListId::Work,
            SectionKind::Education => ListId::Education,
            SectionKind::Skills => ListId::Skills,
            SectionKind::Certificates => ListId::Certificates,
            SectionKind::Organizations => ListId::Volunteer,
            SectionKind::Custom(id) => ListId::CustomEntries(id),
        })
    }

    /// What the button in an empty section says. Naming the thing beats
    /// "Add entry" everywhere: the person is looking at a blank Work section,
    /// not at a generic list.
    fn add_label(section: SectionKind) -> &'static str {
        match section {
            SectionKind::Profile => "Add profile",
            SectionKind::Work => "Add role",
            SectionKind::Education => "Add degree",
            SectionKind::Skills => "Add skill group",
            SectionKind::Certificates => "Add certificate",
            SectionKind::Organizations => "Add organization",
            SectionKind::Custom(_) => "Add entry",
        }
    }

    fn nav_add_button(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
        list: ListId,
    ) -> AnyElement {
        // The same row shape as the entries above it: a different control
        // height here made the list end in a step.
        SelectableRow::new(SharedString::from(format!("nav-add-{section:?}")))
            .indent(px(26.0))
            .aria_label("Add entry")
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_color(cx.theme().accent)
                    .child(Icon::new(IconName::Plus).with_size(cx.theme().icon_sm()))
                    .child("Add entry"),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.checkpoint();
                list.add(&mut this.doc);
                this.selection = super::root_editor_state::EditorSelection {
                    section,
                    item: Some(super::root_editor_state::item_count(&this.doc, section) - 1),
                };
                this.fields_stale = true;
                this.schedule_save(cx);
                this.schedule_recompile(window, cx);
                cx.notify();
            }))
            .into_any_element()
    }

    /// Dispatches a `SectionKind` to its card. The single place that turns a
    /// section identity into rendered UI — `render_sidebar` never special-cases
    /// "one of the six" vs. "a custom one" itself.
    pub(super) fn render_section(&self, cx: &mut Context<Self>, kind: SectionKind) -> AnyElement {
        use SectionKind::*;
        match kind {
            Profile => self.render_profile_section(cx),
            Work => self.render_work_section(cx),
            Education => self.render_education_section(cx),
            Skills => self.render_skills_section(cx),
            Certificates => self.render_certificates_section(cx),
            Organizations => self.render_organizations_section(cx),
            Custom(id) => self.render_custom_section(cx, id),
        }
    }

    fn render_profile_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut f = self.variant_controls(cx, SectionKind::Profile);
        // Identity first: the name and the title are the document's headline.
        // The contact rows below carry the glyph that says what each is faster
        // than its label does.
        if self.selection.item.is_none() {
            f.push(self.field(cx, FieldId::Name, "Name"));
            f.push(self.field(cx, FieldId::Label, "Title"));
            f.push(self.field_with_icon(
                cx,
                FieldId::Email,
                "Email",
                DockIcon::Mail,
                TextStyle::code(),
            ));
            f.push(self.field_with_icon(
                cx,
                FieldId::Phone,
                "Phone",
                DockIcon::Phone,
                TextStyle::code(),
            ));
            f.push(self.field_with_icon(
                cx,
                FieldId::Location,
                "Location",
                DockIcon::MapPin,
                TextStyle::body(),
            ));
            f.push(self.field_with_icon(
                cx,
                FieldId::Url,
                "Website",
                DockIcon::Link,
                TextStyle::code(),
            ));
            f.push(self.field(cx, FieldId::Summary, "Summary"));
        }
        let profiles = self.doc.profile.active().profiles.len();
        for i in (0..profiles).filter(|i| Some(*i) == self.selection.item) {
            f.push(self.field(cx, FieldId::ProfileNetwork(i), "Network"));
            f.push(self.field(cx, FieldId::ProfileUsername(i), "Username"));
            f.push(self.field(cx, FieldId::ProfileUrl(i), "URL"));
        }
        self.card(cx, SectionKind::Profile, profiles, f)
    }

    fn render_work_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut f = self.variant_controls(cx, SectionKind::Work);
        let work = self.doc.work.active();
        for (i, w) in work
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) == self.selection.item)
        {
            // Ordered so the grid pairs what belongs together: role beside
            // employer, start beside end. Location used to sit between them and
            // pushed `End` onto a row of its own.
            f.push(self.field(cx, FieldId::WorkPosition(i), "Position"));
            f.push(self.field(cx, FieldId::WorkName(i), "Company"));
            f.extend(self.date_fields(cx, FieldId::WorkStart(i), FieldId::WorkEnd(i), "Current role"));
            f.push(self.field(cx, FieldId::WorkLocation(i), "Location"));
            f.push(self.field(cx, FieldId::WorkUrl(i), "URL"));
            f.push(self.field(cx, FieldId::WorkSummary(i), "Summary"));
            // C-5: one "Highlights" list, not a `Highlight 1`/`Highlight 2`…
            // row each.
            let highlight_fields: Vec<FieldId> = (0..w.highlights.len())
                .map(|j| FieldId::WorkHighlight(i, j))
                .collect();
            if let Some(highlights) = self.highlight_list(
                cx,
                "Highlights",
                ListId::WorkHighlights(i),
                &highlight_fields,
            ) {
                f.push(highlights);
            }
            f.push(Self::wide(self.add_button(
                cx,
                "Add highlight",
                ListId::WorkHighlights(i),
            )));
            f.push(Self::wide(self.diary_picker_button(cx, i)));
        }
        f.push(Self::wide(
            self.library_picker_button(cx, SectionKind::Work),
        ));
        self.card(cx, SectionKind::Work, work.len(), f)
    }

    fn render_education_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut f = self.variant_controls(cx, SectionKind::Education);
        let edu = self.doc.education.active();
        for (i, entry) in edu
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) == self.selection.item)
        {
            f.push(self.field(cx, FieldId::EduStudyType(i), "Degree"));
            f.push(self.field(cx, FieldId::EduInstitution(i), "Institution"));
            f.extend(self.date_fields(cx, FieldId::EduStart(i), FieldId::EduEnd(i), "Still studying"));
            f.push(self.field(cx, FieldId::EduUrl(i), "URL"));
            let highlight_fields: Vec<FieldId> = (0..entry.highlights.len())
                .map(|j| FieldId::EduHighlight(i, j))
                .collect();
            if let Some(highlights) = self.highlight_list(
                cx,
                "Highlights",
                ListId::EduHighlights(i),
                &highlight_fields,
            ) {
                f.push(highlights);
            }
            f.push(Self::wide(self.add_button(
                cx,
                "Add highlight",
                ListId::EduHighlights(i),
            )));
        }
        f.push(Self::wide(
            self.library_picker_button(cx, SectionKind::Education),
        ));
        self.card(cx, SectionKind::Education, edu.len(), f)
    }

    fn render_skills_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut f = self.variant_controls(cx, SectionKind::Skills);
        let skills = self.doc.skills.active();
        for (i, group) in skills
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) == self.selection.item)
        {
            f.push(self.field(cx, FieldId::SkillName(i), "Category"));
            // A keyword list is the same anatomy as a highlight list: short
            // items whose position is visible, so `Skill 1`, `Skill 2`… label
            // nothing the eye cannot already count.
            let keywords: Vec<FieldId> = (0..group.keywords.len())
                .map(|j| FieldId::SkillKeyword(i, j))
                .collect();
            f.extend(self.highlight_list(cx, "Skills", ListId::SkillKeywords(i), &keywords));
            f.push(Self::wide(self.add_button(
                cx,
                "Add skill",
                ListId::SkillKeywords(i),
            )));
        }
        f.push(Self::wide(
            self.library_picker_button(cx, SectionKind::Skills),
        ));
        self.card(cx, SectionKind::Skills, skills.len(), f)
    }

    fn render_certificates_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut f = self.variant_controls(cx, SectionKind::Certificates);
        let certs = self.doc.certificates.active();
        for i in (0..certs.len()).filter(|i| Some(*i) == self.selection.item) {
            f.push(self.field(cx, FieldId::CertName(i), "Name"));
            f.push(self.field(cx, FieldId::CertIssuer(i), "Issuer"));
            f.push(self.single_date_field(cx, FieldId::CertDate(i), "Date"));
            f.push(self.field(cx, FieldId::CertUrl(i), "URL"));
        }
        f.push(Self::wide(
            self.library_picker_button(cx, SectionKind::Certificates),
        ));
        self.card(cx, SectionKind::Certificates, certs.len(), f)
    }

    fn render_organizations_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut f = self.variant_controls(cx, SectionKind::Organizations);
        let orgs = self.doc.volunteer.active();
        for (i, v) in orgs
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) == self.selection.item)
        {
            f.push(self.field(cx, FieldId::VolPosition(i), "Role"));
            f.push(self.field(cx, FieldId::VolOrg(i), "Organization"));
            f.extend(self.date_fields(cx, FieldId::VolStart(i), FieldId::VolEnd(i), "Still involved"));
            f.push(self.field(cx, FieldId::VolUrl(i), "URL"));
            let highlight_fields: Vec<FieldId> = (0..v.highlights.len())
                .map(|j| FieldId::VolHighlight(i, j))
                .collect();
            if let Some(highlights) = self.highlight_list(
                cx,
                "Highlights",
                ListId::VolHighlights(i),
                &highlight_fields,
            ) {
                f.push(highlights);
            }
            f.push(Self::wide(self.add_button(
                cx,
                "Add highlight",
                ListId::VolHighlights(i),
            )));
        }
        f.push(Self::wide(
            self.library_picker_button(cx, SectionKind::Organizations),
        ));
        self.card(cx, SectionKind::Organizations, orgs.len(), f)
    }

    /// "+ Add" — appends a new custom section (D-9) with a placeholder title
    /// ("New Section") and expands it immediately, so the editable "Section
    /// name" field inside is one click away.
    ///
    /// Not inline-renamed on creation: every other "+" control in this panel
    /// (`add_button`, below) seeds a visible placeholder — "New role", "New
    /// qualification", "New category" — rather than opening straight into an
    /// edit; a brand-new section follows the same convention instead of
    /// inventing focus-on-create plumbing that nothing else here has. Field
    /// input state only exists once `sync_fields` runs on the *next* render
    /// pass (`Root::fields_stale`), so there is no live `TextFieldState` to
    /// focus synchronously from this click handler anyway.
    pub(super) fn add_section_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        Button::new("add-section")
            .quiet()
            .text_color(theme.chip_fg)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.checkpoint();
                let id = this.doc.add_custom_section("New Section");
                this.expanded.insert(SectionKind::Custom(id));
                this.selection = super::root_editor_state::EditorSelection::for_section(
                    &this.doc,
                    SectionKind::Custom(id),
                );
                this.focused_section = SectionKind::Custom(id);
                this.fields_stale = true;
                this.schedule_save(cx);
                cx.notify();
                this.schedule_recompile(window, cx);
            }))
            .icon(IconName::Plus)
            .child("Add")
            .into_any_element()
    }

    /// A small ✕ button removing item `index` from `list`.
    pub(super) fn remove_button(
        &self,
        cx: &mut Context<Self>,
        list: ListId,
        index: usize,
    ) -> AnyElement {
        Button::new(SharedString::from(format!("rm-{list:?}-{index}")))
            .icon_only()
            .icon(IconName::Close)
            .tooltip("Remove this entry")
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.checkpoint();
                list.remove(&mut this.doc, index);
                this.selection.normalize(&this.doc);
                this.schedule_save(cx);
                this.fields_stale = true;
                cx.notify();
                this.schedule_recompile(window, cx);
            }))
            .into_any_element()
    }

    /// A full-width "add" button that appends a new item to `list`.
    pub(super) fn add_button(
        &self,
        cx: &mut Context<Self>,
        label: impl Into<SharedString>,
        list: ListId,
    ) -> AnyElement {
        let theme = *cx.theme();
        let label = label.into();
        Button::new(SharedString::from(format!("add-{list:?}")))
            .chip_dashed(&theme)
            .w_full()
            .mt_1()
            .mb_2()
            // The one dashed affordance that is an accent action rather than a
            // quiet one: adding a row is the field column's primary gesture.
            .text_color(theme.accent)
            .icon(IconName::Plus)
            .child(label)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.checkpoint();
                list.add(&mut this.doc);
                if let Some(section) = match list {
                    ListId::Profiles => Some(SectionKind::Profile),
                    ListId::Work => Some(SectionKind::Work),
                    ListId::Education => Some(SectionKind::Education),
                    ListId::Skills => Some(SectionKind::Skills),
                    ListId::Certificates => Some(SectionKind::Certificates),
                    ListId::Volunteer => Some(SectionKind::Organizations),
                    ListId::CustomEntries(id) => Some(SectionKind::Custom(id)),
                    _ => None,
                } {
                    this.selection = super::root_editor_state::EditorSelection {
                        section,
                        item: Some(super::root_editor_state::item_count(&this.doc, section) - 1),
                    };
                    this.focused_section = section;
                    this.expanded.insert(section);
                }
                this.schedule_save(cx);
                this.fields_stale = true;
                cx.notify();
                this.schedule_recompile(window, cx);
            }))
            .into_any_element()
    }

    /// The inspector body for the selected section. Navigation and contextual
    /// section actions live above it, so this surface only mounts the selected
    /// item's fields and the section's variant timeline.
    pub(super) fn card(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
        count: usize,
        fields: Vec<Field>,
    ) -> AnyElement {
        let theme = *cx.theme();
        let empty = count == 0 && section != SectionKind::Profile;
        div()
            .flex().flex_col().w_full()
            .children(self.render_ats_findings(cx, section))
            .when_some(empty.then(|| Self::section_list(section)).flatten(), |el, list| {
                el.child(
                    div()
                        .mb(px(16.0))
                        .p(px(18.0))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .rounded(theme.radius_md())
                        .bg(theme.elevated)
                        .child(
                            div()
                                .text_style(TextStyle::body())
                                .text_color(theme.text_muted)
                                .child("Nothing here yet."),
                        )
                        // The empty state carries the action rather than
                        // naming where to find it.
                        .child(self.add_button(cx, Self::add_label(section), list)),
                )
            })
            .child(Form::vertical().columns(2).small().children(fields))
            .into_any_element()
    }

    /// The editable box for a field (no label), reused by the form and the
    /// preset/variant name inputs.
    ///
    /// The state lives in [`Root::fields`] and is created by `sync_fields`
    /// before this runs, so a missing entry means the field is not addressable
    /// in the current document — render nothing rather than an input bound to
    /// nowhere.
    /// A control that is not a labelled field — an entry header, an add
    /// button, a picker — laid across both columns so it reads as a divider
    /// between the pairs above and below it rather than as a third field.
    pub(super) fn wide(element: AnyElement) -> Field {
        Field::new().col_span(2).label_indent(false).child(element)
    }

    pub(super) fn field_box(&self, field: FieldId) -> AnyElement {
        let Some(binding) = self.fields.get(&field) else {
            return div().into_any_element();
        };
        // C-1: Small keeps the input's own text at the same `text_sm` step
        // Medium uses (upstream's `input_text_size` maps both the same way)
        // and only trims the box around it — 24px input + 8px padding
        // against Medium's 32px + 14px, the single biggest contributor to
        // the panel's field rows.
        TextField::new(&binding.state).small().into_any_element()
    }

    /// A labeled, editable text field bound to `field`. The value renders as
    /// prose (sans) unless the field is multiline, which reads as résumé
    /// prose proper — mono is reserved for [`Root::field_data`] fields
    /// (design doc §5, L-05).
    pub(super) fn field(
        &self,
        cx: &mut Context<Self>,
        field: FieldId,
        label: impl Into<SharedString>,
    ) -> Field {
        let style = if field.multiline() {
            TextStyle::prose()
        } else {
            TextStyle::body()
        };
        self.field_with_style(cx, field, label, style)
    }

    /// A field whose label carries an icon — the contact rows, where the glyph
    /// says what the value is faster than the word does.
    ///
    /// The style is passed rather than derived: an email and a phone number are
    /// **data** and stay mono (L-05), a location is a place name and does not.
    pub(super) fn field_with_icon(
        &self,
        cx: &mut Context<Self>,
        field: FieldId,
        label: impl Into<SharedString>,
        icon: DockIcon,
        style: TextStyle,
    ) -> Field {
        let theme = *cx.theme();
        let label: SharedString = label.into();
        Field::new()
            .col_span(1)
            // C-1: see `field_with_style` — the same dead description gap,
            // zeroed the same way.
            .gap(px(0.0))
            .label_fn(move |_window, _cx| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .text_style(TextStyle::label())
                    .line_height(FIELD_LABEL_LINE_HEIGHT)
                    .text_color(theme.text_muted)
                    .child(Icon::new(icon).with_size(theme.icon_sm()))
                    .child(label.clone())
            })
            .child(div().text_style(style).child(self.field_box(field)))
    }

    fn field_with_style(
        &self,
        cx: &mut Context<Self>,
        field: FieldId,
        label: impl Into<SharedString>,
        style: TextStyle,
    ) -> Field {
        let theme = *cx.theme();
        let label: SharedString = label.into();
        Field::new()
            // Full width, and this reverses an earlier call. Pairing short
            // fields across two columns bought vertical space back when the
            // panel mounted every entry at once — "one job used to fill a
            // screen, and six of them were an unreadable column". The
            // inspector mounts one entry, so that pressure is gone, and what
            // two columns cost is visible instead: a 148px box truncates
            // "Senior Platform Engineer" while the panel has room to spare.
            //
            // `Start` and `End` keep their pair by asking for `col_span(1)`
            // themselves (`root_dates.rs`) — a month and a year is short by
            // construction, and the two belong on one line.
            .col_span(2)
            // C-1: upstream's `Field` reserves a second internal gap for a
            // `description` row this panel never sets — zeroed explicitly
            // rather than carrying 2px of dead space under every field.
            .gap(px(0.0))
            .label_fn(move |_window, _cx| {
                // P-16: field labels render `text_muted`, never the mockup's
                // literal `#7c8492` (design doc §4).
                div()
                    .text_style(TextStyle::label())
                    .line_height(FIELD_LABEL_LINE_HEIGHT)
                    .text_color(theme.text_muted)
                    .child(label.clone())
            })
            .child(div().text_style(style).child(self.field_box(field)))
    }

    /// An entry subheading with a ✕ remove button and, for block sections, a ★
    /// to save the entry to the vault library.
    /// The star and the cross for the selected entry, for the inspector's own
    /// header.
    ///
    /// `None` when nothing repeatable is selected — Profile's identity is not
    /// an entry and cannot be removed.
    pub(super) fn entry_actions(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
    ) -> Option<AnyElement> {
        let index = self.selection.item?;
        let list = Self::section_list(section)?;
        // Profile's networks and a custom section's entries have no library
        // pool behind them — `save_block_to_library` has nowhere to put one.
        let pooled = !matches!(section, SectionKind::Profile | SectionKind::Custom(_));
        Some(
            div()
                .flex()
                .items_center()
                .flex_none()
                .gap_1()
                .when(pooled, |el| {
                    el.child(
                        Button::new(SharedString::from(format!("star-{section:?}-{index}")))
                            .icon_only()
                            .icon(IconName::Star)
                            .tooltip("Keep this block in the library")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.save_block_to_library(section, index, cx);
                            })),
                    )
                })
                .child(self.remove_button(cx, list, index))
                .into_any_element(),
        )
    }

    /// A "＋ From library" button that opens the picker for `section`.
    pub(super) fn library_picker_button(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
    ) -> AnyElement {
        let count = self.library_count(section);
        // Quiet, not dashed. `Add …` and `From library` were two full-width
        // dashed buttons stacked with identical weight, reading as a choice
        // between equals — but one appends an empty row and the other opens a
        // picker. The dashed affordance stays with the primary gesture.
        Button::new(SharedString::from(format!("fromlib-{section:?}")))
            .quiet()
            .w_full()
            .mb_2()
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.open_library_picker(section, cx);
            }))
            .icon(IconName::Star)
            .child(format!("From library ({count})"))
            .into_any_element()
    }

    /// A "✎ From diary" button on a work entry that opens the diary picker.
    pub(super) fn diary_picker_button(
        &self,
        cx: &mut Context<Self>,
        work_index: usize,
    ) -> AnyElement {
        let theme = *cx.theme();
        let count = self.diary.entries.len();
        Button::new(SharedString::from(format!("fromdiary-{work_index}")))
            .chip_dashed(&theme)
            .w_full()
            .mt_1()
            .mb_2()
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.open_diary_picker(work_index, cx);
            }))
            .icon(DockIcon::Pen)
            .child(format!("From diary ({count})"))
            .into_any_element()
    }
}
