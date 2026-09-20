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

use gpui::{Context, Task};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
pub(super) enum Trust {
    /// The mechanism is documented and has been read.
    Verified,
    /// Plausible, unconfirmed, and asking to be told.
    Unverified,
}

/// How a route reaches the assistant, and therefore how much of the work it
/// can do.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Via {
    /// A command-line tool. It opens the file from its path and answers on
    /// standard output, so this is the only route with no manual step at all.
    Cli {
        program: &'static str,
        argv: &'static [&'static str],
    },
    /// Claude Desktop's `claude://` scheme. Cowork's link carries the file
    /// itself; Code's carries the folder it is in.
    Cowork,
    Code,
    /// A browser. The person attaches the file and brings the answer back.
    Web { base: &'static str },
}

impl Via {
    /// What kind of thing this is, for the glyph on its button.
    ///
    /// The icon says *how it will work* rather than whose product it is, which
    /// is the distinction that actually changes what the person has to do
    /// next: a terminal finishes on its own, an app window and a browser both
    /// come back through the clipboard.
    pub(super) fn icon(self) -> dockcv_ui_components::Icon {
        use dockcv_ui_components::lucide;
        match self {
            Via::Cli { .. } => lucide("square-terminal"),
            Via::Cowork | Via::Code => lucide("window-maximize"),
            Via::Web { .. } => lucide("globe"),
        }
    }

    /// Whether this route can be taken on this machine right now.
    pub(super) fn available(self) -> bool {
        match self {
            Via::Cli { program, .. } => installed().contains(&program),
            Via::Cowork | Via::Code => claude_desktop(),
            Via::Web { .. } => true,
        }
    }
}

pub(super) struct Route {
    pub id: &'static str,
    /// Named for the surface, not the company — the company is the heading
    /// this sits under.
    pub label: &'static str,
    pub via: Via,
    pub trust: Trust,
}

/// One assistant and every way DockCV knows to reach it.
///
/// Grouped this way because that is how a person holds it: they have an
/// assistant, and then a choice about where to use it. The flat list this
/// replaces put eleven buttons in three unlabelled bands, so the first
/// question it asked was "which of these is mine", which is the one question
/// the reader already knows the answer to.
pub(super) struct Assistant {
    pub name: &'static str,
    /// Its wordmark, when one ships. `None` falls back to a plain glyph rather
    /// than borrowing somebody else's mark — see `DockIcon`'s note.
    pub mark: Option<dockcv_ui_components::DockIcon>,
    pub routes: &'static [Route],
}

impl Assistant {
    /// Whether any of its routes can be taken here.
    pub(super) fn reachable(&self) -> bool {
        self.routes.iter().any(|route| route.via.available())
    }

    /// An assistant whose only way in is a browser is a row that says its own
    /// name and the word `Browser`. Four of those in a column is a list of
    /// nothing; they collapse into one line instead.
    pub(super) fn browser_only(&self) -> bool {
        self.routes
            .iter()
            .filter(|route| route.via.available())
            .all(|route| matches!(route.via, Via::Web { .. }))
            && self.routes.iter().filter(|r| r.via.available()).count() == 1
    }
}

pub(super) const ASSISTANTS: &[Assistant] = &[
    Assistant {
        name: "Claude",
        mark: Some(dockcv_ui_components::DockIcon::BrandClaude),
        routes: &[
            Route {
                id: "claude-cowork",
                label: "Cowork",
                via: Via::Cowork,
                trust: Trust::Verified,
            },
            Route {
                id: "claude-code-app",
                label: "Claude Code",
                via: Via::Code,
                trust: Trust::Verified,
            },
            Route {
                id: "claude-cli",
                label: "Terminal",
                via: Via::Cli {
                    program: "claude",
                    argv: &["-p"],
                },
                trust: Trust::Unverified,
            },
            Route {
                id: "claude-web",
                label: "Browser",
                via: Via::Web {
                    base: "https://claude.ai/new?q=",
                },
                trust: Trust::Verified,
            },
        ],
    },
    Assistant {
        name: "ChatGPT",
        mark: Some(dockcv_ui_components::DockIcon::BrandOpenAi),
        routes: &[
            Route {
                id: "codex-cli",
                label: "Codex",
                via: Via::Cli {
                    program: "codex",
                    argv: &["exec"],
                },
                trust: Trust::Unverified,
            },
            Route {
                id: "chatgpt-web",
                label: "Browser",
                via: Via::Web {
                    base: "https://chatgpt.com/?q=",
                },
                trust: Trust::Verified,
            },
        ],
    },
    Assistant {
        // No browser route: Gemini's web chat has no prefill parameter, so a
        // button would open a blank window and read as a bug rather than an
        // omission. Its command line takes one.
        name: "Gemini",
        mark: Some(dockcv_ui_components::DockIcon::BrandGemini),
        routes: &[Route {
            id: "gemini-cli",
            label: "Terminal",
            via: Via::Cli {
                program: "gemini",
                argv: &["-p"],
            },
            trust: Trust::Unverified,
        }],
    },
    Assistant {
        name: "Perplexity",
        mark: Some(dockcv_ui_components::DockIcon::BrandPerplexity),
        routes: &[Route {
            id: "perplexity-web",
            label: "Browser",
            via: Via::Web {
                base: "https://www.perplexity.ai/search?q=",
            },
            trust: Trust::Unverified,
        }],
    },
    Assistant {
        name: "Copilot",
        mark: None,
        routes: &[Route {
            id: "copilot-web",
            label: "Browser",
            via: Via::Web {
                base: "https://copilot.microsoft.com/?q=",
            },
            trust: Trust::Unverified,
        }],
    },
    Assistant {
        name: "Le Chat",
        mark: Some(dockcv_ui_components::DockIcon::BrandMistral),
        routes: &[Route {
            id: "mistral-web",
            label: "Browser",
            via: Via::Web {
                base: "https://chat.mistral.ai/chat?q=",
            },
            trust: Trust::Unverified,
        }],
    },
    Assistant {
        name: "Grok",
        mark: None,
        routes: &[Route {
            id: "grok-web",
            label: "Browser",
            via: Via::Web {
                base: "https://grok.com/?q=",
            },
            trust: Trust::Unverified,
        }],
    },
];

