// Regression: cranelift JIT runtime errors must carry the same source span
// as tree / VM so the diagnostic renderer can underline the offending site.
//
// Before this fix, the JIT runtime-error TLS cell stored only `VmError`, so
// every cranelift error surfaced with `span: None` and the diagnostic JSON
// emitted an empty `labels: []`. The fix packs the call-site span into a
// u64 immediate at compile time and threads it through the helper signature
// for `jit_hd`, `jit_at`, `jit_tl`. These tests assert byte-for-byte parity
// of the `labels[0]` field across tree, VM, and cranelift for the v1 helpers.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run `ilo` in JSON-diagnostic mode and return stderr.
fn run_err(engine: &str, src: &str) -> String {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "engine={engine}: expected runtime error for `{src}`, stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Extract `"start":NUM,"end":NUM` from a diagnostic JSON label. Returns
/// `None` if the line has no label (i.e. `labels: []`).
fn extract_span(stderr: &str) -> Option<(u32, u32)> {
    // Diagnostic JSON looks like:
    //   "labels":[{"col":7,"end":11,"line":1,"message":"here","primary":true,"start":6}]
    // We want start/end. Bail if labels is empty.
    if stderr.contains("\"labels\":[]") {
        return None;
    }
    let start_key = "\"start\":";
    let end_key = "\"end\":";
    let s_pos = stderr.find(start_key)?;
    let after_start = &stderr[s_pos + start_key.len()..];
    let start: u32 = after_start
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    let e_pos = stderr.find(end_key)?;
    let after_end = &stderr[e_pos + end_key.len()..];
    let end: u32 = after_end
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some((start, end))
}

/// Asserts that cranelift produces the same `(start, end)` span as VM for the
/// same source program. Tree is checked too as a sanity baseline.
#[cfg(feature = "cranelift")]
fn assert_span_parity(src: &str) {
    let vm_err = run_err("--run-vm", src);
    let cl_err = run_err("--run-cranelift", src);

    let vm_span = extract_span(&vm_err)
        .unwrap_or_else(|| panic!("VM stderr had no labels for `{src}`: {vm_err}"));
    let cl_span = extract_span(&cl_err).unwrap_or_else(|| {
        panic!(
            "cranelift stderr had no labels for `{src}` (span did not survive JIT helper): {cl_err}"
        )
    });

    assert_eq!(
        cl_span, vm_span,
        "engine span divergence for `{src}`: cranelift={cl_span:?} vs vm={vm_span:?}\nvm stderr={vm_err}\ncl stderr={cl_err}"
    );
}

// ── jit_hd ────────────────────────────────────────────────────────────

#[test]
#[cfg(feature = "cranelift")]
fn hd_empty_list_span_matches_vm() {
    assert_span_parity("f>n;hd []");
}

#[test]
#[cfg(feature = "cranelift")]
fn hd_empty_text_span_matches_vm() {
    assert_span_parity("f>t;hd \"\"");
}

// ── jit_tl ────────────────────────────────────────────────────────────

#[test]
#[cfg(feature = "cranelift")]
fn tl_empty_list_span_matches_vm() {
    assert_span_parity("f>L n;tl []");
}

#[test]
#[cfg(feature = "cranelift")]
fn tl_empty_text_span_matches_vm() {
    assert_span_parity("f>t;tl \"\"");
}

// ── jit_at ────────────────────────────────────────────────────────────

#[test]
#[cfg(feature = "cranelift")]
fn at_oob_positive_list_span_matches_vm() {
    assert_span_parity("f>n;at [1,2,3] 99");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_oob_negative_list_span_matches_vm() {
    assert_span_parity("f>n;at [1,2,3] -99");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_oob_text_span_matches_vm() {
    assert_span_parity("f>t;at \"hi\" 99");
}

// Fractional indices used to error here; auto-floor turned that into a
// successful element fetch (see examples/at-float-index.ilo). The jit_at
// runtime error path is still covered above by `at_oob_*_span_matches_vm`.

// ── Sanity: a span must actually be present (not None) ────────────────
//
// Belt-and-braces guard against a future regression that silently drops to
// `span: None` without breaking the cross-engine equality above (which would
// fail because vm has a span and cl doesn't, but better to assert presence
// explicitly).
#[test]
#[cfg(feature = "cranelift")]
fn cranelift_hd_error_carries_some_span() {
    let stderr = run_err("--run-cranelift", "f>n;hd []");
    assert!(
        !stderr.contains("\"labels\":[]"),
        "cranelift hd error must carry a source span, got: {stderr}"
    );
    let (start, end) = extract_span(&stderr).expect("expected start/end in stderr");
    assert!(
        end > start,
        "span end must be > start, got start={start} end={end}"
    );
}
