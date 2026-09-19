use crate::resume::model::*;

fn full_application() -> Application {
    Application {
        company: "Bramble Tech".into(),
        role: "Staff Engineer".into(),
        status_word: ApplicationStatus::Interviewing.word().into(),
        history: vec![
            StageChange {
                at: "2026-06-02".into(),
                to: "applied".into(),
            },
            StageChange {
                at: "2026-06-18".into(),
                to: "interviewing".into(),
            },
        ],
        rounds: vec![InterviewRound {
            at: "2026-06-18".into(),
            label: "Technical screen".into(),
        }],
        closed_as: Some(Closure::Ghosted),
        created: "2026-06-01".into(),
        applied: Some("2026-06-02".into()),
        sent_as: Some(SentCv {
            document: "albert-senior-swe".into(),
            preset: "FAANG · concise".into(),
        }),
        url: "https://brambletech.example/careers/123".into(),
        notes: "Referred by Dana".into(),
        next_step: Some(NextStep {
            label: "Onsite".into(),
            date: "2026-08-20".into(),
            time: "14:00".into(),
        }),
        compensation: "$168k base · negotiating".into(),
        closure_note: None,
        snapshots: vec![Snapshot {
            version: 1,
            date: "2026-06-02".into(),
            preset: "FAANG · concise".into(),
            file: "bramble-tech-v1.pdf".into(),
        }],
    }
}

/// A fully populated application round-trips through TOML unchanged.
#[test]
fn application_round_trips_through_toml() {
    let apps = Applications {
        entries: vec![full_application()],
    };
    let text = toml::to_string_pretty(&apps).expect("serializes");
    let back: Applications = toml::from_str(&text).expect("round-trips");
    assert_eq!(back.entries.len(), 1);
    assert_eq!(back.entries[0], apps.entries[0]);
}

/// A hand-written minimal entry — company/role/status only — loads with
/// everything else defaulted, and a missing `applications.toml` (the
/// common case: no such file exists in any vault written before this
/// feature) is read as an empty board, not an error.
#[test]
fn minimal_entry_loads_with_everything_else_defaulted() {
    let toml_text = "[[entries]]\ncompany = \"Acme\"\nrole = \"SWE\"\nstatus = \"applied\"\n";
    let apps: Applications = toml::from_str(toml_text).expect("a minimal entry must load");
    assert_eq!(apps.entries.len(), 1);
    let entry = &apps.entries[0];
    assert_eq!(entry.company, "Acme");
    assert_eq!(entry.role, "SWE");
    assert_eq!(entry.status(), ApplicationStatus::Applied);
    assert_eq!(entry.created, "");
    assert!(entry.applied.is_none());
    assert!(entry.sent_as.is_none());
    assert!(entry.next_step.is_none());
    assert!(entry.closure_note.is_none());
    assert!(entry.snapshots.is_empty());
}

