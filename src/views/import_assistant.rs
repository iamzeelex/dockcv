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
use gpui::{div, px, ClickEvent, Context, Div, FontWeight, SharedString, Task};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dockcv_ui_components::{Button, ButtonExt, Spinner, MONO, SANS};

use crate::theme::ActiveTheme;

use super::shell::Shell;

/// Whether the route has been shown to work, or is on the list because it
/// looked like it should.
///
/// Said on screen rather than kept in a comment. Every one of these is another
/// company's product and can change under us without a word; a button that
/// quietly does nothing teaches people the feature is broken, while one that
/// admits it is unverified turns the same failure into a message that names
/// which assistant — which is the only way the list gets fixed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Trust {
    /// The mechanism is documented and has been read.
    Verified,
    /// Plausible, unconfirmed, and asking to be told.
    Unverified,
}

/// How a CV reaches an assistant.
enum Route {
    /// A command-line tool on this machine. It opens the file itself, from the
    /// path, and answers on standard output — so nothing is dragged and
    /// nothing is pasted, and the whole round trip is one click.
    Local {
        /// The program, looked for on `PATH`.
        program: &'static str,
        /// Its non-interactive form. `{}` is where the prompt goes.
        argv: &'static [&'static str],
    },
    /// A deep link into a browser. A URL carries text and cannot carry a file,
    /// so the person attaches it and pastes the answer back.
    Web { new_chat: &'static str },
}

struct Assistant {
    id: &'static str,
    name: &'static str,
    route: Route,
    trust: Trust,
}

/// Tools that read the file themselves.
///
/// All three are unverified, and the distinction matters: their
/// non-interactive flags are read off `--help` on a machine that has them, but
/// whether a given one will look at a PDF of images and answer with clean JSON
/// Resume is a question only a real run answers. That is what the mark is for.
const LOCAL: [Assistant; 3] = [
    Assistant {
        id: "local-claude",
        name: "Claude Code",
        route: Route::Local {
            program: "claude",
            argv: &["-p"],
        },
        trust: Trust::Unverified,
    },
    Assistant {
        id: "local-codex",
        name: "Codex",
        route: Route::Local {
            program: "codex",
            argv: &["exec"],
        },
        trust: Trust::Unverified,
    },
    Assistant {
        id: "local-gemini",
        name: "Gemini CLI",
        route: Route::Local {
            program: "gemini",
            argv: &["-p"],
        },
        trust: Trust::Unverified,
    },
];

/// Assistants that can be opened with a question already in the box.
///
/// Claude and ChatGPT document the parameter. The rest are here because they
/// look like they take one and because a list of two is a list that tells you
/// nothing about the one you actually use — marked unverified so that when one
/// of them opens an empty chat, the person knows it is worth saying so.
///
/// Gemini is deliberately absent: it has no prefill parameter at all, so a
/// button would open a blank window and look like a bug rather than an
/// omission. `Copy the prompt instead` is its route, and Ollama's, and any
/// model running on the person's own machine.
const WEB: [Assistant; 6] = [
    Assistant {
        id: "web-claude",
        name: "Claude",
        route: Route::Web {
            new_chat: "https://claude.ai/new?q=",
        },
        trust: Trust::Verified,
    },
    Assistant {
        id: "web-chatgpt",
        name: "ChatGPT",
        route: Route::Web {
            new_chat: "https://chatgpt.com/?q=",
        },
        trust: Trust::Verified,
    },
    Assistant {
        id: "web-perplexity",
        name: "Perplexity",
        route: Route::Web {
            new_chat: "https://www.perplexity.ai/search?q=",
        },
        trust: Trust::Unverified,
    },
    Assistant {
        id: "web-copilot",
        name: "Copilot",
        route: Route::Web {
            new_chat: "https://copilot.microsoft.com/?q=",
        },
        trust: Trust::Unverified,
    },
    Assistant {
        id: "web-mistral",
        name: "Le Chat",
        route: Route::Web {
            new_chat: "https://chat.mistral.ai/chat?q=",
        },
        trust: Trust::Unverified,
    },
    Assistant {
        id: "web-grok",
        name: "Grok",
        route: Route::Web {
            new_chat: "https://grok.com/?q=",
        },
        trust: Trust::Unverified,
    },
];

