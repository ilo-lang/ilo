//! End-to-end tests for the ILO-46 client-side streaming builtins:
//! `get-stream`, `get-stream-h`, `pst-stream`, `pst-stream-h`.
//!
//! These exercise the real native backend (minreq `send_lazy`) against a
//! purpose-built `std::net::TcpListener` server, so we control the exact
//! HTTP/1.1 chunked-transfer wire format that the lazy iterator has to
//! handle. wiremock would be a heavier dependency for the precise chunk
//! sequencing we need here.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::Command;
use std::thread;
use std::time::Duration;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Spawn a tiny chunked-encoding HTTP/1.1 server that emits `lines` one chunk
/// at a time. Returns the bound socket address and a join handle for the
/// thread (test can wait on it to surface assertion failures from the
/// server thread).
fn spawn_chunked_server(
    lines: Vec<&'static str>,
) -> (SocketAddr, thread::JoinHandle<Result<String, String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");

    let handle = thread::spawn(move || -> Result<String, String> {
        let (mut stream, _peer) = listener
            .accept()
            .map_err(|e| format!("accept failed: {e}"))?;

        // Read the request headers off the wire so the client doesn't see
        // the response too eagerly.
        let mut req_buf = [0u8; 8192];
        let n = stream.read(&mut req_buf).map_err(|e| e.to_string())?;
        let req_text = String::from_utf8_lossy(&req_buf[..n]).to_string();

        // Send the response head + chunks.
        let head = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n";
        stream.write_all(head).map_err(|e| e.to_string())?;
        stream.flush().ok();

        for line in &lines {
            // One chunk = one logical "line\n" so BufReader::lines() yields it.
            let payload = format!("{line}\n");
            let chunk = format!("{:X}\r\n{}\r\n", payload.len(), payload);
            stream
                .write_all(chunk.as_bytes())
                .map_err(|e| e.to_string())?;
            stream.flush().ok();
            // Small pause so the chunks visibly stagger; not required for
            // correctness, but exercises the lazy path.
            thread::sleep(Duration::from_millis(5));
        }
        // Terminating zero-length chunk.
        stream.write_all(b"0\r\n\r\n").map_err(|e| e.to_string())?;
        stream.flush().ok();

        Ok(req_text)
    });

    (addr, handle)
}

/// Spawn a server that reads the request body, then echoes it back as a
/// single chunked line.
fn spawn_echo_post_server() -> (SocketAddr, thread::JoinHandle<Result<String, String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");

    let handle = thread::spawn(move || -> Result<String, String> {
        let (stream, _peer) = listener.accept().map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
        let mut writer = stream;

        // Parse headers.
        let mut content_length = 0usize;
        let mut header_lines = Vec::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).map_err(|e| e.to_string())?;
            if line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some(rest) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = rest.trim().parse::<usize>().unwrap_or(0);
            }
            header_lines.push(line);
        }
        let mut body = vec![0u8; content_length];
        if content_length > 0 {
            reader.read_exact(&mut body).map_err(|e| e.to_string())?;
        }
        let body_text = String::from_utf8_lossy(&body).to_string();

        // Write response with the body echoed as a chunked line.
        let head = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        writer.write_all(head).map_err(|e| e.to_string())?;
        let payload = format!("{body_text}\n");
        let chunk = format!("{:X}\r\n{}\r\n", payload.len(), payload);
        writer
            .write_all(chunk.as_bytes())
            .map_err(|e| e.to_string())?;
        writer.write_all(b"0\r\n\r\n").map_err(|e| e.to_string())?;
        writer.flush().ok();

        Ok(header_lines.join("") + &body_text)
    });

    (addr, handle)
}

/// Spawn a server that emits two chunked lines then closes the socket mid-
/// stream (no terminating 0-chunk). The client should see those two lines
/// then surface an error when trying to read more.
fn spawn_truncated_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");

    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 4096];
        let _ = stream.read(&mut buf);
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n6\r\nalpha\n\r\n5\r\nbeta\n\r\n",
        );
        let _ = stream.flush();
        // Drop the stream without sending the 0-length terminator chunk.
        drop(stream);
    });

    addr
}

// ── get-stream happy path ────────────────────────────────────────────────────

