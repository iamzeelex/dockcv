//! The nav rail — shared chrome for Gallery, Library, Diary, Applications and
//! Settings (`docs/design/gallery.md` §3a). Every one of those screens wears
//! the same wordmark + nav + vault row, so [`Shell::with_rail`] mounts it once
//! around whichever main pane is showing rather than each screen mounting its
//! own.
//!
//! P-19 is left exactly as the mockup draws it: the wordmark at the top and
//! the `cvault` / `Vault ▾` row at the bottom, with no explanation of what
//! "Vault" is. That is a spec-level open question (see the design doc's
//! Open Questions), not something to resolve here.

use gpui::prelude::*;
use gpui::{
    div, linear_color_stop, linear_gradient, px, AnyElement, ClickEvent, Context, FontWeight,
    IntoElement, SharedString,
};

use dockcv_ui_components::{DockIcon, Icon, IconName, Sizable, MONO, SANS};

use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::shell::{Screen, Shell};

impl Shell {
    /// Frame one screen's main pane in the vault chrome: the rail on the left,
    /// the pane filling the rest, and the vault menu floating over both.
    ///
    /// Every screen inside this frame is a tab, so the pane must never draw a
    /// back control of its own — the rail already carries the way out, and a
    /// second one would be two navigations for one move.
    pub(super) fn with_rail(&self, main: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        div()
            .size_full()
            .relative()
            .flex()
            .bg(theme.background)
            .text_color(theme.text)
            .child(self.render_rail(cx))
            .child(main)
            .children(self.menu_open.then(|| self.render_user_menu(cx)))
            .into_any_element()
    }

    /// Left navigation rail: wordmark, section nav, and the vault row.
    pub(super) fn render_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let vault_name = self
            .vault
            .as_ref()
            .and_then(|v| v.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "vault".to_string());

        let active_cvs = matches!(self.screen, Screen::Gallery);
        let active_library = matches!(self.screen, Screen::Library);
        let active_diary = matches!(self.screen, Screen::Diary);
        let active_applications = matches!(self.screen, Screen::Applications);

