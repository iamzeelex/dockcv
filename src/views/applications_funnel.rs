//! The funnel behind the Insights view: which CV was sent, and how far it got.
//!
//! Pure — no `gpui`, no `Theme`. The chart in `applications_analytics.rs`
//! draws whatever this returns, so the part that can be wrong about the user's
//! history is the part that can be tested.
//!
//! ## The counting rule
//!
//! Every **sent** application contributes exactly one unit of flow, from the
//! preset it was sent with to the furthest [`Outcome`] it reached. One link per
//! application, so nothing is counted twice and the widths add up to the number
//! of applications — which is the only way a Sankey can be read at a glance
//! without a legend explaining what it is not saying.
//!
//! Two consequences worth stating, because both are judgement calls:
//!
//! * **An application still on the wishlist is not in the diagram at all.**
//!   Nothing was sent, so there is no CV to attribute and no outcome to
//!   report. The Insights header says how many are excluded rather than
//!   letting the diagram quietly disagree with the board's counts.
//! * **Reaching an interview is attributed to the CV even if the answer was
//!   later no.** The question this diagram exists to answer is *which CV gets
//!   me interviews*, and a rejection after an onsite does not un-earn the
//!   onsite. Rejections that never got that far are their own outcome, and the
//!   stat tiles carry the total either way.

use std::collections::BTreeMap;

use crate::resume::model::{Application, ApplicationStatus, Applications, Closure};

/// What became of an application that was actually sent.
///
/// Ordered as the diagram reads, best first — the order the node column is
/// built in, so a good outcome sits at the top rather than wherever a hash map
/// happened to put it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Outcome {
    /// Reached an offer.
    Offer,
    /// Reached at least one interview, whatever happened afterwards.
    Interviewed,
    /// Sent, and the answer was no, without an interview.
    Rejected,
    /// Sent, no answer yet.
    Awaiting,
}

impl Outcome {
    pub(super) const ALL: [Outcome; 4] = [
        Outcome::Offer,
        Outcome::Interviewed,
        Outcome::Rejected,
        Outcome::Awaiting,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Outcome::Offer => "Offer",
            Outcome::Interviewed => "Interviewed",
            Outcome::Rejected => "Rejected",
            Outcome::Awaiting => "Awaiting reply",
        }
    }

    /// Where an application ended up, or `None` if it was never sent.
    ///
    /// Reads `furthest` rather than `status` for the pipeline stages: `status`
    /// is the column the card sits in now, and a card that interviewed and was
    /// then rejected sits in Rejected while having reached Interviewing.
    /// `Applications::normalize` guarantees `furthest` is at least `status`
    /// for every entry, including hand-edited files.
    pub(super) fn of(app: &Application) -> Option<Self> {
        let reached_interview = matches!(
            app.furthest,
            ApplicationStatus::Interviewing | ApplicationStatus::Offer
        );
        match app.furthest {
            // Never sent: on the wishlist, and `furthest` agrees.
            ApplicationStatus::Wishlist => None,
            ApplicationStatus::Offer => Some(Outcome::Offer),
            _ if reached_interview => Some(Outcome::Interviewed),
            _ if app.status() == ApplicationStatus::Rejected => Some(Outcome::Rejected),
            ApplicationStatus::Applied => Some(Outcome::Awaiting),
            // `furthest` should never be Rejected — it has no pipeline depth,
            // so nothing in the model assigns it — but a hand-edited file can
            // say so. Treat it as what it plainly means.
            ApplicationStatus::Rejected => Some(Outcome::Rejected),
            ApplicationStatus::Interviewing => Some(Outcome::Interviewed),
        }
    }
}

/// The label for applications sent without a named preset.
///
/// Not blank and not hidden: sending a CV without recording which cut it was
/// is a real and common state, and a diagram that dropped those rows would
/// disagree with the board's own numbers.
pub(super) const NO_PRESET: &str = "No preset recorded";

/// One flow: `count` applications sent with `preset` ended at `outcome`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Flow {
    pub preset: String,
    pub outcome: Outcome,
    pub count: usize,
}

