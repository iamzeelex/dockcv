//! What the prompt promises, and what the answer is allowed to be.

use super::*;
use crate::resume::model::SectionKind;
use crate::views::front_door::changes::{Change, ChangeKind};

fn change(name: &str, description: Option<&str>) -> Change {
    Change {
        section: SectionKind::Skills,
        kind: ChangeKind::Hide,
        name: name.to_string(),
        description: description.map(str::to_string),
        applied: false,
    }
}

fn note(text: Option<&str>, confidential: bool) -> Unused {
    Unused {
        source: UnusedSource::Diary {
            date: "2026-03-12".into(),
            confidential,
        },
        text: text.map(str::to_string),
        terms: vec!["incident".into()],
    }
}

/// The whole argument in one assertion: the model is given ids to choose from
/// and told not to write anything. A prompt that asked it to tailor the CV
/// would be the thing this product exists not to do.
#[test]
fn the_prompt_hands_over_a_menu_and_forbids_writing() {
    let prompt = advice_prompt(
        "We need a platform engineer.",
        &[change("Concise work", Some("Three bullets a role"))],
        &[note(Some("Ran the incident review practice."), false)],
    );
    assert!(prompt.contains("C0: \"Concise work\" — Three bullets a role"));
    assert!(prompt.contains("N0: Ran the incident review practice."));
    let lower = prompt.to_lowercase();
    assert!(lower.contains("do not write, rewrite or suggest any wording"));
    assert!(lower.contains("only ids from the menu"));
}

/// Every prompt that travels in a URL is one line — Claude Desktop was seen
/// clearing a composer that had `%0A` in it.
#[test]
fn the_prompt_is_one_line() {
    let prompt = advice_prompt(
        "We need someone.\n\nSecond paragraph.\n\tTabbed.",
        &[change("A", None)],
        &[],
    );
    assert!(!prompt.contains('\n'), "{prompt}");
    assert!(!prompt.contains('\t'), "{prompt}");
}

/// **US-36.** A confidential entry is named and never quoted — including to a
/// model. The read hands this function `None` for its text, and what goes out
/// is the fact that it exists.
#[test]
fn a_confidential_note_is_never_sent_verbatim() {
    let prompt = advice_prompt("anything", &[], &[note(None, true)]);
    assert!(prompt.contains("a diary entry from 2026-03-12, withheld"));
    assert!(!prompt.to_lowercase().contains("acme"));
}

/// A posting can be pages long and `q` is truncated at roughly 14,000
/// characters. Cutting it here, and saying so, beats the link silently losing
/// the end of the job.
#[test]
fn a_long_posting_is_cut_and_says_it_was() {
    let prompt = advice_prompt(&"word ".repeat(4000), &[], &[]);
    assert!(prompt.contains("[…truncated]"));
    assert!(prompt.chars().count() < 14_000, "{}", prompt.chars().count());
}

#[test]
fn an_answer_is_a_set_of_ids_and_reasons() {
    let advice = parse_advice(
        r#"```json
        {"changes":[{"id":"C1","why":"the role is platform-shaped"}],
         "notes":[{"id":"N0","why":"it answers their incident question"}]}
        ```"#,
        3,
        2,
    );
    assert_eq!(advice.changes, vec![(1, "the role is platform-shaped".into())]);
    assert_eq!(advice.notes, vec![(0, "it answers their incident question".into())]);
}

/// An id the menu does not have is dropped rather than reported. The row
/// simply is not there, which is what the person can already see.
#[test]
fn an_invented_id_is_dropped() {
    let advice = parse_advice(r#"{"changes":[{"id":"C9"},{"id":"nonsense"},{"id":"C0"}]}"#, 2, 0);
    assert_eq!(advice.changes, vec![(0, String::new())]);
}

#[test]
fn a_repeated_id_is_counted_once() {
    let advice = parse_advice(r#"{"changes":[{"id":"C0","why":"a"},{"id":"C0","why":"b"}]}"#, 1, 0);
    assert_eq!(advice.changes, vec![(0, "a".into())]);
}

/// Nothing usable is nothing, not an error: the screen already shows the menu,
/// and a failed hand-off leaves the person exactly where they were.
#[test]
fn an_answer_that_is_not_an_answer_selects_nothing() {
    for junk in ["", "I cannot help with that.", "{", "[]"] {
        assert_eq!(parse_advice(junk, 4, 4), Advice::default(), "on {junk:?}");
    }
}
