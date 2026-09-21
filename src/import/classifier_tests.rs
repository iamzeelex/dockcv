//! What the classifier decides, and why.
//!
//! Split from `classifier.rs` by C15 — the file was 2569 lines and a
//! quarter of it was this. Declared by the parent with `#[path]`, the same
//! shape `applications_tests.rs` uses.

use super::*;
// The classifier is four files now; the tests are about all of it.
use crate::import::classifier_headings::*;

/// A phone number with a country code and no separator inside the local
/// part — an ordinary Berlin number — used to fall through: the pattern
/// demanded a separator before the final group, which is what tells
/// `415-555-0134` from `2019 - 2021`. With an explicit `+` there is no such
/// ambiguity, so that shape gets its own alternative.
#[test]
fn a_country_code_makes_the_last_separator_optional() {
    let phone = get_phone_regex();
    for number in [
        "+49 30 123456",
        "+1 415 555 0134",
        "+44 20 7946 0958",
        "+380 44 123 4567",
        "020 7946 0958",
        "415-555-0134",
        "(415) 555-0134",
    ] {
        assert!(phone.is_match(number), "should read as a phone: {number}");
    }
}

/// …and the guard that alternative could have broken: a date range is not a
/// phone number, whichever way it is written.
#[test]
fn a_date_range_is_still_not_a_phone_number() {
    let phone = get_phone_regex();
    for dates in [
        "2019 - 2021",
        "2019 – 2021",
        "Jan 2021 - Mar 2023",
        "1999-2003",
        "2014 2018",
        "2021.01 - 2024.06",
    ] {
        assert!(!phone.is_match(dates), "read as a phone number: {dates}");
    }
}

/// I-09, measured. These three regexes used to run over the whole document
/// and take the first hit each, so a vendor named in a bullet became the
/// person's own website and a number in a bullet became their phone.
#[test]
fn a_url_in_a_bullet_is_not_the_persons_own_website() {
    let raw = "Albert Einstein\n\
               Staff Engineer\n\
               s@example.com\n\
               \n\
               EXPERIENCE\n\
               Staff Engineer, Acme  Jan 2021 – Present\n\
               • Migrated billing to https://stripe.com and cut p99 40%\n\
               • Reached out on +1 415 555 0134 for the vendor escalation\n";
    let basics = classify_raw_text("PDF", raw).doc.profile.active().clone();

    assert_eq!(
        basics.email, "s@example.com",
        "the contact block still reads"
    );
    assert_eq!(
        basics.url, "",
        "a vendor's site is not the user's: {:?}",
        basics.url
    );
    assert_eq!(
        basics.phone, "",
        "someone else's number: {:?}",
        basics.phone
    );
}

/// The exception the asymmetry buys. A two-column PDF interleaves its
/// sidebar with its body, so the contact block is not above the first
/// heading and a region-only reading would drop the address. An email is a
/// strong enough claim of identity to take from anywhere; a URL is not.
#[test]
fn an_email_below_the_first_heading_is_still_the_persons_own() {
    let raw = "Albert Einstein\n\
               EXPERIENCE\n\
               s@example.com\n\
               Staff Engineer, Acme  Jan 2021 – Present\n\
               • Shipped it on https://vendor.example\n";
    let basics = classify_raw_text("PDF", raw).doc.profile.active().clone();

    assert_eq!(basics.email, "s@example.com");
    assert_eq!(
        basics.url, "",
        "a vendor's site is still not the user's: {:?}",
        basics.url
    );
}

/// …and the block itself is still read, which is the half that must not
/// regress: restricting the region would be worthless if it also lost the
/// details it was protecting.
#[test]
fn the_contact_block_still_gives_up_all_three() {
    let raw = "Albert Einstein\n\
               s@example.com · +49 30 555 0134 · https://einstein.example\n\
               \n\
               EXPERIENCE\n\
               Staff Engineer, Acme  Jan 2021 – Present\n";
    let basics = classify_raw_text("PDF", raw).doc.profile.active().clone();

    assert_eq!(basics.email, "s@example.com");
    assert_eq!(basics.phone, "+49 30 555 0134");
    assert_eq!(basics.url, "https://einstein.example");
}

