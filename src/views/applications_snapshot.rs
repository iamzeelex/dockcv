//! Pinning a CV to an application, and capturing what was actually sent
//! (roadmap D4a, review US-04 / P-03).
//!
//! The review's complaint is precise: a card that points at a live preset
//! *lies within a month*. Edit the CV in July and the card for a company you
//! applied to in March now claims you sent them a document they never saw. So
//! the card stores a **file**: real PDF bytes in `<vault>/snapshots/`, written
//! once, never regenerated. Editing the document afterwards cannot reach it.
//!
//! Capture happens at exactly one moment — the first time a card reaches
//! `Applied` — because that is the moment the claim "this is what they got"
//! becomes true. Later moves (Interviewing, Offer, Rejected) do not re-capture:
//! the company is still holding the March PDF no matter how far the process
//! goes.

use std::sync::{Arc, Mutex};

use gpui::Context;

use crate::resume::model::{ApplicationStatus, Snapshot};
use crate::resume::template;
use crate::typst_engine::TypstEngine;
use crate::vault;

use super::save_status;

use super::shell::Shell;

/// One choice in the card menu's "Pin CV" section: a document in the vault,
/// optionally at one of its named presets.
pub(super) struct PinOption {
    /// The document's file stem — what `Application::source_doc` stores.
    pub stem: String,
    /// Preset name, or empty for "whatever the document's active variants are".
    pub preset: String,
    /// What the menu item reads.
    pub label: String,
}

/// Every document × preset combination in the vault, for the pin menu.
///
/// Deliberately computed when the menu opens rather than per frame: it loads
/// every document in the vault to read its preset *names* (`DocMeta` carries
/// only a count), which is far too much work to repeat on every render.
pub(super) fn pin_options(vault: &std::path::Path) -> Vec<PinOption> {
    let mut options = Vec::new();
    for path in vault::list_documents(vault) {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(doc) = vault::load(&path) else {
            continue;
        };
        let name = {
            let person = doc.profile.active().name.trim().to_string();
            if person.is_empty() {
                stem.to_string()
            } else {
                person
            }
        };

        if doc.presets.is_empty() {
            // A document with no presets is still a document you can send;
            // it just has no name for the cut you sent.
            options.push(PinOption {
                stem: stem.to_string(),
                preset: String::new(),
                label: name,
            });
        } else {
            for preset in &doc.presets {
                options.push(PinOption {
                    stem: stem.to_string(),
                    preset: preset.name.clone(),
                    label: format!("{name} · {}", preset.name),
                });
            }
        }
    }
    options
}

impl Shell {
    /// Record which document and preset this application was (or will be) sent
    /// with. Both are stored as **labels**: the document can be renamed or
    /// deleted afterwards and the card still tells the truth about what was
    /// sent, because the snapshot is the real evidence, not this pointer.
    pub(super) fn pin_application_cv(
        &mut self,
        index: usize,
        stem: String,
        preset: String,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let mut applications = vault::load_applications(&vault);
        let Some(application) = applications.entries.get_mut(index) else {
            return;
        };
        application.source_doc = Some(stem);
        application.preset = preset;
        let already_sent = application.status != ApplicationStatus::Wishlist;
        save_status::record(cx, "applications board", vault::save_applications(&vault, &applications));
        cx.notify();

        // Pinning a CV to a card that has already been sent is the user
        // correcting the record after the fact. Capture then, since the
        // Applied transition that would normally have done it is in the past.
        if already_sent {
            self.capture_snapshot(index, cx);
        }
    }

