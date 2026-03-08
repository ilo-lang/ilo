//! LLVM JIT backend for numeric-only functions.
//!
//! Uses the `inkwell` crate (safe Rust wrapper for LLVM C API).
//! LLVM brings the heaviest optimization pipeline (same passes as clang -O2).

use super::*;
use inkwell::context::Context;
use inkwell::OptimizationLevel;
use inkwell::targets::{InitializationConfig, Target};

/// Check if a chunk uses only numeric-safe opcodes.
pub(crate) fn is_jit_eligible(chunk: &Chunk) -> bool {
    for &inst in &chunk.code {
        let op = (inst >> 24) as u8;
        match op {
            OP_ADD_NN | OP_SUB_NN | OP_MUL_NN | OP_DIV_NN |
            OP_ADDK_N | OP_SUBK_N | OP_MULK_N | OP_DIVK_N |
            OP_MOVE | OP_NEG | OP_RET => {}
            OP_LOADK => {
                let bx = (inst & 0xFFFF) as usize;
                if bx >= chunk.constants.len() { return false; }
                if !matches!(chunk.constants[bx], Value::Number(_)) { return false; }
            }
            _ => return false,
        }
    }
    true
}

/// Compiled LLVM function.
pub(crate) struct JitFunction {
    _context: Context,
    func_ptr: *const u8,
    param_count: usize,
}

unsafe impl Send for JitFunction {}

/// Compile a chunk into native code via LLVM.
pub(crate) fn compile(chunk: &Chunk, nan_consts: &[NanVal]) -> Option<JitFunction> {
    if !is_jit_eligible(chunk) { return None; }

    Target::initialize_native(&InitializationConfig::default()).ok()?;

    let context = Context::create();
    let module = context.create_module("jit");
    let builder = context.create_builder();

    // Build function type: (f64, f64, ...) -> f64
    let f64_type = context.f64_type();
    let param_types: Vec<_> = (0..chunk.param_count).map(|_| f64_type.into()).collect();
    let fn_type = f64_type.fn_type(&param_types, false);
    let function = module.add_function("jit_func", fn_type, None);

    let entry = context.append_basic_block(function, "entry");
    builder.position_at_end(entry);

    // Map VM registers to LLVM values
    let reg_count = chunk.reg_count.max(chunk.param_count) as usize;
    let mut regs: Vec<inkwell::values::FloatValue> = Vec::with_capacity(reg_count);

    // Initialize from params
    for i in 0..chunk.param_count as usize {
        regs.push(function.get_nth_param(i as u32).unwrap().into_float_value());
    }
    // Initialize remaining to 0.0
    for _ in chunk.param_count as usize..reg_count {
        regs.push(f64_type.const_float(0.0));
    }

    // Translate bytecode
    for &inst in &chunk.code {
        let op = (inst >> 24) as u8;
        let a = ((inst >> 16) & 0xFF) as usize;
        let b = ((inst >> 8) & 0xFF) as usize;
        let c = (inst & 0xFF) as usize;

        match op {
            OP_ADD_NN => {
                let result = builder.build_float_add(regs[b], regs[c], "add").ok()?;
                regs[a] = result;
            }
            OP_SUB_NN => {
                let result = builder.build_float_sub(regs[b], regs[c], "sub").ok()?;
                regs[a] = result;
            }
            OP_MUL_NN => {
                let result = builder.build_float_mul(regs[b], regs[c], "mul").ok()?;
                regs[a] = result;
            }
            OP_DIV_NN => {
                let result = builder.build_float_div(regs[b], regs[c], "div").ok()?;
                regs[a] = result;
            }
            OP_ADDK_N => {
                let kv = nan_consts.get(c)?.as_number();
                let kval = f64_type.const_float(kv);
                let result = builder.build_float_add(regs[b], kval, "addk").ok()?;
                regs[a] = result;
            }
            OP_SUBK_N => {
                let kv = nan_consts.get(c)?.as_number();
                let kval = f64_type.const_float(kv);
                let result = builder.build_float_sub(regs[b], kval, "subk").ok()?;
                regs[a] = result;
            }
            OP_MULK_N => {
                let kv = nan_consts.get(c)?.as_number();
                let kval = f64_type.const_float(kv);
                let result = builder.build_float_mul(regs[b], kval, "mulk").ok()?;
                regs[a] = result;
            }
            OP_DIVK_N => {
                let kv = nan_consts.get(c)?.as_number();
                let kval = f64_type.const_float(kv);
                let result = builder.build_float_div(regs[b], kval, "divk").ok()?;
                regs[a] = result;
            }
            OP_LOADK => {
                let bx = (inst & 0xFFFF) as usize;
                let val = match &chunk.constants[bx] {
                    Value::Number(n) => *n,
                    _ => return None,
                };
                regs[a] = f64_type.const_float(val);
            }
            OP_MOVE => {
                if a != b {
                    regs[a] = regs[b];
                }
            }
            OP_NEG => {
                let result = builder.build_float_neg(regs[b], "neg").ok()?;
                regs[a] = result;
            }
            OP_RET => {
                builder.build_return(Some(&regs[a])).ok()?;
            }
            _ => return None,
        }
    }

    // Create execution engine with O2 optimization
    let engine = module.create_jit_execution_engine(OptimizationLevel::Aggressive).ok()?;
    let func_ptr = engine.get_function_address("jit_func").ok()? as *const u8;

    // We need to keep the context alive — but execution engine owns the module.
    // SAFETY: The function pointer remains valid as long as context + engine live.
    // We leak the engine to keep the code alive (it's a one-shot JIT).
    std::mem::forget(engine);

    Some(JitFunction {
        _context: context,
        func_ptr,
        param_count: chunk.param_count as usize,
    })
}

