//! What the import screen is told when a file will not come in.
//!
//! An import failure is the first thing a new user can hit (US-01), and the two
//! things they need are not "an error message": they need to know **whether the
//! file is at fault**, and they need somewhere to go next. A `String` carried
//! neither — and the string the wizard did produce was not rendered at all, so
//! a failed import dropped silently back to the drop zone.
//!
//! So a failure carries three things:
//!
//! * a **headline** naming what happened, in the user's words;
//! * a **detail** line that says, where it is true, that the file is fine;
//! * **remedies**, in the order worth trying — and the last one is always the
//!   one that cannot fail, which is writing the CV by hand.

/// A failed import, said in a way a person can act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportError {
    /// What happened. One line, no jargon, no file paths.
    pub headline: String,
    /// Why — and in particular whether the file is at fault. Empty when there
    /// is nothing honest to add.
    pub detail: String,
    /// What to try, best first. May be empty; the screen always offers to start
    /// a blank CV regardless, because that is the one route that always works.
    pub remedies: Vec<String>,
    /// The file is a picture of a document rather than a document.
    ///
    /// A flag rather than a match on the headline. The headline is copy and
    /// will be reworded; a screen that decides what to offer by reading our own
    /// prose breaks the day somebody improves it. This one distinction earns a
    /// field because only this failure has a route out that the others do not
    /// — see `views/import/assistant.rs`.
    pub no_text_layer: bool,
}

impl ImportError {
    pub fn new(headline: impl Into<String>) -> Self {
        Self {
            headline: headline.into(),
            detail: String::new(),
            remedies: Vec::new(),
            no_text_layer: false,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    pub fn remedy(mut self, remedy: impl Into<String>) -> Self {
        self.remedies.push(remedy.into());
        self
    }

    /// Mark this as "the pages are images".
    pub fn no_text_layer(mut self) -> Self {
        self.no_text_layer = true;
        self
    }

    /// Whether a model reading the picture is worth offering.
    pub fn is_unreadable_image(&self) -> bool {
        self.no_text_layer
    }
}

/// Engines that still report a bare string get a headline and nothing else.
///
/// Deliberately lossy in one direction only: a message written for a person
/// stays a message written for a person, and one that was not gains no
/// reassurance it has not earned.
impl From<String> for ImportError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.headline)?;
        if !self.detail.is_empty() {
            write!(f, " — {}", self.detail)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_message_becomes_a_headline_and_claims_nothing_else() {
        let error: ImportError = "Could not read this file".to_string().into();
        assert_eq!(error.headline, "Could not read this file");
        assert!(error.detail.is_empty());
        assert!(error.remedies.is_empty());
    }

    /// The log line and the crash reporter both want one string.
    #[test]
    fn display_joins_the_headline_and_the_detail() {
        let error = ImportError::new("This PDF has no text in it")
            .detail("the file is fine")
            .remedy("Export it again");
        assert_eq!(
            error.to_string(),
            "This PDF has no text in it — the file is fine"
        );
    }
}
