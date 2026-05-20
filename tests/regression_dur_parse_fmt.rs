// Cross-engine regression tests for `dur-parse` and `dur-fmt` — the duration
// parse/format builtins added in 0.12.x.
//
// `dur-parse s > R n t` — lenient parser for human duration strings.
//   Accepts: abbreviations (s/m/h/d/w), full names (singular + plural),
//   decimal quantities, mixed sequences.
// `dur-fmt n > t` — format seconds as human-readable; drops zero parts.
//
// Both are tree-bridge eligible: VM and Cranelift dispatch through the tree
// interpreter, so all engines share the same semantics. Tests fan across
// available engines to catch any future divergence.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES: &[&str] = &["--vm"];

/// Run source with the VM engine and return trimmed stdout.
fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} {entry:?} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} {src:?} {entry:?} unexpectedly succeeded: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn parse_num(s: &str) -> f64 {
    s.lines()
        .next()
        .unwrap_or("")
        .trim()
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("expected number, got: {s:?}"))
}

// ── dur-parse: happy-path parsing ────────────────────────────────────────────

#[test]
fn dur_parse_hours_minutes() {
    let src = "f>R n t;dur-parse \"3h 30m\"";
    for e in ENGINES {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 12600.0, "engine={e}");
    }
}

#[test]
fn dur_parse_week_and_days() {
    let src = "f>R n t;dur-parse \"1 week 2 days\"";
    for e in ENGINES {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 777600.0, "engine={e}");
    }
}

#[test]
fn dur_parse_decimal_hours() {
    let src = "f>R n t;dur-parse \"1.5 hours\"";
    for e in ENGINES {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 5400.0, "engine={e}");
    }
}

#[test]
fn dur_parse_bare_seconds_abbrev() {
    let src = "f>R n t;dur-parse \"90s\"";
    for e in ENGINES {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 90.0, "engine={e}");
    }
}

#[test]
fn dur_parse_no_space_between_number_and_unit() {
    let src = "f>R n t;dur-parse \"4h32m\"";
    for e in ENGINES {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 16320.0, "engine={e}");
    }
}

#[test]
fn dur_parse_full_unit_names() {
    // "3 weeks 2 days 5 hours" — the exact example from the P3 spec
    let src = "f>R n t;dur-parse \"3 weeks 2 days 5 hours\"";
    for e in ENGINES {
        let secs = parse_num(&run_ok(e, src, "f"));
        // 3w = 1814400; 2d = 172800; 5h = 18000 => 2005200
        assert_eq!(secs, 2005200.0, "engine={e}");
    }
}

#[test]
fn dur_parse_singular_forms() {
    let src = "f>R n t;dur-parse \"1 week 1 day 1 hour 1 minute 1 second\"";
    for e in ENGINES {
        let secs = parse_num(&run_ok(e, src, "f"));
        // 604800 + 86400 + 3600 + 60 + 1 = 694861
        assert_eq!(secs, 694861.0, "engine={e}");
    }
}

#[test]
fn dur_parse_abbreviation_d() {
    let src = "f>R n t;dur-parse \"1d\"";
    for e in ENGINES {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 86400.0, "engine={e}");
    }
}

#[test]
fn dur_parse_abbreviation_w() {
    let src = "f>R n t;dur-parse \"2w\"";
    for e in ENGINES {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 1209600.0, "engine={e}");
    }
}

#[test]
fn dur_parse_hr_hrs_aliases() {
    let src = "f>R n t;dur-parse \"4 hrs\"";
    for e in ENGINES {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 14400.0, "engine={e}");
    }
}

#[test]
fn dur_parse_min_alias() {
    let src = "f>R n t;dur-parse \"30 mins\"";
    for e in ENGINES {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 1800.0, "engine={e}");
    }
}

#[test]
fn dur_parse_sec_alias() {
    let src = "f>R n t;dur-parse \"45 sec\"";
    for e in ENGINES {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 45.0, "engine={e}");
    }
}

#[test]
fn dur_parse_zero() {
    let src = "f>R n t;dur-parse \"0h\"";
    for e in ENGINES {
        assert_eq!(parse_num(&run_ok(e, src, "f")), 0.0, "engine={e}");
    }
}

// ── dur-parse: error cases ────────────────────────────────────────────────────

#[test]
fn dur_parse_empty_string_errors() {
    let src = "f>R n t;dur-parse \"\"";
    for e in ENGINES {
        // Should succeed at runtime (returns Ok/Err Result), but result is Err.
        // We run as `dur-parse! ""` to force error propagation.
        let out = ilo()
            .args([src, e, "f"])
            .output()
            .expect("ilo");
        // The program itself succeeds (no crash), output should start with "^"
        // (Err indicator) when printed by the engine.
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        // The result is Err; printed as "^..." without auto-unwrap.
        // We check that it's not an Ok number.
        assert!(
            !stdout.trim().chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false),
            "engine={e}: expected Err result, got numeric stdout: {stdout}"
        );
    }
}

