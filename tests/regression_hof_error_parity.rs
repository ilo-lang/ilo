// Regression: HOF callback errors must surface on every engine.
//
// `jit_call_builtin_tree` (and its OP_CALL_DYN sibling `jit_call_dyn`)
// previously swallowed every non-`Fmt` bridge error as `TAG_NIL`. For HOFs
// whose user-supplied callbacks can fail at runtime (out-of-range `at`,
// type errors, runtime `^err` propagation), this masked the failure as a
// silent nil return on Cranelift even though tree and VM raised. Filed
// during the #306 srt-cranelift-nil P0 investigation as a separate P1.
//
// The fix extends the allow-list in `jit_call_builtin_tree` to cover
// `srt`/`rsrt`/`grp`/`uniqby`/`partition`/`flatmap`/`mapr`, and promotes
// callback errors raised through `jit_call_dyn` (used by native-loop HOFs
// like `flatmap` and by general OP_CALL_DYN dispatch).
//
// Each case below:
//   1. constructs an HOF call whose callback errors at runtime,
//   2. runs it on tree, VM, and Cranelift,
//   3. asserts all three engines fail (non-zero exit, ILO-R009 surfaced).
//
// Before the fix, Cranelift would exit 0 with stdout `nil` (or `[]` for
// `flatmap`'s list accumulator); tree/VM raise.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

fn assert_callback_error(src: &str, entry: &str, expected_code: &str) {
    for engine in ENGINES_ALL {
        let out = ilo()
            .args([src, engine, entry])
            .output()
            .expect("failed to spawn ilo");
        assert!(
            !out.status.success(),
            "engine={engine}: expected callback failure to surface as a runtime error for `{src}`, but it exited 0\nstdout={}",
            String::from_utf8_lossy(&out.stdout)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(expected_code),
            "engine={engine}: expected `{expected_code}` in stderr for `{src}`, got:\n{stderr}"
        );
    }
}

// `at` out-of-range inside a `srt` key function. Tree and VM both raise
// ILO-R009; pre-fix Cranelift silently returned `nil`.
#[test]
fn srt_callback_at_oob_raises_on_every_engine() {
    let src = "bad x:n>n;at [10,20] (* x 100)\nmn>L n;srt bad [1,2,3]";
    assert_callback_error(src, "mn", "ILO-R009");
}

// Same pattern for `rsrt` (descending sort by key). Same bridge contract
// as `srt`, must share error parity.
#[test]
fn rsrt_callback_at_oob_raises_on_every_engine() {
    let src = "bad x:n>n;at [10,20] (* x 100)\nmn>L n;rsrt bad [1,2,3]";
    assert_callback_error(src, "mn", "ILO-R009");
}

// `grp` group-by-key with a failing key callback.
#[test]
fn grp_callback_at_oob_raises_on_every_engine() {
    let src = "bad x:n>n;at [10,20] (* x 100)\nmn>M n (L n);grp bad [1,2,3]";
    assert_callback_error(src, "mn", "ILO-R009");
}

// `uniqby` deduplicate-by-key with a failing key callback.
#[test]
fn uniqby_callback_at_oob_raises_on_every_engine() {
    let src = "bad x:n>n;at [10,20] (* x 100)\nmn>L n;uniqby bad [1,2,3]";
    assert_callback_error(src, "mn", "ILO-R009");
}

// `partition` split-by-predicate with a failing predicate callback.
#[test]
fn partition_callback_at_oob_raises_on_every_engine() {
    // Wrap `at` in an `=` so the predicate's return type is `b`.
    let src = "bad x:n>b;=x (at [10,20] (* x 100))\nmn>L (L n);partition bad [1,2,3]";
    assert_callback_error(src, "mn", "ILO-R009");
}

// `flatmap` uses native OP_CALL_DYN dispatch rather than the tree-bridge,
// so this exercises the `jit_call_dyn` User-fn error path specifically.
// Pre-fix, Cranelift produced `[]` while tree/VM raised.
#[test]
fn flatmap_callback_at_oob_raises_on_every_engine() {
    let src = "bad x:n>L n;at [[10,20],[30,40]] (* x 100)\nmn>L n;flatmap bad [1,2,3]";
    // Tree emits ILO-R009 ("at: index 100 out of range..."); VM emits ILO-R004
    // ("at: index out of range") via its different runtime error class. Assert
    // the shared word; both are non-success, both surface a runtime error.
    for engine in ENGINES_ALL {
        let out = ilo()
            .args([src, engine, "mn"])
            .output()
            .expect("failed to spawn ilo");
        assert!(
            !out.status.success(),
            "engine={engine}: expected flatmap callback failure to surface, got success\nstdout={}",
            String::from_utf8_lossy(&out.stdout)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("out of range"),
            "engine={engine}: expected `out of range` in stderr for flatmap, got:\n{stderr}"
        );
    }
}

// 3-arg closure-bind `rsrt fn ctx xs` with a map context. The callback
// raises through wrong-arg-order `mget` (numeric arg first). Pre-fix,
// Cranelift returned `nil`. Handed off from the consolidated
// srt/rsrt-3-arg-with-collection-ctx P0 investigation, originally filed as
// `fix/srt-rsrt-ctx-collection-nil`; subsumed here because the root cause
// is the same dropped-error edge in `jit_call_builtin_tree` and the
// User-arm of `jit_call_dyn`.
#[test]
fn rsrt_3arg_map_ctx_mget_misuse_raises_on_every_engine() {
    let src = "mn>L t;scores=mset (mset (mset mmap \"a\" 3) \"b\" 1) \"c\" 2;words=[\"a\",\"b\",\"c\"];rsrt (ctx:M t n w:t>n;v=mget ctx w;??v 0) scores words";
    assert_callback_error(src, "mn", "ILO-R009");
}

// 3-arg closure-bind `srt fn ctx xs` with a list context. The named user-fn
// callback hits the User-arm of `jit_call_dyn` (not the builtin-tree arm),
// so this specifically exercises the second of the two error-dropped sites.
#[test]
fn srt_3arg_list_ctx_at_oob_user_fn_callback_raises_on_every_engine() {
    let src = "keyfn i:n ctx:L n>n;at ctx i\nmn>L n;srt keyfn [0,1,2] [10,30,20]";
    assert_callback_error(src, "mn", "ILO-R009");
}

// Happy-path sanity check: with a non-failing callback, every engine still
// produces the expected sorted list (no regression in the success path).
#[test]
fn srt_happy_path_unchanged_across_engines() {
    let src = "absv n:n>n;?<n 0 (-0 n) n\nmn>L n;srt absv [-3,1,-2,4,-1]";
    for engine in ENGINES_ALL {
        let out = ilo()
            .args([src, engine, "mn"])
            .output()
            .expect("failed to spawn ilo");
        assert!(
            out.status.success(),
            "engine={engine}: srt happy-path failed unexpectedly\nstderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.trim() == "[1, -1, -2, -3, 4]",
            "engine={engine}: srt happy-path got `{}`",
            stdout.trim()
        );
    }
}
