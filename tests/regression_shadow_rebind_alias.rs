// Regression tests for the VM/Cranelift shadow-rebind register aliasing bug.
//
// Background:
//
// The VM compiler resolves `Expr::Ref(name)` to the source local's register
// directly with no MOVE (free read). Pre-fix, the `Stmt::Let` new-binding
// path then called `add_local(new_name, src_reg)`, aliasing the new local
// to the source register. A later `new_name = <expr>` would resolve back
// to that shared register and clobber the source. The classic shape:
//
//     a = 5
//     b = a            -- b aliases a's register
//     b = 99           -- writes to the shared slot, corrupts a
//     [a b]            -- pre-fix: [99, 99], post-fix: [5, 99]
//
// Tree-walker is unaffected (Env::set walks named bindings, no register
// aliasing). Cranelift inherited the bug because it lowers VM bytecode.
//
// This bug pre-dates Phase 2b (#261/#273/#276): bisecting against v0.11.1
// shows the same shape mis-behaving on VM+Cranelift on both releases. The
// nlp-engineer Zipf-slope corruption report attributed it to Phase 2b but
// the root cause is this long-standing alias hole.
//
// The fix: when the compiled RHS register is already owned by an existing
// local, allocate a fresh register and emit OP_MOVE before add_local. One
// extra MOVE per shadow-from-ref. No effect on `b = +a 1` / `b = mset a k v`
// because those already allocate fresh registers from arithmetic/builtin ops.
//
// All tests cross-engine (tree, VM, Cranelift) so a divergence between
// backends fails CI.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── Number shadow-rebind: the original Zipf-slope repro shape ──────────────
//
// `t=z; t = *t 2` writes a fresh number into the t-slot. Pre-fix, t shared
// z's register on VM/Cranelift and z came back as 6.28 too.

const NUMBER_SHADOW_REBIND: &str = "go>L n;z=3.14;t=z;t = *t 2;[t z]";

#[test]
fn number_shadow_rebind_tree() {
    assert_eq!(
        run("--run-tree", NUMBER_SHADOW_REBIND, "go"),
        "[6.28, 3.14]"
    );
}

#[test]
fn number_shadow_rebind_vm() {
    assert_eq!(run("--run-vm", NUMBER_SHADOW_REBIND, "go"), "[6.28, 3.14]");
}

#[test]
#[cfg(feature = "cranelift")]
fn number_shadow_rebind_cranelift() {
    assert_eq!(
        run("--run-cranelift", NUMBER_SHADOW_REBIND, "go"),
        "[6.28, 3.14]"
    );
}

// ── Number shadow-then-literal-overwrite: the minimal repro ────────────────
//
// `a=5; b=a; b=99` is the smallest pattern that surfaces the bug. No
// arithmetic, no Phase 2b types, no peepholes. Just bare-Ref shadow and a
// literal rebind.

const NUMBER_SHADOW_LITERAL_OVERWRITE: &str = "go>L n;a=5;b=a;b=99;[a b]";

#[test]
fn number_shadow_literal_overwrite_tree() {
    assert_eq!(
        run("--run-tree", NUMBER_SHADOW_LITERAL_OVERWRITE, "go"),
        "[5, 99]"
    );
}

