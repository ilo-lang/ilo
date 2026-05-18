// Binary-size tripwire.
//
// Compiles a minimal ilo program (`m>n;42`) via the AOT path and asserts the
// resulting native binary stays under a generous ceiling. The point isn't to
// drive size down — it's to catch silent regressions where a dep upgrade or a
// new transitive crate quietly inflates the AOT runtime.
//
// Threshold rationale: under `cargo test` the AOT-linked binary is built
// against the *debug* libilo.a (debuginfo + un-stripped HTTPS / cranelift
// object code). The debug baseline differs across platforms: ~34 MB on
// macOS arm64, ~94 MB on Linux x86_64 (CI). Release + strip + LTO shrinks
// the same program to ~9 MB but isn't what `cargo test` links against in
// CI. The 150 MB ceiling sits ~50% above the Linux debug baseline while
// still failing loudly if a new dep silently inflates the AOT runtime.
//
// Gated on the `cranelift` feature because AOT compile requires it.

#![cfg(feature = "cranelift")]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// Maximum AOT binary size in bytes. Debug baseline is ~34 MB (macOS arm64)
/// / ~94 MB (Linux x86_64); ceiling is 150 MB. Bump deliberately only with
/// a PR note explaining the new floor — never silently.
const MAX_AOT_BINARY_BYTES: u64 = 150 * 1024 * 1024;

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn tmp_paths(tag: &str) -> (PathBuf, PathBuf) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let src = std::env::temp_dir().join(format!("ilo-binsize-{tag}-{pid}-{n}.ilo"));
    let bin = std::env::temp_dir().join(format!("ilo-binsize-{tag}-{pid}-{n}.bin"));
    (src, bin)
}

#[test]
fn aot_binary_size_under_threshold() {
    let (src_path, bin_path) = tmp_paths("min");
    std::fs::write(&src_path, "m>n;42").expect("write ilo source");

    let compile = ilo()
        .arg("compile")
        .arg(&src_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .expect("failed to invoke ilo compile");
    assert!(
        compile.status.success(),
        "ilo compile failed: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr),
    );

    let size = std::fs::metadata(&bin_path)
        .expect("AOT binary should exist after compile")
        .len();

    // Best-effort cleanup; ignore failures.
    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&bin_path);

    assert!(
        size <= MAX_AOT_BINARY_BYTES,
        "AOT binary for `m>n;42` is {size} bytes, exceeds tripwire ceiling of \
         {MAX_AOT_BINARY_BYTES} bytes ({:.2} MB > {:.2} MB). Either a dep \
         upgrade has bloated the runtime, or the threshold should be revisited \
         with an explicit PR note.",
        size as f64 / (1024.0 * 1024.0),
        MAX_AOT_BINARY_BYTES as f64 / (1024.0 * 1024.0),
    );
}
