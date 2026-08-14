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

use crate::resume::model::{Application, ApplicationStatus, Applications};

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
            _ if app.status == ApplicationStatus::Rejected => Some(Outcome::Rejected),
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
            status,
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
