// Cross-engine smoke tests for the datetime builtins `dtfmt` and `dtparse`.
//
// `dtfmt epoch:n fmt:t > R t t` formats a unix epoch as text via strftime
// (UTC). It returns a Result because the epoch may be non-finite or out of
// range for chrono.
// `dtparse text:t fmt:t > R n t` parses text to an epoch, returning a Result
// because the input may fail to parse.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES: &[&str] = &["--run-tree", "--run-vm"];

fn run(engine: &str, src: &str, args: &[&str]) -> (bool, String, String) {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn check_text(src: &str, args: &[&str], expected: &str) {
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, args);
        assert!(ok, "{engine}: ilo failed for `{src}`: {stderr}");
        // Output may be `~<text>` when wrapped in Ok; we look for substring match.
        assert!(
            stdout.contains(expected),
            "{engine}: src=`{src}` args={args:?} expected `{expected}` in output, got `{stdout}`"
        );
    }
}

#[test]
fn dtfmt_epoch_zero() {
    // dtfmt now returns R t t; use `!` to auto-unwrap, then ~ to re-wrap so
    // the enclosing R-returning fn matches.
    check_text(
        "f e:n>R t t;v=dtfmt! e \"%Y-%m-%d\";~v",
        &["f", "0"],
        "1970-01-01",
    );
}

#[test]
fn dtfmt_jan_2025() {
    check_text(
        "f e:n>R t t;v=dtfmt! e \"%Y-%m-%d\";~v",
        &["f", "1735689600"],
        "2025-01-01",
    );
}

#[test]
fn dtparse_jan_2025_auto_unwrap() {
    // dtparse returns R n t; ! auto-unwraps on Ok and propagates Err.
    // Enclosing fn must return R for `!` to be legal; pin the Ok path with
    // `~v` so the output contains the parsed epoch.
    let src = r#"f s:t>R n t;v=dtparse! s "%Y-%m-%d";~v"#;
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, &["f", "2025-01-01"]);
        assert!(ok, "{engine}: ilo failed: {stderr}");
        assert!(
            stdout.contains("1735689600"),
            "{engine}: expected epoch 1735689600 in output, got `{stdout}`"
        );
    }
}

#[test]
fn dtparse_round_trip() {
    // Parse a date back to epoch, then format it again, must be lossless.
    // We wrap in Ok at the end since `!` requires R-returning enclosing fn.
    let src = r#"f s:t>R t t;e=dtparse! s "%Y-%m-%d";d=dtfmt! e "%Y-%m-%d";~d"#;
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, &["f", "2025-01-01"]);
        assert!(ok, "{engine}: ilo failed: {stderr}");
        assert!(
            stdout.contains("2025-01-01"),
            "{engine}: expected round-trip 2025-01-01, got `{stdout}`"
        );
    }
}

#[test]
fn dtparse_invalid_input_produces_err() {
    // Err path: `!` short-circuits and returns the Err to the caller. The
    // surfaced error message includes the `dtparse:` prefix from our impl.
    let src = r#"f s:t>R n t;v=dtparse! s "%Y-%m-%d";~v"#;
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, &["f", "not-a-date"]);
        assert!(ok, "{engine}: ilo failed: {stderr}");
        assert!(
            stdout.contains("dtparse"),
            "{engine}: expected dtparse error message, got `{stdout}`"
        );
    }
}
