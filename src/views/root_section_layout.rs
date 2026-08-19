//! Where one section departs from the document's layout.
//!
//! The document-wide settings live in the layout rail (`root_layout_rail.rs`)
//! and are the primary control: on a CV, uniformity is the default and
//! difference is the exception, so the rail keeps holding the decision that
//! applies to everything. This is the exception surface, and it sits **on the
//! section card** rather than in the rail — the rail would have to grow a
//! scope selector ("applies to: …"), which is a mode, and a mode is a thing
//! you can forget you are in. Here, what you are editing is whatever the
//! button is attached to.
//!
//! Backed by `ResumeDoc::section_overrides`, which is sparse: a section with
//! nothing to say carries no row, and "follow the document" is the absence of
//! an entry rather than a row of defaults.

use gpui::prelude::*;
use gpui::{div, AnyElement, Context, SharedString};

use dockcv_ui_components::{
    Button, ButtonVariants, DropdownMenu, IconName, PopupMenuItem, Sizable,
};

use crate::resume::model::SectionKind;

use super::root::Root;

impl Root {
    /// The section card's own layout control.
    ///
    /// Only on the expanded card. The visibility eye is on the collapsed row
    /// too, because a section you want gone is exactly the one you have no
    /// reason to open — styling is the opposite: you need to see what you are
    /// changing, and the preview beside it answers immediately.
    pub(super) fn section_layout_button(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
    ) -> AnyElement {
        let prints_heading = self.doc.prints_heading(section);
        let root = cx.weak_entity();

        div()
            // The button carries a dropdown, and the card header behind it is
            // itself a click target that expands and collapses the card. Without
            // this, opening the menu also folds the section away (E-16).
            .occlude()
            .child(
                Button::new(SharedString::from(format!("section-layout-{section:?}")))
                    .icon(IconName::Settings2)
                    .ghost()
                    .xsmall()
                    .cursor_pointer()
                    .tooltip(if prints_heading {
                        "Layout for this section"
                    } else {
                        "Layout for this section — heading is off"
                    })
                    .dropdown_menu(move |menu, _window, _cx| {
                        let root = root.clone();
                        menu.item(
                            PopupMenuItem::new("Print heading")
                                .checked(prints_heading)
                                .on_click(move |_ev, window, cx| {
                                    let _ = root.update(cx, |this, cx| {
                                        let now = this.doc.prints_heading(section);
                                        this.doc.set_heading_printed(section, !now);
                                        this.after_layout_change(window, cx);
                                    });
                                }),
                        )
                    }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use crate::resume::model::{ResumeDoc, Resume, SectionKind};

    /// The control is a toggle over the model, so the property that matters is
    /// that it is one section's decision and not the document's.
    #[test]
    fn turning_a_heading_off_leaves_every_other_section_alone() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        doc.set_heading_printed(SectionKind::Profile, false);

        assert!(!doc.prints_heading(SectionKind::Profile));
        for other in [
            SectionKind::Work,
            SectionKind::Education,
            SectionKind::Skills,
            SectionKind::Certificates,
            SectionKind::Organizations,
        ] {
            assert!(doc.prints_heading(other), "{other:?} followed Profile");
        }
    }
}
