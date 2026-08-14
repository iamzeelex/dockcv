//! Recognize an AltaCV (Typst Universe, v1.5.0) document into our [`Resume`].
//!
//! AltaCV 1.5.0 is data-driven: the whole CV is a single `#let cv = (..)`
//! dictionary following the JSON-Resume schema, passed to `#alta(cv, ..)`. So
//! "recognizing sections" means parsing that dictionary — which we do over the
//! real `typst-syntax` AST, not regexes. We locate the dictionary literal that
//! has resume-shaped keys and read its fields, slicing content blocks (`[..]`)
//! straight from the source to preserve the author's markup.
//!
//! Anything we don't model (the import, the `#alta(..)` call, `image`,
//! `interests`, …) is simply ignored.

use typst::syntax::{ast, ast::AstNode, Source, SyntaxKind, SyntaxNode};

use crate::resume::model::{
    Certificate, Education, NetworkProfile, Resume, SkillGroup, Volunteer, Work,
};

/// Parse an AltaCV-style document and recognize its sections.
///
/// Returns `None` only if no resume-shaped dictionary is found.
pub fn import(text: &str) -> Option<Resume> {
    let source = Source::detached(text);
    let dict = find_resume_dict(source.root())?;
    Some(extract(&source, dict))
}

/// Depth-first search for the first dictionary literal whose keys look like a
/// resume (so we skip the `preferences` dict and any others).
fn find_resume_dict(node: &SyntaxNode) -> Option<ast::Dict<'_>> {
    if node.kind() == SyntaxKind::Dict {
        if let Some(dict) = node.cast::<ast::Dict>() {
            if is_resume_dict(dict) {
                return Some(dict);
            }
        }
    }
    node.children().find_map(find_resume_dict)
}

fn is_resume_dict(dict: ast::Dict) -> bool {
    dict.items().any(|item| {
        matches!(
            item,
            ast::DictItem::Named(named)
                if matches!(
                    named.name().as_str(),
                    "basics" | "work" | "education" | "skills" | "certificates" | "volunteer"
                )
        )
    })
}

fn extract(source: &Source, dict: ast::Dict) -> Resume {
    let mut resume = Resume::default();

    if let Some(basics) = named(dict, "basics").and_then(as_dict) {
        let b = &mut resume.basics;
        set(source, basics, "name", &mut b.name);
        set(source, basics, "label", &mut b.label);
        set(source, basics, "summary", &mut b.summary);
        set(source, basics, "email", &mut b.email);
        set(source, basics, "phone", &mut b.phone);
        set(source, basics, "location", &mut b.location);
        set(source, basics, "url", &mut b.url);

        for entry in array_of_dicts(named(basics, "profiles")) {
            let mut p = NetworkProfile::default();
            set(source, entry, "network", &mut p.network);
            set(source, entry, "username", &mut p.username);
            set(source, entry, "url", &mut p.url);
            b.profiles.push(p);
        }
    }

    for entry in array_of_dicts(named(dict, "work")) {
        let mut w = Work::default();
        set(source, entry, "name", &mut w.name);
        set(source, entry, "position", &mut w.position);
        set(source, entry, "location", &mut w.location);
        set(source, entry, "startDate", &mut w.start_date.text);
        set(source, entry, "endDate", &mut w.end_date.text);
        set(source, entry, "summary", &mut w.summary);
        w.highlights = string_list(source, named(entry, "highlights"));
        resume.work.push(w);
    }

    for entry in array_of_dicts(named(dict, "education")) {
        let mut e = Education::default();
        set(source, entry, "institution", &mut e.institution);
        set(source, entry, "studyType", &mut e.study_type);
        set(source, entry, "startDate", &mut e.start_date.text);
        set(source, entry, "endDate", &mut e.end_date.text);
        set(source, entry, "url", &mut e.url);
        resume.education.push(e);
    }

    for entry in array_of_dicts(named(dict, "skills")) {
        let mut s = SkillGroup::default();
        set(source, entry, "name", &mut s.name);
        s.keywords = string_list(source, named(entry, "keywords"));
        resume.skills.push(s);
    }

    for entry in array_of_dicts(named(dict, "certificates")) {
        let mut c = Certificate::default();
        set(source, entry, "name", &mut c.name);
        set(source, entry, "issuer", &mut c.issuer);
        set(source, entry, "date", &mut c.date.text);
        set(source, entry, "url", &mut c.url);
        resume.certificates.push(c);
    }

    for entry in array_of_dicts(named(dict, "volunteer")) {
        let mut v = Volunteer::default();
        set(source, entry, "organization", &mut v.organization);
        set(source, entry, "position", &mut v.position);
        set(source, entry, "startDate", &mut v.start_date.text);
        set(source, entry, "endDate", &mut v.end_date.text);
        v.highlights = string_list(source, named(entry, "highlights"));
        resume.volunteer.push(v);
    }

    resume
}

