//! Semantic classification, NLP fuzzy matching, and entity extraction for raw text blocks.

use regex::Regex;
use serde::Deserialize;
use std::sync::OnceLock;

use crate::import::layout;
use crate::import::model::{ImportedDoc, Unplaced};
use crate::import::notes::{Note, Part};
use crate::resume::model::{
    Certificate, CustomEntry, Education, NetworkProfile, Resume, ResumeDoc, SkillGroup, Volunteer,
    Work,
};

static TAXONOMY_TOML: &str = include_str!("../../assets/taxonomy.toml");
static INDEXED_TAXONOMY: OnceLock<IndexedTaxonomy> = OnceLock::new();

#[derive(Debug, Deserialize, Default)]
struct LanguageCorpus {
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

impl LanguageCorpus {
    fn all_keywords(&self) -> Vec<String> {
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

type Taxonomy = std::collections::BTreeMap<String, LanguageCorpus>;

struct KeywordEntry {
    clean: String,
    char_count: usize,
    family: String,
}

struct IndexedTaxonomy {
    exact_map: std::collections::HashMap<String, String>,
    entries: Vec<KeywordEntry>,
    /// Every word any keyword is made of — see [`every_word_names_a_section`].
    vocabulary: std::collections::HashSet<String>,
}

static TAXONOMY: OnceLock<Taxonomy> = OnceLock::new();

fn get_taxonomy() -> &'static Taxonomy {
    TAXONOMY.get_or_init(|| {
        toml::from_str(TAXONOMY_TOML).expect("embedded taxonomy.toml must be valid TOML")
    })
}

fn get_indexed_taxonomy() -> &'static IndexedTaxonomy {
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

static EMAIL_REGEX: OnceLock<Regex> = OnceLock::new();
static PHONE_REGEX: OnceLock<Regex> = OnceLock::new();
static URL_REGEX: OnceLock<Regex> = OnceLock::new();
static DATE_RANGE_REGEX: OnceLock<Regex> = OnceLock::new();
static SINGLE_DATE_REGEX: OnceLock<Regex> = OnceLock::new();

fn get_email_regex() -> &'static Regex {
    EMAIL_REGEX.get_or_init(|| Regex::new(r"(?i)[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}").unwrap())
}

fn get_phone_regex() -> &'static Regex {
    PHONE_REGEX.get_or_init(|| {
        // Two shapes, because one rule cannot cover both without letting dates
        // in.
        //
        // The second alternative is the original: no country code, so a
        // separator before the final group is what tells `415-555-0134` from
        // `2019 - 2021`. It has to stay.
        //
        // The first is new. With an explicit `+` country code there is no
        // ambiguity left to resolve — nothing writes a date range that way — so
        // the final separator can be optional, and `+49 30 123456` reads. An
        // ordinary Berlin number was falling through: the local part is six
        // digits with nothing between them, and the old pattern demanded a
        // separator there.
        //
        // `[ .-]`, not `[\s.-]`, throughout: `\s` matches a newline, so a postal
        // code and the fragment of a number on the line below joined into one
        // "phone" (`10012\n212-998`). A phone number does not wrap.
        // The `+` alternative counts *groups* rather than prescribing their
        // widths: `+45 28 44 10 92` is how a Danish number is written, and a
        // pattern that demanded a three-digit group in the middle read straight
        // past it. Nothing writes a date range with a leading `+`, so there is
        // no ambiguity left for the group widths to resolve.
        // And the groups after a `+` may be one digit wide, because national
        // numbering plans have one-digit area codes: `+353 1 555 0100` is how
        // Dublin is written and how DockCV's own plain-text export writes it,
        // and demanding two digits read straight past the whole number —
        // leaving a CV that had just been exported with no telephone number on
        // the way back in.
        // The separator class carries the marks a *typesetter* puts in a
        // number as well as the ones a keyboard does: a non-breaking hyphen
        // (U+2011) is what a considerate author writes so `555-0134` never
        // breaks across a line, and `[ .-]` stopped dead at it — `+1 (415)
        // 555‑0134` imported as `+1 (415) 555`, a number that reaches nobody.
        // The no-break and thin spaces are here for the same reason. The en
        // dash is deliberately *not*: it is what separates the two ends of a
        // date range.
        const SEP: &str = r"[ .\-\u{00a0}\u{2009}\u{202f}\u{2010}\u{2011}]";
        Regex::new(&format!(
            r"(?:\+\d{{1,3}}(?:{SEP}?\(?\d{{1,4}}\)?){{2,6}}|\(?\d{{2,4}}\)?{SEP}?\d{{3,4}}{SEP}\d{{3,4}})"
        ))
        .unwrap()
    })
}

fn get_url_regex() -> &'static Regex {
    URL_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)https?://[^\s]+|www\.[a-z0-9-]+\.[a-z]{2,}[^\s]*|github\.com/[^\s]+|linkedin\.com/in/[^\s]+",
        )
        .unwrap()
    })
}

/// Comprehensive date range regex matching English, Russian, ISO (YYYY-MM), and numeric date formats.
fn get_date_range_regex() -> &'static Regex {
    DATE_RANGE_REGEX.get_or_init(|| {
        let months = r"(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|aug(?:ust)?|sep(?:tember)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?|янв(?:арь)?|фев(?:раль)?|мар(?:т)?|апр(?:ель)?|май|июн(?:ь)?|июл(?:ь)?|авг(?:уст)?|сен(?:тябрь)?|окт(?:ябрь)?|ноя(?:брь)?|дек(?:абрь)?)";
        let year = r"(?:[0-9]{4}|[0-9]{2}XX)";
        // The full ISO date comes first, because the alternation is ordered and
        // `2019-01` would otherwise win and leave `-01` behind: `2019-01-01 –
        // 2021-01-01` was read as no range at all, and the title of every entry
        // in a CV written with ISO dates came back as `Project  -01-01 –
        // 2021-01-01`. It is one of DockCV's own date formats.
        // One separator, not a run of them. `[\s./-]+` let `06 - 2022` read as
        // a single date, and with `[0-9]{1,2}[\s./-]+{year}` in the same
        // alternation a stray digit in front of a range swallowed its year:
        // `Company Number 4` above `2019-06 - 2022-01` parsed as `4 2019` to
        // `06 - 2022`, and every job in a CV whose employer ends in a digit
        // came back with the wrong dates. A real date's parts are held together
        // by one mark, never by ` - `, which is what separates the two ends of
        // a range — see `roundtrip_tests::a_number_in_front_of_a_range`.
        let date_elem = format!(
            r"(?:(?:{months}[\s./-]*{year})|(?:{year}[\s./-][0-9]{{1,2}}[\s./-][0-9]{{1,2}})|(?:{year}[\s./-][0-9]{{1,2}})|(?:[0-9]{{1,2}}[\s./-]{year})|(?:{year}))"
        );
        let present = r"(?:present|current|till now|ongoing|настоящее время|н\.в\.|по н\.в\.|по настоящее время)";
        let pattern = format!(r"(?i)(\b{date_elem}\b)\s*(?:–|—|-|~|to|по)\s*(\b{date_elem}\b|{present})");
        Regex::new(&pattern).unwrap()
    })
}

fn get_single_date_regex() -> &'static Regex {
    // A year, and the month and day after it when they are there. Matching the
    // year alone left `-11-07` behind out of `2021-11-07`, which then read as
    // text: a certificate's issuer came back as `Company  2021-11-07`.
    SINGLE_DATE_REGEX.get_or_init(|| {
        Regex::new(r"(?i)\b(19|20)(\d{2}|XX)(?:[-/.][0-9]{1,2}(?:[-/.][0-9]{1,2})?)?\b").unwrap()
    })
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SectionKind {
    Work,
    Education,
    Skills,
    Certificates,
    Volunteer,
    Summary,
    Contact,
    Named,
    Unknown,
}

