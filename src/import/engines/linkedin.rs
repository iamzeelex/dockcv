//! LinkedIn import: the **data export archive**, read locally.
//!
//! Not the profile URL, and not OAuth. Both were considered and neither can
//! work:
//!
//! * There is no public endpoint that returns a profile as data. What is left
//!   is the HTML behind LinkedIn's bot detection, which needs the user's own
//!   session cookie and a solved CAPTCHA — and a network call, which DockCV
//!   does not make (US-10 is a P0 trust promise, not an implementation detail).
//! * "Sign In with LinkedIn" on OpenID Connect returns name, picture, locale
//!   and email. No positions, no education. The scopes that carried those have
//!   been partner-gated since 2019. An OAuth flow would buy the two fields a
//!   user types in five seconds.
//!
//! The archive is the user's own data, handed over by LinkedIn on request
//! (Settings → Data privacy → Get a copy of your data). The network leg is the
//! browser's, not ours; the app still makes no request. And it is *structured* —
//! better material than any text recovered from a PDF's typesetting.
//!
//! Columns are looked up **by header name**. LinkedIn adds and reorders them
//! between exports, and an importer that counts positions would import a
//! person's job titles into their company names one quiet afternoon.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use crate::import::layout::without_bullet;
use crate::import::model::{Confidence, ImportedDoc};
use crate::resume::model::{
    Certificate, CustomEntry, Education, Resume, ResumeDoc, SkillGroup, Work,
};

const MAX_ZIP_ENTRIES: usize = 100;
const MAX_SINGLE_CSV_SIZE: u64 = 10 * 1024 * 1024; // 10 MB limit per CSV entry
const MAX_TOTAL_UNCOMPRESSED_SIZE: u64 = 50 * 1024 * 1024; // 50 MB limit total

pub fn import_linkedin(path: &Path) -> Result<ImportedDoc, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Could not open the archive: {e}"))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| format!("This is not a readable .zip: {e}"))?;

    if zip.len() > MAX_ZIP_ENTRIES {
        return Err(format!(
            "ZIP archive contains too many entries ({}, max allowed {MAX_ZIP_ENTRIES})",
            zip.len()
        ));
    }

    let mut total_bytes_read: u64 = 0;
    let mut tables: HashMap<String, Table> = HashMap::new();
    for i in 0..zip.len() {
        let Ok(entry) = zip.by_index(i) else {
            continue;
        };
        if !entry.is_file() {
            continue;
        }
        // LinkedIn ships the CSVs at the archive root, but a user who re-zips
        // an unpacked folder gets a prefix. Match on the file name alone.
        let name = entry
            .name()
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .to_lowercase();
        if !name.ends_with(".csv") {
            continue;
        }
        if entry.size() > MAX_SINGLE_CSV_SIZE {
            return Err(format!(
                "Archive entry '{name}' size ({} bytes) exceeds limit ({MAX_SINGLE_CSV_SIZE} bytes)",
                entry.size()
            ));
        }

        let mut bytes = Vec::new();
        let bytes_read = entry
            .take(MAX_SINGLE_CSV_SIZE + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Error reading archive entry '{name}': {e}"))? as u64;

        if bytes_read > MAX_SINGLE_CSV_SIZE {
            return Err(format!(
                "Archive entry '{name}' expanded beyond maximum allowed size"
            ));
        }

        total_bytes_read += bytes_read;
        if total_bytes_read > MAX_TOTAL_UNCOMPRESSED_SIZE {
            return Err(format!(
                "Archive total uncompressed data exceeds maximum limit of {} MB",
                MAX_TOTAL_UNCOMPRESSED_SIZE / (1024 * 1024)
            ));
        }

        if let Some(table) = Table::parse(&bytes) {
            tables.insert(name, table);
        }
    }

    if tables.is_empty() {
        return Err(
            "No CSV files in this archive. Export from LinkedIn: Settings → Data privacy → \
             Get a copy of your data."
                .to_string(),
        );
    }

    let mut resume = Resume::default();
    let mut projects = Vec::new();
    let mut languages = Vec::new();

    if let Some(t) = tables.get("profile.csv") {
        read_profile(t, &mut resume);
    }
    if let Some(t) = tables.get("email addresses.csv") {
        read_email(t, &mut resume);
    }
    if let Some(t) = tables.get("positions.csv") {
        resume.work = read_positions(t);
    }
    if let Some(t) = tables.get("education.csv") {
        resume.education = read_education(t);
    }
    if let Some(t) = tables.get("skills.csv") {
        let keywords: Vec<String> = t.column("Name");
        if !keywords.is_empty() {
            // LinkedIn keeps skills as one flat list — it has no categories to
            // lose. One unnamed group is the honest shape; inventing categories
            // here would be inventing data.
            resume.skills.push(SkillGroup {
                name: String::new(),
                keywords,
            });
        }
    }
    if let Some(t) = tables.get("certifications.csv") {
        resume.certificates = read_certifications(t);
    }
    if let Some(t) = tables.get("projects.csv") {
        projects = read_projects(t);
    }
    if let Some(t) = tables.get("languages.csv") {
        languages = read_languages(t);
    }

    let mut doc = ResumeDoc::from_resume(resume, "Base");
    for (title, entries) in [("Projects", projects), ("Languages", languages)] {
        if entries.is_empty() {
            continue;
        }
        let id = doc.add_custom_section(title);
        if let Some(section) = doc.custom_section_mut(id) {
            *section.content.active_mut() = entries;
        }
    }

    let mut imported = ImportedDoc::new("LinkedIn export", doc);
    // Structured input: nothing here was guessed from layout, so nothing is
    // flagged for review on suspicion. A section is only doubtful when the
    // archive did not carry it.
    for (file, key) in [
        ("positions.csv", "work"),
        ("education.csv", "education"),
        ("skills.csv", "skills"),
    ] {
        if !tables.contains_key(file) {
            imported.set_confidence(key, Confidence::Low);
        }
    }
    if imported.doc.profile.active().name.is_empty() {
        imported.set_confidence("profile.name", Confidence::Low);
    }
    Ok(imported)
}

