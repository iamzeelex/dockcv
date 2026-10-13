//! Safe atomic writes to disk.
//!
//! Replaces destination files atomically using uniquely-named temporary files
//! in the same directory, preventing collisions between concurrent writes and
//! cleaning up on failure.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_NONCE: AtomicU64 = AtomicU64::new(0);

/// Atomically write `content` to `path` using a uniquely named temporary
/// file in the same directory, replacing destination on success and cleaning
/// up temporary files on error.
pub fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let pid = std::process::id();
    let nonce = TEMP_FILE_NONCE.fetch_add(1, Ordering::Relaxed);
    let stem = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let tmp = parent.join(format!(".{stem}.{pid}-{nonce}.tmp"));

    if let Err(e) = fs::write(&tmp, content) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("write {}: {e}", tmp.display()));
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("rename {}: {e}", path.display()));
    }
    Ok(())
}
