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
    read_docx, DocumentChild, HyperlinkData, InsertChild, Paragraph, ParagraphChild, Run, RunChild,
    Table, TableCellContent, TableChild, TableRowChild,
};
use std::collections::HashMap;
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
        // An entry that will not open is not one to wave through. Skipping it
        // left its bytes out of the running total, which is exactly the archive
        // a size guard exists to catch — a malformed member is the cheapest way
        // to hide behind one.
        let entry = zip
            .by_index(i)
            .map_err(|e| format!("DOCX archive entry {i} could not be read: {e}"))?;
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

    // `r:id` → the URL behind it. A hyperlink's target lives in the document's
    // relationships, not on the element, so without this map the only thing a
    // link can contribute is whatever text it happened to be showing.
    let links: HashMap<&str, &str> = docx
        .hyperlinks
        .iter()
        .chain(docx.document_rels.hyperlinks.iter())
        .map(|(rid, target, _mode)| (rid.as_str(), target.as_str()))
        .collect();

    let mut lines = Vec::new();
    for element in &docx.document.children {
        match element {
            DocumentChild::Paragraph(p) => push_paragraph(&mut lines, p, &links),
            DocumentChild::Table(t) => push_table(&mut lines, t, &links, 0),
            _ => {}
        }
    }

    Ok(classify_lines("DOCX", join_split_entry_headers(lines)))
}

/// How deep a table may nest before this stops following it.
///
/// A CV laid out in a table often puts another inside a cell for the skills
/// grid, and two or three levels is ordinary. A hundred is a malformed or
/// hostile file, and recursion over one would take the stack with it — so this
/// is a guard, not a judgement about layout.
const MAX_TABLE_DEPTH: usize = 8;

/// Tables are content, not decoration. Many templates lay the entire CV out in
/// one, and reading cells in row order gives back the document as it reads on
/// screen.
///
/// Nested tables were skipped — `TableCellContent::Table` fell into the same
/// catch-all that lost hyperlinks. A skills grid inside a layout table is the
/// common case, so the recursion is the point rather than an edge.
fn push_table(out: &mut Vec<LogicalLine>, table: &Table, links: &HashMap<&str, &str>, depth: usize) {
    if depth >= MAX_TABLE_DEPTH {
        return;
    }
    for TableChild::TableRow(row) in &table.rows {
        for TableRowChild::TableCell(cell) in &row.cells {
            for content in &cell.children {
                match content {
                    TableCellContent::Paragraph(p) => push_paragraph(out, p, links),
                    TableCellContent::Table(nested) => {
                        push_table(out, nested, links, depth + 1)
                    }
                    _ => {}
                }
            }
        }
    }
}

/// One paragraph, as the **lines the author wrote** rather than as one string.
///
/// A paragraph is not always one line. `<w:br/>` inside a run is how a template
/// puts an employer under a job title without starting a new paragraph, and how
/// a multi-line address is written. Those breaks used to fall into a catch-all
/// arm and vanish *without a separator*, so two authored lines came out welded
/// together: `Senior EngineerAcme Corp`.
///
/// The exception is a bullet. There the author pressed shift-enter to wrap one
/// item, not to start a second, so its segments rejoin with a space — the same
/// treatment a tab already gets, and for the same reason.
fn push_paragraph(out: &mut Vec<LogicalLine>, p: &Paragraph, links: &HashMap<&str, &str>) {
    let mut segments: Vec<String> = vec![String::new()];
    for child in &p.children {
        match child {
            ParagraphChild::Run(run) => push_run(&mut segments, run, None),
            // A link's text is inside it, and its *target* is not: the element
            // carries an `r:id` into the document's relationships. Both matter —
            // a CV's LinkedIn and GitHub are hyperlinks in every template, and
            // reading only the runs threw the addresses away.
            ParagraphChild::Hyperlink(link) => {
                let target = match &link.link {
                    HyperlinkData::External { rid, .. } => links.get(rid.as_str()).copied(),
                    HyperlinkData::Anchor { .. } => None,
                };
                for nested in &link.children {
                    if let ParagraphChild::Run(run) = nested {
                        push_run(&mut segments, run, target);
                    }
                }
            }
            // A tracked insertion is text the author added and has not yet
            // accepted. It is on the page; it belongs in the import.
            ParagraphChild::Insert(insert) => {
                for nested in &insert.children {
                    if let InsertChild::Run(run) = nested {
                        push_run(&mut segments, run, None);
                    }
                }
            }
            _ => {}
        }
    }

    let style = p
        .property
        .style
        .as_ref()
        .map(|s| s.val.to_lowercase())
        .unwrap_or_default();
    let numbered = p.property.numbering_property.is_some();
    let is_bullet = numbered || style.contains("listbullet") || style.contains("listparagraph");

    let lines: Vec<String> = if is_bullet {
        vec![segments.join(" ")]
    } else {
        segments
    };

    for line in lines {
        let text = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            continue;
        }
        let kind = kind_of(&style, numbered, &text, out.is_empty());
        out.push(LogicalLine::new(
            if kind == LineKind::Bullet {
                without_bullet(&text)
            } else {
                &text
            },
            kind,
        ));
    }
}

