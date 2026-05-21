#![warn(clippy::all)]
#![deny(rust_2018_idioms)]

pub mod ast;
pub mod builtins;
pub mod caps;
pub mod cli_parse;
pub mod codegen;
pub mod diagnostic;
pub mod graph;
// `interpreter` is soft-deprecated as a user-selectable engine but stays as
// the internal runtime for HOF callbacks that VM/Cranelift bail to, plus
// shared runtime primitives (Value, MapKey, RuntimeError, math helpers).
// Marked `#[doc(hidden)]` so it drops out of rustdoc / IDE completion; `pub`
// is still required for the in-tree `ilo` bin (a separate crate from this
// lib) to wire up dispatch and for the tree-walker tests in `tests/`. New
// external code should not depend on this module - real removal is deferred
// to 0.13.0+ once the tree-bridge HOFs are lifted natively and the shared
// runtime types (Value/MapKey/RuntimeError + math helpers) are extracted to
// a non-engine module.
#[doc(hidden)]
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod tools;
pub mod verify;
pub mod vm;
