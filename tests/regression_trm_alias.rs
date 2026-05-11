// Regression tests pinning the `trm` / `trim` alias contract.
//
// ilo's convention: builtins have a short canonical name (`trm`) and an
// optional long-form alias (`trim`). Both must resolve to the same builtin
// across every engine. An academic reviewer reported that `trm` was rejected
// while `trim` worked. The current behaviour is correct (both resolve), and
// these tests lock it in so the canonical short form cannot silently regress.

use ilo::ast::resolve_alias;
use ilo::builtins::Builtin;
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

const TRM_SRC: &str = "f s:t>t;trm s";
const TRIM_SRC: &str = "f s:t>t;trim s";

#[test]
fn trm_canonical_resolves_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, TRM_SRC, "f", "  hello  ");
        assert_eq!(out, "hello", "{engine}: trm should strip whitespace");
    }
}

#[test]
fn trim_long_alias_resolves_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, TRIM_SRC, "f", "  hello  ");
        assert_eq!(out, "hello", "{engine}: trim alias should strip whitespace");
    }
}

#[test]
fn trm_and_trim_produce_identical_output_cross_engine() {
    let inputs = ["  hello  ", "no-ws", "\t\tspaced\n", "   ", ""];
    for engine in ENGINES_ALL {
        for input in &inputs {
            let a = run(engine, TRM_SRC, "f", input);
            let b = run(engine, TRIM_SRC, "f", input);
            assert_eq!(a, b, "{engine}: trm vs trim diverged on {input:?}");
        }
    }
}

#[test]
fn trm_round_trip_canonical_name() {
    let b = Builtin::from_name("trm").expect("`trm` must be a canonical builtin");
    assert_eq!(b.name(), "trm", "canonical short form is `trm`");
}

#[test]
fn trim_is_only_an_alias_not_a_canonical() {
    // The long form must NOT be a canonical entry in `from_name` — only the
    // short form is canonical. Long forms go through `resolve_alias` first.
    assert!(
        Builtin::from_name("trim").is_none(),
        "`trim` must not be a canonical name; it is a long-form alias"
    );
    assert_eq!(
        resolve_alias("trim"),
        Some("trm"),
        "`trim` must resolve to canonical `trm`"
    );
}
