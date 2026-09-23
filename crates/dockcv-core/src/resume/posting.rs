//! Reading a job posting against what the vault already holds.
//!
//! **Nothing here writes a CV, and nothing here is generated.** The output is a
//! set of pointers: which words a posting uses that tell the user's own
//! versions apart, and which diary entries and library blocks speak to the ones
//! a given page does not say. Every string that reaches the screen was typed by
//! the posting's author *and* by the user — which is US-14's rule ("never
//! invent a metric") applied one level up, to the whole surface.
//!
//! ## Why frequency alone was noise
//!
//! The first pass took the words a posting repeats. That measures the posting,
//! and a posting is mostly boilerplate: `platform` twice and `Kubernetes` twice
//! score the same as `engineers` twice and `production` twice, and a list of
//! twenty-four of them is a word cloud. It told the reader nothing they could
//! act on, because most of it was not about them.
//!
//! What is cheap, local, and actually about them is **the user's own corpus**.
//! Two questions, and each has its own function:
//!
//! - [`deciding_terms`] — words the posting uses that appear in *some* of the
//!   reader's versions and not all of them. Those are the words a choice
//!   between versions turns on, which is the only question the list is asked.
//!   A word every version already carries cannot break a tie; a word none of
//!   them carries is not a tie-break either, it is the next question.
//! - [`gaps`] — words the posting uses that the vault knows *somewhere* and
//!   this particular page does not say. That is what the diary and the library
//!   are searched with.
//!
//! Both sets are grounded in text the user wrote, so neither can fill with
//! noise about somebody else's prose.
//!
//! Pure — no vault, no I/O, no window — so all of it is testable.

use crate::resume::model::{DiaryEntry, Library, SectionKind};

/// A word the posting leans on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Term {
    /// As the posting writes it, in the casing of its first appearance —
    /// `Kubernetes`, not `kubernetes`. It is shown back to the user, and a
    /// lowercased proper noun reads as a transcription error.
    pub word: String,
    /// How many times it appears. The posting says what it cares about more
    /// than once; this is what lets the surface lead with those.
    pub count: usize,
}

/// How many terms one piece of text answers, and which.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    pub matched: Vec<String>,
    pub missing: Vec<String>,
}

impl Coverage {
    pub fn total(&self) -> usize {
        self.matched.len() + self.missing.len()
    }
}

/// Something the vault holds that speaks to a term the reading does not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unused {
    pub source: UnusedSource,
    /// The text, or `None` when it must not be shown verbatim.
    ///
    /// A confidential diary entry is **never** offered to a CV in its own
    /// words (US-36), and the safest place to enforce that is here, in the
    /// type: a caller cannot print what it was not given. The entry still
    /// counts and is still pointed at — "you have something for this, go and
    /// look at it" is exactly the right amount to say about a record whose
    /// wording is the problem.
    pub text: Option<String>,
    /// Which of the posting's terms it answers.
    pub terms: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnusedSource {
    /// A win, by the date it was logged.
    Diary { date: String, confidential: bool },
    /// A block in the pool, by the section it belongs to.
    Library { section: SectionKind },
}

/// The most a read reports on.
///
/// Less load-bearing than it was: the filters above are what keep the list
/// honest, and this only stops a pathological posting from producing a wall.
const MAX_TERMS: usize = 16;

/// The shortest word that can be a term.
///
/// Three letters is where the noise lives — `own`, `end`, `via` — and the
/// technologies that are genuinely that short (`AWS`, `SQL`, `Go`) are caught
/// by the acronym rule below instead.
const MIN_LEN: usize = 4;

/// Words the posting uses that tell the reader's versions apart.
///
/// A term qualifies when the posting uses it, at least one version says it, and
/// at least one does not. That last clause is the whole difference from the
/// first pass: a word every version already carries cannot help choose between
/// them, however often the posting repeats it, and a word none of them carries
/// belongs to [`gaps`] instead.
///
/// With a single version there is nothing to tell apart, so the rule relaxes to
/// "the posting says it and the version says it" — the count then reads as how
/// much of the posting's own vocabulary the page already shares.
///
/// Ordered by how often the posting says it, ties keeping the posting's order.
pub fn deciding_terms(posting: &str, readings: &[String]) -> Vec<Term> {
    let counted = counted_terms(posting);
    let seen: Vec<Vec<String>> = readings.iter().map(|r| tokens_of(r)).collect();
    counted
        .into_iter()
        .filter(|term| {
            let says: usize = seen.iter().filter(|t| answers(t, &term.word)).count();
            match seen.len() {
                0 => false,
                1 => says == 1,
                total => says > 0 && says < total,
            }
        })
        .take(MAX_TERMS)
        .collect()
}

/// Words the posting uses that the vault knows somewhere and this page does not
/// say.
///
/// The other half, and a different question: not "which version" but "what else
/// do I have". Grounded in `vault` — every CV, the library and the diary run
/// together — so a term the user has never written about anywhere does not
/// appear, because there would be nothing to offer for it.
pub fn gaps(posting: &str, page: &str, vault: &str) -> Vec<String> {
    let known = tokens_of(vault);
    let on_page = tokens_of(page);
    counted_terms(posting)
        .into_iter()
        .filter(|term| answers(&known, &term.word) && !answers(&on_page, &term.word))
        .map(|term| term.word)
        .take(MAX_TERMS)
        .collect()
}

