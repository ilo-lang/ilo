//! Cross-engine regression tests for the `run` builtin: argv-list process
//! spawn returning `R (M t t) t`. Mirrors the trm/padl shape — every engine
//! must agree on the result Map.
//!
//! The `run` builtin is the new 0.12.0 process-spawn primitive. argv-list
//! only (no shell, no interpolation, no glob). Non-zero exit is NOT an
//! error — it surfaces as `Ok({"code":"<n>",...})`. Spawn failure IS an
//! error and surfaces as `Err(...)`.
//!
//! Coverage matrix (every test runs on tree/VM/Cranelift):
//!   - happy path: stdout captured, code is "0"
//!   - non-zero exit: code is "1", no Err
//!   - spawn failure: returns Err (command not found)
//!   - argv with multiple args: each entry passed verbatim
//!   - empty argv list works (`run "true" []`)
//!   - stderr captured separately
//!   - `$` sigil equivalence: `$cmd argv` produces identical Map to
//!     `run cmd argv`
//!   - `post` name is GONE — `post u b` produces ILO-T005 (rename pst)

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

/// Run an ilo source string on `engine`, expect success, return stdout.
///
/// Positional argv shape mirrors `tests/regression_pad.rs`: the source
/// string is the first positional, the engine flag follows, then the
/// entry-function name + its call args.
fn run_ok(engine: &str, src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} {args:?} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    s.trim_end_matches('\n').to_string()
}

/// Run an ilo source string on `engine`, expect non-success, return stderr.
fn run_err(engine: &str, src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} {src:?} {args:?} unexpectedly succeeded: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

// ─── Happy path ──────────────────────────────────────────────────────────

#[test]
fn run_echo_returns_stdout_and_zero_code_cross_engine() {
    // `run "echo" ["hi"]` — pull stdout out and print it. The outer fn
    // returns R t t so `run!` propagates spawn failures cleanly.
    let src = r#"f>t;m=run!! "echo" ["hi"];??mget m "stdout" """#;
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f"]);
        assert!(
            out.contains("hi"),
            "{engine}: expected stdout=hi, got {out:?}"
        );
    }
}

#[test]
fn run_echo_code_is_zero_cross_engine() {
    // Pull `code` out: "0" on success.
    let src = r#"f>t;m=run!! "echo" ["hi"];??mget m "code" """#;
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f"]);
        assert_eq!(out, "0", "{engine}: expected code=0 from echo");
    }
}

// ─── Non-zero exit is NOT an error ───────────────────────────────────────

#[test]
fn run_false_returns_code_one_no_err_cross_engine() {
    // `false` exits 1 — must surface as Ok({"code":"1",...}), NOT Err.
    // Using `run!` proves the result is Ok (otherwise `!` would propagate
    // the Err and abort with exit 1 and no stdout).
    let src = r#"f>t;m=run!! "false" [];??mget m "code" """#;
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f"]);
        assert_eq!(out, "1", "{engine}: expected code=1 from false");
    }
}

// ─── Spawn failure IS an error ───────────────────────────────────────────

#[test]
fn run_unknown_cmd_returns_err_cross_engine() {
    // Spawn failure (command not found) must surface as Err. We branch
    // on the Result so the test exits cleanly (status=success) and we
    // can assert on the Err message.
    let src = r#"f>t;r=run "no-such-cmd-xyz123-ilo-test" [];?r{~_:"unexpected ok";^e:e}"#;
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f"]);
        assert!(
            out.contains("failed to spawn") && out.contains("no-such-cmd-xyz123-ilo-test"),
            "{engine}: expected spawn-failure Err, got {out:?}"
        );
    }
}

// ─── argv with multiple args, each entry passed verbatim ─────────────────

