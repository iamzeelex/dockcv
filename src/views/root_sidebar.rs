//! Sidebar section and field form rendering for `Root` (`docs/design/editor.md` §3,
//! section anatomy). Built-in sections live here; a custom section's own anatomy
//! (D-9) is in `root_custom_sections.rs`, which reuses the `card`/`field`/
//! `entry_header`/`add_button` building blocks defined below.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, IntoElement, Pixels, SharedString};

use dockcv_ui_components::{DockIcon, Field, Form, Icon, IconName, Sizable, Tag, TextField, SANS};

use crate::resume::edit::{FieldId, ListId};
use crate::resume::model::SectionKind;
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::Root;

/// C-1 (`docs/design/editor-comfort.md`): upstream's `Field` label inherits
/// `TextStyle::label()`'s 1.45 leading — generous for a heading, wasteful for
/// a one-word field label that never wraps. Tightened at the call site, not
/// in the shared type scale: the 12px `label` step itself is untouched, only
/// the line box around it shrinks. A spacing fix, not a type-size one.
pub(super) const FIELD_LABEL_LINE_HEIGHT: Pixels = px(14.0);

impl Root {
    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        // The document's own order, built-ins and custom sections alike
        // (`ResumeDoc::sections()`) — not a hard-coded six-block list, so a
        // user-added section (D-9) appears here without a separate render
        // path, and reordering (B6b, not built by this task) has exactly one
        // place to hook into.
        let mut cards: Vec<AnyElement> = Vec::new();
        for kind in self.doc.sections() {
            cards.push(self.render_section(cx, kind));
        }

        div()
            .flex()
            .flex_col()
            // Fill the panel, do not measure to content. Without `w_full()` a
            // flex item takes its content's width, so the cards sat in a column
            // of their own and the rest of the panel showed as a second, empty
            // one. A fixed `w()` would fight the drag instead; `w_full()` fills
            // whatever the panel currently is.
            .h_full()
            .w_full()
            .min_w_0()
            // Sections panel shares the window's own background (design doc
            // §4) — it reads as a region of the app, not a raised panel; only
            // the border-right hairline separates it from the preview.
            .bg(theme.background)
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pl(px(22.0))
                    .pr(px(22.0))
                    .pt(px(18.0))
                    .pb(px(12.0))
                    .child(
                        // "SECTIONS" kicker — panel chrome, not data, so sans
                        // rather than the mockup's mono (design doc §5 flag).
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
                    .id("sections-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(16.0))
                    .pb(px(16.0))
                    .children(cards),
            )
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
        // Identity first and full width: the name and the title are the
        // document's headline, not two fields among seven. The four contact
        // rows below them pair off, and each carries the glyph that says what
        // it is faster than its label does.
        f.push(self.field(cx, FieldId::Name, "Name").col_span(2));
        f.push(self.field(cx, FieldId::Label, "Title").col_span(2));
        f.push(self.field_with_icon(cx, FieldId::Email, "Email", DockIcon::Mail, TextStyle::code()));
        f.push(self.field_with_icon(cx, FieldId::Phone, "Phone", DockIcon::Phone, TextStyle::code()));
        f.push(self.field_with_icon(cx, FieldId::Location, "Location", DockIcon::MapPin, TextStyle::body()));
        f.push(self.field_with_icon(cx, FieldId::Url, "Website", DockIcon::Link, TextStyle::code()));
        f.push(self.field(cx, FieldId::Summary, "Summary"));
        let profiles = self.doc.profile.active().profiles.len();
        for i in 0..profiles {
            f.push(Self::wide(self.entry_header(cx, format!("Profile {}", i + 1), ListId::Profiles, i, None)));
            f.push(self.field(cx, FieldId::ProfileNetwork(i), "Network"));
            f.push(self.field(cx, FieldId::ProfileUsername(i), "Username"));
            f.push(self.field(cx, FieldId::ProfileUrl(i), "URL"));
        }
        f.push(Self::wide(self.add_button(cx, "Add profile", ListId::Profiles)));
        self.card(cx, SectionKind::Profile, "Profile", profiles, f, None)
    }

