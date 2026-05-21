/// Live HTTP tests using wiremock — exercises `get` and `post` builtins end-to-end.
/// These start a real local server so the runtime path (minreq) is exercised.
use std::process::Command;

use wiremock::matchers::{body_string, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

// ── get ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_ok_returns_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hello"))
        .respond_with(ResponseTemplate::new(200).set_body_string("world"))
        .mount(&server)
        .await;

    let url = format!("{}/hello", server.uri());
    let out = ilo()
        .args([r#"f url:t>R t t;get url"#, &url])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("world"),
        "expected 'world' in output, got: {stdout}"
    );
}

#[tokio::test]
async fn get_server_error_returns_err_value() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fail"))
        .respond_with(ResponseTemplate::new(500).set_body_string("oops"))
        .mount(&server)
        .await;

    let url = format!("{}/fail", server.uri());
    // get returns R t t — a 500 body is still Ok(body); status codes don't become Err
    let out = ilo()
        .args([r#"f url:t>R t t;get url"#, &url])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("oops"),
        "expected 'oops' in output, got: {stdout}"
    );
}

#[tokio::test]
async fn get_bad_host_returns_err() {
    // Unreachable host → runtime Err *value* (not a crash). The entry function
    // returns Value::Err so the process exits 1 with the err line on stderr
    // (see tests/regression_main_err_exit_code.rs for the cross-engine
    // contract). What matters for this test: the binary completes with a
    // structured err, never a signal/panic.
    let out = ilo()
        .args([r#"f url:t>R t t;get url"#, "http://127.0.0.1:1"])
        .output()
        .expect("failed to run ilo");
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1 from Value::Err return, got {:?}",
        out.status.code(),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Should print Err(...) on stderr — the exact message varies by OS
    assert!(
        stderr.contains("Err") || stderr.contains("err") || stderr.contains('^'),
        "expected Err in stderr, got: {stderr}"
    );
}

// ── post ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn post_ok_returns_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/echo"))
        .respond_with(ResponseTemplate::new(200).set_body_string("echoed"))
        .mount(&server)
        .await;

    let url = format!("{}/echo", server.uri());
    let out = ilo()
        .args([r#"f url:t body:t>R t t;pst url body"#, &url, "hello"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("echoed"),
        "expected 'echoed' in output, got: {stdout}"
    );
}

#[tokio::test]
async fn post_sends_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/submit"))
        .and(body_string("payload"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let url = format!("{}/submit", server.uri());
    let out = ilo()
        .args([r#"f url:t body:t>R t t;pst url body"#, &url, "payload"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ok"),
        "expected 'ok' in output, got: {stdout}"
    );
}

#[tokio::test]
async fn post_ok_and_match_result() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/data"))
        .respond_with(ResponseTemplate::new(200).set_body_string("result"))
        .mount(&server)
        .await;

    let url = format!("{}/data", server.uri());
    // Match on the R t t result to extract the body
    let out = ilo()
        .args([
            r#"f url:t body:t>t;r=pst url body;?r{~v:v;^_:"err"}"#,
            &url,
            "input",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "result");
}

#[tokio::test]
async fn post_bad_host_returns_err() {
    // See `get_bad_host_returns_err` above — same contract: structured err
    // from the entry function -> exit 1 with err on stderr, never a crash.
    let out = ilo()
        .args([
            r#"f url:t body:t>R t t;pst url body"#,
            "http://127.0.0.1:1",
            "body",
        ])
        .output()
        .expect("failed to run ilo");
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1 from Value::Err return, got {:?}",
        out.status.code(),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Err") || stderr.contains("err") || stderr.contains('^'),
        "expected Err in stderr, got: {stderr}"
    );
}

// ── headers ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_with_header_sent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/auth"))
        .and(header("x-api-key", "secret"))
        .respond_with(ResponseTemplate::new(200).set_body_string("authorized"))
        .mount(&server)
        .await;

    let url = format!("{}/auth", server.uri());
    // Pass headers as an ilo map literal via mmap + mset
    let code = r#"f url:t>R t t;h=mmap;h=mset h "x-api-key" "secret";get url h"#;
    let out = ilo()
        .args([code, &url])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("authorized"),
        "expected 'authorized' in output, got: {stdout}"
    );
}

#[tokio::test]
async fn post_with_header_sent() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/submit"))
        .and(header("x-api-key", "tok"))
        .and(body_string("payload"))
        .respond_with(ResponseTemplate::new(200).set_body_string("accepted"))
        .mount(&server)
        .await;

    let url = format!("{}/submit", server.uri());
    let code = r#"f url:t>R t t;h=mmap;h=mset h "x-api-key" "tok";pst url "payload" h"#;
    let out = ilo()
        .args([code, &url])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("accepted"),
        "expected 'accepted' in output, got: {stdout}"
    );
}