/// A template that gives contact details their own heading puts them below
/// the first one, so the region has to follow the heading rather than stop
/// at it.
#[test]
fn an_explicit_contact_section_is_part_of_the_region() {
    let raw = "Albert Einstein\n\
               \n\
               EXPERIENCE\n\
               Staff Engineer, Acme  Jan 2021 – Present\n\
               • Shipped it on https://vendor.example\n\
               \n\
               CONTACT\n\
               https://einstein.example\n";
    let basics = classify_raw_text("PDF", raw).doc.profile.active().clone();
    assert_eq!(basics.url, "https://einstein.example");
}

/// A page break inside a paragraph glues the running header onto the line
/// above it, so the bullet arrived as
/// `…atmospheric profiles.Marie Curiealbert@example.com`. The same name and email
/// stand alone in the contact block and must survive untouched.
#[test]
fn a_running_page_header_is_stripped_out_of_the_bullet_it_bled_into() {
    let raw = "Marie Curie  Systems & Data Engineer\n\
               albert@example.com\n\
               \n\
               EXPERIENCE\n\
               Software Developer\n\
               • Built a pipeline for atmospheric profiles.Marie Curiealbert@example.com\n\
               • Shipped the ingest service.\n";
    let imported = classify_raw_text("PDF", raw);
    let basics = imported.doc.profile.active();

    assert_eq!(basics.name, "Marie Curie");
    assert_eq!(basics.email, "albert@example.com");

    let work = imported.doc.work.active();
    let bullets: Vec<&str> = work
        .iter()
        .flat_map(|w| w.highlights.iter().map(|h| h.as_str()))
        .collect();
    assert!(
        bullets.contains(&"Built a pipeline for atmospheric profiles."),
        "header still glued to the bullet: {bullets:?}"
    );
}

/// Two degrees, written the two ways an exporter writes them: the degree
/// above the university, and both on one line. Reading each line as its own
/// entry imported this as a single, half-empty education entry.
#[test]
fn both_shapes_of_an_education_entry_are_read() {
    let raw = "EDUCATION\n\n\
               Graduate coursework — MSc in Modeling for Science and Engineering\n\
               Universitat Autònoma de Barcelona (UAB) 2025 – 2026  |  Barcelona, Spain\n\
               \n\
               Mathematical modeling, dynamical systems and complexity, HPC\n\
               \n\
               BSc, Applied Mathematics and Computing, Odesa I.I.Mechnikov National University 2019 – 2023\n\
               Numerical methods, optimization and control theory, machine learning\n\
               Completed while working full-time\n";
    let edu = classify_raw_text("PDF", raw)
        .doc
        .education
        .active()
        .to_vec();

    assert_eq!(edu.len(), 2, "{edu:#?}");
    assert!(edu[0].study_type.starts_with("Graduate coursework"));
    assert_eq!(
        edu[0].institution,
        "Universitat Autònoma de Barcelona (UAB)"
    );
    assert_eq!(edu[0].start_date.text, "2025");
    assert_eq!(
        edu[1].institution,
        "Odesa I.I.Mechnikov National University"
    );
    // The coursework line and the note under it are kept, not dropped.
    assert_eq!(edu[1].highlights.len(), 2, "{:#?}", edu[1]);
}

/// A family the taxonomy knows but the model has no shape for keeps its own
/// name. `PROJECTS` used to be a synonym for *work*, so three projects were
/// appended to somebody's last employer.
#[test]
fn a_recognised_family_with_no_built_in_shape_keeps_its_own_name() {
    let raw = "WORK EXPERIENCE\n\n\
               Software Developer, GE Vernova Aug 2024 – Dec 2025  |  Barcelona, Spain\n\
               \n\
               •Built the observability stack.\n\
               \n\
               PROJECTS\n\
               \n\
               pymolt, Python Migration Tool 2026\n\
               \n\
               •Runtime tracing tool built on sys.setprofile.\n";
    let doc = classify_raw_text("PDF", raw).doc;

    assert_eq!(doc.work.active().len(), 1);
    assert_eq!(
        doc.work.active()[0].highlights.len(),
        1,
        "projects leaked into the job"
    );
    assert_eq!(doc.custom_sections.len(), 1, "{:#?}", doc.custom_sections);
    let projects = &doc.custom_sections[0];
    assert_eq!(projects.title, "Projects");
    // The author's line is kept whole — see the branch's comment: the same
    // punctuation carries a name-and-description in one CV and a single job
    // title in the next, and splitting cut the second in half.
    assert_eq!(
        projects.content.active()[0].title,
        "pymolt, Python Migration Tool"
    );
    assert_eq!(projects.content.active()[0].start_date.text, "2026");
}

