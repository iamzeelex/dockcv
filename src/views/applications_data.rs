//! Pure helpers for the Applications board: date arithmetic, funnel/caption
//! formatting, and search matching. Split out of `applications.rs` to keep
//! that file under the house line limit — none of this needs `gpui` or
//! `Theme`, so it lives separately rather than crowding the render code.

use crate::resume::model::{Application, ApplicationStatus, Applications, NextStep, PresetConversion};
use crate::vault;

pub(super) fn matches_query(app: &Application, query: &str) -> bool {
    query.is_empty()
        || app.company.to_lowercase().contains(query)
        || app.role.to_lowercase().contains(query)
}

pub(super) fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// `FAANG · concise — 4 sent → 1 interview → 1 offer`. Cumulative funnel
/// stages — see `Applications::conversion`'s doc comment for the counting
/// rule — so a later stage is only appended once it has anything in it.
pub(super) fn conversion_line(conv: &PresetConversion) -> String {
    let mut line = format!("{} sent", conv.sent);
    if conv.interviews > 0 {
        line.push_str(&format!(
            " → {} interview{}",
            conv.interviews,
            plural(conv.interviews)
        ));
    }
    if conv.offers > 0 {
        line.push_str(&format!(" → {} offer{}", conv.offers, plural(conv.offers)));
    }
    format!("{} — {line}", conv.preset)
}

/// The preset chip (Applied) or the status chip (Interviewing/Offer) —
/// mutually exclusive per column, per the design doc's per-column content
/// table (§3). Wishlist and Rejected carry neither.
pub(super) fn card_chip_text(app: &Application, status: ApplicationStatus) -> Option<String> {
    match status {
        ApplicationStatus::Applied => (!app.preset.is_empty()).then(|| app.preset.clone()),
        ApplicationStatus::Interviewing | ApplicationStatus::Offer => {
            app.next_step.as_ref().map(next_step_caption)
        }
        ApplicationStatus::Wishlist | ApplicationStatus::Rejected => None,
    }
}

/// `Onsite · Thu 14:00`, `Take-home due Fri`, `Decide by Fri` — a caption
/// built from real structured fields (`NextStep::label`/`date`/`time`), never
/// stored pre-formatted (see the design doc's P-04 note on why `Decide by
/// Fri` as a bare string is a dead end for reminders).
pub(super) fn next_step_caption(step: &NextStep) -> String {
    match (weekday_abbrev(&step.date), step.time.trim()) {
        (Some(day), time) if !time.is_empty() => format!("{} · {day} {time}", step.label),
        (Some(day), _) => format!("{} {day}", step.label),
        (None, time) if !time.is_empty() => format!("{} {time}", step.label),
        (None, _) => step.label.clone(),
    }
}

/// Applications currently in Interviewing whose next step falls in the
/// current Monday–Sunday week. Deliberately *not* "cards currently in the
/// Interviewing column" — the design doc (§9) flags that reading as a
/// coincidence of the mockup's sample data, not a derivation the model can
/// rely on; this counts real interview dates instead.
pub(super) fn interviews_this_week(applications: &Applications, today: &str) -> usize {
    applications
        .entries
        .iter()
        .filter(|a| a.status() == ApplicationStatus::Interviewing)
        .filter_map(|a| a.next_step.as_ref())
        .filter(|step| same_week(&step.date, today))
        .count()
}

// ---------------------------------------------------------------------------
// Date arithmetic — local to this screen; see `same_week`'s doc comment.
// ---------------------------------------------------------------------------

/// `2026-06-18` → `(2026, 6, 18)`. Same rule as `diary.rs::parse_iso`: the
/// vault writes this format itself (`vault::today_iso`), so anything else is
/// a hand-edited file, and a card shows what it can rather than hiding.
pub(super) fn parse_iso(date: &str) -> Option<(i32, u32, u32)> {
    let mut parts = date.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    (1..=12).contains(&month).then_some((year, month, day))
}

