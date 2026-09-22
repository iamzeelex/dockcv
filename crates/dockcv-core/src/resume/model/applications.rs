//! Job-application tracking model.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Applications (roadmap D4, the product review US-04/US-05, P-03/P-04)
// ---------------------------------------------------------------------------
//
// A board, not a tracker: `Rejected` is a first-class column, not something
// swept out of the list, because the conversion figures the Library screen
// wants (`4 sent → 1 interview → 1 offer`) are meaningless without the
// denominator Rejected holds (review P-04). Stored at `<vault>/applications.toml`.

/// The board's five columns.
///
/// Serializes to a lowercase string (`status = "wishlist"`) — this file is
/// meant to be hand-edited. A typo or an unrecognised value falls back to
/// `Wishlist` via `#[serde(other)]` rather than failing to parse the whole
/// file: one bad status line should demote a card to the board's default
/// column, not take out every application in the vault. `#[serde(other)]`
/// must name the enum's last variant (a serde-derive requirement), which is
/// why `Wishlist` — semantically the first column — is declared last here;
/// `ApplicationStatus::default()` is what every other reader (counts,
/// conversion) should use to mean "wishlist", not this declaration order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApplicationStatus {
    Applied,
    Interviewing,
    Offer,
    /// The column an application lands in once it is over and there was no
    /// offer. Not "Rejected", which it used to be called: rejections,
    /// ghostings and withdrawals all end up here, and naming the column after
    /// one of the three made the other two look like something they are not.
    /// Which of them it was is [`Application::closed_as`].
    Closed,
    #[default]
    #[serde(other)]
    Wishlist,
}

fn wishlist_word() -> String {
    "wishlist".to_string()
}

impl ApplicationStatus {
    /// The spelling this build writes. Lowercase, matching serde's own
    /// `rename_all` so a file written before this existed round-trips
    /// unchanged.
    pub fn word(self) -> &'static str {
        match self {
            ApplicationStatus::Wishlist => "wishlist",
            ApplicationStatus::Applied => "applied",
            ApplicationStatus::Interviewing => "interviewing",
            ApplicationStatus::Offer => "offer",
            ApplicationStatus::Closed => "closed",
        }
    }

    /// The status a word names, or `None` when this build does not know it —
    /// which is the distinction the whole round-trip rests on.
    pub fn from_word(word: &str) -> Option<Self> {
        match word.trim().to_lowercase().as_str() {
            "wishlist" => Some(ApplicationStatus::Wishlist),
            "applied" => Some(ApplicationStatus::Applied),
            "interviewing" => Some(ApplicationStatus::Interviewing),
            "offer" => Some(ApplicationStatus::Offer),
            "closed" => Some(ApplicationStatus::Closed),
            _ => None,
        }
    }

    /// Ordinal depth of a *pipeline* stage, used only to track how far an
    /// application has ever gotten (`Application::furthest`).
    ///
    /// `Rejected` deliberately has no depth: it is a terminal outcome you can
    /// land in from any stage, not a stage deeper than `Offer`, and it must
    /// never be allowed to raise `furthest` — see `Application::advance_to`.
    /// A private helper rather than `Ord` on the enum itself, because the
    /// enum's declaration order is already pinned by `#[serde(other)]`
    /// needing `Wishlist` last, which is not the order this depth uses.
    pub fn depth(self) -> Option<u8> {
        match self {
            ApplicationStatus::Wishlist => Some(0),
            ApplicationStatus::Applied => Some(1),
            ApplicationStatus::Interviewing => Some(2),
            ApplicationStatus::Offer => Some(3),
            ApplicationStatus::Closed => None,
        }
    }
}

/// The next thing that has to happen, as data rather than a caption. The
/// review is explicit (P-04): `Decide by Fri` drawn as a label is a caption
/// you cannot build a reminder from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NextStep {
    /// What has to happen: "Onsite", "Take-home due", "Decide by".
    pub label: String,
    /// ISO date (YYYY-MM-DD).
    pub date: String,
    /// 24h "14:00", empty when the step is a day rather than a moment.
    pub time: String,
}

/// A PDF as it was actually sent. Never a live reference to a preset:
/// editing the CV in July must not rewrite what a company saw in March (US-04).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// 1-based per application — the mockup's `snapshot v1`, `v2`.
    pub version: u32,
    /// ISO date the snapshot was taken.
    pub date: String,
    /// The preset name at that moment. A label recording history, not a
    /// lookup key — the preset itself may be renamed or deleted later.
    pub preset: String,
    /// File name inside `<vault>/snapshots/`. The bytes are a real file the
    /// user can open in Finder, per File-over-App.
    pub file: String,
}