// `absent` lived here: the names a posting used that the vault had never
// mentioned. It read, on a real posting, "It also names Capgemini, Canada,
// Location, GCP, Equal, Opportunity, Indigenous, Choosing" — one useful item in
// eight. A capital letter does not mean a technology; job postings capitalise
// the company, the country, the equal-opportunity paragraph and the first word
// of every sentence, and no cheap test separates those from `GCP`. The other
// two signals are grounded in words the reader wrote, which is exactly what
// this one could not be, and one honest list beats two with a bad one in it.

/// Every term the posting uses, with its count, most-said first.
fn counted_terms(posting: &str) -> Vec<Term> {
    let mut seen: Vec<(String, String, usize)> = Vec::new();
    for raw in posting.split(|c: char| !(c.is_alphanumeric() || c == '+' || c == '#')) {
        let word = raw.trim_matches(|c: char| c == '+' || c == '#');
        if word.is_empty() || !is_term(word) {
            continue;
        }
        // Keyed by the stem the matcher compares with, not by the folded word.
        // Keying by the word put `process`, `processes` and `processing` in the
        // list as three terms while `answers` treated them as one — so the same
        // word was counted three times against every version.
        let key = stem(&fold(word)).unwrap_or_else(|| fold(word));
        match seen.iter_mut().find(|(k, _, _)| *k == key) {
            Some((_, _, count)) => *count += 1,
            None => seen.push((key, word.to_string(), 1)),
        }
    }
    // `sort_by_key` is stable, so equal counts keep their first-appearance
    // order — which is the posting's own.
    seen.sort_by_key(|(_, _, count)| std::cmp::Reverse(*count));
    seen.into_iter()
        .map(|(_, word, count)| Term { word, count })
        .collect()
}

/// Which of `terms` this text already answers.
pub fn coverage(terms: &[Term], text: &str) -> Coverage {
    let tokens = tokens_of(text);
    let mut out = Coverage::default();
    for term in terms {
        if answers(&tokens, &term.word) {
            out.matched.push(term.word.clone());
        } else {
            out.missing.push(term.word.clone());
        }
    }
    out
}

/// What the vault holds for the terms a reading misses.
///
/// Ordered by how many of them each answers, because a diary entry that speaks
/// to three of the gaps is a better prompt than three that each speak to one.
pub fn unused(
    missing: &[String],
    diary: &[DiaryEntry],
    library: &Library,
    already_in_cv: &str,
) -> Vec<Unused> {
    if missing.is_empty() {
        return Vec::new();
    }
    let in_cv = tokens_of(already_in_cv);
    let mut out = Vec::new();

    for entry in diary {
        let hits = hits_for(missing, &entry.text);
        // Already on the page under some wording of its own is not a gap.
        if hits.is_empty() || hits.iter().all(|t| answers(&in_cv, t)) {
            continue;
        }
        out.push(Unused {
            source: UnusedSource::Diary {
                date: entry.date.clone(),
                confidential: entry.confidential,
            },
            text: (!entry.confidential).then(|| entry.text.clone()),
            terms: hits,
        });
    }

    let mut push_block = |section: SectionKind, text: String| {
        let hits = hits_for(missing, &text);
        if hits.is_empty() {
            return;
        }
        out.push(Unused {
            source: UnusedSource::Library { section },
            text: Some(text),
            terms: hits,
        });
    };
    for block in &library.work {
        push_block(
            SectionKind::Work,
            format!(
                "{} {} {} {}",
                block.position,
                block.name,
                block.summary,
                block.highlights.join(" ")
            ),
        );
    }
    for block in &library.skills {
        push_block(
            SectionKind::Skills,
            format!("{} {}", block.name, block.keywords.join(" ")),
        );
    }
    for block in &library.certificates {
        push_block(
            SectionKind::Certificates,
            format!("{} {}", block.name, block.issuer),
        );
    }
    for block in &library.volunteer {
        push_block(
            SectionKind::Organizations,
            format!(
                "{} {} {}",
                block.position,
                block.organization,
                block.highlights.join(" ")
            ),
        );
    }

    out.sort_by_key(|u| std::cmp::Reverse(u.terms.len()));
    out
}

fn hits_for(missing: &[String], text: &str) -> Vec<String> {
    let tokens = tokens_of(text);
    missing
        .iter()
        .filter(|term| answers(&tokens, term))
        .cloned()
        .collect()
}

fn tokens_of(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '+' || c == '#'))
        .map(|w| fold(w.trim_matches(|c: char| c == '+' || c == '#')))
        .filter(|w| !w.is_empty())
        .collect()
}

