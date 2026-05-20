// Cross-engine regression tests for `dtparse-rel` — relative-date phrase
// resolution anchored at a caller-supplied Unix epoch.
//
// Originating entry in ilo_feedback/pending.md P1 #8:
// > Every date persona manually rolled `last <weekday>`, `N days ago`,
// > `yesterday`, `today`, `in N days` arithmetic — ~40 LoC of helpers per
// > persona.
//
// Anchor: now = 1705276800 (2024-01-15 00:00:00 UTC, Monday).
//
// All tests run across tree, VM, and Cranelift JIT to pin cross-engine parity.
// The builtin is tree-bridge eligible (`is_tree_bridge_eligible`, argc=2).

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> Vec<&'static str> {
    let mut v = vec!["tree", "--run-vm"];
    if cfg!(feature = "cranelift") {
        v.push("--jit");
    }
    v
}

// now = 2024-01-15 (Monday) 00:00:00 UTC
const NOW: &str = "1705276800";

fn run_ok(engine: &str, src: &str, fn_name: &str) -> String {
    let out = match engine {
        "tree" => ilo()
            .args(["run", src, fn_name])
            .output()
            .expect("failed to run ilo"),
        _ => ilo()
            .args([src, engine, fn_name])
            .output()
            .expect("failed to run ilo"),
    };
    assert!(
        out.status.success(),
        "ilo {engine} failed for fn={fn_name}: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_result(engine: &str, src: &str, fn_name: &str) -> (bool, String, String) {
    let out = match engine {
        "tree" => ilo()
            .args(["run", src, fn_name])
            .output()
            .expect("failed to run ilo"),
        _ => ilo()
            .args([src, engine, fn_name])
            .output()
            .expect("failed to run ilo"),
    };
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn mk(phrase: &str) -> String {
    // f>n;dtparse-rel!! "phrase" 1705276800
    format!("f>n;dtparse-rel!! \"{phrase}\" {NOW}")
}

fn run_epoch(engine: &str, phrase: &str) -> i64 {
    let src = mk(phrase);
    let s = run_ok(engine, &src, "f");
    s.trim()
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("expected number epoch for '{phrase}', got: {s:?}")) as i64
}

// ── keywords ────────────────────────────────────────────────────────────────

#[test]
fn today() {
    for e in engines() {
        assert_eq!(run_epoch(e, "today"), 1705276800, "engine={e}");
    }
}

#[test]
fn yesterday() {
    for e in engines() {
        assert_eq!(run_epoch(e, "yesterday"), 1705190400, "engine={e}");
    }
}

#[test]
fn tomorrow() {
    for e in engines() {
        assert_eq!(run_epoch(e, "tomorrow"), 1705363200, "engine={e}");
    }
}

// ── N days ago / in N days ───────────────────────────────────────────────────

#[test]
fn n_days_ago() {
    for e in engines() {
        // 3 days ago from 2024-01-15 = 2024-01-12
        assert_eq!(run_epoch(e, "3 days ago"), 1705017600, "engine={e}");
        // 0 days ago = today
        assert_eq!(run_epoch(e, "0 days ago"), 1705276800, "engine={e}");
    }
}

#[test]
fn in_n_days() {
    for e in engines() {
        // in 5 days from 2024-01-15 = 2024-01-20
        assert_eq!(run_epoch(e, "in 5 days"), 1705708800, "engine={e}");
        // in 0 days = today
        assert_eq!(run_epoch(e, "in 0 days"), 1705276800, "engine={e}");
    }
}

// ── N weeks ago / in N weeks ─────────────────────────────────────────────────

#[test]
fn n_weeks_ago() {
    for e in engines() {
        // 2 weeks ago from 2024-01-15 = 2024-01-01
        assert_eq!(run_epoch(e, "2 weeks ago"), 1704067200, "engine={e}");
    }
}

#[test]
fn in_n_weeks() {
    for e in engines() {
        // in 1 week from 2024-01-15 = 2024-01-22
        assert_eq!(run_epoch(e, "in 1 week"), 1705881600, "engine={e}");
    }
}

// ── N months ago / in N months ───────────────────────────────────────────────

#[test]
fn n_months_ago() {
    for e in engines() {
        // 1 month ago from 2024-01-15 = 2023-12-15 (singular form)
        assert_eq!(run_epoch(e, "1 month ago"), 1702598400, "engine={e}");
    }
}

#[test]
fn in_n_months() {
    for e in engines() {
        // in 2 months from 2024-01-15 = 2024-03-15
        assert_eq!(run_epoch(e, "in 2 months"), 1710460800, "engine={e}");
    }
}

// Month-end clamping: Jan 31 + 1 month clamps to the last valid day of February.
// In 2024 (a leap year) that's Feb 29, so the result is 2024-02-29 = 1709164800.
#[test]
fn month_end_clamp() {
    // now = 2024-01-31 = 1706659200
    let src = "f>n;dtparse-rel!! \"in 1 months\" 1706659200";
    for e in engines() {
        let epoch = run_ok(e, src, "f")
            .trim()
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("expected epoch, engine={e}")) as i64;
        // 2024 is a leap year; 2024-02-29 = 1709164800
        assert_eq!(epoch, 1709164800, "engine={e} (expected 2024-02-29)");
    }
}

