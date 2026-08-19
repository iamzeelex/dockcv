//! The layout rail (C2, review US-07 / P-06): page size, margins and text
//! scale, beside the preview, applying live.
//!
//! Spec: `docs/design/typst-controls.md`. This closes the gap that row names —
//! "Typst is your advantage and you're hiding it". `LayoutSettings` has been on
//! `ResumeDoc` since C1, round-tripping through TOML and driving the Typst
//! preamble; until now **no view read it**, so A4 versus Letter could not be
//! changed from the app at all. The persona applies in the EU and the US
//! (`user-review.md` §1), so that was not a missing nicety.
//!
//! ### Three things the spec left open, decided here
//!
//! **O-1 — the rail overlays the preview, it does not displace it.** Displacing
//! would re-centre and re-fit the paper the moment you open the tool you came
//! to measure the paper with: the thing under inspection moves as you reach for
//! the ruler. The rail floats over the canvas at the right edge instead, the
//! same way the preview's own toolbar floats at the bottom, so the page you are
//! judging stays exactly where it was. It is a toggle, per the row's ruling —
//! layout is a last-forty-minutes activity, and a surface used that way earns
//! space while in use, not always.
//!
//! **O-10 — one slider, three stored edges.** The design draws a single
//! "Margins" control; the model keeps `x`/`top`/`bottom` so a hand-edited file
//! can be asymmetric. Moving the slider unifies them (`Margins::set_uniform`),
//! and the readout says "mixed" beforehand so an asymmetric page announces
//! itself rather than being flattened without warning.
//!
//! **Slider bounds** are `LayoutSettings::*_UI_RANGE`, deliberately narrower
//! than the model's clamps — see those constants for why a slider must not
//! offer its own guard rails as travel.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, IntoElement, SharedString, Window};

use dockcv_ui_components::{Accordion, 
    Button, ButtonVariants, DropdownMenu, IconName, PopupMenuItem, Sizable, Slider, SliderEvent,
    SliderState, Tooltip,
};

use crate::resume::model::{
    BulletGlyph, CategoryMark, ContactLayout, DateFormat, DocumentFont, Emphasis, EntryLayout,
    HeaderAlign, HeaderLayout, HeadingCase, HeadingLayout, HeadingStyle, LayoutSettings, MetaOrder,
    MetaPosition, PageSize, ResumeDoc, RowSpacing, SectionKind, SkillSeparator, SkillsLayout,
    SkillsStyle, TrimCandidate, TypeSizes,
};
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::root::{CompileState, Root};

/// Width from the design row.
const RAIL_WIDTH: f32 = 220.0;

impl Root {
    /// Build the rail's sliders once, seeded from the document, and keep the
    /// document in step with them.
    ///
    /// Subscribed to `Change`, not `Release`: the row's own acceptance text is
    /// "changes are visible immediately", and the preview's existing
    /// draft→crisp debounce (`schedule_recompile`) already absorbs a drag's
    /// worth of events without thrashing the compiler.
    pub(super) fn ensure_layout_sliders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.margin_slider.is_some() {
            return;
        }
        let layout = &self.doc.layout;
        let (margin_min, margin_max) = LayoutSettings::MARGIN_MM_UI_RANGE;
        let (scale_min, scale_max) = LayoutSettings::TEXT_SCALE_PCT_UI_RANGE;

        let margins = cx.new(|_| {
            SliderState::new()
                .min(margin_min)
                .max(margin_max)
                .step(0.5)
                .default_value(layout.margins.x_mm.clamp(margin_min, margin_max))
        });
        self.slider_subscriptions.push(cx.subscribe_in(
            &margins,
            window,
            |this, _slider, event: &SliderEvent, window, cx| {
                let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
                this.doc.layout.margins.set_uniform(value.start());
                this.after_layout_change(window, cx);
            },
        ));

