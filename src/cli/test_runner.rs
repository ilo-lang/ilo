//! `ilo test`. the user-facing test command.
//!
//! Surfaces the `-- run:` / `-- out:` / `-- err:` annotation format that the
//! in-tree `tests/examples_engines.rs` harness already uses, so end-users can
//! assert program behaviour from the same files agents are reading as
//! in-context learning examples.
//!
//! Discovery: a file path runs that file; a directory walks `*.ilo` files
//! recursively. The recursion is unbounded (unlike the integration harness,
//! which caps at one level) because end-user `tests/` trees may nest deeper
//! than the in-tree `examples/apps/<group>/file.ilo` shape.
//!
//! Execution: each case is run by spawning `current_exe` with `<file>
//! <engine_flag> <run-args...>`, exactly like the integration harness. This
//! avoids re-implementing parser + verifier + engine integration here and
//! gives `ilo test` the same exit-code / stdout / stderr semantics every
//! other `ilo` invocation has - which is the contract the assertions are
//! written against in the first place.

use crate::cli::args::{TestArgs, TestEngine};
use std::path::{Path, PathBuf};
use std::process::Command;

/// `-- out:` vs `-- err:` - which stream the expected payload is matched
/// against and which exit status counts as success.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Expect {
    /// `-- out:` - program must exit 0; assertion is against stdout.
    Stdout,
    /// `-- err:` - program must exit non-zero; assertion is against stderr.
    Stderr,
}

/// One `-- run:` / `-- out:` (or `-- err:`) pair parsed out of a source file.
#[derive(Debug, Clone)]
pub struct TestCase {
    /// Tokens passed after the file path: `[<func>] [<arg>...]`.
    pub run_args: Vec<String>,
    /// Trimmed expected payload (stdout for `-- out:`, stderr for `-- err:`).
    pub expected: String,
    /// 1-indexed line number of the originating `-- run:` annotation.
    pub line: usize,
    /// Stream the expected payload is matched against.
    pub expect: Expect,
}

/// Parse `-- run:` / `-- out:` / `-- err:` annotations out of an .ilo source.
///
/// A `-- run:` line opens a pending case; the very next `-- out:` or `-- err:`
/// line closes it. Anything between is ignored, matching the integration
/// harness's "loose pairing" semantics (so explanatory comments can sit
/// between the trigger and the assertion).
pub fn parse_cases(src: &str) -> Vec<TestCase> {
    let mut cases = Vec::new();
    let mut pending: Option<(Vec<String>, usize)> = None;

    for (i, raw) in src.lines().enumerate() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("-- run:") {
            let args = rest.split_whitespace().map(str::to_string).collect();
            pending = Some((args, i + 1));
        } else if let (Some(rest), Some((args, ln))) =
            (line.strip_prefix("-- out:"), pending.as_ref())
        {
            cases.push(TestCase {
                run_args: args.clone(),
                expected: rest.trim().to_string(),
                line: *ln,
                expect: Expect::Stdout,
            });
            pending = None;
        } else if let (Some(rest), Some((args, ln))) =
            (line.strip_prefix("-- err:"), pending.as_ref())
        {
            cases.push(TestCase {
                run_args: args.clone(),
                expected: rest.trim().to_string(),
                line: *ln,
                expect: Expect::Stderr,
            });
            pending = None;
        }
    }
    cases
}

/// Parse `-- engine-skip: <name>...` annotations. Multiple tokens per line
/// are accepted; names match `engine.name()` ("vm" / "jit").
pub fn parse_engine_skips(src: &str) -> std::collections::HashSet<String> {
    let mut skips = std::collections::HashSet::new();
    for raw in src.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("-- engine-skip:") {
            for token in rest.split_whitespace() {
                skips.insert(token.to_string());
            }
        }
    }
    skips
}

