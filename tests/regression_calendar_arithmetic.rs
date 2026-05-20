// Cross-engine regression tests for the calendar arithmetic builtins:
//
//   add-mo   dt:n n:n > n  — add N calendar months, end-of-month snap
//   last-dom dt:n > n      — epoch of last day of month at 00:00 UTC
//   next-business-day dt:n > n — next weekday (skip Sat/Sun)
//   day-of-week dt:n > n   — 0=Sun 1=Mon 2=Tue 3=Wed 4=Thu 5=Fri 6=Sat

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
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn check_num(src: &str, args: &[&str], expected: f64) {
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, args);
        assert!(
            ok,
            "{engine}: ilo failed for `{src}` args={args:?}: {stderr}"
        );
        let got: f64 = stdout
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("{engine}: expected numeric output, got `{stdout}`"));
        assert_eq!(
            got, expected,
            "{engine}: src=`{src}` args={args:?} expected {expected}, got {got}"
        );
    }
}

// Epoch anchors (all 00:00 UTC):
//   2024-01-31 = 1706659200
//   2024-02-29 = 1709164800  (leap year)
//   2024-02-28 = 1709078400
//   2024-03-31 = 1711843200
//   2025-02-28 = 1740700800  (non-leap)
//   2024-02-01 = 1706745600  (first of Feb 2024)
//   2025-03-01 = 1740787200
//   2024-01-15 = 1705276800  (Monday)
//   2024-01-19 = 1705622400  (Friday)
//   2024-01-20 = 1705708800  (Saturday)
//   2024-01-21 = 1705795200  (Sunday)
//   2024-01-22 = 1705881600  (Monday)
//   2024-01-08 = 1704672000  (Monday)
//   2024-02-05 = 1707091200  (Monday)

// ── add-mo ────────────────────────────────────────────────────────────────────

#[test]
fn add_mo_jan31_plus1_leap() {
    // Jan 31 2024 + 1 month = Feb 29 2024 (2024 is a leap year, so snap to 29).
    // 2024-01-31 00:00 UTC = 1706659200
    // 2024-02-29 00:00 UTC = 1709164800
    check_num(
        "f dt:n n:n>n;add-mo dt n",
        &["f", "1706659200", "1"],
        1709164800.0,
    );
}

#[test]
fn add_mo_jan31_plus1_nonleap() {
    // Jan 31 2025 + 1 month = Feb 28 2025 (2025 is not a leap year, snap to 28).
    // 2025-01-31 00:00 UTC = 1738281600
    // 2025-02-28 00:00 UTC = 1740700800
    check_num(
        "f dt:n n:n>n;add-mo dt n",
        &["f", "1738281600", "1"],
        1740700800.0,
    );
}

#[test]
fn add_mo_mar31_minus1() {
    // Mar 31 2024 - 1 month = Feb 29 2024 (leap year, snap to 29).
    // 2024-03-31 00:00 UTC = 1711843200
    // 2024-02-29 00:00 UTC = 1709164800
    check_num(
        "f dt:n n:n>n;add-mo dt n",
        &["f", "1711843200", "-1"],
        1709164800.0,
    );
}

#[test]
fn add_mo_mid_month_no_snap() {
    // Feb 15 2024 + 1 month = Mar 15 2024 (no snap needed — 15 exists in Mar).
    // 2024-02-15 00:00 UTC = 1707955200
    // 2024-03-15 00:00 UTC = 1710460800
    check_num(
        "f dt:n n:n>n;add-mo dt n",
        &["f", "1707955200", "1"],
        1710460800.0,
    );
}

#[test]
fn add_mo_zero() {
    // +0 months = same epoch (midnight normalised).
    // 2024-01-15 00:00 UTC = 1705276800
    check_num(
        "f dt:n n:n>n;add-mo dt n",
        &["f", "1705276800", "0"],
        1705276800.0,
    );
}

