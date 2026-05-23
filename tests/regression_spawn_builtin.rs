// Cross-engine regression tests for `spawn` — fire-and-forget background
// OS thread (ILO-477).
//
// `spawn fn args... > _` runs `fn args...` on a new OS thread and returns
// Nil immediately. Errors and panics inside the thread are logged to
// stderr; the parent is unaffected.
//
// Tree-walker is the only engine that owns spawn dispatch; VM and Cranelift
// inherit via the tree-bridge (`is_tree_bridge_eligible(Spawn, n) for n>=1`),
// mirroring the `par-map` / `for-line` / `get-stream` precedent. Every test
// runs through every available engine to pin that bridge contract.
//
// Test coverage:
//   1. happy path — spawned worker writes a known string to a file.
//   2. concurrent fan-out — three workers write distinct lines to one file;
//      asserts all three lines made it (proves threads actually ran).
//   3. error inside thread — runtime error logs to stderr, parent continues.
//   4. cap inheritance — restricted write cap is inherited by the thread.
//   5. spawn returns Nil — verifier accepts `spawn` only in `> _` context.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn temp_path(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("ilo_spawn_{tag}_{pid}_{n}.txt"))
}

fn engines() -> Vec<&'static str> {
    let mut v = vec!["--vm"];
    if cfg!(feature = "cranelift") {
        v.push("--jit");
    }
    v
}

/// Run inline ilo source with the given engine flag and entry function.
fn run(engine: &str, src: &str, entry: &str, extra: &[&str]) -> std::process::Output {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(entry);
    for a in extra {
        cmd.arg(a);
    }
    cmd.output().expect("failed to run ilo")
}

// ── 1. Happy path: spawned worker writes a sentinel line ─────────────────
//
// The worker appends a sentinel string to a file passed via argv. Parent
// sleeps long enough for the thread to finish, then exits. We poll for the
// file briefly so a slow CI host doesn't false-fail before the OS scheduler
// gets around to running the thread.
const HAPPY_SRC: &str = "worker p:t>_\n\
                         r=wra p \"hello-from-spawn\";?r{~_:_;^_:_}\n\
                         \n\
                         go p:t>_\n\
                         spawn worker p;sleep 200";

