//! Integration tests for `ilo httpd`'s lazy streaming response body (ILO-482).
//!
//! Before ILO-482 `handle_http_connection` materialised the entire response
//! body before writing the first byte, so a handler could not hold a
//! connection open and emit chunks as they were produced (true SSE /
//! long-poll). These tests prove a handler returning a lazy line iterator
//! (`get-stream`, here proxying a test-controlled slow upstream) streams each
//! chunk incrementally, that the eager `L t` body still works unchanged, and
//! that a client disconnecting mid-stream does not panic the server.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Pick a free TCP port by binding to :0 and reading back the assigned port.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local_addr").port()
}

/// Spawn `ilo httpd --port <port> <handler>` and wait until it logs that it is
/// listening (or time out). Returns the child so the caller can kill+wait it.
///
/// Readiness is detected by reading the child's stderr for the
/// `ilo httpd listening on` line, NOT by probing the port with a TCP connect.
/// A raw connect probe is itself an accepted connection: httpd spawns a handler
/// thread for it, and for a handler that proxies an upstream via `get-stream`
/// that thread consumes the upstream's single `accept()` before the real test
/// request ever arrives, so the test sees an empty body. Waiting on the log
/// line avoids triggering the handler during startup.
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

    // Read stderr until the server logs that it is listening. The pipe read
    // blocks, so a server that never starts is bounded by the test runner's
    // own timeout; we cap the line count as a belt-and-braces fallback.
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

/// A tiny test-controlled upstream that responds to one GET with a chunked
/// body, emitting `n` lines spaced `gap` apart. Each emitted line is also sent
/// on `tx` so the test can observe production timing. Runs on its own thread;
/// returns the bound port.
///
/// Each event line is padded past 16 KiB on purpose. The handler proxies this
/// upstream with `get-stream`, whose underlying `minreq::ResponseLazy::read`
/// fills the wrapping `BufReader`'s 16 KiB backing buffer before returning a
/// line. With sub-buffer payloads that read drains the whole (small) response
/// in one go, masking the lazy path; padding past the buffer forces one line
/// per `read`, so the proxy genuinely yields each event as it arrives and the
/// test measures the streaming plumbing rather than minreq's buffer size.
/// (The buffer-granularity quirk in `get-stream` is tracked as a follow-up;
/// `tail-file` is the source crew's `/events/stream` ultimately needs.)
const PAD: usize = 20_000;

fn spawn_slow_upstream(n: usize, gap: Duration, tx: mpsc::Sender<usize>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind upstream");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        // Serve exactly one connection then return.
        if let Ok((mut sock, _)) = listener.accept() {
            // Drain the request headers.
            let mut reader = BufReader::new(sock.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if line == "\r\n" || line == "\n" {
                    break;
                }
            }
            let header = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
            if sock.write_all(header.as_bytes()).is_err() {
                return;
            }
            let _ = sock.flush();
            for i in 0..n {
                let data = format!("event-{i}-{}\n", "x".repeat(PAD));
                let chunk = format!("{:x}\r\n{}\r\n", data.len(), data);
                if sock.write_all(chunk.as_bytes()).is_err() {
                    return;
                }
                let _ = sock.flush();
                let _ = tx.send(i);
                std::thread::sleep(gap);
            }
            let _ = sock.write_all(b"0\r\n\r\n");
            let _ = sock.flush();
        }
    });
    port
}

