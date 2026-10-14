//! cv-application-tracker — job application state machine and funnel metrics (Kraton VMC).

wit_bindgen::generate!({
    path: "wit",
    world: "application-tracker-vmc",
});

use exports::cv::application_tracker::tracker::{
    AddRequest, ApplicationCard, ApplicationStatus, ClosureReason, FunnelSummary, Guest,
    InterviewRequest, InterviewRound, NextStep, ParseRequest, SentCv, StageChange, StepRequest,
    TrackerError, TransitionRequest, UpdateResult,
};

use dockcv_core::resume::model::{
    Application, ApplicationStatus as CoreStatus, Applications, Closure as CoreClosure,
    InterviewRound as CoreInterviewRound, NextStep as CoreNextStep, SentCv as CoreSentCv,
    StageChange as CoreStageChange,
};

struct Tracker;

fn parse_board_internal(toml_str: &str) -> Result<Applications, TrackerError> {
    if toml_str.trim().is_empty() {
        return Ok(Applications::default());
    }
    toml::from_str::<Applications>(toml_str)
        .map_err(|e| TrackerError::InvalidBoardToml(format!("Failed to parse applications TOML: {e}")))
}

fn map_card(app: &Application) -> ApplicationCard {
    let status = match app.status() {
        CoreStatus::Wishlist => ApplicationStatus::Wishlist,
        CoreStatus::Applied => ApplicationStatus::Applied,
        CoreStatus::Interviewing => ApplicationStatus::Interviewing,
        CoreStatus::Offer => ApplicationStatus::Offer,
        CoreStatus::Closed => ApplicationStatus::Closed,
    };

    let closed_as = app.closed_as.map(|c| match c {
        CoreClosure::Rejected => ClosureReason::Rejected,
        CoreClosure::Ghosted => ClosureReason::Ghosted,
        CoreClosure::Withdrew => ClosureReason::Withdrew,
        CoreClosure::Declined => ClosureReason::Declined,
        CoreClosure::Accepted => ClosureReason::Accepted,
    });

    let sent_as = app.sent_as.as_ref().map(|s| SentCv {
        document: s.document.clone(),
        preset: s.preset.clone(),
    });

    let next_step = app.next_step.as_ref().map(|s| NextStep {
        label: s.label.clone(),
        date: s.date.clone(),
        time: s.time.clone(),
    });

    let history = app
        .history
        .iter()
        .map(|h| StageChange {
            at: h.at.clone(),
            to: h.to.clone(),
        })
        .collect();

    let rounds = app
        .rounds
        .iter()
        .map(|r| InterviewRound {
            at: r.at.clone(),
            label: r.label.clone(),
        })
        .collect();

    ApplicationCard {
        company: app.company.clone(),
        role: app.role.clone(),
        status,
        created: app.created.clone(),
        applied: app.applied.clone(),
        url: app.url.clone(),
        posting: app.posting.clone(),
        compensation: app.compensation.clone(),
        notes: app.notes.clone(),
        closure_note: app.closure_note.clone(),
        closed_as,
        sent_as,
        next_step,
        history,
        rounds,
    }
}

fn build_update_result(apps: &Applications) -> Result<UpdateResult, TrackerError> {
    let updated_board_toml = toml::to_string(apps)
        .map_err(|e| TrackerError::InvalidBoardToml(format!("Serialization failed: {e}")))?;

    let cards = apps.entries.iter().map(map_card).collect();

    let funnel = FunnelSummary {
        total_applications: apps.entries.len() as u32,
        active_count: apps.active() as u32,
        wishlist_count: apps.count(CoreStatus::Wishlist) as u32,
        applied_count: apps.count(CoreStatus::Applied) as u32,
        interviewing_count: apps.count(CoreStatus::Interviewing) as u32,
        offer_count: apps.count(CoreStatus::Offer) as u32,
        closed_count: apps.count(CoreStatus::Closed) as u32,
    };

    Ok(UpdateResult {
        updated_board_toml,
        cards,
        funnel,
    })
}

impl Guest for Tracker {
    fn parse_board(req: ParseRequest) -> Result<UpdateResult, TrackerError> {
        let apps = parse_board_internal(&req.board_toml)?;
        build_update_result(&apps)
    }

