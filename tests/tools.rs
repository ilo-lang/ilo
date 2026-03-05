/// Integration tests for the tools infrastructure (D1d–D1f).
///
/// Unit-level tests (Value JSON round-trips, etc.) live in
/// `src/interpreter/json.rs`.  These tests exercise the CLI end-to-end.
use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

// ── Tool stub: program with a tool decl runs without a --tools config ─────────

/// A program that declares a tool and calls it from main should compile and run
/// using the built-in stub (returns ~nil).
#[test]
fn tool_call_stub_via_interp() {
    // ilo syntax: tool <name>"<desc>" params>return
    let prog = r#"tool mytool"a helper" x:t>R _ t
main x:t>R _ t;mytool x"#;
    let out = ilo()
        .args([prog, "--run-interp", "main", "hello"])
        .output()
        .expect("ilo failed to start");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // stub returns ~nil which displays as "~nil"
    assert_eq!(stdout.trim(), "~nil");
}

/// Same program via the register VM.
#[test]
fn tool_call_stub_via_vm() {
    let prog = r#"tool mytool"a helper" x:t>R _ t
main x:t>R _ t;mytool x"#;
    let out = ilo()
        .args([prog, "--run-vm", "main", "hello"])
        .output()
        .expect("ilo failed to start");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "~nil");
}

// ── --tools flag requires a path ────────────────────────────────────────────

/// Passing `--tools` with a path to a file that doesn't exist should
/// produce an error about reading the file.
#[test]
fn tools_flag_nonexistent_path() {
    let prog = r#"tool mytool"a helper" x:t>R _ t
main x:t>R _ t;mytool x"#;
    let out = ilo()
        .args([prog, "--tools", "/nonexistent/path/to/config.json", "--run-interp", "main", "hello"])
        .output()
        .expect("ilo failed to start");
    assert!(
        !out.status.success(),
        "expected failure when tools config path does not exist"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("failed to read tools config") || stderr.contains("No such file"),
        "expected file-not-found error, got: {}",
        stderr
    );
}

// ── --tools with a valid config file ─────────────────────────────────────────

/// Create a minimal tools config JSON and verify the CLI accepts it.
/// The tool call will attempt a network connection but we don't care about
/// the result here — just that parsing and dispatch don't panic.
#[test]
fn tools_flag_with_valid_config() {
    use std::io::Write;

    let (path, mut file) = tempfile_in_tmp("tools_config_valid.json");
    writeln!(
        file,
        r#"{{"tools": {{"mytool": {{"url": "http://127.0.0.1:19999/mytool"}}}}}}"#
    )
    .unwrap();
    drop(file);

    // Program that ignores the tool result
    let prog = r#"tool mytool"a helper" x:t>R _ t
main x:t>t;"hello""#;

    let out = ilo()
        .args([prog, "--tools", &path, "--run-interp", "main", "world"])
        .output()
        .expect("ilo failed to start");

    // No panic / ICE should occur
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("thread 'main' panicked"),
        "unexpected panic: {}",
        stderr
    );

    std::fs::remove_file(&path).ok();
}

// ── AST: tool declaration appears in JSON AST ─────────────────────────────

#[test]
fn tool_decl_in_ast_json() {
    let prog = r#"tool mytool"a helper" x:t>R _ t"#;
    let out = ilo()
        .args([prog])
        .output()
        .expect("ilo failed to start");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    // The AST JSON should contain "Tool" and "mytool"
    assert!(
        stdout.contains("Tool") || stdout.contains("mytool"),
        "expected tool in AST, got: {}",
        stdout
    );
}

// ── HTTP provider config parsing ──────────────────────────────────────────

/// Verify that an invalid JSON tools config produces a clear error message.
#[test]
fn tools_flag_invalid_json_config() {
    use std::io::Write;

    let (path, mut file) = tempfile_in_tmp("bad_config.json");
    write!(file, "not valid json").unwrap();
    drop(file);

    let prog = r#"tool mytool"a helper" x:t>R _ t
main x:t>R _ t;mytool x"#;
    let out = ilo()
        .args([prog, "--tools", &path, "--run-interp", "main", "hello"])
        .output()
        .expect("ilo failed to start");

    assert!(!out.status.success(), "expected failure for invalid JSON config");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("failed to parse tools config") || stderr.contains("parse") || stderr.contains("JSON"),
        "expected parse error in stderr, got: {}",
        stderr
    );

    std::fs::remove_file(&path).ok();
}