        let scale = cx.new(|_| {
            SliderState::new()
                .min(scale_min as f32)
                .max(scale_max as f32)
                .step(1.0)
                .default_value((layout.text_scale_pct as f32).clamp(scale_min as f32, scale_max as f32))
        });
        self.slider_subscriptions.push(cx.subscribe_in(
            &scale,
            window,
            |this, _slider, event: &SliderEvent, window, cx| {
                let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
                this.doc.layout.text_scale_pct = value.start().round() as u16;
                this.after_layout_change(window, cx);
            },
        ));

        self.margin_slider = Some(margins);
        self.scale_slider = Some(scale);
    }

    /// Persist and re-render after any layout control moves.
    pub(super) fn after_layout_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.schedule_save(cx);
        self.schedule_recompile(window, cx);
        cx.notify();
    }

    pub(super) fn toggle_layout_rail(&mut self, cx: &mut Context<Self>) {
        self.layout_rail_open = !self.layout_rail_open;
        cx.notify();
    }

    /// The rail itself. Positioned absolutely by the caller's `relative()`
    /// preview pane, so it floats over the canvas rather than taking a column.
    pub(super) fn render_layout_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let layout = self.doc.layout;

        let open = self.layout_group;
        let root = cx.weak_entity();

        div()
            .absolute()
            .top_0()
            .right_0()
            .h_full()
            .w(px(RAIL_WIDTH))
            .flex()
            .flex_col()
            .bg(theme.surface)
            .border_l_1()
            .border_color(theme.border)
            .shadow_lg()
            // The rail floats *inside* the preview pane, which is itself a
            // scroll container — so without this a wheel over the rail scrolled
            // the document behind it, and the panel you were reaching into slid
            // away under your cursor. `occlude` stops the event reaching what is
            // painted behind, which is the same rule the gallery card's menu and
            // the applications card already follow (E-16).
            .occlude()
            .child(
                div()
                    .flex_none()
                    .px(px(18.0))
                    .pt(px(20.0))
                    .pb(px(12.0))
                    .text_style(TextStyle::eyebrow())
                    .text_color(theme.text_subtle)
                    .child(TextStyle::eyebrow().apply_case("Layout")),
            )
            .child(
                div()
                    .id("layout-rail-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(12.0))
                    .pb(px(20.0))
                    // One heading open at a time. Four groups stacked open is
                    // nine controls in a 220px column — a list to scroll past
                    // rather than a choice to make — and it does not fit a
                    // short window, which is how the rail came to be clipped.
                    //
                    // The wrapper is load-bearing: `Accordion` renders itself
                    // `size_full` and every `AccordionItem` `flex_1`, so given
                    // a definite height the four groups divide it evenly and
                    // collapsed headings sit 120px apart. Inside a
                    // content-height box there is nothing to divide.
                    .child(
                        div().child(
                        Accordion::new("layout-groups")
                            .multiple(false)
                            .bordered(false)
                            .item(|item| {
                                item.title(self.rail_group_title(cx, "Typography"))
                                    .open(open == 0)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(10.0))
                                            .pt(px(4.0))
                                            .child(self.font_row(cx, layout.font))
                                            .child(self.text_scale_row(cx, &layout))
                                            .child(self.rail_subsection(cx, "Element sizes"))
                                            .child(self.size_rows(cx)),
                                    )
                            })
                            .item(|item| {
                                item.title(self.rail_group_title(cx, "Page"))
                                    .open(open == 1)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(10.0))
                                            .pt(px(4.0))
                                            .child(self.page_size_row(cx, layout.page_size))
                                            .child(self.margins_row(cx, &layout)),
                                    )
                            })
                            .item(|item| {
                                item.title(self.rail_group_title(cx, "Dates"))
                                    .open(open == 2)
                                    .child(
                                        div()
                                            .pt(px(4.0))
                                            .child(self.date_format_row(cx, layout.date_format)),
                                    )
                            })
                            // Sections: how a section arranges what is inside
                            // it, as opposed to the document-wide decisions
                            // above. Skills needed it most — a CV's
                            // technologies are what a reader scans for, and
                            // this document spends most of a page on them.
                            .item(|item| {
                                item.title(self.rail_group_title(cx, "Header"))
                                    .open(open == 3)
                                    .child(
                                        div()
                                            .pt(px(4.0))
                                            .child(self.header_rows(cx, layout.header)),
                                    )
                            })
                            .item(|item| {
                                item.title(self.rail_group_title(cx, "Sections"))
                                    .open(open == 4)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(12.0))
                                            .pt(px(4.0))
                                            .child(self.rail_subsection(cx, "Headings"))
                                            .child(self.heading_rows(cx, layout.headings))
                                            .child(self.rail_subsection(cx, "Entries"))
                                            .child(self.entry_rows(cx, layout.entries))
                                            .child(self.rail_subsection(cx, "Skills"))
                                            .child(self.skills_rows(cx, layout.skills)),
                                    )
                            })
                            .on_toggle_click(move |open, _window, cx| {
                                // `multiple(false)` means at most one index.
                                // An empty slice is the group closing itself,
                                // which leaves the rail with four headings and
                                // nothing under them — a legitimate state, and
                                // `usize::MAX` is how it is spelled here.
                                let next = open.first().copied().unwrap_or(usize::MAX);
                                let _ = root.update(cx, |this, cx| {
                                    this.layout_group = next;
                                    cx.notify();
                                });
                            }),
                        ),
                    ),
            )
    }

    /// How the Skills section is set — five decisions, because it is the
    /// densest text on a CV and the one most likely to cost a page.
    ///
    /// Notably **not** a proficiency control. The model stores no level, and a
    /// row of bars assembled from nothing is the invented metric US-14 exists
    /// to forbid — the reference layouts that offer one are reading a field
    /// their model has and ours does not. Where a person wants to say it, they
    /// type it: `Expert: Python` is a keyword like any other.
    fn skills_rows(&self, cx: &mut Context<Self>, skills: SkillsLayout) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(self.skills_pick(
                cx,
                "Layout",
                "layout-skills-style",
                skills.style.label(),
                SkillsStyle::ALL
                    .iter()
                    .map(|s| (s.label(), *s == skills.style, *s))
                    .collect(),
                |doc, style| doc.layout.skills.style = style,
            ))
            .child(self.skills_pick(
                cx,
                "Separator",
                "layout-skills-sep",
                skills.separator.label(),
                SkillSeparator::ALL
                    .iter()
                    .map(|s| (s.label(), *s == skills.separator, *s))
                    .collect(),
                |doc, sep| doc.layout.skills.separator = sep,
            ))
            .child(self.skills_pick(
                cx,
                "Category",
                "layout-skills-mark",
                skills.mark.label(),
                CategoryMark::ALL
                    .iter()
                    .map(|m| (m.label(), *m == skills.mark, *m))
                    .collect(),
                |doc, mark| doc.layout.skills.mark = mark,
            ))
            .child(self.skills_pick(
                cx,
                "Row spacing",
                "layout-skills-spacing",
                skills.spacing.label(),
                RowSpacing::ALL
                    .iter()
                    .map(|s| (s.label(), *s == skills.spacing, *s))
                    .collect(),
                |doc, spacing| doc.layout.skills.spacing = spacing,
            ))
            .child(self.skills_pick(
                cx,
                "Row marker",
                "layout-skills-bullets",
                if skills.bullets { "Bullet" } else { "None" },
                vec![("None", !skills.bullets, false), ("Bullet", skills.bullets, true)],
                |doc, bullets| doc.layout.skills.bullets = bullets,
            ))
    }

    /// A named subsection inside a rail group.
    ///
    /// Without it the Sections group was five bare labels — `Layout`,
    /// `Separator`, `Category` — and nothing said they were about Skills.
    /// `Bubbles` is an obvious answer to a question the panel never asked.
    fn rail_subsection(&self, cx: &mut Context<Self>, title: &'static str) -> impl IntoElement {
        let theme = cx.theme().clone();
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
    fn header_rows(&self, cx: &mut Context<Self>, header: HeaderLayout) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(self.skills_pick(
                cx,
                "Alignment",
                "layout-header-align",
                header.align.label(),
                HeaderAlign::ALL
                    .iter()
                    .map(|a| (a.label(), *a == header.align, *a))
                    .collect(),
                |doc, v| doc.layout.header.align = v,
            ))
            .child(self.skills_pick(
                cx,
                "Contact details",
                "layout-header-contacts",
                header.contacts.label(),
                ContactLayout::ALL
                    .iter()
                    .map(|c| (c.label(), *c == header.contacts, *c))
                    .collect(),
                |doc, v| doc.layout.header.contacts = v,
            ))
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
    fn heading_rows(&self, cx: &mut Context<Self>, headings: HeadingLayout) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(self.skills_pick(
                cx,
                "Style",
                "layout-heading-style",
                headings.style.label(),
                HeadingStyle::ALL
                    .iter()
                    .map(|s| (s.label(), *s == headings.style, *s))
                    .collect(),
                |doc, v| doc.layout.headings.style = v,
            ))
            .child(self.skills_pick(
                cx,
                "Capitalization",
                "layout-heading-case",
                headings.case.label(),
                HeadingCase::ALL
                    .iter()
                    .map(|c| (c.label(), *c == headings.case, *c))
                    .collect(),
                |doc, v| doc.layout.headings.case = v,
            ))
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
    fn entry_rows(&self, cx: &mut Context<Self>, entries: EntryLayout) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(self.skills_pick(
                cx,
                "Date & place",
                "layout-entry-meta-pos",
                entries.meta_position.label(),
                MetaPosition::ALL
                    .iter()
                    .map(|m| (m.label(), *m == entries.meta_position, *m))
                    .collect(),
                |doc, v| doc.layout.entries.meta_position = v,
            ))
            .child(self.skills_pick(
                cx,
                "Order",
                "layout-entry-meta-order",
                entries.meta_order.label(),
                MetaOrder::ALL
                    .iter()
                    .map(|m| (m.label(), *m == entries.meta_order, *m))
                    .collect(),
                |doc, v| doc.layout.entries.meta_order = v,
            ))
            .child(self.skills_pick(
                cx,
                "Subtitle",
                "layout-entry-subtitle",
                entries.subtitle.label(),
                Emphasis::ALL
                    .iter()
                    .map(|e| (e.label(), *e == entries.subtitle, *e))
                    .collect(),
                |doc, v| doc.layout.entries.subtitle = v,
            ))
            .child(self.skills_pick(
                cx,
                "Date & place style",
                "layout-entry-meta-style",
                entries.meta.label(),
                Emphasis::ALL
                    .iter()
                    .map(|e| (e.label(), *e == entries.meta, *e))
                    .collect(),
                |doc, v| doc.layout.entries.meta = v,
            ))
            .child(self.skills_pick(
                cx,
                "Bullets",
                "layout-entry-bullet",
                entries.bullet.label(),
                BulletGlyph::ALL
                    .iter()
                    .map(|b| (b.label(), *b == entries.bullet, *b))
                    .collect(),
                |doc, v| doc.layout.entries.bullet = v,
            ))
            .child(self.skills_pick(
                cx,
                "Body",
                "layout-entry-indent",
                if entries.indent_body { "Indented" } else { "Full width" },
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
        let theme = cx.theme().clone();
        let root = cx.weak_entity();
        let current = SharedString::from(current.to_string());
        div()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .child(self.rail_label(cx, label))
            .child(
                Button::new(id)
                    .cursor_pointer()
                    .ghost()
                    .w_full()
                    .justify_between()
                    .label(current)
                    .icon(IconName::ChevronDown)
                    .border_1()
                    .border_color(theme.border)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        for (text, checked, value) in options.clone() {
                            let root = root.clone();
                            menu = menu.item(
                                PopupMenuItem::new(text)
                                    .checked(checked)
                                    .on_click(move |_ev, window, cx| {
                                        let _ = root.update(cx, |this, cx| {
                                            apply(&mut this.doc, value);
                                            this.after_layout_change(window, cx);
                                        });
                                    }),
                            );
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
    fn font_row(&self, cx: &mut Context<Self>, active: DocumentFont) -> impl IntoElement {
        let theme = cx.theme().clone();
        let root = cx.weak_entity();
        div()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(self.rail_label(cx, "Font"))
            .child(
                Button::new("layout-font")
                    .cursor_pointer()
                    .ghost()
                    .w_full()
                    .justify_between()
                    .label(active.label())
                    .icon(IconName::ChevronDown)
                    .border_1()
                    .border_color(theme.border)
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
    fn date_format_row(&self, cx: &mut Context<Self>, active: DateFormat) -> impl IntoElement {
        let theme = cx.theme().clone();
        let root = cx.weak_entity();
        div()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(self.rail_label(cx, "Date format"))
            .child(
                Button::new("layout-date-format")
                    .cursor_pointer()
                    .ghost()
                    .w_full()
                    .justify_between()
                    .label(active.example())
                    .icon(IconName::ChevronDown)
                    .border_1()
                    .border_color(theme.border)
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
                                .on_click(move |_ev, window, cx| {
                                    let _ = root.update(cx, |this, cx| {
                                        this.doc.layout.date_format = format;
                                        this.after_layout_change(window, cx);
                                    });
                                }),
                            );
                        }
                        menu
                    }),
            )
    }

    fn page_size_row(&self, cx: &mut Context<Self>, active: PageSize) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(self.rail_label(cx, "Page size"))
            .child(
                div()
                    .flex()
                    .gap(px(2.0))
                    .p(px(3.0))
                    .rounded(px(7.0))
                    .bg(theme.chip_bg_neutral)
                    .children([PageSize::Letter, PageSize::A4].map(|size| {
                        let selected = size == active;
                        div()
                            .id(match size {
                                PageSize::Letter => "page-size-letter",
                                PageSize::A4 => "page-size-a4",
                            })
                            .flex_1()
                            .py(px(6.0))
                            .rounded(px(5.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .text_style(TextStyle::control())
                            .when(selected, |el| el.bg(theme.selected).text_color(theme.text))
                            .when(!selected, |el| {
                                el.text_color(theme.text_muted)
                                    .hover(|s| s.text_color(theme.text))
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.doc.layout.page_size = size;
                                this.after_layout_change(window, cx);
                            }))
                            .child(match size {
                                PageSize::Letter => "Letter",
                                PageSize::A4 => "A4",
                            })
                    })),
            )
    }

    fn margins_row(&self, cx: &mut Context<Self>, layout: &LayoutSettings) -> impl IntoElement {
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

    fn text_scale_row(&self, cx: &mut Context<Self>, layout: &LayoutSettings) -> impl IntoElement {
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
    fn size_rows(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
        .icon(if up { IconName::Plus } else { IconName::Minus })
        .ghost()
        .xsmall()
        .cursor_pointer()
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            let (lo, hi) = TypeSizes::DELTA_RANGE;
            let next = (read(&this.doc.layout.sizes) + step).clamp(lo, hi);
            write(&mut this.doc.layout.sizes, next);
            this.after_layout_change(window, cx);
        }))
    }

    /// A group's heading. `GroupBox` accepts any element, so the title keeps
    /// the eyebrow treatment the rail already used for "Layout" rather than
    /// upstream's default label size.
    fn rail_group_title(&self, cx: &mut Context<Self>, text: &'static str) -> impl IntoElement {
        div()
            .text_style(TextStyle::eyebrow())
            .text_color(cx.theme().text_subtle)
            .child(TextStyle::eyebrow().apply_case(text))
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

impl Root {
    /// The section header's show/hide toggle (O-13).
    ///
    /// Visibility is what a *preset* selects, but the editor still needs a way
    /// to set it — a preset records the current state, so there has to be a
    /// current state to record. Hiding here drops the section from the
    /// rendered document immediately (`ResumeDoc::compose`), which is what
    /// makes the preview the answer to "what would this CV look like without
    /// Organizations".
    ///
    /// Profile has no toggle: a résumé without a name is not a shorter résumé.
    pub(super) fn visibility_button(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
    ) -> AnyElement {
        if section == SectionKind::Profile {
            return div().into_any_element();
        }
        let hidden = self.doc.is_hidden(section);
        Button::new(SharedString::from(format!("visibility-{section:?}")))
            .icon(if hidden {
                IconName::EyeOff
            } else {
                IconName::Eye
            })
            .ghost()
            .xsmall()
            .cursor_pointer()
            .tooltip(if hidden {
                "Hidden from this CV — click to show"
            } else {
                "Hide from this CV"
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                let hidden = this.doc.is_hidden(section);
                this.doc.set_hidden(section, !hidden);
                this.after_layout_change(window, cx);
            }))
            .into_any_element()
    }
}

/// Zoom steps the `−`/`+` buttons walk, in percent. A fixed ladder rather
/// than a free multiplier: the design draws a percentage readout, and round
/// numbers are what a user can say out loud and get back to.
const ZOOM_STEPS: [u16; 9] = [50, 67, 80, 90, 100, 110, 125, 150, 200];

/// The range a continuous pinch may reach. The same ends as the stepped
/// control — a gesture must not be able to leave the zoom somewhere `+`/`-`
/// cannot bring it back from.
pub(super) const MIN_ZOOM_PCT: f32 = ZOOM_STEPS[0] as f32;
pub(super) const MAX_ZOOM_PCT: f32 = ZOOM_STEPS[ZOOM_STEPS.len() - 1] as f32;

impl Root {
    /// The floating toolbar under the paper: zoom, page count, compile state,
    /// and — when it overflows — how much is over (US-07, US-08).
    ///
    /// Drawn as one persistent bar rather than the two the mockup shows in
    /// different rows, per `typst-controls.md`'s own synthesis: zoom and the
    /// page counter are always there, the overflow affordance appears only
    /// when there is overflow.
    pub(super) fn render_preview_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let pages = self.geometry.as_ref().map(|g| g.page_count).unwrap_or(1);

        div()
            .absolute()
            .bottom(px(16.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center()

            .child(
                div()
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .px(px(6.0))
                    .rounded(px(10.0))
                    .bg(theme.chrome.opacity(0.92))
                    .border_1()
                    .border_color(theme.border)
                    .shadow_lg()
                    .child(self.zoom_button(cx, "zoom-out", IconName::Minus, -1))
                    .child(
                        div()
                            .min_w(px(46.0))
                            .flex()
                            .justify_center()
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_subtle)
                            .child(format!("{}%", self.zoom_pct.round() as i32)),
                    )
                    .child(self.zoom_button(cx, "zoom-in", IconName::Plus, 1))
                    .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                    .child(
                        div()
                            .px(px(8.0))
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_subtle)
                            .child(format!("1 / {pages}")),
                    )
                    .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                    .child(self.compile_status(cx)),
            )
    }

    fn zoom_button(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        icon: IconName,
        direction: i32,
    ) -> impl IntoElement {
        Button::new(id)
            .icon(icon)
            .ghost()
            .xsmall()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                // From wherever a pinch left the zoom, `+`/`-` moves to the
                // next step in that direction rather than to the step nearest
                // the current value — pressing `+` must always zoom in.
                let next = if direction > 0 {
                    ZOOM_STEPS
                        .iter()
                        .find(|z| f32::from(**z) > this.zoom_pct + 0.5)
                        .copied()
                        .unwrap_or(ZOOM_STEPS[ZOOM_STEPS.len() - 1])
                } else {
                    ZOOM_STEPS
                        .iter()
                        .rev()
                        .find(|z| f32::from(**z) < this.zoom_pct - 0.5)
                        .copied()
                        .unwrap_or(ZOOM_STEPS[0])
                };
                this.zoom_pct = f32::from(next);
                cx.notify();
                // Zooming changes how many pixels the sheet occupies, so it
                // changes the resolution the page must be rasterized at. The
                // bitmap stretches immediately (so the control feels instant)
                // and a sharp pass lands behind it.
                this.schedule_recompile(window, cx);
            }))
    }

    /// Compile state, always visible — US-07's acceptance text asks for
    /// "compiling / ready / error" at all times, and neither mockup draws the
    /// in-flight case at all.
    fn compile_status(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        // A word only when there is something to say. "ready" is the state
        // the preview is in almost always, and naming it every second told
        // the user nothing they could not see by looking at the page; the dot
        // alone carries it. `compiling` and the two failure states are the
        // ones worth reading, so those keep their label.
        let (color, label) = match &self.compile_state {
            CompileState::Compiling => (theme.text_subtle, Some("compiling")),
            CompileState::Ready { warnings } if warnings.is_empty() => (theme.success, None),
            CompileState::Ready { .. } => (theme.warning, Some("warnings")),
            CompileState::Error { .. } => (theme.danger, Some("error")),
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .child(div().size(px(7.0)).rounded_full().bg(color))
            .children(label.map(|label| {
                div()
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_subtle)
                    .child(label)
            }))
    }
}

