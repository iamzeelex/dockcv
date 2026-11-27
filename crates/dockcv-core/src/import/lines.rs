//! Turning a PDF's text layer into **logical lines**.
//!
//! A PDF has no paragraphs. `pdf_extract` gives back one string per line *box*,
//! so a single bullet arrives as two or three lines and a section's prose
//! arrives as five. Every parser downstream wants the opposite: one string per
//! thing the author wrote.
//!
//! The joining rule is measured, not guessed. A line that was **broken by the
//! text measure** runs to the right margin; a line that ended because the author
//! stopped writing does not. So this module derives the document's own measure
//! from its line lengths and treats "the previous line filled the measure" as
//! the signal that the next one continues it. Nothing here hard-codes a column
//! width, a bullet glyph count, or a page size — a CV typeset at any measure,
//! in any language, produces its own threshold.
//!
//! Two guards keep it honest:
//!
//! * a line the author *ended* (`.`, `!`, `?`) is complete however wide it ran;
//! * an **entry header** — one carrying a date range — is a header, not prose,
//!   and is never continued. Without this, `…Odesa, Ukraine` would swallow the
//!   course list printed under it.

use regex::Regex;

/// What a logical line is, structurally. The section parsers dispatch on this
/// rather than re-testing for bullet glyphs and blank lines themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// A section heading (`WORK EXPERIENCE`). Classified by the caller.
    Heading,
    /// The first line of an entry — a job title, a degree, a project name.
    ///
    /// Only a format that *knows* can emit this. DOCX does: a résumé template
    /// marks it `Heading2`, and the dates often sit in a separate cell, so
    /// there is nothing on the line for a date-range test to find. A PDF never
    /// emits it — the evidence does not exist there — and the classifier falls
    /// back to the date range, which is all it ever had.
    EntryHeader,
    /// A bullet point, with its wrapped remainder already joined in.
    Bullet,
    /// Anything else: an entry header, an entry summary, a course list.
    Text,
}

/// One thing the author wrote, whatever the typesetter did to it.
#[derive(Debug, Clone)]
pub struct LogicalLine {
    pub text: String,
    pub kind: LineKind,
    /// Width of the **last physical line** folded into this one.
    ///
    /// Not `text.len()`: once two fragments are joined the total is wider than
    /// any measure, so a joined line would go on swallowing everything under
    /// it. What decides is whether the fragment that ended last ran to the
    /// margin.
    tail_width: usize,
}

impl LogicalLine {
    /// A line from a format that reports its own structure. `tail_width` is a
    /// measurement of a *typeset* line and means nothing here, so it is zeroed:
    /// nothing built this way is ever a continuation candidate.
    pub fn new(text: impl Into<String>, kind: LineKind) -> Self {
        Self {
            text: text.into(),
            kind,
            tail_width: 0,
        }
    }

    pub fn is_bullet(&self) -> bool {
        self.kind == LineKind::Bullet
    }
}

/// Bullet glyphs seen in exported CVs. `-` and `*` are deliberately absent:
/// they open ranges and footnotes far more often than lists in this position,
/// and a mis-read bullet is worse than a missed one — it changes the shape of
/// the entry rather than the text of one line.
// `∙` is U+2219 BULLET OPERATOR, which is a mathematics character and is what
// LinkedIn's own PDF export marks every bullet with — so every description in
// the most common CV file in the world arrived as unmarked prose. `●` and `▸`
// come from the Word gallery. The en dash is deliberately absent: it separates
// the two ends of a date range.
const BULLET_GLYPHS: [char; 8] = ['•', '▪', '‣', '◦', '·', '∙', '●', '▸'];

/// The ASCII markers a list uses when it has no glyph to spare — which is what
/// a plain-text CV, DockCV's own included, is written with.
///
/// Only ever read **indented**. That is the whole distinction: `- 2019` at the
/// left margin opens a date range, and `  - Led the migration` two columns in
/// is a list item, because nothing else in a CV is indented at all.
const ASCII_BULLETS: [char; 3] = ['-', '*', '+'];

