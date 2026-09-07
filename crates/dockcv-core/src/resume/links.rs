//! What the user typed, and what a viewer can actually follow.
//!
//! A URL field holds `vestergaard.dev`, because that is how a person writes
//! their own site on their own CV and it is what should print on the page.
//! It is not, however, a URL: RFC 3986 reads it as a *relative reference*, and
//! a relative reference in a PDF link annotation resolves against the document
//! — so Preview does nothing at all and a browser goes looking for
//! `file:///Users/…/Downloads/vestergaard.dev`. G1 shipped every entry link
//! that way, and fourteen of the fifteen links in a real CV were dead.
//!
//! So the two are separated. The vault keeps what the user typed, the page
//! prints what the user typed, and the *target* — and only the target — is
//! made absolute here, once, for every emitter that has a target to write:
//! Typst (and through it PDF and SVG), DOCX hyperlinks, Markdown, JSON Resume.
//! Plain text has no targets, only text, and is untouched by design.
//!
//! Every function returns `Option<String>`, and `None` means *print the text,
//! link nothing*. A URL field can hold anything a person types into it, and a
//! link to `see my website` is worse than no link: it looks live and is not.

/// Schemes an exported CV may link with.
///
/// An allow-list rather than a deny-list, because the values reaching here are
/// not all the user's own — an imported LinkedIn archive or JSON Resume brings
/// whatever was in the file, and these documents are then opened by strangers
/// in a PDF viewer and by the wasm demo in a browser. `javascript:` and
/// `data:` are live code in both. Nothing a CV needs is missing from this list.
const FOLLOWABLE_SCHEMES: [&str; 4] = ["http", "https", "mailto", "tel"];

/// The absolute target for a URL field, or `None` when nothing followable can
/// be made of what is there.
///
/// `github.com/nvestergaard` becomes `https://github.com/nvestergaard`,
/// `//cdn.example.com` inherits `https:`, and an already-absolute URL is left
/// alone. What is *not* a host — a note to self, a job title pasted into the
/// wrong field — comes back `None`.
pub fn href(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }

    if let Some(scheme) = scheme_of(trimmed) {
        return FOLLOWABLE_SCHEMES
            .iter()
            .any(|s| scheme.eq_ignore_ascii_case(s))
            .then(|| percent_encode(trimmed));
    }

    // `//host/path` is a scheme-relative reference and means "whatever the
    // document is served over" — which, for a file on disk, is nothing.
    if let Some(rest) = trimmed.strip_prefix("//") {
        return looks_like_host(rest).then(|| percent_encode(&format!("https://{rest}")));
    }

    looks_like_host(trimmed).then(|| percent_encode(&format!("https://{trimmed}")))
}

/// The `mailto:` target for an email field, or `None` if it is not an address.
pub fn mailto(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let plausible = trimmed.contains('@')
        && !trimmed.starts_with('@')
        && !trimmed.ends_with('@')
        && !trimmed.chars().any(|c| c.is_whitespace() || c.is_control());
    plausible.then(|| format!("mailto:{}", percent_encode(trimmed)))
}

/// The `tel:` target for a phone field, or `None` if there is no number in it.
///
/// `+45 28 44 10 92` is how a phone number is written and how it must keep
/// printing; `tel:+45 28 44 10 92` is not a URI, and the spaces alone were
/// enough for every viewer to refuse it. RFC 3966 wants the digits and the
/// leading `+`, so that is what is kept — the visual separators a person types
/// (spaces, dashes, dots, parentheses, non-breaking spaces) are dropped.
pub fn tel(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let mut out = String::with_capacity(trimmed.len());
    if trimmed.starts_with('+') {
        out.push('+');
    }
    out.extend(trimmed.chars().filter(char::is_ascii_digit));
    // Three digits is the shortest real number anywhere (emergency lines);
    // below that the field holds something else — an extension, a note, "n/a".
    (out.trim_start_matches('+').len() >= 3)
        .then_some(out)
        .map(|n| format!("tel:{n}"))
}

/// The scheme of an absolute URI, if there is one.
///
/// Deliberately narrower than RFC 3986, which allows `.` in a scheme: a CV's
/// URL field is far likelier to hold `example.com:8080/path`, where the colon
/// introduces a port, than an `org.example.app:` deep link. Reading the former
/// as a scheme would reject a perfectly good address.
fn scheme_of(s: &str) -> Option<&str> {
    let end = s.find(':')?;
    let head = &s[..end];
    let shaped = !head.is_empty()
        && head.starts_with(|c: char| c.is_ascii_alphabetic())
        && head
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-');
    shaped.then_some(head)
}

/// Whether the authority of a scheme-less reference is plausibly a host.
///
/// A dotted name whose last label reads like a TLD — at least two characters,
/// at least one of them a letter. That admits `dtu.dk` and `doi.org/10.0000/x`
/// and turns away the two things people put in a URL field that are not
/// addresses: a sentence, and a bare DOI like `10.1145/example`, whose last
/// label before the slash is a number. A CV does not link to a raw IP address
/// either, and this declines to guess that it might.
fn looks_like_host(s: &str) -> bool {
    let authority = &s[..s.find(['/', '?', '#']).unwrap_or(s.len())];
    // Userinfo and port are not part of the name being checked.
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host);
    if host.starts_with('.') || host.contains(|c: char| c.is_whitespace()) {
        return false;
    }
    let Some((name, tld)) = host.rsplit_once('.') else {
        return false;
    };
    !name.is_empty() && tld.chars().count() >= 2 && tld.contains(char::is_alphabetic)
}

