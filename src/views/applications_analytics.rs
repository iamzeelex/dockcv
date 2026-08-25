//! The Insights view: what the board cannot show, because it is about the
//! cards that have already moved on.
//!
//! A kanban answers *where is everything right now*. It cannot answer *which
//! CV gets me interviews*, because the moment a card reaches Offer or Rejected
//! it stops being evidence about the document that got it there — the column
//! it sits in describes the company's decision, not your cut of the CV.
//!
//! So: a Sankey from each preset to the furthest outcome its applications
//! reached, over the stat tiles that carry the same numbers as text. The
//! counting rule and every judgement in it live in
//! [`super::applications_funnel`], which is pure and tested; this file is
//! drawing only.
//!
//! Colours come from the same [`column_tint`] the board uses, so a ribbon
//! landing in "Offer" is the green the Offer column already is. A chart with
//! its own palette would be a second visual language for one set of facts.

use gpui::prelude::*;
use gpui::{div, px, Context, Hsla, IntoElement, SharedString};

use dockcv_ui_components::{
    Button, ButtonVariants, DropdownMenu, EmptyState, IconName, PopupMenuItem, SankeyAlign,
    SankeyChart, SankeyLabel, SankeyLink, SankeyValueScale, Sizable,
};

use crate::resume::model::{ApplicationStatus, Applications, Closure};
use crate::theme::{ActiveTheme, StyledText, TextStyle, Theme};

use super::applications_card::column_tint;
use super::applications_data::{plural, status_title};
use super::applications_funnel::{journey, missing_history, stage_flow, JourneyNode, Period};
use super::applications_funnel::{Funnel, Outcome};
use super::shell::Shell;

/// One rectangle in the diagram. `SankeyLink` addresses nodes by their index
/// in the node list, so the node vector is built once, in display order, and
/// every link is expressed against that ordering.
#[derive(Clone)]
struct FunnelNode {
    label: SharedString,
    color: Hsla,
}

/// The colour a journey node takes.
///
/// Endings borrow the board's own colours where the board has one, so an
/// "Offer" node is the green the Offer column already is. The three endings
/// the board has no column for are placed by what they *are*: a ghosting is
/// the same silence as a rejection, a withdrawal and a decline are the
/// applicant's own decision and read as neutral rather than as a loss.
fn journey_color(theme: &Theme, node: JourneyNode) -> Hsla {
    match node {
        JourneyNode::Sent => column_tint(theme, ApplicationStatus::Applied).dot,
        JourneyNode::Round(_) => column_tint(theme, ApplicationStatus::Interviewing).dot,
        JourneyNode::Offer => column_tint(theme, ApplicationStatus::Offer).dot,
        JourneyNode::Live => theme.text_muted,
        JourneyNode::Closed(Closure::Accepted) => column_tint(theme, ApplicationStatus::Offer).dot,
        JourneyNode::Closed(Closure::Rejected | Closure::Ghosted) => {
            column_tint(theme, ApplicationStatus::Closed).dot
        }
        JourneyNode::Closed(Closure::Withdrew | Closure::Declined) => theme.text_subtle,
    }
}

/// The colour an outcome's node and its incoming ribbons take — the board's
/// own column colour for the status that outcome corresponds to.
fn outcome_color(theme: &Theme, outcome: Outcome) -> Hsla {
    let status = match outcome {
        Outcome::Offer => ApplicationStatus::Offer,
        Outcome::Interviewed => ApplicationStatus::Interviewing,
        Outcome::Rejected => ApplicationStatus::Closed,
        Outcome::Awaiting => ApplicationStatus::Applied,
    };
    column_tint(theme, status).dot
}

