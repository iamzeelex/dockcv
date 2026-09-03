//! The export sheet: one surface that says what is leaving.
//!
//! A bare save dialog proposes the same name for every preset, which is how
//! `CV-final-2.pdf` gets made — by us, at the moment the person is most
//! stressed and least careful. So the format, the preset and what it resolves
//! to section by section, the filename and the folder are all on screen before
//! anything is written, and cancelling writes nothing (P-08, US-18).

use std::path::{Path, PathBuf};

use gpui::prelude::*;
use gpui::{
    div, px, AnyElement, ClickEvent, Context, Entity, FontWeight, IntoElement, SharedString,
    Subscription, Window,
};

use dockcv_ui_components::{
    Button, ButtonExt, Card, Icon, IconName, ScrollableElement, Sizable, TextField, TextFieldEvent,
    TextFieldState, MONO,
};

use crate::config;
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

/// The name to show for a format recorded in export history.
///
/// History stores the extension, so this is the only place a key becomes words.
/// It also answers the labels written before that was settled — a row that says
/// `Word` predates the change and still has to read as Word, not as a mystery.
/// A key from a future version renders as itself: unknown is not the same as
/// gone, and dropping the row would be worse than an unfamiliar word in it.
pub(super) fn format_label(key: &str) -> String {
    match key.to_ascii_lowercase().as_str() {
        "pdf" => "PDF".into(),
        "docx" | "word" => "Word".into(),
        "txt" | "text" => "Plain text".into(),
        "md" | "markdown" => "Markdown".into(),
        "json" => "JSON Resume".into(),
        "typ" | "typst" => "Typst".into(),
        _ => key.to_string(),
    }
}

/// What a preset resolves to, ready to draw.
pub(super) struct Resolution {
    /// How many sections the document has.
    pub total: usize,
    /// The variant most of them sit on, when more than one does.
    pub common: Option<String>,
    /// The ones worth naming: hidden, or on some other variant.
    pub notable: Vec<(String, String, bool)>,
}

impl Resolution {
    /// The one line that stands in for the whole list.
    fn headline(&self) -> String {
        let sections = if self.total == 1 {
            "section"
        } else {
            "sections"
        };
        match (&self.common, self.notable.len()) {
            (Some(common), 0) => format!("{} {sections}, all on {common}", self.total),
            (Some(common), _) => format!(
                "{} {sections}, {} on {common}",
                self.total,
                self.total - self.notable.len()
            ),
            (None, _) => format!("{} {sections}", self.total),
        }
    }
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
    /// Keeps the sheet repainting while the name is typed. The collision notice
    /// and the filename in the footer are answers to what is in the field, and
    /// an answer that arrives on the next unrelated repaint is not an answer.
    /// It also makes Enter mean Export, which is what Enter means in a dialog
    /// whose text field is the last thing you touch.
    pub _subscription: Subscription,
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
        let subscription =
            cx.subscribe(
                &stem,
                |_this, _field, event: &TextFieldEvent, cx| match event {
                    // Deferred, not immediate: exporting closes the sheet, and
                    // the sheet owns this very subscription. Dropping it from
                    // inside its own callback is not a thing to find out about
                    // in the field. By the next tick the handler has returned.
                    TextFieldEvent::Submitted => {
                        let root = cx.entity();
                        cx.defer(move |cx| {
                            root.update(cx, |this, cx| this.perform_export(cx));
                        });
                    }
                    _ => cx.notify(),
                },
            );

