//! Hand-rolled ARM64 JIT backend for numeric-only functions.
//!
//! Emits raw AArch64 machine code: fmul/fadd/fsub/fdiv/fneg/fmov + ret.
//! VM registers R0-R30 map directly to hardware FP registers d0-d30.
//! Function args arrive in d0-d7 per AAPCS64.

use super::*;

/// Check if a chunk uses only numeric-safe opcodes (no heap, no jumps, no calls).
pub(crate) fn is_jit_eligible(chunk: &Chunk) -> bool {
    for &inst in &chunk.code {
        let op = (inst >> 24) as u8;
        match op {
            OP_ADD_NN | OP_SUB_NN | OP_MUL_NN | OP_DIV_NN |
            OP_ADDK_N | OP_SUBK_N | OP_MULK_N | OP_DIVK_N |
            OP_MOVE | OP_NEG | OP_RET => {}
            OP_LOADK => {
                // Only number constants are eligible
                let bx = (inst & 0xFFFF) as usize;
                if bx >= chunk.constants.len() { return false; }
                if !matches!(chunk.constants[bx], Value::Number(_)) { return false; }
            }
            _ => return false,
        }
    }
    true
}

/// Compile a numeric chunk into native ARM64 code.
/// Returns None if the chunk isn't eligible or compilation fails.
pub fn compile(chunk: &Chunk, nan_consts: &[NanVal]) -> Option<JitFunction> {
    if !is_jit_eligible(chunk) { return None; }
    let mut emitter = Arm64Emitter::new();
    emitter.compile(chunk, nan_consts)?;
    emitter.finalize()
}

/// Call a JIT-compiled function with the given f64 args.
pub fn call(func: &JitFunction, args: &[f64]) -> Option<f64> {
    if args.len() > 8 { return None; }
    Some(match args.len() {
        0 => func.call_0(),
        1 => func.call_1(args[0]),
        2 => func.call_2(args[0], args[1]),
        3 => func.call_3(args[0], args[1], args[2]),
        4 => func.call_4(args[0], args[1], args[2], args[3]),
        5 => func.call_5(args[0], args[1], args[2], args[3], args[4]),
        6 => func.call_6(args[0], args[1], args[2], args[3], args[4], args[5]),
        7 => func.call_7(args[0], args[1], args[2], args[3], args[4], args[5], args[6]),
        8 => func.call_8(args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7]),
        _ => return None,
    })
}

/// Compile and call in one shot (convenience for --run-jit).
pub fn compile_and_call(chunk: &Chunk, nan_consts: &[NanVal], args: &[f64]) -> Option<f64> {
    let func = compile(chunk, nan_consts)?;
    call(&func, args)
}

// ── ARM64 instruction encodings ────────────────────────────────────

fn arm64_fadd(rd: u8, rn: u8, rm: u8) -> u32 {
    0x1E602800 | ((rm as u32) << 16) | ((rn as u32) << 5) | rd as u32
}

fn arm64_fsub(rd: u8, rn: u8, rm: u8) -> u32 {
    0x1E603800 | ((rm as u32) << 16) | ((rn as u32) << 5) | rd as u32
}

fn arm64_fmul(rd: u8, rn: u8, rm: u8) -> u32 {
    0x1E600800 | ((rm as u32) << 16) | ((rn as u32) << 5) | rd as u32
}

fn arm64_fdiv(rd: u8, rn: u8, rm: u8) -> u32 {
    0x1E601800 | ((rm as u32) << 16) | ((rn as u32) << 5) | rd as u32
}

fn arm64_fneg(rd: u8, rn: u8) -> u32 {
    0x1E614000 | ((rn as u32) << 5) | rd as u32
}

fn arm64_fmov(rd: u8, rn: u8) -> u32 {
    0x1E604000 | ((rn as u32) << 5) | rd as u32
}

fn arm64_ret() -> u32 {
    0xD65F03C0
}

/// LDR Dd, [Xn, #imm] — load f64 from [Xn + unsigned_offset]
/// Encoding: size=11 V=1 opc=01 imm12 Rn Rt
/// imm12 is offset / 8 (scaled by 8 for 64-bit loads)
fn arm64_ldr_d_imm(dt: u8, xn: u8, offset_bytes: u32) -> u32 {
    let imm12 = offset_bytes / 8;
    0xFD400000 | ((imm12 & 0xFFF) << 10) | ((xn as u32) << 5) | dt as u32
}