/// Which of the local tools are actually installed.
///
/// Looked up once. `PATH` does not change inside a run, and a directory scan
/// per frame to draw three buttons is the kind of cost that never shows up in
/// a profile because it is spread over every frame.
fn installed() -> &'static [&'static str] {
    static FOUND: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    FOUND.get_or_init(|| {
        let path = std::env::var_os("PATH").unwrap_or_default();
        LOCAL
            .iter()
            .filter_map(|assistant| match assistant.route {
                Route::Local { program, .. } => std::env::split_paths(&path)
                    .any(|dir| dir.join(program).is_file())
                    .then_some(program),
                Route::Web { .. } => None,
            })
            .collect()
    })
}

/// What a browser-side assistant is asked for.
///
/// Two rules carry the weight. **JSON Resume** because DockCV already imports
/// it and has a round-trip test over it, so the answer lands in the ordinary
/// review rather than in a parser written for chat output. And **leave it out
/// rather than guess** because that is the one way a model's reading is worse
/// than a parser's: a missing date is a gap somebody fills in, an invented one
/// is a lie nobody catches.
pub(super) fn transcription_prompt() -> String {
    format!(
        "I am attaching a CV as a PDF whose pages are images, so its text cannot be \
         extracted. Please read it and reply with a single JSON Resume document \
         (jsonresume.org schema) and nothing else.\n\n{RULES}"
    )
}

/// The same job, for a tool that can open the file itself.
fn local_prompt(path: &Path) -> String {
    format!(
        "Read the CV at {} — its pages are images, so transcribe what you can see on them. \
         Reply with a single JSON Resume document (jsonresume.org schema) and nothing else. \
         Do not create, edit or delete any file.\n\n{RULES}",
        path.display()
    )
}

/// The part both prompts share, so the two cannot drift.
const RULES: &str = "Rules:\n\
     - Transcribe only what you can actually read on the page.\n\
     - If a field is unreadable or absent, leave it out. Do not infer it, and do not fill a \
     gap with something plausible.\n\
     - Keep dates exactly as they are written. Do not normalise or correct them.\n\
     - Keep every bullet as its own entry in `highlights`.\n\
     - Do not improve, shorten or reword anything. This is a transcription.";

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

/// A local tool that is running right now.
pub(super) struct LocalRun {
    pub tool: &'static str,
    /// Exactly what was run, shown before and during. A command that reads
    /// somebody's CV and sends it to a model is not a thing to run behind
    /// their back, and "trust me" is not a design.
    pub command: String,
    /// Held, never read. Dropping a `Task` cancels it, so this is what keeps
    /// the tool running while the person looks at the screen — and what stops
    /// it mattering when they leave.
    pub _task: Task<()>,
}

/// How long to wait before giving up on a local tool.
///
/// Generous, because reading a page of images is slow and a model that is
/// working looks exactly like one that is stuck. Finite, because the failure
/// mode without it is a spinner nobody can clear.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(240);

impl Shell {
    /// Hand the file to a tool on this machine and read what it prints.
    ///
    /// This is the route the browser cannot be: the tool opens the file from
    /// its path, so there is nothing to drag, and it answers on standard
    /// output, so there is nothing to paste. One click, and what comes back
    /// lands in the same review as any other import.
    ///
    /// It is also the route that needs saying out loud. In the browser the
    /// person attaches the file themselves — the consent *is* the gesture.
    /// Here DockCV starts a process that reads their CV and sends it to a
    /// model, so the exact command is on screen before the button is pressed
    /// and the panel says who is doing the sending.
    fn run_local_assistant(
        &mut self,
        program: &'static str,
        argv: &'static [&'static str],
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let prompt = local_prompt(&path);
        let command = format!("{program} {} <prompt>", argv.join(" "));
        // In the file's own directory, so reading it is the ordinary case
        // rather than a tool reaching outside where it was started.
        let cwd = path.parent().map(|p| p.to_path_buf());
        let executor = cx.background_executor().clone();

