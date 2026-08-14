//! The one action menu an application has, wherever it is drawn from.
//!
//! It lived inside `applications.rs::card` as a closure. Once the List view
//! existed there were two places that needed it, and two copies of "move to
//! column / pin a CV / open the sent PDF / delete" would have drifted the
//! first time one of them gained an item. So it is built once, here, and the
//! board card hangs it off its `···` button while a list row opens it on
//! right-click.
//!
//! `dropdown_menu` and `context_menu` take the same closure type, which is why
//! one builder can serve both.

use gpui::{Context, Window};

use dockcv_ui_components::{PopupMenu, PopupMenuItem};

use crate::resume::model::{Application, ApplicationStatus};

use super::applications_snapshot::PinOption;
use super::confirm;
use super::shell::Shell;

/// Everything the menu needs, collected at render time so the closure captures
/// owned data rather than borrowing `Shell`.
pub(super) struct MenuContext {
    pub shell: gpui::WeakEntity<Shell>,
    /// Position in `Applications::entries` — the identity every action here
    /// addresses.
    pub index: usize,
    pub status: ApplicationStatus,
    pub company: String,
    pub has_snapshot: bool,
    /// Every document × preset in the vault, already computed from the cache.
    pub pin_choices: Vec<PinOption>,
}

impl MenuContext {
    pub(super) fn of(shell: gpui::WeakEntity<Shell>, index: usize, app: &Application, pin_choices: Vec<PinOption>) -> Self {
        Self {
            shell,
            index,
            status: app.status,
            company: app.company.clone(),
            has_snapshot: !app.snapshots.is_empty(),
            pin_choices,
        }
    }
}

/// The columns a card can be moved to, in board order.
const COLUMNS: [(ApplicationStatus, &str); 5] = [
    (ApplicationStatus::Wishlist, "Wishlist"),
    (ApplicationStatus::Applied, "Applied"),
    (ApplicationStatus::Interviewing, "Interviewing"),
    (ApplicationStatus::Offer, "Offer"),
    (ApplicationStatus::Rejected, "Rejected"),
];

/// Build the menu closure. Both `Button::dropdown_menu` and
/// `ContextMenuExt::context_menu` take exactly this shape.
pub(super) fn application_menu(
    ctx: MenuContext,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |mut menu, _window, _cx| {
        let MenuContext {
            shell,
            index,
            status,
            company,
            has_snapshot,
            pin_choices,
        } = &ctx;

        for (other_status, other_title) in COLUMNS {
            if other_status == *status {
                continue;
            }
            let shell_move = shell.clone();
            let index = *index;
            menu = menu.item(
                PopupMenuItem::new(format!("Move to {other_title}")).on_click(
                    move |_ev, _window, cx| {
                        let _ = shell_move.update(cx, |this, cx| {
                            // The same call the drag uses, so a move made here
                            // still records the send date and captures the PDF
                            // snapshot (D4a). A second, quieter path would
                            // skip both.
                            this.advance_application(index, other_status, cx);
                        });
                    },
                ),
            );
        }

        if !pin_choices.is_empty() {
            menu = menu.separator();
            for option in pin_choices {
                let shell_pin = shell.clone();
                let index = *index;
                let stem = option.stem.clone();
                let preset = option.preset.clone();
                menu = menu.item(PopupMenuItem::new(format!("Pin CV: {}", option.label)).on_click(
                    move |_ev, _window, cx| {
                        let stem = stem.clone();
                        let preset = preset.clone();
                        let _ = shell_pin.update(cx, |this, cx| {
                            this.pin_application_cv(index, stem, preset, cx);
                        });
                    },
                ));
            }
        }

        if *has_snapshot {
            let shell_open = shell.clone();
            let index = *index;
            menu = menu.separator().item(PopupMenuItem::new("Open sent PDF").on_click(
                move |_ev, _window, cx| {
                    let _ = shell_open.update(cx, |this, cx| this.reveal_snapshot(index, cx));
                },
            ));
        }

        let shell_del = shell.clone();
        let index = *index;
        let company = company.clone();
        menu.separator()
            .item(PopupMenuItem::new("Delete").on_click(move |_ev, window, cx| {
                let company = company.clone();
                let _ = shell_del.update(cx, |_this, cx: &mut Context<Shell>| {
                    let who = if company.trim().is_empty() {
                        "this application".to_string()
                    } else {
                        format!("the application to {}", company.trim())
                    };
                    confirm::destructive(
                        format!("Delete {who}?"),
                        format!(
                            "{} Any PDF snapshots stay in your vault's snapshots \
                             folder — only the card goes.",
                            confirm::CANNOT_UNDO
                        ),
                        "Delete",
                        window,
                        cx,
                        move |this, _window, cx| this.delete_application(index, cx),
                    );
                });
            }))
    }
}