#[test]
fn run_argv_multiple_args_each_verbatim_cross_engine() {
    // `printf "%s|%s|%s" a b c` — printf is a portable utility that prints
    // its argv entries with a known separator, proving each list element
    // is passed as a distinct argv entry (no shell join, no glob).
    let src = r#"f>t;m=run!! "printf" ["%s|%s|%s" "a" "b" "c"];??mget m "stdout" """#;
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f"]);
        assert_eq!(
            out, "a|b|c",
            "{engine}: expected argv entries to be passed verbatim"
        );
    }
}

// ─── Empty argv list works ───────────────────────────────────────────────

#[test]
fn run_true_empty_argv_returns_code_zero_cross_engine() {
    // `run "true" []` — empty argv list. Spawn must round-trip and code
    // must be "0".
    let src = r#"f>t;m=run!! "true" [];??mget m "code" """#;
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f"]);
        assert_eq!(
            out, "0",
            "{engine}: expected code=0 from `true` with empty argv"
        );
    }
}

// ─── Stderr captured separately ──────────────────────────────────────────

#[test]
fn run_stderr_captured_separately_cross_engine() {
    // `sh -c "echo err 1>&2"` emits ONLY to stderr — the test exercises
    // `run`'s argv plumbing (zero shell on ilo's side) via a utility that
    // happens to be a shell. ilo passes `["-c", "echo err 1>&2"]` to `sh`
    // as distinct argv entries; sh itself does the parsing.
    let src = r#"f>t;m=run!! "sh" ["-c" "echo err 1>&2"];??mget m "stderr" """#;
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f"]);
        assert!(
            out.contains("err"),
            "{engine}: expected stderr=err, got {out:?}"
        );
    }
}

#[test]
fn run_stderr_only_leaves_stdout_empty_cross_engine() {
    // Same call as above but pulls stdout; should be empty because the
    // child emitted only to stderr.
    let src = r#"f>t;m=run!! "sh" ["-c" "echo err 1>&2"];??mget m "stdout" """#;
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f"]);
        assert_eq!(
            out, "",
            "{engine}: stderr-only emit must leave stdout empty, got {out:?}"
        );
    }
}

// ─── `$` sigil equivalence: $cmd argv ≡ run cmd argv ────────────────────

#[test]
fn dollar_sigil_equivalent_to_run_cross_engine() {
    // `$cmd argv` must produce a Map identical to `run cmd argv`. We
    // pull stdout from each and confirm they match.
    let lhs_src = r#"f>t;m=$!!"echo" ["hi"];??mget m "stdout" """#;
    let rhs_src = r#"f>t;m=run!! "echo" ["hi"];??mget m "stdout" """#;
    for engine in ENGINES_ALL {
        let lhs = run_ok(engine, lhs_src, &["f"]);
        let rhs = run_ok(engine, rhs_src, &["f"]);
        assert_eq!(
            lhs, rhs,
            "{engine}: $ sigil and run must produce identical Maps"
        );
        assert!(
            lhs.contains("hi"),
            "{engine}: expected stdout=hi via $ sigil"
        );
    }
}

// ─── `post` is undefined post-rename ────────────────────────────────────

#[test]
fn post_is_undefined_after_pst_rename() {
    // 0.12.0 renamed `post` → `pst`. `post u b` must now error
    // ILO-T005 (undefined function) with a did-you-mean to `pst`.
    let stderr = run_err(
        "--run-tree",
        r#"f u:t b:t>R t t;post u b"#,
        &["f", "http://x", "body"],
    );
    assert!(
        stderr.contains("ILO-T005"),
        "expected ILO-T005, got: {stderr}"
    );
    assert!(
        stderr.contains("post"),
        "expected error to mention post, got: {stderr}"
    );
    assert!(
        stderr.contains("pst"),
        "expected did-you-mean to suggest pst, got: {stderr}"
    );
}

// ─── pst smoke test (basic verify path) ──────────────────────────────────

#[test]
fn pst_parses_and_verifies() {
    // Smoke test — `pst url body` must verify cleanly. We don't make a
    // real HTTP call; just confirm the AST round-trips.
    let out = ilo()
        .args(["--ast", r#"f url:t body:t>R t t;pst url body"#])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "pst should verify cleanly; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}
