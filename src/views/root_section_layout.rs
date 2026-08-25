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
//! Backed by `ResumeDoc::section_overrides`, which is sparse in two ways: a
//! section with nothing to say carries no row, and within a row each field is
//! its own `Option`. So "Follow document" is the absence of a value, and a
//! section that departs on one field keeps following on the rest.

use gpui::prelude::*;
use gpui::{div, AnyElement, App, Context, Entity, SharedString, WeakEntity, Window};

use dockcv_ui_components::{
    Button, ButtonExt, DropdownMenu, IconName, PopupMenu, PopupMenuItem,
};

use crate::resume::model::{
    BulletGlyph, Emphasis, HeaderAlign, HeadingCase, HeadingStyle, MetaOrder, MetaPosition,
    SectionKind, SectionOverrides,
};

use super::root::Root;

impl Root {
    /// The section card's own layout control.
    ///
    /// Only on the expanded card. The visibility eye is on collapsed rows too,
    /// because a section you want gone is exactly the one you have no reason
    /// to open — styling is the opposite: you need to see what you are
    /// changing, and the preview beside it answers immediately.
    pub(super) fn section_layout_button(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
    ) -> AnyElement {
        let overrides = self.doc.section_overrides(section);
        // Resolved, so a submenu ticks what the section is *actually* set in
        // even while it is following the document.
        let heading = self.doc.headings_for(section);
        let entries = self.doc.entries_for(section);
        let departs = !overrides.is_empty();
        let root = cx.weak_entity();

        div()
            // The button carries a dropdown, and the card header behind it is
            // itself a click target that expands and collapses the card. Without
            // this, opening the menu also folds the section away (E-16).
            .occlude()
            .child(
                Button::new(SharedString::from(format!("section-layout-{section:?}")))
                    .icon_only()
                    .icon(IconName::Settings2)
                    .tooltip(if departs {
                        "Layout for this section — set apart from the document"
                    } else {
                        "Layout for this section"
                    })
                    .dropdown_menu(move |menu, window, cx| {
                        let root = root.clone();
                        build_menu(menu, window, cx, section, overrides, heading, entries, &root)
                    }),
            )
            .into_any_element()
    }
}

#[allow(clippy::too_many_arguments)]
fn build_menu(
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut App,
    section: SectionKind,
    overrides: SectionOverrides,
    heading: crate::resume::model::HeadingLayout,
    entries: crate::resume::model::EntryLayout,
    root: &WeakEntity<Root>,
) -> PopupMenu {
    let prints_heading = !overrides.no_heading;
    let toggle_root = root.clone();

    let mut menu = menu
        .item(
            PopupMenuItem::new("Print heading")
                .checked(prints_heading)
                .on_click(move |_ev, window, cx| {
                    let _ = toggle_root.update(cx, |this, cx| {
                        let now = this.doc.prints_heading(section);
                        this.doc.set_heading_printed(section, !now);
                        this.after_layout_change(window, cx);
                    });
                }),
        )
        .item(PopupMenuItem::separator());

    // The heading half is dead weight while the heading is not printed, and a
    // menu of controls that change nothing teaches the reader something untrue
    // (E-43) — the same rule the rail follows for the contact separator.
    if prints_heading {
        menu = menu
            .item(PopupMenuItem::label("Heading"))
            .item(PopupMenuItem::submenu(
                "Style",
                field_menu(
                    window,
                    cx,
                    section,
                    root,
                    overrides.heading_style,
                    heading.style,
                    HeadingStyle::ALL.map(|v| (v.label(), v)).to_vec(),
                    |o, v| o.heading_style = v,
                ),
            ))
            .item(PopupMenuItem::submenu(
                "Capitalization",
                field_menu(
                    window,
                    cx,
                    section,
                    root,
                    overrides.heading_case,
                    heading.case,
                    HeadingCase::ALL.map(|v| (v.label(), v)).to_vec(),
                    |o, v| o.heading_case = v,
                ),
            ));
        // Alignment cannot move the one style whose words have nowhere to go.
        if heading.style.can_align() {
            menu = menu.item(PopupMenuItem::submenu(
                "Alignment",
                field_menu(
                    window,
                    cx,
                    section,
                    root,
                    overrides.heading_align,
                    heading.align,
                    HeaderAlign::ALL.map(|v| (v.label(), v)).to_vec(),
                    |o, v| o.heading_align = v,
                ),
            ));
        }
        menu = menu.item(PopupMenuItem::separator());
    }

    // Sections with no dated entries — Profile is a paragraph, Skills is a bag
    // of words — have nothing for these to act on.
    if !matches!(section, SectionKind::Profile | SectionKind::Skills) {
        menu = menu
            .item(PopupMenuItem::label("Entries"))
            .item(PopupMenuItem::submenu(
                "Date & place",
                field_menu(
                    window,
                    cx,
                    section,
                    root,
                    overrides.meta_position,
                    entries.meta_position,
                    MetaPosition::ALL.map(|v| (v.label(), v)).to_vec(),
                    |o, v| o.meta_position = v,
                ),
            ))
            .item(PopupMenuItem::submenu(
                "Order",
                field_menu(
                    window,
                    cx,
                    section,
                    root,
                    overrides.meta_order,
                    entries.meta_order,
                    MetaOrder::ALL.map(|v| (v.label(), v)).to_vec(),
                    |o, v| o.meta_order = v,
                ),
            ))
            .item(PopupMenuItem::submenu(
                "Subtitle",
                field_menu(
                    window,
                    cx,
                    section,
                    root,
                    overrides.subtitle,
                    entries.subtitle,
                    Emphasis::ALL.map(|v| (v.label(), v)).to_vec(),
                    |o, v| o.subtitle = v,
                ),
            ))
            .item(PopupMenuItem::submenu(
                "Bullet",
                field_menu(
                    window,
                    cx,
                    section,
                    root,
                    overrides.bullet,
                    entries.bullet,
                    BulletGlyph::ALL.map(|v| (v.label(), v)).to_vec(),
                    |o, v| o.bullet = v,
                ),
            ))
            .item(PopupMenuItem::submenu(
                "Indent body",
                field_menu(
                    window,
                    cx,
                    section,
                    root,
                    overrides.indent_body,
                    entries.indent_body,
                    vec![("Flush left", false), ("Under the title", true)],
                    |o, v| o.indent_body = v,
                ),
            ));
    }

    menu
}

