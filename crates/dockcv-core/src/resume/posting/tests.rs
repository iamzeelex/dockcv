//! What the posting read claims, and what it must never do.

use super::*;
use crate::resume::model::{DiaryEntry, Library, SectionKind, SkillGroup, Work};

const POSTING: &str = "We are looking for a Platform Engineer to own our deployment \
     platform. You will lead the migration of forty services onto one deployment path, \
     run the incident review practice, and work with Kubernetes, Terraform and AWS. \
     Experience with Kubernetes is required. Strong SQL is a plus.";

#[test]
fn a_posting_leads_with_what_it_repeats() {
    let terms = terms(POSTING);
    let at = |w: &str| terms.iter().position(|t| t.word == w);
    let count = |w: &str| terms.iter().find(|t| t.word == w).map(|t| t.count);

    // Three words are said twice; they come before everything said once, and
    // among themselves they keep the order the posting introduced them in.
    assert_eq!(count("Platform"), Some(2));
    assert_eq!(count("deployment"), Some(2));
    assert_eq!(count("Kubernetes"), Some(2));
    assert!(at("Platform") < at("deployment"), "first mention breaks the tie");
    assert!(at("Kubernetes") < at("Terraform"), "twice beats once");

    assert!(at("AWS").is_some(), "an acronym is not too short to be a term");
    assert!(at("SQL").is_some());
}

/// The words a posting is built out of are not what it is about.
#[test]
fn scaffolding_is_not_a_term() {
    let terms = terms(POSTING);
    for noise in ["with", "will", "looking", "work", "team", "the", "our"] {
        assert!(
            !terms.iter().any(|t| t.word.eq_ignore_ascii_case(noise)),
            "{noise} should not be a term"
        );
    }
}

/// Casing is the posting's, because the terms are shown back.
#[test]
fn a_term_keeps_the_casing_it_was_written_in() {
    let terms = terms("Kubernetes and kubernetes and KUBERNETES clusters");
    assert_eq!(terms[0].word, "Kubernetes");
    assert_eq!(terms[0].count, 3);
}

#[test]
fn coverage_splits_what_a_cv_answers_from_what_it_does_not() {
    let terms = terms(POSTING);
    let cv = "Led the migration of 62 services onto one deployment path. Kubernetes, Terraform.";
    let read = coverage(&terms, cv);
    assert!(read.matched.iter().any(|t| t == "Kubernetes"));
    assert!(read.matched.iter().any(|t| t.eq_ignore_ascii_case("migration")));
    assert!(read.missing.iter().any(|t| t == "SQL"));
    assert_eq!(read.total(), terms.len());
}

/// `deploy` and `deployment` are the same word to a reader, and the read has to
/// agree or every second row is a false gap.
/// Containment is not enough here: `migrate` is not a prefix of `migration`,
/// they part at the seventh letter. A reader treats them as one word and so
/// must the read, or half the rows are gaps that are not gaps.
#[test]
fn one_word_in_two_shapes_is_one_word() {
    let terms = terms("Own the deployment path and lead the migration.");
    let read = coverage(&terms, "I deploy services and migrate databases.");
    assert!(
        read.matched.iter().any(|t| t == "deployment"),
        "deploy answers deployment: {read:?}"
    );
    assert!(
        read.matched.iter().any(|t| t == "migration"),
        "migrate answers migration: {read:?}"
    );
}

/// The sentence a requirement arrives in is not the requirement.
#[test]
fn the_frame_a_posting_is_written_in_is_not_a_term() {
    let terms = terms("Experience with Kubernetes required. Strong SQL skills preferred.");
    let words: Vec<&str> = terms.iter().map(|t| t.word.as_str()).collect();
    assert_eq!(words, vec!["Kubernetes", "SQL"], "got {words:?}");
}

