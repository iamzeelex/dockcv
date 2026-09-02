//! What happens to the CVs that already hold a copy of a block you just edited.
//!
//! This is US-03, and the review states the requirement as a question the
//! product may not duck: what happens to three CVs if I change a block they
//! share? Copy or link? P-02 is explicit that either answer is acceptable and
//! that silence is not. If it is a link, there has to be a warning naming the
//! three and a way to detach. If it is a copy, there has to be a way to push
//! the change into the rest. Saying nothing is not on the list.
//!
//! ## The answer, said out loud
//!
//! DockCV is a **copy pool** — that is a data-model invariant, not an accident
//! (`CLAUDE.md`), and every reason for it is a reason the vault works at all: a
//! document is a self-contained TOML file you can read, mail, or restore from a
//! backup on a machine that never had your library. A link would make a CV
//! depend on a file next to it, and File-over-App would stop being true.
//!
//! So the answer is *copy*, and this module is the second half P-02 demands:
//! after saving a block that other documents hold, the user is shown exactly
//! which ones, told that those copies are theirs to keep, and offered the push.
//! Nothing is written to another document without that click.
//!
//! ## Why the status on the card is not the mockup's word
//!
//! The design row draws a `Linked` / `Detached` badge. A stored badge would be
//! a third behaviour on top of copy-or-link — the one thing `CLAUDE.md` says
//! not to add — and, worse, it would claim something the file cannot back:
//! nothing in a document records which library block a copy came from, so a
//! block marked `Linked` would be linked to whatever still happened to match.
//!
//! What *is* real, and is what the user actually needs to know before editing,
//! is whether those copies still say what the library says. So the card carries
//! the derived truth — `2 in sync · 1 tailored` — and the push dialog carries
//! it per document. Recorded as a deliberate deviation in the decisions ledger.
//!
//! ## Tailored copies are protected by default
//!
//! A document whose copy has been reworded is **unticked** when the dialog
//! opens. Tailoring a bullet for one company is the entire point of this
//! product; a push that silently flattens it would destroy the exact work the
//! user came here to do. Ticking it back is one click, and the row says why it
//! started unticked.

use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClickEvent, Context, IntoElement, SharedString};

use dockcv_ui_components::{
    Button, ButtonExt, Checkbox, Disableable, IconName, ScrollableElement, Sizable, Tag,
};

use crate::resume::model::{Library, SectionKind};
use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::vault::{self, DocMeta};

use super::library_usage::UsageIndex;
use super::save_status;
use super::shell::Shell;

/// One document holding a copy of the edited block.
pub(super) struct PushTarget {
    pub stem: String,
    pub label: String,
    pub path: PathBuf,
    /// Its copy had already been reworded away from the library's version.
    pub tailored: bool,
    pub selected: bool,
}

/// The dialog: an edited block, and everywhere a copy of it lives.
pub(super) struct PushReview {
    pub section: SectionKind,
    /// Position in the library pool — where the *new* content is.
    pub index: usize,
    /// The identity the copies still carry, measured before the edit. Editing
    /// an employer's name changes the identity, so this is the only handle
    /// that still finds them.
    pub identity: String,
    /// What to call the block on screen.
    pub title: String,
    pub targets: Vec<PushTarget>,
}

impl PushReview {
    pub(super) fn selected(&self) -> usize {
        self.targets.iter().filter(|t| t.selected).count()
    }
}

/// Everywhere the block at `index` is currently copied, as push targets.
///
/// Takes the library **as it was before the edit**, because that is what the
/// copies in those documents were made from: divergence measured against the
/// new content would be true of every document by definition and would tell
/// the user nothing.
pub(super) fn targets_for<'a>(
    before: &Library,
    docs: impl IntoIterator<Item = (&'a DocMeta, &'a crate::resume::model::ResumeDoc)>,
    section: SectionKind,
    index: usize,
) -> Vec<PushTarget> {
    let mut paths: Vec<(String, PathBuf)> = Vec::new();
    let docs: Vec<_> = docs.into_iter().collect();
    for (meta, _) in &docs {
        paths.push((meta.stem.clone(), meta.path.clone()));
    }

    let usage = UsageIndex::build(before, docs.iter().map(|(m, d)| (*m, *d)));
    usage
        .documents_for(before, section, index)
        .iter()
        .filter_map(|reference| {
            let path = paths
                .iter()
                .find(|(stem, _)| stem == &reference.stem)
                .map(|(_, path)| path.clone())?;
            Some(PushTarget {
                stem: reference.stem.clone(),
                label: reference.label.clone(),
                path,
                tailored: reference.diverged,
                // Untouched copies take the update; tailored ones do not,
                // until the user says so.
                selected: !reference.diverged,
            })
        })
        .collect()
}