/// One field's submenu: "Follow document" and then the values.
///
/// `current` is what the section is set in right now, override or not, so the
/// tick always says what the page is doing. `set` is what the section itself
/// says — `None` puts the tick on "Follow document" instead.
#[allow(clippy::too_many_arguments)]
fn field_menu<T: Copy + PartialEq + 'static>(
    window: &mut Window,
    cx: &mut App,
    section: SectionKind,
    root: &WeakEntity<Root>,
    set: Option<T>,
    current: T,
    options: Vec<(&'static str, T)>,
    apply: fn(&mut SectionOverrides, Option<T>),
) -> Entity<PopupMenu> {
    let root = root.clone();
    PopupMenu::build(window, cx, move |mut menu, _window, _cx| {
        let follow_root = root.clone();
        menu = menu
            .item(
                PopupMenuItem::new("Follow document")
                    .checked(set.is_none())
                    .on_click(move |_ev, window, cx| {
                        let _ = follow_root.update(cx, |this, cx| {
                            let mut o = this.doc.section_overrides(section);
                            apply(&mut o, None);
                            this.doc.set_section_overrides(section, o);
                            this.after_layout_change(window, cx);
                        });
                    }),
            )
            .item(PopupMenuItem::separator());

        for (label, value) in options {
            let root = root.clone();
            menu = menu.item(
                PopupMenuItem::new(label)
                    .checked(set.is_some() && current == value)
                    .on_click(move |_ev, window, cx| {
                        let _ = root.update(cx, |this, cx| {
                            let mut o = this.doc.section_overrides(section);
                            apply(&mut o, Some(value));
                            this.doc.set_section_overrides(section, o);
                            this.after_layout_change(window, cx);
                        });
                    }),
            );
        }
        menu
    })
}

#[cfg(test)]
mod tests {
    use crate::resume::model::{
        HeadingCase, HeadingStyle, Resume, ResumeDoc, SectionKind, SectionOverrides,
    };

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

    /// "Follow document" is the absence of a value, and choosing it back has
    /// to leave the table as empty as it started — otherwise a section that
    /// was briefly customised stays pinned to whatever it was showing.
    #[test]
    fn following_the_document_again_empties_the_row() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");

        doc.set_section_overrides(
            SectionKind::Work,
            SectionOverrides {
                heading_style: Some(HeadingStyle::Boxed),
                heading_case: Some(HeadingCase::AsTyped),
                ..Default::default()
            },
        );
        assert_eq!(doc.section_overrides.len(), 1);

        let mut o = doc.section_overrides(SectionKind::Work);
        o.heading_style = None;
        doc.set_section_overrides(SectionKind::Work, o);
        assert_eq!(
            doc.headings_for(SectionKind::Work).style,
            doc.layout.headings.style,
            "clearing one field did not return it to the document"
        );

        let mut o = doc.section_overrides(SectionKind::Work);
        o.heading_case = None;
        doc.set_section_overrides(SectionKind::Work, o);
        assert!(
            doc.section_overrides.is_empty(),
            "the last field cleared but the row stayed: {:?}",
            doc.section_overrides
        );
    }
}
