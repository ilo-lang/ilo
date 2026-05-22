// Modular skill tests.
//
// Phase 1 (#395) split the monolithic compact spec into six embedded skill
// modules. Phase 2 added `ilo-examples` and `ilo-edit-loop` (PR #419) for
// Zero-parity task coverage. Phase 3 split `ilo-builtins` into four category
// files (core, math, io, text) for language-growth headroom. Phase 4 carved
// records out of `ilo-language` into `ilo-language-records` so record-free
// tasks don't pay the token cost. Each module is loaded on demand by an agent
// via `ilo skill get <name>` (or `--json`).
// These tests guard the structural contract:
//
//   - all twelve modules exist at known paths
//   - each carries valid Anthropic-format YAML frontmatter (`name`,
//     `description`)
//   - every `description` starts with `Use this when`, the routing key agents
//     match against
//   - per-module byte budget is respected (≈ proxy for the hard 1,000-token
//     cap on category modules, or 1,500 tokens for `ilo-language`; the exact
//     tiktoken count is verified by the `skills-tokens` job in
//     `.github/workflows/rust.yml`)
//   - the marketplace manifest lists every skill that's bundled

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest)
}

const SKILL_NAMES: &[&str] = &[
    "ilo-language",
    "ilo-language-records",
    "ilo-builtins-core",
    "ilo-builtins-math",
    "ilo-builtins-io",
    "ilo-builtins-text",
    "ilo-errors",
    "ilo-tools",
    "ilo-engines",
    "ilo-agent",
    "ilo-examples",
    "ilo-edit-loop",
];

/// Aggressive byte budget per module (ILO-382). cl100k_base averages ~3.4
/// bytes/token on dense reference-style markdown like ours, so 6_500 bytes ≈
/// ~1,900 tokens in the worst case. The Python tiktoken job in CI enforces the
/// precise per-module caps (see `scripts/check-skill-tokens.py`); this in-binary
/// check is a defence-in-depth tripwire that catches drift even when CI is
/// bypassed. Set to actual largest module (ilo-builtins-io, ~6,230 bytes) plus
/// ~270 bytes headroom.
const BYTE_BUDGET_PER_MODULE: usize = 6_500;

/// Total bytes across all twelve (ILO-382 aggressive cap). Measured baseline
/// is ~38,082 bytes; the tighter tiktoken job in CI enforces the 12,500-token
/// aggregate cap. Leaving ~10% byte slack here avoids spurious failures when a
/// small edit lands locally without re-running the Python counter.
const BYTE_BUDGET_TOTAL: usize = 42_000;

fn read_skill(name: &str) -> String {
    let p = repo_root().join("skills/ilo").join(format!("{name}.md"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("failed to read {}: {e}", p.display()))
}

#[test]
fn every_skill_file_exists() {
    for n in SKILL_NAMES {
        let p = repo_root().join("skills/ilo").join(format!("{n}.md"));
        assert!(p.exists(), "missing skill file: {}", p.display());
    }
}

#[test]
fn every_skill_has_valid_frontmatter() {
    for n in SKILL_NAMES {
        let s = read_skill(n);
        assert!(
            s.starts_with("---\n"),
            "{n}.md must start with `---` frontmatter delimiter"
        );
        let after = &s[4..];
        let end = after
            .find("\n---\n")
            .unwrap_or_else(|| panic!("{n}.md must close frontmatter with `---`"));
        let fm = &after[..end];
        assert!(
            fm.lines().any(|l| l.trim_start().starts_with("name:")),
            "{n}.md frontmatter must have `name:`"
        );
        assert!(
            fm.lines()
                .any(|l| l.trim_start().starts_with("description:")),
            "{n}.md frontmatter must have `description:`"
        );
    }
}

#[test]
fn every_description_starts_with_use_this_when() {
    for n in SKILL_NAMES {
        let s = read_skill(n);
        let line = s
            .lines()
            .find(|l| l.trim_start().starts_with("description:"))
            .unwrap_or_else(|| panic!("{n}.md missing description line"));
        let value = line.split_once(':').map(|(_, v)| v.trim()).unwrap_or("");
        // Strip optional surrounding quotes
        let value = value.trim_matches('"');
        assert!(
            value.starts_with("Use this when"),
            "{n}.md description must start with 'Use this when' (got: {value:.40})"
        );
    }
}

#[test]
fn every_skill_name_matches_filename() {
    for n in SKILL_NAMES {
        let s = read_skill(n);
        let name_line = s
            .lines()
            .find(|l| l.trim_start().starts_with("name:"))
            .unwrap_or_else(|| panic!("{n}.md missing name line"));
        let value = name_line
            .split_once(':')
            .map(|(_, v)| v.trim())
            .unwrap_or("");
        let value = value.trim_matches('"');
        assert_eq!(value, *n, "{n}.md frontmatter name must match filename");
    }
}

#[test]
fn per_module_byte_budget_respected() {
    for n in SKILL_NAMES {
        let s = read_skill(n);
        let len = s.len();
        assert!(
            len <= BYTE_BUDGET_PER_MODULE,
            "{n}.md is {len} bytes, over the per-module budget of {BYTE_BUDGET_PER_MODULE}. \
             Trim or split. Hard per-module token caps are enforced by the tiktoken CI job \
             (see scripts/check-skill-tokens.py — aggressive caps set in ILO-382)."
        );
    }
}

#[test]
fn total_byte_budget_respected() {
    let total: usize = SKILL_NAMES.iter().map(|n| read_skill(n).len()).sum();
    assert!(
        total <= BYTE_BUDGET_TOTAL,
        "total skills size is {total} bytes, over the aggregate budget of {BYTE_BUDGET_TOTAL}. \
         The hard 12,500-token aggregate cap is enforced by the tiktoken CI job."
    );
}

#[test]
fn marketplace_lists_every_skill() {
    let mp = repo_root().join(".claude-plugin/marketplace.json");
    let raw = std::fs::read_to_string(&mp).expect("marketplace.json present");
    for n in SKILL_NAMES {
        assert!(raw.contains(n), "marketplace.json must list skill {n}");
    }
}