// --- AST helpers ---

/// The value of a named entry in a dictionary, if present.
fn named<'a>(dict: ast::Dict<'a>, key: &str) -> Option<ast::Expr<'a>> {
    dict.items().find_map(|item| match item {
        ast::DictItem::Named(n) if n.name().as_str() == key => Some(n.expr()),
        _ => None,
    })
}

/// Assign a string-valued field if the key is present.
fn set(source: &Source, dict: ast::Dict, key: &str, target: &mut String) {
    if let Some(expr) = named(dict, key) {
        *target = as_string(source, expr);
    }
}

fn as_dict(expr: ast::Expr) -> Option<ast::Dict> {
    match expr {
        ast::Expr::Dict(dict) => Some(dict),
        _ => None,
    }
}

/// Read an array expression as a list of the dictionaries it contains.
fn array_of_dicts<'a>(expr: Option<ast::Expr<'a>>) -> Vec<ast::Dict<'a>> {
    let Some(ast::Expr::Array(array)) = expr else {
        return Vec::new();
    };
    array
        .items()
        .filter_map(|item| match item {
            ast::ArrayItem::Pos(e) => as_dict(e),
            _ => None,
        })
        .collect()
}

/// Read an array expression as a list of strings (handles string literals and
/// content blocks, e.g. `keywords` vs `highlights`).
fn string_list(source: &Source, expr: Option<ast::Expr>) -> Vec<String> {
    let Some(ast::Expr::Array(array)) = expr else {
        return Vec::new();
    };
    array
        .items()
        .filter_map(|item| match item {
            ast::ArrayItem::Pos(e) => Some(as_string(source, e)),
            _ => None,
        })
        .collect()
}

/// Best-effort string for a value: unquote strings, slice content-block markup
/// from the source, and fall back to the raw source text for anything else.
fn as_string(source: &Source, expr: ast::Expr) -> String {
    match expr {
        ast::Expr::Str(s) => s.get().to_string(),
        ast::Expr::ContentBlock(cb) => content_markup(source, cb.to_untyped()),
        ast::Expr::Ident(id) => id.as_str().to_string(),
        other => raw_text(source, other.to_untyped()).trim().to_string(),
    }
}