/// ADRP Xd, #imm — compute page address (we'll use ADR for small offsets)
/// ADR Xd, #imm21 — Xd = PC + imm
fn arm64_adr(xd: u8, imm: i32) -> u32 {
    let imm_lo = (imm & 0x3) as u32;
    let imm_hi = ((imm >> 2) & 0x7FFFF) as u32;
    0x10000000 | (imm_lo << 29) | (imm_hi << 5) | xd as u32
}

// ── Emitter ────────────────────────────────────────────────────────

struct Arm64Emitter {
    code: Vec<u32>,
    /// f64 constants to embed after code section
    const_pool: Vec<f64>,
    /// Map from (instruction index in original bytecode) to const_pool index
    /// Used for LOADK and *K_N instructions
    const_refs: Vec<(usize, usize)>, // (code_offset, const_pool_index)
}

impl Arm64Emitter {
    fn new() -> Self {
        Arm64Emitter {
            code: Vec::with_capacity(64),
            const_pool: Vec::new(),
            const_refs: Vec::new(),
        }
    }

    fn add_const(&mut self, val: f64) -> usize {
        for (i, &c) in self.const_pool.iter().enumerate() {
            if c.to_bits() == val.to_bits() { return i; }
        }
        let idx = self.const_pool.len();
        self.const_pool.push(val);
        idx
    }

    fn emit(&mut self, inst: u32) -> usize {
        let idx = self.code.len();
        self.code.push(inst);
        idx
    }

    fn compile(&mut self, chunk: &Chunk, nan_consts: &[NanVal]) -> Option<()> {
        for &inst in &chunk.code {
            let op = (inst >> 24) as u8;
            let a = ((inst >> 16) & 0xFF) as u8;
            let b = ((inst >> 8) & 0xFF) as u8;
            let c = (inst & 0xFF) as u8;

            match op {
                OP_ADD_NN => { self.emit(arm64_fadd(a, b, c)); }
                OP_SUB_NN => { self.emit(arm64_fsub(a, b, c)); }
                OP_MUL_NN => { self.emit(arm64_fmul(a, b, c)); }
                OP_DIV_NN => { self.emit(arm64_fdiv(a, b, c)); }

                OP_ADDK_N => {
                    let kv = nan_consts.get(c as usize)?.as_number();
                    let ci = self.add_const(kv);
                    // Placeholder: ADR x16, <const> + LDR d31, [x16] + FADD
                    let off = self.code.len();
                    self.emit(0); // placeholder ADR x16
                    self.emit(arm64_ldr_d_imm(31, 16, 0)); // LDR d31, [x16]
                    self.const_refs.push((off, ci));
                    self.emit(arm64_fadd(a, b, 31));
                }
                OP_SUBK_N => {
                    let kv = nan_consts.get(c as usize)?.as_number();
                    let ci = self.add_const(kv);
                    let off = self.code.len();
                    self.emit(0);
                    self.emit(arm64_ldr_d_imm(31, 16, 0));
                    self.const_refs.push((off, ci));
                    self.emit(arm64_fsub(a, b, 31));
                }
                OP_MULK_N => {
                    let kv = nan_consts.get(c as usize)?.as_number();
                    let ci = self.add_const(kv);
                    let off = self.code.len();
                    self.emit(0);
                    self.emit(arm64_ldr_d_imm(31, 16, 0));
                    self.const_refs.push((off, ci));
                    self.emit(arm64_fmul(a, b, 31));
                }
                OP_DIVK_N => {
                    let kv = nan_consts.get(c as usize)?.as_number();
                    let ci = self.add_const(kv);
                    let off = self.code.len();
                    self.emit(0);
                    self.emit(arm64_ldr_d_imm(31, 16, 0));
                    self.const_refs.push((off, ci));
                    self.emit(arm64_fdiv(a, b, 31));
                }

                OP_LOADK => {
                    let bx = (inst & 0xFFFF) as usize;
                    let val = match &chunk.constants[bx] {
                        Value::Number(n) => *n,
                        _ => return None,
                    };
                    let ci = self.add_const(val);
                    let off = self.code.len();
                    self.emit(0); // placeholder ADR x16
                    self.emit(arm64_ldr_d_imm(a, 16, 0)); // LDR Da, [x16]
                    self.const_refs.push((off, ci));
                }

                OP_MOVE => {
                    if a != b {
                        self.emit(arm64_fmov(a, b));
                    }
                }

                OP_NEG => {
                    self.emit(arm64_fneg(a, b));
                }

                OP_RET => {
                    // Move result to d0 if not already there
                    if a != 0 {
                        self.emit(arm64_fmov(0, a));
                    }
                    self.emit(arm64_ret());
                }

                _ => return None,
            }
        }
        Some(())
    }

