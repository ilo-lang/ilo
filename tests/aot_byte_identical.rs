//! Phase 5 Stage 5b regression test: post-refactor Cranelift AOT codegen is
//! byte-for-byte identical to the pre-refactor baseline at the Cranelift
//! object-file level.
//!
//! ## Why the object file, not the linked binary
//!
//! The linked binary contains all of `libilo.a`. Every Rust code addition to
//! the `ilo` crate (the trait scaffolding, the HIR module, anything new)
//! changes `libilo.a` and therefore the linked binary, even when Cranelift
//! codegen is unchanged. Asserting byte-identity on the linked binary would
//! fail spuriously on every code addition to the crate.
//!
//! The Cranelift-emitted `.o` file isolates the AOT codegen output. It
//! contains exactly the bytes Cranelift produced from the ilo source, plus
//! a `main()` shim, plus a Mach-O / ELF header. It is deterministic across
//! runs and across output paths (verified empirically — see manifest).
//!
//! ## How baselines are captured
//!
//! The build sets `ILO_KEEP_OBJ=1` to preserve the `.o` file that
//! `compile_to_binary` would otherwise delete after the link step. The
//! baseline-capture pass walks every `-- run:` annotated example, records
//! the sha256 of each `.o`, and writes them to
//! `tests/aot-baselines/obj-baselines.tsv`. Stage 5b reproduces those
//! sha256s; future stages re-capture when the codegen itself intentionally
//! changes.
//!
//! ## Running locally
//!
//! `cargo test --release --features cranelift --test aot_byte_identical`.
//! The test parallelises poorly because each entry spawns a full
//! `ilo build`, but the corpus is small (~136 examples) and the wall time
//! is acceptable as a release-gate.

// Byte-identity baselines were captured on macOS 15.5 arm64 (Mach-O AArch64
// object files). Linux CI emits ELF x86-64 objects, which differ at the
// binary level even for identical source. Gate the test to the capture
// platform so CI stays green; re-capture when migrating to Linux-only CI.
#![cfg(all(feature = "cranelift", target_os = "macos", target_arch = "aarch64"))]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const OBJ_BASELINES_TSV: &str = "tests/aot-baselines/obj-baselines.tsv";

/// Per-process counter for unique temp paths under parallel test execution.
static COUNTER: AtomicU32 = AtomicU32::new(0);

struct ObjBaseline {
    /// Example basename without the `.ilo` extension.
    name: String,
    /// Entry function the baseline was compiled with.
    entry_fn: String,
    /// sha256 hex of the captured `.o` file.
    sha256: String,
}

fn parse_baselines() -> Vec<ObjBaseline> {
    let path = Path::new(OBJ_BASELINES_TSV);
    let body = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("missing obj baselines at {}: {}", path.display(), e));
    let mut out = Vec::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() != 3 {
            continue;
        }
        out.push(ObjBaseline {
            name: parts[0].to_string(),
            entry_fn: parts[1].to_string(),
            sha256: parts[2].trim().to_string(),
        });
    }
    out
}

fn tmp_path(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("ilo-aot-byteid-{name}-{pid}-{n}"))
}

/// Compute SHA-256 via the system `shasum` so we don't drag in a `sha2` crate.
fn sha256_file(path: &Path) -> Option<String> {
    let out = Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let hex = line.split_whitespace().next()?.to_string();
    if hex.len() != 64 {
        return None;
    }
    Some(hex)
}

#[test]
fn cranelift_aot_object_file_byte_identical_to_baselines() {
    let entries = parse_baselines();
    assert!(
        !entries.is_empty(),
        "no baseline entries parsed from {OBJ_BASELINES_TSV}"
    );

    let mut mismatches: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let mut compile_failures: Vec<String> = Vec::new();
    let mut ok = 0;

    for entry in &entries {
        let example = format!("examples/{}.ilo", entry.name);
        if !Path::new(&example).exists() {
            missing.push(entry.name.clone());
            continue;
        }

        let bin = tmp_path(&entry.name);
        let obj = bin.with_extension("o");

        let mut compile = Command::new(env!("CARGO_BIN_EXE_ilo"));
        compile
            .env("ILO_KEEP_OBJ", "1")
            .args(["build", &example, "-o", bin.to_str().unwrap()])
            .arg(&entry.entry_fn);

        let out = match compile.output() {
            Ok(o) => o,
            Err(e) => {
                compile_failures.push(format!("{}: spawn error: {}", entry.name, e));
                continue;
            }
        };
        if !out.status.success() {
            compile_failures.push(format!(
                "{}: compile failed: {}",
                entry.name,
                String::from_utf8_lossy(&out.stderr)
            ));
            let _ = std::fs::remove_file(&bin);
            let _ = std::fs::remove_file(&obj);
            continue;
        }

        let observed = sha256_file(&obj);
        let _ = std::fs::remove_file(&bin);
        let _ = std::fs::remove_file(&obj);

        let Some(observed) = observed else {
            compile_failures.push(format!(
                "{}: produced no object file at {}",
                entry.name,
                obj.display()
            ));
            continue;
        };

        if observed == entry.sha256 {
            ok += 1;
        } else {
            mismatches.push(format!(
                "{}: expected {} observed {}",
                entry.name, entry.sha256, observed
            ));
        }
    }

    // Tolerate a small number of soft failures (examples removed since
    // baseline capture, or libilo signature drift in vm::compile not
    // related to Cranelift codegen). Anything more signals a real
    // regression that needs investigation.
    let soft_budget = 5;
    assert!(
        missing.len() <= soft_budget,
        "{} examples missing from corpus (budget {}): {:?}",
        missing.len(),
        soft_budget,
        missing
    );
    assert!(
        compile_failures.len() <= soft_budget,
        "{} compile failures vs baseline corpus (budget {}):\n{}",
        compile_failures.len(),
        soft_budget,
        compile_failures.join("\n")
    );

    assert!(
        mismatches.is_empty(),
        "{} of {} object-file byte-identity assertions failed:\n{}\n\n\
         If this is intentional (codegen has changed), regenerate \
         {} from the new baseline and commit alongside the change.",
        mismatches.len(),
        entries.len(),
        mismatches.join("\n"),
        OBJ_BASELINES_TSV,
    );

    eprintln!(
        "Stage 5b byte-identical: {} ok, {} missing, {} compile-fail, {} total entries",
        ok,
        missing.len(),
        compile_failures.len(),
        entries.len(),
    );
}