// ── weekday navigation ────────────────────────────────────────────────────────

#[test]
fn last_weekday_long_form() {
    for e in engines() {
        // last friday from Monday 2024-01-15 = 2024-01-12
        assert_eq!(run_epoch(e, "last friday"), 1705017600, "engine={e}");
        // last sunday from Monday 2024-01-15 = 2024-01-14
        assert_eq!(run_epoch(e, "last sunday"), 1705190400, "engine={e}");
    }
}

#[test]
fn last_weekday_is_never_today() {
    // now = Monday; last monday should be the previous Monday, not today.
    for e in engines() {
        // last monday from 2024-01-15 (Mon) = 2024-01-08
        assert_eq!(run_epoch(e, "last monday"), 1704672000, "engine={e}");
    }
}

#[test]
fn next_weekday_long_form() {
    for e in engines() {
        // next friday from Monday 2024-01-15 = 2024-01-19
        assert_eq!(run_epoch(e, "next friday"), 1705622400, "engine={e}");
    }
}

#[test]
fn next_weekday_is_never_today() {
    // now = Monday; next monday should be the following Monday, not today.
    for e in engines() {
        // next monday from 2024-01-15 (Mon) = 2024-01-22
        assert_eq!(run_epoch(e, "next monday"), 1705881600, "engine={e}");
    }
}

#[test]
fn this_weekday() {
    for e in engines() {
        // this wednesday from Monday 2024-01-15 = 2024-01-17
        assert_eq!(run_epoch(e, "this wednesday"), 1705449600, "engine={e}");
        // this monday from Monday 2024-01-15 = today
        assert_eq!(run_epoch(e, "this monday"), 1705276800, "engine={e}");
    }
}

// ── short weekday names ───────────────────────────────────────────────────────

#[test]
fn short_weekday_names() {
    for e in engines() {
        // last fri = last friday
        assert_eq!(
            run_epoch(e, "last fri"),
            run_epoch(e, "last friday"),
            "engine={e}"
        );
        // next sat from Monday = +5 days = 2024-01-20
        assert_eq!(run_epoch(e, "next sat"), 1705708800, "engine={e}");
        // this tue from Monday = +1 day = 2024-01-16
        assert_eq!(run_epoch(e, "this tue"), 1705363200, "engine={e}");
    }
}

// ── ISO-8601 passthrough ──────────────────────────────────────────────────────

#[test]
fn iso_passthrough() {
    for e in engines() {
        // 2023-12-25 = 1703462400 regardless of now
        assert_eq!(run_epoch(e, "2023-12-25"), 1703462400, "engine={e}");
        // epoch 0 date
        assert_eq!(run_epoch(e, "1970-01-01"), 0, "engine={e}");
    }
}

