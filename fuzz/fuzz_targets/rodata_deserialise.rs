#![no_main]

use libfuzzer_sys::fuzz_target;

// Feed arbitrary bytes into the `.rodata` deserialiser.
//
// The invariant under test: `deserialize_program` must either
//   (a) return `Ok(CompiledProgram)` with well-formed internals, or
//   (b) return `Err(String)` with a descriptive message.
//
// It must NEVER panic, abort, or exhibit undefined behaviour regardless
// of what bytes the fuzzer supplies.
//
// Run with:
//   cargo +nightly fuzz run rodata_deserialise -- -max_total_time=600
//
// See fuzz/README.md for corpus management and CI setup.
fuzz_target!(|data: &[u8]| {
    let result = ilo::vm::aot_blob::deserialize_program(data);

    if let Ok(program) = result {
        // Spot-check structural invariants that must hold after a successful
        // deserialise — any violation here is a logic bug in the deserialiser.
        assert_eq!(
            program.chunks.len(),
            program.func_names.len(),
            "chunk count must equal func_names count"
        );
        assert_eq!(
            program.chunks.len(),
            program.nan_constants.len(),
            "chunk count must equal nan_constants count"
        );
        for (i, (chunk, nans)) in program
            .chunks
            .iter()
            .zip(program.nan_constants.iter())
            .enumerate()
        {
            assert_eq!(
                chunk.constants.len(),
                nans.len(),
                "chunk {i}: constants and nan_constants must have equal length"
            );
        }
    }
    // Err(_) is fine — the deserialiser is expected to reject bad input cleanly.
});