    /// Compile the pinned document at its pinned preset and store the bytes as
    /// this application's next snapshot.
    ///
    /// Silent no-op when nothing is pinned — the card renders "no CV pinned"
    /// in that case, so the absence is visible rather than mysterious.
    pub(super) fn capture_snapshot(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let applications = vault::load_applications(&vault);
        let Some(application) = applications.entries.get(index) else {
            return;
        };
        let Some(stem) = application.source_doc.clone() else {
            return;
        };
        let company = application.company.clone();
        let preset = application.preset.clone();
        let version = application.snapshots.len() as u32 + 1;
        let doc_path = vault.join(format!("{stem}.toml"));

        let engine = self
            .thumb_engine
            .get_or_insert_with(|| Arc::new(Mutex::new(TypstEngine::new(String::new()))))
            .clone();
        let executor = cx.background_executor().clone();

        cx.spawn(async move |this, cx| {
            let preset_for_compile = preset.clone();
            let bytes = executor
                .spawn(async move {
                    let mut doc = vault::load(&doc_path)?;
                    // Apply the pinned preset, so the snapshot is the cut that
                    // was sent rather than whichever variants happen to be
                    // active in the document right now.
                    if !preset_for_compile.is_empty() {
                        if let Some(i) = doc
                            .presets
                            .iter()
                            .position(|p| p.name == preset_for_compile)
                        {
                            doc.apply_preset(i);
                        }
                    }
                    let source = template::generate_for(&doc);
                    let mut engine = engine.lock().map_err(|e| format!("engine busy: {e}"))?;
                    engine.set_source(source);
                    engine.compile_to_pdf()
                })
                .await;

            let _ = this.update(cx, |_this, cx| {
                let bytes = match bytes {
                    Ok(bytes) => bytes,
                    // The compile failed, not the disk. Reported as a snapshot
                    // failure because that is the action the user took.
                    Err(message) => {
                        save_status::record(cx, "snapshot", Err(message));
                        cx.notify();
                        return;
                    }
                };
                let file = match vault::save_snapshot(&vault, &bytes, &company, version) {
                    Ok(file) => file,
                    Err(message) => {
                        save_status::record(cx, "snapshot", Err(message));
                        cx.notify();
                        return;
                    }
                };
                save_status::record(cx, "snapshot", Ok(()));

                // Re-read rather than trusting the index we started with: the
                // compile ran on the background executor, and the board is
                // editable while it does. If the row moved underneath us,
                // dropping the snapshot record is the honest outcome — a
                // snapshot filed against the wrong company is worse than none.
                let mut applications = vault::load_applications(&vault);
                let Some(application) = applications.entries.get_mut(index) else {
                    return;
                };
                if application.company != company {
                    return;
                }
                application.snapshots.push(Snapshot {
                    version,
                    date: vault::today_iso(),
                    preset,
                    file,
                });
                save_status::record(cx, "applications board", vault::save_applications(&vault, &applications));
                cx.notify();
            });
        })
        .detach();
    }

    /// Reveal a stored snapshot through the OS, so the user can read the exact
    /// PDF a company received (US-04: "the snapshot opens from the card").
    pub(super) fn reveal_snapshot(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let applications = vault::load_applications(&vault);
        let Some(file) = applications
            .entries
            .get(index)
            .and_then(|a| a.snapshots.last())
            .map(|s| s.file.clone())
        else {
            return;
        };
        let path = vault::snapshots_dir(&vault).join(file);
        if path.exists() {
            cx.open_with_system(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::pin_options;
    use crate::resume::model::{Preset, Resume, ResumeDoc, SectionKind};
    use crate::vault;

    /// Every document × preset in the vault, and — this is the part worth
    /// pinning down — a document with **no** presets still offers itself.
    /// Dropping it would make its CV unsendable from the board for no reason
    /// the user could see.
    #[test]
    fn every_document_is_offerable_presets_or_not() {
        let dir = std::env::temp_dir().join(format!("dockcv-pin-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp vault");

        let mut with_presets = ResumeDoc::from_resume(
            Resume {
                basics: crate::resume::model::Basics {
                    name: "Sofiia Medvedenko".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            "Base",
        );
        with_presets.presets = vec![
            Preset {
                name: "FAANG · concise".into(),
                selection: vec![(SectionKind::Profile, "Base".into())],
                hidden: Vec::new(),
            },
            Preset {
                name: "Infra-heavy".into(),
                selection: vec![(SectionKind::Profile, "Base".into())],
                hidden: Vec::new(),
            },
        ];
        vault::save(&with_presets, &dir.join("sofiia-senior-swe.toml")).expect("save");

        // No presets, and no person name either — the label must fall back to
        // the file stem rather than rendering an empty menu item.
        let bare = ResumeDoc::from_resume(Resume::default(), "Base");
        vault::save(&bare, &dir.join("draft-cv.toml")).expect("save");

        let options = pin_options(&dir);
        let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();

        assert!(labels.contains(&"Sofiia Medvedenko · FAANG · concise"));
        assert!(labels.contains(&"Sofiia Medvedenko · Infra-heavy"));
        assert!(labels.contains(&"draft-cv"), "got {labels:?}");
        assert_eq!(options.len(), 3);

        // The preset a snapshot records is the one that was picked, and the
        // bare document records none rather than inventing a name.
        let bare_option = options.iter().find(|o| o.stem == "draft-cv").expect("bare");
        assert_eq!(bare_option.preset, "");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