/// Days since the Unix epoch for a valid Gregorian date — Howard Hinnant's
/// `days_from_civil`, the exact inverse of `vault::civil_from_days`. Kept
/// local rather than shared: that one is private to `vault.rs`, and this is
/// the only screen doing calendar-week arithmetic.
pub(super) fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m as i64 - 3 } else { m as i64 + 9 };
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Monday-indexed weekday (0 = Monday) for a day count since the epoch.
/// 1970-01-01 (day 0) was a Thursday.
pub(super) fn monday_index(days: i64) -> i64 {
    (days - 4).rem_euclid(7)
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const SHORT_MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub(super) fn weekday_abbrev(date: &str) -> Option<&'static str> {
    let (y, m, d) = parse_iso(date)?;
    let days = days_from_civil(y as i64, m, d);
    Some(WEEKDAYS[monday_index(days) as usize])
}

/// `Jul 12`.
pub(super) fn short_date(date: &str) -> String {
    match parse_iso(date) {
        Some((_, m, d)) => format!("{} {d}", SHORT_MONTHS[(m - 1) as usize]),
        None => date.to_string(),
    }
}

/// Whether `date` falls in the same Monday–Sunday week as `today`.
pub(super) fn same_week(date: &str, today: &str) -> bool {
    let (Some((dy, dm, dd)), Some((ty, tm, td))) = (parse_iso(date), parse_iso(today)) else {
        return false;
    };
    let day = days_from_civil(dy as i64, dm, dd);
    let anchor = days_from_civil(ty as i64, tm, td);
    let monday = anchor - monday_index(anchor);
    (monday..monday + 7).contains(&day)
}

/// Seconds since the epoch for midnight (UTC) of an ISO date — enough
/// precision for `vault::relative_time`'s "Nd ago" captions.
pub(super) fn iso_to_epoch_secs(date: &str) -> Option<u64> {
    let (y, m, d) = parse_iso(date)?;
    let days = days_from_civil(y as i64, m, d);
    (days >= 0).then(|| days as u64 * 86_400)
}

pub(super) fn relative_from_iso(date: &str, now_secs: u64) -> Option<String> {
    iso_to_epoch_secs(date).map(|secs| vault::relative_time(secs, now_secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Matches `vault.rs`'s own known values for the inverse function
    /// (`civil_from_days`), proving this module's copy agrees with it.
    #[test]
    fn days_from_civil_known_values() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2021, 1, 1), 18_628);
    }

    #[test]
    fn weekday_abbrev_matches_known_dates() {
        // 2026-06-18 is a Thursday.
        assert_eq!(weekday_abbrev("2026-06-18"), Some("Thu"));
        // 1970-01-01 is a Thursday too (day 0 itself).
        assert_eq!(weekday_abbrev("1970-01-01"), Some("Thu"));
        assert_eq!(weekday_abbrev("not a date"), None);
    }

    #[test]
    fn same_week_covers_monday_through_sunday_only() {
        // 2026-06-18 is a Thursday; that week runs Mon 2026-06-15 .. Sun 06-21.
        let today = "2026-06-18";
        assert!(same_week("2026-06-15", today));
        assert!(same_week("2026-06-21", today));
        assert!(!same_week("2026-06-14", today));
        assert!(!same_week("2026-06-22", today));
        assert!(!same_week("not a date", today));
    }

    #[test]
    fn short_date_formats_month_and_day() {
        assert_eq!(short_date("2026-07-12"), "Jul 12");
        assert_eq!(short_date("garbage"), "garbage");
    }

    #[test]
    fn conversion_line_only_appends_stages_that_are_reached() {
        assert_eq!(
            conversion_line(&PresetConversion {
                preset: "Infra-heavy".into(),
                sent: 1,
                interviews: 1,
                offers: 0,
            }),
            "Infra-heavy — 1 sent → 1 interview"
        );
        assert_eq!(
            conversion_line(&PresetConversion {
                preset: "FAANG · concise".into(),
                sent: 4,
                interviews: 1,
                offers: 1,
            }),
            "FAANG · concise — 4 sent → 1 interview → 1 offer"
        );
    }

    #[test]
    fn next_step_caption_formats_with_and_without_a_time() {
        assert_eq!(
            next_step_caption(&NextStep {
                label: "Onsite".into(),
                date: "2026-06-18".into(),
                time: "14:00".into(),
            }),
            "Onsite · Thu 14:00"
        );
        assert_eq!(
            next_step_caption(&NextStep {
                label: "Take-home due".into(),
                date: "2026-06-19".into(),
                time: String::new(),
            }),
            "Take-home due Fri"
        );
    }

    #[test]
    fn matches_query_checks_company_and_role() {
        let app = Application {
            company: "Bramble Tech".into(),
            role: "Senior SWE".into(),
            ..Default::default()
        };
        assert!(matches_query(&app, ""));
        assert!(matches_query(&app, "bramble"));
        assert!(matches_query(&app, "senior"));
        assert!(!matches_query(&app, "meridian"));
    }
}

