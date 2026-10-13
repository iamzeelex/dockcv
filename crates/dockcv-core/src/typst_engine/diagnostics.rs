use typst::diag::{Severity as TypstSeverity, SourceDiagnostic};

#[cfg(feature = "raster")]
use super::geometry::PageGeometry;
#[cfg(feature = "raster")]
use super::raster::Pixels;

/// Join compiler diagnostics into one human-readable error string.
pub(crate) fn join_diagnostics<I: IntoIterator<Item = SourceDiagnostic>>(diagnostics: I) -> String {
    diagnostics
        .into_iter()
        .map(|d| d.message.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Severity Typst itself assigned a diagnostic, carried through unchanged so
/// a warning is never surfaced to the user as if it were a failure (US-07,
/// review P-06: today's `join_diagnostics` flattens both into one string —
/// this is what replaces it for callers that need to tell them apart).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

impl From<TypstSeverity> for Severity {
    fn from(severity: TypstSeverity) -> Self {
        match severity {
            TypstSeverity::Error => Severity::Error,
            TypstSeverity::Warning => Severity::Warning,
        }
    }
}

/// One compiler diagnostic, translated into a short clause a person can
/// read, in place of `SourceDiagnostic`'s own message/span/hint triplet.
///
/// `message` deliberately carries **no** location, span or line/column
/// notation — "not a raw span dump" (review P-06) — and no leading
/// "Couldn't compile" or trailing punctuation, since only the caller knows
/// whether it can name the résumé section responsible
/// (`resume::diagnostics::describe` composes the final sentence).
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    /// Byte offset into the compiled source this diagnostic's span starts
    /// at, when Typst can resolve one (it cannot for a "detached" span — a
    /// diagnostic about the compiler's own behavior rather than a
    /// particular piece of source text, e.g. the missing-font warning).
    pub source_offset: Option<usize>,
}

/// Translate a raw Typst diagnostic message into a clause a résumé author —
/// who has never seen Typst — can read without translation.
///
/// Only the free-text fields the template embeds as raw Typst markup
/// (`summary`, `highlights` — see `resume::template`'s module doc) can ever
/// carry user-typed syntax that breaks compilation; every plain field is
/// quoted into a string literal first, so it cannot. That bounds the
/// realistic failure modes to markup mistakes (an unbalanced bracket, quote
/// or the odd stray `#`), which is what this covers explicitly. Anything
/// else falls back to Typst's own wording — still not a span dump, just not
/// independently translated.
#[cfg(feature = "raster")]
pub(crate) fn humanize(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("unclosed delimiter") {
        "a bracket, parenthesis or quote here is never closed".to_string()
    } else if lower.contains("unexpected closing bracket") {
        "there's an extra `]` with no opening bracket to match it".to_string()
    } else if lower.contains("string not terminated") || lower.contains("unterminated string") {
        "a quote (\") here is never closed".to_string()
    } else if lower.starts_with("unknown variable") {
        "there's a `#` here that Typst reads as code, but what follows isn't something it recognizes"
            .to_string()
    } else {
        raw.trim().to_string()
    }
}

/// Everything one compile attempt produced: the rendered pixels/geometry on
/// success (the same shape [`TypstEngine::compile_to_pixels`] returns), and
/// every diagnostic Typst emitted along the way, translated via
/// [`Diagnostic`]. `compile_to_pixels` itself is unchanged by this — it
/// keeps testing the rasterization/geometry path in isolation; this is the
/// entry point the editor's preview should call instead, since it needs the
/// warnings `compile_to_pixels` throws away to answer "compiling / ready /
/// error" (US-07) rather than just "did it produce pixels."
#[cfg(feature = "raster")]
pub struct CompileAttempt {
    /// `Ok` exactly when the compile succeeded. `Err(())` carries no
    /// message of its own — the *why* is always in `diagnostics`, which is
    /// guaranteed to hold at least one `Severity::Error` entry in that case.
    pub result: Result<(Pixels, PageGeometry), ()>,
    /// Every diagnostic from this attempt, in emission order: any warnings
    /// first, then any compile errors. A non-empty list does not mean
    /// failure — check `result`, or filter on `Severity` directly.
    pub diagnostics: Vec<Diagnostic>,
}
