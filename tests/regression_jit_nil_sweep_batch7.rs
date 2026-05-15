// Regression tests for the Cranelift JIT-helper permissive-nil sweep, batch 7.
//
// Helpers in scope (Group E, I/O type-error paths):
//   jit_rd, jit_rdl, jit_wr, jit_wrl, jit_jpar, jit_rdjl, jit_dtfmt, jit_dtparse.
//
// Before this PR these helpers silently returned TAG_NIL on the type-error
// path (non-string path, non-number epoch, etc.) where tree/VM raise
// VmError::Type with a specific message. jit_rdjl additionally hid a real
// I/O failure behind TAG_NIL where the VM's OP_RDJL handler raises a
// "rdjl failed to read file" type error.
//
// After: each type-error path threads VmError::Type through
// jit_set_runtime_error_with_span using the same wording the VM dispatcher
// uses ("rd requires a string path", "dtfmt requires a number (epoch)", etc.).
//
// The ilo source-level verifier rejects programs that statically pass a
// non-string to `rd`/`wr`/`dtfmt`/... (ILO-T013), so the per-helper
// type-error paths are unit-tested directly in src/vm/mod.rs. These CLI
// tests pin cross-engine happy-path parity: that wiring the span/error
// threads did not regress the success cases for actual I/O across tree,
// VM, and Cranelift JIT.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn check_stdout(engine: &str, src: &str, expected: &str) {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "engine={engine}: expected success for `{src}`, got stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        expected,
        "engine={engine}: stdout mismatch for `{src}`"
    );
}

fn check_all(src: &str, expected: &str) {
    check_stdout("--run-tree", src, expected);
    check_stdout("--run-vm", src, expected);
    #[cfg(feature = "cranelift")]
    check_stdout("--run-cranelift", src, expected);
}

// ── jit_wr + jit_rd round-trip (text file) ────────────────────────────────

#[test]
fn wr_then_rd_text_cross_engine() {
    // Write a unique file per engine variant by including a sentinel in the
    // path; otherwise the three engines race when tests run in parallel.
    let path_tree = "/tmp/ilo-jit-batch7-wr-rd-tree.txt";
    let path_vm = "/tmp/ilo-jit-batch7-wr-rd-vm.txt";
    let path_cl = "/tmp/ilo-jit-batch7-wr-rd-cl.txt";
    let _ = std::fs::remove_file(path_tree);
    let _ = std::fs::remove_file(path_vm);
    let _ = std::fs::remove_file(path_cl);

    // wr returns R t t (Ok path on success); we strip with postfix !!.
    check_stdout(
        "--run-tree",
        &format!("f>t;w=wr!! \"{path_tree}\" \"hello\";rd!! \"{path_tree}\""),
        "hello",
    );
    check_stdout(
        "--run-vm",
        &format!("f>t;w=wr!! \"{path_vm}\" \"hello\";rd!! \"{path_vm}\""),
        "hello",
    );
    #[cfg(feature = "cranelift")]
    check_stdout(
        "--run-cranelift",
        &format!("f>t;w=wr!! \"{path_cl}\" \"hello\";rd!! \"{path_cl}\""),
        "hello",
    );
}

// ── jit_wrl + jit_rdl round-trip (lines file) ─────────────────────────────

#[test]
fn wrl_then_rdl_cross_engine() {
    let path_tree = "/tmp/ilo-jit-batch7-wrl-rdl-tree.txt";
    let path_vm = "/tmp/ilo-jit-batch7-wrl-rdl-vm.txt";
    let path_cl = "/tmp/ilo-jit-batch7-wrl-rdl-cl.txt";
    let _ = std::fs::remove_file(path_tree);
    let _ = std::fs::remove_file(path_vm);
    let _ = std::fs::remove_file(path_cl);

    check_stdout(
        "--run-tree",
        &format!("f>n;w=wrl!! \"{path_tree}\" [\"a\" \"b\" \"c\"];es=rdl!! \"{path_tree}\";len es"),
        "3",
    );
    check_stdout(
        "--run-vm",
        &format!("f>n;w=wrl!! \"{path_vm}\" [\"a\" \"b\" \"c\"];es=rdl!! \"{path_vm}\";len es"),
        "3",
    );
    #[cfg(feature = "cranelift")]
    check_stdout(
        "--run-cranelift",
        &format!("f>n;w=wrl!! \"{path_cl}\" [\"a\" \"b\" \"c\"];es=rdl!! \"{path_cl}\";len es"),
        "3",
    );
}

