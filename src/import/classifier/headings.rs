//! Is this line a section heading, and which section?
//!
//! Split from `classifier.rs` by C15. The taxonomy is the data half of the
//! importer — a shipped table of the words CVs actually use for their
//! sections, in several languages — and everything here reads it: the indexes
//! built once at startup, the fuzzy match that tolerates a typo, and the shape
//! tests (shouting, title case) that say a line is a heading before anything
//! asks which one.

use serde::Deserialize;
use std::sync::OnceLock;

use super::SectionKind;
use super::entries::{get_single_date_regex, sanitize_header_line};

/// The shipped table: language → its words for each section.
pub(crate) type Taxonomy = std::collections::BTreeMap<String, LanguageCorpus>;

static TAXONOMY_TOML: &str = include_str!("../../../assets/taxonomy.toml");

static INDEXED_TAXONOMY: OnceLock<IndexedTaxonomy> = OnceLock::new();

#[derive(Debug, Deserialize, Default)]
pub(crate) struct LanguageCorpus {
    en: Option<Vec<String>>,
    ru: Option<Vec<String>>,
    de: Option<Vec<String>>,
    fr: Option<Vec<String>>,
    es: Option<Vec<String>>,
    it: Option<Vec<String>>,
    pt: Option<Vec<String>>,
    nl: Option<Vec<String>>,
    pl: Option<Vec<String>>,
}

pub(crate) struct KeywordEntry {
    clean: String,
    char_count: usize,
    family: String,
}

pub(crate) struct IndexedTaxonomy {
    exact_map: std::collections::HashMap<String, String>,
    entries: Vec<KeywordEntry>,
    /// Every word any keyword is made of — see [`every_word_names_a_section`].
    vocabulary: std::collections::HashSet<String>,
}

impl LanguageCorpus {
    pub(crate) fn all_keywords(&self) -> Vec<String> {
        let mut list = Vec::new();
        let fields = [
            &self.en, &self.ru, &self.de, &self.fr, &self.es, &self.it, &self.pt, &self.nl,
            &self.pl,
        ];
        for items in fields.into_iter().flatten() {
            list.extend(items.clone());
        }
        list
    }
}

static TAXONOMY: OnceLock<Taxonomy> = OnceLock::new();

pub(crate) fn get_taxonomy() -> &'static Taxonomy {
    TAXONOMY.get_or_init(|| {
        toml::from_str(TAXONOMY_TOML).expect("embedded taxonomy.toml must be valid TOML")
    })
}

pub(crate) fn get_indexed_taxonomy() -> &'static IndexedTaxonomy {
    INDEXED_TAXONOMY.get_or_init(|| {
        let raw_tax = get_taxonomy();
        let mut exact_map = std::collections::HashMap::new();
        let mut entries = Vec::new();

        for (family, corpus) in raw_tax {
            for kw in corpus.all_keywords() {
                let clean = kw.trim().to_lowercase();
                if clean.is_empty() {
                    continue;
                }
                let char_count = clean.chars().count();
                exact_map.insert(clean.clone(), family.clone());
                entries.push(KeywordEntry {
                    clean,
                    char_count,
                    family: family.clone(),
                });
            }
        }

        // Every individual word any keyword is built from, so a heading a
        // person composed out of two of them is still recognisable as one.
        let vocabulary = entries
            .iter()
            .flat_map(|e| e.clean.split(|c: char| !c.is_alphanumeric()))
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect();

        IndexedTaxonomy {
            exact_map,
            entries,
            vocabulary,
        }
    })
}

/// Family names that map onto a built-in section, in the order they are tried.
/// Order is fixed rather than the map's, so a heading matching two families
/// resolves the same way every run.
const BUILT_IN_FAMILIES: [(&str, SectionKind); 7] = [
    ("contact", SectionKind::Contact),
    ("work", SectionKind::Work),
    ("education", SectionKind::Education),
    ("skills", SectionKind::Skills),
    ("certificates", SectionKind::Certificates),
    ("volunteer", SectionKind::Volunteer),
    ("summary", SectionKind::Summary),
];