/// The whole diagram's input, plus the numbers the header has to state so the
/// diagram and the board never appear to disagree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Funnel {
    /// Preset names, in the order their columns should appear: most sent
    /// first, ties alphabetical, `NO_PRESET` always last so a bookkeeping gap
    /// never leads the chart.
    pub presets: Vec<String>,
    /// Outcomes that actually occur, in [`Outcome::ALL`] order. An outcome
    /// nobody reached draws no node — an empty "Offer" node hanging off the
    /// right edge reads as a bug, not as a zero.
    pub outcomes: Vec<Outcome>,
    /// One entry per (preset, outcome) pair that has anything in it.
    pub flows: Vec<Flow>,
    /// Applications that were sent — the total the flows sum to.
    pub sent: usize,
    /// Applications still on the wishlist, and so absent from the diagram.
    pub not_sent: usize,
}

impl Funnel {
    pub(super) fn of(applications: &Applications) -> Self {
        let mut counts: BTreeMap<(String, Outcome), usize> = BTreeMap::new();
        let mut per_preset: BTreeMap<String, usize> = BTreeMap::new();
        let mut sent = 0usize;
        let mut not_sent = 0usize;

        for app in &applications.entries {
            let Some(outcome) = Outcome::of(app) else {
                not_sent += 1;
                continue;
            };
            sent += 1;
            let preset = if app.preset.trim().is_empty() {
                NO_PRESET.to_string()
            } else {
                app.preset.trim().to_string()
            };
            *per_preset.entry(preset.clone()).or_default() += 1;
            *counts.entry((preset, outcome)).or_default() += 1;
        }

        // Busiest CV first: the one you have leaned on hardest is the one you
        // are asking about. `NO_PRESET` is forced last regardless of volume.
        let mut presets: Vec<String> = per_preset.keys().cloned().collect();
        presets.sort_by(|a, b| {
            let a_last = a == NO_PRESET;
            let b_last = b == NO_PRESET;
            a_last
                .cmp(&b_last)
                .then_with(|| per_preset[b].cmp(&per_preset[a]))
                .then_with(|| a.cmp(b))
        });

        let outcomes: Vec<Outcome> = Outcome::ALL
            .into_iter()
            .filter(|outcome| counts.keys().any(|(_, o)| o == outcome))
            .collect();

        // Emitted in (preset, outcome) display order so the ribbons enter each
        // node in a stable sequence and the picture does not reshuffle between
        // renders of identical data.
        let mut flows = Vec::new();
        for preset in &presets {
            for outcome in &outcomes {
                if let Some(&count) = counts.get(&(preset.clone(), *outcome)) {
                    flows.push(Flow {
                        preset: preset.clone(),
                        outcome: *outcome,
                        count,
                    });
                }
            }
        }

        Self {
            presets,
            outcomes,
            flows,
            sent,
            not_sent,
        }
    }

    /// How many sent applications reached each outcome.
    pub(super) fn total(&self, outcome: Outcome) -> usize {
        self.flows
            .iter()
            .filter(|f| f.outcome == outcome)
            .map(|f| f.count)
            .sum()
    }

    /// Interview rate over sent applications, as a percentage, or `None` when
    /// nothing has been sent — a rate over zero is not 0%, it is unknown, and
    /// printing `0%` there would be inventing a metric (US-14).
    pub(super) fn interview_rate(&self) -> Option<f32> {
        (self.sent > 0).then(|| {
            let reached = self.total(Outcome::Interviewed) + self.total(Outcome::Offer);
            reached as f32 * 100.0 / self.sent as f32
        })
    }

