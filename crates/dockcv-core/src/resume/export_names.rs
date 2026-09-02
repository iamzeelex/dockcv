//! What an export is called on disk, and what happens when that name is taken.
//!
//! Filenames are where user data meets the filesystem: a company with a slash
//! in it, a name in a script the disk has never seen, a role long enough to
//! break a 255-byte limit. Kept apart from the model because none of it is
//! about what a résumé *is*, and all of it has to be testable without one.

use std::path::{Path, PathBuf};

use super::model::ResumeDoc;

/// The longest filename stem we will produce.
///
/// Every filesystem this app runs on caps a name component at 255 *bytes*, and
/// a stem is only part of one: the extension and the ` (2)` a collision adds
/// come after it. 200 leaves room for both and is still longer than any name
/// and role a person actually has.
pub const MAX_FILENAME_STEM: usize = 200;

/// Turn a resolved pattern into something a filesystem will accept.
///
/// This is where user data meets the filesystem, so it takes the widest rule of
/// the three platforms rather than the host's: `\ : * ? " < > |` are illegal on
/// Windows and merely awkward elsewhere, and a name that opens on the machine
/// it was written on but not on the one it was copied to is the failure this
/// avoids. Nothing is ever *refused* — a name is repaired, because the export
/// is the thing the user wanted and the punctuation is not.
pub fn sanitize_filename_stem(s: &str) -> String {
    let mut out = String::with_capacity(s.len());

    for c in s.chars() {
        if matches!(
            c,
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'..='\x1f' | '\x7f'
        ) {
            out.push('-');
        } else {
            out.push(c);
        }
    }

    // Replacing a character with a dash creates runs — `a/b` beside an existing
    // ` - ` separator turns into ` - - `. Collapse until it stops changing
    // rather than a fixed number of times, so a pathological name settles too.
    let mut cleaned = out;
    loop {
        let next = cleaned
            .replace(" - - ", " - ")
            .replace("--", "-")
            .replace("  ", " ")
            .replace("-.", ".")
            .replace(" .", ".");
        if next == cleaned {
            break;
        }
        cleaned = next;
    }

    let trimmed = cleaned.trim().trim_matches(&['-', '_', ' '][..]);

    // `.` and `..` are directory entries, not names. They only arise from a
    // pattern that resolved to nothing but punctuation, and the fallback below
    // is a better answer than a write that fails.
    if trimmed.is_empty() || trimmed.chars().all(|c| c == '.') {
        return "CV".to_string();
    }

    // Cut on a character boundary: a stem is user text and may be any script.
    if trimmed.len() > MAX_FILENAME_STEM {
        let mut cut = MAX_FILENAME_STEM;
        while cut > 0 && !trimmed.is_char_boundary(cut) {
            cut -= 1;
        }
        return trimmed[..cut].trim_end().to_string();
    }

    trimmed.to_string()
}

/// Disambiguate a file path if a file already exists at `path` by appending ` (1)`, ` (2)`, etc.
/// If `path` does not exist on disk, it is returned unchanged.
pub fn disambiguate_filename(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    let extension = path.extension().and_then(|s| s.to_str());

    for i in 1..=10000 {
        let candidate_filename = match extension {
            Some(ext) if !ext.is_empty() => format!("{file_stem} ({i}).{ext}"),
            _ => format!("{file_stem} ({i})"),
        };
        let candidate_path = parent.join(candidate_filename);
        if !candidate_path.exists() {
            return candidate_path;
        }
    }

    path.to_path_buf()
}

/// What to do about an export whose name is already taken.
///
/// The third answer A10 names — edit the name — belongs to a rename control the
/// batch sheet does not have yet, and its absence is why this is an enum with
/// two variants rather than three: a choice that cannot be offered should not be
/// representable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnCollision {
    /// Write beside the existing file, under the pattern's own disambiguator.
    KeepBoth,
    /// Overwrite it. Only ever reached by the user saying so.
    Replace,
}

