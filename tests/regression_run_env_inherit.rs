//! ILO-346: Capability inheritance for run-spawned children.
//!
//! Default `run` / `run2` scrub secret env vars (ANTHROPIC_*, CLAUDE_*,
//! GITHUB_TOKEN, *_TOKEN, *_KEY, *_SECRET, etc.) from the child environment.
//! `run-full-env` / `run2-full-env` opt in to passing the full env.
//!
//! Tests spawn the `env` utility (or `sh -c 'echo $VAR'`) and assert that
//! injected secrets are absent by default and present with the full-env variants.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--vm"];

fn run_ok(engine: &str, src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} {args:?} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string()
}

// ─── Default run strips ANTHROPIC_API_KEY ────────────────────────────────────

#[test]
fn run_default_strips_anthropic_api_key() {
    // Set a fake ANTHROPIC_API_KEY in ilo's own env; the child must NOT see it.
    // We use `sh -c 'echo $ANTHROPIC_API_KEY'` — sh prints an empty line when
    // the var is unset, so we check that stdout is empty (after trim).
    let src = r#"f>t;m=run!! "sh" ["-c" "echo $ANTHROPIC_API_KEY"];??mget m "stdout" """#;
    for engine in ENGINES_ALL {
        let mut cmd = ilo();
        cmd.arg(src)
            .arg(engine)
            .arg("f")
            .env("ANTHROPIC_API_KEY", "sk-secret-1234");
        let out = cmd.output().expect("failed to run ilo");
        assert!(
            out.status.success(),
            "{engine}: ilo exited non-zero: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let trimmed = stdout.trim();
        assert!(
            trimmed.is_empty(),
            "{engine}: ANTHROPIC_API_KEY should be stripped by default `run`, got: {trimmed:?}"
        );
    }
}

// ─── Default run strips GITHUB_TOKEN ─────────────────────────────────────────

#[test]
fn run_default_strips_github_token() {
    let src = r#"f>t;m=run!! "sh" ["-c" "echo $GITHUB_TOKEN"];??mget m "stdout" """#;
    for engine in ENGINES_ALL {
        let mut cmd = ilo();
        cmd.arg(src)
            .arg(engine)
            .arg("f")
            .env("GITHUB_TOKEN", "ghp_fakefakefake");
        let out = cmd.output().expect("failed to run ilo");
        assert!(out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        let trimmed = stdout.trim();
        assert!(
            trimmed.is_empty(),
            "{engine}: GITHUB_TOKEN should be stripped by default `run`, got: {trimmed:?}"
        );
    }
}

// ─── Default run strips CLAUDE_* ─────────────────────────────────────────────

#[test]
fn run_default_strips_claude_prefix_vars() {
    let src = r#"f>t;m=run!! "sh" ["-c" "echo $CLAUDE_SECRET"];??mget m "stdout" """#;
    for engine in ENGINES_ALL {
        let mut cmd = ilo();
        cmd.arg(src)
            .arg(engine)
            .arg("f")
            .env("CLAUDE_SECRET", "top-secret");
        let out = cmd.output().expect("failed to run ilo");
        assert!(out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        let trimmed = stdout.trim();
        assert!(
            trimmed.is_empty(),
            "{engine}: CLAUDE_SECRET should be stripped by default `run`, got: {trimmed:?}"
        );
    }
}

// ─── Default run passes through PATH ────────────────────────────────────────

#[test]
fn run_default_passes_through_path() {
    // PATH is not a secret — the child must be able to find executables.
    // `echo $PATH` must produce a non-empty string.
    let src = r#"f>t;m=run!! "sh" ["-c" "echo $PATH"];??mget m "stdout" """#;
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f"]);
        assert!(
            !out.trim().is_empty(),
            "{engine}: PATH should be inherited by default `run`, got empty"
        );
    }
}

// ─── run-full-env passes ANTHROPIC_API_KEY through ───────────────────────────

#[test]
fn run_full_env_passes_anthropic_api_key() {
    let src = r#"f>t;m=run-full-env!! "sh" ["-c" "echo $ANTHROPIC_API_KEY"];??mget m "stdout" """#;
    for engine in ENGINES_ALL {
        let mut cmd = ilo();
        cmd.arg(src)
            .arg(engine)
            .arg("f")
            .env("ANTHROPIC_API_KEY", "sk-exposed-1234");
        let out = cmd.output().expect("failed to run ilo");
        assert!(
            out.status.success(),
            "{engine}: ilo exited non-zero: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let trimmed = stdout.trim();
        assert_eq!(
            trimmed, "sk-exposed-1234",
            "{engine}: run-full-env should pass ANTHROPIC_API_KEY through, got: {trimmed:?}"
        );
    }
}

// ─── run-full-env passes GITHUB_TOKEN through ───────────────────────────────

#[test]
fn run_full_env_passes_github_token() {
    let src = r#"f>t;m=run-full-env!! "sh" ["-c" "echo $GITHUB_TOKEN"];??mget m "stdout" """#;
    for engine in ENGINES_ALL {
        let mut cmd = ilo();
        cmd.arg(src)
            .arg(engine)
            .arg("f")
            .env("GITHUB_TOKEN", "ghp_exposed");
        let out = cmd.output().expect("failed to run ilo");
        assert!(out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        let trimmed = stdout.trim();
        assert_eq!(
            trimmed, "ghp_exposed",
            "{engine}: run-full-env should pass GITHUB_TOKEN through, got: {trimmed:?}"
        );
    }
}

// ─── run2 (structured) also strips secrets by default ────────────────────────

#[test]
fn run2_default_strips_anthropic_api_key() {
    let src = r#"f>t;r=run2!! "sh" ["-c" "echo $ANTHROPIC_API_KEY"];trm r.stdout"#;
    for engine in ENGINES_ALL {
        let mut cmd = ilo();
        cmd.arg(src)
            .arg(engine)
            .arg("f")
            .env("ANTHROPIC_API_KEY", "sk-secret-run2");
        let out = cmd.output().expect("failed to run ilo");
        assert!(
            out.status.success(),
            "{engine}: ilo exited non-zero: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let trimmed = stdout.trim();
        assert!(
            trimmed.is_empty(),
            "{engine}: run2 should strip ANTHROPIC_API_KEY by default, got: {trimmed:?}"
        );
    }
}

// ─── run2-full-env passes ANTHROPIC_API_KEY through ──────────────────────────

#[test]
fn run2_full_env_passes_anthropic_api_key() {
    let src = r#"f>t;r=run2-full-env!! "sh" ["-c" "echo $ANTHROPIC_API_KEY"];trm r.stdout"#;
    for engine in ENGINES_ALL {
        let mut cmd = ilo();
        cmd.arg(src)
            .arg(engine)
            .arg("f")
            .env("ANTHROPIC_API_KEY", "sk-exposed-run2");
        let out = cmd.output().expect("failed to run ilo");
        assert!(
            out.status.success(),
            "{engine}: ilo exited non-zero: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let trimmed = stdout.trim();
        assert_eq!(
            trimmed, "sk-exposed-run2",
            "{engine}: run2-full-env should pass ANTHROPIC_API_KEY through, got: {trimmed:?}"
        );
    }
}

// ─── Generic *_TOKEN pattern is stripped ─────────────────────────────────────

#[test]
fn run_default_strips_generic_token_suffix() {
    let src = r#"f>t;m=run!! "sh" ["-c" "echo $MY_API_TOKEN"];??mget m "stdout" """#;
    for engine in ENGINES_ALL {
        let mut cmd = ilo();
        cmd.arg(src)
            .arg(engine)
            .arg("f")
            .env("MY_API_TOKEN", "tok-secret");
        let out = cmd.output().expect("failed to run ilo");
        assert!(out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.trim().is_empty(),
            "{engine}: MY_API_TOKEN (*_TOKEN) should be stripped, got: {}",
            stdout.trim()
        );
    }
}

// ─── *_KEY pattern is stripped ───────────────────────────────────────────────

#[test]
fn run_default_strips_generic_key_suffix() {
    let src = r#"f>t;m=run!! "sh" ["-c" "echo $OPENAI_API_KEY"];??mget m "stdout" """#;
    for engine in ENGINES_ALL {
        let mut cmd = ilo();
        cmd.arg(src)
            .arg(engine)
            .arg("f")
            .env("OPENAI_API_KEY", "openai-secret");
        let out = cmd.output().expect("failed to run ilo");
        assert!(out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.trim().is_empty(),
            "{engine}: OPENAI_API_KEY (*_KEY) should be stripped, got: {}",
            stdout.trim()
        );
    }
}