pub(crate) fn title_case(heading: &str) -> String {
    if !is_shouted(heading) {
        return heading.to_string();
    }
    heading
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn is_shouted(line: &str) -> bool {
    // A *cased* letter, not merely a letter. Hebrew, Arabic, Japanese, Chinese,
    // Georgian and Devanagari have no capitals at all, so "no lower-case letter
    // in it" was true of every line in them — and the rule this feeds says a
    // shouted line is a section heading even when the taxonomy has never heard
    // of it. The first line of a Hebrew CV is the person's name; read as a
    // heading, it became a section with the whole document filed under it.
    let mut has_case = false;
    for ch in line.chars() {
        if ch.is_lowercase() {
            return false;
        }
        has_case |= ch.is_uppercase();
    }
    has_case
}

/// Zero-allocation 1D Levenshtein distance with early-exit row thresholding.
pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    let n = a.chars().count();
    let m = b.chars().count();

    let diff = (n as isize - m as isize).abs();
    if diff > 2 {
        return 3;
    }
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    if n >= 64 || m >= 64 {
        return 3;
    }

    let mut a_buf = ['\0'; 64];
    let mut b_buf = ['\0'; 64];

    for (i, ch) in a.chars().enumerate() {
        a_buf[i] = ch;
    }
    for (j, ch) in b.chars().enumerate() {
        b_buf[j] = ch;
    }

    let a_chars = &a_buf[..n];
    let b_chars = &b_buf[..m];

    let mut v0 = [0usize; 65];
    let mut v1 = [0usize; 65];

    for (j, cell) in v0[..=m].iter_mut().enumerate() {
        *cell = j;
    }

    for (i, a_ch) in a_chars.iter().enumerate() {
        v1[0] = i + 1;
        let mut row_min = v1[0];

        for (j, b_ch) in b_chars.iter().enumerate() {
            let cost = if a_ch == b_ch { 0 } else { 1 };
            let val = (v1[j] + 1).min(v0[j + 1] + 1).min(v0[j] + cost);
            v1[j + 1] = val;
            if val < row_min {
                row_min = val;
            }
        }

        if row_min > 2 {
            return 3;
        }

        v0[..=m].copy_from_slice(&v1[..=m]);
    }

    v0[m]
}

/// Does the line *name* a section, taken whole?
/// Every word of `clean` is a word the taxonomy uses for some section.
///
/// The whole-line rules below cannot see a heading somebody wrote themselves
/// out of two words the corpus knows separately: `Leadership Experience` is
/// neither an entry nor within two edits of one, so a CV that renames its work
/// section that way had no Work heading at all, and its whole history went
/// wherever the section above it ended. `classify_header` reads it correctly —
/// it is *finding* the heading that failed.
///
/// Asking that **every** word count is what keeps this from being the substring
/// rule that was rejected: `Completed while working full-time` contains *work*
/// and would pass a substring test, and fails here on its first word.
pub(crate) fn every_word_names_a_section(clean: &str) -> bool {
    /// Words that join two nouns and belong to neither.
    const CONNECTORS: [&str; 6] = ["and", "&", "of", "the", "in", "amp"];

    let vocabulary = &get_indexed_taxonomy().vocabulary;
    let mut counted = 0usize;
    for word in clean.split(|c: char| !c.is_alphanumeric()) {
        if word.is_empty() || CONNECTORS.contains(&word) {
            continue;
        }
        if !vocabulary.contains(word) {
            return false;
        }
        counted += 1;
    }
    counted >= 2
}

pub(crate) fn names_a_section(clean: &str) -> bool {
    let tax = get_indexed_taxonomy();
    if tax.exact_map.contains_key(clean) {
        return true;
    }
    let clean_len = clean.chars().count();
    if clean_len < 4 {
        return false;
    }
    tax.entries.iter().any(|entry| {
        entry.char_count >= 4
            && (entry.char_count as isize - clean_len as isize).abs() <= 2
            && levenshtein(clean, &entry.clean) <= 2
    })
}