/// Where one file would land, and what is already there.
///
/// The single export and every row of a batch resolve through
/// [`resolve_destination`], so there is one answer to "may this write happen"
/// and one place to test it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination {
    /// The name the pattern produced, before any collision was considered.
    pub proposed: PathBuf,
    /// The name that will actually be written, once it was.
    pub target: PathBuf,
    /// Whether `proposed` was already taken — by a file on disk, or by a file
    /// an earlier row of the same batch has claimed.
    pub collides: bool,
}

impl Destination {
    /// Whether this write will destroy something that exists.
    ///
    /// The floor A10 sets is that this is only ever true because the user said
    /// [`OnCollision::Replace`].
    pub fn overwrites(&self) -> bool {
        self.collides && self.target == self.proposed
    }
}

/// One preset's row in a batch export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedExport {
    /// The preset this renders.
    pub preset: String,
    /// Its index in `ResumeDoc::presets`.
    pub preset_index: usize,
    /// Where it goes.
    pub destination: Destination,
}

/// Decide where `proposed` may actually be written.
///
/// `claimed` is what other writes in the same gesture have already taken, which
/// the filesystem cannot answer for: two rows of one batch aiming at one name
/// are always separated, whatever the policy, because that is not a collision
/// the user chose to resolve — it is the batch overwriting itself, and under
/// `Replace` only the last preset would survive.
pub fn resolve_destination(
    proposed: PathBuf,
    on_collision: OnCollision,
    claimed: &[PathBuf],
) -> Destination {
    let claimed_here = claimed.contains(&proposed);
    let collides = claimed_here || proposed.exists();

    let target = if collides && (on_collision == OnCollision::KeepBoth || claimed_here) {
        disambiguate_among(&proposed, claimed)
    } else {
        proposed.clone()
    };

    Destination {
        proposed,
        target,
        collides,
    }
}

/// Work out exactly which files a batch export would write, before writing any.
///
/// Two documents can resolve to one name — a pattern of `{name}` alone gives
/// every preset the same one — so a row is checked against the rows planned
/// before it as well as against the disk. Without that, the second file in a
/// batch silently replaces the first, which is the failure this whole plan
/// exists to prevent, arriving from inside the feature meant to prevent it.
pub fn plan_batch(
    doc: &ResumeDoc,
    folder: &Path,
    extension: &str,
    today: &str,
    on_collision: OnCollision,
) -> Vec<PlannedExport> {
    let mut planned: Vec<PlannedExport> = Vec::with_capacity(doc.presets.len());
    let mut claimed: Vec<PathBuf> = Vec::with_capacity(doc.presets.len());

    for (preset_index, preset) in doc.presets.iter().enumerate() {
        let stem = doc.export_filename_stem(Some(&preset.name), None, today);
        let proposed = folder.join(format!("{stem}.{extension}"));
        let destination = resolve_destination(proposed, on_collision, &claimed);

        claimed.push(destination.target.clone());
        planned.push(PlannedExport {
            preset: preset.name.clone(),
            preset_index,
            destination,
        });
    }

    planned
}

