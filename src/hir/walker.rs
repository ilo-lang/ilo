//! Throwaway HIR walker for round-trip testing.
//!
//! Stage 5a doesn't ship a real HIR interpreter. The existing tree-walker is
//! 10k+ lines and rewriting it against HIR before any backend exists would
//! pay zero dividend. Instead we raise HIR back to AST and reuse
//! `interpreter::run`. The round-trip test in `tests/hir_roundtrip.rs`
//! therefore proves:
//!
//!   1. Lowering preserves all AST information needed for execution.
//!   2. Raising reconstructs an AST that the existing interpreter accepts.
//!
//! Stage 5f deletes this module along with `raise.rs` once real backends
//! drive HIR directly.

use crate::hir;
use crate::interpreter::{self, RuntimeError, Value};

/// Walk an HIR program by raising it to AST and dispatching through the
/// existing tree interpreter.
///
/// `func_name` selects the entry function (mirrors `interpreter::run`).
/// `None` runs the first declared function.
pub fn walk(
    hir: &hir::Program,
    func_name: Option<&str>,
    args: Vec<Value>,
) -> Result<Value, RuntimeError> {
    let ast = hir::raise::raise(hir);
    interpreter::run(&ast, func_name, args)
}