/// Taxonomy-driven section header classifier with ISO language corpus & NLP fuzzy matching.
pub(crate) fn classify_header(header: &str) -> SectionKind {
    let clean = sanitize_header_line(header);
    let tax = get_indexed_taxonomy();

    if let Some(family) = tax.exact_map.get(&clean) {
        if let Some((_, kind)) = BUILT_IN_FAMILIES.iter().find(|(name, _)| name == family) {
            return *kind;
        }
    }

    let clean_len = clean.chars().count();
    let mut best: Option<(u8, usize, &str)> = None;

    for entry in &tax.entries {
        let score = if clean == entry.clean {
            2
        } else if clean_len >= 4
            && entry.char_count >= 4
            && (entry.char_count as isize - clean_len as isize).abs() <= 2
            && levenshtein(&clean, &entry.clean) <= 2
        {
            1
        } else {
            continue;
        };
        let rank = (score, entry.char_count);
        if best.is_none_or(|(s, len, _)| rank > (s, len)) {
            best = Some((score, entry.char_count, &entry.family));
        }
    }

    if let Some((_, _, family)) = best {
        return BUILT_IN_FAMILIES
            .iter()
            .find(|(name, _)| *name == family)
            .map_or(SectionKind::Unknown, |(_, kind)| *kind);
    }

    for (family, kind) in BUILT_IN_FAMILIES {
        let family_keywords: Vec<&str> = tax
            .entries
            .iter()
            .filter(|e| e.family == family)
            .map(|e| e.clean.as_str())
            .collect();
        if matches_keywords_clean(&clean, &family_keywords) {
            return kind;
        }
    }

    SectionKind::Unknown
}

pub(crate) fn matches_keywords_clean(input: &str, keywords: &[&str]) -> bool {
    let input_len = input.chars().count();
    for &kw in keywords {
        let kw_len = kw.chars().count();
        if input == kw {
            return true;
        }
        if kw_len >= 4 && input.contains(kw) {
            return true;
        }
        if kw_len >= 4 && input_len >= 4 {
            for word in input.split_whitespace() {
                let word_len = word.chars().count();
                if word_len >= 4
                    && (word_len as isize - kw_len as isize).abs() <= 2
                    && levenshtein(word, kw) <= 2
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Heuristic check whether a line is a section header.
///
/// The shape test comes before the keyword test on purpose. `classify_header`
/// matches substrings and tolerates two typos, so `• Built a pipeline for
/// atmospheric profiles.` matched *profile* and became a Summary heading —
/// which silently moved the section boundary and swallowed every line after it.
/// A body line that reads like a keyword is far more common than a heading that
/// reads like a sentence, so a line only gets to be classified once it looks
/// like a heading at all: not a bullet, not a sentence, and short.
pub(crate) fn is_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.len() > 60 {
        return false;
    }
    if trimmed.starts_with('#') || (trimmed.starts_with("**") && trimmed.ends_with("**")) {
        return classify_header(trimmed) != SectionKind::Unknown;
    }
    if trimmed.starts_with(['•', '▪', '·', '‣', '–', '—'])
        || trimmed.ends_with('.')
        || trimmed.split_whitespace().count() > 5
    {
        return false;
    }
    // A heading matches the corpus **as a whole line**. `classify_header` is
    // free to match substrings — it is answering "which section is this
    // heading", where `WORK EXPERIENCE 2019` should still land on Work. Asking
    // it "is this a heading at all" is a different question, and the substring
    // rule answers it wrongly: `Completed while working full-time` contains
    // *work*, so a line of a CV's own prose became a Work heading and moved the
    // section boundary under it.
    let clean = sanitize_header_line(trimmed);
    if !get_single_date_regex().is_match(trimmed)
        && (names_a_section(&clean) || every_word_names_a_section(&clean))
    {
        return true;
    }
    // A heading the taxonomy has never seen is still a heading if the document
    // set it like one. Without this, `PROJECTS` was not a boundary at all and
    // three projects were appended to the last job's bullets.
    // Capitals alone are not enough: `GPA: 3.72/4.00` has three letters and all
    // of them are capital, so it read as a section and took the rest of the
    // entry with it. A heading the taxonomy has never seen has to look like a
    // *name* — letters, no figures.
    is_shouted(trimmed) && !trimmed.chars().any(|c| c.is_ascii_digit())
}
