// Regression: the 3-arg `wr path data "csv"` / `wr path data "tsv"` overload
// should type-check and serialise `data` as a delimited table.
//
// History: PR #179 added the 3-arg `wr path data "json"` shortcut. The csv/tsv
// variants were deferred until this branch. Behaviour:
//
//   * `L (L _)` (list of lists)         → no header, just rows.
//   * RFC 4180 quoting                  → fields containing the delimiter,
//                                         a `"`, or a newline are wrapped
//                                         and inner `"` doubled.
//   * Round-trip via `rdl` + `spl`     → preserves shape.
//
// The VM csv/tsv path uses a small OP_CSVDMP helper (paralleling OP_JDMP) so
// both engines emit identical bytes — the helper itself defers to the
// tree-walker's `write_csv_tsv` implementation to keep behaviour in lockstep.

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
    &["--run-tree", "--run-vm"]
}

// `L (L t)` → no header row, just data rows.
#[test]
fn wr_csv_list_of_lists_no_header() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wr_csv_ll_{i}.csv");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"f>R t t;wr "{path}" [["a","b"],["c","d"]] "csv""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path).expect("missing output file");
        assert_eq!(body, "a,b\nc,d\n", "engine={engine}");
        let _ = std::fs::remove_file(&path);
    }
}

// tsv variant uses `\t` as the delimiter.
#[test]
fn wr_tsv_list_of_lists() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wr_tsv_ll_{i}.tsv");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"f>R t t;wr "{path}" [["a","b"],["c","d"]] "tsv""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path).expect("missing output file");
        assert_eq!(body, "a\tb\nc\td\n", "engine={engine}");
        let _ = std::fs::remove_file(&path);
    }
}

// Field containing the delimiter is RFC-4180-quoted.
#[test]
fn wr_csv_field_with_comma_is_quoted() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wr_csv_comma_{i}.csv");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"f>R t t;wr "{path}" [["a,b","plain"]] "csv""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path).expect("missing output file");
        assert_eq!(body, "\"a,b\",plain\n", "engine={engine}");
        let _ = std::fs::remove_file(&path);
    }
}

// Inner `"` is escaped as `""`.
#[test]
fn wr_csv_field_with_quote_is_escaped() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wr_csv_quote_{i}.csv");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"f>R t t;wr "{path}" [["he said \"hi\"","x"]] "csv""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path).expect("missing output file");
        assert_eq!(body, "\"he said \"\"hi\"\"\",x\n", "engine={engine}");
        let _ = std::fs::remove_file(&path);
    }
}

// Field containing a newline is quoted.
#[test]
fn wr_csv_field_with_newline_is_quoted() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wr_csv_nl_{i}.csv");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"f>R t t;wr "{path}" [["a\nb","c"]] "csv""#);
        let _ = run_ok(engine, &src, "f");
        let body = std::fs::read_to_string(&path).expect("missing output file");
        assert_eq!(body, "\"a\nb\",c\n", "engine={engine}");
        let _ = std::fs::remove_file(&path);
    }
}

// Round-trip: write csv then read back with `rdl` and confirm we can
// split the first line on `,` to recover the original cells.
#[test]
fn wr_csv_roundtrip_via_rdl_spl() {
    for (i, engine) in engines().iter().enumerate() {
        let path = format!("/tmp/ilo_wr_csv_rt_{i}.csv");
        let _ = std::fs::remove_file(&path);
        let write_src = format!(r#"f>R t t;wr "{path}" [["a","b"],["c","d"]] "csv""#);
        let _ = run_ok(engine, &write_src, "f");
        // Read first line back and split on the csv separator.
        let read_src = format!(r#"g>t;p=rdl "{path}";?p{{~v:hd (spl (hd v) ",");^_:"err"}}"#);
        let first = run_ok(engine, &read_src, "g");
        assert_eq!(first, "a", "engine={engine}: rdl/spl roundtrip mismatch");
        let _ = std::fs::remove_file(&path);
    }
}
