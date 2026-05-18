// Regression: multi-arg map builtins (`mget`, `mset`, `mhas`, `mdel`) and
// the single-arg sibling `mvals` must type-check their first arg against
// `M` at verify time.
//
// Pre-fix, only `mkeys` and `mhas` did. `mget`, `mset`, `mdel`, `mvals`
// silently fell through with `Ty::Unknown` when the first arg was a
// fn-ref, number, list, text, anything other than a map. The verifier
// passed clean and the program only blew up at runtime with a
// misleading `ILO-R004 mget: first arg must be a map` (or sibling).
//
// The shape that bit db-analyst rerun8 was the zero-arg fn-ref vs call
// confusion: `tg = mkmap` (forgot the parens) binds the function
// reference itself (`F M t n`), not its result. Then `mget tg "a"`
// should produce `ILO-T013 'mget' expects a map, got F M t n` at
// verify time — the same way `mkeys tg` already did. Instead it ran
// to completion of verify and only crashed on the engine.
//
// These tests pin the verify-time behaviour for every multi-arg map
// builtin (and `mvals`), across both fn-ref and other-non-map first
// args. Each test runs against every available engine, asserting that
// `ilo` exits non-zero with ILO-T013 on stderr — never reaching the
// engine and emitting ILO-R004.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(tag: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ilo_map_verifier_hole_{tag}_{}_{n}.ilo",
        std::process::id()
    ));
    std::fs::write(&path, src).expect("write src");
    path
}

#[cfg(feature = "cranelift")]
const ENGINES: &[&str] = &["--run-tree", "--run-vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES: &[&str] = &["--run-tree", "--run-vm"];

/// Run `ilo <src> <engine> <entry>` and assert it failed with a
/// verifier ILO-T013 on `builtin`. The engine flag is included to
/// prove every backend honours the verifier verdict — verify runs
/// before dispatch, so a leak through any one engine would surface
/// as ILO-R004 in stderr.
fn assert_verify_error(engine: &str, src: &str, entry: &str, builtin: &str) {
    let path = write_src(builtin, src);
    let out = ilo()
        .args([path.to_str().unwrap(), engine, entry])
        .output()
        .expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success(),
        "engine={engine} builtin={builtin}: expected verify failure but ilo succeeded.\n\
         stdout={stdout}\nstderr={stderr}"
    );
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("ILO-T013"),
        "engine={engine} builtin={builtin}: expected ILO-T013 at verify time, got:\nstdout={stdout}\nstderr={stderr}"
    );
    assert!(
        !combined.contains("ILO-R004"),
        "engine={engine} builtin={builtin}: leaked past verifier to runtime ILO-R004:\nstdout={stdout}\nstderr={stderr}"
    );
    assert!(
        combined.contains(&format!("'{builtin}'")),
        "engine={engine} builtin={builtin}: ILO-T013 message did not mention the builtin name:\nstdout={stdout}\nstderr={stderr}"
    );
}

// ── 1. fn-ref-as-map (the db-analyst originating shape) ──────────────

const FNREF_MGET_SRC: &str =
    "mkmap d:n>M t n;m=mmap;mset m \"a\" 1 main>O n;tg=mkmap;mget tg \"a\"";
const FNREF_MSET_SRC: &str =
    "mkmap d:n>M t n;m=mmap;mset m \"a\" 1 main>M t n;tg=mkmap;mset tg \"b\" 2";
const FNREF_MHAS_SRC: &str = "mkmap d:n>M t n;m=mmap;mset m \"a\" 1 main>b;tg=mkmap;mhas tg \"a\"";
const FNREF_MDEL_SRC: &str =
    "mkmap d:n>M t n;m=mmap;mset m \"a\" 1 main>M t n;tg=mkmap;mdel tg \"a\"";
const FNREF_MVALS_SRC: &str = "mkmap d:n>M t n;m=mmap;mset m \"a\" 1 main>L n;tg=mkmap;mvals tg";
const FNREF_MKEYS_SRC: &str = "mkmap d:n>M t n;m=mmap;mset m \"a\" 1 main>L t;tg=mkmap;mkeys tg";

#[test]
fn fnref_first_arg_mget() {
    for engine in ENGINES {
        assert_verify_error(engine, FNREF_MGET_SRC, "main", "mget");
    }
}

