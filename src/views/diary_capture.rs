//! US-34, the universal adapter: paste whatever you already write, and get
//! candidate wins out of it.
//!
//! The review's premise for this whole surface is that the persona **will not
//! keep a diary by hand** — they already write a weekly status to their
//! manager, and asking them to write it twice is how a diary dies. So: a box
//! that takes a status email, standup notes, a retro, a self-review draft, and
//! splits it into candidates. *«работает в любой профессии и не требует ни
//! одной интеграции»*.
//!
//! **No model, and none needed.** The story is often read as an AI feature —
//! it is filed under `Diary / ИИ` — but the mechanical half is the half that
//! matters: a status report is already a list of things you did, one per line
//! or one per paragraph. Splitting it is text processing, and the person is the
//! one who decides what counts (US-12: *«Да / Нет / Поправить, ничего не
//! попадает в дневник без моего Да»*). A model would improve the wording, not
//! the mechanism, and it can be added later behind the same triage queue.
//!
//! ## What counts as one candidate
//!
//! Two shapes cover almost everything people paste:
//!
//! * **A bullet is one candidate.** A status report is a list; each item is one
//!   win. Its glyph is dropped so the diary stores the words, not the `•`.
//! * **A run of plain lines is one candidate.** An email is prose, hard-wrapped
//!   at some width nobody chose; joining a paragraph back together is what
//!   makes it one thought rather than four fragments.
//!
//! Lines that are plainly not wins are dropped — a heading, a date, a bare name
//! — by shape alone, never by meaning. The rules are listed on [`is_scaffolding`]
//! and each is one a person would agree with on sight; anything less obvious is
//! left in for the author to reject, because a candidate they skip costs a
//! click and a win silently dropped costs the feature its point.
//!
//! Every candidate carries the **verbatim source** it came from, which US-12
//! requires (*«каждый черновик показывает источник со ссылкой/цитатой»*) and
//! which is also the only way to check the split did not mangle anything.

use crate::import::layout::{starts_with_bullet, without_bullet};

/// ASCII list markers, which people type and typesetters do not.
///
/// `import::layout` recognises only the typographic glyphs (`•`, `▪`, `‣`, …)
/// and that is right for its job: it reads PDFs and DOCX, where a leading `-`
/// is as likely to be a hyphenated word carried across a line break as a list
/// marker. Here the input is something a person typed into an email or a
/// notes app, where `- ` and `* ` are exactly what a list looks like — so the
/// set is widened locally rather than loosened for the importer, which would
/// make it split words in half.
const ASCII_MARKERS: [char; 3] = ['-', '*', '+'];

/// Whether a typed line opens a list item, and how many characters the marker
/// takes. Numbered items (`1.`, `2)`) count too — a status report is as often
/// numbered as bulleted.
fn list_marker_len(line: &str) -> Option<usize> {
    if starts_with_bullet(line) {
        return Some(line.chars().next().map(char::len_utf8).unwrap_or(0));
    }

    let mut chars = line.chars();
    match chars.next() {
        // `- item`, but not `-- item` (a rule) and not `-5 degrees`.
        Some(c) if ASCII_MARKERS.contains(&c) => {
            matches!(chars.next(), Some(' ') | Some('\t')).then_some(c.len_utf8())
        }
        // `1. item` / `12) item`
        Some(c) if c.is_ascii_digit() => {
            let digits = line.chars().take_while(char::is_ascii_digit).count();
            let rest = &line[digits..];
            let mut rest_chars = rest.chars();
            match (rest_chars.next(), rest_chars.next()) {
                (Some('.') | Some(')'), Some(' ')) => Some(digits + 1),
                _ => None,
            }
        }
        _ => None,
    }
}

/// One thing the paste might be a win about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Candidate {
    /// The text as it would enter the diary: bullet glyph gone, a wrapped
    /// paragraph rejoined, whitespace collapsed.
    pub text: String,
    /// Exactly what was pasted, as pasted. Shown under the candidate so the
    /// author can see what it was drawn from, and never edited.
    pub quote: String,
}