/// A line of nothing but rule characters: the `-------` a plain-text CV puts
/// under a heading, and Markdown's thematic break.
///
/// It is typography, and reading it as content is how `PROFILE / -------`
/// arrived as a person whose summary was seven dashes.
fn is_rule(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.chars().count() >= 3
        && trimmed
            .chars()
            .all(|c| matches!(c, '-' | '=' | '_' | '─' | '━' | '–' | '—' | '*' | '·'))
}

/// Does the line open with its own label — `Languages: Rust, Go, Python`?
///
/// A wrapped line never introduces one: the label is written by the author at
/// the head of a row, so finding one is proof the row started here.
fn opens_labelled_row(line: &str) -> bool {
    let trimmed = line.trim_start();
    let Some((label, rest)) = trimmed.split_once(':') else {
        return false;
    };
    !rest.trim().is_empty()
        && label.len() <= 32
        && (1..=3).contains(&label.split_whitespace().count())
        && label.starts_with(char::is_uppercase)
        && !label.contains(['.', ',', ';', '(', ')'])
}

/// A line holding two list items, split where the second one starts.
///
/// Only inside what is already a bullet, and only on a glyph that opens lists
/// and nothing else — a `-` or a `*` mid-sentence is punctuation.
fn split_at_inner_bullet(line: &str) -> Option<(String, String)> {
    let body = line.trim_start();
    if !starts_with_bullet(body) {
        return None;
    }
    let rest = without_bullet(body);
    let at = rest.find(BULLET_GLYPHS)?;
    let head = rest[..at].trim();
    let tail = without_bullet(rest[at..].trim());
    (head.len() > 1 && tail.len() > 1).then(|| (head.to_string(), tail.to_string()))
}

/// Undo the letter spacing a typesetter puts under a section heading.
///
/// A PDF's text layer records what was drawn, and a heading set with tracking
/// is drawn one glyph at a time: `WO R K   E X P E R I E N C E`. Nothing
/// downstream can recognise that as a heading — the whole section boundary is
/// lost with it, which is why a CV exported from DockCV and read back as PDF
/// arrived as one section containing the entire document.
///
/// The evidence is the shape of the run rather than any single token: most of
/// the pieces are one character long, which no ordinary sentence is. A wider
/// gap is a word break and stays one.
fn unspace_tracked(line: &str) -> Option<String> {
    let tokens: Vec<&str> = line.split(' ').filter(|t| !t.is_empty()).collect();
    let singles = tokens.iter().filter(|t| t.chars().count() == 1).count();
    if tokens.len() < 4 || singles * 3 < tokens.len() * 2 {
        return None;
    }

    let mut out = String::with_capacity(line.len());
    for word in line.split("  ").filter(|w| !w.trim().is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(word.split(' ').filter(|p| !p.is_empty()));
    }
    (!out.is_empty()).then_some(out)
}

/// A line that is one address and nothing else.
///
/// Brackets around it mean it is not: `(doi.org/10.1002/andp…)` on its own line
/// is the tail of a title the measure broke, and the parentheses are the proof
/// — they were opened on the line above.
pub fn is_lone_address(line: &str) -> bool {
    let token = line.trim();
    !token.contains(char::is_whitespace)
        && !token.contains('@')
        && !token.starts_with(['(', '[', '<'])
        && !token.ends_with([')', ']', '>'])
        && crate::resume::links::href(token).is_some()
}

/// How far a line is indented, and what is left after the indent.
fn indent_of(line: &str) -> (usize, &str) {
    let body = line.trim_start();
    (line.len() - body.len(), body)
}

/// An indented ASCII list marker followed by a space, and the text after it.
fn ascii_bullet(indent: usize, body: &str) -> Option<&str> {
    if indent == 0 {
        return None;
    }
    let mut chars = body.chars();
    let marker = chars.next()?;
    if !ASCII_BULLETS.contains(&marker) || chars.next() != Some(' ') {
        return None;
    }
    Some(body[marker.len_utf8()..].trim_start())
}

/// Strip a bullet glyph and the space after it.
pub fn without_bullet(line: &str) -> &str {
    line.trim_start_matches(BULLET_GLYPHS).trim()
}

pub fn starts_with_bullet(line: &str) -> bool {
    line.starts_with(BULLET_GLYPHS)
}

