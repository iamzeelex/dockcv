//! The two menus on the front door, and the rule that keeps them apart.
//!
//! There are two objects on this screen — a CV and a version of it — and every
//! destructive thing you can do belongs to exactly one of them. They used to
//! share a menu, so `Delete` on a version row deleted the whole CV. Splitting
//! the menus fixed the wiring and left the danger: both hung off a `···` at the
//! same right edge, in the same glyph, at the same size. Nothing on screen said
//! which one you had open.
//!
//! So the rule here is not "different icons". It is:
//!
//! **A CV's menu opens from the CV's own name; a version's opens from a `···`
//! on the version's row.** A control labelled with the thing it acts on cannot
//! be mistaken for one labelled with something else. Under that, two supports:
//! every item names its object (`Delete CV`, `Delete version`, never `Delete`),
//! and each menu opens under a header naming the object it will act on — the
//! menu floats away from its trigger, so it has to say so itself.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, FontWeight, SharedString};
use std::path::Path;

use dockcv_ui_components::{Button, ButtonExt, DropdownMenu, IconName, PopupMenuItem, TextField, SANS};

use crate::theme::ActiveTheme;

use super::shell::Shell;

/// The document actions the menu offers, so one closure serves them all rather
/// than five near-identical ones.
#[derive(Clone, Copy)]
enum Action {
    Rename,
    Versions,
    Duplicate,
    Reveal,
}