impl Shell {
    pub(super) fn render_applications_insights(
        &self,
        cx: &mut Context<Self>,
        applications: &Applications,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let funnel = Funnel::of(applications);

        div()
            .id("applications-insights")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(34.0))
            .pb(px(28.0))
            .flex()
            .flex_col()
            .gap(px(18.0))
            .child(self.insight_tiles(cx, &funnel))
            // The journey first. It is the chart a person screenshots — the
            // shape of their search, ending by ending — and the preset funnel
            // below answers the narrower question of which CV did it.
            .child(self.journey_panel(cx, applications))
            .child(self.stage_movement(cx, applications))
            .child(self.funnel_panel(&theme, &funnel))
    }

    /// Movement between stages over a window — what the Sankey cannot say.
    ///
    /// The Sankey is about *which CV*; this is about *when*, and the two are
    /// different questions about the same board. A card that reached an
    /// interview last March and one that reached one yesterday are the same
    /// ribbon there and very different news here.
    fn stage_movement(
        &self,
        cx: &mut Context<Self>,
        applications: &Applications,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let period = self.applications_period;
        let from = period
            .days()
            .map(crate::vault::iso_days_ago)
            // Before any vault existed, so "all time" is every date there is.
            .unwrap_or_else(|| "0000-01-01".to_string());
        let flows = stage_flow(applications, &from, &crate::vault::today_iso());
        let unaccounted = missing_history(applications);
        let root = cx.weak_entity();

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .text_style(TextStyle::eyebrow())
                    .text_color(theme.text_subtle)
                    .child(TextStyle::eyebrow().apply_case("Movement between stages")),
            )
            .child(
                Button::new("insights-period")
                    .cursor_pointer()
                    .ghost()
                    .xsmall()
                    .label(period.label())
                    .icon(IconName::ChevronDown)
                    .border_1()
                    .border_color(theme.border)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        for option in Period::ALL {
                            let root = root.clone();
                            menu = menu.item(
                                PopupMenuItem::new(option.label())
                                    .checked(option == period)
                                    .on_click(move |_ev, _window, cx| {
                                        let _ = root.update(cx, |this, cx| {
                                            this.applications_period = option;
                                            cx.notify();
                                        });
                                    }),
                            );
                        }
                        menu
                    }),
            );

        let rows = flows.into_iter().map(|flow| {
            let tint = column_tint(&theme, flow.stage);
            // "Of the ones that got here, how many got further." Printed only
            // where there is a denominator: a percentage over nothing is the
            // invented metric US-14 exists to forbid.
            let onward = if flow.entered == 0 {
                "—".to_string()
            } else if flow.stage.depth().is_none() {
                // Nothing advances out of a rejection, so a rate would be a
                // column of zeroes pretending to be a measurement.
                "end of the line".to_string()
            } else {
                format!("{} of {} moved on", flow.advanced, flow.entered)
            };

            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .py(px(5.0))
                .child(
                    div()
                        .w(px(108.0))
                        .text_style(TextStyle::chip())
                        .text_color(tint.fg)
                        .child(status_title(flow.stage)),
                )
                .child(
                    div()
                        .w(px(52.0))
                        .text_style(TextStyle::control())
                        .text_color(theme.text)
                        .child(flow.entered.to_string()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_style(TextStyle::meta())
                        .text_color(theme.text_muted)
                        .child(onward),
                )
        });

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(16.0))
            .rounded(px(12.0))
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .child(header)
            .children(rows)
            .when(unaccounted > 0, |el| {
                // Said out loud rather than folded into the counts. These
                // cards moved before the app recorded moves, so their history
                // does not exist and never will — reporting them as zero
                // would be a quiet lie about the period (US-14).
                el.child(
                    div()
                        .pt(px(6.0))
                        .text_style(TextStyle::meta())
                        .text_color(theme.text_subtle)
                        .child(format!(
                            "{unaccounted} application{} moved before DockCV recorded \
                             stage changes, so they are not in these counts.",
                            plural(unaccounted)
                        )),
                )
            })
    }

    /// Four numbers, stated plainly, above the diagram that shows how they
    /// relate. The tiles are not decoration: a Sankey is read by comparing
    /// widths, and a width is not a number you can quote in a cover letter.
    fn insight_tiles(&self, cx: &mut Context<Self>, funnel: &Funnel) -> impl IntoElement {
        let theme = cx.theme().clone();

        let rate = match funnel.interview_rate() {
            // No denominator: unknown, not zero. Printing 0% over nothing sent
            // would be inventing a metric (US-14).
            None => "—".to_string(),
            Some(rate) => format!("{rate:.0}%"),
        };

        let tiles: Vec<(String, String, Hsla)> = vec![
            (
                funnel.sent.to_string(),
                format!("application{} sent", plural(funnel.sent)),
                theme.text,
            ),
            (
                funnel.total(Outcome::Interviewed).to_string(),
                "reached an interview".to_string(),
                outcome_color(&theme, Outcome::Interviewed),
            ),
            (
                funnel.total(Outcome::Offer).to_string(),
                format!("offer{}", plural(funnel.total(Outcome::Offer))),
                outcome_color(&theme, Outcome::Offer),
            ),
            (rate, "of sent CVs got an interview".to_string(), theme.text),
        ];

        div()
            .flex()
            .gap(px(10.0))
            .children(tiles.into_iter().map(|(value, caption, color)| {
                div()
                    // Equal shares rather than content width: four tiles of
                    // different digit counts should still read as one row of
                    // four, not as a ragged strip.
                    .flex_1()
                    .min_w_0()
                    .px(px(14.0))
                    .py(px(12.0))
                    .rounded(px(10.0))
                    .bg(theme.elevated)
                    .border_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_style(TextStyle::title())
                            .text_color(color)
                            .child(value),
                    )
                    .child(
                        // Sans, not mono: `meta()` is the Data role, and
                        // "applications sent" is a caption, not a count. The
                        // figure above it is the data.
                        div()
                            .text_style(TextStyle::label())
                            .text_color(theme.text_muted)
                            .child(caption),
                    )
            }))
    }

    /// The Sankey itself, in a titled panel that says what it is counting.
    /// The journey: every sent application, from sent to however it ended.
    fn journey_panel(&self, cx: &mut Context<Self>, applications: &Applications) -> impl IntoElement {
        let theme = cx.theme().clone();
        let journey = journey(applications);

        let body = if journey.is_empty() {
            div()
                .h(px(220.0))
                .flex()
                .items_center()
                .justify_center()
                .child(EmptyState::new("Nothing sent yet").body(
                    "Move an application to Applied and this will show the shape of \
                     your search — interviews, offers, and every way one can end.",
                ))
                .into_any_element()
        } else {
            // Nodes in the order the diagram reads: sent, then each round,
            // then the offer, then the endings. Their index is what the links
            // are written against, so this order is the diagram's spine.
            let mut order: Vec<JourneyNode> = vec![JourneyNode::Sent];
            let deepest = journey
                .links
                .iter()
                .filter_map(|(_, to, _)| match to {
                    JourneyNode::Round(n) => Some(*n),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            order.extend((1..=deepest).map(JourneyNode::Round));
            order.push(JourneyNode::Offer);
            order.push(JourneyNode::Live);
            order.extend(Closure::ALL.map(JourneyNode::Closed));
            // Only what the diagram actually reaches: an unused node would
            // draw a labelled zero, which reads as a fact rather than as an
            // absence.
            order.retain(|node| {
                journey
                    .links
                    .iter()
                    .any(|(from, to, _)| from == node || to == node)
            });

            let nodes: Vec<FunnelNode> = order
                .iter()
                .map(|node| FunnelNode {
                    label: node.label().into(),
                    color: journey_color(&theme, *node),
                })
                .collect();
            let links: Vec<SankeyLink> = journey
                .links
                .iter()
                .filter_map(|(from, to, count)| {
                    let source = order.iter().position(|n| n == from)?;
                    let target = order.iter().position(|n| n == to)?;
                    Some(SankeyLink::new(source, target, *count as f64))
                })
                .collect();

            let height = (200.0 + deepest as f32 * 40.0).min(460.0);
            let muted = theme.text_muted;
            div()
                .h(px(height))
                .w_full()
                .child(
                    SankeyChart::new(nodes, links)
                        .node_align(SankeyAlign::Justify)
                        .node_width(12.0)
                        .node_padding(18.0)
                        .node_corner_radius(px(3.0))
                        .value_scale(SankeyValueScale::Linear)
                        .node_color(|node: &FunnelNode| node.color)
                        .labels(move |node: &FunnelNode, value: f64| {
                            let n = value.round() as usize;
                            vec![
                                SankeyLabel::new(format!("{n}")),
                                SankeyLabel::new(node.label.clone()).color(muted),
                            ]
                        }),
                )
                .into_any_element()
        };

        let mut caption = format!("{} sent", journey.sent);
        if journey.not_sent > 0 {
            caption.push_str(&format!(
                "; {} still on the wishlist and not in the diagram",
                journey.not_sent
            ));
        }
        caption.push_str(
            ". An application nobody has answered is \"In progress\" until you say \
             it was ghosted — silence is not a no until you decide it is.",
        );

        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .p(px(18.0))
            .rounded(px(12.0))
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_style(TextStyle::heading())
                    .text_color(theme.text)
                    .child("How the search is going"),
            )
            .child(
                div()
                    .text_style(TextStyle::meta())
                    .text_color(theme.text_subtle)
                    .child(caption),
            )
            .child(body)
    }

    fn funnel_panel(&self, theme: &Theme, funnel: &Funnel) -> impl IntoElement {
        let body = if funnel.is_empty() {
            div()
                .h(px(260.0))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    EmptyState::new("Nothing sent yet")
                        .body(
                            "Move an application to Applied and this will show which \
                             CV you sent, and how far it got.",
                        ),
                )
                .into_any_element()
        } else {
            self.sankey(theme, funnel).into_any_element()
        };

        let mut caption = format!(
            "One ribbon per application, from the CV you sent to the furthest \
             it reached. {} sent",
            funnel.sent
        );
        if funnel.not_sent > 0 {
            // Said out loud, so the diagram's total never appears to disagree
            // with the board's.
            caption.push_str(&format!(
                "; {} still on the wishlist and not counted here",
                funnel.not_sent
            ));
        }
        caption.push('.');

        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .p(px(18.0))
            .rounded(px(12.0))
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_style(TextStyle::heading())
                    .text_color(theme.text)
                    .child("Which CV got you there"),
            )
            .child(
                // A sentence, so sans — see the tile captions above.
                div()
                    .text_style(TextStyle::body())
                    .text_color(theme.text_muted)
                    .child(caption),
            )
            .child(div().mt(px(12.0)).child(body))
    }

    fn sankey(&self, theme: &Theme, funnel: &Funnel) -> impl IntoElement {
        // Nodes first, in display order: presets down the left, outcomes down
        // the right. The index of each is what the links are written against.
        let mut nodes: Vec<FunnelNode> = Vec::new();
        for preset in &funnel.presets {
            nodes.push(FunnelNode {
                label: preset.clone().into(),
                // Presets are the neutral side: the eye should follow the
                // ribbons to the outcomes, which are where the colour is.
                color: theme.text_muted,
            });
        }
        let preset_base = 0;
        let outcome_base = nodes.len();
        for outcome in &funnel.outcomes {
            nodes.push(FunnelNode {
                label: outcome.label().into(),
                color: outcome_color(theme, *outcome),
            });
        }

        let links: Vec<SankeyLink> = funnel
            .flows
            .iter()
            .filter_map(|flow| {
                let source = preset_base + funnel.presets.iter().position(|p| p == &flow.preset)?;
                let target =
                    outcome_base + funnel.outcomes.iter().position(|o| *o == flow.outcome)?;
                Some(SankeyLink::new(source, target, flow.count as f64))
            })
            .collect();

        // Height grows with the number of rows so a busy funnel does not
        // compress into unreadable slivers, but is bounded — past a point the
        // panel is scrolling and the shape is already clear.
        let rows = funnel.presets.len().max(funnel.outcomes.len());
        let height = (140.0 + rows as f32 * 46.0).min(420.0);

        let muted = theme.text_muted;
        div().h(px(height)).w_full().child(
            SankeyChart::new(nodes, links)
                .node_align(SankeyAlign::Justify)
                .node_width(12.0)
                .node_padding(22.0)
                .node_corner_radius(px(3.0))
                // Counts here are small integers — one application is one
                // unit — so a linear scale is honest and readable. `Sqrt`
                // exists for the case where one flow dwarfs the rest; at CV
                // volumes it would flatten exactly the difference the chart is
                // being asked about.
                .value_scale(SankeyValueScale::Linear)
                .node_color(|node: &FunnelNode| node.color)
                // Two lines per node: the count, then the name. The count
                // leads because the question is "how many", and the name is
                // what you already know you are looking at.
                .labels(move |node: &FunnelNode, value: f64| {
                    let n = value.round() as usize;
                    vec![
                        SankeyLabel::new(format!("{n}")),
                        SankeyLabel::new(node.label.clone()).color(muted),
                    ]
                }),
        )
    }
}