impl Shell {
    /// Offer the push after a library block was saved over an existing one.
    pub(super) fn open_push_review(
        &mut self,
        section: SectionKind,
        index: usize,
        identity: String,
        title: String,
        targets: Vec<PushTarget>,
        cx: &mut Context<Self>,
    ) {
        if targets.is_empty() {
            return;
        }
        self.library_push = Some(PushReview {
            section,
            index,
            identity,
            title,
            targets,
        });
        cx.notify();
    }

    pub(super) fn close_push_review(&mut self, cx: &mut Context<Self>) {
        self.library_push = None;
        cx.notify();
    }

    fn toggle_push_target(&mut self, stem: &str, cx: &mut Context<Self>) {
        if let Some(review) = self.library_push.as_mut() {
            if let Some(target) = review.targets.iter_mut().find(|t| t.stem == stem) {
                target.selected = !target.selected;
            }
        }
        cx.notify();
    }

    /// Write the library's version of the block into every ticked document.
    ///
    /// Each document is loaded, rewritten and saved on its own: one unreadable
    /// file must not stop the others, and a partial run is reported as what it
    /// is rather than as a failure of the whole push.
    fn run_push(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.library_push.take() else {
            return;
        };
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let library = vault::load_library(&vault);

        for target in review.targets.iter().filter(|t| t.selected) {
            let mut doc = match vault::load(&target.path) {
                Ok(doc) => doc,
                Err(message) => {
                    save_status::record(cx, "a CV", Err(message));
                    continue;
                }
            };
            let rewritten = super::library_usage::replace_matching(
                &mut doc,
                review.section,
                &review.identity,
                &library,
                review.index,
            );
            if rewritten > 0 {
                save_status::record(cx, "a CV", vault::save(&doc, &target.path));
            }
        }

        cx.notify();
    }

