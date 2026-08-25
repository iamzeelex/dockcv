//! Start and end dates, chosen from a month and a year.
//!
//! **Not a calendar.** A `DatePicker` can only yield a full `NaiveDate` —
//! upstream's month view navigates rather than selects — so every pick carried
//! a day the author never wrote and had to be truncated on the way in. Two
//! selectors state exactly the precision a CV states: `Aug 2024`, never
//! `15 Aug 2024`. Nothing is discarded because nothing extra is collected.
//!
//! **The end cannot be before the start.** It is enforced where it cannot be
//! got around: the end's own menus offer only years from the start's year on,
//! and inside that year only months from the start's month on. There is no
//! invalid choice to make and therefore no error to report.
//!
//! An empty end is not a gap. The renderer prints `start – Present` when the
//! end is blank, which is what "still there" means on a résumé.

use gpui::prelude::*;
use gpui::{div, px, Context, IntoElement, SharedString, Window};

use dockcv_ui_components::{
    Button, ButtonExt, DropdownMenu, Field, PopupMenuItem,
};

use crate::resume::edit::FieldId;
use crate::theme::{ActiveTheme, StyledText, TextStyle};

use super::Root;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The earliest year a working life plausibly starts from.
const FIRST_YEAR: i32 = 1977;

/// This year, read from the clock rather than written down. A constant here
/// would quietly stop offering the current year the moment it passed — the kind
/// of expiry nothing reports.
fn last_year() -> i32 {
    use chrono::Datelike;
    chrono::Local::now().year()
}

/// `Aug 2024` — the precision a résumé states.
fn format_date(year: i32, month: u32) -> String {
    format!("{} {year}", MONTHS[(month.clamp(1, 12) - 1) as usize])
}

/// What a field holds, if it is a month and a year.
///
/// A value the selectors cannot express — `Present`, a template's `20XX`, a
/// bare year — reads as `None`. The control then shows its placeholder **and**
/// the stored text beside it, so nothing is hidden and nothing is rewritten
/// until the user picks.
fn current(text: &str) -> Option<(i32, u32)> {
    let parsed = crate::resume::dates::ResumeDate::new(text).parse()?;
    Some((parsed.year, parsed.month?))
}

/// The first month the end may take in `year`, given the start.
fn first_month_in(year: i32, floor: Option<(i32, u32)>) -> u32 {
    match floor {
        Some((fy, fm)) if year == fy => fm,
        _ => 1,
    }
}

impl Root {
    /// The two date controls of one entry.
    pub(super) fn date_fields(
        &self,
        cx: &mut Context<Self>,
        start: FieldId,
        end: FieldId,
    ) -> [Field; 2] {
        let floor = start.get(&self.doc).and_then(|t| current(t));
        [
            self.month_year_field(cx, start, "Start", None),
            // `None` when the start is not a month and a year — there is
            // nothing for the end to be after.
            self.month_year_field(cx, end, "End", floor),
        ]
    }

    /// One date with no counterpart — a certificate is issued on a date, it
    /// does not run between two. Same control, no floor: there is nothing for
    /// it to be after.
    pub(super) fn single_date_field(
        &self,
        cx: &mut Context<Self>,
        field: FieldId,
        label: &'static str,
    ) -> Field {
        self.month_year_field(cx, field, label, None)
    }

    fn month_year_field(
        &self,
        cx: &mut Context<Self>,
        field: FieldId,
        label: &'static str,
        floor: Option<(i32, u32)>,
    ) -> Field {
        let theme = *cx.theme();
        let text = field.get(&self.doc).cloned().unwrap_or_default();
        let value = current(&text);
        let label: SharedString = label.into();
        let leftover = (value.is_none() && !text.trim().is_empty()).then(|| text.clone());

        Field::new()
            .col_span(1)
            .label_fn(move |_window, _cx| {
                div()
                    .text_style(TextStyle::label())
                    .text_color(theme.text_muted)
                    .child(label.clone())
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            // Each menu takes half the field, so the pair fills
                            // the same box a text input would and the two
                            // columns of the form still line up.
                            .child(div().flex_1().min_w_0().child(
                                self.month_menu(cx, field, value, floor),
                            ))
                            .child(div().flex_1().min_w_0().child(
                                self.year_menu(cx, field, value, floor),
                            )),
                    )
                    // What is stored but not expressible stays on screen rather
                    // than being replaced by a placeholder that says "empty".
                    .children(leftover.map(|text| {
                        div()
                            .text_style(TextStyle::meta())
                            .text_color(cx.theme().text_subtle)
                            .child(text)
                    })),
            )
    }

    fn month_menu(
        &self,
        cx: &mut Context<Self>,
        field: FieldId,
        value: Option<(i32, u32)>,
        floor: Option<(i32, u32)>,
    ) -> impl IntoElement {
        let root = cx.weak_entity();
        let year = value.map_or(floor.map_or_else(last_year, |(y, _)| y), |(y, _)| y);
        let first = first_month_in(year, floor);

        Button::new(SharedString::from(format!("month-{field:?}")))
            .selector()
            .w_full()
            // Full width because a date selector is a field, and a field
            // fills its column. Everything else about the box — fill, hairline,
            // radius, height — comes from `selector()`, which is what makes it
            // agree with the `TextField` sitting beside it in the same row.
            .label(value.map_or("Month".to_string(), |(_, m)| {
                MONTHS[(m - 1) as usize].to_string()
            }))
            .dropdown_menu(move |mut menu, _window, _cx| {
                for (idx, name) in MONTHS.iter().enumerate() {
                    let month = idx as u32 + 1;
                    if month < first {
                        continue;
                    }
                    let root = root.clone();
                    menu = menu.item(PopupMenuItem::new(*name).on_click(
                        move |_ev, window, cx| {
                            let _ = root.update(cx, |this, cx| {
                                this.write_date(field, year, month, window, cx);
                            });
                        },
                    ));
                }
                menu
            })
    }

