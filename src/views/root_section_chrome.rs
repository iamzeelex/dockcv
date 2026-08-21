//! Marks and toggles the section list carries: whether a section is in the CV
//! at all, and whether it is a candidate for trimming.
//!
//! Neither is layout — they answer "what is in this document", which is why
//! they are not in `root_layout_rail.rs` where they were first written.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, IntoElement, SharedString};

use dockcv_ui_components::{Button, ButtonVariants, IconName, Sizable, Tooltip};

use crate::resume::model::{SectionKind, TrimCandidate};
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::root::Root;

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
