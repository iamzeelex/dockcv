//! Résumé dates: what the user typed, plus whatever the app could make of it.
//!
//! ### Why this is a newtype over `String` and not a calendar date
//!
//! A CV date is not a timestamp. It is "2019", or "Jan 2019", or "Summer
//! 2021", or "Present" — as precise as the person chose to be, and sometimes
//! not a date at all. A `chrono::NaiveDate` cannot hold most of those, so
//! storing one would mean either rejecting what the user typed or inventing a
//! day they never gave.
//!
//! So the text stays the source of truth and is **never rewritten**, and
//! parsing is a lens over it: when the app understands a date it can reformat
//! it to the document's chosen style, and when it does not, the text prints
//! exactly as typed. That also makes the on-disk shape unchanged — the TOML
//! still reads `start_date = "2022-01"`, so every existing vault opens with no
//! migration at all, and hand-editing a file stays as easy as it was.
//!
//! ### What is deliberately *not* parsed
//!
//! Ambiguous numeric forms. `01/02/2022` is the first of February to half the
//! world and the second of January to the other half, and this persona works
//! across both (the product review §1). Guessing would silently move a job
//! by a month; the honest answer is to leave it as text and print it as
//! written.

use serde::{Deserialize, Serialize};

use super::language::DocumentLanguage;

/// A date on a résumé, as typed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResumeDate {
    /// Exactly what the user entered. The editor binds a text field straight
    /// to this, so it is also what round-trips through the vault file.
    pub text: String,
}

impl ResumeDate {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// The date this text denotes, if the app can tell.
    pub fn parse(&self) -> Option<CivilDate> {
        parse_date(self.text.trim())
    }

    /// Whether this date says "still there" rather than naming a day.
    ///
    /// `Present` is the one thing a CV writes in an end-date field that is not
    /// a date, and interchange formats have nowhere to put it: JSON Resume
    /// validates its dates against a subset of ISO 8601, so the field has to be
    /// dropped and the fact recorded elsewhere. Asking here rather than in the
    /// emitter keeps the vocabulary in one place.
    pub fn names_the_present(&self) -> bool {
        const PRESENT: [&str; 7] = [
            "present",
            "current",
            "ongoing",
            "now",
            "to date",
            "настоящее время",
            "н.в.",
        ];
        let text = self.text.trim().trim_end_matches('.').to_lowercase();
        PRESENT
            .iter()
            .any(|word| text == *word || text == format!("по {word}"))
    }

    /// The text to print for this date under `format`.
    ///
    /// Falls back to the raw text whenever it cannot be parsed — a CV that
    /// says "Summer 2021" keeps saying it.
    pub fn display(&self, format: DateFormat) -> String {
        self.display_in(format, DocumentLanguage::English)
    }

    /// The text to print under both a date shape and a document language.
    ///
    /// Parsing stays language-neutral because the stored ISO-like values are
    /// the source of truth. Only the words emitted for a parsed date are
    /// localized; text DockCV cannot parse remains exactly what its author
    /// typed.
    pub fn display_in(&self, format: DateFormat, language: DocumentLanguage) -> String {
        match self.parse() {
            Some(date) => format.render_in(date, language),
            None => self.text.trim().to_string(),
        }
    }
}

impl<T: Into<String>> From<T> for ResumeDate {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

/// A calendar date at whatever precision was given: a year, a year and month,
/// or a full date. Precision is part of the value, not a flag beside it — a
/// résumé that says "2019" must not start printing "1 January 2019".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CivilDate {
    pub year: i32,
    pub month: Option<u32>,
    pub day: Option<u32>,
}

/// Formats the document can print its dates in.
///
/// The set the market has settled on (FlowCV ships the same list), plus the
/// fact that on a résumé most dates are month-and-year — every variant below
/// degrades to the precision it was given rather than padding a day it does
/// not have.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateFormat {
    /// `2022-01-15` — what the vault files themselves use.
    ///
    /// Not the default, and the reason is that it once was. A CV printing
    /// `2015-09 – 2017-06` reads like a log line, and the app disagreed with
    /// itself besides: the editor's own month and year pickers say `Sep 2015`
    /// while the page said `2015-09`. A storage format is not a typographic
    /// choice, and it became one here only because both live on the same enum.
    Iso,
    /// `Jan 2022` · `15 Jan 2022` — the default, and what the editor's date
    /// pickers already show.
    #[default]
    DayMonShortYear,
    /// `January 2022` · `15th January 2022`
    DayOrdinalMonthYear,
    /// `Jan 2022` · `Jan 15, 2022`
    MonShortDayYear,
    /// `January 2022` · `January 15th, 2022`
    MonthDayOrdinalYear,
    /// `01/2022` · `15/01/2022`
    SlashDayFirst,
    /// `01/2022` · `01/15/2022`
    SlashMonthFirst,
    /// `01.2022` · `15.01.2022`
    DotDayFirst,
}