/// One card on the Applications board.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Application {
    pub company: String,
    pub role: String,
    /// The `status` word exactly as the file spells it — and the only form
    /// that round-trips.
    ///
    /// `ApplicationStatus` takes `#[serde(other)]`, so a word this build does
    /// not know reads as `Wishlist`; that is the right trade, since one bad
    /// word should cost a card rather than the whole board. What was *not*
    /// right is what happened next: the enum was written back, so a
    /// hand-edited `status = "ofer"` was silently rewritten to `"wishlist"`
    /// and an offer stopped having ever existed. The word is kept verbatim,
    /// This is the **only** stored form: [`Application::status`] reads it,
    /// [`Application::advance_to`] writes it, and there is no second field to
    /// drift out of step with it.
    #[serde(rename = "status", default = "wishlist_word")]
    pub status_word: String,
    /// ISO date the card was created — the mockup's "saved 4d ago".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub created: String,
    /// ISO date it was actually sent. `None` while it is still a wishlist entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied: Option<String>,
    /// The posting.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// The posting's own text, as it was when you read it.
    ///
    /// Kept rather than fetched, because a link to a job is a link to a page
    /// that will be taken down: three months later the board has forgotten the
    /// role and the only record of what it asked for is this. It is what the
    /// tailoring read matches against, and it is why "why did I send *that*
    /// version" stays answerable after the fact.
    ///
    /// Someone else's words in the user's vault, which is fine here and only
    /// here: the vault is a folder on their disk, nothing leaves it, and this
    /// is the one place a job description is the *user's* record of a decision
    /// rather than a publisher's copy.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub posting: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// Free text: "$168k base · negotiating". Deliberately not a number —
    /// compensation is a negotiation state, not a figure to do arithmetic on.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub compensation: String,
    /// Free text about how it ended — `Some("role filled")` vs `None`, which
    /// the card renders as "no reason given".
    ///
    /// Read alongside [`Self::closed_as`], which is the typed half.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closure_note: Option<String>,
    /// How it ended, once it has. `None` while it is still live — which is
    /// not the same as `Ghosted`, and the difference is the user's to draw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_as: Option<Closure>,
    /// The CV this was sent with, if one has been attributed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_as: Option<SentCv>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_step: Option<NextStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snapshots: Vec<Snapshot>,
    /// Every stage this card has moved into, oldest first.
    ///
    /// [`Self::furthest`] answers *how deep did this ever get*; this answers
    /// *when*, which is the question a funnel over a date range cannot be
    /// asked without. Append-only, and written in exactly one place
    /// ([`Self::advance_to`]).
    ///
    /// The card's **first** stage is not in here. A card starts on the
    /// wishlist at [`Self::created`], so recording that would be one fact in
    /// two places; the first entry is the first time it *moved*. A card that
    /// has never moved therefore carries no history at all, which is correct
    /// and is not the same as a card written before this field existed —
    /// those are indistinguishable, and Insights has to say so rather than
    /// draw a month it cannot account for (US-14).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<StageChange>,
    /// The conversations that have happened, oldest first.
    ///
    /// Kept apart from [`Self::history`] on purpose: history is where the card
    /// *moved*, and three rounds of interview are three events inside one
    /// move. Folding them together would make "entered Interviewing" mean two
    /// different things depending on which entry you were reading.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rounds: Vec<InterviewRound>,
}

/// A conversation that actually happened — a screen, an onsite, a panel.
///
/// **Not a stage.** The board has one `Interviewing` column and keeps it: a
/// second and a third round are the same stage happening again, and a column
/// per round is a board that grows sideways for every person who gets deep
/// into one process. What varies between applications is *how many times*,
/// which is a count, and a count belongs in a list rather than in the shape
/// of the UI.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterviewRound {
    /// ISO date it happened.
    pub at: String,
    /// What it was: "Technical screen", "Onsite", "Final panel". Free text
    /// for the same reason `compensation` is — a process names its own steps,
    /// and an enum here would be this app telling companies how to hire.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

/// How an application ended.
///
/// Orthogonal to the column it sits in. The board answers *where is this now*;
/// this answers *how did it finish*, and the two are different questions — an
/// offer you turned down is not a rejection, and silence is not a no even
/// though it ends the same way.
///
/// `Ghosted` is set by the user, never inferred. The app can notice that
/// nothing has moved in eight weeks and say so; deciding that silence is final
/// is a judgement about a company, and inventing it would be inventing the
/// number people quote most (US-14).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Closure {
    /// They said no.
    Rejected,
    /// No reply, ever.
    Ghosted,
    /// You pulled out before there was anything to turn down.
    Withdrew,
    /// You turned down an offer.
    Declined,
    /// You took it.
    Accepted,
}

