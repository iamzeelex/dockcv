//! Reading a picture of a CV with the assistant the person already has.
//!
//! A scanned or flattened PDF has no text in it, so there is nothing for the
//! importer to read and three suggestions that all mean "go away, produce a
//! different file, come back". This is a fourth, and it is the first place in
//! DockCV where a model is involved at all.
//!
//! ## What DockCV does and does not do
//!
//! It does not call anybody. There is no key, no account, no request and no
//! provider SDK in this binary — the claim in `CLAUDE.md` that DockCV makes
//! zero network calls at runtime stays literally true. What this screen does is
//! prepare a hand-off: it writes the prompt, opens the assistant the person
//! already pays for with that prompt in the box, and shows them their file.
//! **They** attach it. The CV leaves the machine because a person decided to
//! send it, in their own session, in their own browser.
//!
//! That is not a technicality, it is the whole design. An in-app integration
//! would mean a key to store, a provider to trust, a bill to explain and a
//! quiet moment where a CV goes somewhere the person did not watch it go.
//!
//! ## Why it is a hand-off and not a pipeline
//!
//! A URL can carry text. It cannot carry a file — no provider's deep link
//! takes an attachment, and none will, because that is an upload. So the file
//! travels by the person's own drag, and the prompt travels by the link. Three
//! gestures, about fifteen seconds, and each one visible.
//!
//! The way back is a paste. The assistant is asked for JSON Resume, which
//! DockCV already imports and round-trips, so what comes back lands in the
//! ordinary review step and is checked section by section like any other
//! import — before a single byte reaches the vault.
//!
//! ## The thing to be careful about
//!
//! What comes back is a **model reading a picture**, which fails differently
//! from a parser. A parser that cannot read a date leaves it out; a model can
//! produce a plausible one that was never on the page. So the prompt says to
//! leave a field out rather than guess it, and the import is labelled with
//! where it came from so the review is read as a transcription to check rather
//! than as extraction to skim.

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, Div, FontWeight, SharedString};

use dockcv_ui_components::{Button, ButtonExt, MONO, SANS};

use crate::theme::ActiveTheme;

use super::shell::Shell;

/// An assistant that can be opened with a question already in the box.
///
/// Only two, and that is a fact about the market rather than a shortlist.
/// Claude and ChatGPT document a query parameter that prefills a new
/// conversation. Gemini has none. Codex is a command-line tool and has no URL
/// at all — it can still do this job perfectly well, which is what `Copy the
/// prompt` is for.
struct Assistant {
    id: &'static str,
    name: &'static str,
    /// The prompt is appended, percent-encoded.
    new_chat: &'static str,
}

const ASSISTANTS: [Assistant; 2] = [
    Assistant {
        id: "assistant-claude",
        name: "Claude",
        new_chat: "https://claude.ai/new?q=",
    },
    Assistant {
        id: "assistant-chatgpt",
        name: "ChatGPT",
        new_chat: "https://chatgpt.com/?q=",
    },
];

/// What the assistant is asked for.
///
/// Two rules carry the weight. **JSON Resume** because DockCV already imports
/// it and has a round-trip test over it, so the answer lands in the ordinary
/// review rather than in a parser written for chat output. And **leave it out
/// rather than guess** because that is the one way a model's reading is worse
/// than a parser's: a missing date is a gap somebody fills in, an invented one
/// is a lie nobody catches.
pub(super) fn transcription_prompt() -> String {
    "I am attaching a CV as a PDF whose pages are images, so its text cannot be \
     extracted. Please read it and reply with a single JSON Resume document \
     (jsonresume.org schema) and nothing else — no explanation, no markdown fence.\n\n\
     Rules:\n\
     - Transcribe only what you can actually read on the page.\n\
     - If a field is unreadable or absent, leave it out. Do not infer it, and do \
     not fill a gap with something plausible.\n\
     - Keep dates exactly as they are written. Do not normalise or correct them.\n\
     - Keep every bullet as its own entry in `highlights`.\n\
     - Do not improve, shorten or reword anything. This is a transcription."
        .to_string()
}

/// Percent-encode for a query string.
///
/// Hand-rolled rather than a dependency: this is the only URL this app builds,
/// the rule is RFC 3986's unreserved set, and adding a crate to the graph to
/// encode one string is a poor trade in a binary that ships no HTTP client.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

