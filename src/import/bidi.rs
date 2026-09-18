//! Putting a right-to-left line back in the order it was written.
//!
//! A PDF's content stream paints glyphs where they land, left to right, and
//! that is the only order it records. For Latin text the two orders are the
//! same and nobody notices. For Hebrew or Arabic they are opposite: the file
//! says `ןהכ לאינד` because that is the order the glyphs sit in on the page,
//! and the name the person typed is `דניאל כהן`. An extractor that hands back
//! glyph order — which `pdf-extract` does, and which is the honest thing for it
//! to do — hands back a name spelled backwards, a section heading no taxonomy
//! can match, and a CV that imports as nonsense.
//!
//! So this is the inverse of the display step: **visual order in, logical order
//! out**. It is a deliberately small piece of UAX #9 — enough for a document
//! that is a CV rather than a legal filing — and it is applied where the input
//! is known to be visual, which is the PDF engine and nowhere else. A `.docx`,
//! a text file and a JSON Resume all store logical order already, and running
//! this over them would reverse what was right.
//!
//! What it does, on a line that contains a right-to-left letter:
//!
//! 1. reverses the line, which puts the right-to-left words the right way round
//!    and the Latin ones the wrong way;
//! 2. reverses each run that is *not* right-to-left back again, so an email
//!    address, a year and a company name in Latin letters read forwards inside
//!    a Hebrew sentence;
//! 3. mirrors the characters that are drawn mirrored — a `(` painted on the
//!    left of a reversed line is the `)` the author typed.

use std::borrow::Cow;

/// Is this a letter of a right-to-left script?
///
/// The blocks a CV can plausibly be written in, not every one Unicode defines:
/// Hebrew, Arabic and its supplements and presentation forms, Syriac, Thaana
/// and N'Ko.
fn is_rtl(ch: char) -> bool {
    matches!(ch as u32,
        0x0590..=0x05FF   // Hebrew
        | 0x0600..=0x06FF // Arabic
        | 0x0700..=0x074F // Syriac
        | 0x0750..=0x077F // Arabic Supplement
        | 0x0780..=0x07BF // Thaana
        | 0x07C0..=0x07FF // N'Ko
        | 0x08A0..=0x08FF // Arabic Extended-A
        | 0xFB1D..=0xFDFF // Hebrew and Arabic presentation forms
        | 0xFE70..=0xFEFF)
}

/// The character this one is drawn as when the run around it is reversed.
fn mirror(ch: char) -> char {
    match ch {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        other => other,
    }
}

/// One line of a PDF's text layer, in the order it was typed.
///
/// A line with no right-to-left letter in it is returned untouched and
/// unallocated, which is every line of every CV this product has read so far.
pub fn to_logical_order(line: &str) -> Cow<'_, str> {
    if !line.chars().any(is_rtl) {
        return Cow::Borrowed(line);
    }

    let reversed: Vec<char> = line.chars().rev().collect();
    let mut out: Vec<char> = Vec::with_capacity(reversed.len());

    let mut at = 0;
    while at < reversed.len() {
        if is_rtl(reversed[at]) {
            out.push(reversed[at]);
            at += 1;
            continue;
        }
        // A run of everything that is not a right-to-left letter: Latin words,
        // digits, punctuation and the spaces between them.
        let start = at;
        while at < reversed.len() && !is_rtl(reversed[at]) {
            at += 1;
        }
        let run = &reversed[start..at];

        // Only the *word* inside the run turns back round. What sits outside it
        // — spaces, a colon, a bracket — never belonged to the Latin run at
        // all: it is punctuation of the right-to-left sentence around it, which
        // is why `דואר: daniel@example.com` keeps its colon against the Hebrew
        // and not against the address. Those characters stay where the reversal
        // put them and are mirrored, because a `(` drawn at one end of a
        // reversed line is the `)` that was typed.
        let core = run
            .iter()
            .position(|c| c.is_alphanumeric())
            .map(|first| {
                let last = run
                    .iter()
                    .rposition(|c| c.is_alphanumeric())
                    .unwrap_or(first);
                first..last + 1
            })
            .unwrap_or(0..0);

        out.extend(run[..core.start].iter().map(|c| mirror(*c)));
        out.extend(run[core.clone()].iter().rev());
        out.extend(run[core.end..].iter().map(|c| mirror(*c)));
    }

    Cow::Owned(out.into_iter().collect())
}

/// Every line of a text layer, put back in the order it was written.
pub fn text_to_logical_order(text: &str) -> Cow<'_, str> {
    if !text.chars().any(is_rtl) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&to_logical_order(line));
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_latin_line_is_not_touched() {
        for line in [
            "Senior Software Engineer, Acme Corp",
            "albert@example.com | +45 28 44 10 92 | Bern, Switzerland",
            "Скоротила затримку p99 удвічі (на платіжному шляху).",
            "",
        ] {
            assert!(
                matches!(to_logical_order(line), Cow::Borrowed(_)),
                "{line:?} should not even be copied"
            );
            assert_eq!(to_logical_order(line), line);
        }
    }

    #[test]
    fn a_hebrew_name_comes_back_the_way_it_was_typed() {
        // What the page paints, and what the author wrote.
        assert_eq!(to_logical_order("ןהכ לאינד"), "דניאל כהן");
        // And the other way round, because the operation is its own inverse on
        // a line with nothing but right-to-left letters in it.
        assert_eq!(to_logical_order("דניאל כהן"), "ןהכ לאינד");
    }

    #[test]
    fn latin_inside_a_right_to_left_line_still_reads_forwards() {
        // A Latin run inside a right-to-left line is *displayed* in its own
        // order, so what reads left to right on the page is what was typed: the
        // line was written `מהנדסים 2019 Acme` and painted with the Hebrew at
        // the right-hand end. Both halves have to come back, each the right way
        // round.
        let visual = "2019 Acme םיסדנהמ";
        let logical = to_logical_order(visual);
        assert_eq!(logical, "מהנדסים 2019 Acme");
    }

    #[test]
    fn an_address_inside_a_hebrew_line_survives_whole() {
        let visual = "daniel@example.com :ראוד";
        let logical = to_logical_order(visual);
        assert!(
            logical.contains("daniel@example.com"),
            "an email must not be turned round: {logical:?}"
        );
    }

    #[test]
    fn brackets_are_mirrored_back() {
        // A `(` drawn at the left of a reversed line is the `)` that was typed.
        let logical = to_logical_order("(2019) םיסדנהמ");
        assert!(
            logical.contains("(2019)"),
            "the brackets should sit round the year: {logical:?}"
        );
    }

    #[test]
    fn whitespace_does_not_wander() {
        let logical = to_logical_order("Acme םיסדנהמ");
        assert_eq!(logical.matches(' ').count(), 1, "{logical:?}");
        assert!(
            !logical.starts_with(' ') && !logical.ends_with(' '),
            "{logical:?}"
        );
    }

    #[test]
    fn a_whole_text_layer_keeps_its_lines() {
        let text = "ןהכ לאינד\ndaniel@example.com\nEngineer";
        let logical = text_to_logical_order(text);
        let lines: Vec<&str> = logical.lines().collect();
        assert_eq!(lines[0], "דניאל כהן");
        assert_eq!(lines[1], "daniel@example.com");
        assert_eq!(lines[2], "Engineer");
    }
}