/// Interests and Languages are headings a CV really uses and the model has
/// no field for. Before the taxonomy carried them they were not headings at
/// all, so their content was absorbed by whatever section was open — in a
/// real template, into Education.
#[test]
fn headings_the_model_has_no_field_for_are_still_sections() {
    for heading in [
        "Interests",
        "Languages",
        "Publications",
        "Awards",
        "References",
    ] {
        assert!(is_section_header(heading), "{heading} should be a heading");
        assert_eq!(
            classify_header(heading),
            SectionKind::Unknown,
            "{heading} has no built-in shape and must not be forced into one"
        );
    }
}

/// Two headings of the same built-in kind: the taxonomy is stretching, and
/// the second is something else under a name it happens to share.
#[test]
fn a_second_section_of_the_same_kind_becomes_its_own_section() {
    let raw = "WORK EXPERIENCE\n\n\
               Software Developer, GE Vernova Aug 2024 – Dec 2025\n\
               \n\
               •Built the observability stack.\n\
               \n\
               RELEVANT EXPERIENCE\n\
               \n\
               Volunteer Mentor, Code Club Jan 2020 – Dec 2021\n";
    let doc = classify_raw_text("PDF", raw).doc;

    assert_eq!(doc.work.active().len(), 1, "{:#?}", doc.work.active());
    assert_eq!(doc.custom_sections.len(), 1, "{:#?}", doc.custom_sections);
    assert_eq!(doc.custom_sections[0].title, "Relevant Experience");
}

/// `Activities and Interests` contains *activities*, which the volunteer
/// corpus lists — so a heading whose own name is in the taxonomy verbatim
/// was rendering as ORGANIZATIONS.
#[test]
fn a_heading_that_is_a_name_outranks_one_that_merely_contains_a_name() {
    assert_eq!(
        classify_header("Activities and Interests"),
        SectionKind::Unknown
    );
    assert_eq!(classify_header("Activities"), SectionKind::Volunteer);
    // The substring path still reads a heading that carries extra words.
    assert_eq!(classify_header("Skills & Abilities"), SectionKind::Skills);
    assert_eq!(
        classify_header("WORK EXPERIENCE 2019–2024"),
        SectionKind::Work
    );
}

/// A name in two families is a tie the ranking cannot break, and the
/// winner then depends on map order — which is how `Activities` resolved
/// differently from `Activities and Interests` for no stated reason.
#[test]
fn no_name_belongs_to_two_families() {
    let mut seen: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
    for (family, corpus) in get_taxonomy() {
        for keyword in corpus.all_keywords() {
            let keyword = keyword.to_lowercase();
            if let Some(other) = seen.insert(keyword.clone(), family) {
                assert_eq!(other, family, "`{keyword}` is in both {other} and {family}");
            }
        }
    }
}

/// A title the author already cased is kept; only a shouted heading is
/// re-cased, or `Activities and Interests` comes back as `... And ...`.
#[test]
fn only_a_shouted_heading_is_recased() {
    assert_eq!(title_case("PROJECTS"), "Projects");
    assert_eq!(
        title_case("Activities and Interests"),
        "Activities and Interests"
    );
}

/// Templates print the school above the degree and the degree above the
/// school with equal enthusiasm, so position cannot say which is which. The
/// words can: one CV filed `Petroleum Engineering` as the university and
/// pushed `University of Calgary` into the bullet list under it.
#[test]
fn a_degree_and_a_school_are_told_apart_by_their_words_not_their_order() {
    let raw = "EDUCATION\n\n\
               Master of Engineering, Petroleum Engineering 2009 – 2011\n\
               University of Calgary, Alberta\n\
               GPA: 3.72/4.00\n";
    let edu = classify_raw_text("PDF", raw)
        .doc
        .education
        .active()
        .to_vec();

    assert_eq!(edu.len(), 1, "{edu:#?}");
    // The field of study stays with the degree — it is not a school.
    assert_eq!(
        edu[0].study_type,
        "Master of Engineering, Petroleum Engineering"
    );
    assert_eq!(edu[0].institution, "University of Calgary, Alberta");
    assert_eq!(edu[0].highlights, vec!["GPA: 3.72/4.00"]);
}

