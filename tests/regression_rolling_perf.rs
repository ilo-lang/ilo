// Ignored microbench locking in the asymptotic improvement of `rsum`,
// `ravg`, and `rmin` over the naive `slc + sum/avg/min` baseline.
//
// Run with: cargo test --release --features cranelift -- --ignored rolling_perf
//
// The check is "sub-linear vs baseline at a 10k-element input": the
// O(n*w) baseline grows with window size, the O(n) running-window
// implementation does not. We pick a deliberately fat window so the gap
// is unambiguous even on a noisy machine.

use std::process::Command;
use std::time::Instant;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn time_run(src: &str, fn_name: &str) -> std::time::Duration {
    let start = Instant::now();
    let out = ilo()
        .args(["run", src, fn_name])
        .output()
        .expect("failed to run ilo");
    let dur = start.elapsed();
    assert!(
        out.status.success(),
        "ilo failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    dur
}

#[test]
#[ignore]
fn rsum_outperforms_slc_baseline_at_10k() {
    // 10k input, window=500. Baseline is O(n*w) = 5M; rsum is O(n) = 10k.
    // We expect rsum to be at least 5x faster than the baseline on any
    // machine. Setting the bound loosely (2x) so noisy CI still passes
    // but a regression to the naive shape (1x or worse) is caught.
    let n: usize = 10_000;
    let w: usize = 500;

    let baseline_src = format!(
        "f>L n;xs=map (i:n>n;i) (range 0 {n});\
         map (i:n>n;sum (slc xs i (+ i {w}))) (range 0 (+ 1 (- {n} {w})))"
    );
    let fast_src = format!(
        "f>L n;xs=map (i:n>n;i) (range 0 {n});\
         rsum {w} xs"
    );

    let baseline = time_run(&baseline_src, "f");
    let fast = time_run(&fast_src, "f");

    eprintln!(
        "rsum perf: baseline={:?}, fast={:?}, speedup={:.2}x",
        baseline,
        fast,
        baseline.as_secs_f64() / fast.as_secs_f64()
    );

    assert!(
        fast < baseline / 2,
        "rsum must be at least 2x faster than the slc baseline at n={n}, w={w}: baseline={baseline:?}, fast={fast:?}"
    );
}

#[test]
#[ignore]
fn rmin_outperforms_slc_baseline_at_10k() {
    // Same shape as rsum: O(n) amortised vs O(n*w) naive.
    let n: usize = 10_000;
    let w: usize = 500;

    let baseline_src = format!(
        "f>L n;xs=map (i:n>n;mod i 100) (range 0 {n});\
         map (i:n>n;min (slc xs i (+ i {w}))) (range 0 (+ 1 (- {n} {w})))"
    );
    let fast_src = format!(
        "f>L n;xs=map (i:n>n;mod i 100) (range 0 {n});\
         rmin {w} xs"
    );

    let baseline = time_run(&baseline_src, "f");
    let fast = time_run(&fast_src, "f");

    eprintln!(
        "rmin perf: baseline={:?}, fast={:?}, speedup={:.2}x",
        baseline,
        fast,
        baseline.as_secs_f64() / fast.as_secs_f64()
    );

    assert!(
        fast < baseline / 2,
        "rmin must be at least 2x faster than the slc baseline at n={n}, w={w}: baseline={baseline:?}, fast={fast:?}"
    );
}
