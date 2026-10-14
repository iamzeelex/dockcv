//! Assistant advice and phrasing sheet in the editor.
//!
//! Provides zero-hallucination, user-in-the-loop assistance directly from
//! the editor sidebar and toolbar:
//! - Google XYZ formula formatting ("Accomplished [X] as measured by [Y] by doing [Z]")
//! - Action verb enhancement (eliminates weak/passive phrasing)
//! - ATS keyword and clarity auditing
//! - Full multi-vacancy stepping wizard (polish all vacancies without closing the modal)
//! - Rich context harvesting (role summary, candidate seniority, core stack, target keywords)
//!
//! Reaches the user's preferred local tool (Claude Desktop, Ollama, Codex,
//! Gemini, custom CLI) or copies structured prompts to the clipboard for browser LLMs.

use gpui::prelude::*;
use gpui::{
    div, px, AnyElement, ClickEvent, Context, Entity, FontWeight, IntoElement, SharedString, Window,
};

use dockcv_ui_components::{
    lucide, Button, ButtonExt, Disableable, DropdownMenu, Icon, IconName, PopupMenuItem,
    ScrollableElement, Sizable, Spinner, TextField, TextFieldState, SANS,
};

use crate::config;
use crate::resume::model::{CustomSectionId, SectionKind};
use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::views::assistant;

use super::editor_state::section_label;
use super::Root;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdvicePreset {
    GoogleXyz,
    ActionVerbs,
    AtsClarity,
}

impl AdvicePreset {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::GoogleXyz => "Google XYZ Formula",
            Self::ActionVerbs => "Action Verbs & Impact",
            Self::AtsClarity => "ATS Clarity Audit",
        }
    }

    pub(crate) fn tag(self) -> &'static str {
        match self {
            Self::GoogleXyz => "IMPACT & METRICS",
            Self::ActionVerbs => "DECISIVE ACTION",
            Self::AtsClarity => "ATS SCANNABLE",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::GoogleXyz => "Accomplished [X], measured by [Y], by doing [Z].",
            Self::ActionVerbs => {
                "Lead with strong active verbs; eliminate passive duty statements."
            }
            Self::AtsClarity => "Standard keywords, scannable phrasing, and clear metrics.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdviceSource {
    Proposed,
    Pasted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TargetEntry {
    Work(usize),
    Education(usize),
    Volunteer(usize),
    Custom(CustomSectionId, usize),
    Summary(SectionKind),
}

/// Rich context extracted from the CV to feed into the prompt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EntryContext {
    pub(crate) candidate_name: String,
    pub(crate) candidate_role: String,
    pub(crate) employer_or_org: String,
    pub(crate) position_or_degree: String,
    pub(crate) location: String,
    pub(crate) dates: String,
    pub(crate) role_summary: String,
    pub(crate) key_skills: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AvailableEntry {
    pub(crate) target: TargetEntry,
    pub(crate) title: String,
    pub(crate) section: String,
}

pub(crate) struct AssistantSheet {
    pub(crate) target: TargetEntry,
    pub(crate) preset: AdvicePreset,
    pub(crate) title: String,
    pub(crate) original_items: Vec<String>,
    pub(crate) proposed_items: Vec<String>,
    pub(crate) context_info: EntryContext,
    pub(crate) prompt: String,
    pub(crate) running: bool,
    pub(crate) error: Option<String>,
    pub(crate) keywords_field: Entity<TextFieldState>,
    pub(crate) paste_field: Entity<TextFieldState>,
    pub(crate) applied_feedback: bool,
    pub(crate) copied_feedback: bool,
}

/// Truncate text cleanly with an ellipsis for compact menus and readouts.
pub(crate) fn truncate_text(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        s.to_string()
    } else {
        format!(
            "{}…",
            chars[..max_len.saturating_sub(1)]
                .iter()
                .collect::<String>()
        )
    }
}

/// Tokenize raw text lines into clean bullet point strings.
pub(crate) fn parse_bullets_from_text(raw: &str) -> Vec<String> {
    raw.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("```"))
        .map(|line| {
            let mut s = line;
            if let Some(rest) = s.strip_prefix('•') {
                s = rest.trim_start();
            } else if let Some(rest) = s.strip_prefix('-') {
                s = rest.trim_start();
            } else if let Some(rest) = s.strip_prefix('*') {
                s = rest.trim_start();
            } else if let Some(idx) = s.find(". ") {
                if s[..idx].chars().all(|c| c.is_ascii_digit()) {
                    s = s[idx + 2..].trim_start();
                }
            }
            s.to_string()
        })
        .filter(|line| !line.is_empty())
        .collect()
}

