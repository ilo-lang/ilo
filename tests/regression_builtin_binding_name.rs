// Regression: binding a local variable whose name collides with a builtin
// must surface the friendly ILO-P011 reserved-name error at parse time,
// not silently accept the binding and later mis-dispatch the use site to
// the builtin (surfacing a misleading ILO-T006 arity error).
//
// Repro before the fix: `flat=cat ls " "` followed by `spl flat ". "`
// silently bound `flat` locally, but the call `spl flat ". "` parsed `flat`
// as a 0-arg call to the `flat` builtin (the verifier checks `is_builtin`
// before locals in operand position), producing
// `ILO-T006 arity mismatch: 'flat' expects 1 args, got 0`. The agent has no
// signal that the local binding is being shadowed.
//
// Mirrors the `parse_fn_decl` precedent from PR #245 (regression_builtin_fn_name.rs):
// reject builtin-named binding LHS at parse time on every engine.
//
// Originating persona report: 2026-05-16 pdf-analyst re-run against v0.11.2,
// friction #6 "`flat` is a builtin name and shadows your local".

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> (bool, String) {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// Top-level (`parse_decl`) binding form: `name=expr` outside any function.
fn check_top_level_binding(engine: &str, name: &str) {
    let src = format!("{name}=5\nmain>n;42");
    let (ok, stderr) = run(engine, &src, "main");
    assert!(
        !ok,
        "engine={engine} name={name}: expected parse failure for top-level `{name}=...`"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "engine={engine} name={name}: missing ILO-P011, stderr={stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "`{name}` is a builtin and cannot be used as a binding name"
        )),
        "engine={engine} name={name}: missing friendly message, stderr={stderr}"
    );
    assert!(
        stderr.contains("rename to something like"),
        "engine={engine} name={name}: missing rename hint, stderr={stderr}"
    );
}

// In-function (`parse_stmt`) binding form: `name=expr` inside a function body.
fn check_in_fn_binding(engine: &str, name: &str) {
    let src = format!("main>n;{name}=5;42");
    let (ok, stderr) = run(engine, &src, "main");
    assert!(
        !ok,
        "engine={engine} name={name}: expected parse failure for in-fn `{name}=...`"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "engine={engine} name={name}: missing ILO-P011, stderr={stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "`{name}` is a builtin and cannot be used as a binding name"
        )),
        "engine={engine} name={name}: missing friendly message, stderr={stderr}"
    );
    // The bug we're fixing: the misleading arity error from the shadowed
    // builtin must not be the first thing the agent sees.
    let p011_pos = stderr.find("ILO-P011").unwrap();
    if let Some(t006_pos) = stderr.find("ILO-T006") {
        assert!(
            p011_pos < t006_pos,
            "engine={engine} name={name}: ILO-P011 must come before any ILO-T006 cascade, stderr={stderr}"
        );
    }
}

// Builtin names a persona is likely to reach for as a local-binding name.
// Mix of the rerun3-cited `flat`, the historical `fld` (covered by an
// earlier specific message but should still surface ILO-P011), list/map
// builtins (`map`, `flt`, `frq`, `cat`, `len`), and short-name builtins
// (`hd`, `tl`, `at`, `ord`) that collide with natural single-letter
// abbreviations.
const BINDING_NAMES: &[&str] = &[
    "flat", "frq", "map", "flt", "cat", "len", "hd", "tl", "at", "ord", "srt", "sum",
];

#[test]
fn builtin_binding_rejected_in_fn_tree() {
    for name in BINDING_NAMES {
        check_in_fn_binding("--run-tree", name);
    }
}

#[test]
fn builtin_binding_rejected_in_fn_vm() {
    for name in BINDING_NAMES {
        check_in_fn_binding("--run-vm", name);
    }
}

#[test]
#[cfg(feature = "cranelift")]
fn builtin_binding_rejected_in_fn_cranelift() {
    for name in BINDING_NAMES {
        check_in_fn_binding("--run-cranelift", name);
    }
}

#[test]
fn builtin_binding_rejected_top_level_tree() {
    for name in BINDING_NAMES {
        check_top_level_binding("--run-tree", name);
    }
}

#[test]
fn builtin_binding_rejected_top_level_vm() {
    for name in BINDING_NAMES {
        check_top_level_binding("--run-vm", name);
    }
}

#[test]
#[cfg(feature = "cranelift")]
fn builtin_binding_rejected_top_level_cranelift() {
    for name in BINDING_NAMES {
        check_top_level_binding("--run-cranelift", name);
    }
}

// The exact pdf-analyst rerun3 repro: `flat=cat ls " "` then a use site.
// Before the fix this surfaced `ILO-T006 arity mismatch: 'flat' expects 1
// args, got 0` from the shadowed builtin, with no signal that the local
// `flat` binding was being silently ignored at the call site. After the fix
// the agent sees ILO-P011 immediately and renames.
const FLAT_REPRO: &str = "main>n;flat=5;flat";

fn check_flat_repro(engine: &str) {
    let (ok, stderr) = run(engine, FLAT_REPRO, "main");
    assert!(!ok, "engine={engine}: expected parse failure");
    assert!(
        stderr.contains("ILO-P011"),
        "engine={engine}: missing ILO-P011, stderr={stderr}"
    );
    assert!(
        stderr.contains("`flat` is a builtin"),
        "engine={engine}: missing friendly message, stderr={stderr}"
    );
    let p011_pos = stderr.find("ILO-P011").unwrap();
    if let Some(t006_pos) = stderr.find("ILO-T006") {
        assert!(
            p011_pos < t006_pos,
            "engine={engine}: ILO-P011 must come before ILO-T006, stderr={stderr}"
        );
    }
}

#[test]
fn flat_repro_tree() {
    check_flat_repro("--run-tree");
}

#[test]
fn flat_repro_vm() {
    check_flat_repro("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn flat_repro_cranelift() {
    check_flat_repro("--run-cranelift");
}

// Sanity: the more-specific `fld` message from the earlier fix still fires
// (and still mentions the fold builtin specifically). The generic builtin
// check runs after the per-name checks, so the friendlier message wins.
fn check_fld_keeps_specific_message(engine: &str) {
    let (ok, stderr) = run(engine, "main>n;fld=5;fld", "main");
    assert!(!ok, "engine={engine}: expected parse failure");
    assert!(
        stderr.contains("ILO-P011"),
        "engine={engine}: missing ILO-P011, stderr={stderr}"
    );
    assert!(
        stderr.contains("`fld` is reserved for the fold builtin"),
        "engine={engine}: expected fld-specific message, stderr={stderr}"
    );
}

#[test]
fn fld_specific_message_preserved_tree() {
    check_fld_keeps_specific_message("--run-tree");
}

#[test]
fn fld_specific_message_preserved_vm() {
    check_fld_keeps_specific_message("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn fld_specific_message_preserved_cranelift() {
    check_fld_keeps_specific_message("--run-cranelift");
}

// Sanity: renaming to a non-builtin name works on every engine. The hint
// the new ILO-P011 produces points to `myflat` / `flatv` style names, so
// that path must actually compile and run.
fn check_renamed_binding_works(engine: &str) {
    let out = ilo()
        .args(["main>n;myflat=5;myflat", engine, "main"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "engine={engine}: rename should compile, stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("5"),
        "engine={engine}: expected 5, got: {stdout}"
    );
}

#[test]
fn rename_workaround_binding_tree() {
    check_renamed_binding_works("--run-tree");
}

#[test]
fn rename_workaround_binding_vm() {
    check_renamed_binding_works("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn rename_workaround_binding_cranelift() {
    check_renamed_binding_works("--run-cranelift");
}
