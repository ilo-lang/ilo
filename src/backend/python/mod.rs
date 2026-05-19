//! Python transpile backend (Phase 5 Stage 5c).
//!
//! Moves the existing Python transpilation (previously
//! `src/codegen/python.rs`) behind the [`Backend`] trait. The emit code itself
//! lives in [`emit`] and still consumes the verified AST directly — the
//! current HIR (Stage 5a) does not yet carry the full surface area Python
//! transpile needs (expression shape, sum types, etc.). Lowering Python emit
//! to consume HIR is a later refinement once HIR grows; the brief explicitly
//! allows the AST-input shape to stay as-is so the refactor is byte-identical.
//!
//! ## CLI surface
//!
//! ```text
//! ilo build file.ilo --py            # → file.py
//! ilo build file.ilo --py -o out.py  # → out.py
//! ```
//!
//! The legacy `ilo <file-or-code> --emit python` form is removed in this
//! stage (manifesto Principle 2: one canonical form). Invoking the old flag
//! prints a migration hint and exits 2.

use std::path::PathBuf;

use super::{Artefact, ArtefactKind, ArtefactMetadata, Backend, BackendError};
use crate::ast::Program;

pub mod emit;

/// Public re-export of the underlying emit-to-string function. The CLI
/// `--py` path goes through [`PythonBackend::emit`] which writes to disk;
/// callers who want the source text directly (e.g. the `--bench` Python
/// comparison) use this function.
pub use emit::emit as emit_to_string;

/// The Python transpile backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct PythonBackend;

/// Python-specific configuration.
///
/// The `program` field is the verified AST the transpile consumes. See the
/// module-level doc on why Python emit still takes the AST rather than HIR.
pub struct PythonConfig<'a> {
    /// Verified AST to transpile.
    pub program: &'a Program,
    /// Output path for the produced `.py` file.
    pub output_path: PathBuf,
}

impl Backend for PythonBackend {
    const NAME: &'static str = "python";

    type Config = PythonConfig<'static>;

    /// Emit a Python source file at `config.output_path`.
    ///
    /// The HIR argument is presently unused; see the module-level doc on why
    /// Python transpile consumes the AST via `config.program` instead.
    fn emit(
        &self,
        _hir: &crate::hir::Program,
        config: Self::Config,
    ) -> Result<Artefact, BackendError> {
        let mut source = emit::emit(config.program);
        // Match the pre-refactor `println!("{}", emit(&program))` behaviour
        // so the on-disk byte stream is identical to what `--emit python`
        // wrote to stdout: a single trailing newline. The byte-identical
        // regression test in `tests/python_emit_byte_identical.rs` pins this.
        if !source.ends_with('\n') {
            source.push('\n');
        }
        std::fs::write(&config.output_path, &source)?;
        Ok(Artefact {
            path: config.output_path,
            kind: ArtefactKind::SourceFile {
                ext: "py".to_string(),
            },
            metadata: ArtefactMetadata::default(),
        })
    }
}

/// Convenience free-function entry point: equivalent to
/// `PythonBackend.emit(hir, config)` but accepts any lifetime on
/// [`PythonConfig`].
///
/// Mirrors the rationale on [`crate::backend::cranelift::emit`]: the trait
/// pins `Config` to `'static`, which is awkward when the caller has local
/// references. A future GAT-based redesign lets the trait carry the lifetime.
pub fn emit<'a>(
    _hir: &crate::hir::Program,
    config: PythonConfig<'a>,
) -> Result<Artefact, BackendError> {
    let mut source = emit::emit(config.program);
    if !source.ends_with('\n') {
        source.push('\n');
    }
    std::fs::write(&config.output_path, &source)?;
    Ok(Artefact {
        path: config.output_path,
        kind: ArtefactKind::SourceFile {
            ext: "py".to_string(),
        },
        metadata: ArtefactMetadata::default(),
    })
}