    /// Nothing to draw: no application has been sent yet.
    pub(super) fn is_empty(&self) -> bool {
        self.flows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::Application;

    fn app(preset: &str, status: ApplicationStatus, furthest: ApplicationStatus) -> Application {
        Application {
            company: "Acme".into(),
            preset: preset.into(),
            status_word: status.word().into(),
            furthest,
            ..Default::default()
        }
    }

    fn board(entries: Vec<Application>) -> Applications {
        let mut applications = Applications { entries };
        applications.normalize();
        applications
    }

    /// The rule the whole diagram rests on: one application, one unit of flow.
    #[test]
    fn every_sent_application_contributes_exactly_one_unit() {
        let funnel = Funnel::of(&board(vec![
            app("FAANG", ApplicationStatus::Applied, ApplicationStatus::Applied),
            app("FAANG", ApplicationStatus::Rejected, ApplicationStatus::Applied),
            app("FAANG", ApplicationStatus::Offer, ApplicationStatus::Offer),
            app("Infra", ApplicationStatus::Interviewing, ApplicationStatus::Interviewing),
            app("Infra", ApplicationStatus::Wishlist, ApplicationStatus::Wishlist),
        ]));

        assert_eq!(funnel.sent, 4);
        assert_eq!(funnel.not_sent, 1);
        assert_eq!(
            funnel.flows.iter().map(|f| f.count).sum::<usize>(),
            funnel.sent,
            "the ribbons must add up to the applications they describe"
        );
    }

    /// A rejection after an interview still credits the CV with the interview.
    /// This is the judgement the module doc calls out, so it gets a test.
    #[test]
    fn an_interview_counts_even_when_the_answer_was_later_no() {
        let funnel = Funnel::of(&board(vec![
            // Interviewed, then rejected.
            app("FAANG", ApplicationStatus::Rejected, ApplicationStatus::Interviewing),
            // Rejected without ever interviewing.
            app("FAANG", ApplicationStatus::Rejected, ApplicationStatus::Applied),
        ]));

        assert_eq!(funnel.total(Outcome::Interviewed), 1);
        assert_eq!(funnel.total(Outcome::Rejected), 1);
    }

    /// Wishlist entries are absent from the diagram *and* accounted for, so
    /// the Insights header can say why its total is smaller than the board's.
    #[test]
    fn nothing_unsent_reaches_the_diagram() {
        let funnel = Funnel::of(&board(vec![app(
            "FAANG",
            ApplicationStatus::Wishlist,
            ApplicationStatus::Wishlist,
        )]));

        assert!(funnel.is_empty());
        assert_eq!(funnel.sent, 0);
        assert_eq!(funnel.not_sent, 1);
        assert_eq!(funnel.interview_rate(), None, "a rate over zero is unknown");
    }

    /// An application sent without a recorded preset is still an application.
    #[test]
    fn a_missing_preset_gets_a_column_of_its_own_and_it_is_last() {
        let funnel = Funnel::of(&board(vec![
            app("", ApplicationStatus::Applied, ApplicationStatus::Applied),
            app("", ApplicationStatus::Applied, ApplicationStatus::Applied),
            app("", ApplicationStatus::Applied, ApplicationStatus::Applied),
            app("FAANG", ApplicationStatus::Applied, ApplicationStatus::Applied),
        ]));

        // Busiest first would put the blank column first; it is pinned last.
        assert_eq!(funnel.presets, vec!["FAANG", NO_PRESET]);
        assert_eq!(funnel.sent, 4);
    }

    /// Busiest CV first, ties alphabetical — a stable order, so the diagram
    /// does not reshuffle between renders of the same data.
    #[test]
    fn presets_are_ordered_by_volume_then_name() {
        let funnel = Funnel::of(&board(vec![
            app("Zebra", ApplicationStatus::Applied, ApplicationStatus::Applied),
            app("Alpha", ApplicationStatus::Applied, ApplicationStatus::Applied),
            app("Busy", ApplicationStatus::Applied, ApplicationStatus::Applied),
            app("Busy", ApplicationStatus::Applied, ApplicationStatus::Applied),
        ]));
        assert_eq!(funnel.presets, vec!["Busy", "Alpha", "Zebra"]);
    }

    /// An outcome nobody reached must not draw a node — an empty "Offer"
    /// hanging off the right edge reads as a bug rather than as a zero.
    #[test]
    fn only_outcomes_that_happened_get_a_node() {
        let funnel = Funnel::of(&board(vec![app(
            "FAANG",
            ApplicationStatus::Applied,
            ApplicationStatus::Applied,
        )]));
        assert_eq!(funnel.outcomes, vec![Outcome::Awaiting]);
    }

    /// The chart addresses nodes by index, so the shape the drawing code
    /// builds has to be well formed for *every* funnel: presets first,
    /// outcomes after, every link strictly left-to-right. A link pointing
    /// backwards is a cycle, and a Sankey layout does not survive one.
    #[test]
    fn every_funnel_yields_a_well_formed_bipartite_graph() {
        let shapes = vec![
            // One of everything.
            vec![app("A", ApplicationStatus::Offer, ApplicationStatus::Offer)],
            // One preset, every outcome.
            vec![
                app("A", ApplicationStatus::Offer, ApplicationStatus::Offer),
                app("A", ApplicationStatus::Interviewing, ApplicationStatus::Interviewing),
                app("A", ApplicationStatus::Rejected, ApplicationStatus::Applied),
                app("A", ApplicationStatus::Applied, ApplicationStatus::Applied),
            ],
            // Several presets landing on one outcome.
            vec![
                app("A", ApplicationStatus::Applied, ApplicationStatus::Applied),
                app("B", ApplicationStatus::Applied, ApplicationStatus::Applied),
                app("", ApplicationStatus::Applied, ApplicationStatus::Applied),
            ],
        ];

        for entries in shapes {
            let funnel = Funnel::of(&board(entries));
            let outcome_base = funnel.presets.len();
            for flow in &funnel.flows {
                let source = funnel.presets.iter().position(|p| p == &flow.preset);
                let target = funnel.outcomes.iter().position(|o| *o == flow.outcome);
                let (source, target) = (source.expect("preset node"), outcome_base + target.expect("outcome node"));
                assert!(source < outcome_base, "a preset must be a source node");
                assert!(target >= outcome_base, "an outcome must be a target node");
                assert!(source < target, "links must run left to right");
                assert!(flow.count > 0, "a zero-width ribbon is not a flow");
            }
        }
    }

    #[test]
    fn the_interview_rate_counts_offers_as_interviews_too() {
        let funnel = Funnel::of(&board(vec![
            app("A", ApplicationStatus::Offer, ApplicationStatus::Offer),
            app("A", ApplicationStatus::Interviewing, ApplicationStatus::Interviewing),
            app("A", ApplicationStatus::Applied, ApplicationStatus::Applied),
            app("A", ApplicationStatus::Applied, ApplicationStatus::Applied),
        ]));
        assert_eq!(funnel.interview_rate(), Some(50.0));
    }
}

/// What one stage did over a window of time.
///
/// A cohort, not a snapshot: `entered` counts the moves *into* this stage that
/// happened inside the window, and `advanced` counts how many of exactly those
/// later reached something deeper. The onward move is counted **whenever it
/// happened**, inside the window or after it — a cohort judged only on what it
/// did before an arbitrary cut-off would make the most recent month look like
/// a disaster every time, which is an artefact of the question and not a fact
/// about the search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StageFlow {
    pub stage: ApplicationStatus,
    pub entered: usize,
    pub advanced: usize,
}

