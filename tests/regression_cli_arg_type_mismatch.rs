// Regression: ILO-517 — a CLI argument that doesn't match its declared
// parameter type used to be bound as-is, so a `n:n` parameter could receive
// `Value::Text`. Arithmetic on it produced `NaN` (or, on the JIT, echoed the
// raw string) and the process still exited 0 — a silent wrong answer.
//
// Before:
//   ilo prog.ilo main   (single fn `tri n:n>n`)  ->  "NaN",  rc=0   default/VM
//                                                ->  "main", rc=0   JIT
// After:
//   ->  ILO-R600 "argument 1 (`n`) expects n, got text `main`", rc=1
//       on every engine.
//
// The CLI was less safe than the language it fronts: in-band, `num "main"`
// returns `R n t`, so the type checker forces the failure to be handled.
//
// Cross-engine on purpose. The guard is wired into all four dispatch sites
// (VM, interpreter, JIT, default) so the error contract can't drift per
// engine the way ILO-177 did.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(args: &[&str]) -> (bool, String, String) {
    let out = ilo()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn write_temp(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("prog.ilo");
    std::fs::write(&path, content).expect("write temp ilo");
    (dir, path)
}

/// Engine dispatch variants. Empty slice = default engine.
fn engines() -> Vec<&'static [&'static str]> {
    vec![&[], &["--vm"], &["--jit"]]
}

const TRI: &str = "tri n:n>n;s=+n 1;p=*n s;/p 2\nmain>n;tri 3\n";

#[test]
fn text_bound_to_number_param_is_rejected_all_engines() {
    let (_d, path) = write_temp(TRI);
    let p = path.to_str().unwrap();
    for eng in engines() {
        let mut args: Vec<&str> = eng.to_vec();
        args.extend_from_slice(&[p, "tri", "main"]);
        let (ok, out, err) = run(&args);
        assert!(
            !ok,
            "engine {eng:?}: expected failure, got success. out={out}"
        );
        assert!(
            err.contains("ILO-R600"),
            "engine {eng:?}: expected ILO-R600, stderr={err}"
        );
        // The silent-corruption symptoms must be gone.
        assert!(
            !out.contains("NaN"),
            "engine {eng:?}: NaN leaked to stdout: {out}"
        );
        assert!(
            !out.trim().eq("main"),
            "engine {eng:?}: raw arg echoed as result: {out}"
        );
    }
}

#[test]
fn valid_number_arg_still_runs_all_engines() {
    let (_d, path) = write_temp(TRI);
    let p = path.to_str().unwrap();
    for eng in engines() {
        let mut args: Vec<&str> = eng.to_vec();
        args.extend_from_slice(&[p, "tri", "10"]);
        let (ok, out, err) = run(&args);
        assert!(ok, "engine {eng:?}: expected success, stderr={err}");
        assert_eq!(out.trim(), "55", "engine {eng:?}");
    }
}

// The ILO-182 single-fn pass-through must survive: passing a bare ident to a
// sole `s:t` parameter is legitimate usage, not a typo'd function name. A
// dispatch-level fix would have broken this; the coercion-level fix must not.
#[test]
fn bare_ident_to_text_param_still_works() {
    let (_d, path) = write_temp("greet s:t>t;+\"hello \" s\n");
    let (ok, out, err) = run(&[path.to_str().unwrap(), "world"]);
    assert!(ok, "expected success, stderr={err}");
    assert_eq!(out.trim(), "hello world");
}

#[test]
fn non_bool_bound_to_bool_param_is_rejected() {
    let (_d, path) = write_temp("f b:b>t;\"ok\"\n");
    let (ok, _out, err) = run(&[path.to_str().unwrap(), "notabool"]);
    assert!(!ok, "expected failure");
    assert!(err.contains("ILO-R600"), "stderr={err}");
}

#[test]
fn bool_param_accepts_true_and_false() {
    let (_d, path) = write_temp("f b:b>t;\"ok\"\n");
    for v in ["true", "false"] {
        let (ok, out, err) = run(&[path.to_str().unwrap(), v]);
        assert!(ok, "value {v}: expected success, stderr={err}");
        assert_eq!(out.trim(), "ok", "value {v}");
    }
}

// `_` means "don't care" — the guard must wave anything through rather than
// re-implementing the type checker at the CLI boundary.
#[test]
fn any_param_accepts_anything() {
    let (_d, path) = write_temp("f x:_>t;\"got\"\n");
    for v in ["main", "42", "true", "nil"] {
        let (ok, out, err) = run(&[path.to_str().unwrap(), v]);
        assert!(ok, "value {v}: expected success, stderr={err}");
        assert_eq!(out.trim(), "got", "value {v}");
    }
}

// `O n` accepts nil (that's the point of an optional) but must still reject a
// non-nil value that can't be the inner type.
#[test]
fn optional_number_accepts_nil_rejects_text() {
    let (_d, path) = write_temp("f x:O n>t;\"ok\"\n");
    let p = path.to_str().unwrap();

    let (ok, out, err) = run(&[p, "nil"]);
    assert!(ok, "nil should be accepted, stderr={err}");
    assert_eq!(out.trim(), "ok");

    let (ok, _out, err) = run(&[p, "main"]);
    assert!(!ok, "text should be rejected for O n");
    assert!(err.contains("ILO-R600"), "stderr={err}");
}

// The diagnostic has to name the parameter and show the offending value,
// otherwise a repair loop gets no more signal than the old bare "NaN" did.
#[test]
fn diagnostic_names_parameter_and_value() {
    let (_d, path) = write_temp(TRI);
    let (_ok, _out, err) = run(&[path.to_str().unwrap(), "tri", "main"]);
    assert!(err.contains("argument 1"), "stderr={err}");
    assert!(err.contains('n'), "should name the param, stderr={err}");
    assert!(err.contains("main"), "should show the value, stderr={err}");
}