#[test]
fn add_mo_twelve() {
    // +12 months = same date next year.
    // 2024-01-15 -> 2025-01-15 00:00 UTC = 1736899200
    check_num(
        "f dt:n n:n>n;add-mo dt n",
        &["f", "1705276800", "12"],
        1736899200.0,
    );
}

// ── last-dom ──────────────────────────────────────────────────────────────────

#[test]
fn last_dom_jan_2024() {
    // Any Jan 2024 epoch -> Jan 31 2024 00:00 UTC = 1706659200
    check_num("f dt:n>n;last-dom dt", &["f", "1705276800"], 1706659200.0);
}

#[test]
fn last_dom_feb_2024_leap() {
    // Any Feb 2024 epoch -> Feb 29 2024 00:00 UTC = 1709164800
    check_num("f dt:n>n;last-dom dt", &["f", "1707955200"], 1709164800.0);
}

#[test]
fn last_dom_feb_2025_nonleap() {
    // Any Feb 2025 epoch -> Feb 28 2025 00:00 UTC = 1740700800
    // 2025-02-15 00:00 UTC = 1739577600
    check_num("f dt:n>n;last-dom dt", &["f", "1739577600"], 1740700800.0);
}

#[test]
fn last_dom_dec_2024() {
    // Dec 2024 -> Dec 31 2024 00:00 UTC = 1735603200
    // 2024-12-01 00:00 UTC = 1733011200
    check_num("f dt:n>n;last-dom dt", &["f", "1733011200"], 1735603200.0);
}

// ── next-business-day ─────────────────────────────────────────────────────────

#[test]
fn next_business_day_monday_to_tuesday() {
    // Mon Jan 15 2024 -> Tue Jan 16 2024 00:00 UTC = 1705363200
    check_num(
        "f dt:n>n;next-business-day dt",
        &["f", "1705276800"],
        1705363200.0,
    );
}

#[test]
fn next_business_day_friday_to_monday() {
    // Fri Jan 19 2024 -> Mon Jan 22 2024 00:00 UTC = 1705881600
    // 2024-01-19 00:00 UTC = 1705622400
    check_num(
        "f dt:n>n;next-business-day dt",
        &["f", "1705622400"],
        1705881600.0,
    );
}

#[test]
fn next_business_day_saturday_to_monday() {
    // Sat Jan 20 2024 -> Mon Jan 22 2024 00:00 UTC = 1705881600
    // 2024-01-20 00:00 UTC = 1705708800
    check_num(
        "f dt:n>n;next-business-day dt",
        &["f", "1705708800"],
        1705881600.0,
    );
}

#[test]
fn next_business_day_sunday_to_monday() {
    // Sun Jan 21 2024 -> Mon Jan 22 2024 00:00 UTC = 1705881600
    // 2024-01-21 00:00 UTC = 1705795200
    check_num(
        "f dt:n>n;next-business-day dt",
        &["f", "1705795200"],
        1705881600.0,
    );
}

// ── day-of-week ───────────────────────────────────────────────────────────────

#[test]
fn day_of_week_monday() {
    // 2024-01-15 is a Monday -> 1
    check_num("f dt:n>n;day-of-week dt", &["f", "1705276800"], 1.0);
}

#[test]
fn day_of_week_friday() {
    // 2024-01-19 is a Friday -> 5
    check_num("f dt:n>n;day-of-week dt", &["f", "1705622400"], 5.0);
}

#[test]
fn day_of_week_saturday() {
    // 2024-01-20 is a Saturday -> 6
    check_num("f dt:n>n;day-of-week dt", &["f", "1705708800"], 6.0);
}

#[test]
fn day_of_week_sunday() {
    // 2024-01-21 is a Sunday -> 0
    check_num("f dt:n>n;day-of-week dt", &["f", "1705795200"], 0.0);
}

#[test]
fn day_of_week_epoch_zero() {
    // 1970-01-01 is a Thursday -> 4
    check_num("f dt:n>n;day-of-week dt", &["f", "0"], 4.0);
}
