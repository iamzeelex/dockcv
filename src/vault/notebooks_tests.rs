//! The vault's own files: the library, the diary, the board, the profiles.
//!
//! Split from `vault.rs` by C15. One notebook per subject, each atomic, each
//! with a migration for the shape it used to have.

use crate::resume::model::{Library, Work};
use crate::resume::{altacv, model::ResumeDoc};

#[test]
fn library_round_trip() {
    let dir = std::env::temp_dir().join(format!("dockcv-lib-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let mut library = Library::default();
    library.work.push(Work {
        position: "Engineer".into(),
        name: "Acme".into(),
        ..Default::default()
    });
    super::save_library(&dir, &library).expect("save library");

    let back = super::load_library(&dir);
    assert_eq!(back.work.len(), 1);
    assert_eq!(back.work[0].position, "Engineer");

    std::fs::remove_dir_all(&dir).ok();
}

/// A board written by the build that had the `status = ""` bug loads as a
/// board of wishlist cards, once, instead of warning about it on every
/// read for the life of the vault. The rule that keeps an unknown word
/// verbatim is untouched — `""` is the one value nobody typed.
#[test]
fn an_empty_status_is_filled_in_rather_than_warned_about_forever() {
    use crate::resume::model::ApplicationStatus;

    let dir = std::env::temp_dir().join(format!(
        "dockcv_status_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    std::fs::write(
        super::applications_path(&dir),
        "[[entries]]\ncompany = \"Northwind\"\nrole = \"Engineer\"\nstatus = \"\"\n\n\
         [[entries]]\ncompany = \"Acme\"\nrole = \"Lead\"\nstatus = \"ofer\"\n",
    )
    .expect("write");

    let board = super::load_applications(&dir);
    assert_eq!(board.entries.len(), 2);
    assert_eq!(
        board.entries[0].status_word,
        ApplicationStatus::Wishlist.word()
    );
    assert!(board.entries[0].status_is_recognised());
    // And a word a person really did type is still theirs.
    assert_eq!(board.entries[1].status_word, "ofer");
    assert!(!board.entries[1].status_is_recognised());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn applications_round_trip() {
    use crate::resume::model::{Application, ApplicationStatus, Applications};

    let dir = std::env::temp_dir().join(format!("dockcv-apps-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let mut applications = Applications::default();
    applications.entries.push(Application {
        company: "Bramble Tech".into(),
        role: "Staff Engineer".into(),
        status_word: ApplicationStatus::Interviewing.word().into(),
        sent_as: Some(crate::resume::model::SentCv {
            document: "albert-senior-swe".into(),
            preset: "FAANG · concise".into(),
        }),
        ..Default::default()
    });
    super::save_applications(&dir, &applications).expect("save applications");

    let back = super::load_applications(&dir);
    assert_eq!(back.entries.len(), 1);
    assert_eq!(back.entries[0].company, "Bramble Tech");
    assert_eq!(back.entries[0].status(), ApplicationStatus::Interviewing);

    std::fs::remove_dir_all(&dir).ok();
}

/// No `applications.toml` on disk at all (every vault written before this
/// feature) must load as an empty board, never an error.
#[test]
fn missing_applications_file_loads_as_empty() {
    let dir = std::env::temp_dir().join(format!("dockcv-apps-missing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let applications = super::load_applications(&dir);
    assert!(applications.entries.is_empty());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_snapshot_writes_bytes_and_sanitizes_the_file_name() {
    let dir = std::env::temp_dir().join(format!("dockcv-snapshot-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let file_name = super::save_snapshot(&dir, b"%PDF-fake", "Bramble Tech / EU", 1)
        .expect("save snapshot");
    assert_eq!(file_name, "bramble-tech-eu-v1.pdf");
    let bytes = std::fs::read(super::snapshots_dir(&dir).join(&file_name)).unwrap();
    assert_eq!(bytes, b"%PDF-fake");

    // A company name that is entirely punctuation must still produce a
    // usable, non-empty file name rather than a hidden dotfile or an
    // empty stem.
    let punctuation_only = super::save_snapshot(&dir, b"x", "!!!", 2).expect("save snapshot");
    assert_eq!(punctuation_only, "application-v2.pdf");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn toml_round_trip_preserves_document() {
    let resume = altacv::import(altacv::ALTACV_SAMPLE).unwrap();
    let mut doc = ResumeDoc::from_resume(resume, "Base");
    doc.add_variant(crate::resume::model::SectionKind::Work);
    doc.section_order = vec![
        crate::resume::model::SectionKind::Skills,
        crate::resume::model::SectionKind::Profile,
        crate::resume::model::SectionKind::Work,
        crate::resume::model::SectionKind::Education,
        crate::resume::model::SectionKind::Certificates,
        crate::resume::model::SectionKind::Organizations,
    ];
    doc.set_section_title(crate::resume::model::SectionKind::Work, "Engineering");
    doc.set_language(crate::resume::model::DocumentLanguage::German);
    doc.add_preset("Tailored");

    let text = super::to_toml(&doc).expect("serialize to TOML");
    let back: ResumeDoc = toml::from_str(&text).expect("deserialize from TOML");

    assert_eq!(back.profile.active().name, doc.profile.active().name);
    assert_eq!(back.work.variants.len(), doc.work.variants.len());
    assert_eq!(back.work.active().len(), doc.work.active().len());
    assert_eq!(back.presets.len(), 1);
    assert_eq!(back.presets[0].name, "Tailored");
    assert_eq!(back.presets[0].order, doc.presets[0].order);
    assert_eq!(back.presets[0].titles, doc.presets[0].titles);
    assert_eq!(back.presets[0].lang.as_deref(), Some("de"));
    assert_eq!(
        back.language(),
        crate::resume::model::DocumentLanguage::German
    );
}

/// Documents written before `layout` existed (page size, margins, text
/// scale) must still load — with defaults that reproduce exactly the
/// values `resume/template.rs`'s old hard-coded `PREAMBLE` used, so an
/// existing vault's CVs render unchanged (US-07/C1).
#[test]
fn layout_defaults_and_old_docs_still_load() {
    use crate::resume::model::{LayoutSettings, Margins, PageSize, TypeSizes};

    let resume = altacv::import(altacv::ALTACV_SAMPLE).unwrap();
    let doc = ResumeDoc::from_resume(resume, "Base");
    let text = super::to_toml(&doc).expect("serialize to TOML");

    // Simulate a pre-`layout` file: strip the `[layout]` table (and its
    // `[layout.margins]` sub-table) that this version now appends.
    let cut = text
        .find("\n[layout]")
        .expect("layout is present in a fresh doc");
    let old_shape = &text[..cut];
    assert!(!old_shape.contains("[layout"));

    let back: ResumeDoc =
        toml::from_str(old_shape).expect("a pre-layout document must still load");
    assert_eq!(back.layout, LayoutSettings::default());
    assert_eq!(back.layout.page_size, PageSize::A4);
    assert_eq!(back.layout.margins.x_mm, 16.0);

    // And the field round-trips for real once it's set.
    let mut with_layout = back;
    with_layout.layout = LayoutSettings {
        page_size: PageSize::Letter,
        font: Default::default(),
        date_format: Default::default(),
        skills: Default::default(),
        entries: Default::default(),
        header: Default::default(),
        headings: Default::default(),
        show_link_marks: true,
        sizes: TypeSizes {
            name_pt: 8.5,
            title_pt: 2.0,
            heading_pt: -1.0,
            entry_pt: 1.0,
        },
        text_scale_pct: 90,
        leading_em: 0.65,
        margins: Margins {
            x_mm: 18.0,
            top_mm: 15.0,
            bottom_mm: 15.0,
        },
    };
    let text2 = super::to_toml(&with_layout).expect("serialize with layout set");
    let back2: ResumeDoc = toml::from_str(&text2).expect("round-trips");
    assert_eq!(back2.layout, with_layout.layout);

    // The shape every document written between the two features is in: a
    // `[layout]` table that predates `[layout.sizes]`. It has to keep the
    // sizes the template hard-coded, not zeroes.
    let mut without_sizes = String::new();
    let mut in_sizes = false;
    for line in text2.lines() {
        if line.starts_with('[') {
            in_sizes = line.starts_with("[layout.sizes]");
        }
        if !in_sizes {
            without_sizes.push_str(line);
            without_sizes.push('\n');
        }
    }
    assert!(
        !without_sizes.contains("name_pt"),
        "the cut missed the table"
    );
    let back3: ResumeDoc = toml::from_str(&without_sizes).expect("loads without sizes");
    assert_eq!(back3.layout.sizes, TypeSizes::default());
    assert_eq!(
        back3.layout.text_scale_pct, 90,
        "the rest of the table survived the cut"
    );
}

/// A document written before custom sections existed (D-9) — no
/// `custom_sections` table, no `next_custom_section_id` key — must still
/// load through the real `vault::load`/`vault::save` path, rendering the
/// same document it always did.
#[test]
fn custom_sections_default_and_old_docs_still_load() {
    use crate::resume::model::SectionKind;

    let resume = altacv::import(altacv::ALTACV_SAMPLE).unwrap();
    let doc = ResumeDoc::from_resume(resume, "Base");
    let text = super::to_toml(&doc).expect("serialize to TOML");
    assert!(!text.contains("custom_sections"));

    let back: ResumeDoc = toml::from_str(&text).expect("a pre-D-9 document must still load");
    assert!(back.custom_sections.is_empty());
    assert_eq!(back.next_custom_section_id, 0);
    assert_eq!(back.sections(), ResumeDoc::SECTIONS.to_vec());

    // And a custom section round-trips for real once one is added, via
    // the actual save/load functions (not just `toml::to_string`/`from_str`).
    let dir =
        std::env::temp_dir().join(format!("dockcv-custom-section-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("doc.toml");

    let mut with_custom = back;
    let id = with_custom.add_custom_section("Languages");
    super::save(&with_custom, &path, crate::vault::OnDisk::read(&path)).expect("save");
    let reloaded = super::load(&path).expect("load");
    assert_eq!(reloaded.custom_sections.len(), 1);
    assert_eq!(reloaded.custom_sections[0].title, "Languages");
    assert_eq!(reloaded.sections().last(), Some(&SectionKind::Custom(id)));

    std::fs::remove_dir_all(&dir).ok();
}

/// A 0.2.0 vault has no `[export]` table and no export history, and both
/// have to arrive as defaults rather than as a parse error — and a document
/// that has never been exported must not start writing either of them, so a
/// vault written by this branch still opens on `main`.
#[test]
fn export_settings_and_history_round_trip_and_stay_out_of_untouched_files() {
    use crate::resume::model::ExportSettings;
    use std::path::PathBuf;

    let resume = altacv::import(altacv::ALTACV_SAMPLE).unwrap();
    let doc = ResumeDoc::from_resume(resume, "Base");

    let text = super::to_toml(&doc).expect("serialize");
    assert!(
        !text.contains("[export]") && !text.contains("filename_pattern"),
        "an untouched document grew an export table:\n{text}"
    );
    assert!(!text.contains("export_history"));

    let loaded: ResumeDoc = toml::from_str(&text).expect("deserialize default");
    assert_eq!(
        loaded.export.filename_pattern,
        ExportSettings::DEFAULT_PATTERN
    );
    assert!(loaded.export_history.is_empty());

    // Once set, every field survives the trip in both directions.
    let mut used = doc;
    used.export.filename_pattern = "{name} - {role} - {company}".into();
    used.record_export(
        "2026-09-01",
        "17:30",
        "pdf",
        "Concise",
        PathBuf::from("/Users/someone/Documents/CVs/Ann Lee - SRE - Concise.pdf"),
    );

    let text = super::to_toml(&used).expect("serialize custom");
    let back: ResumeDoc = toml::from_str(&text).expect("deserialize custom");
    assert_eq!(back.export, used.export);
    assert_eq!(back.export_history, used.export_history);

    // And the same file with the new tables stripped back out — which is
    // what a 0.2.0 vault is — still opens, with defaults in their place.
    let downgraded: String = text
        .lines()
        .take_while(|l| !l.starts_with("[export") && !l.starts_with("[[export"))
        .collect::<Vec<_>>()
        .join("\n");
    let old: ResumeDoc =
        toml::from_str(&downgraded).expect("a document without the export tables still parses");
    assert_eq!(old.export, ExportSettings::default());
    assert!(old.export_history.is_empty());
}

/// A document written while `last_destination` still lived in the vault.
///
/// The field moved to `config.rs` because a folder is a fact about one
/// machine. Nothing here reads it any more, and nothing needs to: it is a
/// convenience the next export re-learns. What must not happen is the
/// document refusing to open over it.
#[test]
fn a_document_that_still_carries_last_destination_opens() {
    let resume = altacv::import(altacv::ALTACV_SAMPLE).unwrap();
    let doc = ResumeDoc::from_resume(resume, "Base");
    let mut text = super::to_toml(&doc).expect("serialize");
    text.push_str(
        r#"
[export]
filename_pattern = "{name} - {role}"
last_destination = "/Users/someone/Downloads"
"#,
    );

    let loaded: ResumeDoc = toml::from_str(&text).expect("a vault-era destination is ignored");
    assert_eq!(loaded.export.filename_pattern, "{name} - {role}");

    // And it is not written back, so the field dies out on the next save.
    let rewritten = super::to_toml(&loaded).expect("serialize");
    assert!(!rewritten.contains("last_destination"));
}

/// The exact block that failed to open: an export history written before
/// `timestamp` became `date` plus `time`.
///
/// Splitting a stored field is a schema change like any other, and this is
/// the test that was missing when it was made — the shape existed only in
/// development vaults, which is to say in somebody's real documents.
#[test]
fn an_export_history_written_before_the_date_split_still_opens() {
    let resume = altacv::import(altacv::ALTACV_SAMPLE).unwrap();
    let doc = ResumeDoc::from_resume(resume, "Base");
    let mut text = super::to_toml(&doc).expect("serialize");
    text.push_str(
        r#"
[[export_history]]
timestamp = "2026-09-01 23:47"
format = "PDF"
preset = "EM"
path = "/Users/someone/Downloads/Ann Lee - Engineering Manager - EM.pdf"

[[export_history]]
timestamp = "2026-08-14"
format = "Word"
preset = "Concise"
path = "/Users/someone/Downloads/Ann Lee - Concise.docx"
"#,
    );

    let loaded: ResumeDoc = toml::from_str(&text).expect("a pre-split history still opens");
    assert_eq!(loaded.export_history.len(), 2);

    // The moment is kept, not defaulted away to a blank date.
    assert_eq!(loaded.export_history[0].date, "2026-09-01");
    assert_eq!(loaded.export_history[0].time, "23:47");
    assert_eq!(loaded.export_history[0].preset, "EM");

    // A blob with no time in it keeps its date and admits it has no time,
    // rather than inventing one.
    assert_eq!(loaded.export_history[1].date, "2026-08-14");
    assert_eq!(loaded.export_history[1].time, "");

    // And it is written back in the new shape, so the old one dies out.
    let rewritten = super::to_toml(&loaded).expect("serialize");
    assert!(!rewritten.contains("timestamp"));
    assert!(rewritten.contains("date = \"2026-09-01\""));
    assert!(rewritten.contains("time = \"23:47\""));
}

/// The defect this task exists for: the box matched the *person's* name,
/// which is the same on every card in a vault of one person's CVs, and
/// ignored the stem the card leads with. Typing `northwind` found nothing.
#[test]
fn search_reaches_every_name_a_document_carries_and_says_which() {
    use super::MatchKind;
    use crate::resume::model::{Basics, Preset, Resume, SectionKind};

    let resume = Resume {
        basics: Basics {
            name: "Albert Einstein".into(),
            label: "Engineering Manager, Platform".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut doc = ResumeDoc::from_resume(resume, "Base");
    doc.profile.variants[0].name = "Short".into();
    doc.presets = vec![Preset {
        name: "FAANG".into(),
        based_on: None,
        description: None,
        profile: None,
        selection: vec![(SectionKind::Profile, doc.profile.active_id())],
        hidden: vec![],
        order: vec![],
        titles: vec![],
        lang: None,
    }];

    let dir = std::env::temp_dir().join(format!(
        "dockcv_search_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    let path = dir.join("northwind-em.toml");
    super::save(&doc, &path, crate::vault::OnDisk::read(&path)).expect("save");

    let meta = super::meta_from(&path, Some(&doc));

    // Each of the five kinds is reachable, and reports itself.
    for (query, kind, text) in [
        ("northwind", MatchKind::Stem, "northwind-em"),
        ("einstein", MatchKind::Person, "Albert Einstein"),
        ("platform", MatchKind::Role, "Engineering Manager, Platform"),
        ("faang", MatchKind::Preset, "FAANG"),
        ("short", MatchKind::Variant, "Short"),
    ] {
        let hit = meta
            .best_match(query)
            .unwrap_or_else(|| panic!("{query:?} found nothing"));
        assert_eq!(hit.kind, kind, "{query:?} matched the wrong kind");
        assert_eq!(hit.text, text);
    }

    // The stem answers first when two kinds both match, because it is what
    // the card leads with.
    assert_eq!(meta.best_match("n").map(|h| h.kind), Some(MatchKind::Stem));

    // A word in nothing finds nothing, rather than everything.
    assert!(meta.best_match("kubernetes").is_none());

    // Stem and person are already on the card, so they explain nothing.
    assert_eq!(MatchKind::Stem.label(), None);
    assert_eq!(MatchKind::Preset.label(), Some("preset"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// A file that will not parse is still findable by its name — it is the
/// only handle on it, and finding it is how it gets fixed.
#[test]
fn an_unreadable_document_is_still_findable_by_its_stem() {
    use super::MatchKind;
    let meta = super::meta_from(std::path::Path::new("/tmp/broken-cv.toml"), None);
    assert!(meta.unreadable);
    assert_eq!(
        meta.best_match("broken").map(|h| h.kind),
        Some(MatchKind::Stem)
    );
}

#[test]
fn profiles_are_a_reserved_atomic_vault_file_and_usage_is_per_document() {
    use crate::resume::model::{LayoutSettings, ProfileCatalog, ResumeDoc, ATS_SAFE_PROFILE};

    let dir = std::env::temp_dir().join(format!(
        "dockcv_profiles_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");

    let compact = LayoutSettings {
        text_scale_pct: 92,
        ..LayoutSettings::default()
    };
    let mut catalog = ProfileCatalog::default();
    catalog.upsert("Compact", compact);
    super::save_profiles(&dir, &catalog).expect("save profiles");
    assert_eq!(super::load_profiles(&dir), catalog);
    assert!(super::list_documents(&dir).is_empty());
    assert_eq!(super::vault_shape(&dir), super::VaultShape::Recognized);

    let current = ResumeDoc {
        layout_profile: Some(ATS_SAFE_PROFILE.to_string()),
        ..ResumeDoc::default()
    };
    super::create_document(&dir, &current, "current").expect("current doc");

    let mut preset_only = ResumeDoc::default();
    preset_only.add_preset("ATS");
    preset_only.presets[0].profile = Some(ATS_SAFE_PROFILE.to_string());
    // A second reference in one document must not inflate the CV count.
    preset_only.add_preset("ATS concise");
    preset_only.presets[1].profile = Some(ATS_SAFE_PROFILE.to_string());
    super::create_document(&dir, &preset_only, "preset-only").expect("preset doc");

    assert_eq!(
        super::documents_using_profile(&dir, ATS_SAFE_PROFILE),
        vec!["current".to_string(), "preset-only".to_string()]
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn notebook_lost_update_prevention_and_atomic_writes() {
    use crate::resume::model::{Applications, Diary, ProfileCatalog};
    use crate::vault::{OnDisk, SaveError};

    let dir = std::env::temp_dir().join(format!(
        "dockcv-notebook-conflict-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");

    // 1. Library conflict detection
    let (_lib, seen_lib) = super::load_library_seen(&dir);
    assert_eq!(seen_lib, OnDisk::ABSENT);
    super::save_library_seen(&dir, &Default::default(), seen_lib).expect("first save");
    let (mut lib2, seen_lib2) = super::load_library_seen(&dir);
    assert_ne!(seen_lib2, OnDisk::ABSENT);

    // External change to library.toml
    let lib_path = super::library_path(&dir);
    std::fs::write(&lib_path, "work = []\n# external edit\n").expect("write external");

    // Attempting to save with stale seen returns Conflict
    lib2.work.push(crate::resume::model::Work {
        name: "Acme".into(),
        ..Default::default()
    });
    assert!(matches!(
        super::save_library_seen(&dir, &lib2, seen_lib2),
        Err(SaveError::Conflict)
    ));

    // 2. Diary conflict detection
    let (_diary, seen_diary) = super::load_diary_seen(&dir);
    super::save_diary_seen(&dir, &Diary::default(), seen_diary).expect("first diary save");
    let (mut diary2, seen_diary2) = super::load_diary_seen(&dir);
    let diary_path = super::diary_path(&dir);
    std::fs::write(&diary_path, "entries = []\n# external diary edit\n").expect("write external");

    diary2.entries.push(crate::resume::model::DiaryEntry {
        text: "New entry".into(),
        ..Default::default()
    });
    assert!(matches!(
        super::save_diary_seen(&dir, &diary2, seen_diary2),
        Err(SaveError::Conflict)
    ));

    // 3. Applications conflict detection
    let (_apps, seen_apps) = super::load_applications_seen(&dir);
    super::save_applications_seen(&dir, &Applications::default(), seen_apps)
        .expect("first apps save");
    let (mut apps2, seen_apps2) = super::load_applications_seen(&dir);
    let apps_path = super::applications_path(&dir);
    std::fs::write(&apps_path, "entries = []\n# external apps edit\n").expect("write external");

    apps2.entries.push(crate::resume::model::Application {
        company: "Beta".into(),
        ..Default::default()
    });
    assert!(matches!(
        super::save_applications_seen(&dir, &apps2, seen_apps2),
        Err(SaveError::Conflict)
    ));

    // 4. Profiles conflict detection
    let (_prof, seen_prof) = super::load_profiles_seen(&dir);
    super::save_profiles_seen(&dir, &ProfileCatalog::default(), seen_prof)
        .expect("first prof save");
    let (mut prof2, seen_prof2) = super::load_profiles_seen(&dir);
    let prof_path = super::profiles_path(&dir);
    std::fs::write(&prof_path, "users = {}\n# external prof edit\n").expect("write external");

    prof2.upsert("Custom", Default::default());
    assert!(matches!(
        super::save_profiles_seen(&dir, &prof2, seen_prof2),
        Err(SaveError::Conflict)
    ));

    // 5. Atomic write helper replaces destination and cleans up
    let test_file = dir.join("atomic_test.txt");
    crate::vault::atomic_write(&test_file, "hello").expect("atomic write");
    assert_eq!(std::fs::read_to_string(&test_file).unwrap(), "hello");
    crate::vault::atomic_write(&test_file, "world").expect("atomic overwrite");
    assert_eq!(std::fs::read_to_string(&test_file).unwrap(), "world");

    let _ = std::fs::remove_dir_all(&dir);
}