/// The width the document was typeset to, in characters.
///
/// Taken from the 90th percentile of line length rather than the maximum: one
/// runaway line (a URL, a footer run) would otherwise raise the bar above every
/// real line and switch continuation-joining off entirely. The 0.85 slack
/// absorbs the last word that did not fit — a full line ends anywhere in the
/// final word, not exactly at the margin.
fn measure_of(lines: &[&str]) -> usize {
    let mut lengths: Vec<usize> = lines.iter().map(|l| l.chars().count()).collect();
    if lengths.is_empty() {
        return usize::MAX;
    }
    lengths.sort_unstable();
    let p90 = lengths[lengths.len() * 9 / 10];
    (p90 as f32 * 0.85) as usize
}

/// Does this line begin mid-sentence?
///
/// The measure is counted in characters, and characters are a poor proxy for
/// typeset width: `Alberta Advanced Education and Technology Achievement
/// scholarship awarded` broke at 76 characters where the same page ran another
/// bullet to 91, because capitals are wide. So width alone missed real wraps.
///
/// A line that opens with a lowercase letter is the far stronger signal, and an
/// independent one: headings, entry headers, names and list items all begin
/// with a capital. Starting lowercase means the sentence began further up.
/// A continuation is **prose**, so it carries a space and no address. Without
/// that guard the rule swallowed a contact block: `albert@example.com` begins with a
/// lowercase letter and was folded into the line naming the city above it.
fn starts_mid_sentence(line: &str) -> bool {
    line.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() && c.is_lowercase())
        && line.contains(' ')
        && !line.contains('@')
        && !line.contains("://")
}

/// A line the author finished. Trailing whitespace is common in extracted text
/// and says nothing either way.
fn ends_a_sentence(line: &str) -> bool {
    matches!(line.trim_end().chars().last(), Some('.' | '!' | '?' | ':'))
}

/// Footer noise that carries no content: the rule-of-pipes some exporters emit
/// at the end of the text layer, and any run of separator glyphs left over from
/// a table.
/// Typesetting whitespace, normalised to the ordinary kind.
///
/// Exporters set a CV's keyword runs with non-breaking spaces so a group never
/// wraps mid-item. Downstream that is invisible and lethal: `split("  ")` never
/// matches `\u{a0} \u{a0}`, so an eight-item skill group arrived as one
/// eighty-character "skill". Content-bearing whitespace is still whitespace.
fn normalize_spaces(line: &str) -> String {
    line.chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect()
}

/// Glyphs a page draws *about* the text rather than as part of it.
///
/// `↗` is DockCV's own mark saying an entry carries a link. It means something
/// to a reader and nothing to a parser, and a PDF's text layer records it like
/// any other character — so a university came back named `Aarhus Universitet ↗`.
const PAGE_ORNAMENTS: [char; 3] = ['↗', '↪', '⧉'];

fn strip_footer_noise(line: &str) -> String {
    // A run of three or more pipes is decoration, never punctuation — a single
    // `|` is the separator an exporter puts before a location and must stay.
    let mut out = String::with_capacity(line.len());
    let mut pipe_run = 0usize;
    for ch in line.chars() {
        if PAGE_ORNAMENTS.contains(&ch) {
            continue;
        }
        if ch == '|' {
            pipe_run += 1;
            continue;
        }
        if pipe_run > 0 && pipe_run < 3 {
            out.push_str(&"|".repeat(pipe_run));
        }
        pipe_run = 0;
        out.push(ch);
    }
    if (1..3).contains(&pipe_run) {
        out.push_str(&"|".repeat(pipe_run));
    }
    out.trim().to_string()
}

