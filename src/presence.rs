//! Whether another DockCV has the same vault open.
//!
//! Two processes on one vault are ordinary on a developer's machine: the
//! installed app keeps running while `cargo run` opens the same vault, from the
//! same `config.toml`, at the same last document. Each one's saves are then a
//! genuine change under the other, and `vault::save` refuses them exactly as it
//! should. What was wrong was the explanation — the banner blamed "another
//! editor, a sync client, a `git checkout`", and the answer was a second
//! DockCV. This module is how one DockCV finds out about the other, so the
//! words can say so. It warns; it never blocks, because nothing here is a
//! reason to stop somebody opening their own files.
//!
//! ## How a DockCV says it is here
//!
//! A pair of files per process in `~/.config/dockcv/running/`: `<pid>.lock`,
//! held under an OS file lock for as long as the process lives, and
//! `<pid>.toml`, which says which build it is and which vault it has open.
//! Another process asks whether the owner is alive by trying to take that lock
//! itself — refused means alive, granted means gone, and a marker found gone is
//! removed by whoever found it.
//!
//! A lock rather than a PID check, because the OS drops a lock when a process
//! dies however it dies — a crash, a `kill -9`, a power cut — so a marker left
//! behind can neither block anything nor be mistaken for a live process; and a
//! PID is reused, so "is 4121 running" can be answered yes by a stranger. Two
//! files rather than one because Windows locks are mandatory: a reader could
//! not read the description out of a file its owner holds locked.
//!
//! ## Why not the vault
//!
//! By the three-homes test in `CLAUDE.md` this is local to the machine: a PID
//! and a file lock mean nothing on a second laptop, and a marker inside a
//! synced vault would arrive there as a file the user must not delete. Not
//! memory either, since the point is that another process reads it. Deleting
//! the directory costs nothing but the warning.
//!
//! Only builds that carry this module announce themselves. One older than it
//! on the same vault goes unnoticed and the banner falls back to the general
//! wording — deliberately: guessing from a process list which vault an old
//! build has open would be a guess presented as a fact.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

/// One running DockCV, as it describes itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    pub pid: u32,
    /// `CARGO_PKG_VERSION` of the build, so the banner can tell 0.4.0 from 0.4.1.
    pub version: String,
    /// The binary it runs from — what tells the installed app from `cargo run`.
    pub executable: PathBuf,
    /// The vault it has open, canonical so two spellings of one folder match.
    pub vault: PathBuf,
}

impl Instance {
    fn this_process(vault: &Path) -> Self {
        Self {
            pid: std::process::id(),
            version: crate::app::APP_VERSION.to_string(),
            executable: std::env::current_exe().unwrap_or_default(),
            vault: canonical(vault),
        }
    }

    /// `DockCV 0.4.0 at /Applications/DockCV.app (pid 4121)`.
    ///
    /// The bundle rather than the binary inside it, since the bundle is what a
    /// person sees in Finder; a build that is not a bundle shows its binary,
    /// and `…/target/debug/dockcv` says "development build" better than a
    /// label would. The home directory becomes `~`, as in the log.
    pub fn describe(&self) -> String {
        let shown = self
            .executable
            .ancestors()
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .unwrap_or(&self.executable);
        let home = crate::vault::user_home_dir();
        let shown = match shown.strip_prefix(&home) {
            Ok(rest) => Path::new("~").join(rest),
            Err(_) => shown.to_path_buf(),
        };
        format!(
            "DockCV {} at {} (pid {})",
            self.version,
            shown.display(),
            self.pid
        )
    }
}

/// This process's marker, held for as long as it has a vault open.
///
/// A GPUI global, like `SaveStatus`: the shell opens vaults, but the words that
/// use the answer are chosen in `save_status`, called from the editor and from
/// background writes that have an `App` and nothing else.
pub struct Presence {
    dir: PathBuf,
    me: Instance,
    /// Never read. Holding it is the announcement: the lock lives exactly as
    /// long as this handle, and so, through the OS, as long as the process.
    _lock: File,
    /// The other DockCVs already mentioned, so coming back to the window says
    /// "another DockCV" once per newcomer rather than once per activation.
    known: Vec<u32>,
}

