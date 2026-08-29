//! Creating and editing a library block.
//!
//! `New block` used to push a placeholder — a Work entry titled *"New role"*,
//! a skill group called *"New category"* — and there was no way to fill it in.
//! You got a card that said "New role" and stayed that way. Nor could an
//! existing block be corrected: a typo in an employer's name was permanent
//! unless you deleted the block and re-starred it from a CV. So the library
//! could be added to and it could be read, and the middle verb was missing.
//!
//! ## Why blocks stay typed
//!
//! The obvious reading of "New block ▸ Work Experience / Education / Skills /
//! …" is that the type is ceremony, and a single free-form block would be
//! kinder. It would not: the type is the **only** reason a block can be
//! dropped into a CV at all. `copy_block_into_document` pushes into
//! `doc.work`, `doc.skills`, `doc.certificates` — a Work entry is an employer,
//! a role, two dates and bullets; a skill group is a name and a list of words;
//! a certificate is a name, an issuer and a link. A free-form block would have
//! to be *guessed* into one of those on the way in — which of your lines is
//! the employer? — and the guess would be wrong exactly when the CV matters.
//!
//! Untyped blocks would also break the thing the Library just gained: `used in
//! N CVs` matches on a section's identifying fields, and there are no
//! identifying fields in a bag of text.
//!
//! So the type stays, and what changes is that choosing one now opens a **form
//! for it** instead of writing a placeholder. Nothing reaches the vault until
//! the form is saved, which is also why "New block" no longer leaves debris
//! behind when you change your mind.
//!
//! ## One form, five shapes
//!
//! Each section declares its fields as data ([`fields_for`]) and the sheet
//! renders whatever it is handed. The alternative — five hand-written forms —
//! is five places to forget a field the day the model gains one.

use crate::resume::model::{
    Certificate, Education, Library, SectionKind, SkillGroup, Volunteer, Work,
};

/// How one field is entered.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldKind {
    /// A single line.
    Line,
    /// A list, one item per line. Bullets and skill keywords are both this —
    /// the alternative is comma separation, and a bullet containing a comma is
    /// the common case, not the edge one.
    Lines,
}

/// One field of a block's form.
pub(super) struct FieldSpec {
    pub label: &'static str,
    pub placeholder: &'static str,
    pub kind: FieldKind,
}

const fn line(label: &'static str, placeholder: &'static str) -> FieldSpec {
    FieldSpec {
        label,
        placeholder,
        kind: FieldKind::Line,
    }
}

const fn lines(label: &'static str, placeholder: &'static str) -> FieldSpec {
    FieldSpec {
        label,
        placeholder,
        kind: FieldKind::Lines,
    }
}

/// An empty "Ended" means "still there", exactly as `date_range` and the Typst
/// renderer already read it — so the placeholder says so rather than leaving
/// the author to wonder whether blank means unknown.
const ENDED: &str = "leave blank for Present";

const WORK: [FieldSpec; 6] = [
    line("Role", "Senior Software Engineer"),
    line("Employer", "Acme Corp"),
    line("Started", "2022-01"),
    line("Ended", ENDED),
    line("Summary", "One line about the job as a whole"),
    lines("Bullets", "One achievement per line"),
];

const EDUCATION: [FieldSpec; 5] = [
    line("Qualification", "BSc Computer Science"),
    line("Institution", "Trinity College Dublin"),
    line("Started", "2013-09"),
    line("Ended", ENDED),
    lines("Notes", "One per line"),
];

const SKILLS: [FieldSpec; 2] = [
    line("Category", "Infrastructure — leave blank for an ungrouped list"),
    lines("Skills", "One per line: Kafka, AWS, Kubernetes…"),
];

const CERTIFICATES: [FieldSpec; 4] = [
    line("Name", "AWS Solutions Architect"),
    line("Issuer", "Amazon Web Services"),
    line("Date", "2024-03"),
    line("Link", "https://…"),
];

const ORGANIZATIONS: [FieldSpec; 5] = [
    line("Role", "Mentor"),
    line("Organization", "CoderDojo"),
    line("Started", "2019-01"),
    line("Ended", ENDED),
    lines("Bullets", "One per line"),
];