/// How a date is written down: the shape, and the language its words are in.
///
/// The two travel together or they come apart. Every emitter used to thread a
/// bare `DateFormat` and reach [`DocumentLanguage::English`] at the bottom, so
/// a German CV exported to DOCX, plain text or Markdown printed `Mar 2021 -
/// Present` under a page that said `Mär 2021 - Heute`. One parameter cannot
/// be half-passed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DateStyle {
    pub format: DateFormat,
    pub language: DocumentLanguage,
}

impl DateStyle {
    pub const fn new(format: DateFormat, language: DocumentLanguage) -> Self {
        Self { format, language }
    }

    /// The shape alone, in English — for a composed `Resume`, which carries no
    /// language of its own.
    pub const fn plain(format: DateFormat) -> Self {
        Self::new(format, DocumentLanguage::English)
    }
}

impl DateFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Iso => "YYYY-MM-DD",
            Self::DayMonShortYear => "DD MMM YYYY",
            Self::DayOrdinalMonthYear => "Do MMMM YYYY",
            Self::MonShortDayYear => "MMM DD, YYYY",
            Self::MonthDayOrdinalYear => "MMMM Do, YYYY",
            Self::SlashDayFirst => "DD/MM/YYYY",
            Self::SlashMonthFirst => "MM/DD/YYYY",
            Self::DotDayFirst => "DD.MM.YYYY",
        }
    }

    /// A worked example, so the picker shows the shape rather than describing
    /// it — the same thing FlowCV's dropdown does, and the reason its list is
    /// readable at a glance.
    pub fn example(self) -> String {
        self.render(CivilDate {
            year: 2026,
            month: Some(8),
            day: Some(8),
        })
    }

    pub const ALL: [DateFormat; 8] = [
        Self::Iso,
        Self::DayMonShortYear,
        Self::DayOrdinalMonthYear,
        Self::MonShortDayYear,
        Self::MonthDayOrdinalYear,
        Self::SlashDayFirst,
        Self::SlashMonthFirst,
        Self::DotDayFirst,
    ];

    /// Render `date` at the precision it carries.
    pub fn render(self, date: CivilDate) -> String {
        self.render_in(date, DocumentLanguage::English)
    }

    /// Render `date` with the words and ordinal convention of `language`.
    pub fn render_in(self, date: CivilDate, language: DocumentLanguage) -> String {
        let year = date.year;
        let Some(month) = date.month else {
            // Year only: every format prints the same thing, because there is
            // nothing else to arrange.
            return year.to_string();
        };
        let month_index = (month as usize - 1).min(11);
        let short = language.month_short(month_index);
        let long = language.month_long(month_index);

        match (self, date.day) {
            (Self::Iso, Some(day)) => format!("{year:04}-{month:02}-{day:02}"),
            (Self::Iso, None) => format!("{year:04}-{month:02}"),

            (Self::DayMonShortYear, Some(day)) => format!("{day:02} {short} {year}"),
            (Self::DayMonShortYear, None) => format!("{short} {year}"),

            (Self::DayOrdinalMonthYear, Some(day)) => match language {
                DocumentLanguage::English => format!("{} {long} {year}", ordinal(day)),
                DocumentLanguage::German => format!("{day}. {long} {year}"),
            },
            (Self::DayOrdinalMonthYear, None) => format!("{long} {year}"),

            (Self::MonShortDayYear, Some(day)) => format!("{short} {day:02}, {year}"),
            (Self::MonShortDayYear, None) => format!("{short} {year}"),

            (Self::MonthDayOrdinalYear, Some(day)) => match language {
                DocumentLanguage::English => format!("{long} {}, {year}", ordinal(day)),
                DocumentLanguage::German => format!("{long} {day}., {year}"),
            },
            (Self::MonthDayOrdinalYear, None) => format!("{long} {year}"),

            (Self::SlashDayFirst, Some(day)) => format!("{day:02}/{month:02}/{year}"),
            (Self::SlashMonthFirst, Some(day)) => format!("{month:02}/{day:02}/{year}"),
            // Both slashed forms collapse to the same month/year: with no day
            // there is no order to disagree about.
            (Self::SlashDayFirst | Self::SlashMonthFirst, None) => format!("{month:02}/{year}"),

            (Self::DotDayFirst, Some(day)) => format!("{day:02}.{month:02}.{year}"),
            (Self::DotDayFirst, None) => format!("{month:02}.{year}"),
        }
    }
}

