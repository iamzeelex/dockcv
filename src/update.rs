//! Whether a newer DockCV exists — and nothing beyond finding out.
//!
//! ## The promise this had to fit inside
//!
//! DockCV makes no network request during ordinary work, and that is not a
//! marketing line: US-10's acceptance criteria ask for no external request
//! *during ordinary work*. That last phrase is doing the work. A check
//! the user switched on, and can switch off, is not ordinary work; a silent
//! one at launch would be.
//!
//! So the promise this module must not break is the honest version of it:
//!
//! * nothing happens unless the user asked, either by pressing the button or
//!   by turning the weekly check on — it is **off** until then;
//! * nothing about the user is sent. No identifier, no vault, no telemetry,
//!   and not even the version being compared: the request is a plain `GET` of
//!   a static file with no query string, and the comparison happens here;
//! * nothing is downloaded and nothing is installed. The one action offered is
//!   opening the release page in the browser.
//!
//! That last one is a security position, not laziness. An app that replaces
//! its own binary needs a signature to verify the replacement against, and
//! DockCV has no Developer ID — a self-updater without one is a hole with a
//! progress bar. Handing the download to the browser costs a click and keeps
//! the update path the same one a first install goes through.
//!
//! ## Why the network code is not in the binary
//!
//! The request is made by the system's `curl`, not by an HTTP client compiled
//! in. A pure-Rust client would have meant ~40 crates, a TLS stack and its
//! CVE surface inside a binary whose entire pitch is that it does not talk to
//! anything. This way the capability is visible from outside — a subprocess
//! anyone can see — and the dependency graph, `deny.toml` and `Cargo.lock` are
//! untouched.
//!
//! `curl` ships with macOS and with Windows since 1803, and is present on
//! essentially every Linux desktop. Where it is missing the check says so and
//! offers the page instead, which is the same place the button leads anyway.

use std::process::Command;

/// Where the version number is published: a small static file attached to the
/// newest release.
///
/// A release *asset* rather than the GitHub API on purpose — the API is rate
/// limited, shaped by someone else, and answers with far more than is being
/// asked. This answers exactly the question.
pub const FEED: &str = "https://github.com/iamzeelex/dockcv/releases/latest/download/latest.json";

/// Where a person goes to get DockCV.
///
/// Named once so that every route out of this feature — the button after a
/// successful check, and the one offered when a check fails — cannot end up
/// pointing at two different places.
pub const DOWNLOADS: &str = "https://github.com/iamzeelex/dockcv/releases";

/// Seconds before a check gives up. Short: nobody is waiting on this, and a
/// hanging check that never resolves is worse than one that says it failed.
const TIMEOUT_SECS: &str = "5";

/// How often DockCV may ask, if at all. Stored in the app config by word, so
/// the file stays readable and an unknown word falls back to the safe end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channel {
    /// Never, not even the button's own request. For someone who wants the
    /// binary to be provably silent.
    Never,
    /// Only when the button is pressed. The default, and the state a user who
    /// never opens Settings stays in.
    #[default]
    Manual,
    /// The button, plus one check a week.
    Weekly,
}

impl Channel {
    pub const ALL: [Channel; 3] = [Channel::Never, Channel::Manual, Channel::Weekly];

    pub fn word(self) -> &'static str {
        match self {
            Channel::Never => "never",
            Channel::Manual => "manual",
            Channel::Weekly => "weekly",
        }
    }

    pub fn from_word(word: &str) -> Self {
        match word {
            "never" => Channel::Never,
            "weekly" => Channel::Weekly,
            _ => Channel::Manual,
        }
    }

    /// What the setting says it does, in the user's terms.
    pub fn label(self) -> &'static str {
        match self {
            Channel::Never => "Never",
            Channel::Manual => "When I ask",
            Channel::Weekly => "Weekly",
        }
    }
}

/// What the feed says about the newest release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    /// `YYYY-MM-DD`, for the line that says how old it is.
    pub published: String,
    /// The release page — where the download and the notes both are.
    pub page: String,
}

/// Why a check produced no answer.
///
/// Separate variants because the remedies differ, and a check that cannot tell
/// "you are offline" from "the file is broken" ends up saying neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckFailure {
    /// No `curl` on this machine.
    NoTool,
    /// The request did not complete: offline, blocked, timed out.
    Unreachable,
    /// It completed and the answer was not a release.
    Malformed,
}

