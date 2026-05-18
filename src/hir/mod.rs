//! High-level Intermediate Representation.
//!
//! HIR sits between the verified AST and concrete code emission. Every Phase 5
//! backend (Cranelift AOT, Python emit, WASM Component Model, Zero transpile)
//! consumes HIR; the lowering pass from AST → HIR is the single place where
//! frontend desugaring lives.
//!
//! Stage 5a (this module) defines the HIR shape, the lowering pass, a raise
//! pass (HIR → AST) used only by the round-trip test harness, and a
//! throwaway walker that proves the lowering is information-preserving.
//!
//! See `DESIGN.md` for the shape decisions, departures from the AST, and
//! open questions for later Phase 5 stages.

pub mod decl;
pub mod expr;
pub mod lower;
pub mod program;
pub mod raise;
pub mod types;
pub mod walker;

pub use decl::{Decl, Param};
pub use expr::{Body, Expr, MatchArm, Pattern, Stmt};
pub use lower::{LowerError, lower};
pub use program::Program;
pub use types::Ty;
pub use walker::walk;
