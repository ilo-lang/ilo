//! WASM Component Model backend (Phase 5 Stage 5d).
//!
//! Emits `.wasm` (and optionally `.wit`) artefacts via the `wasm-encoder`
//! crate. The backend walks HIR directly and supports a constrained but
//! growing subset of the language: top-level entry functions whose body
//! reduces to a sequence of `prnt "<literal>"` calls (plus the implicit
//! `~v`/`Ok` return shape). Anything outside the subset returns
//! [`BackendError::UnsupportedFeature`] with an `ILO-B2##` code and a hint
//! pointing the user at the Cranelift native backend.
//!
//! ## Targets
//!
//! - [`WasmTarget::Component`] (default): emit a wasm core module + run
//!   `wasm-tools component new` with the bundled WASI preview1 adapter to
//!   wrap it as a Component Model component. Also writes a sibling `.wit`
//!   describing the exported world.
//! - [`WasmTarget::Wasip1`]: emit a plain WASI preview1 core module.
//! - [`WasmTarget::Wasip2`]: same wire format as `Wasip1` today; placeholder
//!   for the eventual preview2 split.
//! - [`WasmTarget::UnknownUnknown`]: browser-style wasm with no host imports.
//!   Using `prnt`/`rd`/etc on this target errors with `ILO-B201`.
//!
//! ## HIR consumption path
//!
//! Direct HIR. The backend never touches AST or bytecode — keeping it true to
//! the Stage 5a contract. The trade-off is range: only the hello-world subset
//! is supported in Stage 5d. Subsequent stages will broaden it as HIR carries
//! enough information for arithmetic, branching, and lambda capture.
//!
//! ## Error namespace
//!
//! - `ILO-B201` — builtin not supported on this target (capability mismatch)
//! - `ILO-B202` — HIR construct not supported by the WASM backend yet
//! - `ILO-B203` — wasm-tools subprocess failure (Component Model wrap)
//! - `ILO-B204` — IO failure writing artefact
//! - `ILO-B205` — entry function not found

use std::path::PathBuf;
use std::process::Command;

use super::{Artefact, ArtefactKind, ArtefactMetadata, Backend, BackendError};
use crate::ast::Literal;
use crate::hir::{
    decl::Decl,
    expr::{Body, Expr, Stmt},
    program::Program,
};

mod emit;
pub use emit::{emit_core_module, CapabilitySet};

/// Bundled WASI preview1 → preview2 reactor adapter. Pinned to the version
/// shipped with the Wasmtime v25 release; refreshed alongside the
/// `wasm-encoder` / `wasm-tools` dep bump.
pub const WASI_ADAPTER_BYTES: &[u8] =
    include_bytes!("../../../assets/wasi-adapter/wasi_snapshot_preview1.reactor.wasm");

/// The WASM Component Model backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct WasmBackend;

/// WASM output target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmTarget {
    /// `wasm32-wasip1` — plain WASI preview1 core module.
    Wasip1,
    /// `wasm32-wasip2` — WASI preview2. Same encoder output as `Wasip1`
    /// for now; placeholder until preview2 host imports land.
    Wasip2,
    /// `wasm32-component` (default) — core module wrapped as a Component
    /// Model component via `wasm-tools component new`.
    Component,
    /// `wasm32-unknown-unknown` — browser-style, no host imports.
    UnknownUnknown,
}

impl WasmTarget {
    /// Human-readable name used in error messages and CLI surfaces.
    pub fn name(self) -> &'static str {
        match self {
            WasmTarget::Wasip1 => "wasm32-wasip1",
            WasmTarget::Wasip2 => "wasm32-wasip2",
            WasmTarget::Component => "wasm32-component",
            WasmTarget::UnknownUnknown => "wasm32-unknown-unknown",
        }
    }

    /// Parse a `--target <name>` CLI argument. Accepts both the canonical
    /// triple form and a couple of common aliases so users can type either
    /// `wasm32-wasi` or `wasm32-wasip1`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "wasm32-wasip1" | "wasm32-wasi" => Some(WasmTarget::Wasip1),
            "wasm32-wasip2" => Some(WasmTarget::Wasip2),
            "wasm32-component" => Some(WasmTarget::Component),
            "wasm32-unknown-unknown" | "wasm32-web" => Some(WasmTarget::UnknownUnknown),
            _ => None,
        }
    }
}