impl Root {
    /// Width the paper is drawn at, in display pixels.
    pub(super) fn preview_width(&self) -> f32 {
        480.0 * self.zoom_pct / 100.0
    }
}

impl Root {
    /// The section list's "trim candidate" marks (O-12, second half).
    ///
    /// **Decided: the sidebar, not the paper.** The design row draws these
    /// tags inside the rendered page, but that row has no sections panel — it
    /// is a detail shot of the preview. Putting them on the paper would mean
    /// either baking decoration into the Typst source (which then has to be
    /// kept out of the exported PDF, and doubles what a keystroke compiles) or
    /// reading per-section frame positions back out of the compiler, which it
    /// does not expose. The sidebar needs neither, and — more to the point —
    /// it is where the fix is: the variant switcher is two pixels away.
    ///
    /// Shown only while the document actually overflows. A CV that fits has no
    /// trimming to do, and a permanent "this could be shorter" badge on a
    /// document that is already fine is nagging, not information.
    pub(super) fn trim_candidate_for(&self, section: SectionKind) -> Option<TrimCandidate> {
        let overflows = self
            .geometry
            .as_ref()
            .is_some_and(|g| g.overflow_pt > 0.0);
        if !overflows {
            return None;
        }
        self.doc
            .trim_candidates()
            .into_iter()
            .find(|c| c.section == section)
    }