/// …but a shared first syllable is not a shared word.
#[test]
fn a_shared_prefix_is_not_a_match() {
    let terms = terms("Our data warehouse is central.");
    let read = coverage(&terms, "I maintained a database.");
    assert!(
        read.missing.iter().any(|t| t == "data"),
        "database must not answer data: {read:?}"
    );
}

fn diary(text: &str, confidential: bool) -> DiaryEntry {
    DiaryEntry {
        date: "2026-03-12".into(),
        text: text.into(),
        confidential,
        ..Default::default()
    }
}

#[test]
fn the_vault_is_searched_for_what_the_reading_is_missing() {
    let terms = terms(POSTING);
    let cv = "Led the migration of 62 services onto one deployment path.";
    let missing = coverage(&terms, cv).missing;

    let entries = vec![
        diary("Ran the incident review practice: 41 reviews, no blame section.", false),
        diary("Baked a cake for the team.", false),
    ];
    let library = Library {
        skills: vec![SkillGroup {
            name: "Data".into(),
            keywords: vec!["SQL".into(), "ClickHouse".into()],
        }],
        ..Library::default()
    };

    let found = unused(&missing, &entries, &library, cv);
    assert!(
        found.iter().any(|u| matches!(&u.source, UnusedSource::Diary { .. })
            && u.terms.iter().any(|t| t.eq_ignore_ascii_case("incident"))),
        "the incident win answers a gap: {found:?}"
    );
    assert!(
        found.iter().any(|u| matches!(
            &u.source,
            UnusedSource::Library { section: SectionKind::Skills }
        )),
        "the skills block answers SQL: {found:?}"
    );
    assert!(
        !found.iter().any(|u| u.text.as_deref() == Some("Baked a cake for the team.")),
        "a win that answers nothing is not a prompt"
    );
}

/// **US-36.** A confidential entry is never offered to a CV in its own words.
/// It is still pointed at — the fact of it is useful, the wording is the thing
/// that must not travel — and the type is what makes the rule hold: a caller
/// is handed `None` and cannot print what it does not have.
#[test]
fn a_confidential_win_is_pointed_at_and_never_quoted() {
    let terms = terms(POSTING);
    let missing = coverage(&terms, "").missing;
    let secret = "Personal-data incident at client ACME, contained in one deployment.";
    let found = unused(&missing, &[diary(secret, true)], &Library::default(), "");

    let entry = found.first().expect("it answers several gaps");
    assert!(matches!(
        entry.source,
        UnusedSource::Diary {
            confidential: true,
            ..
        }
    ));
    assert_eq!(entry.text, None, "its wording must not leave the diary");
    assert!(!entry.terms.is_empty(), "but it is still worth looking at");
}

/// A win whose terms the CV already carries in its own wording is not a gap,
/// however well it matches the posting.
#[test]
fn something_already_on_the_page_is_not_offered_again() {
    let terms = terms(POSTING);
    let cv = "Ran the incident review practice for two years.";
    let missing = coverage(&terms, cv).missing;
    let found = unused(
        &missing,
        &[diary("Started the incident review practice.", false)],
        &Library::default(),
        cv,
    );
    assert!(found.is_empty(), "already said: {found:?}");
}

#[test]
fn an_empty_posting_asks_nothing_of_anything() {
    assert!(terms("").is_empty());
    assert!(terms("   \n  ").is_empty());
    let found = unused(&[], &[diary("Anything at all.", false)], &Library::default(), "");
    assert!(found.is_empty());
}

/// The library is a pool of blocks, and a block that answers a gap is as much
/// a prompt as a diary entry is.
#[test]
fn a_library_block_is_a_prompt_too() {
    let terms = terms("Deep Kubernetes and Terraform experience required.");
    let missing = coverage(&terms, "").missing;
    let library = Library {
        work: vec![Work {
            position: "Platform Engineer".into(),
            highlights: vec!["Ran the Kubernetes fleet.".into()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let found = unused(&missing, &[], &library, "");
    assert!(matches!(
        found[0].source,
        UnusedSource::Library {
            section: SectionKind::Work
        }
    ));
}