    fn finalize(mut self) -> Option<JitFunction> {
        // Align code to 8 bytes for const pool (code is 4-byte aligned, add NOP if odd)
        if self.code.len() % 2 != 0 {
            self.emit(0xD503201F); // NOP
        }

        let code_bytes = self.code.len() * 4;

        // Patch const references: ADR x16, <offset_to_const>
        for &(adr_idx, const_idx) in &self.const_refs {
            let adr_pc = adr_idx * 4; // byte offset of the ADR instruction
            let const_offset = code_bytes + const_idx * 8; // byte offset of the constant
            let imm = const_offset as i32 - adr_pc as i32;
            self.code[adr_idx] = arm64_adr(16, imm);
        }

        // Build final buffer: code + const pool
        let total_bytes = code_bytes + self.const_pool.len() * 8;
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
        let alloc_size = (total_bytes + page_size - 1) & !(page_size - 1);

        unsafe {
            // Allocate RW memory with MAP_JIT on macOS
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                alloc_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_JIT,
                -1,
                0,
            );
            if ptr == libc::MAP_FAILED { return None; }

            // macOS: allow writing to JIT memory
            pthread_jit_write_protect_np(0);

            // Copy code
            std::ptr::copy_nonoverlapping(
                self.code.as_ptr() as *const u8,
                ptr as *mut u8,
                code_bytes,
            );

            // Copy constant pool after code
            std::ptr::copy_nonoverlapping(
                self.const_pool.as_ptr() as *const u8,
                (ptr as *mut u8).add(code_bytes),
                self.const_pool.len() * 8,
            );

            // Switch to execute-only
            pthread_jit_write_protect_np(1);

            // Make executable; if this fails, clean up and bail
            if libc::mprotect(ptr, alloc_size, libc::PROT_READ | libc::PROT_EXEC) != 0 {
                libc::munmap(ptr, alloc_size);
                return None;
            }

            // Flush instruction cache
            sys_icache_invalidate(ptr, alloc_size);

            Some(JitFunction { ptr, size: alloc_size })
        }
    }
}

// ── JIT function wrapper ───────────────────────────────────────────

pub struct JitFunction {
    ptr: *mut libc::c_void,
    size: usize,
}