impl Shell {
    /// Put the prompt on the clipboard and show the file, so both are to hand.
    fn stage_transcription(&mut self, path: Option<std::path::PathBuf>, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(transcription_prompt()));
        if let Some(path) = path {
            cx.open_with_system(&path);
        }
    }

    /// The panel under a PDF that turned out to be a picture.
    pub(super) fn render_assistant_handoff(
        &self,
        cx: &mut Context<Self>,
        path: Option<&std::path::Path>,
    ) -> Div {
        let theme = *cx.theme();

        div()
            .mt(px(18.0))
            .pt(px(16.0))
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .font_family(SANS)
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child("Or read it with your own assistant"),
            )
            .child(
                div()
                    .max_w(px(560.0))
                    .text_size(px(11.5))
                    .line_height(px(17.0))
                    .text_color(theme.text_muted)
                    .child(
                        "DockCV opens your assistant with the instructions already written and \
                         shows you the file. You attach it and paste the answer back here — \
                         DockCV itself sends nothing.",
                    ),
            )
            .child(
                // The consequence, in the place where the decision is made. A
                // CV is the most personal document most people own, and this
                // is the one action in DockCV that takes it off the machine.
                div()
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(theme.radius_sm())
                    .bg(theme.warning.opacity(0.08))
                    .border_1()
                    .border_color(theme.warning.opacity(0.3))
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(theme.text_muted)
                    .child(
                        "Attaching it sends your CV to that company under your own account, \
                         with whatever retention their terms set. Nothing else in DockCV leaves \
                         this machine.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .children(ASSISTANTS.iter().map(|assistant| {
                        let url =
                            format!("{}{}", assistant.new_chat, encode(&transcription_prompt()));
                        let file = path.map(|p| p.to_path_buf());
                        Button::new(SharedString::from(assistant.id))
                            .action_secondary()
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.stage_transcription(file.clone(), cx);
                                cx.open_url(&url);
                            }))
                            .child(format!("Open {}  →", assistant.name))
                    }))
                    .child(
                        // For Gemini, for Codex, for a model running on the
                        // person's own machine, and for anybody who would
                        // rather paste than be redirected.
                        Button::new("assistant-copy")
                            .quiet()
                            .text_color(theme.text_muted)
                            .tooltip("Copies the instructions and reveals the PDF")
                            .on_click({
                                let file = path.map(|p| p.to_path_buf());
                                cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                    this.stage_transcription(file.clone(), cx);
                                })
                            })
                            .child("Copy the prompt instead"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(200.0))
                            .font_family(MONO)
                            .text_size(px(10.5))
                            .text_color(theme.text_subtle)
                            .child("Then: paste what it answers back into DockCV."),
                    )
                    .child(
                        Button::new("assistant-paste")
                            .action_primary()
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.import_from_clipboard(cx);
                            }))
                            .child("Paste the answer"),
                    ),
            )
    }

    /// Read a JSON Resume off the clipboard and put it through the ordinary
    /// review.
    ///
    /// The same engine and the same review as a file, deliberately: what came
    /// out of a chat window is not a second kind of import, and the step that
    /// shows a person what was recognised is the step that matters most when a
    /// model did the recognising.
    pub(super) fn import_from_clipboard(&mut self, cx: &mut Context<Self>) {
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        let trimmed = trim_fence(&text);

        match crate::import::engines::structured::import_structured_text(trimmed) {
            Ok(mut imported) => {
                // Provenance, where the review will print it. A transcription
                // is checked differently from an extraction, and the person
                // reading this screen has to know which one they are reading.
                imported.format_name = "transcribed by an assistant".into();
                self.import_step = super::import_flow::ImportStep::Step2Review {
                    imported: Box::new(imported),
                };
            }
            Err(error) => {
                self.import_step = super::import_flow::ImportStep::CouldNotRead {
                    filename: "what was on the clipboard".into(),
                    path: None,
                    error: Box::new(error),
                };
            }
        }
        cx.notify();
    }
}

/// Strip a ```json fence, which assistants add however firmly they are asked
/// not to.
///
/// Forgiving here rather than in the engine: the engine's error about JSON
/// that stops making sense is a true and useful thing to say about a file, and
/// it should not start being said about a fence.
fn trim_fence(text: &str) -> &str {
    let trimmed = text.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let rest = rest.strip_prefix("json").unwrap_or(rest);
    rest.trim_start_matches('\n')
        .trim_end()
        .strip_suffix("```")
        .unwrap_or(rest)
        .trim()
}

#[cfg(test)]
mod tests {
    use super::{encode, transcription_prompt, trim_fence};

    #[test]
    fn the_prompt_survives_being_put_in_a_url() {
        let encoded = encode(&transcription_prompt());
        assert!(!encoded.contains(' '));
        assert!(!encoded.contains('\n'));
        assert!(!encoded.contains('&'), "would start a second query parameter");
        // Round-trips: every escape is a byte, and the bytes are the prompt.
        let mut bytes = Vec::new();
        let raw = encoded.as_bytes();
        let mut i = 0;
        while i < raw.len() {
            if raw[i] == b'%' {
                let hex = std::str::from_utf8(&raw[i + 1..i + 3]).expect("ascii");
                bytes.push(u8::from_str_radix(hex, 16).expect("hex"));
                i += 3;
            } else {
                bytes.push(raw[i]);
                i += 1;
            }
        }
        assert_eq!(String::from_utf8(bytes).expect("utf8"), transcription_prompt());
    }

    /// The instruction that keeps a transcription a transcription. If this
    /// sentence ever leaves the prompt, the model is free to fill a gap with
    /// something plausible and nobody downstream can tell.
    #[test]
    fn the_prompt_forbids_guessing() {
        let prompt = transcription_prompt();
        assert!(prompt.contains("leave it out"));
        assert!(prompt.contains("Do not infer"));
        assert!(prompt.contains("JSON Resume"));
    }

    #[test]
    fn a_fenced_answer_is_still_an_answer() {
        assert_eq!(trim_fence("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(trim_fence("```\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(trim_fence("  {\"a\":1}  "), "{\"a\":1}");
    }
}
