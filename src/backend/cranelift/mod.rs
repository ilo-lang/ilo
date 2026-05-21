//! Cranelift AOT backend (Phase 5 Stage 5b).
//!
//! This module is the trait surface for the Cranelift native-binary backend.
//! The codegen itself still lives in [`crate::vm::compile_cranelift`] — that
//! file is a 6.6k-LOC bytecode-to-Cranelift compiler, and the Stage 5b brief
//! explicitly allows a thin re-export so the byte-identical assertion holds.
//! The codegen module is moved behind this trait in a later stage when
//! Cranelift consumes HIR directly.
//!
//! ## Why the [`CraneliftConfig`] carries verified AST + bytecode
//!
//! The trait promises `emit(&hir, config)`. Cranelift's existing pipeline
//! consumes a `vm::CompiledProgram` (bytecode + type registry), not HIR.
//! Lowering Cranelift to consume HIR directly is a non-trivial change that
//! the brief defers (Risk 3 mitigation: "carry the missing info via
//! Cranelift-specific side channel"). The bytecode is produced from the
//! same verified AST the HIR was lowered from, so semantically the two
//! inputs are redundant; the duplication is a temporary scaffolding cost.

#[cfg(feature = "cranelift")]
use std::path::PathBuf;

use super::{Artefact, ArtefactKind, ArtefactMetadata, Backend, BackendError};

/// The Cranelift AOT backend: emits a native, statically linked binary for
/// the host platform.
#[derive(Debug, Default, Clone, Copy)]
pub struct CraneliftBackend;

/// Cranelift-specific configuration.
///
/// The `program` field is the bytecode `CompiledProgram` the existing
/// codegen consumes; see the module-level doc comment for the rationale.
pub struct CraneliftConfig<'a> {
    /// Bytecode program produced by `vm::compile`. Held by reference so the
    /// caller retains ownership of the (potentially large) compiled program.
    #[cfg(feature = "cranelift")]
    pub program: &'a crate::vm::CompiledProgram,
    /// Name of the entry function to wire up as `main()` in the produced
    /// binary. Must exist in `program.func_names`.
    pub entry: &'a str,
    /// Output path for the produced binary. Cranelift writes a sibling
    /// `<output_path>.o` object file during the build; it is cleaned up on
    /// success or failure.
    pub output_path: &'a str,
    /// When `true`, emit a benchmark binary that loops and reports ns/call.
    /// Default `false` (regular AOT binary).
    pub bench: bool,
    /// Phantom lifetime carrier for builds without the `cranelift` feature
    /// where `program` is omitted.
    #[cfg(not(feature = "cranelift"))]
    pub _phantom: std::marker::PhantomData<&'a ()>,
}

impl Backend for CraneliftBackend {
    const NAME: &'static str = "cranelift";

    type Config = CraneliftConfig<'static>;

    /// Emit a native binary at `config.output_path`.
    ///
    /// The HIR argument is presently unused; see the module-level doc on
    /// why the Cranelift codegen consumes bytecode via `config.program`
    /// instead.
    #[cfg(feature = "cranelift")]
    fn emit(
        &self,
        _hir: &crate::hir::Program,
        config: Self::Config,
    ) -> Result<Artefact, BackendError> {
        let result = if config.bench {
            crate::vm::compile_cranelift::compile_to_bench_binary(
                config.program,
                config.entry,
                config.output_path,
            )
        } else {
            crate::vm::compile_cranelift::compile_to_binary(
                config.program,
                config.entry,
                config.output_path,
            )
        };

        result.map_err(|message| BackendError::CodegenFailed {
            code: "",
            message,
            span: None,
        })?;

        let mut notes = Vec::new();
        if config.bench {
            notes.push("bench".to_string());
        }

        Ok(Artefact {
            path: PathBuf::from(config.output_path),
            kind: ArtefactKind::NativeBinary,
            metadata: ArtefactMetadata {
                entry: Some(config.entry.to_string()),
                notes,
            },
        })
    }

    /// Build without the `cranelift` feature is rejected at runtime; the
    /// trait impl exists so callers compile unconditionally.
    #[cfg(not(feature = "cranelift"))]
    fn emit(
        &self,
        _hir: &crate::hir::Program,
        _config: Self::Config,
    ) -> Result<Artefact, BackendError> {
        Err(BackendError::UnsupportedFeature {
            feature: "cranelift_aot".into(),
            backend: Self::NAME,
        })
    }
}

/// Convenience free-function entry point: equivalent to
/// `CraneliftBackend.emit(hir, config)` but accepts any lifetime on
/// `CraneliftConfig`.
///
/// Stage 5b's [`Backend`] trait pins the associated `Config` to a single
/// concrete lifetime (`'static` here), which makes calling it from a
/// function with local references awkward. The cleaner shape — a GAT
/// `Config<'a>` — is deferred until at least one more backend lands and
/// the trait can be designed against two real callers instead of one.
/// Until then, the CLI dispatch site (`main::compile_cmd`) calls this
/// free function rather than the trait method.
#[cfg(feature = "cranelift")]
pub fn emit<'a>(
    _hir: &crate::hir::Program,
    config: CraneliftConfig<'a>,
) -> Result<Artefact, BackendError> {
    let result = if config.bench {
        crate::vm::compile_cranelift::compile_to_bench_binary(
            config.program,
            config.entry,
            config.output_path,
        )
    } else {
        crate::vm::compile_cranelift::compile_to_binary(
            config.program,
            config.entry,
            config.output_path,
        )
    };
    result.map_err(|message| BackendError::CodegenFailed {
        code: "",
        message,
        span: None,
    })?;

    let mut notes = Vec::new();
    if config.bench {
        notes.push("bench".to_string());
    }

    Ok(Artefact {
        path: PathBuf::from(config.output_path),
        kind: ArtefactKind::NativeBinary,
        metadata: ArtefactMetadata {
            entry: Some(config.entry.to_string()),
            notes,
        },
    })
}
