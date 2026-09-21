//! Semantic classification, NLP fuzzy matching, and entity extraction for raw text blocks.


use crate::import::layout;
use crate::import::model::{ImportedDoc, Unplaced};
use crate::import::notes::{Note, Part};
use crate::resume::model::{
    CustomEntry, Education, Resume, ResumeDoc, SkillGroup, Volunteer,
    Work,
};

use super::classifier_entries::{
    attach_entry_url, clean_bullet, ends_with_parenthesised_date, get_date_range_regex,
    get_single_date_regex, is_only_dates, looks_like_degree,
    looks_like_institution, parse_certificate, split_keywords, split_skill_group,
    strip_running_header,
};
use super::classifier_headings::{
    classify_header, is_section_header, title_case,
};
use super::classifier_contact::{
    absorb_contact, contact_region, get_email_regex, get_phone_regex, get_url_regex,
    looks_like_contact_line, network_of, trim_url_tail,
};

















#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SectionKind {
    Work,
    Education,
    Skills,
    Certificates,
    Volunteer,
    Summary,
    Contact,
    Named,
    Unknown,
}






























/// Classify a raw text stream into a candidate [`ImportedDoc`].
///
/// The path for formats that carry **no structure** — a PDF's text layer, a
/// plain-text file. Everything a section parser needs has to be recovered from
/// typography here, which is what [`layout::logical_lines`] does. A format that
/// reports its own structure should build the lines itself and call
/// [`classify_lines`] instead of flattening to text first: the flattening is
/// lossy and the recovery is a guess, however good.
pub fn classify_raw_text(format_name: &str, raw_text: &str) -> ImportedDoc {
    // A running page header lands in the text layer glued to the line above it
    // (`…atmospheric profiles.Marie Curiealbert@example.com`), so strip it before
    // anything else looks at a line. Blank lines are kept: the layout pass
    // below reads them.
    let email = get_email_regex()
        .find(raw_text)
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let name_fragment = raw_text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !is_section_header(l))
        .map(|l| l.split("  ").next().unwrap_or(l).trim().to_string())
        .filter(|n| n.len() >= 4 && !n.contains('@'))
        .unwrap_or_default();
    let fragments = [name_fragment, email];
    // Leading whitespace is carried through: `layout` reads it to tell an
    // indented list item from a date range at the margin, and to join a wrapped
    // item back onto the one it belongs to. `strip_running_header` trims, so
    // the indent is measured here and put back.
    let cleaned: String = raw_text
        .lines()
        .map(|l| {
            let body = l.trim_start();
            let indent = &l[..l.len() - body.len()];
            let stripped = strip_running_header(body, &fragments);
            if stripped.is_empty() {
                stripped
            } else {
                format!("{indent}{stripped}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Physical line boxes become logical lines: a bullet broken by the text
    // measure is one bullet again, and the section parsers below never have to
    // guess whether a line is a new item or the tail of the last one.
    let lines = layout::logical_lines(&cleaned, is_section_header, |l| {
        // "Does this line already carry its dates?" — asked of the line above,
        // to decide whether the next one continues it. A certificate is written
        // `Name - Issuer (2023-04)`: one date, not a range, and nothing after
        // it, so the line is finished. Read as unfinished, two certificates
        // joined into one.
        get_date_range_regex().is_match(l) || ends_with_parenthesised_date(l)
    });
    classify_lines(format_name, join_split_entry_headers(lines))
}

/// Put an entry's dates back on its title.
///
/// Templates routinely give the dates a cell, a paragraph or a column of their
/// own — styled `Dates`, styled `Heading2` with the title in a plain run beside
/// it, or simply a table with the years down the left. Split that way neither
/// half is a usable entry header: the title carries no date to place it, and
/// the dates carry no title to name them.
///
/// This lived in the DOCX engine, on the reasoning that scattering an entry
/// across cells is a fact about Word. It is not. A PDF exported from the same
/// Word template arrives with the date on its own line for exactly the same
/// reason, and a CV built as a two-column table — which is most of the Word
/// gallery — imported as two jobs that had dates and no employer, no title and
/// no bullets. Both directions are handled: the dates can precede their entry
/// or follow it.
pub fn join_split_entry_headers(lines: Vec<layout::LogicalLine>) -> Vec<layout::LogicalLine> {
    let mut out: Vec<layout::LogicalLine> = Vec::with_capacity(lines.len());
    let mut pending_dates: Option<String> = None;

    for line in lines {
        if line.kind != layout::LineKind::Heading && is_only_dates(&line.text) {
            // An entry's own address sits on a line of its own between the
            // title and the dates — both the Word and the Markdown readers put
            // it there so `attach_entry_url` can pick it up — and it is not a
            // line that can own dates. Gluing them to it made `ethz.ch 1896-10
            // - 1900-07`, which is no longer an address, so the school's link
            // was dropped and the string became an entry of its own.
            let owner = match out.last() {
                Some(last) if layout::is_lone_address(&last.text) => out.len().checked_sub(2),
                _ => out.len().checked_sub(1),
            };
            match owner.and_then(|at| out.get_mut(at)) {
                // The line above claims the dates whenever it is one that could
                // own them. That is the order DockCV's own exporter writes —
                // title, dates, bullets — and holding them for the *next* line
                // stapled them to the entry's first bullet instead, leaving the
                // entry itself undated.
                Some(prev)
                    if prev.kind != layout::LineKind::Heading
                        && prev.kind != layout::LineKind::Bullet =>
                {
                    prev.text = format!("{} {}", prev.text, line.text);
                    prev.kind = layout::LineKind::EntryHeader;
                }
                // A section heading or a list above, so nothing there can own
                // them: this template printed the dates first, and the entry is
                // on the line below.
                _ => pending_dates = Some(line.text.clone()),
            }
            continue;
        }
        match pending_dates.take() {
            // The line after a bare date is the entry that date belongs to —
            // unless it is a list item, which is content under an entry and
            // never an entry itself.
            Some(dates)
                if line.kind != layout::LineKind::Heading
                    && line.kind != layout::LineKind::Bullet =>
            {
                out.push(layout::LogicalLine::new(
                    format!("{} {}", line.text, dates),
                    layout::LineKind::EntryHeader,
                ))
            }
            // A heading or a bullet follows: the dates belonged to the entry
            // above them.
            Some(dates) => {
                if let Some(prev) = out
                    .last_mut()
                    .filter(|p| p.kind == layout::LineKind::EntryHeader)
                {
                    prev.text = format!("{} {}", prev.text, dates);
                }
                out.push(line);
            }
            None => out.push(line),
        }
    }
    out
}


/// Turn logical lines into a candidate [`ImportedDoc`].
///
/// The shared half of the importer: every format ends up here, whether its
/// structure was measured out of a page (PDF) or read off the markup (DOCX).
/// What differs between formats is only how good the evidence was — which is
/// why [`layout::LineKind::EntryHeader`] exists: DOCX can state that a line
/// opens an entry, and a PDF can only infer it from a date range.
pub fn classify_lines(format_name: &str, lines: Vec<layout::LogicalLine>) -> ImportedDoc {
    let mut resume = Resume::default();
    let mut unplaced = Vec::new();

    // Contact details come from the **contact block**, not from anywhere in the
    // document. These three regexes used to run over the whole joined text and
    // take the first hit each, which on an ordinary CV meant a vendor named in
    // a bullet became the user's own website:
    //
    // ```
    // • Migrated billing to https://stripe.com and cut p99 40%
    // • Reached out on +1 415 555 0134 for the vendor escalation
    // ```
    //
    // …imported as `basics.url = "https://stripe.com"` and
    // `basics.phone = "+1 415 555 0134"` — a payment processor's site and
    // somebody else's number, on the person's own CV.
    //
    // The region is the same one `absorb_contact` works over, so the two agree:
    // everything above the first heading, plus anything under an explicit
    // CONTACT heading.
    let whole_text = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let contact_text = contact_region(&lines);
    let contact_text = contact_text.as_str();

    // The email falls back to the whole document; the phone and the website do
    // not. The asymmetry is the point.
    //
    // An address is a strong claim of identity — a CV that carries one carries
    // the author's, and a company address in a bullet is rare enough to lose to
    // the cost of dropping the user's own. A URL is the opposite: naming a
    // vendor, a repository or a client's site inside a bullet is ordinary, which
    // is exactly how `https://stripe.com` became someone's personal website. A
    // phone number sits with the URL.
    //
    // What makes the fallback earn its keep rather than undo the fix: a
    // two-column PDF interleaves its sidebar with its body (see
    // `engines::pdf`'s `a_two_column_page_is_read_in_content_stream_order`), so
    // the contact block is not above the first heading at all and a
    // region-only reading would lose the address outright.
    let email_scope = match get_email_regex().is_match(contact_text) {
        true => contact_text,
        false => whole_text.as_str(),
    };
    if let Some(mat) = get_email_regex().find(email_scope) {
        resume.basics.email = mat.as_str().to_string();
    }
    if let Some(mat) = get_phone_regex().find(contact_text) {
        resume.basics.phone = mat.as_str().to_string();
    }
    // The person's *own* site, not the first address in the block: a CV lists
    // its GitHub and its LinkedIn there too, and taking the first left the
    // Website field pointing at a profile — which the line walk below then
    // listed a second time.
    // Only an address that is nobody's profile. A GitHub or a LinkedIn is not
    // the person's website, and filing one there both mislabelled it and left
    // the real site to be listed again underneath as a profile — the header
    // then printed the same address twice. A network address is not lost by
    // this: the walk below files it under `profiles`, which is where it goes.
    if let Some(own) = get_url_regex()
        .find_iter(contact_text)
        .map(|m| trim_url_tail(m.as_str()))
        .find(|u| network_of(u) == "Website")
    {
        resume.basics.url = own.to_string();
    }

    let mut current_section = SectionKind::Unknown;
    // A CV with no section headings at all is a real template — the
    // minimalist one — and it used to import as a name, an email and a couple
    // of lines the wizard offered to adopt. Everything else was read as part
    // of the contact block and dropped, because `current_section` never left
    // `Unknown` and the arm for `Unknown` is the contact block.
    //
    // With nothing to segment on, the shape of a line is all there is: the
    // first one carrying a date range ends the contact block and opens the
    // work history. Filing a degree under Work is wrong and is *visibly*
    // wrong, where losing it is neither — so the reading is stated in a note
    // rather than performed quietly.
    let has_headings = lines.iter().any(|l| l.kind == layout::LineKind::Heading);
    let implicit_work_at = if has_headings {
        None
    } else {
        lines
            .iter()
            .position(|l| !l.is_bullet() && get_date_range_regex().is_match(&l.text))
    };

    let mut first_lines: Vec<&str> = Vec::new();
    let mut custom: Vec<(String, Vec<CustomEntry>)> = Vec::new();
    let mut seen: Vec<SectionKind> = Vec::new();

    for (idx, entry) in lines.iter().enumerate() {
        // A sub-heading is told from a sub-entry by what follows it: an entry
        // owns the bullets under it, a heading is followed by the entries it
        // names. Both are non-bullet lines, so the line alone cannot say which.
        let next_is_bullet = lines.get(idx + 1).is_some_and(|l| l.is_bullet());
        // And a list is only interrupted where a list was actually running: a
        // section that never used a bullet glyph has no sub-headings to find,
        // only prose.
        let after_bullet = idx
            .checked_sub(1)
            .and_then(|i| lines.get(i))
            .is_some_and(|l| l.is_bullet());
        // …and a line followed by nothing but dates is an entry, whatever else
        // it looks like. A heading is never dated. Without this, the second
        // degree in an Education section — printed after the first degree's
        // bullets, with its years on the line below — was read as a sub-heading
        // and became a section of its own, taking the degree out of Education.
        let next_is_dates = lines
            .get(idx + 1)
            .is_some_and(|l| l.kind != layout::LineKind::Heading && is_only_dates(&l.text));
        if Some(idx) == implicit_work_at {
            current_section = SectionKind::Work;
            seen.push(SectionKind::Work);
        }
        if entry.kind == layout::LineKind::Heading {
            current_section = classify_header(&entry.text);
            // A document never has two Work sections. When a second heading
            // classifies as one already used, the taxonomy is stretching — the
            // corpus files `projects` under work, which is right for a CV whose
            // *only* history is projects and wrong for this one, where PROJECTS
            // sits beside WORK EXPERIENCE and was being appended to the last
            // job. The first heading of a kind keeps it; a repeat becomes a
            // section of its own, under its own name.
            if current_section != SectionKind::Unknown {
                if seen.contains(&current_section) {
                    current_section = SectionKind::Unknown;
                } else {
                    seen.push(current_section);
                }
            }
            // A heading the taxonomy does not know is still a heading — the
            // shape test proved that much. Giving it a custom section keeps its
            // content addressable instead of letting it fall into whichever
            // section happened to be open, which is how PROJECTS ended up
            // inside the last job.
            if current_section == SectionKind::Unknown {
                current_section = SectionKind::Named;
                custom.push((title_case(&entry.text), Vec::new()));
            }
            continue;
        }
        let line = entry.text.as_str();

        // An entry's link is printed on its own line beneath it — on the page,
        // and in the plain-text export the same way. Read as content it became
        // an entry of its own, so a section of one certificate imported as two,
        // the second of them called `https://certificate.com`.
        if !entry.is_bullet()
            && layout::is_lone_address(line)
            && attach_entry_url(current_section, &mut resume, &mut custom, line)
        {
            continue;
        }

        match current_section {
            SectionKind::Named => {
                let Some((_, entries)) = custom.last_mut() else {
                    continue;
                };
                if entry.is_bullet() {
                    match entries.last_mut() {
                        Some(last) => last.highlights.push(line.to_string()),
                        None => entries.push(CustomEntry {
                            highlights: vec![line.to_string()],
                            ..Default::default()
                        }),
                    }
                } else if let Some(open) = entries
                    .last_mut()
                    .filter(|e| e.subtitle.is_empty() && e.highlights.is_empty())
                {
                    // The line under an entry's title, before any bullet, is
                    // the entry's organisation — `A+ Tutors, Calgary, Alberta`
                    // under `Mathematics, Physics, and Chemistry Tutor`. Each
                    // was becoming an entry of its own.
                    open.subtitle = line.to_string();
                } else {
                    // The same reader Work and Education use, rather than a
                    // second, cruder one. Splitting on the first comma cut
                    // `Mathematics, Physics, and Chemistry Tutor` in half, and
                    // removing a single year left the rest of the range behind
                    // as `–Current` in the middle of the title.
                    let header = layout::EntryHeader::parse(line, get_date_range_regex());
                    let (start, end, rest) = if header.start.is_empty() {
                        // No range: a lone year, as a project usually carries.
                        let year = get_single_date_regex()
                            .find(line)
                            .map(|m| m.as_str().to_string())
                            .unwrap_or_default();
                        (year.clone(), String::new(), line.replace(&year, ""))
                    } else {
                        (header.start.clone(), header.end.clone(), header.whole())
                    };
                    // The line is **not** split on its commas. `pymolt, Python
                    // Migration Tool` and `Mathematics, Physics, and Chemistry
                    // Tutor` are the same punctuation and different intents —
                    // one is a name and a description, the other is one job
                    // title. Guessing cut the second in half; keeping the
                    // author's line whole costs the first a split it can make
                    // itself. The subtitle is filled from the line *below*,
                    // where the organisation actually is.
                    entries.push(CustomEntry {
                        title: rest
                            .trim()
                            .trim_end_matches([',', '-', '–', '—'])
                            .trim()
                            .to_string(),
                        subtitle: String::new(),
                        start_date: start.into(),
                        end_date: end.into(),
                        ..Default::default()
                    });
                }
            }
            SectionKind::Unknown => {
                // The block above the first heading is the contact block, not
                // "the first three lines". Anything in it that *is* contact
                // data gets consumed as contact data; only genuinely
                // unrecognised text is reported as dropped.
                //
                // Before this, `first_lines` took three lines and everything
                // after went to `unplaced`, which is why a CV's own email,
                // LinkedIn and website were reported as "didn't fit any
                // section" — while the email had in fact already been picked
                // up by the regex pass above, so it was both used *and*
                // reported lost.
                if absorb_contact(line, &mut resume) {
                    // Consumed as contact data.
                } else if first_lines.len() < 3 {
                    first_lines.push(line);
                } else {
                    unplaced.push(Unplaced::line_only(line));
                }
            }
            SectionKind::Contact => {
                // Under an explicit CONTACT heading every line is contact data
                // or nothing. The name is never re-read here — it belongs at
                // the top of the document, and a second reading would overwrite
                // it with whatever the block happened to start with.
                if !absorb_contact(line, &mut resume) {
                    unplaced.push(Unplaced::line_only(line));
                }
            }
            SectionKind::Summary => {
                if resume.basics.summary.is_empty() {
                    resume.basics.summary = line.to_string();
                } else {
                    resume.basics.summary.push('\n');
                    resume.basics.summary.push_str(line);
                }
            }
            SectionKind::Work => {
                if entry.is_bullet() {
                    match resume.work.last_mut() {
                        Some(last) => last.highlights.push(line.to_string()),
                        None => resume.work.push(Work {
                            highlights: vec![line.to_string()],
                            ..Default::default()
                        }),
                    }
                    continue;
                }

                // A dated line opens an entry. The role may have been printed
                // on the line *above* it — `Backend Engineer, ML
                // Infrastructure` / `Sembly AI … Oct 2019 – Jul 2021` — in
                // which case the entry is already open and waiting for its
                // employer and dates rather than being a second one.
                let stated = entry.kind == layout::LineKind::EntryHeader;
                if stated || get_date_range_regex().is_match(line) {
                    let header = layout::EntryHeader::parse(line, get_date_range_regex());
                    // A format that *states* an entry opens here is never
                    // second-guessed. The merge below is for the inferred case,
                    // where a role was printed on the line above its employer
                    // and only the dated line proves they are one entry.
                    let awaiting = resume
                        .work
                        .last_mut()
                        .filter(|_| !stated)
                        .filter(|w| w.start_date.is_empty() && w.highlights.is_empty());
                    match awaiting {
                        Some(open) => {
                            if open.name.is_empty() {
                                open.name = header.whole();
                            }
                            open.start_date = header.start.into();
                            open.end_date = header.end.into();
                            open.location = header.location;
                        }
                        None => resume.work.push(Work {
                            position: header.lead,
                            name: header.org,
                            location: header.location,
                            start_date: header.start.into(),
                            end_date: header.end.into(),
                            ..Default::default()
                        }),
                    }
                } else if let Some(last) = resume
                    .work
                    .last_mut()
                    .filter(|w| w.name.is_empty() && !w.position.is_empty())
                    // …but only while the entry is still being opened. An
                    // employer is printed *above* the dates, never after the
                    // bullets, so once either has arrived the entry is finished
                    // and the next bare line begins the following job. Without
                    // this, a plain-text CV filed every job's title as the
                    // previous job's employer and lost one entry per job.
                    .filter(|w| w.start_date.is_empty() && w.highlights.is_empty())
                {
                    // The employer, printed under the job title. DOCX templates
                    // scatter an entry across cells this way, and each stray
                    // line was becoming a job of its own.
                    last.name = line.to_string();
                } else if let Some(last) = resume
                    .work
                    .last_mut()
                    .filter(|w| !w.start_date.is_empty() && w.highlights.is_empty())
                {
                    // Dated entry, bullets not started yet: this is the entry's
                    // own blurb, or the link that follows it.
                    if last.summary.is_empty() {
                        last.summary = line.to_string();
                    } else {
                        last.summary.push(' ');
                        last.summary.push_str(line);
                    }
                } else {
                    // A role printed on its own line, waiting for the employer
                    // and dates underneath. Kept whole: `Backend Engineer, ML
                    // Infrastructure` is one job title, and the comma in it
                    // separates nothing.
                    resume.work.push(Work {
                        position: line.to_string(),
                        ..Default::default()
                    });
                }
            }
            SectionKind::Education => {
                // Same three shapes as Work, and for the same reason: the
                // degree is often printed above the university, so a dated line
                // completes the entry opened by the line before it rather than
                // starting a second one. Reading each line as its own entry is
                // why a CV with two degrees imported as one.
                let stated = entry.kind == layout::LineKind::EntryHeader;
                if stated || get_date_range_regex().is_match(line) {
                    let header = layout::EntryHeader::parse(line, get_date_range_regex());
                    let awaiting = resume
                        .education
                        .last_mut()
                        .filter(|_| !stated)
                        .filter(|e| e.start_date.is_empty() && e.highlights.is_empty());
                    match awaiting {
                        Some(open) => {
                            open.institution = header.whole();
                            open.start_date = header.start.into();
                            open.end_date = header.end.into();
                        }
                        // Which half of the entry this header carries is
                        // decided by the words, not by the position. Templates
                        // print the school above the degree and the degree
                        // above the school with equal enthusiasm — one marked
                        // `Bellows College`, the next `Doctor of Medicine
                        // (MD)`, both in the same slot.
                        None => {
                            let whole = header.whole();
                            let (study_type, institution) = if header.org.is_empty() {
                                if looks_like_degree(&whole) {
                                    (whole, String::new())
                                } else {
                                    (String::new(), whole)
                                }
                            } else if looks_like_institution(&header.org) {
                                (header.lead.clone(), header.org.clone())
                            } else {
                                // `Master of Engineering, Petroleum Engineering`
                                // — the second half is the field, not the
                                // school. Kept together; the institution comes
                                // from the line below.
                                (whole, String::new())
                            };
                            resume.education.push(Education {
                                study_type,
                                institution,
                                start_date: header.start.into(),
                                end_date: header.end.into(),
                                ..Default::default()
                            });
                        }
                    }
                } else if !entry.is_bullet() && after_bullet && !next_is_bullet && !next_is_dates {
                    // A non-bullet line arriving after the bullet list has
                    // started is not another bullet — the document stopped
                    // listing and started naming. Followed by more names rather
                    // than by bullets, it is a **sub-heading**: `Graduate
                    // Projects` above three projects, inside a degree.
                    //
                    // It becomes a section of its own (D-9), which is what the
                    // page means and the only shape the model has for it.
                    // Flattened into the bullet list it read as an achievement
                    // of the degree, which is worse than being in the wrong
                    // place: it is a claim the CV never made.
                    current_section = SectionKind::Named;
                    custom.push((title_case(line), Vec::new()));
                } else if let Some(last) = resume.education.last_mut() {
                    // The counterpart the header did not carry, if this is it.
                    // Otherwise coursework, thesis, honours — not gated on the
                    // entry being dated, since a DOCX template states where an
                    // entry begins and may carry no date at all.
                    let text = layout::without_bullet(line).to_string();
                    if last.institution.is_empty() && looks_like_institution(&text) {
                        last.institution = text;
                    } else if last.study_type.is_empty() && looks_like_degree(&text) {
                        last.study_type = text;
                    } else {
                        last.highlights.push(text);
                    }
                } else {
                    resume.education.push(Education {
                        study_type: layout::without_bullet(line).to_string(),
                        ..Default::default()
                    });
                }
            }
            SectionKind::Skills => {
                let bullet = clean_bullet(line);
                // A group is `Name — kw   kw   kw`; a line with no separator
                // is the **wrap** of the group above it, not a new group.
                //
                // PDF extraction breaks long skill rows across lines, and
                // treating each fragment as its own group turned six groups
                // into thirty-one — a number visibly absurd on the review
                // screen, and one that would have shipped into the document.
                match split_skill_group(bullet) {
                    Some((name, keywords)) => resume.skills.push(SkillGroup { name, keywords }),
                    None => match resume.skills.last_mut() {
                        Some(group) => group.keywords.extend(split_keywords(bullet)),
                        // Nothing to continue: the section's first line had no
                        // separator, so it is a bare list of skills.
                        None => resume.skills.push(SkillGroup {
                            name: String::new(),
                            keywords: split_keywords(bullet),
                        }),
                    },
                }
            }
            SectionKind::Certificates => {
                resume
                    .certificates
                    .push(parse_certificate(clean_bullet(line)));
            }
            SectionKind::Volunteer => {
                // The same three shapes as Work, because it is the same shape
                // of thing: a role at an organisation, over a period, with
                // bullets under it. This branch used to push a new entry for
                // every line and put the whole line in `position`, so a section
                // of two roles imported as six roles with no dates, no
                // organisation and their bullets promoted to entries of their
                // own.
                if entry.is_bullet() {
                    match resume.volunteer.last_mut() {
                        Some(last) => last.highlights.push(line.to_string()),
                        None => resume.volunteer.push(Volunteer {
                            highlights: vec![line.to_string()],
                            ..Default::default()
                        }),
                    }
                    continue;
                }

                let stated = entry.kind == layout::LineKind::EntryHeader;
                if stated || get_date_range_regex().is_match(line) {
                    let header = layout::EntryHeader::parse(line, get_date_range_regex());
                    let awaiting = resume
                        .volunteer
                        .last_mut()
                        .filter(|_| !stated)
                        .filter(|v| v.start_date.is_empty() && v.highlights.is_empty());
                    match awaiting {
                        Some(open) => {
                            if open.organization.is_empty() {
                                open.organization = header.whole();
                            }
                            open.start_date = header.start.into();
                            open.end_date = header.end.into();
                        }
                        None => resume.volunteer.push(Volunteer {
                            position: header.lead,
                            organization: header.org,
                            start_date: header.start.into(),
                            end_date: header.end.into(),
                            ..Default::default()
                        }),
                    }
                } else if let Some(last) = resume
                    .volunteer
                    .last_mut()
                    .filter(|v| v.organization.is_empty() && !v.position.is_empty())
                    .filter(|v| v.start_date.is_empty() && v.highlights.is_empty())
                {
                    last.organization = line.to_string();
                } else {
                    resume.volunteer.push(Volunteer {
                        position: clean_bullet(line).to_string(),
                        ..Default::default()
                    });
                }
            }
        }
    }

    // Name inference from initial lines if empty
    if resume.basics.name.is_empty() && !first_lines.is_empty() {
        // Exporters commonly put the name and the professional title on one
        // line, separated by a run of spaces rather than punctuation —
        // `Marie Curie  Systems & Data Engineer`. Splitting on that run
        // recovers both; without it the title became part of the name and the
        // person's own CV greeted them with it fused.
        match first_lines[0].split_once("  ") {
            Some((name, title)) if !title.trim().is_empty() => {
                resume.basics.name = name.trim().to_string();
                resume.basics.label = title.trim().to_string();
            }
            _ => resume.basics.name = first_lines[0].to_string(),
        }
        // The second line is the professional title *unless* it is contact
        // data. One template puts the whole address there, and it arrived as
        // somebody's job title: `Home or Campus Street Address • City, State
        // Zip • • phone number`.
        if resume.basics.label.is_empty() {
            if let Some(second) = first_lines.get(1).filter(|l| !looks_like_contact_line(l)) {
                resume.basics.label = second.to_string();
            }
        }
    }

    // The paragraph under the contact block, when the CV gives it no heading of
    // its own. It was collected into `first_lines` and then dropped on the
    // floor: every CV written the way DockCV writes one — name, title, contacts,
    // then the summary — imported with no summary at all.
    if resume.basics.summary.is_empty() {
        // Eight words was the old test, and it was a test about English. A
        // summary in Ukrainian says the same thing in seven — «Інженерка з
        // восьмирічним досвідом у розподілених системах.» — and so does a short
        // one in English, so both were dropped on the floor while the same CV
        // in longer words came through. What a summary *is* travels better than
        // how many words it takes: it is a sentence, and it ends like one.
        let is_prose = |l: &&&str| {
            !looks_like_contact_line(l)
                && (l.split_whitespace().count() >= 8
                    || (l.chars().count() >= 30
                        && l.trim_end().ends_with(['.', '!', '?', '。', '！', '？'])))
        };
        if let Some(prose) = first_lines.iter().skip(1).find(is_prose) {
            resume.basics.summary = (*prose).to_string();
            if resume.basics.label == **prose {
                resume.basics.label.clear();
            }
        }
    }

    let mut doc = ResumeDoc::from_resume(resume, "Base");

    // Sections the taxonomy has no shape for become custom sections (D-9) —
    // the extension point that already exists, rather than a seventh built-in
    // or, as before, silent absorption into whatever section was last open.
    for (title, entries) in custom {
        if entries.is_empty() {
            continue;
        }
        let id = doc.add_custom_section(title);
        if let Some(section) = doc.custom_section_mut(id) {
            *section.content.active_mut() = entries;
        }
    }

    let mut imported = ImportedDoc::new(format_name, doc);

    // The one thing a result cannot say: whether a section was *supposed* to
    // have content. `seen` is the list of headings this document actually
    // carried, so a heading that produced nothing is a defect, while a section
    // the CV never had is just a CV without one.
    for kind in &seen {
        let part = match kind {
            SectionKind::Work => Part::Work,
            SectionKind::Education => Part::Education,
            SectionKind::Skills => Part::Skills,
            SectionKind::Certificates => Part::Certificates,
            _ => continue,
        };
        let empty = match kind {
            SectionKind::Work => imported.doc.work.active().is_empty(),
            SectionKind::Education => imported.doc.education.active().is_empty(),
            SectionKind::Skills => imported.doc.skills.active().is_empty(),
            SectionKind::Certificates => imported.doc.certificates.active().is_empty(),
            _ => false,
        };
        if empty {
            imported.note(part, Note::Empty);
        }
    }

    if implicit_work_at.is_some() {
        let entries = imported.doc.work.active().len();
        if entries > 0 {
            imported.note(Part::Work, Note::ReadWithoutHeadings { entries });
        }
    }

    // Everything else is derivable from the document, and therefore the same
    // for every engine — which is what the old per-engine key-writing was not.
    imported.observe();
    imported.unplaced = unplaced;
    imported
}

#[cfg(test)]
#[path = "classifier_tests.rs"]
mod tests;