/// Append one run's text to the segment being built, starting a new segment at
/// every line break.
///
/// `target` is the URL a surrounding hyperlink points at. It is appended once,
/// and only when the link's own text is not already the address — `linkedin.com/in/x`
/// shown as itself needs no help, while a link reading `LinkedIn` loses
/// everything without it.
fn push_run(segments: &mut Vec<String>, run: &Run, target: Option<&str>) {
    let before = segments.len();
    let start = segments.last().map(|s| s.len()).unwrap_or(0);

    for run_child in &run.children {
        match run_child {
            RunChild::Text(t) => segments.last_mut().expect("never empty").push_str(&t.text),
            // A tab inside a run separates two fields on one line —
            // `Title\tDates`. Kept as a space so the split survives.
            RunChild::Tab(_) => segments.last_mut().expect("never empty").push(' '),
            // The line the author actually ended.
            RunChild::Break(_) | RunChild::CarriageReturn(_) => segments.push(String::new()),
            _ => {}
        }
    }

    let Some(target) = target.map(str::trim).filter(|t| !t.is_empty()) else {
        return;
    };
    // Compare against what this run contributed, not the whole paragraph: a
    // contact line holds several links and each one answers for itself.
    let contributed: String = if segments.len() == before {
        segments.last().map(|s| s[start..].to_string()).unwrap_or_default()
    } else {
        segments[before - 1..].join(" ")
    };
    let bare = target.trim_start_matches("mailto:");
    if !contributed.contains(bare) {
        let last = segments.last_mut().expect("never empty");
        if !last.is_empty() {
            last.push(' ');
        }
        last.push_str(bare);
    }
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

    use docx_rs::{BreakType, Hyperlink, Run as DocxRun};

    /// Run one paragraph through the reader half, with a rels table behind it.
    fn read(paragraph: Paragraph, rels: &[(&str, &str)]) -> Vec<LogicalLine> {
        let links: HashMap<&str, &str> = rels.iter().copied().collect();
        let mut out = Vec::new();
        push_paragraph(&mut out, &paragraph, &links);
        out
    }

    fn texts(lines: &[LogicalLine]) -> Vec<&str> {
        lines.iter().map(|l| l.text.as_str()).collect()
    }

    /// I-04. A `<w:br/>` is where the author ended a line. It used to fall into
    /// a catch-all arm and disappear *without a separator*, welding two lines
    /// into one: `Senior EngineerAcme Corp`.
    #[test]
    fn a_line_break_inside_a_run_ends_the_line() {
        let paragraph = Paragraph::new().add_run(
            DocxRun::new()
                .add_text("Senior Engineer")
                .add_break(BreakType::TextWrapping)
                .add_text("Acme Corp"),
        );
        assert_eq!(
            texts(&read(paragraph, &[])),
            vec!["Senior Engineer", "Acme Corp"]
        );
    }

    /// …except in a bullet, where shift-enter wraps one item rather than
    /// starting a second. Same reasoning the tab arm already carries.
    #[test]
    fn a_line_break_inside_a_bullet_is_a_space() {
        let paragraph = Paragraph::new().style("ListBullet").add_run(
            DocxRun::new()
                .add_text("Cut p99 latency in half")
                .add_break(BreakType::TextWrapping)
                .add_text("across the billing path"),
        );
        let lines = read(paragraph, &[]);
        assert_eq!(
            texts(&lines),
            vec!["Cut p99 latency in half across the billing path"]
        );
        assert_eq!(lines[0].kind, LineKind::Bullet);
    }

    /// I-03. A hyperlink's text lives inside it and its target does not — the
    /// element carries an `r:id` into the document's relationships. Reading only
    /// `ParagraphChild::Run` threw away every address on the contact line.
    #[test]
    fn a_hyperlink_contributes_both_its_words_and_its_address() {
        let link = Hyperlink {
            link: HyperlinkData::External {
                rid: "rId7".to_string(),
                path: String::new(),
            },
            history: None,
            children: vec![ParagraphChild::Run(Box::new(
                DocxRun::new().add_text("LinkedIn"),
            ))],
        };
        let lines = read(
            Paragraph::new().add_hyperlink(link),
            &[("rId7", "https://linkedin.com/in/sofiia")],
        );
        assert_eq!(texts(&lines), vec!["LinkedIn https://linkedin.com/in/sofiia"]);
    }

    /// A link that already shows its own address gains nothing from having it
    /// repeated — the common shape on a CV's contact line.
    #[test]
    fn a_link_that_shows_its_address_is_not_made_to_say_it_twice() {
        let link = Hyperlink {
            link: HyperlinkData::External {
                rid: "rId3".to_string(),
                path: String::new(),
            },
            history: None,
            children: vec![ParagraphChild::Run(Box::new(
                DocxRun::new().add_text("s@example.com"),
            ))],
        };
        let lines = read(
            Paragraph::new().add_hyperlink(link),
            &[("rId3", "mailto:s@example.com")],
        );
        assert_eq!(texts(&lines), vec!["s@example.com"]);
    }

    /// A link whose target is not in the rels table still gives up its words
    /// rather than the whole paragraph.
    #[test]
    fn an_unresolvable_link_still_contributes_its_text() {
        let link = Hyperlink {
            link: HyperlinkData::External {
                rid: "rId99".to_string(),
                path: String::new(),
            },
            history: None,
            children: vec![ParagraphChild::Run(Box::new(
                DocxRun::new().add_text("Portfolio"),
            ))],
        };
        assert_eq!(texts(&read(Paragraph::new().add_hyperlink(link), &[])), vec!["Portfolio"]);
    }

    /// I-11. A skills grid inside a layout table is the ordinary shape, and
    /// `TableCellContent::Table` fell into the same catch-all that lost links.
    #[test]
    fn a_table_inside_a_cell_is_read_rather_than_skipped() {
        use docx_rs::{Table as DocxTable, TableCell, TableRow};

        let inner = DocxTable::new(vec![TableRow::new(vec![
            TableCell::new().add_paragraph(Paragraph::new().add_run(DocxRun::new().add_text("Rust"))),
            TableCell::new().add_paragraph(Paragraph::new().add_run(DocxRun::new().add_text("Kafka"))),
        ])]);
        let outer = DocxTable::new(vec![TableRow::new(vec![TableCell::new()
            .add_paragraph(Paragraph::new().add_run(DocxRun::new().add_text("Skills")))
            .add_table(inner)])]);

        let mut out = Vec::new();
        push_table(&mut out, &outer, &HashMap::new(), 0);
        assert_eq!(texts(&out), vec!["Skills", "Rust", "Kafka"]);
    }

    /// The guard, not the layout: a file that nests past the cap stops rather
    /// than taking the stack with it.
    #[test]
    fn nesting_stops_at_the_cap() {
        use docx_rs::{Table as DocxTable, TableCell, TableRow};

        let mut table = DocxTable::new(vec![TableRow::new(vec![TableCell::new()
            .add_paragraph(Paragraph::new().add_run(DocxRun::new().add_text("deepest")))])]);
        for level in 0..MAX_TABLE_DEPTH + 3 {
            table = DocxTable::new(vec![TableRow::new(vec![TableCell::new()
                .add_paragraph(
                    Paragraph::new().add_run(DocxRun::new().add_text(format!("level{level}"))),
                )
                .add_table(table)])]);
        }

        let mut out = Vec::new();
        push_table(&mut out, &table, &HashMap::new(), 0);
        assert_eq!(out.len(), MAX_TABLE_DEPTH, "one line per level reached");
        assert!(
            !texts(&out).contains(&"deepest"),
            "the cap has to actually stop: {:?}",
            texts(&out)
        );
    }

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