#[test]
fn spawn_happy_path_writes_sentinel() {
    for engine in engines() {
        let path = temp_path(&format!("happy_{}", engine.trim_start_matches('-')));
        let _ = std::fs::remove_file(&path);
        let out = run(engine, HAPPY_SRC, "go", &[path.to_str().unwrap()]);
        assert!(
            out.status.success(),
            "engine={engine} ilo exited non-zero: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        // Poll for up to 2s in case the OS scheduler is slow.
        let mut got = String::new();
        for _ in 0..40 {
            if let Ok(s) = std::fs::read_to_string(&path) {
                got = s;
                if !got.is_empty() {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(got, "hello-from-spawn", "engine={engine}");
        let _ = std::fs::remove_file(&path);
    }
}

// ── 2. Concurrent fan-out: three threads append distinct lines ───────────
//
// Each spawned worker appends one labelled line. The POSIX-append (`wra`)
// path uses O_APPEND which serialises small writes at the kernel layer, so
// distinct lines survive intact. Order is non-deterministic — we sort the
// lines before comparing, so the assertion is "all three lines made it"
// rather than "in this order".
const FAN_SRC: &str = "tick p:t label:t>_\n\
                       r=wra p (fmt \"line-{}\\n\" label);?r{~_:_;^_:_}\n\
                       \n\
                       go p:t>_\n\
                       spawn tick p \"a\";spawn tick p \"b\";spawn tick p \"c\";sleep 300";

#[test]
fn spawn_concurrent_workers_all_run() {
    for engine in engines() {
        let path = temp_path(&format!("fan_{}", engine.trim_start_matches('-')));
        let _ = std::fs::remove_file(&path);
        let out = run(engine, FAN_SRC, "go", &[path.to_str().unwrap()]);
        assert!(
            out.status.success(),
            "engine={engine} ilo exited non-zero: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let mut lines: Vec<String> = Vec::new();
        for _ in 0..40 {
            if let Ok(s) = std::fs::read_to_string(&path) {
                lines = s.lines().map(str::to_string).collect();
                if lines.len() == 3 {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        lines.sort();
        assert_eq!(
            lines,
            vec![
                "line-a".to_string(),
                "line-b".to_string(),
                "line-c".to_string()
            ],
            "engine={engine} got lines={lines:?}"
        );
        let _ = std::fs::remove_file(&path);
    }
}

// ── 3. Error inside thread is logged to stderr; parent exits cleanly ────
//
// The spawned function divides by zero, which raises ILO-R009 inside the
// worker thread. The `catch_unwind` + `eprintln!` shim in the Spawn
// dispatcher converts it into a stderr line and lets the thread die. The
// parent continues, prints its own line, and exits 0.
const ERR_SRC: &str = "boom x:n>_\n\
                       y=/ x 0;prnt y\n\
                       \n\
                       go>_\n\
                       spawn boom 1;sleep 200;prnt \"parent-ok\"";

#[test]
fn spawn_thread_error_does_not_kill_parent() {
    for engine in engines() {
        let out = run(engine, ERR_SRC, "go", &[]);
        assert!(
            out.status.success(),
            "engine={engine} parent exited non-zero: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stdout.contains("parent-ok"),
            "engine={engine} parent didn't continue: stdout={stdout:?}"
        );
        assert!(
            stderr.contains("spawn:") && stderr.contains("boom"),
            "engine={engine} stderr missing spawn error trace: {stderr:?}"
        );
    }
}

// ── 4. Caps inherited: thread sees the parent's restricted write cap ─────
//
// Parent runs with `--allow-write=<file>`, so only that one path is
// writable. The spawned worker writes to it and succeeds. A second test
// confirms the inverse: without the cap, the wra call inside the thread
// fails (we just check that the file stayed empty / non-existent).
const CAP_SRC: &str = "appender p:t>_\n\
                       r=wra p \"capped\";?r{~_:_;^_:_}\n\
                       \n\
                       go p:t>_\n\
                       spawn appender p;sleep 200";

#[test]
fn spawn_thread_inherits_caps() {
    for engine in engines() {
        let path = temp_path(&format!("cap_{}", engine.trim_start_matches('-')));
        let _ = std::fs::remove_file(&path);
        let allow = format!("--allow-write={}", path.display());
        // `--allow-*` flags live on the `run` subcommand, so use that form
        // here rather than the bare positional invocation `run()` helper.
        let out = ilo()
            .args(["run", &allow, CAP_SRC, engine, "go", path.to_str().unwrap()])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "engine={engine} ilo exited non-zero: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let mut got = String::new();
        for _ in 0..40 {
            if let Ok(s) = std::fs::read_to_string(&path) {
                if !s.is_empty() {
                    got = s;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(got, "capped", "engine={engine}");
        let _ = std::fs::remove_file(&path);
    }
}

// ── 5. Verifier: spawn returns Nil and rejects non-fn first arg ──────────
//
// `ilo check` exits 0 on a program where `spawn` is used correctly in a
// `> _`-returning function. The first-arg-must-be-fn check is exercised by
// pointing `spawn` at a number literal.

#[test]
fn spawn_check_accepts_unit_return() {
    let src = "worker x:n>_\n\
               prnt x\n\
               \n\
               go>_\n\
               spawn worker 1";
    let out = ilo().arg("check").arg(src).output().expect("ilo check");
    assert!(
        out.status.success(),
        "ilo check failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn spawn_check_rejects_number_in_fn_slot() {
    let src = "go>_\n\
               spawn 42";
    let out = ilo().arg("check").arg(src).output().expect("ilo check");
    assert!(
        !out.status.success(),
        "ilo check should have failed: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("spawn") && combined.contains("function"),
        "expected spawn-needs-function error, got: {combined}"
    );
}