/// Below this, a line is a label rather than an achievement.
///
/// Tuned to the shortest real win anyone writes — *"Shipped the billing fix"*
/// is 24 characters — rather than to a round number.
const MIN_CANDIDATE_CHARS: usize = 24;

/// Whether a line is structure rather than content.
///
/// Every rule here is about **shape**, never meaning, and each is one a person
/// would agree with without being told the heuristic:
///
/// * a heading ends with `:` and carries no sentence after it
/// * a date line is a date and nothing else
/// * `---`, `===`, `***` are rules, not writing
/// * a line in block capitals is a header, not a paragraph
fn is_scaffolding(line: &str) -> bool {
    let line = line.trim();
    if line.is_empty() {
        return true;
    }
    if line.ends_with(':') {
        return true;
    }
    // A separator: nothing but punctuation.
    if line.chars().all(|c| !c.is_alphanumeric()) {
        return true;
    }
    // A date line — `2026-08-11`, `Week of 11 Aug`, `11/08/2026` — is a header
    // wherever it appears alone. Detected as "mostly digits and separators".
    let letters = line.chars().filter(|c| c.is_alphabetic()).count();
    if letters <= 4 && line.chars().any(|c| c.is_ascii_digit()) {
        return true;
    }
    // BLOCK CAPITALS with no lowercase at all. Guarded on having several
    // letters so an acronym-only line ("SRE") is not the deciding case.
    if letters >= 4 && !line.chars().any(|c| c.is_lowercase()) {
        return true;
    }
    false
}

/// Collapse the internal whitespace of a rejoined paragraph.
fn tidy(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Split pasted text into candidate wins.
///
/// Order is the order they appeared: the author is reading down their own
/// status report, and re-sorting it would make them hunt.
pub(super) fn candidates(pasted: &str) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    // Lines of the paragraph currently being accumulated, verbatim.
    let mut paragraph: Vec<&str> = Vec::new();

    fn flush(paragraph: &mut Vec<&str>, out: &mut Vec<Candidate>) {
        if paragraph.is_empty() {
            return;
        }
        let quote = paragraph.join("\n");
        let text = tidy(&paragraph.join(" "));
        paragraph.clear();
        if text.chars().count() >= MIN_CANDIDATE_CHARS {
            out.push(Candidate { text, quote });
        }
    }

    for raw in pasted.lines() {
        let line = raw.trim();

        if line.is_empty() {
            flush(&mut paragraph, &mut out);
            continue;
        }

        if let Some(marker) = list_marker_len(line) {
            // A list item begins a new candidate whatever came before it.
            flush(&mut paragraph, &mut out);
            // `without_bullet` also handles the typographic glyphs and the
            // space after any marker.
            let text = tidy(without_bullet(&line[marker..]));
            if text.chars().count() >= MIN_CANDIDATE_CHARS {
                out.push(Candidate {
                    text,
                    quote: raw.trim_end().to_string(),
                });
            }
            continue;
        }

        if is_scaffolding(line) {
            // Structure ends whatever paragraph was running and contributes
            // nothing of its own.
            flush(&mut paragraph, &mut out);
            continue;
        }

        paragraph.push(raw.trim_end());
    }
    flush(&mut paragraph, &mut out);

    out
}

// --- the triage sheet ----------------------------------------------------

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, Entity, IntoElement, SharedString, Window};

use dockcv_ui_components::{Button, ButtonExt, TextField, TextFieldState};

use crate::resume::model::DiaryEntry;
use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::vault;

use super::save_status;
use super::shell::Shell;