impl CheckFailure {
    /// What the user is told.
    ///
    /// Written for whoever is standing in front of the app, which is not
    /// necessarily somebody who knows what `curl`, a release feed or an HTTP
    /// status is — naming any of those explains nothing and makes an ordinary
    /// hiccup look like a fault in their machine. The distinctions the code
    /// keeps are for the log; the person gets one plain sentence, and the row
    /// gives them the download page either way, so there is always somewhere
    /// to go.
    pub fn message(self) -> &'static str {
        match self {
            CheckFailure::NoTool => "Couldn't check for updates from this computer.",
            CheckFailure::Unreachable => {
                "Couldn't check for updates — there may be no connection. Nothing else is \
                 affected: DockCV works offline."
            }
            CheckFailure::Malformed => "Couldn't check for updates just now.",
        }
    }
}

/// Ask the feed what the newest version is.
///
/// Blocking, and meant for a background thread. Nothing here touches the
/// vault, the config or the UI, so it is safe to run anywhere and easy to test
/// against a file:// URL.
pub fn check(feed: &str) -> Result<Release, CheckFailure> {
    parse(&fetch(feed)?)
}

/// One GET, with everything that could leak turned off.
///
/// `--user-agent` is set rather than left to curl's default: the default names
/// curl's own version, and a fixed string is one fewer thing said about the
/// machine. `--proto =https` and `--tlsv1.2` refuse a redirect that would
/// downgrade the request to plaintext; `--fail` makes an HTTP error an error
/// rather than a body to parse.
fn fetch(url: &str) -> Result<String, CheckFailure> {
    let mut command = Command::new("curl");
    command.args([
        "--silent",
        "--show-error",
        "--fail",
        "--location",
        "--max-time",
        TIMEOUT_SECS,
        "--user-agent",
        "DockCV",
    ]);
    // Only for the real feed: the tests fetch a `file://` URL, and these two
    // flags would refuse it — correctly, which is the point of having them.
    if url.starts_with("https://") {
        command.args(["--proto", "=https", "--tlsv1.2"]);
    }
    command.arg(url);

    let output = match command.output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            log::warn!("update check: no curl on this machine");
            return Err(CheckFailure::NoTool);
        }
        Err(error) => {
            log::warn!("update check: could not run curl: {error}");
            return Err(CheckFailure::Unreachable);
        }
    };

    if !output.status.success() {
        // curl's own diagnosis, which names the cause (DNS, TLS, 404) without
        // naming anything of the user's.
        log::info!(
            "update check: curl exited {} — {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return Err(CheckFailure::Unreachable);
    }

    String::from_utf8(output.stdout).map_err(|_| CheckFailure::Malformed)
}

/// Read the feed. Anything missing or empty is malformed rather than guessed:
/// a release with no version is not a release.
pub fn parse(json: &str) -> Result<Release, CheckFailure> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| CheckFailure::Malformed)?;
    let string = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    let version = string("version").ok_or(CheckFailure::Malformed)?;
    if triple(&version).is_none() {
        return Err(CheckFailure::Malformed);
    }
    Ok(Release {
        version,
        published: string("published").unwrap_or_default(),
        page: string("page").ok_or(CheckFailure::Malformed)?,
    })
}

/// `x.y.z`, with a leading `v` and any pre-release or build suffix ignored.
fn triple(version: &str) -> Option<(u64, u64, u64)> {
    let core = version
        .trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()?;
    let mut parts = core.split('.');
    let mut next = || parts.next()?.parse::<u64>().ok();
    let (major, minor, patch) = (next()?, next()?, next()?);
    parts.next().is_none().then_some((major, minor, patch))
}

/// Whether `candidate` is a version worth telling the user about.
///
/// Pre-release suffixes are compared as their release version, so `0.3.0-rc.1`
/// never announces itself as newer than `0.3.0`. That is the conservative
/// direction: the cost is a release candidate going unmentioned, and the cost
/// the other way is nagging someone about a build they already run.
///
/// Anything unparseable is not newer. A feed that starts answering nonsense
/// should go quiet, not raise a banner nobody can act on.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (triple(candidate), triple(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