        div()
            .w(px(228.0))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            // The extra top clearance is for macOS's traffic lights: the
            // window's titlebar is transparent, so the rail's own content
            // sits directly under them unless it's pushed down.
            .pt(px(34.0))
            .pb(px(22.0))
            .px(px(16.0))
            .bg(theme.surface)
            .border_r_1()
            .border_color(theme.border)
            // Wordmark: "Dock" + "CV" concatenated, two colors, one weight.
            .child(
                div()
                    .px_2()
                    .pb_6()
                    .flex()
                    .items_baseline()
                    .font_family(SANS)
                    .text_size(px(21.0))
                    .font_weight(FontWeight::BOLD)
                    .child(div().text_color(theme.text).child("Dock"))
                    .child(div().text_color(theme.accent).child("CV")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(self.nav_item(
                        cx,
                        "nav-cvs",
                        Icon::new(IconName::GalleryVerticalEnd),
                        "CVs",
                        active_cvs,
                        |this, cx| {
                            this.screen = Screen::Gallery;
                            cx.notify();
                        },
                    ))
                    .child(self.nav_item(
                        cx,
                        "nav-lib",
                        Icon::new(IconName::Star),
                        "Library",
                        active_library,
                        |this, cx| {
                            this.screen = Screen::Library;
                            cx.notify();
                        },
                    ))
                    .child(self.nav_item(
                        cx,
                        "nav-diary",
                        Icon::new(IconName::BookOpen),
                        "Diary",
                        active_diary,
                        |this, cx| {
                            this.screen = Screen::Diary;
                            cx.notify();
                        },
                    ))
                    .child(self.nav_item(
                        cx,
                        "nav-apps",
                        Icon::new(DockIcon::Kanban),
                        "Applications",
                        active_applications,
                        |this, cx| {
                            this.screen = Screen::Applications;
                            cx.notify();
                        },
                    )),
            )
            // The design puts a `Roles` list in the rail on the Diary row and
            // nowhere else — it is that screen's filter, not vault-wide
            // navigation, so it appears with the screen.
            .children(active_diary.then(|| self.render_roles_facet(cx)).flatten())
            .child(div().flex_1())
            // Vault row → opens the menu.
            .child(
                div()
                    .id("user-row")
                    .flex()
                    .items_center()
                    .gap(px(11.0))
                    .px(px(10.0))
                    .py(px(8.0))
                    .mt(px(8.0))
                    .rounded(px(8.0))
                    .border_t_1()
                    .border_color(theme.border)
                    .cursor_pointer()
                    .when(self.menu_open, |s| s.bg(theme.hover))
                    .hover(|s| s.bg(theme.hover))
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.menu_open = !this.menu_open;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(30.0))
                            .h(px(30.0))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(linear_gradient(
                                150.0,
                                linear_color_stop(theme.accent, 0.0),
                                linear_color_stop(theme.warning, 1.0),
                            ))
                            .text_color(theme.on_accent)
                            .font_family(SANS)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(13.0))
                            .child(
                                vault_name
                                    .chars()
                                    .next()
                                    .unwrap_or('V')
                                    .to_uppercase()
                                    .to_string(),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .line_height(gpui::relative(1.25))
                            .child(
                                div()
                                    .font_family(SANS)
                                    .text_size(px(13.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .truncate()
                                    .child(vault_name),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(11.5))
                                    .text_color(theme.text_subtle)
                                    .child("Vault ▾"),
                            ),
                    ),
            )
    }

    /// `Roles · Acme Corp 9 · CoderDojo 4` — the Diary's own facet, and the
    /// answer US-12 gives instead of a streak: coverage per role, so an
    /// uncovered one is visible rather than a guilt counter for missed weeks.
    /// Absent until a win is actually tagged, since an empty list explains
    /// nothing.
    fn render_roles_facet(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let counts = self.role_counts(cx);
        if counts.is_empty() {
            return None;
        }
        let theme = cx.theme().clone();

        let mut list = div()
            .mt(px(18.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(12.0))
                    .pb(px(6.0))
                    .text_style(TextStyle::eyebrow())
                    .text_color(theme.text_subtle)
                    .child(TextStyle::eyebrow().apply_case("Roles")),
            )
            .child(self.role_row(cx, "role-all", "All wins", None, None));

        for (role, count) in counts {
            let id = SharedString::from(format!("role-{role}"));
            list = list.child(self.role_row(cx, id, &role.clone(), Some(count), Some(role)));
        }
        Some(list)
    }

    fn role_row(
        &self,
        cx: &mut Context<Self>,
        id: impl Into<SharedString>,
        label: &str,
        count: Option<usize>,
        role: Option<String>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let active = self.diary_role_filter == role;

        div()
            .id(id.into())
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px(px(12.0))
            .py(px(6.0))
            .rounded(px(7.0))
            .cursor_pointer()
            .text_style(TextStyle::control())
            .text_color(if active { theme.text } else { theme.text_muted })
            .when(active, |el| el.bg(theme.hover))
            .hover(|s| s.text_color(theme.text))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.diary_role_filter = role.clone();
                cx.notify();
            }))
            // `flex_1` with the `min_w_0` — see the gallery card's title.
            .child(div().flex_1().min_w_0().truncate().child(label.to_string()))
            .children(count.map(|count| {
                div()
                    .flex_none()
                    .text_style(TextStyle::chip())
                    .text_color(theme.text_subtle)
                    .child(format!("{count}"))
            }))
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn nav_item(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        icon: Icon,
        label: &'static str,
        active: bool,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let icon_color = if active {
            theme.accent
        } else {
            theme.text_subtle
        };
        let text_color = if active { theme.text } else { theme.text_muted };
        let weight = if active {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        };

        div()
            .id(id)
            .flex()
            .items_center()
            .gap(px(11.0))
            .px(px(12.0))
            .py(px(10.0))
            .mb(px(3.0))
            .rounded(px(8.0))
            .font_family(SANS)
            .text_size(px(14.5))
            .font_weight(weight)
            .text_color(text_color)
            // Active item: filled + a left accent bar (Slate signature). The
            // mockup draws this as an inset box-shadow; GPUI has no inset
            // shadow, so a 2px left border is the sanctioned equivalent (see
            // the design doc §3a).
            .when(active, |e| {
                e.bg(theme.hover).border_l_2().border_color(theme.accent)
            })
            .cursor_pointer()
            .hover(|s| s.text_color(theme.text))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| action(this, cx)))
            .child(icon.with_size(px(15.0)).text_color(icon_color))
            .child(label)
    }

    /// The vault dropdown anchored above the rail's vault row.
    pub(super) fn render_user_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let item = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .px_3()
                .py_2()
                .rounded_md()
                .text_style(TextStyle::control())
                .text_color(theme.text)
                .cursor_pointer()
                .hover(|s| s.bg(theme.surface))
                .child(label)
        };

        div()
            .absolute()
            .left(px(12.0))
            .bottom(px(64.0))
            .w(px(184.0))
            .flex()
            .flex_col()
            .gap_1()
            .p_1()
            .rounded_lg()
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .child(
                item("menu-change-vault", "⇄  Change vault").on_click(cx.listener(
                    |this, _: &ClickEvent, _window, cx| {
                        this.menu_open = false;
                        this.screen = Screen::Setup;
                        cx.notify();
                    },
                )),
            )
            // Settings opens a window (O-21), which is where macOS keeps it —
            // so the menu dispatches the same action `⌘,` does rather than
            // switching the pane behind the rail.
            .child(item("menu-settings", "⚙  Settings").on_click(cx.listener(
                |this, _: &ClickEvent, window, cx| {
                    this.menu_open = false;
                    window.dispatch_action(Box::new(crate::app::OpenSettings), cx);
                    cx.notify();
                },
            )))
    }
}
