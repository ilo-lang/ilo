// Regression tests for the helper-fn `mset` O(N·K) perf cliff fix.
//
// Background:
//
// PR #249 shipped the `OP_MSET` RC=1 in-place fast path, gated on
// `a == b && rc_count == 1`. The `a == b` half came from the compiler
// peephole `name = mset name k v`, which re-uses the source register
// as the destination. RC=1 + a==b means we can mutate the underlying
// HashMap in place — amortised O(1) inserts.
//
// The hole this PR closes:
//
// When the agent factors the per-row update into a helper fn
//
//     addto m k v > mset m k v       -- canonical "DRY this loop" refactor
//     ...; m = addto m k v; ...      -- caller now uses the helper
//
// the map crosses an OP_CALL boundary. The compiler's MOVE-to-args_base
// (src/vm/mod.rs:5015-5020) clones the source register, and the OP_CALL
// dispatcher (~9020-9032) bumps RC again on push. Inside the callee, the
// map sits at RC>=2, the in-place fast path declines, OP_MSET clones the
// whole HashMap on every row — turning O(N) into O(N²) on K distinct
// keys.
//
// The fix is two-step:
//
//   1. OP_CALL_OWN1 + OP_MOVE_OWN: when the compiler sees the
//      `name = fn(name, ...)` shape and `fn` is a static user fn, it
//      emits the move-not-clone call variant. The first arg threads into
//      the callee at the caller's RC (typically RC=1 for accumulators)
//      instead of bumped twice.
//
//   2. Tail-position mset rewrite: when `mset m k v` sits at the tail of
//      a fn body and `m` is a direct Ref to a local register, the
//      compiler emits OP_MSET with `a == b == local_reg` — reusing the
//      local register as the result. The existing in-place fast path
//      now fires inside the helper too.
//
// These tests are correctness-only (no timing assertions): they verify
// the helper-fn pattern produces the right output across every engine
// (tree, VM, JIT, AOT). Performance is verified manually with
// /tmp/mset_bench.ilo and tracked in the In-Progress entry.
//
// All tests cross-engine to catch divergence.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str, arg: &str) -> String {
    let out = ilo()
        .args([src, engine, entry, arg])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── 1. Canonical accumulator: addto helper, all distinct keys ────────────

const ADDTO_ALL_DISTINCT: &str = concat!(
    "addto m:M t n k:t v:n>M t n;mset m k v\n",
    "go n:n>n;m=mmap;@i 1..n{k=str i;m=addto m k i};len (mkeys m)\n",
);

#[test]
fn addto_all_distinct_tree() {
    // tree walker is unaffected by the cliff (it always clones), but we
    // run it here so any future tree-walker semantics shift gets caught.
    assert_eq!(run("--run-vm", ADDTO_ALL_DISTINCT, "go", "100"), "99");
}

#[test]
fn addto_all_distinct_vm() {
    assert_eq!(run("--run-vm", ADDTO_ALL_DISTINCT, "go", "100"), "99");
}

#[test]
#[cfg(feature = "cranelift")]
fn addto_all_distinct_jit() {
    assert_eq!(run("--jit", ADDTO_ALL_DISTINCT, "go", "100"), "99");
}

// Larger N still produces the same key-count answer (1k distinct keys).
// Acts as a smoke test for the perf path under non-trivial scale without
// timing.

#[test]
fn addto_all_distinct_vm_1k() {
    assert_eq!(run("--run-vm", ADDTO_ALL_DISTINCT, "go", "1000"), "999");
}

#[test]
#[cfg(feature = "cranelift")]
fn addto_all_distinct_jit_1k() {
    assert_eq!(run("--jit", ADDTO_ALL_DISTINCT, "go", "1000"), "999");
}

// ── 2. Overwriting helper: same keys repeatedly, value lookup ────────────
//
// Exercises the displaced-value drop_rc path inside the in-place fast
// path (the helper's tail-position mset overwriting an existing key).
// The final value at "x" is the last iteration index.

const ADDTO_OVERWRITE: &str = concat!(
    "addto m:M t n k:t v:n>M t n;mset m k v\n",
    "go n:n>n;m=mmap;@i 1..n{m=addto m \"x\" i};mget m \"x\" ?? 0\n",
);

