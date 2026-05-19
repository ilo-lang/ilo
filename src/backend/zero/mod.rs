//! Zero transpile backend (Phase 5 Stage 5e).
//!
//! Emits idiomatic Zero source (`.0`) from HIR. Exposed via the CLI as
//! `ilo build file.ilo --0` (source only) and `ilo build file.ilo --0bin`
//! (source then subprocess `zero build` for a native binary).
//!
//! ## HIR consumption path
//!
//! Direct HIR. Matches the WASM backend's choice and keeps the contract
//! with Stage 5a clean. The walker is narrow in v1: a top-level function
//! whose body is a sequence of `prnt "<literal>"` calls. Anything outside
//! the subset surfaces as [`BackendError::CodegenFailed`] with an
//! `ILO-B3##` code so users get a clear pointer at the workaround.
//!
//! ## Error namespace
//!
//! - `ILO-B301` — `zero check`/`zero build` rejected the emitted source
//! - `ILO-B302` — HIR construct not supported by the Zero backend yet
//! - `ILO-B303` — `zero` compiler missing on PATH (--0bin only)
//! - `ILO-B304` — IO failure writing artefact
//! - `ILO-B305` — entry function not found
//!
//! ## Pinned toolchain
//!
//! The Zero compiler version is recorded in `.zero-version` at the repo
//! root. Stage 5e targets `0.1.2`. Upgrade procedure: re-run the full Zero
//! backend test suite, update `.zero-version`, update
//! `docs/zero-transpile-capabilities.md` if syntax changed, CHANGELOG
//! entry under the patch release.

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
pub use emit::emit_to_string;

/// The pinned Zero compiler version. Kept in sync with `.zero-version`.
pub const PINNED_ZERO_VERSION: &str = "0.1.2";

/// Default install path for the pinned Zero compiler, relative to `$HOME`.
/// Resolved lazily by [`resolve_zero_bin`]; falls back to `zero` on PATH
/// when missing.
const DEFAULT_ZERO_PATH_REL: &str = ".zero/bin/zero";

/// Resolve the pinned install path against `$HOME` at call time. Returns
/// `None` if `$HOME` is unset (typical only inside container builds).
pub fn default_zero_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(DEFAULT_ZERO_PATH_REL))
}

/// The Zero transpile backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct ZeroBackend;

/// Stage 5e output mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZeroMode {
    /// Emit `.0` source. No subprocess.
    Source,
    /// Emit `.0` source then invoke `zero build` to produce a native binary.
    Binary,
}

/// Zero-specific configuration.
pub struct ZeroConfig {
    /// Output path. For [`ZeroMode::Source`] this is the `.0` file; for
    /// [`ZeroMode::Binary`] this is the final native binary (the `.0`
    /// source is written next to it with a `.0` suffix).
    pub output_path: PathBuf,
    /// Source-only or chain through `zero build`.
    pub mode: ZeroMode,
    /// Entry function name. Defaults to the first function in source order
    /// when `None`.
    pub entry: Option<String>,
}

impl Backend for ZeroBackend {
    const NAME: &'static str = "zero";

    type Config = ZeroConfig;

    fn emit(&self, hir: &Program, config: Self::Config) -> Result<Artefact, BackendError> {
        emit_program(hir, config)
    }
}

/// Convenience free function, mirroring `backend::python::emit` and
/// `backend::wasm::emit`.
pub fn emit(hir: &Program, config: ZeroConfig) -> Result<Artefact, BackendError> {
    emit_program(hir, config)
}

fn codegen(code: &'static str, message: impl Into<String>) -> BackendError {
    BackendError::CodegenFailed {
        code,
        message: message.into(),
        span: None,
    }
}

fn unsupported(feature: impl Into<String>) -> BackendError {
    BackendError::UnsupportedFeature {
        feature: feature.into(),
        backend: ZeroBackend::NAME,
    }
}

fn emit_program(hir: &Program, config: ZeroConfig) -> Result<Artefact, BackendError> {
    let entry_decl = match &config.entry {
        Some(name) => hir
            .function(name)
            .ok_or_else(|| codegen("ILO-B305", format!("entry function `{}` not found", name)))?,
        None => hir
            .first_function()
            .ok_or_else(|| codegen("ILO-B305", "no function declarations to emit"))?,
    };

    let (entry_name, body) = match entry_decl {
        Decl::Function { name, body, .. } => (name.clone(), body),
        _ => return Err(codegen("ILO-B305", "entry is not a function")),
    };

    // v1 walker: collect the `prnt`-ed literal strings (with `\n` appended,
    // matching ilo's print semantics) so we can emit a single idiomatic
    // Zero `main` body of `check world.out.write(...)` calls.
    let mut prints: Vec<String> = Vec::new();
    walk_body(body, &mut prints)?;

    let zero_src = emit::render_main(&prints);

    // For source-only mode we write the `.0` to `output_path` exactly.
    // For binary mode we write `<output_path>.0` and run `zero build` to
    // produce the binary at `output_path`.
    let source_path = match config.mode {
        ZeroMode::Source => config.output_path.clone(),
        ZeroMode::Binary => {
            let mut p = config.output_path.clone();
            // <stem>.0 next to the binary. If the user wrote `-o foo`, the
            // source is `foo.0` and the binary is `foo`. If they wrote
            // `-o foo.0` we still produce `foo.0` for source and `foo.0`
            // as the binary alias; the binary is what gets invoked.
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "out".to_string());
            p.set_file_name(format!("{}.0", stem));
            p
        }
    };

    std::fs::write(&source_path, &zero_src).map_err(|e| {
        codegen(
            "ILO-B304",
            format!("write {}: {}", source_path.display(), e),
        )
    })?;

    let mut notes: Vec<String> = vec![format!("zero {}", PINNED_ZERO_VERSION)];

    let final_path = match config.mode {
        ZeroMode::Source => source_path.clone(),
        ZeroMode::Binary => {
            run_zero_build(&source_path, &config.output_path)?;
            notes.push(format!("source:{}", source_path.display()));
            config.output_path.clone()
        }
    };

    let kind = match config.mode {
        ZeroMode::Source => ArtefactKind::SourceFile {
            ext: "0".to_string(),
        },
        ZeroMode::Binary => ArtefactKind::NativeBinary,
    };

    Ok(Artefact {
        path: final_path,
        kind,
        metadata: ArtefactMetadata {
            entry: Some(entry_name),
            notes,
        },
    })
}