#[test]
fn fnref_first_arg_mset() {
    for engine in ENGINES {
        assert_verify_error(engine, FNREF_MSET_SRC, "main", "mset");
    }
}

#[test]
fn fnref_first_arg_mhas() {
    for engine in ENGINES {
        assert_verify_error(engine, FNREF_MHAS_SRC, "main", "mhas");
    }
}

#[test]
fn fnref_first_arg_mdel() {
    for engine in ENGINES {
        assert_verify_error(engine, FNREF_MDEL_SRC, "main", "mdel");
    }
}

#[test]
fn fnref_first_arg_mvals() {
    for engine in ENGINES {
        assert_verify_error(engine, FNREF_MVALS_SRC, "main", "mvals");
    }
}

// `mkeys` was the one that already worked before this PR — pin it so
// regressions in the existing single-arg path also surface here.
#[test]
fn fnref_first_arg_mkeys() {
    for engine in ENGINES {
        assert_verify_error(engine, FNREF_MKEYS_SRC, "main", "mkeys");
    }
}

// ── 2. non-map non-fn-ref first arg (number) ─────────────────────────

const NUM_MGET_SRC: &str = "main>O n;mget 42 \"k\"";
const NUM_MSET_SRC: &str = "main>M t n;mset 42 \"k\" 1";
const NUM_MHAS_SRC: &str = "main>b;mhas 42 \"k\"";
const NUM_MDEL_SRC: &str = "main>M t n;mdel 42 \"k\"";
const NUM_MVALS_SRC: &str = "main>L n;mvals 42";
const NUM_MKEYS_SRC: &str = "main>L t;mkeys 42";

#[test]
fn num_first_arg_mget() {
    for engine in ENGINES {
        assert_verify_error(engine, NUM_MGET_SRC, "main", "mget");
    }
}

#[test]
fn num_first_arg_mset() {
    for engine in ENGINES {
        assert_verify_error(engine, NUM_MSET_SRC, "main", "mset");
    }
}

#[test]
fn num_first_arg_mhas() {
    for engine in ENGINES {
        assert_verify_error(engine, NUM_MHAS_SRC, "main", "mhas");
    }
}

#[test]
fn num_first_arg_mdel() {
    for engine in ENGINES {
        assert_verify_error(engine, NUM_MDEL_SRC, "main", "mdel");
    }
}

#[test]
fn num_first_arg_mvals() {
    for engine in ENGINES {
        assert_verify_error(engine, NUM_MVALS_SRC, "main", "mvals");
    }
}

#[test]
fn num_first_arg_mkeys() {
    for engine in ENGINES {
        assert_verify_error(engine, NUM_MKEYS_SRC, "main", "mkeys");
    }
}

// ── 3. call shape still works (regression guard for false positives) ─

const CALL_MGET_SRC: &str = "mkmap d:n>M t n;m=mmap;mset m \"a\" 1 main>O n;tg=mkmap;mget tg \"a\"";

fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let path = write_src(entry, src);
    let out = ilo()
        .args([path.to_str().unwrap(), engine, entry])
        .output()
        .expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "engine={engine}: run failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// Same body but with the parens that the agent originally forgot.
// `tg=mkmap()` produces an `M t n`, so `mget tg "a"` is fine. This
// pins that the new guard doesn't false-positive on the proper shape.
#[test]
fn call_shape_still_verifies_and_runs() {
    let src = "mkmap d:n>M t n;m=mmap;mset m \"a\" 1 main>O n;tg=mkmap 0;mget tg \"a\"";
    for engine in ENGINES {
        let out = run_ok(engine, src, "main");
        // Output of an Option<n> with Some(1) is engine-dependent;
        // we only care that it ran to completion. Avoid asserting
        // the exact stringified shape so the test stays focused on
        // the verifier behaviour. Just ensure stdout is non-empty.
        assert!(!out.is_empty(), "engine={engine}: empty stdout");
        // Sanity check: the parens path also accepts `1` in some form.
        assert!(out.contains('1'), "engine={engine}: expected '1' in {out}");
    }
    // Suppress unused warning if no engines remain.
    let _ = CALL_MGET_SRC;
}
