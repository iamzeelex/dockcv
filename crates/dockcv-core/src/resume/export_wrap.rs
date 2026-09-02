//! Wrapping plain text to a fixed column, in whatever script the CV is written
//! in.
//!
//! Plain text is the base format — the one every other export is measured
//! against and the one an ATS is most likely to read cleanly — so its one hard
//! rule, *no line exceeds the column*, has to hold for a CV in Russian, in
//! Japanese, in Thai and in Hindi, not only for one in English.
//!
//! Three things have to be right, and each of them is a different problem:
//!
//! **Width is not length.** `str::len()` counts bytes, so `Разработчик` reads
//! as 22 and wraps at half the intended column; `chars().count()` counts
//! scalars, so `日本語` reads as 3 when it occupies 6 terminal cells and
//! `é` written as `e` + combining acute reads as 2 when it occupies 1. The
//! measure that matches what a reader sees is East Asian Width (UAX #11), which
//! is what [`unicode_width`] implements. Ambiguous-width characters — Cyrillic,
//! Greek, `±`, `°` — count as one, which is right everywhere except inside a
//! CJK terminal, and being wrong by one column there is better than being wrong
//! by half a line everywhere else.
//!
//! **Not every script separates words with spaces.** Splitting on whitespace
//! gives a Japanese or Thai paragraph as a single unbreakable "word", which the
//! wrapper can then only cut arbitrarily. Break opportunities come from
//! [`icu_segmenter::LineSegmenter`] instead — UAX #14 plus the LSTM models that
//! find breaks in Thai, Lao, Khmer and Burmese, where the algorithm alone
//! cannot. It is also the segmenter `typst-layout` breaks the *page* with, so
//! the text export and the PDF agree about where a line may end, and it is
//! already compiled into every build for that reason: this costs nothing.
//!
//! **A cut must land on a grapheme boundary.** Break opportunities never fall
//! inside a grapheme cluster, so ordinary wrapping is safe by construction. The
//! one place it has to be enforced is the fallback for a run with no break in
//! it at all — a long URL, a German compound — where the cut is ours to choose
//! and must not land between a base character and its combining mark, or in the
//! middle of a family emoji.
//!
//! Right-to-left text needs no special handling here and gets none: a plain
//! text file stores logical order, and the terminal, editor or parser reading
//! it applies the bidirectional algorithm itself. Reordering on the way out
//! would corrupt the file for every correct reader.

use std::sync::LazyLock;

use icu_segmenter::options::LineBreakOptions;
use icu_segmenter::{LineSegmenter, LineSegmenterBorrowed};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The line breaker, built once. `new_lstm` is the constructor `typst-layout`
/// uses, so a paragraph breaks in the same places in the text file and on the
/// page.
static SEGMENTER: LazyLock<LineSegmenterBorrowed<'static>> =
    LazyLock::new(|| LineSegmenter::new_lstm(LineBreakOptions::default()));

/// How many terminal columns `s` occupies.
pub fn width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Wrap `text` to `columns`, putting `first_prefix` in front of the first line
/// and `rest_indent` spaces in front of every line after it.
///
/// Returns the finished lines without their newlines. Whitespace inside `text`
/// is normalised to single spaces first: a bullet is one paragraph, and a
/// newline the user happened to type inside it is not a line break they chose.
pub fn wrap(text: &str, first_prefix: &str, rest_indent: usize, columns: usize) -> Vec<String> {
    let normalized = normalize_whitespace(text);
    if normalized.is_empty() {
        return Vec::new();
    }

    let rest_prefix = " ".repeat(rest_indent);
    let mut lines: Vec<String> = Vec::new();
    let mut line = first_prefix.to_string();
    let mut has_content = false;

    // Whether the piece just placed ended in the space that caused its break.
    // Reinserting exactly what was trimmed is the only rule that is right in
    // every script: a space between two Latin words, nothing between two
    // Japanese characters, and nothing after the hyphen in `high-throughput`,
    // which is a break opportunity with no space in it.
    let mut pending_space = false;

    for raw in break_pieces(&normalized) {
        // A break opportunity falls *after* the space that caused it, so the
        // space belongs to the line being closed and never to the next one.
        let piece = raw.trim_end_matches(' ');
        let trailing_space = piece.len() < raw.len();
        if piece.is_empty() {
            pending_space |= trailing_space;
            continue;
        }

        let separator = if has_content && pending_space {
            " "
        } else {
            ""
        };
        pending_space = trailing_space;

        if !has_content || width(&line) + width(separator) + width(piece) <= columns {
            line.push_str(separator);
            line.push_str(piece);
        } else {
            lines.push(std::mem::take(&mut line));
            line = rest_prefix.clone();
            line.push_str(piece);
        }
        has_content = true;

        // One run with no break opportunity in it can still overflow — a long
        // URL, a compound noun, a path. That is the only place we choose a cut
        // point ourselves, and it goes on a grapheme boundary.
        while width(&line) > columns {
            let floor = if lines.is_empty() {
                width(first_prefix)
            } else {
                rest_indent
            };
            let Some((head, tail)) = split_at_width(&line, columns, floor) else {
                break;
            };
            lines.push(head);
            line = format!("{rest_prefix}{tail}");
        }
    }

    if has_content {
        lines.push(line);
    }
    lines
}