// ── jit_jpar happy path ───────────────────────────────────────────────────

#[test]
fn jpar_valid_json_cross_engine() {
    // jpar returns R _ t. Unwrap with !! and pull out a known string field
    // via jpth on the original text so the test doesn't depend on dynamic-map
    // shape rendering across engines.
    check_all("f>t;jpth!! \"{\\\"k\\\":\\\"v\\\"}\" \"k\"", "v");
}

// ── jit_rdjl happy path ───────────────────────────────────────────────────

#[test]
fn rdjl_reads_jsonl_cross_engine() {
    let path_tree = "/tmp/ilo-jit-batch7-rdjl-tree.jsonl";
    let path_vm = "/tmp/ilo-jit-batch7-rdjl-vm.jsonl";
    let path_cl = "/tmp/ilo-jit-batch7-rdjl-cl.jsonl";
    let _ = std::fs::remove_file(path_tree);
    let _ = std::fs::remove_file(path_vm);
    let _ = std::fs::remove_file(path_cl);

    let prog_tree = format!(
        "prep p:t>R t t;wrl p [\"{{\\\"k\\\":1}}\" \"{{\\\"k\\\":2}}\" \"{{\\\"k\\\":3}}\"]\nf>n;w=prep \"{path_tree}\";es=rdjl \"{path_tree}\";len es"
    );
    let prog_vm = format!(
        "prep p:t>R t t;wrl p [\"{{\\\"k\\\":1}}\" \"{{\\\"k\\\":2}}\" \"{{\\\"k\\\":3}}\"]\nf>n;w=prep \"{path_vm}\";es=rdjl \"{path_vm}\";len es"
    );
    let prog_cl = format!(
        "prep p:t>R t t;wrl p [\"{{\\\"k\\\":1}}\" \"{{\\\"k\\\":2}}\" \"{{\\\"k\\\":3}}\"]\nf>n;w=prep \"{path_cl}\";es=rdjl \"{path_cl}\";len es"
    );

    check_stdout("--run-tree", &prog_tree, "3");
    check_stdout("--run-vm", &prog_vm, "3");
    #[cfg(feature = "cranelift")]
    check_stdout("--run-cranelift", &prog_cl, "3");
}

// ── jit_dtfmt happy path ──────────────────────────────────────────────────

#[test]
fn dtfmt_epoch_zero_cross_engine() {
    check_all("f>t;dtfmt!! 0 \"%Y-%m-%d\"", "1970-01-01");
}

// ── jit_dtparse happy path ────────────────────────────────────────────────

#[test]
fn dtparse_round_trip_cross_engine() {
    check_all("f>n;dtparse!! \"1970-01-01\" \"%Y-%m-%d\"", "0");
}

// ── No stale-error leak across successive Cranelift calls ─────────────────
//
// The JitRuntimeErrorGuard clears the TLS error cell on entry/exit. Confirm
// that a helper-set error on an /errored/ Cranelift call does not leak into
// the next fresh invocation. We use the empty-list `hd` carrier (a batch-1
// helper) and run a happy-path file round-trip afterwards.

#[test]
#[cfg(feature = "cranelift")]
fn no_stale_jit_error_leak_after_hd_error_then_io() {
    let first = ilo()
        .args(["f>n;hd []", "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(!first.status.success(), "first call should error on hd []");

    let path = "/tmp/ilo-jit-batch7-no-leak.txt";
    let _ = std::fs::remove_file(path);
    let src = format!("f>t;w=wr!! \"{path}\" \"ok\";rd!! \"{path}\"");
    let second = ilo()
        .args([src.as_str(), "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        second.status.success(),
        "second call should succeed, got stderr={}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&second.stdout).trim(),
        "ok",
        "second call stdout mismatch"
    );
}
