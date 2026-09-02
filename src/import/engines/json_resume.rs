//! JSON Resume, read against **its** schema rather than ours.
//!
//! The importer used to deserialize a JSON Resume straight into
//! [`Resume`](crate::resume::model::Resume) and hope the two agreed. They do
//! not, in two ways that both lose the whole file:
//!
//! * the spec types `basics.location` as an **object** (`{city, region,
//!   countryCode…}`) and ours is a `String`, so serde refuses the document with
//!   `invalid type: map, expected a string`;
//! * the spec is camelCase (`startDate`, `studyType`) and ours is snake_case, so
//!   even the fields that did line up arrived empty.
//!
//! The refusal was then swallowed by the dispatcher and the raw JSON handed to
//! the prose classifier, which produced a CV whose name was `{`. A foreign
//! schema deserves its own types; this module is them, and the mapping into
//! DockCV's model is written out rather than left to a name collision.
//!
//! ### What DockCV has nowhere for
//!
//! The spec carries more kinds of thing than the six built-in sections.
//! `projects`, `languages`, `awards` and `publications` become **custom
//! sections** — the same shape the LinkedIn engine already gives them.
//! `interests`, `references` and `basics.image` have no home at all, so they are
//! reported in [`ImportedDoc::unplaced`] instead of being dropped in silence
//! (US-01).

use serde::Deserialize;

use crate::import::model::ImportedDoc;
use crate::import::notes::{Note, Part};
use crate::resume::model::{
    Certificate, CustomEntry, Education, NetworkProfile, Resume, ResumeDoc, SkillGroup, Volunteer,
    Work,
};

/// Parse `text` as a JSON Resume, or return `None` if it is not one.
///
/// Every field is `default`, so *any* JSON object deserializes — which means
/// parsing is not evidence. [`JsonResume::is_a_resume`] is: a document that
/// carries neither a name nor a single work, education or skill entry is some
/// other JSON that happens to be syntactically fine.
pub fn import(text: &str) -> Option<ImportedDoc> {
    let parsed: JsonResume = serde_json::from_str(text).ok()?;
    if !parsed.is_a_resume() {
        return None;
    }
    Some(parsed.into_imported())
}

// ---------------------------------------------------------------------------
// The schema, as jsonresume.org publishes it
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct JsonResume {
    basics: Basics,
    work: Vec<Work_>,
    volunteer: Vec<Volunteer_>,
    education: Vec<Education_>,
    awards: Vec<Award>,
    certificates: Vec<Certificate_>,
    publications: Vec<Publication>,
    skills: Vec<Skill>,
    languages: Vec<Language>,
    interests: Vec<Interest>,
    references: Vec<Reference>,
    projects: Vec<Project>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Basics {
    name: String,
    label: String,
    image: String,
    email: String,
    phone: String,
    url: String,
    summary: String,
    location: Location,
    profiles: Vec<Profile>,
}

/// The field that broke the old path. Every part is optional in the wild —
/// plenty of real résumés carry only `city`.
///
/// `camelCase` like the rest: `postalCode` and `countryCode` are two more of
/// the names the old path could not see.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Location {
    address: String,
    postal_code: String,
    city: String,
    country_code: String,
    region: String,
}

