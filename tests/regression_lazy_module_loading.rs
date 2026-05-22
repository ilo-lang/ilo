// Regression tests for lazy / on-demand module loading (ILO-400).
//
// `use lazy:"./path"` defers module resolution until at least one
// `<stem>-*` symbol from the module is referenced in the program.
// If no symbol is referenced the file is never opened.
//
// Contracts locked in here:
// 1. A lazy import where no `<stem>-*` symbol is referenced → module
//    never opened; missing file causes no error.
// 2. A lazy import where a `<stem>-*` symbol IS referenced → module
//    is loaded and its declarations are available at runtime.

use std::io::Write;
use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

// Write `content` to `path`, returning the path.
fn write_tmp(path: &str, content: &str) -> String {
    let mut f = std::fs::File::create(path).expect("create tmp file");
    write!(f, "{}", content).expect("write tmp file");
    path.to_string()
}

// ── 1. Lazy import not opened when unreferenced ───────────────────────────

#[test]
fn lazy_import_not_opened_when_unreferenced() {
    // The big-module file does NOT exist. The importer never calls big-module-*.
    // Resolution must succeed with no errors.
    let importer_path = write_tmp(
        "/tmp/ilo_lazy_importer_ILO400.ilo",
        // Uses lazy but calls nothing from big-module
        r#"use lazy:"./ilo_lazy_BIG_MODULE_NONEXISTENT_ILO400.ilo"
main>n;42"#,
    );

    let out = ilo()
        .args([&importer_path, "--vm", "main"])
        .output()
        .expect("failed to run ilo");

    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

    assert!(
        out.status.success(),
        "lazy import of missing module must succeed when unreferenced; stderr={stderr}"
    );
    assert_eq!(stdout, "42", "expected 42, got: {stdout}");

    std::fs::remove_file(&importer_path).ok();
}

// ── 2. Lazy import loaded when referenced ────────────────────────────────

#[test]
fn lazy_import_loaded_when_referenced() {
    // The module file is named `bigmod.ilo` → stem is `bigmod`.
    // The importer calls `bigmod-answer` (stem-prefixed symbol).
    // The module declares `answer>n;99`; after lazy load it becomes `bigmod-answer`.
    let module_path = write_tmp("/tmp/bigmod.ilo", "answer>n;99");
    let importer_path = write_tmp(
        "/tmp/ilo_lazy_caller_ILO400.ilo",
        "use lazy:\"./bigmod.ilo\"\nmain>n;bigmod-answer()",
    );

    let out = ilo()
        .args([&importer_path, "--vm", "main"])
        .output()
        .expect("failed to run ilo");

    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

    assert!(
        out.status.success(),
        "lazy import must be loaded when referenced; stderr={stderr}"
    );
    assert_eq!(
        stdout, "99",
        "expected 99 from bigmod-answer, got: {stdout}"
    );

    std::fs::remove_file(&module_path).ok();
    std::fs::remove_file(&importer_path).ok();
}