impl Global for Presence {}

impl Presence {
    fn announce_in(dir: &Path, me: Instance) -> Result<Self, String> {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let lock_path = marker(dir, me.pid, "lock");
        let mut attempts = 0;
        let lock = loop {
            let lock = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(&lock_path)
                .map_err(|e| e.to_string())?;
            match lock.try_lock() {
                Ok(()) => {}
                // Only a live process with this PID could hold it — which this
                // process is, so a second announcement went wrong somewhere.
                Err(TryLockError::WouldBlock) => {
                    return Err(format!("{} is already held", lock_path.display()))
                }
                Err(TryLockError::Error(e)) => return Err(e.to_string()),
            }
            // Between creating the file and locking it, it was briefly a file
            // nobody held — and a DockCV looking at that moment takes that for
            // a dead one and deletes it. Holding a lock on a deleted file
            // announces nothing, so start again. Once held, nobody else can.
            if lock_path.exists() {
                break lock;
            }
            attempts += 1;
            if attempts == 3 {
                return Err(format!("{} keeps disappearing", lock_path.display()));
            }
        };
        // Built before the description is written, so a failure from here on
        // is cleaned up by `Drop` like any other exit.
        let presence = Self {
            dir: dir.to_path_buf(),
            me,
            _lock: lock,
            known: Vec::new(),
        };
        presence.describe_self()?;
        Ok(presence)
    }

    /// Say which vault this process has open now. A rename onto the real name,
    /// so a reader never parses half a file.
    fn describe_self(&self) -> Result<(), String> {
        let text = toml::to_string_pretty(&self.me).map_err(|e| e.to_string())?;
        let partial = marker(&self.dir, self.me.pid, "toml.tmp");
        fs::write(&partial, text).map_err(|e| e.to_string())?;
        fs::rename(&partial, marker(&self.dir, self.me.pid, "toml")).map_err(|e| e.to_string())
    }

    fn move_to(&mut self, vault: &Path) -> Result<(), String> {
        self.me.vault = canonical(vault);
        self.known.clear();
        self.describe_self()
    }

    /// Every other live DockCV with this vault open, oldest PID first. Clears
    /// away the markers of any that have died on the way.
    pub fn others(&self) -> Vec<Instance> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "lock") {
                continue;
            }
            let Some(pid) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            if pid == self.me.pid {
                continue;
            }
            match holder_alive(&path) {
                Some(true) => {
                    // A live owner with no readable description is one still
                    // writing it. The next look will find it.
                    let other = fs::read_to_string(marker(&self.dir, pid, "toml"))
                        .ok()
                        .and_then(|text| toml::from_str::<Instance>(&text).ok());
                    if let Some(other) = other.filter(|o| o.vault == self.me.vault) {
                        found.push(other);
                    }
                }
                Some(false) => remove_markers(&self.dir, pid),
                // Could not tell. Saying nothing is the honest answer.
                None => {}
            }
        }
        found.sort_by_key(|o| o.pid);
        found
    }

    /// The other DockCVs this one has not mentioned yet.
    fn newcomers(&mut self) -> Vec<Instance> {
        let others = self.others();
        let fresh: Vec<Instance> = others
            .iter()
            .filter(|o| !self.known.contains(&o.pid))
            .cloned()
            .collect();
        // Replaced rather than extended, so one that quits and is started
        // again is a newcomer again.
        self.known = others.iter().map(|o| o.pid).collect();
        fresh
    }
}

impl Drop for Presence {
    /// Gone on a clean exit. On any other kind the OS drops the lock and the
    /// next DockCV to look clears the files.
    fn drop(&mut self) {
        remove_markers(&self.dir, self.me.pid);
    }
}

fn marker(dir: &Path, pid: u32, extension: &str) -> PathBuf {
    dir.join(format!("{pid}.{extension}"))
}

/// Description first, lock last: the order `announce_in` creates them in,
/// reversed, so nothing ever sees a description without its lock.
fn remove_markers(dir: &Path, pid: u32) {
    for extension in ["toml", "toml.tmp", "lock"] {
        let _ = fs::remove_file(marker(dir, pid, extension));
    }
}

