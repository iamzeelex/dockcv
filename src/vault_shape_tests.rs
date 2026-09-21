//! Is this directory a vault, and what does it say about its documents?
//!
//! Split from `vault.rs` by C15: the marker, the trash, relative time, and the
//! metadata a card is drawn from.

use crate::resume::model::Diary;
use crate::resume::{altacv, model::ResumeDoc};

#[test]
fn trashing_the_same_name_twice_keeps_both() {
    let dir = std::env::temp_dir().join(format!("dockcv-trash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp vault");

    let mut first = ResumeDoc::from_resume(crate::resume::model::Resume::default(), "Base");
    first.profile.active_mut().name = "The first one".into();
    let path = super::create_document(&dir, &first, "cv").expect("create");
    super::delete_document(&path).expect("delete");

    // Same name again, different contents.
    let mut second = ResumeDoc::from_resume(crate::resume::model::Resume::default(), "Base");
    second.profile.active_mut().name = "The second one".into();
    let path = super::create_document(&dir, &second, "cv").expect("recreate");
    assert_eq!(
        path.file_name().unwrap(),
        "cv.toml",
        "the name is free again"
    );
    super::delete_document(&path).expect("delete again");

    assert_eq!(super::trash_count(&dir), 2, "both must survive");
    let names: Vec<String> = std::fs::read_dir(dir.join(".trash"))
        .expect("trash")
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();
    assert!(names.contains(&"cv.toml".to_string()), "{names:?}");
    assert!(names.contains(&"cv-2.toml".to_string()), "{names:?}");

    let mut recovered: Vec<String> = std::fs::read_dir(dir.join(".trash"))
        .expect("trash")
        .flatten()
        .filter_map(|e| super::load(&e.path()).ok())
        .map(|d| d.profile.active().name.clone())
        .collect();
    recovered.sort();
    assert_eq!(recovered, vec!["The first one", "The second one"]);

    let _ = std::fs::remove_dir_all(&dir);
}

/// The picker used to accept any directory, so choosing `~/` made the
/// gallery parse every unrelated `.toml` on the machine and draw the
/// failures as cards. These are the four answers that decides between.
#[test]
fn a_directory_is_classified_before_it_becomes_a_vault() {
    use super::VaultShape;

    let root = std::env::temp_dir().join(format!("dockcv-shape-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let make = |name: &str| {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    };

    // A folder with nothing in it is a perfectly good new vault.
    assert_eq!(super::vault_shape(&make("empty")), VaultShape::Empty);

    // A real vault, made the normal way, carries the marker.
    let created = super::create_vault(&make("parent")).expect("create");
    assert_eq!(super::vault_shape(&created), VaultShape::Marked);

    // A vault from before the marker existed: no `.cvault`, but its
    // contents give it away. This is the case that must not regress —
    // failing it would hide an existing vault from its owner.
    let legacy = make("legacy");
    let doc = ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base");
    super::create_document(&legacy, &doc, "cv").expect("create");
    assert_eq!(super::vault_shape(&legacy), VaultShape::Recognized);

    // …and one recognised by its notebooks alone, with no documents yet.
    let notebooks = make("notebooks");
    super::save_diary(&notebooks, &Diary::default()).expect("save");
    assert_eq!(super::vault_shape(&notebooks), VaultShape::Recognized);

    // A folder of TOML that is not CVs — a config directory, say.
    let strays = make("strays");
    for name in ["Cargo.toml", "settings.toml", "rust-toolchain.toml"] {
        std::fs::write(strays.join(name), "[package]\nname = \"x\"\n").expect("write");
    }
    assert_eq!(
        super::vault_shape(&strays),
        VaultShape::Unrecognized { stray_toml: 3 }
    );

    // Adopting it anyway leaves the marker, so it stops asking.
    super::mark_as_vault(&strays);
    assert_eq!(super::vault_shape(&strays), VaultShape::Marked);

    let _ = std::fs::remove_dir_all(&root);
}

/// The marker is a readable file, not a magic byte — and it is not a
/// document, so it must never show up in the gallery.
#[test]
fn the_marker_is_plain_text_and_is_not_a_document() {
    let dir = std::env::temp_dir().join(format!("dockcv-marker-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp vault");

    super::mark_as_vault(&dir);
    let text = std::fs::read_to_string(dir.join(".cvault")).expect("marker exists");
    assert!(text.contains("DockCV vault"), "{text}");
    assert!(
        toml::from_str::<toml::Value>(&text).is_ok(),
        "the marker must parse as TOML: {text}"
    );
    assert!(super::list_documents(&dir).is_empty());

    // Writing it twice must not clobber the original creation date.
    super::mark_as_vault(&dir);
    assert_eq!(std::fs::read_to_string(dir.join(".cvault")).unwrap(), text);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn civil_from_days_known_values() {
    assert_eq!(super::civil_from_days(0), (1970, 1, 1));
    assert_eq!(super::civil_from_days(18_628), (2021, 1, 1));
}

#[test]
fn relative_time_boundaries() {
    use super::relative_time as rt;

    assert_eq!(rt(1000, 1000), "just now"); // 0s
    assert_eq!(rt(1000, 1000 + 59), "just now"); // 59s
    assert_eq!(rt(1000, 1000 + 60), "1m ago"); // 60s
    assert_eq!(rt(1000, 1000 + 59 * 60), "59m ago"); // 59m
    assert_eq!(rt(1000, 1000 + 60 * 60), "1h ago"); // 60m
    assert_eq!(rt(1000, 1000 + 23 * 3600), "23h ago"); // 23h
    assert_eq!(rt(1000, 1000 + 24 * 3600), "1d ago"); // 24h
    assert_eq!(rt(1000, 1000 + 6 * 86_400), "6d ago"); // 6d
    assert_eq!(rt(1000, 1000 + 7 * 86_400), "1w ago"); // 7d
    assert_eq!(rt(1000, 1000 + 27 * 86_400), "3w ago"); // 27d
    assert_eq!(rt(1000, 1000 + 28 * 86_400), "4w ago"); // 28d

    // Clock skew / copied file with a future mtime: must not underflow
    // or panic. Decision: treat as elapsed-zero, same as "0s".
    assert_eq!(rt(2000, 1000), "just now");
}

#[test]
fn read_meta_populates_modified_secs() {
    let dir =
        std::env::temp_dir().join(format!("dockcv-meta-mtime-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("doc.toml");
    std::fs::write(&path, "").unwrap();

    let meta = super::meta_from(&path, super::load(&path).ok().as_ref());
    assert!(meta.modified_secs.is_some());

    std::fs::remove_dir_all(&dir).ok();
}

/// Diaries written before `source_doc` existed must still load. A new field
/// that breaks old vaults is a data-loss bug, not a schema change.
#[test]
fn diary_without_source_doc_still_loads() {
    let old = "[[entries]]\ndate = \"2026-06-18\"\ntext = \"Cut p99 latency in half\"\n";
    let diary: Diary = toml::from_str(old).expect("a pre-source_doc diary must load");
    assert_eq!(diary.entries.len(), 1);
    assert!(diary.entries[0].source_doc.is_none());

    // And the new field round-trips when it is set.
    let mut with_source = diary;
    with_source.entries[0].source_doc = Some("albert-senior-swe".into());
    let text = toml::to_string_pretty(&with_source).expect("serializes");
    let back: Diary = toml::from_str(&text).expect("round-trips");
    assert_eq!(
        back.entries[0].source_doc.as_deref(),
        Some("albert-senior-swe")
    );
}

/// `used_in` arrived with `Use in a CV` (US-06). A diary written before it
/// existed must load with the list empty, and an entry that has never been
/// promoted must not gain a key — the vault is the user's own files, often
/// under git, and a diff full of `used_in = []` is noise.
#[test]
fn a_diary_without_used_in_still_loads_and_gains_no_key() {
    let old = "[[entries]]\ndate = \"2026-06-18\"\ntext = \"Cut p99 latency in half\"\n";
    let diary: Diary = toml::from_str(old).expect("a pre-used_in diary must load");
    assert!(diary.entries[0].used_in.is_empty());

    let untouched = toml::to_string_pretty(&diary).expect("serializes");
    assert!(
        !untouched.contains("used_in"),
        "an entry that was never promoted must not gain the key:\n{untouched}"
    );

    // …and it round-trips once a win actually goes into a CV.
    let mut promoted = diary;
    promoted.entries[0].used_in = vec!["albert-senior-swe".into(), "albert-em".into()];
    let text = toml::to_string_pretty(&promoted).expect("serializes");
    let back: Diary = toml::from_str(&text).expect("round-trips");
    assert_eq!(
        back.entries[0].used_in,
        vec!["albert-senior-swe", "albert-em"]
    );
}

/// The confidential mark (US-36) is additive, and an unmarked entry must
/// gain no key — the vault is the user's own files, often under git.
#[test]
fn a_diary_without_the_confidential_mark_still_loads() {
    let old = "[[entries]]\ndate = \"2026-06-18\"\ntext = \"Cut p99 latency in half\"\n";
    let diary: Diary = toml::from_str(old).expect("a pre-confidential diary must load");
    assert!(!diary.entries[0].confidential);

    let untouched = toml::to_string_pretty(&diary).expect("serializes");
    assert!(
        !untouched.contains("confidential"),
        "an unmarked entry must not gain the key:\n{untouched}"
    );

    let mut marked = diary;
    marked.entries[0].confidential = true;
    let text = toml::to_string_pretty(&marked).expect("serializes");
    assert!(text.contains("confidential = true"));
    let back: Diary = toml::from_str(&text).expect("round-trips");
    assert!(back.entries[0].confidential);
}

/// Role and tags arrived after entries were already on disk. A diary
/// written before they existed must load with both empty, and neither may
/// reach the file until it holds something — a `role = ""` line in every
/// entry would be noise in a format whose point is being readable.
#[test]
fn diary_without_role_or_tags_still_loads() {
    let old = "[[entries]]\ndate = \"2026-06-18\"\ntext = \"Cut p99 latency in half\"\n";
    let diary: Diary = toml::from_str(old).expect("a pre-role diary must load");
    assert_eq!(diary.entries[0].role, "");
    assert!(diary.entries[0].tags.is_empty());

    let empty = toml::to_string_pretty(&diary).expect("serializes");
    assert!(!empty.contains("role"), "an empty role must not be written");
    assert!(!empty.contains("tags"), "empty tags must not be written");

    let mut tagged = diary;
    tagged.entries[0].role = "Acme Corp · Senior SWE".into();
    tagged.entries[0].tags = vec!["performance".into(), "architecture".into()];
    let text = toml::to_string_pretty(&tagged).expect("serializes");
    let back: Diary = toml::from_str(&text).expect("round-trips");
    assert_eq!(back.entries[0].role, "Acme Corp · Senior SWE");
    assert_eq!(back.entries[0].tags, vec!["performance", "architecture"]);
}
