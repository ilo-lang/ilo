// Regression for csv-pipeline rerun10:
//
//   `wr path data "csv"` correctly emits a quoted, multi-line field per
//   RFC 4180. But `rd path "csv"` used to split content on `\n` before
//   tracking quote state, so the embedded newline was treated as a record
//   break and one logical row came back as two.
//
//   Repro from the persona report:
//     wrote 3 rows: [name,note,n], [Frame, Gamma,"has ""quote""",1],
//                   [plain,"line\nbreak",2]
//     re-read got 4 rows: [name,note,n], [Frame, Gamma,has "quote",1],
//                         [plain,line], [break,2]
//
// The fix replaces the line-by-line approach in `parse_format` with a
// single-pass quote-aware scanner (`parse_csv_content`). These tests pin
// the round-trip across every available engine — if any engine regresses,
// or a future change drifts the writer and reader out of step, this fails.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn engines() -> &'static [&'static str] {
    &["--vm"]
}

// Canonical regression: write a row with an embedded newline, then read
// the file back as csv. The row count must survive the round-trip.
#[test]
fn csv_multiline_quoted_field_round_trip_row_count() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_csv_ml_rt_count_{i}.csv");
        let _ = std::fs::remove_file(&path);
        // Two rows: a header and a body row whose middle cell contains a
        // literal newline. Entry returns the row count read back.
        let src = format!(
            r#"f>R n t;wr! "{path}" [["name","note","n"],["plain","line\nbreak","2"]] "csv";rows=rd! "{path}" "csv";~len rows"#
        );
        let got = run_ok(engine, &src, "f");
        assert_eq!(
            got, "2",
            "engine={engine}: round-trip row count drifted (writer emitted multi-line quoted cell, reader split it)"
        );
        let _ = std::fs::remove_file(&path);
    }
}

// The embedded-newline cell must come back as a single cell with the
// `\n` preserved, not as two cells across two rows.
#[test]
fn csv_multiline_quoted_field_round_trip_cell_value() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_csv_ml_rt_cell_{i}.csv");
        let _ = std::fs::remove_file(&path);
        // After read-back, rows[1][1] should be "line\nbreak".
        let src = format!(
            r#"f>R t t;wr! "{path}" [["name","note","n"],["plain","line\nbreak","2"]] "csv";rows=rd! "{path}" "csv";~at (at rows 1) 1"#
        );
        let got = run_ok(engine, &src, "f");
        assert_eq!(
            got, "line\nbreak",
            "engine={engine}: multi-line cell did not round-trip verbatim"
        );
        let _ = std::fs::remove_file(&path);
    }
}

// Combined edge case: a single cell with BOTH an embedded newline AND an
// escaped double-quote. This exercises quote-state tracking across the
// embedded `""` and the embedded `\n`.
#[test]
fn csv_multiline_with_escaped_quote_round_trip() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_csv_ml_rt_qn_{i}.csv");
        let _ = std::fs::remove_file(&path);
        let src = format!(
            r#"f>R t t;wr! "{path}" [["a","he said \"hi\"\nfoo","b"]] "csv";rows=rd! "{path}" "csv";~at (at rows 0) 1"#
        );
        let got = run_ok(engine, &src, "f");
        assert_eq!(
            got, "he said \"hi\"\nfoo",
            "engine={engine}: quote+newline cell did not round-trip"
        );
        let _ = std::fs::remove_file(&path);
    }
}

// Negative control: round-trip on a CSV with NO embedded newlines must
// still produce the same number of rows as before the fix.
#[test]
fn csv_plain_round_trip_unchanged() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_csv_plain_rt_{i}.csv");
        let _ = std::fs::remove_file(&path);
        let src = format!(
            r#"f>R n t;wr! "{path}" [["name","n"],["alice","1"],["bob","2"]] "csv";rows=rd! "{path}" "csv";~len rows"#
        );
        let got = run_ok(engine, &src, "f");
        assert_eq!(got, "3", "engine={engine}: plain csv row count regressed");
        let _ = std::fs::remove_file(&path);
    }
}
