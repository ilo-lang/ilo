// Regression tests for `.?` safe field access on present records with a
// missing field.
//
// Pre-fix behaviour (all three engines diverged):
//   - tree-walk: errored ILO-R005 "no field 'b' on record"
//   - VM:        SIGSEGV (the JMPNN-wrapped strict opcode landed on a
//                FieldNotFound path that didn't unwind cleanly under release)
//   - Cranelift: returned nil by accident (jit_recfld* helpers fall through
//                to TAG_NIL on miss; works for `.?` but also silently nils
//                strict `.field` access — pre-existing bug, separate scope)
//
// Post-fix behaviour: `r.?missingField` returns nil on every engine when the
// record is present but lacks the field. Strict `r.field` still errors on
// tree-walk and VM. (Cranelift strict still silently nils — pre-existing bug
// in jit_recfld/jit_recfld_name returning TAG_NIL for the strict opcode too;
// tracked as a follow-up.)
//
// The fix introduces two new VM opcodes (`OP_RECFLD_SAFE`,
// `OP_RECFLD_NAME_SAFE`) emitted by the compiler whenever the AST node has
// `safe: true`. The strict opcodes (`OP_RECFLD`, `OP_RECFLD_NAME`) keep their
// FieldNotFound semantics so typo detection on statically-typed records is
// preserved.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for `{src}` on {engine}, stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

// ── Missing field on present dynamic record returns nil ────────────────────
//
// jpar produces a `R ? t` record whose static field set is unknown to the
// verifier. Heterogeneous JSON (NVD CVE metrics, GitHub events, Stripe
// payloads) routinely puts different fields on different records of the same
// API. `r.?absentField` is the documented shorthand for "give me this if it's
// there, else nil" and must work without an outer jdmp+jpth fallback.

const MISSING_FIELD: &str = "f j:t>R t t;r=jpar! j;~fmt \"{}\" r.?missing";

