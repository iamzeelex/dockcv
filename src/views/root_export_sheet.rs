//! The export sheet: one surface that says what is leaving.
//!
//! A bare save dialog proposes the same name for every preset, which is how
//! `CV-final-2.pdf` gets made — by us, at the moment the person is most
//! stressed and least careful. So the format, the preset and what it resolves
//! to section by section, the filename and the folder are all on screen before
//! anything is written, and cancelling writes nothing (P-08, US-18).

use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{
    div, px, AnyElement, ClickEvent, Context, Entity, FontWeight, IntoElement, SharedString, Window,
};

use dockcv_ui_components::{Button, ButtonExt, Card, IconName, Sizable, TextField, TextFieldState};

use crate::resume::export_names::{resolve_destination, Destination, OnCollision};
use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::vault;

use super::root::Root;
use super::shell::pick_dir;

/// Supported export formats in DockCV.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Pdf,
    Docx,
    PlainText,
    Markdown,
    JsonResume,
    Typst,
}

impl ExportFormat {
    pub const ALL: [Self; 6] = [
        Self::Pdf,
        Self::Docx,
        Self::PlainText,
        Self::Markdown,
        Self::JsonResume,
        Self::Typst,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Pdf => "PDF Document",
            Self::Docx => "Microsoft Word",
            Self::PlainText => "Plain Text",
            Self::Markdown => "Markdown",
            Self::JsonResume => "JSON Resume",
            Self::Typst => "Typst Source",
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Self::Pdf => ".pdf",
            Self::Docx => ".docx",
            Self::PlainText => ".txt",
            Self::Markdown => ".md",
            Self::JsonResume => ".json",
            Self::Typst => ".typ",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::Docx => "Word",
            Self::PlainText => "Text",
            Self::Markdown => "Markdown",
            Self::JsonResume => "JSON",
            Self::Typst => "Typst",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Pdf => "Typeset vector PDF",
            Self::Docx => "ATS-friendly Word doc",
            Self::PlainText => "72-column plain text",
            Self::Markdown => "GitHub-ready Markdown",
            Self::JsonResume => "v1.0.0 JSON schema",
            Self::Typst => "Standalone source file",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Docx => "docx",
            Self::PlainText => "txt",
            Self::Markdown => "md",
            Self::JsonResume => "json",
            Self::Typst => "typ",
        }
    }
}

/// What the sheet calls "no preset" — and what export history records for it,
/// so the chip the user clicked and the row they read back say the same thing.
pub(super) const CURRENT_VIEW: &str = "Current view";

/// The `{date}` token, in the only form that sorts correctly in Finder.
pub(super) fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// State of the open Export Sheet.
pub struct ExportSheetState {
    pub format: ExportFormat,
    pub preset_index: Option<usize>,
    /// Where the file goes. The native dialog is now only reached through
    /// "Change…", which is A9's rule: the dialog is for the folder, and the
    /// name is decided here where the user can see what it collides with.
    pub folder: PathBuf,
    /// The filename stem, editable. This is A10's third answer — "edit the
    /// name" — and it is the only place in the app a one-off name can be typed
    /// without changing the document's pattern.
    pub stem: Entity<TextFieldState>,
    /// What the pattern last produced. The field is re-seeded from the pattern
    /// when the format or preset changes, but only while it still holds this —
    /// once the user has typed their own name, changing the preset must not
    /// take it away from them.
    pub seeded: String,
    /// What to do if that name is taken.
    pub on_collision: OnCollision,
}

impl Root {
    /// Open the export sheet overlay.
    ///
    /// Building the filename field needs a `Window`, which the toolbar's
    /// `Export` handler has — the same reason `open_capture_sheet` takes one.
    pub(super) fn open_export_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let preset_name = self
            .active_preset
            .and_then(|idx| self.doc.presets.get(idx))
            .map(|p| p.name.clone());
        let seeded = self
            .doc
            .export_filename_stem(preset_name.as_deref(), None, &today());
        let stem = cx.new(|cx| TextFieldState::single_line(window, cx));
        stem.update(cx, |field, cx| field.seed(seeded.clone(), window, cx));

