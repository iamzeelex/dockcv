//! The controls inside the layout rail.
//!
//! One row per decision, in the groups `root_layout_rail.rs` arranges them
//! into. They are here rather than beside the rail's own chrome because they
//! are what changes: every layout feature adds a row, and the panel that holds
//! them has not changed since it was written.
//!
//! Two shapes repeat. `skills_pick` is the dropdown almost every row is —
//! label, current value, a menu of the rest — and `size_row` is the stepper,
//! for the rows whose value is a quantity rather than a choice from a set.
//! Reach for one of those before writing a third.

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, IntoElement, SharedString};

use dockcv_ui_components::{
    Button, ButtonExt, ButtonGroup, DropdownMenu, IconName, PopupMenuItem, Selectable, Slider,
};

use crate::resume::model::{
    BulletGlyph, CategoryMark, ContactLayout, DateFormat, DocumentFont, Emphasis, EntryLayout,
    HeaderAlign, HeaderLayout, HeadingCase, HeadingLayout, HeadingStyle, LayoutSettings, MetaOrder,
    MetaPosition, PageSize, ResumeDoc, RowSpacing, SkillSeparator, SkillsLayout, SkillsStyle,
    TypeSizes,
};
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::root::Root;

/// The page sizes, in the order the segmented control draws them — which is
/// also the order `ButtonGroup` reports a click by index.
const PAGE_SIZES: [PageSize; 2] = [PageSize::Letter, PageSize::A4];

impl Root {
    /// How the Skills section is set — five decisions, because it is the
    /// densest text on a CV and the one most likely to cost a page.
    ///
    /// Notably **not** a proficiency control. The model stores no level, and a
    /// row of bars assembled from nothing is the invented metric US-14 exists
    /// to forbid — the reference layouts that offer one are reading a field
    /// their model has and ours does not. Where a person wants to say it, they
    /// type it: `Expert: Python` is a keyword like any other.
    pub(super) fn skills_rows(
        &self,
        cx: &mut Context<Self>,
        skills: SkillsLayout,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(
                self.skills_pick(
                    cx,
                    "Layout",
                    "layout-skills-style",
                    skills.style.label(),
                    SkillsStyle::ALL
                        .iter()
                        .map(|s| (s.label(), *s == skills.style, *s))
                        .collect(),
                    |doc, style| doc.layout.skills.style = style,
                ),
            )
            .child(
                self.skills_pick(
                    cx,
                    "Separator",
                    "layout-skills-sep",
                    skills.separator.label(),
                    SkillSeparator::ALL
                        .iter()
                        .map(|s| (s.label(), *s == skills.separator, *s))
                        .collect(),
                    |doc, sep| doc.layout.skills.separator = sep,
                ),
            )
            .child(
                self.skills_pick(
                    cx,
                    "Category",
                    "layout-skills-mark",
                    skills.mark.label(),
                    CategoryMark::ALL
                        .iter()
                        .map(|m| (m.label(), *m == skills.mark, *m))
                        .collect(),
                    |doc, mark| doc.layout.skills.mark = mark,
                ),
            )
            .child(
                self.skills_pick(
                    cx,
                    "Row spacing",
                    "layout-skills-spacing",
                    skills.spacing.label(),
                    RowSpacing::ALL
                        .iter()
                        .map(|s| (s.label(), *s == skills.spacing, *s))
                        .collect(),
                    |doc, spacing| doc.layout.skills.spacing = spacing,
                ),
            )
            .child(self.skills_pick(
                cx,
                "Row marker",
                "layout-skills-bullets",
                if skills.bullets { "Bullet" } else { "None" },
                vec![
                    ("None", !skills.bullets, false),
                    ("Bullet", skills.bullets, true),
                ],
                |doc, bullets| doc.layout.skills.bullets = bullets,
            ))
    }

