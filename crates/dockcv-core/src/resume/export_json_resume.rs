//! JSON Resume export emitter for a composed [`Resume`].
//!
//! Emits valid JSON conforming to the official JSON Resume Schema (v1.0.0):
//! https://jsonresume.org/schema

use serde::{Deserialize, Serialize};

use super::export_text::strip_typst_markup;
use super::model::{
    Basics as CoreBasics, Certificate as CoreCert, Education as CoreEdu,
    NetworkProfile as CoreProfile, Resume, SkillGroup as CoreSkill, Volunteer as CoreVol,
    Work as CoreWork,
};

/// Export a composed [`Resume`] to pretty-printed JSON string conforming to JSON Resume schema.
pub fn export_json_resume(resume: &Resume) -> Result<String, serde_json::Error> {
    let schema_doc = SchemaJsonResume::from_resume(resume);
    serde_json::to_string_pretty(&schema_doc)
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
        };

        // Map custom sections to the appropriate JSON Resume array
        for cs in &r.custom_sections {
            let lower_title = cs.title.trim().to_lowercase();
            if lower_title == "publications" || lower_title == "papers" {
                for e in &cs.entries {
                    doc.publications.push(SchemaPublication {
                        name: e.title.clone(),
                        publisher: e.subtitle.clone(),
                        release_date: e.start_date.text.clone(),
                        url: e.url.clone(),
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
                        date: e.start_date.text.clone(),
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
                        start_date: e.start_date.text.clone(),
                        end_date: e.end_date.text.clone(),
                        url: e.url.clone(),
                        highlights: e
                            .highlights
                            .iter()
                            .map(|h| strip_typst_markup(h))
                            .collect(),
                        keywords: Vec::new(),
                    });
                }
            }
        }

        doc
    }
}

fn convert_basics(b: &CoreBasics) -> SchemaBasics {
    SchemaBasics {
        name: b.name.clone(),
        label: b.label.clone(),
        image: String::new(),
        email: b.email.clone(),
        phone: b.phone.clone(),
        url: b.url.clone(),
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
        url: p.url.clone(),
    }
}

fn convert_work(w: &CoreWork) -> SchemaWork {
    SchemaWork {
        name: w.name.clone(),
        position: w.position.clone(),
        url: String::new(),
        start_date: w.start_date.text.clone(),
        end_date: w.end_date.text.clone(),
        summary: strip_typst_markup(&w.summary),
        highlights: w.highlights.iter().map(|h| strip_typst_markup(h)).collect(),
        location: w.location.clone(),
    }
}

fn convert_volunteer(v: &CoreVol) -> SchemaVolunteer {
    SchemaVolunteer {
        organization: v.organization.clone(),
        position: v.position.clone(),
        url: String::new(),
        start_date: v.start_date.text.clone(),
        end_date: v.end_date.text.clone(),
        summary: String::new(),
        highlights: v.highlights.iter().map(|h| strip_typst_markup(h)).collect(),
    }
}

fn convert_education(e: &CoreEdu) -> SchemaEducation {
    SchemaEducation {
        institution: e.institution.clone(),
        url: e.url.clone(),
        area: String::new(),
        study_type: e.study_type.clone(),
        start_date: e.start_date.text.clone(),
        end_date: e.end_date.text.clone(),
        score: String::new(),
        courses: e.highlights.iter().map(|h| strip_typst_markup(h)).collect(),
    }
}