        self.export_sheet = Some(ExportSheetState {
            format: ExportFormat::Pdf,
            preset_index: self.active_preset,
            // Where this document went last time, remembered on this machine
            // rather than in the document (A11, and the storage rules).
            folder: config::load()
                .export_destination(&self.doc_path)
                .map(Path::to_path_buf)
                .unwrap_or_else(vault::user_home_dir),
            stem,
            seeded,
            // The answer that cannot lose somebody's file, until they say
            // otherwise about a name they can see.
            on_collision: OnCollision::KeepBoth,
            _subscription: subscription,
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

    /// What a preset resolves to, summarised so a person can scan it.
    ///
    /// A9's point is that a wrong-variant export should be caught by the sender
    /// rather than the recruiter — and a list of ten rows all saying `Base` is
    /// how it stops being caught by anybody. Ten identical rows train the eye to
    /// skip the block, which is exactly where the eleventh, different one hides.
    /// So the common case gets one line, and only the sections that depart from
    /// it are named.
    fn preset_resolution(&self, preset_index: Option<usize>) -> Resolution {
        let mut doc = self.doc.clone();
        if let Some(idx) = preset_index {
            doc.apply_preset(idx);
        }

        let sections: Vec<(String, String, bool)> = doc
            .sections()
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
            .collect();

        // The variant the most sections sit on — the backdrop everything else
        // is an exception to. Hidden sections are exceptions by definition and
        // never vote for it.
        let mut tally: Vec<(&str, usize)> = Vec::new();
        for (_, variant, hidden) in &sections {
            if *hidden {
                continue;
            }
            match tally.iter_mut().find(|(name, _)| name == variant) {
                Some((_, count)) => *count += 1,
                None => tally.push((variant, 1)),
            }
        }
        let common = tally
            .iter()
            .max_by_key(|(_, count)| *count)
            .filter(|(_, count)| *count > 1)
            .map(|(name, _)| (*name).to_string());

        let notable = sections
            .iter()
            .filter(|(_, variant, hidden)| *hidden || common.as_deref() != Some(variant.as_str()))
            .cloned()
            .collect();

        Resolution {
            total: sections.len(),
            common,
            notable,
        }
    }

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
        let overwrites = destination.as_ref().is_some_and(|d| d.overwrites());
        let final_name = destination
            .as_ref()
            .and_then(|d| d.target.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        let person_name = self.doc.profile.active().name.trim();
        let person_role = self.doc.profile.active().label.trim();
        let doc_subtitle = if !person_name.is_empty() && !person_role.is_empty() {
            format!("{person_name} · {person_role}")
        } else if !person_name.is_empty() {
            person_name.to_string()
        } else {
            "Document".to_string()
        };

        // Left sidebar: format selector with rich card indicators
        let format_rail = div()
            .w(px(270.0))
            .flex_none()
            .bg(theme.surface)
            .border_r_1()
            .border_color(theme.border)
            .p_4()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_1()
                    .mb(px(2.0))
                    .child(
                        div()
                            .text_style(TextStyle::eyebrow())
                            .text_color(theme.text_muted)
                            .child("EXPORT FORMAT"),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_muted)
                            .child("6 formats"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .children(ExportFormat::ALL.map(|fmt| {
                        let is_selected = fmt == active_format;
                        Card::new()
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
                                theme.elevated
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                if let Some(s) = this.export_sheet.as_mut() {
                                    s.format = fmt;
                                }
                                this.reseed_export_name(window, cx);
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.0))
                                    .w_full()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .w_full()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(6.0))
                                                    .children(is_selected.then(|| {
                                                        div()
                                                            .w(px(3.0))
                                                            .h(px(14.0))
                                                            .rounded(px(1.5))
                                                            .bg(theme.accent)
                                                    }))
                                                    .child(
                                                        div()
                                                            .text_style(TextStyle::control())
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
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .px(px(6.0))
                                                    .py(px(1.5))
                                                    .rounded(theme.radius_sm())
                                                    .font_family(MONO)
                                                    .text_size(px(11.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .bg(if is_selected {
                                                        theme.accent.opacity(0.18)
                                                    } else {
                                                        theme.hover
                                                    })
                                                    .text_color(if is_selected {
                                                        theme.accent
                                                    } else {
                                                        theme.text_muted
                                                    })
                                                    .child(fmt.badge()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .pl(if is_selected { px(9.0) } else { px(0.0) })
                                            .text_size(px(11.5))
                                            .text_color(if is_selected {
                                                theme.text_subtle
                                            } else {
                                                theme.text_muted
                                            })
                                            .child(fmt.subtitle()),
                                    ),
                            )
                    })),
            );

        // Section 1: Preset selection & section resolution summary
        let notable_count = resolution.notable.len();
        let preset_section = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_style(TextStyle::eyebrow())
                            .text_color(theme.text_muted)
                            .child("PRESET & CONTENT"),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_muted)
                            .child(format!("{} presets", self.doc.presets.len() + 1)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.0))
                    .child({
                        let selected = selected_preset.is_none();
                        Button::new("export-preset-none")
                            .chip(selected, &theme)
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                if let Some(s) = this.export_sheet.as_mut() {
                                    s.preset_index = None;
                                }
                                this.reseed_export_name(window, cx);
                                cx.notify();
                            }))
                            .child(CURRENT_VIEW)
                    })
                    .children(self.doc.presets.iter().enumerate().map(|(i, preset)| {
                        let selected = selected_preset == Some(i);
                        Button::new(SharedString::from(format!("export-preset-{i}")))
                            .chip(selected, &theme)
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                if let Some(s) = this.export_sheet.as_mut() {
                                    s.preset_index = Some(i);
                                }
                                this.reseed_export_name(window, cx);
                                cx.notify();
                            }))
                            .child(preset.name.clone())
                    })),
            )
            .child(
                // Resolution summary card
                div()
                    .rounded(theme.radius_md())
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        // Header banner of the card
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px_3()
                            .py_2()
                            .bg(theme.elevated)
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        Icon::new(IconName::Check).small().text_color(theme.accent),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text)
                                            .child(resolution.headline()),
                                    ),
                            )
                            .child(
                                div()
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .rounded(theme.radius_sm())
                                    .text_size(px(10.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .bg(if notable_count > 0 {
                                        theme.selected
                                    } else {
                                        theme.hover
                                    })
                                    .text_color(if notable_count > 0 {
                                        theme.accent
                                    } else {
                                        theme.text_muted
                                    })
                                    .child(if notable_count > 0 {
                                        format!("{notable_count} custom")
                                    } else {
                                        "all standard".to_string()
                                    }),
                            ),
                    )
                    .children((!resolution.notable.is_empty()).then(|| {
                        div().p_3().flex().flex_col().gap(px(6.0)).children(
                            resolution
                                .notable
                                .into_iter()
                                .map(|(section, variant, hidden)| {
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(12.0))
                                        .py(px(1.5))
                                        .px_1()
                                        .child(
                                            div()
                                                .text_size(px(12.5))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(if hidden {
                                                    theme.text_muted
                                                } else {
                                                    theme.text
                                                })
                                                .child(section),
                                        )
                                        .child(
                                            div()
                                                .px(px(7.0))
                                                .py(px(1.5))
                                                .rounded(theme.radius_sm())
                                                .text_size(px(11.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .bg(if hidden {
                                                    theme.hover
                                                } else {
                                                    theme.selected
                                                })
                                                .border_1()
                                                .border_color(if hidden {
                                                    theme.border
                                                } else {
                                                    theme.accent.opacity(0.3)
                                                })
                                                .text_color(if hidden {
                                                    theme.text_muted
                                                } else {
                                                    theme.accent
                                                })
                                                .child(if hidden {
                                                    "hidden".to_string()
                                                } else {
                                                    variant
                                                }),
                                        )
                                }),
                        )
                    })),
            );

        // Section 2: Output filename and destination folder in an integrated card
        let output_section = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .text_style(TextStyle::eyebrow())
                    .text_color(theme.text_muted)
                    .child("OUTPUT DESTINATION"),
            )
            .child(
                div()
                    .rounded(theme.radius_md())
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .p_3p5()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        // Filename
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.text_subtle)
                                            .child("Filename"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(theme.text_muted)
                                            .child("press Enter to export"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .w_full()
                                    .rounded(theme.radius_md())
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.elevated)
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .child(TextField::new(&name_field).small()),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .px(px(12.0))
                                            .py(px(6.0))
                                            .border_l_1()
                                            .border_color(theme.border)
                                            .bg(theme.surface)
                                            .rounded_r(theme.radius_md())
                                            .font_family(MONO)
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.text_muted)
                                            .child(format!(".{}", active_format.extension())),
                                    ),
                            ),
                    )
                    .child(
                        // Destination Folder
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_subtle)
                                    .child("Save into folder"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(10.0))
                                    .p_2p5()
                                    .rounded(theme.radius_md())
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.elevated)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .min_w_0()
                                            .child(
                                                Icon::new(IconName::Folder)
                                                    .small()
                                                    .text_color(theme.text_muted),
                                            )
                                            .child(
                                                div()
                                                    .font_family(MONO)
                                                    .text_size(px(12.0))
                                                    .text_color(theme.text)
                                                    .truncate()
                                                    .child(folder.display().to_string()),
                                            ),
                                    )
                                    .child(
                                        Button::new("export-change-folder")
                                            .toolbar()
                                            .label("Change…")
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _window, cx| {
                                                    this.change_export_folder(cx);
                                                },
                                            )),
                                    ),
                            ),
                    ),
            );

        // Section 3: Collision Warning (when collides)
        let collision_warning = collides.then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .p_3p5()
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
                        .flex()
                        .gap(px(10.0))
                        .child(
                            div().pt(px(2.0)).child(
                                Icon::new(IconName::TriangleAlert).small().text_color(
                                    if overwrites {
                                        theme.danger
                                    } else {
                                        theme.warning
                                    },
                                ),
                            ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.text)
                                        .child(
                                            "A file with this name already exists in this folder",
                                        ),
                                )
                                .child(
                                    div()
                                        .font_family(MONO)
                                        .text_size(px(12.0))
                                        .text_color(if overwrites {
                                            theme.danger
                                        } else {
                                            theme.text_subtle
                                        })
                                        .child(format!("Target: {final_name}")),
                                ),
                        ),
                )
                .child(div().flex().gap(px(8.0)).pl(px(26.0)).children(
                    [OnCollision::KeepBoth, OnCollision::Replace].map(|policy| {
                        let (id, label) = match policy {
                            OnCollision::KeepBoth => {
                                ("export-keep-both", "Keep both (auto-numbered)")
                            }
                            OnCollision::Replace => ("export-replace", "Replace existing file"),
                        };
                        Button::new(SharedString::from(id))
                            .chip(on_collision == policy, &theme)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                if let Some(sheet) = this.export_sheet.as_mut() {
                                    sheet.on_collision = policy;
                                }
                                cx.notify();
                            }))
                            .child(label)
                    }),
                ))
        });

        let panel = div()
            .w(px(880.0))
            .flex()
            .flex_col()
            .rounded(theme.radius_xl())
            .overflow_hidden()
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .child(
                // Header
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_6()
                    .py_4()
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_style(TextStyle::heading())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child("Export Document"),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::meta())
                                    .text_color(theme.text_muted)
                                    .child(doc_subtitle),
                            ),
                    )
                    .child(
                        Button::new("export-sheet-close")
                            .icon_only()
                            .icon(IconName::Close)
                            .tooltip("Close (Esc)")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.close_export_sheet(cx);
                            })),
                    ),
            )
            .child(
                // Body: 2 columns
                div()
                    .id("export-sheet-body")
                    .max_h(px(580.0))
                    .flex()
                    .child(format_rail)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .p_6()
                            .overflow_y_scrollbar()
                            .flex()
                            .flex_col()
                            .gap(px(18.0))
                            .child(preset_section)
                            .child(output_section)
                            .children(collision_warning),
                    ),
            )
            .child(
                // Footer
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(20.0))
                    .px_6()
                    .py_4()
                    .border_t_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_style(TextStyle::eyebrow())
                                    .text_color(if overwrites {
                                        theme.danger
                                    } else {
                                        theme.text_muted
                                    })
                                    .child(if overwrites {
                                        "REPLACES EXISTING FILE"
                                    } else {
                                        "OUTPUT TARGET"
                                    }),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if overwrites { theme.danger } else { theme.text })
                                    .truncate()
                                    .child(final_name),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
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
                                    .icon(IconName::ArrowDown)
                                    .label(format!("Export {}", active_format.short_name()))
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.perform_export(cx);
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
}