/// Whether the process holding `lock` is still running: `None` when the
/// question could not be asked.
fn holder_alive(lock: &Path) -> Option<bool> {
    let file = match OpenOptions::new().read(true).write(true).open(lock) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return Some(false),
        Err(_) => return None,
    };
    match file.try_lock() {
        // Taken, so nobody had it. Released when `file` drops.
        Ok(()) => Some(false),
        Err(TryLockError::WouldBlock) => Some(true),
        Err(TryLockError::Error(_)) => None,
    }
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Beside `config.toml`, because both are this machine's and neither is the
/// user's data.
fn running_dir() -> PathBuf {
    crate::config::config_dir().join("running")
}

/// Mark this process as having `vault` open, and return the other DockCVs
/// that have it open too.
///
/// Failing to announce is logged and otherwise ignored: the cost is that the
/// other process's banner keeps its general wording, which is no reason to
/// stand in the way of opening a vault.
pub fn announce(cx: &mut App, vault: &Path) -> Vec<Instance> {
    if cx.has_global::<Presence>() {
        if let Err(message) = cx.global_mut::<Presence>().move_to(vault) {
            log::warn!("could not update this DockCV's presence marker: {message}");
        }
    } else {
        match Presence::announce_in(&running_dir(), Instance::this_process(vault)) {
            Ok(presence) => cx.set_global(presence),
            Err(message) => {
                log::warn!("could not mark this vault as open here: {message}");
                return Vec::new();
            }
        }
    }
    cx.global_mut::<Presence>().newcomers()
}

/// The other DockCVs on this vault right now — asked fresh, because it is asked
/// at the moment a banner is about to name them.
pub fn others(cx: &App) -> Vec<Instance> {
    cx.try_global::<Presence>()
        .map(Presence::others)
        .unwrap_or_default()
}

/// The other DockCVs that have opened this vault since this one last said so.
pub fn newcomers(cx: &mut App) -> Vec<Instance> {
    if !cx.has_global::<Presence>() {
        return Vec::new();
    }
    cx.global_mut::<Presence>().newcomers()
}