/// The inner markup of a content block (without the surrounding `[ ]`).
fn content_markup(source: &Source, node: &SyntaxNode) -> String {
    let raw = raw_text(source, node);
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

/// The exact source text covered by a node.
fn raw_text(source: &Source, node: &SyntaxNode) -> String {
    source
        .find(node.span())
        .map(|linked| source.text()[linked.range()].to_string())
        .unwrap_or_default()
}

/// The AltaCV starter document (Typst Universe `@preview/altacv:1.5.0`), used
/// as the built-in demo so the editor opens on a recognized resume.
pub const ALTACV_SAMPLE: &str = r#"
#import "@preview/altacv:1.5.0": alta, avatar-placeholder

#let cv = (
  basics: (
    name: "Seán Ó Murchú",
    label: "Senior Software Engineer",
    summary: [
      Backend engineer with eight years of experience designing
      distributed, event-driven systems. Specialises in functional
      programming, observability, and developer experience.
    ],
    email: "sean@example.com",
    phone: "+353 1 555 0100",
    location: "Tallaght, Dublin",
    url: "https://seanomurchu.dev",
    image: avatar-placeholder,
    profiles: (
      (network: "GitHub", username: "seanomurchu", url: "https://github.com/seanomurchu"),
      (network: "LinkedIn", username: "seanomurchu", url: "https://linkedin.com/in/seanomurchu"),
    ),
  ),

  work: (
    (
      name: "Acme Corp",
      url: "https://acme.example.com",
      position: "Senior Software Engineer",
      location: "Dublin, Ireland",
      startDate: "2022-01",
      summary: [Platform team lead. Owns the event-sourcing stack.],
      highlights: (
        [Migrated a customer-facing monolith to event-driven services, halving p99 latency.],
        [Rolled out an event-sourcing platform now used by four product teams.],
      ),
    ),
    (
      name: "Liffey Labs",
      position: "Software Engineer",
      location: "Remote",
      startDate: "2019-06",
      endDate: "2022-01",
      highlights: (
        [Shipped the first version of a SaaS product alongside a two-person team.],
        [Built the CI/CD pipeline that scaled the engineering org from 3 to 15.],
      ),
    ),
    (
      name: "Grand Canal Systems",
      position: "Software Engineer",
      location: "Dublin, Ireland",
      startDate: "2017-09",
      endDate: "2019-06",
      highlights: (
        [Led the migration of services from VMs to Kubernetes.],
      ),
    ),
  ),

  skills: (
    (name: "Languages", keywords: ("Scala", "Haskell", "Go")),
    (name: "Infra",     keywords: ("Kafka", "AWS", "Kubernetes")),
  ),

  education: (
    (
      institution: "Tallaght Institute of Technology",
      url: "https://example.edu/tit",
      studyType: "M.Sc. in Computer Science",
      startDate: "2015",
      endDate: "2017",
    ),
  ),

  certificates: (
    (
      name: "Certified Kubernetes Administrator",
      issuer: "CNCF",
      date: "2023-09",
      url: "https://www.cncf.io/training/certification/cka/",
    ),
    (
      name: "Certified Kubernetes Application Developer",
      issuer: "CNCF",
      date: "2024-04",
      url: "https://www.cncf.io/training/certification/ckad/",
    ),
  ),

  volunteer: (
    (
      organization: "CoderDojo Dublin",
      position: "Mentor",
      startDate: "2020-09",
      highlights: (
        [Weekly mentoring sessions for 10–14 year-olds learning to code.],
      ),
    ),
  ),
)

#alta(cv)
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render;
    use crate::resume::template;
    use crate::typst_engine::TypstEngine;

    #[test]
    fn recognizes_altacv_sections() {
        let resume = import(ALTACV_SAMPLE).expect("resume dictionary recognized");

        assert_eq!(resume.basics.name, "Seán Ó Murchú");
        assert_eq!(resume.basics.label, "Senior Software Engineer");
        assert!(resume.basics.summary.contains("Backend engineer"));
        assert_eq!(resume.basics.profiles.len(), 2);

        assert_eq!(resume.work.len(), 3);
        assert_eq!(resume.work[0].position, "Senior Software Engineer");
        assert_eq!(resume.work[0].highlights.len(), 2);
        assert!(resume.work[0].highlights[0].contains("event-driven"));

        assert_eq!(resume.education.len(), 1);
        assert_eq!(resume.skills.len(), 2);
        assert_eq!(resume.skills[0].keywords, ["Scala", "Haskell", "Go"]);
        assert_eq!(resume.certificates.len(), 2);
        assert_eq!(resume.certificates[0].issuer, "CNCF");
        assert_eq!(resume.volunteer.len(), 1);
        assert_eq!(resume.volunteer[0].organization, "CoderDojo Dublin");
    }

    #[test]
    fn generated_template_compiles_and_rasterizes() {
        let resume = import(ALTACV_SAMPLE).unwrap();
        let engine = TypstEngine::new(template::generate(&resume));

        let (pixels, geometry) = engine
            .compile_to_pixels(2.0)
            .expect("generated template compiles");
        assert!(pixels.width > 0 && pixels.height > 0);
        assert_eq!(
            pixels.rgba.len(),
            (pixels.width * pixels.height * 4) as usize
        );
        assert!(geometry.page_count >= 1);

        let rendered = render::pixels_to_render_image(pixels, 2.0).expect("rasterizes");
        assert!(rendered.width > 0.0 && rendered.height > 0.0);
    }

    #[test]
    fn exports_valid_pdf() {
        let resume = import(ALTACV_SAMPLE).unwrap();
        let engine = TypstEngine::new(template::generate(&resume));
        let pdf = engine.compile_to_pdf().expect("pdf export");
        assert!(pdf.starts_with(b"%PDF"), "output is not a PDF");
    }
}
