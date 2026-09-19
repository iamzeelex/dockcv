//! The layout rail (C2, review US-07 / P-06): page size, margins and text
//! scale, beside the preview, applying live.
//!
//! Spec: the Typst-controls spec. This closes the gap that row names —
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
//!
//! This file is the rail's own chrome — when it opens, what it is made of,
//! which group is showing. The controls that go inside the groups are
//! `root_layout_rows.rs`: a panel and the twenty widgets it holds are two
//! different things to read, and only one of them changes when a layout
//! feature is added.
//!
//! Two other things used to live here and did not belong: the bar under the
//! paper (`root_preview_chrome.rs`), which changes how the document is looked
//! at rather than what it is, and the section list's own marks
//! (`root_section_chrome.rs`).

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, IntoElement, Window};

use dockcv_ui_components::{
    Accordion, Button, ButtonExt, ButtonVariants, DropdownMenu, PopupMenuItem, ScrollableElement,
    Selectable, Sizable, SliderEvent, SliderState, TextField, TextFieldEvent, TextFieldState,
};

use crate::resume::model::{LayoutSettings, ATS_SAFE_PROFILE};
use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::vault;

use super::root::{ProfileDetachment, ProfileFork, Root};
use super::save_status;

/// How wide the rail floats over the preview.
const RAIL_WIDTH: f32 = 220.0;

impl Root {
    /// The page settings currently reaching the renderer.
    pub(super) fn effective_layout(&self) -> LayoutSettings {
        self.profiles
            .resolve(self.doc.layout_profile.as_deref(), self.doc.layout)
    }

    /// Rebuild lazy sliders from the effective layout on the next frame.
    pub(super) fn reset_layout_sliders(&mut self) {
        self.margin_slider = None;
        self.scale_slider = None;
        self.slider_subscriptions.clear();
    }

    /// Turn a shared profile into an editable document layout before the first
    /// manual mutation touches it. The checkpoint is the pinned state, so one
    /// Undo restores the profile; the mutation lands on the copied layout.
    pub(super) fn prepare_document_layout_change(&mut self, changed: &str) {
        let Some(profile) = self.doc.layout_profile.clone() else {
            return;
        };
        self.checkpoint();
        self.doc.layout = self.effective_layout();
        self.doc.layout_profile = None;
        self.profile_detachment = Some(ProfileDetachment {
            affected: vault::documents_using_profile(&self.vault_dir, &profile),
            profile,
            changed: changed.to_string(),
        });
        self.profile_fork = None;
    }

