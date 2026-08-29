//! The nav rail — shared chrome for Gallery, Library, Diary, Applications and
//! Settings (the gallery spec §3a). Every one of those screens wears
//! the same wordmark + nav + vault row, so [`Shell::with_rail`] mounts it once
//! around whichever main pane is showing rather than each screen mounting its
//! own.
//!
//! **P-19, answered.** The mockup draws two brands — `DockCV` at the top and
//! `cvault · Vault ▾` at the bottom — and the review's complaint is that the
//! most expensive place in the window is spent on a word that explains nothing.
//! *Vault* is not a brand and not an account: it is the folder the documents
//! are in. So the bottom row names that folder and prints its path, which is
//! also the standing answer to "where are my files" (P-11).

use gpui::prelude::*;
use gpui::{
    div, linear_color_stop, linear_gradient, px, Action, AnyElement, ClickEvent, Context,
    FontWeight, IntoElement, SharedString, Window,
};

use dockcv_ui_components::{
    DockIcon, Icon, IconName, Kbd, ListItem, ListItemExt, Sizable, MONO, SANS,
};

use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::shell::{Screen, Shell};

/// One entry in the rail: what it is called, what it looks like, and the chord
/// that reaches it.
///
/// A struct rather than four more parameters — `nav_item` also takes the active
/// flag and the action, and eight positional arguments is a call site nobody can
/// read.
pub(super) struct NavEntry {
    pub id: &'static str,
    pub icon: Icon,
    pub label: &'static str,
    pub chord: Option<SharedString>,
}

/// The chord a nav entry advertises, resolved from the binding that is actually
/// registered rather than typed out beside it.
///
/// `root.rs` takes the same approach for its tooltips, and for the same reason:
/// a hint written as a literal drifts the moment the keymap moves, and a hint
/// for a chord that does nothing is worse than no hint at all.
/// `~/Documents/cvault` rather than `/Users/name/Documents/cvault` — the home
/// prefix is noise, and on a 228px rail noise is what pushes the part that
/// identifies the folder off the end.
fn home_relative(path: &std::path::Path) -> String {
    let text = path.to_string_lossy().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && text.starts_with(&home) => {
            format!("~{}", &text[home.len()..])
        }
        _ => text,
    }
}

/// Elide from the **left**, keeping the tail.
///
/// GPUI's `truncate()` only clips the end, which on a path throws away the
/// folder name and keeps the part every path shares. Done in Rust because the
/// element has no way to express it.
fn elide_path_start(path: &str, max_chars: usize) -> String {
    let count = path.chars().count();
    if count <= max_chars {
        return path.to_string();
    }
    let tail: String = path
        .chars()
        .skip(count.saturating_sub(max_chars.saturating_sub(1)))
        .collect();
    format!("…{tail}")
}

fn chord_for(action: &dyn Action, window: &Window) -> Option<SharedString> {
    let binding = window.highest_precedence_binding_for_action(action)?;
    let stroke = binding.keystrokes().first()?;
    Some(SharedString::from(Kbd::format(stroke.inner())))
}

impl Shell {
    /// Frame one screen's main pane in the vault chrome: the rail on the left,
    /// the pane filling the rest, and the vault menu floating over both.
    ///
    /// Every screen inside this frame is a tab, so the pane must never draw a
    /// back control of its own — the rail already carries the way out, and a
    /// second one would be two navigations for one move.
    pub(super) fn with_rail(
        &self,
        main: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *cx.theme();
        div()
            .size_full()
            .relative()
            .flex()
            .bg(theme.background)
            .text_color(theme.text)
            .child(self.render_rail(window, cx))
            .child(main)
            .children(self.menu_open.then(|| self.render_user_menu(cx)))
            .into_any_element()
    }

