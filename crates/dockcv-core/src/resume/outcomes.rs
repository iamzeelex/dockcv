//! What a reading has actually done: sent under it, and how far that got.
//!
//! One query, in the core, because two surfaces ask it — the Preset Matrix's
//! column headers (C11) and the front door's rows (C12a). Two copies of this
//! arithmetic would be two chances to disagree about what an interview is.
//!
//! It lives here rather than in `model.rs` on purpose: that file is 4000 lines
//! against a ~800 limit and owes a split (#17). An `impl Applications` block
//! is legal in any file of this crate, so new subjects can stop landing there.

use crate::resume::model::{ApplicationStatus, Applications};

/// How one reading has done. Counts, never a rate.
///
/// A rate would need a denominator big enough to mean something. The market
/// baseline is around one interview per seventeen sends, so a percentage over
/// a handful of applications is noise wearing a decimal point — the same
/// reason Track B refuses a score out of a hundred.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresetRecord {
    /// Applications actually sent under this reading.
    pub sent: usize,
    /// Of those, the ones that reached an interview or better.
    pub interviewed: usize,
}

impl PresetRecord {
    pub fn is_empty(self) -> bool {
        self.sent == 0
    }
}

impl Applications {
    /// What was sent under `document` · `preset`, and how far it got.
    ///
    /// Keyed by the **pair**, not by the preset name alone: two CVs can each
    /// have a "Concise", and merging them would attribute one document's
    /// interviews to another's reading. An empty `preset` matches the
    /// applications sent from a document that had no preset, which is a real
    /// state rather than missing data.
    ///
    /// Reaching an interview is credited to the reading even when the answer
    /// was later no — `furthest`, not `status`. A rejection after an onsite
    /// does not un-earn the onsite, and the question a column header answers
    /// is *which cut of me gets interviews*.
    pub fn record_for(&self, document: &str, preset: &str) -> PresetRecord {
        let mut record = PresetRecord::default();
        for app in &self.entries {
            let Some(sent) = app.sent_as.as_ref() else {
                continue;
            };
            if sent.document != document || sent.preset != preset {
                continue;
            }
            // Never sent, so there is nothing to attribute — and `furthest`
            // is the only thing that knows. A card closed straight off the
            // wishlist reads `Closed` as its *status* while its furthest stage
            // is still `Wishlist`, because `advance_to` refuses to let a
            // rejection count as pipeline depth. That is a role you decided
            // against, not a CV that failed, and the funnel excludes it for
            // the same reason (`Outcome::of`).
            if app.furthest() == ApplicationStatus::Wishlist {
                continue;
            }
            record.sent += 1;
            if matches!(
                app.furthest(),
                ApplicationStatus::Interviewing | ApplicationStatus::Offer
            ) {
                record.interviewed += 1;
            }
        }
        record
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::{Application, SentCv};

    /// A card with a CV pinned, still on the wishlist.
    fn pinned(document: &str, preset: &str) -> Application {
        Application {
            company: format!("{document}-{preset}-co"),
            sent_as: Some(SentCv {
                document: document.into(),
                preset: preset.into(),
            }),
            ..Default::default()
        }
    }

    /// One that actually went out, and got as far as `reached`.
    ///
    /// Always through `Applied` first, because that is the transition that
    /// makes a card sent — moving straight to `Closed` is a role dropped from
    /// the wishlist, and `advance_to` deliberately does not let that raise
    /// `furthest`.
    fn sent(document: &str, preset: &str, reached: ApplicationStatus) -> Application {
        let mut app = pinned(document, preset);
        app.advance_to(ApplicationStatus::Applied, "2026-09-18");
        if reached != ApplicationStatus::Applied {
            app.advance_to(reached, "2026-09-19");
        }
        app
    }

    #[test]
    fn counts_are_kept_per_document_and_preset_pair() {
        let applications = Applications {
            entries: vec![
                sent("resume", "Concise", ApplicationStatus::Interviewing),
                sent("resume", "Concise", ApplicationStatus::Applied),
                sent("academic-cv", "Concise", ApplicationStatus::Offer),
            ],
        };

        let resume = applications.record_for("resume", "Concise");
        assert_eq!(resume.sent, 2);
        assert_eq!(
            resume.interviewed, 1,
            "the other document's Concise is a different reading"
        );

        let academic = applications.record_for("academic-cv", "Concise");
        assert_eq!(academic.sent, 1);
        assert_eq!(academic.interviewed, 1, "an offer reached an interview");
    }

    /// A rejection after an interview still credits the interview, and a
    /// rejection before one does not.
    #[test]
    fn an_interview_survives_the_rejection_that_followed_it() {
        let mut interviewed_then_rejected =
            sent("resume", "Infra", ApplicationStatus::Interviewing);
        interviewed_then_rejected.advance_to(ApplicationStatus::Closed, "2026-09-19");

        let applications = Applications {
            entries: vec![
                interviewed_then_rejected,
                sent("resume", "Infra", ApplicationStatus::Closed),
            ],
        };

        let record = applications.record_for("resume", "Infra");
        assert_eq!(record.sent, 2);
        assert_eq!(record.interviewed, 1);
    }

    /// A card with a CV pinned but nothing sent yet is not a send — and
    /// neither is one closed straight off the wishlist, which is a role
    /// decided against rather than a CV that failed.
    #[test]
    fn a_wishlist_card_with_a_cv_pinned_is_not_a_send() {
        let applications = Applications {
            entries: vec![pinned("resume", "Base")],
        };
        assert_eq!(applications.record_for("resume", "Base"), PresetRecord::default());
        assert!(applications.record_for("resume", "Base").is_empty());

        let mut dropped = pinned("resume", "Base");
        dropped.advance_to(ApplicationStatus::Closed, "2026-09-18");
        let applications = Applications {
            entries: vec![dropped],
        };
        assert_eq!(applications.record_for("resume", "Base").sent, 0);
    }
}
