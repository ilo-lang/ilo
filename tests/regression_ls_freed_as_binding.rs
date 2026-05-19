// Regression: `ls` is no longer a reserved builtin name (renamed to `lsd` in
// 0.12.1) and must accept `ls=...` as a binding on every engine. Six rerun10
// persona reports (filesystem-walk, env-config, devops-sre, ecommerce-analytics,
// pdf-analyst, monorepo-analyst) all reached for `ls=rdl! p` as the natural
// local for "lines" and tripped ILO-P011 in 0.12.0 because `ls` was the
// directory-listing builtin. This test pins the rename so any future
// re-reservation surfaces immediately.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> Vec<&'static str> {
    if cfg!(feature = "cranelift") {
        vec!["--vm", "--jit"]
    } else {
        vec!["--vm"]
    }
}

fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "engine={engine} src={src:?}: stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// `ls` as an in-function binding (the exact persona shape, minus rdl!).
#[test]
fn ls_binding_in_fn_cross_engine() {
    let src = "main>n;ls=[\"x\",\"y\"];len ls";
    for engine in engines() {
        let out = run_ok(engine, src, "main");
        assert_eq!(out, "2", "{engine}: in-fn ls binding");
    }
}

// `ls` as a function parameter name.
#[test]
fn ls_param_cross_engine() {
    let src = "f ls:L t>n;len ls\nmain>n;f [\"a\",\"b\",\"c\",\"d\"]";
    for engine in engines() {
        let out = run_ok(engine, src, "main");
        assert_eq!(out, "4", "{engine}: ls as fn param");
    }
}

// The new `lsd` name still resolves to the directory-listing builtin.
#[test]
fn lsd_resolves_to_directory_listing() {
    // Probe by reading the crate's `examples/` directory which is part of the
    // checkout. We only assert that the call succeeds and returns a non-empty
    // list — the full surface is covered by regression_fs_builtins.rs.
    let src = "main>R (L t) t;lsd \"examples\"";
    for engine in engines() {
        let out = run_ok(engine, src, "main");
        assert!(
            out.starts_with('[') && out.len() > 2,
            "{engine}: expected non-empty list from lsd, got {out:?}"
        );
    }
}
