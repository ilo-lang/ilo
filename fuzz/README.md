# ilo fuzz targets

## Prerequisites

Fuzzing with cargo-fuzz requires the nightly Rust toolchain and cargo-fuzz:

```sh
rustup install nightly
cargo install cargo-fuzz
```

## rodata_deserialise

Feeds arbitrary bytes into `ilo::vm::aot_blob::deserialize_program` — the
function that runs at AOT binary startup to reconstruct a `CompiledProgram`
from the `.rodata` blob.

**Invariant under test:** the deserialiser must either return a well-formed
`CompiledProgram` (with `chunks.len() == func_names.len() == nan_constants.len()`)
or return a descriptive `Err` string.  It must never panic or exhibit UB.

### Quick run (10 minutes)

```sh
cargo +nightly fuzz run rodata_deserialise -- -max_total_time=600
```

### Overnight run with ASAN

```sh
cargo +nightly fuzz run rodata_deserialise \
    --sanitizer address \
    -- -max_total_time=28800
```

### Corpus management

Interesting inputs found by the fuzzer land in `fuzz/corpus/rodata_deserialise/`.
Commit new corpus entries so future runs start from a richer seed.

To minimise the corpus:

```sh
cargo +nightly fuzz cmin rodata_deserialise
```

### CI

See `.github/workflows/fuzz.yml` for the nightly CI job that runs a 10-minute
fuzz pass on every push to `main`.