/// Which of the command-line tools are actually installed.
///
/// Looked up once. `PATH` does not change inside a run, and a directory scan
/// per frame to draw a handful of buttons is the kind of cost that never shows
/// up in a profile because it is spread over every frame.
fn installed() -> &'static [&'static str] {
    static FOUND: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    FOUND.get_or_init(|| {
        let path = std::env::var_os("PATH").unwrap_or_default();
        ["claude", "codex", "gemini"]
            .into_iter()
            .filter(|program| {
                std::env::split_paths(&path).any(|dir| dir.join(program).is_file())
            })
            .collect()
    })
}

/// Whether Claude Desktop is here to answer a `claude://` link.
///
/// By its bundle, and only on macOS. The scheme is registered by the app on
/// every platform it ships for, but "is it installed" is asked differently on
/// each, and a wrong guess here is a button that opens nothing — which is the
/// exact failure the unverified mark exists to avoid, so it is better not to
/// offer the button at all than to offer one that lies.
fn claude_desktop() -> bool {
    #[cfg(target_os = "macos")]
    {
        static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *FOUND.get_or_init(|| {
            let user = std::env::var_os("HOME")
                .map(|home| Path::new(&home).join("Applications/Claude.app"))
                .is_some_and(|p| p.is_dir());
            user || Path::new("/Applications/Claude.app").is_dir()
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// What an assistant is asked for, when the file is attached or dragged.
///
/// Two rules carry the weight. **JSON Resume** because DockCV already imports
/// it and has a round-trip test over it, so the answer lands in the ordinary
/// review rather than in a parser written for chat output. And **leave it out
/// rather than guess** because that is the one way a model's reading is worse
/// than a parser's: a missing date is a gap somebody fills in, an invented one
/// is a lie nobody catches.
pub(super) fn transcription_prompt() -> String {
    format!(
        "I am attaching a CV as a PDF whose pages are images, so its text cannot be extracted. \
         Please read it and reply with a single JSON Resume document (jsonresume.org schema) \
         and nothing else. {RULES}"
    )
}

/// The same job, for a route that hands over a path instead of a file.
pub(super) fn local_prompt(path: &Path) -> String {
    format!(
        "Read the CV at {} — its pages are images, so transcribe what you can see on them. \
         Reply with a single JSON Resume document (jsonresume.org schema) and nothing else. \
         Do not create, edit or delete any file. {RULES}",
        path.display()
    )
}

/// The part every prompt shares, so they cannot drift.
///
/// One line, with no newlines in it at all. Cowork attached the file from a
/// link and left the composer empty, and a `q` full of `%0A` is the likeliest
/// difference between the two — so every prompt that travels in a URL is a
/// sentence now, and the rules are separated by semicolons rather than by line
/// breaks.
const RULES: &str = "Rules: transcribe only what you can actually read on the page; \
     if a field is unreadable or absent, leave it out — do not infer it, and do not fill a gap \
     with something plausible; keep dates exactly as they are written, without normalising or \
     correcting them; keep every bullet as its own entry in `highlights`; do not improve, \
     shorten or reword anything, because this is a transcription.";

/// Percent-encode for a query string.
///
/// Hand-rolled rather than a dependency: this is the only URL this app builds,
/// the rule is RFC 3986's unreserved set, and adding a crate to the graph to
/// encode one string is a poor trade in a binary that ships no HTTP client.
pub(super) fn encode(text: &str) -> String {
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

/// The link a route opens, with the file or the folder it can carry.
///
/// Built here rather than in a click handler because every one of these fails
/// silently when wrong: a bad `file` attaches nothing and Cowork opens empty,
/// a bad `folder` puts Code in the wrong place, a bad `q` leaves a composer
/// blank. None of them can report anything back to us.
pub(super) fn route_url(via: Via, path: &Path) -> Option<String> {
    Some(match via {
        // `file` takes an absolute path and Claude Desktop attaches it. This
        // is the one link in the product that carries the document itself.
        Via::Cowork => format!(
            "claude://cowork/new?q={}&file={}",
            encode(&transcription_prompt()),
            encode(&path.to_string_lossy())
        ),
        // `folder`, not `file`: Code's `file` parameter is documented as
        // accepted and not yet supported, so passing it would look like it
        // worked and attach nothing. The folder goes across and the prompt
        // names the file inside it.
        Via::Code => format!(
            "claude://code/new?q={}&folder={}",
            encode(&local_prompt(path)),
            encode(&path.parent().unwrap_or(Path::new("/")).to_string_lossy())
        ),
        Via::Web { base } => format!("{base}{}", encode(&transcription_prompt())),
        Via::Cli { .. } => return None,
    })
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
    pub(super) fn run_local_assistant(
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

    /// Put the prompt on the clipboard and show the file, so both are to hand.
    pub(super) fn stage_transcription(&mut self, path: Option<std::path::PathBuf>, cx: &mut Context<Self>) {
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
        // Case-insensitive, because the rule is the sentence and not its
        // capitalisation — this assertion has already failed once for a
        // reword that kept the instruction perfectly intact, which is a test
        // failing about itself rather than about the thing it guards.
        let prompt = transcription_prompt().to_lowercase();
        assert!(prompt.contains("leave it out"), "{prompt}");
        assert!(prompt.contains("do not infer"), "{prompt}");
        assert!(prompt.contains("json resume"), "{prompt}");
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

    /// A route with no link of its own says so rather than building a wrong
    /// one — the terminal reads the file itself.
    #[test]
    fn the_command_line_has_no_url() {
        use std::path::Path;
        assert!(super::route_url(
            super::Via::Cli {
                program: "claude",
                argv: &["-p"]
            },
            Path::new("/tmp/x.pdf")
        )
        .is_none());
    }

    /// Every prompt that travels in a URL is one line.
    ///
    /// Cowork attached the file from a link and left the composer empty. A `q`
    /// full of `%0A` is the likeliest reason, and it is the kind of failure
    /// that reports nothing: the link opens, the file arrives, and the
    /// instruction is simply missing.
    #[test]
    fn no_prompt_that_travels_in_a_url_has_a_newline_in_it() {
        use std::path::Path;
        assert!(!super::transcription_prompt().contains('\n'));
        assert!(!super::local_prompt(Path::new("/tmp/x.pdf")).contains('\n'));
    }

    /// The two links that carry a path. Both fail silently when wrong — a bad
    /// `file` attaches nothing and Cowork opens empty, a bad `folder` puts
    /// Code in the wrong place — so what is asserted is that the path arrives
    /// encoded and under the parameter the documentation names.
    #[test]
    fn the_desktop_links_carry_the_file_and_the_folder() {
        use std::path::Path;

        let path = Path::new("/Users/me/Down loads/scan & copy.pdf");

        let cowork = super::route_url(super::Via::Cowork, path).expect("a link");
        assert!(cowork.starts_with("claude://cowork/new?q="));
        assert!(
            cowork.contains("&file=%2FUsers%2Fme%2FDown%20loads%2Fscan%20%26%20copy.pdf"),
            "the space and the ampersand have to survive: {cowork}"
        );

        let code = super::route_url(super::Via::Code, path).expect("a link");
        assert!(code.starts_with("claude://code/new?q="));
        assert!(
            code.contains("&folder=%2FUsers%2Fme%2FDown%20loads"),
            "Code takes the folder, never the file: {code}"
        );
        assert!(!code.contains("&file="), "Code's file parameter does nothing yet");
        // Code is not handed the file, so the prompt has to name it.
        assert!(super::local_prompt(path).contains("/Users/me/Down loads/scan & copy.pdf"));
        // Well under the ~14,000 characters Claude Desktop truncates `q` at.
        assert!(cowork.len() < 2000 && code.len() < 2000);
    }

    /// Broken JSON stays broken. Repairing it would mean deciding what the
    /// model meant, on the one screen where guessing is the risk being managed.
    #[test]
    fn a_truncated_answer_is_not_repaired() {
        let cut = "{\"basics\":{\"name\":\"A\"";
        assert_eq!(extract_json(cut), cut);
    }
}