impl Closure {
    pub const ALL: [Closure; 5] = [
        Closure::Rejected,
        Closure::Ghosted,
        Closure::Withdrew,
        Closure::Declined,
        Closure::Accepted,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Closure::Rejected => "Rejected",
            Closure::Ghosted => "Ghosted",
            Closure::Withdrew => "Withdrew",
            Closure::Declined => "Declined",
            Closure::Accepted => "Accepted",
        }
    }

    /// The column a finished application belongs in.
    ///
    /// The board says *where*, the closure says *how it ended*, and those are
    /// two spellings of one fact — so one of them has to derive the other or
    /// they will disagree. They did: a card dragged to Rejected read as
    /// rejected on the board and as still in progress in the journey diagram,
    /// because the board wrote the column and the diagram read the closure.
    pub fn column(self) -> ApplicationStatus {
        match self {
            // There was an offer, so the card belongs where offers live —
            // a search that ended in a yes should not be filed under no.
            Closure::Accepted | Closure::Declined => ApplicationStatus::Offer,
            Closure::Rejected | Closure::Ghosted | Closure::Withdrew => ApplicationStatus::Closed,
        }
    }
}

/// The CV an application was sent with.
///
/// One field rather than the two it was — a `source_doc: Option<String>` and a
/// `preset: String` — because it is one fact and the split made its absence
/// spellable four ways. Three of those four were unreachable, and every reader
/// had to decide for itself what a preset with no document meant.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentCv {
    /// The document's file stem — a label, not a live reference. A document
    /// can be renamed or deleted and the card must still tell the truth about
    /// what was sent (US-04).
    pub document: String,
    /// The preset within it. Empty when the document has none, which is a
    /// document sent whole rather than a missing value.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub preset: String,
}

impl SentCv {
    /// How it reads on a card: `resume · Fintech Staff`, or just the document.
    pub fn label(&self) -> String {
        if self.preset.is_empty() {
            self.document.clone()
        } else {
            format!("{} · {}", self.document, self.preset)
        }
    }
}

/// One recorded move between stages.
///
/// `to` only. Where a move came *from* is the previous entry's `to`, or the
/// wishlist for the first one — storing it as well would be one fact in two
/// places, free to disagree, which is the reason `status_word` has no second
/// field beside it either.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageChange {
    /// ISO date the move was made. Supplied by the caller rather than read
    /// from a clock here: a model that tells the time cannot be tested.
    pub at: String,
    /// The stage moved into, as the word — the form that round-trips, for the
    /// same reason [`Application::status_word`] is stored that way.
    pub to: String,
}

impl Default for Application {
    /// Hand-written for one field. `status_word` carries a serde default so a
    /// card read from disk without a status is a wishlist card; the derived
    /// `Default` gave it `""` instead, and every card made in the app with
    /// `..Default::default()` was written back as `status = ""` — which the
    /// next read then warned about and silently treated as wishlist. A card
    /// with no status is not a card, and the two defaults have to agree.
    fn default() -> Self {
        Self {
            company: String::new(),
            role: String::new(),
            status_word: wishlist_word(),
            created: String::new(),
            applied: None,
            url: String::new(),
            posting: String::new(),
            notes: String::new(),
            compensation: String::new(),
            closure_note: None,
            closed_as: None,
            sent_as: None,
            next_step: None,
            snapshots: Vec::new(),
            history: Vec::new(),
            rounds: Vec::new(),
        }
    }
}

impl Application {
    /// The one way `status` should change. Sets `status`, and raises
    /// `furthest` to the deeper of its current value and the new status —
    /// never backwards, and `Rejected` (which has no `depth()`) never raises
    /// it at all, since a rejection is a terminal outcome, not a deeper
    /// pipeline stage than `Offer`.
    /// Where this application sits now.
    ///
    /// A word this build does not know reads as `Wishlist` — one bad word
    /// costs a card rather than the whole board — while
    /// [`Application::status_word`] keeps what the file actually said.
    pub fn status(&self) -> ApplicationStatus {
        ApplicationStatus::from_word(&self.status_word).unwrap_or(ApplicationStatus::Wishlist)
    }

    /// Whether this build understood the file's `status` word.
    pub fn status_is_recognised(&self) -> bool {
        ApplicationStatus::from_word(&self.status_word).is_some()
    }

