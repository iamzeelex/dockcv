//! Markdown import: read the markup, do not guess at it.
//!
//! Markdown states its own structure — `##` is a section, `###` opens an entry,
//! `-` is a list item — and the shared classifier is built to be *told* those
//! things ([`layout::LineKind`]) rather than to infer them from typography.
//! Handing a Markdown file to the plain-prose path threw all of that away and
//! then guessed it back, badly: `## Work Experience` became a section titled
//! `## Work Experience`, `### Senior Platform Engineer, Kollekt` became one of
//! its entries' *titles* with the hashes still on it, and `**Languages:** Rust,
//! Go` arrived as a skill called `Languages:** Rust`.
//!
//! So this engine reads the markup and produces logical lines, the way
//! [`super::docx`] reads styles. What it deliberately does not do is render
//! Markdown: emphasis, code spans and links are unwrapped to the text a reader
//! would see, because the classifier's job is to find a name and a date in a
//! line, and `**Kollekt**` is not a different employer from `Kollekt`.

use crate::import::classifier::{classify_lines, is_only_dates};
use crate::import::layout::{LineKind, LogicalLine};
use crate::import::model::ImportedDoc;

/// Read a Markdown CV.
pub fn import_markdown(format_name: &str, content: &str) -> ImportedDoc {
    classify_lines(format_name, logical_lines(content))
}

/// Whether a line is a fence opening or closing a code block.
fn is_fence(line: &str) -> bool {
    line.starts_with("```") || line.starts_with("~~~")
}

/// A thematic break: `---`, `***`, `___`, and any longer run.
fn is_thematic_break(line: &str) -> bool {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    compact.len() >= 3
        && (compact.chars().all(|c| c == '-')
            || compact.chars().all(|c| c == '*')
            || compact.chars().all(|c| c == '_'))
}

/// An ATX heading: its level and its text.
fn atx_heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &line[level..];
    // `#hashtag` is not a heading; a heading needs the space.
    if !rest.starts_with(' ') && !rest.is_empty() {
        return None;
    }
    Some((level, rest.trim().trim_end_matches('#').trim()))
}

/// A list item: the text after the marker.
///
/// Unlike the plain-text path, indentation carries no weight here — a Markdown
/// list item says what it is, at any depth.
fn list_item(line: &str) -> Option<&str> {
    let body = line.trim_start();
    for marker in ['-', '*', '+'] {
        if let Some(rest) = body.strip_prefix(marker) {
            if rest.starts_with(' ') {
                return Some(rest.trim_start());
            }
        }
    }
    // `1. item` — an ordered list.
    let digits = body.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &body[digits..];
        if let Some(rest) = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')')) {
            if rest.starts_with(' ') {
                return Some(rest.trim_start());
            }
        }
    }
    None
}

/// Markdown inline markup, reduced to what a reader sees.
///
/// A link keeps its label and, when the target says something the label does
/// not, the target in parentheses after it — the same shape the plain-text
/// exporter writes, so both formats reach the classifier looking alike.
/// `[dtu.dk](https://dtu.dk)` adds nothing and stays one word; `[GitHub](https://github.com/x)`
/// keeps both halves, because the address is the half that identifies the
/// profile.
fn inline_text(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    let bytes = markup.as_bytes();
    let mut i = 0;

    while i < markup.len() {
        if !markup.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &markup[i..];

        if let Some(consumed) = take_link(rest, &mut out) {
            i += consumed;
            continue;
        }
        // Emphasis and code spans are typography; the words are the content.
        if rest.starts_with("**") || rest.starts_with("__") {
            i += 2;
            continue;
        }
        if bytes[i] == b'*' || bytes[i] == b'`' {
            i += 1;
            continue;
        }
        // A backslash escape hides the character after it from Markdown, not
        // from the reader.
        if bytes[i] == b'\\' && i + 1 < markup.len() {
            let next = markup[i + 1..].chars().next().unwrap_or('\\');
            out.push(next);
            i += 1 + next.len_utf8();
            continue;
        }
        let ch = rest.chars().next().unwrap_or_default();
        out.push(ch);
        i += ch.len_utf8();
    }
    // `**Languages:** Rust` unwraps to `Languages: Rust`, which is the shape
    // `split_skill_group` reads. Collapse the spaces emphasis left behind.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `[label](target)` at the head of `rest`, appended to `out`. Returns how many
/// bytes it consumed.
fn take_link(whole: &str, out: &mut String) -> Option<usize> {
    // `![alt](src)` is an image; a CV's is a photo or a badge, and either way
    // the alt text is what a reader is left with.
    let bang = usize::from(whole.starts_with('!'));
    let rest = &whole[bang..];
    if !rest.starts_with('[') {
        return None;
    }
    let close = rest.find("](")?;
    let end = rest[close..].find(')')? + close;
    let label = inline_text(&rest[1..close]);
    let target = rest[close + 2..end].trim();

    let bare = target
        .split_once("://")
        .map(|(_, host)| host)
        .unwrap_or(target)
        .trim_start_matches("mailto:")
        .trim_start_matches("tel:")
        .trim_end_matches('/');

    out.push_str(&label);
    // A target that only repeats the label — or spells the same phone number
    // without its spaces — is not worth printing twice.
    let redundant = label.is_empty()
        || bare.eq_ignore_ascii_case(&label)
        || bare.replace(['-', ' ', '(', ')'], "") == label.replace(['-', ' ', '(', ')'], "");
    if !redundant {
        out.push_str(" (");
        out.push_str(target);
        out.push(')');
    }
    Some(bang + end + 1)
}

/// Turn a Markdown document into logical lines.
fn logical_lines(content: &str) -> Vec<LogicalLine> {
    let mut out: Vec<LogicalLine> = Vec::new();
    let mut in_code = false;
    let mut seen_heading = false;

    for raw in content.lines() {
        let line = raw.trim_end();
        if is_fence(line.trim_start()) {
            in_code = !in_code;
            continue;
        }
        if in_code || line.trim().is_empty() || is_thematic_break(line) {
            continue;
        }

        if let Some((level, text)) = atx_heading(line.trim_start()) {
            let text = inline_text(text);
            if text.is_empty() {
                continue;
            }
            // The first `#` is the person's name, the way a Markdown CV opens
            // — and the way `docx::kind_of` treats a template's `Heading1` on
            // the first line. Reading it as a section filed the whole document
            // under a section named after its author.
            let kind = match level {
                1 if !seen_heading => LineKind::Text,
                1 | 2 => LineKind::Heading,
                _ => LineKind::EntryHeader,
            };
            seen_heading |= kind == LineKind::Heading;
            out.push(LogicalLine::new(text, kind));
            continue;
        }

        if let Some(item) = list_item(line) {
            let text = inline_text(item);
            if !text.is_empty() {
                out.push(LogicalLine::new(text, LineKind::Bullet));
            }
            continue;
        }

        let text = inline_text(line.trim_start().trim_start_matches("> "));
        if text.is_empty() {
            continue;
        }
        // `*2022-03 - Present*` under an entry header is that entry's dates, on
        // their own line because Markdown has nowhere else to put them.
        if is_only_dates(&text) {
            if let Some(open) = out.last_mut().filter(|l| l.kind == LineKind::EntryHeader) {
                open.text = format!("{} {text}", open.text);
                continue;
            }
        }
        out.push(LogicalLine::new(text, LineKind::Text));
    }
    out
}
