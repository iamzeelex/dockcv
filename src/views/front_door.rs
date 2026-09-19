//! The front door: a list of what you can send, not a grid of files.
//!
//! The gallery listed documents because that is what a file browser lists. But
//! the estate this product is for is one or two CVs and three readings of them
//! (the design doc's §2 numbers), so a grid of cards was a lobby with one door
//! in it — and the largest thing on screen was `+ New CV`, which is the
//! behaviour presets exist to make unnecessary.
//!
//! Two facts said the unit was already wrong. E3 made the preset chips on a
//! card the thing people actually click, and `vault.rs` has indexed preset
//! names for search since E1. The click and the query had both moved to the
//! reading while the layout was still about the file.
//!
//! So the rows are readings. The word "reading" is this file's, not the UI's:
//! on screen a row is called by the preset's own name.

use std::path::PathBuf;

use crate::resume::outcomes::PresetRecord;
use crate::typst_engine::PageGeometry;
use crate::vault::DocMeta;

/// One row: something you could send today.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Reading {
    pub path: PathBuf,
    /// The document's file stem — the heading rows group under, and the label
    /// a document with no presets carries on its own.
    pub stem: String,
    /// Which preset, and its index in the document. `None` for a document that
    /// has no presets: it has exactly one reading, which is itself.
    pub preset: Option<(usize, String)>,
    /// What this version is for, when its author said so. The row's subtitle.
    pub description: Option<String>,
    /// The version this one was made from, by name — a label, not a link.
    pub based_on: Option<String>,
    /// The document's mtime. A property of the file, so every reading of one
    /// document shows the same thing — which is honest: a preset has no
    /// modification date of its own, and inventing one per row would be a
    /// number that looks per-reading and is not.
    pub modified_secs: Option<u64>,
    pub unreadable: bool,
}

impl Reading {
    /// What the row is called. The preset's name, or the file's stem when the
    /// document has no presets to name it by.
    pub(super) fn label(&self) -> &str {
        match &self.preset {
            Some((_, name)) => name,
            None => &self.stem,
        }
    }

    /// The key the applications board records a send under.
    pub(super) fn sent_as(&self) -> (&str, &str) {
        (
            &self.stem,
            self.preset.as_ref().map(|(_, n)| n.as_str()).unwrap_or(""),
        )
    }

    /// The one line under the name: what this version is for, and what it came
    /// from. Composed rather than stacked, so the row keeps its height.
    pub(super) fn subtitle(&self) -> Option<String> {
        let lineage = self.based_on.as_ref().map(|from| format!("based on {from}"));
        match (self.description.clone(), lineage) {
            (Some(what), Some(from)) => Some(format!("{what} · {from}")),
            (Some(what), None) => Some(what),
            (None, Some(from)) => Some(from),
            // One word. The sentence that used to be here explained our data
            // model to somebody who never asked about it, on every row of an
            // unorganised vault — and the callout says it once instead.
            (None, None) => self.is_draft().then(|| "draft".to_string()),
        }
    }

    /// A document with no presets has not been organised into one yet — the
    /// gallery called that a draft and the word still fits.
    pub(super) fn is_draft(&self) -> bool {
        self.preset.is_none()
    }
}

/// One document and everything it can send.
///
/// The list is grouped because that is the model: a CV owns versions, and a
/// version has no existence apart from its CV. The flat list did not say so — a
/// draft row and a version row were drawn identically, each with a ··· that
/// looked the same and destroyed different things.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Group {
    pub path: PathBuf,
    pub stem: String,
    /// At least one. A document with no presets contributes itself.
    pub readings: Vec<Reading>,
}

impl Group {
    /// Whether the document is drawn as a header over its rows.
    ///
    /// Only when it holds more than one. With one version the header and the
    /// row named two halves of the same thing on two lines — `imported-5` over
    /// `Preset 1` — so the single row carries the whole address instead and the
    /// header is not drawn.
    pub(super) fn needs_header(&self) -> bool {
        self.readings.len() > 1
    }
}

