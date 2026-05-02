// Compatibility tests for `skills/ilo/SKILL.md`.
//
// These guard against regressions that would silently break the skill in
// Claude Code, Claude Desktop, Codex, or any other Agent Skills-compatible
// host. The full reference validator (`skills-ref validate`) is run as a
// separate CI step when available; these tests provide a hard, dependency-free
// regression layer that runs in `cargo test`.
//
// What we assert:
//   1. Frontmatter shape (name/description constraints, allowed-tools is a
//      string per spec, no top-level `argument-hint`).
//   2. Body contains no `${CLAUDE_*}` env-var references (those are
//      Claude-Code-only and break in Codex / other agents).
//   3. Every `scripts/<name>` path mentioned in the body resolves under
//      `skills/ilo/scripts/`.
//   4. Body contains the canonical sections that Claude Code's loader and
//      humans rely on — a regression test mirroring what an agent sees.

use std::path::{Path, PathBuf};

fn skill_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skills/ilo")
}

fn skill_md_text() -> String {
    let p = skill_dir().join("SKILL.md");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Split the file into (frontmatter, body). The frontmatter is the YAML
/// between the first two `---` lines; the body is everything after.
fn split_frontmatter(text: &str) -> (&str, &str) {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .expect("SKILL.md must start with `---` frontmatter delimiter");
    let end = rest
        .find("\n---\n")
        .or_else(|| rest.find("\n---\r\n"))
        .expect("SKILL.md must close its frontmatter with a `---` line");
    let fm = &rest[..end];
    // Skip past `\n---\n` (5 bytes) or `\n---\r\n` (6 bytes).
    let body_start = end
        + if rest[end..].starts_with("\n---\r\n") {
            6
        } else {
            5
        };
    (fm, &rest[body_start..])
}

/// Extract a top-level scalar string value for `key:` from frontmatter.
/// Returns `None` if the key is missing or the value is a block (sequence /
/// mapping). Strips matching surrounding quotes.
fn top_level_scalar(fm: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    for line in fm.lines() {
        // Top-level keys have no leading whitespace.
        if line.starts_with(&prefix) {
            let rest = line[prefix.len()..].trim();
            if rest.is_empty() {
                return None; // block value
            }
            // Strip surrounding double or single quotes.
            let unquoted = if (rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2)
                || (rest.starts_with('\'') && rest.ends_with('\'') && rest.len() >= 2)
            {
                &rest[1..rest.len() - 1]
            } else {
                rest
            };
            return Some(unquoted.to_string());
        }
    }
    None
}

/// True if a top-level key exists at all (regardless of scalar/block).
fn has_top_level_key(fm: &str, key: &str) -> bool {
    let prefix = format!("{key}:");
    fm.lines().any(|l| l.starts_with(&prefix))
}

#[test]
fn skill_md_exists() {
    let p = skill_dir().join("SKILL.md");
    assert!(
        p.is_file(),
        "skills/ilo/SKILL.md missing at {}",
        p.display()
    );
}

#[test]
fn frontmatter_name_is_valid() {
    let text = skill_md_text();
    let (fm, _) = split_frontmatter(&text);
    let name = top_level_scalar(fm, "name").expect("frontmatter must have `name`");
    assert_eq!(name, "ilo", "skill name must be `ilo`");
    assert!(name.len() <= 64, "name must be <= 64 chars");
    // Pattern: ^[a-z][a-z0-9-]*[a-z0-9]$
    let bytes = name.as_bytes();
    assert!(
        bytes.len() >= 2,
        "name must be at least 2 chars to satisfy spec pattern"
    );
    assert!(
        bytes[0].is_ascii_lowercase(),
        "name must start with a lowercase letter"
    );
    let last = bytes[bytes.len() - 1];
    assert!(
        last.is_ascii_lowercase() || last.is_ascii_digit(),
        "name must end with [a-z0-9]"
    );
    for &b in bytes {
        assert!(
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-',
            "name must match ^[a-z][a-z0-9-]*[a-z0-9]$, got byte {b:#x}"
        );
    }
}

#[test]
fn frontmatter_description_is_valid() {
    let text = skill_md_text();
    let (fm, _) = split_frontmatter(&text);
    let desc =
        top_level_scalar(fm, "description").expect("frontmatter must have a scalar `description`");
    assert!(!desc.is_empty(), "description must not be empty");
    assert!(
        desc.len() <= 1024,
        "description must be <= 1024 chars (got {})",
        desc.len()
    );
}

#[test]
fn allowed_tools_is_string_form() {
    // The Agent Skills spec defines `allowed-tools` as a space-separated
    // string. Claude Code also accepts a YAML sequence, but Codex and other
    // hosts only accept the string form. We pin to the spec.
    let text = skill_md_text();
    let (fm, _) = split_frontmatter(&text);
    if !has_top_level_key(fm, "allowed-tools") {
        return; // optional field
    }
    let val = top_level_scalar(fm, "allowed-tools")
        .expect("`allowed-tools`, if present, must be a scalar string (not a YAML sequence)");
    assert!(
        !val.is_empty(),
        "`allowed-tools` must not be the empty string"
    );
    // Each whitespace-separated token must look like a tool name.
    for tok in val.split_whitespace() {
        assert!(
            tok.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "unexpected token in allowed-tools: {tok:?}"
        );
    }
}

#[test]
fn no_top_level_argument_hint() {
    // `argument-hint` is a Claude-only convenience; per Agent Skills spec it
    // belongs under `metadata:`. Codex etc. ignore unknown top-level keys but
    // this catches a regression where someone re-adds it at the top level.
    let text = skill_md_text();
    let (fm, _) = split_frontmatter(&text);
    assert!(
        !has_top_level_key(fm, "argument-hint"),
        "`argument-hint` must live under `metadata:`, not at the top level"
    );
}

#[test]
fn body_has_no_claude_env_vars() {
    // ${CLAUDE_PROJECT_DIR} and friends are injected only by Claude Code and
    // break when the skill runs under Codex or other Agent Skills hosts.
    let text = skill_md_text();
    let (_fm, body) = split_frontmatter(&text);
    // Look for `${CLAUDE_` literally.
    if let Some(idx) = body.find("${CLAUDE_") {
        let snippet_end = (idx + 60).min(body.len());
        panic!(
            "SKILL.md body references a Claude-Code-only env var; this breaks portability.\n\
             First match: {:?}",
            &body[idx..snippet_end]
        );
    }
}

#[test]
fn referenced_scripts_exist() {
    // Any `scripts/<name>` path mentioned in the body must resolve under
    // `skills/ilo/scripts/` so other hosts can find them.
    let text = skill_md_text();
    let (_fm, body) = split_frontmatter(&text);
    let scripts_dir = skill_dir().join("scripts");
    let mut checked = 0usize;
    let mut search_from = 0usize;
    while let Some(rel) = body[search_from..].find("scripts/") {
        let abs = search_from + rel;
        // Walk forward gathering the path component until whitespace, backtick,
        // or other delimiter.
        let after = &body[abs + "scripts/".len()..];
        let end = after
            .find(|c: char| {
                c.is_whitespace() || c == '`' || c == ')' || c == '"' || c == '\'' || c == ','
            })
            .unwrap_or(after.len());
        let name = &after[..end];
        if !name.is_empty() {
            let candidate = scripts_dir.join(name);
            assert!(
                candidate.is_file(),
                "SKILL.md references `scripts/{name}` but {} does not exist",
                candidate.display()
            );
            checked += 1;
        }
        search_from = abs + "scripts/".len() + end;
    }
    assert!(
        checked > 0,
        "expected at least one `scripts/<name>` reference in SKILL.md (e.g. ensure-ilo.sh)"
    );
}

#[test]
fn body_has_canonical_sections() {
    // Regression test: if someone deletes a section that hosts (and humans)
    // expect, this fails. The headings below mirror what Claude Code's
    // skill loader currently surfaces.
    let text = skill_md_text();
    let (_fm, body) = split_frontmatter(&text);
    let required_headings = [
        "## Setup",
        "## Load the Full Spec",
        "## Overview",
        "## Core Syntax",
        "## Types",
        "## Guards",
        "## Match",
        "## Results and Error Handling",
        "## Loops",
        "## Higher-Order Functions",
        "## Pipe Operator",
        "## Records",
        "## Maps",
        "## Builtins Reference",
        "## Naming Convention",
        "## Running",
        "## Multi-Function File Rules",
        "## Examples",
        "## Common Mistakes",
    ];
    for h in required_headings {
        assert!(
            body.contains(h),
            "SKILL.md is missing canonical heading: {h}"
        );
    }
}

#[test]
fn ensure_ilo_script_runs_without_claude_env_vars() {
    // Portability check for the install script. We run it with a sanitised
    // environment that strips any `CLAUDE_*` variables, with the working
    // directory at the skill root, exactly as a non-Claude host (Codex etc.)
    // would invoke it. The script must either succeed or exit with a clear
    // error — it must not silently misbehave because of a missing
    // Claude-injected env var.
    //
    // We run the script in `--check`-equivalent mode by piping it through
    // `bash -n` (no-exec syntax check) first to ensure it parses; then we
    // execute it in a sanitised env. For determinism in CI we don't require
    // the install to actually succeed (network may be flaky), but the
    // exit code must be 0 on success or non-zero with a useful message.
    let script = skill_dir().join("scripts/ensure-ilo.sh");
    assert!(
        script.is_file(),
        "ensure-ilo.sh missing at {}",
        script.display()
    );

    // Syntax check: must parse under `sh -n`.
    let syntax = std::process::Command::new("sh")
        .arg("-n")
        .arg(&script)
        .output()
        .expect("invoke sh -n");
    assert!(
        syntax.status.success(),
        "ensure-ilo.sh failed `sh -n` syntax check: {}",
        String::from_utf8_lossy(&syntax.stderr)
    );

    // Sanitised execution: strip CLAUDE_* env vars so we exercise the
    // non-Claude-host code path. We don't require success (network may be
    // unavailable in sandboxed CI), only that:
    //   - the process terminates,
    //   - it does not panic / crash with an interpreter error,
    //   - if it fails, stderr is non-empty (i.e. it explained itself).
    let path = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();

    let out = std::process::Command::new("sh")
        .arg(skill_dir().join("scripts/ensure-ilo.sh"))
        .env_clear()
        .env("PATH", &path)
        .env("HOME", &home)
        .current_dir(skill_dir())
        .output()
        .expect("run ensure-ilo.sh");

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stderr.trim().is_empty() || !stdout.trim().is_empty(),
            "ensure-ilo.sh failed silently with no output (exit {:?}); a non-Claude host \
             would have no idea what went wrong",
            out.status.code()
        );
    }
}

// Helper to keep clippy happy about unused std::path::Path in earlier drafts.
#[allow(dead_code)]
fn _unused(_: &Path) {}
