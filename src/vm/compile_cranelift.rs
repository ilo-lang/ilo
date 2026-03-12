//! Cranelift AOT (ahead-of-time) compiler — emits a standalone native binary.
//!
//! Reuses the same NanVal / I64 IR generation strategy as `jit_cranelift.rs`,
//! but targets `ObjectModule` instead of `JITModule` to produce a relocatable
//! `.o` file. A generated `main()` handles CLI arg parsing + result printing.
//! The object is linked with the system `cc` to produce the final executable.
//!
//! Phase 1: supports numeric-only programs (no heap helpers needed).
//! Unsupported opcodes produce a clear compile-time error.

use super::*;
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlags};
use cranelift_codegen::ir::types::{I32, I64, F64};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{default_libcall_names, Module, Linkage, FuncId};
use cranelift_object::{ObjectBuilder, ObjectModule};
use std::collections::HashMap;

/// Check whether a chunk can be AOT compiled with the current feature set.
fn check_aot_eligible(chunk: &Chunk, nan_consts: &[NanVal], func_name: &str) -> Result<(), String> {
    for (ip, &inst) in chunk.code.iter().enumerate() {
        let op = (inst >> 24) as u8;
        match op {
            // Fully supported opcodes
            OP_ADD_NN | OP_SUB_NN | OP_MUL_NN | OP_DIV_NN |
            OP_ADDK_N | OP_SUBK_N | OP_MULK_N | OP_DIVK_N |
            OP_EQ | OP_NE | OP_LT | OP_GT | OP_LE | OP_GE |
            OP_MOVE | OP_NEG | OP_NOT | OP_MOD |
            OP_JMP | OP_JMPF | OP_JMPT | OP_JMPNN | OP_RET => {}
            OP_LOADK => {
                let bx = (inst & 0xFFFF) as usize;
                if let Some(nv) = nan_consts.get(bx)
                    && NanVal(nv.0).is_heap()
                {
                    return Err(format!(
                        "function '{}' loads a heap value (string/list) at instruction {}, not yet supported for AOT",
                        func_name, ip
                    ));
                }
            }
            OP_CALL => return Err(format!(
                "function '{}' uses function calls at instruction {}, not yet supported for AOT",
                func_name, ip
            )),
            _ => return Err(format!(
                "function '{}' uses opcode {} at instruction {}, not yet supported for AOT compilation",
                func_name, op, ip
            )),
        }
    }
    Ok(())
}