fn read_profile(t: &Table, resume: &mut Resume) {
    let Some(row) = t.rows.first() else {
        return;
    };
    let name = [t.get(row, "First Name"), t.get(row, "Last Name")]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    resume.basics.name = name;
    resume.basics.label = t.get(row, "Headline");
    resume.basics.summary = t.get(row, "Summary");
    resume.basics.location = first_non_empty(&[
        t.get(row, "Geo Location"),
        t.get(row, "Location"),
        t.get(row, "Address"),
    ]);
    // `Websites` is `[TYPE:OTHER:https://…]` or a bare URL, depending on the
    // export's vintage. Take the first thing that looks like one.
    let websites = t.get(row, "Websites");
    if let Some(at) = websites.find("http") {
        resume.basics.url = websites[at..]
            .trim_end_matches([']', ',', ' '])
            .split(',')
            .next()
            .unwrap_or_default()
            .to_string();
    }
}

fn read_email(t: &Table, resume: &mut Resume) {
    // Several addresses may be listed; the one marked primary is the one on the
    // CV. Falls back to the first row when the column is absent.
    let primary = t
        .rows
        .iter()
        .find(|r| t.get(r, "Primary").eq_ignore_ascii_case("yes"))
        .or_else(|| t.rows.first());
    if let Some(row) = primary {
        let email = t.get(row, "Email Address");
        if !email.is_empty() {
            resume.basics.email = email;
        }
    }
}

fn read_positions(t: &Table) -> Vec<Work> {
    t.rows
        .iter()
        .map(|row| Work {
            name: t.get(row, "Company Name"),
            position: t.get(row, "Title"),
            location: t.get(row, "Location"),
            start_date: t.get(row, "Started On").into(),
            end_date: t.get(row, "Finished On").into(),
            highlights: as_highlights(&t.get(row, "Description")),
            ..Default::default()
        })
        .filter(|w| !(w.name.is_empty() && w.position.is_empty()))
        .collect()
}

fn read_education(t: &Table) -> Vec<Education> {
    t.rows
        .iter()
        .map(|row| {
            let mut highlights = as_highlights(&t.get(row, "Notes"));
            highlights.extend(as_highlights(&t.get(row, "Activities")));
            Education {
                institution: t.get(row, "School Name"),
                study_type: t.get(row, "Degree Name"),
                start_date: t.get(row, "Start Date").into(),
                end_date: t.get(row, "End Date").into(),
                highlights,
                ..Default::default()
            }
        })
        .filter(|e| !e.institution.is_empty())
        .collect()
}

fn read_certifications(t: &Table) -> Vec<Certificate> {
    t.rows
        .iter()
        .map(|row| Certificate {
            name: t.get(row, "Name"),
            issuer: t.get(row, "Authority"),
            date: t.get(row, "Started On").into(),
            url: t.get(row, "Url"),
        })
        .filter(|c| !c.name.is_empty())
        .collect()
}

fn read_projects(t: &Table) -> Vec<CustomEntry> {
    t.rows
        .iter()
        .map(|row| CustomEntry {
            title: t.get(row, "Title"),
            subtitle: String::new(),
            start_date: t.get(row, "Started On").into(),
            end_date: t.get(row, "Finished On").into(),
            url: t.get(row, "Url"),
            highlights: as_highlights(&t.get(row, "Description")),
        })
        .filter(|p| !p.title.is_empty())
        .collect()
}

fn read_languages(t: &Table) -> Vec<CustomEntry> {
    t.rows
        .iter()
        .map(|row| CustomEntry {
            title: t.get(row, "Name"),
            subtitle: t.get(row, "Proficiency"),
            ..Default::default()
        })
        .filter(|l| !l.title.is_empty())
        .collect()
}