/// The open paste sheet.
pub(super) struct DiaryPaste {
    /// The box you paste into. Kept after parsing so "Find wins again" can
    /// re-read an edited paste rather than making you start over.
    pub source: Entity<TextFieldState>,
    /// The queue, in the order the candidates appeared. Empty before the first
    /// parse — which is how the sheet knows to show the paste box alone.
    pub queue: Vec<PendingCandidate>,
    /// Whether a parse has run, so "nothing found" can be said out loud
    /// instead of looking like nothing happened.
    pub parsed: bool,
    /// How many have been accepted in this session, for the running count.
    pub accepted: usize,
}

/// One candidate awaiting a yes, a no, or an edit.
pub(super) struct PendingCandidate {
    /// Editable — this is US-12's "Поправить".
    pub text: Entity<TextFieldState>,
    /// The verbatim source, shown and never edited.
    pub quote: SharedString,
}

impl Shell {
    pub(super) fn open_diary_paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source = cx.new(|cx| TextFieldState::auto_grow(6, 14, window, cx));
        self.diary_paste = Some(DiaryPaste {
            source,
            queue: Vec::new(),
            parsed: false,
            accepted: 0,
        });
        cx.notify();
    }

    pub(super) fn close_diary_paste(&mut self, cx: &mut Context<Self>) {
        self.diary_paste = None;
        cx.notify();
    }

    /// Split what was pasted into candidates.
    fn find_wins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = self.diary_paste.as_ref() else {
            return;
        };
        let pasted = sheet.source.read(cx).value(cx).to_string();
        let found = candidates(&pasted);

        let queue: Vec<PendingCandidate> = found
            .into_iter()
            .map(|candidate| {
                let field = cx.new(|cx| TextFieldState::auto_grow(2, 6, window, cx));
                field.update(cx, |state, cx| state.seed(&candidate.text, window, cx));
                PendingCandidate {
                    text: field,
                    quote: candidate.quote.into(),
                }
            })
            .collect();

        if let Some(sheet) = self.diary_paste.as_mut() {
            sheet.queue = queue;
            sheet.parsed = true;
        }
        cx.notify();
    }

    /// Accept the candidate at `index`: it becomes a diary entry, tagged with
    /// whatever role the quick-capture is set to, and leaves the queue.
    fn accept_candidate(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(sheet) = self.diary_paste.as_ref() else {
            return;
        };
        let Some(candidate) = sheet.queue.get(index) else {
            return;
        };
        let text = candidate.text.read(cx).value(cx).trim().to_string();
        if text.is_empty() {
            return;
        }

        if let Some(vault_dir) = self.vault.clone() {
            let mut diary = vault::load_diary(&vault_dir);
            diary.entries.insert(
                0,
                DiaryEntry {
                    date: vault::today_iso(),
                    text,
                    // The role the quick-capture is already set to. A paste is
                    // one week of one job, so asking per candidate would be
                    // asking the same question five times.
                    role: self.diary_role.clone(),
                    ..Default::default()
                },
            );
            save_status::record(cx, "diary", vault::save_diary(&vault_dir, &diary));
        }

        if let Some(sheet) = self.diary_paste.as_mut() {
            sheet.queue.remove(index);
            sheet.accepted += 1;
        }
        cx.notify();
    }

    fn skip_candidate(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(sheet) = self.diary_paste.as_mut() {
            if index < sheet.queue.len() {
                sheet.queue.remove(index);
            }
        }
        cx.notify();
    }

    pub(super) fn render_diary_paste_sheet(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sheet = self.diary_paste.as_ref()?;
        let theme = cx.theme().clone();

        let body: AnyElement = if !sheet.parsed {
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_style(TextStyle::body())
                        .text_color(theme.text_muted)
                        .child(
                            "Paste the weekly status you already send, your standup notes, \
                             a retro, a self-review draft — anything. Each bullet or \
                             paragraph becomes a candidate you can keep, edit or skip.",
                        ),
                )
                .child(TextField::new(&sheet.source).placeholder(
                    "Paste here…",
                ))
                .into_any_element()
        } else if sheet.queue.is_empty() {
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_style(TextStyle::body())
                        .text_color(theme.text)
                        .child(if sheet.accepted > 0 {
                            format!(
                                "Done — {} win{} added.",
                                sheet.accepted,
                                if sheet.accepted == 1 { "" } else { "s" }
                            )
                        } else {
                            "Nothing in that text looked like a win.".to_string()
                        }),
                )
                .children((sheet.accepted == 0).then(|| {
                    div()
                        .text_style(TextStyle::meta())
                        .text_color(theme.text_subtle)
                        .child(
                            "Lines shorter than a sentence, headings and dates are skipped. \
                             Edit the paste and try again.",
                        )
                }))
                .child(TextField::new(&sheet.source).placeholder("Paste here…"))
                .into_any_element()
        } else {
            div()
                .id("diary-paste-queue")
                .max_h(px(360.0))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .children(sheet.queue.iter().enumerate().map(|(index, candidate)| {
                    div()
                        .p_3()
                        .rounded_lg()
                        .bg(theme.surface)
                        .border_1()
                        .border_color(theme.border)
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(TextField::new(&candidate.text))
                        .child(
                            // The source, verbatim. US-12 asks for it, and it
                            // is also the only way to see the split did not
                            // mangle anything.
                            div()
                                .text_style(TextStyle::meta())
                                .text_color(theme.text_subtle)
                                .child(format!("from: {}", candidate.quote)),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.0))
                                .child(
                                    Button::new(SharedString::from(format!("cand-skip-{index}")))
                                        .cursor_pointer()
                                        .toolbar_secondary()
                                        .label("Skip")
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, _window, cx| {
                                                this.skip_candidate(index, cx);
                                            },
                                        )),
                                )
                                .child(
                                    Button::new(SharedString::from(format!("cand-add-{index}")))
                                        .cursor_pointer()
                                        .toolbar_primary()
                                        .label("Keep")
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, _window, cx| {
                                                this.accept_candidate(index, cx);
                                            },
                                        )),
                                ),
                        )
                }))
                .into_any_element()
        };

        let remaining = sheet.queue.len();
        let subtitle = if !sheet.parsed {
            "Nothing is logged until you say so.".to_string()
        } else if remaining > 0 {
            format!(
                "{remaining} candidate{} left. Nothing is logged until you say so.",
                if remaining == 1 { "" } else { "s" }
            )
        } else {
            "Nothing is logged until you say so.".to_string()
        };

        let panel = div()
            .w(px(600.0))
            .flex()
            .flex_col()
            .rounded_lg()
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_style(TextStyle::heading())
                            .text_color(theme.text)
                            .child("Find wins in something you already wrote"),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::body())
                            .text_color(theme.text_muted)
                            .child(subtitle),
                    ),
            )
            .child(div().p_4().flex().flex_col().gap(px(12.0)).child(body).child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        Button::new("diary-paste-close")
                            .cursor_pointer()
                            .toolbar_secondary()
                            .label(if sheet.queue.is_empty() && sheet.parsed {
                                "Done"
                            } else {
                                "Cancel"
                            })
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.close_diary_paste(cx);
                            })),
                    )
                    .children(sheet.queue.is_empty().then(|| {
                        Button::new("diary-paste-find")
                            .cursor_pointer()
                            .toolbar_primary()
                            .label("Find wins")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.find_wins(window, cx);
                            }))
                    })),
            ));

        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.scrim)
                .child(panel)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape most people paste: a short header, then bullets.
    #[test]
    fn a_bulleted_status_report_yields_one_candidate_per_bullet() {
        let pasted = "\
Weekly status — 11 Aug
Highlights:
• Cut p99 latency in half by moving the orders service to event sourcing
- Onboarded two engineers onto the platform team, both shipping in week one
* Ran the incident retro for the Kafka outage and wrote the runbook
";
        let found = candidates(pasted);
        assert_eq!(found.len(), 3, "{found:#?}");
        assert!(found[0].text.starts_with("Cut p99 latency"));
        // The glyph is dropped from the text and kept in the quote, so the
        // author can see exactly what it came from.
        assert!(!found[0].text.starts_with('•'));
        assert!(found[0].quote.starts_with('•'));
        assert!(found[2].text.contains("runbook"));
    }

    /// The other shape: an email, hard-wrapped at whatever width the client
    /// chose. Four fragments are one thought.
    #[test]
    fn a_wrapped_paragraph_is_rejoined_into_one_candidate() {
        let pasted = "\
Hi team — quick update on the migration.

We finished moving the orders service to event sourcing this week,
which halved p99 latency and unblocked the four product teams that
were waiting on it.

Next week I am picking up the billing backlog.
";
        let found = candidates(pasted);
        assert_eq!(found.len(), 3, "{found:#?}");
        assert_eq!(
            found[1].text,
            "We finished moving the orders service to event sourcing this week, \
             which halved p99 latency and unblocked the four product teams that \
             were waiting on it."
        );
        // The quote keeps the original line breaks.
        assert!(found[1].quote.contains('\n'));
    }

    /// Headings, dates and rules are structure. Dropping them by shape is safe
    /// in a way dropping them by meaning would not be.
    #[test]
    fn scaffolding_is_dropped_and_content_is_not() {
        for line in [
            "Highlights:",
            "2026-08-11",
            "11/08/2026",
            "-----",
            "WEEKLY STATUS REPORT",
            "",
        ] {
            assert!(is_scaffolding(line), "should be scaffolding: {line:?}");
        }
        for line in [
            "Cut p99 latency in half on the orders service",
            "Shipped the billing fix everyone was waiting on",
        ] {
            assert!(!is_scaffolding(line), "should be content: {line:?}");
        }
    }

    /// A fragment too short to be a win is not offered. The threshold errs
    /// toward offering: a candidate the author skips costs one click, and a win
    /// silently dropped costs the feature its purpose.
    #[test]
    fn lines_too_short_to_be_a_win_are_not_offered() {
        let found = candidates("• ok\n• Shipped the billing fix everyone was waiting on\n");
        assert_eq!(found.len(), 1);
        assert!(found[0].text.starts_with("Shipped"));
    }

    /// A numbered status report is as common as a bulleted one.
    #[test]
    fn numbered_items_are_list_items_too() {
        let pasted = "\
1. Cut p99 latency in half on the orders service
2) Onboarded two engineers onto the platform team this week
";
        let found = candidates(pasted);
        assert_eq!(found.len(), 2, "{found:#?}");
        assert!(found[0].text.starts_with("Cut p99"));
        assert!(found[1].text.starts_with("Onboarded"));
    }

    /// A leading `-` is only a marker when a space follows it. `-5% churn` is a
    /// number, and `--` is a rule.
    #[test]
    fn a_dash_is_only_a_marker_when_a_space_follows_it() {
        assert_eq!(list_marker_len("- Shipped it"), Some(1));
        assert_eq!(list_marker_len("-5% churn this quarter across the board"), None);
        assert_eq!(list_marker_len("--"), None);
        assert_eq!(list_marker_len("2026-08-11 was the release date"), None);
    }

    /// Nothing pasted, nothing offered — and no panic on the degenerate input
    /// a paste box will certainly see.
    #[test]
    fn empty_and_whitespace_input_yield_nothing() {
        assert!(candidates("").is_empty());
        assert!(candidates("   \n\n\t\n").is_empty());
    }

    /// A bullet interrupts a running paragraph rather than being swallowed by
    /// it — otherwise a list under a lead-in sentence would come out as one
    /// enormous candidate.
    #[test]
    fn a_bullet_ends_the_paragraph_before_it() {
        let pasted = "\
This week we focused on the migration and on paying down the backlog.
• Cut p99 latency in half on the orders service
";
        let found = candidates(pasted);
        assert_eq!(found.len(), 2);
        assert!(found[0].text.starts_with("This week"));
        assert!(found[1].text.starts_with("Cut p99"));
    }
}