/// Compile an ilo program to a standalone native binary.
pub(crate) fn compile_to_binary(
    program: &CompiledProgram,
    entry_func: &str,
    output_path: &str,
) -> Result<(), String> {
    let entry_idx = program.func_names.iter().position(|n| n == entry_func)
        .ok_or_else(|| format!("undefined function: {}", entry_func))?;

    let chunk = &program.chunks[entry_idx];
    let nan_consts = &program.nan_constants[entry_idx];
    check_aot_eligible(chunk, nan_consts, entry_func)?;

    // Set up Cranelift for the host target
    let mut flag_builder = settings::builder();
    flag_builder.set("opt_level", "speed").map_err(|e| e.to_string())?;
    flag_builder.set("is_pic", "true").map_err(|e| e.to_string())?;
    let isa_builder = cranelift_native::builder().map_err(|e| e.to_string())?;
    let isa = isa_builder.finish(settings::Flags::new(flag_builder)).map_err(|e| e.to_string())?;

    let obj_builder = ObjectBuilder::new(
        isa.clone(),
        "ilo_aot",
        default_libcall_names(),
    ).map_err(|e| e.to_string())?;
    let mut module = ObjectModule::new(obj_builder);

    // Generate a C runtime helper file with non-variadic wrappers.
    // Variadic functions like printf/snprintf have different ABI on ARM64,
    // so we wrap them in regular C functions.
    let runtime_c_path = format!("{}_rt.c", output_path);
    std::fs::write(&runtime_c_path, concat!(
        "#include <stdio.h>\n",
        "#include <stdlib.h>\n",
        "void ilo_print_int(long v) { printf(\"%ld\\n\", v); }\n",
        "void ilo_print_float(double v) { printf(\"%g\\n\", v); }\n",
        "void ilo_print_str(const char* s) { puts(s); }\n",
        "double ilo_atof(const char* s) { return atof(s); }\n",
    )).map_err(|e| format!("failed to write runtime C file: {}", e))?;

    let runtime_o_path = format!("{}_rt.o", output_path);
    let cc_status = std::process::Command::new("cc")
        .arg("-c")
        .arg("-O2")
        .arg(&runtime_c_path)
        .arg("-o")
        .arg(&runtime_o_path)
        .status()
        .map_err(|e| format!("failed to compile runtime: {}", e))?;
    let _ = std::fs::remove_file(&runtime_c_path);
    if !cc_status.success() {
        let _ = std::fs::remove_file(&runtime_o_path);
        return Err("failed to compile C runtime helpers".to_string());
    }

    // Declare non-variadic helper functions
    let ilo_print_int = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(I64)); // long value
        module.declare_function("ilo_print_int", Linkage::Import, &sig).map_err(|e| e.to_string())?
    };

    let ilo_print_float = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(F64)); // double value
        module.declare_function("ilo_print_float", Linkage::Import, &sig).map_err(|e| e.to_string())?
    };

    let ilo_print_str = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(I64)); // const char*
        module.declare_function("ilo_print_str", Linkage::Import, &sig).map_err(|e| e.to_string())?
    };

    let ilo_atof = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(I64)); // const char*
        sig.returns.push(AbiParam::new(F64));
        module.declare_function("ilo_atof", Linkage::Import, &sig).map_err(|e| e.to_string())?
    };

    // Helper to clean up temp files on any error path
    let cleanup = |obj: &str, rt: &str| {
        let _ = std::fs::remove_file(obj);
        let _ = std::fs::remove_file(rt);
    };

    // Compile the entry function
    let user_func_id = compile_function(&mut module, chunk, nan_consts, &format!("ilo_{}", entry_func))
        .inspect_err(|_| { cleanup("", &runtime_o_path); })?;

    // Generate main()
    generate_main(
        &mut module,
        user_func_id,
        chunk.param_count as usize,
        ilo_print_int,
        ilo_print_float,
        ilo_print_str,
        ilo_atof,
    ).inspect_err(|_| { cleanup("", &runtime_o_path); })?;

    // Emit object file
    let obj_product = module.finish();
    let obj_bytes = obj_product.emit().map_err(|e| { cleanup("", &runtime_o_path); e.to_string() })?;

    let obj_path = format!("{}.o", output_path);
    std::fs::write(&obj_path, &obj_bytes)
        .map_err(|e| { cleanup("", &runtime_o_path); format!("failed to write object file: {}", e) })?;

    // Link both objects with cc
    let status = std::process::Command::new("cc")
        .arg(&obj_path)
        .arg(&runtime_o_path)
        .arg("-o")
        .arg(output_path)
        .arg("-lm")
        .status()
        .map_err(|e| { cleanup(&obj_path, &runtime_o_path); format!("failed to run cc: {}", e) })?;

    cleanup(&obj_path, &runtime_o_path);

    if !status.success() {
        return Err(format!("linker failed with exit code: {}", status));
    }

    Ok(())
}