#[test]
fn dur_parse_no_unit_errors() {
    // Input has a number but no recognisable unit — should Err.
    let src = "f>t;r=dur-parse \"42\";?r{~_:\"ok\";^_:\"err\"}";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "err", "engine={e}");
    }
}

#[test]
fn dur_parse_bad_unit_errors() {
    let src = "f>t;r=dur-parse \"3 parsecs\";?r{~_:\"ok\";^_:\"err\"}";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "err", "engine={e}");
    }
}

// ── dur-fmt: happy-path formatting ───────────────────────────────────────────

#[test]
fn dur_fmt_hours_and_minutes() {
    let src = "f>t;dur-fmt 9720";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "2h 42m", "engine={e}");
    }
}

#[test]
fn dur_fmt_exactly_one_day() {
    let src = "f>t;dur-fmt 86400";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "1 day", "engine={e}");
    }
}

#[test]
fn dur_fmt_exactly_one_week() {
    let src = "f>t;dur-fmt 604800";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "1 week", "engine={e}");
    }
}

#[test]
fn dur_fmt_minutes_and_seconds() {
    let src = "f>t;dur-fmt 90";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "1m 30s", "engine={e}");
    }
}

#[test]
fn dur_fmt_bare_seconds() {
    let src = "f>t;dur-fmt 45";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "45s", "engine={e}");
    }
}

#[test]
fn dur_fmt_zero() {
    let src = "f>t;dur-fmt 0";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "0s", "engine={e}");
    }
}

#[test]
fn dur_fmt_week_and_days() {
    // 1 week + 2 days = 604800 + 172800 = 777600
    let src = "f>t;dur-fmt 777600";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "1 week 2 days", "engine={e}");
    }
}

#[test]
fn dur_fmt_complex() {
    // 2 days + 3 hours = 172800 + 10800 = 183600
    let src = "f>t;dur-fmt 183600";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "2 days 3h", "engine={e}");
    }
}

#[test]
fn dur_fmt_hours_only() {
    let src = "f>t;dur-fmt 3600";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "1h", "engine={e}");
    }
}

#[test]
fn dur_fmt_plural_hours() {
    let src = "f>t;dur-fmt 7200";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "2h", "engine={e}");
    }
}

#[test]
fn dur_fmt_plural_weeks() {
    // 3 weeks
    let src = "f>t;dur-fmt 1814400";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "3 weeks", "engine={e}");
    }
}

// ── round-trip: parse then format ────────────────────────────────────────────

#[test]
fn dur_round_trip_3h_30m() {
    // "3h 30m" -> 12600s -> "3h 30m"
    let src = "f>R t t;n=dur-parse! \"3h 30m\";~dur-fmt n";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "3h 30m", "engine={e}");
    }
}

#[test]
fn dur_round_trip_days_and_hours() {
    // "2 days 3 hours" -> 183600s -> "2 days 3h"
    // Note: full name "hours" normalises to abbreviated "h" in output.
    let src = "f>R t t;n=dur-parse! \"2 days 3 hours\";~dur-fmt n";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "2 days 3h", "engine={e}");
    }
}

#[test]
fn dur_round_trip_1_week() {
    let src = "f>R t t;n=dur-parse! \"1 week\";~dur-fmt n";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "1 week", "engine={e}");
    }
}

// ── bad-type errors ───────────────────────────────────────────────────────────
//
// Note: the JIT engine silently returns nil for tree-bridge builtins that
// receive wrong-type args (same pattern as `dirname 42` on JIT). This is a
// known JIT limitation for tree-bridge builtins; only the VM engine is tested
// for wrong-type error propagation here. The VM gets the runtime error through
// the tree interpreter dispatch path. This matches the test pattern used by
// the dirname/basename/pathjoin tests.

#[test]
fn dur_parse_wrong_type_errors() {
    // dur-parse expects text; passing a number should error on VM.
    let src = "f>R n t;dur-parse 42";
    let stderr = run_err("--vm", src, "f");
    assert!(
        stderr.contains("dur-parse"),
        "VM: expected dur-parse error, got: {stderr}"
    );
}

#[test]
fn dur_fmt_wrong_type_errors() {
    // dur-fmt expects a number; passing text should error on VM.
    let src = "f>t;dur-fmt \"hello\"";
    let stderr = run_err("--vm", src, "f");
    assert!(
        stderr.contains("dur-fmt"),
        "VM: expected dur-fmt error, got: {stderr}"
    );
}