    fn year_menu(
        &self,
        cx: &mut Context<Self>,
        field: FieldId,
        value: Option<(i32, u32)>,
        floor: Option<(i32, u32)>,
    ) -> impl IntoElement {
        let root = cx.weak_entity();
        let first_year = floor.map_or(FIRST_YEAR, |(y, _)| y);

        Button::new(SharedString::from(format!("year-{field:?}")))
            .selector()
            .w_full()
            .label(value.map_or("Year".to_string(), |(y, _)| y.to_string()))
            .dropdown_menu(move |mut menu, _window, _cx| {
                // Newest first: a CV is written from the present backwards.
                for year in (first_year..=last_year()).rev() {
                    let root = root.clone();
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(year.to_string())).on_click(
                            move |_ev, window, cx| {
                                let _ = root.update(cx, |this, cx| {
                                    // Moving to the start's own year can put the
                                    // month behind the floor; raise the month
                                    // rather than refuse the year.
                                    let month = value.map_or(1, |(_, m)| m);
                                    let month = month.max(first_month_in(year, floor));
                                    this.write_date(field, year, month, window, cx);
                                });
                            },
                        ),
                    );
                }
                menu
            })
    }

    /// Write a chosen month and year **through** the addressing layer — see
    /// E-42: reaching past it is how a field ends up in the model and nowhere
    /// else.
    fn write_date(
        &mut self,
        field: FieldId,
        year: i32,
        month: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(slot) = field.get_mut(&mut self.doc) {
            *slot = format_date(year, month);
        }
        // **Not** `fields_stale`. That flag means "the set of fields changed",
        // and it makes `sync_fields` drop and rebuild every `TextFieldState` in
        // the document — every input in the panel re-measures, which is the
        // flicker and the jump. A date is not a text field any more, so writing
        // one changes a value and nothing structural: the menus read it back
        // from the model on the next render.
        self.schedule_save(cx);
        cx.notify();
        self.schedule_recompile(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{current, first_month_in, format_date, last_year, FIRST_YEAR};

    /// The control's precision is the résumé's precision — a month and a year,
    /// never a day. Nothing is truncated because nothing extra is collected.
    #[test]
    fn a_choice_is_written_as_a_month_and_a_year() {
        assert_eq!(format_date(2024, 8), "Aug 2024");
        assert_eq!(format_date(2019, 1), "Jan 2019");
        assert_eq!(format_date(2021, 12), "Dec 2021");
    }

    /// What the control writes must be readable by the parser that formats it,
    /// or a chosen date would print literally and ignore the document's own
    /// date format — the same failure as E-32, one layer down.
    #[test]
    fn what_the_control_writes_the_model_can_parse() {
        use crate::resume::dates::{DateFormat, ResumeDate};

        let date = ResumeDate::new(format_date(2024, 8));
        let parsed = date.parse().expect("its own output must parse");
        assert_eq!((parsed.year, parsed.month, parsed.day), (2024, Some(8), None));
        assert_eq!(date.display(DateFormat::Iso), "2024-08");
    }

    /// A value the selectors cannot express reads as absent, so the control
    /// shows its placeholder — and the stored text is displayed beside it
    /// rather than silently replaced.
    #[test]
    fn a_value_the_selectors_cannot_express_reads_as_absent() {
        assert_eq!(current("Aug 2024"), Some((2024, 8)));
        assert_eq!(current(""), None);
        assert_eq!(current("Present"), None);
        assert_eq!(current("June 20XX"), None);
        // A bare year has no month, and this control always states one.
        assert_eq!(current("2019"), None);
    }

    /// The range is read from the clock, not written down — a constant would
    /// stop offering the current year the moment it passed.
    #[test]
    fn the_year_range_reaches_from_1977_to_this_year() {
        assert_eq!(FIRST_YEAR, 1977);
        assert!(
            last_year() >= 2026,
            "the top of the range must track the clock, got {}",
            last_year()
        );
        assert!(last_year() > FIRST_YEAR, "the range must not be empty");
    }

    /// The rule is enforced by what the menus contain, not by a check after
    /// the fact: with a start in `Aug 2024` the end offers 2024 onwards, and
    /// inside 2024 only August onwards. There is no invalid choice to make.
    #[test]
    fn the_end_is_never_offered_a_date_before_the_start() {
        let floor = Some((2024_i32, 8_u32));

        assert_eq!(
            floor.map_or(FIRST_YEAR, |(y, _)| y),
            2024,
            "earlier years are not in the menu at all"
        );
        assert!(last_year() >= 2024, "the year range must not be empty");

        assert_eq!(first_month_in(2024, floor), 8, "the start's own year is floored");
        assert_eq!(first_month_in(2025, floor), 1, "a later year is unconstrained");
        assert_eq!(first_month_in(2024, None), 1, "no start means no floor");
    }

    /// Choosing the start's year for an end that was later must not silently
    /// produce an earlier date; the month is raised to the floor instead.
    #[test]
    fn moving_the_end_back_to_the_starts_year_raises_its_month() {
        let floor = Some((2024_i32, 8_u32));
        // End was Mar 2026; the user picks 2024.
        let month = 3_u32.max(first_month_in(2024, floor));
        assert_eq!(format_date(2024, month), "Aug 2024");
    }
}
