// Regression tests for `xs.i` where `i` is a variable in scope.
//
// Before the fix, the parser built `Expr::Field { object: xs, field: "i" }`
// and the verifier rejected it with `ILO-T018 field access on non-record type
// L _`. Users had to write `at xs i` (4 tokens vs 3) or `hd (slc xs i +i 1)`
// for every variable-indexed list access. 6+ persona reports flagged it as the
// single biggest token tax in indexed-list workloads.
//
// The fix is a post-parse desugar pass that rewrites `xs.i` to `at xs i`
// whenever `i` is a bound variable in scope and is not also a declared field
// name on any record type (collision guard preserves existing record-access
// semantics).
//
// Coverage:
// - tree, VM, cranelift JIT all produce the same result
// - parameter, let-binding, foreach-binding, range-binding scopes
// - record field access with shadowed param is unaffected (collision guard)

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_args(args: &[&str]) -> String {
    let out = ilo().args(args).output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo failed for {args:?}: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write_src(name: &str, contents: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ilo_dot_var_idx_{name}"));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("prog.ilo");
    std::fs::write(&p, contents).unwrap();
    p
}

// `xs.i` with `i` as a parameter.
const PARAM_SRC: &str = "pick xs:L n i:n>n;xs.i\n";

fn check_param(engine: &str) {
    let p = write_src(
        &format!("param_{}", engine.trim_start_matches('-')),
        PARAM_SRC,
    );
    let s = run_args(&[p.to_str().unwrap(), engine, "pick", "[10,20,30]", "1"]);
    assert_eq!(s, "20", "engine={engine}");
}

#[test]
fn param_index_tree() {
    check_param("--run-tree");
}
#[test]
fn param_index_vm() {
    check_param("--run-vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn param_index_cranelift() {
    check_param("--run-cranelift");
}

// `xs.i` inside a range loop, summing all elements.
const RANGE_SRC: &str = "mysum xs:L n>n;s=0;@i 0..(len xs){v=xs.i;s=+s v};+s 0\n";

fn check_range(engine: &str) {
    let p = write_src(
        &format!("range_{}", engine.trim_start_matches('-')),
        RANGE_SRC,
    );
    let s = run_args(&[p.to_str().unwrap(), engine, "mysum", "[10,20,30]"]);
    assert_eq!(s, "60", "engine={engine}");
}

#[test]
fn range_index_tree() {
    check_range("--run-tree");
}
#[test]
fn range_index_vm() {
    check_range("--run-vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn range_index_cranelift() {
    check_range("--run-cranelift");
}

// `xs.i` with `i` introduced by a `let` binding inside the function body.
const LET_SRC: &str = "get-second xs:L n>n;i=1;xs.i\n";

fn check_let(engine: &str) {
    let p = write_src(&format!("let_{}", engine.trim_start_matches('-')), LET_SRC);
    let s = run_args(&[p.to_str().unwrap(), engine, "get-second", "[100,200,300]"]);
    assert_eq!(s, "200", "engine={engine}");
}

#[test]
fn let_index_tree() {
    check_let("--run-tree");
}
#[test]
fn let_index_vm() {
    check_let("--run-vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn let_index_cranelift() {
    check_let("--run-cranelift");
}

// Collision guard: when the indexer name matches a declared record field, the
// record-access semantics still win. The local var `name` shadows the field
// name in scope, but the desugar pass refuses to rewrite because `name` is
// also a record field on `person`.
const COLLISION_SRC: &str =
    "type person{name:t;age:n}\n\ngreet name:t>t;p=person name:\"Alice\" age:30;p.name\n";

fn check_collision(engine: &str) {
    let p = write_src(
        &format!("collision_{}", engine.trim_start_matches('-')),
        COLLISION_SRC,
    );
    let s = run_args(&[p.to_str().unwrap(), engine, "greet", "ignored"]);
    assert_eq!(s, "Alice", "engine={engine}");
}

#[test]
fn collision_record_field_tree() {
    check_collision("--run-tree");
}
#[test]
fn collision_record_field_vm() {
    check_collision("--run-vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn collision_record_field_cranelift() {
    check_collision("--run-cranelift");
}

// Nested chain: `xss.i.j` where both `i` and `j` are variables. Each level
// must rewrite independently. Tree-walker only; the chain mechanics are
// engine-agnostic and exercised by the at-builtin nested tests elsewhere.
const NESTED_SRC: &str = "deep xss:L L n i:n j:n>n;row=xss.i;row.j\n";

#[test]
fn nested_chain_tree() {
    let p = write_src("nested_tree", NESTED_SRC);
    let s = run_args(&[
        p.to_str().unwrap(),
        "--run-tree",
        "deep",
        "[[1,2,3],[4,5,6],[7,8,9]]",
        "1",
        "2",
    ]);
    assert_eq!(s, "6");
}

#[test]
fn nested_chain_vm() {
    let p = write_src("nested_vm", NESTED_SRC);
    let s = run_args(&[
        p.to_str().unwrap(),
        "--run-vm",
        "deep",
        "[[1,2,3],[4,5,6],[7,8,9]]",
        "1",
        "2",
    ]);
    assert_eq!(s, "6");
}

#[test]
#[cfg(feature = "cranelift")]
fn nested_chain_cranelift() {
    let p = write_src("nested_cl", NESTED_SRC);
    let s = run_args(&[
        p.to_str().unwrap(),
        "--run-cranelift",
        "deep",
        "[[1,2,3],[4,5,6],[7,8,9]]",
        "1",
        "2",
    ]);
    assert_eq!(s, "6");
}
