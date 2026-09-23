//! Asking the assistant which of *your* changes fit the job.
//!
//! The machinery for reaching an assistant is shared (`views::assistant`); what
//! lives here is the prompt, because the prompt is the whole of the argument.
//!
//! **The model is handed a menu and asked to choose from it.** Not "tailor this
//! CV", which is the thing every other tool does and the thing this product
//! exists not to do — a model that rewrites a bullet invents the number in it,
//! and US-14 says a number on a CV traces to something the user typed. The
//! menu is what `changes::available` already computes: the other cuts of each
//! section, under the names and descriptions their author gave them, plus the
//! diary entries and library blocks the read found. Every item has an id that
//! already exists.
//!
//! So the answer can only ever be a set of ids and a sentence of reasoning per
//! id. There is no field in it that could carry text onto the page, which means
//! the worst a wrong answer can do is suggest a change the person then does not
//! make. That is a different risk class from a model writing prose into a
//! document somebody sends to an employer.
//!
//! Nothing is applied by arriving. The picks come back marked, and the person
//! clicks them.

use super::changes::Change;
use crate::resume::posting::Unused;
use crate::resume::posting::UnusedSource;

/// What the assistant is asked, with the menu spelled out.
///
/// One line, no newlines at all — the same constraint the import prompt is
/// under, for the same reason: these travel in a `q=` parameter and Claude
/// Desktop was observed clearing a composer that had `%0A` in it.
pub(crate) fn advice_prompt(posting: &str, changes: &[Change], unused: &[Unused]) -> String {
    let menu = changes
        .iter()
        .enumerate()
        .map(|(i, change)| match &change.description {
            Some(what) => format!("C{i}: \"{}\" — {what}", change.name),
            None => format!("C{i}: \"{}\"", change.name),
        })
        .collect::<Vec<_>>()
        .join("; ");

    let notes = unused
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            // A confidential entry is named and never quoted — including to a
            // model. `text` is `None` for one, and there is nothing else here
            // that could leak the wording (US-36).
            let body = match (&item.text, &item.source) {
                (Some(text), _) => text.clone(),
                (None, UnusedSource::Diary { date, .. }) => {
                    format!("a diary entry from {date}, withheld")
                }
                (None, _) => return None,
            };
            Some(format!("N{i}: {body}"))
        })
        .collect::<Vec<_>>()
        .join("; ");

    let mut prompt = String::from(
        "I am choosing which version of my CV to send for a job. Below is the job posting, \
         then a numbered menu of changes I could make and notes I have already written. \
         Tell me which to use. ",
    );
    prompt.push_str(RULES);
    prompt.push_str(" JOB POSTING: ");
    prompt.push_str(&one_line(posting));
    if !menu.is_empty() {
        prompt.push_str(" CHANGES I COULD MAKE: ");
        prompt.push_str(&one_line(&menu));
    }
    if !notes.is_empty() {
        prompt.push_str(" THINGS I HAVE WRITTEN DOWN: ");
        prompt.push_str(&one_line(&notes));
    }
    prompt
}

/// The part that keeps the answer a choice rather than a draft.
const RULES: &str = "Rules: reply with a single JSON object and nothing else, shaped \
     {\"changes\":[{\"id\":\"C0\",\"why\":\"one short sentence\"}],\"notes\":[{\"id\":\"N0\",\"why\":\"one \
     short sentence\"}]}; use only ids from the menu below and never invent one; leave a list \
     empty rather than padding it; do not write, rewrite or suggest any wording for the CV, \
     because I am not going to paste your text anywhere — you are picking from what I already \
     have; do not comment on whether I am qualified.";

/// Flatten to one line, and cap the posting so the whole prompt stays inside
/// the roughly 14,000 characters Claude Desktop truncates `q` at.
fn one_line(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= MAX_SEGMENT {
        return flat;
    }
    // Cut on a character boundary, and say that it was cut — a prompt that
    // silently loses the last third of a posting produces advice about half a
    // job with no sign that is what happened.
    let head: String = flat.chars().take(MAX_SEGMENT).collect();
    format!("{head} […truncated]")
}

/// Per segment, not for the whole prompt: three of these plus the rules stay
/// well inside the limit, and no single part can crowd out the others.
const MAX_SEGMENT: usize = 3600;

/// What came back: ids, and why.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Advice {
    pub changes: Vec<(usize, String)>,
    pub notes: Vec<(usize, String)>,
}

/// Read the answer, keeping only ids that exist.
///
/// A model that invents `C9` against a menu of four is not corrected and not
/// reported as an error — the row simply is not there. The alternative is an
/// error message about somebody else's product for a suggestion the person can
/// see is missing.
pub(crate) fn parse_advice(answer: &str, changes: usize, notes: usize) -> Advice {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(extract_json(answer)) else {
        return Advice::default();
    };
    Advice {
        changes: picks(&value, "changes", 'C', changes),
        notes: picks(&value, "notes", 'N', notes),
    }
}

fn picks(value: &serde_json::Value, key: &str, prefix: char, limit: usize) -> Vec<(usize, String)> {
    let Some(list) = value.get(key).and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in list {
        let Some(id) = item.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(index) = id
            .trim()
            .strip_prefix(prefix)
            .and_then(|n| n.parse::<usize>().ok())
        else {
            continue;
        };
        if index >= limit || out.iter().any(|(i, _)| *i == index) {
            continue;
        }
        let why = item
            .get("why")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        out.push((index, why));
    }
    out
}

/// The same tolerance the import answer gets: a model that says hello first is
/// still answering.
fn extract_json(answer: &str) -> &str {
    let trimmed = answer.trim();
    let inner = match trimmed.strip_prefix("```") {
        Some(rest) => {
            let rest = rest.strip_prefix("json").unwrap_or(rest);
            let rest = rest.trim_start_matches('\n').trim_end();
            rest.strip_suffix("```").unwrap_or(rest).trim()
        }
        None => trimmed,
    };
    match (inner.find('{'), inner.rfind('}')) {
        (Some(open), Some(close)) if close > open => &inner[open..=close],
        _ => inner,
    }
}

#[cfg(test)]
mod tests;