/// Which of the Applications screen's three surfaces is showing.
///
/// The mockup draws a two-way `Board`/`List` pill and never draws List's
/// layout (design doc §10), which is why List shipped as an inert pill. Both
/// halves are real now, and Insights is a third — the funnel is a question the
/// board cannot answer no matter how it is arranged, because it is about the
/// cards that have already left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ApplicationsView {
    #[default]
    Board,
    List,
    Insights,
}

impl ApplicationsView {
    pub(super) const ALL: [ApplicationsView; 3] = [
        ApplicationsView::Board,
        ApplicationsView::List,
        ApplicationsView::Insights,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            ApplicationsView::Board => "Board",
            ApplicationsView::List => "List",
            ApplicationsView::Insights => "Insights",
        }
    }
}

// --- sorting -------------------------------------------------------------

/// How the board's columns and the list are ordered.
///
/// One setting drives both surfaces on purpose: switching Board→List must not
/// silently re-order the same rows, or the two views stop being two views of
/// one thing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ApplicationSort {
    /// Most recently created first — the order the board had before sorting
    /// existed, reversed. Default because a tracker is mostly about what you
    /// did lately.
    #[default]
    Newest,
    /// Oldest first: the literal file order, and what you want when working
    /// through a backlog.
    Oldest,
    /// Alphabetical by company, then role.
    Company,
    /// Furthest through the pipeline first — offers at the top.
    Stage,
    /// Longest since anything happened. The one that finds forgotten cards.
    Stale,
    /// Most recently *sent* first. Distinct from `Newest`, which is the day a
    /// card was created: a company you added in January and applied to in June
    /// is a June application, and the List's `Applied` column shows that date,
    /// so the header has to sort by it or the caret is a lie. Entries never
    /// sent sort last — they have no send date, not an early one.
    Applied,
}

impl ApplicationSort {
    pub(super) const ALL: [ApplicationSort; 6] = [
        ApplicationSort::Newest,
        ApplicationSort::Oldest,
        ApplicationSort::Applied,
        ApplicationSort::Company,
        ApplicationSort::Stage,
        ApplicationSort::Stale,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            ApplicationSort::Newest => "Newest first",
            ApplicationSort::Oldest => "Oldest first",
            ApplicationSort::Company => "Company A–Z",
            ApplicationSort::Stage => "Furthest stage",
            ApplicationSort::Stale => "Least recently touched",
            ApplicationSort::Applied => "Most recently sent",
        }
    }

    /// The short form the toolbar control shows once something is chosen.
    pub(super) fn short_label(self) -> &'static str {
        match self {
            ApplicationSort::Newest => "Newest",
            ApplicationSort::Oldest => "Oldest",
            ApplicationSort::Company => "Company",
            ApplicationSort::Stage => "Stage",
            ApplicationSort::Stale => "Stale",
            ApplicationSort::Applied => "Sent",
        }
    }
}

/// How far through the pipeline an entry has ever been, as a number that can
/// be ordered. Mirrors `ApplicationStatus::depth` — which is private to the
/// model, deliberately, because the enum's declaration order is pinned by
/// serde and is not this order.
fn stage_rank(app: &Application) -> u8 {
    match app.furthest {
        ApplicationStatus::Offer => 3,
        ApplicationStatus::Interviewing => 2,
        ApplicationStatus::Applied => 1,
        // Rejected has no pipeline depth (it is an outcome, not a stage), and
        // an entry still on the wishlist has not started one.
        ApplicationStatus::Rejected | ApplicationStatus::Wishlist => 0,
    }
}

