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
    /// What CV is pinned right now: file stem and preset name. `None` when
    /// nothing is — which is what makes the menu item read "Pin" or "Change".
    pub pinned: Option<(String, String)>,
}

impl MenuContext {
    pub(super) fn of(shell: gpui::WeakEntity<Shell>, index: usize, app: &Application) -> Self {
        Self {
            shell,
            index,
            status: app.status(),
            company: app.company.clone(),
            has_snapshot: !app.snapshots.is_empty(),
            pinned: app
                .source_doc
                .clone()
                .map(|stem| (stem, app.preset.clone())),
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
            pinned,
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

        // One item, not one per document × preset. The choosing happens in a
        // sheet that can group, filter and unpin — see `applications_pin`.
        {
            let shell_pin = shell.clone();
            let index = *index;
            let company_pin = company.clone();
            let pinned = pinned.clone();
            menu = menu.separator().item(
                PopupMenuItem::new(if pinned.is_some() {
                    "Change the CV sent…"
                } else {
                    "Pin the CV sent…"
                })
                .on_click(move |_ev, window, cx| {
                    let company = company_pin.clone();
                    let pinned = pinned.clone();
                    let _ = shell_pin.update(cx, |this, cx| {
                        this.open_pin_pick(index, company, pinned, window, cx);
                    });
                }),
            );
        }

        if *has_snapshot {
            let shell_open = shell.clone();
            let index = *index;
            menu = menu.separator().item(PopupMenuItem::new("Open sent PDF").on_click(
                move |_ev, _window, cx| {
                    let _ = shell_open.update(cx, |this, cx| this.reveal_snapshot(index, cx));
                },
            ));
        } else if pinned.is_some() {
            // O-19: the capture that runs on send can fail — a document that
            // no longer compiles, a vault gone read-only — and until now the
            // only way to try again was to move the card out of its column and
            // back. The banner that reported the failure is long gone by then.
            let shell_retry = shell.clone();
            let index = *index;
            menu = menu.separator().item(PopupMenuItem::new("Capture snapshot now").on_click(
                move |_ev, _window, cx| {
                    let _ = shell_retry.update(cx, |this, cx| this.capture_snapshot(index, cx));
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