// ── get-to / pst-to ──────────────────────────────────────────────────────────

#[tokio::test]
async fn get_to_ok_returns_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hello"))
        .respond_with(ResponseTemplate::new(200).set_body_string("world"))
        .mount(&server)
        .await;

    let url = format!("{}/hello", server.uri());
    let out = ilo()
        .args([r#"f url:t>R t t;get-to url 5000"#, &url])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("world"),
        "expected 'world' in output, got: {stdout}"
    );
}

#[tokio::test]
async fn get_to_bad_host_returns_err() {
    // Connection refused -> immediate Err, no actual timeout wait needed.
    let out = ilo()
        .args([
            r#"f url:t>t;r=get-to url 5000;?r{~_:"ok";^_:"err"}"#,
            "http://127.0.0.1:1",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "err", "expected 'err', got: {stdout}");
}

#[tokio::test]
async fn pst_to_ok_returns_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/echo"))
        .respond_with(ResponseTemplate::new(200).set_body_string("echoed"))
        .mount(&server)
        .await;

    let url = format!("{}/echo", server.uri());
    let out = ilo()
        .args([
            r#"f url:t body:t>R t t;pst-to url body 5000"#,
            &url,
            "hello",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("echoed"),
        "expected 'echoed' in output, got: {stdout}"
    );
}

#[tokio::test]
async fn pst_to_bad_host_returns_err() {
    let out = ilo()
        .args([
            r#"f url:t body:t>t;r=pst-to url body 5000;?r{~_:"ok";^_:"err"}"#,
            "http://127.0.0.1:1",
            "body",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "err", "expected 'err', got: {stdout}");
}

#[tokio::test]
async fn get_to_verifier_rejects_wrong_arg_types() {
    // Passing a number as the URL should produce a verifier error, not panic.
    let out = ilo()
        .args([r#"f>R t t;get-to 42 1000"#])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected failure for wrong arg type");
}

#[tokio::test]
async fn pst_to_verifier_rejects_wrong_timeout_type() {
    let out = ilo()
        .args([r#"f>R t t;pst-to "http://x" "body" "not-a-number""#])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for wrong timeout type"
    );
}

// ── getx / pstx — rich-shape variants ──────────────────────────────────────
//
// `getx` and `pstx` return `R (M t _) t` with keys `status` (n), `headers`
// (M t t), `body` (t). Additive over `get`/`pst`. Tree-bridge dispatched,
// so VM and Cranelift inherit through the bridge — every test runs on both
// engines where Cranelift is feature-enabled.

#[cfg(feature = "cranelift")]
const ENGINES: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES: &[&str] = &["--vm"];

#[tokio::test]
async fn getx_returns_status_headers_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hello"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-trace", "abc123")
                .set_body_string("world"),
        )
        .mount(&server)
        .await;

    let url = format!("{}/hello", server.uri());
    // Extract status, body, and a known custom header. All three must show up.
    let code = r#"f url:t>t;r=getx url;?r{~m:fmt "{}|{}|{}" (str (mget!! m "status")) (mget!! m "body") (mget!! (mget!! m "headers") "x-trace");^_:"err"}"#;

    for engine in ENGINES {
        let out = ilo()
            .args([code, engine, "f", &url])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "engine {engine}: stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("200|world|abc123"),
            "engine {engine}: expected '200|world|abc123', got: {stdout}"
        );
    }
}

#[tokio::test]
async fn getx_304_returned_as_status_not_err() {
    // The whole point of getx: a non-2xx status must surface as a status
    // code on the Ok-map, NOT collapse into Err. 304 with an empty body is
    // the canonical conditional-request shape.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/cached"))
        .respond_with(ResponseTemplate::new(304))
        .mount(&server)
        .await;

    let url = format!("{}/cached", server.uri());
    let code = r#"f url:t>t;r=getx url;?r{~m:str (mget!! m "status");^_:"err"}"#;
    for engine in ENGINES {
        let out = ilo()
            .args([code, engine, "f", &url])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "engine {engine}: stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            stdout.trim(),
            "304",
            "engine {engine}: expected '304', got: {stdout}"
        );
    }
}

