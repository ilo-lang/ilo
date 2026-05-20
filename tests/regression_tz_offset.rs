// Cross-engine regression tests for `tz-offset tz:t epoch:n > R n t`.
//
// Verifies:
//   - UTC offset in seconds for known timezones at specific epochs
//   - DST transitions: London BST/GMT, New York EDT/EST
//   - No-DST zone: Tokyo JST (always UTC+9)
//   - Unknown timezone name returns Err (exit 1, error on stderr)
//   - Auto-unwrap (`!`) works across all engines
//
// All checks run against VM and Cranelift JIT (when the feature is active)
// because tz-offset is tree-bridge eligible — the bridge round-trip is the
// most likely source of future regressions.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES: &[&str] = &["--vm"];

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
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    )
}

fn check_offset(engine: &str, tz: &str, epoch: i64, expected: i64) {
    let src = r#"f tz:t n:n>R n t;tz-offset tz n"#;
    let epoch_str = epoch.to_string();
    let (ok, stdout, stderr) = run(engine, src, &["f", tz, &epoch_str]);
    assert!(ok, "{engine}: tz-offset {tz:?} {epoch} failed: {stderr}");
    let got: i64 = stdout
        .parse()
        .unwrap_or_else(|_| panic!("{engine}: expected number, got {stdout:?}"));
    assert_eq!(
        got, expected,
        "{engine}: tz-offset {tz:?} at {epoch}: expected {expected}s, got {got}s"
    );
}

// --- London: BST/GMT DST transitions ---

#[test]
fn london_winter_is_utc_cross_engine() {
    // 2024-01-01 12:00:00 UTC — London is GMT (UTC+0)
    for engine in ENGINES {
        check_offset(engine, "Europe/London", 1_704_110_400, 0);
    }
}

#[test]
fn london_summer_is_bst_cross_engine() {
    // 2024-07-01 12:00:00 UTC — London is BST (UTC+1 = 3600s)
    for engine in ENGINES {
        check_offset(engine, "Europe/London", 1_719_835_200, 3600);
    }
}

// --- Tokyo: no DST, always UTC+9 ---

#[test]
fn tokyo_no_dst_cross_engine() {
    // Tokyo is permanently JST (UTC+9 = 32400s) regardless of epoch.
    // Test two distant epochs to confirm no DST flip.
    for engine in ENGINES {
        check_offset(engine, "Asia/Tokyo", 0, 32400);
        check_offset(engine, "Asia/Tokyo", 1_719_835_200, 32400);
    }
}

// --- New York: EST/EDT transitions ---

#[test]
fn nyc_winter_est_cross_engine() {
    // 2024-01-15 12:00:00 UTC — New York is EST (UTC-5 = -18000s)
    for engine in ENGINES {
        check_offset(engine, "America/New_York", 1_705_320_000, -18000);
    }
}

#[test]
fn nyc_summer_edt_cross_engine() {
    // 2024-07-15 12:00:00 UTC — New York is EDT (UTC-4 = -14400s)
    for engine in ENGINES {
        check_offset(engine, "America/New_York", 1_721_044_800, -14400);
    }
}

// --- Invalid timezone name returns Err ---

#[test]
fn invalid_tz_returns_err_cross_engine() {
    let src = r#"f tz:t n:n>R n t;tz-offset tz n"#;
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, &["f", "Not/A/Timezone", "0"]);
        assert!(
            !ok,
            "{engine}: expected exit 1 for bad tz name, got success. stdout={stdout:?}"
        );
        assert!(
            stderr.contains("tz-offset"),
            "{engine}: expected 'tz-offset' in error message, got: {stderr:?}"
        );
        assert!(
            stderr.contains("Not/A/Timezone"),
            "{engine}: expected tz name in error message, got: {stderr:?}"
        );
    }
}

// --- Auto-unwrap (`!`) works correctly ---

#[test]
fn tz_offset_bang_unwrap_cross_engine() {
    // `tz-offset!` auto-unwraps the R n t; the outer fn must also return R.
    let src = r#"f tz:t n:n>R n t;v=tz-offset! tz n;~v"#;
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, &["f", "Asia/Tokyo", "0"]);
        assert!(ok, "{engine}: tz-offset! failed: {stderr}");
        assert_eq!(
            stdout, "32400",
            "{engine}: tz-offset! expected 32400, got {stdout:?}"
        );
    }
}

// --- UTC is always zero offset ---

#[test]
fn utc_zero_offset_cross_engine() {
    for engine in ENGINES {
        check_offset(engine, "UTC", 0, 0);
        check_offset(engine, "UTC", 1_719_835_200, 0);
    }
}
