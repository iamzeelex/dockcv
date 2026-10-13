use std::time::Duration;

use gpui::{
    div, ease_out_quint, linear_color_stop, linear_gradient, prelude::*, px, Animation,
    AnimationExt, AnyElement, App, Context, IntoElement, Render, Window,
};

use crate::theme::ActiveTheme;
use crate::views::save_status;

use super::navigation::Screen;
use super::Shell;

fn slide_in(id: &'static str, content: gpui::Div) -> AnyElement {
    content
        .with_animation(
            id,
            Animation::new(Duration::from_millis(240)).with_easing(ease_out_quint()),
            |el, delta| el.opacity(delta).ml(px((1.0 - delta) * 16.0)),
        )
        .into_any_element()
}

impl Shell {
    /// The shared gradient backdrop used by full-screen entry screens.
    pub(crate) fn backdrop(&self, cx: &App) -> gpui::Div {
        let theme = cx.theme();
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(linear_gradient(
                165.0,
                linear_color_stop(theme.background, 0.0),
                linear_color_stop(theme.hover, 1.0),
            ))
    }

    /// Wrap content in a fade + gentle slide-up entrance.
    pub(crate) fn fade_in(&self, id: &'static str, content: gpui::Div) -> impl IntoElement {
        content.with_animation(
            id,
            Animation::new(Duration::from_millis(650)).with_easing(ease_out_quint()),
            |el, delta| el.opacity(delta).mt(px((1.0 - delta) * 18.0)),
        )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_inputs(window, cx);
        self.ensure_vault_watch(window, cx);
        // Before anything draws. Every screen below reads `self.cache` rather
        // than the disk, so this one call is the whole of the vault I/O in a
        // frame — and it does nothing at all unless the directory moved.
        let revision = save_status::vault_revision(cx);
        let vault_changed = self.cache.refresh(self.vault.as_deref(), revision);
        if vault_changed {
            self.refresh_draft_read();
        }
        if let Screen::PresetMatrix(pm) = &mut self.screen {
            if &pm.profiles != self.cache.profiles() {
                pm.profiles = self.cache.profiles().clone();
            }
        }
        // The update settings, read once a launch, and the weekly check if it
        // is due. Not on the pre-vault screens: somebody's first thirty
        // seconds with this app are not the moment to mention versions of it.
        if !matches!(
            self.screen,
            Screen::Opening | Screen::Welcome | Screen::Setup
        ) {
            self.start_update_checks(cx);
        }
        if matches!(self.screen, Screen::Gallery) {
            self.ensure_reading_pages(cx);
        }

        // The vault's screens are **tabs of one window, not separate pages** —
        // the rail stays put and only the main pane changes, which is how the
        // mockup draws every one of them (each row's digest opens with the
        // same `@@ sidebar` block). That is also why none of these screens
        // draws a back control any more: the rail *is* the way back.
        //
        // Outside the chrome: Welcome/Setup (pre-vault, full-bleed), the
        // Editor (its own 46px titlebar, the editor spec §3) and the
        // Preset Matrix (document-scoped, reached from a document and drawn
        // with a breadcrumb rather than the rail — see its own design row).
        let body = match &self.screen {
            // Deliberately empty. This state lasts a frame or two; anything
            // drawn in it would be a flicker, and a spinner for work that is
            // usually instant teaches the user to expect a wait.
            Screen::Opening => self.backdrop(cx).into_any_element(),
            Screen::Welcome => self.render_welcome(cx).into_any_element(),
            Screen::Setup => self.render_setup(cx).into_any_element(),
            Screen::Editor(editor) => editor.clone().into_any_element(),
            Screen::PresetMatrix(pm) => pm.render_matrix(cx).into_any_element(),

            // Import keeps the rail, like every other vault screen.
            //
            // It used to take the whole window as a modal, for a reason that
            // was true of the surface it had: a 560px card inside the
            // gallery's scroll, with the grid competing behind it. The fix was
            // to stop being a card — `import.rs` is the pane — and once
            // it is the pane there is nothing left for the rail to compete
            // with, and no reason for this one flow to be the screen where the
            // product's own furniture disappears.
            Screen::Gallery => {
                let main = self.render_gallery_main(cx).into_any_element();
                self.with_rail(main, window, cx)
            }
            Screen::Library => {
                let main = slide_in("enter-library", self.render_library_screen(cx));
                self.with_rail(main, window, cx)
            }
            Screen::Diary => {
                let main = slide_in("enter-diary", self.render_diary_screen(cx));
                self.with_rail(main, window, cx)
            }
            Screen::Applications => {
                let main = slide_in("enter-applications", self.render_applications_screen(cx));
                self.with_rail(main, window, cx)
            }
        };

        div()
            .size_full()
            // `relative` so the notice can be an overlay: a vault problem is
            // reported without moving anything the user is looking at.
            .relative()
            .bg(cx.theme().background)
            .text_color(cx.theme().text)
            .child(body)
            // Over the matrix, and above the body so the list of files it is
            // about is behind it rather than beside it.
            .children(
                self.batch_export
                    .is_some()
                    .then(|| self.render_batch_export_sheet(cx)),
            )
            // Drawn once, here, rather than per screen — this is the outermost
            // element in the app, so one call covers the gallery, the library,
            // the diary, the board, the matrix *and* the editor.
            .children(save_status::banner(cx))
    }
}
