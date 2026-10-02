//! Exp-09 S3: soft `L>` ≡ `L _` (elided list element type).
//! Shared P007 footgun from artefact record/rollup emits (`norm x:L>L`,
//! `classify p:L>t`). Accept-or-rewrite; docs still prefer explicit `L _`.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn check_ok(src: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("t.ilo");
    std::fs::write(&path, src).expect("write");
    let out = ilo()
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("ilo check");
    assert!(
        out.status.success(),
        "expected soft L> to check ok\nsrc:\n{src}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn soft_l_before_return_arrow() {
    check_ok("f x:L>_ ;\n  x\n");
    check_ok("classify p:L>t\n  \"ok\"\n");
}

#[test]
fn soft_l_both_sides_of_arrow() {
    check_ok("norm x:L>L;\n  x\n");
}

#[test]
fn soft_l_bare_return_after_explicit_param() {
    check_ok("g x:L _>L;\n  x\n");
}

#[test]
fn explicit_element_types_unchanged() {
    check_ok("f x:L _>_\n  x\n");
    check_ok("f x:L n>L n\n  x\n");
    check_ok("f x:L (M t t)>L (M t t)\n  x\n");
}
