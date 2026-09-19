//! The language of one document reading.
//!
//! This is deliberately smaller than a locale system. A preset needs a
//! stable BCP 47 language tag for PDF metadata, Typst's hyphenation rules,
//! and the few date words DockCV itself emits. It does not translate content
//! and it does not create another [`super::versioning::Versioned`] axis.

/// A language DockCV can emit without guessing any of its own words.
///
/// The stored model uses `Option<String>` because that is the public TOML
/// shape specified for C5. This resolved enum is the boundary that prevents a
/// hand-edited value from being interpolated into Typst source and makes the
/// short, user-facing list explicit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DocumentLanguage {
    #[default]
    English,
    German,
}

impl DocumentLanguage {
    pub const ALL: [Self; 2] = [Self::English, Self::German];

    /// Resolve a stored BCP 47 language tag. Unknown values safely render as
    /// English while remaining untouched in the hand-editable source file.
    pub fn from_code(code: Option<&str>) -> Self {
        match code.map(str::trim) {
            Some(code) if code.eq_ignore_ascii_case("de") => Self::German,
            _ => Self::English,
        }
    }

    /// The tag Typst writes to the PDF catalog's `/Lang` entry.
    pub const fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::German => "de",
        }
    }

    /// The sparse TOML representation. English is the backwards-compatible
    /// document default, so it needs no key at all.
    pub const fn stored(self) -> Option<&'static str> {
        match self {
            Self::English => None,
            Self::German => Some("de"),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::German => "Deutsch",
        }
    }

    pub const fn badge(self) -> &'static str {
        match self {
            Self::English => "EN",
            Self::German => "DE",
        }
    }

    /// The word used when an end date is absent or explicitly says the role
    /// is current.
    pub const fn present(self) -> &'static str {
        match self {
            Self::English => "Present",
            Self::German => "Heute",
        }
    }

    pub(crate) const fn month_long(self, month: usize) -> &'static str {
        const EN: [&str; 12] = [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];
        const DE: [&str; 12] = [
            "Januar",
            "Februar",
            "März",
            "April",
            "Mai",
            "Juni",
            "Juli",
            "August",
            "September",
            "Oktober",
            "November",
            "Dezember",
        ];
        match self {
            Self::English => EN[month],
            Self::German => DE[month],
        }
    }

    pub(crate) const fn month_short(self, month: usize) -> &'static str {
        const EN: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        const DE: [&str; 12] = [
            "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
        ];
        match self {
            Self::English => EN[month],
            Self::German => DE[month],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DocumentLanguage;

    #[test]
    fn only_the_supported_short_list_reaches_typst() {
        assert_eq!(
            DocumentLanguage::from_code(Some(" DE ")),
            DocumentLanguage::German
        );
        assert_eq!(
            DocumentLanguage::from_code(Some("de; panic(\"no\")")),
            DocumentLanguage::English
        );
        assert_eq!(DocumentLanguage::English.stored(), None);
        assert_eq!(DocumentLanguage::German.stored(), Some("de"));
    }
}
