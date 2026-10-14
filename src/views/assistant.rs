//! Reaching the assistant the person already has.
//!
//! Shared machinery, and it used to live inside the import screen because that
//! is where it was first needed. The registry of assistants, how each is
//! reached, whether a route has been verified, how a prompt is turned into a
//! link and how a local tool is run are none of them about importing a file —
//! and the moment a second surface wanted them, `views::front_door` importing
//! `views::import::assistant` would have said the front door depends on the
//! import screen, which is not true and would not stay harmless.
//!
//! What stays with each caller is the **prompt**: what the assistant is being
//! asked to do, and the rules it has to keep. That is the part that is about
//! the job, and it is the part worth reading beside the job.
//!
//! ## What a route is allowed to do
//!
//! Browser and Cowork tailoring prompts return IDs for the user to review;
//! they cannot write into the CV. A local coding route is different by design:
//! it can add a new variant and preset to the selected TOML file, but its prompt
//! limits it to exact content already in that CV or `library.toml`. DockCV
//! watches the file and asks before adopting those changes into an open draft.
//! The import prompt remains a transcription and still forbids inferred facts.

use gpui::{BackgroundExecutor, Task};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trust {
    /// The mechanism is documented and has been read.
    Verified,
    /// Plausible, unconfirmed, and asking to be told.
    Unverified,
}

/// How a route reaches the assistant, and therefore how much of the work it
/// can do.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Via {
    /// A command-line coding assistant. It works in the file's folder and may
    /// write the requested new CV variants; its response is shown on screen.
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
    pub(crate) fn icon(self) -> dockcv_ui_components::Icon {
        use dockcv_ui_components::lucide;
        match self {
            Via::Cli { .. } => lucide("square-terminal"),
            // A workspace, not a window outline: `window-maximize` at this
            // size is an empty rectangle, which reads as an unticked checkbox.
            Via::Cowork => lucide("layout-dashboard"),
            Via::Code => lucide("square-terminal"),
            Via::Web { .. } => lucide("globe"),
        }
    }

    /// Whether this route can be taken on this machine right now.
    pub(crate) fn available(self) -> bool {
        match self {
            Via::Cli { program, .. } => installed().contains(&program),
            Via::Cowork | Via::Code => claude_desktop(),
            Via::Web { .. } => true,
        }
    }
}

pub(crate) struct Route {
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
pub(crate) struct Assistant {
    pub name: &'static str,
    /// Its wordmark, when one ships. `None` falls back to a plain glyph rather
    /// than borrowing somebody else's mark — see `DockIcon`'s note.
    pub mark: Option<dockcv_ui_components::DockIcon>,
    pub routes: &'static [Route],
}

impl Route {
    /// How much of the work this route does without the person.
    ///
    /// Used to pick which one an assistant leads with. They are not equal and
    /// the screen was drawing them as though they were: a terminal finishes on
    /// its own, Cowork arrives with the file attached, Code arrives with the
    /// folder, and a browser needs the file dragged in by hand.
    pub(crate) fn rank(&self) -> u8 {
        match self.via {
            Via::Cli { .. } => 0,
            Via::Cowork => 1,
            Via::Code => 2,
            Via::Web { .. } => 3,
        }
    }
}

impl Assistant {
    /// Whether any of its routes can be taken here.
    pub(crate) fn reachable(&self) -> bool {
        self.routes.iter().any(|route| route.via.available())
    }

    /// Its routes, easiest first.
    ///
    /// Sorted rather than left in declaration order, because the row is read
    /// left to right and the outlined one was landing wherever it happened to
    /// sit — third in Claude's row, first in ChatGPT's. Four rows with the
    /// emphasis in four different places is the ragged look; the lead is
    /// always the first thing after the name now.
    pub(crate) fn ordered(&self) -> Vec<&'static Route> {
        let mut routes: Vec<&'static Route> = self
            .routes
            .iter()
            .filter(|route| route.via.available())
            .collect();
        routes.sort_by_key(|route| route.rank());
        routes
    }

    /// An assistant whose only way in is a browser is a row that says its own
    /// name and the word `Browser`. Four of those in a column is a list of
    /// nothing; they collapse into one line instead.
    pub(crate) fn browser_only(&self) -> bool {
        self.routes
            .iter()
            .filter(|route| route.via.available())
            .all(|route| matches!(route.via, Via::Web { .. }))
            && self.routes.iter().filter(|r| r.via.available()).count() == 1
    }
}

