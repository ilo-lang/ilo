//! Pluggable codegen backends.
//!
//! WIP scaffolding for the Phase 5 codegen layer. See `DESIGN.md` in this
//! directory for the architecture sketch and open questions.
//!
//! Not yet wired into the rest of the compiler. The existing Cranelift AOT
//! path under `src/vm/compile_cranelift.rs` and Python emit under
//! `src/codegen/python.rs` continue to be the canonical paths until this
//! scaffolding is filled in.
