//! Pluggable codegen backends.
//!
//! Phase 5 Stage 5b. Defines the [`Backend`] trait that all codegen backends
//! implement, plus shared [`Artefact`] and [`BackendError`] types. The first
//! concrete impl is [`cranelift::CraneliftBackend`] (AOT native binary).
//!
//! Future stages add Python, WASM Component Model, and Zero backends behind
//! the same trait.
//!
//! ## Shape
//!
//! ```ignore
//! let backend = CraneliftBackend::default();
//! let artefact = backend.emit(&hir, config)?;
//! ```
//!
//! ## Why HIR is the input
//!
//! Every backend consumes [`hir::Program`](crate::hir::Program) — the typed
//! intermediate representation produced by Stage 5a's `hir::lower` pass.
//! This is the single contract between frontend and backends: the lowering
//! pass lives once, backends are pluggable.
//!
//! Cranelift presently also needs the verified AST and the bytecode
//! `CompiledProgram` because its codegen consumes bytecode, not HIR. Stage 5b
//! threads those through [`cranelift::CraneliftConfig`] as a documented
//! side-channel so the refactor stays byte-identical with pre-refactor
//! output. Lowering Cranelift to consume HIR directly is a later-stage
//! concern.
//!
//! ## Why backend `Config` is per-backend (associated type)
//!
//! Each backend's options are distinct (target triple for Cranelift, profile
//! flag for Zero, component-model toggle for WASM). An associated type keeps
//! configuration strongly typed at the call site instead of a `dyn Any`
//! escape hatch.
//!
//! ## Why `BackendError::to_json` exists
//!
//! `ilo build --json` should emit structured failure output that an agent
//! can parse without screen-scraping. The JSON shape is documented on
//! [`BackendError::to_json`].

use crate::ast::Span;
use std::io;
use std::path::PathBuf;

pub mod cranelift;
pub mod python;
pub mod wasm;
pub mod zero;

/// A pluggable codegen backend.
///
/// Implementations live in `src/backend/<name>/`. The default install ships
/// with the Cranelift backend; Stages 5c+ add Python, WASM, and Zero.
///
/// ## HIR-first contract (with side channels)
///
/// The `emit` signature is HIR-first by design so future stages can swap
/// backends without touching `main.rs`. Two backends currently consume
/// additional input via their per-backend `Config` rather than reading the
/// HIR directly:
///
/// - **Cranelift** uses `CraneliftConfig.program: &CompiledProgram` (the
///   VM-compiled bytecode) because the HIR doesn't yet carry the lowered
///   control-flow shape Cranelift needs.
/// - **Python** uses `PythonConfig.program: &Program` (the verified AST)
///   because the HIR doesn't yet carry the expression-level surface
///   (sum types, full match shapes) that Python transpile relies on.
///
/// Both side channels disappear once HIR grows. The `_hir` argument is
/// still threaded through so callers can be HIR-only at the boundary.
pub trait Backend {
    /// Canonical identifier for the backend. Surfaces in diagnostics and
    /// (Stage 5f) the `--backend <name>` CLI flag.
    const NAME: &'static str;

    /// Per-backend configuration.
    type Config;

    /// Produce the output artefact from the typed HIR.
    fn emit(
        &self,
        hir: &crate::hir::Program,
        config: Self::Config,
    ) -> Result<Artefact, BackendError>;
}

/// A produced backend artefact: on-disk path plus a discriminator and a bag
/// of metadata the CLI may surface to the user.
#[derive(Debug, Clone)]
pub struct Artefact {
    /// Output path on disk.
    pub path: PathBuf,
    /// What the file is (native binary, WASM module, source file, etc.).
    pub kind: ArtefactKind,
    /// Optional human-readable metadata.
    pub metadata: ArtefactMetadata,
}

/// Discriminator for what kind of output a backend produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtefactKind {
    /// A native, executable binary linked for the host platform.
    NativeBinary,
    /// A WASM module (`.wasm`).
    Wasm,
    /// A source file in some target language, e.g. `.py`, `.0`, `.c`.
    SourceFile {
        /// File extension without the leading dot.
        ext: String,
    },
}

/// Optional metadata attached to an [`Artefact`]. All fields are best-effort.
#[derive(Debug, Clone, Default)]
pub struct ArtefactMetadata {
    /// Entry function the backend used as the program entry, if applicable.
    pub entry: Option<String>,
    /// Backend-defined notes (e.g. `"bench"` for the bench-binary mode).
    pub notes: Vec<String>,
}