/// An unrecognised/typo'd status must not fail the whole file — it falls
/// back to `Wishlist`, the board's own default column.
#[test]
fn a_preset_already_in_effect_is_not_applied_again() {
    let resume = Resume {
        basics: Basics {
            name: "Albert Einstein".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut doc = ResumeDoc::from_resume(resume, "Base");
    let base = doc.profile.active_id();
    doc.add_variant(SectionKind::Profile); // "Base copy", now active
    let short = doc.profile.active_id();
    doc.profile.variants[1].name = "Short".into();
    doc.profile.variants[1].data = Basics::default();
    doc.set_active_variant(SectionKind::Profile, 0);
    doc.presets = vec![
        Preset {
            name: "Short profile".into(),
            based_on: None,
            description: None,
            selection: vec![(SectionKind::Profile, short)],
            hidden: vec![],
        },
        Preset {
            name: "Base profile".into(),
            based_on: None,
            description: None,
            selection: vec![(SectionKind::Profile, base)],
            hidden: vec![],
        },
        Preset {
            name: "Names nothing".into(),
            based_on: None,
            description: None,
            selection: vec![],
            hidden: vec![],
        },
    ];

    // The document opens on `Base`, so preset 1 is already in effect and
    // preset 0 is not.
    assert!(!doc.is_preset_active(0));
    assert!(doc.is_preset_active(1));

    doc.apply_preset(0);
    assert!(doc.is_preset_active(0));
    assert!(!doc.is_preset_active(1));

    // A preset that selects nothing agrees with nothing.
    assert!(!doc.is_preset_active(2));
    assert!(!doc.is_preset_active(99));

    // Hiding a section the preset does not hide takes it out of effect.
    doc.hidden_sections = vec![SectionKind::Skills];
    assert!(!doc.is_preset_active(0));
}

/// Active is a fact derived from the working copy, not the last preset a
/// control happened to apply. When two presets say the same thing, the
/// first one in document order owns the mark.
#[test]
fn the_active_preset_is_derived_with_a_document_order_tie_break() {
    let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
    doc.add_preset("First");
    doc.add_preset("Same reading");

    assert_eq!(doc.active_preset_index(), Some(0));

    doc.add_variant(SectionKind::Work);
    assert_eq!(doc.active_preset_index(), None);
    assert_eq!(doc.nearest_preset_index(), Some(0));

    assert!(doc.update_preset(1));
    assert_eq!(doc.active_preset_index(), Some(1));
    assert_eq!(doc.presets[1].name, "Same reading");
}

/// Distance is counted in matrix rows. A section whose variant and
/// visibility both changed is one differing cell, and the closest preset
/// wins before document order is needed as the tie-break.
#[test]
fn the_nearest_preset_counts_differing_section_cells() {
    let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
    doc.add_preset("Base");

    doc.add_variant(SectionKind::Work);
    doc.add_preset("Work tailored");

    doc.add_variant(SectionKind::Skills);
    doc.hidden_sections.push(SectionKind::Skills);

    assert_eq!(doc.preset_distance(0), Some(2));
    assert_eq!(doc.preset_distance(1), Some(1));
    assert_eq!(doc.nearest_preset_index(), Some(1));
    assert_eq!(doc.preset_distance(99), None);
}

#[test]
fn last_sent_is_the_move_into_applied_not_the_card() {
    let sent_as = |stem: &str| {
        Some(SentCv {
            document: stem.into(),
            preset: String::new(),
        })
    };
    let change = |at: &str, to: ApplicationStatus| StageChange {
        at: at.into(),
        to: to.word().into(),
    };

    let apps = Applications {
        entries: vec![
            // Sent twice; the later date wins.
            Application {
                sent_as: sent_as("northwind-em"),
                history: vec![
                    change("2026-03-01", ApplicationStatus::Applied),
                    change("2026-04-02", ApplicationStatus::Interviewing),
                ],
                ..Default::default()
            },
            Application {
                sent_as: sent_as("northwind-em"),
                history: vec![change("2026-07-14", ApplicationStatus::Applied)],
                ..Default::default()
            },
            // A wishlist card with a CV attached: nothing has left the
            // building, so it is not a send.
            Application {
                sent_as: sent_as("research-cv"),
                history: vec![change("2026-08-01", ApplicationStatus::Wishlist)],
                ..Default::default()
            },
        ],
    };

    assert_eq!(apps.last_sent_for("northwind-em"), Some("2026-07-14"));
    assert_eq!(apps.last_sent_for("research-cv"), None);
    assert_eq!(apps.last_sent_for("never-heard-of-it"), None);
}

#[test]
fn unknown_status_falls_back_to_wishlist_rather_than_erroring() {
    let toml_text = "[[entries]]\ncompany = \"Acme\"\nrole = \"SWE\"\nstatus = \"ghosted\"\n";
    let apps: Applications =
        toml::from_str(toml_text).expect("a typo'd status must not fail the whole file");
    assert_eq!(apps.entries[0].status(), ApplicationStatus::Wishlist);
}

/// Empty/absent optional fields must not be written to disk at all —
/// noise in a format whose whole point is being readable and diffable.
#[test]
fn empty_optional_fields_are_not_serialized() {
    let apps = Applications {
        entries: vec![Application {
            company: "Acme".into(),
            role: "SWE".into(),
            status_word: ApplicationStatus::Wishlist.word().into(),
            ..Default::default()
        }],
    };
    let text = toml::to_string_pretty(&apps).expect("serializes");
    for absent in [
        "created",
        "applied",
        "source_doc",
        "preset",
        "url",
        "notes",
        "next_step",
        "compensation",
        "rejection_reason",
        "snapshots",
    ] {
        assert!(
            !text.contains(absent),
            "empty field `{absent}` should not be written:\n{text}"
        );
    }
}

#[test]
fn count_and_active_tally_by_status() {
    let apps = Applications {
        entries: vec![
            Application {
                status_word: ApplicationStatus::Wishlist.word().into(),
                ..Default::default()
            },
            Application {
                status_word: ApplicationStatus::Applied.word().into(),
                ..Default::default()
            },
            Application {
                status_word: ApplicationStatus::Applied.word().into(),
                ..Default::default()
            },
            Application {
                status_word: ApplicationStatus::Closed.word().into(),
                ..Default::default()
            },
        ],
    };
    assert_eq!(apps.count(ApplicationStatus::Applied), 2);
    assert_eq!(apps.count(ApplicationStatus::Closed), 1);
    assert_eq!(apps.active(), 3); // everything but the one Rejected
}

/// A card dragged back to an earlier column has still been through what
/// it has been through, and `furthest()` reads that off the history —
/// which does not un-happen because a board was tidied up.
///
/// This is the whole reason the deepest stage is asked for at all: most
/// interviews end in a rejection, so counting from the current column
/// would erase every interview that did not end in an offer (P-04).
#[test]
fn a_rejection_does_not_erase_the_interview_that_came_before_it() {
    let mut app = Application::default();
    app.advance_to(ApplicationStatus::Applied, "2026-06-01");
    app.advance_to(ApplicationStatus::Interviewing, "2026-06-10");
    assert_eq!(app.furthest(), ApplicationStatus::Interviewing);

    // A rejection is where it *is*, not how deep it got.
    app.advance_to(ApplicationStatus::Closed, "2026-06-20");
    assert_eq!(app.furthest(), ApplicationStatus::Interviewing);

    // And neither does dragging it back by hand: the interview happened
    // whether or not the board still says so.
    app.advance_to(ApplicationStatus::Wishlist, "2026-06-21");
    assert_eq!(app.furthest(), ApplicationStatus::Interviewing);
}

/// A hand-written entry that only says `status = "offer"` still counts as
/// an offer. `furthest` is read from the history and the current column,
/// so a file with no history is not a file with no funnel.
#[test]
fn a_hand_written_entry_still_counts_its_offer() {
    let hand_written = "[[entries]]\ncompany = \"Meridian\"\nrole = \"Senior SWE\"\n\
                        status = \"offer\"\n\n[entries.sent_as]\n\
                        document = \"resume\"\npreset = \"FAANG · concise\"\n";
    let apps: Applications = toml::from_str(hand_written).expect("loads");
    assert_eq!(apps.entries[0].furthest(), ApplicationStatus::Offer);
}

fn tokens<'a>(name: &'a str, role: &'a str, preset: &'a str) -> ExportTokens<'a> {
    ExportTokens {
        name,
        role,
        preset,
        ..Default::default()
    }
}