/// Every document in the vault, in the order they were given, each with its
/// readings.
///
/// A document that will not parse still gets a group: it is in the vault, and a
/// list that silently omitted it would be the vault disagreeing with the folder.
pub(super) fn groups(metas: &[DocMeta]) -> Vec<Group> {
    metas
        .iter()
        .map(|meta| {
            let readings = if meta.presets.is_empty() {
                vec![Reading {
                    path: meta.path.clone(),
                    stem: meta.stem.clone(),
                    preset: None,
                    description: None,
                    based_on: None,
                    modified_secs: meta.modified_secs,
                    unreadable: meta.unreadable,
                }]
            } else {
                meta.presets
                    .iter()
                    .enumerate()
                    .map(|(index, preset)| Reading {
                        path: meta.path.clone(),
                        stem: meta.stem.clone(),
                        preset: Some((index, preset.name.clone())),
                        description: preset.description.clone(),
                        based_on: preset.based_on.clone(),
                        modified_secs: meta.modified_secs,
                        unreadable: meta.unreadable,
                    })
                    .collect()
            };
            Group {
                path: meta.path.clone(),
                stem: meta.stem.clone(),
                readings,
            }
        })
        .collect()
}

/// The same thing flattened, for the callers that want rows rather than
/// documents — the tailor sheet picks one reading out of the whole vault.
pub(super) fn readings(metas: &[DocMeta]) -> Vec<Reading> {
    groups(metas)
        .into_iter()
        .flat_map(|group| group.readings)
        .collect()
}

/// How many pages this version prints to.
///
/// A plain fact, and deliberately nothing more. The meter and the `+14 lines
/// over` that stood here were both measuring against **one page** — a target
/// the person never set. Our own numbers say a two-page CV converts slightly
/// better than a one-page one, so "over" was the app having an opinion about a
/// choice that belongs to the author. When a document can declare the length it
/// is aiming at, this can go back to reporting the difference; until then it
/// reports the length.
///
/// Empty until measured.
pub(super) fn page_count_label(geometry: Option<&PageGeometry>) -> String {
    let Some(geometry) = geometry else {
        return String::new();
    };
    let pages = geometry.page_count.max(1);
    let noun = if pages == 1 { "page" } else { "pages" };
    format!("{pages} {noun}")
}

/// Whether this is a name DockCV invented rather than one a person chose.
///
/// Saving the working copy hands out `Version 1`, `Version 2`, … and the front
/// door then showed that as if it were a title, with the word `unnamed` beside
/// it saying the same thing twice. A generated name is a placeholder: the row
/// prints it muted and makes it the control that replaces it, instead of
/// dressing it as an answer and then contradicting itself.
///
/// `Preset N` is the same thing under the word this product used before it
/// settled on "version", and vaults written then are still on disk — so both
/// prefixes are recognised and neither is rewritten.
pub(super) fn is_generated_name(name: &str) -> bool {
    ["Version ", "Preset "].iter().any(|prefix| {
        name.strip_prefix(prefix)
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
    })
}