/// A handler returning a lazy body (`get-stream` over a slow upstream) streams
/// each chunk to the client as the upstream produces it — the client reads an
/// early chunk before the upstream has emitted the final one, proving no full
/// buffering.
#[test]
fn lazy_body_streams_incrementally() {
    let dir = tempfile::tempdir().expect("tempdir");

    // Slow upstream: 4 events, 200ms apart.
    let (tx, upstream_rx) = mpsc::channel();
    let upstream_port = spawn_slow_upstream(4, Duration::from_millis(200), tx);

    // Handler proxies the upstream stream as its lazy response body.
    let handler_src = format!(
        "type rsp{{status:n;body:_}}\nhandler req:_>rsp\n  rsp status:200 body:(get-stream \"http://127.0.0.1:{upstream_port}/\")\n"
    );
    std::fs::write(dir.path().join("handler.ilo"), handler_src).expect("write handler");

    let port = free_port();
    let mut child = spawn_httpd(&dir.path().join("handler.ilo"), port);

    // Open the connection and read incrementally.
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to httpd");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write request");
    stream.flush().ok();

    let mut reader = BufReader::new(stream);

    // Read the response status + headers.
    let mut saw_chunked = false;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read header line");
        if line.to_lowercase().contains("transfer-encoding: chunked") {
            saw_chunked = true;
        }
        if line == "\r\n" {
            break;
        }
    }
    assert!(
        saw_chunked,
        "expected chunked transfer-encoding for lazy body"
    );

    // Read the first event chunk. We must receive it well before the upstream
    // has emitted all 4 events (which takes ~800ms total).
    let first_seen = Instant::now();
    let mut got_first = String::new();
    // chunk size line
    let mut size_line = String::new();
    reader.read_line(&mut size_line).expect("read chunk size");
    let want = usize::from_str_radix(size_line.trim(), 16).expect("hex chunk size");
    let mut buf = vec![0u8; want];
    reader.read_exact(&mut buf).expect("read chunk body");
    got_first.push_str(&String::from_utf8_lossy(&buf));
    let mut crlf = [0u8; 2];
    reader.read_exact(&mut crlf).ok();
    let first_elapsed = first_seen.elapsed();

    assert!(
        got_first.contains("event-0"),
        "expected first chunk to be event-0, got: {got_first:?}"
    );
    // The upstream has produced at most 1-2 events by now, definitely not all 4.
    assert!(
        first_elapsed < Duration::from_millis(700),
        "first chunk arrived too late ({first_elapsed:?}); body was likely buffered"
    );

    // Now drain the remaining chunks and collect every event prefix.
    let event_prefix = |s: &str| s.trim().split('-').take(2).collect::<Vec<_>>().join("-");
    let mut events = vec![event_prefix(&got_first)];
    loop {
        let mut size_line = String::new();
        if reader.read_line(&mut size_line).unwrap_or(0) == 0 {
            break;
        }
        let trimmed = size_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let want = match usize::from_str_radix(trimmed, 16) {
            Ok(n) => n,
            Err(_) => break,
        };
        if want == 0 {
            break; // terminating chunk
        }
        let mut buf = vec![0u8; want];
        if reader.read_exact(&mut buf).is_err() {
            break;
        }
        events.push(event_prefix(&String::from_utf8_lossy(&buf)));
        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf).ok();
    }

    child.kill().ok();
    child.wait().ok();

    // Drain the upstream observer channel so the thread can finish.
    while upstream_rx.try_recv().is_ok() {}

    assert_eq!(
        events,
        vec!["event-0", "event-1", "event-2", "event-3"],
        "expected all four events in order, got: {events:?}"
    );
}

/// The eager `L t` body (each list element a chunk) still works unchanged.
#[test]
fn eager_list_body_still_works() {
    let dir = tempfile::tempdir().expect("tempdir");

    std::fs::write(
        dir.path().join("handler.ilo"),
        "type rsp{status:n;body:_}\nhandler req:_>rsp\n  rsp status:200 body:[\"a\" \"b\" \"c\"]\n",
    )
    .expect("write handler");

    let port = free_port();
    let mut child = spawn_httpd(&dir.path().join("handler.ilo"), port);

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write request");
    stream.flush().ok();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).expect("read response");

    child.kill().ok();
    child.wait().ok();

    assert!(
        resp.to_lowercase().contains("transfer-encoding: chunked"),
        "expected chunked encoding for list body, got: {resp:?}"
    );
    for want in ["a", "b", "c"] {
        assert!(resp.contains(want), "expected chunk {want:?} in {resp:?}");
    }
}

/// A client disconnecting mid-stream must not panic the server: the connection
/// thread exits cleanly and the server keeps serving subsequent requests.
#[test]
fn client_disconnect_midstream_is_clean() {
    let dir = tempfile::tempdir().expect("tempdir");

    // Slow upstream the handler proxies. Generous count so the stream is still
    // open when we hang up.
    let (tx, upstream_rx) = mpsc::channel();
    let upstream_port = spawn_slow_upstream(50, Duration::from_millis(50), tx);

    let handler_src = format!(
        "type rsp{{status:n;body:_}}\nhandler req:_>rsp\n  rsp status:200 body:(get-stream \"http://127.0.0.1:{upstream_port}/\")\n"
    );
    std::fs::write(dir.path().join("handler.ilo"), handler_src).expect("write handler");

    let port = free_port();
    let mut child = spawn_httpd(&dir.path().join("handler.ilo"), port);

    // Connect, read just the first chunk, then drop the socket mid-stream.
    {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .expect("write request");
        stream.flush().ok();
        let mut buf = [0u8; 64];
        // Read something (headers + first chunk) then drop the stream below.
        let _ = stream.read(&mut buf);
        // stream dropped here -> client disconnect mid-stream
    }

    // Give the server a moment to notice the broken pipe on its next write.
    std::thread::sleep(Duration::from_millis(200));

    // The server must still be alive and able to serve. A handler-error or
    // panic would have torn down the process or the accept loop.
    let still_up = TcpStream::connect(("127.0.0.1", port)).is_ok();

    child.kill().ok();
    child.wait().ok();
    while upstream_rx.try_recv().is_ok() {}

    assert!(
        still_up,
        "server should keep accepting connections after a client disconnect"
    );

    // The process should not have crashed; killing a healthy child yields a
    // signal-terminated status (not a panic exit we triggered ourselves).
    // Nothing more to assert — reaching here without a hang is the signal.
}
