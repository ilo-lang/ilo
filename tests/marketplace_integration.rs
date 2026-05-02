// Integration tests for the Claude plugin marketplace bundle that ilo ships.
//
// These guard against drift between `.claude-plugin/marketplace.json`,
// `Cargo.toml`, and `skills/ilo/SKILL.md`. Catching this drift in CI
// avoids submitting a broken bundle to the Claude plugin marketplace.
//
// What we assert:
//   1. `marketplace.json` parses and has the expected top-level shape and
//      per-plugin fields (name, source, description, version, author,
//      homepage, license, keywords).
//   2. Each plugin's `source` resolves to a real file or directory under the
//      repo root.
//   3. The `version` declared in `marketplace.json` matches the package
//      `version` in `Cargo.toml` for every plugin entry.
//   4. `PRIVACY.md` exists at the repo root, is non-empty (>100 chars), and
//      mentions "ilo" — the marketplace submission requires it.
//   5. Soft cross-link check: the plugin description in `marketplace.json`
//      and the description in `skills/ilo/SKILL.md` frontmatter both contain
//      "ilo" (case-insensitive) and are at least 50 chars long.

use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_string(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn marketplace_json() -> Value {
    let p = repo_root().join(".claude-plugin/marketplace.json");
    let raw = read_string(&p);
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("marketplace.json must parse as JSON: {e}"))
}

fn cargo_toml_version() -> String {
    let raw = read_string(&repo_root().join("Cargo.toml"));
    // Find the `[package]` section, then grab the first `version = "..."`
    // line within it. Avoids pulling in the `toml` crate as a dev-dep.
    let pkg_idx = raw
        .find("[package]")
        .expect("Cargo.toml must contain a [package] section");
    let after_pkg = &raw[pkg_idx..];
    // Bound to the next `[section]` so we don't accidentally grab a
    // `version = "..."` from `[dependencies]`.
    let end = after_pkg[1..]
        .find("\n[")
        .map(|i| i + 1)
        .unwrap_or(after_pkg.len());
    let pkg_section = &after_pkg[..end];

    let re = Regex::new(r#"(?m)^version\s*=\s*"([^"]+)"\s*$"#).unwrap();
    let caps = re
        .captures(pkg_section)
        .expect("Cargo.toml [package] must have a version line");
    caps.get(1).unwrap().as_str().to_string()
}

fn nonempty_str<'a>(v: &'a Value, label: &str) -> &'a str {
    let s = v
        .as_str()
        .unwrap_or_else(|| panic!("{label} must be a string"));
    assert!(!s.trim().is_empty(), "{label} must be non-empty");
    s
}

#[test]
fn marketplace_json_top_level_shape() {
    let m = marketplace_json();
    nonempty_str(&m["name"], "marketplace.name");
    nonempty_str(&m["owner"]["name"], "marketplace.owner.name");
    nonempty_str(
        &m["metadata"]["description"],
        "marketplace.metadata.description",
    );

    let plugins = m["plugins"]
        .as_array()
        .expect("marketplace.plugins must be an array");
    assert!(
        !plugins.is_empty(),
        "marketplace.plugins must be a non-empty array"
    );
}

#[test]
fn marketplace_json_each_plugin_has_required_fields() {
    let m = marketplace_json();
    let plugins = m["plugins"].as_array().expect("plugins array");

    let semver_re = Regex::new(r"^\d+\.\d+\.\d+").unwrap();
    let url_re = Regex::new(r"^https?://").unwrap();

    for (i, p) in plugins.iter().enumerate() {
        let label = |k: &str| format!("plugins[{i}].{k}");
        nonempty_str(&p["name"], &label("name"));
        nonempty_str(&p["source"], &label("source"));
        nonempty_str(&p["description"], &label("description"));

        let version = nonempty_str(&p["version"], &label("version"));
        assert!(
            semver_re.is_match(version),
            "{} must look like semver, got {:?}",
            label("version"),
            version
        );

        nonempty_str(&p["author"]["name"], &label("author.name"));

        let homepage = nonempty_str(&p["homepage"], &label("homepage"));
        assert!(
            url_re.is_match(homepage),
            "{} must be an http(s) URL, got {:?}",
            label("homepage"),
            homepage
        );

        nonempty_str(&p["license"], &label("license"));

        let kw = p["keywords"]
            .as_array()
            .unwrap_or_else(|| panic!("{} must be an array", label("keywords")));
        assert!(
            !kw.is_empty(),
            "{} must be a non-empty array",
            label("keywords")
        );
        for (j, k) in kw.iter().enumerate() {
            nonempty_str(k, &format!("plugins[{i}].keywords[{j}]"));
        }
    }
}

