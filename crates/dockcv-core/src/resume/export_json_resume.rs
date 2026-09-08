//! JSON Resume export emitter for a composed [`Resume`].
//!
//! Emits valid JSON conforming to the official JSON Resume Schema (v1.0.0):
//! https://jsonresume.org/schema
//!
//! **Dates are the one place this format is stricter than the model.** The
//! schema validates every date against
//! `^([1-2][0-9]{3}-[0-1][0-9]-[0-3][0-9]|[1-2][0-9]{3}-[0-1][0-9]|[1-2][0-9]{3})$`
//! — a year, a year and month, or a full day, and nothing else. A CV says
//! `Present`, and `Summer 2021`, and the model keeps both because that is what
//! the person typed ([`crate::resume::dates`]). A field that cannot be written
//! in that form is therefore **omitted**, not coerced and not passed through:
//! a validator rejects the whole document over one field, so a value it will
//! not accept is worse than a missing one.
//!
//! What that loses is the difference between a job that ended and a job that
//! has not, which is the most useful fact in a CV. It goes in `meta`, where
//! the spec allows extensions (`additionalProperties: true`), addressed by
//! JSON Pointer so a reader can put it back — see [`SchemaAvailability`].

use serde::{Deserialize, Serialize};

use super::export_text::strip_typst_markup;
use super::links;
use super::model::{
    Basics as CoreBasics, Certificate as CoreCert, Education as CoreEdu,
    NetworkProfile as CoreProfile, Resume, ResumeDate, SkillGroup as CoreSkill,
    Volunteer as CoreVol, Work as CoreWork,
};

/// What the file says about itself, rather than about the person.
///
/// Every field is optional and omitted when empty, because a wrong answer here
/// is worse than no answer: `signature` in particular stays empty until there
/// is something to sign with (tracks F1/F2), and a placeholder would claim a
/// provenance that does not exist.
///
/// `last_modified` is the caller's to supply. This crate has no clock, and it
/// should not grow one for a field: an export that stamped itself would differ
/// byte for byte from the same export a second later, which is exactly what a
/// file kept under version control must not do.
#[derive(Clone, Debug, Default)]
pub struct ResumeMeta {
    /// Where this file lives, if it is published somewhere.
    pub canonical: String,
    /// The résumé's own version, semver, as the spec asks.
    pub version: String,
    /// ISO 8601, e.g. `2026-09-07T12:00:00Z`.
    pub last_modified: String,
    /// Who produced the file — a platform or an agency, per the spec's example.
    pub author: String,
    /// A signature over the document: `sha256:…`, PGP, a JWT. Not yet issued.
    pub signature: String,
}

/// Export a composed [`Resume`] to a pretty-printed JSON Resume document.
pub fn export_json_resume(resume: &Resume) -> Result<String, serde_json::Error> {
    export_json_resume_with_meta(resume, &ResumeMeta::default())
}

/// The same, with what the caller knows about the file itself — the timestamp
/// above all, which this crate cannot honestly produce.
pub fn export_json_resume_with_meta(
    resume: &Resume,
    meta: &ResumeMeta,
) -> Result<String, serde_json::Error> {
    let mut schema_doc = SchemaJsonResume::from_resume(resume);
    schema_doc.meta = SchemaMeta::describe(resume, meta);
    serde_json::to_string_pretty(&schema_doc)
}

/// A date as the schema will accept it, or nothing at all.
///
/// The three precisions the pattern allows are the three [`CivilDate`] carries,
/// so the conversion is a rendering and never a guess. Anything the model could
/// not parse — `Present`, `Summer 2021`, a date in an ambiguous numeric form —
/// yields an empty string, and every date field is `skip_serializing_if
/// = "String::is_empty"`, so the field simply does not appear.
///
/// [`CivilDate`]: crate::resume::dates::CivilDate
fn iso_8601(date: &ResumeDate) -> String {
    let Some(civil) = date.parse() else {
        return String::new();
    };
    // The pattern's first character class is `[1-2]`, so a year outside this
    // range would not validate however well we formatted it.
    if !(1000..=2999).contains(&civil.year) {
        return String::new();
    }
    match (civil.month, civil.day) {
        (Some(month), Some(day)) => format!("{:04}-{month:02}-{day:02}", civil.year),
        (Some(month), None) => format!("{:04}-{month:02}", civil.year),
        _ => format!("{:04}", civil.year),
    }
}

