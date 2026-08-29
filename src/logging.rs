//! Where DockCV says what went wrong, in a file a user can send you.
//!
//! Until this existed the app reported failures with five `eprintln!` calls.
//! That works under `cargo run` and nowhere else: a double-clicked `.app` has
//! no terminal, so its stderr goes to the unified system log where no ordinary
//! person will ever look. A tester's bug report was therefore always going to
//! be "it didn't save" with nothing behind it.
//!
//! ## What this is not
//!
//! It is not telemetry, and there is no mechanism here that could become
//! telemetry: the log is a file on the user's own disk, and DockCV makes no
//! network calls at all (US-10). Sending it is a thing a person does, by hand,
//! having read it.
//!
//! ## The content rule
//!
//! **Log what happened, never what was written.** This app holds résumés,
//! salary-adjacent diary entries and notes people mark confidential — a log
//! that quotes any of it is a leak the moment someone pastes it into a chat to
//! ask for help. So: counts, kinds, outcomes, error text from the OS or the
//! compiler. Never a field value, a bullet, a diary entry or a document's
//! contents. Paths are logged, because "which file failed" is usually the whole
//! question, but [`redact`] rewrites the home directory to `~` first so the log
//! does not also hand over the user's account name.
//!
//! ## Why `log` and not something bigger
//!
//! `log` is already in the tree — GPUI depends on it and logs its own font,
//! asset and renderer problems through it. Taking it as a direct dependency
//! adds **no new crate**, and installing a global logger picks up everything
//! GPUI has to say for free, which is exactly the layer a graphics bug would
//! be reported from. `tracing` would be several new crates for structured
//! fields this app has no use for.
//!
//! Two levels, so a dependency cannot drown the app: DockCV's own crates log at
//! `Info` and everything else at `Warn`. `DOCKCV_LOG` and `DOCKCV_LOG_DEPS`
//! override them (`error`, `warn`, `info`, `debug`, `trace`, `off`).
//!
//! Deliberately not recorded: the macOS version. Reading it means spawning
//! `sw_vers` at startup or reaching into Objective-C, and neither is worth a
//! process launch on the path to the first frame — the app version, OS family
//! and architecture already identify a build.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use log::{Level, LevelFilter, Log, Metadata, Record};

/// Rotate once past this size. Two files of this at most, so a long-running
/// install cannot quietly spend a user's disk on our diagnostics.
const MAX_BYTES: u64 = 2 * 1024 * 1024;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// The file this session writes to. Available before [`init`] runs, since it is
/// derived from the environment rather than from any state.
pub fn log_path() -> PathBuf {
    log_dir().join("dockcv.log")
}

/// `~/Library/Logs/DockCV` on macOS — the directory the platform already means
/// by "application logs", and the one Console.app opens on. Elsewhere, beside
/// the config, because those platforms have no equally settled answer.
fn log_dir() -> PathBuf {
    let home = crate::vault::user_home_dir();
    if cfg!(target_os = "macos") {
        home.join("Library").join("Logs").join("DockCV")
    } else {
        home.join(".local").join("state").join("dockcv")
    }
}

/// Install the logger and the panic hook. Call once, first thing in `main`.
///
/// Returns the path being written to. Failing to open the file is not worth
/// refusing to start over — the app runs, `stderr` still gets everything, and
/// the failure is printed once.
pub fn init() -> PathBuf {
    let path = log_path();
    let _ = LOG_PATH.set(path.clone());

    if let Err(error) = std::fs::create_dir_all(log_dir()) {
        eprintln!("DockCV: no log directory ({error}); logging to stderr only");
    }
    rotate_if_large(&path);

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();
    if file.is_none() {
        eprintln!("DockCV: could not open {}; stderr only", path.display());
    }

    let logger = FileLogger {
        app_level: level_from_env("DOCKCV_LOG", LevelFilter::Info),
        dep_level: level_from_env("DOCKCV_LOG_DEPS", LevelFilter::Warn),
        sink: Mutex::new(Sink {
            file,
            home: crate::vault::user_home_dir().to_string_lossy().into_owned(),
        }),
    };
    let max = logger.app_level.max(logger.dep_level);

    // A second `init` would be a bug, not a condition to handle: nothing else
    // may install a logger, and the process has exactly one.
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(max);
        install_panic_hook();
        log::info!(
            "DockCV {} started — {} {}, log level {}",
            crate::app::APP_VERSION,
            std::env::consts::OS,
            std::env::consts::ARCH,
            max
        );
    }
    path
}

/// A panic is the one failure the user cannot describe, because the window is
/// simply gone. The default hook writes to stderr, which in a bundle means
/// nowhere, so this puts it where the rest of the session already is.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".into());

        // `payload_as_str` is not stable here; these two cover every panic the
        // standard library and this codebase produce.
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panicked".into());

        log::error!(
            "PANIC at {location}: {message}\n{}",
            std::backtrace::Backtrace::force_capture()
        );
        previous(info);
    }));
}