#[test]
fn number_shadow_literal_overwrite_vm() {
    assert_eq!(
        run("--run-vm", NUMBER_SHADOW_LITERAL_OVERWRITE, "go"),
        "[5, 99]"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn number_shadow_literal_overwrite_cranelift() {
    assert_eq!(
        run("--run-cranelift", NUMBER_SHADOW_LITERAL_OVERWRITE, "go"),
        "[5, 99]"
    );
}

// ── Map shadow-rebind ──────────────────────────────────────────────────────
//
// Shadow then re-key. The mset peephole only fires for `name = mset name k v`,
// so this is the alias-hazard path (`b = mset b k v` after `b = a`).

const MAP_SHADOW_REBIND: &str = concat!(
    "go>t;",
    "a=mset (mmap) \"k\" 1;",
    "b=a;",
    "b = mset b \"k\" 99;",
    "fmt \"{}|{}\" (mget a \"k\") (mget b \"k\")"
);

#[test]
fn map_shadow_rebind_tree() {
    assert_eq!(run("--run-tree", MAP_SHADOW_REBIND, "go"), "1|99");
}

#[test]
fn map_shadow_rebind_vm() {
    assert_eq!(run("--run-vm", MAP_SHADOW_REBIND, "go"), "1|99");
}

#[test]
#[cfg(feature = "cranelift")]
fn map_shadow_rebind_cranelift() {
    assert_eq!(run("--run-cranelift", MAP_SHADOW_REBIND, "go"), "1|99");
}

// ── List shadow-rebind ─────────────────────────────────────────────────────

const LIST_SHADOW_REBIND: &str = "go>L L n;a=[1 2];b=a;b = +=b 99;[a b]";

#[test]
fn list_shadow_rebind_tree() {
    assert_eq!(
        run("--run-tree", LIST_SHADOW_REBIND, "go"),
        "[[1, 2], [1, 2, 99]]"
    );
}

#[test]
fn list_shadow_rebind_vm() {
    assert_eq!(
        run("--run-vm", LIST_SHADOW_REBIND, "go"),
        "[[1, 2], [1, 2, 99]]"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn list_shadow_rebind_cranelift() {
    assert_eq!(
        run("--run-cranelift", LIST_SHADOW_REBIND, "go"),
        "[[1, 2], [1, 2, 99]]"
    );
}

// ── Text shadow-rebind ─────────────────────────────────────────────────────

const TEXT_SHADOW_REBIND: &str = "go>t;a=\"x\";b=a;b = +b \"y\";fmt \"{}|{}\" a b";

#[test]
fn text_shadow_rebind_tree() {
    assert_eq!(run("--run-tree", TEXT_SHADOW_REBIND, "go"), "x|xy");
}

#[test]
fn text_shadow_rebind_vm() {
    assert_eq!(run("--run-vm", TEXT_SHADOW_REBIND, "go"), "x|xy");
}

#[test]
#[cfg(feature = "cranelift")]
fn text_shadow_rebind_cranelift() {
    assert_eq!(run("--run-cranelift", TEXT_SHADOW_REBIND, "go"), "x|xy");
}

// ── Transitive shadow: a → b → c, then write c ─────────────────────────────
//
// Pre-fix, all three would alias the same register and the c write would
// corrupt both a and b. Confirms the fix handles chained shadows, not just
// the two-level case.

const TRANSITIVE_SHADOW: &str = "go>L n;a=7;b=a;c=b;c=99;[a b c]";

#[test]
fn transitive_shadow_tree() {
    assert_eq!(run("--run-tree", TRANSITIVE_SHADOW, "go"), "[7, 7, 99]");
}

#[test]
fn transitive_shadow_vm() {
    assert_eq!(run("--run-vm", TRANSITIVE_SHADOW, "go"), "[7, 7, 99]");
}

#[test]
#[cfg(feature = "cranelift")]
fn transitive_shadow_cranelift() {
    assert_eq!(
        run("--run-cranelift", TRANSITIVE_SHADOW, "go"),
        "[7, 7, 99]"
    );
}

// ── Non-aliasing happy path stays cheap ────────────────────────────────────
//
// `b = +a 1` allocates a fresh register from the BinOp::Add path and never
// aliases. The fix's `locals.iter().any(...)` check must not trip here.
// Pin the result to lock in that we still get correct values.

const NON_ALIASING_HAPPY_PATH: &str = "go>L n;a=5;b=+a 1;b=99;[a b]";

#[test]
fn non_aliasing_happy_path_tree() {
    assert_eq!(run("--run-tree", NON_ALIASING_HAPPY_PATH, "go"), "[5, 99]");
}

#[test]
fn non_aliasing_happy_path_vm() {
    assert_eq!(run("--run-vm", NON_ALIASING_HAPPY_PATH, "go"), "[5, 99]");
}

#[test]
#[cfg(feature = "cranelift")]
fn non_aliasing_happy_path_cranelift() {
    assert_eq!(
        run("--run-cranelift", NON_ALIASING_HAPPY_PATH, "go"),
        "[5, 99]"
    );
}
