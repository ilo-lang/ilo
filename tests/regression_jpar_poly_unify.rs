// Regression tests for ILO-373: jpar polymorphic `_` unification with
// downstream HOFs and accessors.
//
// Root cause: `jpar!` unwraps to `_` (Unknown). Calling `mget` on that
// produces `O _` (Optional Unknown). Prior to this fix, the verifier
// rejected `O _` wherever it saw `Ty::Unknown` as the only escape hatch,
// causing spurious ILO-T013 errors on valid chains like:
//
//   r=jpar! body; v=mget r "items"; len v
//
// The fix adds `is_opaque(ty)` — true for `_` and `O _` — to all
// builtin argument type checks that previously only accepted `Ty::Unknown`.
//
// Persona A/B run 2026-05-21: pair 2 (block-validator) and pair 13
// (fix-plan-emitter) both hit this shape and had to write workaround
// `?r{...}` blocks (40-80 extra tokens each entry point).

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run `ilo check` on inline code and return stderr.
fn check(code: &str) -> (bool, String) {
    let out = ilo()
        .arg("check")
        .arg(code)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.success(), stderr)
}

fn assert_clean(code: &str, label: &str) {
    let (ok, stderr) = check(code);
    assert!(ok, "{label}: expected clean check, got:\n{stderr}");
}

fn assert_err_code(code: &str, expected_code: &str, label: &str) {
    let (ok, stderr) = check(code);
    assert!(
        !ok,
        "{label}: expected error {expected_code}, program passed"
    );
    assert!(
        stderr.contains(expected_code),
        "{label}: expected {expected_code} in stderr, got:\n{stderr}"
    );
}

// ── block-validator shape ────────────────────────────────────────────────────
// Agents parsed a JSON block, mget'd the "txs" field (a list), then used
// len/map on it.  The verifier rejected the `len v` call because `v` was
// typed `O _` rather than `L _`.

#[test]
fn block_validator_jpar_mget_len_verifies_clean() {
    // r=jpar! body → _
    // v=mget r "txs" → O _   (was: ILO-T013 "len expects a list, got O _")
    // n=len v → n             (should pass: O _ is opaque → treated as _)
    assert_clean(
        "f body:t>R n t;r=jpar! body;v=mget r \"txs\";n=len v;~n",
        "block-validator: jpar! → mget → len",
    );
}

#[test]
fn block_validator_jpar_mget_map_verifies_clean() {
    // After mget → O _, map a fn over the result should verify.
    assert_clean(
        "f body:t>R (L t) t;r=jpar! body;v=mget r \"txs\";ys=map (x:_>t; x.hash) v;~ys",
        "block-validator: jpar! → mget → map",
    );
}

#[test]
fn block_validator_jpar_mget_flt_verifies_clean() {
    assert_clean(
        "f body:t>R (L _) t;r=jpar! body;xs=mget r \"items\";ys=flt (x:_>b; mhas x \"id\") xs;~ys",
        "block-validator: jpar! → mget → flt",
    );
}

// ── fix-plan-emitter shape ───────────────────────────────────────────────────
// Agents parsed JSON, then walked a nested key chain with multiple mget calls,
// feeding each result to downstream list ops.

#[test]
fn fix_plan_emitter_mget_chain_verifies_clean() {
    // Two levels of mget: r=jpar! b; s=mget r "steps"; n=len s
    assert_clean(
        "f b:t>R n t;r=jpar! b;s=mget r \"steps\";n=len s;~n",
        "fix-plan-emitter: jpar! → mget → mget → len",
    );
}

#[test]
fn fix_plan_emitter_mget_hd_verifies_clean() {
    // hd on O _ should be clean
    assert_clean(
        "f b:t>R _ t;r=jpar! b;xs=mget r \"items\";first=hd xs;~first",
        "fix-plan-emitter: jpar! → mget → hd",
    );
}

#[test]
fn fix_plan_emitter_mget_at_verifies_clean() {
    // at on O _ should be clean
    assert_clean(
        "f b:t>R _ t;r=jpar! b;xs=mget r \"cmds\";first=at xs 0;~first",
        "fix-plan-emitter: jpar! → mget → at",
    );
}

// ── map-with-jpar-lambda shape (from ticket description) ────────────────────
// `map (l:t>R _ t; jpar l) lines` — lambda returns R _ t, map result is
// L (R _ t).

#[test]
fn map_jpar_lambda_verifies_clean() {
    assert_clean(
        "process lines:L t>L (R _ t);rs=map (l:t>R _ t; jpar l) lines;rs",
        "map-jpar-lambda chain",
    );
}

// ── jpar-list direct chain ───────────────────────────────────────────────────

#[test]
fn jpar_list_map_field_access_verifies_clean() {
    assert_clean(
        "f body:t>R (L t) t;xs=jpar-list! body;ys=map (x:_>t; x.name) xs;~ys",
        "jpar-list! → map → field access",
    );
}

// ── guard: real type errors are still caught ─────────────────────────────────

#[test]
fn mget_on_concrete_non_map_still_errors() {
    // mget on a known list (not a map or Unknown) must still emit T013.
    assert_err_code(
        "f xs:L t>n;v=mget xs \"k\";len v",
        "ILO-T013",
        "mget on L t must still fail",
    );
}

#[test]
fn len_on_concrete_non_collection_still_errors() {
    assert_err_code("f x:n>n;len x", "ILO-T013", "len on n must still fail");
}