/// WASM-specific configuration.
pub struct WasmConfig {
    /// Output target (Component Model by default).
    pub target: WasmTarget,
    /// Output path for the `.wasm` artefact. A sibling `.wit` is written
    /// alongside for [`WasmTarget::Component`] builds.
    pub output_path: PathBuf,
    /// Entry function name. Defaults to the first function in source order
    /// when `None`, mirroring the tree interpreter and Cranelift backend.
    pub entry: Option<String>,
}

impl Backend for WasmBackend {
    const NAME: &'static str = "wasm";

    type Config = WasmConfig;

    fn emit(&self, hir: &Program, config: Self::Config) -> Result<Artefact, BackendError> {
        emit_program(hir, config)
    }
}

/// Convenience free function: same as `WasmBackend.emit` without going
/// through the trait. Used by `main::compile_cmd` for symmetry with
/// `cranelift::emit` and `python::emit`.
pub fn emit(hir: &Program, config: WasmConfig) -> Result<Artefact, BackendError> {
    emit_program(hir, config)
}

/// Per-builtin capability lookup. Returns `Ok(())` if the builtin is
/// available on `target`, otherwise an [`BackendError::UnsupportedFeature`]
/// whose message carries the `ILO-B201` code and a hint pointing at a
/// supported target. Capabilities mirror `docs/wasm-capabilities.md` (and
/// the prep matrix at `docs/phase-5-prep/wasm-capability-matrix.md`).
pub fn check_builtin(builtin: &str, target: WasmTarget) -> Result<(), BackendError> {
    let supported = match (builtin, target) {
        // Pure ops are everywhere.
        ("len" | "hd" | "tl" | "at" | "map" | "flt" | "rdc" | "rng", _) => true,

        // stdout / clock / random / env / fs / http have host requirements.
        (
            "prnt" | "now" | "now-ms" | "env" | "rd" | "wr" | "get" | "post",
            WasmTarget::Wasip1 | WasmTarget::Wasip2 | WasmTarget::Component,
        ) => true,
        ("prnt" | "now" | "now-ms" | "env" | "rd" | "wr" | "get" | "post", WasmTarget::UnknownUnknown) => false,

        // `run` (subprocess spawn) is not supported on any wasm target —
        // there is no WASI or Component Model interface for it.
        ("run", _) => false,

        // Default: assume pure (arithmetic helpers etc.) and allow on every
        // target. The HIR walker still gates on what it can lower, so an
        // unknown builtin that slips past here will fail with ILO-B202
        // rather than masquerading as supported.
        _ => true,
    };

    if supported {
        Ok(())
    } else {
        let hint = match builtin {
            "run" => "no WASM target supports `run`. Build with the native Cranelift backend (drop --wasm).".to_string(),
            _ => format!(
                "use --target wasm32-wasip1 or --target wasm32-component (default). `{}` needs WASI host imports.",
                builtin
            ),
        };
        Err(BackendError::CodegenFailed {
            code: "ILO-B201",
            message: format!(
                "builtin `{}` is not supported on {}. hint: {}",
                builtin,
                target.name(),
                hint
            ),
            span: None,
        })
    }
}

/// Walker rejection helper for HIR constructs the WASM backend doesn't
/// lower yet. Emits a structured `ILO-B202` so the conformance harness
/// (and any other consumer that gates on the `ILO-B###` namespace) can
/// classify this as a soft "unsupported" rather than a hard failure.
///
/// The free-form `BackendError::UnsupportedFeature` variant has no error
/// code, so a message like "backend 'wasm' does not support feature 'X'"
/// would slip past a `\bILO-B[0-9]{3}\b` gate and be miscounted as a
/// real failure. Routing through `CodegenFailed` keeps the gate honest.
fn unsupported(feature: impl Into<String>) -> BackendError {
    let feature = feature.into();
    BackendError::CodegenFailed {
        code: "ILO-B202",
        message: format!(
            "{} backend does not support feature '{}'",
            WasmBackend::NAME,
            feature
        ),
        span: None,
    }
}

fn codegen(code: &'static str, message: impl Into<String>) -> BackendError {
    BackendError::CodegenFailed {
        code,
        message: message.into(),
        span: None,
    }
}