/// The stages a card passed through, oldest first, as (date, stage).
///
/// The wishlist is prepended from `created` rather than stored: a card starts
/// there, so recording it would be one fact in two places (see
/// `Application::history`).
fn timeline(app: &Application) -> Vec<(&str, ApplicationStatus)> {
    let mut steps: Vec<(&str, ApplicationStatus)> = Vec::with_capacity(app.history.len() + 1);
    if !app.created.is_empty() {
        steps.push((app.created.as_str(), ApplicationStatus::Wishlist));
    }
    for change in &app.history {
        if let Some(stage) = ApplicationStatus::from_word(&change.to) {
            steps.push((change.at.as_str(), stage));
        }
    }
    steps
}

/// Applications whose stage history is provably incomplete.
///
/// A card that has moved but recorded nothing predates the history field — it
/// is sitting somewhere past the wishlist with no account of how it got there.
/// A card still *on* the wishlist with no history has simply never moved,
/// which is not missing data. Distinguishing the two is what lets Insights
/// report a real gap instead of implying every quiet card is one.
pub(super) fn missing_history(applications: &Applications) -> usize {
    applications
        .entries
        .iter()
        .filter(|a| a.history.is_empty() && a.status() != ApplicationStatus::Wishlist)
        .count()
}

/// Movement through every stage between `from` and `to`, inclusive.
///
/// ISO dates compare lexicographically, which is the whole reason the vault
/// stores them that way.
pub(super) fn stage_flow(applications: &Applications, from: &str, to: &str) -> Vec<StageFlow> {
    const STAGES: [ApplicationStatus; 5] = [
        ApplicationStatus::Wishlist,
        ApplicationStatus::Applied,
        ApplicationStatus::Interviewing,
        ApplicationStatus::Offer,
        ApplicationStatus::Rejected,
    ];
    let mut entered = [0usize; 5];
    let mut advanced = [0usize; 5];

    for app in &applications.entries {
        let steps = timeline(app);
        for (i, (at, stage)) in steps.iter().enumerate() {
            if *at < from || *at > to {
                continue;
            }
            let Some(slot) = STAGES.iter().position(|s| s == stage) else {
                continue;
            };
            entered[slot] += 1;

            // Deeper *later*, by the model's own ordering. `Rejected` has no
            // depth, so an application cannot advance out of it and nothing
            // advances into it — a rejection is an outcome, not a stage on
            // the way somewhere.
            let here = stage.depth();
            let went_on = steps[i + 1..].iter().any(|(_, later)| {
                matches!((later.depth(), here), (Some(l), Some(h)) if l > h)
            });
            if went_on {
                advanced[slot] += 1;
            }
        }
    }

    STAGES
        .iter()
        .enumerate()
        .map(|(i, stage)| StageFlow {
            stage: *stage,
            entered: entered[i],
            advanced: advanced[i],
        })
        .collect()
}