/// Whether the weekly check should run now.
///
/// `cutoff` is the date a week ago (`vault::iso_days_ago(7)`), passed in rather
/// than read from the clock so the rule can be tested without one. ISO dates
/// compare correctly as strings, which is the whole reason the config stores
/// the date in that shape.
pub fn due(channel: Channel, last_checked: Option<&str>, cutoff: &str) -> bool {
    if channel != Channel::Weekly {
        return false;
    }
    match last_checked {
        // Never checked: the first launch after switching it on is the first
        // check, not a week later.
        None => true,
        Some(last) => last < cutoff,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_higher_version_is_newer_and_the_same_one_is_not() {
        assert!(is_newer("0.3.0", "0.2.0"));
        assert!(is_newer("0.2.1", "0.2.0"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
    }

    /// Components are numbers, not text. String comparison puts 0.10.0 before
    /// 0.9.0 and would strand everyone on the version before the tenth.
    #[test]
    fn ten_is_after_nine() {
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("0.9.0", "0.10.0"));
    }

    /// A release candidate does not announce itself over the release, and a
    /// feed answering nonsense says nothing at all.
    #[test]
    fn unparseable_and_pre_release_versions_never_nag() {
        assert!(!is_newer("0.3.0-rc.1", "0.3.0"));
        assert!(!is_newer("banana", "0.2.0"));
        assert!(!is_newer("0.3", "0.2.0"));
        assert!(!is_newer("0.3.0.1", "0.2.0"));
        assert!(is_newer("v0.3.0", "0.2.0"));
    }

    #[test]
    fn a_release_is_read_from_the_feed() {
        let release = parse(
            r#"{"version":"0.3.0","published":"2026-09-04",
                "page":"https://github.com/iamzeelex/dockcv/releases/tag/v0.3.0"}"#,
        )
        .expect("well-formed feed");
        assert_eq!(release.version, "0.3.0");
        assert_eq!(release.published, "2026-09-04");
        assert!(release.page.ends_with("v0.3.0"));
    }

    /// The two fields an offer cannot be made without. A feed missing either
    /// is refused rather than half-rendered — a banner with no page behind it
    /// is a dead end, and one with no version cannot be compared.
    #[test]
    fn a_feed_without_a_version_or_a_page_is_refused() {
        assert_eq!(parse("{}"), Err(CheckFailure::Malformed));
        assert_eq!(
            parse(r#"{"version":"0.3.0"}"#),
            Err(CheckFailure::Malformed)
        );
        assert_eq!(
            parse(r#"{"page":"https://example.invalid"}"#),
            Err(CheckFailure::Malformed)
        );
        assert_eq!(
            parse(r#"{"version":"","page":"https://example.invalid"}"#),
            Err(CheckFailure::Malformed)
        );
        assert_eq!(parse("not json at all"), Err(CheckFailure::Malformed));
    }

    /// A version the app could never compare is not a release either — better
    /// caught here than as a banner that can never be dismissed by updating.
    #[test]
    fn a_feed_whose_version_is_not_a_version_is_refused() {
        assert_eq!(
            parse(r#"{"version":"latest","page":"https://example.invalid"}"#),
            Err(CheckFailure::Malformed)
        );
    }

    /// The date is optional — it decorates the offer, it does not make it.
    #[test]
    fn a_feed_without_a_date_still_offers_the_release() {
        let release = parse(r#"{"version":"0.3.0","page":"https://example.invalid"}"#)
            .expect("version and page are enough");
        assert!(release.published.is_empty());
    }

    /// Off means off: neither of the two quiet channels ever schedules itself,
    /// however long ago the last check was.
    #[test]
    fn only_the_weekly_channel_is_ever_due() {
        for channel in [Channel::Never, Channel::Manual] {
            assert!(!due(channel, None, "2026-08-22"));
            assert!(!due(channel, Some("2020-01-01"), "2026-08-22"));
        }
    }

    #[test]
    fn weekly_is_due_when_it_has_never_run_or_ran_before_the_cutoff() {
        assert!(due(Channel::Weekly, None, "2026-08-22"));
        assert!(due(Channel::Weekly, Some("2026-08-21"), "2026-08-22"));
        assert!(!due(Channel::Weekly, Some("2026-08-22"), "2026-08-22"));
        assert!(!due(Channel::Weekly, Some("2026-08-29"), "2026-08-22"));
    }

    /// The word in the config file survives a round trip, and a word this
    /// build does not know falls back to the button rather than to the network.
    #[test]
    fn the_channel_round_trips_through_its_word() {
        for channel in Channel::ALL {
            assert_eq!(Channel::from_word(channel.word()), channel);
        }
        assert_eq!(Channel::from_word("hourly"), Channel::Manual);
        assert_eq!(Channel::default(), Channel::Manual);
    }

    /// The whole path, against a file on disk: no HTTP, but every step from
    /// running the tool to a parsed release is the one production uses.
    #[test]
    fn a_check_reads_a_feed_end_to_end() {
        let dir = std::env::temp_dir().join(format!("dockcv-update-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let feed = dir.join("latest.json");
        std::fs::write(
            &feed,
            r#"{"version":"9.9.9","published":"2026-09-04","page":"https://example.invalid"}"#,
        )
        .expect("write feed");

        let url = format!("file://{}", feed.display());
        match check(&url) {
            Ok(release) => {
                assert_eq!(release.version, "9.9.9");
                assert!(is_newer(&release.version, crate::app::APP_VERSION));
            }
            // A machine with no curl is a supported state, not a failed test.
            Err(CheckFailure::NoTool) => {}
            Err(other) => panic!("unexpected failure: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