    /// Select a reusable profile, or return to this CV's own layout.
    pub(super) fn select_layout_profile(
        &mut self,
        profile: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.layout_profile == profile {
            return;
        }
        self.checkpoint();
        self.doc.layout_profile = profile;
        self.profile_detachment = None;
        self.profile_fork = None;
        self.reset_layout_sliders();
        self.schedule_save(cx);
        self.schedule_recompile(window, cx);
        cx.notify();
    }

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
        let layout = self.effective_layout();
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
                let mut changed = this.effective_layout();
                changed.margins.set_uniform(value.start());
                if changed == this.effective_layout() {
                    return;
                }
                this.prepare_document_layout_change("Margins");
                this.doc.layout.margins.set_uniform(value.start());
                this.after_layout_change(window, cx);
            },
        ));

        let scale = cx.new(|_| {
            SliderState::new()
                .min(scale_min as f32)
                .max(scale_max as f32)
                .step(1.0)
                .default_value(
                    (layout.text_scale_pct as f32).clamp(scale_min as f32, scale_max as f32),
                )
        });
        self.slider_subscriptions.push(cx.subscribe_in(
            &scale,
            window,
            |this, _slider, event: &SliderEvent, window, cx| {
                let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
                if this.effective_layout().text_scale_pct == value.start().round() as u16 {
                    return;
                }
                this.prepare_document_layout_change("Text scale");
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
        let theme = *cx.theme();
        let layout = self.effective_layout();

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
            .child(self.render_profile_picker(cx))
            .child(
                div()
                    .id("layout-rail-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
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
                                            div().pt(px(4.0)).child(
                                                self.date_format_row(cx, layout.date_format),
                                            ),
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
                                .item(|item| {
                                    item.title(self.rail_group_title(cx, "Export"))
                                        .open(open == 5)
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(10.0))
                                                .pt(px(4.0))
                                                .child(self.filename_pattern_row(cx))
                                                .child(self.rail_subsection(cx, "Recent Exports"))
                                                .child(self.export_history_rows(cx, open == 5)),
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

    /// Profile selector at the top of the rail. ATS-safe is also a direct
    /// button: reaching the proven layout should not require opening a menu.
    fn render_profile_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let active = self.doc.layout_profile.clone();
        let label = active.clone().unwrap_or_else(|| "This CV".to_string());
        let names = self.profiles.names();
        let root = cx.weak_entity();

        div()
            .px(px(12.0))
            .pb(px(12.0))
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(
                div()
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_subtle)
                    .child("Profile"),
            )
            .child(
                Button::new("layout-profile")
                    .selector_inline()
                    .w_full()
                    .label(label)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        let own_root = root.clone();
                        menu = menu.item(
                            PopupMenuItem::new("This CV")
                                .checked(active.is_none())
                                .on_click(move |_ev, window, cx| {
                                    let _ = own_root.update(cx, |this, cx| {
                                        this.select_layout_profile(None, window, cx);
                                    });
                                }),
                        );
                        for name in names.clone() {
                            let selected = active.as_deref() == Some(name.as_str());
                            let item_root = root.clone();
                            let selected_name = name.clone();
                            menu = menu.item(PopupMenuItem::new(name).checked(selected).on_click(
                                move |_ev, window, cx| {
                                    let profile = selected_name.clone();
                                    let _ = item_root.update(cx, |this, cx| {
                                        this.select_layout_profile(Some(profile), window, cx);
                                    });
                                },
                            ));
                        }
                        menu
                    }),
            )
            .child(
                Button::new("layout-profile-ats-safe")
                    .quiet()
                    .w_full()
                    .selected(self.doc.layout_profile.as_deref() == Some(ATS_SAFE_PROFILE))
                    .tooltip("Apply the layout proven by the ATS conformance harness")
                    .child("Use ATS-safe")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.select_layout_profile(Some(ATS_SAFE_PROFILE.to_string()), window, cx);
                    })),
            )
    }

    /// Quiet, non-blocking answer to editing a shared profile.
    pub(super) fn render_profile_detachment(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let detachment = self
            .profile_detachment
            .as_ref()
            .expect("rendered only while detached");
        let affected = detachment.affected.len();
        let affected_names = if detachment.affected.is_empty() {
            "No saved CV currently names this profile.".to_string()
        } else {
            format!("Updates: {}", detachment.affected.join(", "))
        };

        let mut bar = div()
            .flex_none()
            .min_h(px(44.0))
            .px(px(18.0))
            .py(px(7.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.0))
            .bg(theme.elevated)
            .border_b_1()
            .border_color(theme.border)
            .text_style(TextStyle::body())
            .text_color(theme.text_muted)
            .child(format!(
                "{} changed. This CV no longer matches {}.",
                detachment.changed, detachment.profile
            ))
            .child(div().flex_1());

        if let Some(fork) = &self.profile_fork {
            bar = bar
                .child(
                    div()
                        .w(px(180.0))
                        .child(TextField::new(&fork.field).small()),
                )
                .child(
                    Button::new("profile-fork-save")
                        .primary()
                        .child("Save profile")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.commit_profile_fork(window, cx);
                        })),
                )
                .child(
                    Button::new("profile-fork-cancel")
                        .quiet()
                        .child("Cancel")
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.profile_fork = None;
                            cx.notify();
                        })),
                );
        } else {
            bar = bar
                .child(
                    Button::new("profile-update")
                        .quiet()
                        .tooltip(affected_names)
                        .child(format!(
                            "Update {} ({} CV{})",
                            detachment.profile,
                            affected,
                            if affected == 1 { "" } else { "s" }
                        ))
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.update_detached_profile(window, cx);
                        })),
                )
                .child(
                    Button::new("profile-fork")
                        .quiet()
                        .child("Save as a new profile…")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.start_profile_fork(window, cx);
                        })),
                );
        }
        bar
    }

    pub(super) fn update_detached_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(detachment) = self.profile_detachment.clone() else {
            return;
        };
        let mut profiles = self.profiles.clone();
        profiles.upsert(detachment.profile.clone(), self.doc.layout);
        let result = vault::save_profiles(&self.vault_dir, &profiles);
        save_status::record(cx, "profiles", result.clone());
        if result.is_err() {
            return;
        }
        self.profiles = profiles;
        self.doc.layout_profile = Some(detachment.profile);
        self.profile_detachment = None;
        self.profile_fork = None;
        self.reset_layout_sliders();
        self.schedule_save(cx);
        self.schedule_recompile(window, cx);
        cx.notify();
    }

    pub(super) fn start_profile_fork(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.profile_fork.is_some() {
            return;
        }
        let mut n = 1;
        let suggested = loop {
            let name = format!("Profile {n}");
            if !self.profiles.contains_name(&name) {
                break name;
            }
            n += 1;
        };
        let field = cx.new(|cx| {
            let state = TextFieldState::single_line(window, cx);
            state.seed(suggested, window, cx);
            state
        });
        let subscription = cx.subscribe_in(
            &field,
            window,
            |this, _state, event: &TextFieldEvent, window, cx| {
                if matches!(event, TextFieldEvent::Submitted) {
                    this.commit_profile_fork(window, cx);
                }
            },
        );
        let focus = field.read(cx).focus_handle(cx);
        self.profile_fork = Some(ProfileFork {
            field,
            _subscription: subscription,
        });
        focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn commit_profile_fork(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(fork) = &self.profile_fork else {
            return;
        };
        let name = fork.field.read(cx).value(cx).trim().to_string();
        if name.is_empty() {
            return;
        }
        if self.profiles.contains_name(&name) {
            save_status::record(
                cx,
                "profiles",
                Err(format!("A profile named “{name}” already exists.")),
            );
            return;
        }
        let mut profiles = self.profiles.clone();
        profiles.upsert(name.clone(), self.doc.layout);
        let result = vault::save_profiles(&self.vault_dir, &profiles);
        save_status::record(cx, "profiles", result.clone());
        if result.is_err() {
            return;
        }
        self.profiles = profiles;
        self.doc.layout_profile = Some(name);
        self.profile_detachment = None;
        self.profile_fork = None;
        self.reset_layout_sliders();
        self.schedule_save(cx);
        self.schedule_recompile(window, cx);
        cx.notify();
    }

    /// A group's heading. `GroupBox` accepts any element, so the title keeps
    /// the eyebrow treatment the rail already used for "Layout" rather than
    /// upstream's default label size.
    /// An accordion heading, which is a control you click — so it sits at the
    /// top of the rail's own hierarchy rather than below everything in it.
    ///
    /// It used to be an 11px mono eyebrow: smaller than the 12px labels inside
    /// the group it opens, and smaller than the 11.5px readouts beside them. A
    /// heading that is the smallest text on the panel is not a heading.
    fn rail_group_title(&self, cx: &mut Context<Self>, text: &'static str) -> impl IntoElement {
        div()
            .text_style(TextStyle::control())
            .text_color(cx.theme().text)
            .child(text)
    }
}