        let task = cx.spawn(async move |this, cx| {
            let outcome = executor
                .spawn({
                    let executor = executor.clone();
                    async move {
                        let mut child = std::process::Command::new(program);
                        child.args(argv).arg(&prompt);
                        if let Some(cwd) = cwd {
                            child.current_dir(cwd);
                        }
                        let mut child = child
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .spawn()
                            .map_err(|e| format!("{program} would not start: {e}"))?;

                        let deadline = Instant::now() + LOCAL_TIMEOUT;
                        loop {
                            match child.try_wait() {
                                Ok(Some(_)) => break,
                                Ok(None) if Instant::now() < deadline => {
                                    executor.timer(Duration::from_millis(250)).await;
                                }
                                Ok(None) => {
                                    let _ = child.kill();
                                    return Err(format!(
                                        "{program} was still going after {} minutes, so it was \
                                         stopped. It may have been waiting for an answer it \
                                         could not ask for here.",
                                        LOCAL_TIMEOUT.as_secs() / 60
                                    ));
                                }
                                Err(e) => return Err(format!("{program} failed: {e}")),
                            }
                        }
                        let out = child
                            .wait_with_output()
                            .map_err(|e| format!("{program} failed: {e}"))?;
                        if !out.status.success() {
                            let why = String::from_utf8_lossy(&out.stderr);
                            let why = why.trim();
                            return Err(if why.is_empty() {
                                format!("{program} exited without saying why")
                            } else {
                                // Its own words. A tool that refused because it
                                // wanted permission says so, and that is the
                                // one thing worth reading here.
                                why.chars().take(400).collect()
                            });
                        }
                        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
                    }
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                // Only if this is still the run on screen. Leaving the step
                // while a tool is working is an ordinary thing to do.
                if this.import_run.as_ref().map(|r| r.tool) != Some(program) {
                    return;
                }
                this.import_run = None;
                match outcome {
                    Ok(answer) => this.adopt_transcription(&answer, cx),
                    Err(why) => {
                        this.import_step = super::import_flow::ImportStep::CouldNotRead {
                            filename: program.into(),
                            path: None,
                            error: Box::new(
                                crate::import::ImportError::new(format!(
                                    "{program} did not return a CV"
                                ))
                                .detail(why)
                                .remedy("Try it in the browser instead, or another assistant")
                                .remedy("Run the command yourself to see what it says"),
                            ),
                        };
                        cx.notify();
                    }
                }
            });
        });