    /// Left navigation rail: wordmark, section nav, and the vault row.
    pub(super) fn render_rail(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        let vault_name = self
            .vault
            .as_ref()
            .and_then(|v| v.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "vault".to_string());
        let vault_path = self
            .vault
            .as_ref()
            .map(|v| elide_path_start(&home_relative(v), 26))
            .unwrap_or_else(|| "no folder chosen".to_string());

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
            //
            // 17/600, not the mockup's 21/700. It was the loudest element in
            // the rail and the only one that never changes or responds to
            // anything — outranking the navigation the user actually came to
            // click.
            .child(
                div()
                    .px_2()
                    .pb(px(20.0))
                    .flex()
                    .items_baseline()
                    .font_family(SANS)
                    .text_size(px(17.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(div().text_color(theme.text).child("Dock"))
                    .child(div().text_color(theme.accent).child("CV")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(self.nav_item(
                        cx,
                        NavEntry {
                            id: "nav-cvs",
                            icon: Icon::new(IconName::GalleryVerticalEnd),
                            label: "CVs",
                            chord: chord_for(&crate::app::GoToCvs, window),
                        },
                        active_cvs,
                        |this, cx| {
                            this.screen = Screen::Gallery;
                            cx.notify();
                        },
                    ))
                    .child(self.nav_item(
                        cx,
                        NavEntry {
                            id: "nav-lib",
                            icon: Icon::new(IconName::Star),
                            label: "Library",
                            chord: chord_for(&crate::app::GoToLibrary, window),
                        },
                        active_library,
                        |this, cx| {
                            this.screen = Screen::Library;
                            cx.notify();
                        },
                    ))
                    .child(self.nav_item(
                        cx,
                        NavEntry {
                            id: "nav-diary",
                            icon: Icon::new(IconName::BookOpen),
                            label: "Diary",
                            chord: chord_for(&crate::app::GoToDiary, window),
                        },
                        active_diary,
                        |this, cx| {
                            this.screen = Screen::Diary;
                            cx.notify();
                        },
                    ))
                    .child(self.nav_item(
                        cx,
                        NavEntry {
                            id: "nav-apps",
                            icon: Icon::new(DockIcon::Kanban),
                            label: "Applications",
                            chord: chord_for(&crate::app::GoToApplications, window),
                        },
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
            .children(self.render_recent(cx))
            .child(div().flex_1())
            // Above the vault row, below everything that is navigation: the
            // rail's chrome is where the app is allowed to talk about itself.
            .children(self.render_update_notice(cx))
            // Vault row → opens the menu.
            .child(
                ListItem::new("user-row")
                    .row()
                    .justify_start()
                    .selected(self.menu_open)
                    .gap(px(11.0))
                    .px(px(10.0))
                    .py(px(8.0))
                    .mt(px(8.0))
                    .border_t_1()
                    .border_color(theme.border)
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.menu_open = !this.menu_open;
                        cx.notify();
                    }))
                    // Avatar beside a two-line identity: one flex child, since
                    // `ListItem` hands its children to a block div.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(11.0))
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
                            // The folder, then where it is. `Vault ▾` said neither —
                            // and the path is the answer to P-11 sitting on the screen
                            // the user is on most.
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
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
                                            .text_size(px(11.0))
                                            .text_color(theme.text_subtle)
                                            .child(vault_path),
                                    ),
                            )
                            // A real glyph, not a `▾` typed into the label: the
                            // character rendered at the label's size in whichever font
                            // happened to carry the codepoint.
                            .child(
                                Icon::new(IconName::ChevronDown)
                                    .with_size(theme.icon_sm())
                                    .text_color(theme.text_subtle),
                            ),
                    ),
            )
    }

