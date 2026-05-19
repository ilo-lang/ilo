// Cross-engine regression tests for multi-line function bodies whose
// tail expression is a record constructor (`type-name field:val
// field:val ...`).
//
// Before this fix, the parser's `is_fn_decl_start_strict` helper —
// which decides at top-level body boundaries whether a candidate
// `Ident param:type ...` shape is a real fn header or a record-
// constructor expression — scanned forward looking for a `>` (the
// return-type separator that confirms a real header) and stopped only
// at `;`, `}`, or EOF. It did NOT stop at a top-level declaration
// boundary (an un-indented newline filtered out of the token stream
// before parsing), so the scan would walk past the end of the current
// line and find the `>` of the *next* function's header, incorrectly
// classifying the candidate as a fn-decl start. That terminated the
// current body, sent control back to `parse_program`, and parsing the
// would-be record (`cr country:name revenue:rv`) as a fn header then
// tripped ILO-P020 because the `cr ...` line has no `>` at all.
//
// The fix adds a per-token decl-boundary check inside the scan loop:
// a real fn header always has its `>` on the same logical line as the
// name, so finding a declaration boundary before `>` means the
// candidate is a record, not a header.
//
// Coverage:
//   (a) `cr` tail with 1, 2, 3 fields — happy path, the original
//       db-analyst rerun8 repro.
//   (b) Record tail with NO trailing function — exercises the EOF
//       branch of the scan (no `>` ever found).
//   (c) Record tail after an explicit `;` continuation (single-line
//       multi-statement body) still parses cleanly.
//   (d) Real fn declaration whose first statement happens to be a
//       record constructor still parses as a fn decl — the strict
//       check correctly identifies a real header on the SAME line as
//       its `>`.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} {args:?} unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--vm"];

// ── (a) Record-constructor tail with 1, 2, 3 fields ──────────────────

#[test]
fn record_tail_one_field_cross_engine() {
    // Single-field record tail in a multi-line body, followed by
    // another top-level fn. Before the fix this errored with ILO-P020
    // because the scan walked into `mk>cr`'s `>`.
    let src = "type r{x:n}\n\
               build>r\n  v=1\n  r x:v\n\
               mk>n\n  rec=build()\n  rec.x";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["mk"]),
            "1",
            "{engine}: 1-field record tail"
        );
    }
}

#[test]
fn record_tail_two_fields_cross_engine() {
    let src = "type r{x:n;y:n}\n\
               build>r\n  a=10\n  b=20\n  r x:a y:b\n\
               mk>n\n  rec=build()\n  +rec.x rec.y";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["mk"]),
            "30",
            "{engine}: 2-field record tail"
        );
    }
}

#[test]
fn record_tail_three_fields_cross_engine() {
    // The original db-analyst rerun8 repro shape — text + number
    // fields, multi-line body, record tail, another fn after.
    // Read individual fields rather than asserting on Display order:
    // record-field iteration order on the tree-walker is HashMap-
    // backed and therefore non-deterministic across runs, so we'd
    // rather pin the *field values* than the field order.
    let src = "type cr{country:t;revenue:n;rank:n}\n\
               build-rec p:L _>cr\n  name=p.0\n  rv=p.1\n  cr country:name revenue:rv rank:1\n\
               mk-rev>n\n  p=[ \"uk\", 100 ]\n  rec=build-rec p\n  rec.revenue\n\
               mk-rank>n\n  p=[ \"uk\", 100 ]\n  rec=build-rec p\n  rec.rank\n\
               mk-country>t\n  p=[ \"uk\", 100 ]\n  rec=build-rec p\n  rec.country";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["mk-rev"]),
            "100",
            "{engine}: 3-field record tail .revenue"
        );
        assert_eq!(
            run_ok(engine, src, &["mk-rank"]),
            "1",
            "{engine}: 3-field record tail .rank"
        );
        assert_eq!(
            run_ok(engine, src, &["mk-country"]),
            "uk",
            "{engine}: 3-field record tail .country"
        );
    }
}

// ── (b) Record tail with no trailing function (EOF branch) ───────────

#[test]
fn record_tail_no_trailing_fn_cross_engine() {
    // The scan must hit EOF without finding a `>` and return false,
    // keeping the body intact. Catches a regression where the scan
    // might be reordered to check boundary AFTER `Greater`.
    // Read the field rather than Display the whole record — see
    // `record_tail_three_fields_cross_engine` for the rationale.
    let src = "type r{x:n}\nbuild>n\n  v=42\n  rec=r x:v\n  rec.x";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["build"]),
            "42",
            "{engine}: record tail, no trailing fn"
        );
    }
}

// ── (c) Record tail on a single-line body, after explicit `;` ────────

#[test]
fn record_tail_after_semicolon_cross_engine() {
    // Single-line form: `build>r;v=1;r x:v`. The strict check fires
    // at the `;` between `v=1` and `r x:v`, and must NOT terminate
    // the body. Same scan path — the boundary check sits inside the
    // shared scan loop.
    let src = "type r{x:n}\n\
               build>r;v=1;r x:v\n\
               mk>n;rec=build();rec.x";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["mk"]),
            "1",
            "{engine}: single-line record tail after `;`"
        );
    }
}

// ── (d) Negative: real fn decl with `>` on the header line ───────────

#[test]
fn real_fn_decl_still_terminates_body_single_line_cross_engine() {
    // Single-line file: no newlines, all decls separated by `;`. The
    // strict check is what STOPS `helper`'s body from greedily slurping
    // `main x:n>n;helper x` as continuation statements. With the boundary
    // check added by this fix, the strict scan finds `>` (in `main>n`)
    // before any boundary (there are none), so it correctly returns
    // true and the body terminates. Catches regressions where someone
    // tightens the boundary check too far and accidentally rejects
    // legitimate single-line multi-fn files.
    let src = "helper x:n>n;*x 2;main>n;helper 5";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["main"]),
            "10",
            "{engine}: single-line multi-fn file still parses"
        );
    }
}