/// Build the logical lines of a document.
///
/// `is_heading` and `has_date_range` are passed in rather than imported so this
/// module stays free of the taxonomy: layout is about how text was set on the
/// page, not about what a résumé means.
pub fn logical_lines(
    raw: &str,
    is_heading: impl Fn(&str) -> bool,
    has_date_range: impl Fn(&str) -> bool,
) -> Vec<LogicalLine> {
    // Blank lines are separators, so they are recorded and then dropped.
    // Indentation is evidence and is carried alongside the text: it is what
    // tells a list item from a date range, and a wrapped item from the next one.
    let mut source: Vec<(usize, String)> = Vec::new();
    for raw_line in raw.lines() {
        let normalized = normalize_spaces(raw_line);
        let (indent, _) = indent_of(&normalized);
        let normalized = unspace_tracked(&normalized).unwrap_or(normalized);
        let line = strip_footer_noise(&normalized);
        // A rule under a heading is how the heading was drawn, not something
        // the author wrote.
        if line.is_empty() || is_rule(&line) {
            continue;
        }
        source.push((indent, line));
    }

    let widths: Vec<&str> = source.iter().map(|(_, l)| l.as_str()).collect();
    let measure = measure_of(&widths);

    let mut out: Vec<LogicalLine> = Vec::new();
    // The indent of the line that opened the item still being read, so its
    // wrapped remainder — indented further — is joined back onto it.
    let mut open_indent: Option<usize> = None;
    for (indent, line) in source {
        let heading = is_heading(&line);
        let glyph_bullet = starts_with_bullet(&line);
        let ascii = ascii_bullet(indent, &line);
        let bullet = glyph_bullet || ascii.is_some();

        let hangs_under_open =
            !heading && !bullet && open_indent.is_some_and(|opened| indent > opened);

        // Does this continue the line above?
        let continues = !heading
            && !bullet
            && out.last().is_some_and(|prev| {
                if prev.kind == LineKind::Heading {
                    return false;
                }
                // Indentation under an open item is the author saying this is
                // the same item, and outranks everything guessed from the text
                // — including the full stop that ends `…per-branch stacks.`
                // one line before the sentence that finishes the thought.
                if hangs_under_open {
                    return true;
                }
                // An address alone on its own line was put there; it is not the
                // tail of the line above. Joining it glued a personal site onto
                // the contact line and produced a city called
                // `Copenhagen, Denmark vestergaard.dev`.
                if is_lone_address(&line) {
                    return false;
                }
                // A row that names itself is a new row. `Languages: Rust, Go`
                // under a full-width `Platform: …` line was folded into it by
                // the width rule alone, and two skill groups became one.
                if opens_labelled_row(&line) {
                    return false;
                }
                !has_date_range(&prev.text)
                    && !ends_a_sentence(&prev.text)
                    && (prev.tail_width >= measure || starts_mid_sentence(&line))
            });

        // A bullet glyph in the middle of a line is where the *next* item
        // began: a PDF's text layer runs two list items together when they sit
        // on one typeset line, and joined they read as one achievement the CV
        // never claimed.
        if let Some((head, tail)) = split_at_inner_bullet(&line) {
            out.push(LogicalLine {
                tail_width: head.chars().count(),
                text: head,
                kind: LineKind::Bullet,
            });
            open_indent = Some(indent);
            out.push(LogicalLine {
                tail_width: tail.chars().count(),
                text: tail,
                kind: LineKind::Bullet,
            });
            continue;
        }

        if continues {
            let prev = out.last_mut().expect("checked by is_some_and above");
            if !prev.text.ends_with(' ') {
                prev.text.push(' ');
            }
            prev.tail_width = line.chars().count();
            prev.text.push_str(line.trim_start());
            continue;
        }

        open_indent = bullet.then_some(indent);
        let tail_width = line.chars().count();
        out.push(LogicalLine {
            tail_width,
            text: match ascii {
                Some(rest) => rest.to_string(),
                None if glyph_bullet => without_bullet(&line).to_string(),
                None => line,
            },
            kind: if heading {
                LineKind::Heading
            } else if bullet {
                LineKind::Bullet
            } else {
                LineKind::Text
            },
        });
    }

    for line in &mut out {
        line.text = line.text.trim().to_string();
    }
    out.retain(|l| !l.text.is_empty());
    out
}

/// The parts of an entry's header line, whichever order the exporter wrote them.
///
/// `Software Developer, GE Vernova Aug 2024 – Dec 2025  |  Barcelona, Spain`
/// and `Universitat Autònoma de Barcelona (UAB) 2025 – 2026  |  Barcelona,
/// Spain` are the same shape with different fields filled — one struct reads
/// both, so Work and Education do not each grow their own splitter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryHeader {
    /// What comes before the organisation: a job title, a degree.
    pub lead: String,
    /// The organisation: employer, university, issuer.
    pub org: String,
    pub start: String,
    pub end: String,
    pub location: String,
}