pub(crate) const ASSISTANTS: &[Assistant] = &[
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
        name: "Ollama",
        mark: None,
        routes: &[Route {
            id: "ollama-cli",
            label: "Terminal",
            via: Via::Cli {
                program: "ollama",
                argv: &["run", "llama3.2"],
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
        mark: Some(dockcv_ui_components::DockIcon::BrandX),
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

/// The ones we have actually seen work, named.
///
/// Third attempt at this, and the first two failed the same way. A circled `i`
/// beside seven buttons was a texture rather than a mark, and the glyph said
/// "info" when it meant "caution". A 4px dot was smaller and no clearer. Both
/// were trying to annotate eleven controls with a fact about four of them.
///
/// So it is a sentence. It names the verified ones, which is short, and says
/// what to do when one of the others misbehaves — which is the only reason the
/// distinction is on screen at all: a route that quietly does nothing has to
/// become a message naming the assistant, or the list never gets fixed.
pub(crate) fn verified_note() -> String {
    let names: Vec<&str> = ASSISTANTS
        .iter()
        .filter(|assistant| assistant.reachable())
        .flat_map(|assistant| {
            assistant
                .ordered()
                .into_iter()
                .filter(|route| route.trust == Trust::Verified)
                .map(move |route| (assistant.name, route.label))
        })
        .map(|(name, label)| if label == "Browser" { name } else { label })
        .collect();

    match names.len() {
        0 => "None of these are verified yet — tell us how yours goes.".to_string(),
        _ => format!(
            "We have checked {}. The rest should work the same way — tell us if one does not.",
            join_with_and(&names)
        ),
    }
}

/// `a, b and c`.
fn join_with_and(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Standard directories searched for command-line assistants.
///
/// In addition to inherited PATH, we inspect standard Homebrew and Cargo
/// directories on macOS/Linux. GUI apps launched from Finder/Dock frequently
/// inherit a minimal PATH (`/usr/bin:/bin`) without shell profile exports;
/// checking these standard roots ensures tools like `ollama` or `codex`
/// installed via brew or cargo are discovered reliably and without crutches.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    for standard in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let p = PathBuf::from(standard);
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = Path::new(&home);
        let cargo_bin = home.join(".cargo/bin");
        if !dirs.contains(&cargo_bin) {
            dirs.push(cargo_bin);
        }
        let local_bin = home.join(".local/bin");
        if !dirs.contains(&local_bin) {
            dirs.push(local_bin);
        }
    }
    dirs
}

/// Find the absolute path to an executable binary if present.
pub(crate) fn find_executable(program: &str) -> Option<PathBuf> {
    let p = Path::new(program);
    if p.is_file() {
        return Some(p.to_path_buf());
    }
    search_dirs().into_iter().find_map(|dir| {
        let candidate = dir.join(program);
        if candidate.is_file() {
            Some(candidate)
        } else {
            None
        }
    })
}

/// Whether a CLI tool is available on this system.
pub(crate) fn is_installed(program: &str) -> bool {
    find_executable(program).is_some()
}

/// Which of the command-line tools are actually installed.
///
/// Looked up once. `PATH` does not change inside a run, and a directory scan
/// per frame to draw a handful of buttons is the kind of cost that never shows
/// up in a profile because it is spread over every frame.
fn installed() -> &'static [&'static str] {
    static FOUND: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    FOUND.get_or_init(|| {
        ["claude", "codex", "gemini", "ollama"]
            .into_iter()
            .filter(|program| is_installed(program))
            .collect()
    })
}

/// Whether Claude Desktop is here to answer a `claude://` link.
pub(crate) fn claude_desktop_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let user = Path::new(&home).join("Applications/Claude.app");
            if user.is_dir() {
                return Some(user);
            }
        }
        let sys = PathBuf::from("/Applications/Claude.app");
        if sys.is_dir() {
            return Some(sys);
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub(crate) fn claude_desktop() -> bool {
    claude_desktop_path().is_some()
}

/// Status of an assistant on this machine, for Settings and discovery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssistantStatus {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
    pub available: bool,
    pub detail: String,
}

/// Enumerate all known local assistants and their detection status.
pub(crate) fn detect_all_assistants() -> Vec<AssistantStatus> {
    let claude_app = claude_desktop_path();
    let claude_cli = find_executable("claude");
    let codex_cli = find_executable("codex");
    let gemini_cli = find_executable("gemini");
    let ollama_cli = find_executable("ollama");

    vec![
        AssistantStatus {
            id: "claude-desktop",
            name: "Claude Desktop",
            kind: "Desktop app (macOS)",
            available: claude_app.is_some(),
            detail: claude_app
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "Not found in /Applications".into()),
        },
        AssistantStatus {
            id: "ollama",
            name: "Ollama CLI",
            kind: "Local LLM runner",
            available: ollama_cli.is_some(),
            detail: ollama_cli
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "Not found in PATH".into()),
        },
        AssistantStatus {
            id: "codex",
            name: "Codex CLI",
            kind: "CLI tool",
            available: codex_cli.is_some(),
            detail: codex_cli
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "Not found in PATH".into()),
        },
        AssistantStatus {
            id: "gemini",
            name: "Gemini CLI",
            kind: "CLI tool",
            available: gemini_cli.is_some(),
            detail: gemini_cli
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "Not found in PATH".into()),
        },
        AssistantStatus {
            id: "claude-cli",
            name: "Claude CLI",
            kind: "CLI tool",
            available: claude_cli.is_some(),
            detail: claude_cli
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "Not found in PATH".into()),
        },
    ]
}

