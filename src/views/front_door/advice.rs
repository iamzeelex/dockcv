//! Asking the assistant which of *your* changes fit the job.
//!
//! The machinery for reaching an assistant is shared (`views::assistant`); what
//! lives here is the prompt, because the prompt is the whole of the argument.
//!
//! Browser assistants get a menu of existing CV changes and exact reusable
//! blocks. Local coding assistants can edit the TOML to add a new variant and
//! preset, using only saved content. Either path is grounded in what the user
//! wrote: a model must never invent the metrics or facts that reach a CV.
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
use crate::resume::posting::UnusedSource;
use crate::resume::posting::Unused;

/// What the assistant is asked, with the menu spelled out.
///
/// One line, no newlines at all — the same constraint the import prompt is
/// under, for the same reason: these travel in a `q=` parameter and Claude
/// Desktop was observed clearing a composer that had `%0A` in it.
pub(crate) fn advice_prompt(
    posting: &str,
    current_cv: &str,
    changes: &[Change],
    unused: &[Unused],
    alternatives: &[(usize, String)],
) -> String {
    let menu = changes
        .iter()
        .enumerate()
        .map(|(i, change)| match &change.description {
            Some(what) => format!("C{i}: {:?} · \"{}\" — {what}", change.section, change.name),
            None => format!("C{i}: {:?} · \"{}\"", change.section, change.name),
        })
        .collect::<Vec<_>>()
        .join("; ");

    // These are concrete blocks the user has already written and saved for
    // reuse. Diary entries are intentionally excluded: a diary is not a CV
    // component picker, and even a withheld/confidential entry must not become
    // a candidate id that an assistant can point at.
    let components = component_candidates(unused)
        .into_iter()
        .filter_map(|(_, item)| {
            let UnusedSource::Library { section, .. } = &item.source else {
                unreachable!("component_candidates only returns library blocks")
            };
            let body = item.text.as_deref()?;
            Some(format!(
                "{:?}: {} (answers {})",
                section,
                snippet(body),
                item.terms.join(", ")
            ))
        })
        .enumerate()
        .map(|(i, item)| format!("N{i}: {item}"))
        .collect::<Vec<_>>()
        .join("; ");
    let alternatives = alternatives
        .iter()
        .take(MAX_ALTERNATIVES)
        .map(|(index, text)| format!("C{index}: {}", snippet(text)))
        .collect::<Vec<_>>()
        .join("; ");

    let mut prompt = String::from(
        "Help me make this CV a strong fit for the job using only content I have already written. \
         Compare the posting with my current CV, the existing version choices, and reusable blocks. ",
    );
    prompt.push_str(RULES);
    prompt.push_str(" JOB POSTING: ");
    prompt.push_str(&one_line(posting));
    prompt.push_str(" CURRENT CV: ");
    prompt.push_str(&one_line(current_cv));
    if !menu.is_empty() {
        prompt.push_str(" CHANGES I COULD MAKE: ");
        prompt.push_str(&one_line(&menu));
    }
    if !alternatives.is_empty() {
        prompt.push_str(" CONTENT OF ALTERNATIVE SECTIONS: ");
        prompt.push_str(&one_line(&alternatives));
    }
    if !components.is_empty() {
        prompt.push_str(" SAVED CV COMPONENTS I COULD ADD: ");
        prompt.push_str(&one_line(&components));
    }
    prompt
}

/// Library items in the same order and with the same N ids used in the prompt.
/// Keeping the original read index lets the returned choice resolve to the
/// exact block the user saw even when diary entries are interleaved in the UI.
pub(crate) fn component_candidates(unused: &[Unused]) -> Vec<(usize, &Unused)> {
    unused
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(&item.source, UnusedSource::Library { .. }))
        .take(MAX_COMPONENTS)
        .collect()
}

/// The part that keeps the answer a choice rather than a draft.
const RULES: &str = "Rules: reply with a single JSON object and nothing else, shaped \
     {\"changes\":[{\"id\":\"C0\",\"why\":\"one short sentence\"}],\"components\":[{\"id\":\"N0\",\"why\":\"one \
     short sentence\"}]}; use only ids from the menus below and never invent one; leave a list \
     empty rather than padding it; only recommend listed changes or exact saved components; \
     never invent, rewrite or suggest wording, dates, employers or metrics; do not comment on \
     whether I am qualified.";