    /// The three documents touched most recently, as a way back into work.
    ///
    /// The rail's own answer to being four entries tall in a window six hundred
    /// pixels taller than that. Deliberately **not** a count of anything: the
    /// only figure here is each document's age, which is the number that says
    /// which one you were in last night.
    ///
    /// Each row leads with the role rather than the person, the same reasoning
    /// the gallery card follows — in a vault of one person's documents the name
    /// is the same string on every row.
    fn render_recent(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        const SHOWN: usize = 3;

        let theme = *cx.theme();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut recent: Vec<&crate::vault::DocMeta> = self
            .cache
            .metadata()
            .iter()
            .filter(|m| m.modified_secs.is_some())
            .collect();
        if recent.len() < 2 {
            // One document is not a list, and zero is the empty vault the
            // gallery already explains.
            return None;
        }
        recent.sort_by_key(|m| std::cmp::Reverse(m.modified_secs.unwrap_or(0)));
        recent.truncate(SHOWN);

        let rows: Vec<AnyElement> = recent
            .into_iter()
            .map(|meta| {
                let path = meta.path.clone();
                let title = if meta.label.trim().is_empty() {
                    meta.stem.clone()
                } else {
                    meta.label.clone()
                };
                let age = meta
                    .modified_secs
                    .map(|secs| crate::vault::relative_time(secs, now))
                    .unwrap_or_default();

                ListItem::new(SharedString::from(format!(
                    "recent-{}",
                    meta.path.to_string_lossy()
                )))
                .row()
                .text_color(theme.text_muted)
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.open_doc(path.clone(), cx);
                }))
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .justify_between()
                        .gap_2()
                        .child(div().flex_1().min_w_0().truncate().child(title))
                        .child(
                            div()
                                .flex_none()
                                .text_style(TextStyle::chip())
                                .text_color(theme.text_subtle)
                                .child(age),
                        ),
                )
                .into_any_element()
            })
            .collect();

        Some(
            div()
                .mt(px(18.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(12.0))
                        .mb(px(6.0))
                        .text_style(TextStyle::eyebrow())
                        .text_color(theme.text_subtle)
                        .child(TextStyle::eyebrow().apply_case("Recent")),
                )
                .children(rows),
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
        let theme = *cx.theme();

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
        let theme = *cx.theme();
        let active = self.diary_role_filter == role;

        ListItem::new(id.into())
            .row()
            .selected(active)
            .px(px(12.0))
            .text_style(TextStyle::control())
            .text_color(if active { theme.text } else { theme.text_muted })
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.diary_role_filter = role.clone();
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    // `flex_1` with the `min_w_0` — see the gallery card's title.
                    .child(div().flex_1().min_w_0().truncate().child(label.to_string()))
                    .children(count.map(|count| {
                        div()
                            .flex_none()
                            .text_style(TextStyle::chip())
                            .text_color(theme.text_subtle)
                            .child(format!("{count}"))
                    })),
            )
    }

    pub(super) fn nav_item(
        &self,
        cx: &mut Context<Self>,
        entry: NavEntry,
        active: bool,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        let NavEntry {
            id,
            icon,
            label,
            chord,
        } = entry;
        let theme = *cx.theme();
        // One weight of attention per row: a hairline glyph two steps below
        // its own label reads as disabled rather than as quiet.
        let icon_color = if active {
            theme.accent
        } else {
            theme.text_muted
        };
        let text_color = if active { theme.text } else { theme.text_muted };

        ListItem::new(id)
            .row()
            .selected(active)
            .justify_start()
            .gap(px(11.0))
            .px(px(12.0))
            .py(px(10.0))
            .mb(px(3.0))
            .text_color(text_color)
            .when(active, |e| e.font_weight(FontWeight::MEDIUM))
            // Active item: filled + a left accent bar (Slate signature). The
            // mockup draws this as an inset box-shadow; GPUI has no inset
            // shadow, so a 2px left border is the sanctioned equivalent (see
            // the design doc §3a).
            .when(active, |e| e.border_l_2().border_color(theme.accent))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| action(this, cx)))
            // A group, so the chord can react to the *row's* hover — GPUI's
            // `.hover()` refines style, not children, and cannot swap one for
            // another. Named per row so each chord resolves against its own.
            .group(SharedString::from(format!("{id}-hover")))
            // One flex child, not two: `ListItem` puts its children in a block
            // div, so an icon and a label handed over separately stack.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(11.0))
                    .child(icon.with_size(theme.icon_md()).text_color(icon_color))
                    .child(label)
                    // The chord, on hover only. Always-on shortcuts are noise
                    // on a rail you look at all day; on hover they are a
                    // tooltip that costs no popup and no delay.
                    .children(chord.map(|chord| {
                        div()
                            .ml_auto()
                            .pl_2()
                            .text_style(TextStyle::chip())
                            .text_color(theme.text_subtle)
                            .opacity(0.0)
                            .group_hover(SharedString::from(format!("{id}-hover")), |s| {
                                s.opacity(1.0)
                            })
                            .child(chord)
                    })),
            )
    }

    /// The vault dropdown anchored above the rail's vault row.
    pub(super) fn render_user_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        // Icon then label, not a codepoint smuggled into the string: a glyph
        // typed into a label renders at the label's size and takes the label's
        // colour, which is exactly what the icon ladder exists to decide.
        let item = |id: &'static str, icon: IconName, label: &'static str| {
            ListItem::new(id)
                .row()
                .justify_start()
                .text_style(TextStyle::control())
                .text_color(theme.text)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(9.0))
                        .child(
                            Icon::new(icon)
                                .with_size(theme.icon_sm())
                                .text_color(theme.text_subtle),
                        )
                        .child(label),
                )
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
            .rounded(theme.radius_md())
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .child(
                item("menu-change-vault", IconName::Replace, "Change vault").on_click(cx.listener(
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
            .child(
                item("menu-settings", IconName::Settings, "Settings").on_click(cx.listener(
                    |this, _: &ClickEvent, window, cx| {
                        this.menu_open = false;
                        window.dispatch_action(Box::new(crate::app::OpenSettings), cx);
                        cx.notify();
                    },
                )),
            )
    }
}

#[cfg(test)]
mod tests {
    /// The rail is 228px wide and a path is the one string in it that has no
    /// natural end. Clipping the tail would keep `~/Documents/` — the part every
    /// path shares — and throw away the folder's name.
    #[test]
    fn a_path_too_long_for_the_rail_keeps_its_tail() {
        let elided = super::elide_path_start("~/Documents/work/applications/cvault", 20);
        assert!(elided.starts_with('…'), "{elided}");
        assert!(elided.ends_with("cvault"), "{elided}");
        assert_eq!(elided.chars().count(), 20);
    }

    #[test]
    fn a_path_that_fits_is_left_alone() {
        assert_eq!(
            super::elide_path_start("~/Documents/cvault", 26),
            "~/Documents/cvault"
        );
    }
}
