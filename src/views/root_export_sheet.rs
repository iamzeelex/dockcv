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
    div, px, AnyElement, ClickEvent, Context, Entity, IntoElement, SharedString, Subscription,
    Window,
};

use dockcv_ui_components::{
    Button, ButtonExt, Card, IconName, ScrollableElement, Sizable, TextField, TextFieldEvent,
    TextFieldState,
};

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

        // Formats down the side rather than two-abreast across the body. Six
        // cards in a grid set the panel's width, and then every decision that
        // followed from the choice stacked underneath them set its height — the
        // sheet ran off the top and the bottom of the window. As a rail they
        // cost one column, and what depends on them sits beside them where the
        // two can be read together.
        //
        // One line each: the six names say what they are, and the description
        // of the one actually chosen sits under the rail, where it is read once
        // rather than six times.
        let format_rail = div()
            .w(px(186.0))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_style(TextStyle::eyebrow())
                    .text_color(theme.text_muted)
                    .child("Format"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(ExportFormat::ALL.map(|fmt| {
                        let is_selected = fmt == active_format;
                        Card::new()
                            .xsmall()
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
                                    .items_center()
                                    .justify_between()
                                    .w_full()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .text_style(TextStyle::control())
                                            .text_color(if is_selected {
                                                theme.text
                                            } else {
                                                theme.text_subtle
                                            })
                                            .truncate()
                                            .child(fmt.title()),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_style(TextStyle::meta())
                                            .text_color(if is_selected {
                                                theme.accent
                                            } else {
                                                theme.text_muted
                                            })
                                            .child(fmt.badge()),
                                    ),
                            )
                    })),
            )
            .child(
                div()
                    .pt(px(2.0))
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_muted)
                    .child(active_format.subtitle()),
            );

        let labelled = |label: &'static str, trailing: Option<AnyElement>, body: AnyElement| {
            div()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .min_h(px(18.0))
                        .child(
                            div()
                                .text_style(TextStyle::eyebrow())
                                .text_color(theme.text_muted)
                                .child(label),
                        )
                        .children(trailing),
                )
                .child(body)
        };

        let preset_chips = div()
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
                        // The pattern may name the preset, so the filename
                        // follows the chip — unless the user typed their own.
                        this.reseed_export_name(window, cx);
                        cx.notify();
                    }))
                    .child(preset.name.clone())
            }))
            .into_any_element();

        let resolution_body = div()
            .flex()
            .flex_col()
            .gap(px(3.0))
            .child(
                div()
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_muted)
                    .child(resolution.headline()),
            )
            .children(
                resolution
                    .notable
                    .into_iter()
                    .map(|(section, variant, hidden)| {
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_style(TextStyle::body())
                                    .text_color(if hidden { theme.text_muted } else { theme.text })
                                    .truncate()
                                    .child(section),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_style(TextStyle::meta())
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
            .into_any_element();

        let details = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(labelled("Preset", None, preset_chips))
            .child(labelled("Resolves to", None, resolution_body))
            .child(labelled(
                "Name",
                Some(
                    div()
                        .text_style(TextStyle::meta())
                        .text_color(theme.text_muted)
                        .child(format!(".{}", active_format.extension()))
                        .into_any_element(),
                ),
                // A field, which is A10's third answer: editing the name is not
                // a mode to enter, it is the name waiting to be typed over.
                TextField::new(&name_field).small().into_any_element(),
            ))
            .child(labelled(
                "Folder",
                Some(
                    Button::new("export-change-folder")
                        .quiet()
                        .text_xs()
                        .label("Change…")
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.change_export_folder(cx);
                        }))
                        .into_any_element(),
                ),
                div()
                    .text_style(TextStyle::code())
                    .text_color(theme.text_muted)
                    .truncate()
                    .child(folder.display().to_string())
                    .into_any_element(),
            ))
            .children(collides.then(|| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .p_2p5()
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
                                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                    if let Some(sheet) = this.export_sheet.as_mut() {
                                        sheet.on_collision = policy;
                                    }
                                    cx.notify();
                                }))
                                .child(label)
                        }),
                    ))
            }));

        let panel = div()
            .w(px(640.0))
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
                    .px_4()
                    .py_3()
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
                // A ceiling rather than `flex_1`: the panel sizes to its
                // content, so a child asking to fill it asks for a share of
                // nothing and collapses to zero — which is how the body came to
                // render as an empty strip. The cap is high enough that only a
                // document with many sections on unexpected variants reaches it.
                div()
                    .id("export-sheet-body")
                    .max_h(px(440.0))
                    .overflow_y_scrollbar()
                    .flex()
                    .gap(px(18.0))
                    .p_4()
                    .child(format_rail)
                    .child(details),
            )
            .child(
                // The filename sits with the button that writes it. It is the
                // one thing on this sheet that leaves the building, and reading
                // it in the same glance as the verb is the whole point.
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_baseline()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex_none()
                                    .text_style(TextStyle::eyebrow())
                                    .text_color(if overwrites {
                                        theme.danger
                                    } else {
                                        theme.text_muted
                                    })
                                    .child(if overwrites { "Replaces" } else { "Writes" }),
                            )
                            .child(
                                div()
                                    .text_style(TextStyle::code())
                                    .text_color(if overwrites { theme.danger } else { theme.text })
                                    .truncate()
                                    .child(final_name),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .gap(px(8.0))
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