/// Engine descriptor - name shown in PASS/FAIL output and the CLI flag passed
/// through to the spawned `ilo` invocation.
#[derive(Clone, Copy, Debug)]
struct EngineDesc {
    name: &'static str,
    flag: &'static str,
}

const VM: EngineDesc = EngineDesc {
    name: "vm",
    flag: "--vm",
};
const JIT: EngineDesc = EngineDesc {
    name: "jit",
    flag: "--jit",
};

fn engines_for(sel: TestEngine) -> Vec<EngineDesc> {
    match sel {
        TestEngine::Vm => vec![VM],
        TestEngine::Jit => vec![JIT],
        TestEngine::All => vec![VM, JIT],
    }
}

/// Recursively collect `*.ilo` files under `dir`.
///
/// Unbounded recursion is fine for `ilo test`: a user's tests/ tree can nest
/// however deep they want, unlike the in-tree integration harness which
/// caps at one level for the `examples/apps/<group>/` layout.
fn collect_ilo_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            collect_ilo_recursive(&p, out)?;
        } else if p.extension().map(|e| e == "ilo").unwrap_or(false) {
            out.push(p);
        }
    }
    Ok(())
}

/// Resolve the user-supplied path arg into the list of files to test.
fn resolve_targets(path: &Path) -> Result<Vec<PathBuf>, String> {
    if !path.exists() {
        return Err(format!("no such file or directory: {}", path.display()));
    }
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut files = Vec::new();
    collect_ilo_recursive(path, &mut files)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    files.sort();
    Ok(files)
}