fn check_missing(engine: &str) {
    assert_eq!(
        run(engine, MISSING_FIELD, "f", &[r#"{"present":1}"#]),
        "~nil",
        "engine={engine}"
    );
}

#[test]
fn safe_field_missing_tree() {
    check_missing("--run-tree");
}

#[test]
fn safe_field_missing_vm() {
    check_missing("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn safe_field_missing_cranelift() {
    check_missing("--run-cranelift");
}

// ── Present field still returns the value (no regression on the hit path) ──

const PRESENT_FIELD: &str = "f j:t>R t t;r=jpar! j;~fmt \"{}\" r.?present";

fn check_present(engine: &str) {
    assert_eq!(
        run(engine, PRESENT_FIELD, "f", &[r#"{"present":42}"#]),
        "~42",
        "engine={engine}"
    );
}

#[test]
fn safe_field_present_tree() {
    check_present("--run-tree");
}

#[test]
fn safe_field_present_vm() {
    check_present("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn safe_field_present_cranelift() {
    check_present("--run-cranelift");
}

// ── Nil-propagation through chained .? on dynamic records ──────────────────
//
// `r.?outer.?inner` — outer field absent, the whole chain must collapse to
// nil rather than erroring on the second `.?` (which would now be probing
// `nil.?inner`, the original supported case).

const CHAINED_MISSING: &str = "f j:t>R t t;r=jpar! j;~fmt \"{}\" r.?outer.?inner";

fn check_chained_missing(engine: &str) {
    assert_eq!(
        run(engine, CHAINED_MISSING, "f", &[r#"{"other":1}"#]),
        "~nil",
        "engine={engine}"
    );
}

#[test]
fn safe_field_chained_missing_tree() {
    check_chained_missing("--run-tree");
}

#[test]
fn safe_field_chained_missing_vm() {
    check_chained_missing("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn safe_field_chained_missing_cranelift() {
    check_chained_missing("--run-cranelift");
}

// ── Chained .? on present nested records still walks the chain ─────────────

const CHAINED_PRESENT: &str = "f j:t>R t t;r=jpar! j;~fmt \"{}\" r.?outer.?inner";

fn check_chained_present(engine: &str) {
    assert_eq!(
        run(
            engine,
            CHAINED_PRESENT,
            "f",
            &[r#"{"outer":{"inner":"x"}}"#]
        ),
        "~x",
        "engine={engine}"
    );
}

#[test]
fn safe_field_chained_present_tree() {
    check_chained_present("--run-tree");
}

#[test]
fn safe_field_chained_present_vm() {
    check_chained_present("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn safe_field_chained_present_cranelift() {
    check_chained_present("--run-cranelift");
}

// ── Original nil-object case still works (no regression) ───────────────────

const NIL_OBJECT: &str = "f j:t>R t t;r=jpar! j;v=r.?missing;~fmt \"{}\" v.?anything";

fn check_nil_object(engine: &str) {
    // r.?missing → nil, then nil.?anything → nil.
    assert_eq!(
        run(engine, NIL_OBJECT, "f", &[r#"{"a":1}"#]),
        "~nil",
        "engine={engine}"
    );
}

#[test]
fn safe_field_nil_object_tree() {
    check_nil_object("--run-tree");
}

#[test]
fn safe_field_nil_object_vm() {
    check_nil_object("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn safe_field_nil_object_cranelift() {
    check_nil_object("--run-cranelift");
}

// ── Strict access on missing field still errors (verifier+runtime guard) ───
//
// `.field` (no `?`) on a dynamic record with the field missing must still
// raise ILO-R005 at runtime so genuine typos surface rather than silently
// returning nil. Tree-walk and VM enforce this; the Cranelift backend has a
// pre-existing bug where the strict opcode also routes through the same
// always-nil helper (separate follow-up — out of scope for this fix).

const STRICT_MISSING: &str = "f j:t>R t t;r=jpar! j;vb=r.missing;~vb";

#[test]
fn strict_field_missing_still_errors_tree() {
    let err = run_err("--run-tree", STRICT_MISSING, "f", &[r#"{"a":1}"#]);
    assert!(
        err.contains("ILO-R005") && err.contains("missing"),
        "stderr: {err}"
    );
}

#[test]
fn strict_field_missing_still_errors_vm() {
    let err = run_err("--run-vm", STRICT_MISSING, "f", &[r#"{"a":1}"#]);
    assert!(
        err.contains("ILO-R005") && err.contains("missing"),
        "stderr: {err}"
    );
}

// ── Cross-engine consistency: `.?` on a non-record value returns nil ──────
//
// On statically-typed non-record values (`xs:L n`), the verifier catches the
// shape mismatch as ILO-T018 before the runtime is involved — that path is
// covered by `safe_field_on_typed_list_caught_by_verifier` below.
//
// The runtime non-record arm only fires when the verifier can't see the type
// (Ty::Unknown chains where a field eventually resolves to a non-record at
// runtime). In that case, the tree-walk now also returns Nil (matching VM
// and Cranelift) instead of raising ILO-R005, so all three engines agree on
// the "give me this if it makes sense, else nil" semantics for `.?`.
//
// Triggering that path from pure ilo is awkward (the camelCase / dynamic-
// field paths through jpar always produce records, never bare lists/texts),
// so we lock in the verifier-time behaviour instead and rely on the unit
// tests inside src/vm/mod.rs (e.g. `vm_safe_field_on_list_returns_nil`,
// which uses `.?0` to bypass the verifier's field-access rule via the
// `Expr::Index` path) to anchor the VM-side semantics.

#[test]
fn safe_field_on_typed_list_caught_by_verifier() {
    // `xs:L n` is statically known to be a list. `.?name` is structurally
    // wrong on a list — the verifier rejects this with ILO-T018 at compile
    // time, regardless of `.?`. This is the right place to catch the shape
    // mismatch; runtime nil-tolerance is reserved for genuinely-dynamic
    // shapes (jpar records).
    let out = ilo()
        .args(["f>n;xs=[1,2,3];xs.?name??99", "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "verifier should reject .?field on a typed list"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("ILO-T018"),
        "expected ILO-T018, stderr: {err}"
    );
}

// ── Static record typo detection still fires at verify time ────────────────
//
// User-defined `T` records keep the strong ILO-T019 verifier guard. `.?` on a
// typo against a known static type must still be rejected at verify time so
// the typo-on-known-shape protection isn't lost.

#[test]
fn safe_field_typo_on_static_record_still_errors() {
    let src = "type pt{x:n;y:n}\nf>n;p=pt x:1 y:2;p.?z";
    let out = ilo()
        .args([src, "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected verify error for typo on static record; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("ILO-T019") && err.contains("'z'"),
        "stderr: {err}"
    );
}
