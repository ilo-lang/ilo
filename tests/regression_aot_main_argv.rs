// Regression: AOT-compiled binaries must bind `main args:L t` (and every
// other list-typed parameter) the same way the tree-walker / VM / JIT do.
//
// The bug (0.12.0): the AOT entry shim ran every argv slot through
// `ilo_aot_parse_arg`, which only ever produces a NanVal number or string —
// never a list. So for `main args:L t > t; cat args ","`:
//
//   * tree / VM / JIT (input `'[foo,bar,baz]'`) → `foo,bar,baz`
//   * AOT             (input `'[foo,bar,baz]'`) → nil  (cat needed a `L t`,
//                                                       got a `t`, no-op'd)
//
// And for `main args:L t > n; len args`:
//
//   * tree / VM / JIT (input `foo`)             → 1   (single-elem list)
//   * AOT             (input `foo`)             → 3   (chars of "foo")
//   * tree / VM / JIT (input `'[foo,bar,baz]'`) → 3   (list length)
//   * AOT             (input `'[foo,bar,baz]'`) → 13  (chars of the literal)
//
// The verifier passed in both cases because the param type is satisfied at
// compile time — the divergence was silent at runtime. cli-builder rerun11
// caught it in a realistic CLI dispatcher.
//
// The fix: `generate_main` now consults the entry function's AST to decide
// per-param whether to call `ilo_aot_parse_arg` (scalar) or the new
// `ilo_aot_parse_arg_list` helper (list-coerce, mirrors the binary-side
// `parse_cli_args_typed` path). The list helper handles `[a,b,c]` literals,
// bare comma lists, and wraps every other shape as `[value]`, matching
// `cli_parse::parse_cli_arg_as_list`.
//
// Coverage: every shape of `main` an agent would plausibly write.
//
// Gated on `cranelift` because AOT compile requires it.

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
    let src = std::env::temp_dir().join(format!("ilo-aot-argv-{tag}-{pid}-{n}.ilo"));
    let bin = std::env::temp_dir().join(format!("ilo-aot-argv-{tag}-{pid}-{n}.bin"));
    (src, bin)
}