#[cfg(test)]
mod stage_flow_tests {
    use super::*;

    fn app(created: &str, moves: &[(&str, ApplicationStatus)]) -> Application {
        let mut a = Application {
            created: created.into(),
            ..Default::default()
        };
        for (at, stage) in moves {
            a.advance_to(*stage, at);
        }
        a
    }

    fn apps(list: Vec<Application>) -> Applications {
        Applications { entries: list }
    }

    /// The window selects the move, not the card. A card created in June and
    /// interviewed in August contributes to June's wishlist and August's
    /// interviews, and to neither the other way round.
    #[test]
    fn a_move_is_counted_in_the_month_it_happened() {
        let board = apps(vec![app(
            "2026-06-01",
            &[
                ("2026-06-10", ApplicationStatus::Applied),
                ("2026-08-04", ApplicationStatus::Interviewing),
            ],
        )]);

        let june = stage_flow(&board, "2026-06-01", "2026-06-30");
        let by = |flows: &[StageFlow], s: ApplicationStatus| {
            flows.iter().find(|f| f.stage == s).unwrap().entered
        };
        assert_eq!(by(&june, ApplicationStatus::Wishlist), 1);
        assert_eq!(by(&june, ApplicationStatus::Applied), 1);
        assert_eq!(by(&june, ApplicationStatus::Interviewing), 0);

        let august = stage_flow(&board, "2026-08-01", "2026-08-31");
        assert_eq!(by(&august, ApplicationStatus::Wishlist), 0);
        assert_eq!(by(&august, ApplicationStatus::Applied), 0);
        assert_eq!(by(&august, ApplicationStatus::Interviewing), 1);
    }

    /// A cohort is judged on what it eventually did, not on what it had done
    /// by the edge of the window — otherwise the most recent month always
    /// reads as a catastrophe, which says more about the question than about
    /// the search.
    #[test]
    fn advancing_counts_even_when_it_happens_after_the_window() {
        let board = apps(vec![app(
            "2026-06-01",
            &[
                ("2026-06-10", ApplicationStatus::Applied),
                ("2026-08-04", ApplicationStatus::Interviewing),
            ],
        )]);
        let june = stage_flow(&board, "2026-06-01", "2026-06-30");
        let applied = june
            .iter()
            .find(|f| f.stage == ApplicationStatus::Applied)
            .unwrap();
        assert_eq!(applied.entered, 1);
        assert_eq!(
            applied.advanced, 1,
            "the August interview did not credit the June application"
        );
    }

    /// A rejection is an outcome, not a deeper stage. Nothing advances out of
    /// it, and reaching it is not advancing.
    #[test]
    fn a_rejection_is_not_progress() {
        let board = apps(vec![app(
            "2026-06-01",
            &[
                ("2026-06-10", ApplicationStatus::Applied),
                ("2026-06-20", ApplicationStatus::Rejected),
            ],
        )]);
        let flows = stage_flow(&board, "2026-06-01", "2026-06-30");
        let get = |s| flows.iter().find(|f| f.stage == s).unwrap();
        assert_eq!(get(ApplicationStatus::Applied).advanced, 0);
        assert_eq!(get(ApplicationStatus::Rejected).entered, 1);
        assert_eq!(get(ApplicationStatus::Rejected).advanced, 0);
    }

