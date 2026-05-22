#![warn(clippy::all)]
#![deny(rust_2018_idioms)]

pub mod ast;
pub mod backend;
pub mod builtins;
pub mod caps;
pub mod cli_parse;
pub mod codegen;
pub mod diagnostic;
pub mod graph;
pub mod hir;
pub mod lexer;
pub mod parser;
pub mod pkg;
pub mod rng;
// `interpreter` is the tree-walker module. Main soft-deprecates it; next's hard
// deletion is deferred to a follow-up to keep this merge buildable.
#[doc(hidden)]
pub mod interpreter;
// Shared runtime primitives: Value, MapKey, RuntimeError, math helpers, and
// the per-builtin implementations dispatched by the VM/Cranelift tree-bridge
// (call_builtin_for_bridge). The tree-walking eval loop was removed in
// 0.13.0; only the bridge surface and shared types live here now.
pub mod runtime;
pub mod runtime_guard;
pub mod tools;
pub mod verify;
pub mod vm;
