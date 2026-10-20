//! Text-layer classification and parsing into canonical resume structures.

pub mod bidi;
pub mod classifier;
pub mod lines;
pub mod model;
pub mod notes;

pub use classifier::{classify_lines, classify_raw_text, join_split_entry_headers};
pub use model::{ImportedDoc, Unplaced, UnplacedOffer, UnplacedSource};
pub use notes::{Note, Part};