fn title_case(heading: &str) -> String {
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

fn is_shouted(line: &str) -> bool {
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
fn levenshtein(a: &str, b: &str) -> usize {
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

/// Clean markdown, zero-width spaces, or structural decoration from a header candidate.
fn sanitize_header_line(line: &str) -> String {
    let trimmed = line
        .trim()
        .trim_matches(|c: char| c.is_whitespace() || c == '\u{200b}' || c == '\u{feff}');
    let stripped = trimmed
        .trim_start_matches(['#', '*', '=', '-', '[', ']', ':', '•', '▪', '‣'])
        .trim_end_matches(['*', ':', '=', '#', '[', ']']);

    let no_num = stripped.find(' ').map_or(stripped, |idx| {
        let prefix = &stripped[..idx];
        if prefix
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ')')
        {
            stripped[idx..].trim()
        } else {
            stripped
        }
    });

    no_num.to_lowercase()
}

/// Does this line carry contact data rather than a title?
///
/// Separator-heavy, or holding an address, a number or a handle. A job title is
/// a phrase; a contact line is a list.
fn looks_like_contact_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    let separators = line.matches(['•', '|', '·']).count();
    separators >= 2
        || line.contains('@')
        || get_phone_regex().is_match(line)
        || get_url_regex().is_match(line)
        || ["street", "address", "avenue", "road", "suite", "p.o."]
            .iter()
            .any(|m| lower.contains(m))
}

/// Does this name a place of study rather than a course of study?
///
/// Templates put the degree and the school in whichever order they like — a
/// `Heading2` carrying `Bellows College` in one entry and `Doctor of Medicine
/// (MD)` in the next, with the counterpart on the line below. Position cannot
/// resolve that; the words can.
pub fn looks_like_institution(line: &str) -> bool {
    const MARKERS: [&str; 10] = [
        "university",
        "college",
        "school",
        "institute",
        "academy",
        "universit",
        "universidad",
        "hochschule",
        "politec",
        "университет",
    ];
    let lower = line.to_lowercase();
    MARKERS.iter().any(|m| lower.contains(m))
}

/// Does this name a degree?
pub fn looks_like_degree(line: &str) -> bool {
    const WORDS: [&str; 14] = [
        "bachelor",
        "master",
        "doctor",
        "phd",
        "ph.d",
        "mba",
        "diploma",
        "degree",
        "coursework",
        "b.s",
        "b.a",
        "m.s",
        "m.a",
        "бакалавр",
    ];
    let lower = line.to_lowercase();
    if WORDS.iter().any(|w| lower.contains(w)) {
        return true;
    }
    // Abbreviations stand alone as a first token: `BSc, Applied Mathematics`,
    // `MD`, `MEng`. Matched on the whole token so `made` is not a doctorate.
    let first = lower
        .split(|c: char| !c.is_alphanumeric())
        .find(|t| !t.is_empty())
        .unwrap_or_default();
    matches!(
        first,
        "bsc" | "msc" | "md" | "meng" | "beng" | "bs" | "ba" | "ms" | "ma"
    )
}

/// Take a line as contact data, if that is what it is.
fn absorb_contact(line: &str, resume: &mut Resume) -> bool {
    let mut absorbed = false;

    for url in get_url_regex().find_iter(line) {
        absorbed = true;
        take_address(trim_url_tail(url.as_str()), resume);
    }

    if get_email_regex().is_match(line) {
        absorbed = true;
    }
    if get_phone_regex().is_match(line) {
        absorbed = true;
    }

    // A contact block is usually **one line of several fields** —
    // `you@example.com | +45 28 44 10 92 | Copenhagen, Denmark`. Returning at
    // the first field found is how the phone number and the city were dropped
    // from every CV that wrote them beside the address, DockCV's own exports
    // included. So each field is read out of its own part.
    //
    // The comma is a separator here as much as the pipe is: a CV whose header
    // is set with commas writes `Copenhagen, Denmark, you@example.com, …`, and
    // splitting on the strong separators alone left that as one part and no
    // location at all. The place is then two parts wide, which is why the
    // windows below are up to two.
    let parts: Vec<&str> = line
        .split([',', '|', '·', '•', '‧'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();

    for part in &parts {
        // A personal site is written the way people write one — `vestergaard.dev`,
        // with no scheme and no `www.` — which the URL pattern above cannot see.
        // Only a part that is a single token counts, so a sentence that happens
        // to contain `Node.js` is not read as somebody's homepage.
        if !part.contains(char::is_whitespace)
            && !part.contains('@')
            && crate::resume::links::href(part).is_some()
            && !get_url_regex().is_match(part)
        {
            absorbed = true;
            take_address(part, resume);
        }
    }

    if resume.basics.location.is_empty() {
        let is_other_field = |p: &&str| {
            p.contains('@')
                || get_phone_regex().is_match(p)
                || (!p.contains(char::is_whitespace) && crate::resume::links::href(p).is_some())
        };
        // One part first, so `Berlin, Germany` on a line of its own is not
        // widened into the field after it.
        'search: for width in 1..=2 {
            for window in parts.windows(width) {
                if window.iter().any(is_other_field) {
                    continue;
                }
                let candidate = window.join(", ");
                if looks_like_place(&candidate) {
                    resume.basics.location = candidate;
                    absorbed = true;
                    break 'search;
                }
            }
        }

        // `Dublin` with nothing after it is a place too, and most CVs write the
        // city alone — but a bare word is also a name, a job title and half the
        // other things on a header line, so it counts only where a place
        // belongs: *among* the contact details rather than in front of them.
        // `A Person | person@example.com | Dublin` gives up its city and keeps
        // its name; `A Person | Senior Engineer` gives up neither.
        //
        // After the shape with a region in it, never before: `Bern,
        // Switzerland` is two parts and its first part is a bare place, so a
        // bare reading that ran first would take the city and drop the country.
        if resume.basics.location.is_empty() {
            for (at, part) in parts.iter().enumerate() {
                if is_other_field(part) || !parts[..at].iter().any(is_other_field) {
                    continue;
                }
                if looks_like_bare_place(part) {
                    resume.basics.location = (*part).to_string();
                    absorbed = true;
                    break;
                }
            }
        }
    }

    absorbed
}

/// File one address under the person's own site or under their profiles.
///
/// A recognised network is a profile, and anything else is the site — reading
/// them first-come-first-served made `github.com/…` somebody's homepage and
/// then listed their actual homepage again beneath it, so the header printed
/// the same address twice.
fn take_address(url: &str, resume: &mut Resume) {
    let network = network_of(url);
    let is_own_site = network == "Website";

    if is_own_site && resume.basics.url.is_empty() {
        resume.basics.url = url.to_string();
        return;
    }
    if resume.basics.url == url || resume.basics.profiles.iter().any(|p| p.url == url) {
        return;
    }
    resume.basics.profiles.push(NetworkProfile {
        network: network.to_string(),
        username: String::new(),
        url: url.to_string(),
    });
}

/// Punctuation that ends the sentence a URL sits in, not the URL.
///
/// `GitHub (https://github.com/nvestergaard)` yields the closing bracket to a
/// greedy `[^\s]+`, and the profile then pointed at an address with a `)` on
/// the end of it. A bracket the URL opened itself is kept.
fn trim_url_tail(url: &str) -> &str {
    let mut end = url.len();
    while let Some(last) = url[..end].chars().last() {
        let unbalanced_close =
            last == ')' && url[..end].matches('(').count() < url[..end].matches(')').count();
        if matches!(
            last,
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | ']' | '>'
        ) || unbalanced_close
        {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    &url[..end]
}

/// Does the line end in `(2023-04)` — a date in brackets, closing it?
fn ends_with_parenthesised_date(line: &str) -> bool {
    let trimmed = line.trim_end();
    let Some(rest) = trimmed.strip_suffix(')') else {
        return false;
    };
    // Its own test rather than `is_only_dates`, which reads a *range* and says
    // no to the bare `2023-04` a certificate is stamped with: a year, and
    // nothing that could be a word.
    rest.rfind('(').is_some_and(|at| {
        let inner = rest[at + 1..].trim();
        !inner.is_empty()
            && get_single_date_regex().is_match(inner)
            && !inner.chars().any(char::is_alphabetic)
    })
}

/// Is the line nothing but a date or a date range?
pub fn is_only_dates(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let stripped = get_date_range_regex().replace_all(trimmed, "");
    let stripped = get_single_date_regex().replace_all(&stripped, "");
    let mut stripped = stripped
        .to_lowercase()
        .replace("present", "")
        .replace("current", "")
        .replace("ongoing", "");
    // A month is part of a date, and `Apr 2023` printed beside a certificate is
    // as much "only a date" as `2023` is. Without this the month was left over
    // and the line read as text.
    for month in MONTH_NAMES {
        stripped = stripped.replace(month, "");
    }
    // …but a date has a figure in it. Otherwise `May` — which is also a name —
    // would be a date, and so would the bare word `present`.
    trimmed.chars().any(|c| c.is_ascii_digit())
        && stripped.chars().count() < trimmed.chars().count()
        && !stripped.chars().any(|c| c.is_alphanumeric())
}

/// Month names long and short, lowercase, longest first so `january` is
/// removed whole rather than leaving `uary` behind after `jan`.
const MONTH_NAMES: [&str; 24] = [
    "january",
    "february",
    "september",
    "november",
    "december",
    "october",
    "august",
    "march",
    "april",
    "june",
    "july",
    "may",
    "jan",
    "feb",
    "mar",
    "apr",
    "jun",
    "jul",
    "aug",
    "sep",
    "sept",
    "oct",
    "nov",
    "dec",
];

/// The three fields a certificate line carries.
///
/// Each emitter writes them differently — `Name - Issuer (2023-04) (site.org)`
/// in plain text and Markdown, `Name, Issuer 2023-04` on a page, `Name —
/// Issuer (2023-04)` in Word — and the whole line was going into the name, so
/// a CV's certifications came back as three long strings with no issuer and no
/// date between them.
fn parse_certificate(text: &str) -> Certificate {
    let mut head = text.trim();
    let mut date = String::new();
    let mut url = String::new();

    // Brackets at the end hold the date and the address, in either order, and
    // never anything else.
    while let Some(open) = head
        .strip_suffix(')')
        .and_then(|inner| inner.rfind('('))
        .filter(|_| date.is_empty() || url.is_empty())
    {
        let inner = head[open + 1..head.len() - 1].trim();
        if url.is_empty() && layout::is_lone_address(inner) {
            url = inner.to_string();
        } else if date.is_empty() && is_only_dates(inner) {
            date = inner.to_string();
        } else {
            break;
        }
        head = head[..open].trim_end();
    }

    // Word has nowhere to put a link but on the words themselves, so the DOCX
    // reader writes the target inline after the name it was hiding behind. An
    // address anywhere in a certificate line is that certificate's link.
    let without_address;
    if url.is_empty() {
        let address = head
            .split_whitespace()
            .find(|token| layout::is_lone_address(token));
        if let Some(address) = address {
            let rest = head.replace(address, " ");
            if rest.split_whitespace().count() > 0 {
                url = address.to_string();
                without_address = rest.split_whitespace().collect::<Vec<_>>().join(" ");
                head = without_address.as_str();
            }
        }
    }

    // A page prints the date after the issuer with no brackets around it.
    let (head, trailing) = split_trailing_date(head);
    if date.is_empty() {
        date = trailing.to_string();
    }

    // The issuer is written last and is one field, so the **last** separator is
    // the boundary: `AWS Solutions Architect — Associate, Amazon Web Services`
    // keeps the dash inside its own name.
    let boundary = ["—", "–", ",", " - "]
        .iter()
        .filter_map(|sep| head.rfind(sep).map(|at| (at, sep.len())))
        .max_by_key(|(at, _)| *at);
    let (name, issuer) = match boundary {
        Some((at, len)) => (head[..at].trim(), head[at + len..].trim()),
        None => (head, ""),
    };

    Certificate {
        name: name.to_string(),
        issuer: issuer.to_string(),
        date: date.into(),
        url,
    }
}

/// File a bare address under the entry it was printed beneath.
///
/// `false` when there is no open entry to put it on, or when that entry
/// already has one — in which case it is a line like any other and is read as
/// content, which is what it must be.
fn attach_entry_url(
    section: SectionKind,
    resume: &mut Resume,
    custom: &mut [(String, Vec<CustomEntry>)],
    url: &str,
) -> bool {
    let slot: Option<&mut String> = match section {
        SectionKind::Work => resume.work.last_mut().map(|w| &mut w.url),
        SectionKind::Education => resume.education.last_mut().map(|e| &mut e.url),
        SectionKind::Certificates => resume.certificates.last_mut().map(|c| &mut c.url),
        SectionKind::Volunteer => resume.volunteer.last_mut().map(|v| &mut v.url),
        SectionKind::Named => custom
            .last_mut()
            .and_then(|(_, entries)| entries.last_mut())
            .map(|e| &mut e.url),
        _ => None,
    };
    match slot {
        Some(slot) if slot.is_empty() => {
            *slot = url.to_string();
            true
        }
        _ => false,
    }
}

/// What a line says, and the date printed after it.
///
/// `Certified Kubernetes Administrator, The Linux Foundation Apr 2023` is one
/// line carrying three fields, and the date is the one that can be found from
/// the right without guessing: it is the longest tail that is nothing but a
/// date.
fn split_trailing_date(text: &str) -> (&str, &str) {
    let mut boundary = text.len();
    let mut at = text.len();
    while let Some(space) = text[..at].rfind(char::is_whitespace) {
        let tail = text[space..].trim();
        at = space;
        if tail.is_empty() {
            continue;
        }
        if is_only_dates(tail) {
            boundary = space;
        } else {
            break;
        }
    }
    (
        text[..boundary].trim().trim_end_matches(',').trim(),
        text[boundary..].trim(),
    )
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
fn every_word_names_a_section(clean: &str) -> bool {
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

/// The first telephone number in a block of text, if there is one.
///
/// The regex is this module's; the sidebar reader needs the answer and has no
/// business knowing how it is arrived at.
pub fn first_phone(text: &str) -> Option<String> {
    get_phone_regex()
        .find(text)
        .map(|m| m.as_str().trim().to_string())
}

pub fn names_a_section(clean: &str) -> bool {
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
pub fn classify_header(header: &str) -> SectionKind {
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

fn matches_keywords_clean(input: &str, keywords: &[&str]) -> bool {
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
pub fn is_section_header(line: &str) -> bool {
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

/// Clean bullet glyphs from text lines.
/// The site a URL belongs to, for the profile list. Only names sites the URL
/// itself identifies — never guesses a network from a bare domain.
fn network_of(url: &str) -> &'static str {
    let url = url.to_lowercase();
    if url.contains("linkedin.") {
        "LinkedIn"
    } else if url.contains("github.") {
        "GitHub"
    } else if url.contains("gitlab.") {
        "GitLab"
    } else if url.contains("leetcode.") {
        "LeetCode"
    } else if url.contains("behance.") {
        "Behance"
    } else if url.contains("dribbble.") {
        "Dribbble"
    } else if url.contains("twitter.com") || url.contains("x.com") {
        "X/Twitter"
    } else if url.contains("medium.com") {
        "Medium"
    } else if url.contains("stackoverflow.com") {
        "StackOverflow"
    } else if url.contains("kaggle.com") {
        "Kaggle"
    } else {
        "Website"
    }
}

/// Whether a contact-block line reads as a place — `Calgary, Canada`.
///
/// Deliberately narrow: a short line, one comma, no digits and no `@`. A CV's
/// contact block is the only place this runs, and anything it declines simply
/// stays reported rather than being filed as a location it is not.
/// A city on its own — `Dublin`, `San Francisco`, `Київ`.
///
/// Deliberately strict about shape, because it is only ever asked about a part
/// that already sits among contact details: a word or three, each of them
/// capitalised, no digits and nothing that belongs to another field.
fn looks_like_bare_place(line: &str) -> bool {
    let line = line.trim();
    let words: Vec<&str> = line.split_whitespace().collect();
    !line.is_empty()
        && line.len() <= 32
        && (1..=3).contains(&words.len())
        && !line.contains(['@', ',', ':', '/'])
        && !line.chars().any(|c| c.is_ascii_digit())
        && words
            .iter()
            .all(|w| w.chars().next().is_some_and(|c| c.is_uppercase()))
}

fn looks_like_place(line: &str) -> bool {
    let line = line.trim();
    // One comma is the `City, Region` shape. Digits are allowed — a postcode is
    // part of an address — but not a majority: `212-998-1212` has a comma-free
    // shape anyway, and a line that is mostly figures is a number, not a place.
    let digits = line.chars().filter(char::is_ascii_digit).count();
    !line.is_empty()
        && line.len() <= 48
        && line.matches(',').count() == 1
        && !line.contains('@')
        && !line.contains("://")
        && digits * 3 < line.len()
}

/// Split `Name — kw   kw` into its group name and keywords, or decline when
/// the line carries no group separator.
fn split_skill_group(line: &str) -> Option<(String, Vec<String>)> {
    // Em dash, en dash or colon — the three an exporter actually uses. A
    // comma is *not* one: `C/C++, Rust, Java` is a keyword list, and treating
    // its first item as a group name is how a skills section becomes a list
    // of one-item groups.
    let (name, rest) = line
        .split_once(" — ")
        .or_else(|| line.split_once(" – "))
        .or_else(|| line.split_once(": "))?;
    let name = name.trim();
    if name.is_empty() || name.len() > 60 {
        return None;
    }
    Some((name.to_string(), split_keywords(rest)))
}

/// Keywords from one run of text: separated by two or more spaces, or by
/// commas. Single spaces are kept, because `Model Predictive Control` is one
/// skill and not three.
fn split_keywords(text: &str) -> Vec<String> {
    text.split(&[',', ';'][..])
        .flat_map(|part| part.split("  "))
        .map(|k| k.trim().trim_start_matches(['—', '–', '-']).trim())
        .filter(|k| !k.is_empty())
        .map(|k| k.to_string())
        .collect()
}

/// Remove a page header that bled into the middle of a line.
///
/// The give-away is that the fragment is **glued** — no whitespace before it —
/// because that is what `pdf_extract` produces when a page break falls inside a
/// paragraph. In the contact block the very same name and email stand on their
/// own, so anchoring on the missing space leaves them intact.
fn strip_running_header(line: &str, fragments: &[String]) -> String {
    let mut out = line.to_string();

    // A header can also occupy a line of its own — `Jane Doe    Page 2`. Left
    // in, it opened a job called by the person's own name.
    //
    // The rule is deliberately narrow: the fragment must look like a *name*
    // (no figures), and what follows it must be a **page marker** and nothing
    // else. Dropping a line merely because it begins with the fragment deleted
    // real content — the fragment is only a guess at the name, and in a
    // document whose first line is not a name it is a line of the CV.
    for fragment in fragments {
        if fragment.is_empty()
            || fragment.chars().any(|c| c.is_ascii_digit())
            || !out.starts_with(fragment.as_str())
        {
            continue;
        }
        let rest = out[fragment.len()..].trim().to_lowercase();
        let marker = rest
            .trim_start_matches(['-', '–', '—'])
            .trim()
            .trim_start_matches("page")
            .trim_start_matches("стр.")
            .trim_start_matches("стр")
            .trim();
        if marker != rest
            && !marker.is_empty()
            && marker.chars().all(|c| c.is_ascii_digit() || c == '/')
        {
            return String::new();
        }
    }

    for fragment in fragments {
        if fragment.is_empty() {
            continue;
        }
        let glued: Vec<usize> = out
            .match_indices(fragment.as_str())
            .filter(|(at, _)| *at > 0 && !out[..*at].ends_with(char::is_whitespace))
            .map(|(at, _)| at)
            .collect();
        for at in glued.into_iter().rev() {
            out.replace_range(at..at + fragment.len(), "");
        }
    }
    out.trim().to_string()
}

fn clean_bullet(line: &str) -> &str {
    line.trim_start_matches(['•', '-', '*', '▪', '►', '–', '—', '+', ' '])
        .trim()
}

/// Classify a raw text stream into a candidate [`ImportedDoc`].
///
/// The path for formats that carry **no structure** — a PDF's text layer, a
/// plain-text file. Everything a section parser needs has to be recovered from
/// typography here, which is what [`layout::logical_lines`] does. A format that
/// reports its own structure should build the lines itself and call
/// [`classify_lines`] instead of flattening to text first: the flattening is
/// lossy and the recovery is a guess, however good.
pub fn classify_raw_text(format_name: &str, raw_text: &str) -> ImportedDoc {
    // A running page header lands in the text layer glued to the line above it
    // (`…atmospheric profiles.Marie Curiealbert@example.com`), so strip it before
    // anything else looks at a line. Blank lines are kept: the layout pass
    // below reads them.
    let email = get_email_regex()
        .find(raw_text)
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let name_fragment = raw_text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !is_section_header(l))
        .map(|l| l.split("  ").next().unwrap_or(l).trim().to_string())
        .filter(|n| n.len() >= 4 && !n.contains('@'))
        .unwrap_or_default();
    let fragments = [name_fragment, email];
    // Leading whitespace is carried through: `layout` reads it to tell an
    // indented list item from a date range at the margin, and to join a wrapped
    // item back onto the one it belongs to. `strip_running_header` trims, so
    // the indent is measured here and put back.
    let cleaned: String = raw_text
        .lines()
        .map(|l| {
            let body = l.trim_start();
            let indent = &l[..l.len() - body.len()];
            let stripped = strip_running_header(body, &fragments);
            if stripped.is_empty() {
                stripped
            } else {
                format!("{indent}{stripped}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Physical line boxes become logical lines: a bullet broken by the text
    // measure is one bullet again, and the section parsers below never have to
    // guess whether a line is a new item or the tail of the last one.
    let lines = layout::logical_lines(&cleaned, is_section_header, |l| {
        // "Does this line already carry its dates?" — asked of the line above,
        // to decide whether the next one continues it. A certificate is written
        // `Name - Issuer (2023-04)`: one date, not a range, and nothing after
        // it, so the line is finished. Read as unfinished, two certificates
        // joined into one.
        get_date_range_regex().is_match(l) || ends_with_parenthesised_date(l)
    });
    classify_lines(format_name, join_split_entry_headers(lines))
}

/// Put an entry's dates back on its title.
///
/// Templates routinely give the dates a cell, a paragraph or a column of their
/// own — styled `Dates`, styled `Heading2` with the title in a plain run beside
/// it, or simply a table with the years down the left. Split that way neither
/// half is a usable entry header: the title carries no date to place it, and
/// the dates carry no title to name them.
///
/// This lived in the DOCX engine, on the reasoning that scattering an entry
/// across cells is a fact about Word. It is not. A PDF exported from the same
/// Word template arrives with the date on its own line for exactly the same
/// reason, and a CV built as a two-column table — which is most of the Word
/// gallery — imported as two jobs that had dates and no employer, no title and
/// no bullets. Both directions are handled: the dates can precede their entry
/// or follow it.
pub fn join_split_entry_headers(lines: Vec<layout::LogicalLine>) -> Vec<layout::LogicalLine> {
    let mut out: Vec<layout::LogicalLine> = Vec::with_capacity(lines.len());
    let mut pending_dates: Option<String> = None;

    for line in lines {
        if line.kind != layout::LineKind::Heading && is_only_dates(&line.text) {
            // An entry's own address sits on a line of its own between the
            // title and the dates — both the Word and the Markdown readers put
            // it there so `attach_entry_url` can pick it up — and it is not a
            // line that can own dates. Gluing them to it made `ethz.ch 1896-10
            // - 1900-07`, which is no longer an address, so the school's link
            // was dropped and the string became an entry of its own.
            let owner = match out.last() {
                Some(last) if layout::is_lone_address(&last.text) => out.len().checked_sub(2),
                _ => out.len().checked_sub(1),
            };
            match owner.and_then(|at| out.get_mut(at)) {
                // The line above claims the dates whenever it is one that could
                // own them. That is the order DockCV's own exporter writes —
                // title, dates, bullets — and holding them for the *next* line
                // stapled them to the entry's first bullet instead, leaving the
                // entry itself undated.
                Some(prev)
                    if prev.kind != layout::LineKind::Heading
                        && prev.kind != layout::LineKind::Bullet =>
                {
                    prev.text = format!("{} {}", prev.text, line.text);
                    prev.kind = layout::LineKind::EntryHeader;
                }
                // A section heading or a list above, so nothing there can own
                // them: this template printed the dates first, and the entry is
                // on the line below.
                _ => pending_dates = Some(line.text.clone()),
            }
            continue;
        }
        match pending_dates.take() {
            // The line after a bare date is the entry that date belongs to —
            // unless it is a list item, which is content under an entry and
            // never an entry itself.
            Some(dates)
                if line.kind != layout::LineKind::Heading
                    && line.kind != layout::LineKind::Bullet =>
            {
                out.push(layout::LogicalLine::new(
                    format!("{} {}", line.text, dates),
                    layout::LineKind::EntryHeader,
                ))
            }
            // A heading or a bullet follows: the dates belonged to the entry
            // above them.
            Some(dates) => {
                if let Some(prev) = out
                    .last_mut()
                    .filter(|p| p.kind == layout::LineKind::EntryHeader)
                {
                    prev.text = format!("{} {}", prev.text, dates);
                }
                out.push(line);
            }
            None => out.push(line),
        }
    }
    out
}

/// The part of the document a contact detail may come from.
///
/// Everything above the first heading — which is the contact block by
/// definition, since a CV puts its name and how to reach you before it starts
/// saying anything — plus the body of an explicit `CONTACT` section for the
/// templates that give it one.
///
/// A document with no headings at all yields the whole thing, and that is
/// deliberate rather than an oversight: the classifier is already treating every
/// line as contact-block material in that case (`SectionKind::Unknown` runs
/// `absorb_contact` over all of them), and having the two disagree about where
/// the block ends would be worse than the degenerate case itself.
fn contact_region(lines: &[layout::LogicalLine]) -> String {
    let mut region: Vec<&str> = Vec::new();
    let mut in_contact_section = true;

    for line in lines {
        if line.kind == layout::LineKind::Heading {
            in_contact_section = classify_header(&line.text) == SectionKind::Contact;
            continue;
        }
        if in_contact_section {
            region.push(line.text.as_str());
        }
    }
    region.join("\n")
}

/// Turn logical lines into a candidate [`ImportedDoc`].
///
/// The shared half of the importer: every format ends up here, whether its
/// structure was measured out of a page (PDF) or read off the markup (DOCX).
/// What differs between formats is only how good the evidence was — which is
/// why [`layout::LineKind::EntryHeader`] exists: DOCX can state that a line
/// opens an entry, and a PDF can only infer it from a date range.
pub fn classify_lines(format_name: &str, lines: Vec<layout::LogicalLine>) -> ImportedDoc {
    let mut resume = Resume::default();
    let mut unplaced = Vec::new();

    // Contact details come from the **contact block**, not from anywhere in the
    // document. These three regexes used to run over the whole joined text and
    // take the first hit each, which on an ordinary CV meant a vendor named in
    // a bullet became the user's own website:
    //
    // ```
    // • Migrated billing to https://stripe.com and cut p99 40%
    // • Reached out on +1 415 555 0134 for the vendor escalation
    // ```
    //
    // …imported as `basics.url = "https://stripe.com"` and
    // `basics.phone = "+1 415 555 0134"` — a payment processor's site and
    // somebody else's number, on the person's own CV.
    //
    // The region is the same one `absorb_contact` works over, so the two agree:
    // everything above the first heading, plus anything under an explicit
    // CONTACT heading.
    let whole_text = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let contact_text = contact_region(&lines);
    let contact_text = contact_text.as_str();

    // The email falls back to the whole document; the phone and the website do
    // not. The asymmetry is the point.
    //
    // An address is a strong claim of identity — a CV that carries one carries
    // the author's, and a company address in a bullet is rare enough to lose to
    // the cost of dropping the user's own. A URL is the opposite: naming a
    // vendor, a repository or a client's site inside a bullet is ordinary, which
    // is exactly how `https://stripe.com` became someone's personal website. A
    // phone number sits with the URL.
    //
    // What makes the fallback earn its keep rather than undo the fix: a
    // two-column PDF interleaves its sidebar with its body (see
    // `engines::pdf`'s `a_two_column_page_is_read_in_content_stream_order`), so
    // the contact block is not above the first heading at all and a
    // region-only reading would lose the address outright.
    let email_scope = match get_email_regex().is_match(contact_text) {
        true => contact_text,
        false => whole_text.as_str(),
    };
    if let Some(mat) = get_email_regex().find(email_scope) {
        resume.basics.email = mat.as_str().to_string();
    }
    if let Some(mat) = get_phone_regex().find(contact_text) {
        resume.basics.phone = mat.as_str().to_string();
    }
    // The person's *own* site, not the first address in the block: a CV lists
    // its GitHub and its LinkedIn there too, and taking the first left the
    // Website field pointing at a profile — which the line walk below then
    // listed a second time.
    // Only an address that is nobody's profile. A GitHub or a LinkedIn is not
    // the person's website, and filing one there both mislabelled it and left
    // the real site to be listed again underneath as a profile — the header
    // then printed the same address twice. A network address is not lost by
    // this: the walk below files it under `profiles`, which is where it goes.
    if let Some(own) = get_url_regex()
        .find_iter(contact_text)
        .map(|m| trim_url_tail(m.as_str()))
        .find(|u| network_of(u) == "Website")
    {
        resume.basics.url = own.to_string();
    }

    let mut current_section = SectionKind::Unknown;
    // A CV with no section headings at all is a real template — the
    // minimalist one — and it used to import as a name, an email and a couple
    // of lines the wizard offered to adopt. Everything else was read as part
    // of the contact block and dropped, because `current_section` never left
    // `Unknown` and the arm for `Unknown` is the contact block.
    //
    // With nothing to segment on, the shape of a line is all there is: the
    // first one carrying a date range ends the contact block and opens the
    // work history. Filing a degree under Work is wrong and is *visibly*
    // wrong, where losing it is neither — so the reading is stated in a note
    // rather than performed quietly.
    let has_headings = lines.iter().any(|l| l.kind == layout::LineKind::Heading);
    let implicit_work_at = if has_headings {
        None
    } else {
        lines
            .iter()
            .position(|l| !l.is_bullet() && get_date_range_regex().is_match(&l.text))
    };

    let mut first_lines: Vec<&str> = Vec::new();
    let mut custom: Vec<(String, Vec<CustomEntry>)> = Vec::new();
    let mut seen: Vec<SectionKind> = Vec::new();

    for (idx, entry) in lines.iter().enumerate() {
        // A sub-heading is told from a sub-entry by what follows it: an entry
        // owns the bullets under it, a heading is followed by the entries it
        // names. Both are non-bullet lines, so the line alone cannot say which.
        let next_is_bullet = lines.get(idx + 1).is_some_and(|l| l.is_bullet());
        // And a list is only interrupted where a list was actually running: a
        // section that never used a bullet glyph has no sub-headings to find,
        // only prose.
        let after_bullet = idx
            .checked_sub(1)
            .and_then(|i| lines.get(i))
            .is_some_and(|l| l.is_bullet());
        // …and a line followed by nothing but dates is an entry, whatever else
        // it looks like. A heading is never dated. Without this, the second
        // degree in an Education section — printed after the first degree's
        // bullets, with its years on the line below — was read as a sub-heading
        // and became a section of its own, taking the degree out of Education.
        let next_is_dates = lines
            .get(idx + 1)
            .is_some_and(|l| l.kind != layout::LineKind::Heading && is_only_dates(&l.text));
        if Some(idx) == implicit_work_at {
            current_section = SectionKind::Work;
            seen.push(SectionKind::Work);
        }
        if entry.kind == layout::LineKind::Heading {
            current_section = classify_header(&entry.text);
            // A document never has two Work sections. When a second heading
            // classifies as one already used, the taxonomy is stretching — the
            // corpus files `projects` under work, which is right for a CV whose
            // *only* history is projects and wrong for this one, where PROJECTS
            // sits beside WORK EXPERIENCE and was being appended to the last
            // job. The first heading of a kind keeps it; a repeat becomes a
            // section of its own, under its own name.
            if current_section != SectionKind::Unknown {
                if seen.contains(&current_section) {
                    current_section = SectionKind::Unknown;
                } else {
                    seen.push(current_section);
                }
            }
            // A heading the taxonomy does not know is still a heading — the
            // shape test proved that much. Giving it a custom section keeps its
            // content addressable instead of letting it fall into whichever
            // section happened to be open, which is how PROJECTS ended up
            // inside the last job.
            if current_section == SectionKind::Unknown {
                current_section = SectionKind::Named;
                custom.push((title_case(&entry.text), Vec::new()));
            }
            continue;
        }
        let line = entry.text.as_str();

        // An entry's link is printed on its own line beneath it — on the page,
        // and in the plain-text export the same way. Read as content it became
        // an entry of its own, so a section of one certificate imported as two,
        // the second of them called `https://certificate.com`.
        if !entry.is_bullet()
            && layout::is_lone_address(line)
            && attach_entry_url(current_section, &mut resume, &mut custom, line)
        {
            continue;
        }

        match current_section {
            SectionKind::Named => {
                let Some((_, entries)) = custom.last_mut() else {
                    continue;
                };
                if entry.is_bullet() {
                    match entries.last_mut() {
                        Some(last) => last.highlights.push(line.to_string()),
                        None => entries.push(CustomEntry {
                            highlights: vec![line.to_string()],
                            ..Default::default()
                        }),
                    }
                } else if let Some(open) = entries
                    .last_mut()
                    .filter(|e| e.subtitle.is_empty() && e.highlights.is_empty())
                {
                    // The line under an entry's title, before any bullet, is
                    // the entry's organisation — `A+ Tutors, Calgary, Alberta`
                    // under `Mathematics, Physics, and Chemistry Tutor`. Each
                    // was becoming an entry of its own.
                    open.subtitle = line.to_string();
                } else {
                    // The same reader Work and Education use, rather than a
                    // second, cruder one. Splitting on the first comma cut
                    // `Mathematics, Physics, and Chemistry Tutor` in half, and
                    // removing a single year left the rest of the range behind
                    // as `–Current` in the middle of the title.
                    let header = layout::EntryHeader::parse(line, get_date_range_regex());
                    let (start, end, rest) = if header.start.is_empty() {
                        // No range: a lone year, as a project usually carries.
                        let year = get_single_date_regex()
                            .find(line)
                            .map(|m| m.as_str().to_string())
                            .unwrap_or_default();
                        (year.clone(), String::new(), line.replace(&year, ""))
                    } else {
                        (header.start.clone(), header.end.clone(), header.whole())
                    };
                    // The line is **not** split on its commas. `pymolt, Python
                    // Migration Tool` and `Mathematics, Physics, and Chemistry
                    // Tutor` are the same punctuation and different intents —
                    // one is a name and a description, the other is one job
                    // title. Guessing cut the second in half; keeping the
                    // author's line whole costs the first a split it can make
                    // itself. The subtitle is filled from the line *below*,
                    // where the organisation actually is.
                    entries.push(CustomEntry {
                        title: rest
                            .trim()
                            .trim_end_matches([',', '-', '–', '—'])
                            .trim()
                            .to_string(),
                        subtitle: String::new(),
                        start_date: start.into(),
                        end_date: end.into(),
                        ..Default::default()
                    });
                }
            }
            SectionKind::Unknown => {
                // The block above the first heading is the contact block, not
                // "the first three lines". Anything in it that *is* contact
                // data gets consumed as contact data; only genuinely
                // unrecognised text is reported as dropped.
                //
                // Before this, `first_lines` took three lines and everything
                // after went to `unplaced`, which is why a CV's own email,
                // LinkedIn and website were reported as "didn't fit any
                // section" — while the email had in fact already been picked
                // up by the regex pass above, so it was both used *and*
                // reported lost.
                if absorb_contact(line, &mut resume) {
                    // Consumed as contact data.
                } else if first_lines.len() < 3 {
                    first_lines.push(line);
                } else {
                    unplaced.push(Unplaced::line_only(line));
                }
            }
            SectionKind::Contact => {
                // Under an explicit CONTACT heading every line is contact data
                // or nothing. The name is never re-read here — it belongs at
                // the top of the document, and a second reading would overwrite
                // it with whatever the block happened to start with.
                if !absorb_contact(line, &mut resume) {
                    unplaced.push(Unplaced::line_only(line));
                }
            }
            SectionKind::Summary => {
                if resume.basics.summary.is_empty() {
                    resume.basics.summary = line.to_string();
                } else {
                    resume.basics.summary.push('\n');
                    resume.basics.summary.push_str(line);
                }
            }
            SectionKind::Work => {
                if entry.is_bullet() {
                    match resume.work.last_mut() {
                        Some(last) => last.highlights.push(line.to_string()),
                        None => resume.work.push(Work {
                            highlights: vec![line.to_string()],
                            ..Default::default()
                        }),
                    }
                    continue;
                }

                // A dated line opens an entry. The role may have been printed
                // on the line *above* it — `Backend Engineer, ML
                // Infrastructure` / `Sembly AI … Oct 2019 – Jul 2021` — in
                // which case the entry is already open and waiting for its
                // employer and dates rather than being a second one.
                let stated = entry.kind == layout::LineKind::EntryHeader;
                if stated || get_date_range_regex().is_match(line) {
                    let header = layout::EntryHeader::parse(line, get_date_range_regex());
                    // A format that *states* an entry opens here is never
                    // second-guessed. The merge below is for the inferred case,
                    // where a role was printed on the line above its employer
                    // and only the dated line proves they are one entry.
                    let awaiting = resume
                        .work
                        .last_mut()
                        .filter(|_| !stated)
                        .filter(|w| w.start_date.is_empty() && w.highlights.is_empty());
                    match awaiting {
                        Some(open) => {
                            if open.name.is_empty() {
                                open.name = header.whole();
                            }
                            open.start_date = header.start.into();
                            open.end_date = header.end.into();
                            open.location = header.location;
                        }
                        None => resume.work.push(Work {
                            position: header.lead,
                            name: header.org,
                            location: header.location,
                            start_date: header.start.into(),
                            end_date: header.end.into(),
                            ..Default::default()
                        }),
                    }
                } else if let Some(last) = resume
                    .work
                    .last_mut()
                    .filter(|w| w.name.is_empty() && !w.position.is_empty())
                    // …but only while the entry is still being opened. An
                    // employer is printed *above* the dates, never after the
                    // bullets, so once either has arrived the entry is finished
                    // and the next bare line begins the following job. Without
                    // this, a plain-text CV filed every job's title as the
                    // previous job's employer and lost one entry per job.
                    .filter(|w| w.start_date.is_empty() && w.highlights.is_empty())
                {
                    // The employer, printed under the job title. DOCX templates
                    // scatter an entry across cells this way, and each stray
                    // line was becoming a job of its own.
                    last.name = line.to_string();
                } else if let Some(last) = resume
                    .work
                    .last_mut()
                    .filter(|w| !w.start_date.is_empty() && w.highlights.is_empty())
                {
                    // Dated entry, bullets not started yet: this is the entry's
                    // own blurb, or the link that follows it.
                    if last.summary.is_empty() {
                        last.summary = line.to_string();
                    } else {
                        last.summary.push(' ');
                        last.summary.push_str(line);
                    }
                } else {
                    // A role printed on its own line, waiting for the employer
                    // and dates underneath. Kept whole: `Backend Engineer, ML
                    // Infrastructure` is one job title, and the comma in it
                    // separates nothing.
                    resume.work.push(Work {
                        position: line.to_string(),
                        ..Default::default()
                    });
                }
            }
            SectionKind::Education => {
                // Same three shapes as Work, and for the same reason: the
                // degree is often printed above the university, so a dated line
                // completes the entry opened by the line before it rather than
                // starting a second one. Reading each line as its own entry is
                // why a CV with two degrees imported as one.
                let stated = entry.kind == layout::LineKind::EntryHeader;
                if stated || get_date_range_regex().is_match(line) {
                    let header = layout::EntryHeader::parse(line, get_date_range_regex());
                    let awaiting = resume
                        .education
                        .last_mut()
                        .filter(|_| !stated)
                        .filter(|e| e.start_date.is_empty() && e.highlights.is_empty());
                    match awaiting {
                        Some(open) => {
                            open.institution = header.whole();
                            open.start_date = header.start.into();
                            open.end_date = header.end.into();
                        }
                        // Which half of the entry this header carries is
                        // decided by the words, not by the position. Templates
                        // print the school above the degree and the degree
                        // above the school with equal enthusiasm — one marked
                        // `Bellows College`, the next `Doctor of Medicine
                        // (MD)`, both in the same slot.
                        None => {
                            let whole = header.whole();
                            let (study_type, institution) = if header.org.is_empty() {
                                if looks_like_degree(&whole) {
                                    (whole, String::new())
                                } else {
                                    (String::new(), whole)
                                }
                            } else if looks_like_institution(&header.org) {
                                (header.lead.clone(), header.org.clone())
                            } else {
                                // `Master of Engineering, Petroleum Engineering`
                                // — the second half is the field, not the
                                // school. Kept together; the institution comes
                                // from the line below.
                                (whole, String::new())
                            };
                            resume.education.push(Education {
                                study_type,
                                institution,
                                start_date: header.start.into(),
                                end_date: header.end.into(),
                                ..Default::default()
                            });
                        }
                    }
                } else if !entry.is_bullet() && after_bullet && !next_is_bullet && !next_is_dates {
                    // A non-bullet line arriving after the bullet list has
                    // started is not another bullet — the document stopped
                    // listing and started naming. Followed by more names rather
                    // than by bullets, it is a **sub-heading**: `Graduate
                    // Projects` above three projects, inside a degree.
                    //
                    // It becomes a section of its own (D-9), which is what the
                    // page means and the only shape the model has for it.
                    // Flattened into the bullet list it read as an achievement
                    // of the degree, which is worse than being in the wrong
                    // place: it is a claim the CV never made.
                    current_section = SectionKind::Named;
                    custom.push((title_case(line), Vec::new()));
                } else if let Some(last) = resume.education.last_mut() {
                    // The counterpart the header did not carry, if this is it.
                    // Otherwise coursework, thesis, honours — not gated on the
                    // entry being dated, since a DOCX template states where an
                    // entry begins and may carry no date at all.
                    let text = layout::without_bullet(line).to_string();
                    if last.institution.is_empty() && looks_like_institution(&text) {
                        last.institution = text;
                    } else if last.study_type.is_empty() && looks_like_degree(&text) {
                        last.study_type = text;
                    } else {
                        last.highlights.push(text);
                    }
                } else {
                    resume.education.push(Education {
                        study_type: layout::without_bullet(line).to_string(),
                        ..Default::default()
                    });
                }
            }
            SectionKind::Skills => {
                let bullet = clean_bullet(line);
                // A group is `Name — kw   kw   kw`; a line with no separator
                // is the **wrap** of the group above it, not a new group.
                //
                // PDF extraction breaks long skill rows across lines, and
                // treating each fragment as its own group turned six groups
                // into thirty-one — a number visibly absurd on the review
                // screen, and one that would have shipped into the document.
                match split_skill_group(bullet) {
                    Some((name, keywords)) => resume.skills.push(SkillGroup { name, keywords }),
                    None => match resume.skills.last_mut() {
                        Some(group) => group.keywords.extend(split_keywords(bullet)),
                        // Nothing to continue: the section's first line had no
                        // separator, so it is a bare list of skills.
                        None => resume.skills.push(SkillGroup {
                            name: String::new(),
                            keywords: split_keywords(bullet),
                        }),
                    },
                }
            }
            SectionKind::Certificates => {
                resume
                    .certificates
                    .push(parse_certificate(clean_bullet(line)));
            }
            SectionKind::Volunteer => {
                // The same three shapes as Work, because it is the same shape
                // of thing: a role at an organisation, over a period, with
                // bullets under it. This branch used to push a new entry for
                // every line and put the whole line in `position`, so a section
                // of two roles imported as six roles with no dates, no
                // organisation and their bullets promoted to entries of their
                // own.
                if entry.is_bullet() {
                    match resume.volunteer.last_mut() {
                        Some(last) => last.highlights.push(line.to_string()),
                        None => resume.volunteer.push(Volunteer {
                            highlights: vec![line.to_string()],
                            ..Default::default()
                        }),
                    }
                    continue;
                }

                let stated = entry.kind == layout::LineKind::EntryHeader;
                if stated || get_date_range_regex().is_match(line) {
                    let header = layout::EntryHeader::parse(line, get_date_range_regex());
                    let awaiting = resume
                        .volunteer
                        .last_mut()
                        .filter(|_| !stated)
                        .filter(|v| v.start_date.is_empty() && v.highlights.is_empty());
                    match awaiting {
                        Some(open) => {
                            if open.organization.is_empty() {
                                open.organization = header.whole();
                            }
                            open.start_date = header.start.into();
                            open.end_date = header.end.into();
                        }
                        None => resume.volunteer.push(Volunteer {
                            position: header.lead,
                            organization: header.org,
                            start_date: header.start.into(),
                            end_date: header.end.into(),
                            ..Default::default()
                        }),
                    }
                } else if let Some(last) = resume
                    .volunteer
                    .last_mut()
                    .filter(|v| v.organization.is_empty() && !v.position.is_empty())
                    .filter(|v| v.start_date.is_empty() && v.highlights.is_empty())
                {
                    last.organization = line.to_string();
                } else {
                    resume.volunteer.push(Volunteer {
                        position: clean_bullet(line).to_string(),
                        ..Default::default()
                    });
                }
            }
        }
    }

    // Name inference from initial lines if empty
    if resume.basics.name.is_empty() && !first_lines.is_empty() {
        // Exporters commonly put the name and the professional title on one
        // line, separated by a run of spaces rather than punctuation —
        // `Marie Curie  Systems & Data Engineer`. Splitting on that run
        // recovers both; without it the title became part of the name and the
        // person's own CV greeted them with it fused.
        match first_lines[0].split_once("  ") {
            Some((name, title)) if !title.trim().is_empty() => {
                resume.basics.name = name.trim().to_string();
                resume.basics.label = title.trim().to_string();
            }
            _ => resume.basics.name = first_lines[0].to_string(),
        }
        // The second line is the professional title *unless* it is contact
        // data. One template puts the whole address there, and it arrived as
        // somebody's job title: `Home or Campus Street Address • City, State
        // Zip • • phone number`.
        if resume.basics.label.is_empty() {
            if let Some(second) = first_lines.get(1).filter(|l| !looks_like_contact_line(l)) {
                resume.basics.label = second.to_string();
            }
        }
    }

    // The paragraph under the contact block, when the CV gives it no heading of
    // its own. It was collected into `first_lines` and then dropped on the
    // floor: every CV written the way DockCV writes one — name, title, contacts,
    // then the summary — imported with no summary at all.
    if resume.basics.summary.is_empty() {
        // Eight words was the old test, and it was a test about English. A
        // summary in Ukrainian says the same thing in seven — «Інженерка з
        // восьмирічним досвідом у розподілених системах.» — and so does a short
        // one in English, so both were dropped on the floor while the same CV
        // in longer words came through. What a summary *is* travels better than
        // how many words it takes: it is a sentence, and it ends like one.
        let is_prose = |l: &&&str| {
            !looks_like_contact_line(l)
                && (l.split_whitespace().count() >= 8
                    || (l.chars().count() >= 30
                        && l.trim_end().ends_with(['.', '!', '?', '。', '！', '？'])))
        };
        if let Some(prose) = first_lines.iter().skip(1).find(is_prose) {
            resume.basics.summary = (*prose).to_string();
            if resume.basics.label == **prose {
                resume.basics.label.clear();
            }
        }
    }

    let mut doc = ResumeDoc::from_resume(resume, "Base");

    // Sections the taxonomy has no shape for become custom sections (D-9) —
    // the extension point that already exists, rather than a seventh built-in
    // or, as before, silent absorption into whatever section was last open.
    for (title, entries) in custom {
        if entries.is_empty() {
            continue;
        }
        let id = doc.add_custom_section(title);
        if let Some(section) = doc.custom_section_mut(id) {
            *section.content.active_mut() = entries;
        }
    }

    let mut imported = ImportedDoc::new(format_name, doc);

    // The one thing a result cannot say: whether a section was *supposed* to
    // have content. `seen` is the list of headings this document actually
    // carried, so a heading that produced nothing is a defect, while a section
    // the CV never had is just a CV without one.
    for kind in &seen {
        let part = match kind {
            SectionKind::Work => Part::Work,
            SectionKind::Education => Part::Education,
            SectionKind::Skills => Part::Skills,
            SectionKind::Certificates => Part::Certificates,
            _ => continue,
        };
        let empty = match kind {
            SectionKind::Work => imported.doc.work.active().is_empty(),
            SectionKind::Education => imported.doc.education.active().is_empty(),
            SectionKind::Skills => imported.doc.skills.active().is_empty(),
            SectionKind::Certificates => imported.doc.certificates.active().is_empty(),
            _ => false,
        };
        if empty {
            imported.note(part, Note::Empty);
        }
    }

    if implicit_work_at.is_some() {
        let entries = imported.doc.work.active().len();
        if entries > 0 {
            imported.note(Part::Work, Note::ReadWithoutHeadings { entries });
        }
    }

    // Everything else is derivable from the document, and therefore the same
    // for every engine — which is what the old per-engine key-writing was not.
    imported.observe();
    imported.unplaced = unplaced;
    imported
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A phone number with a country code and no separator inside the local
    /// part — an ordinary Berlin number — used to fall through: the pattern
    /// demanded a separator before the final group, which is what tells
    /// `415-555-0134` from `2019 - 2021`. With an explicit `+` there is no such
    /// ambiguity, so that shape gets its own alternative.
    #[test]
    fn a_country_code_makes_the_last_separator_optional() {
        let phone = get_phone_regex();
        for number in [
            "+49 30 123456",
            "+1 415 555 0134",
            "+44 20 7946 0958",
            "+380 44 123 4567",
            "020 7946 0958",
            "415-555-0134",
            "(415) 555-0134",
        ] {
            assert!(phone.is_match(number), "should read as a phone: {number}");
        }
    }

    /// …and the guard that alternative could have broken: a date range is not a
    /// phone number, whichever way it is written.
    #[test]
    fn a_date_range_is_still_not_a_phone_number() {
        let phone = get_phone_regex();
        for dates in [
            "2019 - 2021",
            "2019 – 2021",
            "Jan 2021 - Mar 2023",
            "1999-2003",
            "2014 2018",
            "2021.01 - 2024.06",
        ] {
            assert!(!phone.is_match(dates), "read as a phone number: {dates}");
        }
    }

    /// I-09, measured. These three regexes used to run over the whole document
    /// and take the first hit each, so a vendor named in a bullet became the
    /// person's own website and a number in a bullet became their phone.
    #[test]
    fn a_url_in_a_bullet_is_not_the_persons_own_website() {
        let raw = "Albert Einstein\n\
                   Staff Engineer\n\
                   s@example.com\n\
                   \n\
                   EXPERIENCE\n\
                   Staff Engineer, Acme  Jan 2021 – Present\n\
                   • Migrated billing to https://stripe.com and cut p99 40%\n\
                   • Reached out on +1 415 555 0134 for the vendor escalation\n";
        let basics = classify_raw_text("PDF", raw).doc.profile.active().clone();

        assert_eq!(
            basics.email, "s@example.com",
            "the contact block still reads"
        );
        assert_eq!(
            basics.url, "",
            "a vendor's site is not the user's: {:?}",
            basics.url
        );
        assert_eq!(
            basics.phone, "",
            "someone else's number: {:?}",
            basics.phone
        );
    }

    /// The exception the asymmetry buys. A two-column PDF interleaves its
    /// sidebar with its body, so the contact block is not above the first
    /// heading and a region-only reading would drop the address. An email is a
    /// strong enough claim of identity to take from anywhere; a URL is not.
    #[test]
    fn an_email_below_the_first_heading_is_still_the_persons_own() {
        let raw = "Albert Einstein\n\
                   EXPERIENCE\n\
                   s@example.com\n\
                   Staff Engineer, Acme  Jan 2021 – Present\n\
                   • Shipped it on https://vendor.example\n";
        let basics = classify_raw_text("PDF", raw).doc.profile.active().clone();

        assert_eq!(basics.email, "s@example.com");
        assert_eq!(
            basics.url, "",
            "a vendor's site is still not the user's: {:?}",
            basics.url
        );
    }

    /// …and the block itself is still read, which is the half that must not
    /// regress: restricting the region would be worthless if it also lost the
    /// details it was protecting.
    #[test]
    fn the_contact_block_still_gives_up_all_three() {
        let raw = "Albert Einstein\n\
                   s@example.com · +49 30 555 0134 · https://einstein.example\n\
                   \n\
                   EXPERIENCE\n\
                   Staff Engineer, Acme  Jan 2021 – Present\n";
        let basics = classify_raw_text("PDF", raw).doc.profile.active().clone();

        assert_eq!(basics.email, "s@example.com");
        assert_eq!(basics.phone, "+49 30 555 0134");
        assert_eq!(basics.url, "https://einstein.example");
    }

    /// A template that gives contact details their own heading puts them below
    /// the first one, so the region has to follow the heading rather than stop
    /// at it.
    #[test]
    fn an_explicit_contact_section_is_part_of_the_region() {
        let raw = "Albert Einstein\n\
                   \n\
                   EXPERIENCE\n\
                   Staff Engineer, Acme  Jan 2021 – Present\n\
                   • Shipped it on https://vendor.example\n\
                   \n\
                   CONTACT\n\
                   https://einstein.example\n";
        let basics = classify_raw_text("PDF", raw).doc.profile.active().clone();
        assert_eq!(basics.url, "https://einstein.example");
    }

    /// A page break inside a paragraph glues the running header onto the line
    /// above it, so the bullet arrived as
    /// `…atmospheric profiles.Marie Curiealbert@example.com`. The same name and email
    /// stand alone in the contact block and must survive untouched.
    #[test]
    fn a_running_page_header_is_stripped_out_of_the_bullet_it_bled_into() {
        let raw = "Marie Curie  Systems & Data Engineer\n\
                   albert@example.com\n\
                   \n\
                   EXPERIENCE\n\
                   Software Developer\n\
                   • Built a pipeline for atmospheric profiles.Marie Curiealbert@example.com\n\
                   • Shipped the ingest service.\n";
        let imported = classify_raw_text("PDF", raw);
        let basics = imported.doc.profile.active();

        assert_eq!(basics.name, "Marie Curie");
        assert_eq!(basics.email, "albert@example.com");

        let work = imported.doc.work.active();
        let bullets: Vec<&str> = work
            .iter()
            .flat_map(|w| w.highlights.iter().map(|h| h.as_str()))
            .collect();
        assert!(
            bullets.contains(&"Built a pipeline for atmospheric profiles."),
            "header still glued to the bullet: {bullets:?}"
        );
    }

    /// Two degrees, written the two ways an exporter writes them: the degree
    /// above the university, and both on one line. Reading each line as its own
    /// entry imported this as a single, half-empty education entry.
    #[test]
    fn both_shapes_of_an_education_entry_are_read() {
        let raw = "EDUCATION\n\n\
                   Graduate coursework — MSc in Modeling for Science and Engineering\n\
                   Universitat Autònoma de Barcelona (UAB) 2025 – 2026  |  Barcelona, Spain\n\
                   \n\
                   Mathematical modeling, dynamical systems and complexity, HPC\n\
                   \n\
                   BSc, Applied Mathematics and Computing, Odesa I.I.Mechnikov National University 2019 – 2023\n\
                   Numerical methods, optimization and control theory, machine learning\n\
                   Completed while working full-time\n";
        let edu = classify_raw_text("PDF", raw)
            .doc
            .education
            .active()
            .to_vec();

        assert_eq!(edu.len(), 2, "{edu:#?}");
        assert!(edu[0].study_type.starts_with("Graduate coursework"));
        assert_eq!(
            edu[0].institution,
            "Universitat Autònoma de Barcelona (UAB)"
        );
        assert_eq!(edu[0].start_date.text, "2025");
        assert_eq!(
            edu[1].institution,
            "Odesa I.I.Mechnikov National University"
        );
        // The coursework line and the note under it are kept, not dropped.
        assert_eq!(edu[1].highlights.len(), 2, "{:#?}", edu[1]);
    }

    /// A family the taxonomy knows but the model has no shape for keeps its own
    /// name. `PROJECTS` used to be a synonym for *work*, so three projects were
    /// appended to somebody's last employer.
    #[test]
    fn a_recognised_family_with_no_built_in_shape_keeps_its_own_name() {
        let raw = "WORK EXPERIENCE\n\n\
                   Software Developer, GE Vernova Aug 2024 – Dec 2025  |  Barcelona, Spain\n\
                   \n\
                   •Built the observability stack.\n\
                   \n\
                   PROJECTS\n\
                   \n\
                   pymolt, Python Migration Tool 2026\n\
                   \n\
                   •Runtime tracing tool built on sys.setprofile.\n";
        let doc = classify_raw_text("PDF", raw).doc;

        assert_eq!(doc.work.active().len(), 1);
        assert_eq!(
            doc.work.active()[0].highlights.len(),
            1,
            "projects leaked into the job"
        );
        assert_eq!(doc.custom_sections.len(), 1, "{:#?}", doc.custom_sections);
        let projects = &doc.custom_sections[0];
        assert_eq!(projects.title, "Projects");
        // The author's line is kept whole — see the branch's comment: the same
        // punctuation carries a name-and-description in one CV and a single job
        // title in the next, and splitting cut the second in half.
        assert_eq!(
            projects.content.active()[0].title,
            "pymolt, Python Migration Tool"
        );
        assert_eq!(projects.content.active()[0].start_date.text, "2026");
    }

    /// Interests and Languages are headings a CV really uses and the model has
    /// no field for. Before the taxonomy carried them they were not headings at
    /// all, so their content was absorbed by whatever section was open — in a
    /// real template, into Education.
    #[test]
    fn headings_the_model_has_no_field_for_are_still_sections() {
        for heading in [
            "Interests",
            "Languages",
            "Publications",
            "Awards",
            "References",
        ] {
            assert!(is_section_header(heading), "{heading} should be a heading");
            assert_eq!(
                classify_header(heading),
                SectionKind::Unknown,
                "{heading} has no built-in shape and must not be forced into one"
            );
        }
    }

    /// Two headings of the same built-in kind: the taxonomy is stretching, and
    /// the second is something else under a name it happens to share.
    #[test]
    fn a_second_section_of_the_same_kind_becomes_its_own_section() {
        let raw = "WORK EXPERIENCE\n\n\
                   Software Developer, GE Vernova Aug 2024 – Dec 2025\n\
                   \n\
                   •Built the observability stack.\n\
                   \n\
                   RELEVANT EXPERIENCE\n\
                   \n\
                   Volunteer Mentor, Code Club Jan 2020 – Dec 2021\n";
        let doc = classify_raw_text("PDF", raw).doc;

        assert_eq!(doc.work.active().len(), 1, "{:#?}", doc.work.active());
        assert_eq!(doc.custom_sections.len(), 1, "{:#?}", doc.custom_sections);
        assert_eq!(doc.custom_sections[0].title, "Relevant Experience");
    }

    /// `Activities and Interests` contains *activities*, which the volunteer
    /// corpus lists — so a heading whose own name is in the taxonomy verbatim
    /// was rendering as ORGANIZATIONS.
    #[test]
    fn a_heading_that_is_a_name_outranks_one_that_merely_contains_a_name() {
        assert_eq!(
            classify_header("Activities and Interests"),
            SectionKind::Unknown
        );
        assert_eq!(classify_header("Activities"), SectionKind::Volunteer);
        // The substring path still reads a heading that carries extra words.
        assert_eq!(classify_header("Skills & Abilities"), SectionKind::Skills);
        assert_eq!(
            classify_header("WORK EXPERIENCE 2019–2024"),
            SectionKind::Work
        );
    }

    /// A name in two families is a tie the ranking cannot break, and the
    /// winner then depends on map order — which is how `Activities` resolved
    /// differently from `Activities and Interests` for no stated reason.
    #[test]
    fn no_name_belongs_to_two_families() {
        let mut seen: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
        for (family, corpus) in get_taxonomy() {
            for keyword in corpus.all_keywords() {
                let keyword = keyword.to_lowercase();
                if let Some(other) = seen.insert(keyword.clone(), family) {
                    assert_eq!(other, family, "`{keyword}` is in both {other} and {family}");
                }
            }
        }
    }

    /// A title the author already cased is kept; only a shouted heading is
    /// re-cased, or `Activities and Interests` comes back as `... And ...`.
    #[test]
    fn only_a_shouted_heading_is_recased() {
        assert_eq!(title_case("PROJECTS"), "Projects");
        assert_eq!(
            title_case("Activities and Interests"),
            "Activities and Interests"
        );
    }

    /// Templates print the school above the degree and the degree above the
    /// school with equal enthusiasm, so position cannot say which is which. The
    /// words can: one CV filed `Petroleum Engineering` as the university and
    /// pushed `University of Calgary` into the bullet list under it.
    #[test]
    fn a_degree_and_a_school_are_told_apart_by_their_words_not_their_order() {
        let raw = "EDUCATION\n\n\
                   Master of Engineering, Petroleum Engineering 2009 – 2011\n\
                   University of Calgary, Alberta\n\
                   GPA: 3.72/4.00\n";
        let edu = classify_raw_text("PDF", raw)
            .doc
            .education
            .active()
            .to_vec();

        assert_eq!(edu.len(), 1, "{edu:#?}");
        // The field of study stays with the degree — it is not a school.
        assert_eq!(
            edu[0].study_type,
            "Master of Engineering, Petroleum Engineering"
        );
        assert_eq!(edu[0].institution, "University of Calgary, Alberta");
        assert_eq!(edu[0].highlights, vec!["GPA: 3.72/4.00"]);
    }

    #[test]
    fn the_two_halves_of_an_education_entry_are_recognised_either_way_round() {
        assert!(looks_like_institution("Bellows College"));
        assert!(looks_like_institution("Universitat Autònoma de Barcelona"));
        assert!(!looks_like_institution("Doctor of Medicine (MD)"));

        assert!(looks_like_degree("Doctor of Medicine (MD)"));
        assert!(looks_like_degree("BSc, Applied Mathematics"));
        assert!(looks_like_degree("Graduate coursework — MSc in Modeling"));
        assert!(!looks_like_degree("Bellows College"));
        // A word that merely contains an abbreviation is not a degree.
        assert!(!looks_like_degree("Madeleine Consulting"));
    }

    /// `Graduate Projects` sits inside a degree, above three projects that each
    /// own their bullets. Flattened into the degree's bullet list it read as an
    /// achievement of that degree — a claim the CV never made.
    #[test]
    fn a_sub_heading_inside_a_section_becomes_a_section_of_its_own() {
        let raw = "EDUCATION\n\n\
                   Bachelor of Science, Chemical Engineering 2005 – 2008\n\
                   Prestigious University, Iran\n\
                   •  Thesis Project: Pinch Technology\n\
                   Graduate Projects\n\
                   Shell Scotford Upgrader Expansion\n\
                   •  Evaluated Upgrader Alley\n\
                   Industrial Water Treatment\n\
                   •  Evaluated processes of treating waste water\n";
        let doc = classify_raw_text("PDF", raw).doc;
        let edu = doc.education.active();

        assert_eq!(edu.len(), 1, "{edu:#?}");
        assert_eq!(
            edu[0].highlights.len(),
            1,
            "the projects leaked into the degree"
        );
        assert_eq!(doc.custom_sections.len(), 1, "{:#?}", doc.custom_sections);
        let projects = &doc.custom_sections[0];
        assert_eq!(projects.title, "Graduate Projects");
        let entries = projects.content.active();
        assert_eq!(entries.len(), 2, "{entries:#?}");
        assert_eq!(entries[0].title, "Shell Scotford Upgrader Expansion");
        assert_eq!(entries[0].highlights.len(), 1);
    }

    /// The rule must not fire in a section that never used a bullet glyph:
    /// there is no list to interrupt, only prose.
    #[test]
    fn prose_under_an_entry_is_not_mistaken_for_a_sub_heading() {
        let raw = "EDUCATION\n\n\
                   BSc, Applied Mathematics, Odesa National University 2019 – 2023\n\
                   Numerical methods, optimization and control theory\n\
                   Completed while working full-time\n";
        let doc = classify_raw_text("PDF", raw).doc;

        assert!(doc.custom_sections.is_empty(), "{:#?}", doc.custom_sections);
        assert_eq!(doc.education.active()[0].highlights.len(), 2);
    }

    /// A `CONTACT` heading is not a section of entries: its lines are the
    /// profile's fields. They were becoming the three entries of a custom
    /// section called Contact while the profile they belong to stayed empty.
    #[test]
    fn a_contact_section_fills_the_profile_rather_than_making_a_section() {
        let raw = "Dr. Amelia Evelyn\n\
                   Cardiothoracic Surgeon\n\
                   \n\
                   PROFILE\n\
                   An accomplished surgeon.\n\
                   \n\
                   CONTACT\n\
                   \n\
                   someone@example.com\n\
                   (201) 555-0101\n\
                   https://www.excellentwebsite.com\n";
        let imported = classify_raw_text("DOCX", raw);
        let basics = imported.doc.profile.active();

        assert_eq!(
            basics.name, "Dr. Amelia Evelyn",
            "the name must survive the block"
        );
        assert_eq!(basics.email, "someone@example.com");
        assert!(!basics.phone.is_empty(), "phone should be read");
        assert!(
            basics.url.contains("excellentwebsite"),
            "got {:?}",
            basics.url
        );
        assert!(
            imported.doc.custom_sections.is_empty(),
            "contact is not a section: {:#?}",
            imported.doc.custom_sections
        );
        assert!(imported.unplaced.is_empty(), "{:?}", imported.unplaced);
    }

    /// A line of the CV's own prose containing the word *work* is not a Work
    /// heading. The substring rule that made it one moved the section boundary
    /// and swallowed everything under it.
    #[test]
    fn prose_containing_a_section_keyword_is_not_a_heading() {
        assert!(!is_section_header("Completed while working full-time"));
        assert!(!is_section_header("pymolt, Python Migration Tool 2026"));
        // Capitals with figures in them are data, not a heading.
        assert!(!is_section_header("GPA: 3.72/4.00"));
        assert!(!is_section_header("MSC 2019"));
        assert!(is_section_header("WORK EXPERIENCE"));
        assert!(is_section_header("Education"));
        assert!(is_section_header("PROJECTS"));
    }

    /// A running header on a line of its own — `Jane Doe    Page 2` — opened a
    /// job called by the person's own name. The glued case was already handled;
    /// this is the same header, laid out differently.
    #[test]
    fn a_running_header_on_its_own_line_is_dropped() {
        let raw = "Jane Doe\n\
                   jdoe@ucalgary.ca\n\
                   \n\
                   EXPERIENCE\n\
                   Co-op Engineering Student, National Petrochemical 2007 – 2008\n\
                   • Part of a five-member engineering team.\n\
                   \n\
                   Jane Doe                      Page 2\n\
                   • Continued on the second page.\n";
        let doc = classify_raw_text("PDF", raw).doc;
        let work = doc.work.active();

        assert_eq!(work.len(), 1, "{work:#?}");
        assert_eq!(work[0].position, "Co-op Engineering Student");
        assert_eq!(work[0].highlights.len(), 2);
    }

    /// A contact block taken from a real FlowCV export. Each assertion here
    /// is a defect that shipped and was visible on the review screen.
    #[test]
    fn a_real_contact_block_is_read_rather_than_reported_as_lost() {
        let raw = "Marie Curie  Systems & Data Engineer\n\
                   For legal purpose: Albert Einstein\n\
                   Calgary, Canada\n\
                   albert@example.com\n\
                   https://www.linkedin.com/in/aeinstein\n\
                   https://www.example.com/\n\
                   \n\
                   PROFILE\n\
                   Systems & Data Engineer with an applied mathematics background.\n";
        let imported = classify_raw_text("PDF", raw);
        let basics = imported.doc.profile.active();

        // The name and the title shared one line, split by a run of spaces.
        assert_eq!(basics.name, "Marie Curie");
        assert_eq!(basics.label, "Systems & Data Engineer");
        assert_eq!(basics.email, "albert@example.com");
        assert_eq!(basics.location, "Calgary, Canada");

        // Both URLs are kept, one as the primary and one as a profile.
        let urls: Vec<&str> = std::iter::once(basics.url.as_str())
            .chain(basics.profiles.iter().map(|p| p.url.as_str()))
            .collect();
        assert!(
            urls.iter().any(|u| u.contains("linkedin.com")),
            "got {urls:?}"
        );
        assert!(
            urls.iter().any(|u| u.contains("example.com/")),
            "got {urls:?}"
        );

        // The heart of it: contact data must not be reported as dropped. The
        // email in particular was both parsed *and* listed as lost.
        for line in imported.unplaced.iter().map(Unplaced::line) {
            assert!(
                !line.contains('@') && !line.contains("http"),
                "contact line reported as unplaced: {line}"
            );
        }
    }

    /// PDF extraction wraps long skill rows. Each fragment used to become its
    /// own group, turning six groups into thirty-one on the review screen.
    #[test]
    fn wrapped_skill_lines_continue_the_group_above_them() {
        // Real widths matter: the wrap is detected by measuring the document,
        // so a fixture whose lines are all short describes a document that was
        // never typeset and proves nothing about one that was.
        let raw = "SKILLS\n\
                   Programming Languages — Expert: Python   Competent: C/C++, Rust, Java\n\
                   Mathematical Modeling & HPC — Numerical Methods   Optimization & Control Theory   High-Performance Computing\n\
                   (HPC)   Time-Series Analysis   Vectorized Algorithms   Dynamical Systems\n";
        let imported = classify_raw_text("PDF", raw);
        let skills = imported.doc.skills.active();

        assert_eq!(
            skills.len(),
            2,
            "got {:?}",
            skills.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert_eq!(skills[0].name, "Programming Languages");
        assert_eq!(skills[1].name, "Mathematical Modeling & HPC");
        // The wrapped fragment joined the group above rather than starting one.
        assert!(
            skills[1].keywords.iter().any(|k| k == "Dynamical Systems"),
            "got {:?}",
            skills[1].keywords
        );
        // Multi-word skills stay whole.
        assert!(skills[1]
            .keywords
            .iter()
            .any(|k| k == "Optimization & Control Theory"));
    }

    #[test]
    fn test_date_range_regex_formats() {
        let regex = get_date_range_regex();
        assert!(regex.is_match("Jan 2020 – Dec 2022"));
        assert!(regex.is_match("January 2020 - Present"));
        assert!(regex.is_match("2018-05 — 2021-11"));
        assert!(regex.is_match("Сентябрь 2019 – Настоящее время"));
        assert!(regex.is_match("01/2020 to 12/2022"));
        assert!(regex.is_match("Янв 2021 — по н.в."));
    }

    #[test]
    fn test_multilingual_iso_header_classification() {
        assert_eq!(classify_header("# WORK EXPERIENCE"), SectionKind::Work);
        assert_eq!(classify_header("## 1. Опыт работы:"), SectionKind::Work);
        assert_eq!(classify_header("Berufserfahrung"), SectionKind::Work); // German
        assert_eq!(
            classify_header("Expérience professionnelle"),
            SectionKind::Work
        ); // French
        assert_eq!(classify_header("Experiencia laboral"), SectionKind::Work); // Spanish
        assert_eq!(classify_header("**Образование**"), SectionKind::Education);
        assert_eq!(classify_header("Ausbildung"), SectionKind::Education); // German
        assert_eq!(
            classify_header("Technical Skills & Tools"),
            SectionKind::Skills
        );
        assert_eq!(classify_header("Compétences"), SectionKind::Skills); // French
        assert_eq!(
            classify_header("Certifications & Licenses"),
            SectionKind::Certificates
        );
        assert_eq!(classify_header("О себе"), SectionKind::Summary);
    }

    #[test]
    fn test_nlp_fuzzy_matching_typos() {
        assert_eq!(classify_header("Experiance"), SectionKind::Work);
        assert_eq!(classify_header("Educaton"), SectionKind::Education);
        assert_eq!(classify_header("Skils"), SectionKind::Skills);
    }

    /// The third of three sources, and the only one whose offer nothing
    /// asserted: a line the classifier read and could not place has no label,
    /// so the person names the section and the lines become its bullets.
    ///
    /// The kinds must not merge. Nothing in `offer()` stops someone changing
    /// one arm of that match, and the difference is the whole of what the
    /// import panel can honestly say.
    #[test]
    fn a_line_that_cannot_be_placed_is_offered_as_a_section_the_person_names() {
        use crate::import::model::{adopt_as_section, Unplaced, UnplacedOffer, UnplacedSource};

        let raw = "Albert Einstein\n\
                   CONTACT\n\
                   albert@example.com\n\
                   Member of the Prussian Academy\n";
        let imported = classify_raw_text("PDF", raw);

        // The email is contact data and is consumed; the sentence is not.
        let leftover = imported
            .unplaced
            .iter()
            .find(|u| u.source == UnplacedSource::Classifier)
            .expect("the sentence is reported, not dropped");
        assert_eq!(leftover.title, "Member of the Prussian Academy");
        assert_eq!(
            leftover.offer(),
            UnplacedOffer::NamedByPerson,
            "a classifier line has no heading to propose"
        );

        // And the panel's promise — "each becomes one bullet" — is what
        // actually happens. Built here rather than taken from the import
        // because the contact reader joins consecutive stray lines into one,
        // and the shape worth pinning is what adoption does with several.
        let lines = [
            Unplaced::line_only("Member of the Prussian Academy"),
            Unplaced::line_only("Willing to relocate"),
        ];
        let mut doc = imported.doc.clone();
        assert_eq!(adopt_as_section(&mut doc, "Notes", &lines), 2);

        let section = doc.custom_sections.last().expect("the section");
        assert_eq!(section.title, "Notes");
        let entries = section.content.active();
        assert_eq!(
            entries.len(),
            1,
            "unlabelled lines are bullets of one entry, not headings of several"
        );
        assert!(entries[0].title.is_empty());
        assert_eq!(
            entries[0].highlights,
            ["Member of the Prussian Academy", "Willing to relocate"]
        );
    }
}