    /// A named subsection inside a rail group.
    ///
    /// Without it the Sections group was five bare labels — `Layout`,
    /// `Separator`, `Category` — and nothing said they were about Skills.
    /// `Bubbles` is an obvious answer to a question the panel never asked.
    pub(super) fn rail_subsection(
        &self,
        cx: &mut Context<Self>,
        title: &'static str,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        div()
            .flex()
            .items_center()
            .gap_2()
            .pt(px(2.0))
            .child(
                div()
                    .text_style(TextStyle::chip())
                    .text_color(theme.text)
                    .child(title),
            )
            .child(div().flex_1().h(px(1.0)).bg(theme.border))
    }

    /// The block above the first section: the name, the title under it, and
    /// the contact details.
    ///
    /// No icon control. Drawing a glyph before each detail needs an icon font
    /// inside the document, which is its own piece of work rather than a
    /// fourth dropdown — see `HeaderLayout`.
    pub(super) fn header_rows(
        &self,
        cx: &mut Context<Self>,
        header: HeaderLayout,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(
                self.skills_pick(
                    cx,
                    "Alignment",
                    "layout-header-align",
                    header.align.label(),
                    HeaderAlign::ALL
                        .iter()
                        .map(|a| (a.label(), *a == header.align, *a))
                        .collect(),
                    |doc, v| doc.layout.header.align = v,
                ),
            )
            .child(
                self.skills_pick(
                    cx,
                    "Contact details",
                    "layout-header-contacts",
                    header.contacts.label(),
                    ContactLayout::ALL
                        .iter()
                        .map(|c| (c.label(), *c == header.contacts, *c))
                        .collect(),
                    |doc, v| doc.layout.header.contacts = v,
                ),
            )
            // Only when they share a line. On the two shapes that give each
            // detail its own row there is nothing between them, and a control
            // that changes nothing teaches the user something untrue about the
            // product (E-43).
            .children(header.contacts.uses_separator().then(|| {
                self.skills_pick(
                    cx,
                    "Separator",
                    "layout-header-sep",
                    header.separator.label(),
                    SkillSeparator::ALL
                        .iter()
                        .map(|s| (s.label(), *s == header.separator, *s))
                        .collect(),
                    |doc, v| doc.layout.header.separator = v,
                )
            }))
    }

    /// The bar above each section — the one piece of the layout that repeats
    /// on every section of every page, which is why it gets its own
    /// subsection rather than living under Entries.
    ///
    /// No icon control, for the reason `header_rows` gives: a glyph inside the
    /// document needs an icon font in the Typst source, which is a piece of
    /// work rather than a fourth dropdown.
    pub(super) fn heading_rows(
        &self,
        cx: &mut Context<Self>,
        headings: HeadingLayout,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(
                self.skills_pick(
                    cx,
                    "Style",
                    "layout-heading-style",
                    headings.style.label(),
                    HeadingStyle::ALL
                        .iter()
                        .map(|s| (s.label(), *s == headings.style, *s))
                        .collect(),
                    |doc, v| doc.layout.headings.style = v,
                ),
            )
            .child(
                self.skills_pick(
                    cx,
                    "Capitalization",
                    "layout-heading-case",
                    headings.case.label(),
                    HeadingCase::ALL
                        .iter()
                        .map(|c| (c.label(), *c == headings.case, *c))
                        .collect(),
                    |doc, v| doc.layout.headings.case = v,
                ),
            )
            // Hidden for the one style whose words have nowhere to go: the
            // rule takes whatever they leave, so they are always at the left.
            // A control that changes nothing teaches something untrue (E-43).
            .children(headings.style.can_align().then(|| {
                self.skills_pick(
                    cx,
                    "Alignment",
                    "layout-heading-align",
                    headings.align.label(),
                    HeaderAlign::ALL
                        .iter()
                        .map(|a| (a.label(), *a == headings.align, *a))
                        .collect(),
                    |doc, v| doc.layout.headings.align = v,
                )
            }))
    }

