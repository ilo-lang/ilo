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

// Run and assert the program failed with a stderr matching `needle`. Used to
// exercise the error arms in the calendar builtins (out-of-range epochs,
// month overflow). These are otherwise dead lines from a coverage view.
fn check_err(src: &str, args: &[&str], needle: &str) {
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, args);
        assert!(
            !ok,
            "{engine}: expected failure for `{src}` args={args:?}, got stdout=`{stdout}`"
        );
        assert!(
            stderr.contains(needle),
            "{engine}: expected stderr to contain `{needle}`, got: {stderr}"
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

// ── day-of-week: remaining weekdays (Tue/Wed/Thu) ─────────────────────────────
// These cover the otherwise-uncovered Weekday::Tue/Wed/Thu match arms in
// `day-of-week`. The arms are otherwise dead from a coverage perspective.

#[test]
fn day_of_week_tuesday() {
    // 2024-01-16 is a Tuesday -> 2
    // 2024-01-16 00:00 UTC = 1705363200
    check_num("f dt:n>n;day-of-week dt", &["f", "1705363200"], 2.0);
}

#[test]
fn day_of_week_wednesday() {
    // 2024-01-17 is a Wednesday -> 3
    // 2024-01-17 00:00 UTC = 1705449600
    check_num("f dt:n>n;day-of-week dt", &["f", "1705449600"], 3.0);
}

#[test]
fn day_of_week_thursday() {
    // 2024-01-18 is a Thursday -> 4
    // 2024-01-18 00:00 UTC = 1705536000
    check_num("f dt:n>n;day-of-week dt", &["f", "1705536000"], 4.0);
}

// ── next-business-day: midweek (Tue/Wed/Thu fall through `_ => 1`) ────────────
// The single arm covers Mon/Tue/Wed/Thu but coverage gave us Mon only. Add the
// midweek days for both explicit weekday-table coverage and confidence that
// the catch-all advances by exactly one calendar day across the +1 branch.

#[test]
fn next_business_day_tuesday_to_wednesday() {
    // Tue Jan 16 2024 -> Wed Jan 17 2024
    // 2024-01-16 00:00 UTC = 1705363200
    // 2024-01-17 00:00 UTC = 1705449600
    check_num(
        "f dt:n>n;next-business-day dt",
        &["f", "1705363200"],
        1705449600.0,
    );
}

#[test]
fn next_business_day_thursday_to_friday() {
    // Thu Jan 18 2024 -> Fri Jan 19 2024
    // 2024-01-18 00:00 UTC = 1705536000
    // 2024-01-19 00:00 UTC = 1705622400
    check_num(
        "f dt:n>n;next-business-day dt",
        &["f", "1705536000"],
        1705622400.0,
    );
}

// ── add-mo: year-boundary crossings + m==12 branch in add_months_snap ─────────
// `add_months_snap` has a `if m == 12` branch (computing days-in-month by
// asking for the first of next year). Existing tests never landed on Dec, so
// the December arm and the y+1 NaiveDate lookup were uncovered.

#[test]
fn add_mo_nov30_to_dec() {
    // Nov 30 2024 + 1 month = Dec 30 2024 (no snap, but exercises the m==12
    // path inside add_months_snap when computing Dec's day count).
    // 2024-11-30 00:00 UTC = 1732924800
    // 2024-12-30 00:00 UTC = 1735516800
    check_num(
        "f dt:n n:n>n;add-mo dt n",
        &["f", "1732924800", "1"],
        1735516800.0,
    );
}

#[test]
fn add_mo_dec_to_jan_next_year() {
    // Dec 31 2024 + 1 month = Jan 31 2025 (cross-year +1, no snap because Jan
    // has 31 days). Also exercises the y+1 branch in add_months_snap.
    // 2024-12-31 00:00 UTC = 1735603200
    // 2025-01-31 00:00 UTC = 1738281600
    check_num(
        "f dt:n n:n>n;add-mo dt n",
        &["f", "1735603200", "1"],
        1738281600.0,
    );
}

#[test]
fn add_mo_jan_to_dec_prev_year_negative() {
    // Jan 15 2024 + -1 month = Dec 15 2023. Negative months across the year
    // boundary exercise total.div_euclid(12) with a negative intermediate.
    // 2024-01-15 00:00 UTC = 1705276800
    // 2023-12-15 00:00 UTC = 1702598400
    check_num(
        "f dt:n n:n>n;add-mo dt n",
        &["f", "1705276800", "-1"],
        1702598400.0,
    );
}

#[test]
fn add_mo_minus_twelve() {
    // -12 months = same date previous year.
    // 2024-06-15 -> 2023-06-15
    // 2024-06-15 00:00 UTC = 1718409600
    // 2023-06-15 00:00 UTC = 1686787200
    check_num(
        "f dt:n n:n>n;add-mo dt n",
        &["f", "1718409600", "-12"],
        1686787200.0,
    );
}

#[test]
fn add_mo_jan29_leap_plus13() {
    // Jan 29 2024 + 13 months = Feb 28 2025 (snap: 2025 is non-leap, so Feb
    // only has 28 days). Crosses a year boundary and exercises snap + the
    // y-stride logic for multi-year shifts.
    // 2024-01-29 00:00 UTC = 1706486400
    // 2025-02-28 00:00 UTC = 1740700800
    check_num(
        "f dt:n n:n>n;add-mo dt n",
        &["f", "1706486400", "13"],
        1740700800.0,
    );
}

// ── error paths: out-of-range epochs ─────────────────────────────────────────
// `Utc.timestamp_opt(secs, 0).single()` returns `None` for epochs outside
// chrono's representable range (~262144 BC to 262143 AD). Reaching that
// branch via a f64 arg requires a value large enough to saturate the `as i64`
// cast to i64::MAX, which chrono then rejects. Using a numeric literal in the
// source lets us pass a value beyond f64-as-i64-saturation without losing the
// "out of range" trigger. The `add-mo: result out of calendar range` path is
// hit by a huge month offset rather than a huge epoch.

#[test]
fn add_mo_epoch_out_of_range() {
    check_err(
        "f>n;add-mo 99999999999999999999 0",
        &["f"],
        "add-mo: epoch out of range",
    );
}

#[test]
fn add_mo_result_out_of_calendar_range() {
    // Max i32 months from epoch 0 pushes far past chrono's max year (262143).
    // Exercises the `None` arm of `match add_months_snap(date, months)`.
    check_err(
        "f>n;add-mo 0 2147483647",
        &["f"],
        "add-mo: result out of calendar range",
    );
}

#[test]
fn last_dom_epoch_out_of_range() {
    check_err(
        "f>n;last-dom 99999999999999999999",
        &["f"],
        "last-dom: epoch out of range",
    );
}

#[test]
fn next_business_day_epoch_out_of_range() {
    check_err(
        "f>n;next-business-day 99999999999999999999",
        &["f"],
        "next-business-day: epoch out of range",
    );
}

#[test]
fn day_of_week_epoch_out_of_range() {
    check_err(
        "f>n;day-of-week 99999999999999999999",
        &["f"],
        "day-of-week: epoch out of range",
    );
}