/// A LinkedIn description is one free-text field holding what the author wrote
/// as a list. Each line becomes a bullet; the glyphs they typed are dropped so
/// the renderer draws its own.
fn as_highlights(description: &str) -> Vec<String> {
    description
        .lines()
        .map(|l| without_bullet(l.trim()).trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn first_non_empty(candidates: &[String]) -> String {
    candidates
        .iter()
        .find(|c| !c.is_empty())
        .cloned()
        .unwrap_or_default()
}

/// A CSV read by column *name*.
struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    fn parse(bytes: &[u8]) -> Option<Self> {
        // LinkedIn prefixes some files with a "Notes:" preamble before the real
        // header row. `flexible` keeps the reader from failing on it, and the
        // header row is found by looking for the first record that names a
        // column we recognise — see `find_header`.
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .has_headers(false)
            .from_reader(bytes);
        let records: Vec<Vec<String>> = reader
            .records()
            .filter_map(|r| r.ok())
            .map(|r| r.iter().map(|f| f.trim().to_string()).collect())
            .collect();

        let header_at = find_header(&records)?;
        let headers = records[header_at].clone();
        let rows = records[header_at + 1..]
            .iter()
            .filter(|r| r.iter().any(|f| !f.is_empty()))
            .cloned()
            .collect();
        Some(Self { headers, rows })
    }

    fn get(&self, row: &[String], column: &str) -> String {
        self.headers
            .iter()
            .position(|h| h.eq_ignore_ascii_case(column))
            .and_then(|i| row.get(i))
            .cloned()
            .unwrap_or_default()
    }

    fn column(&self, name: &str) -> Vec<String> {
        self.rows
            .iter()
            .map(|r| self.get(r, name))
            .filter(|v| !v.is_empty())
            .collect()
    }
}

/// The row that names the columns.
///
/// Usually the first, but several files open with a free-text note LinkedIn
/// adds about the export. A header row is one whose fields are all short,
/// non-empty labels — a note is one long sentence in a single field.
fn find_header(records: &[Vec<String>]) -> Option<usize> {
    records.iter().position(|r| {
        r.len() > 1 && r.iter().all(|f| !f.is_empty() && f.chars().count() <= 40)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(csv: &str) -> Table {
        Table::parse(csv.as_bytes()).expect("a header row")
    }

    #[test]
    fn columns_are_found_by_name_not_by_position() {
        // Same data, columns reordered — as LinkedIn has done between exports.
        let a = table("Company Name,Title,Started On\nGE Vernova,Software Developer,Aug 2024\n");
        let b = table("Title,Started On,Company Name\nSoftware Developer,Aug 2024,GE Vernova\n");

        for t in [&a, &b] {
            let work = read_positions(t);
            assert_eq!(work[0].name, "GE Vernova");
            assert_eq!(work[0].position, "Software Developer");
            assert_eq!(work[0].start_date.text, "Aug 2024");
        }
    }

    /// A column the export drops must leave a blank field, never shift every
    /// other value one place left.
    #[test]
    fn a_missing_column_costs_only_its_own_field() {
        let t = table("Company Name,Title\nGE Vernova,Software Developer\n");
        let work = read_positions(&t);
        assert_eq!(work[0].name, "GE Vernova");
        assert!(work[0].location.is_empty());
        assert!(work[0].start_date.text.is_empty());
    }

    #[test]
    fn a_description_becomes_one_bullet_per_line() {
        let t = table(
            "Company Name,Title,Description\n\
             Acme,Engineer,\"• Built the thing.\n• Shipped it.\n\nMeasured it.\"\n",
        );
        let work = read_positions(&t);
        assert_eq!(
            work[0].highlights,
            vec!["Built the thing.", "Shipped it.", "Measured it."]
        );
    }

    #[test]
    fn the_primary_address_is_the_one_that_reaches_the_cv() {
        let t = table(
            "Email Address,Primary\nold@example.com,No\nhi@zeelex.me,Yes\n",
        );
        let mut resume = Resume::default();
        read_email(&t, &mut resume);
        assert_eq!(resume.basics.email, "hi@zeelex.me");
    }

    #[test]
    fn a_note_above_the_header_row_is_skipped() {
        let t = table(
            "\"Notes: This file contains your positions as of the export date.\"\n\
             Company Name,Title\n\
             Acme,Engineer\n",
        );
        assert_eq!(read_positions(&t)[0].name, "Acme");
    }

    #[test]
    fn an_archive_without_csvs_says_where_to_get_one() {
        let dir = std::env::temp_dir().join("dockcv-linkedin-empty");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("empty.zip");
        // Smallest valid zip: an end-of-central-directory record and nothing else.
        std::fs::write(
            &path,
            [
                0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        )
        .expect("write");

        let err = match import_linkedin(&path) {
            Err(e) => e,
            Ok(_) => panic!("an empty archive is an error"),
        };
        assert!(err.contains("Data privacy"), "{err}");
        let _ = std::fs::remove_file(&path);
    }
}
