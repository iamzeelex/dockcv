//! Data models for the document importer.

use crate::resume::model::ResumeDoc;
use std::collections::HashMap;

/// Confidence rating for an extracted section or field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// The result of importing an external document file.
#[derive(Clone)]
pub struct ImportedDoc {
    /// Candidate document model converted to our ResumeDoc format.
    pub doc: ResumeDoc,
    /// Map of field key paths to confidence levels.
    pub confidence: HashMap<String, Confidence>,
    /// Any raw text lines or blocks that could not be mapped into the schema.
    pub unparsed: Vec<String>,
    /// Source format name (e.g. "PDF", "DOCX", "JSON Resume", "Markdown").
    pub format_name: String,
}

impl ImportedDoc {
    pub fn new(format_name: impl Into<String>, doc: ResumeDoc) -> Self {
        Self {
            doc,
            confidence: HashMap::new(),
            unparsed: Vec::new(),
            format_name: format_name.into(),
        }
    }

    pub fn set_confidence(&mut self, key: impl Into<String>, level: Confidence) {
        self.confidence.insert(key.into(), level);
    }
}
