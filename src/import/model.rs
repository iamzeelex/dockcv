//! Data models for the document importer.

use crate::render::Rendered;
use crate::resume::model::ResumeDoc;
use std::collections::HashMap;

/// Confidence rating for an extracted section or field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[allow(dead_code)]
impl Confidence {
    pub fn label(&self) -> &'static str {
        match self {
            Confidence::High => "Verified",
            Confidence::Medium => "Review Suggested",
            Confidence::Low => "⚠️ Requires Review",
        }
    }
}

/// The result of importing an external document file.
#[allow(dead_code)]
#[derive(Clone)]
pub struct ImportedDoc {
    /// Candidate document model converted to our ResumeDoc format.
    pub doc: ResumeDoc,
    /// Map of field key paths to confidence levels.
    pub confidence: HashMap<String, Confidence>,
    /// Any raw text lines or blocks that could not be mapped into the schema.
    pub unparsed: Vec<String>,
    /// Optional raster preview of the first page.
    ///
    /// Always `None` for PDF since the engine went pure-Rust — see
    /// `engines/pdf.rs`. The `FIRST-RUN IMPORT` mockup never draws a thumbnail,
    /// so no screen is waiting on it; `import_review.rs` already handles `None`.
    pub preview: Option<Rendered>,
    /// Source format name (e.g. "PDF", "DOCX", "JSON Resume", "Markdown").
    pub format_name: String,
}

impl ImportedDoc {
    pub fn new(format_name: impl Into<String>, doc: ResumeDoc) -> Self {
        Self {
            doc,
            confidence: HashMap::new(),
            unparsed: Vec::new(),
            preview: None,
            format_name: format_name.into(),
        }
    }

    pub fn set_confidence(&mut self, key: impl Into<String>, level: Confidence) {
        self.confidence.insert(key.into(), level);
    }
}
