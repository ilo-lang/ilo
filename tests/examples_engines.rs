// Multi-engine integration tests: runs all *.ilo examples that have
// -- run: / -- out: annotations through every available engine and
// asserts that each engine produces the same output.
//
// Supported engines tested here:
//   --run-tree   Tree-walking interpreter
//   --run-vm     Register VM
//
// Per-example skip annotations (anywhere in the file):
//   -- engine-skip: vm     Skip the VM engine for this example
//   -- engine-skip: tree   Skip the interpreter for this example

use std::path::PathBuf;
use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn find_examples() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read examples/ at {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "ilo").unwrap_or(false))
        .collect();
    paths.sort();
    paths
}

#[derive(Clone, Copy, PartialEq)]
enum Expect {
    /// `-- out:` — program must exit 0 and the assertion is against stdout.
    Stdout,
    /// `-- err:` — program must exit 1 with the assertion against stderr.
    /// Used for entry functions that return `Value::Err(_)`.
    Stderr,
}

struct TestCase {
    run_args: Vec<String>,
    expected: String,
    line: usize,
    expect: Expect,
}

fn parse_cases(src: &str) -> Vec<TestCase> {
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

/// Parse `-- engine-skip: X` annotations from the source.
/// Returns a set of engine names to skip ("jit", "vm", "tree").
fn parse_engine_skips(src: &str) -> std::collections::HashSet<String> {
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

#[derive(Debug, Clone, Copy)]
struct Engine {
    name: &'static str,
    flag: &'static str,
}

fn engines() -> Vec<Engine> {
    vec![
        Engine {
            name: "tree",
            flag: "--run-tree",
        },
        Engine {
            name: "vm",
            flag: "--run-vm",
        },
    ]
}

#[test]
fn examples_all_engines() {
    let files = find_examples();
    assert!(!files.is_empty(), "no .ilo files found in examples/");

    let all_engines = engines();
    let mut total = 0;
    let mut failures: Vec<String> = Vec::new();

    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let src =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {name}: {e}"));
        let cases = parse_cases(&src);

        if cases.is_empty() {
            continue;
        }

        let skips = parse_engine_skips(&src);

        for engine in &all_engines {
            if skips.contains(engine.name) {
                continue;
            }

            for case in &cases {
                total += 1;

                let out = ilo()
                    .arg(path)
                    .arg(engine.flag)
                    .args(&case.run_args)
                    .output()
                    .unwrap_or_else(|e| {
                        panic!(
                            "failed to run ilo for {name} ({engine_name}): {e}",
                            engine_name = engine.name
                        )
                    });

                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

                match case.expect {
                    Expect::Stdout => {
                        if !out.status.success() {
                            failures.push(format!(
                                "{name} [{engine_name}] (line {}): `ilo {} {} {}`\n  FAILED (exit {})\n  stderr: {stderr}",
                                case.line,
                                path.display(),
                                engine.flag,
                                case.run_args.join(" "),
                                out.status,
                                engine_name = engine.name,
                            ));
                        } else if stdout != case.expected {
                            failures.push(format!(
                                "{name} [{engine_name}] (line {}): `ilo {} {} {}`\n  expected: {:?}\n  actual:   {:?}",
                                case.line,
                                path.display(),
                                engine.flag,
                                case.run_args.join(" "),
                                case.expected,
                                stdout,
                                engine_name = engine.name,
                            ));
                        }
                    }
                    Expect::Stderr => {
                        if out.status.success() {
                            failures.push(format!(
                                "{name} [{engine_name}] (line {}): `ilo {} {} {}`\n  EXPECTED FAILURE but exit 0\n  stdout: {stdout}",
                                case.line,
                                path.display(),
                                engine.flag,
                                case.run_args.join(" "),
                                engine_name = engine.name,
                            ));
                        } else if stderr != case.expected {
                            failures.push(format!(
                                "{name} [{engine_name}] (line {}): `ilo {} {} {}` (-- err:)\n  expected stderr: {:?}\n  actual stderr:   {:?}",
                                case.line,
                                path.display(),
                                engine.flag,
                                case.run_args.join(" "),
                                case.expected,
                                stderr,
                                engine_name = engine.name,
                            ));
                        }
                    }
                }
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "{}/{} multi-engine example test(s) failed:\n\n{}",
            failures.len(),
            total,
            failures.join("\n\n")
        );
    }

    println!(
        "{total} multi-engine example tests passed across {} files using {} engines",
        files.len(),
        all_engines.len()
    );
}