/// Month names, for telling a date field from a text field. Three letters is
/// enough to match both `Sep` and `September`.
const MONTH_STEMS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// Is this pipe-delimited field nothing but a date?
///
/// `B.S. in Business Administration | June 2020 | Bigtown College` puts the
/// date in the middle field, so counting fields left-to-right filed the date as
/// the institution and dropped the college.
fn is_date_field(part: &str) -> bool {
    let tokens: Vec<&str> = part
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    !tokens.is_empty()
        && tokens.iter().all(|t| {
            let lower = t.to_lowercase();
            MONTH_STEMS.iter().any(|m| lower.starts_with(m))
                || matches!(lower.as_str(), "present" | "current" | "ongoing" | "to")
                || (lower.len() <= 4 && lower.chars().all(|c| c.is_ascii_digit() || c == 'x'))
        })
}

/// Split at the last `", "` that is not inside `()` or `[]`.
fn split_outside_brackets(line: &str) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    let mut cut = None;
    let bytes: Vec<(usize, char)> = line.char_indices().collect();
    for (i, (at, ch)) in bytes.iter().enumerate() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            ',' if depth == 0 => {
                if matches!(bytes.get(i + 1), Some((_, ' '))) {
                    cut = Some(*at);
                }
            }
            _ => {}
        }
    }
    cut.map(|at| (&line[..at], line[at + 1..].trim_start()))
}

impl EntryHeader {
    /// The entry-naming text as the line had it, when the split into lead and
    /// organisation is not wanted — filling an entry a previous line already
    /// opened, where the whole line names the employer or the school.
    pub fn whole(&self) -> String {
        match (self.lead.is_empty(), self.org.is_empty()) {
            (false, false) => format!("{}, {}", self.lead, self.org),
            (true, _) => self.org.clone(),
            (_, true) => self.lead.clone(),
        }
    }