/// Keep at most one previous log. Rotating on startup rather than mid-session
/// means no size check on the hot path and no second thread.
fn rotate_if_large(path: &Path) {
    let too_big = std::fs::metadata(path).map(|m| m.len() >= MAX_BYTES);
    if matches!(too_big, Ok(true)) {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
}

fn level_from_env(var: &str, fallback: LevelFilter) -> LevelFilter {
    match std::env::var(var)
        .unwrap_or_default()
        .trim()
        .to_lowercase()
        .as_str()
    {
        "off" => LevelFilter::Off,
        "error" => LevelFilter::Error,
        "warn" => LevelFilter::Warn,
        "info" => LevelFilter::Info,
        "debug" => LevelFilter::Debug,
        "trace" => LevelFilter::Trace,
        _ => fallback,
    }
}

struct Sink {
    /// `None` when the file could not be opened — stderr still works, and a
    /// user with an unwritable home directory has larger problems than this.
    file: Option<File>,
    home: String,
}

struct FileLogger {
    app_level: LevelFilter,
    dep_level: LevelFilter,
    sink: Mutex<Sink>,
}

impl FileLogger {
    /// DockCV's crates are `dockcv` and `dockcv_ui_components`; both start the
    /// same way, and nothing else in the tree does.
    fn threshold(&self, target: &str) -> LevelFilter {
        if target.starts_with("dockcv") {
            self.app_level
        } else {
            self.dep_level
        }
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.threshold(metadata.target())
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let Ok(mut sink) = self.sink.lock() else {
            return; // a poisoned log lock must not take the app with it
        };

        let line = format!(
            "{}  {:<5} {}  {}\n",
            timestamp(),
            record.level(),
            record.target(),
            redact(&record.args().to_string(), &sink.home)
        );

        // Unbuffered on purpose: the records worth having are the ones written
        // immediately before a crash, and a BufWriter loses exactly those.
        // Volume is a handful of lines per session, so there is nothing to gain.
        if let Some(file) = sink.file.as_mut() {
            let _ = file.write_all(line.as_bytes());
        }
        if record.level() <= Level::Warn {
            eprint!("{line}");
        }
    }

    fn flush(&self) {
        if let Ok(mut sink) = self.sink.lock() {
            if let Some(file) = sink.file.as_mut() {
                let _ = file.flush();
            }
        }
    }
}

/// Rewrite the home directory to `~`.
///
/// A vault path is often `/Users/<their name>/…`, and a log exists to be sent
/// to someone. This keeps the part that answers "which file" and drops the part
/// that only identifies the person.
fn redact(message: &str, home: &str) -> String {
    if home.is_empty() || home == "." {
        return message.to_string();
    }
    message.replace(home, "~")
}

/// `2026-08-15 12:34:56.789`, local-clock-agnostic (UTC).
///
/// Wall-clock formatting without a date crate: the civil-date arithmetic is
/// `vault`'s, reused rather than repeated, because two copies of a calendar are
/// two calendars that can disagree.
fn timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let (y, mo, d) = crate::vault::civil_from_days((secs / 86_400) as i64);
    let (h, mi, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    format!(
        "{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}.{:03}",
        now.subsec_millis()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_home_directory_never_reaches_the_log() {
        let home = "/Users/sofiia";
        let line = redact("could not save /Users/sofiia/CVs/cv.toml: read-only", home);
        assert_eq!(line, "could not save ~/CVs/cv.toml: read-only");
        assert!(!line.contains("sofiia"), "the account name is the point");
    }

    /// `user_home_dir` falls back to `.` when `HOME` is unset, and rewriting
    /// every `.` in a message would mangle it into nonsense.
    #[test]
    fn a_degenerate_home_redacts_nothing() {
        assert_eq!(
            redact("saved cv.toml in 1.2s", "."),
            "saved cv.toml in 1.2s"
        );
        assert_eq!(redact("saved cv.toml", ""), "saved cv.toml");
    }

    #[test]
    fn the_level_comes_from_the_environment_and_falls_back_when_unset() {
        // Not `set_var` — tests share a process, and one test changing the
        // environment under another is the kind of flake that costs a day.
        assert_eq!(
            level_from_env("DOCKCV_LOG_TEST_UNSET", LevelFilter::Info),
            LevelFilter::Info
        );
    }

    #[test]
    fn the_app_and_its_dependencies_have_separate_thresholds() {
        let logger = FileLogger {
            app_level: LevelFilter::Info,
            dep_level: LevelFilter::Warn,
            sink: Mutex::new(Sink {
                file: None,
                home: String::new(),
            }),
        };
        assert_eq!(logger.threshold("dockcv::vault"), LevelFilter::Info);
        assert_eq!(
            logger.threshold("dockcv_ui_components::theme"),
            LevelFilter::Info
        );
        // The reason the split exists: GPUI is chatty at info, and a log the
        // user has to scroll is a log nobody reads.
        assert_eq!(logger.threshold("gpui::text_system"), LevelFilter::Warn);
    }

    #[test]
    fn a_timestamp_is_fixed_width_so_the_log_stays_a_column() {
        let stamp = timestamp();
        assert_eq!(stamp.len(), "2026-08-15 12:34:56.789".len(), "got {stamp}");
        assert_eq!(&stamp[4..5], "-");
        assert_eq!(&stamp[10..11], " ");
    }

    #[test]
    fn rotation_leaves_one_previous_log_and_only_one() {
        let dir = std::env::temp_dir().join(format!("dockcv-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("dockcv.log");

        std::fs::write(&path, vec![b'x'; (MAX_BYTES + 1) as usize]).expect("big log");
        rotate_if_large(&path);
        assert!(!path.exists(), "the oversized log moved aside");
        assert!(path.with_extension("log.1").exists());

        // A small log is left alone, so an ordinary session does not lose its
        // predecessor for nothing.
        std::fs::write(&path, b"fresh").expect("small log");
        rotate_if_large(&path);
        assert_eq!(std::fs::read(&path).unwrap(), b"fresh");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
