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
const BULLET_GLYPHS: [char; 5] = ['•', '▪', '‣', '◦', '·'];

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

fn strip_footer_noise(line: &str) -> String {
    // A run of three or more pipes is decoration, never punctuation — a single
    // `|` is the separator an exporter puts before a location and must stay.
    let mut out = String::with_capacity(line.len());
    let mut pipe_run = 0usize;
    for ch in line.chars() {
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
    let mut source: Vec<(String, bool)> = Vec::new();
    let mut break_pending = false;
    for raw_line in raw.lines() {
        let line = strip_footer_noise(&normalize_spaces(raw_line));
        if line.is_empty() {
            break_pending = true;
            continue;
        }
        source.push((line, break_pending));
        break_pending = false;
    }

    let widths: Vec<&str> = source.iter().map(|(l, _)| l.as_str()).collect();
    let measure = measure_of(&widths);

    let mut out: Vec<LogicalLine> = Vec::new();
    for (line, _after_break) in source {
        let heading = is_heading(&line);
        let bullet = starts_with_bullet(&line);

        // Does this continue the line above?
        let continues = !heading
            && !bullet
            && out.last().is_some_and(|prev| {
                prev.kind != LineKind::Heading
                    && !has_date_range(&prev.text)
                    && !ends_a_sentence(&prev.text)
                    && (prev.tail_width >= measure || starts_mid_sentence(&line))
            });

        if continues {
            let prev = out.last_mut().expect("checked by is_some_and above");
            if !prev.text.ends_with(' ') {
                prev.text.push(' ');
            }
            prev.tail_width = line.chars().count();
            prev.text.push_str(&line);
            continue;
        }

        let tail_width = line.chars().count();
        out.push(LogicalLine {
            tail_width,
            text: if bullet {
                without_bullet(&line).to_string()
            } else {
                line
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
            // Anything after the dates is location-ish; a `|` may or may not
            // separate it.
            let tail = after.trim().trim_start_matches('|').trim();
            if !tail.is_empty() {
                header.location = tail.to_string();
            }
            rest = before;
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