/// Escape the bytes a URI may not carry literally, leaving existing `%XX`
/// escapes alone so an already-encoded address is not encoded twice.
///
/// Non-ASCII is the case that matters: a PDF `/URI` action is defined over
/// 7-bit ASCII, so a Cyrillic path reaches the viewer as mojibake or not at
/// all unless it is percent-encoded here.
fn percent_encode(s: &str) -> String {
    const SAFE_PUNCTUATION: &str = "-._~:/?#[]@!$&'()*+,;=";

    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let already_escaped = b == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit();
        if already_escaped {
            out.push_str(&s[i..i + 3]);
            i += 3;
        } else if b.is_ascii_alphanumeric() || SAFE_PUNCTUATION.as_bytes().contains(&b) {
            out.push(b as char);
            i += 1;
        } else {
            out.push_str(&format!("%{b:02X}"));
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole bug, in the values that were actually in the vault: every one
    /// of these printed a live-looking link that no viewer would follow.
    #[test]
    fn a_bare_host_becomes_an_absolute_url() {
        for (typed, expected) in [
            ("vestergaard.dev", "https://vestergaard.dev"),
            ("github.com/nvestergaard", "https://github.com/nvestergaard"),
            ("dtu.dk", "https://dtu.dk"),
            (
                "doi.org/10.0000/nssp.2021.14",
                "https://doi.org/10.0000/nssp.2021.14",
            ),
            ("www.example.com", "https://www.example.com"),
            ("  spaced.example.com  ", "https://spaced.example.com"),
            ("//cdn.example.com/a.pdf", "https://cdn.example.com/a.pdf"),
            ("example.com:8080/path", "https://example.com:8080/path"),
        ] {
            assert_eq!(href(typed).as_deref(), Some(expected), "for {typed:?}");
        }
    }

    #[test]
    fn an_absolute_url_is_left_alone() {
        for typed in [
            "https://example.com",
            "http://example.com/a?b=c#d",
            "HTTPS://Example.COM/Path",
            "mailto:someone@example.com",
        ] {
            assert_eq!(href(typed).as_deref(), Some(typed), "for {typed:?}");
        }
    }

    /// A port is not a scheme. Reading `example.com:8080` as one would have
    /// rejected the address outright.
    #[test]
    fn a_port_is_not_mistaken_for_a_scheme() {
        assert_eq!(
            href("example.com:8080/path").as_deref(),
            Some("https://example.com:8080/path")
        );
    }

    /// The field takes free text, and free text is not a link.
    #[test]
    fn what_is_not_a_host_is_not_linked() {
        for typed in [
            "",
            "   ",
            "see my website",
            "TBD",
            "n/a",
            // A bare DOI, which is not a host however much it looks like one.
            "10.1145/example",
            "192.168.1.1",
        ] {
            assert_eq!(href(typed), None, "for {typed:?}");
        }
    }

    /// These reach the field through an import, not through the keyboard, and
    /// are live code in a browser and in some PDF viewers.
    #[test]
    fn executable_schemes_are_not_followable() {
        for typed in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html;base64,PHNjcmlwdD4=",
            "vbscript:msgbox",
            "file:///etc/passwd",
        ] {
            assert_eq!(href(typed), None, "for {typed:?}");
        }
    }

    /// A newline in the middle of a target is how an annotation dictionary
    /// gets a second line; there is no honest value that contains one.
    #[test]
    fn control_characters_are_never_followable() {
        assert_eq!(href("https://example.com\n/evil"), None);
        assert_eq!(mailto("a\n@example.com"), None);
    }

    #[test]
    fn a_non_ascii_target_is_percent_encoded() {
        assert_eq!(
            href("uk.wikipedia.org/wiki/Ейнштейн").as_deref(),
            Some("https://uk.wikipedia.org/wiki/%D0%95%D0%B9%D0%BD%D1%88%D1%82%D0%B5%D0%B9%D0%BD")
        );
        // A space is encoded rather than rejected: the host is still a host.
        assert_eq!(
            href("example.com/my file").as_deref(),
            Some("https://example.com/my%20file")
        );
        // Already-encoded stays encoded — `%25` here would be double-encoding.
        assert_eq!(
            href("example.com/a%20b").as_deref(),
            Some("https://example.com/a%20b")
        );
    }

    /// The other half of the shipped bug: a phone number written the way
    /// people write phone numbers is not a `tel:` URI.
    #[test]
    fn a_phone_number_loses_its_visual_separators() {
        assert_eq!(tel("+45 28 44 10 92").as_deref(), Some("tel:+4528441092"));
        assert_eq!(tel("(555) 019-9").as_deref(), Some("tel:5550199"));
        assert_eq!(tel("+1.555.0199").as_deref(), Some("tel:+15550199"));
        assert_eq!(tel(""), None);
        assert_eq!(tel("ask me"), None);
        assert_eq!(tel("+"), None);
    }

    #[test]
    fn an_email_becomes_a_mailto_only_when_it_is_one() {
        assert_eq!(
            mailto("nora@vestergaard.dev").as_deref(),
            Some("mailto:nora@vestergaard.dev")
        );
        assert_eq!(
            mailto("  nora@vestergaard.dev "),
            Some("mailto:nora@vestergaard.dev".into())
        );
        assert_eq!(mailto("nora at vestergaard dot dev"), None);
        assert_eq!(mailto("@handle"), None);
        assert_eq!(mailto(""), None);
    }
}
