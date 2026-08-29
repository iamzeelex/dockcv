//! Data models for the document importer.

use crate::import::notes::{Note, Part};
use crate::resume::model::ResumeDoc;

/// The result of importing an external document file.
#[derive(Clone)]
pub struct ImportedDoc {
    /// Candidate document model converted to our ResumeDoc format.
    pub doc: ResumeDoc,
    /// What the parser noticed and a person should check.
    ///
    /// Replaces a `HashMap<String, Confidence>` — see `import::notes` for why a
    /// level was the wrong shape. Empty on a clean import, which is the point:
    /// a flag that is always lit carries no information.
    pub notes: Vec<(Part, Note)>,
    /// Content the importer **read and had nowhere to put**.
    ///
    /// Named `unparsed` once, which claimed something untrue in both
    /// directions. The classifier absorbs: Work and Education open a new entry
    /// for any line that matches nothing, Skills continues the group above. So
    /// nothing here failed to *parse* — it parsed and DockCV has no field for
    /// it, which is a different fact and the one US-01 is about.
    ///
    /// What lands here:
    ///
    /// * from the classifier, lines in the contact block that are not contact
    ///   details;
    /// * from JSON Resume, the spec sections DockCV does not model —
    ///   `interests`, `references`, `basics.image`;
    /// * from LinkedIn, every CSV in the archive this engine does not map,
    ///   named with its row count.
    pub unplaced: Vec<String>,
    /// Source format name (e.g. "PDF", "DOCX", "JSON Resume", "Markdown").
    pub format_name: String,
}

impl ImportedDoc {
    pub fn new(format_name: impl Into<String>, doc: ResumeDoc) -> Self {
        Self {
            doc,
            notes: Vec::new(),
            unplaced: Vec::new(),
            format_name: format_name.into(),
        }
    }

    /// Read the document and record what is odd about it.
    ///
    /// Every engine calls this, which is what the old per-engine key-writing
    /// never managed: three of the five keys the review screen asked for were
    /// written by nobody.
    pub fn observe(&mut self) {
        self.notes.extend(crate::import::notes::observe(&self.doc));
    }

    /// A note only an engine can raise, because only the source says whether
    /// something was supposed to be there.
    pub fn note(&mut self, part: Part, note: Note) {
        self.notes.push((part, note));
    }

    /// Everything noticed about one part, in the order it was noticed.
    pub fn notes_for(&self, part: Part) -> impl Iterator<Item = String> + '_ {
        self.notes
            .iter()
            .filter(move |(p, _)| *p == part)
            .map(|(p, note)| note.message(*p))
    }
}
