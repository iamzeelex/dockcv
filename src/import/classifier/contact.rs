//! The contact block: the lines at the top that are not prose.
//!
//! Split from `classifier.rs` by C15. Everything here answers one question —
//! is this line an email, a phone number, a profile URL, a place — and the
//! answers are regexes and shape tests rather than the taxonomy the rest of
//! the classifier runs on, which is why they travel together.

use std::sync::OnceLock;

use regex::Regex;

use crate::import::lines;
use crate::resume::model::{NetworkProfile, Resume};

use super::SectionKind;
use super::headings::classify_header;

static EMAIL_REGEX: OnceLock<Regex> = OnceLock::new();

static PHONE_REGEX: OnceLock<Regex> = OnceLock::new();

static URL_REGEX: OnceLock<Regex> = OnceLock::new();

pub(crate) fn get_email_regex() -> &'static Regex {
    EMAIL_REGEX.get_or_init(|| Regex::new(r"(?i)[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}").unwrap())
}

pub(crate) fn get_phone_regex() -> &'static Regex {
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

pub(crate) fn get_url_regex() -> &'static Regex {
    URL_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)https?://[^\s]+|www\.[a-z0-9-]+\.[a-z]{2,}[^\s]*|github\.com/[^\s]+|linkedin\.com/in/[^\s]+",
        )
        .unwrap()
    })
}

/// Does this line carry contact data rather than a title?
///
/// Separator-heavy, or holding an address, a number or a handle. A job title is
/// a phrase; a contact line is a list.
pub(crate) fn looks_like_contact_line(line: &str) -> bool {
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

/// Take a line as contact data, if that is what it is.
pub(crate) fn absorb_contact(line: &str, resume: &mut Resume) -> bool {
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
pub(crate) fn take_address(url: &str, resume: &mut Resume) {
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
pub(crate) fn trim_url_tail(url: &str) -> &str {
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

/// The first telephone number in a block of text, if there is one.
///
/// The regex is this module's; the sidebar reader needs the answer and has no
/// business knowing how it is arrived at.
pub fn first_phone(text: &str) -> Option<String> {
    get_phone_regex()
        .find(text)
        .map(|m| m.as_str().trim().to_string())
}

/// Clean bullet glyphs from text lines.
/// The site a URL belongs to, for the profile list. Only names sites the URL
/// itself identifies — never guesses a network from a bare domain.
pub(crate) fn network_of(url: &str) -> &'static str {
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
pub(crate) fn looks_like_bare_place(line: &str) -> bool {
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

pub(crate) fn looks_like_place(line: &str) -> bool {
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
pub(crate) fn contact_region(lines: &[lines::LogicalLine]) -> String {
    let mut region: Vec<&str> = Vec::new();
    let mut in_contact_section = true;

    for line in lines {
        if line.kind == lines::LineKind::Heading {
            in_contact_section = classify_header(&line.text) == SectionKind::Contact;
            continue;
        }
        if in_contact_section {
            region.push(line.text.as_str());
        }
    }
    region.join("\n")
}
