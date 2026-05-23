//! Integration tests for `ilo httpd` resolving `use` imports in handler files
//! (ILO-481).
//!
//! Before ILO-481 the httpd command loaded and verified only the single
//! handler file and skipped import resolution, so a handler could not `use` a
//! sibling module. These tests spawn a real `ilo httpd` server against a temp
//! handler that imports a sibling module and assert the imported function is
//! reachable over HTTP, that a missing import surfaces a diagnostic, and that
//! plain single-file handlers still work.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Pick a free TCP port by binding to :0 and reading back the assigned port.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local_addr").port()
}

/// Spawn `ilo httpd --port <port> <handler>` and wait until the port accepts
/// connections (or time out). Returns the child so the caller can kill it.
fn spawn_httpd(handler: &std::path::Path, port: u16) -> Child {
    let child = ilo()
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

    // Poll the port until it's listening.
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return child;
        }
        std::thread::sleep(Duration::from_millis(50));
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

    assert!(
        resp.contains("standalone"),
        "expected single-file handler response, got: {resp:?}"
    );
}