#[test]
fn the_two_halves_of_an_education_entry_are_recognised_either_way_round() {
    assert!(looks_like_institution("Bellows College"));
    assert!(looks_like_institution("Universitat Autònoma de Barcelona"));
    assert!(!looks_like_institution("Doctor of Medicine (MD)"));

    assert!(looks_like_degree("Doctor of Medicine (MD)"));
    assert!(looks_like_degree("BSc, Applied Mathematics"));
    assert!(looks_like_degree("Graduate coursework — MSc in Modeling"));
    assert!(!looks_like_degree("Bellows College"));
    // A word that merely contains an abbreviation is not a degree.
    assert!(!looks_like_degree("Madeleine Consulting"));
}

/// `Graduate Projects` sits inside a degree, above three projects that each
/// own their bullets. Flattened into the degree's bullet list it read as an
/// achievement of that degree — a claim the CV never made.
#[test]
fn a_sub_heading_inside_a_section_becomes_a_section_of_its_own() {
    let raw = "EDUCATION\n\n\
               Bachelor of Science, Chemical Engineering 2005 – 2008\n\
               Prestigious University, Iran\n\
               •  Thesis Project: Pinch Technology\n\
               Graduate Projects\n\
               Shell Scotford Upgrader Expansion\n\
               •  Evaluated Upgrader Alley\n\
               Industrial Water Treatment\n\
               •  Evaluated processes of treating waste water\n";
    let doc = classify_raw_text("PDF", raw).doc;
    let edu = doc.education.active();

    assert_eq!(edu.len(), 1, "{edu:#?}");
    assert_eq!(
        edu[0].highlights.len(),
        1,
        "the projects leaked into the degree"
    );
    assert_eq!(doc.custom_sections.len(), 1, "{:#?}", doc.custom_sections);
    let projects = &doc.custom_sections[0];
    assert_eq!(projects.title, "Graduate Projects");
    let entries = projects.content.active();
    assert_eq!(entries.len(), 2, "{entries:#?}");
    assert_eq!(entries[0].title, "Shell Scotford Upgrader Expansion");
    assert_eq!(entries[0].highlights.len(), 1);
}

/// The rule must not fire in a section that never used a bullet glyph:
/// there is no list to interrupt, only prose.
#[test]
fn prose_under_an_entry_is_not_mistaken_for_a_sub_heading() {
    let raw = "EDUCATION\n\n\
               BSc, Applied Mathematics, Odesa National University 2019 – 2023\n\
               Numerical methods, optimization and control theory\n\
               Completed while working full-time\n";
    let doc = classify_raw_text("PDF", raw).doc;

    assert!(doc.custom_sections.is_empty(), "{:#?}", doc.custom_sections);
    assert_eq!(doc.education.active()[0].highlights.len(), 2);
}

/// A `CONTACT` heading is not a section of entries: its lines are the
/// profile's fields. They were becoming the three entries of a custom
/// section called Contact while the profile they belong to stayed empty.
#[test]
fn a_contact_section_fills_the_profile_rather_than_making_a_section() {
    let raw = "Dr. Amelia Evelyn\n\
               Cardiothoracic Surgeon\n\
               \n\
               PROFILE\n\
               An accomplished surgeon.\n\
               \n\
               CONTACT\n\
               \n\
               someone@example.com\n\
               (201) 555-0101\n\
               https://www.excellentwebsite.com\n";
    let imported = classify_raw_text("DOCX", raw);
    let basics = imported.doc.profile.active();

    assert_eq!(
        basics.name, "Dr. Amelia Evelyn",
        "the name must survive the block"
    );
    assert_eq!(basics.email, "someone@example.com");
    assert!(!basics.phone.is_empty(), "phone should be read");
    assert!(
        basics.url.contains("excellentwebsite"),
        "got {:?}",
        basics.url
    );
    assert!(
        imported.doc.custom_sections.is_empty(),
        "contact is not a section: {:#?}",
        imported.doc.custom_sections
    );
    assert!(imported.unplaced.is_empty(), "{:?}", imported.unplaced);
}

