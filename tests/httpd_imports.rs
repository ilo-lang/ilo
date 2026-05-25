//! Integration tests for `ilo httpd` resolving `use` imports in handler files
//! (ILO-481).
//!
//! Before ILO-481 the httpd command loaded and verified only the single
//! handler file and skipped import resolution, so a handler could not `use` a
//! sibling module. These tests spawn a real `ilo httpd` server against a temp
//! handler that imports a sibling module and assert the imported function is
//! reachable over HTTP, that a missing import surfaces a diagnostic, and that
//! plain single-file handlers still work.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Pick a free TCP port by binding to :0 and reading back the assigned port.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local_addr").port()
}

/// Spawn `ilo httpd --port <port> <handler>` and wait until it logs that it is
/// listening (or time out). Returns the child so the caller can kill it.
///
/// Readiness is detected by reading the child's stderr for the
/// `ilo httpd listening on` line, NOT by probing the port with a TCP connect.
/// A raw connect probe is itself an accepted connection that httpd dispatches
/// to a handler thread; under load that spurious startup request races the
/// real test request (ILO-505). Waiting on the log line avoids running the
/// handler during startup at all.
fn spawn_httpd(handler: &std::path::Path, port: u16) -> Child {
    let mut child = ilo()
        .args([
            "httpd",
            "--port",
            &port.to_string(),
            handler.to_str().unwrap(),
        ])
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn ilo httpd");

    // Read stderr until the server logs that it is listening, rather than
    // probing the port (which would trigger a spurious startup handler call).
    let stderr = child.stderr.take().expect("piped stderr");
    let mut reader = BufReader::new(stderr);
    for _ in 0..100 {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break; // EOF: server exited before logging readiness
        }
        if line.contains("listening on") {
            break;
        }
    }
    child
}

/// Send a minimal GET request and return the full raw response text.
fn http_get(port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to ilo httpd");
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).expect("write request");
    stream.flush().ok();
    let mut buf = String::new();
    stream.read_to_string(&mut buf).expect("read response");
    buf
}

/// A handler that `use`s a sibling module and calls one of its functions
/// serves correctly under `ilo httpd`.
#[test]
fn handler_uses_sibling_module() {
    let dir = tempfile::tempdir().expect("tempdir");

    // Sibling module exporting a greeting helper.
    std::fs::write(
        dir.path().join("store.ilo"),
        "greet name:t>t\n  +\"hello, \" name\n",
    )
    .expect("write module");

    // Handler imports the module and calls its function.
    std::fs::write(
        dir.path().join("handler.ilo"),
        "use \"store.ilo\"\ntype rsp{status:n;body:t}\nhandler req:_>rsp\n  rsp status:200 body:(greet \"world\")\n",
    )
    .expect("write handler");

    let port = free_port();
    let mut child = spawn_httpd(&dir.path().join("handler.ilo"), port);

    let resp = http_get(port, "/");
    child.kill().ok();
    let stderr = {
        let mut s = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut s);
        }
        s
    };
    child.wait().ok();

    assert!(
        resp.contains("hello, world"),
        "expected imported function output in response, got: {resp:?}\nserver stderr: {stderr}"
    );
}

/// A missing module surfaces a real import diagnostic (and the server never
/// starts), not a silent skip or generic parse error.
#[test]
fn missing_import_surfaces_diagnostic() {
    let dir = tempfile::tempdir().expect("tempdir");

    std::fs::write(
        dir.path().join("handler.ilo"),
        "use \"does-not-exist.ilo\"\ntype rsp{status:n;body:t}\nhandler req:_>rsp\n  rsp status:200 body:\"x\"\n",
    )
    .expect("write handler");

    let out = ilo()
        .args([
            "httpd",
            "--port",
            "0",
            dir.path().join("handler.ilo").to_str().unwrap(),
        ])
        .output()
        .expect("run ilo httpd");

    assert!(
        !out.status.success(),
        "expected non-zero exit for missing import, stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The loader emits an ILO-P017 import diagnostic referencing the bad path.
    assert!(
        stderr.contains("does-not-exist.ilo") || stderr.contains("P017"),
        "expected an import diagnostic mentioning the missing module, got: {stderr}"
    );
}

/// A single-file handler with no imports still works unchanged.
#[test]
fn single_file_handler_still_works() {
    let dir = tempfile::tempdir().expect("tempdir");

    std::fs::write(
        dir.path().join("handler.ilo"),
        "type rsp{status:n;body:t}\nhandler req:_>rsp\n  rsp status:200 body:\"standalone\"\n",
    )
    .expect("write handler");

    let port = free_port();
    let mut child = spawn_httpd(&dir.path().join("handler.ilo"), port);

    let resp = http_get(port, "/");
    child.kill().ok();
    child.wait().ok();

    assert!(
        resp.contains("standalone"),
        "expected single-file handler response, got: {resp:?}"
    );
}
