// ILO-403: recursive sum types (linked list)
//
// Pins the behaviour of `type list = nil | cons(cell)` patterns where a
// sum-type variant payload references the enclosing sum type, forming a
// classic heap-chained linked list.  Covers:
//   - `nil` as a variant name (was rejected by the parser as a reserved word)
//   - `nil` in expression position resolves to the variant constructor when
//     a `nil` variant is in scope, or to the built-in nil value otherwise
//   - `nil:` in a match arm covers both nil literal and `nil` variant tag
//   - Self-referential payloads via a record cons-cell type
//   - 5-element list construction and recursive walk (sum, length)

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_file(path: &str) -> String {
    let out = ilo().args(["run", path]).output().expect("spawn ilo");
    assert!(
        out.status.success(),
        "ilo failed for {path}:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_src(src: &str) -> String {
    let tmp = tempfile(src);
    run_file(&tmp)
}

fn tempfile(src: &str) -> String {
    use std::io::Write;
    let path = format!(
        "/tmp/ilo-ilo403-test-{}.ilo",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    );
    let mut f = std::fs::File::create(&path).expect("create temp file");
    f.write_all(src.as_bytes()).expect("write temp file");
    path
}

// ── Core: 5-element list via engine-matrix file ──────────────────────────

#[test]
fn five_element_list_sum() {
    let result = run_file("tests/engine-matrix/50-recursive-sum-type.ilo");
    assert_eq!(result, "15", "sum of 1..5 list should be 15");
}

// ── nil as variant name and in expressions ───────────────────────────────

#[test]
fn nil_variant_name_and_expression() {
    // `nil` can be used as a 0-arg variant constructor.
    // `nil:` in a match arm covers both the built-in nil and the variant.
    let result = run_src(
        "type nlist = nil | cons(n)\nllen xs:nlist>n;?xs{nil:0;cons(h):+1 0}\nmain>n;a=cons 3;b=nil;+llen(a) llen(b)\n",
    );
    assert_eq!(result, "1");
}

// ── nil backward compat: nil literal still works outside sum type scope ──

#[test]
fn nil_literal_backward_compat() {
    let result = run_src("main>n\n  x=nil\n  5\n");
    assert_eq!(result, "5");
}

// ── Self-referential recursive type ──────────────────────────────────────

#[test]
fn recursive_sum_type_length_and_sum() {
    let result = run_src(
        "type nlist = nil | cons(cell)\ntype cell { hd:n; tl:nlist }\nllen xs:nlist>n;?xs{nil:0;cons(c):+1(llen c.tl)}\nlsum xs:nlist>n;?xs{nil:0;cons(c):+c.hd(lsum c.tl)}\nmain>n;l=cons (cell hd:10 tl:(cons (cell hd:20 tl:(cons (cell hd:30 tl:nil)))));+llen(l) lsum(l)\n",
    );
    // length=3, sum=60, total=63
    assert_eq!(result, "63");
}