/// The last date anything is known to have happened to an entry: the next
/// step if one is scheduled, else the last snapshot, else the send date, else
/// the day it was created. `""` sorts first, which is what "nothing is known
/// about this one" should do under *Least recently touched*.
fn last_touched(app: &Application) -> &str {
    if let Some(step) = app.next_step.as_ref() {
        if !step.date.is_empty() {
            return &step.date;
        }
    }
    if let Some(snapshot) = app.snapshots.last() {
        if !snapshot.date.is_empty() {
            return &snapshot.date;
        }
    }
    if let Some(applied) = app.applied.as_deref() {
        if !applied.is_empty() {
            return applied;
        }
    }
    &app.created
}

/// Order `rows` in place. Rows are `(index, application)` pairs, where the
/// index is the entry's position in `Applications::entries` — the identity
/// every action on the board addresses, so it must survive re-ordering.
///
/// Every comparison ends by falling back to that index, which makes the sort
/// **total**: two entries that tie on the chosen key keep a stable, defined
/// order rather than one the sort algorithm happens to produce. Without it a
/// re-render could shuffle equal rows under the cursor.
pub(super) fn sort_rows(rows: &mut [(usize, Application)], sort: ApplicationSort) {
    match sort {
        // ISO dates sort correctly as strings, which is half of why the vault
        // writes them. Reversed for Newest so a missing date lands last
        // rather than first.
        ApplicationSort::Newest => {
            rows.sort_by(|(ai, a), (bi, b)| b.created.cmp(&a.created).then(bi.cmp(ai)))
        }
        ApplicationSort::Oldest => {
            rows.sort_by(|(ai, a), (bi, b)| a.created.cmp(&b.created).then(ai.cmp(bi)))
        }
        ApplicationSort::Company => rows.sort_by(|(ai, a), (bi, b)| {
            a.company
                .to_lowercase()
                .cmp(&b.company.to_lowercase())
                .then_with(|| a.role.to_lowercase().cmp(&b.role.to_lowercase()))
                .then(ai.cmp(bi))
        }),
        ApplicationSort::Stage => rows.sort_by(|(ai, a), (bi, b)| {
            stage_rank(b)
                .cmp(&stage_rank(a))
                .then_with(|| b.created.cmp(&a.created))
                .then(bi.cmp(ai))
        }),
        ApplicationSort::Stale => rows.sort_by(|(ai, a), (bi, b)| {
            last_touched(a).cmp(last_touched(b)).then(ai.cmp(bi))
        }),
        // An unsent entry has no send date. Sorting it as `""` would put it
        // first under a descending sort, which reads as "sent longest ago" —
        // the opposite of the truth — so absence is pushed to the end.
        ApplicationSort::Applied => rows.sort_by(|(ai, a), (bi, b)| {
            let sent = |app: &Application| {
                app.applied
                    .as_deref()
                    .filter(|d| !d.is_empty())
                    .map(str::to_string)
            };
            match (sent(a), sent(b)) {
                (Some(a), Some(b)) => b.cmp(&a),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
            .then(bi.cmp(ai))
        }),
    }
}

#[cfg(test)]
mod sort_tests {
    use super::*;
    use crate::resume::model::{Application, ApplicationStatus, NextStep};

    fn app(company: &str, created: &str, furthest: ApplicationStatus) -> Application {
        Application {
            company: company.into(),
            created: created.into(),
            furthest,
            ..Default::default()
        }
    }

    fn rows() -> Vec<(usize, Application)> {
        vec![
            (0, app("Bramble Tech", "2026-03-01", ApplicationStatus::Applied)),
            (1, app("acme", "2026-05-20", ApplicationStatus::Offer)),
            (2, app("Cardinal", "2026-01-09", ApplicationStatus::Interviewing)),
        ]
    }

    fn order(sort: ApplicationSort) -> Vec<usize> {
        let mut r = rows();
        sort_rows(&mut r, sort);
        r.into_iter().map(|(i, _)| i).collect()
    }

    #[test]
    fn each_sort_orders_by_the_thing_it_names() {
        assert_eq!(order(ApplicationSort::Newest), vec![1, 0, 2]);
        assert_eq!(order(ApplicationSort::Oldest), vec![2, 0, 1]);
        // Case-insensitive, or "acme" sorts after every capitalised company.
        assert_eq!(order(ApplicationSort::Company), vec![1, 0, 2]);
        assert_eq!(order(ApplicationSort::Stage), vec![1, 2, 0]);
    }

    /// The index is the identity every board action addresses. Sorting must
    /// re-order the rows and carry each index with its own entry.
    /// The List's `Applied` column shows the send date, so its header has to
    /// sort by the send date — not by the day the card was created, which is a
    /// different date and was what the caret used to point at.
    #[test]
    fn most_recently_sent_orders_by_the_send_date_and_puts_unsent_last() {
        let sent = |company: &str, created: &str, applied: Option<&str>| Application {
            company: company.into(),
            created: created.into(),
            applied: applied.map(str::to_string),
            ..Default::default()
        };
        // Created oldest, sent newest — the two orders disagree on purpose.
        let mut rows = vec![
            (0, sent("Early add, late send", "2026-01-01", Some("2026-06-01"))),
            (1, sent("Late add, early send", "2026-05-01", Some("2026-05-02"))),
            (2, sent("Never sent", "2026-04-01", None)),
        ];
        sort_rows(&mut rows, ApplicationSort::Applied);
        assert_eq!(
            rows.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![0, 1, 2],
            "newest send first, and the unsent one last"
        );

        // …and Newest, over the same rows, disagrees — which is the point.
        sort_rows(&mut rows, ApplicationSort::Newest);
        assert_eq!(rows[0].0, 1, "Newest is by creation, not by send");
    }

    #[test]
    fn sorting_carries_each_entry_index_with_it() {
        let mut r = rows();
        sort_rows(&mut r, ApplicationSort::Company);
        for (index, app) in &r {
            assert_eq!(&rows()[*index].1.company, &app.company);
        }
    }

    /// Ties must not shuffle between renders — the board re-sorts on every
    /// frame, and a cursor over a card would land somewhere else.
    #[test]
    fn ties_keep_a_defined_order() {
        let tied: Vec<(usize, Application)> = (0..4)
            .map(|i| (i, app("Same Co", "2026-04-04", ApplicationStatus::Applied)))
            .collect();
        for sort in ApplicationSort::ALL {
            let mut a = tied.clone();
            let mut b = tied.clone();
            b.reverse();
            sort_rows(&mut a, sort);
            sort_rows(&mut b, sort);
            let ai: Vec<usize> = a.iter().map(|(i, _)| *i).collect();
            let bi: Vec<usize> = b.iter().map(|(i, _)| *i).collect();
            assert_eq!(ai, bi, "{sort:?} is not a total order");
        }
    }

    /// `last_touched` walks a specific ladder — a scheduled next step beats a
    /// snapshot beats the send date beats the creation date — because those
    /// are in order of how recently a human looked at the card.
    #[test]
    fn least_recently_touched_prefers_the_most_specific_date_it_has() {
        let mut bare = app("A", "2026-01-01", ApplicationStatus::Applied);
        assert_eq!(last_touched(&bare), "2026-01-01");

        bare.applied = Some("2026-02-02".into());
        assert_eq!(last_touched(&bare), "2026-02-02");

        bare.next_step = Some(NextStep {
            label: "Onsite".into(),
            date: "2026-06-06".into(),
            time: String::new(),
        });
        assert_eq!(last_touched(&bare), "2026-06-06");

        // Stale sorts the untouched one first.
        let mut r = vec![(0, bare), (1, app("B", "2020-01-01", ApplicationStatus::Wishlist))];
        sort_rows(&mut r, ApplicationSort::Stale);
        assert_eq!(r[0].0, 1);
    }
}

/// A stage's printed name.
///
/// One definition, because there were three: two board-order tables and a
/// private helper in the list, each free to disagree with the others about
/// what a column is called. `const` so the order tables can still be consts.
pub(super) const fn status_title(status: ApplicationStatus) -> &'static str {
    match status {
        ApplicationStatus::Wishlist => "Wishlist",
        ApplicationStatus::Applied => "Applied",
        ApplicationStatus::Interviewing => "Interviewing",
        ApplicationStatus::Offer => "Offer",
        ApplicationStatus::Rejected => "Rejected",
    }
}