        self.import_run = Some(LocalRun {
            tool: program,
            command,
            _task: task,
        });
        cx.notify();
    }

    /// The panel under a PDF that turned out to be a picture.
    pub(super) fn render_assistant_handoff(
        &self,
        cx: &mut Context<Self>,
        path: Option<&std::path::Path>,
    ) -> Div {
        let theme = *cx.theme();
        let local: Vec<&Assistant> = LOCAL
            .iter()
            .filter(|a| match a.route {
                Route::Local { program, .. } => installed().contains(&program),
                Route::Web { .. } => false,
            })
            .collect();

        div()
            .mt(px(18.0))
            .pt(px(16.0))
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                div()
                    .font_family(SANS)
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child("Or have an assistant read it"),
            )
            .children(
                self.import_run
                    .as_ref()
                    .map(|run| self.render_local_run(cx, run)),
            )
            .children((self.import_run.is_none() && !local.is_empty() && path.is_some()).then(
                || self.render_local_choices(cx, &local, path.expect("checked")),
            ))
            .children(
                self.import_run
                    .is_none()
                    .then(|| self.render_web_choices(cx, path)),
            )
    }

    /// The tools on this machine, which do the whole round trip.
    fn render_local_choices(
        &self,
        cx: &mut Context<Self>,
        local: &[&'static Assistant],
        path: &std::path::Path,
    ) -> Div {
        let theme = *cx.theme();
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(
                div()
                    .max_w(px(600.0))
                    .text_size(px(11.5))
                    .line_height(px(17.0))
                    .text_color(theme.text_muted)
                    .child(
                        "These are installed here and can open the file themselves — one click, \
                         nothing to drag or paste. DockCV starts the command and reads what it \
                         prints; the tool sends your CV wherever it normally sends things.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .children(local.iter().map(|assistant| {
                        let Route::Local { program, argv } = assistant.route else {
                            unreachable!("filtered to local routes");
                        };
                        let file = path.to_path_buf();
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .child(
                                Button::new(SharedString::from(assistant.id))
                                    .action_secondary()
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, _window, cx| {
                                            this.run_local_assistant(
                                                program,
                                                argv,
                                                file.clone(),
                                                cx,
                                            );
                                        },
                                    ))
                                    .child(format!("Run {}", assistant.name)),
                            )
                            .children(self.trust_mark(cx, assistant.trust))
                    })),
            )
    }

    /// While a local tool is working.
    fn render_local_run(&self, cx: &mut Context<Self>, run: &LocalRun) -> Div {
        let theme = *cx.theme();
        div()
            .px(px(12.0))
            .py(px(11.0))
            .rounded(theme.radius_sm())
            .border_1()
            .border_color(theme.accent.opacity(0.4))
            .bg(theme.accent.opacity(0.06))
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .child(Spinner::new().color(theme.accent))
                    .child(
                        div()
                            .font_family(SANS)
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(format!("{} is reading the pages…", run.tool)),
                    ),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(theme.text_subtle)
                    .child(run.command.clone()),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(
                        "Reading a page of images is slow. Leaving this screen stops DockCV \
                         waiting for it.",
                    ),
            )
    }

    /// The browser routes, where the person carries the file.
    fn render_web_choices(&self, cx: &mut Context<Self>, path: Option<&std::path::Path>) -> Div {
        let theme = *cx.theme();
        div()
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(
                div()
                    .max_w(px(600.0))
                    .text_size(px(11.5))
                    .line_height(px(17.0))
                    .text_color(theme.text_muted)
                    .child(
                        "Or open one in your browser with the instructions already written. A \
                         link can carry text but not a file, so you attach it and paste the \
                         answer back — DockCV itself sends nothing.",
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
                        "Either way your CV goes to that company under your own account, with \
                         whatever retention their terms set. Nothing else in DockCV leaves this \
                         machine.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .children(WEB.iter().map(|assistant| {
                        let Route::Web { new_chat } = assistant.route else {
                            unreachable!("WEB holds web routes");
                        };
                        let url = format!("{new_chat}{}", encode(&transcription_prompt()));
                        let file = path.map(|p| p.to_path_buf());
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .child(
                                Button::new(SharedString::from(assistant.id))
                                    .quiet()
                                    .text_color(theme.text)
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, _window, cx| {
                                            this.stage_transcription(file.clone(), cx);
                                            cx.open_url(&url);
                                        },
                                    ))
                                    .child(assistant.name),
                            )
                            .children(self.trust_mark(cx, assistant.trust))
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(10.0))
                    .child(
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
                    )
                    .child(div().flex_1().min_w(px(20.0)))
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

    /// The mark that invites a bug report.
    fn trust_mark(&self, cx: &mut Context<Self>, trust: Trust) -> Option<Div> {
        let theme = *cx.theme();
        (trust == Trust::Unverified).then(|| {
            div()
                .font_family(MONO)
                .text_size(px(9.0))
                .px(px(4.0))
                .py(px(1.0))
                .rounded(theme.radius_sm())
                .bg(theme.warning.opacity(0.12))
                .text_color(theme.warning)
                .child("?")
        })
    }

    /// Put the prompt on the clipboard and show the file, so both are to hand.
    fn stage_transcription(&mut self, path: Option<std::path::PathBuf>, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(transcription_prompt()));
        if let Some(path) = path {
            cx.open_with_system(&path);
        }
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
        self.adopt_transcription(&text, cx);
    }

    /// Take an assistant's answer — pasted, or printed by a local tool — and
    /// put it through the ordinary review.
    ///
    /// The same engine and the same review as a file, deliberately: what came
    /// out of a chat window is not a second kind of import, and the step that
    /// shows a person what was recognised is the step that matters most when a
    /// model did the recognising.
    pub(super) fn adopt_transcription(&mut self, answer: &str, cx: &mut Context<Self>) {
        match crate::import::engines::structured::import_structured_text(extract_json(answer)) {
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
                    filename: "the assistant's answer".into(),
                    path: None,
                    error: Box::new(error),
                };
            }
        }
        cx.notify();
    }
}

