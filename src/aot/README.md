# AOT `.rodata` deserialiser — invariants and safety audit

## Overview

AOT-compiled ilo binaries embed a serialised `CompiledProgram` in a `.rodata`
section.  At startup the cranelift-emitted `main` shim calls
`ilo_aot_publish_program(ptr, len)` (defined in `src/vm/mod.rs`) to
deserialise the blob and publish it into TLS slots used by HOF dispatch
helpers (`jit_call_dyn`, `jit_call_builtin_tree`).

Serialisation uses **postcard** for the VM bytecode (`ProgramBlob`) and
**serde_json** for the AST (see `src/vm/aot_blob.rs` for the rationale).

## Deserialiser invariants

After a successful `deserialize_program` call the returned `CompiledProgram`
satisfies:

1. `chunks.len() == func_names.len() == nan_constants.len() == is_tool.len()`
2. For every chunk `i`: `chunk.constants.len() == nan_constants[i].len()`
3. `schema_version == BLOB_SCHEMA_VERSION` (checked before any heap
   allocation; a mismatch returns `Err` immediately).
4. The `type_registry` is consistent: every `name_to_id` key maps to a valid
   index in `types`.

These invariants are spot-checked by the fuzz harness in
`fuzz/fuzz_targets/rodata_deserialise.rs`.

## `unsafe` audit

The following `unsafe` blocks are in the AOT rodata path.  Each carries a
`SAFETY` comment in the source; this table is the human-readable summary.

| Location | Block | Invariant | Violation scenario |
|---|---|---|---|
| `src/vm/mod.rs` `ilo_aot_publish_program` | `std::slice::from_raw_parts(ptr, len)` | `ptr` is a `.rodata` base address; `len` is its byte count — both are compile-time constants baked into the binary by the Cranelift codegen. Valid for process lifetime. | An untrusted caller passing arbitrary `ptr`/`len` (e.g. a future plugin ABI) would break the guarantee. |
| `src/vm/mod.rs` `jit_string_const` | `CStr::from_ptr(ptr)` | `ptr` is a `.rodata` NUL-terminated C string emitted by the Cranelift AOT codegen (NUL byte appended in `compile_cranelift.rs`). | A future codegen change that omits the NUL byte would cause an out-of-bounds scan. |
| `src/vm/mod.rs` `ilo_aot_parse_arg` | `CStr::from_ptr(ptr)` | `ptr` is `argv[i]` forwarded by the `main` shim as a u64. The OS guarantees NUL-termination for the duration of `main`. | Passing a computed u64 that is not an `argv` pointer would be UB. |
| `src/vm/compile_cranelift.rs` `OP_LOADK` | `nv.as_heap_ref()` | Called only after `nv.is_string()` is checked true; the NanVal points to a live `HeapObj::Str` owned by the constant pool. | A stale NanVal (freed or moved heap object) would produce a dangling reference. |

No `unsafe` blocks are present in `src/vm/aot_blob.rs` itself — the
serialisation/deserialisation logic is fully safe Rust via postcard + serde_json.

## Fuzzing

See `fuzz/README.md` and `.github/workflows/fuzz.yml`.

Target: `fuzz/fuzz_targets/rodata_deserialise.rs`  
Run: `cargo +nightly fuzz run rodata_deserialise -- -max_total_time=600`