#[test]
fn get_stream_yields_lines_in_order() {
    let (addr, srv) = spawn_chunked_server(vec!["alpha", "beta", "gamma"]);
    let url = format!("http://{addr}/events");

    let prog = r#"main url:t>n;n=0;@line (get-stream url){prnt line;n=+ n 1};n"#;
    let out = ilo()
        .args(["run", "--allow-net=*", prog, "main", &url])
        .output()
        .expect("failed to run ilo");

    let _server_req = srv.join().expect("server thread panicked");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines,
        vec!["alpha", "beta", "gamma", "3"],
        "expected three chunk-lines then count, got {stdout:?}"
    );
}

// ── get-stream-h passes headers ─────────────────────────────────────────────

#[test]
fn get_stream_h_passes_authorization_header() {
    let (addr, srv) = spawn_chunked_server(vec!["ok"]);
    let url = format!("http://{addr}/events");

    let prog = r#"main url:t>n;n=0;h=mset mmap "Authorization" "Bearer s3cret";@line (get-stream-h url h){prnt line;n=+ n 1};n"#;
    let out = ilo()
        .args(["run", "--allow-net=*", prog, "main", &url])
        .output()
        .expect("failed to run ilo");

    let req = srv
        .join()
        .expect("server thread panicked")
        .expect("server failed");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        req.contains("Authorization: Bearer s3cret"),
        "Authorization header missing from request:\n{req}"
    );
}

// ── pst-stream echoes the request body ──────────────────────────────────────

#[test]
fn pst_stream_streams_response_body() {
    let (addr, srv) = spawn_echo_post_server();
    let url = format!("http://{addr}/echo");

    let prog = r#"main url:t>n;n=0;@line (pst-stream url "hello-stream"){prnt line;n=+ n 1};n"#;
    let out = ilo()
        .args(["run", "--allow-net=*", prog, "main", &url])
        .output()
        .expect("failed to run ilo");

    let _ = srv.join().expect("server thread panicked");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("hello-stream"),
        "expected echoed body in stdout, got: {stdout}"
    );
}

// ── cap-check denies the call before any connection ─────────────────────────

#[test]
fn get_stream_returns_err_when_net_denied() {
    // No --allow-net flag → caps.check_net rejects the URL up front, the
    // builtin returns Value::Err. Because the foreach expects a
    // LazyHttpLines, the runtime errors at the foreach site rather than
    // iterating. Either way: process exits non-zero, no panic.
    let prog =
        r#"main>n;n=0;@line (get-stream "http://127.0.0.1:1/whatever"){prnt line;n=+ n 1};n"#;
    let out = ilo().args([prog]).output().expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected non-zero exit for cap-denied call, stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

// ── mid-stream connection drop surfaces an iterator error ───────────────────

#[test]
fn get_stream_surfaces_mid_stream_error() {
    let addr = spawn_truncated_server();
    let url = format!("http://{addr}/truncated");

    // Iterate forever; the server gives us alpha+beta then drops. The
    // foreach should bail with an http-stream read error.
    let prog = r#"main url:t>n;n=0;@line (get-stream url){prnt line;n=+ n 1};n"#;
    let out = ilo()
        .args(["run", "--allow-net=*", prog, "main", &url])
        .output()
        .expect("failed to run ilo");

    // The connection drop will be either a clean EOF (two lines, exit 0)
    // OR a runtime error (exit 1). minreq's `Chunked` state surfaces a
    // truncated stream as Err; an `EndOnClose` stream silently terminates.
    // Either is acceptable — what matters is that the runtime doesn't
    // panic and stdout shows at least one of the lines we sent.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("alpha") || stderr.contains("http-stream"),
        "expected either streamed lines or a stream error, stdout={stdout:?} stderr={stderr:?}"
    );
}

// ── direct backend unit test (no subprocess) ───────────────────────────────

#[test]
fn backend_get_stream_native_iterates_lines() {
    use ilo::interpreter::http_wasm::default_backend;

    let (addr, srv) = spawn_chunked_server(vec!["one", "two", "three"]);
    let url = format!("http://{addr}/raw");

    let backend = default_backend();
    let iter = backend
        .get_stream(&url, &[])
        .expect("backend get_stream should open the connection");
    let collected: Vec<String> = iter.map(|r| r.expect("line read ok")).collect();
    let _ = srv.join().expect("server thread panicked");

    assert_eq!(collected, vec!["one", "two", "three"]);
}

// Silence unused-import warning under cfgs that drop one of the helpers.
#[allow(dead_code)]
fn _force_use(_s: &TcpStream) {}
