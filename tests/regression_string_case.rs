// Cross-engine regression tests for the string-case builtins: `upr`, `lwr`, `cap`.
//
// Each case-conversion builtin must produce identical output across the
// tree-walking interpreter, the bytecode VM, and (when enabled) the Cranelift
// JIT. ASCII-only is fine for the first cut.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str, arg: &str) -> String {
    let out = ilo()
        .args([src, engine, entry, arg])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

const UPR_SRC: &str = "f s:t>t;upr s";
const LWR_SRC: &str = "f s:t>t;lwr s";
const CAP_SRC: &str = "f s:t>t;cap s";

#[test]
fn upr_basic_ascii_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, UPR_SRC, "f", "hello world");
        assert_eq!(out, "HELLO WORLD", "{engine}: upr basic ascii");
    }
}

#[test]
fn upr_already_upper_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, UPR_SRC, "f", "ALREADY");
        assert_eq!(out, "ALREADY", "{engine}: upr already-upper");
    }
}

#[test]
fn upr_mixed_case_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, UPR_SRC, "f", "MiXeD-CaSe");
        assert_eq!(out, "MIXED-CASE", "{engine}: upr mixed");
    }
}

#[test]
fn upr_empty_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, UPR_SRC, "f", "");
        assert_eq!(out, "", "{engine}: upr empty");
    }
}

#[test]
fn lwr_basic_ascii_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, LWR_SRC, "f", "HELLO WORLD");
        assert_eq!(out, "hello world", "{engine}: lwr basic ascii");
    }
}

#[test]
fn lwr_already_lower_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, LWR_SRC, "f", "already");
        assert_eq!(out, "already", "{engine}: lwr already-lower");
    }
}

#[test]
fn lwr_mixed_case_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, LWR_SRC, "f", "MiXeD-CaSe");
        assert_eq!(out, "mixed-case", "{engine}: lwr mixed");
    }
}

#[test]
fn lwr_empty_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, LWR_SRC, "f", "");
        assert_eq!(out, "", "{engine}: lwr empty");
    }
}

#[test]
fn cap_basic_ascii_cross_engine() {
    // `cap` uppercases the first character and leaves the rest unchanged.
    for engine in ENGINES_ALL {
        let out = run(engine, CAP_SRC, "f", "hello world");
        assert_eq!(out, "Hello world", "{engine}: cap basic ascii");
    }
}

#[test]
fn cap_already_capitalised_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, CAP_SRC, "f", "Hello");
        assert_eq!(out, "Hello", "{engine}: cap already-capitalised");
    }
}

#[test]
fn cap_mixed_preserves_tail_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, CAP_SRC, "f", "hELLO");
        assert_eq!(out, "HELLO", "{engine}: cap preserves rest");
    }
}

#[test]
fn cap_empty_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, CAP_SRC, "f", "");
        assert_eq!(out, "", "{engine}: cap empty");
    }
}

#[test]
fn cap_single_char_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, CAP_SRC, "f", "a");
        assert_eq!(out, "A", "{engine}: cap single char");
    }
}
