//! HIR types — re-export of `verify::Ty`.
//!
//! HIR uses the verifier's type lattice unchanged. Re-exporting (rather than
//! duplicating) keeps the two in sync if the verifier grows new variants
//! (effect rows, refinement types, etc.) in later phases. See `DESIGN.md`.

pub use crate::verify::Ty;