#[cfg(test)]
mod tests {
    use crate::resume::model::{
        Education, LayoutSettings, Margins, PageSize, Resume, ResumeDoc, ATS_SAFE_PROFILE,
    };

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
                    show_link_marks: true,
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

    #[test]
    fn ats_safe_removes_the_link_mark_from_the_real_pdf_text_layer() {
        let resume = Resume {
            education: vec![Education {
                institution: "MIT".into(),
                study_type: "B.S.".into(),
                url: "https://mit.edu".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut doc = ResumeDoc::from_resume(resume, "Base");

        let control =
            crate::typst_engine::TypstEngine::new(crate::resume::template::generate_for(&doc))
                .compile_to_pdf()
                .expect("control PDF compiles");
        let control_text = pdf_extract::extract_text_from_mem(&control).expect("extract control");
        assert!(control_text.contains('↗'));

        doc.layout_profile = Some(ATS_SAFE_PROFILE.to_string());
        let ats =
            crate::typst_engine::TypstEngine::new(crate::resume::template::generate_for(&doc))
                .compile_to_pdf()
                .expect("ATS-safe PDF compiles");
        let ats_text = pdf_extract::extract_text_from_mem(&ats).expect("extract ATS-safe");
        assert!(!ats_text.contains('↗'), "{ats_text:?}");
        assert!(ats_text.contains("MIT"), "the linked content was lost too");
    }
}