impl Location {
    /// One line, in the order a postal address reads, skipping what is absent.
    fn one_line(&self) -> String {
        [
            self.address.as_str(),
            self.city.as_str(),
            self.region.as_str(),
            self.postal_code.as_str(),
            self.country_code.as_str(),
        ]
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Profile {
    network: String,
    username: String,
    url: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Work_ {
    name: String,
    position: String,
    url: String,
    start_date: String,
    end_date: String,
    summary: String,
    highlights: Vec<String>,
    /// Not in the published schema, but emitted by enough exporters to be worth
    /// reading when it is there.
    location: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Volunteer_ {
    organization: String,
    position: String,
    start_date: String,
    end_date: String,
    summary: String,
    highlights: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Education_ {
    institution: String,
    url: String,
    area: String,
    study_type: String,
    start_date: String,
    end_date: String,
    score: String,
    courses: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Certificate_ {
    name: String,
    date: String,
    issuer: String,
    url: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Skill {
    name: String,
    level: String,
    keywords: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Award {
    title: String,
    date: String,
    awarder: String,
    summary: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Publication {
    name: String,
    publisher: String,
    release_date: String,
    url: String,
    summary: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Language {
    language: String,
    fluency: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Interest {
    name: String,
    keywords: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Reference {
    name: String,
    reference: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Project {
    name: String,
    description: String,
    highlights: Vec<String>,
    url: String,
    start_date: String,
    end_date: String,
}

// ---------------------------------------------------------------------------
// The mapping
// ---------------------------------------------------------------------------

impl JsonResume {
    fn is_a_resume(&self) -> bool {
        !self.basics.name.trim().is_empty()
            || !self.work.is_empty()
            || !self.education.is_empty()
            || !self.skills.is_empty()
    }

    fn into_imported(self) -> ImportedDoc {
        // Taken before the lists are consumed by the mapping below.
        let had_work = !self.work.is_empty();
        let had_education = !self.education.is_empty();
        let had_skills = !self.skills.is_empty();
        let had_certificates = !self.certificates.is_empty();

        let mut resume = Resume::default();

        resume.basics.name = self.basics.name;
        resume.basics.label = self.basics.label;
        resume.basics.summary = self.basics.summary;
        resume.basics.email = self.basics.email;
        resume.basics.phone = self.basics.phone;
        resume.basics.url = self.basics.url;
        resume.basics.location = self.basics.location.one_line();
        resume.basics.profiles = self
            .basics
            .profiles
            .into_iter()
            .map(|p| NetworkProfile {
                network: p.network,
                username: p.username,
                url: p.url,
            })
            .collect();

        resume.work = self
            .work
            .into_iter()
            .map(|w| Work {
                name: w.name,
                position: w.position,
                location: w.location,
                start_date: w.start_date.into(),
                end_date: w.end_date.into(),
                summary: w.summary,
                // `Work` has no url field. The employer's site is not lost —
                // it goes to the head of the bullets, where the entry's own
                // prose already lives.
                highlights: prepend_summary(w.url, w.highlights),
            })
            .collect();

        resume.volunteer = self
            .volunteer
            .into_iter()
            .map(|v| Volunteer {
                organization: v.organization,
                position: v.position,
                start_date: v.start_date.into(),
                end_date: v.end_date.into(),
                // The spec gives a volunteer entry both a summary and
                // highlights; ours has only highlights, so the summary leads
                // them rather than being dropped.
                highlights: prepend_summary(v.summary, v.highlights),
            })
            .collect();

        resume.education = self
            .education
            .into_iter()
            .map(|e| {
                // `area` is the field of study and `studyType` the level. Ours
                // has one line for both, and "BSc, Computer Science" is how a
                // CV prints it.
                let study = [e.study_type.trim(), e.area.trim()]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                let mut highlights = e.courses;
                if !e.score.trim().is_empty() {
                    highlights.insert(0, format!("Score: {}", e.score.trim()));
                }
                Education {
                    institution: e.institution,
                    study_type: study,
                    start_date: e.start_date.into(),
                    end_date: e.end_date.into(),
                    url: e.url,
                    highlights,
                }
            })
            .collect();

        resume.skills = self
            .skills
            .into_iter()
            .map(|s| SkillGroup {
                name: s.name,
                // `level` is a property of the group, and ours has no field for
                // it. Kept as a keyword rather than lost — it is the user's own
                // word about their own skill.
                keywords: prepend_summary(s.level, s.keywords),
            })
            .collect();

        resume.certificates = self
            .certificates
            .into_iter()
            .map(|c| Certificate {
                name: c.name,
                issuer: c.issuer,
                date: c.date.into(),
                url: c.url,
            })
            .collect();

        let mut doc = ResumeDoc::from_resume(resume, "Base");

        // Four kinds the spec carries that DockCV models as custom sections —
        // the same treatment the LinkedIn engine gives Projects and Languages.
        let projects: Vec<CustomEntry> = self
            .projects
            .into_iter()
            .map(|p| CustomEntry {
                title: p.name,
                subtitle: String::new(),
                start_date: p.start_date.into(),
                end_date: p.end_date.into(),
                url: p.url,
                highlights: prepend_summary(p.description, p.highlights),
            })
            .collect();
        let awards: Vec<CustomEntry> = self
            .awards
            .into_iter()
            .map(|a| CustomEntry {
                title: a.title,
                subtitle: a.awarder,
                start_date: a.date.into(),
                highlights: prepend_summary(a.summary, Vec::new()),
                ..Default::default()
            })
            .collect();
        let publications: Vec<CustomEntry> = self
            .publications
            .into_iter()
            .map(|p| CustomEntry {
                title: p.name,
                subtitle: p.publisher,
                start_date: p.release_date.into(),
                url: p.url,
                highlights: prepend_summary(p.summary, Vec::new()),
                ..Default::default()
            })
            .collect();
        let languages: Vec<CustomEntry> = self
            .languages
            .into_iter()
            .map(|l| CustomEntry {
                title: l.language,
                subtitle: l.fluency,
                ..Default::default()
            })
            .collect();

        for (title, entries) in [
            ("Projects", projects),
            ("Awards", awards),
            ("Publications", publications),
            ("Languages", languages),
        ] {
            if entries.iter().all(|e| e.title.trim().is_empty()) {
                continue;
            }
            let id = doc.add_custom_section(title);
            if let Some(section) = doc.custom_section_mut(id) {
                *section.content.active_mut() = entries;
            }
        }

        let mut imported = ImportedDoc::new("JSON Resume", doc);

        // The one thing the result cannot say: whether the file carried
        // something that did not survive. An array that was there and produced
        // nothing is a defect; a section the résumé simply does not have is not.
        for (present, part, empty) in [
            (had_work, Part::Work, imported.doc.work.active().is_empty()),
            (
                had_education,
                Part::Education,
                imported.doc.education.active().is_empty(),
            ),
            (
                had_skills,
                Part::Skills,
                imported.doc.skills.active().is_empty(),
            ),
            (
                had_certificates,
                Part::Certificates,
                imported.doc.certificates.active().is_empty(),
            ),
        ] {
            if present && empty {
                imported.note(part, Note::Empty);
            }
        }
        imported.observe();

        // Everything the file carried that DockCV has nowhere to put. Named,
        // not counted: US-01's promise is that nothing goes missing quietly, and
        // this is the one import path that can say exactly what did.
        let mut unplaced = Vec::new();
        for interest in self.interests {
            if interest.name.trim().is_empty() && interest.keywords.is_empty() {
                continue;
            }
            let keywords = interest.keywords.join(", ");
            unplaced.push(match keywords.is_empty() {
                true => format!("interests: {}", interest.name),
                false => format!("interests: {} — {keywords}", interest.name),
            });
        }
        for reference in self.references {
            if reference.name.trim().is_empty() && reference.reference.trim().is_empty() {
                continue;
            }
            unplaced.push(format!(
                "references: {} — {}",
                reference.name, reference.reference
            ));
        }
        if !self.basics.image.trim().is_empty() {
            unplaced.push(format!("basics.image: {}", self.basics.image));
        }
        imported.unplaced = unplaced;
        imported
    }
}

/// Put a summary line at the head of the bullets it introduces.
///
/// The spec routinely gives an entry both a prose `summary` and a `highlights`
/// list; several of DockCV's shapes have only the list. Leading with the summary
/// keeps the sentence rather than choosing between them.
fn prepend_summary(summary: String, mut rest: Vec<String>) -> Vec<String> {
    let summary = summary.trim();
    if !summary.is_empty() {
        rest.insert(0, summary.to_string());
    }
    rest
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact document that used to import as a CV named `{`.
    const SPEC_SHAPED: &str = r#"{
      "basics": {
        "name": "Albert Einstein",
        "label": "Staff Engineer",
        "email": "s@example.com",
        "phone": "+49 30 123456",
        "url": "https://einstein.example",
        "summary": "Builds data platforms.",
        "location": { "city": "Berlin", "region": "Berlin", "countryCode": "DE" },
        "profiles": [{ "network": "GitHub", "username": "sm", "url": "https://github.com/sm" }]
      },
      "work": [{
        "name": "Acme",
        "position": "Staff Engineer",
        "startDate": "2021-01",
        "endDate": "2024-06",
        "summary": "Owned the billing platform.",
        "highlights": ["Cut p99 in half"]
      }],
      "education": [{
        "institution": "TU Berlin",
        "area": "Computer Science",
        "studyType": "BSc",
        "startDate": "2014",
        "endDate": "2018",
        "courses": ["Distributed Systems"]
      }],
      "skills": [{ "name": "Backend", "level": "Advanced", "keywords": ["Rust", "Kafka"] }],
      "certificates": [{ "name": "CKA", "issuer": "CNCF", "date": "2023-03" }],
      "volunteer": [{ "organization": "CoderDojo", "position": "Mentor", "startDate": "2019" }],
      "projects": [{ "name": "dockcv", "description": "A résumé workbench", "url": "https://x.dev" }],
      "awards": [{ "title": "Best Paper", "date": "2023", "awarder": "ACM" }],
      "publications": [{ "name": "On Pipelines", "publisher": "ACM", "releaseDate": "2022" }],
      "languages": [{ "language": "German", "fluency": "C1" }],
      "interests": [{ "name": "Climbing", "keywords": ["bouldering"] }],
      "references": [{ "name": "Jane Roe", "reference": "Ships things." }]
    }"#;

    /// The regression this module exists for. Before it, this document produced
    /// `name = "{"`, no email and zero jobs — and `import_file` returned `Ok`.
    #[test]
    fn a_spec_shaped_resume_imports_its_actual_contents() {
        let imported = import(SPEC_SHAPED).expect("a JSON Resume is recognised");
        let basics = imported.doc.profile.active();

        assert_eq!(basics.name, "Albert Einstein");
        assert_eq!(basics.email, "s@example.com");
        assert_eq!(basics.phone, "+49 30 123456");
        assert_eq!(basics.label, "Staff Engineer");
        assert_eq!(basics.profiles.len(), 1);

        let work = imported.doc.work.active();
        assert_eq!(work.len(), 1);
        assert_eq!(work[0].name, "Acme");
        assert_eq!(work[0].position, "Staff Engineer");
        // The camelCase field names the old path could not see.
        assert_eq!(work[0].start_date.text, "2021-01");
        assert_eq!(work[0].end_date.text, "2024-06");
        assert_eq!(work[0].highlights, vec!["Cut p99 in half".to_string()]);
    }

    /// The field that made serde reject the whole document.
    #[test]
    fn a_location_object_becomes_the_line_a_cv_prints() {
        let imported = import(SPEC_SHAPED).expect("parses");
        assert_eq!(imported.doc.profile.active().location, "Berlin, Berlin, DE");
    }

    /// `studyType` and `area` are two halves of the line ours prints as one.
    #[test]
    fn a_degree_keeps_both_its_level_and_its_subject() {
        let imported = import(SPEC_SHAPED).expect("parses");
        let education = imported.doc.education.active();
        assert_eq!(education[0].study_type, "BSc, Computer Science");
        assert_eq!(education[0].institution, "TU Berlin");
        assert_eq!(education[0].highlights, vec!["Distributed Systems"]);
    }

    /// Four spec sections DockCV models as custom sections rather than losing.
    #[test]
    fn projects_awards_publications_and_languages_become_sections() {
        let imported = import(SPEC_SHAPED).expect("parses");
        let titles: Vec<String> = imported
            .doc
            .custom_sections
            .iter()
            .map(|s| s.title.clone())
            .collect();
        assert_eq!(
            titles,
            vec!["Projects", "Awards", "Publications", "Languages"]
        );
    }

    /// US-01: what the file carried and DockCV cannot hold is *named*.
    #[test]
    fn what_has_nowhere_to_go_is_reported_rather_than_dropped() {
        let imported = import(SPEC_SHAPED).expect("parses");
        assert!(
            imported
                .unplaced
                .iter()
                .any(|l| l.starts_with("interests:")),
            "{:?}",
            imported.unplaced
        );
        assert!(
            imported
                .unplaced
                .iter()
                .any(|l| l.starts_with("references:")),
            "{:?}",
            imported.unplaced
        );
    }

    /// Every field is `default`, so any JSON object deserializes. Parsing is not
    /// evidence, and a `package.json` must not import as somebody's CV.
    #[test]
    fn other_json_is_not_a_resume() {
        assert!(import(r#"{"name":"dockcv","version":"0.1.0"}"#).is_none());
        assert!(import("{}").is_none());
        assert!(import("[1, 2, 3]").is_none());
        assert!(import("not json at all").is_none());
    }

    /// The minimum a résumé needs to be one: a name and nothing else.
    #[test]
    fn a_name_alone_is_enough() {
        let imported = import(r#"{"basics":{"name":"Marie Curie"}}"#).expect("a name is a resume");
        assert_eq!(imported.doc.profile.active().name, "Marie Curie");
    }

    /// We own both ends, so a lossy field is a failing test here rather than a
    /// bug report from someone whose CV lost its dates.
    ///
    /// The assertions walk the fields the *importer* reads, which is the list
    /// that matters: an importer field with no writer behind it is exactly the
    /// hole a stranger falls into when they take their data out and put it back.
    #[test]
    fn export_round_trips_every_field_the_importer_reads() {
        use dockcv_core::resume::export_json_resume;

        let initial = import(SPEC_SHAPED).expect("parses spec-shaped sample");
        let composed = initial.doc.compose();

        let exported_json = export_json_resume(&composed).expect("exports valid json");
        let back = import(&exported_json).expect("re-imports exported json");

        let before = initial.doc.profile.active();
        let after = back.doc.profile.active();
        assert_eq!(after.name, before.name);
        assert_eq!(after.label, before.label);
        assert_eq!(after.email, before.email);
        assert_eq!(after.phone, before.phone);
        assert_eq!(after.url, before.url);
        assert_eq!(after.summary, before.summary);
        assert_eq!(after.location, before.location);
        assert_eq!(after.profiles.len(), before.profiles.len());
        for (b, a) in before.profiles.iter().zip(&after.profiles) {
            assert_eq!(a.network, b.network);
            assert_eq!(a.username, b.username);
            assert_eq!(a.url, b.url);
        }

        // Dates are the field a lossy exporter drops first, and the one whose
        // loss a reader notices last.
        let (jobs_before, jobs_after) = (initial.doc.work.active(), back.doc.work.active());
        assert_eq!(jobs_after.len(), jobs_before.len());
        for (b, a) in jobs_before.iter().zip(jobs_after) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.position, b.position);
            assert_eq!(a.start_date.text, b.start_date.text, "job start date");
            assert_eq!(a.end_date.text, b.end_date.text, "job end date");
            assert_eq!(a.summary, b.summary);
            assert_eq!(a.highlights, b.highlights);
        }

        let (edu_before, edu_after) = (initial.doc.education.active(), back.doc.education.active());
        assert_eq!(edu_after.len(), edu_before.len());
        for (b, a) in edu_before.iter().zip(edu_after) {
            assert_eq!(a.institution, b.institution);
            assert_eq!(a.study_type, b.study_type);
            assert_eq!(a.start_date.text, b.start_date.text, "education start date");
            assert_eq!(a.end_date.text, b.end_date.text, "education end date");
        }

        let (skills_before, skills_after) = (initial.doc.skills.active(), back.doc.skills.active());
        assert_eq!(skills_after.len(), skills_before.len());
        for (b, a) in skills_before.iter().zip(skills_after) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.keywords, b.keywords);
        }

        let (cert_before, cert_after) = (
            initial.doc.certificates.active(),
            back.doc.certificates.active(),
        );
        assert_eq!(cert_after.len(), cert_before.len());
        for (b, a) in cert_before.iter().zip(cert_after) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.issuer, b.issuer);
            assert_eq!(a.date.text, b.date.text, "certificate date");
        }

        let (vol_before, vol_after) = (initial.doc.volunteer.active(), back.doc.volunteer.active());
        assert_eq!(vol_after.len(), vol_before.len());
        for (b, a) in vol_before.iter().zip(vol_after) {
            assert_eq!(a.organization, b.organization);
            assert_eq!(a.position, b.position);
            assert_eq!(a.start_date.text, b.start_date.text, "volunteer start date");
        }

        // `projects`, `awards`, `publications` and `languages` arrive as custom
        // sections; each has to leave through the array it came from and come
        // back under the same heading, or a whole section of the CV vanishes on
        // the way out and back.
        let titles = |doc: &dockcv_core::resume::model::ResumeDoc| {
            let mut t: Vec<String> = doc
                .custom_sections
                .iter()
                .map(|s| s.title.clone())
                .collect();
            t.sort();
            t
        };
        assert_eq!(titles(&back.doc), titles(&initial.doc));
        for section in &initial.doc.custom_sections {
            let same = back
                .doc
                .custom_sections
                .iter()
                .find(|s| s.title == section.title)
                .unwrap_or_else(|| panic!("{:?} did not survive the round trip", section.title));
            assert_eq!(
                same.content.active().len(),
                section.content.active().len(),
                "{:?} lost entries",
                section.title
            );
        }
    }
}
