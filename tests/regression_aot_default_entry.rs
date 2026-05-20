// Regression: `ilo compile` must pick `main` as the entry function when a
// file declares multiple functions, NOT the first declared function.
//
// The bug: AOT's `compile_cmd` historically did
//
//     compiled.func_names.first().unwrap_or("main")
//
// which produced a binary whose `main()` called the first-declared function.
// For a file like
//
//     helper>n;42
//     main>n;helper
//
// the produced binary called `helper` instead of `main` and (depending on
// shape) SIGSEGV'd at runtime — exit code 139, no diagnostic. Tree, VM, and
// Cranelift JIT all canonicalise on `main` via the dispatch in `run_cmd`,
// so AOT was the lone divergence.
//
// The fix mirrors the in-process convention exactly:
//   1. explicit positional `func` argument wins
//   2. else single user-defined function → use it
//   3. else `main` exists → use `main`
//   4. else error ILO-E801 (no SIGSEGV)
//
// Gated on the `cranelift` feature because AOT compile requires it.

#![cfg(feature = "cranelift")]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn tmp_paths(tag: &str) -> (PathBuf, PathBuf) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let src = std::env::temp_dir().join(format!("ilo-aot-entry-{tag}-{pid}-{n}.@"));
    let bin = std::env::temp_dir().join(format!("ilo-aot-entry-{tag}-{pid}-{n}.bin"));
    (src, bin)
}

// ── 1. multi-fn file WITH `main`: AOT must pick `main`, not the first fn ──

#[test]
fn aot_picks_main_over_first_declared_function() {
    let (src, bin) = tmp_paths("picks-main");
    std::fs::write(&src, "helper>n;42\nmain>n;helper\n").expect("write src");

    let compile = ilo()
        .args(["compile"])
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("compile");
    assert!(
        compile.status.success(),
        "compile failed: stderr={:?}",
        String::from_utf8_lossy(&compile.stderr),
    );

    let run = Command::new(&bin).output().expect("run binary");
    let stdout = String::from_utf8_lossy(&run.stdout);
    let code = run.status.code().unwrap_or(-1);

    // `main` returns 42 (via `helper`). The bug shape would either crash
    // (exit 139 / signalled) or print whatever `helper` evaluated to via
    // the wrong wrapper. We pin both stdout and exit so a regression to
    // either shape fails loudly.
    assert_eq!(stdout.trim(), "42", "wrong stdout: {stdout:?}");
    assert_eq!(code, 0, "wrong exit: {code}");

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&bin);
}

// ── 2. multi-fn file WITHOUT `main`: ILO-E801, exit 1, no binary ──

#[test]
fn aot_errors_clearly_when_no_main_and_no_explicit_entry() {
    let (src, bin) = tmp_paths("no-main");
    std::fs::write(&src, "helper>n;42\ngo>n;helper\n").expect("write src");

    let compile = ilo()
        .args(["compile"])
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("compile");

    let stderr = String::from_utf8_lossy(&compile.stderr);
    let code = compile.status.code().unwrap_or(-1);

    assert_eq!(code, 1, "expected exit 1, got {code}; stderr={stderr:?}");
    assert!(
        stderr.contains("ILO-E801"),
        "stderr should mention ILO-E801, got: {stderr:?}"
    );
    assert!(
        stderr.contains("helper") && stderr.contains("go"),
        "stderr should list available functions, got: {stderr:?}"
    );
    // The whole point: no binary produced (no SIGSEGV path possible).
    assert!(
        !bin.exists(),
        "no binary should be written on ILO-E801; found one at {bin:?}"
    );

    let _ = std::fs::remove_file(&src);
}

// ── 3. multi-fn file WITHOUT `main` but WITH explicit `-c entryFn`: works ──

#[test]
fn aot_honours_explicit_entry_when_no_main_present() {
    let (src, bin) = tmp_paths("explicit-entry");
    std::fs::write(&src, "helper>n;42\ngo>n;helper\n").expect("write src");

    let compile = ilo()
        .args(["compile"])
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .arg("go")
        .output()
        .expect("compile");
    assert!(
        compile.status.success(),
        "compile failed: stderr={:?}",
        String::from_utf8_lossy(&compile.stderr),
    );

    let run = Command::new(&bin).output().expect("run binary");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "42");
    assert_eq!(run.status.code().unwrap_or(-1), 0);

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&bin);
}

// ── 4. single user-fn shortcut: AOT picks the lone fn (the `m>...` idiom) ──

#[test]
fn aot_uses_single_user_fn_when_only_one_defined() {
    let (src, bin) = tmp_paths("single-fn");
    // `m` is the canonical single-fn shape used across the AOT regression
    // tests (see regression_aot_wrapper_strip.rs). Pinning it here ensures
    // the new entry-pick logic preserves that idiom.
    std::fs::write(&src, "m>n;7\n").expect("write src");

    let compile = ilo()
        .args(["compile"])
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("compile");
    assert!(
        compile.status.success(),
        "compile failed: stderr={:?}",
        String::from_utf8_lossy(&compile.stderr),
    );

    let run = Command::new(&bin).output().expect("run binary");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "7");
    assert_eq!(run.status.code().unwrap_or(-1), 0);

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&bin);
}