    fn add_application(req: AddRequest) -> Result<UpdateResult, TrackerError> {
        let mut apps = parse_board_internal(&req.current_board_toml)?;

        let core_status = match req.status {
            ApplicationStatus::Wishlist => CoreStatus::Wishlist,
            ApplicationStatus::Applied => CoreStatus::Applied,
            ApplicationStatus::Interviewing => CoreStatus::Interviewing,
            ApplicationStatus::Offer => CoreStatus::Offer,
            ApplicationStatus::Closed => CoreStatus::Closed,
        };

        let sent_as = match (req.sent_document, req.sent_preset) {
            (Some(doc), preset) => Some(CoreSentCv {
                document: doc,
                preset: preset.unwrap_or_default(),
            }),
            _ => None,
        };

        let applied = if core_status == CoreStatus::Applied {
            Some(req.date.clone())
        } else {
            None
        };

        let mut app = Application {
            company: req.company,
            role: req.role,
            status_word: core_status.word().to_string(),
            created: req.date.clone(),
            applied,
            url: req.url,
            posting: req.posting,
            notes: String::new(),
            compensation: req.compensation,
            closure_note: None,
            closed_as: None,
            sent_as,
            next_step: None,
            snapshots: Vec::new(),
            history: Vec::new(),
            rounds: Vec::new(),
        };

        if core_status != CoreStatus::Wishlist {
            app.history.push(CoreStageChange {
                at: req.date,
                to: core_status.word().to_string(),
            });
        }

        apps.entries.push(app);
        build_update_result(&apps)
    }

    fn advance_application(req: TransitionRequest) -> Result<UpdateResult, TrackerError> {
        let mut apps = parse_board_internal(&req.current_board_toml)?;

        let target = apps
            .entries
            .iter_mut()
            .find(|a| a.company.eq_ignore_ascii_case(&req.company) && a.role.eq_ignore_ascii_case(&req.role))
            .ok_or_else(|| {
                TrackerError::ApplicationNotFound(format!(
                    "No application found for {} at {}",
                    req.role, req.company
                ))
            })?;

        let new_core_status = match req.new_status {
            ApplicationStatus::Wishlist => CoreStatus::Wishlist,
            ApplicationStatus::Applied => CoreStatus::Applied,
            ApplicationStatus::Interviewing => CoreStatus::Interviewing,
            ApplicationStatus::Offer => CoreStatus::Offer,
            ApplicationStatus::Closed => CoreStatus::Closed,
        };

        if let Some(closure) = req.closure {
            let core_closure = match closure {
                ClosureReason::Rejected => CoreClosure::Rejected,
                ClosureReason::Ghosted => CoreClosure::Ghosted,
                ClosureReason::Withdrew => CoreClosure::Withdrew,
                ClosureReason::Declined => CoreClosure::Declined,
                ClosureReason::Accepted => CoreClosure::Accepted,
            };
            target.close_as(core_closure, &req.date);
            target.closure_note = req.closure_note;
        } else {
            target.advance_to(new_core_status, &req.date);
        }

        build_update_result(&apps)
    }

    fn log_interview(req: InterviewRequest) -> Result<UpdateResult, TrackerError> {
        let mut apps = parse_board_internal(&req.current_board_toml)?;

        let target = apps
            .entries
            .iter_mut()
            .find(|a| a.company.eq_ignore_ascii_case(&req.company) && a.role.eq_ignore_ascii_case(&req.role))
            .ok_or_else(|| {
                TrackerError::ApplicationNotFound(format!(
                    "No application found for {} at {}",
                    req.role, req.company
                ))
            })?;

        target.rounds.push(CoreInterviewRound {
            at: req.date,
            label: req.label,
        });

        build_update_result(&apps)
    }

    fn schedule_step(req: StepRequest) -> Result<UpdateResult, TrackerError> {
        let mut apps = parse_board_internal(&req.current_board_toml)?;

        let target = apps
            .entries
            .iter_mut()
            .find(|a| a.company.eq_ignore_ascii_case(&req.company) && a.role.eq_ignore_ascii_case(&req.role))
            .ok_or_else(|| {
                TrackerError::ApplicationNotFound(format!(
                    "No application found for {} at {}",
                    req.role, req.company
                ))
            })?;

        target.next_step = Some(CoreNextStep {
            label: req.label,
            date: req.date,
            time: req.time,
        });

        build_update_result(&apps)
    }
}

export!(Tracker);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_advance_application() {
        let result = Tracker::add_application(AddRequest {
            current_board_toml: String::new(),
            company: "Anthropic".into(),
            role: "Systems Engineer".into(),
            status: ApplicationStatus::Applied,
            date: "2026-10-14".into(),
            posting: "Building safe sovereign systems".into(),
            url: "https://example.com/job".into(),
            compensation: "$220k".into(),
            sent_document: Some("resume".into()),
            sent_preset: Some("Rust-Systems".into()),
        })
        .expect("add succeeds");

        assert_eq!(result.cards.len(), 1);
        assert_eq!(result.funnel.applied_count, 1);
        assert_eq!(result.cards[0].company, "Anthropic");

        let advanced = Tracker::advance_application(TransitionRequest {
            current_board_toml: result.updated_board_toml,
            company: "Anthropic".into(),
            role: "Systems Engineer".into(),
            new_status: ApplicationStatus::Interviewing,
            date: "2026-10-16".into(),
            closure: None,
            closure_note: None,
        })
        .expect("advance succeeds");

        assert_eq!(advanced.cards[0].status, ApplicationStatus::Interviewing);
        assert_eq!(advanced.cards[0].history.len(), 2);
        assert_eq!(advanced.funnel.interviewing_count, 1);
    }
}