// ── Tool stub returns ~nil from VM with is_tool flag ──────────────────────

/// Verify the `is_tool` flag in CompiledProgram is respected by the VM:
/// the stub chunk returns ~nil without entering a normal call frame.
#[test]
fn vm_tool_stub_returns_ok_nil() {
    let prog = r#"tool mytool"a helper" x:t>R _ t
f>R _ t;mytool "test""#;
    let out = ilo()
        .args([prog, "--run-vm", "f"])
        .output()
        .expect("ilo failed to start");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "~nil");
}

/// Multiple tool declarations — all return stubs.
#[test]
fn vm_multiple_tool_stubs() {
    let prog = r#"tool a"first" x:t>R _ t
tool b"second" x:t>R _ t
f>R _ t;b "test""#;
    let out = ilo()
        .args([prog, "--run-vm", "f"])
        .output()
        .expect("ilo failed to start");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "~nil");
}

// ── Ignored: real HTTP test ───────────────────────────────────────────────

/// End-to-end HTTP test — ignored by default, requires a live server.
#[test]
#[ignore]
fn get_builtin_real_http() {
    let prog = "main x:t>R t t;$x";
    let out = ilo()
        .args([prog, "--run-interp", "main", "https://httpbin.org/get"])
        .output()
        .expect("ilo failed to start");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

// ── Helper ────────────────────────────────────────────────────────────────

/// Create a named temp file in the system temp directory.
/// Returns (path_string, File handle).
fn tempfile_in_tmp(name: &str) -> (String, std::fs::File) {
    let mut path = std::env::temp_dir();
    path.push(format!("ilo_test_{name}"));
    let f = std::fs::File::create(&path).expect("create temp file");
    (path.to_string_lossy().into_owned(), f)
}

// ── Wiremock HTTP provider test (feature-gated) ───────────────────────────

#[cfg(feature = "tools")]
mod http_tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn http_provider_success() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/double"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(4.0)))
            .mount(&server)
            .await;

        let config_json = serde_json::json!({
            "tools": {
                "double": {
                    "url": format!("{}/double", server.uri())
                }
            }
        });

        let mut p = std::env::temp_dir();
        p.push("ilo_wiremock_test.json");
        std::fs::write(&p, config_json.to_string()).unwrap();

        let prog = r#"tool double"doubles a number" x:n>R n n
main x:n>R n n;double x"#;
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_ilo"))
            .args([
                prog,
                "--tools",
                p.to_str().unwrap(),
                "--run-interp",
                "main",
                "2",
            ])
            .output()
            .expect("ilo failed to start");

        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "stderr: {stderr}\nstdout: {stdout}");

        std::fs::remove_file(&p).ok();
    }

    #[tokio::test]
    async fn http_provider_not_configured() {
        let config_json = serde_json::json!({ "tools": {} });
        let mut p = std::env::temp_dir();
        p.push("ilo_wiremock_not_configured.json");
        std::fs::write(&p, config_json.to_string()).unwrap();

        let prog = r#"tool double"doubles a number" x:n>R n n
main x:n>R n n;double x"#;
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_ilo"))
            .args([
                prog,
                "--tools",
                p.to_str().unwrap(),
                "--run-interp",
                "main",
                "2",
            ])
            .output()
            .expect("ilo failed to start");

        // The call fails with "tool not configured: double" → RuntimeError → exit 1
        assert!(!out.status.success(), "expected failure for unconfigured tool");

        std::fs::remove_file(&p).ok();
    }

    #[tokio::test]
    async fn http_provider_server_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/double"))
            .respond_with(
                ResponseTemplate::new(500)
                    .set_body_json(serde_json::json!({"error": "internal"})),
            )
            .mount(&server)
            .await;

        let config_json = serde_json::json!({
            "tools": {
                "double": {
                    "url": format!("{}/double", server.uri())
                }
            }
        });

        let mut p = std::env::temp_dir();
        p.push("ilo_wiremock_server_error.json");
        std::fs::write(&p, config_json.to_string()).unwrap();

        let prog = r#"tool double"doubles a number" x:n>R n n
main x:n>R n n;double x"#;
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_ilo"))
            .args([
                prog,
                "--tools",
                p.to_str().unwrap(),
                "--run-interp",
                "main",
                "2",
            ])
            .output()
            .expect("ilo failed to start");

        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("thread 'main' panicked"),
            "unexpected panic: {stderr}"
        );

        std::fs::remove_file(&p).ok();
    }
}
