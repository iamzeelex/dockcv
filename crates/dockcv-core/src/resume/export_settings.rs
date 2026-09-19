//! Export naming and per-document export history.

use serde::{Deserialize, Serialize};

use super::model::ResumeDoc;

/// What the filename pattern's tokens stand for on one particular export.
///
/// Named fields rather than six positional `&str`s: every one of them is a
/// string, so a transposed pair would compile and quietly produce a filename
/// with the company where the role belongs.
///
/// A field left empty removes its token *and* the separator beside it, so a
/// pattern is safe to write with tokens that only some export paths can fill —
/// `{company}` resolves only when the export starts from an application card.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExportTokens<'a> {
    pub name: &'a str,
    pub role: &'a str,
    pub preset: &'a str,
    pub company: &'a str,
    pub variant: &'a str,
    pub date: &'a str,
}

/// How this document names the files it exports.
///
/// Document-level rather than app-level: two CVs in a vault are for different
/// jobs, and an app-wide pattern would make the second export of one of them
/// propose the other's name.
///
/// *Where* it last sent one is deliberately not here. A folder is a fact about
/// one machine, and `/Users/somebody/Downloads` in a document TOML points at
/// nothing on the second laptop — so it lives in `config.rs` with the other
/// preferences about looking. See the storage rules in `CLAUDE.md`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSettings {
    #[serde(default = "ExportSettings::default_pattern")]
    pub filename_pattern: String,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            filename_pattern: Self::default_pattern(),
        }
    }
}

impl ExportSettings {
    pub const DEFAULT_PATTERN: &'static str = "{name} - {role} - {preset}";

    pub fn default_pattern() -> String {
        Self::DEFAULT_PATTERN.to_string()
    }

    /// Whether the export settings match the default state.
    pub fn is_default(&self) -> bool {
        self.filename_pattern.trim() == Self::DEFAULT_PATTERN
            || self.filename_pattern.trim().is_empty()
    }

    /// Preset filename patterns offered in the layout rail.
    pub const PRESETS: &'static [(&'static str, &'static str)] = &[
        ("Name · Role · Preset", "{name} - {role} - {preset}"),
        ("Name · Role · Company", "{name} - {role} - {company}"),
        ("Name · Role", "{name} - {role}"),
        ("Name · Preset", "{name} - {preset}"),
        ("Name · Date", "{name} - {date}"),
        ("Name only", "{name}"),
    ];

    /// Resolve tokens in the pattern and sanitize the resulting filename stem.
    pub fn resolve_filename(&self, tokens: &ExportTokens<'_>) -> String {
        let ExportTokens {
            name,
            role,
            preset,
            company,
            variant,
            date,
        } = *tokens;

        let pattern = if self.filename_pattern.trim().is_empty() {
            Self::DEFAULT_PATTERN
        } else {
            self.filename_pattern.as_str()
        };

        let mut result = pattern.to_string();

        let clean_company = company.trim();
        if clean_company.is_empty() {
            result = remove_token_with_separators(&result, "company");
        } else {
            result = result.replace("{company}", clean_company);
        }

        let clean_preset = preset.trim();
        if clean_preset.is_empty() {
            result = remove_token_with_separators(&result, "preset");
        } else {
            result = result.replace("{preset}", clean_preset);
        }

        let clean_role = role.trim();
        if clean_role.is_empty() {
            result = remove_token_with_separators(&result, "role");
            result = remove_token_with_separators(&result, "label");
        } else {
            result = result
                .replace("{role}", clean_role)
                .replace("{label}", clean_role);
        }

        let clean_variant = variant.trim();
        if clean_variant.is_empty() {
            result = remove_token_with_separators(&result, "variant");
        } else {
            result = result.replace("{variant}", clean_variant);
        }

        let clean_date = date.trim();
        if clean_date.is_empty() {
            result = remove_token_with_separators(&result, "date");
        } else {
            result = result.replace("{date}", clean_date);
        }

        let name_val = if name.trim().is_empty() {
            "CV"
        } else {
            name.trim()
        };
        result = result.replace("{name}", name_val);

        super::export_names::sanitize_filename_stem(&result)
    }
}

/// One file that left the building.
///
/// The answer to "which file did I actually send in July" for a CV that never
/// became an application card — and the only place a preset name is allowed to
/// sit beside a filename, because this is inside the vault and the file is not.
///
/// Deliberately *not* a [`crate::resume::model::Snapshot`], though the two are the same idea one level
/// apart. A snapshot is a copy the vault keeps: `file` names it inside
/// `<vault>/snapshots/`, and the bytes are ours for as long as the vault exists.
/// An export is a record of a file we handed over, at a path the user chose,
/// which we do not own and cannot promise is still there. Bending one into the
/// other would mean a `version` nobody increments and a `file` that is sometimes
/// a name and sometimes an absolute path — and it would put the applications
/// board's guarantee at the mercy of a folder somebody emptied. What they do
/// share is the shape of the fields they have in common, so the two read the
/// same in TOML and on screen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "ExportRecordWire")]
pub struct ExportRecord {
    /// ISO date (YYYY-MM-DD), spelled as [`crate::resume::model::Snapshot::date`] and
    /// [`crate::resume::model::NextStep::date`] spell it: sortable as a string, and readable by
    /// somebody who opens the TOML.
    pub date: String,
    /// 24h `17:30`. Separate from the date, as in [`crate::resume::model::NextStep`] — two exports of
    /// one document on one afternoon are told apart by the time, and a single
    /// blob would have to be parsed to sort or to display.
    #[serde(default)]
    pub time: String,
    /// The format's file extension — `pdf`, `docx`, `md` — never the label the
    /// user saw. A label recorded in history is wrong the day it is reworded,
    /// and a row from 2026 would then disagree with a row from 2027 about the
    /// same format. The view maps the key back to a name; a key it does not
    /// know renders as itself rather than being dropped, which is also why this
    /// is a `String` and not an enum.
    pub format: String,
    /// The preset name at that moment. A label recording history, not a lookup
    /// key — the preset itself may be renamed or deleted later.
    pub preset: String,
    /// Where it was written. Absolute, and quite possibly gone: the rail says so
    /// rather than pretending, which is the point of keeping the path at all.
    pub path: std::path::PathBuf,
}

