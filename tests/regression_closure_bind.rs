// Regression tests for the closure-bind HOF variant.
//
// Every HOF (`srt`, `map`, `flt`, `fld`) accepts an optional extra `ctx`
// argument that is forwarded to each invocation of the function. This lets
// agents pass external state (lookup tables, thresholds, accumulators) into
// the callback without bundling the state into per-element records.
//
// The verifier disambiguates by arity:
//   srt fn xs       (2-arg) vs. srt fn ctx xs       (3-arg)
//   map fn xs       (2-arg) vs. map fn ctx xs       (3-arg)
//   flt fn xs       (2-arg) vs. flt fn ctx xs       (3-arg)
//   fld fn xs init  (3-arg) vs. fld fn ctx xs init  (4-arg)
//
// As of PR 3c, the closure-bind variants run on every engine. The VM and
// Cranelift paths route through the tree-bridge (the same mechanism `grp`,
// `uniqby`, `partition`, and 2-arg `srt` use from PR 3b). The bridge calls
// back into the tree interpreter for the inner HOF loop, which is where the
// user-function dispatch already works correctly.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("ilo_cbind_{name}_{}_{n}.ilo", std::process::id()));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_ok_on(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_all(src: &str, entry: &str, args: &[&str], expected: &str) {
    for engine in ["--run-tree", "--run-vm", "--run-cranelift"] {
        let actual = run_ok_on(engine, src, entry, args);
        assert_eq!(
            actual, expected,
            "engine {engine} produced {actual:?}, expected {expected:?} for src `{src}`"
        );
    }
}

// Tree-only variant kept for the verifier-error tests below — those produce
// the same diagnostic on every engine, but the verifier runs before any
// engine-specific dispatch, so we only need one path to confirm.
fn run_ok(src: &str, entry: &str, args: &[&str]) -> String {
    run_ok_on("--run-tree", src, entry, args)
}

fn run_err(src: &str, entry: &str) -> String {
    let path = write_src(entry, src);
    let out = ilo()
        .arg(&path)
        .arg("--run-tree")
        .arg(entry)
        .output()
        .expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        !out.status.success(),
        "expected failure but ilo succeeded for `{src}`"
    );
    let mut s = String::from_utf8_lossy(&out.stderr).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stdout));
    s
}

// ── srt: sort by external lookup map — the "top-N by magnitude" pattern ────
// Sort symbols by an externally provided priority map (lower priority first).
//
// Without closure-bind, agents would have to fold the map into each list
// element (`[("a", 3), ("b", 1)]`) and write a key fn that pulls the second
// field — the per-program tax this feature removes.

const SRT_BY_LOOKUP: &str = "\
pri sym:t m:M t n>n;r=mget m sym;?r{n v:v;_:99999}
top pri-map:M t n syms:L t>L t;srt pri pri-map syms";

#[test]
fn srt_with_external_lookup() {
    // pri-map: c=1, a=2, b=3 → sorted by priority: c, a, b
    let src = format!(
        "{SRT_BY_LOOKUP}\nmain>L t;m=mset mmap \"a\" 2;m=mset m \"b\" 3;m=mset m \"c\" 1;\
         top m [\"a\",\"b\",\"c\"]"
    );
    run_all(&src, "main", &[], "[c, a, b]");
}

// ── map: enrich elements via external lookup ───────────────────────────────

const MAP_WITH_LOOKUP: &str = "\
look sym:t m:M t n>n;r=mget m sym;?r{n v:v;_:0}
prices pm:M t n syms:L t>L n;map look pm syms";

#[test]
fn map_with_lookup() {
    let src = format!(
        "{MAP_WITH_LOOKUP}\nmain>L n;m=mset mmap \"a\" 10;m=mset m \"b\" 20;\
         prices m [\"a\",\"b\",\"a\"]"
    );
    run_all(&src, "main", &[], "[10, 20, 10]");
}

// ── flt: filter by external threshold ──────────────────────────────────────

const FLT_WITH_THRESHOLD: &str = "\
big x:n thr:n>b;>=x thr
above t:n xs:L n>L n;flt big t xs";

#[test]
fn flt_with_threshold() {
    run_all(
        &format!("{FLT_WITH_THRESHOLD}\nmain>L n;above 4 [1,5,3,8,2]"),
        "main",
        &[],
        "[5, 8]",
    );
}

// ── fld: fold using external multiplier ────────────────────────────────────

const FLD_WITH_ACCUM: &str = "\
add-scaled acc:n x:n k:n>n;+acc *x k
total k:n xs:L n>n;fld add-scaled k xs 0";

#[test]
fn fld_with_external_accumulator() {
    // sum of [1,2,3] each scaled by 10 → 60
    run_all(
        &format!("{FLD_WITH_ACCUM}\nmain>n;total 10 [1,2,3]"),
        "main",
        &[],
        "60",
    );
}

// ── 2-arg variants still work (no regression for existing programs) ────────

const SRT_2ARG: &str = "abs1 x:n>n;abs x\nf xs:L n>L n;srt abs1 xs";

#[test]
fn srt_2arg_unchanged() {
    assert_eq!(run_ok(SRT_2ARG, "f", &["[-3,1,-5,2]"]), "[1, 2, -3, -5]");
}

// ── verifier: 3-arg srt with 1-arg fn is rejected ──────────────────────────

#[test]
fn srt_3arg_rejects_1arg_fn() {
    let src = "abs1 x:n>n;abs x\nf c:n xs:L n>L n;srt abs1 c xs";
    let err = run_err(src, "f");
    assert!(
        err.contains("srt") && (err.contains("2 args") || err.contains("closure-bind")),
        "expected verifier error about srt fn arity, got: {err}"
    );
}

// ── verifier: 3-arg map with 1-arg fn is rejected ──────────────────────────

#[test]
fn map_3arg_rejects_1arg_fn() {
    let src = "abs1 x:n>n;abs x\nf c:n xs:L n>L n;map abs1 c xs";
    let err = run_err(src, "f");
    assert!(
        err.contains("map") && (err.contains("2 args") || err.contains("closure-bind")),
        "expected verifier error about map fn arity, got: {err}"
    );
}

// ── verifier: 4-arg fld with 2-arg fn is rejected ──────────────────────────

#[test]
fn fld_4arg_rejects_2arg_fn() {
    let src = "add a:n b:n>n;+a b\nf c:n xs:L n>n;fld add c xs 0";
    let err = run_err(src, "f");
    assert!(
        err.contains("fld") && (err.contains("3 args") || err.contains("closure-bind")),
        "expected verifier error about fld fn arity, got: {err}"
    );
}
