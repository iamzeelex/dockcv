//! DOCX import: read the **markup**, not the flattened text.
//!
//! A .docx says what its lines are. A résumé template marks the person's name
//! `Title`, its section headings `Heading1`, the first line of a job `Heading2`,
//! and its bullets `ListBullet` or a numbering property. The previous engine
//! concatenated every run into one string and handed it to the text classifier,
//! which then tried to recover from wording and punctuation what the file had
//! stated outright — and dropped the rest: **tables were skipped entirely**, so
//! a template whose whole CV is one table (a common shape) imported as nothing.
//!
//! So the split is: this engine produces
//! [`LogicalLine`](crate::import::layout::LogicalLine)s from the markup, a PDF
//! produces them by measuring the page, and
//! [`classify_lines`](crate::import::classifier::classify_lines) — the half that
//! knows what a résumé is — is shared. Only the *evidence* differs between
//! formats, and here it is exact.

use docx_rs::{
    read_docx, DocumentChild, Paragraph, ParagraphChild, RunChild, TableCellContent, TableChild,
    TableRowChild,
};
use std::path::Path;

use crate::import::classifier::{classify_lines, is_only_dates, names_a_section};
use crate::import::layout::{without_bullet, LineKind, LogicalLine};
use crate::import::model::ImportedDoc;

const MAX_DOCX_SIZE: u64 = 50 * 1024 * 1024; // 50 MB
const MAX_DOCX_UNCOMPRESSED_TOTAL: u64 = 50 * 1024 * 1024; // 50 MB total uncompressed
const MAX_DOCX_ENTRY_SIZE: u64 = 20 * 1024 * 1024; // 20 MB per inner entry

fn validate_docx_container(buf: &[u8]) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(buf))
        .map_err(|e| format!("Invalid DOCX container: {e}"))?;

    let mut total_size: u64 = 0;
    for i in 0..zip.len() {
        if let Ok(entry) = zip.by_index(i) {
            let size = entry.size();
            if size > MAX_DOCX_ENTRY_SIZE {
                return Err(format!(
                    "DOCX archive entry '{}' size ({size} bytes) exceeds limit ({MAX_DOCX_ENTRY_SIZE} bytes)",
                    entry.name()
                ));
            }
            total_size += size;
            if total_size > MAX_DOCX_UNCOMPRESSED_TOTAL {
                return Err(format!(
                    "DOCX total uncompressed data size exceeds maximum allowed limit of {} MB",
                    MAX_DOCX_UNCOMPRESSED_TOTAL / (1024 * 1024)
                ));
            }
        }
    }
    Ok(())
}

