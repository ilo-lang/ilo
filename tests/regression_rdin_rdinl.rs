// Cross-engine regression tests for `rdin` and `rdinl` — the stdin read
// primitives added in 0.12.1.
//
// `rdin > R t t` reads all of stdin as text.
// `rdinl > R (L t) t` reads stdin as a list of lines (newlines stripped).
//
// Both are 0-arg and lower through the tree-bridge, so VM and Cranelift
// inherit them without native opcodes. The test fans across engines to catch
// any future bridge divergence.
//
// Testing stdin requires subprocess piping. Each case writes a short ilo
// program to a tempfile, invokes `ilo` with stdin piped, and asserts stdout.

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
    let mut v = vec!["--vm"];
    if cfg!(feature = "cranelift") {
        v.push("--jit");
    }
    v
}

fn with_tempfile(src: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("tempfile");
    f.write_all(src.as_bytes()).expect("write tempfile");
    f
}

// ── rdin ──────────────────────────────────────────────────────────────────

#[test]
fn rdin_reads_full_stdin() {
    let src = "main>t\nrdin!";
    let f = with_tempfile(src);
    let path = f.path().to_str().unwrap();
    let input = b"hello world\n";
    for eng in engines() {
        let got = run_with_stdin(eng, path, "main", input);
        assert_eq!(got, "hello world", "rdin full read failed on engine {eng}");
    }
}

#[test]
fn rdin_multiline() {
    let src = "main>t\nrdin!";
    let f = with_tempfile(src);
    let path = f.path().to_str().unwrap();
    let input = b"line1\nline2\nline3\n";
    for eng in engines() {
        let got = run_with_stdin(eng, path, "main", input);
        // rdin returns the raw text including embedded newlines
        assert!(
            got.contains("line1") && got.contains("line2") && got.contains("line3"),
            "rdin multiline failed on engine {eng}: got {got:?}"
        );
    }
}

#[test]
fn rdin_empty_stdin() {
    // empty stdin -> rdin returns "" -> len is 0
    let src = "main>n;s=rdin!;len s";
    let f = with_tempfile(src);
    let path = f.path().to_str().unwrap();
    for eng in engines() {
        let got = run_with_stdin(eng, path, "main", b"");
        assert_eq!(
            got, "0",
            "rdin empty stdin should return empty string, engine {eng}"
        );
    }
}

// ── rdinl ─────────────────────────────────────────────────────────────────

#[test]
fn rdinl_splits_lines() {
    let src = "main>t;lines=rdinl!;cat lines \",\"";
    let f = with_tempfile(src);
    let path = f.path().to_str().unwrap();
    let input = b"alpha\nbeta\ngamma\n";
    for eng in engines() {
        let got = run_with_stdin(eng, path, "main", input);
        assert_eq!(
            got, "alpha,beta,gamma",
            "rdinl line split failed on engine {eng}"
        );
    }
}

#[test]
fn rdinl_empty_stdin_is_empty_list() {
    let src = "main>n;lines=rdinl!;len lines";
    let f = with_tempfile(src);
    let path = f.path().to_str().unwrap();
    for eng in engines() {
        let got = run_with_stdin(eng, path, "main", b"");
        assert_eq!(
            got, "0",
            "rdinl empty stdin should return empty list, engine {eng}"
        );
    }
}

#[test]
fn rdinl_single_line_no_newline() {
    // stdin without trailing newline: still one line element
    let src = "main>n;lines=rdinl!;len lines";
    let f = with_tempfile(src);
    let path = f.path().to_str().unwrap();
    for eng in engines() {
        let got = run_with_stdin(eng, path, "main", b"hello");
        assert_eq!(
            got, "1",
            "rdinl single line (no trailing newline) failed on engine {eng}"
        );
    }
}

#[test]
fn rdinl_strips_newlines() {
    // Lines must not contain the newline character
    let src = "main>t;lines=rdinl!;hd lines";
    let f = with_tempfile(src);
    let path = f.path().to_str().unwrap();
    for eng in engines() {
        let got = run_with_stdin(eng, path, "main", b"hello\nworld\n");
        assert_eq!(
            got, "hello",
            "rdinl did not strip newline from head element, engine {eng}"
        );
    }
}