/// An assistant with access to the local file can prepare a job-specific
/// version directly in the existing TOML document. Existing content remains
/// available as history: the prompt asks for new variants and a new preset,
/// never a rewrite of the starting version.
pub(crate) fn file_advice_prompt(
    posting: &str,
    path: &std::path::Path,
    company: &str,
    role: &str,
    source: &str,
) -> String {
    let mut prompt = format!(
        "Work on the existing DockCV document at {} for the job below. The vault's reusable CV blocks are in library.toml in the same folder. ",
        one_line(&path.to_string_lossy()),
    );
    prompt.push_str("JOB POSTING: ");
    prompt.push_str(&one_line(posting));
    prompt.push_str(" TARGET VERSION: ");
    prompt.push_str(&one_line(&format!("{company} · {role}")));
    prompt.push_str(" STARTING POINT: ");
    prompt.push_str(&one_line(source));
    prompt.push_str(" INSTRUCTIONS: ");
    prompt.push_str(FILE_RULES);
    prompt
}

const FILE_RULES: &str = "Use the named preset as the baseline when it exists; otherwise use the current saved reading. Inspect the other existing variants too, and let the new preset select an exact existing variant when it fits the job. Use only facts and wording already present in this CV or exact blocks in library.toml. Add only clearly relevant saved blocks. Do not read diary.toml. Do not invent or rewrite bullets, dates, employers, qualifications or metrics. Do not alter or delete any existing variant or preset. Only when adding a library block, duplicate that section's baseline variant, append the block verbatim and select the new variant in the job preset. Create one new preset named for the target job using those selections and the baseline choices for every other section. Keep the document's original active reading unchanged. Edit only the named CV TOML, preserve its schema and all unrelated data, and leave it untouched if no existing variant or saved component is a clear fit. After editing, report the exact variant and preset names added.";

/// Flatten to one line, and cap the posting so the whole prompt stays inside
/// the roughly 14,000 characters Claude Desktop truncates `q` at.
fn one_line(text: &str) -> String {
    compact(text, MAX_SEGMENT)
}

fn snippet(text: &str) -> String {
    compact(text, 320)
}

fn compact(text: &str, max_chars: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max_chars {
        return flat;
    }
    // Cut on a character boundary, and say that it was cut — a prompt that
    // silently loses the last third of a posting produces advice about half a
    // job with no sign that is what happened.
    let head: String = flat.chars().take(max_chars).collect();
    format!("{head} […truncated]")
}

/// Per segment, not for the whole prompt: five capped sections and the rules
/// stay within the limit of browser deep links.
const MAX_SEGMENT: usize = 2400;
const MAX_COMPONENTS: usize = 10;
const MAX_ALTERNATIVES: usize = 10;

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
#[cfg(test)]
pub(crate) fn parse_advice(answer: &str, changes: usize, notes: usize) -> Advice {
    parse_advice_checked(answer, changes, notes)
        .map(|(advice, _)| advice)
        .unwrap_or_default()
}

/// Parse the answer and say when the response did not match the contract.
/// The tolerant wrapper above is kept for callers that only need safe ids;
/// the screen uses this one so malformed answers are not silently ignored.
pub(crate) fn parse_advice_checked(
    answer: &str,
    changes: usize,
    notes: usize,
) -> Result<(Advice, usize), String> {
    let value = serde_json::from_str::<serde_json::Value>(extract_json(answer))
        .map_err(|_| "I couldn't read that as JSON. Copy the prompt and ask for the JSON format.".to_string())?;
    let object = value.as_object().ok_or_else(|| {
        "The answer needs to be one JSON object with changes and components lists.".to_string()
    })?;
    let has_components = object.contains_key("components") || object.contains_key("notes");
    let has_changes = object.contains_key("changes");
    if !has_changes && !has_components {
        return Err("The answer is missing the changes or components list. Copy the prompt and try again.".into());
    }
    let chosen_components = if object.contains_key("components") { "components" } else { "notes" };
    let raw_changes = if has_changes {
        object
            .get("changes")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "The changes field needs to be a JSON list.".to_string())?
            .len()
    } else {
        0
    };
    let raw_components = if has_components {
        object
            .get(chosen_components)
            .and_then(|v| v.as_array())
            .ok_or_else(|| "The components field needs to be a JSON list.".to_string())?
            .len()
    } else {
        0
    };
    let picked_changes = if has_changes {
        picks(&value, "changes", 'C', changes)
    } else {
        Vec::new()
    };
    let picked_components = if has_components {
        picks(&value, chosen_components, 'N', notes)
    } else {
        Vec::new()
    };
    let ignored = (raw_changes - picked_changes.len()) + (raw_components - picked_components.len());
    if raw_changes + raw_components > 0 && picked_changes.is_empty() && picked_components.is_empty() {
        return Err("The answer did not use any IDs from this draft. Copy a fresh prompt and try again.".into());
    }
    Ok((
        Advice {
            changes: picked_changes,
            notes: picked_components,
        },
        ignored,
    ))
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