    pub(super) fn render_library_push_sheet(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let review = self.library_push.as_ref()?;
        let theme = *cx.theme();
        let count = review.targets.len();
        let selected = review.selected();
        let tailored = review.targets.iter().filter(|t| t.tailored).count();

        let rows = review.targets.iter().map(|target| {
            let stem = target.stem.clone();
            let checked = target.selected;
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .py(px(6.0))
                .child(
                    Checkbox::new(SharedString::from(format!("push-{}", target.stem)))
                        .checked(checked)
                        .label(target.label.clone())
                        .on_click(cx.listener(move |this, _checked: &bool, _window, cx| {
                            this.toggle_push_target(&stem, cx);
                        })),
                )
                .children(
                    target
                        .tailored
                        .then(|| Tag::secondary().small().child("tailored here")),
                )
        });

        let panel = div()
            .w(px(520.0))
            .flex()
            .flex_col()
            .rounded(theme.radius_md())
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_style(TextStyle::heading())
                            .text_color(theme.text)
                            .child(SharedString::from(format!(
                                "This block is in {count} CV{}",
                                if count == 1 { "" } else { "s" }
                            ))),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::body())
                            .text_color(theme.text_muted)
                            // The state of the world, in one line: the library
                            // is already saved, and nothing else has changed
                            // yet. A dialog that leaves the user guessing which
                            // half happened is worse than no dialog.
                            .child(SharedString::from(format!(
                                "Saved to your library. Each CV keeps its own copy of \u{201c}{}\u{201d} \
                                 — tick the ones that should take the new wording.",
                                review.title
                            ))),
                    ),
            )
            .child(
                div()
                    .id("library-push-targets")
                    .max_h(px(320.0))
                    .overflow_y_scrollbar()
                    .px_4()
                    .py_2()
                    .flex()
                    .flex_col()
                    .children(rows),
            )
            .children((tailored > 0).then(|| {
                div()
                    .px_4()
                    .pb_2()
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_subtle)
                    .child(SharedString::from(format!(
                        "{tailored} of them reworded this block for that CV, so {} left unticked.",
                        if tailored == 1 { "it is" } else { "they are" }
                    )))
            }))
            .child(
                div()
                    .px_4()
                    .pb_4()
                    .pt_2()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        Button::new("library-push-skip")
                            .toolbar()
                            .label("Leave them as they are")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.close_push_review(cx);
                            })),
                    )
                    .child(
                        Button::new("library-push-apply")
                            .toolbar_primary()
                            .icon(IconName::Check)
                            .disabled(selected == 0)
                            .label(match selected {
                                0 => "Update nothing".to_string(),
                                1 => "Update 1 CV".to_string(),
                                n => format!("Update {n} CVs"),
                            })
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.run_push(cx);
                            })),
                    ),
            );

        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.scrim)
                .child(panel)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::{Resume, ResumeDoc, Work};

    fn meta(stem: &str) -> DocMeta {
        DocMeta {
            path: std::path::PathBuf::from(format!("{stem}.toml")),
            stem: stem.into(),
            name: "Albert".into(),
            label: String::new(),
            presets: 0,
            preset_names: Vec::new(),
            unreadable: false,
            modified_secs: None,
            search: Vec::new(),
        }
    }

    fn work(highlight: &str) -> Work {
        Work {
            name: "Acme Corp".into(),
            position: "Senior SWE".into(),
            start_date: "2022-01".into(),
            highlights: vec![highlight.into()],
            ..Default::default()
        }
    }

    fn doc(entry: Work) -> ResumeDoc {
        ResumeDoc::from_resume(
            Resume {
                work: vec![entry],
                ..Default::default()
            },
            "Base",
        )
    }

    /// The safety property of the whole feature: a CV that reworded the block
    /// is offered, but not ticked. Flip this and a push quietly overwrites the
    /// tailoring the product exists to let people do.
    #[test]
    fn a_tailored_copy_starts_unticked() {
        let library = Library {
            work: vec![work("Cut p99 in half")],
            ..Default::default()
        };
        let untouched = doc(work("Cut p99 in half"));
        let tailored = doc(work("Halved p99 latency on the orders service"));
        let (meta_a, meta_b) = (meta("cv-a"), meta("cv-b"));

        let targets = targets_for(
            &library,
            vec![(&meta_a, &untouched), (&meta_b, &tailored)],
            SectionKind::Work,
            0,
        );

        assert_eq!(targets.len(), 2);
        assert!(targets[0].selected && !targets[0].tailored);
        assert!(!targets[1].selected && targets[1].tailored);
    }

    /// A block nobody has placed produces no dialog at all — `open_push_review`
    /// returns early on an empty list, and this is where the list comes from.
    #[test]
    fn an_unplaced_block_has_no_targets() {
        let library = Library {
            work: vec![work("Cut p99 in half")],
            ..Default::default()
        };
        let elsewhere = doc(Work {
            name: "Other Co".into(),
            ..work("x")
        });
        let meta_a = meta("cv-a");

        let targets = targets_for(&library, vec![(&meta_a, &elsewhere)], SectionKind::Work, 0);
        assert!(targets.is_empty());
    }

    /// Every target carries the path it will be written to. A target whose
    /// document the cache cannot place is dropped rather than pushed to a
    /// guessed location.
    #[test]
    fn each_target_carries_the_file_it_will_write() {
        let library = Library {
            work: vec![work("Cut p99 in half")],
            ..Default::default()
        };
        let placed = doc(work("Cut p99 in half"));
        let meta_a = meta("cv-a");

        let targets = targets_for(&library, vec![(&meta_a, &placed)], SectionKind::Work, 0);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, std::path::PathBuf::from("cv-a.toml"));
    }
}