/// `1` → `1st`, `2` → `2nd`, `11` → `11th`.
fn ordinal(day: u32) -> String {
    let suffix = match (day % 10, day % 100) {
        // 11th, 12th, 13th break the pattern the other teens follow.
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{day}{suffix}")
}

/// Read a date from text, or decline.
///
/// Accepts only forms that mean one thing: ISO (`2022`, `2022-01`,
/// `2022-01-15`) and month-name forms (`Jan 2022`, `January 2022`,
/// `15 January 2022`). See this module's header for why `01/02/2022` is
/// deliberately refused.
fn parse_date(text: &str) -> Option<CivilDate> {
    if text.is_empty() {
        return None;
    }

    // ISO, dash-separated.
    let parts: Vec<&str> = text.split('-').collect();
    if parts.len() <= 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        let year: i32 = parts[0].parse().ok()?;
        if parts[0].len() != 4 {
            return None;
        }
        let month = parts.get(1).and_then(|m| m.parse::<u32>().ok());
        let day = parts.get(2).and_then(|d| d.parse::<u32>().ok());
        return valid(year, month, day);
    }

    // Month name, in either order, with an optional leading day.
    let words: Vec<&str> = text.split_whitespace().collect();
    if (2..=3).contains(&words.len()) {
        let month = words.iter().find_map(|w| month_number(w));
        let year = words
            .iter()
            .find_map(|w| (w.len() == 4).then(|| w.parse::<i32>().ok()).flatten());
        let day = words.iter().find_map(|w| {
            (w.len() <= 2)
                .then(|| {
                    w.trim_end_matches(|c: char| !c.is_ascii_digit())
                        .parse::<u32>()
                        .ok()
                })
                .flatten()
        });
        if let (Some(month), Some(year)) = (month, year) {
            return valid(year, Some(month), day);
        }
    }

    None
}

fn month_number(word: &str) -> Option<u32> {
    let word = word.trim_end_matches(',').to_lowercase();
    (0..12)
        .position(|month| {
            let long = DocumentLanguage::English.month_long(month).to_lowercase();
            let short = DocumentLanguage::English.month_short(month).to_lowercase();
            long == word || short == word
        })
        .map(|i| i as u32 + 1)
}