    /// Dropping a card back where it already was is not a move. Without this
    /// the counts measure how often the board was fidgeted with.
    #[test]
    fn re_dropping_a_card_in_its_own_column_records_nothing() {
        let mut a = app("2026-06-01", &[("2026-06-10", ApplicationStatus::Applied)]);
        assert_eq!(a.history.len(), 1);
        a.advance_to(ApplicationStatus::Applied, "2026-06-11");
        assert_eq!(a.history.len(), 1, "a no-op move was recorded as a move");

        // A real move back, however, happened and is recorded.
        a.advance_to(ApplicationStatus::Wishlist, "2026-06-12");
        assert_eq!(a.history.len(), 2);
    }

    /// Cards written before this field existed have to be *reported*, not
    /// quietly counted as zero. A card past the wishlist with no history is
    /// provably missing its account; a card still on the wishlist with none
    /// has simply never moved, and that is not a gap.
    #[test]
    fn only_cards_that_moved_without_recording_it_count_as_missing() {
        let board = apps(vec![
            Application {
                status_word: ApplicationStatus::Interviewing.word().into(),
                ..Default::default()
            },
            Application::default(),
            app("2026-06-01", &[("2026-06-10", ApplicationStatus::Applied)]),
        ]);
        assert_eq!(missing_history(&board), 1);
    }
}

/// How far back Insights looks.
///
/// Fixed windows rather than a date picker: the question is "how is the search
/// going", and nobody asks it about an arbitrary fortnight. Each one is
/// counted from today, so the answer moves with the calendar rather than going
/// stale the day after it was chosen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Period {
    #[default]
    AllTime,
    Days30,
    Days90,
    Days365,
}

impl Period {
    pub(super) const ALL: [Period; 4] = [
        Period::AllTime,
        Period::Days30,
        Period::Days90,
        Period::Days365,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Period::AllTime => "All time",
            Period::Days30 => "Last 30 days",
            Period::Days90 => "Last 90 days",
            Period::Days365 => "Last year",
        }
    }

    pub(super) fn days(self) -> Option<i64> {
        match self {
            Period::AllTime => None,
            Period::Days30 => Some(30),
            Period::Days90 => Some(90),
            Period::Days365 => Some(365),
        }
    }
}

/// A node in the journey diagram — the chart people actually share.
///
/// The preset funnel above answers *which CV works*, which is DockCV's own
/// question. This one answers *how is it going*, which is the one a person
/// screenshots and sends to a friend. Both are true about the same board and
/// neither replaces the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum JourneyNode {
    /// The left edge: everything actually sent.
    Sent,
    /// An interview, 1-based. Not a stage — see `InterviewRound`.
    Round(usize),
    Offer,
    /// Still live. Deliberately its own node rather than folded into an
    /// ending: an application nobody has answered yet is not a rejection and
    /// not a ghosting, and a diagram that pretends otherwise reads worse than
    /// the truth on a bad week.
    Live,
    Closed(Closure),
}

impl JourneyNode {
    pub(super) fn label(self) -> String {
        match self {
            JourneyNode::Sent => "Applications".to_string(),
            JourneyNode::Round(1) => "1st interview".to_string(),
            JourneyNode::Round(2) => "2nd interview".to_string(),
            JourneyNode::Round(3) => "3rd interview".to_string(),
            JourneyNode::Round(n) => format!("{n}th interview"),
            JourneyNode::Offer => "Offer".to_string(),
            JourneyNode::Live => "In progress".to_string(),
            JourneyNode::Closed(c) => c.label().to_string(),
        }
    }
}

/// The whole diagram: one path per sent application, summed into links.
pub(super) struct Journey {
    pub sent: usize,
    pub not_sent: usize,
    pub links: Vec<(JourneyNode, JourneyNode, usize)>,
}

impl Journey {
    pub(super) fn is_empty(&self) -> bool {
        self.links.is_empty()
    }
}

/// How many interviews an application has been through.
///
/// The recorded rounds, or one if the board says it reached `Interviewing`
/// without any being written down. That floor is not an invention: `furthest`
/// reaching Interviewing *means* an interview happened — the user said so by
/// moving the card. The list only refines a count the board already asserted,
/// which is what keeps this chart useful on a vault that predates rounds.
fn rounds_of(app: &Application) -> usize {
    let asserted = matches!(
        app.furthest,
        ApplicationStatus::Interviewing | ApplicationStatus::Offer
    );
    app.rounds.len().max(usize::from(asserted))
}

