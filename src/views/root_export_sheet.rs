//! The export sheet: one surface that says what is leaving.
//!
//! A bare save dialog proposes the same name for every preset, which is how
//! `CV-final-2.pdf` gets made — by us, at the moment the person is most
//! stressed and least careful. So the format, the preset and what it resolves
//! to section by section, the filename and the folder are all on screen before
//! anything is written, and cancelling writes nothing (P-08, US-18).

use gpui::prelude::*;
use gpui::{
    div, px, AnyElement, ClickEvent, Context, FontWeight, IntoElement, SharedString, Window,
};

use dockcv_ui_components::{Button, ButtonExt, Card, IconName, Sizable};

use crate::resume::diagnostics::CompileMessage;
use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::typst_engine::Severity;
use crate::vault;

use super::root::{CompileState, Root};
use super::save_status;

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
const CURRENT_VIEW: &str = "Current view";

/// The `{date}` token, in the only form that sorts correctly in Finder.
pub(super) fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// State of the open Export Sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportSheetState {
    pub format: ExportFormat,
    pub preset_index: Option<usize>,
}

impl Default for ExportSheetState {
    fn default() -> Self {
        Self {
            format: ExportFormat::Pdf,
            preset_index: None,
        }
    }
}

impl Root {
    /// Open the export sheet overlay.
    pub(super) fn open_export_sheet(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.export_sheet = Some(ExportSheetState {
            format: ExportFormat::Pdf,
            preset_index: self.active_preset,
        });
        cx.notify();
    }

    /// Close the export sheet overlay.
    pub(super) fn close_export_sheet(&mut self, cx: &mut Context<Self>) {
        self.export_sheet = None;
        cx.notify();
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

        let preset_name = selected_preset
            .and_then(|idx| self.doc.presets.get(idx))
            .map(|p| p.name.as_str());
        let stem = self.doc.export_filename_stem(preset_name, None, &today());
        let preview_filename = format!("{stem}.{}", active_format.extension());
        let resolution = self.preset_resolution(selected_preset);
        let destination = self
            .doc
            .export
            .last_destination
            .clone()
            .unwrap_or_else(vault::user_home_dir);

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
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    if let Some(s) = this.export_sheet.as_mut() {
                        s.preset_index = idx;
                    }
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
                            }))
                            .child(
                                div()
                                    .mt(px(6.0))
                                    .pt(px(6.0))
                                    .border_t_1()
                                    .border_color(theme.border)
                                    .text_style(TextStyle::eyebrow())
                                    .text_color(theme.text_muted)
                                    .child("Writes"),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::code())
                                    .text_color(theme.text)
                                    .child(preview_filename),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::code())
                                    .text_color(theme.text_muted)
                                    .child(format!("in {}", destination.display())),
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
                                            this.perform_export(active_format, selected_preset, cx);
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

    /// Dispatch the actual export to the file dialog and filesystem.
    pub(super) fn perform_export(
        &mut self,
        format: ExportFormat,
        preset_index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let preset_name = preset_index
            .and_then(|idx| self.doc.presets.get(idx))
            .map(|p| p.name.clone());

        // Clone doc and apply preset if one is selected
        let mut export_doc = self.doc.clone();
        if let Some(idx) = preset_index {
            export_doc.apply_preset(idx);
        }

        let stem = export_doc.export_filename_stem(preset_name.as_deref(), None, &today());
        let suggested = format!("{stem}.{}", format.extension());
        let dir = self
            .doc
            .export
            .last_destination
            .clone()
            .unwrap_or_else(vault::user_home_dir);

        self.close_export_sheet(cx);

        let receiver = cx.prompt_for_new_path(&dir, Some(&suggested));
        let engine = self.engine.clone();
        let executor = cx.background_executor().clone();

        cx.spawn(async move |this, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(path))) => path,
                _ => return, // cancelled or dialog error
            };

            let write_path = path.clone();
            let outcome = executor
                .spawn(async move {
                    let composed = export_doc.compose();
                    match format {
                        ExportFormat::Pdf => {
                            let source = crate::resume::template::generate_for(&export_doc);
                            let mut engine = engine.lock().unwrap_or_else(|e| e.into_inner());
                            engine.set_source(source);
                            let pdf_bytes = engine.compile_to_pdf()?;
                            std::fs::write(&write_path, pdf_bytes)
                                .map_err(|e| format!("write failed: {e}"))
                        }
                        ExportFormat::Docx => {
                            let bytes = crate::resume::export_docx(&composed)
                                .map_err(|e| format!("DOCX generation failed: {e}"))?;
                            std::fs::write(&write_path, bytes)
                                .map_err(|e| format!("write failed: {e}"))
                        }
                        ExportFormat::PlainText => {
                            let text = crate::resume::export_plain_text(&composed);
                            std::fs::write(&write_path, text.as_bytes())
                                .map_err(|e| format!("write failed: {e}"))
                        }
                        ExportFormat::Markdown => {
                            let md = crate::resume::export_markdown(&composed);
                            std::fs::write(&write_path, md.as_bytes())
                                .map_err(|e| format!("write failed: {e}"))
                        }
                        ExportFormat::JsonResume => {
                            let json = crate::resume::export_json_resume(&composed)
                                .map_err(|e| format!("JSON Resume generation failed: {e}"))?;
                            std::fs::write(&write_path, json.as_bytes())
                                .map_err(|e| format!("write failed: {e}"))
                        }
                        ExportFormat::Typst => {
                            let typst = crate::resume::export_typst(&export_doc);
                            std::fs::write(&write_path, typst.as_bytes())
                                .map_err(|e| format!("write failed: {e}"))
                        }
                    }
                })
                .await;

            let preset_title = preset_name.unwrap_or_else(|| CURRENT_VIEW.to_string());
            let _ = this.update(cx, |this, cx| {
                match &outcome {
                    Ok(()) => {
                        log::info!("exported {} to {}", format.short_name(), path.display());
                        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
                        this.doc.record_export(
                            timestamp,
                            format.short_name(),
                            preset_title,
                            path.clone(),
                        );
                        if let Some(parent) = path.parent() {
                            this.doc.export.last_destination = Some(parent.to_path_buf());
                        }
                        save_status::record(cx, "document", vault::save(&this.doc, &this.doc_path));
                    }
                    Err(message) => {
                        // A failed export has to reach the screen. The banner
                        // `render_preview` already draws is the one surface an
                        // export and a compile share, and a silent `log::error!`
                        // leaves the user staring at a dialog that closed and a
                        // file that never appeared.
                        log::error!("export to {} failed: {message}", path.display());
                        this.compile_state = CompileState::Error {
                            messages: vec![CompileMessage {
                                severity: Severity::Error,
                                section: None,
                                text: format!(
                                    "Couldn't export {}: {message}.",
                                    format.short_name()
                                ),
                            }],
                        };
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