    fn render_work_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut f = self.variant_controls(cx, SectionKind::Work);
        let work = self.doc.work.active();
        for (i, w) in work.iter().enumerate() {
            f.push(Self::wide(self.entry_header(
                cx,
                format!("Entry {}", i + 1),
                ListId::Work,
                i,
                Some(SectionKind::Work),
            )));
            // Ordered so the grid pairs what belongs together: role beside
            // employer, start beside end. Location used to sit between them and
            // pushed `End` onto a row of its own.
            f.push(self.field(cx, FieldId::WorkPosition(i), "Position"));
            f.push(self.field(cx, FieldId::WorkName(i), "Company"));
            f.extend(self.date_fields(cx, FieldId::WorkStart(i), FieldId::WorkEnd(i)));
            f.push(self.field(cx, FieldId::WorkLocation(i), "Location"));
            f.push(self.field(cx, FieldId::WorkSummary(i), "Summary"));
            // C-5: one "Highlights" list, not a `Highlight 1`/`Highlight 2`…
            // row each.
            let highlight_fields: Vec<FieldId> =
                (0..w.highlights.len()).map(|j| FieldId::WorkHighlight(i, j)).collect();
            if let Some(highlights) =
                self.highlight_list(cx, "Highlights", ListId::WorkHighlights(i), &highlight_fields)
            {
                f.push(highlights);
            }
            f.push(Self::wide(self.add_button(cx, "Add highlight", ListId::WorkHighlights(i))));
            f.push(Self::wide(self.diary_picker_button(cx, i)));
        }
        f.push(Self::wide(self.add_button(cx, "Add work entry", ListId::Work)));
        f.push(Self::wide(self.library_picker_button(cx, SectionKind::Work)));
        self.card(
            cx,
            SectionKind::Work,
            "Work Experience",
            work.len(),
            f,
            None,
        )
    }

    fn render_education_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut f = self.variant_controls(cx, SectionKind::Education);
        let edu = self.doc.education.active();
        for (i, entry) in edu.iter().enumerate() {
            f.push(Self::wide(self.entry_header(
                cx,
                format!("Entry {}", i + 1),
                ListId::Education,
                i,
                Some(SectionKind::Education),
            )));
            f.push(self.field(cx, FieldId::EduStudyType(i), "Degree"));
            f.push(self.field(cx, FieldId::EduInstitution(i), "Institution"));
            f.extend(self.date_fields(cx, FieldId::EduStart(i), FieldId::EduEnd(i)));
            f.push(self.field(cx, FieldId::EduUrl(i), "URL"));
            let highlight_fields: Vec<FieldId> =
                (0..entry.highlights.len()).map(|j| FieldId::EduHighlight(i, j)).collect();
            if let Some(highlights) =
                self.highlight_list(cx, "Highlights", ListId::EduHighlights(i), &highlight_fields)
            {
                f.push(highlights);
            }
            f.push(Self::wide(self.add_button(cx, "Add highlight", ListId::EduHighlights(i))));
        }
        f.push(Self::wide(self.add_button(cx, "Add education entry", ListId::Education)));
        f.push(Self::wide(self.library_picker_button(cx, SectionKind::Education)));
        self.card(cx, SectionKind::Education, "Education", edu.len(), f, None)
    }

    fn render_skills_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut f = self.variant_controls(cx, SectionKind::Skills);
        let skills = self.doc.skills.active();
        for (i, group) in skills.iter().enumerate() {
            f.push(Self::wide(self.entry_header(
                cx,
                format!("Group {}", i + 1),
                ListId::Skills,
                i,
                Some(SectionKind::Skills),
            )));
            f.push(self.field(cx, FieldId::SkillName(i), "Category"));
            // A keyword list is the same anatomy as a highlight list: short
            // items whose position is visible, so `Skill 1`, `Skill 2`… label
            // nothing the eye cannot already count.
            let keywords: Vec<FieldId> = (0..group.keywords.len())
                .map(|j| FieldId::SkillKeyword(i, j))
                .collect();
            f.extend(self.highlight_list(cx, "Skills", ListId::SkillKeywords(i), &keywords));
            f.push(Self::wide(self.add_button(cx, "Add skill", ListId::SkillKeywords(i))));
        }
        f.push(Self::wide(self.add_button(cx, "Add skill group", ListId::Skills)));
        f.push(Self::wide(self.library_picker_button(cx, SectionKind::Skills)));
        self.card(cx, SectionKind::Skills, "Skills", skills.len(), f, None)
    }

    fn render_certificates_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut f = self.variant_controls(cx, SectionKind::Certificates);
        let certs = self.doc.certificates.active();
        for i in 0..certs.len() {
            f.push(Self::wide(self.entry_header(
                cx,
                format!("Entry {}", i + 1),
                ListId::Certificates,
                i,
                Some(SectionKind::Certificates),
            )));
            f.push(self.field(cx, FieldId::CertName(i), "Name"));
            f.push(self.field(cx, FieldId::CertIssuer(i), "Issuer"));
            f.push(self.single_date_field(cx, FieldId::CertDate(i), "Date"));
            f.push(self.field(cx, FieldId::CertUrl(i), "URL"));
        }
        f.push(Self::wide(self.add_button(cx, "Add certificate", ListId::Certificates)));
        f.push(Self::wide(self.library_picker_button(cx, SectionKind::Certificates)));
        self.card(
            cx,
            SectionKind::Certificates,
            "Certifications",
            certs.len(),
            f,
            None,
        )
    }

    fn render_organizations_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut f = self.variant_controls(cx, SectionKind::Organizations);
        let orgs = self.doc.volunteer.active();
        for (i, v) in orgs.iter().enumerate() {
            f.push(Self::wide(self.entry_header(
                cx,
                format!("Entry {}", i + 1),
                ListId::Volunteer,
                i,
                Some(SectionKind::Organizations),
            )));
            f.push(self.field(cx, FieldId::VolPosition(i), "Role"));
            f.push(self.field(cx, FieldId::VolOrg(i), "Organization"));
            f.extend(self.date_fields(cx, FieldId::VolStart(i), FieldId::VolEnd(i)));
            let highlight_fields: Vec<FieldId> =
                (0..v.highlights.len()).map(|j| FieldId::VolHighlight(i, j)).collect();
            if let Some(highlights) =
                self.highlight_list(cx, "Highlights", ListId::VolHighlights(i), &highlight_fields)
            {
                f.push(highlights);
            }
            f.push(Self::wide(self.add_button(cx, "Add highlight", ListId::VolHighlights(i))));
        }
        f.push(Self::wide(self.add_button(cx, "Add organization", ListId::Volunteer)));
        f.push(Self::wide(self.library_picker_button(cx, SectionKind::Organizations)));
        self.card(
            cx,
            SectionKind::Organizations,
            "Organizations",
            orgs.len(),
            f,
            None,
        )
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
        let theme = cx.theme().clone();
        div()
            .id("add-section")
            .cursor_pointer()
            .font_family(SANS)
            .text_size(px(12.5))
            .text_color(theme.chip_fg)
            .hover(|s| s.text_color(theme.accent))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.checkpoint();
                let id = this.doc.add_custom_section("New Section");
                this.expanded.insert(SectionKind::Custom(id));
                this.fields_stale = true;
                this.schedule_save(cx);
                cx.notify();
                this.schedule_recompile(window, cx);
            }))
            .child("+ Add")
            .into_any_element()
    }

    /// A small ✕ button removing item `index` from `list`.
    pub(super) fn remove_button(
        &self,
        cx: &mut Context<Self>,
        list: ListId,
        index: usize,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        div()
            .id(SharedString::from(format!("rm-{list:?}-{index}")))
            .px_1()
            .rounded_md()
            .text_style(TextStyle::meta())
            .text_color(theme.text_muted)
            .cursor_pointer()
            .hover(|s| s.text_color(theme.danger))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.checkpoint();
                list.remove(&mut this.doc, index);
                this.schedule_save(cx);
                this.fields_stale = true;
                cx.notify();
                this.schedule_recompile(window, cx);
            }))
            .child(Icon::new(IconName::Close).with_size(px(11.0)))
            .into_any_element()
    }

    /// A full-width "add" button that appends a new item to `list`.
    pub(super) fn add_button(
        &self,
        cx: &mut Context<Self>,
        label: impl Into<SharedString>,
        list: ListId,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let label = label.into();
        div()
            .id(SharedString::from(format!("add-{list:?}")))
            .mt_1()
            .mb_2()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_dashed()
            .border_color(theme.border)
            .text_style(TextStyle::control())
            .text_color(theme.accent)
            .cursor_pointer()
            .hover(|s| s.bg(theme.hover))
            .child(format!("＋ {label}"))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.checkpoint();
                list.add(&mut this.doc);
                this.schedule_save(cx);
                this.fields_stale = true;
                cx.notify();
                this.schedule_recompile(window, cx);
            }))
            .into_any_element()
    }

    /// A section card: expanded (drag handle, status dot, title, collapse
    /// chevron, variant switcher and fields) or collapsed (drag handle,
    /// title, variant-name chip, entry count, expand chevron) — anatomy per
    /// design doc §3.
    ///
    /// `section` is both the card's identity (keys `Root::expanded` and the
    /// element id, via its `Debug` form — stable even while `title` is being
    /// edited, which matters for a custom section's user-editable title) and
    /// the key into every `ResumeDoc` accessor `card` needs (variant names,
    /// active variant). `extra`, when present, renders a control after the
    /// title and before the chevron in both states — today only a custom
    /// section's "···" delete menu (`root_custom_sections.rs`); no built-in
    /// section passes one.
    ///
    /// `Card` (`dockcv-ui-components`) was the natural container for this,
    /// but its `RenderOnce` currently replaces its own base style
    /// (background/border/radius/padding) with the caller's via a raw
    /// `*el.style() = self.style` assignment instead of a merge — see
    /// `crates/ui-components/src/components/card.rs`, `impl RenderOnce for
    /// Card`. Any `Styled` call a caller makes (or the mere presence of an
    /// empty, un-touched `StyleRefinement`) wipes the variant's bg/border/
    /// radius, so a `Card::new().elevated()` currently paints nothing. That
    /// needs a `crates/ui-components` fix (call `.refine()`, matching
    /// `StyledExt::refine_style` used correctly elsewhere in the same
    /// crate family) before this screen can safely depend on it — flagged
    /// rather than silently worked around by editing that crate.
    pub(super) fn card(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
        title: impl Into<SharedString>,
        count: usize,
        fields: Vec<Field>,
        extra: Option<AnyElement>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let expanded = self.expanded.contains(&section);
        let title: SharedString = title.into();
        // Keyboard navigation cursor (`FocusNextSection`/`FocusPrevSection`,
        // `docs/design/editor.md`'s "Discoverability"/P-17): the same accent
        // border a focused `TextField` draws, borrowed here since the mockup
        // never drew a keyboard-focus state for a section card.
        let keyboard_focused = self.focused_section == section;
        let card_border = if keyboard_focused {
            theme.accent
        } else {
            theme.border
        };

        // Design doc §10, open question: the mockup suggests (but doesn't
        // confirm) the chip is suppressed for a section with only one
        // variant — Education (single variant) carries none while Work
        // ("Detailed") and Skills ("Infra-heavy") do. Implemented on that
        // reading, since it's groundable in the model.
        let variant_chip = (self.doc.variant_names(section).len() > 1)
            .then(|| self.doc.variant_name(section).clone());

        let drag_handle = self.render_drag_handle(cx, section, title.clone());
        let heading = self.render_section_heading(cx, section, title);
        let rename_button = self.rename_button(cx, section);

        let chevron = Icon::new(if expanded {
            IconName::ChevronUp
        } else {
            IconName::ChevronDown
        })
        .with_size(px(12.0))
        .text_color(theme.text_subtle);

        if expanded {
            let mut header = div()
                .id(SharedString::from(format!("card-header-{section:?}")))
                .flex()
                .items_center()
                .gap(px(10.0))
                .mb(px(14.0))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    if !this.expanded.insert(section) {
                        this.expanded.remove(&section);
                    }
                    cx.notify();
                }))
                .child(drag_handle)
                .child(heading)
                .child(rename_button)
                .child(self.visibility_button(cx, section))
                .children(self.render_trim_chip(cx, section));
            if let Some(extra) = extra {
                header = header.child(extra);
            }
            header = header.child(chevron);

            let card = div()
                .id(SharedString::from(format!("card-{section:?}")))
                .flex()
                .flex_col()
                .mb(px(9.0))
                .rounded(px(11.0))
                .bg(theme.elevated)
                .border_1()
                .border_color(card_border)
                .px(px(15.0))
                .py(px(14.0))
                .child(header)
                // C-1: explicit `small()` tightens the Form's own row/column
                // gap (8px/24px → 6px/18px) rather than the implicit Medium
                // default upstream falls back to when no size is set.
                .child(Form::vertical().columns(2).small().children(fields));
            self.section_drop_target(cx, section, card)
                .into_any_element()
        } else {
            let mut row = div()
                .id(SharedString::from(format!("card-{section:?}")))
                .flex()
                .items_center()
                .gap(px(10.0))
                .mb(px(9.0))
                .rounded(px(11.0))
                .border_1()
                .border_color(card_border)
                .px(px(15.0))
                .py(px(13.0))
                .cursor_pointer()
                .hover(|s| s.border_color(theme.border_strong))
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    if !this.expanded.insert(section) {
                        this.expanded.remove(&section);
                    }
                    cx.notify();
                }))
                .child(drag_handle)
                .child(heading)
                .child(rename_button);

            if let Some(name) = variant_chip {
                row = row.child(
                    Tag::custom(theme.chip_bg, theme.chip_fg, theme.chip_bg)
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(6.0))
                        .text_style(TextStyle::chip())
                        .child(name),
                );
            }

            if count > 0 {
                row = row.child(
                    div()
                        .text_style(TextStyle::meta())
                        .text_color(theme.text_subtle)
                        .child(format!("{count}")),
                );
            }

            if let Some(extra) = extra {
                row = row.child(extra);
            }

            // Collapsed rows carry the visibility toggle too. It was only on
            // the expanded card, so hiding a section meant expanding it first
            // — and a hidden section is exactly the one you have no reason to
            // open.
            row = row.child(self.visibility_button(cx, section));

            let row = row.child(chevron);
            self.section_drop_target(cx, section, row)
                .into_any_element()
        }
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
        let theme = cx.theme().clone();
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
                    .child(Icon::new(icon).with_size(px(12.0)))
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
        let theme = cx.theme().clone();
        let label: SharedString = label.into();
        Field::new()
            // Prose takes the full width; a short value shares its line with
            // the field beside it. `Start` and `End` are a pair and cost one
            // line between them, where before they cost two — one job used to
            // fill a screen, and six of them were an unreadable column.
            .col_span(if field.multiline() { 2 } else { 1 })
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
    pub(super) fn entry_header(
        &self,
        cx: &mut Context<Self>,
        label: impl Into<SharedString>,
        list: ListId,
        index: usize,
        library_section: Option<SectionKind>,
    ) -> AnyElement {
        let theme = cx.theme().clone();

        let mut controls = div().flex().items_center().gap_1();
        if let Some(section) = library_section {
            controls = controls.child(
                div()
                    .id(SharedString::from(format!("star-{section:?}-{index}")))
                    .px_1()
                    .rounded_md()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme.accent))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.save_block_to_library(section, index, cx);
                    }))
                    .child(Icon::new(IconName::Star).with_size(px(11.0))),
            );
        }
        controls = controls.child(self.remove_button(cx, list, index));

        div()
            .flex()
            .items_center()
            .justify_between()
            .mt_2()
            .mb_1()
            .child(div().text_xs().text_color(theme.accent).child(label.into()))
            .child(controls)
            .into_any_element()
    }

    /// A "＋ From library" button that opens the picker for `section`.
    pub(super) fn library_picker_button(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let count = self.library_count(section);
        div()
            .id(SharedString::from(format!("fromlib-{section:?}")))
            .mt_1()
            .mb_2()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_dashed()
            .border_color(theme.border)
            .text_xs()
            .text_color(theme.text_muted)
            .cursor_pointer()
            .hover(|s| s.text_color(theme.accent))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.open_library_picker(section, cx);
            }))
            .child(format!("★ From library ({count})"))
            .into_any_element()
    }

    /// A "✎ From diary" button on a work entry that opens the diary picker.
    pub(super) fn diary_picker_button(
        &self,
        cx: &mut Context<Self>,
        work_index: usize,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let count = self.diary.entries.len();
        div()
            .id(SharedString::from(format!("fromdiary-{work_index}")))
            .mt_1()
            .mb_2()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_dashed()
            .border_color(theme.border)
            .text_xs()
            .text_color(theme.text_muted)
            .cursor_pointer()
            .hover(|s| s.text_color(theme.accent))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.open_diary_picker(work_index, cx);
            }))
            .child(format!("✎ From diary ({count})"))
            .into_any_element()
    }
}