/// Errors a backend can return. Designed to round-trip cleanly through
/// [`BackendError::to_json`] for `ilo build --json`.
#[derive(Debug)]
pub enum BackendError {
    /// Underlying IO failed (write object file, link step, etc.).
    Io(io::Error),
    /// Codegen failed with a structured cause. `code` is an ILO-XXXX style
    /// stable identifier; `message` is human-readable; `span` is the source
    /// location if the failure can be attributed to one.
    CodegenFailed {
        /// Stable error code (e.g. `"ILO-B001"`). Empty string if untyped.
        code: &'static str,
        /// Human-readable message.
        message: String,
        /// Source span responsible for the failure, if known.
        span: Option<Span>,
    },
    /// The HIR carries a construct this backend cannot lower (e.g. WASM
    /// emitting an `MCP` tool call). The CLI should suggest a different
    /// backend.
    UnsupportedFeature {
        /// Short identifier of the unsupported feature.
        feature: String,
        /// Which backend rejected it.
        backend: &'static str,
    },
}

impl BackendError {
    /// Render this error as JSON for `ilo build --json`. Shape:
    ///
    /// ```json
    /// {
    ///   "kind": "io" | "codegen_failed" | "unsupported_feature",
    ///   "message": "human readable",
    ///   "code": "ILO-XXXX",                  // codegen_failed only
    ///   "span": { "start": 0, "end": 0 },    // codegen_failed only, when known
    ///   "feature": "name",                   // unsupported_feature only
    ///   "backend": "cranelift"               // unsupported_feature only
    /// }
    /// ```
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            BackendError::Io(e) => serde_json::json!({
                "kind": "io",
                "message": e.to_string(),
            }),
            BackendError::CodegenFailed {
                code,
                message,
                span,
            } => {
                let mut obj = serde_json::json!({
                    "kind": "codegen_failed",
                    "code": code,
                    "message": message,
                });
                if let Some(s) = span {
                    obj["span"] = serde_json::json!({
                        "start": s.start,
                        "end": s.end,
                    });
                }
                obj
            }
            BackendError::UnsupportedFeature { feature, backend } => serde_json::json!({
                "kind": "unsupported_feature",
                "feature": feature,
                "backend": backend,
                "message": format!(
                    "backend '{}' does not support feature '{}'",
                    backend, feature
                ),
            }),
        }
    }
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::Io(e) => write!(f, "{e}"),
            BackendError::CodegenFailed { message, .. } => write!(f, "{message}"),
            BackendError::UnsupportedFeature { feature, backend } => write!(
                f,
                "backend '{backend}' does not support feature '{feature}'"
            ),
        }
    }
}

impl std::error::Error for BackendError {}

impl From<io::Error> for BackendError {
    fn from(e: io::Error) -> Self {
        BackendError::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_error_json_io() {
        let e = BackendError::Io(io::Error::new(io::ErrorKind::NotFound, "missing"));
        let v = e.to_json();
        assert_eq!(v["kind"], "io");
        assert_eq!(v["message"], "missing");
    }

    #[test]
    fn backend_error_json_codegen_failed_with_span() {
        let e = BackendError::CodegenFailed {
            code: "ILO-B001",
            message: "boom".into(),
            span: Some(Span { start: 4, end: 9 }),
        };
        let v = e.to_json();
        assert_eq!(v["kind"], "codegen_failed");
        assert_eq!(v["code"], "ILO-B001");
        assert_eq!(v["message"], "boom");
        assert_eq!(v["span"]["start"], 4);
        assert_eq!(v["span"]["end"], 9);
    }

    #[test]
    fn backend_error_json_codegen_failed_no_span() {
        let e = BackendError::CodegenFailed {
            code: "",
            message: "boom".into(),
            span: None,
        };
        let v = e.to_json();
        assert_eq!(v["kind"], "codegen_failed");
        assert!(v.get("span").is_none() || v["span"].is_null());
    }

    #[test]
    fn backend_error_json_unsupported_feature() {
        let e = BackendError::UnsupportedFeature {
            feature: "mcp_tool".into(),
            backend: "wasm",
        };
        let v = e.to_json();
        assert_eq!(v["kind"], "unsupported_feature");
        assert_eq!(v["feature"], "mcp_tool");
        assert_eq!(v["backend"], "wasm");
    }
}
