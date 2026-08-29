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
use crate::import::model::ImportedDoc;
use crate::import::notes::{Note, Part};
use crate::resume::model::{
    Certificate, CustomEntry, Education, NetworkProfile, Resume, ResumeDoc, SkillGroup, Volunteer,
    Work,
};

/// How many archive members this will look at.
///
/// A **cap on what is read**, not a reason to refuse the file. It used to be a
/// hard reject, which is the wrong shape for a guard sitting on the primary
/// happy path: the user waited a day for that export, and "too many entries" is
/// not something they can act on. The CSVs this engine reads are a handful and
/// LinkedIn ships them at the archive root, so a bounded walk finds them.
const MAX_ZIP_ENTRIES: usize = 512;
const MAX_SINGLE_CSV_SIZE: u64 = 10 * 1024 * 1024; // 10 MB limit per CSV entry
const MAX_TOTAL_UNCOMPRESSED_SIZE: u64 = 50 * 1024 * 1024; // 50 MB limit total

pub fn import_linkedin(path: &Path) -> Result<ImportedDoc, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Could not open the archive: {e}"))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| format!("This is not a readable .zip: {e}"))?;

    let mut total_bytes_read: u64 = 0;
    let mut tables: HashMap<String, Table> = HashMap::new();
    for i in 0..zip.len().min(MAX_ZIP_ENTRIES) {
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
    // Organizations is a section DockCV already has, and the archive already
    // carries the data. It was being dropped with everything else this engine
    // did not name — the worst kind of gap, because both ends existed.
    if let Some(t) = tables.get("volunteering.csv") {
        resume.volunteer = read_volunteering(t);
    }

    // Every file this engine looked at. What is *not* here was in the archive
    // and went nowhere, which is the thing US-01 says must not happen quietly.
    const READ: [&str; 9] = [
        "profile.csv",
        "email addresses.csv",
        "positions.csv",
        "education.csv",
        "skills.csv",
        "certifications.csv",
        "projects.csv",
        "languages.csv",
        "volunteering.csv",
    ];

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
    // Structured input: nothing here was guessed from layout. The one thing the
    // document cannot say is whether a table was in the archive and produced
    // nothing — a CSV that was there and came out empty is a defect, a CSV that
    // was never exported is not.
    for (file, part) in [
        ("positions.csv", Part::Work),
        ("education.csv", Part::Education),
        ("skills.csv", Part::Skills),
        ("certifications.csv", Part::Certificates),
    ] {
        let present = tables.contains_key(file);
        let empty = match part {
            Part::Work => imported.doc.work.active().is_empty(),
            Part::Education => imported.doc.education.active().is_empty(),
            Part::Skills => imported.doc.skills.active().is_empty(),
            _ => imported.doc.certificates.active().is_empty(),
        };
        if present && empty {
            imported.note(part, Note::Empty);
        }
    }
    // Named, not counted. This is the one import path that can say exactly what
    // it left behind: the archive hands over its file names and its row counts,
    // so "Honors.csv — 4 rows" is a fact rather than an estimate.
    let mut unread: Vec<(&String, usize)> = tables
        .iter()
        .filter(|(name, table)| !READ.contains(&name.as_str()) && !table.rows.is_empty())
        .map(|(name, table)| (name, table.rows.len()))
        .collect();
    unread.sort_by(|a, b| a.0.cmp(b.0));
    imported.unplaced.extend(unread.into_iter().map(|(name, rows)| {
        format!(
            "{name} — {rows} row{} the archive carried and DockCV has no section for",
            if rows == 1 { "" } else { "s" }
        )
    }));

    imported.observe();
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
    // `Websites` is `[TYPE:OTHER:https://…]`, several of those comma-joined, or
    // a bare URL, depending on the export's vintage.
    let mut websites = websites_in(&t.get(row, "Websites")).into_iter();
    if let Some(first) = websites.next() {
        resume.basics.url = first;
    }
    // The rest are profiles rather than nothing. A person with a site *and* a
    // blog had the blog dropped, and the field is a list for a reason.
    for url in websites {
        if resume.basics.profiles.iter().any(|p| p.url == url) {
            continue;
        }
        resume.basics.profiles.push(NetworkProfile {
            network: String::new(),
            username: String::new(),
            url,
        });
    }
}

