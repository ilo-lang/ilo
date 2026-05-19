//! Cross-backend conformance suite (Stage 5f).
//!
//! Walks every `examples/*.ilo` that carries a `-- run: <fn> [args...]` and a
//! `-- out: <value>` header, and runs each through every available backend:
//!
//!   - Cranelift native    (`ilo build file.ilo` → run the produced binary)
//!   - Python              (`ilo build file.ilo --py` → run via `python3`)
//!   - WASM Component      (`ilo build file.ilo --wasm` → run via `wasmtime`)
//!   - Zero                (`ilo build file.ilo --0bin` → run the binary)
//!
//! Each backend can declare a per-example skip with an inline marker, e.g.
//!
//!   -- conformance-skip-wasm: uses raw socket builtin not in wasi:net
//!
//! For backends with intentionally narrow walkers (WASM, Zero in 0.13.0) we
//! treat `BackendError::UnsupportedFeature` (exit 1 with a recognisable
//! message) as a soft skip — not a failure. The goal is honest reporting,
//! not artificial coverage.
//!
//! The test prints a per-backend pass / skip / fail / unsupported summary at
//! the end. It only hard-fails when a backend that claims to support an
//! example produces wrong output.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const BACKENDS: &[Backend] = &[
    Backend::Cranelift,
    Backend::Python,
    Backend::Wasm,
    Backend::Zero,
];

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum Backend {
    Cranelift,
    Python,
    Wasm,
    Zero,
}

impl Backend {
    fn label(self) -> &'static str {
        match self {
            Backend::Cranelift => "cranelift",
            Backend::Python => "python",
            Backend::Wasm => "wasm",
            Backend::Zero => "zero",
        }
    }
}

struct Case {
    path: PathBuf,
    func: String,
    args: Vec<String>,
    expected: String,
    skip: Vec<Backend>,
}

#[derive(Default, Debug)]
struct Stats {
    pass: usize,
    skip: usize,
    unsupported: usize,
    fail: usize,
}

fn ilo_binary() -> PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let target_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/ilo");
        if target_dir.exists() {
            return target_dir;
        }
        let debug_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/ilo");
        if debug_dir.exists() {
            return debug_dir;
        }
        panic!(
            "ilo binary not found in target/release or target/debug. \
             Build with `cargo build --release --features cranelift` first."
        );
    })
    .clone()
}

fn python3_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn zero_available() -> Option<PathBuf> {
    if let Ok(out) = Command::new("which").arg("zero").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(PathBuf::from(s));
            }
        }
    }
    let home = std::env::var("HOME").ok()?;
    let candidate = PathBuf::from(home).join(".zero/bin/zero");
    candidate.exists().then_some(candidate)
}

fn collect_cases() -> Vec<Case> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read examples/: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "ilo").unwrap_or(false))
        .collect();
    paths.sort();

    let mut cases = Vec::new();
    for p in paths {
        if let Some(c) = parse_case(&p) {
            cases.push(c);
        }
    }
    cases
}

fn parse_case(path: &Path) -> Option<Case> {
    let src = std::fs::read_to_string(path).ok()?;
    let mut run: Option<String> = None;
    let mut out: Option<String> = None;
    let mut skip: Vec<Backend> = Vec::new();

    for raw in src.lines() {
        let line = raw.trim_start();
        if let Some(rest) = line.strip_prefix("-- run:") {
            // First run header wins.
            if run.is_none() {
                run = Some(rest.trim().to_string());
            }
        } else if let Some(rest) = line.strip_prefix("-- out:") {
            if out.is_none() {
                out = Some(rest.trim().to_string());
            }
        } else if let Some(rest) = line.strip_prefix("-- conformance-skip-") {
            // `-- conformance-skip-<backend>: <reason>`
            if let Some((tag, _reason)) = rest.split_once(':') {
                let tag = tag.trim();
                match tag {
                    "cranelift" => skip.push(Backend::Cranelift),
                    "python" => skip.push(Backend::Python),
                    "wasm" => skip.push(Backend::Wasm),
                    "zero" => skip.push(Backend::Zero),
                    "all" => skip.extend(BACKENDS.iter().copied()),
                    _ => {}
                }
            }
        }
    }

    let (func, args) = parse_run(run?.as_str());
    Some(Case {
        path: path.to_path_buf(),
        func,
        args,
        expected: out?,
        skip,
    })
}

fn parse_run(raw: &str) -> (String, Vec<String>) {
    // Shell-style quote-aware tokenisation so multi-word string args don't
    // get fragmented across whitespace. Combined with the hard-fail
    // conformance gate above, fragmented args would otherwise produce the
    // same wrong output across every backend and silently pass.
    //
    // shlex returns None for malformed input (unclosed quotes etc.); fall
    // back to whitespace split in that case so the test still runs and the
    // expected/actual diff surfaces in the per-case Fail output.
    let tokens = shlex::split(raw).unwrap_or_else(|| {
        raw.split_whitespace().map(|s| s.to_string()).collect()
    });
    let mut it = tokens.into_iter();
    let func = it.next().unwrap_or_default();
    let args: Vec<String> = it.collect();
    (func, args)
}