        self.export_sheet = Some(ExportSheetState {
            format: ExportFormat::Pdf,
            preset_index: self.active_preset,
            folder: self
                .doc
                .export
                .last_destination
                .clone()
                .unwrap_or_else(vault::user_home_dir),
            stem,
            seeded,
            // The answer that cannot lose somebody's file, until they say
            // otherwise about a name they can see.
            on_collision: OnCollision::KeepBoth,
        });
        cx.notify();
    }

    /// Close the export sheet overlay.
    pub(super) fn close_export_sheet(&mut self, cx: &mut Context<Self>) {
        self.export_sheet = None;
        cx.notify();
    }

    /// Re-derive the filename after the format or the preset changed — but only
    /// while the field still holds what the pattern last put there. A name the
    /// user typed is theirs, and switching preset must not take it back.
    fn reseed_export_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = &self.export_sheet else {
            return;
        };
        if sheet.stem.read(cx).value(cx).as_ref() != sheet.seeded {
            return;
        }
        let preset_name = sheet
            .preset_index
            .and_then(|idx| self.doc.presets.get(idx))
            .map(|p| p.name.clone());
        let seeded = self
            .doc
            .export_filename_stem(preset_name.as_deref(), None, &today());
        let field = sheet.stem.clone();
        field.update(cx, |field, cx| field.seed(seeded.clone(), window, cx));
        if let Some(sheet) = self.export_sheet.as_mut() {
            sheet.seeded = seeded;
        }
        cx.notify();
    }

    /// Ask for a different folder. The native dialog's one remaining job.
    pub(super) fn change_export_folder(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = &self.export_sheet else {
            return;
        };
        let receiver = cx.prompt_for_paths(pick_dir());
        let current = sheet.folder.clone();

        cx.spawn(async move |this, cx| {
            let folder = match receiver.await {
                Ok(Ok(Some(mut paths))) if !paths.is_empty() => paths.remove(0),
                _ => return, // cancelled or dialog error: keep the folder we had
            };
            if folder == current {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                if let Some(sheet) = this.export_sheet.as_mut() {
                    sheet.folder = folder;
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Where the current name, format and folder would put the file — and what
    /// is already there.
    pub(super) fn export_destination(&self, cx: &Context<Self>) -> Option<Destination> {
        let sheet = self.export_sheet.as_ref()?;
        let typed = sheet.stem.read(cx).value(cx);
        let stem = crate::resume::export_names::sanitize_filename_stem(typed.as_ref());
        let proposed = sheet
            .folder
            .join(format!("{stem}.{}", sheet.format.extension()));
        Some(resolve_destination(proposed, sheet.on_collision, &[]))
    }

    /// What a preset resolves to, section by section: `(title, variant, hidden)`.
    ///
    /// A9's whole point is that the composition is visible *before* the write —
    /// a wrong-variant export is otherwise discovered by the recruiter.
    fn preset_resolution(&self, preset_index: Option<usize>) -> Vec<(String, String, bool)> {
        let mut doc = self.doc.clone();
        if let Some(idx) = preset_index {
            doc.apply_preset(idx);
        }
        doc.sections()
            .into_iter()
            .map(|kind| {
                let active = doc.active_variant(kind);
                let variant = doc
                    .variant_names(kind)
                    .get(active)
                    .cloned()
                    .unwrap_or_default();
                (
                    doc.section_title(kind),
                    variant,
                    doc.hidden_sections.contains(&kind),
                )
            })
            .collect()
    }

    /// Render the modal Export Sheet.
    pub(super) fn render_export_sheet(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let Some(sheet) = &self.export_sheet else {
            return div().into_any_element();
        };

        let active_format = sheet.format;
        let selected_preset = sheet.preset_index;

        let resolution = self.preset_resolution(selected_preset);
        let folder = sheet.folder.clone();
        let name_field = sheet.stem.clone();
        let on_collision = sheet.on_collision;
        let destination = self.export_destination(cx);
        let collides = destination.as_ref().is_some_and(|d| d.collides);
        let final_name = destination
            .as_ref()
            .and_then(|d| d.target.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let overwrites = destination.as_ref().is_some_and(|d| d.overwrites());

        // 2-column grid of format cards
        let half = ExportFormat::ALL.len() / 2;
        let col1_formats = &ExportFormat::ALL[..half];
        let col2_formats = &ExportFormat::ALL[half..];

        let render_col = |formats: &[ExportFormat], cx: &mut Context<Self>| {
            let theme = *cx.theme();
            div()
                .flex()
                .flex_col()
                .flex_1()
                .gap(px(8.0))
                .children(formats.iter().map(|&fmt| {
                    let is_selected = fmt == active_format;
                    Card::new()
                        .small()
                        .interactive(SharedString::from(format!(
                            "export-fmt-{}",
                            fmt.extension()
                        )))
                        .border_color(if is_selected {
                            theme.accent
                        } else {
                            theme.border
                        })
                        .bg(if is_selected {
                            theme.selected
                        } else {
                            theme.surface
                        })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            if let Some(s) = this.export_sheet.as_mut() {
                                s.format = fmt;
                            }
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .items_start()
                                .w_full()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .w_full()
                                        .child(
                                            div()
                                                .text_size(px(13.0))
                                                .font_weight(if is_selected {
                                                    FontWeight::SEMIBOLD
                                                } else {
                                                    FontWeight::MEDIUM
                                                })
                                                .text_color(if is_selected {
                                                    theme.text
                                                } else {
                                                    theme.text_subtle
                                                })
                                                .child(fmt.title()),
                                        )
                                        .child(
                                            div()
                                                .text_style(TextStyle::chip())
                                                .px(px(4.0))
                                                .py(px(1.0))
                                                .rounded(px(4.0))
                                                .bg(theme.elevated)
                                                .text_color(theme.text_muted)
                                                .child(fmt.badge()),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(theme.text_muted)
                                        .child(fmt.subtitle()),
                                ),
                        )
                }))
        };

        let format_grid = div()
            .flex()
            .gap(px(8.0))
            .child(render_col(col1_formats, cx))
            .child(render_col(col2_formats, cx));

        let presets: Vec<(Option<usize>, String)> =
            std::iter::once((None, CURRENT_VIEW.to_string()))
                .chain(
                    self.doc
                        .presets
                        .iter()
                        .enumerate()
                        .map(|(i, p)| (Some(i), p.name.clone())),
                )
                .collect();

        let preset_chips = div()
            .flex()
            .flex_wrap()
            .gap(px(6.0))
            .children(presets.into_iter().map(|(idx, name)| {
                let is_selected = idx == selected_preset;
                Button::new(SharedString::from(format!(
                    "export-preset-{}",
                    idx.map(|i| i.to_string()).unwrap_or_else(|| "none".into())
                )))
                .chip(is_selected, &theme)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    if let Some(s) = this.export_sheet.as_mut() {
                        s.preset_index = idx;
                    }
                    // The pattern may name the preset, so the filename follows
                    // the chip — unless the user has typed one of their own.
                    this.reseed_export_name(window, cx);
                    cx.notify();
                }))
                .child(name)
            }));

        let panel = div()
            .w(px(520.0))
            .flex()
            .flex_col()
            .rounded(theme.radius_lg())
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_5()
                    .py_3p5()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_style(TextStyle::heading())
                            .text_color(theme.text)
                            .child("Export Document"),
                    )
                    .child(
                        Button::new("export-sheet-close")
                            .icon_only()
                            .icon(IconName::Close)
                            .tooltip("Close")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.close_export_sheet(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .p_5()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_style(TextStyle::label())
                                    .text_color(theme.text_subtle)
                                    .child("Format"),
                            )
                            .child(format_grid),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_style(TextStyle::label())
                                    .text_color(theme.text_subtle)
                                    .child("Preset"),
                            )
                            .child(preset_chips),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .p_3()
                            .rounded(theme.radius_md())
                            .bg(theme.surface)
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_style(TextStyle::eyebrow())
                                    .text_color(theme.text_muted)
                                    .child("This preset resolves to"),
                            )
                            .children(resolution.into_iter().map(|(section, variant, hidden)| {
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_style(TextStyle::body())
                                            .text_color(if hidden {
                                                theme.text_muted
                                            } else {
                                                theme.text
                                            })
                                            .child(section),
                                    )
                                    .child(
                                        div()
                                            .text_style(TextStyle::meta())
                                            .text_color(theme.text_muted)
                                            .child(if hidden {
                                                "hidden".to_string()
                                            } else {
                                                variant
                                            }),
                                    )
                            })),
                    )
                    // The name, editable. A10's third answer is a field rather
                    // than a button: "edit the name" is not a mode to enter, it
                    // is the name sitting there waiting to be typed over.
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_style(TextStyle::label())
                                            .text_color(theme.text_subtle)
                                            .child("Name"),
                                    )
                                    .child(
                                        div()
                                            .text_style(TextStyle::meta())
                                            .text_color(theme.text_muted)
                                            .child(format!(".{}", active_format.extension())),
                                    ),
                            )
                            .child(TextField::new(&name_field).small()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_style(TextStyle::label())
                                            .text_color(theme.text_subtle)
                                            .child("Folder"),
                                    )
                                    .child(
                                        Button::new("export-change-folder")
                                            .quiet()
                                            .text_xs()
                                            .label("Change…")
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _window, cx| {
                                                    this.change_export_folder(cx);
                                                },
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::code())
                                    .text_color(theme.text_muted)
                                    .truncate()
                                    .child(folder.display().to_string()),
                            ),
                    )
                    // Only asked when there is something to answer.
                    .children(collides.then(|| {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .p_3()
                            .rounded(theme.radius_md())
                            .bg(theme.selected)
                            .border_1()
                            .border_color(if overwrites {
                                theme.danger
                            } else {
                                theme.warning
                            })
                            .child(
                                div()
                                    .text_style(TextStyle::body())
                                    .text_color(theme.text)
                                    .child("That name is already taken in this folder."),
                            )
                            .child(div().flex().gap(px(6.0)).children(
                                [OnCollision::KeepBoth, OnCollision::Replace].map(|policy| {
                                    let (id, label) = match policy {
                                        OnCollision::KeepBoth => ("export-keep-both", "Keep both"),
                                        OnCollision::Replace => ("export-replace", "Replace"),
                                    };
                                    Button::new(SharedString::from(id))
                                        .chip(on_collision == policy, &theme)
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, _window, cx| {
                                                if let Some(sheet) = this.export_sheet.as_mut() {
                                                    sheet.on_collision = policy;
                                                }
                                                cx.notify();
                                            },
                                        ))
                                        .child(label)
                                }),
                            ))
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .p_3()
                            .rounded(theme.radius_md())
                            .bg(theme.surface)
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_style(TextStyle::eyebrow())
                                    .text_color(theme.text_muted)
                                    .child(if overwrites { "Replaces" } else { "Writes" }),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::code())
                                    .text_color(if overwrites { theme.danger } else { theme.text })
                                    .child(final_name),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .pt_1()
                            .child(
                                Button::new("export-cancel")
                                    .toolbar()
                                    .label("Cancel")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.close_export_sheet(cx);
                                    })),
                            )
                            .child(
                                Button::new("export-confirm")
                                    .toolbar_primary()
                                    .label(format!("Export {}", active_format.short_name()))
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, _window, cx| {
                                            this.perform_export(cx);
                                        },
                                    )),
                            ),
                    ),
            );

        // Scrim background and centered modal
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
}
