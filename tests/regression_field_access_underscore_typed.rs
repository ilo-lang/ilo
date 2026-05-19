// Regression tests for the json-shaper rerun10 silent-correctness bug:
// field access on `_`-typed params was resolved via the offsets of a
// same-named declared `type`, not the runtime record's actual layout.
//
// Pre-fix behaviour (tree-walk correct, VM + JIT + AOT silently wrong):
//
//     type row{sku:t;qty:n;rev:n}
//     main>R t t;all=jpar! "[{\"sku\":\"x\",\"qty\":2,\"rev\":99}]";~jdmp (map (it:_>t;it.sku) all)
//
//   Expected `["x"]`. Tree-walker returned `["x"]`; VM, Cranelift JIT, and
//   AOT all returned `[2]` because `jpar` records are sorted alphabetically
//   (qty, rev, sku) and the bytecode compiler reached for the row-type's
//   field offset (sku=0) via `search_field_index`. That offset happened to
//   point at `qty` in the live record, so the agent silently got the wrong
//   value with no diagnostic.
//
// Post-fix behaviour: when the object's static record type is unknown
// (e.g. a `_`-typed param), the VM emits the name-based `OP_RECFLD_NAME`
// instead of the positional `OP_RECFLD`. The runtime cost is one map
// lookup; the correctness win is "agents can trust `it.sku`."
//
// The fix touches three call sites in `src/vm/mod.rs` that used
// `search_field_index` as a fallback when `reg_record_type == u16::MAX`:
// `Expr::Field`, `Stmt::Destructure`, and `Expr::With`. All three now
// fall through to the name-keyed dynamic path instead.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(entry);
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── The persona's exact repro (json-shaper rerun10) ───────────────────────
//
// `type row{sku:t;qty:n;rev:n}` is declared; the lambda parameter is `_`;
// the runtime record comes out of `jpar` with alphabetical keys
// (`qty, rev, sku`). `it.sku` must return `"x"`, not `2`.

const REPRO: &str = r#"type row{sku:t;qty:n;rev:n}
main>R t t
  all=jpar! "[{\"sku\":\"x\",\"qty\":2,\"rev\":99}]"
  ~jdmp (map (it:_>t;it.sku) all)"#;

fn check_repro(engine: &str) {
    assert_eq!(
        run(engine, REPRO, "main"),
        r#"["x"]"#,
        "engine={engine}: `it.sku` on a `_`-typed jpar'd record must \
         return \"x\", not the value at row-type offset 0"
    );
}

#[test]
fn field_access_underscore_typed_tree() {
    check_repro("--run-tree");
}

#[test]
fn field_access_underscore_typed_vm() {
    check_repro("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn field_access_underscore_typed_jit() {
    check_repro("--jit");
}

// ── Also exercise the other two fields to be sure the mapping is correct
// across the layout difference, not just luckily-aligned. The user-type
// puts sku at 0, qty at 1, rev at 2; the jpar record (alphabetical) puts
// qty at 0, rev at 1, sku at 2.

const REPRO_QTY: &str = r#"type row{sku:t;qty:n;rev:n}
main>R t t
  all=jpar! "[{\"sku\":\"x\",\"qty\":2,\"rev\":99}]"
  ~jdmp (map (it:_>n;it.qty) all)"#;

const REPRO_REV: &str = r#"type row{sku:t;qty:n;rev:n}
main>R t t
  all=jpar! "[{\"sku\":\"x\",\"qty\":2,\"rev\":99}]"
  ~jdmp (map (it:_>n;it.rev) all)"#;

fn check_qty(engine: &str) {
    assert_eq!(run(engine, REPRO_QTY, "main"), "[2]", "engine={engine}");
}

fn check_rev(engine: &str) {
    assert_eq!(run(engine, REPRO_REV, "main"), "[99]", "engine={engine}");
}

#[test]
fn field_access_qty_tree() {
    check_qty("--run-tree");
}
#[test]
fn field_access_qty_vm() {
    check_qty("--run-vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn field_access_qty_jit() {
    check_qty("--jit");
}

#[test]
fn field_access_rev_tree() {
    check_rev("--run-tree");
}
#[test]
fn field_access_rev_vm() {
    check_rev("--run-vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn field_access_rev_jit() {
    check_rev("--jit");
}

// ── Typed-object fast path is unaffected ──────────────────────────────────
//
// When the object's static type IS a declared record (no `_` indirection),
// the compiler still uses the positional `OP_RECFLD` fast path, and the
// answer remains correct.

const TYPED_FAST_PATH: &str = r#"type pt{x:n;y:n}
main>R n t
  p=pt x:10 y:20
  ~p.x"#;

fn check_typed_fast(engine: &str) {
    assert_eq!(
        run(engine, TYPED_FAST_PATH, "main"),
        "10",
        "engine={engine}"
    );
}

#[test]
fn typed_record_fast_path_tree() {
    check_typed_fast("--run-tree");
}
#[test]
fn typed_record_fast_path_vm() {
    check_typed_fast("--run-vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn typed_record_fast_path_jit() {
    check_typed_fast("--jit");
}