/// # Safety (internal — all `call_N` methods)
/// Each `transmute` casts the mmap'd JIT code pointer to a typed `extern "C"` fn.
/// This is sound because:
/// 1. `compile()` emits raw ARM64 machine code using the AAPCS64 calling
///    convention: f64 params in d0..d7, f64 return in d0.
/// 2. The pointer comes from `mmap(PROT_READ|PROT_EXEC)` after writing the
///    machine code and flushing the instruction cache.
/// 3. The caller matches the param count to the compiled function's arity.
impl JitFunction {
    fn call_0(&self) -> f64 {
        // SAFETY: see JitFunction doc — 0 f64 params, returns f64 in d0.
        let f: extern "C" fn() -> f64 = unsafe { std::mem::transmute(self.ptr) };
        f()
    }
    fn call_1(&self, a0: f64) -> f64 {
        // SAFETY: see JitFunction doc — 1 f64 param in d0, returns f64.
        let f: extern "C" fn(f64) -> f64 = unsafe { std::mem::transmute(self.ptr) };
        f(a0)
    }
    fn call_2(&self, a0: f64, a1: f64) -> f64 {
        let f: extern "C" fn(f64, f64) -> f64 = unsafe { std::mem::transmute(self.ptr) };
        f(a0, a1)
    }
    fn call_3(&self, a0: f64, a1: f64, a2: f64) -> f64 {
        let f: extern "C" fn(f64, f64, f64) -> f64 = unsafe { std::mem::transmute(self.ptr) };
        f(a0, a1, a2)
    }
    fn call_4(&self, a0: f64, a1: f64, a2: f64, a3: f64) -> f64 {
        let f: extern "C" fn(f64, f64, f64, f64) -> f64 = unsafe { std::mem::transmute(self.ptr) };
        f(a0, a1, a2, a3)
    }
    fn call_5(&self, a0: f64, a1: f64, a2: f64, a3: f64, a4: f64) -> f64 {
        let f: extern "C" fn(f64, f64, f64, f64, f64) -> f64 = unsafe { std::mem::transmute(self.ptr) };
        f(a0, a1, a2, a3, a4)
    }
    fn call_6(&self, a0: f64, a1: f64, a2: f64, a3: f64, a4: f64, a5: f64) -> f64 {
        let f: extern "C" fn(f64, f64, f64, f64, f64, f64) -> f64 = unsafe { std::mem::transmute(self.ptr) };
        f(a0, a1, a2, a3, a4, a5)
    }
    #[allow(clippy::too_many_arguments)]
    fn call_7(&self, a0: f64, a1: f64, a2: f64, a3: f64, a4: f64, a5: f64, a6: f64) -> f64 {
        let f: extern "C" fn(f64, f64, f64, f64, f64, f64, f64) -> f64 = unsafe { std::mem::transmute(self.ptr) };
        f(a0, a1, a2, a3, a4, a5, a6)
    }
    #[allow(clippy::too_many_arguments)]
    fn call_8(&self, a0: f64, a1: f64, a2: f64, a3: f64, a4: f64, a5: f64, a6: f64, a7: f64) -> f64 {
        let f: extern "C" fn(f64, f64, f64, f64, f64, f64, f64, f64) -> f64 = unsafe { std::mem::transmute(self.ptr) };
        f(a0, a1, a2, a3, a4, a5, a6, a7)
    }
}

impl Drop for JitFunction {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr, self.size); }
    }
}

// ── macOS-specific JIT support ─────────────────────────────────────