/// Call a compiled function.
///
/// # Safety (internal)
/// Each `transmute` casts the LLVM JIT function pointer to a typed `extern "C"` fn.
/// This is sound because:
/// 1. `compile()` generates LLVM IR using the C calling convention with all
///    parameters and return value as `f64` (LLVMDoubleType).
/// 2. The function pointer is obtained from `LLVMGetFunctionAddress()` after
///    successful compilation by the LLVM MCJIT engine.
/// 3. `args.len() == func.param_count` is checked before dispatch.
pub(crate) fn call(func: &JitFunction, args: &[f64]) -> Option<f64> {
    if args.len() != func.param_count { return None; }
    Some(match args.len() {
        0 => {
            // SAFETY: see call() doc — LLVM compiled with 0 f64 params, returns f64.
            let f: extern "C" fn() -> f64 = unsafe { std::mem::transmute(func.func_ptr) };
            f()
        }
        1 => {
            // SAFETY: see call() doc — LLVM compiled with 1 f64 param, returns f64.
            let f: extern "C" fn(f64) -> f64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0])
        }
        2 => {
            let f: extern "C" fn(f64, f64) -> f64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1])
        }
        3 => {
            let f: extern "C" fn(f64, f64, f64) -> f64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2])
        }
        4 => {
            let f: extern "C" fn(f64, f64, f64, f64) -> f64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2], args[3])
        }
        5 => {
            let f: extern "C" fn(f64, f64, f64, f64, f64) -> f64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2], args[3], args[4])
        }
        6 => {
            let f: extern "C" fn(f64, f64, f64, f64, f64, f64) -> f64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2], args[3], args[4], args[5])
        }
        7 => {
            let f: extern "C" fn(f64, f64, f64, f64, f64, f64, f64) -> f64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2], args[3], args[4], args[5], args[6])
        }
        8 => {
            let f: extern "C" fn(f64, f64, f64, f64, f64, f64, f64, f64) -> f64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7])
        }
        _ => return None,
    })
}

