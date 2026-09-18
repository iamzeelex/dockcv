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

pub mod docx;
pub mod external;
pub mod fields;
pub mod readers;

#[cfg(test)]
mod conformance;