/// Official JSON Resume Schema (v1.0.0).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaJsonResume {
    #[serde(rename = "$schema", skip_serializing_if = "String::is_empty")]
    pub schema: String,
    pub basics: SchemaBasics,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub work: Vec<SchemaWork>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volunteer: Vec<SchemaVolunteer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub education: Vec<SchemaEducation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub awards: Vec<SchemaAward>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub certificates: Vec<SchemaCertificate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publications: Vec<SchemaPublication>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<SchemaSkill>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<SchemaLanguage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interests: Vec<SchemaInterest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<SchemaReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projects: Vec<SchemaProject>,
    #[serde(default, skip_serializing_if = "SchemaMeta::is_empty")]
    pub meta: SchemaMeta,
}

/// What the document says about itself.
///
/// The spec defines `canonical`, `version` and `lastModified` here and allows
/// anything else (`additionalProperties: true`), which is the sanctioned place
/// for what the rest of the schema has no field for. Two things go in on that
/// footing: who generated the file, and what the date fields could not say.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaMeta {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub canonical: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_modified: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    /// A signature over the document. Empty until there is one to make —
    /// a field that always said `unsigned` would be noise, and one that said
    /// anything else would be a lie.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub signature: String,
    /// The tool that wrote the file. Not in the spec; `additionalProperties`
    /// is where a producer identifies itself, and a reader that meets a
    /// malformed document is owed the name of whatever produced it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub generator: String,
    #[serde(default, skip_serializing_if = "SchemaAvailability::is_empty")]
    pub availability: SchemaAvailability,
}

/// What `endDate` could not carry.
///
/// A job with no end is the most useful fact in a CV and the one the schema's
/// date pattern forbids saying, so it is said here instead: `current` for the
/// question a reader actually asks, and `ongoing` naming the entries by JSON
/// Pointer (RFC 6901) so the omission can be undone exactly.
///
/// ```json
/// "availability": { "current": true, "ongoing": ["/work/0", "/volunteer/1"] }
/// ```
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaAvailability {
    /// Whether any **work** entry is still open. Volunteering and study do not
    /// answer "is this person employed", which is what a reader means by it.
    #[serde(default)]
    pub current: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ongoing: Vec<String>,
}

impl SchemaAvailability {
    pub fn is_empty(&self) -> bool {
        !self.current && self.ongoing.is_empty()
    }
}

impl SchemaMeta {
    pub fn is_empty(&self) -> bool {
        self.canonical.is_empty()
            && self.version.is_empty()
            && self.last_modified.is_empty()
            && self.author.is_empty()
            && self.signature.is_empty()
            && self.generator.is_empty()
            && self.availability.is_empty()
    }