/// Compile a single function chunk into the ObjectModule.
fn compile_function(
    module: &mut ObjectModule,
    chunk: &Chunk,
    nan_consts: &[NanVal],
    name: &str,
) -> Result<FuncId, String> {
    let mut sig = module.make_signature();
    for _ in 0..chunk.param_count {
        sig.params.push(AbiParam::new(I64));
    }
    sig.returns.push(AbiParam::new(I64));

    let func_id = module.declare_function(name, Linkage::Local, &sig)
        .map_err(|e| e.to_string())?;

    let mut ctx = Context::new();
    ctx.func.signature = sig;

    let mut fn_builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fn_builder_ctx);

    let reg_count = chunk.reg_count.max(chunk.param_count) as usize;
    let mut vars: Vec<Variable> = Vec::with_capacity(reg_count);
    for i in 0..reg_count {
        let var = Variable::from_u32(i as u32);
        builder.declare_var(var, I64);
        vars.push(var);
    }

    let leaders = find_block_leaders(&chunk.code);
    let mut block_map: HashMap<usize, cranelift_codegen::ir::Block> = HashMap::new();
    for &leader in &leaders {
        let block = builder.create_block();
        block_map.insert(leader, block);
    }

    let entry_block = block_map[&0];
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);

    for (i, var) in vars.iter().enumerate().take(chunk.param_count as usize) {
        let val = builder.block_params(entry_block)[i];
        builder.def_var(*var, val);
    }

    let nil_bits = TAG_NIL;
    for var in vars.iter().take(reg_count).skip(chunk.param_count as usize) {
        let zero = builder.ins().iconst(I64, nil_bits as i64);
        builder.def_var(*var, zero);
    }

    let mut block_terminated = false;

    for (ip, &inst) in chunk.code.iter().enumerate() {
        if ip > 0 && block_map.contains_key(&ip) {
            let block = block_map[&ip];
            if !block_terminated {
                builder.ins().jump(block, &[]);
            }
            builder.switch_to_block(block);
            block_terminated = false;
        }

        if block_terminated {
            continue;
        }

        let op = (inst >> 24) as u8;
        let a_idx = ((inst >> 16) & 0xFF) as usize;
        let b_idx = ((inst >> 8) & 0xFF) as usize;
        let c_idx = (inst & 0xFF) as usize;
        let mf = MemFlags::new();

        match op {
            OP_ADD_NN => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let bf = builder.ins().bitcast(F64, mf, bv);
                let cf = builder.ins().bitcast(F64, mf, cv);
                let r = builder.ins().fadd(bf, cf);
                let result = builder.ins().bitcast(I64, mf, r);
                builder.def_var(vars[a_idx], result);
            }
            OP_SUB_NN => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let bf = builder.ins().bitcast(F64, mf, bv);
                let cf = builder.ins().bitcast(F64, mf, cv);
                let r = builder.ins().fsub(bf, cf);
                let result = builder.ins().bitcast(I64, mf, r);
                builder.def_var(vars[a_idx], result);
            }
            OP_MUL_NN => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let bf = builder.ins().bitcast(F64, mf, bv);
                let cf = builder.ins().bitcast(F64, mf, cv);
                let r = builder.ins().fmul(bf, cf);
                let result = builder.ins().bitcast(I64, mf, r);
                builder.def_var(vars[a_idx], result);
            }
            OP_DIV_NN => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let bf = builder.ins().bitcast(F64, mf, bv);
                let cf = builder.ins().bitcast(F64, mf, cv);
                let r = builder.ins().fdiv(bf, cf);
                let result = builder.ins().bitcast(I64, mf, r);
                builder.def_var(vars[a_idx], result);
            }
            OP_ADDK_N | OP_SUBK_N | OP_MULK_N | OP_DIVK_N => {
                let bv = builder.use_var(vars[b_idx]);
                let kv = nan_consts[c_idx].as_number();
                let bf = builder.ins().bitcast(F64, mf, bv);
                let kval = builder.ins().f64const(kv);
                let r = match op {
                    OP_ADDK_N => builder.ins().fadd(bf, kval),
                    OP_SUBK_N => builder.ins().fsub(bf, kval),
                    OP_MULK_N => builder.ins().fmul(bf, kval),
                    OP_DIVK_N => builder.ins().fdiv(bf, kval),
                    _ => unreachable!(),
                };
                let result = builder.ins().bitcast(I64, mf, r);
                builder.def_var(vars[a_idx], result);
            }
            OP_EQ | OP_NE | OP_LT | OP_GT | OP_LE | OP_GE => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let bf = builder.ins().bitcast(F64, mf, bv);
                let cf = builder.ins().bitcast(F64, mf, cv);
                use cranelift_codegen::ir::condcodes::FloatCC;
                let cc = match op {
                    OP_LT => FloatCC::LessThan,
                    OP_GT => FloatCC::GreaterThan,
                    OP_LE => FloatCC::LessThanOrEqual,
                    OP_GE => FloatCC::GreaterThanOrEqual,
                    OP_EQ => FloatCC::Equal,
                    OP_NE => FloatCC::NotEqual,
                    _ => unreachable!(),
                };
                let cmp = builder.ins().fcmp(cc, bf, cf);
                let tv = builder.ins().iconst(I64, TAG_TRUE as i64);
                let fv = builder.ins().iconst(I64, TAG_FALSE as i64);
                let result = builder.ins().select(cmp, tv, fv);
                builder.def_var(vars[a_idx], result);
            }
            OP_MOVE => {
                if a_idx != b_idx {
                    let bv = builder.use_var(vars[b_idx]);
                    builder.def_var(vars[a_idx], bv);
                }
            }
            OP_NEG => {
                let bv = builder.use_var(vars[b_idx]);
                let bf = builder.ins().bitcast(F64, mf, bv);
                let r = builder.ins().fneg(bf);
                let result = builder.ins().bitcast(I64, mf, r);
                builder.def_var(vars[a_idx], result);
            }
            OP_NOT => {
                let bv = builder.use_var(vars[b_idx]);
                let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                let masked = builder.ins().band(bv, qnan_val);
                let is_num = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual, masked, qnan_val);

                let num_block = builder.create_block();
                let tag_block = builder.create_block();
                let merge_block = builder.create_block();
                builder.append_block_param(merge_block, I64);

                builder.ins().brif(is_num, num_block, &[], tag_block, &[]);

                builder.switch_to_block(num_block);
                let bf = builder.ins().bitcast(F64, mf, bv);
                let zero = builder.ins().f64const(0.0);
                let is_zero = builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::Equal, bf, zero);
                let tv = builder.ins().iconst(I64, TAG_TRUE as i64);
                let fv = builder.ins().iconst(I64, TAG_FALSE as i64);
                let nr = builder.ins().select(is_zero, tv, fv);
                builder.ins().jump(merge_block, &[nr]);

                builder.switch_to_block(tag_block);
                let nil_c = builder.ins().iconst(I64, TAG_NIL as i64);
                let false_c = builder.ins().iconst(I64, TAG_FALSE as i64);
                let is_nil = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, bv, nil_c);
                let is_f = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, bv, false_c);
                let is_falsy = builder.ins().bor(is_nil, is_f);
                let tv2 = builder.ins().iconst(I64, TAG_TRUE as i64);
                let fv2 = builder.ins().iconst(I64, TAG_FALSE as i64);
                let tr = builder.ins().select(is_falsy, tv2, fv2);
                builder.ins().jump(merge_block, &[tr]);

                builder.switch_to_block(merge_block);
                builder.def_var(vars[a_idx], builder.block_params(merge_block)[0]);
            }
            OP_MOD => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let bf = builder.ins().bitcast(F64, mf, bv);
                let cf = builder.ins().bitcast(F64, mf, cv);
                let div = builder.ins().fdiv(bf, cf);
                let trunc = builder.ins().trunc(div);
                let prod = builder.ins().fmul(trunc, cf);
                let r = builder.ins().fsub(bf, prod);
                let result = builder.ins().bitcast(I64, mf, r);
                builder.def_var(vars[a_idx], result);
            }
            OP_LOADK => {
                let bx = (inst & 0xFFFF) as usize;
                let bits = nan_consts[bx].0;
                let kval = builder.ins().iconst(I64, bits as i64);
                builder.def_var(vars[a_idx], kval);
            }
            OP_JMP => {
                let sbx = (inst & 0xFFFF) as i16;
                let target = (ip as isize + 1 + sbx as isize) as usize;
                let tb = block_map.get(&target)
                    .ok_or_else(|| format!("JMP target {} at ip {} has no block leader", target, ip))?;
                builder.ins().jump(*tb, &[]);
                block_terminated = true;
            }
            OP_JMPF | OP_JMPT => {
                let sbx = (inst & 0xFFFF) as i16;
                let target = (ip as isize + 1 + sbx as isize) as usize;
                let fallthrough = ip + 1;
                let av = builder.use_var(vars[a_idx]);

                let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                let masked = builder.ins().band(av, qnan_val);
                let is_num = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual, masked, qnan_val);

                let num_truthy_block = builder.create_block();
                let tag_truthy_block = builder.create_block();
                let merge_truthy = builder.create_block();
                builder.append_block_param(merge_truthy, I64);

                builder.ins().brif(is_num, num_truthy_block, &[], tag_truthy_block, &[]);

                builder.switch_to_block(num_truthy_block);
                let af = builder.ins().bitcast(F64, mf, av);
                let zero = builder.ins().f64const(0.0);
                let cmp = builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::NotEqual, af, zero);
                let num_result = builder.ins().uextend(I64, cmp);
                builder.ins().jump(merge_truthy, &[num_result]);

                builder.switch_to_block(tag_truthy_block);
                let nil_val = builder.ins().iconst(I64, TAG_NIL as i64);
                let false_val = builder.ins().iconst(I64, TAG_FALSE as i64);
                let not_nil = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::NotEqual, av, nil_val);
                let not_false = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::NotEqual, av, false_val);
                let tag_truthy = builder.ins().band(not_nil, not_false);
                let tag_result = builder.ins().uextend(I64, tag_truthy);
                builder.ins().jump(merge_truthy, &[tag_result]);

                builder.switch_to_block(merge_truthy);
                let truthy_val = builder.block_params(merge_truthy)[0];

                let target_block = block_map.get(&target)
                    .ok_or_else(|| format!("JMPF/JMPT target {} at ip {} has no block leader", target, ip))?;
                let fall_block = block_map.get(&fallthrough)
                    .ok_or_else(|| format!("JMPF/JMPT fallthrough {} at ip {} has no block leader", fallthrough, ip))?;
                if op == OP_JMPF {
                    builder.ins().brif(truthy_val, *fall_block, &[], *target_block, &[]);
                } else {
                    builder.ins().brif(truthy_val, *target_block, &[], *fall_block, &[]);
                }
                block_terminated = true;
            }
            OP_JMPNN => {
                let sbx = (inst & 0xFFFF) as i16;
                let target = (ip as isize + 1 + sbx as isize) as usize;
                let fallthrough = ip + 1;
                let av = builder.use_var(vars[a_idx]);
                let nil_const = builder.ins().iconst(I64, TAG_NIL as i64);
                let is_nil = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, av, nil_const);
                let tb = block_map.get(&target)
                    .ok_or_else(|| format!("JMPNN target {} at ip {} has no block leader", target, ip))?;
                let fb = block_map.get(&fallthrough)
                    .ok_or_else(|| format!("JMPNN fallthrough {} at ip {} has no block leader", fallthrough, ip))?;
                builder.ins().brif(is_nil, *fb, &[], *tb, &[]);
                block_terminated = true;
            }
            OP_RET => {
                let av = builder.use_var(vars[a_idx]);
                builder.ins().return_(&[av]);
                block_terminated = true;
            }
            _ => {
                return Err(format!("unsupported opcode {} at instruction {}", op, ip));
            }
        }
    }

    if !block_terminated {
        let nil = builder.ins().iconst(I64, TAG_NIL as i64);
        builder.ins().return_(&[nil]);
    }

    builder.seal_all_blocks();
    builder.finalize();

    module.define_function(func_id, &mut ctx).map_err(|e| e.to_string())?;
    Ok(func_id)
}