/// Entry point invoked from `main()` for `Cmd::Test`.
///
/// Returns the process exit code: 0 if every case passed, 1 if any failed
/// (or no cases were discovered, which is treated as an error rather than a
/// silent success - an `ilo test` invocation that finds nothing to do is
/// almost always a mistake worth surfacing).
pub fn run(args: TestArgs) -> i32 {
    let path_arg = args.path.unwrap_or_else(|| "examples".to_string());
    let path = PathBuf::from(&path_arg);

    let targets = match resolve_targets(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    let engines = engines_for(args.engine);

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot resolve current executable: {e}");
            return 1;
        }
    };

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut files_with_cases = 0usize;

    for file in &targets {
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read {}: {e}", file.display());
                failed += 1;
                continue;
            }
        };

        let cases = parse_cases(&src);
        if cases.is_empty() {
            continue;
        }
        files_with_cases += 1;

        let skips = parse_engine_skips(&src);

        for engine in &engines {
            if skips.contains(engine.name) {
                continue;
            }

            for case in &cases {
                // Label format mirrors `cargo test`: `<file>::<func> [engine]`.
                // The function label is the first run-arg when present (the
                // common case); when `-- run:` has no args we fall back to
                // `<default>` so the line still parses cleanly. Engine tag is
                // suppressed when only one engine is in play to keep single-
                // engine output uncluttered, matching `cargo test --quiet`'s
                // restraint.
                let func_label = case
                    .run_args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "<default>".to_string());
                let engine_tag = if engines.len() > 1 {
                    format!(" [{}]", engine.name)
                } else {
                    String::new()
                };
                let label = format!("{}::{}{engine_tag}", file.display(), func_label);

                let out = Command::new(&exe)
                    .arg(file)
                    .arg(engine.flag)
                    .args(&case.run_args)
                    .output();
                let out = match out {
                    Ok(o) => o,
                    Err(e) => {
                        println!("FAIL  {label} (line {}): spawn error: {e}", case.line);
                        failed += 1;
                        continue;
                    }
                };

                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                // Advisory `hint:` lines (e.g. the `.ilo` extension
                // deprecation) are not part of a program's error output;
                // matching on them would make every `-- err:` assertion
                // depend on the file extension it happens to be run from.
                let stderr = String::from_utf8_lossy(&out.stderr)
                    .lines()
                    .filter(|l| !l.trim_start().starts_with("hint:"))
                    .collect::<Vec<_>>()
                    .join("\n")
                    .trim()
                    .to_string();

                let (ok, detail) = match case.expect {
                    Expect::Stdout => {
                        if !out.status.success() {
                            (
                                false,
                                format!(
                                    "exit {} (expected 0), stderr: {stderr}",
                                    out.status.code().unwrap_or(-1)
                                ),
                            )
                        } else if stdout != case.expected {
                            (false, format!("got: {stdout:?}, want: {:?}", case.expected))
                        } else {
                            (true, String::new())
                        }
                    }
                    Expect::Stderr => {
                        if out.status.success() {
                            (
                                false,
                                format!("expected non-zero exit (got 0), stdout: {stdout}"),
                            )
                        } else if stderr != case.expected {
                            (
                                false,
                                format!("got stderr: {stderr:?}, want: {:?}", case.expected),
                            )
                        } else {
                            (true, String::new())
                        }
                    }
                };

                if ok {
                    println!("PASS  {label} (line {})", case.line);
                    passed += 1;
                } else {
                    println!("FAIL  {label} (line {}) ({detail})", case.line);
                    failed += 1;
                }
            }
        }
    }

    println!();
    println!("{passed} passed, {failed} failed");

    if files_with_cases == 0 && failed == 0 {
        eprintln!(
            "error: no `-- run:` / `-- out:` annotations found under {}",
            path.display()
        );
        return 1;
    }

    if failed > 0 { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cases_simple_pair() {
        let src = "-- run: main foo\n-- out: foo\n";
        let cases = parse_cases(src);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].run_args, vec!["main", "foo"]);
        assert_eq!(cases[0].expected, "foo");
        assert_eq!(cases[0].expect, Expect::Stdout);
        assert_eq!(cases[0].line, 1);
    }

    #[test]
    fn parse_cases_err_variant() {
        let src = "-- run: bad\n-- err: ILO-R001\n";
        let cases = parse_cases(src);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].expect, Expect::Stderr);
        assert_eq!(cases[0].expected, "ILO-R001");
    }

    #[test]
    fn parse_cases_multiple_with_comments_between() {
        let src = "-- run: a 1\n-- comment\n-- out: 2\n-- run: a 2\n-- out: 4\n";
        let cases = parse_cases(src);
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].expected, "2");
        assert_eq!(cases[1].expected, "4");
    }

    #[test]
    fn parse_cases_orphan_out_ignored() {
        // A `-- out:` without a preceding `-- run:` is silently dropped, same
        // as the integration harness. (No way to reasonably associate it.)
        let src = "-- out: orphan\n-- run: a\n-- out: real\n";
        let cases = parse_cases(src);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].expected, "real");
    }

    #[test]
    fn parse_engine_skips_basic() {
        let s = parse_engine_skips("-- engine-skip: vm jit\n");
        assert!(s.contains("vm"));
        assert!(s.contains("jit"));
    }

    #[test]
    fn engines_for_selects() {
        assert_eq!(engines_for(TestEngine::Vm).len(), 1);
        assert_eq!(engines_for(TestEngine::Jit).len(), 1);
        assert_eq!(engines_for(TestEngine::All).len(), 2);
    }

    #[test]
    fn collect_ilo_recursive_finds_nested() {
        let tmp = tempdir();
        let nested = tmp.join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(tmp.join("top.ilo"), "").unwrap();
        std::fs::write(nested.join("deep.ilo"), "").unwrap();
        std::fs::write(tmp.join("not.txt"), "").unwrap();

        let mut out = Vec::new();
        collect_ilo_recursive(&tmp, &mut out).unwrap();
        out.sort();
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|p| p.ends_with("top.ilo")));
        assert!(out.iter().any(|p| p.ends_with("deep.ilo")));

        std::fs::remove_dir_all(&tmp).ok();
    }

    fn tempdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("ilo-test-runner-{pid}-{nonce}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