    /// The deepest stage this application ever reached, which is not where it
    /// sits now: most interviews end in a rejection, and a funnel counted from
    /// the current column alone would erase every one of them (review P-04).
    ///
    /// Read from [`Self::history`] rather than stored beside it. It was a
    /// field, kept in step by `advance_to` — one fact in two places, and the
    /// stored copy could only ever be a slower, staler way of asking the list
    /// that already knows. `Rejected` has no depth, so a rejection never
    /// counts as deeper than the stage it interrupted.
    pub fn furthest(&self) -> ApplicationStatus {
        self.history
            .iter()
            .filter_map(|change| ApplicationStatus::from_word(&change.to))
            .chain(std::iter::once(self.status()))
            .filter(|stage| stage.depth().is_some())
            .max_by_key(|stage| stage.depth().unwrap_or(0))
            .unwrap_or(ApplicationStatus::Wishlist)
    }

    pub fn advance_to(&mut self, status: ApplicationStatus, on: &str) {
        // Dropping a card back into the column it is already in is not a
        // move. Recording it would invent a transition, and every count over
        // a date range would be a tally of how often the board was fidgeted
        // with rather than of what happened.
        let moved = self.status_word != status.word();

        // Whatever the file used to say, the user has now said otherwise —
        // this is the one place an unrecognised word is allowed to be lost.
        self.status_word = status.word().to_string();

        if moved {
            self.history.push(StageChange {
                at: on.to_string(),
                to: status.word().to_string(),
            });
        }

        // Keep the closure in step with the column, in the one place the
        // column is written. Dragging a card is the commonest way to say an
        // application is over, and it must mean the same thing as saying so
        // in the panel.
        self.closed_as = match (status, self.closed_as) {
            // Moved into the closed column and nothing said about why: it
            // stays unsaid. Guessing "rejected" put words in the user's mouth
            // — a job they withdrew from would have been filed as a company
            // turning them down, and the diagram would have counted it that
            // way. Unsaid is a state the board can ask about; a wrong answer
            // is not.
            (ApplicationStatus::Closed, some) => some,
            // An offer keeps only the endings that follow one.
            (ApplicationStatus::Offer, Some(c @ (Closure::Accepted | Closure::Declined))) => {
                Some(c)
            }
            // Anywhere else the application is live again — which is exactly
            // what dragging a ghosted card back to Applied is for.
            _ => None,
        };
    }

    /// Say how this ended, and file it where that ending belongs.
    ///
    /// The counterpart to [`Self::advance_to`]: that one is told a column and
    /// derives the closure, this one is told a closure and derives the column.
    /// Both go through `advance_to` so the move is recorded in the history
    /// either way.
    pub fn close_as(&mut self, closure: Closure, on: &str) {
        self.advance_to(closure.column(), on);
        self.closed_as = Some(closure);
    }

    /// Reopen — it replied after all.
    pub fn reopen(&mut self, to: ApplicationStatus, on: &str) {
        self.advance_to(to, on);
        self.closed_as = None;
    }

    /// The stage this card was in immediately before `index` in its history —
    /// the previous entry's destination, or the wishlist it started on.
    pub fn stage_before(&self, index: usize) -> ApplicationStatus {
        if index == 0 {
            return ApplicationStatus::Wishlist;
        }
        self.history
            .get(index - 1)
            .and_then(|c| ApplicationStatus::from_word(&c.to))
            .unwrap_or(ApplicationStatus::Wishlist)
    }
}

/// The application board: every company/role the user is tracking. Stored at
/// `<vault>/applications.toml`, mirroring Diary and Library.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Applications {
    pub entries: Vec<Application>,
}

impl Applications {
    /// Number of cards in a given column.
    /// The last date `document_stem` was sent, as the ISO date it was sent on.
    ///
    /// "Sent" is the move into Applied, not the card's creation: a card can sit
    /// in Wishlist for a month with a CV attached and nothing has left the
    /// building. `sent_as` names the document as a label rather than a live
    /// reference (US-04), so this matches on the stem it recorded — a document
    /// renamed since keeps its history under the old name, which is the truth
    /// about what was sent.
    ///
    /// ISO dates compare correctly as strings, which is why they are stored
    /// that way; `max` here needs no parsing and cannot fail on a hand-edited
    /// date it does not understand.
    pub fn last_sent_for(&self, document_stem: &str) -> Option<&str> {
        let applied = ApplicationStatus::Applied.word();
        self.entries
            .iter()
            .filter(|entry| {
                entry
                    .sent_as
                    .as_ref()
                    .is_some_and(|sent| sent.document == document_stem)
            })
            .filter_map(|entry| {
                entry
                    .history
                    .iter()
                    .filter(|change| change.to == applied)
                    .map(|change| change.at.as_str())
                    .max()
            })
            .max()
    }

    pub fn count(&self, status: ApplicationStatus) -> usize {
        self.entries.iter().filter(|a| a.status() == status).count()
    }

    /// Everything not `Rejected` — the header line's "N active".
    pub fn active(&self) -> usize {
        self.entries
            .iter()
            .filter(|a| a.status() != ApplicationStatus::Closed)
            .count()
    }
}

