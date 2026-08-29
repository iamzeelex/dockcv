//! Toolbar and overlay rendering for `Root` (`docs/design/editor.md` §3–§8).

use gpui::prelude::*;
use gpui::{
    canvas, div, img, px, AnyElement, App, ClickEvent, Context, DispatchPhase, FontWeight,
    IntoElement, PinchEvent, SharedString, TouchPhase, Window,
};

use super::root_preview_chrome::{MAX_ZOOM_PCT, MIN_ZOOM_PCT};
use dockcv_ui_components::{
    Button, ButtonExt, DropdownMenu, Icon, IconName, ListItem, ListItemExt, PopupMenuItem,
    ScrollableElement, Sizable, TextField, CHROME_HEIGHT, SANS,
};

use crate::resume::model::SectionKind;
use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::vault;

use super::root::{ExportPdf, NextPreset, OpenCapture, EDITOR_CONTEXT};
use super::{EditorEvent, Root};

impl Root {
    /// The merged toolbar: breadcrumb identity, the preset menu (P-01), and
    /// Export PDF — one 46px bar, replacing what used to be a breadcrumb bar
    /// plus a separate wrapping row of every preset as a chip (design doc §3,
    /// "Layout notes against the current build").
    pub(super) fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let identity = self.document_identity();

        div()
            .flex()
            .items_center()
            .gap(px(14.0))
            .h(CHROME_HEIGHT)
            // A fixed height inside a flex column is still shrinkable — the bar
            // sits above a `flex_1` pane, and without this the pane's growth
            // squeezes it. The Preset Matrix's bar is in a container that never
            // pressed on it, which is why the same constant showed two heights.
            .flex_none()
            .w_full()
            // Clears macOS's native traffic lights — the window's titlebar
            // is transparent, so content sits directly under them otherwise.
            .pl(px(80.0))
            .pr(px(16.0))
            .bg(theme.chrome)
            .border_b_1()
            .border_color(theme.border)
            .child(
                // Back is a **button**, not a text link. It is the only way
                // out of this screen, it is the control a user reaches for
                // most often after Export, and at 13.5px of muted body text
                // with a `‹` glued to it, it read as decoration rather than
                // something to press.
                Button::new("back-to-gallery")
                    .quiet()
                    .icon(IconName::ChevronLeft)
                    .label("Vault")
                    .tooltip("Back to your CVs")
                    .on_click(cx.listener(|_this, _: &ClickEvent, _window, cx| {
                        cx.emit(EditorEvent::BackToGallery);
                    })),
            )
            .child(
                // Identity, stacked rather than run together with an em dash.
                // `Seán Ó Murchú — Senior Software Engineer` in one 13.5px
                // line next to a `/` separator was the widest, densest thing
                // in the bar and told you least: the person is the same in
                // every document you own. The **file name** is what says
                // which CV this is — the same identity the gallery card now
                // leads with — so it takes the line, and the role sits under
                // it as context.
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .line_height(gpui::relative(1.15))
                    .child(
                        div()
                            .truncate()
                            .font_family(SANS)
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(self.document_file_name()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_subtle)
                            .child(identity),
                    ),
            )
            .child(div().flex_1())
            .child(self.render_preset_control(cx))
            // D-7: quick-capture into the Diary. `.tooltip` is the fix §7
            // names for P-09 — a bare word next to Export PDF read as inert;
            // the tooltip plus the sheet it opens make it legible as "send
            // this document into the Diary".
            // C2: the layout rail is a toggle, not permanent chrome — layout
            // is a last-forty-minutes activity, so it earns space while in use.
            .child(
                Button::new("layout-rail")
                    .toolbar()
                    .gap(px(6.0))
                    .label("Layout")
                    .tooltip("Page size, margins and text scale")
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.toggle_layout_rail(cx);
                    })),
            )
            .child(
                Button::new("capture")
                    .toolbar()
                    .gap(px(6.0))
                    .label("Capture")
                    // P-17 discoverability: the chord is resolved live from
                    // the registered `OpenCapture` binding, so this can never
                    // drift from `views::root::init_keybindings`.
                    .tooltip_with_action(
                        "Save a note about this document to the Diary",
                        &OpenCapture,
                        Some(EDITOR_CONTEXT),
                    )
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_capture_sheet(window, cx);
                    })),
            )
            .child(
                Button::new("export-pdf")
                    .toolbar_primary()
                    .label("Export PDF")
                    .tooltip_with_action(
                        "Export the composed document to PDF",
                        &ExportPdf,
                        Some(EDITOR_CONTEXT),
                    )
                    // The checked path, same as ⌘E and the File menu — an
                    // export that skipped the confidential warning would be
                    // the one that actually leaves the machine.
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.export_pdf_checked(window, cx);
                    })),
            )
    }

    /// The preset control: `Preset  <name>  ▾`, a real menu (P-01) rather than
    /// a static label — lists this document's presets, applies one, offers
    /// "＋ Save as preset", and is a door into the Preset Matrix screen
    /// (`EditorEvent::OpenPresetMatrix`, handled by `Shell`).
    fn render_preset_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let value = self
            .active_preset
            .and_then(|i| self.doc.preset_name(i))
            .cloned()
            .unwrap_or_else(|| "No preset".to_string());
        let presets: Vec<String> = self.doc.presets.iter().map(|p| p.name.clone()).collect();
        let active_preset = self.active_preset;
        let root = cx.weak_entity();

        Button::new("preset-control")
            .selector()
            // P-17: cycling presets from the keyboard has no other visible
            // control to hang a hint on but this one; `PrevPreset`'s chord
            // (Alt+Shift+Up) is the mirror of the one shown here and is named
            // in the text since a tooltip only resolves one action's binding.
            .tooltip_with_action(
                "Cycle to the next preset (Alt+Shift+Up for the previous)",
                &NextPreset,
                Some(EDITOR_CONTEXT),
            )
            .child(
                div()
                    .font_family(SANS)
                    .text_size(px(10.5))
                    .text_color(theme.text_subtle)
                    .child("PRESET"),
            )
            .child(
                div()
                    .font_family(SANS)
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(value),
            )
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu;
                if presets.is_empty() {
                    menu = menu.item(PopupMenuItem::label("No presets yet"));
                } else {
                    for (i, name) in presets.iter().enumerate() {
                        let root = root.clone();
                        menu = menu.item(
                            PopupMenuItem::new(name.clone())
                                .checked(active_preset == Some(i))
                                .on_click(move |_ev, window, cx| {
                                    let _ = root.update(cx, |this, cx| {
                                        this.apply_preset(i, window, cx);
                                    });
                                }),
                        );
                    }
                }
                if active_preset.is_some() {
                    menu = menu.item(PopupMenuItem::new("Remove current preset").on_click({
                        let root = root.clone();
                        move |_ev, _window, cx| {
                            let _ = root.update(cx, |this, cx| {
                                this.remove_active_preset(cx);
                            });
                        }
                    }));
                }
                menu = menu.separator();
                menu = menu.item(PopupMenuItem::new("＋ Save as preset").on_click({
                    let root = root.clone();
                    move |_ev, _window, cx| {
                        let _ = root.update(cx, |this, cx| {
                            this.save_current_as_preset(cx);
                        });
                    }
                }));
                menu = menu.separator();
                menu.item(PopupMenuItem::new("Preset Matrix…").on_click({
                    let root = root.clone();
                    move |_ev, _window, cx| {
                        let _ = root.update(cx, |_this, cx| {
                            cx.emit(EditorEvent::OpenPresetMatrix);
                        });
                    }
                }))
            })
            .into_any_element()
    }

    /// The modal overlay listing diary entries for inserting one as a highlight.
    pub(super) fn render_diary_overlay(
        &self,
        cx: &mut Context<Self>,
        work_index: usize,
    ) -> AnyElement {
        let theme = *cx.theme();

        let list: AnyElement = if self.diary.entries.is_empty() {
            div()
                .p_4()
                .text_sm()
                .text_color(theme.text_muted)
                .child("Your diary is empty. Log achievements in the Diary screen first.")
                .into_any_element()
        } else {
            div()
                .id("diary-list")
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .max_h(px(340.0))
                .overflow_y_scrollbar()
                .children(self.diary.entries.iter().enumerate().map(|(i, entry)| {
                    let date = entry.date.clone();
                    let text = entry.text.clone();
                    ListItem::new(SharedString::from(format!("diaryitem-{i}")))
                        .row()
                        .py_2()
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.insert_diary_highlight(work_index, i, window, cx);
                        }))
                        // A date over the win itself — one flex column, since
                        // `ListItem` hands its children to a block div.
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().text_xs().text_color(theme.accent).child(date))
                                .child(div().text_sm().text_color(theme.text).child(text)),
                        )
                }))
                .into_any_element()
        };

        let panel = div()
            // 560, not 460: the role chips are `employer · position` and a CV
            // with six jobs fills the panel with them. The extra 100px is the
            // difference between most chips reading whole and most of them
            // ending in an ellipsis.
            .w(px(560.0))
            .flex()
            .flex_col()
            .rounded(theme.radius_md())
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text)
                            .child("Add from diary"),
                    )
                    .child(
                        Button::new("diary-close")
                            .icon_only()
                            .icon(IconName::Close)
                            .tooltip("Close")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.diary_picker = None;
                                cx.notify();
                            }))
                            .child(Icon::new(IconName::Close).with_size(cx.theme().icon_sm())),
                    ),
            )
            .child(list);

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.scrim)
            .child(panel)
            .into_any_element()
    }

    /// The modal overlay listing library blocks for the open picker section.
    pub(super) fn render_library_overlay(
        &self,
        cx: &mut Context<Self>,
        section: SectionKind,
    ) -> AnyElement {
        let theme = *cx.theme();
        let labels = self.library_labels(section);

        let list: AnyElement = if labels.is_empty() {
            div()
                .p_4()
                .text_sm()
                .text_color(theme.text_muted)
                .child("No saved blocks yet. Use ★ on an entry to add it to your library.")
                .into_any_element()
        } else {
            div()
                .id("library-list")
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .max_h(px(340.0))
                .overflow_y_scrollbar()
                .children(labels.into_iter().enumerate().map(|(i, label)| {
                    ListItem::new(SharedString::from(format!("libitem-{i}")))
                        .row()
                        .text_color(theme.text)
                        .hover(|s| s.bg(theme.surface))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.insert_library_block(section, i, window, cx);
                        }))
                        .child(label)
                }))
                .into_any_element()
        };

        let panel = div()
            .w(px(460.0))
            .flex()
            .flex_col()
            .rounded(theme.radius_md())
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text)
                            .child(format!("Add from library ({section:?})")),
                    )
                    .child(
                        Button::new("library-close")
                            .icon_only()
                            .icon(IconName::Close)
                            .tooltip("Close")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.library_picker = None;
                                cx.notify();
                            }))
                            .child(Icon::new(IconName::Close).with_size(cx.theme().icon_sm())),
                    ),
            )
            .child(list);

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.scrim)
            .child(panel)
            .into_any_element()
    }

    /// D-7: the quick-capture sheet behind the toolbar's `Capture` button — a
    /// note, today's date, and which document it will be linked to
    /// (`DiaryEntry::source_doc`, the fix for P-05). Not drawn by the mockup
    /// (design doc §10 leaves its exact shape open); this is the minimal
    /// reading that satisfies "Cancel and Save, plus a visible link".
    pub(super) fn render_capture_sheet(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let Some(sheet) = &self.capture_sheet else {
            return div().into_any_element();
        };
        let text = sheet.text.clone();
        let identity = self.document_identity();
        let date = vault::today_iso();

        // Inline chips rather than a menu: a popover opened from inside a
        // scrimmed sheet renders into the window's overlay layer, under the
        // scrim, and a picker you cannot see is worse than a row that costs
        // four more lines (the same finding as `diary_use`). It also suits the
        // data — a CV holds a handful of jobs, not a hundred.
        let chosen = sheet.role.clone();
        let roles = self.capture_roles();
        let chip = |label: SharedString, value: String, selected: bool, cx: &mut Context<Self>| {
            Button::new(SharedString::from(format!("capture-role-{value}")))
                .chip(selected, &theme)
                // A role is `employer · position` and a job title can run to a
                // sentence; without a ceiling the longest one pushes the chip
                // straight out of the panel. `flex_wrap` wraps between chips,
                // never inside one.
                // The chip sizes itself and the *text inside it* truncates.
                // Putting `truncate()` on the chip made every chip ellipsise,
                // including ones with room to spare: it sets `overflow_hidden`,
                // which gives the box an automatic minimum of zero, and a box
                // that may collapse does. With the clip one level in, the chip
                // keeps its content width until `max_w` binds — and `max_w`
                // sits under the panel's content width, so a capped chip wraps
                // instead of leaving the panel. `flex_none` on top, because a
                // flex item shrinks by default and two chips sharing a wrapped
                // line would otherwise split it between them.
                .flex_none()
                .max_w(px(496.0))
                .rounded_full()
                .border_1()
                .border_color(if selected { theme.accent } else { theme.border })
                .text_color(if selected {
                    theme.text
                } else {
                    theme.text_muted
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    if let Some(sheet) = this.capture_sheet.as_mut() {
                        sheet.role = value.clone();
                    }
                    cx.notify();
                }))
                .child(div().truncate().child(label))
        };

        let role_picker = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_style(TextStyle::label())
                    .text_color(theme.text_subtle)
                    .child("Role"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.0))
                    .child(chip("No role".into(), String::new(), chosen.is_empty(), cx))
                    .children(roles.into_iter().map(|role| {
                        let selected = role == chosen;
                        chip(SharedString::from(role.clone()), role, selected, cx)
                    })),
            );

        let panel = div()
            .w(px(460.0))
            .flex()
            .flex_col()
            .rounded(theme.radius_md())
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_style(TextStyle::heading())
                            .text_color(theme.text)
                            .child("Capture to Diary"),
                    )
                    .child(
                        Button::new("capture-close")
                            .icon_only()
                            .icon(IconName::Close)
                            .tooltip("Close")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.close_capture_sheet(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .p_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .text_style(TextStyle::label())
                                            .text_color(theme.text_subtle)
                                            .child("Linked to"),
                                    )
                                    .child(
                                        div()
                                            .text_style(TextStyle::body())
                                            .text_color(theme.text)
                                            .child(identity),
                                    ),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::meta())
                                    .text_color(theme.text_subtle)
                                    .child(date),
                            ),
                    )
                    .child(
                        TextField::new(&text)
                            .placeholder("What happened? Shipped a fix, landed an offer…"),
                    )
                    .child(role_picker)
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                Button::new("capture-cancel")
                                    .toolbar()
                                    .label("Cancel")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.close_capture_sheet(cx);
                                    })),
                            )
                            .child(
                                Button::new("capture-save")
                                    .toolbar_primary()
                                    .label("Save")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.commit_capture(cx);
                                    })),
                            ),
                    ),
            );

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.scrim)
            .child(panel)
            .into_any_element()
    }

    /// The preview pane: the paper framed as a document on a desk, not
    /// another UI panel (L-08, design doc §3–§4). The pane's own backdrop is
    /// `canvas` — deliberately deeper than `background` — so the sheet reads
    /// as sitting apart from the chrome around it.
    pub(super) fn render_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        let content: AnyElement = if let Some(err) = self.compile_error_text() {
            div()
                .max_w(px(520.0))
                .p_4()
                .rounded(theme.radius_md())
                .bg(theme.elevated)
                .border_1()
                .border_color(theme.danger)
                .text_xs()
                .font_family(crate::theme::MONO)
                .text_color(theme.danger)
                .child(err)
                .into_any_element()
        } else if let Some(rendered) = &self.rendered {
            if rendered.width <= 0.0 || rendered.height <= 0.0 {
                div()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child("rendering…")
                    .into_any_element()
            } else {
                let ratio = rendered.height / rendered.width;
                // Zoom scales the sheet the compiler already rasterized, so
                // the control is instant — it does not wait on a recompile,
                // which is what makes it feel like looking closer rather than
                // changing the document.
                let target_w = self.preview_width();
                let target_h = target_w * ratio;

                // The "document on a desk" framing: a tight radius (an order
                // of magnitude under the app chrome's) and a hard drop shadow.
                //
                // Deliberately **no background fill and no border**. Each page
                // is opaque on its own (`#set page(fill: white)`), so a sheet
                // behind the whole stack would only show through the gaps
                // between pages — which is exactly what made the page break
                // read as a pale band instead of a break. With no fill, the
                // gap is the canvas, and two pages look like two pages.
                div()
                    .rounded(px(2.0))
                    .shadow_lg()
                    .child(img(rendered.image.clone()).w(px(target_w)).h(px(target_h)))
                    .into_any_element()
            }
        } else {
            div()
                .text_sm()
                .text_color(theme.text_muted)
                .child("compiling preview…")
                .into_any_element()
        };

        div()
            .id("preview-pane")
            .relative()
            .flex_1()
            // `min_h_0` and no `h_full`: a flex child refuses to shrink below
            // its content without it, so the pane grew past the window and the
            // toolbar — anchored 16px above the *pane's* bottom — hung below
            // the visible area. It was already clipped by the window edge
            // before the chrome bar went from 46px to 56px; that pushed it out
            // entirely. `h_full` alongside `flex_1` was the redundant half of
            // the same instruction.
            .min_h_0()
            .bg(theme.canvas)
            // **First**, not last. `Canvas::paint` runs `style.paint(bounds, …)`
            // for the element's own box, so a full-size canvas added after the
            // toolbar is a layer over it. Painting order decides which of two
            // overlapping siblings the user sees; window-level mouse listeners
            // do not care about it, so the gesture is unaffected by the move.
            .child(self.pinch_listener(cx))
            .child(
                div()
                    .id("preview-scroll")
                    .size_full()
                    .flex()
                    // Top-aligned, not centered: the mockup's own framing —
                    // a page sits at the top of a desk, it doesn't float in
                    // the middle of it.
                    .items_start()
                    .justify_center()
                    .overflow_y_scrollbar()
                    .pt(px(34.0))
                    .pb(px(34.0))
                    .child(content),
            )
            // C2: the layout rail floats over the canvas rather than taking a
            // column, so opening it does not re-centre the page you opened it
            // to measure. See `root_layout_rail.rs` for that ruling (O-1).
            .children(self.layout_rail_open.then(|| self.render_layout_rail(cx)))
            .child(self.render_preview_toolbar(cx))
    }

    /// Trackpad pinch over the preview.
    ///
    /// `PinchEvent` is a window-level event — `div()` has no `on_pinch`, and
    /// upstream exposes it only through `Window::on_mouse_event`, which may be
    /// called from the paint phase. A zero-size `canvas` is the hook: its paint
    /// closure runs each frame with the pane's own bounds, so the listener can
    /// ignore a pinch that happened over the fields panel.
    ///
    /// **Nothing recompiles while the fingers are down.** A pinch reports many
    /// events a second; rasterizing each one would stall the gesture and put
    /// the compiler under a load nothing reads. The existing bitmap stretches
    /// for the duration — that is what makes the gesture feel attached to the
    /// page — and one sharp pass is scheduled when the gesture *ends*.
    fn pinch_listener(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let root = cx.weak_entity();
        canvas(
            |_, _, _| (),
            move |bounds, _, window: &mut Window, _cx: &mut App| {
                let root = root.clone();
                window.on_mouse_event(move |event: &PinchEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble || !bounds.contains(&event.position) {
                        return;
                    }
                    let _ = root.update(cx, |this, cx| {
                        // `delta` is a fraction: 0.1 means "10% larger".
                        this.zoom_pct =
                            (this.zoom_pct * (1.0 + event.delta)).clamp(MIN_ZOOM_PCT, MAX_ZOOM_PCT);
                        cx.notify();
                        if event.phase == TouchPhase::Ended {
                            this.schedule_recompile(window, cx);
                        }
                    });
                });
            },
        )
        // Explicit insets rather than `size_full()`: inside an absolutely
        // positioned element the two are not the same thing, and a canvas whose
        // box resolves to zero would hit-test nothing — the gesture would
        // simply never fire.
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
    }

    // The preview toolbar is not rendered yet. Its frame alone was an empty
    // floating pill under the document — visual noise that looks like a broken
    // control, which is the same complaint as review P-09. It comes back with
    // the zoom controls (US-07) and the page counter (US-08), both specced in
    // `docs/design/typst-controls.md`; `PageGeometry::page_count` is already
    // available for the latter.
}