/// Take this process's marker away. Called on quit.
pub fn withdraw(cx: &mut App) {
    if cx.has_global::<Presence>() {
        drop(cx.remove_global::<Presence>());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two "processes" in one test: `flock` and `LockFileEx` both lock per open
    /// handle, so two `Presence`s here contend exactly as two DockCVs would.
    fn instance(pid: u32, vault: &Path) -> Instance {
        Instance {
            pid,
            version: "0.0.0-test".into(),
            executable: PathBuf::from("/Applications/DockCV.app/Contents/MacOS/dockcv"),
            vault: canonical(vault),
        }
    }

    /// A running dir and a vault of its own per test, so tests in parallel
    /// never see each other's markers.
    fn scratch(name: &str) -> (PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("dockcv-presence-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let vault = root.join("vault");
        fs::create_dir_all(&vault).expect("vault dir");
        (root.join("running"), vault)
    }

    #[test]
    fn two_dockcvs_on_one_vault_see_each_other() {
        let (running, vault) = scratch("both");
        let first = Presence::announce_in(&running, instance(101, &vault)).expect("first");
        let second = Presence::announce_in(&running, instance(102, &vault)).expect("second");

        assert_eq!(first.others(), vec![instance(102, &vault)]);
        assert_eq!(second.others(), vec![instance(101, &vault)]);
        let _ = fs::remove_dir_all(running.parent().expect("root"));
    }

    #[test]
    fn a_dockcv_on_another_vault_is_not_mentioned() {
        let (running, vault) = scratch("elsewhere");
        let elsewhere = vault.with_file_name("other-vault");
        fs::create_dir_all(&elsewhere).expect("second vault");
        let here = Presence::announce_in(&running, instance(201, &vault)).expect("here");
        let _there = Presence::announce_in(&running, instance(202, &elsewhere)).expect("there");

        assert!(here.others().is_empty());
        let _ = fs::remove_dir_all(running.parent().expect("root"));
    }

    /// A clean exit takes the marker with it.
    #[test]
    fn quitting_withdraws_the_marker() {
        let (running, vault) = scratch("quit");
        let staying = Presence::announce_in(&running, instance(301, &vault)).expect("staying");
        let leaving = Presence::announce_in(&running, instance(302, &vault)).expect("leaving");
        drop(leaving);

        assert!(staying.others().is_empty());
        assert!(!marker(&running, 302, "toml").exists());
        assert!(!marker(&running, 302, "lock").exists());
        let _ = fs::remove_dir_all(running.parent().expect("root"));
    }

    /// The case the lock exists for: a process that died without cleaning up
    /// left both files, and nobody holds the lock. It must neither be reported
    /// nor survive being looked at.
    #[test]
    fn a_crashed_dockcv_is_neither_reported_nor_kept() {
        let (running, vault) = scratch("crash");
        let alive = Presence::announce_in(&running, instance(401, &vault)).expect("alive");
        let dead = instance(402, &vault);
        fs::write(
            marker(&running, 402, "toml"),
            toml::to_string_pretty(&dead).expect("toml"),
        )
        .expect("left-behind description");
        fs::write(marker(&running, 402, "lock"), "").expect("left-behind lock");

        assert!(alive.others().is_empty());
        assert!(!marker(&running, 402, "toml").exists());
        assert!(!marker(&running, 402, "lock").exists());
        let _ = fs::remove_dir_all(running.parent().expect("root"));
    }

    /// The crash happened before, and the same PID opens the vault again.
    #[test]
    fn a_stale_marker_does_not_block_announcing() {
        let (running, vault) = scratch("reuse");
        fs::create_dir_all(&running).expect("running dir");
        fs::write(marker(&running, 501, "lock"), "").expect("left-behind lock");
        fs::write(marker(&running, 501, "toml"), "not toml at all").expect("left-behind");

        let me = Presence::announce_in(&running, instance(501, &vault)).expect("announces");
        let observer = Presence::announce_in(&running, instance(502, &vault)).expect("observer");
        assert_eq!(observer.others(), vec![instance(501, &vault)]);
        drop(me);
        let _ = fs::remove_dir_all(running.parent().expect("root"));
    }

    #[test]
    fn moving_to_another_vault_is_seen_by_the_others() {
        let (running, vault) = scratch("move");
        let elsewhere = vault.with_file_name("other-vault");
        fs::create_dir_all(&elsewhere).expect("second vault");
        let watcher = Presence::announce_in(&running, instance(601, &vault)).expect("watcher");
        let mut mover = Presence::announce_in(&running, instance(602, &vault)).expect("mover");
        assert_eq!(watcher.others().len(), 1);

        mover.move_to(&elsewhere).expect("moves");
        assert!(watcher.others().is_empty());
        let _ = fs::remove_dir_all(running.parent().expect("root"));
    }

    /// Coming back to the window mentions a DockCV once, not on every return.
    #[test]
    fn a_newcomer_is_mentioned_once() {
        let (running, vault) = scratch("newcomer");
        let mut me = Presence::announce_in(&running, instance(701, &vault)).expect("me");
        assert!(me.newcomers().is_empty());

        let _other = Presence::announce_in(&running, instance(702, &vault)).expect("other");
        assert_eq!(me.newcomers(), vec![instance(702, &vault)]);
        assert!(me.newcomers().is_empty(), "already mentioned");
        let _ = fs::remove_dir_all(running.parent().expect("root"));
    }

    #[test]
    fn a_bundle_is_named_by_the_bundle_and_a_build_by_its_binary() {
        let mut installed = instance(4121, Path::new("/nonexistent"));
        installed.version = "0.4.0".into();
        assert_eq!(
            installed.describe(),
            "DockCV 0.4.0 at /Applications/DockCV.app (pid 4121)"
        );

        let mut dev = installed.clone();
        dev.executable = PathBuf::from("/srv/dockcv/target/debug/dockcv");
        assert_eq!(
            dev.describe(),
            "DockCV 0.4.0 at /srv/dockcv/target/debug/dockcv (pid 4121)"
        );
    }
}