    /// A chip on an overflowing section that already has a leaner cut written.
    /// Clicking switches to it — the whole point is that the shorter text
    /// exists, so acting on the suggestion costs nothing.
    pub(super) fn render_trim_chip(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
    ) -> Option<AnyElement> {
        let candidate = self.trim_candidate_for(section)?;
        let theme = cx.theme().clone();
        let variant = candidate.variant.clone();

        Some(
            div()
                .id(SharedString::from(format!("trim-{section:?}")))
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(px(7.0))
                .py(px(2.0))
                .rounded(px(5.0))
                .bg(theme.warning.opacity(0.18))
                .cursor_pointer()
                .text_style(TextStyle::chip())
                .text_color(theme.warning)
                .tooltip({
                    let variant = variant.clone();
                    move |window, cx| {
                        Tooltip::new(format!("Switch to “{variant}”, a shorter cut you already wrote"))
                            .build(window, cx)
                    }
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.doc.set_active_variant_by_name(section, &variant);
                    this.after_layout_change(window, cx);
                }))
                .child("trim candidate")
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::format_margin;
    use crate::resume::model::{LayoutSettings, Margins, PageSize};

    /// The rail's travel must sit inside what `sanitized()` will accept, or a
    /// slider dragged to an end would produce a value the model then silently
    /// changes underneath the readout.
    #[test]
    fn the_sliders_cannot_reach_a_value_the_model_would_clamp() {
        let (margin_min, margin_max) = LayoutSettings::MARGIN_MM_UI_RANGE;
        let (scale_min, scale_max) = LayoutSettings::TEXT_SCALE_PCT_UI_RANGE;

        for page_size in [PageSize::A4, PageSize::Letter] {
            for (margin, scale) in [(margin_min, scale_min), (margin_max, scale_max)] {
                let settings = LayoutSettings {
                    page_size,
                    font: Default::default(),
                    date_format: Default::default(),
                    skills: Default::default(),
                    entries: Default::default(),
                    header: Default::default(),
                    headings: Default::default(),
                    sizes: Default::default(),
                    text_scale_pct: scale,
                    leading_em: 0.62,
                    margins: Margins {
                        x_mm: margin,
                        top_mm: margin,
                        bottom_mm: margin,
                    },
                };
                let sanitized = settings.sanitized();
                assert_eq!(sanitized.text_scale_pct, scale);
                assert_eq!(sanitized.margins.x_mm, margin);
                assert_eq!(sanitized.margins.top_mm, margin);
            }
        }
    }

    /// The unit follows the page, so neither user has to convert.
    #[test]
    fn the_readout_speaks_the_pages_own_unit() {
        assert_eq!(format_margin(20.0, PageSize::A4), "20 mm");
        assert_eq!(format_margin(25.4, PageSize::Letter), "1.00 in");
    }
}

#[cfg(test)]
mod zoom_tests {
    use super::{MAX_ZOOM_PCT, MIN_ZOOM_PCT, ZOOM_STEPS};