/// Find the JSON in an answer that was asked for JSON and nothing else.
///
/// Forgiving here rather than in the engine. A ```-fence and a sentence of
/// throat-clearing are what assistants do however firmly they are told not to,
/// and the engine's message about JSON that stops making sense is a true and
/// useful thing to say about a *file* — it should not start being said about a
/// polite preamble.
///
/// Only the outside is trimmed. Whatever is between the first `{` and the last
/// `}` is handed over exactly as written, including if it is broken: repairing
/// a model's JSON would mean deciding what it meant, and this is the one code
/// path in DockCV where guessing is already the risk being managed.
fn extract_json(answer: &str) -> &str {
    let trimmed = answer.trim();
    let inner = match trimmed.strip_prefix("```") {
        Some(rest) => {
            let rest = rest.strip_prefix("json").unwrap_or(rest);
            let rest = rest.trim_start_matches('\n').trim_end();
            rest.strip_suffix("```").unwrap_or(rest).trim()
        }
        None => trimmed,
    };
    match (inner.find('{'), inner.rfind('}')) {
        (Some(open), Some(close)) if close > open => &inner[open..=close],
        _ => inner,
    }
}

#[cfg(test)]
mod tests {
    use super::{encode, extract_json, transcription_prompt};

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
    /// How long the deeplink actually is. Not an assertion about taste: a
    /// query string is the one part of this hand-off with a hard ceiling, and
    /// a prompt that gets truncated by a browser or a CDN fails silently —
    /// the assistant opens with half an instruction and transcribes what it
    /// feels like.
    #[test]
    fn the_deeplink_fits_in_a_url() {
        let encoded = encode(&transcription_prompt());
        let longest = "https://claude.ai/new?q=".len() + encoded.len();
        println!("PROBE prompt {} chars, encoded {}, URL {longest}", transcription_prompt().len(), encoded.len());
        // 2000 is the floor every mainstream browser and CDN is safe under;
        // IE's old 2083 is the origin of the number and nothing sane is lower.
        assert!(longest < 2000, "the deeplink is {longest} characters");
    }

    #[test]
    fn the_prompt_forbids_guessing() {
        let prompt = transcription_prompt();
        assert!(prompt.contains("leave it out"));
        assert!(prompt.contains("Do not infer"));
        assert!(prompt.contains("JSON Resume"));
    }

    /// What assistants actually send back, however the prompt is worded.
    #[test]
    fn an_answer_wrapped_in_manners_is_still_an_answer() {
        let want = "{\"basics\":{\"name\":\"A\"}}";
        for wrapped in [
            "{\"basics\":{\"name\":\"A\"}}",
            "  {\"basics\":{\"name\":\"A\"}}  ",
            "```json\n{\"basics\":{\"name\":\"A\"}}\n```",
            "```\n{\"basics\":{\"name\":\"A\"}}\n```",
            "Here is the JSON Resume:\n\n{\"basics\":{\"name\":\"A\"}}\n\nLet me know if you need changes.",
        ] {
            assert_eq!(extract_json(wrapped), want, "failed on {wrapped:?}");
        }
    }

    /// Broken JSON stays broken. Repairing it would mean deciding what the
    /// model meant, on the one screen where guessing is the risk being managed.
    #[test]
    fn a_truncated_answer_is_not_repaired() {
        let cut = "{\"basics\":{\"name\":\"A\"";
        assert_eq!(extract_json(cut), cut);
    }
}