fn reached_offer(app: &Application) -> bool {
    app.furthest == ApplicationStatus::Offer
        || matches!(
            app.closed_as,
            // Only the two endings that can *only* follow an offer. A
            // withdrawal or a ghosting says nothing about whether one was
            // ever made.
            Some(Closure::Declined | Closure::Accepted)
        )
}

/// Every sent application, as one path from `Sent` to how it ended.
///
/// One unit of flow per application, exactly as the preset funnel counts —
/// so the widths add up to the number of applications and the diagram can be
/// read without a legend explaining what it is not saying.
pub(super) fn journey(applications: &Applications) -> Journey {
    let mut counts: BTreeMap<(JourneyNode, JourneyNode), usize> = BTreeMap::new();
    let mut sent = 0usize;
    let mut not_sent = 0usize;

    for app in &applications.entries {
        if Outcome::of(app).is_none() {
            not_sent += 1;
            continue;
        }
        sent += 1;

        let mut at = JourneyNode::Sent;
        let mut step = |from: JourneyNode, to: JourneyNode| {
            *counts.entry((from, to)).or_insert(0) += 1;
        };

        for n in 1..=rounds_of(app) {
            let next = JourneyNode::Round(n);
            step(at, next);
            at = next;
        }
        if reached_offer(app) {
            step(at, JourneyNode::Offer);
            at = JourneyNode::Offer;
        }
        step(
            at,
            match app.closed_as {
                Some(closure) => JourneyNode::Closed(closure),
                None => JourneyNode::Live,
            },
        );
    }

    Journey {
        sent,
        not_sent,
        links: counts.into_iter().map(|((a, b), n)| (a, b, n)).collect(),
    }
}

#[cfg(test)]
mod journey_tests {
    use super::*;
    use crate::resume::model::InterviewRound;

    fn sent(rounds: usize, furthest: ApplicationStatus, closed: Option<Closure>) -> Application {
        Application {
            status_word: furthest.word().into(),
            furthest,
            applied: Some("2026-06-01".into()),
            rounds: (0..rounds)
                .map(|i| InterviewRound {
                    at: format!("2026-06-{:02}", i + 2),
                    ..Default::default()
                })
                .collect(),
            closed_as: closed,
            ..Default::default()
        }
    }

    fn board(entries: Vec<Application>) -> Applications {
        Applications { entries }
    }

    fn link(j: &Journey, from: JourneyNode, to: JourneyNode) -> usize {
        j.links
            .iter()
            .find(|(a, b, _)| *a == from && *b == to)
            .map(|(_, _, n)| *n)
            .unwrap_or(0)
    }

    /// One application is one unit of flow the whole way across. If a path
    /// ever split or doubled, the widths would stop adding up to the number
    /// of applications and the diagram would need a paragraph explaining what
    /// it is not saying.
    #[test]
    fn every_application_walks_exactly_one_path() {
        let j = journey(&board(vec![sent(
            2,
            ApplicationStatus::Offer,
            Some(Closure::Accepted),
        )]));
        assert_eq!(j.sent, 1);
        assert_eq!(link(&j, JourneyNode::Sent, JourneyNode::Round(1)), 1);
        assert_eq!(link(&j, JourneyNode::Round(1), JourneyNode::Round(2)), 1);
        assert_eq!(link(&j, JourneyNode::Round(2), JourneyNode::Offer), 1);
        assert_eq!(
            link(&j, JourneyNode::Offer, JourneyNode::Closed(Closure::Accepted)),
            1
        );
        assert_eq!(j.links.len(), 4, "the path branched: {:?}", j.links);
    }

    /// Silence is its own ending, and an unanswered application is neither
    /// silence-forever nor a rejection until someone says so.
    #[test]
    fn an_unanswered_application_is_in_progress_not_ghosted() {
        let j = journey(&board(vec![sent(0, ApplicationStatus::Applied, None)]));
        assert_eq!(link(&j, JourneyNode::Sent, JourneyNode::Live), 1);
        assert_eq!(
            link(&j, JourneyNode::Sent, JourneyNode::Closed(Closure::Ghosted)),
            0
        );

        let j = journey(&board(vec![sent(
            0,
            ApplicationStatus::Applied,
            Some(Closure::Ghosted),
        )]));
        assert_eq!(
            link(&j, JourneyNode::Sent, JourneyNode::Closed(Closure::Ghosted)),
            1
        );
    }

