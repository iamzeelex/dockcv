//! Turning the export sheet's answers into a file on disk.
//!
//! Split from the sheet itself because the two halves fail differently: the
//! sheet is a rendering problem, and this is a compile that can take a second
//! and a write that can run out of disk. Nothing here asks the user anything —
//! every decision was made on the sheet, and re-asking is where those answers
//! get quietly overruled.

use gpui::Context;

use crate::resume::diagnostics::CompileMessage;
use crate::typst_engine::Severity;
use crate::vault;

use super::root::{CompileState, Root};
use super::root_export_sheet::{ExportFormat, CURRENT_VIEW};
use super::save_status;

impl Root {
    /// Write the file the sheet described, and nothing else.
    ///
    /// No save dialog: the sheet already named the file, showed the folder and
    /// answered the collision, and a second dialog asking the same questions
    /// again is where the first one's answers get quietly overruled.
    pub(super) fn perform_export(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = &self.export_sheet else {
            return;
        };
        let Some(destination) = self.export_destination(cx) else {
            return;
        };
        let format = sheet.format;
        let preset_index = sheet.preset_index;
        let folder = sheet.folder.clone();
        let preset_name = preset_index
            .and_then(|idx| self.doc.presets.get(idx))
            .map(|p| p.name.clone());

        let mut export_doc = self.doc.clone();
        if let Some(idx) = preset_index {
            export_doc.apply_preset(idx);
        }

        self.close_export_sheet(cx);

        let path = destination.target;
        let write_path = path.clone();
        let engine = self.engine.clone();
        let executor = cx.background_executor().clone();

        cx.spawn(async move |this, cx| {
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
                        let now = chrono::Local::now();
                        this.doc.record_export(
                            now.format("%Y-%m-%d").to_string(),
                            now.format("%H:%M").to_string(),
                            format.short_name(),
                            preset_title,
                            path.clone(),
                        );
                        this.doc.export.last_destination = Some(folder);
                        save_status::record(cx, "document", vault::save(&this.doc, &this.doc_path));
                    }
                    Err(message) => {
                        // A failed export has to reach the screen. The banner
                        // `render_preview` already draws is the one surface an
                        // export and a compile share, and a silent `log::error!`
                        // leaves the user staring at a sheet that closed and a
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
