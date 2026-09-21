//! What one line of a CV is, once you know which section it is in.
//!
//! Split from `classifier.rs` by C15. A heading tells you the section; these
//! tell you what the lines under it are — an institution, a degree, a date
//! range, a certificate, a skill group, a bullet — and strip what is not
//! content at all, like the running header a two-page export repeats.
//!
//! The dates live here rather than with the contact block because a date on a
//! line is what most often decides whether that line is an entry heading or
//! prose.

use std::sync::OnceLock;

use regex::Regex;

use crate::resume::model::{Certificate, CustomEntry, Resume};

use crate::import::lines;

use super::SectionKind;

static DATE_RANGE_REGEX: OnceLock<Regex> = OnceLock::new();

static SINGLE_DATE_REGEX: OnceLock<Regex> = OnceLock::new();

/// Comprehensive date range regex matching English, Russian, ISO (YYYY-MM), and numeric date formats.
pub(crate) fn get_date_range_regex() -> &'static Regex {
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

pub(crate) fn get_single_date_regex() -> &'static Regex {
    // A year, and the month and day after it when they are there. Matching the
    // year alone left `-11-07` behind out of `2021-11-07`, which then read as
    // text: a certificate's issuer came back as `Company  2021-11-07`.
    SINGLE_DATE_REGEX.get_or_init(|| {
        Regex::new(r"(?i)\b(19|20)(\d{2}|XX)(?:[-/.][0-9]{1,2}(?:[-/.][0-9]{1,2})?)?\b").unwrap()
    })
}

/// Clean markdown, zero-width spaces, or structural decoration from a header candidate.
pub(crate) fn sanitize_header_line(line: &str) -> String {
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

/// Does this name a place of study rather than a course of study?
///
/// Templates put the degree and the school in whichever order they like — a
/// `Heading2` carrying `Bellows College` in one entry and `Doctor of Medicine
/// (MD)` in the next, with the counterpart on the line below. Position cannot
/// resolve that; the words can.
pub(crate) fn looks_like_institution(line: &str) -> bool {
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
pub(crate) fn looks_like_degree(line: &str) -> bool {
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

/// Does the line end in `(2023-04)` — a date in brackets, closing it?
pub(crate) fn ends_with_parenthesised_date(line: &str) -> bool {
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
pub(crate) fn is_only_dates(line: &str) -> bool {
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
pub(crate) const MONTH_NAMES: [&str; 24] = [
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
pub(crate) fn parse_certificate(text: &str) -> Certificate {
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
        if url.is_empty() && lines::is_lone_address(inner) {
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
            .find(|token| lines::is_lone_address(token));
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
pub(crate) fn attach_entry_url(
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
pub(crate) fn split_trailing_date(text: &str) -> (&str, &str) {
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

/// Split `Name — kw   kw` into its group name and keywords, or decline when
/// the line carries no group separator.
pub(crate) fn split_skill_group(line: &str) -> Option<(String, Vec<String>)> {
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
pub(crate) fn split_keywords(text: &str) -> Vec<String> {
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
pub(crate) fn strip_running_header(line: &str, fragments: &[String]) -> String {
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

pub(crate) fn clean_bullet(line: &str) -> &str {
    line.trim_start_matches(['•', '-', '*', '▪', '►', '–', '—', '+', ' '])
        .trim()
}