/// Resolve which `zero` binary to invoke. Prefers the pinned install at
/// `$HOME/.zero/bin/zero` (see [`default_zero_path`]); falls back to
/// `zero` on PATH so CI environments that install elsewhere still work.
fn resolve_zero_bin() -> Option<String> {
    if let Some(p) = default_zero_path() {
        if p.is_file() {
            return Some(p.to_string_lossy().into_owned());
        }
    }
    // `which` via PATH probe. `output().is_ok()` only tells us the process
    // spawned; we need a successful exit status to know `zero --version`
    // actually worked. A broken binary on PATH should fall through, not be
    // reported as working (would surface later as a cryptic ILO-B301).
    let ok = Command::new("zero")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if ok {
        return Some("zero".to_string());
    }
    None
}

fn run_zero_build(source: &std::path::Path, out: &std::path::Path) -> Result<(), BackendError> {
    let zero_bin = resolve_zero_bin().ok_or_else(|| {
        let hint_path = default_zero_path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("~/{}", DEFAULT_ZERO_PATH_REL));
        codegen(
            "ILO-B303",
            format!(
                "`zero` compiler not found on PATH (and not at {}). \
                 Install the pinned version with:\n  \
                 curl https://zerolang.ai/install.sh | sh\n\
                 ilo's --0bin path targets zero {}.",
                hint_path, PINNED_ZERO_VERSION
            ),
        )
    })?;

    // `zero build` writes errors to stdout (per the 5e capability matrix);
    // capture both streams so we surface whichever has the diagnostic.
    let output = Command::new(&zero_bin)
        .arg("build")
        .arg(source)
        .arg("--json")
        .arg("--out")
        .arg(out)
        .output()
        .map_err(|e| {
            codegen(
                "ILO-B303",
                format!("failed to invoke `{} build`: {}", zero_bin, e),
            )
        })?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Zero prints diagnostics to stdout; stderr is usually empty but
        // we include it for completeness.
        let mut detail = String::new();
        if !stdout.trim().is_empty() {
            detail.push_str(stdout.trim());
        }
        if !stderr.trim().is_empty() {
            if !detail.is_empty() {
                detail.push('\n');
            }
            detail.push_str(stderr.trim());
        }
        return Err(codegen(
            "ILO-B301",
            format!(
                "zero rejected the transpiled output ({}): {}",
                output.status, detail
            ),
        ));
    }

    Ok(())
}

/// Walk a HIR body, collecting the `prnt`-ed string contents in source
/// order. v1 only understands `prnt "<literal>"` (number/bool/text);
/// anything else surfaces as `ILO-B302` with a workaround hint.
fn walk_body(body: &Body, prints: &mut Vec<String>) -> Result<(), BackendError> {
    for stmt in &body.stmts {
        walk_stmt(stmt, prints)?;
    }
    if let Some(tail) = &body.tail {
        match tail {
            Expr::Call { function, .. } if function == "prnt" => walk_call(tail, prints)?,
            Expr::Ok { .. } | Expr::Literal { .. } => {}
            _ => {
                return Err(codegen(
                    "ILO-B302",
                    "Stage 5e Zero backend only lowers a sequence of `prnt \"...\"` calls. \
                     hint: rewrite the function body as bare `prnt` statements, or build \
                     with the Cranelift native backend (drop --0/--0bin)."
                        .to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn walk_stmt(stmt: &Stmt, prints: &mut Vec<String>) -> Result<(), BackendError> {
    match stmt {
        Stmt::Expr { value, .. } => walk_call(value, prints),
        _ => Err(codegen(
            "ILO-B302",
            "Stage 5e Zero backend only lowers top-level expression statements. \
             hint: this construct (let/if/match/loop) is not yet supported by --0; \
             build with the Cranelift native backend.",
        )),
    }
}

fn walk_call(expr: &Expr, prints: &mut Vec<String>) -> Result<(), BackendError> {
    match expr {
        Expr::Call { function, args, .. } => {
            if function != "prnt" {
                return Err(codegen(
                    "ILO-B302",
                    format!(
                        "call `{}` is not supported by the Zero backend yet. \
                         hint: Stage 5e only lowers `prnt` literals; build with \
                         the Cranelift native backend for the full surface.",
                        function
                    ),
                ));
            }
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
                _ => {
                    return Err(codegen(
                        "ILO-B302",
                        "Stage 5e Zero backend only lowers `prnt` of literal arguments \
                         (text/number/bool). hint: hoist the value to a literal or build \
                         with the Cranelift native backend.",
                    ))
                }
            };
            prints.push(s);
            Ok(())
        }
        _ => Err(codegen(
            "ILO-B302",
            "non-call expression at statement position is not supported by the Zero backend yet.",
        )),
    }
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.is_finite() {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}