    /// Read a header line, most reliable signal first.
    ///
    /// The order matters: the date range and the `|`-delimited location are
    /// unambiguous, so they are taken out first and whatever remains is the
    /// title-and-organisation text. Splitting that remainder on a comma *first*
    /// would break `Data & MLOps Engineer, Contract (DataArt, Inmost,
    /// Virtuace)` at the wrong comma.
    pub fn parse(line: &str, date_range: &Regex) -> Self {
        let mut header = Self::default();
        let mut rest = line.to_string();

        if let Some(caps) = date_range.captures(&rest) {
            let whole = caps.get(0).expect("group 0 always matches");
            header.start = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
            header.end = caps.get(2).map_or("", |m| m.as_str()).trim().to_string();
            let (before, after) = (
                rest[..whole.start()].to_string(),
                rest[whole.end()..].to_string(),
            );
            // Which side of the dates the entry is on depends on which side
            // has anything on it.
            //
            // Dates last is the ordinary shape and what this used to assume:
            // `Senior Engineer, Acme 2019 – 2022 | Dublin`, where everything
            // after them is location-ish. But a CV laid out as a table — most
            // of the Word gallery, and every template with a column of years
            // down the left — extracts as `2021-02 – Present Principal
            // Engineer, Atlantic Systems`, and reading *that* tail as a
            // location filed the job title and the employer under where the
            // person worked, leaving the entry itself nameless. Two jobs with
            // dates and nothing else is what a Word-template CV imported as.
            if before.trim().is_empty() && !after.trim().is_empty() {
                rest = after;
            } else {
                let tail = after.trim().trim_start_matches('|').trim();
                if !tail.is_empty() {
                    header.location = tail.to_string();
                }
                rest = before;
            }
        }

        // `|` is a field separator, and what the fields *are* depends on how
        // many there are. Reading the first one as "location follows" — the
        // only shape the PDF path ever produced — turned
        // `Restaurant Manager | Contoso Bar and Grill | Sept 2019 – 2021` into
        // a job whose location was its employer.
        let mut parts: Vec<String> = Vec::new();
        for part in rest.split('|').map(str::trim).filter(|p| !p.is_empty()) {
            if is_date_field(part) {
                if header.start.is_empty() {
                    header.start = part.to_string();
                }
                continue;
            }
            parts.push(part.to_string());
        }
        if parts.len() >= 2 {
            header.lead = parts[0].clone();
            header.org = parts[1].clone();
            if let Some(third) = parts.get(2) {
                if header.location.is_empty() {
                    header.location = third.clone();
                }
            }
            return header;
        }
        rest = parts.into_iter().next().unwrap_or_default();

        let rest = rest.trim().trim_end_matches(',').trim();

        // `Role, Employer` — split at the last comma **outside brackets**.
        //
        // Neither end works alone: the first comma of `BSc, Applied Mathematics
        // and Computing, Odesa I.I.Mechnikov National University` cuts the
        // degree in half, and the last comma of `Contract (DataArt, Inmost,
        // Virtuace)` cuts the employer's name in half. A bracketed list is one
        // token, and once it is skipped the last comma is the right one both
        // times.
        match split_outside_brackets(rest) {
            Some((lead, org)) if !org.trim().is_empty() => {
                header.lead = lead.trim().to_string();
                header.org = org.trim().to_string();
            }
            // One part, no comma: the line names the entry and nothing else.
            // It belongs in `lead` — which section field that becomes is the
            // section's decision, not this splitter's.
            _ => header.lead = rest.to_string(),
        }
        header
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_headings(_: &str) -> bool {
        false
    }

    fn dates() -> Regex {
        Regex::new(r"(\b(?:19|20)\d{2}\b)\s*[–—-]\s*(\b(?:19|20)\d{2}\b|Present)").unwrap()
    }

    fn has_dates(line: &str) -> bool {
        dates().is_match(line)
    }

    /// The defect this replaces: the continuation was treated as a new item, so
    /// every wrapped bullet lost its tail and the tail was filed as the entry's
    /// summary. Both halves are visible in the rendered CV.
    #[test]
    fn a_bullet_broken_by_the_measure_is_put_back_together() {
        let raw = "•Built and maintained the Python ecosystem behind wind resource assessment - SODAR (Sonic Detection) and \n\
                   met-mast processing, vectorized in NumPy.\n\
                   \n\
                   •Rewrote critical Airflow DAGs around vectorized algorithms: 5-10X faster task execution, fewer failures.\n";
        let lines = logical_lines(raw, no_headings, has_dates);

        assert_eq!(lines.len(), 2, "{lines:#?}");
        assert!(
            lines[0].text.ends_with("vectorized in NumPy."),
            "{:?}",
            lines[0].text
        );
        assert!(lines.iter().all(|l| l.is_bullet()));
    }

    /// A header runs the full measure too, but the line under it is the course
    /// list, not the rest of the university's name.
    #[test]
    fn an_entry_header_is_never_continued_by_the_line_below_it() {
        let raw = "BSc, Applied Mathematics and Computing, Odesa I.I.Mechnikov National University 2019 – 2023  |  Odesa, Ukraine\n\
                   Numerical methods, optimization and control theory, machine learning, econometrics\n";
        let lines = logical_lines(raw, no_headings, has_dates);

        assert_eq!(lines.len(), 2, "{lines:#?}");
        assert!(lines[0].text.ends_with("Odesa, Ukraine"));
        assert!(lines[1].text.starts_with("Numerical methods"));
    }

    /// A blank line between the two halves does not mean they are separate —
    /// exporters emit one wherever the line box changed. Width decides.
    #[test]
    fn a_continuation_is_joined_across_a_blank_line() {
        let raw = "Mathematical Modeling & HPC — Numerical Methods   Optimization & Control Theory   High-Performance Comp\n\
                   \n\
                   (HPC)   Time-Series Analysis   Vectorized Algorithms   Dynamical Systems\n\
                   \n\
                   Observability — Prometheus   Grafana   Loki   Promtail   monitoring   incident response\n";
        let lines = logical_lines(raw, no_headings, has_dates);

        assert_eq!(lines.len(), 2, "{lines:#?}");
        assert!(lines[0].text.contains("Dynamical Systems"));
        assert!(lines[1].text.starts_with("Observability"));
    }

    /// Characters are a poor proxy for typeset width, so a wrap that broke
    /// early was read as a new bullet: `…scholarship awarded` / `for academic
    /// excellence` arrived as two. Where the line ends is one signal; where the
    /// next one *starts* is the other.
    #[test]
    fn a_wrap_that_broke_short_of_the_measure_is_still_a_wrap() {
        let raw = "•  GPA: 3.72/4.00\n\
                   •  Alberta Advanced Education and Technology Achievement scholarship awarded\n\
                   for academic excellence\n\
                   Prestigious University, Iran\n\
                   •  Thesis Project: Pinch Technology\n\
                   •  Co-curricular activity: played table tennis professionally and received many awards and\n\
                   recognitions nationally\n";
        let lines = logical_lines(raw, no_headings, has_dates);

        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert!(
            texts
                .iter()
                .any(|t| t.ends_with("scholarship awarded for academic excellence")),
            "{texts:#?}"
        );
        // A line that begins with a capital is a new item, not a wrap — the
        // institution must not be folded into the bullet above it.
        assert!(
            texts.contains(&"Prestigious University, Iran"),
            "{texts:#?}"
        );
        assert_eq!(texts.len(), 5, "{texts:#?}");
    }

    #[test]
    fn a_sentence_that_ends_at_the_margin_is_not_continued() {
        let raw = "R&D venture on autonomous high-altitude airships and hybrid energy propulsion. At WebSummit 2021.\n\
                   https://elliscope.example.com\n";
        let lines = logical_lines(raw, no_headings, has_dates);
        assert_eq!(lines.len(), 2, "{lines:#?}");
    }

    #[test]
    fn a_pipe_rule_left_by_the_exporter_is_not_content() {
        let raw = "MySQL   MongoDB|||||||||||||||||||||\n";
        let lines = logical_lines(raw, no_headings, has_dates);
        assert_eq!(lines[0].text, "MySQL   MongoDB");
    }

    /// Pipes separate fields, and how many there are says which fields they
    /// hold. Reading the first as "location follows" made an employer a place.
    #[test]
    fn pipes_separate_title_employer_and_dates() {
        let h = EntryHeader::parse(
            "Restaurant Manager | Contoso Bar and Grill | September 2019 – 2021",
            &dates(),
        );
        assert_eq!(h.lead, "Restaurant Manager");
        assert_eq!(h.org, "Contoso Bar and Grill");
        assert_eq!(h.start, "2019");
    }

    /// A date sitting in the middle field must not be counted as one of the
    /// text fields, or every field after it lands one place to the left.
    #[test]
    fn a_date_between_two_pipes_is_a_date_not_a_field() {
        let h = EntryHeader::parse(
            "B.S. in Business Administration | June 2020 | Bigtown College, Chicago",
            &dates(),
        );
        assert_eq!(h.lead, "B.S. in Business Administration");
        assert_eq!(h.org, "Bigtown College, Chicago");
        assert_eq!(h.start, "June 2020");
    }

    #[test]
    fn a_work_header_gives_up_role_employer_dates_and_place() {
        let h = EntryHeader::parse(
            "Software Developer, GE Vernova 2024 – 2025  |  Barcelona, Spain",
            &dates(),
        );
        assert_eq!(h.lead, "Software Developer");
        assert_eq!(h.org, "GE Vernova");
        assert_eq!(h.start, "2024");
        assert_eq!(h.end, "2025");
        assert_eq!(h.location, "Barcelona, Spain");
    }

    /// The employer's own commas must not be split on — the last `, ` is the
    /// one that separates role from employer.
    #[test]
    fn an_employer_with_commas_in_its_name_survives() {
        let h = EntryHeader::parse(
            "Data & MLOps Engineer, Contract (DataArt, Inmost, Virtuace) 2021 – 2023",
            &dates(),
        );
        assert_eq!(h.org, "Contract (DataArt, Inmost, Virtuace)");
        assert_eq!(h.lead, "Data & MLOps Engineer");
        assert!(h.location.is_empty());
    }

    #[test]
    fn an_education_header_reads_as_institution_and_years() {
        let h = EntryHeader::parse(
            "Universitat Autònoma de Barcelona (UAB) 2025 – 2026  |  Barcelona, Spain",
            &dates(),
        );
        // One part and no comma: the splitter cannot know whether that names a
        // school or a job, so it stays in `lead` and the section decides.
        assert_eq!(h.lead, "Universitat Autònoma de Barcelona (UAB)");
        assert!(h.org.is_empty());
        assert_eq!(h.start, "2025");
        assert_eq!(h.location, "Barcelona, Spain");
    }
}