    /// How a dated entry is set — a job, a degree, a certificate. The other
    /// half of `Sections`, and the one that repeats most on a CV.
    pub(super) fn entry_rows(
        &self,
        cx: &mut Context<Self>,
        entries: EntryLayout,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(
                self.skills_pick(
                    cx,
                    "Date & place",
                    "layout-entry-meta-pos",
                    entries.meta_position.label(),
                    MetaPosition::ALL
                        .iter()
                        .map(|m| (m.label(), *m == entries.meta_position, *m))
                        .collect(),
                    |doc, v| doc.layout.entries.meta_position = v,
                ),
            )
            .child(
                self.skills_pick(
                    cx,
                    "Order",
                    "layout-entry-meta-order",
                    entries.meta_order.label(),
                    MetaOrder::ALL
                        .iter()
                        .map(|m| (m.label(), *m == entries.meta_order, *m))
                        .collect(),
                    |doc, v| doc.layout.entries.meta_order = v,
                ),
            )
            .child(
                self.skills_pick(
                    cx,
                    "Subtitle",
                    "layout-entry-subtitle",
                    entries.subtitle.label(),
                    Emphasis::ALL
                        .iter()
                        .map(|e| (e.label(), *e == entries.subtitle, *e))
                        .collect(),
                    |doc, v| doc.layout.entries.subtitle = v,
                ),
            )
            .child(
                self.skills_pick(
                    cx,
                    "Date & place style",
                    "layout-entry-meta-style",
                    entries.meta.label(),
                    Emphasis::ALL
                        .iter()
                        .map(|e| (e.label(), *e == entries.meta, *e))
                        .collect(),
                    |doc, v| doc.layout.entries.meta = v,
                ),
            )
            .child(
                self.skills_pick(
                    cx,
                    "Bullets",
                    "layout-entry-bullet",
                    entries.bullet.label(),
                    BulletGlyph::ALL
                        .iter()
                        .map(|b| (b.label(), *b == entries.bullet, *b))
                        .collect(),
                    |doc, v| doc.layout.entries.bullet = v,
                ),
            )
            .child(self.skills_pick(
                cx,
                "Body",
                "layout-entry-indent",
                if entries.indent_body {
                    "Indented"
                } else {
                    "Full width"
                },
                vec![
                    ("Full width", !entries.indent_body, false),
                    ("Indented", entries.indent_body, true),
                ],
                |doc, v| doc.layout.entries.indent_body = v,
            ))
    }

