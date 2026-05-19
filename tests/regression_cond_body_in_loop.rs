// Regression tests for the `cond{body}` braced-guard semantics inside `@`
// loops. Pre-fix, the braced form silently early-returned `nil` from the
// enclosing function when fired inside a loop body, which contradicted
// SPEC.md and ai.txt ("conditional execution, no early return") and was the
// manifesto's worst case: silent nil with no diagnostic. Two persona reruns
// (config-shaper rerun10 and bioinformatics rerun10) flagged this within
// the same day.
//
// Each fixture below is a verbatim minimisation of the persona repro. They
// run across every backend (tree, VM, Cranelift JIT) so the split-semantics
// contract holds at every level of lowering.
//
// The braced guard must:
//   * run its body when the condition is truthy,
//   * NOT early-return from the enclosing function,
//   * let `brk` / `cnt` / `ret` propagate as usual,
//   * leave control falling through to the next statement so loop bodies
//     can mutate-and-continue with `>i 3{k=+k 10}` etc.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str) -> String {
    let out = ilo()
        .args([src, engine])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "engine={engine}: run failed, stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(feature = "cranelift")]
const ENGINES: &[&str] = &["--run-tree", "--run-vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES: &[&str] = &["--run-tree", "--run-vm"];

// --- bioinformatics rerun10 -------------------------------------------------
// Minimal repro from the persona report. Pre-fix this printed nothing and
// returned nil out of `main` at exit 0. Expected: `k=25` then `0`.
const BIOINFORMATICS_REPRO: &str = "main>R n t\n  k=0\n  @i 0..5{\n    >i 2{k=+k 10}\n    k=+k 1\n  }\n  prnt fmt \"k={}\" k\n  ~0";

#[test]
fn cond_body_in_range_loop_does_not_early_return() {
    for engine in ENGINES {
        let out = run(engine, BIOINFORMATICS_REPRO);
        assert_eq!(
            out, "k=25\n0",
            "engine={engine}: expected `k=25\\n0`, got `{out}`"
        );
    }
}

// --- config-shaper rerun10 --------------------------------------------------
// Minimal repro: filter+set into a map inside an `@` over a list. Pre-fix
// this returned `nil`. Expected output: a JSON map with the matching key.
const CONFIG_SHAPER_REPRO: &str = "main>R t t\n  ks=[\"APP_PORT\" \"FOO\"]\n  out=mmap\n  @k ks{\n    pre=slc k 0 4\n    c= =pre \"APP_\"\n    c{out=mset out k k}\n  }\n  ~jdmp out";

#[test]
fn cond_body_in_foreach_loop_does_not_early_return() {
    for engine in ENGINES {
        let out = run(engine, CONFIG_SHAPER_REPRO);
        assert_eq!(
            out, "{\"APP_PORT\":\"APP_PORT\"}",
            "engine={engine}: expected map with one entry, got `{out}`"
        );
    }
}

// --- brk inside a braced guard must still break the loop --------------------
// The fix swaps OP_RET for value-discard in the VM lowering, but explicit
// `brk` inside the braced body must still propagate to the enclosing loop.
// Body prints 0,1,2 then brk; tail `s` reflects partial sum.
const BRK_IN_BRACED: &str =
    "f>n\n  s=0\n  @i 0..10{\n    >=i 3{brk}\n    prnt i\n    s=+s i\n  }\n  s";

#[test]
fn brk_in_braced_guard_still_breaks_loop() {
    for engine in ENGINES {
        let out = run(engine, BRK_IN_BRACED);
        assert_eq!(
            out, "0\n1\n2\n3",
            "engine={engine}: expected `0\\n1\\n2\\n3`, got `{out}`"
        );
    }
}

// --- cnt inside a braced guard must still continue the loop -----------------
const CNT_IN_BRACED: &str = "f>n\n  s=0\n  @i 0..5{\n    =i 2{cnt}\n    s=+s i\n  }\n  s";

#[test]
fn cnt_in_braced_guard_still_continues_loop() {
    for engine in ENGINES {
        let out = run(engine, CNT_IN_BRACED);
        // 0+1+3+4 = 8 (i=2 skipped)
        assert_eq!(out, "8", "engine={engine}: expected `8`, got `{out}`");
    }
}

// --- explicit ret inside a braced guard still early-returns -----------------
const RET_IN_BRACED: &str = "f x:n>n\n  >x 0{ret 99}\n  +x 1";

#[test]
fn ret_in_braced_guard_still_early_returns() {
    for engine in ENGINES {
        // x=5: ret 99 fires
        let out = run(engine, &format!("{RET_IN_BRACED}\nmain>n\n  f 5"));
        assert_eq!(out, "99", "engine={engine}: x=5 expected 99, got `{out}`");
        // x=-3: guard false, falls through to +x 1 = -2
        let out2 = run(engine, &format!("{RET_IN_BRACED}\nmain>n\n  f -3"));
        assert_eq!(
            out2, "-2",
            "engine={engine}: x=-3 expected -2, got `{out2}`"
        );
    }
}

// --- braced guard outside a loop discards body value, falls through ---------
const DISCARD_OUTSIDE_LOOP: &str = "f x:n>n;>x 0{99};+x 1";

#[test]
fn braced_guard_outside_loop_discards_body_value() {
    for engine in ENGINES {
        // x=5: guard true, body 99 evaluated and discarded, +5 1 = 6
        let out = run(engine, &format!("{DISCARD_OUTSIDE_LOOP}\nmain>n\n  f 5"));
        assert_eq!(out, "6", "engine={engine}: expected 6, got `{out}`");
        // x=-3: guard false, +x 1 = -2
        let out2 = run(engine, &format!("{DISCARD_OUTSIDE_LOOP}\nmain>n\n  f -3"));
        assert_eq!(out2, "-2", "engine={engine}: expected -2, got `{out2}`");
    }
}

// --- braceless form keeps early-return -------------------------------------
const BRACELESS_EARLY_RETURN: &str = "f x:n>n;>x 0 99;+x 1";

#[test]
fn braceless_guard_still_early_returns() {
    for engine in ENGINES {
        let out = run(engine, &format!("{BRACELESS_EARLY_RETURN}\nmain>n\n  f 5"));
        assert_eq!(out, "99", "engine={engine}: expected 99, got `{out}`");
        let out2 = run(engine, &format!("{BRACELESS_EARLY_RETURN}\nmain>n\n  f -3"));
        assert_eq!(out2, "-2", "engine={engine}: expected -2, got `{out2}`");
    }
}
