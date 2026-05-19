//! Core wasm module encoder for the WASM backend.
//!
//! Emits a WASI preview1 core module that writes the supplied strings to
//! stdout via `fd_write` then exits with status 0. The shape is deliberately
//! minimal: one memory, one imported host function, one exported `_start`
//! function that runs all `fd_write` calls in sequence.
//!
//! Stage 5d does not yet emit arithmetic, loops, or branches — those land
//! when the HIR walker grows. The encoder is structured so adding them is
//! local: every new HIR construct lowers into another sequence of wasm
//! instructions appended to `_start` (or a dedicated function plus call).

use wasm_encoder::{
    CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection, Function,
    FunctionSection, ImportSection, Instruction, MemArg, MemorySection, MemoryType, Module,
    TypeSection, ValType,
};

use super::WasmTarget;

/// Capability flags driving which imports the encoder declares.
///
/// Today only `needs_stdout` is wired; future capabilities (clock, random,
/// filesystem, http) will add more flags and toggle the relevant WASI
/// import. Keeping this as a struct rather than bitflags lets each new
/// capability carry attached config without a downstream bitflag refactor.
#[derive(Debug, Clone, Copy)]
pub struct CapabilitySet {
    /// WASM target the module is being emitted for. Drives the import shape
    /// (preview1 `wasi_snapshot_preview1.fd_write` vs the eventual
    /// preview2/component module imports).
    pub target: WasmTarget,
    /// True if any `prnt` call appeared and the module must wire stdout.
    pub needs_stdout: bool,
}

/// Emit a WASI core module that prints each string in `strings` to stdout
/// then returns from `_start`.
///
/// On [`WasmTarget::UnknownUnknown`] with `needs_stdout = false`, emits an
/// empty `_start` returning immediately. With `needs_stdout = true` on
/// unknown-unknown the caller is expected to have already errored at
/// capability-check time; we defensively skip the import here so a misuse
/// produces an empty module rather than an invalid one.
pub fn emit_core_module(
    strings: &[String],
    caps: CapabilitySet,
) -> Result<Vec<u8>, String> {
    let mut module = Module::new();

    // ---- type section -----------------------------------------------------
    //
    // type 0: (i32, i32, i32, i32) -> i32   -- fd_write signature
    // type 1: () -> ()                       -- _start signature
    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32, ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function([], []);
    module.section(&types);

    // ---- import section ---------------------------------------------------
    let mut imports = ImportSection::new();
    let has_fd_write = caps.needs_stdout
        && matches!(
            caps.target,
            WasmTarget::Wasip1 | WasmTarget::Wasip2 | WasmTarget::Component
        );
    if has_fd_write {
        imports.import(
            "wasi_snapshot_preview1",
            "fd_write",
            EntityType::Function(0),
        );
    }
    module.section(&imports);

    // ---- function section -------------------------------------------------
    //
    // Local function indices come after imports. With fd_write imported,
    // function 0 = fd_write (host), function 1 = _start (us). Without it,
    // function 0 = _start.
    let mut functions = FunctionSection::new();
    functions.function(1); // _start
    module.section(&functions);

    // ---- memory section --------------------------------------------------
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memories);

    // ---- export section --------------------------------------------------
    let mut exports = ExportSection::new();
    let start_idx = if has_fd_write { 1 } else { 0 };
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("_start", ExportKind::Func, start_idx);
    module.section(&exports);

    // Section ordering note: the WASM core spec requires sections to appear
    // in: type, import, function, table, memory, global, export, start,
    // element, datacount, code, data. Data comes after code — we encode
    // the data segments below but `module.section(&data)` is appended last,
    // after the code section.

    // ---- data section ----------------------------------------------------
    //
    // Layout in linear memory:
    //
    //   [iov_base | iov_len]  -- one 8-byte iovec per string, packed at offset 0
    //   <strings>             -- string bytes follow
    //   [nwritten]             -- last 4 bytes are the fd_write nwritten output
    //
    // We compute offsets statically: iovec table size = 8 * N, then string
    // data, then nwritten slot.
    let mut data = DataSection::new();

    let iovec_table_size = (strings.len() * 8) as u32;
    let strings_start: u32 = iovec_table_size;
    let mut string_offsets: Vec<(u32, u32)> = Vec::with_capacity(strings.len());

    // Lay out strings (each followed by a newline for `prnt` semantics).
    {
        let mut payload: Vec<u8> = Vec::new();
        let mut cursor = strings_start;
        for s in strings {
            let mut bytes: Vec<u8> = s.as_bytes().to_vec();
            bytes.push(b'\n');
            let len = bytes.len() as u32;
            string_offsets.push((cursor, len));
            payload.extend_from_slice(&bytes);
            cursor += len;
        }
        if !payload.is_empty() {
            data.active(0, &ConstExpr::i32_const(strings_start as i32), payload);
        }

        // Lay out iovec table contents as a separate data segment so it can
        // reference the string offsets above.
        if !strings.is_empty() {
            let mut iov_bytes: Vec<u8> = Vec::with_capacity(strings.len() * 8);
            for (off, len) in &string_offsets {
                iov_bytes.extend_from_slice(&off.to_le_bytes());
                iov_bytes.extend_from_slice(&len.to_le_bytes());
            }
            data.active(0, &ConstExpr::i32_const(0), iov_bytes);
        }
    }

    // ---- code section ----------------------------------------------------
    let mut codes = CodeSection::new();
    let mut func = Function::new([]);

    if has_fd_write && !strings.is_empty() {
        // Compute nwritten slot offset — just past the strings. WASI
        // fd_write writes a 4-byte i32 to this pointer, so align to 4.
        let raw_end: u32 = string_offsets
            .last()
            .map(|(off, len)| off + len)
            .unwrap_or(iovec_table_size);
        let nwritten_offset: u32 = (raw_end + 3) & !3u32;

        // One fd_write call per string. Could batch into a single call with
        // multiple iovecs, but per-string is simpler to reason about and
        // matches `prnt`'s line-at-a-time semantics on flushing/buffering.
        for (i, _) in strings.iter().enumerate() {
            let iov_addr = (i * 8) as i32; // pointer to this iovec
            // fd = 1 (stdout)
            func.instruction(&Instruction::I32Const(1));
            // iovs ptr
            func.instruction(&Instruction::I32Const(iov_addr));
            // iovs len = 1
            func.instruction(&Instruction::I32Const(1));
            // nwritten ptr
            func.instruction(&Instruction::I32Const(nwritten_offset as i32));
            // call fd_write (import index 0)
            func.instruction(&Instruction::Call(0));
            // drop returned errno — Stage 5d ignores write failures; we'll
            // surface them once HIR carries Result-aware tail handling.
            func.instruction(&Instruction::Drop);
        }

        // Touch nwritten_offset so wasmparser knows the high water mark is
        // legitimately part of memory. (The active data segment for strings
        // already covers it implicitly; this comment exists to flag where
        // we'd add a bss-style reservation if we ever shrink the data seg.)
        let _ = MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        };
    }

    func.instruction(&Instruction::End);
    codes.function(&func);
    module.section(&codes);

    // Data section must come AFTER code per the wasm core spec.
    module.section(&data);

    Ok(module.finish())
}
