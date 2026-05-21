//! Phase 5 Stage 5c regression test: post-refactor Python transpile output is
//! byte-for-byte identical to the pre-refactor `--emit python` output.
//!
//! ## How the baselines were captured
//!
//! Before moving `src/codegen/python.rs` to `src/backend/python/`, the
//! pre-refactor `ilo <example> --emit python` stdout was captured for a
//! curated set of examples and committed to `tests/python-baselines/`. The
//! post-refactor canonical form is `ilo build <example> --py -o <out>`;
//! this test asserts the new file matches the captured baseline byte-for-byte.
//!
//! The baselines include a trailing newline because the pre-refactor path
//! used `println!`. Stage 5c's `PythonBackend::emit` appends a trailing `\n`
//! to match — see the note in `src/backend/python/mod.rs`.
//!
//! ## Why a curated corpus
//!
//! Python transpile covers a different surface area from the Cranelift AOT
//! path: it depends on AST shape (let/match/guard/expressions) and the
//! `_ilo_rd` / `_ilo_unwrap` helper emission. The corpus picks examples that
//! exercise: arithmetic, indexing, ternaries, bang-propagation, the unwrap
//! helper, the rd helper (`builtin-bridge`), struct field access, string
//! handling, list chunking, and clamp/numeric utility shape. Adding more
//! examples is cheap: drop `<name>.py` into `tests/python-baselines/` and the
//! test picks it up automatically.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const BASELINE_DIR: &str = "tests/python-baselines";

/// Per-process counter for unique temp paths under parallel test execution.
static COUNTER: AtomicU32 = AtomicU32::new(0);

fn tmp_path(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("ilo-py-byteid-{name}-{pid}-{n}.py"))
}

#[test]
fn python_emit_byte_identical_to_baselines() {
    let baseline_dir = std::path::Path::new(BASELINE_DIR);
    let entries: Vec<PathBuf> = std::fs::read_dir(baseline_dir)
        .unwrap_or_else(|e| panic!("missing baseline dir {BASELINE_DIR}: {e}"))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "py"))
        .collect();
    assert!(
        !entries.is_empty(),
        "no python baseline files in {BASELINE_DIR}"
    );

    let mut mismatches: Vec<String> = Vec::new();
    let mut compile_failures: Vec<String> = Vec::new();
    let mut ok = 0;

    for baseline in &entries {
        // baseline filename is `<example>.ilo.py`; strip the trailing `.py`
        // to get the corresponding source path. Examples may use either the
        // legacy `.ilo` extension or the newer `.@` extension (Phase 5 rename),
        // so we probe both.
        let stem = baseline
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("baseline filename must be utf8");
        // Strip a trailing `.ilo` if present to get the bare name, then probe
        // for `.@` first (new convention), falling back to the full stem path.
        let bare = stem.strip_suffix(".ilo").unwrap_or(stem);
        let source_at = format!("examples/{bare}.@");
        let source_ilo = format!("examples/{stem}");
        let source = if std::path::Path::new(&source_at).exists() {
            source_at
        } else if std::path::Path::new(&source_ilo).exists() {
            source_ilo
        } else {
            compile_failures.push(format!(
                "{stem}: source missing at {source_ilo} (also tried {source_at})"
            ));
            continue;
        };

        let out_path = tmp_path(stem);
        let out = Command::new(env!("CARGO_BIN_EXE_ilo"))
            .args(["build", &source, "--py", "-o", out_path.to_str().unwrap()])
            .output();

        let out = match out {
            Ok(o) => o,
            Err(e) => {
                compile_failures.push(format!("{stem}: spawn error: {e}"));
                continue;
            }
        };
        if !out.status.success() {
            compile_failures.push(format!(
                "{stem}: ilo build --py failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
            let _ = std::fs::remove_file(&out_path);
            continue;
        }

        let observed = match std::fs::read(&out_path) {
            Ok(b) => b,
            Err(e) => {
                compile_failures.push(format!("{stem}: read output: {e}"));
                let _ = std::fs::remove_file(&out_path);
                continue;
            }
        };
        let expected = std::fs::read(baseline)
            .unwrap_or_else(|e| panic!("read baseline {}: {e}", baseline.display()));
        let _ = std::fs::remove_file(&out_path);

        if observed == expected {
            ok += 1;
        } else {
            mismatches.push(format!(
                "{stem}: byte mismatch ({} expected vs {} observed bytes)",
                expected.len(),
                observed.len()
            ));
        }
    }

    assert!(
        compile_failures.is_empty(),
        "{} python-emit compile failures:\n{}",
        compile_failures.len(),
        compile_failures.join("\n")
    );
    assert!(
        mismatches.is_empty(),
        "{} of {} python-emit byte-identity assertions failed:\n{}\n\n\
         If this is intentional (python transpile has intentionally changed), \
         regenerate the baselines in {BASELINE_DIR}/ from the new output.",
        mismatches.len(),
        entries.len(),
        mismatches.join("\n"),
    );

    eprintln!(
        "Stage 5c byte-identical: {} ok / {} total baselines",
        ok,
        entries.len(),
    );
}