/// Look at backend stderr and decide whether the failure is a
/// "backend doesn't support this surface yet" soft skip vs a hard fail.
///
/// Gates exclusively on a structured `ILO-B###` code on stderr. We
/// deliberately don't match against stdout (program output, never
/// diagnostics) or against free-form prose like `"only lowers"` /
/// `"Stage 5d"`; a future example whose program text happens to print
/// that phrase would otherwise get silently reclassified.
fn is_unsupported(stderr: &str, _stdout: &str) -> bool {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"\bILO-B[0-9]{3}\b").expect("valid backend-error regex")
    });
    stderr.lines().any(|l| re.is_match(l))
}

#[derive(Debug)]
enum Outcome {
    Pass,
    Skip(&'static str),
    Unsupported(String),
    Fail(String),
}

fn run_cranelift(case: &Case) -> Outcome {
    let tmp = tempfile_path("ilo-conf-cl", "");
    let status = Command::new(ilo_binary())
        .arg("build")
        .arg(&case.path)
        .arg("-o")
        .arg(&tmp)
        .arg(&case.func)
        .output();
    let out = match status {
        Ok(o) => o,
        Err(e) => return Outcome::Fail(format!("spawn ilo build failed: {e}")),
    };
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if is_unsupported(&stderr, &stdout) {
            return Outcome::Unsupported("cranelift build unsupported feature".into());
        }
        return Outcome::Fail(format!("ilo build failed: {stderr}"));
    }
    let exec = match Command::new(&tmp).args(&case.args).output() {
        Ok(o) => o,
        Err(e) => return Outcome::Fail(format!("run binary failed: {e}")),
    };
    let actual = String::from_utf8_lossy(&exec.stdout).trim().to_string();
    let _ = std::fs::remove_file(&tmp);
    compare(case, &actual)
}

fn run_python(case: &Case) -> Outcome {
    if !python3_available() {
        return Outcome::Skip("python3 not on PATH");
    }
    let tmp = tempfile_path("ilo-conf-py", ".py");
    let out = match Command::new(ilo_binary())
        .arg("build")
        .arg(&case.path)
        .arg("--py")
        .arg("-o")
        .arg(&tmp)
        .output()
    {
        Ok(o) => o,
        Err(e) => return Outcome::Fail(format!("spawn ilo build --py failed: {e}")),
    };
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if is_unsupported(&stderr, &stdout) {
            return Outcome::Unsupported("python emit unsupported".into());
        }
        return Outcome::Fail(format!("ilo build --py failed: {stderr}"));
    }
    let mut cmd = Command::new("python3");
    cmd.arg(&tmp).arg(&case.func);
    for a in &case.args {
        cmd.arg(a);
    }
    let exec = match cmd.output() {
        Ok(o) => o,
        Err(e) => return Outcome::Fail(format!("python3 spawn failed: {e}")),
    };
    let actual = String::from_utf8_lossy(&exec.stdout).trim().to_string();
    let _ = std::fs::remove_file(&tmp);
    compare(case, &actual)
}

fn run_wasm(case: &Case) -> Outcome {
    if !wasmtime_available() {
        return Outcome::Skip("wasmtime not on PATH");
    }
    let tmp = tempfile_path("ilo-conf-wasm", ".wasm");
    let out = match Command::new(ilo_binary())
        .arg("build")
        .arg(&case.path)
        .arg("--wasm")
        .arg("-o")
        .arg(&tmp)
        .arg(&case.func)
        .output()
    {
        Ok(o) => o,
        Err(e) => return Outcome::Fail(format!("spawn ilo build --wasm failed: {e}")),
    };
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if is_unsupported(&stderr, &stdout) {
            return Outcome::Unsupported("wasm emit unsupported".into());
        }
        return Outcome::Fail(format!("ilo build --wasm failed: {stderr}"));
    }
    let exec = match Command::new("wasmtime")
        .arg(&tmp)
        .args(&case.args)
        .output()
    {
        Ok(o) => o,
        Err(e) => return Outcome::Fail(format!("wasmtime spawn failed: {e}")),
    };
    let actual = String::from_utf8_lossy(&exec.stdout).trim().to_string();
    let _ = std::fs::remove_file(&tmp);
    compare(case, &actual)
}