/// [`disambiguate_filename`], also avoiding names this gesture has claimed.
fn disambiguate_among(path: &Path, claimed: &[PathBuf]) -> PathBuf {
    let taken = |candidate: &Path| candidate.exists() || claimed.iter().any(|c| c == candidate);
    if !taken(path) {
        return path.to_path_buf();
    }

    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    let extension = path.extension().and_then(|s| s.to_str());

    for i in 1..=10_000 {
        let name = match extension {
            Some(ext) if !ext.is_empty() => format!("{stem} ({i}).{ext}"),
            _ => format!("{stem} ({i})"),
        };
        let candidate = parent.join(name);
        if !taken(&candidate) {
            return candidate;
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::{Basics, Preset, Resume};

    /// A scratch directory that cleans itself up, so a test that plans against
    /// real files does not leave any behind.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the clock is after 1970")
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("dockcv_{tag}_{nanos}"));
            std::fs::create_dir_all(&dir).expect("scratch directory");
            Self(dir)
        }

        fn touch(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, b"already here").expect("write");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_sanitizer_repairs_rather_than_refuses() {
        assert_eq!(sanitize_filename_stem("My/Resume:2026*?"), "My-Resume-2026");
        assert_eq!(sanitize_filename_stem("a\\b<c>d|e\"f"), "a-b-c-d-e-f");
        assert_eq!(sanitize_filename_stem("   ---   "), "CV");
        assert_eq!(sanitize_filename_stem(""), "CV");
        // A stem that is only dots is not a filename any OS will accept.
        assert_eq!(sanitize_filename_stem(".."), "CV");
        assert_eq!(sanitize_filename_stem("."), "CV");
        // Names survive whole: a hyphen someone typed is not a separator we left.
        assert_eq!(
            sanitize_filename_stem("Ann-Marie O'Neil - Sr. Dev"),
            "Ann-Marie O'Neil - Sr. Dev"
        );
        // Long enough to break every filesystem there is, so it gets cut — and
        // cut on a character boundary, whatever the script.
        let long = sanitize_filename_stem(&"Разработчик ".repeat(40));
        assert!(long.len() <= MAX_FILENAME_STEM, "{}", long.len());
        assert!(long.starts_with("Разработчик"));
    }

    #[test]
    fn disambiguate_filename_walks_up_until_the_name_is_free() {
        let dir = TempDir::new("disambiguate");
        let base = dir.0.join("Resume.pdf");
        assert_eq!(disambiguate_filename(&base), base);

        dir.touch("Resume.pdf");
        assert_eq!(disambiguate_filename(&base), dir.0.join("Resume (1).pdf"));

        dir.touch("Resume (1).pdf");
        assert_eq!(disambiguate_filename(&base), dir.0.join("Resume (2).pdf"));
    }

    /// A10's floor, stated as an invariant over the one function every write
    /// resolves through: a file is destroyed only because the user chose
    /// `Replace`. Nothing about the UI can make this false without failing
    /// here first.
    #[test]
    fn nothing_is_overwritten_unless_the_user_asked_for_it() {
        let dir = TempDir::new("floor");
        let taken = dir.touch("Albert Einstein.pdf");
        let free = dir.0.join("Marie Curie.pdf");

        for policy in [OnCollision::KeepBoth, OnCollision::Replace] {
            for proposed in [taken.clone(), free.clone()] {
                let d = resolve_destination(proposed.clone(), policy, &[]);
                assert_eq!(d.proposed, proposed);
                assert_eq!(d.collides, proposed.exists());
                assert!(
                    !d.overwrites() || policy == OnCollision::Replace,
                    "{policy:?} would destroy {:?}",
                    d.target
                );
                if policy == OnCollision::KeepBoth {
                    assert!(
                        !d.target.exists(),
                        "keeping both still aimed at {:?}",
                        d.target
                    );
                }
            }
        }
    }

    #[test]
    fn keeping_both_walks_past_a_name_this_gesture_already_claimed() {
        let dir = TempDir::new("claimed");
        let proposed = dir.0.join("Albert Einstein.pdf");
        // Nothing on disk, but an earlier write in the same gesture wants it.
        let claimed = vec![proposed.clone()];

        let d = resolve_destination(proposed.clone(), OnCollision::KeepBoth, &claimed);
        assert!(d.collides);
        assert_eq!(d.target, dir.0.join("Albert Einstein (1).pdf"));

        // Even under Replace: two writes of one gesture aiming at one file is
        // not a collision the user resolved, it is the gesture eating itself.
        let d = resolve_destination(proposed, OnCollision::Replace, &claimed);
        assert_eq!(d.target, dir.0.join("Albert Einstein (1).pdf"));
        assert!(!d.overwrites());
    }

    fn doc_with(pattern: &str, presets: &[&str]) -> ResumeDoc {
        let resume = Resume {
            basics: Basics {
                name: "Albert Einstein".into(),
                label: "Principal Systems Architect".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut doc = ResumeDoc::from_resume(resume, "Base");
        doc.export.filename_pattern = pattern.into();
        doc.presets = presets
            .iter()
            .map(|name| Preset {
                name: (*name).to_string(),
                selection: vec![],
                hidden: vec![],
            })
            .collect();
        doc
    }

    #[test]
    fn a_plan_with_nothing_in_its_way_writes_the_names_the_pattern_produced() {
        let dir = TempDir::new("plan_clear");
        let doc = doc_with("{name} - {preset}", &["Concise", "Extended"]);

        let plan = plan_batch(&doc, &dir.0, "pdf", "2026-09-02", OnCollision::KeepBoth);
        assert_eq!(plan.len(), 2);
        assert_eq!(
            plan[0].destination.target,
            dir.0.join("Albert Einstein - Concise.pdf")
        );
        assert_eq!(
            plan[1].destination.target,
            dir.0.join("Albert Einstein - Extended.pdf")
        );
        assert!(plan
            .iter()
            .all(|p| !p.destination.collides && !p.destination.overwrites()));
    }

    /// The floor A10 sets: nothing is overwritten unless the user said so, and
    /// the assertion is on the resolution logic rather than on reading the UI.
    #[test]
    fn keep_both_never_targets_a_file_that_already_exists() {
        let dir = TempDir::new("plan_keep_both");
        dir.touch("Albert Einstein - Concise.pdf");
        let doc = doc_with("{name} - {preset}", &["Concise", "Extended"]);

        let plan = plan_batch(&doc, &dir.0, "pdf", "2026-09-02", OnCollision::KeepBoth);
        assert!(plan[0].destination.collides);
        assert_eq!(
            plan[0].destination.target,
            dir.0.join("Albert Einstein - Concise (1).pdf")
        );
        assert!(!plan[0].destination.overwrites());
        assert!(!plan[1].destination.collides);

        for step in &plan {
            assert!(
                !step.destination.target.exists(),
                "{:?} would be written over",
                step.destination.target
            );
        }
    }

    #[test]
    fn replace_targets_the_existing_file_and_says_so() {
        let dir = TempDir::new("plan_replace");
        dir.touch("Albert Einstein - Concise.pdf");
        let doc = doc_with("{name} - {preset}", &["Concise", "Extended"]);

        let plan = plan_batch(&doc, &dir.0, "pdf", "2026-09-02", OnCollision::Replace);
        assert!(plan[0].destination.collides);
        assert!(plan[0].destination.overwrites());
        assert_eq!(plan[0].destination.target, plan[0].destination.proposed);
        assert!(!plan[1].destination.overwrites());
    }

    /// Two presets can resolve to one name. Without checking the rows planned
    /// before it, the second file replaces the first — the failure the whole
    /// sheet exists to prevent, arriving from inside it.
    #[test]
    fn two_presets_that_resolve_to_one_name_are_still_two_files() {
        let dir = TempDir::new("plan_self");
        // A pattern with no `{preset}` in it: every preset gets the same stem.
        let doc = doc_with("{name}", &["Concise", "Extended", "FAANG"]);

        for policy in [OnCollision::KeepBoth, OnCollision::Replace] {
            let plan = plan_batch(&doc, &dir.0, "pdf", "2026-09-02", policy);
            let targets: Vec<_> = plan.iter().map(|p| &p.destination.target).collect();
            assert_eq!(
                targets,
                vec![
                    &dir.0.join("Albert Einstein.pdf"),
                    &dir.0.join("Albert Einstein (1).pdf"),
                    &dir.0.join("Albert Einstein (2).pdf"),
                ],
                "under {policy:?} the batch overwrote itself"
            );
        }
    }
}
