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
//! ## The rule every prompt here keeps
//!
//! **A model may point at things; it may not write them.** The import prompt
//! asks for a transcription and forbids inferring a field that is not on the
//! page. The tailoring prompt hands over a menu that DockCV built and asks
//! which items to pick. Neither can produce a sentence that reaches a CV,
//! because in neither case is DockCV reading prose back out of the answer.

use gpui::Task;
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
}