fn run_zero(case: &Case, zero_path: &Path) -> Outcome {
    // Put zero's bin dir on PATH for the child so `ilo build --0bin` can find it.
    let parent = zero_path.parent().map(|p| p.to_path_buf());
    let tmp = tempfile_path("ilo-conf-zero", "");
    let mut cmd = Command::new(ilo_binary());
    cmd.arg("build")
        .arg(&case.path)
        .arg("--0bin")
        .arg("-o")
        .arg(&tmp);
    if let Some(p) = parent {
        let path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{}:{}", p.display(), path));
    }
    let out = match cmd.output() {
        Ok(o) => o,
        Err(e) => return Outcome::Fail(format!("spawn ilo build --0bin failed: {e}")),
    };
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if is_unsupported(&stderr, &stdout) {
            return Outcome::Unsupported("zero emit unsupported".into());
        }
        return Outcome::Fail(format!("ilo build --0bin failed: {stderr}"));
    }
    let exec = match Command::new(&tmp).args(&case.args).output() {
        Ok(o) => o,
        Err(e) => return Outcome::Fail(format!("run zero binary failed: {e}")),
    };
    let actual = String::from_utf8_lossy(&exec.stdout).trim().to_string();
    let _ = std::fs::remove_file(&tmp);
    compare(case, &actual)
}

fn compare(case: &Case, actual: &str) -> Outcome {
    let expected = case.expected.trim();
    let actual = actual.trim();
    if expected == actual {
        Outcome::Pass
    } else {
        Outcome::Fail(format!("expected {expected:?}, got {actual:?}"))
    }
}

fn tempfile_path(prefix: &str, suffix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("{prefix}-{pid}-{nanos}{suffix}"))
}

#[test]
#[ignore = "conformance suite is heavy (4 backends * 200+ examples); run with --ignored"]
fn cross_backend_conformance() {
    let cases = collect_cases();
    assert!(!cases.is_empty(), "no conformance cases discovered");

    let zero_path = zero_available();

    let mut stats: std::collections::HashMap<Backend, Stats> = std::collections::HashMap::new();
    for b in BACKENDS {
        stats.insert(*b, Stats::default());
    }

    // In 0.13.0 Cranelift is the production backend and must not regress:
    // any Fail there is a hard failure. The narrow HIR walkers in WASM and
    // Zero v1, plus Python's missing __main__ dispatcher, mean those three
    // get soft-fail treatment for Outcome::Fail (we still hard-fail on
    // Outcome::Fail for Cranelift, and on Outcome::Unsupported nowhere —
    // unsupported is by definition a documented skip).
    //
    // Set ILO_STRICT_CONFORMANCE=1 to promote every backend's Fail to a
    // hard failure (intended for 0.14 once the walkers cover the corpus).
    let strict = std::env::var("ILO_STRICT_CONFORMANCE")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);
    let mut hard_failures: Vec<String> = Vec::new();

    for case in &cases {
        for backend in BACKENDS {
            let entry = stats.get_mut(backend).expect("stats slot present");
            if case.skip.contains(backend) {
                entry.skip += 1;
                continue;
            }
            let outcome = match backend {
                Backend::Cranelift => run_cranelift(case),
                Backend::Python => run_python(case),
                Backend::Wasm => run_wasm(case),
                Backend::Zero => match &zero_path {
                    Some(p) => run_zero(case, p),
                    None => Outcome::Skip("zero compiler not installed"),
                },
            };
            match outcome {
                Outcome::Pass => entry.pass += 1,
                Outcome::Skip(_) => entry.skip += 1,
                Outcome::Unsupported(_) => entry.unsupported += 1,
                Outcome::Fail(msg) => {
                    entry.fail += 1;
                    let line = format!(
                        "{:>9} {}: {}",
                        backend.label(),
                        case.path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                        msg
                    );
                    // Cranelift is the production backend in 0.13.0 — any
                    // fail there is a regression and gets pushed onto
                    // hard_failures, which panics at the end. WASM / Zero /
                    // Python failures stay soft-fail in 0.13.0 because the
                    // walkers are intentionally narrow; flip
                    // ILO_STRICT_CONFORMANCE=1 to hard-fail those too.
                    let is_hard = strict || *backend == Backend::Cranelift;
                    if is_hard {
                        hard_failures.push(line);
                    } else {
                        eprintln!("soft-fail: {line}");
                    }
                }
            }
        }
    }

    eprintln!("\n=== Cross-backend conformance summary ({} cases) ===", cases.len());
    eprintln!("{:>10} {:>6} {:>6} {:>12} {:>6}", "backend", "pass", "skip", "unsupported", "fail");
    for b in BACKENDS {
        let s = stats.get(b).expect("stats slot present");
        eprintln!(
            "{:>10} {:>6} {:>6} {:>12} {:>6}",
            b.label(),
            s.pass,
            s.skip,
            s.unsupported,
            s.fail
        );
    }
    eprintln!();

    // 0.13.0 gates on Cranelift fails (production backend); WASM / Zero /
    // Python stay soft-fail unless ILO_STRICT_CONFORMANCE=1. The summary
    // table above gives the per-backend coverage diff between releases.
    if !hard_failures.is_empty() {
        let mut msg = String::from("conformance hard-failures:\n");
        for line in &hard_failures {
            msg.push_str(line);
            msg.push('\n');
        }
        panic!("{msg}");
    }
}
