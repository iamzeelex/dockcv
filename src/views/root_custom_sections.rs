//! Custom-section rendering for `Root` (`docs/ROADMAP.md` D-9,
//! `docs/design/editor.md` §3 section anatomy). A custom section shares the
//! built-ins' card anatomy — drag handle, collapse/expand, per-section
//! variant switcher, entry count (`root_sidebar.rs::card`) — but its entries
//! are one generic `CustomEntry` shape rather than a section-specific one,
//! and its card carries a "···" menu with the one action no built-in section
//! offers: deleting the whole section. The six built-ins are the document's
//! spine and cannot be removed; a custom section can, and because that loses
//! every variant's content, the action lives behind a menu rather than a
//! bare ✕ — the same treatment Delete gets on a CV card (review L-07), not
//! the lighter-weight ✕ this screen uses for removing a single entry or
//! highlight.

use gpui::prelude::*;
use gpui::{div, AnyElement, Context, IntoElement, SharedString};

use dockcv_ui_components::{
    Button, ButtonVariants, DropdownMenu, IconName, PopupMenuItem, Sizable,
};

use crate::resume::edit::{FieldId, ListId};
use crate::resume::model::{CustomSectionId, SectionKind};

use super::Root;

impl Root {
    /// A user-added section (D-9): the variant timeline plus a "Section name"
    /// field — the section's title is user-editable content, not a fixed
    /// label like "Profile", so it is renamed the same way a variant is: via
    /// a field in the card body, not inline in the header — then its
    /// `CustomEntry` entries (title/subtitle/date range/URL/highlights, the
    /// vocabulary `CustomEntry`'s own doc comment sets out as wide enough for
    /// a publication, a language, an award, a talk or a patent without a
    /// field per section type).
    pub(super) fn render_custom_section(
        &self,
        cx: &mut Context<Self>,
        id: CustomSectionId,
    ) -> AnyElement {
        let Some(section) = self.doc.custom_section(id) else {
            // Stale id — a deletion landed between scheduling this render and
            // running it. `ResumeDoc::sections()` repairs the dangling
            // reference away by the next frame; nothing to draw meanwhile.
            return div().into_any_element();
        };
        let title = if section.title.trim().is_empty() {
            "Untitled section".to_string()
        } else {
            section.title.clone()
        };

        // The section's own title is renamed from its card header
        // (`root_section_rename.rs`), the same gesture a built-in section's
        // heading uses — not a field in the body. `FieldId::CustomSectionTitle`
        // still addresses `section.title` (`edit.rs`, `sync_fields`), it just
        // has no input rendered against it here anymore.
        let mut f = self.variant_controls(cx, SectionKind::Custom(id));

        let entries = section.content.active();
        let count = entries.len();
        for (i, entry) in entries.iter().enumerate() {
            f.push(Self::wide(self.entry_header(
                cx,
                format!("Entry {}", i + 1),
                ListId::CustomEntries(id),
                i,
                // No library pool for custom sections (`Root::save_block_to_library`
                // doesn't have one either) — no ★.
                None,
            )));
            f.push(self.field(cx, FieldId::CustomEntryTitle(id, i), "Title"));
            f.push(self.field(cx, FieldId::CustomEntrySubtitle(id, i), "Subtitle"));
            f.extend(self.date_fields(cx, FieldId::CustomEntryStart(id, i), FieldId::CustomEntryEnd(id, i)));
            f.push(self.field(cx, FieldId::CustomEntryUrl(id, i), "URL"));
            let highlights: Vec<FieldId> = (0..entry.highlights.len())
                .map(|j| FieldId::CustomEntryHighlight(id, i, j))
                .collect();
            f.extend(self.highlight_list(
                cx,
                "Highlights",
                ListId::CustomEntryHighlights(id, i),
                &highlights,
            ));
            f.push(Self::wide(self.add_button(cx, "Add highlight", ListId::CustomEntryHighlights(id, i))));
        }
        f.push(Self::wide(self.add_button(cx, "Add entry", ListId::CustomEntries(id))));

        let menu = self.section_menu_button(cx, id);
        self.card(cx, SectionKind::Custom(id), title, count, f, Some(menu))
    }

    /// The "···" trigger on a custom section's card, offering the one action
    /// a built-in section never gets: deleting the section. Behind a menu
    /// rather than an always-visible ✕ (review L-07) because it is
    /// destructive — every variant's content goes with it. `Button`'s own
    /// mouse-down handling stops propagation (see
    /// `.research/gpui-component/crates/ui/src/popover.rs`'s trigger wrapper),
    /// so clicking this doesn't also toggle the card's own expand/collapse.
    pub(super) fn section_menu_button(
        &self,
        cx: &mut Context<Self>,
        id: CustomSectionId,
    ) -> AnyElement {
        let root = cx.weak_entity();
        Button::new(SharedString::from(format!("section-menu-{id:?}")))
            .icon(IconName::Ellipsis)
            .ghost()
            .xsmall()
            .cursor_pointer()
            .tooltip("More")
            .dropdown_menu(move |menu, _window, _cx| {
                let root = root.clone();
                menu.item(
                    PopupMenuItem::new("Delete section").on_click(move |_ev, window, cx| {
                        let _ = root.update(cx, |this, cx| {
                            this.checkpoint();
                            this.doc.remove_custom_section(id);
                            this.expanded.remove(&SectionKind::Custom(id));
                            this.fields_stale = true;
                            this.schedule_save(cx);
                            cx.notify();
                            this.schedule_recompile(window, cx);
                        });
                    }),
                )
            })
            .into_any_element()
    }
}