    /// Build the block from what the caller knows plus what the résumé shows.
    fn describe(resume: &Resume, meta: &ResumeMeta) -> Self {
        // An entry is ongoing when its end date **says so**. An empty end date
        // is an empty end date: a CV leaves one blank for a one-day course as
        // readily as for a job still held, and reading it as "still there"
        // would put a claim in the file the person never made.
        let mut ongoing = Vec::new();
        for (index, work) in resume.work.iter().enumerate() {
            if work.end_date.names_the_present() {
                ongoing.push(format!("/work/{index}"));
            }
        }
        let current = !ongoing.is_empty();
        for (index, volunteer) in resume.volunteer.iter().enumerate() {
            if volunteer.end_date.names_the_present() {
                ongoing.push(format!("/volunteer/{index}"));
            }
        }
        for (index, education) in resume.education.iter().enumerate() {
            if education.end_date.names_the_present() {
                ongoing.push(format!("/education/{index}"));
            }
        }

        Self {
            canonical: meta.canonical.clone(),
            version: meta.version.clone(),
            last_modified: meta.last_modified.clone(),
            author: meta.author.clone(),
            signature: meta.signature.clone(),
            generator: format!("DockCV {}", env!("CARGO_PKG_VERSION")),
            availability: SchemaAvailability { current, ongoing },
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaBasics {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(default, skip_serializing_if = "SchemaLocation::is_empty")]
    pub location: SchemaLocation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<SchemaProfile>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaLocation {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub address: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub postal_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub city: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub region: String,
}

impl SchemaLocation {
    pub fn is_empty(&self) -> bool {
        self.address.is_empty()
            && self.postal_code.is_empty()
            && self.city.is_empty()
            && self.country_code.is_empty()
            && self.region.is_empty()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaProfile {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub network: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaWork {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub position: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub start_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub end_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub location: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaVolunteer {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub organization: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub position: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub start_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub end_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaEducation {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub institution: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub area: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub study_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub start_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub end_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub score: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub courses: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaAward {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub awarder: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaCertificate {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub issuer: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaPublication {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub publisher: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub release_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaSkill {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub level: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaLanguage {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fluency: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaInterest {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaReference {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reference: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaProject {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub start_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub end_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

impl SchemaJsonResume {
    pub fn from_resume(r: &Resume) -> Self {
        let mut doc = Self {
            schema: "https://raw.githubusercontent.com/jsonresume/resume-schema/v1.0.0/schema.json"
                .into(),
            basics: convert_basics(&r.basics),
            work: r.work.iter().map(convert_work).collect(),
            volunteer: r.volunteer.iter().map(convert_volunteer).collect(),
            education: r.education.iter().map(convert_education).collect(),
            certificates: r.certificates.iter().map(convert_certificate).collect(),
            skills: r.skills.iter().map(convert_skill).collect(),
            awards: Vec::new(),
            publications: Vec::new(),
            languages: Vec::new(),
            interests: Vec::new(),
            references: Vec::new(),
            projects: Vec::new(),
            // Filled by `export_json_resume_with_meta`, which is the only place
            // that knows what the caller can say about the file.
            meta: SchemaMeta::default(),
        };

        // Map custom sections to the appropriate JSON Resume array
        for cs in &r.custom_sections {
            let lower_title = cs.title.trim().to_lowercase();
            if lower_title == "publications" || lower_title == "papers" {
                for e in &cs.entries {
                    doc.publications.push(SchemaPublication {
                        name: e.title.clone(),
                        publisher: e.subtitle.clone(),
                        release_date: iso_8601(&e.start_date),
                        url: uri(&e.url),
                        summary: e
                            .highlights
                            .iter()
                            .map(|h| strip_typst_markup(h))
                            .collect::<Vec<_>>()
                            .join(" "),
                    });
                }
            } else if lower_title == "awards" || lower_title == "honors" {
                for e in &cs.entries {
                    doc.awards.push(SchemaAward {
                        title: e.title.clone(),
                        awarder: e.subtitle.clone(),
                        date: iso_8601(&e.start_date),
                        summary: e
                            .highlights
                            .iter()
                            .map(|h| strip_typst_markup(h))
                            .collect::<Vec<_>>()
                            .join(" "),
                    });
                }
            } else if lower_title == "languages" {
                for e in &cs.entries {
                    doc.languages.push(SchemaLanguage {
                        language: e.title.clone(),
                        fluency: e.subtitle.clone(),
                    });
                }
            } else {
                // Default: map custom section into projects
                for e in &cs.entries {
                    doc.projects.push(SchemaProject {
                        name: e.title.clone(),
                        description: e.subtitle.clone(),
                        start_date: iso_8601(&e.start_date),
                        end_date: iso_8601(&e.end_date),
                        url: uri(&e.url),
                        highlights: e.highlights.iter().map(|h| strip_typst_markup(h)).collect(),
                        keywords: Vec::new(),
                    });
                }
            }
        }

        doc
    }
}

/// A URL field the way JSON Resume declares it — the schema says `"format":
/// "uri"`, and a consumer that renders `dtu.dk` into an `href` produces exactly
/// the dead link the PDF did. What cannot be made absolute is passed through
/// unchanged rather than dropped: this is an interchange file, and it should
/// lose nothing on the way out.
fn uri(raw: &str) -> String {
    links::href(raw).unwrap_or_else(|| raw.to_string())
}

fn convert_basics(b: &CoreBasics) -> SchemaBasics {
    SchemaBasics {
        name: b.name.clone(),
        label: b.label.clone(),
        image: String::new(),
        email: b.email.clone(),
        phone: b.phone.clone(),
        url: uri(&b.url),
        summary: strip_typst_markup(&b.summary),
        location: parse_location(&b.location),
        profiles: b.profiles.iter().map(convert_profile).collect(),
    }
}

fn parse_location(loc: &str) -> SchemaLocation {
    if loc.trim().is_empty() {
        return SchemaLocation::default();
    }
    // Location in DockCV is typically a city/region/country string
    let parts: Vec<&str> = loc.split(',').map(str::trim).collect();
    if parts.len() == 1 {
        SchemaLocation {
            city: parts[0].to_string(),
            ..Default::default()
        }
    } else if parts.len() == 2 {
        SchemaLocation {
            city: parts[0].to_string(),
            region: parts[1].to_string(),
            ..Default::default()
        }
    } else {
        SchemaLocation {
            city: parts[0].to_string(),
            region: parts[1].to_string(),
            country_code: parts[2].to_string(),
            ..Default::default()
        }
    }
}

fn convert_profile(p: &CoreProfile) -> SchemaProfile {
    SchemaProfile {
        network: p.network.clone(),
        username: p.username.clone(),
        url: uri(&p.url),
    }
}

fn convert_work(w: &CoreWork) -> SchemaWork {
    SchemaWork {
        name: w.name.clone(),
        position: w.position.clone(),
        url: uri(&w.url),
        start_date: iso_8601(&w.start_date),
        end_date: iso_8601(&w.end_date),
        summary: strip_typst_markup(&w.summary),
        highlights: w.highlights.iter().map(|h| strip_typst_markup(h)).collect(),
        location: w.location.clone(),
    }
}

fn convert_volunteer(v: &CoreVol) -> SchemaVolunteer {
    SchemaVolunteer {
        organization: v.organization.clone(),
        position: v.position.clone(),
        url: uri(&v.url),
        start_date: iso_8601(&v.start_date),
        end_date: iso_8601(&v.end_date),
        summary: String::new(),
        highlights: v.highlights.iter().map(|h| strip_typst_markup(h)).collect(),
    }
}

fn convert_education(e: &CoreEdu) -> SchemaEducation {
    SchemaEducation {
        institution: e.institution.clone(),
        url: uri(&e.url),
        area: String::new(),
        study_type: e.study_type.clone(),
        start_date: iso_8601(&e.start_date),
        end_date: iso_8601(&e.end_date),
        score: String::new(),
        courses: e.highlights.iter().map(|h| strip_typst_markup(h)).collect(),
    }
}

fn convert_certificate(c: &CoreCert) -> SchemaCertificate {
    SchemaCertificate {
        name: c.name.clone(),
        date: iso_8601(&c.date),
        issuer: c.issuer.clone(),
        url: uri(&c.url),
    }
}

fn convert_skill(s: &CoreSkill) -> SchemaSkill {
    SchemaSkill {
        name: s.name.clone(),
        level: String::new(),
        keywords: s.keywords.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::export_walk::sample_resume;

    /// Every date in the file matches the pattern the schema validates with.
    ///
    /// Written out here rather than referred to, because it *is* the contract:
    /// a validator rejects the whole document over one field, and `Present` —
    /// which every current job carries — fails it. This walks the emitted JSON
    /// rather than the emitter, so a date field added later is covered without
    /// anyone remembering to add it.
    #[test]
    fn no_date_reaches_the_file_that_the_schema_would_refuse() {
        const SCHEMA_DATE: &str =
            r"^([1-2][0-9]{3}-[0-1][0-9]-[0-3][0-9]|[1-2][0-9]{3}-[0-1][0-9]|[1-2][0-9]{3})$";
        let pattern = regex::Regex::new(SCHEMA_DATE).expect("the schema's own pattern");

        let mut resume = sample_resume();
        // The shapes a CV actually holds and the schema cannot take.
        resume.work[0].end_date = ResumeDate::new("Present");
        resume.education[0].end_date = ResumeDate::new("Summer 2021");
        resume.certificates[0].date = ResumeDate::new("sometime in 2022");

        let json = export_json_resume(&resume).expect("serializes");
        let document: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

        let mut checked = 0;
        walk_dates(&document, &mut |path, value| {
            checked += 1;
            assert!(
                pattern.is_match(value),
                "{path} = {value:?} would fail the schema's date pattern"
            );
        });
        assert!(checked > 0, "the fixture has dates; none were emitted");

        // Not coerced into some nearby day, either: absent.
        assert!(!json.contains("Present"));
        assert!(!json.contains("Summer 2021"));
        assert!(!json.contains("sometime in 2022"));
    }

    /// Walk every `startDate` / `endDate` / `date` / `releaseDate` string in a
    /// JSON Resume document, wherever the schema puts one.
    fn walk_dates(value: &serde_json::Value, visit: &mut impl FnMut(&str, &str)) {
        fn go(value: &serde_json::Value, path: &str, visit: &mut impl FnMut(&str, &str)) {
            match value {
                serde_json::Value::Object(fields) => {
                    for (key, child) in fields {
                        let here = format!("{path}/{key}");
                        match (key.as_str(), child.as_str()) {
                            ("startDate" | "endDate" | "date" | "releaseDate", Some(text)) => {
                                visit(&here, text)
                            }
                            _ => go(child, &here, visit),
                        }
                    }
                }
                serde_json::Value::Array(items) => {
                    for (index, child) in items.iter().enumerate() {
                        go(child, &format!("{path}/{index}"), visit);
                    }
                }
                _ => {}
            }
        }
        go(value, "", visit);
    }

    /// What the date field could not say, said in `meta` — and nowhere else.
    #[test]
    fn a_job_that_has_not_ended_is_recorded_in_meta() {
        let mut resume = sample_resume();
        resume.work[0].end_date = ResumeDate::new("Present");
        resume.volunteer[0].end_date = ResumeDate::new("Present");

        let json = export_json_resume(&resume).expect("serializes");
        let document: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

        assert!(
            document["work"][0]["endDate"].is_null(),
            "an end date the schema refuses has to be absent, not blank"
        );
        assert_eq!(document["meta"]["availability"]["current"], true);
        assert_eq!(
            document["meta"]["availability"]["ongoing"],
            serde_json::json!(["/work/0", "/volunteer/0"]),
            "the entries are named by JSON Pointer so the omission can be undone"
        );

        // A CV with nothing open says nothing: an `availability` block that is
        // always there carries no information.
        let ended = sample_resume();
        let json = export_json_resume(&ended).expect("serializes");
        let document: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert!(document["meta"]["availability"].is_null());
    }

    /// Volunteering is not employment, and `current` answers the question a
    /// reader is asking.
    #[test]
    fn only_work_makes_a_person_currently_employed() {
        let mut resume = sample_resume();
        resume.volunteer[0].end_date = ResumeDate::new("Present");

        let json = export_json_resume(&resume).expect("serializes");
        let document: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(document["meta"]["availability"]["current"], false);
        assert_eq!(
            document["meta"]["availability"]["ongoing"],
            serde_json::json!(["/volunteer/0"])
        );
    }

    /// The caller's timestamp reaches the file; nothing invents one.
    #[test]
    fn the_file_says_what_made_it_and_when_the_caller_knows() {
        let plain = export_json_resume(&sample_resume()).expect("serializes");
        let document: serde_json::Value = serde_json::from_str(&plain).expect("valid JSON");
        assert!(
            document["meta"]["lastModified"].is_null(),
            "this crate has no clock and must not pretend to one"
        );
        assert!(document["meta"]["generator"]
            .as_str()
            .is_some_and(|g| g.starts_with("DockCV ")));
        assert!(
            document["meta"]["signature"].is_null(),
            "an unsigned document must not carry a signature field"
        );

        let meta = ResumeMeta {
            last_modified: "2026-09-07T12:00:00Z".into(),
            author: "Recruitment Agency".into(),
            canonical: "https://example.com/resume.json".into(),
            ..Default::default()
        };
        let stamped = export_json_resume_with_meta(&sample_resume(), &meta).expect("serializes");
        let document: serde_json::Value = serde_json::from_str(&stamped).expect("valid JSON");
        assert_eq!(document["meta"]["lastModified"], "2026-09-07T12:00:00Z");
        assert_eq!(document["meta"]["author"], "Recruitment Agency");
        assert_eq!(
            document["meta"]["canonical"],
            "https://example.com/resume.json"
        );
    }

    #[test]
    fn json_resume_export_serializes_valid_schema() {
        let resume = sample_resume();
        let json_str = export_json_resume(&resume).expect("Export JSON should succeed");

        assert!(json_str.contains("\"name\": \"Albert Einstein\""));
        assert!(json_str.contains("\"label\": \"Principal Systems Architect\""));
        assert!(json_str.contains("\"email\": \"albert@example.com\""));
        assert!(json_str.contains("\"city\": \"San Francisco\""));
        assert!(json_str.contains("\"region\": \"CA\""));
        assert!(json_str.contains("\"countryCode\": \"US\""));
        assert!(json_str.contains("\"network\": \"GitHub\""));
        assert!(json_str.contains("\"name\": \"Tech Corp\""));
        assert!(json_str.contains("\"institution\": \"State University\""));
        assert!(json_str.contains("\"courses\""));
        assert!(json_str.contains("\"publications\""));
        assert!(json_str.contains("\"projects\""));
        assert!(json_str.contains("\"DockCV\""));

        // Verify that it deserializes cleanly into SchemaJsonResume
        let parsed: SchemaJsonResume =
            serde_json::from_str(&json_str).expect("Valid JSON Resume structure");
        assert_eq!(parsed.basics.name, "Albert Einstein");
        assert_eq!(parsed.work.len(), 1);
        assert_eq!(parsed.education.len(), 1);
        assert_eq!(parsed.skills.len(), 1);
        assert_eq!(parsed.certificates.len(), 1);
        assert_eq!(parsed.volunteer.len(), 1);
        assert_eq!(parsed.publications.len(), 1);
        assert_eq!(parsed.projects.len(), 1);
    }
}