/// Does this text use the term, in any of the shapes the same word takes?
///
/// Plurals fold, and one word may be the start of the other when the shorter is
/// at least [`STEM_LEN`] — which is what puts `deploy` and `deployment`, or
/// `migrate` and `migration`, together. Anything cleverer is a stemmer, and a
/// wrong stem is a match the user cannot explain when they see it.
fn answers(tokens: &[String], term: &str) -> bool {
    let want = fold(term);
    let want_stem = stem(&want);
    tokens
        .iter()
        .any(|token| *token == want || (want_stem.is_some() && stem(token) == want_stem))
}

/// The first [`STEM_LEN`] characters, when the word is at least that long.
///
/// `None` for anything shorter, which is what keeps `data` from reaching
/// `database`: a short word has to match exactly or not at all.
fn stem(word: &str) -> Option<String> {
    let head: String = word.chars().take(STEM_LEN).collect();
    (head.chars().count() == STEM_LEN).then_some(head)
}

/// How much of a word has to agree before it is the same word.
///
/// Six, and it is a blunt instrument standing in for a stemmer. Containment is
/// not enough — `migrate` is not a prefix of `migration`, they diverge at the
/// seventh character — and those two are the same word to anyone reading a CV
/// against a posting. Six letters puts `deploy`/`deployment`,
/// `migrate`/`migration` and `manage`/`managed` together.
///
/// It also puts `product` and `produce` together, which is wrong. That is the
/// price, it is visible (the term is shown beside the text that matched it, so
/// a bad match explains itself), and the alternative is a stemmer — a
/// dependency, a language to pick, and a wrong stem the user cannot see the
/// reason for.
const STEM_LEN: usize = 6;

fn fold(word: &str) -> String {
    let lower = word.to_lowercase();
    lower.strip_suffix('s').unwrap_or(&lower).to_string()
}

/// Is this word worth counting?
fn is_term(word: &str) -> bool {
    // An acronym carries a whole technology in two or three characters, and the
    // length floor would drop every one of them.
    let acronym = word.len() >= 2
        && word.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && word.chars().any(|c| c.is_ascii_uppercase());
    if acronym {
        return !is_stopword(word);
    }
    word.chars().count() >= MIN_LEN && word.chars().any(|c| c.is_alphabetic()) && !is_stopword(word)
}

/// Both the word and its folded form, so the list can hold `skill` and catch
/// `skills` without carrying every plural twice.
fn is_stopword(word: &str) -> bool {
    let lower = word.to_lowercase();
    let folded = fold(&lower);
    STOPWORDS.contains(&lower.as_str())
        || STOPWORDS.contains(&folded.as_str())
        || SECTION_WORDS.contains(&folded.as_str())
}

/// The headings DockCV prints on a CV.
///
/// `export_plain_text` emits `EDUCATION` and `CERTIFICATIONS` above the
/// sections they name, so a posting using either word matched the *chrome* of
/// the page rather than anything on it — and because a preset can hide a
/// section, those words differed between versions and scored as deciding
/// terms. They were telling the reader about DockCV's own layout.
const SECTION_WORDS: &[&str] = &[
    "profile",
    "summary",
    "work",
    "experience",
    "education",
    "skill",
    "certificate",
    "certification",
    "organization",
    "volunteer",
    "project",
    "publication",
];

/// Words a posting is made of rather than about.
///
/// English and German, because those are the languages a shipped reading can be
/// written in (C5). The list is deliberately short: over-filtering hides real
/// terms, and a stopword that slips through costs one row of a list the user is
/// reading anyway.
const STOPWORDS: &[&str] = &[
    // English
    "a", "about", "abs", "across", "all", "also", "and", "any", "are", "as", "at", "be", "been",
    "both", "build", "but", "by", "can", "for", "from", "has", "have", "help", "here", "how",
    "into", "is", "it", "its", "join", "just", "like", "look", "looking", "make", "many", "may",
    "more", "most", "much", "must", "need", "new", "not", "of", "on", "one", "only", "or",
    "other", "our", "out", "over", "role", "same", "she", "should", "so", "some", "strong",
    "such", "team", "than", "that", "the", "their", "them", "then", "there", "these", "they",
    "this", "those", "through", "to", "up", "us", "use", "using", "very", "want", "was", "we",
    "well", "what", "when", "where", "which", "while", "who", "why", "will", "with", "within",
    "work", "working", "would", "you", "your",
    // The frame a job posting is written in. `Experience with Kubernetes is
    // required` is about Kubernetes; the other four words are the sentence it
    // arrives in, and every posting has them, so they carry no signal at all
    // while crowding out terms that do.
    "ability", "experience", "experienced", "familiar", "including", "knowledge", "preferred",
    "required", "requirements", "skill", "understanding", "year",
    // German
    "aber", "alle", "allen", "als", "auch", "auf", "aus", "bei", "bist", "dass", "dein", "dem",
    "den", "der", "des", "die", "dir", "doch", "durch", "ein", "eine", "einem", "einen", "einer",
    "für", "haben", "hast", "ihr", "ist", "kann", "mehr", "mit", "nach", "nicht", "noch", "oder",
    "sehr", "sein", "sich", "sie", "sind", "über", "und", "uns", "unser", "von", "vor", "wie",
    "wir", "zum", "zur",
];

#[cfg(test)]
mod tests;