fn emit_program(hir: &Program, config: WasmConfig) -> Result<Artefact, BackendError> {
    let entry_decl = match &config.entry {
        Some(name) => hir
            .function(name)
            .ok_or_else(|| codegen("ILO-B205", format!("entry function `{}` not found", name)))?,
        None => hir
            .first_function()
            .ok_or_else(|| codegen("ILO-B205", "no function declarations to emit"))?,
    };

    let (entry_name, body) = match entry_decl {
        Decl::Function { name, body, .. } => (name.clone(), body),
        _ => return Err(codegen("ILO-B205", "entry is not a function")),
    };

    // Collect the `prnt`-ed strings in source order. Any HIR construct we
    // don't understand surfaces as `ILO-B202` so the user gets a clear
    // pointer at the native backend.
    let mut strings: Vec<String> = Vec::new();
    walk_body(body, &mut strings, config.target)?;

    let caps = CapabilitySet {
        target: config.target,
        needs_stdout: !strings.is_empty(),
    };

    if caps.needs_stdout {
        // Re-check `prnt` against the target. `walk_body` does this per
        // call too, but this guards against an empty corpus on
        // unknown-unknown still claiming stdout.
        check_builtin("prnt", config.target)?;
    }

    let wasm_bytes = emit_core_module(&strings, caps)
        .map_err(|e| codegen("ILO-B202", format!("wasm encode failed: {}", e)))?;

    // Write the core module to disk. For Component target we then run
    // `wasm-tools component new` to wrap it.
    let core_path = match config.target {
        WasmTarget::Component => {
            // Stash the core module next to the final output with a `.core.wasm`
            // suffix so users can inspect it if the wrap fails.
            let mut p = config.output_path.clone();
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "out".to_string());
            p.set_file_name(format!("{}.core.wasm", stem));
            p
        }
        _ => config.output_path.clone(),
    };
    std::fs::write(&core_path, &wasm_bytes)
        .map_err(|e| codegen("ILO-B204", format!("write {}: {}", core_path.display(), e)))?;

    let mut notes: Vec<String> = vec![config.target.name().to_string()];

    if matches!(config.target, WasmTarget::Component) {
        // Component Model wrap: drop the adapter to a NamedTempFile (RAII
        // cleanup, unique filename so concurrent ilo builds can't collide
        // on a reused PID) and shell out to wasm-tools.
        let mut adapter_file = tempfile::Builder::new()
            .prefix("ilo-wasi-adapter-")
            .suffix(".wasm")
            .tempfile()
            .map_err(|e| codegen("ILO-B204", format!("create adapter tempfile: {}", e)))?;
        {
            use std::io::Write;
            adapter_file
                .write_all(WASI_ADAPTER_BYTES)
                .map_err(|e| codegen("ILO-B204", format!("write adapter: {}", e)))?;
            adapter_file
                .flush()
                .map_err(|e| codegen("ILO-B204", format!("flush adapter: {}", e)))?;
        }
        let adapter_path = adapter_file.path().to_path_buf();

        let component_out = config.output_path.clone();
        let status = Command::new("wasm-tools")
            .arg("component")
            .arg("new")
            .arg(&core_path)
            .arg("--adapt")
            .arg(format!(
                "wasi_snapshot_preview1={}",
                adapter_path.display()
            ))
            .arg("-o")
            .arg(&component_out)
            .output();

        // `adapter_file` drops at end of scope — RAII delete. No manual
        // remove_file with a swallowed error.

        let output = status.map_err(|e| {
            codegen(
                "ILO-B203",
                format!(
                    "failed to invoke `wasm-tools component new`: {}. Install with `cargo install wasm-tools` or `brew install wasm-tools`.",
                    e
                ),
            )
        })?;
        if !output.status.success() {
            // Mirror run_zero_build: include both streams since wasm-tools
            // doesn't guarantee which one a given diagnostic lands on.
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut detail = String::new();
            if !stderr.trim().is_empty() {
                detail.push_str(stderr.trim());
            }
            if !stdout.trim().is_empty() {
                if !detail.is_empty() {
                    detail.push('\n');
                }
                detail.push_str(stdout.trim());
            }
            // Some wasm-tools failures (signals, exec errors) leave both
            // streams empty. Without a fallback we end up with a stray
            // trailing `": "` and no useful diagnostic.
            if detail.is_empty() {
                detail.push_str("(no output captured)");
            }
            return Err(codegen(
                "ILO-B203",
                format!("wasm-tools component new failed ({}): {}", output.status, detail),
            ));
        }

        // Write a sibling `.wit` describing the exported world.
        let mut wit_path = component_out.clone();
        wit_path.set_extension("wit");
        let wit = generate_wit(&entry_name);
        std::fs::write(&wit_path, &wit)
            .map_err(|e| codegen("ILO-B204", format!("write {}: {}", wit_path.display(), e)))?;

        notes.push(format!("wit:{}", wit_path.display()));
    }

    Ok(Artefact {
        path: config.output_path,
        kind: ArtefactKind::Wasm,
        metadata: ArtefactMetadata {
            entry: Some(entry_name),
            notes,
        },
    })
}

