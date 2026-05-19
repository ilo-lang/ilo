//! High-level Intermediate Representation.
//!
//! HIR sits between the verified AST and concrete code emission. Every Phase 5
//! backend (Cranelift AOT, Python emit, WASM Component Model, Zero transpile)
//! consumes HIR; the lowering pass from AST → HIR is the single place where
//! frontend desugaring lives.
//!
//! Stage 5a defined the HIR shape and the lowering pass. Stage 5f removed the
//! throwaway raise/walker scaffolding that proved lowering was information
//! preserving — the cross-backend conformance suite supersedes it.
//!
//! See `DESIGN.md` for the shape decisions, departures from the AST, and
//! open questions for later Phase 5 stages.

pub mod decl;
pub mod expr;
pub mod lower;
pub mod program;
pub mod types;

pub use decl::{Decl, Param};
pub use expr::{Body, Expr, MatchArm, Pattern, Stmt};
pub use lower::{LowerError, lower};
pub use program::Program;
pub use types::Ty;