/// A line of the CV's own prose containing the word *work* is not a Work
/// heading. The substring rule that made it one moved the section boundary
/// and swallowed everything under it.
#[test]
fn prose_containing_a_section_keyword_is_not_a_heading() {
    assert!(!is_section_header("Completed while working full-time"));
    assert!(!is_section_header("pymolt, Python Migration Tool 2026"));
    // Capitals with figures in them are data, not a heading.
    assert!(!is_section_header("GPA: 3.72/4.00"));
    assert!(!is_section_header("MSC 2019"));
    assert!(is_section_header("WORK EXPERIENCE"));
    assert!(is_section_header("Education"));
    assert!(is_section_header("PROJECTS"));
}

/// A running header on a line of its own — `Jane Doe    Page 2` — opened a
/// job called by the person's own name. The glued case was already handled;
/// this is the same header, laid out differently.
#[test]
fn a_running_header_on_its_own_line_is_dropped() {
    let raw = "Jane Doe\n\
               jdoe@ucalgary.ca\n\
               \n\
               EXPERIENCE\n\
               Co-op Engineering Student, National Petrochemical 2007 – 2008\n\
               • Part of a five-member engineering team.\n\
               \n\
               Jane Doe                      Page 2\n\
               • Continued on the second page.\n";
    let doc = classify_raw_text("PDF", raw).doc;
    let work = doc.work.active();

    assert_eq!(work.len(), 1, "{work:#?}");
    assert_eq!(work[0].position, "Co-op Engineering Student");
    assert_eq!(work[0].highlights.len(), 2);
}

/// A contact block taken from a real FlowCV export. Each assertion here
/// is a defect that shipped and was visible on the review screen.
#[test]
fn a_real_contact_block_is_read_rather_than_reported_as_lost() {
    let raw = "Marie Curie  Systems & Data Engineer\n\
               For legal purpose: Albert Einstein\n\
               Calgary, Canada\n\
               albert@example.com\n\
               https://www.linkedin.com/in/aeinstein\n\
               https://www.example.com/\n\
               \n\
               PROFILE\n\
               Systems & Data Engineer with an applied mathematics background.\n";
    let imported = classify_raw_text("PDF", raw);
    let basics = imported.doc.profile.active();

    // The name and the title shared one line, split by a run of spaces.
    assert_eq!(basics.name, "Marie Curie");
    assert_eq!(basics.label, "Systems & Data Engineer");
    assert_eq!(basics.email, "albert@example.com");
    assert_eq!(basics.location, "Calgary, Canada");

    // Both URLs are kept, one as the primary and one as a profile.
    let urls: Vec<&str> = std::iter::once(basics.url.as_str())
        .chain(basics.profiles.iter().map(|p| p.url.as_str()))
        .collect();
    assert!(
        urls.iter().any(|u| u.contains("linkedin.com")),
        "got {urls:?}"
    );
    assert!(
        urls.iter().any(|u| u.contains("example.com/")),
        "got {urls:?}"
    );

    // The heart of it: contact data must not be reported as dropped. The
    // email in particular was both parsed *and* listed as lost.
    for line in imported.unplaced.iter().map(Unplaced::line) {
        assert!(
            !line.contains('@') && !line.contains("http"),
            "contact line reported as unplaced: {line}"
        );
    }
}

