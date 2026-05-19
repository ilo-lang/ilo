// Regression tests for the `mpairs` builtin.
//
// `mpairs m > L (L _)` returns a sorted-by-key list of [k, v] 2-element
// lists, satisfying the invariant `mpairs m == zip (mkeys m) (mvals m)`.
//
// These tests cross-cut tree, VM and JIT to lock the invariant in place
// across every engine. The opcode lives at OP_MPAIRS = 184; the JIT
// path goes through `jit_mpairs`; the tree path lives in the Mpairs
// arm of the interpreter. Any drift between engines should fail here
// rather than surface as a silent value mismatch in user code.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("ilo_mpairs_{name}_{}_{n}.ilo", std::process::id()));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg(engine).arg(entry);
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_all(src: &str, entry: &str, expected: &str) {
    // --run-tree was removed from the public CLI in 0.12.1; the VM
    // bridge re-enters the tree-walker transparently for shapes the
    // VM hasn't lifted natively, so --run-vm exercises both engines.
    let engines: &[&str] = if cfg!(feature = "cranelift") {
        &["--run-vm", "--jit"]
    } else {
        &["--run-vm"]
    };
    for engine in engines {
        let actual = run_ok(engine, src, entry);
        assert_eq!(
            actual, expected,
            "engine {engine} produced {actual:?}, expected {expected:?} for src `{src}`"
        );
    }
}

// ── Basic shape: text-keyed map, returns sorted [k, v] pairs ────────────

const MPAIRS_BASIC: &str = "f>L (L _)\n  m = mset (mset (mset mmap \"b\" 2) \"a\" 1) \"c\" 3\n  mpairs m\n";

#[test]
fn mpairs_basic_sorted_pairs() {
    run_all(MPAIRS_BASIC, "f", "[[a, 1], [b, 2], [c, 3]]");
}

// ── Invariant: mpairs m == zip (mkeys m) (mvals m) ──────────────────────
//
// Encoded as a literal projection: we serialise the parallel
// `zip (mkeys m) (mvals m)` shape via mpairs and compare against the
// equivalent zip on the same map. Different keys + multiple keys with
// different cardinalities to keep the sort path honest.

const MPAIRS_INVARIANT: &str = "f>L (L _)\n  m = mset (mset (mset (mset mmap \"q\" 4) \"a\" 1) \"m\" 3) \"b\" 2\n  mpairs m\n";

#[test]
fn mpairs_invariant_matches_mkeys_mvals_output() {
    // Same keys (a, b, m, q) in sorted order; same values aligned. If
    // mpairs ever drifts from the (mkeys, mvals) sort it changes here.
    run_all(MPAIRS_INVARIANT, "f", "[[a, 1], [b, 2], [m, 3], [q, 4]]");
}

const MPAIRS_ZIP_EQUIV: &str = "f>L (L _)\n  m = mset (mset (mset (mset mmap \"q\" 4) \"a\" 1) \"m\" 3) \"b\" 2\n  zip (mkeys m) (mvals m)\n";

#[test]
fn mpairs_invariant_zip_form_matches() {
    // The zip-form output must be identical to MPAIRS_INVARIANT above.
    // Locks the invariant `mpairs m == zip (mkeys m) (mvals m)` at the
    // engine output level.
    run_all(MPAIRS_ZIP_EQUIV, "f", "[[a, 1], [b, 2], [m, 3], [q, 4]]");
}

// ── Empty map returns an empty list ─────────────────────────────────────

const MPAIRS_EMPTY: &str = "f>L (L _)\n  mpairs mmap\n";

#[test]
fn mpairs_empty_map() {
    run_all(MPAIRS_EMPTY, "f", "[]");
}

// ── Single entry ────────────────────────────────────────────────────────

const MPAIRS_SINGLE: &str = "f>L (L _)\n  mpairs (mset mmap \"k\" 42)\n";

#[test]
fn mpairs_single_entry() {
    run_all(MPAIRS_SINGLE, "f", "[[k, 42]]");
}

// ── Numeric keys: sort order is numeric, not lexicographic ──────────────
//
// MapKey's Ord puts numbers ahead of text and sorts numerics numerically,
// so 2 < 10 here. A naive string sort would put "10" before "2".

const MPAIRS_NUMERIC_KEYS: &str = "f>L (L _)\n  m = mset (mset (mset mmap 10 \"a\") 2 \"b\") 5 \"c\"\n  mpairs m\n";

#[test]
fn mpairs_numeric_keys_sort_numerically() {
    run_all(MPAIRS_NUMERIC_KEYS, "f", "[[2, b], [5, c], [10, a]]");
}

// ── After mdel: removed key is absent from pairs ────────────────────────

const MPAIRS_AFTER_MDEL: &str = "f>L (L _)\n  m = mset (mset (mset mmap \"a\" 1) \"b\" 2) \"c\" 3\n  mpairs (mdel m \"b\")\n";

#[test]
fn mpairs_after_mdel_drops_entry() {
    run_all(MPAIRS_AFTER_MDEL, "f", "[[a, 1], [c, 3]]");
}