/// Walk a HIR body collecting `prnt "<literal>"` calls. Any other shape
/// returns `ILO-B202`.
fn walk_body(body: &Body, strings: &mut Vec<String>, target: WasmTarget) -> Result<(), BackendError> {
    for stmt in &body.stmts {
        walk_stmt(stmt, strings, target)?;
    }
    if let Some(tail) = &body.tail {
        // Tail expression. `prnt` calls at the tail are treated as
        // side-effect statements — the implicit return value isn't observable
        // from a host that's just running `_start`. `Ok`/literal tails are
        // similarly no-ops at the wasm boundary today.
        match tail {
            Expr::Call { function, .. } if function == "prnt" => {
                walk_call(tail, strings, target)?;
            }
            Expr::Ok { .. } | Expr::Literal { .. } => {}
            _ => return Err(unsupported("hir-tail-expr (Stage 5d emits stdout-only)")),
        }
    }
    Ok(())
}

fn walk_stmt(stmt: &Stmt, strings: &mut Vec<String>, target: WasmTarget) -> Result<(), BackendError> {
    match stmt {
        Stmt::Expr { value, .. } => walk_call(value, strings, target),
        _ => Err(unsupported(
            "hir-stmt (Stage 5d only lowers top-level expression statements)",
        )),
    }
}

fn walk_call(expr: &Expr, strings: &mut Vec<String>, target: WasmTarget) -> Result<(), BackendError> {
    match expr {
        Expr::Call { function, args, .. } => {
            check_builtin(function, target)?;
            if function == "prnt" {
                if args.len() != 1 {
                    return Err(unsupported("prnt with non-unary args"));
                }
                let s = match &args[0] {
                    Expr::Literal {
                        value: Literal::Text(s),
                        ..
                    } => s.clone(),
                    Expr::Literal {
                        value: Literal::Number(n),
                        ..
                    } => format_number(*n),
                    Expr::Literal {
                        value: Literal::Bool(b),
                        ..
                    } => b.to_string(),
                    _ => return Err(unsupported("prnt of non-literal (Stage 5d limit)")),
                };
                strings.push(s);
                Ok(())
            } else {
                Err(unsupported(format!(
                    "call `{}` (Stage 5d only lowers `prnt` literals)",
                    function
                )))
            }
        }
        _ => Err(unsupported("non-call expression statement")),
    }
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.is_finite() {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

fn generate_wit(entry: &str) -> String {
    format!(
        "// Auto-generated by the ilo WASM backend (Phase 5 Stage 5d).\n\
         package ilo:program;\n\
         \n\
         world program {{\n\
         \x20\x20// Entry function: {entry}.\n\
         \x20\x20// Uses wasi:cli/stdout via the WASI preview1 adapter.\n\
         \x20\x20import wasi:cli/stdout@0.2.0;\n\
         \x20\x20export run: func();\n\
         }}\n",
        entry = entry,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    // Regression: the conformance harness gates Outcome::Unsupported on a
    // `\bILO-B(?:201|202|205|...)\b` regex against stderr. Before this fix
    // `unsupported()` returned `BackendError::UnsupportedFeature`, whose
    // Display is `"backend 'wasm' does not support feature 'X'"` — no code,
    // so walker rejections were misclassified as hard failures. This test
    // pins both halves: the variant is `CodegenFailed` with code `ILO-B202`,
    // and the rendered message satisfies the conformance regex.
    #[test]
    fn unsupported_emits_ilo_b202_matching_conformance_gate() {
        let err = unsupported("some-hir-construct");
        match &err {
            BackendError::CodegenFailed { code, message, .. } => {
                assert_eq!(*code, "ILO-B202", "expected unsupported() to use ILO-B202");
                assert!(
                    message.contains("some-hir-construct"),
                    "feature name should survive into the message: {message}"
                );
            }
            other => panic!(
                "expected CodegenFailed; got {other:?}. UnsupportedFeature would slip past the conformance regex."
            ),
        }
        // The Display path is what reaches the conformance harness via the
        // `ilo build` stderr stream. It needs to carry the code.
        let rendered = format!("{err}");
        let re = Regex::new(r"\bILO-B(?:201|202|205|301|302|305)\b").unwrap();
        assert!(
            re.is_match(&rendered),
            "rendered error must match conformance unsupported regex: {rendered}"
        );
    }
}