impl Shell {
    /// The CV's name, which is also the way into everything you can do to it.
    ///
    /// `header` is whether this is the strip over a document's versions (where
    /// the name is the heading) or the address line inside a single row (where
    /// it is the quiet half of `imported-5 › Northwind`). The menu is the same
    /// either way; only the type changes, because it is the same object.
    pub(super) fn render_document_name(
        &self,
        cx: &mut Context<Self>,
        path: &Path,
        stem: &str,
        header: bool,
    ) -> AnyElement {
        let theme = *cx.theme();
        if self.renaming_doc.as_deref() == Some(path) {
            return div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .w(px(220.0))
                        .children(self.rename_field.as_ref().map(|state| {
                            TextField::new(state).placeholder("CV name")
                        })),
                )
                // A way out that is not "press Escape and hope": the box
                // replaces the name, so without this the only exits are
                // committing or restarting the app.
                .child(
                    Button::new("rename-cancel")
                        .icon_only()
                        .icon(IconName::Close)
                        .tooltip("Cancel")
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.cancel_rename(cx);
                        })),
                )
                .into_any_element();
        }

        let shell = cx.weak_entity();
        let vault_dir = self.vault.clone();
        let owned = path.to_path_buf();
        let label = stem.to_string();

        Button::new(SharedString::from(format!(
            "cv-menu-{}",
            path.to_string_lossy()
        )))
        .quiet()
        // The caret is what makes a name read as a control. Passing it as
        // `.icon(..)` would put it *before* the label, which is upstream's
        // documented trap and the reason `ButtonExt::selector` exists.
        .dropdown_caret(true)
        .tooltip("Actions for this CV")
        .child(
            div()
                .font_family(SANS)
                .text_size(if header { px(13.0) } else { px(11.5) })
                .font_weight(if header {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if header { theme.text } else { theme.text_muted })
                .child(label.clone()),
        )
        .dropdown_menu(move |menu, _window, _cx| {
            // The menu names what it will act on. It opens away from the name
            // that summoned it, so the trigger alone cannot carry the fact.
            let mut menu = menu.item(PopupMenuItem::label(label.clone()));
            for (item, action) in [
                ("Rename CV…", Action::Rename),
                ("All versions…", Action::Versions),
                ("Duplicate CV", Action::Duplicate),
                ("Show in Finder", Action::Reveal),
            ] {
                let shell = shell.clone();
                let path = owned.clone();
                let vault_dir = vault_dir.clone();
                menu = menu.item(PopupMenuItem::new(item).on_click(move |_ev, window, cx| {
                    let _ = shell.update(cx, |this, cx| match action {
                        Action::Rename => this.start_rename(path.clone(), window, cx),
                        Action::Versions => this.open_preset_matrix(path.clone(), cx),
                        Action::Duplicate => this.duplicate_doc(path.clone(), cx),
                        Action::Reveal => {
                            if let Some(dir) = vault_dir.clone() {
                                cx.open_with_system(&dir);
                            }
                        }
                    });
                }));
            }
            menu = menu.separator();
            let new_shell = shell.clone();
            menu = menu.item(
                PopupMenuItem::new("New CV…").on_click(move |_ev, _window, cx| {
                    let _ = new_shell.update(cx, |this, cx| {
                        this.gallery_creating = true;
                        cx.notify();
                    });
                }),
            );
            let del_shell = shell.clone();
            let del_path = owned.clone();
            menu.separator().item(
                // Named, because this is the one that takes the writing with
                // it. `Delete` on its own is what made the two menus dangerous.
                PopupMenuItem::new("Delete CV").on_click(move |_ev, _window, cx| {
                    let _ = del_shell.update(cx, |this, cx| this.delete_doc(del_path.clone(), cx));
                }),
            )
        })
        .into_any_element()
    }

    /// The `···` on a version row. Everything in it acts on the **version**.
    ///
    /// Nothing here can destroy writing: a version owns no content — it is a
    /// set of selections over variants that all still exist — so `Delete
    /// version` destroys the naming of a combination and nothing else. That is
    /// why it needs no confirmation and why the CV's own `Delete` keeps one.
    pub(super) fn render_version_menu(
        &self,
        cx: &mut Context<Self>,
        path: &Path,
        index: usize,
        name: &str,
    ) -> AnyElement {
        let shell = cx.weak_entity();
        let owned = path.to_path_buf();
        let label = name.to_string();

        Button::new(SharedString::from(format!(
            "version-menu-{}-{index}",
            path.to_string_lossy()
        )))
        .icon_only()
        .icon(IconName::Ellipsis)
        .tooltip("Actions for this version")
        .dropdown_menu(move |menu, _window, _cx| {
            let mut menu = menu.item(PopupMenuItem::label(label.clone()));

            let rename = shell.clone();
            let rename_path = owned.clone();
            menu = menu.item(PopupMenuItem::new("Rename version…").on_click(
                move |_ev, window, cx| {
                    let _ = rename.update(cx, |this, cx| {
                        this.start_version_rename(&rename_path, index, window, cx);
                    });
                },
            ));

            let tailor = shell.clone();
            menu = menu.item(
                PopupMenuItem::new(format!("Tailor from “{label}”…")).on_click(
                    move |_ev, window, cx| {
                        let _ = tailor.update(cx, |this, cx| this.open_tailor(window, cx));
                    },
                ),
            );

            let duplicate = shell.clone();
            let duplicate_path = owned.clone();
            menu = menu.item(PopupMenuItem::new("Duplicate version").on_click(
                move |_ev, _window, cx| {
                    let _ = duplicate.update(cx, |this, cx| {
                        this.duplicate_version(&duplicate_path, index, cx);
                    });
                },
            ));

            let matrix = shell.clone();
            let matrix_path = owned.clone();
            menu = menu.item(PopupMenuItem::new("Compare versions…").on_click(
                move |_ev, _window, cx| {
                    let _ = matrix.update(cx, |this, cx| {
                        this.open_preset_matrix(matrix_path.clone(), cx);
                    });
                },
            ));

            let delete = shell.clone();
            let delete_path = owned.clone();
            menu.separator()
                .item(PopupMenuItem::new("Delete version").on_click(
                    move |_ev, _window, cx| {
                        let _ = delete.update(cx, |this, cx| {
                            this.delete_version(&delete_path, index, cx);
                        });
                    },
                ))
        })
        .into_any_element()
    }
}