// ── error cases ───────────────────────────────────────────────────────────────

#[test]
fn unknown_phrase_returns_err() {
    // dtparse-rel returns R n t; unrecognised phrase gives Value::Err, which
    // the runtime prints as "^<msg>" (to stdout on VM/JIT, stderr on tree) and exits 1.
    let src = "f>R n t;dtparse-rel \"sometime soon\" 1705276800";
    for e in engines() {
        let (ok, stdout, stderr) = run_result(e, src, "f");
        assert!(!ok, "engine={e}: expected exit 1 for Err result");
        let combined = format!("{stdout}{stderr}");
        assert!(
            combined.contains("dtparse-rel") || combined.contains("unrecognised"),
            "engine={e}: expected Err with 'dtparse-rel', got stdout={stdout:?} stderr={stderr:?}"
        );
    }
}

// Regression for the suffix-strip false-positive: a phrase like "in this day"
// or "in some day" used to slip through the "in N day(s)" arm because
// `strip_suffix(" day")` matched the trailing 4 chars and the integer parser
// then failed on the non-digit remainder, surfacing a misleading
// "invalid day count" error instead of the unrecognised-phrase fallback.
// Same risk class for `" day(s)/week(s)/month(s)"` suffixes; covered together.
#[test]
fn non_digit_count_falls_through_to_unrecognised_phrase() {
    let cases = [
        ("in this day", "in 0 days"),    // " day" suffix, non-digit remainder
        ("in some days", "in 0 days"),   // " days" suffix, non-digit remainder
        ("for week ago", "0 weeks ago"), // " week ago" suffix, non-digit remainder
        ("over the month ago", "0 months ago"), // " month ago" suffix, non-digit remainder
        ("in some months", "in 0 months"),
    ];
    for (phrase, _hint) in cases {
        let src = format!("f>R n t;dtparse-rel \"{phrase}\" 1705276800");
        for e in engines() {
            let (ok, stdout, stderr) = run_result(e, &src, "f");
            assert!(!ok, "engine={e} phrase={phrase:?}: expected Err exit 1");
            let combined = format!("{stdout}{stderr}");
            // Must surface the unrecognised-phrase error, NOT "invalid <unit> count".
            assert!(
                combined.contains("unrecognised") || combined.contains("expected"),
                "engine={e} phrase={phrase:?}: expected unrecognised-phrase Err, \
                 got stdout={stdout:?} stderr={stderr:?}"
            );
            assert!(
                !combined.contains("invalid day count")
                    && !combined.contains("invalid week count")
                    && !combined.contains("invalid month count"),
                "engine={e} phrase={phrase:?}: leaked 'invalid <unit> count' error \
                 from suffix-strip false positive, got stdout={stdout:?} stderr={stderr:?}"
            );
        }
    }
}

#[test]
fn unknown_weekday_name_returns_err() {
    let src = "f>R n t;dtparse-rel \"last flursday\" 1705276800";
    for e in engines() {
        let (ok, stdout, stderr) = run_result(e, src, "f");
        assert!(!ok, "engine={e}: expected exit 1 for Err result");
        let combined = format!("{stdout}{stderr}");
        assert!(
            combined.contains("flursday") || combined.contains("weekday"),
            "engine={e}: expected Err naming unknown weekday, got stdout={stdout:?} stderr={stderr:?}"
        );
    }
}

// ── type signature via verifier ───────────────────────────────────────────────

#[test]
fn verify_wrong_arg_types() {
    // First arg should be t, not n — verifier should emit ILO-T013.
    let src = "f>R n t;dtparse-rel 42 0";
    let out = ilo()
        .args([src, "--vm", "f"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ILO-T013") || stderr.contains("dtparse-rel"),
        "expected type error, got: {stderr}"
    );
}