    /// A board that predates recorded rounds still draws a first interview,
    /// because moving a card to Interviewing already asserted one happened.
    #[test]
    fn the_board_still_accounts_for_an_interview_it_never_wrote_down() {
        let j = journey(&board(vec![sent(0, ApplicationStatus::Interviewing, None)]));
        assert_eq!(link(&j, JourneyNode::Sent, JourneyNode::Round(1)), 1);
        assert_eq!(link(&j, JourneyNode::Round(1), JourneyNode::Live), 1);
        // …and it does not invent a second one.
        assert_eq!(link(&j, JourneyNode::Round(1), JourneyNode::Round(2)), 0);
    }

    /// The wishlist is not in the diagram — nothing was sent, so there is no
    /// path — but it is counted so the total can be said out loud.
    #[test]
    fn the_wishlist_is_excluded_and_reported() {
        let j = journey(&board(vec![
            Application::default(),
            sent(0, ApplicationStatus::Applied, Some(Closure::Rejected)),
        ]));
        assert_eq!(j.sent, 1);
        assert_eq!(j.not_sent, 1);
    }

    /// Turning an offer down is not a rejection, and the diagram has to route
    /// it through the offer that was actually made.
    #[test]
    fn declining_an_offer_still_goes_through_the_offer() {
        let j = journey(&board(vec![sent(
            1,
            ApplicationStatus::Interviewing,
            Some(Closure::Declined),
        )]));
        assert_eq!(link(&j, JourneyNode::Round(1), JourneyNode::Offer), 1);
        assert_eq!(
            link(&j, JourneyNode::Offer, JourneyNode::Closed(Closure::Declined)),
            1
        );
    }
}

#[cfg(test)]
mod closure_invariant_tests {
    use crate::resume::model::{Application, ApplicationStatus, Closure};

    /// The board and the diagram must not be able to disagree.
    ///
    /// They did: dragging a card to Rejected wrote the column and nothing
    /// else, so the board called it rejected while the journey diagram — which
    /// reads `closed_as` — still called it in progress. Both surfaces were
    /// confident and one of them was wrong.
    #[test]
    fn dragging_a_card_to_the_closed_column_is_an_ending() {
        let mut app = Application::default();
        app.advance_to(ApplicationStatus::Applied, "2026-06-01");
        assert_eq!(app.closed_as, None, "a sent application is not finished");

        app.advance_to(ApplicationStatus::Rejected, "2026-06-20");
        assert_eq!(app.closed_as, Some(Closure::Rejected));
    }

    /// …and refining *why* must not move the card somewhere else.
    #[test]
    fn refining_a_rejection_into_a_ghosting_leaves_it_where_it_is() {
        let mut app = Application::default();
        app.advance_to(ApplicationStatus::Rejected, "2026-06-20");
        app.close_as(Closure::Ghosted, "2026-06-21");
        assert_eq!(app.closed_as, Some(Closure::Ghosted));
        assert_eq!(app.status(), ApplicationStatus::Rejected);
    }

    /// An ending that follows an offer files the card with the offers. A
    /// search that ended in a yes should not sit under "rejected".
    #[test]
    fn accepting_and_declining_are_filed_with_the_offer() {
        for closure in [Closure::Accepted, Closure::Declined] {
            let mut app = Application::default();
            app.advance_to(ApplicationStatus::Interviewing, "2026-06-10");
            app.close_as(closure, "2026-07-01");
            assert_eq!(app.status(), ApplicationStatus::Offer, "{closure:?}");
            assert_eq!(app.closed_as, Some(closure));
        }
    }

    /// Dragging a finished card back into the pipeline reopens it, which is
    /// what happens when a company that ghosted you finally writes back.
    #[test]
    fn moving_a_closed_card_back_into_the_pipeline_reopens_it() {
        let mut app = Application::default();
        app.close_as(Closure::Ghosted, "2026-06-20");
        assert!(app.closed_as.is_some());

        app.advance_to(ApplicationStatus::Interviewing, "2026-08-01");
        assert_eq!(app.closed_as, None, "the card is live again");
        // And the history kept every one of those moves, so the reopening is
        // itself part of the record.
        assert_eq!(app.history.len(), 2);
    }
}