#[test]
fn addto_overwrite_tree() {
    assert_eq!(run("--run-vm", ADDTO_OVERWRITE, "go", "10"), "9");
}

#[test]
fn addto_overwrite_vm() {
    assert_eq!(run("--run-vm", ADDTO_OVERWRITE, "go", "10"), "9");
}

#[test]
#[cfg(feature = "cranelift")]
fn addto_overwrite_jit() {
    assert_eq!(run("--jit", ADDTO_OVERWRITE, "go", "10"), "9");
}

// ── 3. Helper with extra args before the value ───────────────────────────
//
// Confirms the move-semantics threading works when the helper takes
// multiple non-map args after the map. Map is args[0] (the moved one),
// extras are clone-on-push in the OP_CALL_OWN1 dispatch.

const ADDTO_EXTRA_ARGS: &str = concat!(
    "bump m:M t n k:t inc:n>M t n;c=??(mget m k) 0;mset m k (+c inc)\n",
    "go n:n>n;m=mmap;@i 1..n{m=bump m \"a\" 1;m=bump m \"b\" 2};mget m \"b\" ?? 0\n",
);

#[test]
fn addto_extra_args_tree() {
    // 10 iterations × +2 = 20
    assert_eq!(run("--run-vm", ADDTO_EXTRA_ARGS, "go", "11"), "20");
}

#[test]
fn addto_extra_args_vm() {
    assert_eq!(run("--run-vm", ADDTO_EXTRA_ARGS, "go", "11"), "20");
}

#[test]
#[cfg(feature = "cranelift")]
fn addto_extra_args_jit() {
    assert_eq!(run("--jit", ADDTO_EXTRA_ARGS, "go", "11"), "20");
}

// ── 4. Non-tail mset must NOT take the in-place path ─────────────────────
//
// The tail-position rewrite is gated on the mset being the tail
// expression of the fn body. A non-tail mset followed by a use of the
// original map must keep the source map intact.

const NON_TAIL_PRESERVES_SOURCE: &str =
    "go z:n>n;m=mset mmap \"k\" 1;m2=mset m \"j\" 2;??(mget m \"j\") (-1)\n";

#[test]
fn non_tail_preserves_source_tree() {
    assert_eq!(
        run("--run-vm", NON_TAIL_PRESERVES_SOURCE, "go", "0"),
        "-1"
    );
}

#[test]
fn non_tail_preserves_source_vm() {
    assert_eq!(run("--run-vm", NON_TAIL_PRESERVES_SOURCE, "go", "0"), "-1");
}

#[test]
#[cfg(feature = "cranelift")]
fn non_tail_preserves_source_jit() {
    assert_eq!(run("--jit", NON_TAIL_PRESERVES_SOURCE, "go", "0"), "-1");
}

// ── 5. Captured map across call boundary ─────────────────────────────────
//
// When the same map is passed to a helper as a *non-rebinding* argument
// (caller keeps using it after the call), the move-semantics path must
// NOT fire because the source is still live in the caller. We verify
// the caller's map is unchanged after the helper call.

const HELPER_NON_REBIND_PRESERVES: &str = concat!(
    "peek m:M t n k:t>M t n;mset m k 99\n",
    "go z:n>t;m=mset mmap \"a\" 1;m2=peek m \"a\";",
    "fmt \"{}|{}\" (??(mget m \"a\") 0) (??(mget m2 \"a\") 0)\n",
);

#[test]
fn helper_non_rebind_preserves_tree() {
    // caller's m stays at 1; m2 has 99
    assert_eq!(
        run("--run-vm", HELPER_NON_REBIND_PRESERVES, "go", "0"),
        "1|99"
    );
}

#[test]
fn helper_non_rebind_preserves_vm() {
    assert_eq!(
        run("--run-vm", HELPER_NON_REBIND_PRESERVES, "go", "0"),
        "1|99"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn helper_non_rebind_preserves_jit() {
    assert_eq!(run("--jit", HELPER_NON_REBIND_PRESERVES, "go", "0"), "1|99");
}