#[test]
fn marketplace_plugin_source_resolves_under_repo_root() {
    let m = marketplace_json();
    let plugins = m["plugins"].as_array().expect("plugins array");
    let root = repo_root();

    for (i, p) in plugins.iter().enumerate() {
        let source = p["source"].as_str().expect("source string");
        // Strip a leading `./` so `Path::join` treats it as relative to the
        // repo root rather than CWD.
        let rel = source.strip_prefix("./").unwrap_or(source);
        let target = if rel.is_empty() {
            root.clone()
        } else {
            root.join(rel)
        };
        assert!(
            target.exists(),
            "plugins[{i}].source = {source:?} -> {} must exist",
            target.display()
        );
        // Read the directory listing or the file metadata to confirm it's
        // readable. For `./` we expect at least the `skills/` directory.
        let md = std::fs::metadata(&target)
            .unwrap_or_else(|e| panic!("plugins[{i}].source = {source:?} must be readable: {e}"));
        if md.is_dir() {
            // Soft check: the plugin bundle should ship a skills/ directory.
            let skills = target.join("skills");
            assert!(
                skills.is_dir(),
                "plugins[{i}].source resolved dir {} must contain skills/",
                target.display()
            );
        }
    }
}

#[test]
fn marketplace_version_matches_cargo_toml() {
    let m = marketplace_json();
    let cargo_version = cargo_toml_version();
    let plugins = m["plugins"].as_array().expect("plugins array");

    for (i, p) in plugins.iter().enumerate() {
        let version = p["version"].as_str().expect("version string");
        assert_eq!(
            version, cargo_version,
            "plugins[{i}].version ({version}) must equal Cargo.toml package.version ({cargo_version})"
        );
    }
}

#[test]
fn privacy_md_is_present_and_mentions_ilo() {
    let p = repo_root().join("PRIVACY.md");
    assert!(p.is_file(), "{} must exist", p.display());
    let body = read_string(&p);
    assert!(
        body.len() > 100,
        "PRIVACY.md must be non-trivial (>100 chars), got {}",
        body.len()
    );
    assert!(
        body.contains("ilo"),
        "PRIVACY.md must mention the literal string \"ilo\""
    );
}

/// Mirror of the frontmatter parser used in `tests/skill_md.rs` — kept here
/// instead of imported because integration tests are separate crates.
fn split_frontmatter(text: &str) -> (&str, &str) {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .expect("SKILL.md must start with `---` frontmatter delimiter");
    let end = rest
        .find("\n---\n")
        .or_else(|| rest.find("\n---\r\n"))
        .expect("SKILL.md must close its frontmatter with a `---` line");
    (&rest[..end], "")
}

fn top_level_scalar(fm: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    for line in fm.lines() {
        if line.starts_with(&prefix) {
            let rest = line[prefix.len()..].trim();
            if rest.is_empty() {
                return None;
            }
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

#[test]
fn marketplace_and_skill_descriptions_cross_link() {
    let m = marketplace_json();
    let plugin = &m["plugins"][0];
    let market_desc = plugin["description"]
        .as_str()
        .expect("plugins[0].description");

    let skill_md = read_string(&repo_root().join("skills/ilo/SKILL.md"));
    let (fm, _) = split_frontmatter(&skill_md);
    let skill_desc =
        top_level_scalar(fm, "description").expect("SKILL.md frontmatter must have a description");

    for (label, desc) in [
        ("marketplace plugin description", market_desc),
        ("SKILL.md description", skill_desc.as_str()),
    ] {
        assert!(
            desc.to_lowercase().contains("ilo"),
            "{label} must contain \"ilo\" (case-insensitive), got {desc:?}"
        );
        assert!(
            desc.len() >= 50,
            "{label} must be at least 50 chars, got {} ({desc:?})",
            desc.len()
        );
    }
}