#[test]
fn export_filename_pattern_resolves_every_token() {
    let stem = ExportSettings::default().resolve_filename(&tokens(
        "Albert Einstein",
        "Principal Systems Architect",
        "Backend",
    ));
    assert_eq!(
        stem,
        "Albert Einstein - Principal Systems Architect - Backend"
    );

    // Every token the pattern advertises has to reach the name. A token the
    // menu offers and the resolver drops is a menu item that does nothing.
    let all = ExportSettings {
        filename_pattern: "{name} - {role} - {preset} - {company} - {variant} - {date}".into(),
    };
    assert_eq!(
        all.resolve_filename(&ExportTokens {
            name: "Albert Einstein",
            role: "Staff SWE",
            preset: "Concise",
            company: "Acme",
            variant: "Short",
            date: "2026-09-01",
        }),
        "Albert Einstein - Staff SWE - Concise - Acme - Short - 2026-09-01"
    );

    // Every pattern the layout rail offers must survive a full token set.
    for (label, pattern) in ExportSettings::PRESETS {
        let settings = ExportSettings {
            filename_pattern: (*pattern).into(),
        };
        let stem = settings.resolve_filename(&ExportTokens {
            name: "Ann Lee",
            role: "SRE",
            preset: "Concise",
            company: "Acme",
            variant: "Short",
            date: "2026-09-01",
        });
        for (token, value) in [
            ("{name}", "Ann Lee"),
            ("{role}", "SRE"),
            ("{preset}", "Concise"),
            ("{company}", "Acme"),
            ("{variant}", "Short"),
            ("{date}", "2026-09-01"),
        ] {
            if pattern.contains(token) {
                assert!(
                    stem.contains(value),
                    "the {label:?} pattern dropped {token}: {stem:?}"
                );
            }
        }
        assert!(
            !stem.contains('{'),
            "the {label:?} pattern left a token in the filename: {stem:?}"
        );
    }
}

#[test]
fn export_filename_drops_missing_tokens_with_their_separators() {
    let settings = ExportSettings {
        filename_pattern: "{name} - {company} - {role} - {preset}".into(),
    };
    // Company and role are empty here — neither may leave a stray dash.
    assert_eq!(
        settings.resolve_filename(&tokens("Albert Einstein", "", "Concise")),
        "Albert Einstein - Concise"
    );
    assert_eq!(
        settings.resolve_filename(&tokens("Albert Einstein", "", "")),
        "Albert Einstein"
    );
}

