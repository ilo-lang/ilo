// Cross-engine regression test: every emitted error code lands in its
// documented namespace range.
//
// This is the enforcement arm of the `ILO-XYZNN` namespace allocation
// documented at the top of `src/diagnostic/registry.rs`. If a new
// diagnostic ships without being slotted into a documented range, this
// test fails — agents and tools relying on prefix-based routing stay
// reliable.
//
// Two layers of coverage:
//
// 1. Static: every entry in REGISTRY (the explainer database) must live
//    in a documented range. Caught by a unit test in registry.rs.
//
// 2. Dynamic, cross-engine: spawn the ilo CLI on a battery of snippets
//    designed to provoke each known diagnostic, parse every `ILO-XYZNN`
//    code that appears on stderr, and assert each one is in a documented
//    range. Run under every available engine selector (default / tree /
//    VM / JIT) so the namespace contract holds regardless of which
//    backend raised the error.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run an inline snippet under a specific engine flag (or none) and return
/// the combined stderr.
fn run_with_engine(code: &str, engine_flag: Option<&str>) -> String {
    let mut cmd = ilo();
    if let Some(flag) = engine_flag {
        cmd.arg(flag);
    }
    cmd.arg(code);
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    let mut s = String::from_utf8_lossy(&out.stderr).to_string();
    s.push_str(&String::from_utf8_lossy(&out.stdout));
    s
}

/// Extract all `ILO-<letter><digits>` codes from a stderr blob.
fn extract_codes(blob: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = blob.as_bytes();
    let mut i = 0;
    while i + 5 < bytes.len() {
        if &bytes[i..i + 4] == b"ILO-" {
            let letter = bytes[i + 4];
            if letter.is_ascii_uppercase() {
                let mut j = i + 5;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > i + 5 {
                    out.push(String::from_utf8_lossy(&bytes[i..j]).to_string());
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Snippets known to emit at least one diagnostic. The point is breadth
/// across namespaces, not exhaustive per-code coverage — coverage is in
/// `tests/errors.rs`. Each snippet is run under every engine selector.
fn diagnostic_snippets() -> Vec<&'static str> {
    vec![
        // L001 — unexpected character
        "f x:n>n;$x",
        // L002 — underscore in identifier
        "my_func x:n>n;x",
        // L003 — uppercase identifier
        "MyFunc x:n>n;x",
        // P001 — unexpected token at top level
        "= 1 2",
        // P020 — incomplete function header
        "f1 a:n>n;+a 1 f2 a:n",
        // T004 — undefined variable
        "f x:n>n;+x y",
        // T005 — undefined function
        "f x:n>n;no-such x",
        // T006 — arity mismatch
        "g a:n b:n>n;+a b f x:n>n;g x",
        // T007 — type mismatch at call site
        "g x:n>n;*x 2 f s:t>n;g s",
        // T008 — return type mismatch
        r#"f x:n>n;"hello""#,
        // T024 — non-exhaustive match
        "f r:R n t>n;?r{~v:v}",
        // R003 — division by zero (runtime)
        "f>n;/1 0",
    ]
}

/// Engine selectors to exercise. `None` is the default runner.
fn engines() -> &'static [Option<&'static str>] {
    // --jit is opt-in and only relevant where the cranelift feature is on;
    // include it under that feature gate.
    #[cfg(feature = "cranelift")]
    {
        &[None, Some("--vm"), Some("--vm"), Some("--jit")]
    }
    #[cfg(not(feature = "cranelift"))]
    {
        &[None, Some("--vm"), Some("--vm")]
    }
}

#[test]
fn every_emitted_code_is_in_a_documented_namespace() {
    use ilo::diagnostic::registry::in_documented_range;

    let mut total_codes = 0usize;
    for snippet in diagnostic_snippets() {
        for engine in engines() {
            let blob = run_with_engine(snippet, *engine);
            for code in extract_codes(&blob) {
                total_codes += 1;
                assert!(
                    in_documented_range(&code),
                    "emitted code {code} (snippet {snippet:?}, engine {engine:?}) \
                     is not in any documented namespace range — update \
                     NAMESPACE_RANGES in src/diagnostic/registry.rs"
                );
            }
        }
    }
    assert!(
        total_codes > 0,
        "no error codes were observed — the snippets are probably wrong"
    );
}

#[test]
fn explain_resolves_every_registry_code() {
    // Every registry entry must be reachable via `ilo explain`. This is the
    // user-facing contract — if a code emits and `explain` can't find it,
    // the agent loop breaks.
    use ilo::diagnostic::registry::REGISTRY;
    for entry in REGISTRY {
        let out = ilo()
            .arg("explain")
            .arg(entry.code)
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
        assert!(
            out.status.success(),
            "ilo explain {} failed: stderr={}",
            entry.code,
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains(entry.code),
            "ilo explain {} did not echo the code in its output: {}",
            entry.code,
            stdout
        );
    }
}