unsafe extern "C" {
    fn pthread_jit_write_protect_np(enabled: i32);
    fn sys_icache_invalidate(start: *mut libc::c_void, size: usize);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    /// Compile ilo source, extract the named function's chunk, JIT-compile it
    /// via the ARM64 backend, and call it with the given f64 args.
    fn jit_run_numeric(source: &str, func_name: &str, args: &[f64]) -> Option<f64> {
        let tokens: Vec<crate::lexer::Token> = lexer::lex(source)
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == func_name)?;
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        compile_and_call(chunk, nan_consts, args)
    }

    // ── Basic arithmetic ───────────────────────────────────────────────

    #[test]
    fn arm64_add_nn() {
        let result = jit_run_numeric("f a:n b:n>n;+a b", "f", &[3.0, 7.0]);
        assert_eq!(result, Some(10.0));
    }

    #[test]
    fn arm64_sub_nn() {
        let result = jit_run_numeric("f a:n b:n>n;-a b", "f", &[10.0, 3.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn arm64_mul_nn() {
        let result = jit_run_numeric("f a:n b:n>n;*a b", "f", &[4.0, 5.0]);
        assert_eq!(result, Some(20.0));
    }

    #[test]
    fn arm64_div_nn() {
        let result = jit_run_numeric("f a:n b:n>n;/a b", "f", &[10.0, 2.0]);
        assert_eq!(result, Some(5.0));
    }

    #[test]
    fn arm64_neg() {
        let result = jit_run_numeric("f x:n>n;-x", "f", &[5.0]);
        assert_eq!(result, Some(-5.0));
    }

    // ── Constant operations ────────────────────────────────────────────

    #[test]
    fn arm64_addk_n() {
        let result = jit_run_numeric("f x:n>n;+x 10", "f", &[5.0]);
        assert_eq!(result, Some(15.0));
    }

    #[test]
    fn arm64_subk_n() {
        let result = jit_run_numeric("f x:n>n;-x 3", "f", &[10.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn arm64_mulk_n() {
        let result = jit_run_numeric("f x:n>n;*x 4", "f", &[5.0]);
        assert_eq!(result, Some(20.0));
    }

    #[test]
    fn arm64_divk_n() {
        let result = jit_run_numeric("f x:n>n;/x 4", "f", &[20.0]);
        assert_eq!(result, Some(5.0));
    }

    #[test]
    fn arm64_loadk_constant() {
        let result = jit_run_numeric("f>n;42", "f", &[]);
        assert_eq!(result, Some(42.0));
    }

    // ── Move and identity ──────────────────────────────────────────────

    #[test]
    fn arm64_move_passthrough() {
        let result = jit_run_numeric("f x:n>n;x", "f", &[7.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn arm64_move_via_let_binding() {
        let result = jit_run_numeric("f x:n>n;y=x;y", "f", &[7.0]);
        assert_eq!(result, Some(7.0));
    }

    // ── Multi-arg functions (0-4 args) ─────────────────────────────────

    #[test]
    fn arm64_zero_args() {
        let result = jit_run_numeric("f>n;99", "f", &[]);
        assert_eq!(result, Some(99.0));
    }

    #[test]
    fn arm64_one_arg() {
        let result = jit_run_numeric("f x:n>n;+x 1", "f", &[41.0]);
        assert_eq!(result, Some(42.0));
    }

    #[test]
    fn arm64_two_args() {
        let result = jit_run_numeric("f a:n b:n>n;+a b", "f", &[3.0, 4.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn arm64_three_args() {
        let result = jit_run_numeric("f a:n b:n c:n>n;+a +b c", "f", &[1.0, 2.0, 3.0]);
        assert_eq!(result, Some(6.0));
    }

    #[test]
    fn arm64_four_args() {
        let result = jit_run_numeric("f a:n b:n c:n d:n>n;+a +b +c d", "f", &[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(result, Some(10.0));
    }

    // ── Arg count mismatch ─────────────────────────────────────────────

    #[test]
    fn arm64_too_many_args_returns_none() {
        // call() rejects > 8 args
        let result = call_with_n_args(9);
        assert_eq!(result, None);
    }

    /// Helper: build a trivial JIT function and call it with `n` args.
    fn call_with_n_args(n: usize) -> Option<f64> {
        // Compile a simple 0-arg function that returns 0
        let result = jit_run_numeric("f>n;0", "f", &[]);
        // That should succeed
        assert!(result.is_some());

        // Now test the call() function's arg-count guard directly
        let tokens: Vec<crate::lexer::Token> = lexer::lex("f>n;0")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|nm| nm == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let func = compile(chunk, nan_consts)?;
        let args: Vec<f64> = (0..n).map(|i| i as f64).collect();
        call(&func, &args)
    }

    // ── Drop impl (munmap) ─────────────────────────────────────────────

    #[test]
    fn arm64_drop_does_not_crash() {
        // Compile a function, then drop it — verifies munmap works
        let tokens: Vec<crate::lexer::Token> = lexer::lex("f>n;1")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let func = compile(chunk, nan_consts).expect("should compile");
        // Explicit drop — if munmap is buggy this would crash
        drop(func);
    }

    // ── Eligibility checks ─────────────────────────────────────────────

    #[test]
    fn arm64_ineligible_function_returns_none() {
        // A function with string ops isn't JIT-eligible
        let tokens: Vec<crate::lexer::Token> = lexer::lex(r#"f a:t b:t>t;+a b"#)
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        assert!(compile(chunk, nan_consts).is_none());
    }

    // ── Compound expressions ───────────────────────────────────────────

    #[test]
    fn arm64_compound_arithmetic() {
        // (a + b) * (a - b)
        let result = jit_run_numeric("f a:n b:n>n;* +a b -a b", "f", &[5.0, 3.0]);
        assert_eq!(result, Some(16.0)); // 8 * 2
    }

    #[test]
    fn arm64_nested_constants() {
        // x * 2 + 10
        let result = jit_run_numeric("f x:n>n;+ *x 2 10", "f", &[5.0]);
        assert_eq!(result, Some(20.0));
    }
}
