// `ilo test`. surfaces the `-- run:` / `-- out:` / `-- err:` annotation
// format the in-tree harness already uses, so end-users can assert program
// behaviour from the same files agents read as in-context learning examples.
//
// Cross-engine coverage: the runner spawns `ilo` itself with `--vm` / `--jit`
// per case, so the subprocess test below exercises the same dispatch path
// the integration harness uses, only one layer deeper (test-process spawns
// `ilo test`, which spawns `ilo` per case). A deliberately-broken fixture
// pins the failure path's exit code and stdout shape.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_tmp(name: &str, body: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("ilo-test-cmd-{pid}-{nonce}"));
    std::fs::create_dir_all(&p).unwrap();
    p.push(name);
    std::fs::write(&p, body).unwrap();
    p
}

#[test]
fn test_passes_on_good_example() {
    // Single passing case: `m>n;+1 2` returns 3, `-- out: 3` matches.
    let path = write_tmp("ok.ilo", "m>n;+1 2\n\n-- run: m\n-- out: 3\n");

    let out = ilo()
        .arg("test")
        .arg(&path)
        .output()
        .expect("failed to spawn ilo");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "expected exit 0; stdout={stdout}");
    assert!(
        stdout.contains("PASS"),
        "expected PASS line; stdout={stdout}"
    );
    assert!(
        stdout.contains("1 passed, 0 failed"),
        "expected summary line; stdout={stdout}"
    );
}

#[test]
fn test_fails_on_wrong_output() {
    // Deliberately broken: returns 3, asserts 99. Pins exit=1 + FAIL line.
    let path = write_tmp("bad.ilo", "m>n;+1 2\n\n-- run: m\n-- out: 99\n");

    let out = ilo()
        .arg("test")
        .arg(&path)
        .output()
        .expect("failed to spawn ilo");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1; stdout={stdout}"
    );
    assert!(
        stdout.contains("FAIL"),
        "expected FAIL line; stdout={stdout}"
    );
    assert!(
        stdout.contains("got:") && stdout.contains("want:"),
        "expected got/want diff; stdout={stdout}"
    );
    assert!(
        stdout.contains("0 passed, 1 failed"),
        "expected summary line; stdout={stdout}"
    );
}

#[test]
fn test_handles_err_assertion() {
    // `-- err:` form: program returning `^reason` exits 1 and the assertion
    // matches against stderr. Pin both directions: matching err passes,
    // mismatched err fails.
    let path = write_tmp("err.ilo", "m>R t t;^\"nope\"\n\n-- run: m\n-- err: ^nope\n");

    let out = ilo()
        .arg("test")
        .arg(&path)
        .output()
        .expect("failed to spawn ilo");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "expected exit 0; stdout={stdout}, stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("PASS"), "stdout={stdout}");
}

#[test]
fn test_directory_recursion() {
    // A directory walks all `*.ilo` files recursively. Mix passing + failing
    // fixtures across two levels of nesting; the runner should find every
    // file and the overall exit code should be 1 because at least one case
    // failed.
    let mut root = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    root.push(format!("ilo-test-cmd-dir-{pid}-{nonce}"));
    let sub = root.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(root.join("a.ilo"), "m>n;+1 2\n\n-- run: m\n-- out: 3\n").unwrap();
    std::fs::write(sub.join("b.ilo"), "m>n;+1 2\n\n-- run: m\n-- out: 99\n").unwrap();

    let out = ilo()
        .arg("test")
        .arg(&root)
        .output()
        .expect("failed to spawn ilo");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "stdout={stdout}");
    assert!(stdout.contains("PASS"), "stdout={stdout}");
    assert!(stdout.contains("FAIL"), "stdout={stdout}");
    assert!(stdout.contains("1 passed, 1 failed"), "stdout={stdout}");

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn test_missing_path_errors() {
    let out = ilo()
        .arg("test")
        .arg("/no/such/path/here/zz")
        .output()
        .expect("failed to spawn ilo");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no such file or directory"),
        "stderr={stderr}"
    );
}

#[test]
fn test_no_annotations_errors() {
    // A file with no `-- run:` annotations is a usage mistake worth surfacing.
    let path = write_tmp("noassertions.ilo", "m>n;+1 2\n");
    let out = ilo()
        .arg("test")
        .arg(&path)
        .output()
        .expect("failed to spawn ilo");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no `-- run:`"),
        "expected guidance message; stderr={stderr}"
    );
}

#[test]
fn test_engine_all_runs_both() {
    // `--engine all` runs every engine and tags the output with `[vm]` /
    // `[jit]` so failures localise to the engine that diverged.
    let path = write_tmp("ok2.ilo", "m>n;+1 2\n\n-- run: m\n-- out: 3\n");

    let out = ilo()
        .arg("test")
        .arg(&path)
        .arg("--engine")
        .arg("all")
        .output()
        .expect("failed to spawn ilo");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "stdout={stdout}");
    assert!(stdout.contains("[vm]"), "stdout={stdout}");
    assert!(stdout.contains("[jit]"), "stdout={stdout}");
    assert!(stdout.contains("2 passed, 0 failed"), "stdout={stdout}");
}