/// Construct a zero-hallucination prompt enriched with candidate and role context.
pub(crate) fn build_bullet_prompt(
    title: &str,
    preset: AdvicePreset,
    bullets: &[String],
    ctx: &EntryContext,
    target_keywords: &str,
) -> String {
    let mut header_context = Vec::new();
    if !ctx.candidate_name.is_empty() {
        if !ctx.candidate_role.is_empty() {
            header_context.push(format!(
                "Candidate: {} ({})",
                ctx.candidate_name, ctx.candidate_role
            ));
        } else {
            header_context.push(format!("Candidate: {}", ctx.candidate_name));
        }
    }
    if !ctx.position_or_degree.is_empty() || !ctx.employer_or_org.is_empty() {
        let mut entry_line = format!(
            "Target Role: {} at {}",
            ctx.position_or_degree, ctx.employer_or_org
        );
        if !ctx.dates.is_empty() {
            entry_line.push_str(&format!(" ({})", ctx.dates));
        }
        if !ctx.location.is_empty() {
            entry_line.push_str(&format!(" [{}]", ctx.location));
        }
        header_context.push(entry_line);
    }
    if !ctx.role_summary.is_empty() {
        header_context.push(format!("Role Overview: {}", ctx.role_summary));
    }
    if !ctx.key_skills.is_empty() {
        header_context.push(format!(
            "Candidate Core Stack & Skills: {}",
            ctx.key_skills.join(", ")
        ));
    }
    if !target_keywords.trim().is_empty() {
        header_context.push(format!(
            "Target Job Posting / Focus Keywords: {}",
            target_keywords.trim()
        ));
    }

    let context_block = if header_context.is_empty() {
        String::new()
    } else {
        format!("CONTEXT:\n{}\n\n", header_context.join("\n"))
    };

    let instructions = match preset {
        AdvicePreset::GoogleXyz => {
            "Rewrite each bullet point using Google's XYZ formula:\n\
             'Accomplished [X], as measured by [Y], by doing [Z]'.\n\
             Emphasize quantifiable business outcomes, technical complexity, and precise action verbs."
        }
        AdvicePreset::ActionVerbs => {
            "Rewrite each bullet point starting with a powerful active verb in the past tense \
             (or present tense if ongoing). Remove passive constructions like 'was responsible for' \
             or 'worked on'. Directly convey ownership and impact."
        }
        AdvicePreset::AtsClarity => {
            "Polish each bullet point for high ATS readability and clarity. Ensure standard \
             industry terminology, scannable phrasing, and concise technical impact aligned with the context."
        }
    };

    let bullets_str = if bullets.is_empty() {
        "(No existing bullets)".to_string()
    } else {
        bullets
            .iter()
            .map(|b| format!("- {b}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "You are an expert executive CV editor. Polish the achievements for '{title}'.\n\n\
         {context_block}\
         TASK:\n\
         {instructions}\n\n\
         CRITICAL RULES (ZERO-HALLUCINATION):\n\
         1. Strictly preserve all facts, companies, technologies, and achievements.\n\
         2. DO NOT invent metrics, numbers, or facts not present in the original text.\n\
         3. If a metric is missing, use a bracketed placeholder like [X%] or [N] so the author can fill in the exact number.\n\
         4. Output ONLY the improved bullet points (one per line starting with '- ').\n\
         5. Do NOT include greetings, intro, outro, or markdown codeblocks.\n\n\
         ORIGINAL ACHIEVEMENTS:\n\
         {bullets_str}\n\n\
         IMPROVED ACHIEVEMENTS:\n"
    )
}

impl Root {
    /// Return all navigable entries across the document.
    pub(crate) fn available_assistant_entries(&self) -> Vec<AvailableEntry> {
        let mut entries = Vec::new();

        // Work Experience entries
        for (i, entry) in self.doc.work.active().iter().enumerate() {
            let title = match (entry.name.is_empty(), entry.position.is_empty()) {
                (false, false) => format!("{} — {}", entry.name, entry.position),
                (false, true) => entry.name.clone(),
                (true, false) => entry.position.clone(),
                (true, true) => format!("Work Experience #{}", i + 1),
            };
            entries.push(AvailableEntry {
                target: TargetEntry::Work(i),
                title,
                section: "Work Experience".to_string(),
            });
        }

        // Education entries
        for (i, entry) in self.doc.education.active().iter().enumerate() {
            let title = match (entry.institution.is_empty(), entry.study_type.is_empty()) {
                (false, false) => format!("{} — {}", entry.institution, entry.study_type),
                (false, true) => entry.institution.clone(),
                (true, false) => entry.study_type.clone(),
                (true, true) => format!("Education #{}", i + 1),
            };
            entries.push(AvailableEntry {
                target: TargetEntry::Education(i),
                title,
                section: "Education".to_string(),
            });
        }

        // Custom sections
        for sec in &self.doc.custom_sections {
            let sec_label = sec.title.clone();
            let sec_title = if sec_label.is_empty() {
                "Custom Section".to_string()
            } else {
                sec_label
            };
            for (i, entry) in sec.content.active().iter().enumerate() {
                let title = if entry.title.is_empty() {
                    format!("{sec_title} #{}", i + 1)
                } else {
                    format!("{sec_title} — {}", entry.title)
                };
                entries.push(AvailableEntry {
                    target: TargetEntry::Custom(sec.id, i),
                    title,
                    section: sec_title.clone(),
                });
            }
        }

        // Volunteer / Organizations
        for (i, entry) in self.doc.volunteer.active().iter().enumerate() {
            let title = match (entry.organization.is_empty(), entry.position.is_empty()) {
                (false, false) => format!("{} — {}", entry.organization, entry.position),
                (false, true) => entry.organization.clone(),
                (true, false) => entry.position.clone(),
                (true, true) => format!("Organization #{}", i + 1),
            };
            entries.push(AvailableEntry {
                target: TargetEntry::Volunteer(i),
                title,
                section: "Organizations".to_string(),
            });
        }

        // Profile Summary
        if !self.doc.profile.active().summary.is_empty() {
            entries.push(AvailableEntry {
                target: TargetEntry::Summary(SectionKind::Profile),
                title: "Profile Summary".to_string(),
                section: "Profile".to_string(),
            });
        }

        entries
    }

    /// Open the assistant sheet for a specific entry or default to current selection.
    pub(crate) fn open_assistant_sheet(
        &mut self,
        target: Option<TargetEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = target.unwrap_or_else(|| {
            // First check the active editor selection so clicking the toolbar
            // respects what the user is currently editing on screen.
            let from_selection = match self.selection.section {
                SectionKind::Work => self.selection.item.map(TargetEntry::Work),
                SectionKind::Education => self.selection.item.map(TargetEntry::Education),
                SectionKind::Organizations => self.selection.item.map(TargetEntry::Volunteer),
                SectionKind::Custom(id) => self.selection.item.map(|i| TargetEntry::Custom(id, i)),
                SectionKind::Profile => Some(TargetEntry::Summary(SectionKind::Profile)),
                _ => None,
            };

            if let Some(t) = from_selection {
                t
            } else if !self.doc.work.active().is_empty() {
                TargetEntry::Work(0)
            } else if !self.doc.education.active().is_empty() {
                TargetEntry::Education(0)
            } else {
                TargetEntry::Summary(SectionKind::Profile)
            }
        });

        let (title, original_items) = self.resolve_assistant_target(&target);
        let context_info = self.resolve_assistant_context(&target);
        let preset = AdvicePreset::GoogleXyz;
        let prompt = build_bullet_prompt(&title, preset, &original_items, &context_info, "");
        let keywords_field = cx.new(|cx| TextFieldState::single_line(window, cx));
        let paste_field = cx.new(|cx| TextFieldState::auto_grow(3, 7, window, cx));

        self.assistant_sheet = Some(AssistantSheet {
            target,
            preset,
            title,
            original_items,
            proposed_items: Vec::new(),
            context_info,
            prompt,
            running: false,
            error: None,
            keywords_field,
            paste_field,
            applied_feedback: false,
            copied_feedback: false,
        });

        cx.notify();
    }

    /// Close the assistant advice sheet.
    pub(crate) fn close_assistant_sheet(&mut self, cx: &mut Context<Self>) {
        self.assistant_sheet = None;
        cx.notify();
    }

    /// Switch active target entry directly within the sheet.
    pub(crate) fn switch_assistant_target(
        &mut self,
        target: TargetEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (title, original_items) = self.resolve_assistant_target(&target);
        let context_info = self.resolve_assistant_context(&target);

        if let Some(sheet) = self.assistant_sheet.as_mut() {
            sheet.target = target;
            sheet.title = title;
            sheet.original_items = original_items;
            sheet.context_info = context_info;
            sheet.proposed_items.clear();
            sheet.error = None;
            sheet.applied_feedback = false;
            sheet.copied_feedback = false;
            sheet.paste_field.update(cx, |state, cx| {
                state.seed(String::new(), window, cx);
            });
        }
        self.refresh_assistant_prompt(cx);
        cx.notify();
    }

    /// Resolve title and text items for a target entry.
    fn resolve_assistant_target(&self, target: &TargetEntry) -> (String, Vec<String>) {
        match target {
            TargetEntry::Work(i) => {
                if let Some(entry) = self.doc.work.active().get(*i) {
                    let title = match (entry.name.is_empty(), entry.position.is_empty()) {
                        (false, false) => format!("{} — {}", entry.name, entry.position),
                        (false, true) => entry.name.clone(),
                        (true, false) => entry.position.clone(),
                        (true, true) => "Work Experience".to_string(),
                    };
                    (title, entry.highlights.clone())
                } else {
                    ("Work Experience".to_string(), Vec::new())
                }
            }
            TargetEntry::Education(i) => {
                if let Some(entry) = self.doc.education.active().get(*i) {
                    let title = match (entry.institution.is_empty(), entry.study_type.is_empty()) {
                        (false, false) => format!("{} — {}", entry.institution, entry.study_type),
                        (false, true) => entry.institution.clone(),
                        (true, false) => entry.study_type.clone(),
                        (true, true) => "Education".to_string(),
                    };
                    (title, entry.highlights.clone())
                } else {
                    ("Education".to_string(), Vec::new())
                }
            }
            TargetEntry::Volunteer(i) => {
                if let Some(entry) = self.doc.volunteer.active().get(*i) {
                    let title = match (entry.organization.is_empty(), entry.position.is_empty()) {
                        (false, false) => format!("{} — {}", entry.organization, entry.position),
                        (false, true) => entry.organization.clone(),
                        (true, false) => entry.position.clone(),
                        (true, true) => "Organizations & Leadership".to_string(),
                    };
                    (title, entry.highlights.clone())
                } else {
                    ("Organizations & Leadership".to_string(), Vec::new())
                }
            }
            TargetEntry::Custom(id, i) => {
                let sec_title = section_label(&self.doc, SectionKind::Custom(*id));
                if let Some(entry) = self
                    .doc
                    .custom_section(*id)
                    .and_then(|sec| sec.content.active().get(*i))
                {
                    let title = if entry.title.is_empty() {
                        sec_title
                    } else {
                        format!("{sec_title} — {}", entry.title)
                    };
                    (title, entry.highlights.clone())
                } else {
                    (sec_title, Vec::new())
                }
            }
            TargetEntry::Summary(sec) => {
                let sec_title = section_label(&self.doc, *sec);
                let items = match sec {
                    SectionKind::Profile => {
                        let sum = self.doc.profile.active().summary.clone();
                        if sum.is_empty() {
                            Vec::new()
                        } else {
                            vec![sum]
                        }
                    }
                    _ => Vec::new(),
                };
                (format!("{sec_title} Summary"), items)
            }
        }
    }

    /// Extract rich candidate and role context for prompt generation.
    fn resolve_assistant_context(&self, target: &TargetEntry) -> EntryContext {
        let key_skills: Vec<String> = self
            .doc
            .skills
            .active()
            .iter()
            .flat_map(|sg| sg.keywords.clone())
            .take(15)
            .collect();

        let mut ctx = EntryContext {
            candidate_name: self.doc.profile.active().name.clone(),
            candidate_role: self.doc.profile.active().label.clone(),
            key_skills,
            ..Default::default()
        };

        match target {
            TargetEntry::Work(i) => {
                if let Some(entry) = self.doc.work.active().get(*i) {
                    ctx.employer_or_org = entry.name.clone();
                    ctx.position_or_degree = entry.position.clone();
                    ctx.location = entry.location.clone();
                    let start = &entry.start_date.text;
                    let end = if entry.end_date.text.is_empty() {
                        "Present"
                    } else {
                        &entry.end_date.text
                    };
                    ctx.dates = if start.is_empty() && end == "Present" {
                        String::new()
                    } else if start.is_empty() {
                        end.to_string()
                    } else {
                        format!("{start} — {end}")
                    };
                    ctx.role_summary = entry.summary.clone();
                }
            }
            TargetEntry::Education(i) => {
                if let Some(entry) = self.doc.education.active().get(*i) {
                    ctx.employer_or_org = entry.institution.clone();
                    ctx.position_or_degree = entry.study_type.clone();
                    let start = &entry.start_date.text;
                    let end = if entry.end_date.text.is_empty() {
                        "Present"
                    } else {
                        &entry.end_date.text
                    };
                    ctx.dates = if start.is_empty() && end == "Present" {
                        String::new()
                    } else if start.is_empty() {
                        end.to_string()
                    } else {
                        format!("{start} — {end}")
                    };
                }
            }
            TargetEntry::Volunteer(i) => {
                if let Some(entry) = self.doc.volunteer.active().get(*i) {
                    ctx.employer_or_org = entry.organization.clone();
                    ctx.position_or_degree = entry.position.clone();
                    let start = &entry.start_date.text;
                    let end = if entry.end_date.text.is_empty() {
                        "Present"
                    } else {
                        &entry.end_date.text
                    };
                    ctx.dates = if start.is_empty() && end == "Present" {
                        String::new()
                    } else if start.is_empty() {
                        end.to_string()
                    } else {
                        format!("{start} — {end}")
                    };
                }
            }
            TargetEntry::Custom(id, i) => {
                if let Some(entry) = self
                    .doc
                    .custom_section(*id)
                    .and_then(|sec| sec.content.active().get(*i))
                {
                    ctx.position_or_degree = entry.title.clone();
                }
            }
            TargetEntry::Summary(SectionKind::Profile) => {
                ctx.employer_or_org = "Candidate Profile".to_string();
                ctx.position_or_degree = self.doc.profile.active().label.clone();
            }
            _ => {}
        }

        ctx
    }

    /// Re-generate the prompt when the preset, context, or target changes.
    pub(crate) fn refresh_assistant_prompt(&mut self, cx: &Context<Self>) {
        if let Some(sheet) = self.assistant_sheet.as_mut() {
            let keywords = sheet.keywords_field.read(cx).value(cx).to_string();
            sheet.prompt = build_bullet_prompt(
                &sheet.title,
                sheet.preset,
                &sheet.original_items,
                &sheet.context_info,
                &keywords,
            );
        }
    }

    /// Trigger the assistant according to the user's preferred route.
    pub(crate) fn run_current_assistant(&mut self, cx: &mut Context<Self>) {
        self.refresh_assistant_prompt(cx);

        let Some(sheet) = self.assistant_sheet.as_mut() else {
            return;
        };
        sheet.error = None;
        sheet.applied_feedback = false;

        let pref = config::load()
            .assistant_preferred
            .unwrap_or_else(|| "auto".to_string());
        let prompt = sheet.prompt.clone();
        let executor = cx.background_executor().clone();
        let cwd = self.doc_path.parent().map(|p| p.to_path_buf());

        // Resolve which assistant to invoke
        match pref.as_str() {
            "browser" => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(prompt.clone()));
                if let Some(sheet) = self.assistant_sheet.as_mut() {
                    sheet.copied_feedback = true;
                }
                cx.notify();
                return;
            }
            "claude-desktop" => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(prompt.clone()));
                let url = format!("claude://cowork/new?q={}", assistant::encode(&prompt));
                cx.open_url(&url);
                if let Some(sheet) = self.assistant_sheet.as_mut() {
                    sheet.copied_feedback = true;
                }
                cx.notify();
                return;
            }
            _ => {}
        }

        // Try local CLI execution
        let (prog, args): (String, Vec<String>) = if pref == "custom" {
            if let Some(custom) = config::load().assistant_custom_cmd {
                let words = assistant::parse_command_words(&custom);
                if words.is_empty() {
                    sheet.error = Some("No custom command specified in Settings.".into());
                    cx.notify();
                    return;
                }
                (words[0].clone(), words[1..].to_vec())
            } else {
                sheet.error = Some("No custom command specified in Settings.".into());
                cx.notify();
                return;
            }
        } else if pref == "ollama" || (pref == "auto" && assistant::is_installed("ollama")) {
            ("ollama".into(), vec!["run".into(), "llama3.2".into()])
        } else if pref == "codex" || (pref == "auto" && assistant::is_installed("codex")) {
            ("codex".into(), vec!["exec".into()])
        } else if pref == "gemini" || (pref == "auto" && assistant::is_installed("gemini")) {
            ("gemini".into(), vec!["-p".into()])
        } else if pref == "claude-cli" || (pref == "auto" && assistant::is_installed("claude")) {
            ("claude".into(), vec!["-p".into()])
        } else if assistant::claude_desktop() {
            // Fallback to desktop app
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(prompt.clone()));
            let url = format!("claude://cowork/new?q={}", assistant::encode(&prompt));
            cx.open_url(&url);
            sheet.copied_feedback = true;
            cx.notify();
            return;
        } else {
            // Fallback to clipboard
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(prompt.clone()));
            sheet.copied_feedback = true;
            sheet.error = Some(
                "No local CLI tools detected. The prompt has been copied to your clipboard!".into(),
            );
            cx.notify();
            return;
        };

        sheet.running = true;
        let prog_display = prog.clone();

        cx.spawn(async move |this, cx| {
            let arg_slices: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            let result = assistant::execute_command(
                &prog,
                &arg_slices,
                &prompt,
                cwd.as_deref(),
                &executor,
            )
            .await;

            let _ = this.update(cx, |this, cx| {
                if let Some(sheet) = this.assistant_sheet.as_mut() {
                    sheet.running = false;
                    match result {
                        Ok(raw) => {
                            let parsed = parse_bullets_from_text(&raw);
                            if parsed.is_empty() {
                                sheet.error = Some(format!(
                                    "{prog_display} returned text without bullet lines. See paste field below."
                                ));
                            } else {
                                sheet.proposed_items = parsed;
                            }
                        }
                        Err(err) => {
                            sheet.error = Some(err);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();

        cx.notify();
    }

    /// Adopt the proposed or pasted highlights into the document without dismissing the sheet.
    pub(crate) fn apply_assistant_advice(
        &mut self,
        source: AdviceSource,
        append: bool,
        cx: &mut Context<Self>,
    ) {
        let (items_to_apply, target) = if let Some(sheet) = self.assistant_sheet.as_ref() {
            let items = match source {
                AdviceSource::Proposed => sheet.proposed_items.clone(),
                AdviceSource::Pasted => {
                    let val = sheet.paste_field.read(cx).value(cx);
                    parse_bullets_from_text(&val)
                }
            };
            (items, sheet.target.clone())
        } else {
            return;
        };

        if items_to_apply.is_empty() {
            cx.notify();
            return;
        }

        self.checkpoint();

        match &target {
            TargetEntry::Work(i) => {
                if let Some(entry) = self.doc.work.active_mut().get_mut(*i) {
                    if append {
                        entry.highlights.extend(items_to_apply);
                    } else {
                        entry.highlights = items_to_apply;
                    }
                }
            }
            TargetEntry::Education(i) => {
                if let Some(entry) = self.doc.education.active_mut().get_mut(*i) {
                    if append {
                        entry.highlights.extend(items_to_apply);
                    } else {
                        entry.highlights = items_to_apply;
                    }
                }
            }
            TargetEntry::Volunteer(i) => {
                if let Some(entry) = self.doc.volunteer.active_mut().get_mut(*i) {
                    if append {
                        entry.highlights.extend(items_to_apply);
                    } else {
                        entry.highlights = items_to_apply;
                    }
                }
            }
            TargetEntry::Custom(id, i) => {
                if let Some(entry) = self
                    .doc
                    .custom_section_mut(*id)
                    .and_then(|sec| sec.content.active_mut().get_mut(*i))
                {
                    if append {
                        entry.highlights.extend(items_to_apply);
                    } else {
                        entry.highlights = items_to_apply;
                    }
                }
            }
            TargetEntry::Summary(SectionKind::Profile) => {
                self.doc.profile.active_mut().summary = items_to_apply.join(" ");
            }
            _ => {}
        }

        self.unsaved = true;
        self.fields_stale = true;
        self.schedule_save(cx);

        let (_, updated_items) = self.resolve_assistant_target(&target);

        // Keep the sheet open and update its highlights and feedback
        if let Some(sheet) = self.assistant_sheet.as_mut() {
            sheet.original_items = updated_items;
            sheet.proposed_items.clear();
            sheet.applied_feedback = true;
        }
        self.refresh_assistant_prompt(cx);
        cx.notify();
    }

    /// Render the assistant advice modal dialog.
    pub(super) fn render_assistant_sheet(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let Some(sheet) = &self.assistant_sheet else {
            return div().into_any_element();
        };

        let title = sheet.title.clone();
        let preset = sheet.preset;
        let running = sheet.running;
        let original_items = sheet.original_items.clone();
        let proposed_items = sheet.proposed_items.clone();
        let error = sheet.error.clone();
        let keywords_field = sheet.keywords_field.clone();
        let paste_field = sheet.paste_field.clone();
        let applied = sheet.applied_feedback;
        let copied = sheet.copied_feedback;

        // Navigation entries for entry-switching wizard
        let nav_entries = self.available_assistant_entries();
        let current_target = &sheet.target;
        let current_index = nav_entries
            .iter()
            .position(|e| &e.target == current_target)
            .unwrap_or(0);
        let total_entries = nav_entries.len();
        let prev_target = if current_index > 0 {
            Some(nav_entries[current_index - 1].target.clone())
        } else {
            None
        };
        let next_target = if current_index + 1 < total_entries {
            Some(nav_entries[current_index + 1].target.clone())
        } else {
            None
        };

        let (current_label, current_section) = match nav_entries.get(current_index) {
            Some(e) => (e.title.clone(), e.section.clone()),
            None => ("Entry".to_string(), "Section".to_string()),
        };

        let pref = config::load()
            .assistant_preferred
            .unwrap_or_else(|| "auto".to_string());
        let (route_display, button_label) = match pref.as_str() {
            "browser" => (
                "Web Browser".to_string(),
                "Copy Prompt for Browser".to_string(),
            ),
            "claude-desktop" => (
                "Claude Desktop".to_string(),
                "Open in Claude Desktop".to_string(),
            ),
            "ollama" => ("Ollama (Local)".to_string(), "Run with Ollama".to_string()),
            "codex" => ("Codex CLI".to_string(), "Run with Codex".to_string()),
            "gemini" => ("Gemini CLI".to_string(), "Run with Gemini".to_string()),
            "claude-cli" => ("Claude CLI".to_string(), "Run with Claude CLI".to_string()),
            "custom" => (
                "Custom Command".to_string(),
                "Run Custom Command".to_string(),
            ),
            _ => {
                if assistant::is_installed("ollama") {
                    (
                        "Ollama (Auto)".to_string(),
                        "Run with Ollama (Local)".to_string(),
                    )
                } else if assistant::claude_desktop() {
                    (
                        "Claude Desktop (Auto)".to_string(),
                        "Open in Claude Desktop".to_string(),
                    )
                } else if assistant::is_installed("codex") {
                    ("Codex (Auto)".to_string(), "Run with Codex".to_string())
                } else {
                    (
                        "Clipboard".to_string(),
                        "Copy Prompt to Clipboard".to_string(),
                    )
                }
            }
        };

        // Preset selector cards: Eyebrow tag at top, title on own row, description below
        let presets = [
            AdvicePreset::GoogleXyz,
            AdvicePreset::ActionVerbs,
            AdvicePreset::AtsClarity,
        ];
        let preset_cards = div().flex().gap_3().children(presets.into_iter().map(|p| {
            let active = p == preset;
            div()
                .id(SharedString::from(format!("preset-choice-{:?}", p)))
                .flex_1()
                .min_w_0()
                .cursor_pointer()
                .p_3()
                .rounded(theme.radius_md())
                .border_1()
                .when(active, |el| {
                    el.bg(theme.selected).border_color(theme.accent)
                })
                .when(!active, |el| {
                    el.bg(theme.surface)
                        .border_color(theme.border)
                        .hover(|el| el.bg(theme.hover))
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    if let Some(sheet) = this.assistant_sheet.as_mut() {
                        sheet.preset = p;
                        sheet.proposed_items.clear();
                        sheet.error = None;
                        sheet.applied_feedback = false;
                    }
                    this.refresh_assistant_prompt(cx);
                    cx.notify();
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .mb(px(3.0))
                        .child(
                            div()
                                .text_style(TextStyle::eyebrow())
                                .text_color(if active {
                                    theme.accent
                                } else {
                                    theme.text_subtle
                                })
                                .child(p.tag()),
                        )
                        .when(active, |el| {
                            el.child(Icon::new(IconName::Check).small().text_color(theme.accent))
                        }),
                )
                .child(
                    div()
                        .font_family(SANS)
                        .text_size(px(13.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if active { theme.text } else { theme.text_muted })
                        .truncate()
                        .mb(px(3.0))
                        .child(p.label()),
                )
                .child(
                    div()
                        .text_style(TextStyle::meta())
                        .text_color(theme.text_subtle)
                        .line_height(gpui::relative(1.25))
                        .child(p.description()),
                )
        }));

        let original_items_copy = original_items.clone();
        let original_count = original_items.len();
        let root = cx.weak_entity();
        let nav_entries_clone = nav_entries.clone();
        let current_target_clone = current_target.clone();

        // Entry switcher sub-header bar
        let entry_switcher_bar = div()
            .flex()
            .items_center()
            .justify_between()
            .px_5()
            .py_2p5()
            .bg(theme.surface)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_none()
                            .text_style(TextStyle::eyebrow())
                            .text_color(theme.text_subtle)
                            .child("VACANCY"),
                    )
                    .child(
                        div().flex_1().min_w_0().max_w(px(460.0)).child(
                            Button::new("assistant-entry-selector")
                                .selector()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1p5()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .font_family(SANS)
                                                .text_size(px(13.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text)
                                                .truncate()
                                                .child(truncate_text(&current_label, 42)),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .text_style(TextStyle::meta())
                                                .text_color(theme.text_subtle)
                                                .child(format!("({current_section})")),
                                        ),
                                )
                                .dropdown_menu(move |menu, _window, _cx| {
                                    let mut menu = menu;
                                    for (idx, entry) in nav_entries_clone.iter().enumerate() {
                                        let target = entry.target.clone();
                                        let root = root.clone();
                                        let is_curr = target == current_target_clone;
                                        let short_title = truncate_text(&entry.title, 36);
                                        let item_label = format!(
                                            "{}. {} ({})",
                                            idx + 1,
                                            short_title,
                                            entry.section
                                        );
                                        menu = menu.item(
                                            PopupMenuItem::new(item_label)
                                                .checked(is_curr)
                                                .on_click(move |_ev, window, cx| {
                                                    let _ = root.update(cx, |this, cx| {
                                                        this.switch_assistant_target(
                                                            target.clone(),
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                }),
                                        );
                                    }
                                    menu
                                }),
                        ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_style(TextStyle::meta())
                            .text_color(theme.text_subtle)
                            .child(format!("{} of {}", current_index + 1, total_entries.max(1))),
                    )
                    .child(
                        Button::new("assistant-prev-entry")
                            .quiet()
                            .icon(IconName::ChevronLeft)
                            .tooltip("Previous entry")
                            .disabled(prev_target.is_none())
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                if let Some(target) = prev_target.clone() {
                                    this.switch_assistant_target(target, window, cx);
                                }
                            })),
                    )
                    .child({
                        let next_target_for_click = next_target.clone();
                        Button::new("assistant-next-entry")
                            .quiet()
                            .icon(IconName::ChevronRight)
                            .tooltip("Next entry")
                            .disabled(next_target.is_none())
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                if let Some(target) = next_target_for_click.clone() {
                                    this.switch_assistant_target(target, window, cx);
                                }
                            }))
                    }),
            );

        let next_target_for_banner = next_target;

        let panel = div()
            .id("assistant-sheet-panel")
            .w(px(760.0))
            .max_h(px(800.0))
            .flex()
            .flex_col()
            .rounded(theme.radius_xl())
            .overflow_hidden()
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .shadow_xl()
            .child(
                // Header
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_5()
                    .py_3p5()
                    .bg(theme.surface)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(px(32.0))
                                    .rounded(theme.radius_md())
                                    .bg(theme.selected)
                                    .border_1()
                                    .border_color(theme.accent)
                                    .child(
                                        lucide("bot")
                                            .with_size(px(18.0))
                                            .text_color(theme.accent),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(1.0))
                                    .child(
                                        div()
                                            .font_family(SANS)
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_size(px(14.5))
                                            .text_color(theme.text)
                                            .child("Assistant Advice & Bullet Polish"),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_style(TextStyle::meta())
                                            .text_color(theme.text_subtle)
                                            .child(title),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .ml_3()
                            .child(
                                Button::new("assistant-sheet-close")
                                    .icon_only()
                                    .icon(IconName::Close)
                                    .tooltip("Close (Esc)")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                        this.close_assistant_sheet(cx);
                                    })),
                            ),
                    ),
            )
            .child(entry_switcher_bar)
            .child(
                // Body
                div()
                    .flex()
                    .flex_col()
                    .gap_3p5()
                    .p_5()
                    .overflow_y_scrollbar()
                    .child(
                        // Preset section
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_style(TextStyle::eyebrow())
                                    .text_color(theme.text_subtle)
                                    .child("IMPROVEMENT STRATEGY"),
                            )
                            .child(preset_cards),
                    )
                    .child(
                        // Target Vacancy Keywords input
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_style(TextStyle::label())
                                            .text_color(theme.text_subtle)
                                            .child("Target Vacancy / Focus Keywords (Optional)"),
                                    )
                                    .child(
                                        div()
                                            .text_style(TextStyle::meta())
                                            .text_color(theme.text_subtle)
                                            .child("Enriches prompt with target JD keywords"),
                                    ),
                            )
                            .child(
                                TextField::new(&keywords_field)
                                    .placeholder("e.g. Airflow DAGs, Distributed Systems, Python 3.12, Latency, AWS…"),
                            ),
                    )
                    .child(
                        // Current bullets card
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .p_3p5()
                            .rounded(theme.radius_md())
                            .bg(theme.canvas)
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_style(TextStyle::eyebrow())
                                                    .text_color(theme.text_subtle)
                                                    .child("CURRENT HIGHLIGHTS"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .px(px(6.0))
                                                    .py(px(1.0))
                                                    .rounded(theme.radius_sm())
                                                    .bg(theme.chip_bg_neutral)
                                                    .text_color(theme.text_subtle)
                                                    .child(format!("{original_count} items")),
                                            ),
                                    )
                                    .child(
                                        Button::new("copy-orig-bullets")
                                            .quiet()
                                            .icon(lucide("copy"))
                                            .label("Copy bullets")
                                            .tooltip("Copy original bullets to clipboard")
                                            .on_click(cx.listener(move |_this, _: &ClickEvent, _window, cx| {
                                                let text = original_items_copy.join("\n• ");
                                                let text = if text.is_empty() {
                                                    String::new()
                                                } else {
                                                    format!("• {text}")
                                                };
                                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(5.0))
                                    .max_h(px(150.0))
                                    .overflow_y_scrollbar()
                                    .children(if original_items.is_empty() {
                                        vec![div()
                                            .text_style(TextStyle::meta())
                                            .text_color(theme.text_subtle)
                                            .py_2()
                                            .child("(No highlights recorded yet)")]
                                    } else {
                                        original_items
                                            .iter()
                                            .map(|item| {
                                                div()
                                                    .flex()
                                                    .items_start()
                                                    .gap_2()
                                                    .py(px(1.5))
                                                    .child(
                                                        div()
                                                            .text_style(TextStyle::meta())
                                                            .text_color(theme.text_subtle)
                                                            .mt(px(2.0))
                                                            .child("•"),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .text_style(TextStyle::prose())
                                                            .text_color(theme.text)
                                                            .line_height(gpui::relative(1.35))
                                                            .child(item.clone()),
                                                    )
                                            })
                                            .collect()
                                    }),
                            ),
                    )
                    .when(applied, |el| {
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .p_3()
                                .rounded(theme.radius_md())
                                .bg(theme.selected)
                                .border_1()
                                .border_color(theme.accent)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(Icon::new(IconName::Check).small().text_color(theme.accent))
                                        .child(
                                            div()
                                                .font_family(SANS)
                                                .text_size(px(13.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.accent)
                                                .child("✓ Changes applied to CV! Undo anytime with Cmd+Z."),
                                        ),
                                )
                                .when_some(next_target_for_banner, |el, target| {
                                    el.child(
                                        Button::new("step-next-entry-btn")
                                            .toolbar_primary()
                                            .icon(IconName::ChevronRight)
                                            .label("Next Entry ›")
                                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                                this.switch_assistant_target(target.clone(), window, cx);
                                            })),
                                    )
                                }),
                        )
                    })
                    .child(
                        // Trigger button bar
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        Button::new("run-assistant-btn")
                                            .action_primary()
                                            .icon(lucide("bot"))
                                            .disabled(running)
                                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                this.run_current_assistant(cx);
                                            }))
                                            .child(if running {
                                                "Running Assistant\u{2026}".to_string()
                                            } else {
                                                button_label
                                            }),
                                    )
                                    .child(
                                        Button::new("copy-assistant-prompt")
                                            .toolbar()
                                            .icon(lucide("copy"))
                                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                this.refresh_assistant_prompt(cx);
                                                if let Some(sheet) = this.assistant_sheet.as_mut() {
                                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                                        sheet.prompt.clone(),
                                                    ));
                                                    sheet.copied_feedback = true;
                                                }
                                                cx.notify();
                                            }))
                                            .child(if copied { "✓ Prompt Copied!" } else { "Copy Full Prompt" }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .text_style(TextStyle::meta())
                                    .text_color(theme.text_subtle)
                                    .child(format!("Target: {route_display}")),
                            ),
                    )
                    .when_some(error, |el, err| {
                        el.child(
                            div()
                                .p_3()
                                .rounded(theme.radius_md())
                                .bg(theme.canvas)
                                .border_1()
                                .border_color(theme.danger)
                                .text_style(TextStyle::meta())
                                .text_color(theme.danger)
                                .child(err),
                        )
                    })
                    .when(running, |el| {
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .gap_3()
                                .py_4()
                                .child(Spinner::new())
                                .child(
                                    div()
                                        .text_style(TextStyle::body())
                                        .text_color(theme.text_muted)
                                        .child("Generating suggestions without hallucinations…"),
                                ),
                        )
                    })
                    .when(!proposed_items.is_empty(), |el| {
                        let proposed_count = proposed_items.len();
                        el.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .p_3p5()
                                .rounded(theme.radius_md())
                                .bg(theme.surface)
                                .border_1()
                                .border_color(theme.accent)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    Icon::new(IconName::Check)
                                                        .small()
                                                        .text_color(theme.accent),
                                                )
                                                .child(
                                                    div()
                                                        .font_family(SANS)
                                                        .text_size(px(13.5))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(theme.accent)
                                                        .child("Suggested Improvements"),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(11.0))
                                                        .px(px(6.0))
                                                        .py(px(1.0))
                                                        .rounded(theme.radius_sm())
                                                        .bg(theme.chip_bg)
                                                        .text_color(theme.chip_fg)
                                                        .child(format!("{proposed_count} items")),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .gap_2()
                                                .child(
                                                    Button::new("apply-asst-replace")
                                                        .toolbar_primary()
                                                        .icon(IconName::Check)
                                                        .label("Replace Highlights")
                                                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                            this.apply_assistant_advice(AdviceSource::Proposed, false, cx);
                                                        })),
                                                )
                                                .child(
                                                    Button::new("apply-asst-append")
                                                        .toolbar()
                                                        .icon(IconName::Plus)
                                                        .label("Append to CV")
                                                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                            this.apply_assistant_advice(AdviceSource::Proposed, true, cx);
                                                        })),
                                                ),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(5.0))
                                        .max_h(px(180.0))
                                        .overflow_y_scrollbar()
                                        .children(proposed_items.iter().map(|item| {
                                            div()
                                                .flex()
                                                .items_start()
                                                .gap_2()
                                                .py(px(1.5))
                                                .child(
                                                    div()
                                                        .text_style(TextStyle::meta())
                                                        .text_color(theme.accent)
                                                        .mt(px(2.0))
                                                        .child("✓"),
                                                )
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .text_style(TextStyle::prose())
                                                        .text_color(theme.text)
                                                        .line_height(gpui::relative(1.35))
                                                        .child(item.clone()),
                                                )
                                        })),
                                ),
                        )
                    })
                    .child(
                        // Manual paste box
                        {
                            let has_pasted = !paste_field.read(cx).value(cx).trim().is_empty();
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .pt_3()
                                .border_t_1()
                                .border_color(theme.border)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(1.0))
                                                .child(
                                                    div()
                                                        .text_style(TextStyle::label())
                                                        .text_color(theme.text_subtle)
                                                        .child("Paste Assistant Output"),
                                                )
                                                .child(
                                                    div()
                                                        .text_style(TextStyle::meta())
                                                        .text_color(theme.text_subtle)
                                                        .child("If using ChatGPT or Claude Desktop, paste the generated bullets here"),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    Button::new("adopt-paste-replace")
                                                        .toolbar_primary()
                                                        .disabled(!has_pasted)
                                                        .label("Adopt & Replace")
                                                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                            this.apply_assistant_advice(AdviceSource::Pasted, false, cx);
                                                        })),
                                                )
                                                .child(
                                                    Button::new("adopt-paste-append")
                                                        .toolbar()
                                                        .disabled(!has_pasted)
                                                        .label("Append")
                                                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                                            this.apply_assistant_advice(AdviceSource::Pasted, true, cx);
                                                        })),
                                                ),
                                        ),
                                )
                                .child(
                                    TextField::new(&paste_field)
                                        .placeholder("Paste raw bullet points from browser or chat here (e.g. • Accomplished X by doing Y)…"),
                                )
                        },
                    ),
            );

        // Scrim overlay with click-outside to dismiss
        div()
            .id("assistant-sheet-backdrop")
            .absolute()
            .inset_0()
            .bg(theme.scrim)
            .flex()
            .items_center()
            .justify_center()
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.close_assistant_sheet(cx);
            }))
            .child(panel.on_click(|_: &ClickEvent, _window, cx| {
                cx.stop_propagation();
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bullets_cleans_prefixes_and_markdown() {
        let raw = "Here are improved points:\n\
                   • Accomplished X by doing Y\n\
                   - Reduced latency by 40%\n\
                   * Led 5 engineers\n\
                   1. Shipped microservices architecture\n\
                   ```\n\
                   ignored code block\n\
                   ```\n\
                   Final line without bullet";
        let parsed = parse_bullets_from_text(raw);
        assert_eq!(
            parsed,
            vec![
                "Here are improved points:",
                "Accomplished X by doing Y",
                "Reduced latency by 40%",
                "Led 5 engineers",
                "Shipped microservices architecture",
                "ignored code block",
                "Final line without bullet",
            ]
        );
    }

    #[test]
    fn prompt_contains_strict_zero_hallucination_rules() {
        let bullets = vec!["Implemented cache layer".into()];
        let ctx = EntryContext::default();
        let prompt = build_bullet_prompt("Acme Corp", AdvicePreset::GoogleXyz, &bullets, &ctx, "");
        assert!(prompt.contains("ZERO-HALLUCINATION"));
        assert!(prompt.contains("DO NOT invent metrics"));
        assert!(prompt.contains("Google's XYZ formula"));
        assert!(prompt.contains("Acme Corp"));
        assert!(prompt.contains("Implemented cache layer"));
    }

    #[test]
    fn action_verbs_preset_instructions() {
        let bullets = vec!["Helped with testing".into()];
        let ctx = EntryContext::default();
        let prompt =
            build_bullet_prompt("Acme Corp", AdvicePreset::ActionVerbs, &bullets, &ctx, "");
        assert!(prompt.contains("powerful active verb"));
        assert!(prompt.contains("Remove passive constructions"));
    }

    #[test]
    fn prompt_incorporates_context_and_keywords() {
        let bullets = vec!["Optimized DB queries".into()];
        let ctx = EntryContext {
            candidate_name: "Jane Doe".into(),
            candidate_role: "Staff Engineer".into(),
            employer_or_org: "Tech Innovations".into(),
            position_or_degree: "Lead Architect".into(),
            location: "Toronto, Canada".into(),
            dates: "2022 — Present".into(),
            role_summary: "Distributed backend systems".into(),
            key_skills: vec!["Rust".into(), "PostgreSQL".into()],
        };
        let prompt = build_bullet_prompt(
            "Tech Innovations",
            AdvicePreset::GoogleXyz,
            &bullets,
            &ctx,
            "Kafka, high-throughput",
        );
        assert!(prompt.contains("Jane Doe"));
        assert!(prompt.contains("Staff Engineer"));
        assert!(prompt.contains("Tech Innovations"));
        assert!(prompt.contains("Toronto, Canada"));
        assert!(prompt.contains("Rust, PostgreSQL"));
        assert!(prompt.contains("Kafka, high-throughput"));
    }
}
