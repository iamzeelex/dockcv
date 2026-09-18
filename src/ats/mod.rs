//! ATS: the file a parser can read, and the proof that it can.
//!
//! An applicant tracking system does not read a CV, it reads whatever its
//! extraction stack recovered from one. This module is where DockCV takes that
//! seriously: [`readers`] reads our own PDF back the several ways those stacks
//! do, and the conformance harness asserts that they all recover the same
//! fields — on every commit, for every layout preset we ship.
//!
//! The rule the whole track is built on, from the review: **no model anywhere
//! near this, and no score out of a hundred.** Parsing is mechanics. A
//! confident number invented by a language model is the thing this exists to
//! be an answer to. See `docs/design/ats.md`.

#[cfg(test)]
pub mod adversarial;
pub mod docx;
pub mod external;
pub mod fields;
// The tag reader moved to `import::pdf_tags` when the importer started using
// it: reading a tagged PDF's structure is a capability of the app and not a
// fixture of its tests. The harness still calls it `readers`, which is what it
// is from here.
pub use crate::import::pdf_tags as readers;

#[cfg(test)]
mod conformance;
