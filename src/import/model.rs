//! Data models for the document importer.

use crate::import::notes::{Note, Part};
use crate::resume::model::ResumeDoc;

/// What can honestly be offered for something the importer could not place.
///
/// The panel used to offer two things and both lost: copy it by hand, which
/// puts the work back on the person at the moment the machine is holding the
/// data already parsed, or undo the import, which throws a good import away
/// over two fields. What it can offer instead depends on how much the source
/// knew, and the kinds must not be merged — pretending an unlabelled line is a
/// typed entry makes it lie about what it knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnplacedOffer {
    /// A section under a heading we can propose, because the source named the
    /// field this came out of.
    Section { heading: String },
    /// A section the person names, one line per highlight. The source had no
    /// label to offer — a line from a contact block is just a line.
    NamedByPerson,
    /// Nothing. A photo URL is not a section, and the value of the warning is
    /// precisely that it shrinks to cases like it.
    Nothing,
}

/// Where a leftover came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnplacedSource {
    /// A JSON Resume field DockCV does not model, named as the spec names it.
    JsonResume { field: &'static str },
    /// A CSV in a LinkedIn archive with no section behind it.
    LinkedIn { csv: String },
    /// A line the classifier read and could not place. No structure at all.
    Classifier,
}

/// One thing the importer read and had nowhere to put.
///
/// Structured, not a sentence. The engines used to flatten a name and a keyword
/// list into `"interests: Wildlife — Ferrets, Unicorns"` before the UI ever saw
/// it, and a typed section cannot be built back out of that sentence — so the
/// parts are kept and [`Unplaced::line`] derives the sentence from them, never
/// the other way round.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unplaced {
    pub source: UnplacedSource,
    /// The entry's own name: an interest, a referee, a CSV file.
    pub title: String,
    /// A second line under it, when the source had one.
    pub subtitle: String,
    /// Everything else it carried — keywords, the text of a reference, the rows
    /// of a table. One highlight each if this becomes a section.
    pub details: Vec<String>,
}

impl Unplaced {
    /// A line the classifier could not place.
    pub fn line_only(text: impl Into<String>) -> Self {
        Self {
            source: UnplacedSource::Classifier,
            title: text.into(),
            subtitle: String::new(),
            details: Vec::new(),
        }
    }

    /// What the import screen may offer to do with this.
    pub fn offer(&self) -> UnplacedOffer {
        match &self.source {
            // A photo is the one case with no honest offer.
            UnplacedSource::JsonResume { field } if *field == "basics.image" => {
                UnplacedOffer::Nothing
            }
            UnplacedSource::JsonResume { field } => UnplacedOffer::Section {
                heading: heading_for(field),
            },
            // The file name is a heading — `Honors.csv` is called Honors. The
            // rows are not typed entries, though: nothing here knows what the
            // columns mean, and guessing is A13's problem, not this one.
            UnplacedSource::LinkedIn { csv } => UnplacedOffer::Section {
                heading: heading_for(csv.trim_end_matches(".csv")),
            },
            UnplacedSource::Classifier => UnplacedOffer::NamedByPerson,
        }
    }

    /// The sentence the panel shows. Derived from the parts, so the parts
    /// survive — which is the whole reason this type exists.
    pub fn line(&self) -> String {
        let rest = self
            .details
            .iter()
            .filter(|d| !d.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let head = match &self.source {
            UnplacedSource::JsonResume { field } => format!("{field}: {}", self.title),
            UnplacedSource::LinkedIn { csv } => format!("{csv}: {}", self.title),
            UnplacedSource::Classifier => self.title.clone(),
        };
        match (self.subtitle.trim().is_empty(), rest.is_empty()) {
            (true, true) => head,
            (true, false) => format!("{head} — {rest}"),
            (false, true) => format!("{head} — {}", self.subtitle),
            (false, false) => format!("{head} — {} — {rest}", self.subtitle),
        }
    }
}

/// `interests` becomes `Interests`, `Volunteer Experiences` stays as it reads.
fn heading_for(field: &str) -> String {
    let mut chars = field.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Turn leftovers into a custom section on `doc`, one entry each.
///
/// This is the destination the panel used to lack. Custom sections (D-9) and
/// `CustomEntry` already fit the shapes that fall out: an interest is a title
/// with its keywords, a reference is a title with the text as a highlight, a
/// CSV row is a line. Nothing new had to be modelled — the data just had to
/// survive the trip, which is what [`Unplaced`] is for.
///
/// Returns the entries it created, so the caller can say how many rather than
/// claiming success it has not checked.
pub fn adopt_as_section(doc: &mut ResumeDoc, heading: &str, items: &[Unplaced]) -> usize {
    use crate::resume::model::{CustomEntry, ResumeDate};

    let entries: Vec<CustomEntry> = items
        .iter()
        .filter(|item| item.offer() != UnplacedOffer::Nothing)
        .map(|item| CustomEntry {
            title: item.title.clone(),
            subtitle: item.subtitle.clone(),
            start_date: ResumeDate::new(""),
            end_date: ResumeDate::new(""),
            url: String::new(),
            highlights: item
                .details
                .iter()
                .filter(|d| !d.trim().is_empty())
                .cloned()
                .collect(),
        })
        .collect();

    if entries.is_empty() {
        return 0;
    }
    let created = entries.len();
    let id = doc.add_custom_section(heading);
    if let Some(section) = doc.custom_section_mut(id) {
        *section.content.active_mut() = entries;
    }
    created
}

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
    pub unplaced: Vec<Unplaced>,
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