/// Tokenize a shell command line string into words, respecting single/double quotes.
pub(crate) fn parse_command_words(cmd: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = ' ';

    for ch in cmd.chars() {
        match ch {
            '"' | '\'' if in_quotes && ch == quote_char => {
                in_quotes = false;
            }
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = ch;
            }
            ' ' | '\t' if !in_quotes => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Execute a local command asynchronously with timeout and output capture.
pub(crate) async fn execute_command(
    program: &str,
    argv: &[&str],
    prompt: &str,
    cwd: Option<&Path>,
    executor: &BackgroundExecutor,
) -> Result<String, String> {
    let resolved = find_executable(program)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| program.to_string());

    let cwd = cwd.map(PathBuf::from);
    let prompt = prompt.to_string();
    let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    let prog_name = program.to_string();
    let executor_clone = executor.clone();

    executor
        .spawn(async move {
            let mut cmd = std::process::Command::new(&resolved);
            cmd.args(&argv).arg(&prompt);
            if let Some(ref cwd) = cwd {
                cmd.current_dir(cwd);
            }
            let mut child = cmd
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| format!("{prog_name} would not start: {e}"))?;

            let deadline = std::time::Instant::now() + LOCAL_TIMEOUT;
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if std::time::Instant::now() < deadline => {
                        executor_clone.timer(Duration::from_millis(250)).await;
                    }
                    Ok(None) => {
                        let _ = child.kill();
                        return Err(format!(
                            "{prog_name} was still running after {} minutes and was stopped.",
                            LOCAL_TIMEOUT.as_secs() / 60
                        ));
                    }
                    Err(e) => return Err(format!("{prog_name} failed: {e}")),
                }
            }
            let out = child
                .wait_with_output()
                .map_err(|e| format!("{prog_name} failed: {e}"))?;
            if !out.status.success() {
                let why = String::from_utf8_lossy(&out.stderr);
                let why = why.trim();
                return Err(if why.is_empty() {
                    format!("{prog_name} exited without a result (status {})", out.status)
                } else {
                    why.chars().take(400).collect()
                });
            }
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        })
        .await
}