/// The form for a section, or nothing for a section with no pool.
pub(super) fn fields_for(section: SectionKind) -> &'static [FieldSpec] {
    match section {
        SectionKind::Work => &WORK,
        SectionKind::Education => &EDUCATION,
        SectionKind::Skills => &SKILLS,
        SectionKind::Certificates => &CERTIFICATES,
        SectionKind::Organizations => &ORGANIZATIONS,
        // No pool — see `root.rs::save_block_to_library`.
        SectionKind::Profile | SectionKind::Custom(_) => &[],
    }
}

/// What the section's blocks are called in the singular, for the sheet's title.
pub(super) fn block_noun(section: SectionKind) -> &'static str {
    match section {
        SectionKind::Work => "work entry",
        SectionKind::Education => "qualification",
        SectionKind::Skills => "skill group",
        SectionKind::Certificates => "certificate",
        SectionKind::Organizations => "organization",
        SectionKind::Profile | SectionKind::Custom(_) => "block",
    }
}

fn joined(items: &[String]) -> String {
    items.join("\n")
}

fn split(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// The current values of an existing block, in [`fields_for`] order — what the
/// form is seeded with when editing. Empty when the block does not exist,
/// which is also the "creating a new one" case.
pub(super) fn values_of(
    library: &Library,
    section: SectionKind,
    index: Option<usize>,
) -> Vec<String> {
    let empty = || vec![String::new(); fields_for(section).len()];
    let Some(index) = index else {
        return empty();
    };

    match section {
        SectionKind::Work => library.work.get(index).map(|w| {
            vec![
                w.position.clone(),
                w.name.clone(),
                w.start_date.text.clone(),
                w.end_date.text.clone(),
                w.summary.clone(),
                joined(&w.highlights),
            ]
        }),
        SectionKind::Education => library.education.get(index).map(|e| {
            vec![
                e.study_type.clone(),
                e.institution.clone(),
                e.start_date.text.clone(),
                e.end_date.text.clone(),
                joined(&e.highlights),
            ]
        }),
        SectionKind::Skills => library
            .skills
            .get(index)
            .map(|s| vec![s.name.clone(), joined(&s.keywords)]),
        SectionKind::Certificates => library.certificates.get(index).map(|c| {
            vec![
                c.name.clone(),
                c.issuer.clone(),
                c.date.text.clone(),
                c.url.clone(),
            ]
        }),
        SectionKind::Organizations => library.volunteer.get(index).map(|v| {
            vec![
                v.position.clone(),
                v.organization.clone(),
                v.start_date.text.clone(),
                v.end_date.text.clone(),
                joined(&v.highlights),
            ]
        }),
        SectionKind::Profile | SectionKind::Custom(_) => None,
    }
    .unwrap_or_else(empty)
}

/// Read `values` back into the pool: replacing the block at `index`, or
/// appending when `index` is `None`.
///
/// Returns `false` when there is nothing worth storing — every field blank —
/// so the caller can decline to write rather than adding an empty card, which
/// is the bug this whole module exists to fix.
/// The identity of the block at `index`, as [`super::library_usage`] computes
/// it — the handle that still finds the copies after the block itself changes.
fn library_link_identity(library: &Library, section: SectionKind, index: usize) -> Option<String> {
    super::library_usage::block_identity(library, section, index)
}

/// Every field of a block, as one comparable string. Used only to ask whether
/// a save changed anything; see the note on fingerprints in `library_usage`.
fn block_fingerprint(library: &Library, section: SectionKind, index: usize) -> Option<String> {
    match section {
        SectionKind::Work => library.work.get(index).map(|b| format!("{b:?}")),
        SectionKind::Education => library.education.get(index).map(|b| format!("{b:?}")),
        SectionKind::Skills => library.skills.get(index).map(|b| format!("{b:?}")),
        SectionKind::Certificates => library.certificates.get(index).map(|b| format!("{b:?}")),
        SectionKind::Organizations => library.volunteer.get(index).map(|b| format!("{b:?}")),
        SectionKind::Profile | SectionKind::Custom(_) => None,
    }
}

/// What to call the block in the push dialog: the user's own words for it,
/// never the section's generic noun when there is something better.
fn block_title(library: &Library, section: SectionKind, index: usize) -> Option<String> {
    let title = match section {
        SectionKind::Work => library.work.get(index).map(|b| b.position.clone()),
        SectionKind::Education => library.education.get(index).map(|b| b.study_type.clone()),
        SectionKind::Skills => library.skills.get(index).map(|b| b.name.clone()),
        SectionKind::Certificates => library.certificates.get(index).map(|b| b.name.clone()),
        SectionKind::Organizations => library.volunteer.get(index).map(|b| b.position.clone()),
        SectionKind::Profile | SectionKind::Custom(_) => None,
    }?;
    (!title.trim().is_empty()).then_some(title)
}

pub(super) fn apply(
    library: &mut Library,
    section: SectionKind,
    index: Option<usize>,
    values: &[String],
) -> bool {
    let at = |i: usize| values.get(i).map(|v| v.trim().to_string()).unwrap_or_default();
    if values.iter().all(|v| v.trim().is_empty()) {
        return false;
    }

    /// Put `built` where `index` says, or on the end.
    fn place<T>(pool: &mut Vec<T>, index: Option<usize>, built: T) {
        match index {
            Some(i) if i < pool.len() => pool[i] = built,
            _ => pool.push(built),
        }
    }

    match section {
        SectionKind::Work => place(
            &mut library.work,
            index,
            Work {
                position: at(0),
                name: at(1),
                start_date: at(2).into(),
                end_date: at(3).into(),
                summary: at(4),
                highlights: split(&at(5)),
                ..Default::default()
            },
        ),
        SectionKind::Education => place(
            &mut library.education,
            index,
            Education {
                study_type: at(0),
                institution: at(1),
                start_date: at(2).into(),
                end_date: at(3).into(),
                highlights: split(&at(4)),
                ..Default::default()
            },
        ),
        SectionKind::Skills => place(
            &mut library.skills,
            index,
            SkillGroup {
                name: at(0),
                keywords: split(&at(1)),
            },
        ),
        SectionKind::Certificates => place(
            &mut library.certificates,
            index,
            Certificate {
                name: at(0),
                issuer: at(1),
                date: at(2).into(),
                url: at(3),
            },
        ),
        SectionKind::Organizations => place(
            &mut library.volunteer,
            index,
            Volunteer {
                position: at(0),
                organization: at(1),
                start_date: at(2).into(),
                end_date: at(3).into(),
                highlights: split(&at(4)),
            },
        ),
        SectionKind::Profile | SectionKind::Custom(_) => return false,
    }
    true
}

// --- the sheet -----------------------------------------------------------

use gpui::prelude::*;
use gpui::{
    div, px, AnyElement, ClickEvent, Context, Entity, IntoElement, SharedString, Subscription,
    Window,
};

use dockcv_ui_components::{ScrollableElement, 
    Button, ButtonExt, Disableable, TextField, TextFieldEvent, TextFieldState,
};

use crate::theme::{ActiveTheme, StyledText, TextStyle};
use crate::vault;

use super::save_status;
use super::shell::Shell;

/// The open block form.
pub(super) struct LibraryEdit {
    pub section: SectionKind,
    /// `None` while creating. The pool position when editing — which is what
    /// makes an edit replace rather than duplicate.
    pub index: Option<usize>,
    /// One box per [`fields_for`] entry, in the same order.
    pub values: Vec<Entity<TextFieldState>>,
    /// Kept so a keystroke re-renders the sheet — that is what keeps "Save
    /// block" from staying greyed out while you type into it. Dropped with the
    /// form, which is why the fields can be built per-opening.
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    /// Open the form: for a new block when `index` is `None`, otherwise seeded
    /// from the block at that position.
    pub(super) fn open_library_edit(
        &mut self,
        section: SectionKind,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let specs = fields_for(section);
        if specs.is_empty() {
            return;
        }
        let seeded = values_of(self.cache.library(), section, index);

        let mut values: Vec<Entity<TextFieldState>> = Vec::with_capacity(specs.len());
        let mut subscriptions: Vec<Subscription> = Vec::with_capacity(specs.len());
        for (spec, value) in specs.iter().zip(seeded) {
            let field = match spec.kind {
                FieldKind::Line => cx.new(|cx| TextFieldState::single_line(window, cx)),
                // Grows with the list rather than scrolling a two-line box:
                // bullets are the field people spend the most time in.
                FieldKind::Lines => cx.new(|cx| TextFieldState::auto_grow(3, 10, window, cx)),
            };
            if !value.is_empty() {
                field.update(cx, |state, cx| state.seed(&value, window, cx));
            }
            subscriptions.push(cx.subscribe(
                &field,
                |this, _field, event: &TextFieldEvent, cx| match event {
                    // Enter in any single-line field saves, the way it does in
                    // every other one-screen form here.
                    TextFieldEvent::Submitted => this.commit_library_edit(cx),
                    TextFieldEvent::Changed => cx.notify(),
                    _ => {}
                },
            ));
            values.push(field);
        }

        // Open with the caret in the first field: this sheet exists to be
        // typed into, and every one of its fields is empty on a new block.
        if let Some(first) = values.first() {
            let handle = first.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        }

        self.library_edit = Some(LibraryEdit {
            section,
            index,
            values,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    pub(super) fn close_library_edit(&mut self, cx: &mut Context<Self>) {
        self.library_edit = None;
        cx.notify();
    }

    fn commit_library_edit(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.library_edit.as_ref() else {
            return;
        };
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let values: Vec<String> = form
            .values
            .iter()
            .map(|f| f.read(cx).value(cx).to_string())
            .collect();
        let (section, index) = (form.section, form.index);

        let mut library = vault::load_library(&vault);

        // Everything the push offer needs is read here, *before* the edit
        // lands: the identity the copies out there still carry, and which of
        // those copies had already been reworded. Both are questions about the
        // block as it was, and neither can be asked once it has been
        // overwritten (US-03).
        let radius = index.and_then(|index| {
            let identity = library_link_identity(&library, section, index)?;
            let before = block_fingerprint(&library, section, index)?;
            let targets = super::library_link::targets_for(
                &library,
                self.cache.readable_documents(),
                section,
                index,
            );
            (!targets.is_empty()).then_some((index, identity, before, targets))
        });

        // An all-blank form writes nothing — the sheet keeps Save disabled for
        // that case, and this is the guard behind the guard.
        if apply(&mut library, section, index, &values) {
            save_status::record(cx, "library", vault::save_library(&vault, &library));
        }
        self.library_edit = None;

        // Only offer the push when the save actually changed the block. An
        // edit sheet opened and closed unchanged — or one that only corrected
        // whitespace — has nothing to propagate, and a dialog about it would
        // be noise on a no-op.
        if let Some((index, identity, before, targets)) = radius {
            let changed = block_fingerprint(&library, section, index)
                .is_some_and(|after| after != before);
            if changed {
                let title = block_title(&library, section, index)
                    .unwrap_or_else(|| block_noun(section).to_string());
                self.open_push_review(section, index, identity, title, targets, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn render_library_edit_sheet(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let form = self.library_edit.as_ref()?;
        let theme = *cx.theme();
        let specs = fields_for(form.section);
        // An empty form has nothing to save, and saying so on the button beats
        // accepting the click and writing nothing.
        let blank = form
            .values
            .iter()
            .all(|f| f.read(cx).value(cx).trim().is_empty());

        let (title, subtitle) = match form.index {
            None => (
                format!("New {}", block_noun(form.section)),
                "Saved to your library, not to a CV. Reuse drops it into one.",
            ),
            // The question an edit raises, answered before it is asked — and
            // the same answer the delete dialog gives, for the same reason.
            // Half of this used to be the whole answer to P-02. It still is
            // the default — a block is a source to copy from — but saving now
            // *offers* to carry the change into the CVs that hold a copy, and
            // a subtitle that denied that would be describing the old app.
            Some(_) => (
                format!("Edit {}", block_noun(form.section)),
                "CVs you already built keep their copy. If any of them hold \
                 this block, saving asks whether they should take the change.",
            ),
        };

        let rows = specs.iter().zip(&form.values).map(|(spec, state)| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_style(TextStyle::label())
                        .text_color(theme.text_muted)
                        .child(spec.label),
                )
                .child(TextField::new(state).placeholder(spec.placeholder))
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
                            .child(SharedString::from(title)),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::body())
                            .text_color(theme.text_muted)
                            .child(subtitle),
                    ),
            )
            .child(
                div()
                    .id("library-edit-fields")
                    .max_h(px(420.0))
                    .overflow_y_scrollbar()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .children(rows),
            )
            .child(
                div()
                    .px_4()
                    .pb_4()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        Button::new("library-edit-cancel")
                            .toolbar()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.close_library_edit(cx);
                            })),
                    )
                    .child(
                        Button::new("library-edit-save")
                            .toolbar_primary()
                            .disabled(blank)
                            .label("Save block")
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.commit_library_edit(cx);
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

    /// The property the form rests on: what you typed is what comes back when
    /// you reopen it. Every section, because each maps its own field order and
    /// an off-by-one there would put the employer in the role.
    #[test]
    fn every_section_round_trips_its_form() {
        for section in [
            SectionKind::Work,
            SectionKind::Education,
            SectionKind::Skills,
            SectionKind::Certificates,
            SectionKind::Organizations,
        ] {
            let specs = fields_for(section);
            // Distinct per field, so a swapped pair cannot pass.
            let typed: Vec<String> = specs
                .iter()
                .enumerate()
                .map(|(i, spec)| match spec.kind {
                    FieldKind::Line => format!("value {i}"),
                    FieldKind::Lines => format!("first {i}\nsecond {i}"),
                })
                .collect();

            let mut library = Library::default();
            assert!(apply(&mut library, section, None, &typed));
            let back = values_of(&library, section, Some(0));
            assert_eq!(back, typed, "{section:?} did not round-trip");
        }
    }

    /// Editing replaces in place rather than appending — the bug that would
    /// turn every correction into a duplicate.
    #[test]
    fn editing_replaces_and_does_not_append() {
        let mut library = Library::default();
        apply(&mut library, SectionKind::Skills, None, &["A".into(), "x".into()]);
        apply(&mut library, SectionKind::Skills, None, &["B".into(), "y".into()]);
        assert_eq!(library.skills.len(), 2);

        apply(
            &mut library,
            SectionKind::Skills,
            Some(0),
            &["A renamed".into(), "x\nz".into()],
        );
        assert_eq!(library.skills.len(), 2, "editing must not add a block");
        assert_eq!(library.skills[0].name, "A renamed");
        assert_eq!(library.skills[0].keywords, vec!["x", "z"]);
        assert_eq!(library.skills[1].name, "B", "the other block is untouched");
    }

    /// An empty form writes nothing. This is the whole complaint: "New block"
    /// used to add a card reading "New role" that could never be filled in.
    #[test]
    fn an_empty_form_creates_nothing() {
        let mut library = Library::default();
        let blank = vec![String::new(); fields_for(SectionKind::Work).len()];
        assert!(!apply(&mut library, SectionKind::Work, None, &blank));
        assert!(library.work.is_empty());

        // Whitespace is empty too.
        let spaces = vec!["   ".into(), "\n".into(), String::new(), String::new(), String::new(), String::new()];
        assert!(!apply(&mut library, SectionKind::Work, None, &spaces));
        assert!(library.work.is_empty());
    }

    /// A list field is one item per line, and blank lines are not items — a
    /// trailing newline is what every text box leaves behind.
    #[test]
    fn list_fields_are_one_item_per_line() {
        let mut library = Library::default();
        apply(
            &mut library,
            SectionKind::Skills,
            None,
            &["Infra".into(), "Kafka\n\n  AWS  \nKubernetes\n".into()],
        );
        assert_eq!(library.skills[0].keywords, vec!["Kafka", "AWS", "Kubernetes"]);
    }

    /// A comma belongs to the bullet, not to the parser — which is why list
    /// fields split on lines rather than commas.
    #[test]
    fn a_bullet_may_contain_a_comma() {
        let mut library = Library::default();
        apply(
            &mut library,
            SectionKind::Work,
            None,
            &[
                "Engineer".into(),
                "Acme".into(),
                String::new(),
                String::new(),
                String::new(),
                "Cut p99 in half, and kept it there".into(),
            ],
        );
        assert_eq!(
            library.work[0].highlights,
            vec!["Cut p99 in half, and kept it there"]
        );
    }
}
