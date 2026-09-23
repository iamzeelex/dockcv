//! What the posting read claims, and what it must never do.

use super::*;
use crate::resume::model::{DiaryEntry, Library, SectionKind, SkillGroup, Work};

const POSTING: &str = "We are looking for a Platform Engineer to own our deployment \
     platform. You will lead the migration of forty services onto one deployment path, \
     run the incident review practice, and work with Kubernetes, Terraform and AWS. \
     Experience with Kubernetes is required. Strong SQL is a plus.";

/// The heart of it: a word only helps choose between versions when the versions
/// disagree about it. `deployment` is in both, so it breaks no tie however
/// often the posting says it; `Kubernetes` is in one, so it does.
#[test]
fn a_deciding_term_is_one_the_versions_disagree_about() {
    let long = "Led the deployment platform migration. Kubernetes, Terraform.".to_string();
    let short = "Led the deployment platform migration.".to_string();
    let words: Vec<String> = deciding_terms(POSTING, &[long, short])
        .into_iter()
        .map(|t| t.word)
        .collect();

    assert!(words.iter().any(|w| w == "Kubernetes"), "got {words:?}");
    assert!(
        !words.iter().any(|w| w.eq_ignore_ascii_case("deployment")),
        "both versions say it, so it decides nothing: {words:?}"
    );
    assert!(
        !words.iter().any(|w| w == "SQL"),
        "neither version says it, so it is a gap and not a tie-break: {words:?}"
    );
}

/// The frequency read this replaces called these terms, and they were most of
/// what it found: a posting is mostly the sentence every posting is written in.
#[test]
fn the_frame_a_posting_shares_with_every_posting_cannot_reach_the_list() {
    let a = "We work with our team on production services every year.".to_string();
    let b = "Our team works on production.".to_string();
    let words: Vec<String> = deciding_terms(
        "You will work with our team on production services. Experience required.",
        &[a, b],
    )
    .into_iter()
    .map(|t| t.word)
    .collect();
    // `services` survives, and that is the rule working rather than failing:
    // it is the reader's own word and their two versions disagree about it.
    // What cannot get through is the frame — and the frequency read this
    // replaces returned nothing but the frame.
    for frame in ["work", "team", "production", "experience", "required", "year"] {
        assert!(
            !words.iter().any(|w| w.eq_ignore_ascii_case(frame)),
            "{frame} is the sentence, not the job: {words:?}"
        );
    }
}

/// One version is a different question — there is nothing to tell apart — so
/// the count becomes "how much of the posting's vocabulary this page shares".
#[test]
fn a_single_version_is_measured_against_itself() {
    let only = "Kubernetes and Terraform, on one deployment path.".to_string();
    let words: Vec<String> = deciding_terms(POSTING, std::slice::from_ref(&only))
        .into_iter()
        .map(|t| t.word)
        .collect();
    assert!(words.iter().any(|w| w == "Kubernetes"), "got {words:?}");
    assert!(!words.iter().any(|w| w == "SQL"), "not on the page: {words:?}");
}

/// A gap is a word the vault knows and this page does not — which is what makes
/// it worth offering something for.
#[test]
fn a_gap_is_something_the_vault_can_answer() {
    let page = "Led the migration of 62 services onto one deployment path.";
    let vault = "Ran the incident review practice. Data: PostgreSQL, SQL, ClickHouse.";
    let found = gaps(POSTING, page, vault);
    assert!(found.iter().any(|t| t == "SQL"), "got {found:?}");
    assert!(
        found.iter().any(|t| t.eq_ignore_ascii_case("incident")),
        "got {found:?}"
    );
    assert!(
        !found.iter().any(|t| t.eq_ignore_ascii_case("deployment")),
        "the page already says it: {found:?}"
    );
    assert!(
        !found.iter().any(|t| t == "Kubernetes"),
        "the vault has never heard of it, so there is nothing to offer: {found:?}"
    );
}

/// …and the one thing DockCV can say about a gap it cannot help with.
#[test]
fn what_the_vault_has_never_heard_of_is_named_separately() {
    let vault = "Led the migration of 62 services onto one deployment path.";
    let never = absent(POSTING, vault);
    assert!(never.iter().any(|t| t == "Kubernetes"), "got {never:?}");
    assert!(never.iter().any(|t| t == "SQL"), "got {never:?}");
    assert!(
        !never.iter().any(|t| t.eq_ignore_ascii_case("migration")),
        "the vault knows it: {never:?}"
    );
    // Names, not words: the unfiltered version answered `forty` and `practice`.
    for word in ["forty", "lead", "incident", "practice"] {
        assert!(!never.iter().any(|t| t == word), "{word} is not a name: {never:?}");
    }
}

/// Casing is the posting's, because the terms are shown back.
#[test]
fn a_term_keeps_the_casing_it_was_written_in() {
    let terms = deciding_terms(
        "Kubernetes and kubernetes and KUBERNETES clusters",
        &["we run Kubernetes".to_string(), String::new()],
    );
    assert_eq!(terms[0].word, "Kubernetes");
    assert_eq!(terms[0].count, 3);
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
    let cv = "Led the migration of 62 services onto one deployment path.";
    let vault = format!("{cv} Ran the incident review practice. Data: SQL, ClickHouse.");
    let missing = gaps(POSTING, cv, &vault);

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
    let secret = "Personal-data incident at client ACME, contained in one deployment.";
    let missing = gaps(POSTING, "", secret);
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
    let cv = "Ran the incident review practice for two years.";
    let missing = gaps(POSTING, cv, cv);
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
    assert!(deciding_terms("", &["anything".to_string()]).is_empty());
    assert!(gaps("   \n  ", "", "anything at all").is_empty());
    let found = unused(&[], &[diary("Anything at all.", false)], &Library::default(), "");
    assert!(found.is_empty());
}

/// The library is a pool of blocks, and a block that answers a gap is as much
/// a prompt as a diary entry is.
#[test]
fn a_library_block_is_a_prompt_too() {
    let missing = gaps(
        "Deep Kubernetes and Terraform experience required.",
        "",
        "Ran the Kubernetes fleet and the Terraform modules.",
    );
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