/// Generate the `main(argc, argv)` entry point.
fn generate_main(
    module: &mut ObjectModule,
    user_func_id: FuncId,
    param_count: usize,
    ilo_print_int: FuncId,
    ilo_print_float: FuncId,
    ilo_print_str: FuncId,
    ilo_atof: FuncId,
) -> Result<(), String> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(I32)); // argc
    sig.params.push(AbiParam::new(I64)); // argv
    sig.returns.push(AbiParam::new(I32)); // exit code

    let main_id = module.declare_function("main", Linkage::Export, &sig)
        .map_err(|e| e.to_string())?;

    let mut ctx = Context::new();
    ctx.func.signature = sig;

    let mut fn_builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fn_builder_ctx);

    let entry_block = builder.create_block();
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);
    builder.seal_block(entry_block);

    let _argc = builder.block_params(entry_block)[0];
    let argv = builder.block_params(entry_block)[1];
    let mf = MemFlags::new();

    let user_fref = module.declare_func_in_func(user_func_id, builder.func);
    let print_int_fref = module.declare_func_in_func(ilo_print_int, builder.func);
    let print_float_fref = module.declare_func_in_func(ilo_print_float, builder.func);
    let print_str_fref = module.declare_func_in_func(ilo_print_str, builder.func);
    let atof_fref = module.declare_func_in_func(ilo_atof, builder.func);

    // Convert CLI args to NanVal via atof
    let mut call_args = Vec::with_capacity(param_count);
    for i in 0..param_count {
        let idx = builder.ins().iconst(I64, ((i + 1) * 8) as i64);
        let arg_ptr_ptr = builder.ins().iadd(argv, idx);
        let arg_ptr = builder.ins().load(I64, mf, arg_ptr_ptr, 0);
        let call_inst = builder.ins().call(atof_fref, &[arg_ptr]);
        let f64_val = builder.inst_results(call_inst)[0];
        let nan_val = builder.ins().bitcast(I64, mf, f64_val);
        call_args.push(nan_val);
    }

    // Call the user function
    let call_inst = builder.ins().call(user_fref, &call_args);
    let result = builder.inst_results(call_inst)[0];

    // Check if number (QNAN bits NOT set)
    let qnan_val = builder.ins().iconst(I64, QNAN as i64);
    let masked = builder.ins().band(result, qnan_val);
    let is_not_num = builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::Equal, masked, qnan_val);

    let num_block = builder.create_block();
    let tag_block = builder.create_block();
    let exit_block = builder.create_block();

    builder.ins().brif(is_not_num, tag_block, &[], num_block, &[]);

    // ── Number path ──
    builder.switch_to_block(num_block);
    let result_f64 = builder.ins().bitcast(F64, mf, result);
    let as_int = builder.ins().fcvt_to_sint_sat(I64, result_f64);
    let back_to_f64 = builder.ins().fcvt_from_sint(F64, as_int);
    let is_integer = builder.ins().fcmp(
        cranelift_codegen::ir::condcodes::FloatCC::Equal, result_f64, back_to_f64);

    let int_print_block = builder.create_block();
    let float_print_block = builder.create_block();
    builder.ins().brif(is_integer, int_print_block, &[], float_print_block, &[]);

    // Integer: call ilo_print_int(int_val)
    builder.switch_to_block(int_print_block);
    builder.ins().call(print_int_fref, &[as_int]);
    builder.ins().jump(exit_block, &[]);

    // Float: call ilo_print_float(f64_val)
    builder.switch_to_block(float_print_block);
    builder.ins().call(print_float_fref, &[result_f64]);
    builder.ins().jump(exit_block, &[]);

    // ── Tag path (true/false/nil) ──
    builder.switch_to_block(tag_block);
    let true_tag = builder.ins().iconst(I64, TAG_TRUE as i64);
    let is_true = builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::Equal, result, true_tag);
    let true_block = builder.create_block();
    let not_true_block = builder.create_block();
    builder.ins().brif(is_true, true_block, &[], not_true_block, &[]);

    builder.switch_to_block(true_block);
    let true_str = create_data_section(module, &mut builder, "ilo_str_true", b"true\0")?;
    builder.ins().call(print_str_fref, &[true_str]);
    builder.ins().jump(exit_block, &[]);

    builder.switch_to_block(not_true_block);
    let false_tag = builder.ins().iconst(I64, TAG_FALSE as i64);
    let is_false = builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::Equal, result, false_tag);
    let false_block = builder.create_block();
    let nil_block = builder.create_block();
    builder.ins().brif(is_false, false_block, &[], nil_block, &[]);

    builder.switch_to_block(false_block);
    let false_str = create_data_section(module, &mut builder, "ilo_str_false", b"false\0")?;
    builder.ins().call(print_str_fref, &[false_str]);
    builder.ins().jump(exit_block, &[]);

    builder.switch_to_block(nil_block);
    let nil_str = create_data_section(module, &mut builder, "ilo_str_nil", b"nil\0")?;
    builder.ins().call(print_str_fref, &[nil_str]);
    builder.ins().jump(exit_block, &[]);

    // ── Exit ──
    builder.switch_to_block(exit_block);
    let zero = builder.ins().iconst(I32, 0);
    builder.ins().return_(&[zero]);

    builder.seal_block(num_block);
    builder.seal_block(tag_block);
    builder.seal_block(int_print_block);
    builder.seal_block(float_print_block);
    builder.seal_block(true_block);
    builder.seal_block(not_true_block);
    builder.seal_block(false_block);
    builder.seal_block(nil_block);
    builder.seal_block(exit_block);

    builder.finalize();

    module.define_function(main_id, &mut ctx).map_err(|e| e.to_string())?;
    Ok(())
}