/// Wrap `text` and append the lines to `out`, each with its newline.
pub fn wrap_into(
    out: &mut String,
    text: &str,
    first_prefix: &str,
    rest_indent: usize,
    columns: usize,
) {
    for line in wrap(text, first_prefix, rest_indent, columns) {
        out.push_str(&line);
        out.push('\n');
    }
}

/// Every run of whitespace becomes one space, and the ends are trimmed.
fn normalize_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(c);
        }
    }
    out
}

/// The slices between consecutive break opportunities. The segmenter yields
/// byte offsets, starting at zero, so the first one closes nothing.
fn break_pieces(text: &str) -> impl Iterator<Item = &str> {
    let mut previous = 0;
    SEGMENTER.segment_str(text).filter_map(move |boundary| {
        if boundary <= previous {
            return None;
        }
        let piece = &text[previous..boundary];
        previous = boundary;
        Some(piece)
    })
}

/// Split `s` so the first part is at most `columns` wide, cutting on a grapheme
/// boundary and never leaving the head shorter than `floor` columns — otherwise
/// a prefix wider than the column would make no progress and loop forever.
///
/// `None` when there is nothing to gain by cutting.
fn split_at_width(s: &str, columns: usize, floor: usize) -> Option<(String, String)> {
    let mut head = String::new();
    let mut used = 0;
    let mut graphemes = s.graphemes(true).peekable();

    while let Some(g) = graphemes.peek() {
        let w = width(g);
        // Always take at least one grapheme past the prefix, so a column
        // narrower than the indent still terminates.
        if used + w > columns && used > floor {
            break;
        }
        used += w;
        head.push_str(g);
        graphemes.next();
    }

    let tail: String = graphemes.collect();
    if tail.is_empty() {
        None
    } else {
        Some((head, tail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule the whole module exists for, checked in five scripts.
    #[test]
    fn no_line_exceeds_the_column_in_any_script() {
        let paragraphs = [
            "Designed a high-throughput commit log processing 50M ops/sec with \
             sub-millisecond p99 latency across three regions.",
            // Russian: two bytes per letter, so byte length is twice the width.
            "Спроектировал высокопроизводительный журнал фиксации, обрабатывающий \
             50 миллионов операций в секунду с задержкой p99 менее миллисекунды.",
            // Japanese: no spaces at all, and every character is two columns.
            "分散ストレージシステムの設計と実装を担当し、毎秒五千万件の書き込みを\
             処理する高性能なコミットログを構築しました。",
            // Thai: no spaces and no algorithmic break opportunities either —
            // this is the case the LSTM model exists for.
            "ออกแบบและพัฒนาระบบจัดเก็บข้อมูลแบบกระจายที่รองรับการเขียนห้าสิบล้านรายการต่อวินาที",
            // Devanagari: combining marks, so scalar count overstates the width.
            "वितरित भंडारण प्रणाली के लिए उच्च प्रदर्शन वाला कमिट लॉग तैयार किया।",
        ];

        for text in paragraphs {
            for columns in [40, 72, 100] {
                for line in wrap(text, "  * ", 4, columns) {
                    assert!(
                        width(&line) <= columns,
                        "{:?} is {} columns wide, over {columns}",
                        line,
                        width(&line)
                    );
                }
            }
        }
    }

    /// Wrapping must not lose or invent a character. Whitespace is normalised,
    /// so the comparison is against the text with its own runs collapsed.
    #[test]
    fn wrapping_preserves_the_text() {
        for text in [
            "Designed a high-throughput commit log with sub-millisecond latency.",
            "Спроектировал журнал фиксации с задержкой менее миллисекунды.",
            "分散ストレージシステムの設計と実装を担当しました。",
        ] {
            let joined: String = wrap(text, "", 0, 30)
                .iter()
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join(" ");
            let expected: String = normalize_whitespace(text);
            // Latin words rejoin with the space the break took; scripts without
            // spaces rejoin with one the line break introduced, so compare with
            // spaces removed as well.
            assert_eq!(
                joined.replace(' ', ""),
                expected.replace(' ', ""),
                "characters were lost or invented wrapping {text:?}"
            );
        }
    }

    /// A run with no break opportunity in it still has to fit, and the cut has
    /// to land between graphemes rather than inside one.
    #[test]
    fn an_unbreakable_run_is_cut_on_a_grapheme_boundary() {
        let url = format!("https://example.com/{}", "a".repeat(200));
        let lines = wrap(&url, "  * ", 4, 40);
        assert!(lines.len() > 4);
        for line in &lines {
            assert!(width(line) <= 40);
        }

        // `e` + combining acute is one grapheme and must never be split.
        let combining = "e\u{0301}".repeat(60);
        for line in wrap(&combining, "", 0, 20) {
            assert!(width(&line) <= 20);
            assert!(
                !line.starts_with('\u{0301}'),
                "a combining mark was orphaned onto its own line"
            );
        }

        // A family emoji is one grapheme of width two, joined by ZWJ.
        let family = "👨‍👩‍👧‍👦".repeat(20);
        for line in wrap(&family, "", 0, 10) {
            assert!(width(&line) <= 10);
            assert!(!line.contains('\u{200D}') || line.ends_with('👦'));
        }
    }

    #[test]
    fn the_prefix_and_the_hanging_indent_are_part_of_the_width() {
        let lines = wrap(
            "Mentored eight senior engineers and authored five architecture RFCs \
             for the storage platform.",
            "  * ",
            4,
            32,
        );
        assert!(lines[0].starts_with("  * "));
        for line in &lines[1..] {
            assert!(line.starts_with("    "), "{line:?} lost its hanging indent");
            assert!(!line.starts_with("     "), "{line:?} gained an indent");
        }
        for line in &lines {
            assert!(width(line) <= 32);
        }
    }

    /// A column narrower than the indent itself must still terminate.
    #[test]
    fn a_column_narrower_than_the_indent_still_finishes() {
        let lines = wrap("supercalifragilistic expialidocious", "        ", 8, 4);
        assert!(!lines.is_empty());
        assert!(lines.len() < 100, "the cut made no progress");
    }

    #[test]
    fn empty_and_blank_text_produce_no_lines() {
        assert!(wrap("", "  * ", 4, 72).is_empty());
        assert!(wrap("   \n\t  ", "  * ", 4, 72).is_empty());
    }

    /// Right-to-left text is stored in logical order — the reader reverses it,
    /// not us — and still has to fit the column.
    #[test]
    fn right_to_left_text_is_left_in_logical_order() {
        let hebrew = "תכנן והטמיע מערכת אחסון מבוזרת בעלת ביצועים גבוהים לאורך שלוש שנים";
        let lines = wrap(hebrew, "", 0, 30);
        assert!(!lines.is_empty());
        for line in &lines {
            assert!(width(line) <= 30);
        }
        let rejoined = lines.join(" ");
        assert!(
            rejoined.starts_with("תכנן"),
            "the first word of the source is no longer first: {rejoined:?}"
        );
    }
}
