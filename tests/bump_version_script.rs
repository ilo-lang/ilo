// Regression for scripts/bump-version.sh: every release-facing version
// reference must move when the script runs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

// Restores snapshotted file contents when dropped, so the working tree never
// ends up with the sentinel version on test failure or panic.
struct FileRestore(Vec<(PathBuf, String)>);

impl Drop for FileRestore {
    fn drop(&mut self) {
        for (path, body) in &self.0 {
            let _ = fs::write(path, body);
        }
    }
}

#[test]
fn bump_version_script_rewrites_every_version_site() {
    let root = repo_root();
    let script = root.join("scripts/bump-version.sh");
    assert!(script.exists(), "scripts/bump-version.sh missing");

    let targets = [
        root.join("Cargo.toml"),
        root.join("AGENTS.md"),
        root.join(".claude-plugin/plugin.json"),
        root.join(".claude-plugin/marketplace.json"),
    ];

    // Cargo.lock is also mutated (by `cargo update` inside the script), so
    // snapshot it alongside the targets so the restore guard puts it back.
    let snapshots: Vec<PathBuf> = targets
        .iter()
        .cloned()
        .chain(std::iter::once(root.join("Cargo.lock")))
        .collect();
    let _restore = FileRestore(snapshots.iter().map(|p| (p.clone(), read(p))).collect());

    let sentinel = "9.99.99";

    let status = Command::new("bash")
        .arg(&script)
        .arg(sentinel)
        .current_dir(&root)
        .status()
        .expect("run bump-version.sh");
    assert!(status.success(), "bump-version.sh exited with {status}");

    let mut failures = Vec::new();
    for path in &targets {
        if !read(path).contains(sentinel) {
            failures.push(path.display().to_string());
        }
    }

    assert!(
        failures.is_empty(),
        "sentinel {sentinel} did not land in: {failures:?}"
    );
}

#[test]
fn bump_version_script_rejects_bad_input() {
    let root = repo_root();
    let script = root.join("scripts/bump-version.sh");

    let status = Command::new("bash")
        .arg(&script)
        .arg("not-a-version")
        .current_dir(&root)
        .status()
        .expect("run bump-version.sh");
    assert!(!status.success(), "script should reject 'not-a-version'");

    let status = Command::new("bash")
        .arg(&script)
        .current_dir(&root)
        .status()
        .expect("run bump-version.sh");
    assert!(!status.success(), "script should reject missing arg");
}