/// Percent-encode for a query string.
///
/// Hand-rolled rather than a dependency: these are the only URLs this app
/// builds, the rule is RFC 3986's unreserved set, and adding a crate to the
/// graph to encode one string is a poor trade in a binary that ships no HTTP
/// client.
pub(crate) fn encode(text: &str) -> String {
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

/// What a hand-off carries: the words, and a file when the job is about one.
///
/// Two callers, two shapes. The import screen sends a CV whose pages are
/// images and needs the file to travel; the tailoring screen sends a job
/// posting and a menu, and has nothing to attach. Both need the prompt on the
/// clipboard whatever the route does, because none of these links can report
/// back whether they worked.
pub(crate) struct Handoff {
    pub prompt: String,
    pub file: Option<PathBuf>,
}

/// The link a route opens, with the file or the folder it can carry.
///
/// Built here rather than in a click handler because every one of these fails
/// silently when wrong: a bad `file` attaches nothing and Cowork opens empty,
/// a bad `folder` puts Code in the wrong place, a bad `q` leaves a composer
/// blank. None of them can report anything back to us.
pub(crate) fn route_url(via: Via, job: &Handoff) -> Option<String> {
    let path = job.file.as_deref().unwrap_or(Path::new("/"));
    Some(match via {
        // `file` takes an absolute path and Claude Desktop attaches it. This
        // is the one link in the product that carries the document itself.
        //
        // **`file` before `q`, and that order is a guess.** Reported
        // behaviour: the composer fills with the prompt and then empties,
        // while the attachment arrives — which reads like the attach handler
        // running second and resetting what the prefill put there. If the
        // parameters are applied in the order they appear, this swaps the two.
        // It is unverifiable from here and costs nothing if it is wrong; the
        // instructions go to the clipboard on every route regardless, which is
        // the fix that does not depend on somebody else's app.
        Via::Cowork if job.file.is_some() => format!(
            "claude://cowork/new?file={}&q={}",
            encode(&path.to_string_lossy()),
            encode(&job.prompt)
        ),
        Via::Cowork => format!("claude://cowork/new?q={}", encode(&job.prompt)),
        // `folder`, not `file`: Code's `file` parameter is documented as
        // accepted and not yet supported, so passing it would look like it
        // worked and attach nothing. The folder goes across and the prompt
        // names the file inside it.
        Via::Code if job.file.is_some() => format!(
            "claude://code/new?q={}&folder={}",
            encode(&job.prompt),
            encode(&path.parent().unwrap_or(Path::new("/")).to_string_lossy())
        ),
        Via::Code => format!("claude://code/new?q={}", encode(&job.prompt)),
        Via::Web { base } => format!("{base}{}", encode(&job.prompt)),
        Via::Cli { .. } => return None,
    })
}


/// A local tool that is running right now.
pub(crate) struct LocalRun {
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
pub(crate) const LOCAL_TIMEOUT: Duration = Duration::from_secs(240);

#[cfg(test)]
mod tests {
    use super::{route_url, verified_note, Handoff, Trust, Via, ASSISTANTS};

    /// The note names what we have checked, and the point of it is that the
    /// list stays true as the table changes — a sentence claiming Perplexity
    /// is verified would be worse than no sentence at all.
    #[test]
    fn the_note_names_only_what_is_verified() {
        let note = verified_note();
        for assistant in ASSISTANTS {
            for route in assistant.routes {
                if route.trust == Trust::Verified && route.via.available() {
                    let named = if route.label == "Browser" {
                        assistant.name
                    } else {
                        route.label
                    };
                    assert!(note.contains(named), "{named:?} is verified but not in: {note}");
                }
            }
        }
        for unverified in ["Perplexity", "Le Chat", "Grok"] {
            assert!(
                !note.contains(unverified),
                "{unverified:?} has never been checked: {note}"
            );
        }
    }

    /// A route with no link of its own says so rather than building a wrong
    /// one — the terminal reads the file itself.
    #[test]
    fn the_command_line_has_no_url() {
        let job = Handoff {
            prompt: "anything".into(),
            file: Some(std::path::PathBuf::from("/tmp/x.pdf")),
        };
        assert!(route_url(
            Via::Cli {
                program: "claude",
                argv: &["-p"]
            },
            &job
        )
        .is_none());
    }

    /// A hand-off with nothing to attach still gets a link. The tailoring
    /// screen sends a posting and a menu and has no file at all, and before
    /// this the desktop routes built `file=` around an empty path.
    #[test]
    fn a_handoff_with_no_file_still_has_a_link() {
        let job = Handoff {
            prompt: "pick from this menu".into(),
            file: None,
        };
        for via in [Via::Cowork, Via::Code, Via::Web { base: "https://x/?q=" }] {
            let url = route_url(via, &job).expect("a link");
            assert!(url.contains("pick"), "{url}");
            assert!(!url.contains("file="), "nothing to attach: {url}");
            assert!(!url.contains("folder="), "nothing to open: {url}");
        }
    }

    #[test]
    fn parse_command_words_handles_quotes_and_spaces() {
        assert_eq!(
            super::parse_command_words("ollama run llama3:8b"),
            vec!["ollama", "run", "llama3:8b"]
        );
        assert_eq!(
            super::parse_command_words("custom-cli --model 'my custom model' -v"),
            vec!["custom-cli", "--model", "my custom model", "-v"]
        );
    }

    #[test]
    fn detect_all_assistants_reports_known_assistants() {
        let detected = super::detect_all_assistants();
        assert!(detected.iter().any(|a| a.id == "claude-desktop"));
        assert!(detected.iter().any(|a| a.id == "ollama"));
        assert!(detected.iter().any(|a| a.id == "gemini"));
    }
}