#[tokio::test]
async fn getx_with_request_headers_sent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/auth"))
        .and(header("x-api-key", "secret"))
        .respond_with(ResponseTemplate::new(200).set_body_string("authorized"))
        .mount(&server)
        .await;

    let url = format!("{}/auth", server.uri());
    let code = r#"f url:t>t;h=mset mmap "x-api-key" "secret";r=getx url h;?r{~m:mget!! m "body";^_:"err"}"#;
    for engine in ENGINES {
        let out = ilo()
            .args([code, engine, "f", &url])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "engine {engine}: stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            stdout.trim(),
            "authorized",
            "engine {engine}: expected 'authorized', got: {stdout}"
        );
    }
}

#[tokio::test]
async fn pstx_returns_status_headers_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/echo"))
        .and(body_string("hello"))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("location", "/echo/42")
                .set_body_string("echoed"),
        )
        .mount(&server)
        .await;

    let url = format!("{}/echo", server.uri());
    let code = r#"f url:t body:t>t;r=pstx url body;?r{~m:fmt "{}|{}|{}" (str (mget!! m "status")) (mget!! m "body") (mget!! (mget!! m "headers") "location");^_:"err"}"#;
    for engine in ENGINES {
        let out = ilo()
            .args([code, engine, "f", &url, "hello"])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "engine {engine}: stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("201|echoed|/echo/42"),
            "engine {engine}: expected '201|echoed|/echo/42', got: {stdout}"
        );
    }
}

#[tokio::test]
async fn pstx_with_request_headers_sent() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/submit"))
        .and(header("x-api-key", "tok"))
        .and(body_string("payload"))
        .respond_with(ResponseTemplate::new(200).set_body_string("accepted"))
        .mount(&server)
        .await;

    let url = format!("{}/submit", server.uri());
    let code = r#"f url:t>t;h=mset mmap "x-api-key" "tok";r=pstx url "payload" h;?r{~m:mget!! m "body";^_:"err"}"#;
    for engine in ENGINES {
        let out = ilo()
            .args([code, engine, "f", &url])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "engine {engine}: stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            stdout.trim(),
            "accepted",
            "engine {engine}: expected 'accepted', got: {stdout}"
        );
    }
}

#[tokio::test]
async fn getx_bad_host_returns_err() {
    // Connection refused on loopback is stable across OS/CI.
    let code = r#"f url:t>t;r=getx url;?r{~_:"ok";^_:"err"}"#;
    for engine in ENGINES {
        let out = ilo()
            .args([code, engine, "f", "http://127.0.0.1:1"])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "engine {engine}: stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(stdout.trim(), "err", "engine {engine}: got: {stdout}");
    }
}

#[tokio::test]
async fn pstx_bad_host_returns_err() {
    let code = r#"f url:t>t;r=pstx url "body";?r{~_:"ok";^_:"err"}"#;
    for engine in ENGINES {
        let out = ilo()
            .args([code, engine, "f", "http://127.0.0.1:1"])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "engine {engine}: stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(stdout.trim(), "err", "engine {engine}: got: {stdout}");
    }
}

#[tokio::test]
async fn getx_verifier_rejects_wrong_arg_types() {
    let out = ilo()
        .args([r#"f>R t t;getx 42"#])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected failure for wrong arg type");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("getx") && stderr.contains("ILO-T"),
        "expected ILO-T error mentioning getx, got: {stderr}"
    );
}

#[tokio::test]
async fn pstx_verifier_rejects_wrong_headers_type() {
    // headers must be M t t — passing a list should error.
    let out = ilo()
        .args([r#"f>R t t;pstx "http://x" "body" [1,2,3]"#])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for wrong headers type"
    );
}