/// PDF extraction wraps long skill rows. Each fragment used to become its
/// own group, turning six groups into thirty-one on the review screen.
#[test]
fn wrapped_skill_lines_continue_the_group_above_them() {
    // Real widths matter: the wrap is detected by measuring the document,
    // so a fixture whose lines are all short describes a document that was
    // never typeset and proves nothing about one that was.
    let raw = "SKILLS\n\
               Programming Languages — Expert: Python   Competent: C/C++, Rust, Java\n\
               Mathematical Modeling & HPC — Numerical Methods   Optimization & Control Theory   High-Performance Computing\n\
               (HPC)   Time-Series Analysis   Vectorized Algorithms   Dynamical Systems\n";
    let imported = classify_raw_text("PDF", raw);
    let skills = imported.doc.skills.active();

    assert_eq!(
        skills.len(),
        2,
        "got {:?}",
        skills.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert_eq!(skills[0].name, "Programming Languages");
    assert_eq!(skills[1].name, "Mathematical Modeling & HPC");
    // The wrapped fragment joined the group above rather than starting one.
    assert!(
        skills[1].keywords.iter().any(|k| k == "Dynamical Systems"),
        "got {:?}",
        skills[1].keywords
    );
    // Multi-word skills stay whole.
    assert!(skills[1]
        .keywords
        .iter()
        .any(|k| k == "Optimization & Control Theory"));
}

#[test]
fn test_date_range_regex_formats() {
    let regex = get_date_range_regex();
    assert!(regex.is_match("Jan 2020 – Dec 2022"));
    assert!(regex.is_match("January 2020 - Present"));
    assert!(regex.is_match("2018-05 — 2021-11"));
    assert!(regex.is_match("Сентябрь 2019 – Настоящее время"));
    assert!(regex.is_match("01/2020 to 12/2022"));
    assert!(regex.is_match("Янв 2021 — по н.в."));
}

#[test]
fn test_multilingual_iso_header_classification() {
    assert_eq!(classify_header("# WORK EXPERIENCE"), SectionKind::Work);
    assert_eq!(classify_header("## 1. Опыт работы:"), SectionKind::Work);
    assert_eq!(classify_header("Berufserfahrung"), SectionKind::Work); // German
    assert_eq!(
        classify_header("Expérience professionnelle"),
        SectionKind::Work
    ); // French
    assert_eq!(classify_header("Experiencia laboral"), SectionKind::Work); // Spanish
    assert_eq!(classify_header("**Образование**"), SectionKind::Education);
    assert_eq!(classify_header("Ausbildung"), SectionKind::Education); // German
    assert_eq!(
        classify_header("Technical Skills & Tools"),
        SectionKind::Skills
    );
    assert_eq!(classify_header("Compétences"), SectionKind::Skills); // French
    assert_eq!(
        classify_header("Certifications & Licenses"),
        SectionKind::Certificates
    );
    assert_eq!(classify_header("О себе"), SectionKind::Summary);
}

#[test]
fn test_nlp_fuzzy_matching_typos() {
    assert_eq!(classify_header("Experiance"), SectionKind::Work);
    assert_eq!(classify_header("Educaton"), SectionKind::Education);
    assert_eq!(classify_header("Skils"), SectionKind::Skills);
}

/// The third of three sources, and the only one whose offer nothing
/// asserted: a line the classifier read and could not place has no label,
/// so the person names the section and the lines become its bullets.
///
/// The kinds must not merge. Nothing in `offer()` stops someone changing
/// one arm of that match, and the difference is the whole of what the
/// import panel can honestly say.
#[test]
fn a_line_that_cannot_be_placed_is_offered_as_a_section_the_person_names() {
    use crate::import::model::{adopt_as_section, Unplaced, UnplacedOffer, UnplacedSource};

    let raw = "Albert Einstein\n\
               CONTACT\n\
               albert@example.com\n\
               Member of the Prussian Academy\n";
    let imported = classify_raw_text("PDF", raw);

    // The email is contact data and is consumed; the sentence is not.
    let leftover = imported
        .unplaced
        .iter()
        .find(|u| u.source == UnplacedSource::Classifier)
        .expect("the sentence is reported, not dropped");
    assert_eq!(leftover.title, "Member of the Prussian Academy");
    assert_eq!(
        leftover.offer(),
        UnplacedOffer::NamedByPerson,
        "a classifier line has no heading to propose"
    );

    // And the panel's promise — "each becomes one bullet" — is what
    // actually happens. Built here rather than taken from the import
    // because the contact reader joins consecutive stray lines into one,
    // and the shape worth pinning is what adoption does with several.
    let lines = [
        Unplaced::line_only("Member of the Prussian Academy"),
        Unplaced::line_only("Willing to relocate"),
    ];
    let mut doc = imported.doc.clone();
    assert_eq!(adopt_as_section(&mut doc, "Notes", &lines), 2);

    let section = doc.custom_sections.last().expect("the section");
    assert_eq!(section.title, "Notes");
    let entries = section.content.active();
    assert_eq!(
        entries.len(),
        1,
        "unlabelled lines are bullets of one entry, not headings of several"
    );
    assert!(entries[0].title.is_empty());
    assert_eq!(
        entries[0].highlights,
        ["Member of the Prussian Academy", "Willing to relocate"]
    );
}