fn convert_certificate(c: &CoreCert) -> SchemaCertificate {
    SchemaCertificate {
        name: c.name.clone(),
        date: c.date.text.clone(),
        issuer: c.issuer.clone(),
        url: c.url.clone(),
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
    use crate::resume::model::*;

    fn sample_resume() -> Resume {
        Resume {
            basics: CoreBasics {
                name: "Alexey Belochenko".into(),
                label: "Principal Systems Architect".into(),
                summary: "Experienced *systems engineer* specializing in distributed storage."
                    .into(),
                email: "alexey@example.com".into(),
                phone: "+1 555-0199".into(),
                location: "San Francisco, CA, US".into(),
                url: "https://example.com".into(),
                profiles: vec![CoreProfile {
                    network: "GitHub".into(),
                    username: "zeelex".into(),
                    url: "https://github.com/zeelex".into(),
                }],
            },
            work: vec![CoreWork {
                name: "Tech Corp".into(),
                position: "Staff Software Engineer".into(),
                location: "Mountain View, CA".into(),
                start_date: ResumeDate::new("2021-03-01"),
                end_date: ResumeDate::new("2024-01-01"),
                summary: "Lead storage engine architecture.".into(),
                highlights: vec![
                    "Designed commit log processing 50M ops/sec with sub-millisecond p99 latency."
                        .into(),
                ],
            }],
            education: vec![CoreEdu {
                institution: "State University".into(),
                study_type: "B.S. in Computer Science".into(),
                start_date: ResumeDate::new("2015-09-01"),
                end_date: ResumeDate::new("2019-06-01"),
                url: "https://university.edu".into(),
                highlights: vec!["Graduated Summa Cum Laude.".into()],
            }],
            skills: vec![CoreSkill {
                name: "Languages".into(),
                keywords: vec!["Rust".into(), "C++".into(), "Go".into()],
            }],
            certificates: vec![CoreCert {
                name: "AWS Solutions Architect".into(),
                issuer: "Amazon Web Services".into(),
                date: ResumeDate::new("2022-05-15"),
                url: "https://aws.amazon.com".into(),
            }],
            volunteer: vec![CoreVol {
                organization: "Open Source Collective".into(),
                position: "Core Maintainer".into(),
                start_date: ResumeDate::new("2020-01-01"),
                end_date: ResumeDate::new("2023-01-01"),
                highlights: vec!["Maintain networking libraries.".into()],
            }],
            custom_sections: vec![
                ComposedCustomSection {
                    id: CustomSectionId::from_u32(1),
                    title: "Publications".into(),
                    entries: vec![CustomEntry {
                        title: "High Performance Storage in Rust".into(),
                        subtitle: "ACM Systems Conference".into(),
                        start_date: ResumeDate::new("2023-11-01"),
                        end_date: ResumeDate::new(""),
                        url: "https://doi.org/10.1145/example".into(),
                        highlights: vec!["Awarded Best Paper.".into()],
                    }],
                },
                ComposedCustomSection {
                    id: CustomSectionId::from_u32(2),
                    title: "Side Projects".into(),
                    entries: vec![CustomEntry {
                        title: "DockCV".into(),
                        subtitle: "Modern CV builder in Rust".into(),
                        start_date: ResumeDate::new("2024-01-01"),
                        end_date: ResumeDate::new("Present"),
                        url: "https://dockcv.com".into(),
                        highlights: vec!["Fast typst-backed CV desktop app.".into()],
                    }],
                },
            ],
            section_titles: Vec::new(),
            section_overrides: Vec::new(),
            section_order: Vec::new(),
        }
    }

    #[test]
    fn json_resume_export_serializes_valid_schema() {
        let resume = sample_resume();
        let json_str = export_json_resume(&resume).expect("Export JSON should succeed");

        assert!(json_str.contains("\"name\": \"Alexey Belochenko\""));
        assert!(json_str.contains("\"label\": \"Principal Systems Architect\""));
        assert!(json_str.contains("\"email\": \"alexey@example.com\""));
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
        assert_eq!(parsed.basics.name, "Alexey Belochenko");
        assert_eq!(parsed.work.len(), 1);
        assert_eq!(parsed.education.len(), 1);
        assert_eq!(parsed.skills.len(), 1);
        assert_eq!(parsed.certificates.len(), 1);
        assert_eq!(parsed.volunteer.len(), 1);
        assert_eq!(parsed.publications.len(), 1);
        assert_eq!(parsed.projects.len(), 1);
    }
}