    /// A pinch leaves the zoom between steps. `+` must still zoom *in* from
    /// there — stepping to the nearest step instead would sometimes move the
    /// wrong way, or not at all.
    #[test]
    fn plus_and_minus_step_past_a_value_a_pinch_left_behind() {
        let step = |from: f32, direction: i32| -> f32 {
            if direction > 0 {
                ZOOM_STEPS
                    .iter()
                    .find(|z| f32::from(**z) > from + 0.5)
                    .copied()
                    .unwrap_or(ZOOM_STEPS[ZOOM_STEPS.len() - 1])
            } else {
                ZOOM_STEPS
                    .iter()
                    .rev()
                    .find(|z| f32::from(**z) < from - 0.5)
                    .copied()
                    .unwrap_or(ZOOM_STEPS[0])
            }
            .into()
        };

        assert_eq!(step(103.7, 1), 110.0, "+ must leave a mid-step value upward");
        assert_eq!(step(103.7, -1), 100.0, "- must leave it downward");
        // Exactly on a step, it moves to the next one rather than standing still.
        assert_eq!(step(100.0, 1), 110.0);
        assert_eq!(step(100.0, -1), 90.0);
        // The ends hold.
        assert_eq!(step(MAX_ZOOM_PCT, 1), MAX_ZOOM_PCT);
        assert_eq!(step(MIN_ZOOM_PCT, -1), MIN_ZOOM_PCT);
    }

    /// A gesture must not be able to leave the zoom somewhere the buttons
    /// cannot bring it back from.
    #[test]
    fn a_pinch_cannot_leave_the_stepped_range() {
        let pinch = |from: f32, delta: f32| (from * (1.0 + delta)).clamp(MIN_ZOOM_PCT, MAX_ZOOM_PCT);
        assert_eq!(pinch(200.0, 0.5), MAX_ZOOM_PCT);
        assert_eq!(pinch(50.0, -0.5), MIN_ZOOM_PCT);
        assert!((pinch(100.0, 0.1) - 110.0).abs() < 0.01);
    }
}
