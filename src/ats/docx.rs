//! The other file a CV gets sent as.
//!
//! Several applicant tracking systems parse `.docx` better than they parse a
//! PDF, and a good many ask for it by name — so the Word file is not a lesser
//! export to be checked once. It has the same two questions as the PDF: does
//! every field come back, and does a reader see the *structure* or only the
//! words.
//!
//! The structure lives somewhere else in this format, and that is the whole
//! reason this module exists rather than a `to_string()`. Word records a
//! heading two ways — a paragraph *style* (`Heading1`), and an *outline level*
//! — and readers do not agree on which one they look at. `python-docx` reports
//! the style name; Word's own navigation pane honours the outline level;
//! DockCV's own importer reads styles. A heading written one way and read the
//! other is invisible, and invisible is exactly what a conformance harness is
//! for.

use docx_rs::{read_docx, DocumentChild, InsertChild, ParagraphChild, RunChild};

/// One paragraph, and everything a reader could use to decide what it is.
#[derive(Debug, Clone)]
pub struct Para {
    pub text: String,
    /// The paragraph style's name, lowercased — `heading1`, `listbullet`.
    pub style: Option<String>,
    /// Word's outline level, which the navigation pane uses and many parsers
    /// ignore.
    pub outline: Option<usize>,
    /// Carries a numbering property: a list item rather than a paragraph that
    /// begins with a dash.
    pub list: bool,
}

impl Para {
    /// Would a reader that looks for headings find one here?
    pub fn is_heading(&self) -> bool {
        self.outline.is_some()
            || self
                .style
                .as_deref()
                .is_some_and(|s| s.starts_with("heading") || s == "title")
    }
}

/// Every paragraph of a `.docx`, in document order.
pub fn paragraphs(bytes: &[u8]) -> Result<Vec<Para>, String> {
    let docx = read_docx(bytes).map_err(|e| format!("this is not a .docx we can open: {e}"))?;

    let mut out = Vec::new();
    for child in &docx.document.children {
        let DocumentChild::Paragraph(p) = child else {
            // Tables hold whole CVs in some templates, but not in ours: what
            // this harness reads is what DockCV itself writes.
            continue;
        };

        let mut text = String::new();
        for child in &p.children {
            match child {
                ParagraphChild::Run(run) => push(&mut text, run),
                ParagraphChild::Hyperlink(link) => {
                    for nested in &link.children {
                        if let ParagraphChild::Run(run) = nested {
                            push(&mut text, run);
                        }
                    }
                }
                ParagraphChild::Insert(insert) => {
                    for nested in &insert.children {
                        if let InsertChild::Run(run) = nested {
                            push(&mut text, run);
                        }
                    }
                }
                _ => {}
            }
        }

        out.push(Para {
            text: text.split_whitespace().collect::<Vec<_>>().join(" "),
            style: p.property.style.as_ref().map(|s| s.val.to_lowercase()),
            outline: p.property.outline_lvl.as_ref().map(|o| o.v),
            list: p.property.numbering_property.is_some(),
        });
    }
    Ok(out)
}

/// The paragraphs as a plain reading, one to a line — what an extractor that
/// keeps nothing but the runs recovers.
pub fn flat_text(bytes: &[u8]) -> Result<String, String> {
    Ok(paragraphs(bytes)?
        .into_iter()
        .map(|p| p.text)
        .filter(|t| !t.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n"))
}

fn push(out: &mut String, run: &docx_rs::Run) {
    for child in &run.children {
        if let RunChild::Text(t) = child {
            out.push_str(&t.text);
        }
    }
}
