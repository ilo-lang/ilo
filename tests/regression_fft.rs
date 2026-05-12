// Cross-engine regression tests for the `fft` / `ifft` builtins.
//
// Inputs are zero-padded to the next power of two internally, so the
// expected output lengths follow that padding rather than the raw input
// length.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> &'static [&'static str] {
    // Tree-walker and register VM cover the same builtin code paths via
    // shared helpers; cranelift defers to the same Rust helpers via JIT
    // helper calls, so its behavior is covered by the VM path.
    &["--run-tree", "--run-vm"]
}

fn run_ok(engine: &str, src: &str, fn_name: &str) -> String {
    let out = ilo()
        .args([src, engine, fn_name])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, fn_name: &str) -> String {
    let out = ilo()
        .args([src, engine, fn_name])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// Parse the printed list-of-pairs representation into a flat Vec<f64>
/// of [re0, im0, re1, im1, ...] so the test can compare numerically.
fn parse_pairs(s: &str) -> Vec<f64> {
    // ilo prints lists like `[[1, 0], [0, 0], [0, 0], [0, 0]]` — strip
    // brackets and commas, then parse as floats.
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '[' | ']' | ',' => ' ',
            _ => c,
        })
        .collect();
    cleaned
        .split_whitespace()
        .map(|tok| tok.parse::<f64>().expect("non-numeric token in output"))
        .collect()
}

fn parse_reals(s: &str) -> Vec<f64> {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '[' | ']' | ',' => ' ',
            _ => c,
        })
        .collect();
    cleaned
        .split_whitespace()
        .map(|tok| tok.parse::<f64>().expect("non-numeric token in output"))
        .collect()
}

#[test]
fn fft_dc_unit_impulse() {
    // fft([1, 0, 0, 0]) should produce four bins each equal to (1, 0).
    let src = "f>L (L n);fft [1, 0, 0, 0]";
    for engine in engines() {
        let out = run_ok(engine, src, "f");
        let flat = parse_pairs(&out);
        assert_eq!(flat.len(), 8, "engine={engine}: got {out}");
        for chunk in flat.chunks_exact(2) {
            assert!(
                (chunk[0] - 1.0).abs() < 1e-10,
                "engine={engine}: re={}",
                chunk[0]
            );
            assert!(chunk[1].abs() < 1e-10, "engine={engine}: im={}", chunk[1]);
        }
    }
}

#[test]
fn fft_pure_cosine_has_single_nonzero_bin() {
    // cos(2*pi*k/8) for k in 0..8 — one cycle of a cosine sampled at 8
    // points. The FFT should produce magnitude peaks at bins 1 and 7 (the
    // conjugate symmetric pair), each with magnitude N/2 = 4. All other
    // bins should be ~0.
    let samples: Vec<f64> = (0..8)
        .map(|k| (2.0 * std::f64::consts::PI * (k as f64) / 8.0).cos())
        .collect();
    let lit = samples
        .iter()
        .map(|x| format!("{x}"))
        .collect::<Vec<_>>()
        .join(", ");
    let src = format!("f>L (L n);fft [{lit}]");
    for engine in engines() {
        let out = run_ok(engine, &src, "f");
        let flat = parse_pairs(&out);
        assert_eq!(flat.len(), 16, "engine={engine}: got {out}");
        for (i, chunk) in flat.chunks_exact(2).enumerate() {
            let mag = (chunk[0] * chunk[0] + chunk[1] * chunk[1]).sqrt();
            if i == 1 || i == 7 {
                assert!(
                    (mag - 4.0).abs() < 1e-10,
                    "engine={engine} bin {i}: mag={mag}"
                );
            } else {
                assert!(mag < 1e-10, "engine={engine} bin {i}: mag={mag}");
            }
        }
    }
}

#[test]
fn ifft_round_trip_recovers_input() {
    // Round-trip an exact power-of-two input (no zero padding) so the
    // recovered samples should match exactly within FP noise.
    let src = "f>L n;ifft (fft [1, 2, 3, 4])";
    for engine in engines() {
        let out = run_ok(engine, src, "f");
        let reals = parse_reals(&out);
        assert_eq!(reals.len(), 4, "engine={engine}: got {out}");
        let expected = [1.0, 2.0, 3.0, 4.0];
        for (i, (got, want)) in reals.iter().zip(expected.iter()).enumerate() {
            assert!(
                (got - want).abs() < 1e-10,
                "engine={engine} idx {i}: got {got}, want {want}"
            );
        }
    }
}

#[test]
fn ifft_round_trip_with_zero_padding() {
    // Non-power-of-two input: padded to length 4. After round-trip the
    // first three reals match; trailing entries are the zero-padding.
    let src = "f>L n;ifft (fft [1, 2, 3])";
    for engine in engines() {
        let out = run_ok(engine, src, "f");
        let reals = parse_reals(&out);
        assert_eq!(reals.len(), 4, "engine={engine}: got {out}");
        let expected = [1.0, 2.0, 3.0, 0.0];
        for (i, (got, want)) in reals.iter().zip(expected.iter()).enumerate() {
            assert!(
                (got - want).abs() < 1e-10,
                "engine={engine} idx {i}: got {got}, want {want}"
            );
        }
    }
}

#[test]
fn fft_empty_input_errors() {
    let src = "f>L (L n);fft []";
    for engine in engines() {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("fft") || err.contains("empty"),
            "engine={engine}: stderr={err}"
        );
    }
}

#[test]
fn fft_single_element_returns_single_pair() {
    // Single-element input is already a power of two (n=1); FFT of [x] is
    // [(x, 0)].
    let src = "f>L (L n);fft [7]";
    for engine in engines() {
        let out = run_ok(engine, src, "f");
        let flat = parse_pairs(&out);
        assert_eq!(flat.len(), 2, "engine={engine}: got {out}");
        assert!((flat[0] - 7.0).abs() < 1e-10, "engine={engine}");
        assert!(flat[1].abs() < 1e-10, "engine={engine}");
    }
}