    /// One labelled dropdown, since these groups need eleven of them and
    /// eleven hand-written copies is eleven places for one of them to drift.
    fn skills_pick<T: Copy + 'static>(
        &self,
        cx: &mut Context<Self>,
        label: &'static str,
        id: &'static str,
        current: &str,
        options: Vec<(&'static str, bool, T)>,
        apply: fn(&mut ResumeDoc, T),
    ) -> impl IntoElement {
        let _theme = *cx.theme();
        let root = cx.weak_entity();
        let current = SharedString::from(current.to_string());
        div()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .child(self.rail_label(cx, label))
            .child(
                Button::new(id)
                    .selector_inline()
                    .w_full()
                    .label(current)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        for (text, checked, value) in options.clone() {
                            let root = root.clone();
                            menu = menu.item(PopupMenuItem::new(text).checked(checked).on_click(
                                move |_ev, window, cx| {
                                    let _ = root.update(cx, |this, cx| {
                                        apply(&mut this.doc, value);
                                        this.after_layout_change(window, cx);
                                    });
                                },
                            ));
                        }
                        menu
                    }),
            )
    }

    /// The font picker — the control that used to be a dead "Template" pill.
    ///
    /// It is not a template chooser wearing a different label: until this
    /// existed the compiler had four families and none of them were the app's
    /// own, so every CV was Typst's default serif and a sans-serif résumé was
    /// not expressible. Font *is* the first real difference between templates,
    /// so this is the honest version of that row.
    pub(super) fn font_row(
        &self,
        cx: &mut Context<Self>,
        active: DocumentFont,
    ) -> impl IntoElement {
        let _theme = *cx.theme();
        let root = cx.weak_entity();
        div()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(self.rail_label(cx, "Font"))
            .child(
                Button::new("layout-font")
                    .selector_inline()
                    .w_full()
                    .label(active.label())
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        for font in DocumentFont::ALL {
                            let root = root.clone();
                            menu = menu.item(PopupMenuItem::new(font.label()).on_click(
                                move |_ev, window, cx| {
                                    let _ = root.update(cx, |this, cx| {
                                        this.doc.layout.font = font;
                                        this.after_layout_change(window, cx);
                                    });
                                },
                            ));
                        }
                        menu
                    }),
            )
    }

    /// How every date in the document prints.
    ///
    /// Each menu item shows its **worked example** beside the pattern, the
    /// way FlowCV's does — `DD MMM YYYY` tells you the shape only if you
    /// already know the notation, while `08 Aug 2026` just shows it.
    pub(super) fn date_format_row(
        &self,
        cx: &mut Context<Self>,
        active: DateFormat,
    ) -> impl IntoElement {
        let _theme = *cx.theme();
        let root = cx.weak_entity();
        div()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(self.rail_label(cx, "Date format"))
            .child(
                Button::new("layout-date-format")
                    .selector_inline()
                    .w_full()
                    .label(active.example())
                    .tooltip("How dates print. What you type stays as you typed it.")
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        for format in DateFormat::ALL {
                            let root = root.clone();
                            menu = menu.item(
                                PopupMenuItem::new(format!(
                                    "{}     {}",
                                    format.label(),
                                    format.example()
                                ))
                                .on_click(
                                    move |_ev, window, cx| {
                                        let _ = root.update(cx, |this, cx| {
                                            this.doc.layout.date_format = format;
                                            this.after_layout_change(window, cx);
                                        });
                                    },
                                ),
                            );
                        }
                        menu
                    }),
            )
    }

    pub(super) fn page_size_row(
        &self,
        cx: &mut Context<Self>,
        active: PageSize,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(self.rail_label(cx, "Page size"))
            .child(
                ButtonGroup::new("page-size")
                    .outline()
                    .w_full()
                    .children(PAGE_SIZES.map(|size| {
                        Button::new(match size {
                            PageSize::Letter => "page-size-letter",
                            PageSize::A4 => "page-size-a4",
                        })
                        .toolbar()
                        .flex_1()
                        .selected(size == active)
                        .label(match size {
                            PageSize::Letter => "Letter",
                            PageSize::A4 => "A4",
                        })
                    }))
                    .on_click(cx.listener(move |this, clicked: &Vec<usize>, window, cx| {
                        if let Some(size) = clicked.first().and_then(|i| PAGE_SIZES.get(*i)) {
                            this.doc.layout.page_size = *size;
                            this.after_layout_change(window, cx);
                        }
                    })),
            )
    }

    pub(super) fn margins_row(
        &self,
        cx: &mut Context<Self>,
        layout: &LayoutSettings,
    ) -> impl IntoElement {
        let readout = if layout.margins.is_uniform() {
            format_margin(layout.margins.x_mm, layout.page_size)
        } else {
            // An asymmetric page cannot be described by one number, so it says
            // so rather than showing one edge and implying the others match.
            "mixed — drag to even out".to_string()
        };

        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(self.rail_label(cx, "Margins"))
            .children(self.margin_slider.as_ref().map(Slider::new))
            .child(self.rail_readout(cx, readout))
    }

    pub(super) fn text_scale_row(
        &self,
        cx: &mut Context<Self>,
        layout: &LayoutSettings,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(self.rail_label(cx, "Text scale"))
            .children(self.scale_slider.as_ref().map(Slider::new))
            .child(self.rail_readout(cx, format!("{}%", layout.text_scale_pct)))
    }

    /// The four sizes a CV's hierarchy is made of.
    ///
    /// Steppers rather than dropdowns because the value is a quantity, not a
    /// choice from a set — and half a point at a time is the granularity that
    /// decides whether a name looks set or shouted.
    ///
    /// Each readout is the size the element is **actually set at**, not the
    /// offset stored in the file. That is the whole reason offsets are what is
    /// stored: turning Text scale down moves all four readouts together, which
    /// says "these follow the base" without a word of explanation.
    pub(super) fn size_rows(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(self.size_row(cx, "Name", |s| s.name_pt, |s, v| s.name_pt = v))
            .child(self.size_row(cx, "Title", |s| s.title_pt, |s, v| s.title_pt = v))
            .child(self.size_row(cx, "Headings", |s| s.heading_pt, |s, v| s.heading_pt = v))
            .child(self.size_row(cx, "Entry title", |s| s.entry_pt, |s, v| s.entry_pt = v))
    }

    fn size_row(
        &self,
        cx: &mut Context<Self>,
        label: &'static str,
        read: fn(&TypeSizes) -> f32,
        write: fn(&mut TypeSizes, f32),
    ) -> impl IntoElement {
        let layout = self.doc.layout;
        let resolved = TypeSizes::resolve(layout.base_size_pt(), read(&layout.sizes));
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(div().flex_1().min_w_0().child(self.rail_label(cx, label)))
            .child(self.size_step(cx, label, false, read, write))
            .child(
                div()
                    .flex_none()
                    .w(px(40.0))
                    .flex()
                    .justify_center()
                    .text_style(TextStyle::meta())
                    .text_color(cx.theme().text_subtle)
                    .child(format!("{}pt", fmt_pt(resolved))),
            )
            .child(self.size_step(cx, label, true, read, write))
    }

    fn size_step(
        &self,
        cx: &mut Context<Self>,
        label: &'static str,
        up: bool,
        read: fn(&TypeSizes) -> f32,
        write: fn(&mut TypeSizes, f32),
    ) -> impl IntoElement {
        let step = if up {
            TypeSizes::STEP_PT
        } else {
            -TypeSizes::STEP_PT
        };
        Button::new(SharedString::from(format!(
            "size-{label}-{}",
            if up { "up" } else { "down" }
        )))
        .icon_only()
        .icon(if up { IconName::Plus } else { IconName::Minus })
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            let (lo, hi) = TypeSizes::DELTA_RANGE;
            let next = (read(&this.doc.layout.sizes) + step).clamp(lo, hi);
            write(&mut this.doc.layout.sizes, next);
            this.after_layout_change(window, cx);
        }))
    }

    fn rail_label(&self, cx: &mut Context<Self>, text: &'static str) -> impl IntoElement {
        div()
            .text_style(TextStyle::label())
            .text_color(cx.theme().text_muted)
            .child(text)
    }

    /// A control's current value. Data, so mono — the type scale's `meta` step.
    fn rail_readout(&self, cx: &mut Context<Self>, text: String) -> impl IntoElement {
        div()
            .text_style(TextStyle::meta())
            .text_color(cx.theme().text_subtle)
            .child(text)
    }
}

/// A point size with no trailing `.0` — `20pt`, `12.5pt`. The step is half a
/// point, so one decimal is all a value can ever have.
fn fmt_pt(pt: f32) -> String {
    if (pt.fract()).abs() < 0.01 {
        format!("{}", pt.round() as i32)
    } else {
        format!("{pt:.1}")
    }
}

/// Millimetres for A4, inches for Letter.
///
/// The model stores mm because Typst does and A4 is a metric page, but a US
/// user setting up a Letter résumé thinks in inches — and this persona sets up
/// both. The unit follows the page rather than making them convert.
fn format_margin(mm: f32, page: PageSize) -> String {
    match page {
        PageSize::A4 => format!("{mm:.0} mm"),
        PageSize::Letter => format!("{:.2} in", mm / 25.4),
    }
}

#[cfg(test)]
mod tests {
    use super::format_margin;
    use crate::resume::model::PageSize;

    /// The unit follows the page, so neither user has to convert.
    #[test]
    fn the_readout_speaks_the_pages_own_unit() {
        assert_eq!(format_margin(20.0, PageSize::A4), "20 mm");
        assert_eq!(format_margin(25.4, PageSize::Letter), "1.00 in");
    }
}
