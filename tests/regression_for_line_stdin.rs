// Regression tests for `for-line stdin` — the lazy stdin line iterator (ILO-70).
//
// `for-line stdin > LazyStdinLines` produces a handle that `@binding` foreach
// drains one line at a time, enabling processing of unbounded piped streams
// without buffering the full input.
//
// Tests follow the pattern established by regression_rdin_rdinl.rs:
// spawn `ilo` as a subprocess with stdin piped, assert stdout.

use std::io::Write;
use std::process::{Command, Stdio};

fn ilo_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_ilo"))
}

/// Run `ilo <src_path> <engine_flag> <entry>` with `stdin_data` piped in.
/// Returns trimmed stdout on success (exit 0), panics with stderr on failure.
fn run_with_stdin(engine: &str, src: &str, entry: &str, stdin_data: &[u8]) -> String {
    let bin = ilo_bin();
    let args: Vec<&str> = vec![src, engine, entry];
    let mut child = Command::new(&bin)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ilo");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin_data)
        .expect("failed to write stdin");
    let out = child.wait_with_output().expect("failed to wait");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn engines() -> Vec<&'static str> {
    // for-line `@binding` foreach runs through the VM's OP_FOREACHPREP /
    // OP_FOREACHNEXT which have been extended to handle HeapObj::LazyStdinLines.
    // The Cranelift JIT compiles OP_FOREACHPREP into machine code that reads
    // the Vec layout directly — it does not yet have a special path for lazy
    // handles. JIT support is a follow-up (ILO-70 scopes to tree + VM only).
    vec!["--vm"]
}

fn with_tempfile(src: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("tempfile");
    f.write_all(src.as_bytes()).expect("write tempfile");
    f
}

// ── basic foreach over for-line stdin ────────────────────────────────────────

#[test]
fn for_line_basic_foreach() {
    // Collect lines into a list by appending, then join with comma.
    // Note: ilo uses semicolons as statement separators within a function.
    // Print each line via prnt; verify each line appears in output.
    let src = r#"main>n;n=0;@line (for-line "stdin"){prnt line;n=+ n 1};n"#;
    let f = with_tempfile(src);
    let path = f.path().to_str().unwrap();
    let input = b"alpha\nbeta\ngamma\n";
    for eng in engines() {
        let got = run_with_stdin(eng, path, "main", input);
        assert!(
            got.contains("alpha") && got.contains("beta") && got.contains("gamma"),
            "for-line should emit each line, engine {eng}; got: {got:?}"
        );
        assert!(
            got.ends_with('3'),
            "for-line should count 3 lines, engine {eng}; got: {got:?}"
        );
    }
}

#[test]
fn for_line_empty_stdin() {
    // Empty stdin: loop body never runs, result is the initial accumulator (0).
    let src = r#"main>n;n=0;@line (for-line "stdin"){n=+ n 1};n"#;
    let f = with_tempfile(src);
    let path = f.path().to_str().unwrap();
    for eng in engines() {
        let got = run_with_stdin(eng, path, "main", b"");
        assert_eq!(
            got, "0",
            "for-line empty stdin should iterate zero times, engine {eng}"
        );
    }
}

#[test]
fn for_line_counts_lines() {
    // Count lines: verify each line triggers one iteration.
    let src = r#"main>n;n=0;@line (for-line "stdin"){n=+ n 1};n"#;
    let f = with_tempfile(src);
    let path = f.path().to_str().unwrap();
    let input = b"a\nb\nc\n";
    for eng in engines() {
        let got = run_with_stdin(eng, path, "main", input);
        assert_eq!(got, "3", "for-line should count 3 lines, engine {eng}");
    }
}

#[test]
fn for_line_strips_newlines() {
    // Each line value must not contain the trailing newline character.
    let src = r#"main>t;last="";@line (for-line "stdin"){last=line};last"#;
    let f = with_tempfile(src);
    let path = f.path().to_str().unwrap();
    let input = b"hello\nworld\n";
    for eng in engines() {
        let got = run_with_stdin(eng, path, "main", input);
        assert_eq!(
            got, "world",
            "for-line last line should be 'world' without newline, engine {eng}"
        );
    }
}

#[test]
fn for_line_single_line_no_trailing_newline() {
    // Input without trailing newline — should yield one line.
    let src = r#"main>n;n=0;@line (for-line "stdin"){n=+ n 1};n"#;
    let f = with_tempfile(src);
    let path = f.path().to_str().unwrap();
    let input = b"hello";
    for eng in engines() {
        let got = run_with_stdin(eng, path, "main", input);
        assert_eq!(
            got, "1",
            "for-line single line (no trailing newline) should count 1, engine {eng}"
        );
    }
}