pub fn import_docx(path: &Path) -> Result<ImportedDoc, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("Could not stat DOCX file: {e}"))?;
    if metadata.len() > MAX_DOCX_SIZE {
        return Err(format!(
            "DOCX file exceeds maximum allowed size of {} MB",
            MAX_DOCX_SIZE / (1024 * 1024)
        ));
    }
    let buf = std::fs::read(path).map_err(|e| format!("Could not open DOCX file: {e}"))?;
    validate_docx_container(&buf)?;
    let docx = read_docx(&buf).map_err(|e| format!("Failed to parse DOCX structure: {e}"))?;

    let mut lines = Vec::new();
    for element in &docx.document.children {
        match element {
            DocumentChild::Paragraph(p) => push_paragraph(&mut lines, p),
            // Tables are content, not decoration. Many templates lay the entire
            // CV out in one, and reading cells in row order gives back the
            // document as it reads on screen.
            DocumentChild::Table(t) => {
                for TableChild::TableRow(row) in &t.rows {
                    for TableRowChild::TableCell(cell) in &row.cells {
                        for content in &cell.children {
                            if let TableCellContent::Paragraph(p) = content {
                                push_paragraph(&mut lines, p);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok(classify_lines("DOCX", join_split_entry_headers(lines)))
}

fn push_paragraph(out: &mut Vec<LogicalLine>, p: &Paragraph) {
    let mut text = String::new();
    for child in &p.children {
        if let ParagraphChild::Run(run) = child {
            for run_child in &run.children {
                match run_child {
                    RunChild::Text(t) => text.push_str(&t.text),
                    // A tab inside a run separates two fields on one line —
                    // `Title\tDates`. Kept as a space so the split survives.
                    RunChild::Tab(_) => text.push(' '),
                    _ => {}
                }
            }
        }
    }
    let text = text.trim();
    if text.is_empty() {
        return;
    }

    let style = p
        .property
        .style
        .as_ref()
        .map(|s| s.val.to_lowercase())
        .unwrap_or_default();
    let numbered = p.property.numbering_property.is_some();

    let kind = kind_of(&style, numbered, text, out.is_empty());

    out.push(LogicalLine::new(
        if kind == LineKind::Bullet {
            without_bullet(text)
        } else {
            text
        },
        kind,
    ));
}

/// What a paragraph is, from its style and its words.
///
/// What the line *says* outranks the style it was given, in both directions. A
/// style is a typographic choice and templates make it inconsistently: one sets
/// `Experience` as bold body text, another marks its sub-sections `Heading2`,
/// a third makes the person's name a `Heading1`. Trusting the style alone cost
/// a whole document each time — a missed heading does not lose one line, it
/// moves the section boundary and swallows everything under it.
fn kind_of(style: &str, numbered: bool, text: &str, first_line: bool) -> LineKind {
    if numbered || style.contains("listbullet") || style.contains("listparagraph") {
        return LineKind::Bullet;
    }
    if names_a_section(&text.to_lowercase()) {
        return LineKind::Heading;
    }
    if style == "heading1" {
        // A top-level heading is a section even when the taxonomy has never
        // heard of it — `Leadership & Activities` is somebody's section, and it
        // keeps its own name (D-9).
        //
        // The exception is the document's first line: one template sets the
        // person's *name* as `Heading1`, and reading that as a section filed
        // their address and their whole education under a section called by
        // their own name. As text it reaches the contact block, where every
        // other format's title is read — one path, shared.
        return if first_line {
            LineKind::Text
        } else {
            LineKind::Heading
        };
    }
    if style.starts_with("heading") {
        // A deeper heading opens an entry inside the section above it.
        return LineKind::EntryHeader;
    }
    LineKind::Text
}

/// Put an entry's dates back on its title.
///
/// Templates routinely give the dates their own cell or paragraph — sometimes
/// styled `Dates`, sometimes `Heading2` with the title in a plain run beside it.
/// Split that way, neither half is a usable entry header: the title carries no
/// date to place it, and the dates carry no title to name it. Joining them is
/// this engine's job, not the shared classifier's — how a template scatters an
/// entry across cells is a fact about DOCX.
fn join_split_entry_headers(lines: Vec<LogicalLine>) -> Vec<LogicalLine> {
    let mut out: Vec<LogicalLine> = Vec::with_capacity(lines.len());
    let mut pending_dates: Option<String> = None;

    for line in lines {
        if line.kind != LineKind::Heading && is_only_dates(&line.text) {
            pending_dates = Some(line.text.clone());
            continue;
        }
        match pending_dates.take() {
            // The line after a bare date is the entry that date belongs to.
            Some(dates) if line.kind != LineKind::Heading => out.push(LogicalLine::new(
                format!("{} {}", line.text, dates),
                LineKind::EntryHeader,
            )),
            // A heading follows: the dates belonged to the entry above it.
            Some(dates) => {
                if let Some(prev) = out
                    .last_mut()
                    .filter(|p| p.kind == LineKind::EntryHeader)
                {
                    prev.text = format!("{} {}", prev.text, dates);
                }
                out.push(line);
            }
            None => out.push(line),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(input: Vec<(&str, LineKind)>) -> Vec<LogicalLine> {
        input
            .into_iter()
            .map(|(t, k)| LogicalLine::new(t, k))
            .collect()
    }

    /// A degree line that carries its own dates and school between pipes must
    /// come apart into three fields — one template writes every education entry
    /// that way, and read whole they merged into a single unreadable heading.
    #[test]
    fn a_pipe_delimited_degree_line_is_still_one_entry_per_paragraph() {
        let joined = join_split_entry_headers(lines(vec![
            ("Education", LineKind::Heading),
            (
                "B.S. in Business Administration | June 2020 | Bigtown College, Chicago",
                LineKind::EntryHeader,
            ),
            (
                "A.A. in Hospitality Management | June 2018 | Bigtown College, Chicago",
                LineKind::EntryHeader,
            ),
        ]));

        // Two entry headers in, two out: neither absorbs the other, and the
        // date between the pipes is not mistaken for a bare date line.
        assert_eq!(joined.len(), 3, "{joined:#?}");
        assert!(joined.iter().filter(|l| l.kind == LineKind::EntryHeader).count() == 2);
    }

    /// Harvard sets `Experience` as bold body text. Missing it did not lose one
    /// line — it moved the section boundary and swallowed the twenty under it.
    #[test]
    fn a_heading_is_read_from_its_words_when_the_style_does_not_say_so() {
        assert_eq!(kind_of("bodytext", false, "Experience", false), LineKind::Heading);
        assert_eq!(kind_of("", false, "Skills", false), LineKind::Heading);
    }

    /// One template makes the person's name a `Heading1`. Read as a section it
    /// filed their address and their whole education under their own name.
    #[test]
    fn the_first_heading_naming_no_section_is_the_documents_title() {
        assert_eq!(kind_of("heading1", false, "Barry Alan Manilow", true), LineKind::Text);
        // The same style later in the document is a section of its own (D-9).
        assert_eq!(
            kind_of("heading1", false, "Leadership & Activities", false),
            LineKind::Heading
        );
        // And a deeper one opens an entry, not a section.
        assert_eq!(
            kind_of("heading2", false, "Cardiothoracic Surgery Fellow", false),
            LineKind::EntryHeader
        );
    }

    /// The template puts the dates in their own cell, above the job title. Left
    /// apart, the title has nothing to date it and the dates name no job.
    #[test]
    fn dates_in_their_own_cell_rejoin_the_entry_below_them() {
        let joined = join_split_entry_headers(lines(vec![
            ("Work experience", LineKind::Heading),
            ("8.2019 – 12.2021", LineKind::Text),
            ("Cardiothoracic Surgery Fellow", LineKind::EntryHeader),
            ("Led clinical trials.", LineKind::Bullet),
        ]));

        assert_eq!(joined.len(), 3, "{joined:#?}");
        assert_eq!(joined[1].text, "Cardiothoracic Surgery Fellow 8.2019 – 12.2021");
        assert_eq!(joined[1].kind, LineKind::EntryHeader);
    }

    /// A date-only line styled as a heading is a date, not a section.
    #[test]
    fn a_heading_that_is_only_a_date_becomes_part_of_an_entry() {
        let joined = join_split_entry_headers(lines(vec![
            ("June 2020 - Present", LineKind::EntryHeader),
            ("Assistant Manager", LineKind::Text),
        ]));

        assert_eq!(joined.len(), 1, "{joined:#?}");
        assert_eq!(joined[0].text, "Assistant Manager June 2020 - Present");
    }

    /// Nothing to rejoin: a document that keeps dates on the title line must
    /// come through untouched.
    #[test]
    fn an_entry_that_already_carries_its_dates_is_left_alone() {
        let input = lines(vec![
            ("Experience", LineKind::Heading),
            (
                "Restaurant Manager | Contoso Bar and Grill | September 2019 – 2021",
                LineKind::EntryHeader,
            ),
        ]);
        let joined = join_split_entry_headers(input);
        assert_eq!(joined.len(), 2);
        assert!(joined[1].text.starts_with("Restaurant Manager"));
    }
}
