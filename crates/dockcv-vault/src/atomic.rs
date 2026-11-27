//! Safe atomic writes to disk.
//!
//! Replaces destination files atomically using uniquely-named temporary files
//! in the same directory, preventing collisions between concurrent writes and
//! cleaning up on failure.
//!
//! A replace is a new file where the old one was, so whatever belonged to the
//! old one and not to its bytes is lost unless it is carried across on purpose.
//! Three things are, because each was lost in a way the person could not see:
//! the file's permissions, its being read-only, and its being a symlink.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_NONCE: AtomicU64 = AtomicU64::new(0);

/// Atomically write `content` to `path` using a uniquely named temporary
/// file in the same directory, replacing destination on success and cleaning
/// up temporary files on error.
///
/// Refuses a file whose permissions say nobody may write it. A rename does not
/// ask the file — only its folder — so `chmod 444` on a CV used to be replaced
/// by a writable copy on the next keystroke; a person who locked a file meant
/// it. The permissions of a file that *is* replaced are carried over, so a CV
/// kept `600` does not come back readable by everyone on the machine.
pub fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    let target = through_symlink(path);
    let existing = fs::metadata(&target).ok();
    if existing
        .as_ref()
        .is_some_and(|m| m.permissions().readonly())
    {
        return Err(format!(
            "{} is read-only, so DockCV left it as it is",
            path.display()
        ));
    }

    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let pid = std::process::id();
    let nonce = TEMP_FILE_NONCE.fetch_add(1, Ordering::Relaxed);
    let stem = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let tmp = parent.join(format!(".{stem}.{pid}-{nonce}.tmp"));

    if let Err(e) = fs::write(&tmp, content) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("write {}: {e}", tmp.display()));
    }
    if let Some(meta) = &existing {
        // Best effort: a filesystem that will not take the mode still takes
        // the write, and refusing the save over it would be the wrong trade.
        let _ = fs::set_permissions(&tmp, meta.permissions());
    }
    if let Err(e) = fs::rename(&tmp, &target) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("rename {}: {e}", path.display()));
    }
    Ok(())
}

/// The file a write to `path` should replace: the one a symlink names, or
/// `path` itself.
///
/// A CV kept in a git checkout elsewhere and linked into the vault is a
/// File-over-App thing to do. Renaming onto the link replaced it with a plain
/// copy, so the file the person actually keeps stopped receiving edits and
/// nothing said so.
fn through_symlink(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    // A few hops, not a loop until done: a link that names itself is a cycle.
    for _ in 0..8 {
        match fs::read_link(&current) {
            Ok(target) if target.is_absolute() => current = target,
            Ok(target) => {
                current = current
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(target)
            }
            Err(_) => break,
        }
    }
    current
}