/// [`ExportRecord`] as it may be found on disk, including the shape it had
/// before the date and the time were separate fields.
///
/// The one-blob `timestamp` never reached a tagged release, only a vault
/// written while 0.3.0 was being built — but "only a development vault" is
/// still somebody's real documents, and a document that will not open is the
/// failure this project's storage rules exist to prevent. Splitting the blob
/// keeps the moment rather than defaulting it away to a blank date.
#[derive(Deserialize)]
struct ExportRecordWire {
    #[serde(default)]
    date: String,
    #[serde(default)]
    time: String,
    /// The pre-split spelling: `"2026-09-01 23:47"`. Read, never written.
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    format: String,
    #[serde(default)]
    preset: String,
    path: std::path::PathBuf,
}

impl From<ExportRecordWire> for ExportRecord {
    fn from(wire: ExportRecordWire) -> Self {
        let (date, time) = match (wire.date.is_empty(), wire.timestamp) {
            // A blob to split. Anything after the first space is the time; a
            // blob with no space in it was a date, so the time is simply
            // unknown rather than invented.
            (true, Some(blob)) => match blob.split_once(' ') {
                Some((date, time)) => (date.trim().to_string(), time.trim().to_string()),
                None => (blob.trim().to_string(), String::new()),
            },
            (_, _) => (wire.date, wire.time),
        };

        Self {
            date,
            time,
            format: wire.format,
            preset: wire.preset,
            path: wire.path,
        }
    }
}

/// How many exports one document remembers.
///
/// A document's TOML *is* the product, so the history cannot grow without end;
/// fifty is well past the point where the oldest rows name paths that no longer
/// exist. Re-exporting to the same path does not consume one of the fifty — see
/// [`ResumeDoc::record_export`].
pub const MAX_EXPORT_HISTORY: usize = 50;

fn remove_token_with_separators(s: &str, token: &str) -> String {
    let t = format!("{{{token}}}");
    if !s.contains(&t) {
        return s.to_string();
    }
    let patterns = [
        format!(" - {t}"),
        format!("{t} - "),
        format!(" – {t}"),
        format!("{t} – "),
        format!(" · {t}"),
        format!("{t} · "),
        format!("_{t}"),
        format!("{t}_"),
        format!(" {t}"),
        format!("{t} "),
        t,
    ];
    let mut cur = s.to_string();
    for pat in &patterns {
        cur = cur.replace(pat, "");
    }
    cur
}

impl ResumeDoc {
    /// Resolve the filename stem (without extension) for one export.
    ///
    /// `today` is passed in rather than read from a clock here: this crate is
    /// the data model, it compiles to wasm, and a function whose result depends
    /// on the hour is not one a round-trip test can pin down. Callers format it
    /// as `YYYY-MM-DD`, which is the only form that sorts correctly in Finder.
    pub fn export_filename_stem(
        &self,
        preset_name: Option<&str>,
        company: Option<&str>,
        today: &str,
    ) -> String {
        let profile = self.profile.active();
        self.export.resolve_filename(&ExportTokens {
            name: &profile.name,
            role: &profile.label,
            preset: preset_name.unwrap_or_default(),
            company: company.unwrap_or_default(),
            variant: self.profile.active_name(),
            date: today,
        })
    }

    /// Record one export, newest last.
    ///
    /// Exporting twice to the same path is one file, not two facts: the second
    /// write replaced the first on disk, and a history that lists both is
    /// describing a file that no longer exists beside one that does. So a path
    /// already in the history is replaced rather than appended, which also keeps
    /// the list short for the common case of re-exporting the same CV all week.
    pub fn record_export(
        &mut self,
        date: impl Into<String>,
        time: impl Into<String>,
        format: impl Into<String>,
        preset: impl Into<String>,
        path: std::path::PathBuf,
    ) {
        self.export_history.retain(|record| record.path != path);
        self.export_history.push(ExportRecord {
            date: date.into(),
            time: time.into(),
            format: format.into(),
            preset: preset.into(),
            path,
        });
        // Oldest first, so the drop takes the ones least likely to still exist.
        let overflow = self.export_history.len().saturating_sub(MAX_EXPORT_HISTORY);
        self.export_history.drain(..overflow);
    }
}