/// Every URL in a LinkedIn `Websites` cell, in the order it was written.
///
/// The old reading trimmed the trailing `]` off the *whole* cell and then split
/// on commas, which is correct for one website and wrong for two: with
/// `[TYPE:OTHER:https://a.com],[TYPE:BLOG:https://b.com]` the trim took the
/// bracket from the last entry and the split returned the first — still carrying
/// its own. The url imported as `https://a.com]` and did not resolve.
///
/// A URL here ends at the bracket that closes its entry, the comma that
/// separates it from the next, or any space. Reading it that way makes the
/// one-website and many-website cases the same case.
fn websites_in(cell: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = cell;
    while let Some(at) = rest.find("http") {
        let tail = &rest[at..];
        let url = tail
            .split(|c: char| c == ']' || c == ',' || c.is_whitespace())
            .next()
            .unwrap_or_default();
        if !url.is_empty() && !out.iter().any(|seen| seen == url) {
            out.push(url.to_string());
        }
        rest = &tail[url.len().max(1)..];
    }
    out
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

/// Volunteering, which DockCV surfaces as **Organizations**.
///
/// Columns looked up by name like everything else here, so a header LinkedIn
/// renames between exports costs its own field and nothing else — the
/// `a_missing_column_costs_only_its_own_field` guarantee.
fn read_volunteering(t: &Table) -> Vec<Volunteer> {
    t.rows
        .iter()
        .map(|row| {
            let mut highlights = as_highlights(&t.get(row, "Description"));
            // The cause is why the entry is on the CV at all, and it has no
            // field of its own here.
            let cause = t.get(row, "Cause");
            if !cause.trim().is_empty() {
                highlights.insert(0, cause);
            }
            Volunteer {
                organization: t.get(row, "Company Name"),
                position: t.get(row, "Role"),
                start_date: t.get(row, "Started On").into(),
                end_date: t.get(row, "Finished On").into(),
                highlights,
            }
        })
        .filter(|v| !(v.organization.is_empty() && v.position.is_empty()))
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
/// Which record is the header row.
///
/// LinkedIn prefixes some files with a `Notes:` sentence before the real header,
/// so the rule is "the first record that reads like a list of column names":
/// every field filled, and none of them long enough to be prose.
///
/// The `len() > 1` half is what tells a header from that preamble — and it also
/// meant a **single-column file never parsed at all**. `Skills.csv` is one
/// column (`Name`) in plenty of exports, so a person's whole skill list was
/// dropped before anything looked at it, and no error said so. A file whose
/// records are all one field wide has no ambiguity to resolve: its header is the
/// first record that looks like one.
fn find_header(records: &[Vec<String>]) -> Option<usize> {
    let looks_like_header =
        |r: &Vec<String>| !r.is_empty() && r.iter().all(|f| !f.is_empty() && f.chars().count() <= 40);
    let single_column = records.iter().all(|r| r.len() <= 1);

    records
        .iter()
        .position(|r| r.len() > 1 && looks_like_header(r))
        .or_else(|| {
            single_column
                .then(|| records.iter().position(looks_like_header))
                .flatten()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(csv: &str) -> Table {
        Table::parse(csv.as_bytes()).expect("a header row")
    }

    /// I-08. Volunteering is a section DockCV already has, and the archive
    /// already carries it — both ends existed and the data went nowhere.
    #[test]
    fn volunteering_reaches_the_organizations_section() {
        let csv = "Company Name,Role,Cause,Started On,Finished On,Description\n\
                   CoderDojo,Mentor,Science and Technology,2019,2021,\"Ran the Saturday club\"\n";
        let table = super::Table::parse(csv.as_bytes()).expect("parses");
        let entries = super::read_volunteering(&table);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].organization, "CoderDojo");
        assert_eq!(entries[0].position, "Mentor");
        assert_eq!(entries[0].start_date.text, "2019");
        // The cause leads the bullets: it is why the entry is on the CV, and it
        // has no field of its own.
        assert_eq!(
            entries[0].highlights,
            vec![
                "Science and Technology".to_string(),
                "Ran the Saturday club".to_string()
            ]
        );
    }

    /// I-10, measured. The old reading trimmed the closing bracket off the
    /// *whole* cell and then split on commas, so with two websites the first
    /// came back still carrying its own: `https://a.com]`, which does not
    /// resolve.
    #[test]
    fn a_second_website_does_not_leave_a_bracket_on_the_first() {
        assert_eq!(
            super::websites_in("[TYPE:OTHER:https://a.com]"),
            vec!["https://a.com".to_string()]
        );
        assert_eq!(
            super::websites_in("[TYPE:OTHER:https://a.com],[TYPE:BLOG:https://b.com]"),
            vec!["https://a.com".to_string(), "https://b.com".to_string()]
        );
    }

    /// Older exports write the cell as a bare URL, and some write nothing.
    #[test]
    fn a_bare_url_and_an_empty_cell_both_read() {
        assert_eq!(
            super::websites_in("https://sofiia.dev"),
            vec!["https://sofiia.dev".to_string()]
        );
        assert!(super::websites_in("").is_empty());
        assert!(super::websites_in("[TYPE:OTHER:]").is_empty());
    }

    /// The first is the person's site; the rest are profiles rather than
    /// nothing, because the field is a list for a reason.
    #[test]
    fn every_website_reaches_the_profile() {
        // Quoted, as a real export writes it — the cell contains a comma.
        let csv = "First Name,Last Name,Websites\n\
                   Sofiia,Medvedenko,\"[TYPE:OTHER:https://a.com],[TYPE:BLOG:https://b.com]\"\n";
        let table = super::Table::parse(csv.as_bytes()).expect("parses");
        let mut resume = crate::resume::model::Resume::default();
        super::read_profile(&table, &mut resume);

        assert_eq!(resume.basics.url, "https://a.com");
        assert_eq!(
            resume
                .basics
                .profiles
                .iter()
                .map(|p| p.url.as_str())
                .collect::<Vec<_>>(),
            vec!["https://b.com"]
        );
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

    /// A one-column file used to fail `find_header`'s `len() > 1` test and be
    /// skipped whole. `Skills.csv` is one column in plenty of exports, so a
    /// person's entire skill list was dropped before anything looked at it.
    #[test]
    fn a_single_column_file_still_has_a_header() {
        let t = table("Name\nRust\nKafka\nKubernetes\n");
        assert_eq!(t.column("Name"), vec!["Rust", "Kafka", "Kubernetes"]);
    }

    /// …and the preamble it was guarding against is still skipped, because the
    /// length test is what actually tells prose from a column name.
    #[test]
    fn a_note_above_a_single_column_header_is_still_skipped() {
        let t = table(
            "\"Notes: This file contains the skills listed on your profile.\"\n\
             Name\n\
             Rust\n",
        );
        assert_eq!(t.headers, vec!["Name".to_string()]);
        assert_eq!(t.column("Name"), vec!["Rust"]);
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

    /// I-08. Eight files were read and the rest of the archive was dropped in
    /// silence — including Organizations, which DockCV has a section for. What
    /// this engine does not map is now *named*, with its row count, because the
    /// archive hands over both.
    #[test]
    fn a_table_the_engine_does_not_map_is_named_rather_than_dropped() {
        let dir = std::env::temp_dir().join(format!("dockcv-linkedin-extra-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("export.zip");

        let file = std::fs::File::create(&path).expect("create");
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in [
            ("Profile.csv", "First Name,Last Name\nSofiia,Medvedenko\n"),
            ("Honors.csv", "Title,Description\nBest Paper,ACM\nRunner Up,IEEE\n"),
            ("Courses.csv", "Name\nDistributed Systems\n"),
        ] {
            use std::io::Write as _;
            zip.start_file(name, options).expect("entry");
            zip.write_all(body.as_bytes()).expect("write");
        }
        zip.finish().expect("finish");

        let imported = import_linkedin(&path).expect("the archive imports");
        assert_eq!(
            imported.doc.profile.active().name,
            "Sofiia Medvedenko",
            "the mapped table still reads"
        );

        let reported = imported.unplaced.join("\n");
        assert!(reported.contains("courses.csv — 1 row "), "{reported}");
        assert!(reported.contains("honors.csv — 2 rows "), "{reported}");
        assert!(
            !reported.contains("profile.csv"),
            "a table that *was* read must not be reported lost: {reported}"
        );

        let _ = std::fs::remove_dir_all(&dir);
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