/// One sentence about what this version has done, and when it last changed.
///
/// Three mono columns each with a qualifier under it read as telemetry about a
/// document rather than as a description of one — and on a fresh vault four of
/// the six lines were identical on every row. The same facts in a sentence say
/// only what there is to say: a version nobody has sent gets three words.
pub(super) fn history_line(record: PresetRecord, modified_secs: Option<u64>, now: u64) -> String {
    let sent = match (record.sent, record.interviewed) {
        (0, _) => "Never sent".to_string(),
        (1, 0) => "Sent once".to_string(),
        (n, 0) => format!("Sent {n} times"),
        (1, 1) => "Sent once, 1 interview".to_string(),
        (n, 1) => format!("Sent {n} times, 1 interview"),
        (n, m) => format!("Sent {n} times, {m} interviews"),
    };
    match modified_secs {
        Some(secs) => format!("{sent} · updated {}", crate::vault::relative_time(secs, now)),
        None => sent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(stem: &str, presets: &[&str]) -> DocMeta {
        DocMeta {
            path: PathBuf::from(format!("/vault/{stem}.toml")),
            stem: stem.into(),
            name: "Albert Einstein".into(),
            label: "Engineer".into(),
            presets: presets
                .iter()
                .map(|p| crate::vault::PresetMeta {
                    name: p.to_string(),
                    based_on: None,
                    description: None,
                })
                .collect(),
            unreadable: false,
            modified_secs: Some(1_700_000_000),
            search: Vec::new(),
        }
    }

    /// A document contributes one row per preset, and a document with none
    /// contributes itself.
    #[test]
    fn every_preset_is_a_row_and_a_draft_is_one_too() {
        let rows = readings(&[
            meta("resume", &["Infra-heavy", "FAANG · concise"]),
            meta("academic-cv", &[]),
        ]);

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].label(), "Infra-heavy");
        assert_eq!(rows[1].label(), "FAANG · concise");
        assert_eq!(rows[0].preset, Some((0, "Infra-heavy".to_string())));
        assert_eq!(rows[1].preset, Some((1, "FAANG · concise".to_string())));

        assert_eq!(rows[2].label(), "academic-cv");
        assert!(rows[2].is_draft());
        assert_eq!(rows[2].sent_as(), ("academic-cv", ""));
    }

    /// Length is a fact the row states, not a verdict it passes.
    #[test]
    fn the_page_count_is_stated_and_nothing_is_inferred_from_it() {
        assert_eq!(page_count_label(None), "", "nothing before the compiler ran");
        assert_eq!(page_count_label(Some(&geometry(1, 0.0, Some(14.0)))), "1 page");
        assert_eq!(page_count_label(Some(&geometry(2, 350.0, Some(14.0)))), "2 pages");
        assert_eq!(page_count_label(Some(&geometry(3, 900.0, Some(14.0)))), "3 pages");
    }

    /// The names DockCV hands out are placeholders, and the screen has to be
    /// able to tell them from titles a person chose — under the word this
    /// product uses now and the one it used before.
    #[test]
    fn a_generated_name_is_recognised_as_a_placeholder() {
        assert!(is_generated_name("Version 1"));
        assert!(is_generated_name("Version 12"));
        assert!(is_generated_name("Preset 1"));
        assert!(is_generated_name("Preset 12"));
        assert!(!is_generated_name("Version"));
        assert!(!is_generated_name("Preset"));
        assert!(!is_generated_name("Preset one"));
        assert!(!is_generated_name("Infra-heavy"));
        assert!(!is_generated_name("Preset 1 · concise"));
    }

    /// A document is a group, and the group is what the screen draws: a header
    /// only when there is more than one version under it, and never over a
    /// draft — which is the whole of the fix for `imported-5 · 1 version`
    /// costing two lines to say one thing.
    #[test]
    fn a_document_with_one_reading_does_not_get_a_header() {
        let all = groups(&[
            meta("resume", &["Infra-heavy", "FAANG · concise"]),
            meta("imported-5", &["Northwind"]),
            meta("academic-cv", &[]),
        ]);

        assert_eq!(all.len(), 3, "one group per document, in the vault's order");

        assert_eq!(all[0].stem, "resume");
        assert_eq!(all[0].readings.len(), 2);
        assert!(all[0].needs_header());

        assert_eq!(all[1].readings.len(), 1);
        assert!(!all[1].needs_header(), "one version addresses itself");
        assert!(
            !all[1].readings[0].is_draft(),
            "it has a version, it is simply the only one"
        );

        assert!(all[2].readings[0].is_draft());
        assert!(!all[2].needs_header());
        assert_eq!(all[2].readings[0].label(), "academic-cv");
    }

    /// Flattening the groups is the old list exactly, so the callers that want
    /// rows rather than documents keep working.
    #[test]
    fn the_flat_list_is_the_groups_in_order() {
        let metas = [
            meta("resume", &["Infra-heavy", "FAANG · concise"]),
            meta("academic-cv", &[]),
        ];
        let flat = readings(&metas);
        let grouped: Vec<Reading> = groups(&metas)
            .into_iter()
            .flat_map(|group| group.readings)
            .collect();
        assert_eq!(flat, grouped);
        assert_eq!(flat.len(), 3);
    }

    /// The history is a sentence, and it says only what there is to say.
    #[test]
    fn the_history_line_reads_like_a_person_wrote_it() {
        let now = 1_700_000_000;
        assert_eq!(
            history_line(PresetRecord::default(), Some(now - 60), now),
            "Never sent · updated 1m ago"
        );
        assert_eq!(
            history_line(
                PresetRecord {
                    sent: 11,
                    interviewed: 4
                },
                None,
                now
            ),
            "Sent 11 times, 4 interviews"
        );
        assert_eq!(
            history_line(
                PresetRecord {
                    sent: 1,
                    interviewed: 1
                },
                None,
                now
            ),
            "Sent once, 1 interview"
        );
    }

    /// A4 at the template's margins: a 700pt column on an 842pt sheet. The
    /// distinction is the whole point of `column_pt`, so the fixture keeps it.
    fn geometry(pages: usize, overflow_pt: f64, line_advance_pt: Option<f64>) -> PageGeometry {
        PageGeometry {
            page_count: pages,
            page_height_pt: 842.0,
            column_pt: 700.0,
            last_page_used_pt: if pages > 1 { overflow_pt } else { 300.0 },
            last_page_content_top_pt: 40.0,
            overflow_pt,
            line_advance_pt,
        }
    }
}
