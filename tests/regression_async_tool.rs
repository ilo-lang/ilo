/// Regression tests for `get-many` — concurrent HTTP GET fan-out.
///
/// `get-many urls:L t > L (R t t)` issues GET requests in parallel
/// (chunked at `GET_MANY_MAX_CONCURRENCY`) and returns one Result per URL,
/// in the same order as the input list.
use std::process::Command;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[tokio::test]
async fn get_many_all_ok() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/a"))
        .respond_with(ResponseTemplate::new(200).set_body_string("alpha"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/b"))
        .respond_with(ResponseTemplate::new(200).set_body_string("beta"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/c"))
        .respond_with(ResponseTemplate::new(200).set_body_string("gamma"))
        .mount(&server)
        .await;

    let base = server.uri();
    let urls = format!("{base}/a,{base}/b,{base}/c");
    // Map each Result body into a flat list of strings; join with "|" so order is testable.
    let out = ilo()
        .args([
            r#"f us:L t>t;rs=get-many us;cat (map pick rs) "|"
pick r:R t t>t;?r{~v:v;^e:e}"#,
            "f",
            &urls,
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "alpha|beta|gamma");
}

#[tokio::test]
async fn get_many_mixed_ok_and_err() {
    // wiremock returns 404 for unmounted paths — but a 404 body is still Ok(body)
    // from minreq's perspective (no transport failure). To force an Err we point
    // one URL at an unreachable host.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ok"))
        .respond_with(ResponseTemplate::new(200).set_body_string("good"))
        .mount(&server)
        .await;

    let base = server.uri();
    let urls = format!("{base}/ok,http://127.0.0.1:1/never");
    // pick returns "OK:body" for Ok, "ERR" for Err, so we can verify order + tag.
    let out = ilo()
        .args([
            r#"f us:L t>t;rs=get-many us;cat (map pick rs) "|"
pick r:R t t>t;?r{~v:cat ["OK", v] ":";^_:"ERR"}"#,
            "f",
            &urls,
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let trimmed = stdout.trim();
    assert!(
        trimmed.starts_with("OK:good|") && trimmed.ends_with("ERR"),
        "expected 'OK:good|ERR', got: {trimmed}"
    );
}

#[test]
fn get_many_empty_list() {
    // Empty list → empty list of results. Use len to verify.
    let out = ilo()
        .args(["f>n;len (get-many [])", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
}