fn valid(year: i32, month: Option<u32>, day: Option<u32>) -> Option<CivilDate> {
    if !(1000..=9999).contains(&year) {
        return None;
    }
    if let Some(m) = month {
        if !(1..=12).contains(&m) {
            return None;
        }
    }
    if let Some(d) = day {
        if !(1..=31).contains(&d) || month.is_none() {
            return None;
        }
    }
    Some(CivilDate { year, month, day })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Precision is preserved: a résumé that says "2019" must never start
    /// claiming a month, and one that says "Jan 2019" must never claim a day.
    #[test]
    fn a_date_is_never_printed_more_precisely_than_it_was_given() {
        let year_only = ResumeDate::new("2019");
        for format in DateFormat::ALL {
            assert_eq!(year_only.display(format), "2019", "{format:?}");
        }

        let month = ResumeDate::new("2019-03");
        assert_eq!(month.display(DateFormat::DayMonShortYear), "Mar 2019");
        assert_eq!(month.display(DateFormat::SlashDayFirst), "03/2019");
        assert_eq!(month.display(DateFormat::Iso), "2019-03");
    }

    #[test]
    fn a_full_date_renders_in_every_offered_shape() {
        let d = ResumeDate::new("2026-08-08");
        assert_eq!(d.display(DateFormat::Iso), "2026-08-08");
        assert_eq!(d.display(DateFormat::DayMonShortYear), "08 Aug 2026");
        assert_eq!(
            d.display(DateFormat::DayOrdinalMonthYear),
            "8th August 2026"
        );
        assert_eq!(d.display(DateFormat::MonShortDayYear), "Aug 08, 2026");
        assert_eq!(
            d.display(DateFormat::MonthDayOrdinalYear),
            "August 8th, 2026"
        );
        assert_eq!(d.display(DateFormat::SlashDayFirst), "08/08/2026");
        assert_eq!(d.display(DateFormat::SlashMonthFirst), "08/08/2026");
        assert_eq!(d.display(DateFormat::DotDayFirst), "08.08.2026");
    }

    #[test]
    fn german_localizes_only_words_the_model_understands() {
        let month = ResumeDate::new("2019-03");
        assert_eq!(
            month.display_in(DateFormat::DayMonShortYear, DocumentLanguage::German),
            "Mär 2019"
        );

        let full = ResumeDate::new("2026-08-08");
        assert_eq!(
            full.display_in(DateFormat::DayOrdinalMonthYear, DocumentLanguage::German),
            "8. August 2026"
        );

        let authors_words = ResumeDate::new("Sommer 2021");
        assert_eq!(
            authors_words.display_in(DateFormat::DayOrdinalMonthYear, DocumentLanguage::German),
            "Sommer 2021"
        );
        assert_eq!(authors_words.text, "Sommer 2021");
    }

    /// The rule this module exists to hold: text the app cannot read is
    /// printed exactly as typed, never dropped and never guessed at.
    #[test]
    fn text_that_is_not_a_date_survives_verbatim() {
        for raw in ["Present", "Summer 2021", "ongoing", "—"] {
            let date = ResumeDate::new(raw);
            assert!(date.parse().is_none(), "{raw} must not parse");
            assert_eq!(date.display(DateFormat::MonthDayOrdinalYear), raw);
        }
    }

    /// `01/02/2022` means two different months either side of the Atlantic,
    /// and this user works on both. Refusing to parse it is the point.
    #[test]
    fn ambiguous_numeric_dates_are_refused_rather_than_guessed() {
        let ambiguous = ResumeDate::new("01/02/2022");
        assert!(ambiguous.parse().is_none());
        assert_eq!(ambiguous.display(DateFormat::Iso), "01/02/2022");
    }

    #[test]
    fn month_names_parse_in_either_order() {
        assert_eq!(
            ResumeDate::new("Jan 2022").parse(),
            Some(CivilDate {
                year: 2022,
                month: Some(1),
                day: None
            })
        );
        assert_eq!(
            ResumeDate::new("January 2022").parse(),
            Some(CivilDate {
                year: 2022,
                month: Some(1),
                day: None
            })
        );
        assert_eq!(
            ResumeDate::new("15 March 2022").parse(),
            Some(CivilDate {
                year: 2022,
                month: Some(3),
                day: Some(15)
            })
        );
    }

    #[test]
    fn ordinals_handle_the_teens() {
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(3), "3rd");
        assert_eq!(ordinal(4), "4th");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(12), "12th");
        assert_eq!(ordinal(13), "13th");
        assert_eq!(ordinal(21), "21st");
    }

    /// Nonsense that looks numeric must not become a date.
    #[test]
    fn out_of_range_values_are_refused() {
        assert!(ResumeDate::new("2022-13").parse().is_none());
        assert!(ResumeDate::new("2022-00").parse().is_none());
        assert!(ResumeDate::new("2022-01-32").parse().is_none());
        assert!(ResumeDate::new("99").parse().is_none());
    }

    /// The stored shape is still a plain string, so existing vault files load
    /// untouched and stay hand-editable.
    #[test]
    fn the_stored_shape_is_still_a_bare_string() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Row {
            start_date: ResumeDate,
        }
        let row: Row = toml::from_str("start_date = \"2022-01\"").expect("loads old data");
        assert_eq!(row.start_date.text, "2022-01");
        let out = toml::to_string(&row).expect("serializes");
        assert_eq!(out.trim(), "start_date = \"2022-01\"");
    }
}
