// Integration tests: runs all *.ilo files in examples/ that have
// -- run: / -- out: annotations and asserts the output matches.
//
// Annotation format (anywhere in the file, usually at the bottom):
//   -- run: [func] [args...]   <- args to pass after the filename
//   -- run: fac 10
//   -- out: 3628800
//
// Multiple run/out pairs per file are supported; they are matched in order.

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

struct TestCase {
    run_args: Vec<String>,
    expected: String,
    line: usize,
}

fn parse_cases(src: &str) -> Vec<TestCase> {
    let mut cases = Vec::new();
    let mut pending: Option<(Vec<String>, usize)> = None;

    for (i, raw) in src.lines().enumerate() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("-- run:") {
            let args = rest.split_whitespace().map(str::to_string).collect();
            pending = Some((args, i + 1));
        } else if let Some(rest) = line.strip_prefix("-- out:") {
            if let Some((args, ln)) = pending.take() {
                cases.push(TestCase { run_args: args, expected: rest.trim().to_string(), line: ln });
            }
        }
    }
    cases
}

#[test]
fn examples() {
    let files = find_examples();
    assert!(!files.is_empty(), "no .ilo files found in examples/");

    let mut total = 0;
    let mut failures: Vec<String> = Vec::new();

    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {name}: {e}"));
        let cases = parse_cases(&src);

        for case in &cases {
            total += 1;
            let out = ilo()
                .arg(path)
                .args(&case.run_args)
                .output()
                .unwrap_or_else(|e| panic!("failed to run ilo for {name}: {e}"));

            let actual = String::from_utf8_lossy(&out.stdout).trim().to_string();

            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                failures.push(format!(
                    "{name} (line {}): `ilo {} {}`\n  FAILED (exit {})\n  stderr: {stderr}",
                    case.line,
                    path.display(),
                    case.run_args.join(" "),
                    out.status,
                ));
            } else if actual != case.expected {
                failures.push(format!(
                    "{name} (line {}): `ilo {} {}`\n  expected: {:?}\n  actual:   {:?}",
                    case.line,
                    path.display(),
                    case.run_args.join(" "),
                    case.expected,
                    actual,
                ));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "{}/{} example test(s) failed:\n\n{}",
            failures.len(),
            total,
            failures.join("\n\n")
        );
    }

    println!("{total} example tests passed across {} files", files.len());
}
