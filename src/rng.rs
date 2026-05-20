/// Shared RNG for all ilo engines (tree, VM, JIT, AOT).
///
/// Uses SplitMix64 — a minimal, deterministic, platform-independent PRNG.
/// All three properties are required for cross-engine reproducibility:
///   - Same algorithm everywhere (no crate-internal variation).
///   - Same default seed on every run (opt in to entropy via `seed (now)`).
///   - Same output on Mac, Linux, Windows for the same seed.
///
/// The state lives in a single `AtomicU64` so JIT-compiled helpers (plain
/// `extern "C"` functions with no context pointer) can reach it without
/// passing state through the call frame. ilo programs are single-threaded
/// by design, so relaxed loads/stores are safe — there is no concurrent
/// mutation.
///
/// Algorithm reference: Sebastiano Vigna, "Further scramblings of Marsaglia's
/// xorshift generators", 2018. Output quality: BigCrush passes with no
/// failures.
use std::sync::atomic::{AtomicU64, Ordering};

/// Default seed — deterministic, non-zero, and memorable.
/// Programs that want entropy should call `seed (now)`.
pub const DEFAULT_SEED: u64 = 0xCAFE_BABE_DEAD_BEEF;

static RNG_STATE: AtomicU64 = AtomicU64::new(DEFAULT_SEED);

/// Set the global PRNG state. Calling `seed n` in ilo maps here.
/// The value is stored directly as the SplitMix64 state — any u64 works
/// (including 0, unlike some older generators).
#[inline]
pub fn seed(s: u64) {
    RNG_STATE.store(s, Ordering::Relaxed);
}

/// Advance the PRNG by one step and return the next u64.
#[inline]
fn next_u64() -> u64 {
    // SplitMix64: add a Weyl-sequence constant, then scramble.
    let mut z = RNG_STATE
        .fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed)
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Return a uniform float in `[0, 1)`.
/// Uses the upper 53 bits so every representable f64 mantissa is reachable.
#[inline]
pub fn f64() -> f64 {
    // 53-bit mantissa — standard trick for uniform f64 in [0,1).
    (next_u64() >> 11) as f64 * (1.0_f64 / (1u64 << 53) as f64)
}

/// Return a uniform integer in `[lo, hi]` (inclusive on both ends).
/// Uses rejection sampling to avoid modulo bias.
#[inline]
pub fn i64_range(lo: i64, hi: i64) -> i64 {
    debug_assert!(lo <= hi);
    let range = (hi as u64).wrapping_sub(lo as u64).wrapping_add(1);
    if range == 0 {
        // Full u64 range — any value is valid.
        return next_u64() as i64;
    }
    // Rejection sampling: discard values in the biased tail.
    let limit = u64::MAX - (u64::MAX % range);
    loop {
        let v = next_u64();
        if v <= limit {
            return lo.wrapping_add((v % range) as i64);
        }
    }
}

/// Box-Muller transform: sample from N(mu, sigma).
/// Shared across all engines so `rndn` is deterministic too.
#[inline]
pub fn normal(mu: f64, sigma: f64) -> f64 {
    if sigma == 0.0 {
        return mu;
    }
    // Avoid u1 == 0 so ln() stays finite.
    let mut u1 = f64();
    while u1 <= std::f64::MIN_POSITIVE {
        u1 = f64();
    }
    let u2 = f64();
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    mu + sigma * z
}
