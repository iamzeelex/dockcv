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
        .filter(|a| a.status == ApplicationStatus::Interviewing)
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