/// Compile and call in one shot.
pub(crate) fn compile_and_call(chunk: &Chunk, nan_consts: &[NanVal], args: &[f64]) -> Option<f64> {
    let func = compile(chunk, nan_consts)?;
    call(&func, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    /// Compile ilo source, extract the named function's chunk, JIT-compile it
    /// via the LLVM backend, and call it with the given f64 args.
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
    fn llvm_add_nn() {
        let result = jit_run_numeric("f a:n b:n>n;+a b", "f", &[3.0, 7.0]);
        assert_eq!(result, Some(10.0));
    }

    #[test]
    fn llvm_sub_nn() {
        let result = jit_run_numeric("f a:n b:n>n;-a b", "f", &[10.0, 3.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn llvm_mul_nn() {
        let result = jit_run_numeric("f a:n b:n>n;*a b", "f", &[4.0, 5.0]);
        assert_eq!(result, Some(20.0));
    }

    #[test]
    fn llvm_div_nn() {
        let result = jit_run_numeric("f a:n b:n>n;/a b", "f", &[10.0, 2.0]);
        assert_eq!(result, Some(5.0));
    }

    #[test]
    fn llvm_neg() {
        let result = jit_run_numeric("f x:n>n;-x", "f", &[5.0]);
        assert_eq!(result, Some(-5.0));
    }

    // ── Constant operations ────────────────────────────────────────────

    #[test]
    fn llvm_addk_n() {
        let result = jit_run_numeric("f x:n>n;+x 10", "f", &[5.0]);
        assert_eq!(result, Some(15.0));
    }

    #[test]
    fn llvm_subk_n() {
        let result = jit_run_numeric("f x:n>n;-x 3", "f", &[10.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn llvm_mulk_n() {
        let result = jit_run_numeric("f x:n>n;*x 4", "f", &[5.0]);
        assert_eq!(result, Some(20.0));
    }

    #[test]
    fn llvm_divk_n() {
        let result = jit_run_numeric("f x:n>n;/x 4", "f", &[20.0]);
        assert_eq!(result, Some(5.0));
    }

    #[test]
    fn llvm_loadk_constant() {
        let result = jit_run_numeric("f>n;42", "f", &[]);
        assert_eq!(result, Some(42.0));
    }

    // ── Move and identity ──────────────────────────────────────────────

    #[test]
    fn llvm_move_passthrough() {
        let result = jit_run_numeric("f x:n>n;x", "f", &[7.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn llvm_move_via_let_binding() {
        let result = jit_run_numeric("f x:n>n;y=x;y", "f", &[7.0]);
        assert_eq!(result, Some(7.0));
    }

    // ── Multi-arg functions ────────────────────────────────────────────

    #[test]
    fn llvm_zero_args() {
        let result = jit_run_numeric("f>n;99", "f", &[]);
        assert_eq!(result, Some(99.0));
    }

    #[test]
    fn llvm_two_args() {
        let result = jit_run_numeric("f a:n b:n>n;+a b", "f", &[3.0, 4.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn llvm_four_args() {
        let result = jit_run_numeric("f a:n b:n c:n d:n>n;+a +b +c d", "f", &[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(result, Some(10.0));
    }

    // ── Arg count mismatch ─────────────────────────────────────────────

    #[test]
    fn llvm_arg_mismatch_returns_none() {
        // Function expects 2 args, called with 1
        let tokens: Vec<crate::lexer::Token> = lexer::lex("f a:n b:n>n;+a b")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let func = compile(chunk, nan_consts).unwrap();
        assert_eq!(call(&func, &[1.0]), None);
    }

    // ── Eligibility ────────────────────────────────────────────────────

    #[test]
    fn llvm_ineligible_function_returns_none() {
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
    fn llvm_compound_arithmetic() {
        let result = jit_run_numeric("f a:n b:n>n;* +a b -a b", "f", &[5.0, 3.0]);
        assert_eq!(result, Some(16.0));
    }

    #[test]
    fn llvm_nested_constants() {
        let result = jit_run_numeric("f x:n>n;+ *x 2 10", "f", &[5.0]);
        assert_eq!(result, Some(20.0));
    }
}
