// Regression: defining a user function whose name collides with a builtin
// must surface the friendly ILO-P011 reserved-name error, not the misleading
// ILO-T006 arity mismatch that leaked when verify's call-dispatch resolved
// the builtin first and reported its signature.
//
// Repro before the fix: `lst>n;42` + `main>n;lst()` returned
// `ILO-T006 arity mismatch: 'lst' expects 3 args, got 0` (from the 3-arg
// list-constructor builtin), with no hint that the user function was being
// silently shadowed. Renaming to anything non-builtin fixed it.
//
// The fix lives in the parser, so all engines surface the same error before
// any engine-specific dispatch runs. This regression covers several builtin
// names that personas reach for as natural function names (`lst`, `hd`,
// `tl`, `map`, `cat`, `len`, `str`, `num`, `has`, `slc`) across all engines.

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

// A user function whose entire program is just that function. No caller, so
// no downstream verify cascade — the ILO-P011 should be the sole diagnostic.
fn check_single_decl(engine: &str, name: &str) {
    let src = format!("{name}>n;42");
    let (ok, stderr) = run(engine, &src, name);
    assert!(!ok, "engine={engine} name={name}: expected parse failure");
    assert!(
        stderr.contains("ILO-P011"),
        "engine={engine} name={name}: missing ILO-P011, stderr={stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "`{name}` is a builtin and cannot be used as a function name"
        )),
        "engine={engine} name={name}: missing friendly message, stderr={stderr}"
    );
    assert!(
        stderr.contains("rename to something like"),
        "engine={engine} name={name}: missing rename hint, stderr={stderr}"
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

// Builtin names a persona is likely to reach for as a function name. Mix of
// list (`lst`, `hd`, `tl`, `slc`, `cat`, `has`), string (`len`, `str`),
// arithmetic-ish (`num`), and HOF (`map`) — covers all the builtin shapes
// the dispatch path treats specially.
const BUILTIN_NAMES: &[&str] = &[
    "lst", "hd", "tl", "slc", "cat", "has", "len", "str", "num", "map",
];

#[test]
fn builtin_fn_name_rejected_tree() {
    for name in BUILTIN_NAMES {
        check_single_decl("--run-tree", name);
    }
}

#[test]
fn builtin_fn_name_rejected_vm() {
    for name in BUILTIN_NAMES {
        check_single_decl("--run-vm", name);
    }
}

#[test]
#[cfg(feature = "cranelift")]
fn builtin_fn_name_rejected_cranelift() {
    for name in BUILTIN_NAMES {
        check_single_decl("--run-cranelift", name);
    }
}

// The original repro: `lst` as fn name + a separate `main` that calls it.
// The downstream verify cascade against `lst()` is expected (the user fn
// never got registered), but ILO-P011 must be the headline diagnostic.
const LST_REPRO: &str = "lst>n;42\nmain>n;lst()";

fn check_lst_repro(engine: &str) {
    let (ok, stderr) = run(engine, LST_REPRO, "main");
    assert!(!ok, "engine={engine}: expected parse failure");
    assert!(
        stderr.contains("ILO-P011"),
        "engine={engine}: missing ILO-P011, stderr={stderr}"
    );
    assert!(
        stderr.contains("`lst` is a builtin"),
        "engine={engine}: missing friendly message, stderr={stderr}"
    );
    // ILO-P011 must precede any ILO-T006 cascade so the agent's first
    // action is the rename, not chasing the misleading arity error.
    let p011_pos = stderr.find("ILO-P011").unwrap();
    if let Some(t006_pos) = stderr.find("ILO-T006") {
        assert!(
            p011_pos < t006_pos,
            "engine={engine}: ILO-P011 must come before ILO-T006, stderr={stderr}"
        );
    }
}

#[test]
fn lst_repro_tree() {
    check_lst_repro("--run-tree");
}

#[test]
fn lst_repro_vm() {
    check_lst_repro("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn lst_repro_cranelift() {
    check_lst_repro("--run-cranelift");
}

// Sanity: renaming the function to something non-builtin works on every
// engine. This is the workaround the new ILO-P011 hint pushes the agent
// toward, so it must actually work.
fn check_renamed_works(engine: &str) {
    let out = ilo()
        .args(["show>n;42\nmain>n;show()", engine, "main"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "engine={engine}: rename should compile, stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("42"),
        "engine={engine}: expected 42, got: {stdout}"
    );
}

#[test]
fn rename_workaround_tree() {
    check_renamed_works("--run-tree");
}

#[test]
fn rename_workaround_vm() {
    check_renamed_works("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn rename_workaround_cranelift() {
    check_renamed_works("--run-cranelift");
}

// Sanity: the underlying builtins still work after the fix. If we
// accidentally broke builtin dispatch in service of the friendlier error,
// huge swaths of programs would silently regress.
#[test]
fn lst_builtin_still_works() {
    let out = ilo()
        .args(["main>L n;lst [1 2 3] 0 42", "--run-tree", "main"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "lst builtin broken: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}