/// Create a read-only data section and return a pointer to it.
fn create_data_section(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    name: &str,
    bytes: &[u8],
) -> Result<cranelift_codegen::ir::Value, String> {
    use cranelift_module::DataDescription;
    let data_id = module.declare_data(name, Linkage::Local, false, false)
        .map_err(|e| e.to_string())?;
    let mut desc = DataDescription::new();
    desc.define(bytes.to_vec().into_boxed_slice());
    module.define_data(data_id, &desc).map_err(|e| e.to_string())?;
    let gv = module.declare_data_in_func(data_id, builder.func);
    Ok(builder.ins().global_value(I64, gv))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    fn compile_program(source: &str) -> CompiledProgram {
        let tokens = lexer::lex(source).unwrap();
        let token_spans: Vec<(crate::lexer::Token, crate::ast::Span)> = tokens
            .into_iter()
            .map(|(t, r)| (t, crate::ast::Span { start: r.start, end: r.end }))
            .collect();
        let (prog, errors) = parser::parse(token_spans);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        crate::vm::compile(&prog).unwrap()
    }

    #[test]
    fn aot_compile_simple_multiply() {
        let compiled = compile_program("f x:n>n;*x 2");
        let tmp = std::env::temp_dir().join("ilo_test_aot_mul");
        let out = tmp.to_str().unwrap();
        compile_to_binary(&compiled, "f", out).unwrap();

        let output = std::process::Command::new(out)
            .arg("5")
            .output()
            .expect("failed to run compiled binary");
        let _ = std::fs::remove_file(out);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "10", "expected 10, got: {}", stdout.trim());
    }

    #[test]
    fn aot_compile_add_two_args() {
        let compiled = compile_program("f a:n b:n>n;+a b");
        let tmp = std::env::temp_dir().join("ilo_test_aot_add");
        let out = tmp.to_str().unwrap();
        compile_to_binary(&compiled, "f", out).unwrap();

        let output = std::process::Command::new(out)
            .args(["3", "4"])
            .output()
            .expect("failed to run compiled binary");
        let _ = std::fs::remove_file(out);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "7");
    }

    #[test]
    fn aot_compile_conditional() {
        // if x > 0 then x * 2 else neg(x)
        let compiled = compile_program("f x:n>n;?>x 0 *x 2 *x -1");
        let tmp = std::env::temp_dir().join("ilo_test_aot_cond");
        let out = tmp.to_str().unwrap();
        compile_to_binary(&compiled, "f", out).unwrap();

        let output = std::process::Command::new(out)
            .arg("5")
            .output()
            .expect("failed to run compiled binary");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "10");

        let output2 = std::process::Command::new(out)
            .arg("-3")
            .output()
            .expect("failed to run compiled binary");
        let _ = std::fs::remove_file(out);
        let stdout2 = String::from_utf8_lossy(&output2.stdout);
        assert_eq!(stdout2.trim(), "3");
    }

    #[test]
    fn aot_compile_recursive_rejected() {
        // Recursive function uses OP_CALL, should be rejected
        let compiled = compile_program("fac n:n>n;<=n 1 1;r=fac -n 1;*n r");
        let result = compile_to_binary(&compiled, "fac", "/tmp/ilo_test_aot_reject");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not yet supported"));
    }

    #[test]
    fn aot_no_args_function() {
        let compiled = compile_program("f >n;42");
        let tmp = std::env::temp_dir().join("ilo_test_aot_noargs");
        let out = tmp.to_str().unwrap();
        compile_to_binary(&compiled, "f", out).unwrap();

        let output = std::process::Command::new(out)
            .output()
            .expect("failed to run compiled binary");
        let _ = std::fs::remove_file(out);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "42");
    }
}