/// Compile `source` to `bin` with the AOT pipeline. Panics with a useful
/// message on failure so the test names stay readable.
fn compile(src: &PathBuf, bin: &PathBuf, source: &str) {
    std::fs::write(src, source).expect("write src");
    let out = ilo()
        .args(["compile"])
        .arg(src)
        .arg("-o")
        .arg(bin)
        .output()
        .expect("compile");
    assert!(
        out.status.success(),
        "AOT compile failed: stderr={:?}",
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Run the AOT binary with `args` and return trimmed stdout.
fn run_bin(bin: &PathBuf, args: &[&str]) -> String {
    let out = Command::new(bin)
        .args(args)
        .output()
        .expect("run AOT binary");
    assert!(
        out.status.success(),
        "AOT binary exited non-zero (code={:?}): stderr={:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Run the same source through the tree-walker for cross-engine pinning.
fn run_tree(src: &PathBuf, args: &[&str]) -> String {
    let out = ilo().arg(src).args(args).output().expect("run tree");
    assert!(
        out.status.success(),
        "tree run failed: stderr={:?}",
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Run via the VM engine (`ilo run`) for cross-engine pinning.
fn run_vm(src: &PathBuf, args: &[&str]) -> String {
    let out = ilo()
        .arg("run")
        .arg(src)
        .args(args)
        .output()
        .expect("run vm");
    assert!(
        out.status.success(),
        "vm run failed: stderr={:?}",
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn cleanup(src: &PathBuf, bin: &PathBuf) {
    let _ = std::fs::remove_file(src);
    let _ = std::fs::remove_file(bin);
}

// ── 1. The headline bug: `main args:L t > t; cat args ","` ────────────────

#[test]
fn aot_main_args_l_t_cat_join_matches_tree_and_vm() {
    let (src, bin) = tmp_paths("cat-join");
    let source = "main args:L t>t;cat args \",\"\n";
    compile(&src, &bin, source);

    // Bracketed literal — the canonical multi-element shape.
    let aot = run_bin(&bin, &["[foo,bar,baz]"]);
    let tree = run_tree(&src, &["[foo,bar,baz]"]);
    let vm = run_vm(&src, &["[foo,bar,baz]"]);
    assert_eq!(aot, "foo,bar,baz", "AOT diverged: {aot:?}");
    assert_eq!(aot, tree, "AOT vs tree diverged");
    assert_eq!(aot, vm, "AOT vs VM diverged");

    // Single bare value — must coerce to a single-element list.
    let aot1 = run_bin(&bin, &["foo"]);
    let tree1 = run_tree(&src, &["foo"]);
    assert_eq!(aot1, "foo");
    assert_eq!(aot1, tree1);

    cleanup(&src, &bin);
}

// ── 2. `main args:L t > n; len args` — pins the original failure mode ────

#[test]
fn aot_main_args_l_t_len_matches_tree_and_vm() {
    let (src, bin) = tmp_paths("len");
    let source = "main args:L t>n;len args\n";
    compile(&src, &bin, source);

    // Bracketed list of 3 → len 3 (was 13 = chars of "[foo,bar,baz]")
    assert_eq!(run_bin(&bin, &["[foo,bar,baz]"]), "3");
    assert_eq!(
        run_bin(&bin, &["[foo,bar,baz]"]),
        run_tree(&src, &["[foo,bar,baz]"])
    );
    assert_eq!(
        run_bin(&bin, &["[foo,bar,baz]"]),
        run_vm(&src, &["[foo,bar,baz]"])
    );

    // Bare single value → len 1 (was 3 = chars of "foo")
    assert_eq!(run_bin(&bin, &["foo"]), "1");
    assert_eq!(run_bin(&bin, &["foo"]), run_tree(&src, &["foo"]));

    // Empty list literal → len 0
    assert_eq!(run_bin(&bin, &["[]"]), "0");

    cleanup(&src, &bin);
}

// ── 3. `main xs:L n > n; sum xs` — list-of-numbers shape ──────────────────

#[test]
fn aot_main_args_l_n_sum_matches_tree() {
    let (src, bin) = tmp_paths("sum");
    let source = "main xs:L n>n;sum xs\n";
    compile(&src, &bin, source);

    assert_eq!(run_bin(&bin, &["[1,2,3,4]"]), "10");
    assert_eq!(
        run_bin(&bin, &["[1,2,3,4]"]),
        run_tree(&src, &["[1,2,3,4]"])
    );

    // Bare comma-list also coerces — same path the tree/VM take.
    assert_eq!(run_bin(&bin, &["1,2,3"]), "6");

    // Single number → wrapped as [n], sum returns the number.
    assert_eq!(run_bin(&bin, &["7"]), "7");

    cleanup(&src, &bin);
}

// ── 4. Scalar `main x:n > n; x` — must NOT regress (was already correct) ─

#[test]
fn aot_main_scalar_number_param_still_works() {
    let (src, bin) = tmp_paths("scalar-num");
    let source = "main x:n>n;x\n";
    compile(&src, &bin, source);

    assert_eq!(run_bin(&bin, &["42"]), "42");
    assert_eq!(run_bin(&bin, &["3.5"]), "3.5");

    cleanup(&src, &bin);
}

// ── 5. Mixed scalar + list params: `main name:t xs:L n > t; cat ...` ──────

#[test]
fn aot_main_mixed_scalar_then_list_param() {
    let (src, bin) = tmp_paths("mixed");
    // `cat [name, str (sum xs)] ":"` — pins both that scalar-text and
    // list-of-number params land in the same call with the correct shapes.
    let source = "main name:t xs:L n>t;cat [name, str (sum xs)] \":\"\n";
    compile(&src, &bin, source);

    assert_eq!(run_bin(&bin, &["total", "[1,2,3]"]), "total:6");
    assert_eq!(
        run_bin(&bin, &["total", "[1,2,3]"]),
        run_tree(&src, &["total", "[1,2,3]"])
    );

    cleanup(&src, &bin);
}

// ── 6. `main>n` (zero-arg main) — must NOT regress (no params, no parse) ─

#[test]
fn aot_main_zero_arg_still_works() {
    let (src, bin) = tmp_paths("zero-arg");
    let source = "main>n;42\n";
    compile(&src, &bin, source);

    assert_eq!(run_bin(&bin, &[]), "42");

    cleanup(&src, &bin);
}

// ── 7. funcname-argv off-by-one: `./bin main arg` must bind `arg`, not `main`
//
// The bug: `generate_main` read argv[1..=param_count] unconditionally, so
// `./bin main 42` bound "main" as the first param and 42 as the second (or
// off the end). A ternary `?(x){true:1;false:0}` silently picked false because
// the string "main" is truthy in ilo but the binding was one slot off, feeding
// an out-of-bounds or wrong value. The fix calls `ilo_aot_argv_skip` at runtime
// to detect whether argv[1] matches the entry name and shifts the base by 8
// bytes when it does, so both `./bin arg` and `./bin funcname arg` are correct.
// ─────────────────────────────────────────────────────────────────────────────

/// `./bin main 42` with a scalar numeric param — must produce 42, not bind "main".
#[test]
fn aot_funcname_argv_scalar_number() {
    let (src, bin) = tmp_paths("fn-argv-num");
    let source = "main x:n>n;x\n";
    compile(&src, &bin, source);

    // Without funcname prefix - baseline.
    assert_eq!(run_bin(&bin, &["42"]), "42");

    // With funcname prefix - the bug would have bound "main" as x.
    assert_eq!(run_bin(&bin, &["main", "42"]), "42");

    cleanup(&src, &bin);
}

/// `./bin main hello` with a scalar text param.
#[test]
fn aot_funcname_argv_scalar_text() {
    let (src, bin) = tmp_paths("fn-argv-txt");
    let source = "main s:t>t;s\n";
    compile(&src, &bin, source);

    assert_eq!(run_bin(&bin, &["hello"]), "hello");
    assert_eq!(run_bin(&bin, &["main", "hello"]), "hello");

    cleanup(&src, &bin);
}

/// Ternary branch on the bound value - this was the silent false-branch symptom.
#[test]
fn aot_funcname_argv_ternary_picks_correct_branch() {
    let (src, bin) = tmp_paths("fn-argv-ternary");
    // `?(x > 10){true:1;false:0}` - with x=42 should be 1; x bound to "main"
    // would break the comparison and likely return 0.
    let source = "main x:n>n;?(x>10){true:1;false:0}\n";
    compile(&src, &bin, source);

    assert_eq!(
        run_bin(&bin, &["42"]),
        "1",
        "without funcname: x=42 > 10 should be true"
    );
    assert_eq!(
        run_bin(&bin, &["main", "42"]),
        "1",
        "with funcname: x=42 > 10 should be true - was silently false before fix"
    );
    assert_eq!(
        run_bin(&bin, &["5"]),
        "0",
        "without funcname: x=5 <= 10 should be false"
    );
    assert_eq!(
        run_bin(&bin, &["main", "5"]),
        "0",
        "with funcname: x=5 <= 10 should be false"
    );

    cleanup(&src, &bin);
}

/// Two-param function with funcname prefix - both args must shift correctly.
#[test]
fn aot_funcname_argv_two_params() {
    let (src, bin) = tmp_paths("fn-argv-2p");
    let source = "main a:n b:n>n;a+b\n";
    compile(&src, &bin, source);

    assert_eq!(run_bin(&bin, &["3", "4"]), "7");
    assert_eq!(run_bin(&bin, &["main", "3", "4"]), "7");

    cleanup(&src, &bin);
}

/// List-typed param with funcname prefix - must use the list-parse path after skip.
#[test]
fn aot_funcname_argv_list_param() {
    let (src, bin) = tmp_paths("fn-argv-list");
    let source = "main xs:L n>n;sum xs\n";
    compile(&src, &bin, source);

    assert_eq!(run_bin(&bin, &["[1,2,3]"]), "6");
    assert_eq!(run_bin(&bin, &["main", "[1,2,3]"]), "6");

    cleanup(&src, &bin);
}

/// Zero-arg function with funcname prefix - no args to shift, must not crash.
#[test]
fn aot_funcname_argv_zero_params_with_funcname() {
    let (src, bin) = tmp_paths("fn-argv-zero");
    let source = "main>n;99\n";
    compile(&src, &bin, source);

    assert_eq!(run_bin(&bin, &[]), "99");
    // funcname with no user args: argc=2, argv[1]="main" matches, skip=8.
    // The param loop runs 0 times so no out-of-bounds access.
    assert_eq!(run_bin(&bin, &["main"]), "99");

    cleanup(&src, &bin);
}

/// Cross-engine pin: both invocation forms produce output matching tree and VM.
#[test]
fn aot_funcname_argv_cross_engine_pin() {
    let (src, bin) = tmp_paths("fn-argv-cross");
    let source = "main x:n>t;str x\n";
    compile(&src, &bin, source);

    let direct = run_bin(&bin, &["7"]);
    let with_fn = run_bin(&bin, &["main", "7"]);
    let tree = run_tree(&src, &["7"]);
    let vm = run_vm(&src, &["7"]);

    assert_eq!(direct, "7");
    assert_eq!(with_fn, "7");
    assert_eq!(direct, tree, "AOT vs tree diverged");
    assert_eq!(direct, vm, "AOT vs VM diverged");

    cleanup(&src, &bin);
}
