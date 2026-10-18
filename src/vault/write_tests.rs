//! Writing a document, and what happens when the disk disagrees.
//!
//! Split from `vault.rs` by C15: external edits, save conflicts, and what the
//! folder looks like to `list_documents`.

use crate::resume::{altacv, model::ResumeDoc};

/// The lost update, which happened to a real vault.
///
/// DockCV held a document open, the file was edited in another editor, and
/// the next debounced write replaced it with what the app was holding —
/// silently. The whole promise on the front of the README is that the
/// vault is plain text you can edit anywhere, so this is the one write
/// that must never happen.
#[test]
fn a_file_changed_outside_the_app_is_not_overwritten() {
    use crate::vault::{load_seen, save, OnDisk, SaveError};

    let dir = std::env::temp_dir().join(format!("dockcv-lost-update-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("cv.toml");

    let mut doc = ResumeDoc::default();
    doc.profile.active_mut().name = "Albert Einstein".into();
    save(&doc, &path, OnDisk::ABSENT).expect("the first write creates the file");

    // What an editor does: read it, and keep it.
    let (mut held, seen) = load_seen(&path).expect("load");

    // What somebody else does, meanwhile.
    let outside = std::fs::read_to_string(&path)
        .expect("read")
        .replace("Albert Einstein", "Marie Curie");
    std::fs::write(&path, &outside).expect("the other editor writes");

    // And what the app tries next.
    held.profile.active_mut().label = "Principal Systems Architect".into();
    assert!(
        matches!(save(&held, &path, seen), Err(SaveError::Conflict)),
        "a file that is no longer the one we read must not be replaced"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        outside,
        "the other editor's version is still exactly as they left it"
    );

    // And the way out: agree with the file, then write.
    let (_, now) = load_seen(&path).expect("reload");
    save(&held, &path, now).expect("a write that knows what it is replacing");
    assert!(std::fs::read_to_string(&path)
        .expect("read")
        .contains("Principal Systems Architect"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// The three answers, and the two that are silent when wrong.
#[test]
fn an_external_edit_is_adopted_only_when_there_is_nothing_to_lose() {
    use crate::vault::{external_change, load_seen, save, ExternalChange, OnDisk};

    let dir = std::env::temp_dir().join(format!("dockcv-external-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("cv.toml");

    let mut doc = ResumeDoc::default();
    doc.profile.active_mut().name = "Albert Einstein".into();
    save(&doc, &path, OnDisk::ABSENT).expect("write");
    let (held, seen) = load_seen(&path).expect("load");
    let agreed = OnDisk::of_document(&held);

    assert_eq!(
        external_change(&held, &path, seen, agreed),
        ExternalChange::None,
        "an untouched file is not a change"
    );

    // Somebody else writes; we have typed nothing.
    let theirs = std::fs::read_to_string(&path)
        .expect("read")
        .replace("Albert Einstein", "Marie Curie");
    std::fs::write(&path, &theirs).expect("write");
    assert_eq!(
        external_change(&held, &path, seen, agreed),
        ExternalChange::Adopt,
        "with nothing unsaved, the file's version is free to take"
    );

    // And now with something of our own on screen.
    let mut edited = held.clone();
    edited.profile.active_mut().label = "Principal Systems Architect".into();
    assert_eq!(
        external_change(&edited, &path, seen, agreed),
        ExternalChange::Conflict,
        "two versions, and taking either one silently loses the other"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Our own write is not somebody else's edit.
///
/// A debounced save lands on a background thread and the state recording
/// it is applied afterwards. A watch tick in that gap sees a file that no
/// longer matches what the holder last agreed with — and the naive reading
/// of that is "two versions exist", which would put a conflict banner in
/// front of somebody who had done nothing but type.
#[test]
fn a_write_we_have_not_finished_recording_is_not_a_conflict() {
    use crate::vault::{external_change, load_seen, save, ExternalChange, OnDisk};

    let dir = std::env::temp_dir().join(format!("dockcv-own-write-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("cv.toml");

    let mut doc = ResumeDoc::default();
    doc.profile.active_mut().name = "Albert Einstein".into();
    save(&doc, &path, OnDisk::ABSENT).expect("write");
    let (mut held, seen) = load_seen(&path).expect("load");
    let agreed = OnDisk::of_document(&held);

    // Typed, and written — but `seen` is still the state from before it.
    held.profile.active_mut().label = "Principal Systems Architect".into();
    save(&held, &path, seen).expect("write");

    assert_eq!(
        external_change(&held, &path, seen, agreed),
        ExternalChange::None,
        "the file holds what this document holds; nobody else has been here"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A document nobody edited is a document nobody writes.
///
/// Opening a CV and leaving used to rewrite its file from memory, which is
/// a modification to git, to a sync client and to anything watching the
/// folder — and it is what made the lost update above so easy to hit, since
/// merely having the document open was enough to arm it.
#[test]
fn writing_a_document_that_has_not_changed_leaves_the_file_alone() {
    use crate::vault::{load_seen, save, OnDisk};

    let dir = std::env::temp_dir().join(format!("dockcv-idle-write-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("cv.toml");

    let mut doc = ResumeDoc::default();
    doc.profile.active_mut().name = "Albert Einstein".into();
    save(&doc, &path, OnDisk::ABSENT).expect("write");
    let written_at = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .expect("mtime");

    let (held, seen) = load_seen(&path).expect("load");
    save(&held, &path, seen).expect("write");

    assert_eq!(
        std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .expect("mtime"),
        written_at,
        "the file was rewritten with the bytes it already had"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn list_documents_excludes_reserved_notebooks() {
    let dir = std::env::temp_dir().join(format!("dockcv-list-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in [
        "alpha.toml",
        "library.toml",
        "diary.toml",
        "applications.toml",
    ] {
        std::fs::write(dir.join(name), "x = 1\n").unwrap();
    }
    let docs = super::list_documents(&dir);
    let names: Vec<String> = docs
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
        .collect();
    assert!(names.contains(&"alpha.toml".to_string()));
    assert!(!names.contains(&"library.toml".to_string()));
    assert!(!names.contains(&"diary.toml".to_string()));
    assert!(!names.contains(&"applications.toml".to_string()));
    std::fs::remove_dir_all(&dir).ok();
}

/// A document that will not parse must come back as an error and be left
/// **byte for byte** as it was.
///
/// The editor used to answer a failed load by seeding the bundled AltaCV
/// sample and writing it to that same path, so a stray character in a file
/// the product tells people to hand-edit destroyed the CV on one click.
/// Reading is now `Shell::open_doc`'s job and `Root::new` takes a document
/// it cannot have failed to load — but the property the fix rests on is
/// this one, and it belongs where the loading lives.
#[test]
fn a_document_that_will_not_parse_is_left_exactly_as_it_is() {
    let dir = std::env::temp_dir().join(format!("dockcv-corrupt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp vault");
    let path = dir.join("hand-edited.toml");

    // A real document with one line mangled the way a person would.
    let doc = ResumeDoc::from_resume(altacv::import(altacv::ALTACV_SAMPLE).unwrap(), "Base");
    let good = super::to_toml(&doc).expect("serializes");
    let broken = good.replace("[profile]", "[profile");
    assert_ne!(broken, good, "the fixture must actually be broken");
    std::fs::write(&path, &broken).expect("write");

    assert!(
        super::load(&path).is_err(),
        "a broken document must not load"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("still there"),
        broken,
        "a failed load must not rewrite, repair or replace the file"
    );

    // And the vault still lists it, so the gallery can show it and say so
    // rather than pretending the document does not exist.
    let listed = super::list_documents(&dir);
    assert_eq!(listed, vec![path.clone()]);
    assert!(super::meta_from(&path, super::load(&path).ok().as_ref()).unreadable);

    let _ = std::fs::remove_dir_all(&dir);
}

/// G1: Work and Volunteer urls round-trip cleanly, and empty URLs are omitted
/// from TOML serialization to preserve clean, diffable files.
#[test]
fn entry_urls_round_trip_cleanly_through_toml() {
    use crate::resume::model::{Resume, ResumeDoc, Volunteer, Work};

    let resume = Resume {
        work: vec![
            Work {
                name: "Acme Corp".into(),
                position: "Senior Engineer".into(),
                url: "https://acme.example.com".into(),
                ..Default::default()
            },
            Work {
                name: "Beta Inc".into(),
                position: "Junior Engineer".into(),
                url: String::new(),
                ..Default::default()
            },
        ],
        volunteer: vec![Volunteer {
            organization: "Open Source".into(),
            position: "Maintainer".into(),
            url: "https://oss.example.org".into(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let doc = ResumeDoc::from_resume(resume, "Base");

    let toml_str = super::to_toml(&doc).expect("serializes to toml");
    assert!(
        toml_str.contains("url = \"https://acme.example.com\""),
        "non-empty work url must be serialized in TOML"
    );
    assert!(
        toml_str.contains("url = \"https://oss.example.org\""),
        "non-empty volunteer url must be serialized in TOML"
    );

    // Empty URL on Beta Inc must NOT be serialized as `url = ""`
    let beta_block = toml_str
        .split("[[work.variants.data]]")
        .find(|b| b.contains("Beta Inc"))
        .expect("Beta Inc block");
    let beta_entry = beta_block.split("\n[").next().unwrap();
    assert!(
        !beta_entry.contains("url ="),
        "empty url must be skipped when serializing TOML, got:\n{beta_entry}"
    );

    let loaded: ResumeDoc = toml::from_str(&toml_str).expect("deserializes from toml");
    let work = loaded.work.active();
    assert_eq!(work[0].url, "https://acme.example.com");
    assert_eq!(work[1].url, "");
    let vol = loaded.volunteer.active();
    assert_eq!(vol[0].url, "https://oss.example.org");
}

/// The trash exists to make deletion reversible, so a delete must never
/// destroy something already in it. `fs::rename` replaces the destination
/// silently on Unix, so this was a real way to lose a document permanently
/// through the reversible path.
/// L-8: a typo in a hand-edited `status` used to cost the record it named.
/// The word read as `Wishlist` — the right trade, one bad word costing one
/// card rather than the board — and was then **written back** as
/// `"wishlist"` on the next save, so `status = "ofer"` quietly turned an
/// offer into a wishlist entry with nothing left to notice.
#[test]
fn a_status_this_build_cannot_read_survives_a_save() {
    let dir = std::env::temp_dir().join(format!("dockcv-status-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp vault");

    std::fs::write(
        super::applications_path(&dir),
        "[[entries]]\ncompany = \"Acme\"\nrole = \"Engineer\"\nstatus = \"ofer\"\n",
    )
    .expect("write board");

    let applications = super::load_applications(&dir);
    assert_eq!(
        applications.entries[0].status(),
        crate::resume::model::ApplicationStatus::Wishlist,
        "an unknown word still reads as Wishlist, so the board stays usable"
    );

    super::save_applications(&dir, &applications).expect("save");
    let text = std::fs::read_to_string(super::applications_path(&dir)).expect("read back");
    assert!(
        text.contains("status = \"ofer\""),
        "the user's own word must come back out; got:\n{text}"
    );

    // And a word we *do* understand is still written from the enum, so a
    // card moved on the board writes where it was moved to.
    let mut applications = super::load_applications(&dir);
    applications.entries[0]
        .advance_to(crate::resume::model::ApplicationStatus::Offer, "2026-08-21");
    super::save_applications(&dir, &applications).expect("save");
    let text = std::fs::read_to_string(super::applications_path(&dir)).expect("read back");
    assert!(text.contains("status = \"offer\""), "got:\n{text}");
    assert!(
        !text.contains("ofer\""),
        "the typo is gone once it is corrected"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A card built in code sets the enum and leaves the word at its default;
/// the file must say what the enum means, not what the default was.
#[test]
fn a_card_created_in_code_writes_the_status_it_was_given() {
    let dir = std::env::temp_dir().join(format!("dockcv-status-new-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp vault");

    let mut applications = crate::resume::model::Applications::default();
    applications
        .entries
        .push(crate::resume::model::Application {
            company: "Acme".into(),
            status_word: crate::resume::model::ApplicationStatus::Applied
                .word()
                .into(),
            ..Default::default()
        });
    super::save_applications(&dir, &applications).expect("save");

    let text = std::fs::read_to_string(super::applications_path(&dir)).expect("read back");
    assert!(text.contains("status = \"applied\""), "got:\n{text}");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Every schema change needs a forward migration and a round-trip test
/// (CLAUDE.md). `LayoutSettings::skills` is `#[serde(default)]`, so a
/// document written before it existed must still load — and must load as
/// the arrangement it was actually printed with, not as whichever variant
/// happens to be listed first.
#[test]
fn a_document_without_a_skills_style_loads_as_the_one_it_was_printed_with() {
    use crate::resume::model::SkillsStyle;

    let dir = std::env::temp_dir().join(format!("dockcv-skills-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp vault");
    let path = dir.join("cv.toml");

    // Save with the default, then strip the key the way a file written
    // before this field existed would not have had it at all.
    let doc = crate::resume::model::ResumeDoc::from_resume(
        crate::resume::model::Resume::default(),
        "Base",
    );
    super::save(&doc, &path, crate::vault::OnDisk::read(&path)).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let without: String = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("skills = "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!without.contains("skills = "), "the key really is gone");
    std::fs::write(&path, &without).expect("write");

    let loaded = super::load(&path).expect("an older document still loads");
    assert_eq!(
        loaded.layout.skills.style,
        SkillsStyle::Rows,
        "an older file must keep rendering the way it did"
    );

    // And a chosen style round-trips.
    let mut doc = loaded;
    doc.layout.skills.style = SkillsStyle::Bubbles;
    super::save(&doc, &path, crate::vault::OnDisk::read(&path)).expect("save");
    assert_eq!(
        super::load(&path).expect("reload").layout.skills.style,
        SkillsStyle::Bubbles
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A file DockCV did not write — by hand, by an older version, with a comment
/// in it — parses to a document DockCV would write differently. Judging
/// "anything unsaved?" by the file's bytes called that document edited the
/// moment it opened, so every edit made outside arrived as a conflict.
#[test]
fn a_file_dockcv_did_not_write_still_reloads_when_edited_outside() {
    use crate::vault::{external_change, load_seen, save, to_toml, ExternalChange, OnDisk};

    let dir = std::env::temp_dir().join(format!("dockcv-hand-written-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("cv.toml");

    let mut doc = ResumeDoc::default();
    doc.profile.active_mut().name = "Albert Einstein".into();
    let canonical = to_toml(&doc).expect("serializes");
    std::fs::write(&path, format!("# kept by hand\n{canonical}")).expect("write");

    let (held, seen) = load_seen(&path).expect("load");
    let agreed = OnDisk::of_document(&held);
    assert_ne!(seen, agreed, "the fixture is a file DockCV would not write");

    let theirs = std::fs::read_to_string(&path)
        .expect("read")
        .replace("Albert Einstein", "Marie Curie");
    std::fs::write(&path, theirs).expect("the other editor writes");
    assert_eq!(
        external_change(&held, &path, seen, agreed),
        ExternalChange::Adopt,
        "nothing was typed here, so there is nothing to lose by reloading"
    );

    // And typing still makes it a conflict.
    let mut edited = held.clone();
    edited.profile.active_mut().label = "Physicist".into();
    assert_eq!(
        external_change(&edited, &path, seen, agreed),
        ExternalChange::Conflict
    );
    // A write against the fresh file then goes through as before.
    save(&held, &path, OnDisk::read(&path)).expect("write");

    let _ = std::fs::remove_dir_all(&dir);
}

/// The editor writes from three places — the debounce, the export recording
/// itself, the flush on leaving — and `Root::save_now` is the one they share.
/// What it relies on: a holder that takes each write's result as its next
/// `seen` never refuses its own work, however the three interleave.
///
/// The defect this replaced was a writer that captured `seen` before another
/// one ran and wrote after it — or whose result was dropped while its write
/// still landed — and from then on refused every save as "changed outside".
#[test]
fn a_holder_that_records_every_write_never_refuses_itself() {
    use crate::vault::{load_seen, save, OnDisk};

    let dir = std::env::temp_dir().join(format!("dockcv-one-writer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("cv.toml");
    save(&ResumeDoc::default(), &path, OnDisk::ABSENT).expect("write");
    let (mut doc, mut on_disk) = load_seen(&path).expect("load");

    let save_now = |doc: &ResumeDoc, on_disk: &mut OnDisk| {
        *on_disk = save(doc, &path, *on_disk).expect("a holder never conflicts with itself");
    };
    for round in 0..20 {
        doc.profile
            .active_mut()
            .summary
            .push_str(&format!(" {round}"));
        save_now(&doc, &mut on_disk); // the debounce
        doc.record_export(
            "2026-10-18",
            "01:00",
            "pdf",
            "Current view",
            dir.join("cv.pdf"),
        );
        save_now(&doc, &mut on_disk); // the export
        save_now(&doc, &mut on_disk); // the flush, with nothing new to say
    }
    assert!(std::fs::read_to_string(&path)
        .expect("read")
        .contains(" 19"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// "Keep my version" on the conflict banner — `Root::keep_my_version`, which
/// re-reads the file's hash and then writes as it always does.
///
/// Before it existed a real conflict wedged the editor: every write was
/// refused against the file state from before the other editor's, for as long
/// as the document stayed open. The person's choice is what unblocks the
/// write, and nothing else may: the re-read is the only new step, so a version
/// that lands after it is refused like any other.
#[test]
fn keeping_my_version_writes_only_over_the_file_it_read_again() {
    use crate::vault::{load_seen, save, OnDisk, SaveError};

    let dir = std::env::temp_dir().join(format!("dockcv-keep-mine-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("cv.toml");

    let mut doc = ResumeDoc::default();
    doc.profile.active_mut().name = "Albert Einstein".into();
    save(&doc, &path, OnDisk::ABSENT).expect("write");
    let (mut mine, seen) = load_seen(&path).expect("load");
    mine.profile.active_mut().label = "Principal Systems Architect".into();

    let theirs = std::fs::read_to_string(&path)
        .expect("read")
        .replace("Albert Einstein", "Marie Curie");
    std::fs::write(&path, &theirs).expect("the other editor writes");

    // The wedge: the debounce, and every one after it, refuses.
    for _ in 0..2 {
        assert!(matches!(save(&mine, &path, seen), Err(SaveError::Conflict)));
    }
    assert_eq!(std::fs::read_to_string(&path).expect("read"), theirs);

    // Re-read, then write: mine replaces theirs, and the holder agrees with
    // the result, so the next write is an ordinary one.
    let now = save(&mine, &path, OnDisk::read(&path)).expect("the chosen version is written");
    let written = std::fs::read_to_string(&path).expect("read");
    assert!(written.contains("Principal Systems Architect"));
    assert!(!written.contains("Marie Curie"));
    assert_eq!(now, OnDisk::read(&path));
    mine.profile.active_mut().label = "Physicist".into();
    save(&mine, &path, now).expect("no conflict left behind");

    // Their version need not parse to be replaced: the choice was made about
    // it, not about whether DockCV can read it.
    std::fs::write(&path, "[profile\nname = ").expect("left half-written");
    assert!(load_seen(&path).is_err(), "the fixture does not parse");
    save(&mine, &path, OnDisk::read(&path)).expect("written over the broken file");
    assert!(load_seen(&path).is_ok());

    // A version that lands after the re-read was not part of the choice.
    let reread = OnDisk::read(&path);
    std::fs::write(&path, &theirs).expect("the other editor writes again");
    assert!(matches!(
        save(&mine, &path, reread),
        Err(SaveError::Conflict)
    ));
    assert_eq!(std::fs::read_to_string(&path).expect("read"), theirs);

    let _ = std::fs::remove_dir_all(&dir);
}

/// Who may change a file is the file's business. A rename only asks the
/// folder, so a CV made read-only used to be replaced by a writable copy, and
/// a CV kept private came back readable by everyone.
#[cfg(unix)]
#[test]
fn a_write_respects_the_files_permissions() {
    use std::os::unix::fs::PermissionsExt;

    use crate::vault::{atomic_write, OnDisk};

    let dir = std::env::temp_dir().join(format!("dockcv-permissions-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("cv.toml");
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;

    std::fs::write(&path, "before").expect("write");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    assert!(
        atomic_write(&path, "after").is_err(),
        "a read-only file is refused"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "before");
    assert_eq!(mode(&path), 0o444);

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    atomic_write(&path, "after").expect("a writable file is written");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "after");
    assert_eq!(
        mode(&path),
        0o600,
        "the replacement keeps the original's mode"
    );
    assert_eq!(OnDisk::read(&path), OnDisk::of("after"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// A CV linked into the vault from somewhere else is edited where it lives.
#[cfg(unix)]
#[test]
fn a_symlinked_document_is_written_through_its_link() {
    use crate::vault::{load_seen, save};

    let dir = std::env::temp_dir().join(format!("dockcv-symlink-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let elsewhere = dir.join("elsewhere");
    let vault = dir.join("vault");
    std::fs::create_dir_all(&elsewhere).expect("temp dir");
    std::fs::create_dir_all(&vault).expect("temp dir");

    let real = elsewhere.join("cv.toml");
    let mut doc = ResumeDoc::default();
    doc.profile.active_mut().name = "Albert Einstein".into();
    std::fs::write(&real, super::to_toml(&doc).unwrap()).expect("write");
    let link = vault.join("cv.toml");
    std::os::unix::fs::symlink("../elsewhere/cv.toml", &link).expect("link");

    let (mut held, seen) = load_seen(&link).expect("load through the link");
    held.profile.active_mut().label = "Physicist".into();
    save(&held, &link, seen).expect("write through the link");

    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the link is still a link"
    );
    assert!(std::fs::read_to_string(&real)
        .unwrap()
        .contains("Physicist"));

    let _ = std::fs::remove_dir_all(&dir);
}
