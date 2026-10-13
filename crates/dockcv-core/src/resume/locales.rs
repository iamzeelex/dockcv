//! Data-driven localization for dates and calendars.
//!
//! Loads vocabulary from `assets/dates.toml` at compile-time via `include_str!`,
//! and exposes fast lookup for months, aliases, and "Present" markers.
//! Adding or refining language date words requires editing `assets/dates.toml`,
//! leaving Rust code clean of hardcoded language tables.

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

static DATES_TOML: &str = include_str!("../../../../assets/dates.toml");
static DATE_LOCALES: OnceLock<DateLocales> = OnceLock::new();

#[derive(Debug, Deserialize, Default)]
pub struct ExtraMonthName {
    pub month: u32,
    pub names: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct LanguageDateData {
    #[serde(default)]
    pub present: Vec<String>,
    #[serde(default)]
    pub months_long: Vec<String>,
    #[serde(default)]
    pub months_short: Vec<String>,
    #[serde(default)]
    pub extra_month_names: Vec<ExtraMonthName>,
}

pub type DateLocalesFile = BTreeMap<String, LanguageDateData>;

#[derive(Clone, Debug)]
pub struct LanguageDisplay {
    pub present_default: String,
    pub months_long: Vec<String>,
    pub months_short: Vec<String>,
}

#[derive(Debug)]
pub struct DateLocales {
    /// Normalized word -> month number (1..=12)
    month_lookup: HashMap<String, u32>,
    /// Normalized word / phrase -> true
    present_markers: HashSet<String>,
    /// Language code -> display data
    display: HashMap<String, LanguageDisplay>,
    /// All month names & abbreviations across all languages, sorted by length descending
    all_month_words_desc: Vec<String>,
    /// All present markers across all languages, sorted by length descending
    all_present_markers_desc: Vec<String>,
}

impl DateLocales {
    /// Get the global singleton date locales registry.
    pub fn global() -> &'static Self {
        DATE_LOCALES.get_or_init(Self::init)
    }

    fn init() -> Self {
        let file: DateLocalesFile = toml::from_str(DATES_TOML)
            .expect("embedded assets/dates.toml must be valid TOML");

        let mut month_lookup = HashMap::new();
        let mut present_markers = HashSet::new();
        let mut display = HashMap::new();
        let mut all_months = HashSet::new();

        for (lang, data) in file {
            let present_default = data
                .present
                .first()
                .cloned()
                .unwrap_or_else(|| "Present".to_string());

            for p in &data.present {
                let clean = p.trim().to_lowercase();
                let stripped = clean.trim_end_matches('.').to_string();
                present_markers.insert(clean.clone());
                if stripped != clean {
                    present_markers.insert(stripped.clone());
                }
                if lang == "ru" || lang == "uk" {
                    if !clean.starts_with("по ") {
                        present_markers.insert(format!("по {clean}"));
                    }
                    if !stripped.starts_with("по ") {
                        present_markers.insert(format!("по {stripped}"));
                    }
                }
            }

            for (idx, name) in data.months_long.iter().enumerate() {
                let month_num = (idx + 1) as u32;
                let norm = name.trim().to_lowercase();
                all_months.insert(norm.clone());
                month_lookup.insert(norm, month_num);
            }

            for (idx, name) in data.months_short.iter().enumerate() {
                let month_num = (idx + 1) as u32;
                let norm = name.trim().to_lowercase();
                all_months.insert(norm.clone());
                month_lookup.insert(norm, month_num);
            }

            for extra in &data.extra_month_names {
                for name in &extra.names {
                    let norm = name.trim().to_lowercase();
                    all_months.insert(norm.clone());
                    month_lookup.insert(norm, extra.month);
                }
            }

            display.insert(
                lang,
                LanguageDisplay {
                    present_default,
                    months_long: data.months_long,
                    months_short: data.months_short,
                },
            );
        }

        let mut all_month_words_desc: Vec<String> = all_months.into_iter().collect();
        all_month_words_desc.sort_by(|a, b| {
            b.chars().count().cmp(&a.chars().count()).then_with(|| a.cmp(b))
        });

        let mut all_present_markers_desc: Vec<String> = present_markers.iter().cloned().collect();
        all_present_markers_desc.sort_by(|a, b| {
            b.chars().count().cmp(&a.chars().count()).then_with(|| a.cmp(b))
        });

        Self {
            month_lookup,
            present_markers,
            display,
            all_month_words_desc,
            all_present_markers_desc,
        }
    }

    /// All registered month names and abbreviations across all locales, sorted longest first.
    pub fn all_month_words_desc(&self) -> &[String] {
        &self.all_month_words_desc
    }

    /// All registered present markers across all locales, sorted longest first.
    pub fn all_present_markers_desc(&self) -> &[String] {
        &self.all_present_markers_desc
    }

    /// Look up month number (1..=12) for a given word across all registered languages.
    pub fn month_number(&self, word: &str) -> Option<u32> {
        let clean = word
            .trim_matches(|c: char| !c.is_alphabetic())
            .to_lowercase();
        if clean.is_empty() {
            return None;
        }
        self.month_lookup.get(&clean).copied()
    }

    /// Check if the text denotes the present / current role across all registered languages.
    pub fn is_present(&self, text: &str) -> bool {
        let clean = text.trim().to_lowercase();
        let stripped = clean.trim_end_matches('.');
        self.present_markers.contains(&clean)
            || self.present_markers.contains(stripped)
    }

    /// Get the default "present" word for a given language code.
    pub fn present_word<'a>(&'a self, lang: &str) -> Option<&'a str> {
        self.display.get(lang).map(|d| d.present_default.as_str())
    }

    /// Get long month name for a given language and 0-based month index (0..=11).
    pub fn month_long<'a>(&'a self, lang: &str, month_index: usize) -> Option<&'a str> {
        self.display
            .get(lang)
            .and_then(|d| d.months_long.get(month_index))
            .map(|s| s.as_str())
    }

    /// Get short month name for a given language and 0-based month index (0..=11).
    pub fn month_short<'a>(&'a self, lang: &str, month_index: usize) -> Option<&'a str> {
        self.display
            .get(lang)
            .and_then(|d| d.months_short.get(month_index))
            .map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales_load_from_embedded_toml() {
        let locales = DateLocales::global();
        assert_eq!(locales.month_number("January"), Some(1));
        assert_eq!(locales.month_number("sept"), Some(9));
        assert_eq!(locales.month_number("März"), Some(3));
        assert_eq!(locales.month_number("августа"), Some(8));
        assert_eq!(locales.month_number("лютого"), Some(2));
        assert_eq!(locales.month_number("février"), Some(2));
        assert_eq!(locales.month_number("diciembre"), Some(12));

        assert!(locales.is_present("Present"));
        assert!(locales.is_present("heute"));
        assert!(locales.is_present("настоящее время"));
        assert!(locales.is_present("н.в."));
        assert!(locales.is_present("по н.в."));

        assert_eq!(locales.present_word("en"), Some("Present"));
        assert_eq!(locales.present_word("de"), Some("Heute"));

        assert!(locales.all_month_words_desc().contains(&"january".to_string()));
        assert!(locales.all_month_words_desc().contains(&"jan".to_string()));
        assert!(locales.all_month_words_desc().contains(&"märz".to_string()));
        assert!(locales.all_month_words_desc().contains(&"лютого".to_string()));
        assert!(locales.all_present_markers_desc().contains(&"present".to_string()));
        assert!(locales.all_present_markers_desc().contains(&"heute".to_string()));
        assert!(locales.all_present_markers_desc().contains(&"по н.в.".to_string()));
    }
}