#[test]
fn a_company_with_a_slash_in_it_does_not_reach_the_filesystem() {
    let settings = ExportSettings {
        filename_pattern: "{name} - {company}".into(),
    };
    assert_eq!(
        settings.resolve_filename(&ExportTokens {
            name: "Albert Einstein",
            company: "Acme Corp / Tech",
            ..Default::default()
        }),
        "Albert Einstein - Acme Corp - Tech"
    );
}

#[test]
fn export_filename_stem_feeds_every_token_the_rail_offers() {
    let resume = Resume {
        basics: Basics {
            name: "Albert Einstein".into(),
            label: "Principal Systems Architect".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut doc = ResumeDoc::from_resume(resume, "Base");

    // The rail offers "Name · Date"; the stem it produces has to carry one.
    doc.export.filename_pattern = "{name} - {date}".into();
    assert_eq!(
        doc.export_filename_stem(None, None, "2026-09-01"),
        "Albert Einstein - 2026-09-01"
    );

    // `{variant}` is the profile's active variant, which is what names the
    // document in the editor.
    doc.export.filename_pattern = "{name} - {variant}".into();
    assert_eq!(
        doc.export_filename_stem(None, None, "2026-09-01"),
        "Albert Einstein - Base"
    );

    // `{company}` resolves only from an application card, and drops its
    // separator everywhere else.
    doc.export.filename_pattern = "{name} - {company} - {preset}".into();
    assert_eq!(
        doc.export_filename_stem(Some("Concise"), None, "2026-09-01"),
        "Albert Einstein - Concise"
    );
    assert_eq!(
        doc.export_filename_stem(Some("Concise"), Some("Acme"), "2026-09-01"),
        "Albert Einstein - Acme - Concise"
    );
}

#[test]
fn record_export_tracks_entries_and_serializes_to_toml() {
    let mut doc = ResumeDoc::default();
    assert!(doc.export_history.is_empty());

    let concise = std::path::PathBuf::from("/tmp/Albert Einstein - Concise.pdf");
    doc.record_export("2026-09-02", "17:30", "PDF", "Concise", concise.clone());
    assert_eq!(doc.export_history.len(), 1);
    assert_eq!(doc.export_history[0].format, "PDF");
    assert_eq!(doc.export_history[0].preset, "Concise");
    assert_eq!(doc.export_history[0].date, "2026-09-02");
    assert_eq!(doc.export_history[0].time, "17:30");

    let serialized = toml::to_string(&doc).expect("serializes");
    assert!(serialized.contains("export_history"));
    assert!(serialized.contains("Concise"));

    let deserialized: ResumeDoc = toml::from_str(&serialized).expect("round-trips");
    assert_eq!(deserialized.export_history, doc.export_history);
}

/// Exporting twice to one path is one file, not two facts: the second write
/// replaced the first on disk, and listing both describes a file that is no
/// longer there beside one that is.
#[test]
fn re_exporting_to_the_same_path_updates_the_row_rather_than_adding_one() {
    let mut doc = ResumeDoc::default();
    let path = std::path::PathBuf::from("/tmp/Albert Einstein - Concise.pdf");

    doc.record_export("2026-09-01", "09:00", "PDF", "Concise", path.clone());
    doc.record_export("2026-09-02", "17:30", "PDF", "Extended", path.clone());

    assert_eq!(doc.export_history.len(), 1);
    assert_eq!(doc.export_history[0].date, "2026-09-02");
    assert_eq!(doc.export_history[0].preset, "Extended");

    // A different path is a different file and keeps its own row.
    doc.record_export(
        "2026-09-02",
        "17:31",
        "Word",
        "Concise",
        std::path::PathBuf::from("/tmp/Albert Einstein - Concise.docx"),
    );
    assert_eq!(doc.export_history.len(), 2);
}

/// A document's TOML is the product, so the history has a ceiling — and the
/// rows it drops are the oldest, which are the ones whose paths are least
/// likely to still exist.
#[test]
fn export_history_stops_growing_and_drops_the_oldest_first() {
    let mut doc = ResumeDoc::default();
    for i in 0..MAX_EXPORT_HISTORY + 10 {
        doc.record_export(
            "2026-09-02",
            "17:30",
            "PDF",
            "Concise",
            std::path::PathBuf::from(format!("/tmp/cv-{i}.pdf")),
        );
    }

    assert_eq!(doc.export_history.len(), MAX_EXPORT_HISTORY);
    assert_eq!(
        doc.export_history[0].path,
        std::path::PathBuf::from("/tmp/cv-10.pdf")
    );
    assert_eq!(
        doc.export_history[MAX_EXPORT_HISTORY - 1].path,
        std::path::PathBuf::from(format!("/tmp/cv-{}.pdf", MAX_EXPORT_HISTORY + 9))
    );
}
