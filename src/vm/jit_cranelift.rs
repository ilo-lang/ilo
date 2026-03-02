//! Cranelift NanVal JIT backend — compiles ALL functions to native code.
//!
//! Works with u64 (NanVal) registers instead of f64. For numeric operations,
//! bitcasts u64↔f64 and uses FP instructions. For everything else, calls
//! `extern "C"` Rust helper functions. This eliminates the bytecode dispatch
//! loop while reusing all existing VM logic.

use super::*;
use cranelift_codegen::ir::{AbiParam, InstBuilder};
use cranelift_codegen::ir::types::{I64, F64};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, Module, Linkage, FuncId};
use std::collections::HashMap;

/// Compiled Cranelift function that can be called repeatedly.
pub(crate) struct JitFunction {
    _module: JITModule,
    func_ptr: *const u8,
    param_count: usize,
}

// The function pointer is safe to call from any thread (it's immutable code).
unsafe impl Send for JitFunction {}

// Helper function IDs registered with the JIT module
#[allow(dead_code)]
struct HelperFuncs {
    add: FuncId,
    sub: FuncId,
    mul: FuncId,
    div: FuncId,
    eq: FuncId,
    ne: FuncId,
    gt: FuncId,
    lt: FuncId,
    ge: FuncId,
    le: FuncId,
    not: FuncId,
    neg: FuncId,
    truthy: FuncId,
    wrapok: FuncId,
    wraperr: FuncId,
    isok: FuncId,
    iserr: FuncId,
    unwrap: FuncId,
    jit_move: FuncId,
    drop_rc: FuncId,
    len: FuncId,
    str_fn: FuncId,
    num: FuncId,
    abs: FuncId,
    min: FuncId,
    max: FuncId,
    flr: FuncId,
    cel: FuncId,
    rnd0: FuncId,
    rnd2: FuncId,
    now: FuncId,
    env: FuncId,
    get: FuncId,
    spl: FuncId,
    cat: FuncId,
    has: FuncId,
    hd: FuncId,
    tl: FuncId,
    rev: FuncId,
    srt: FuncId,
    slc: FuncId,
    listappend: FuncId,
    index: FuncId,
    recfld: FuncId,
    recnew: FuncId,
    recwith: FuncId,
    listnew: FuncId,
    listget: FuncId,
    jpth: FuncId,
    jdmp: FuncId,
    jpar: FuncId,
    call: FuncId,
}

fn declare_helper(module: &mut JITModule, name: &str, n_params: usize, n_returns: usize) -> FuncId {
    let mut sig = module.make_signature();
    for _ in 0..n_params {
        sig.params.push(AbiParam::new(I64));
    }
    for _ in 0..n_returns {
        sig.returns.push(AbiParam::new(I64));
    }
    module.declare_function(name, Linkage::Import, &sig).unwrap()
}

fn register_helpers(builder: &mut JITBuilder) {
    let helpers: &[(&str, *const u8)] = &[
        ("jit_add", jit_add as *const u8),
        ("jit_sub", jit_sub as *const u8),
        ("jit_mul", jit_mul as *const u8),
        ("jit_div", jit_div as *const u8),
        ("jit_eq", jit_eq as *const u8),
        ("jit_ne", jit_ne as *const u8),
        ("jit_gt", jit_gt as *const u8),
        ("jit_lt", jit_lt as *const u8),
        ("jit_ge", jit_ge as *const u8),
        ("jit_le", jit_le as *const u8),
        ("jit_not", jit_not as *const u8),
        ("jit_neg", jit_neg as *const u8),
        ("jit_truthy", jit_truthy as *const u8),
        ("jit_wrapok", jit_wrapok as *const u8),
        ("jit_wraperr", jit_wraperr as *const u8),
        ("jit_isok", jit_isok as *const u8),
        ("jit_iserr", jit_iserr as *const u8),
        ("jit_unwrap", jit_unwrap as *const u8),
        ("jit_move", jit_move as *const u8),
        ("jit_drop_rc", jit_drop_rc as *const u8),
        ("jit_len", jit_len as *const u8),
        ("jit_str", jit_str as *const u8),
        ("jit_num", jit_num as *const u8),
        ("jit_abs", jit_abs as *const u8),
        ("jit_min", jit_min as *const u8),
        ("jit_max", jit_max as *const u8),
        ("jit_flr", jit_flr as *const u8),
        ("jit_cel", jit_cel as *const u8),
        ("jit_rnd0", jit_rnd0 as *const u8),
        ("jit_rnd2", jit_rnd2 as *const u8),
        ("jit_now", jit_now as *const u8),
        ("jit_env", jit_env as *const u8),
        ("jit_get", jit_get as *const u8),
        ("jit_spl", jit_spl as *const u8),
        ("jit_cat", jit_cat as *const u8),
        ("jit_has", jit_has as *const u8),
        ("jit_hd", jit_hd as *const u8),
        ("jit_tl", jit_tl as *const u8),
        ("jit_rev", jit_rev as *const u8),
        ("jit_srt", jit_srt as *const u8),
        ("jit_slc", jit_slc as *const u8),
        ("jit_listappend", jit_listappend as *const u8),
        ("jit_index", jit_index as *const u8),
        ("jit_recfld", jit_recfld as *const u8),
        ("jit_recnew", jit_recnew as *const u8),
        ("jit_recwith", jit_recwith as *const u8),
        ("jit_listnew", jit_listnew as *const u8),
        ("jit_listget", jit_listget as *const u8),
        ("jit_jpth", jit_jpth as *const u8),
        ("jit_jdmp", jit_jdmp as *const u8),
        ("jit_jpar", jit_jpar as *const u8),
        ("jit_call", jit_call as *const u8),
    ];
    for &(name, ptr) in helpers {
        builder.symbol(name, ptr);
    }
}

fn declare_all_helpers(module: &mut JITModule) -> HelperFuncs {
    HelperFuncs {
        add: declare_helper(module, "jit_add", 2, 1),
        sub: declare_helper(module, "jit_sub", 2, 1),
        mul: declare_helper(module, "jit_mul", 2, 1),
        div: declare_helper(module, "jit_div", 2, 1),
        eq: declare_helper(module, "jit_eq", 2, 1),
        ne: declare_helper(module, "jit_ne", 2, 1),
        gt: declare_helper(module, "jit_gt", 2, 1),
        lt: declare_helper(module, "jit_lt", 2, 1),
        ge: declare_helper(module, "jit_ge", 2, 1),
        le: declare_helper(module, "jit_le", 2, 1),
        not: declare_helper(module, "jit_not", 1, 1),
        neg: declare_helper(module, "jit_neg", 1, 1),
        truthy: declare_helper(module, "jit_truthy", 1, 1),
        wrapok: declare_helper(module, "jit_wrapok", 1, 1),
        wraperr: declare_helper(module, "jit_wraperr", 1, 1),
        isok: declare_helper(module, "jit_isok", 1, 1),
        iserr: declare_helper(module, "jit_iserr", 1, 1),
        unwrap: declare_helper(module, "jit_unwrap", 1, 1),
        jit_move: declare_helper(module, "jit_move", 1, 1),
        drop_rc: declare_helper(module, "jit_drop_rc", 1, 0),
        len: declare_helper(module, "jit_len", 1, 1),
        str_fn: declare_helper(module, "jit_str", 1, 1),
        num: declare_helper(module, "jit_num", 1, 1),
        abs: declare_helper(module, "jit_abs", 1, 1),
        min: declare_helper(module, "jit_min", 2, 1),
        max: declare_helper(module, "jit_max", 2, 1),
        flr: declare_helper(module, "jit_flr", 1, 1),
        cel: declare_helper(module, "jit_cel", 1, 1),
        rnd0: declare_helper(module, "jit_rnd0", 0, 1),
        rnd2: declare_helper(module, "jit_rnd2", 2, 1),
        now: declare_helper(module, "jit_now", 0, 1),
        env: declare_helper(module, "jit_env", 1, 1),
        get: declare_helper(module, "jit_get", 1, 1),
        spl: declare_helper(module, "jit_spl", 2, 1),
        cat: declare_helper(module, "jit_cat", 2, 1),
        has: declare_helper(module, "jit_has", 2, 1),
        hd: declare_helper(module, "jit_hd", 1, 1),
        tl: declare_helper(module, "jit_tl", 1, 1),
        rev: declare_helper(module, "jit_rev", 1, 1),
        srt: declare_helper(module, "jit_srt", 1, 1),
        slc: declare_helper(module, "jit_slc", 3, 1),
        listappend: declare_helper(module, "jit_listappend", 2, 1),
        index: declare_helper(module, "jit_index", 2, 1),
        recfld: declare_helper(module, "jit_recfld", 2, 1),
        recnew: declare_helper(module, "jit_recnew", 4, 1),
        recwith: declare_helper(module, "jit_recwith", 4, 1),
        listnew: declare_helper(module, "jit_listnew", 2, 1),
        listget: declare_helper(module, "jit_listget", 2, 1),
        jpth: declare_helper(module, "jit_jpth", 2, 1),
        jdmp: declare_helper(module, "jit_jdmp", 1, 1),
        jpar: declare_helper(module, "jit_jpar", 1, 1),
        call: declare_helper(module, "jit_call", 4, 1),
    }
}

/// Compile a chunk into native code via Cranelift (NanVal / I64 mode).
pub(crate) fn compile(chunk: &Chunk, nan_consts: &[NanVal], program: &CompiledProgram) -> Option<JitFunction> {
    let mut flag_builder = settings::builder();
    flag_builder.set("opt_level", "speed").ok()?;
    let isa_builder = cranelift_native::builder().ok()?;
    let isa = isa_builder.finish(settings::Flags::new(flag_builder)).ok()?;

    let mut jit_builder = JITBuilder::with_isa(isa, default_libcall_names());
    register_helpers(&mut jit_builder);
    let mut module = JITModule::new(jit_builder);
    let helpers = declare_all_helpers(&mut module);

    // Build function signature: (i64, i64, ...) -> i64
    let mut sig = module.make_signature();
    for _ in 0..chunk.param_count {
        sig.params.push(AbiParam::new(I64));
    }
    sig.returns.push(AbiParam::new(I64));

    let func_id = module.declare_function("jit_func", Linkage::Local, &sig).ok()?;

    let mut ctx = Context::new();
    ctx.func.signature = sig;

    let mut fn_builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fn_builder_ctx);

    // Declare variables for all VM registers as I64
    let reg_count = chunk.reg_count.max(chunk.param_count) as usize;
    let mut vars: Vec<Variable> = Vec::with_capacity(reg_count);
    for i in 0..reg_count {
        let var = Variable::from_u32(i as u32);
        builder.declare_var(var, I64);
        vars.push(var);
    }

    // Find block leaders for control flow
    let leaders = find_block_leaders(&chunk.code);
    let mut block_map: HashMap<usize, cranelift_codegen::ir::Block> = HashMap::new();
    for &leader in &leaders {
        let block = builder.create_block();
        block_map.insert(leader, block);
    }

    let entry_block = block_map[&0];
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);

    // Initialize params
    for (i, var) in vars.iter().enumerate().take(chunk.param_count as usize) {
        let val = builder.block_params(entry_block)[i];
        builder.def_var(*var, val);
    }

    // Initialize non-param registers to TAG_NIL
    let nil_bits = TAG_NIL;
    for var in vars.iter().take(reg_count).skip(chunk.param_count as usize) {
        let zero = builder.ins().iconst(I64, nil_bits as i64);
        builder.def_var(*var, zero);
    }

    // Import helper function references
    let mut func_refs: HashMap<FuncId, cranelift_codegen::ir::FuncRef> = HashMap::new();
    let mut get_func_ref = |builder: &mut FunctionBuilder, module: &mut JITModule, id: FuncId| -> cranelift_codegen::ir::FuncRef {
        *func_refs.entry(id).or_insert_with(|| module.declare_func_in_func(id, builder.func))
    };

    // Store the program pointer as a constant for jit_call
    let program_ptr_val = program as *const CompiledProgram as u64;

    // Pre-serialize record descriptors and field names for RECNEW/RECWITH/RECFLD
    // so we can pass stable pointers to helper functions.
    // We'll allocate these as leaked &'static [u8] — acceptable since JIT functions
    // are long-lived (same lifetime as the JitFunction).

    // Track whether the current block has been terminated
    let mut block_terminated = false;

    // Translate bytecode instruction by instruction
    for (ip, &inst) in chunk.code.iter().enumerate() {
        // Switch to new block if this is a leader (skip ip==0, already switched above)
        if ip > 0 && block_map.contains_key(&ip) {
            let block = block_map[&ip];
            // If the previous block doesn't have a terminator, jump to this block
            if !block_terminated {
                builder.ins().jump(block, &[]);
            }
            builder.switch_to_block(block);
            block_terminated = false;
        }

        // Skip dead code after a terminator within the same block
        if block_terminated {
            continue;
        }

        let op = (inst >> 24) as u8;
        let a_idx = ((inst >> 16) & 0xFF) as usize;
        let b_idx = ((inst >> 8) & 0xFF) as usize;
        let c_idx = (inst & 0xFF) as usize;

        match op {
            OP_ADD_NN => {
                // Both known numeric — inline bitcast+fadd+bitcast
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let bf = builder.ins().bitcast(F64, cranelift_codegen::ir::MemFlags::new(), bv);
                let cf = builder.ins().bitcast(F64, cranelift_codegen::ir::MemFlags::new(), cv);
                let result_f = builder.ins().fadd(bf, cf);
                let result = builder.ins().bitcast(I64, cranelift_codegen::ir::MemFlags::new(), result_f);
                builder.def_var(vars[a_idx], result);
            }
            OP_SUB_NN => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let bf = builder.ins().bitcast(F64, cranelift_codegen::ir::MemFlags::new(), bv);
                let cf = builder.ins().bitcast(F64, cranelift_codegen::ir::MemFlags::new(), cv);
                let result_f = builder.ins().fsub(bf, cf);
                let result = builder.ins().bitcast(I64, cranelift_codegen::ir::MemFlags::new(), result_f);
                builder.def_var(vars[a_idx], result);
            }
            OP_MUL_NN => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let bf = builder.ins().bitcast(F64, cranelift_codegen::ir::MemFlags::new(), bv);
                let cf = builder.ins().bitcast(F64, cranelift_codegen::ir::MemFlags::new(), cv);
                let result_f = builder.ins().fmul(bf, cf);
                let result = builder.ins().bitcast(I64, cranelift_codegen::ir::MemFlags::new(), result_f);
                builder.def_var(vars[a_idx], result);
            }
            OP_DIV_NN => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let bf = builder.ins().bitcast(F64, cranelift_codegen::ir::MemFlags::new(), bv);
                let cf = builder.ins().bitcast(F64, cranelift_codegen::ir::MemFlags::new(), cv);
                let result_f = builder.ins().fdiv(bf, cf);
                let result = builder.ins().bitcast(I64, cranelift_codegen::ir::MemFlags::new(), result_f);
                builder.def_var(vars[a_idx], result);
            }
            OP_ADDK_N => {
                let bv = builder.use_var(vars[b_idx]);
                let kv = nan_consts.get(c_idx)?.as_number();
                let bf = builder.ins().bitcast(F64, cranelift_codegen::ir::MemFlags::new(), bv);
                let kval = builder.ins().f64const(kv);
                let result_f = builder.ins().fadd(bf, kval);
                let result = builder.ins().bitcast(I64, cranelift_codegen::ir::MemFlags::new(), result_f);
                builder.def_var(vars[a_idx], result);
            }
            OP_SUBK_N => {
                let bv = builder.use_var(vars[b_idx]);
                let kv = nan_consts.get(c_idx)?.as_number();
                let bf = builder.ins().bitcast(F64, cranelift_codegen::ir::MemFlags::new(), bv);
                let kval = builder.ins().f64const(kv);
                let result_f = builder.ins().fsub(bf, kval);
                let result = builder.ins().bitcast(I64, cranelift_codegen::ir::MemFlags::new(), result_f);
                builder.def_var(vars[a_idx], result);
            }
            OP_MULK_N => {
                let bv = builder.use_var(vars[b_idx]);
                let kv = nan_consts.get(c_idx)?.as_number();
                let bf = builder.ins().bitcast(F64, cranelift_codegen::ir::MemFlags::new(), bv);
                let kval = builder.ins().f64const(kv);
                let result_f = builder.ins().fmul(bf, kval);
                let result = builder.ins().bitcast(I64, cranelift_codegen::ir::MemFlags::new(), result_f);
                builder.def_var(vars[a_idx], result);
            }
            OP_DIVK_N => {
                let bv = builder.use_var(vars[b_idx]);
                let kv = nan_consts.get(c_idx)?.as_number();
                let bf = builder.ins().bitcast(F64, cranelift_codegen::ir::MemFlags::new(), bv);
                let kval = builder.ins().f64const(kv);
                let result_f = builder.ins().fdiv(bf, kval);
                let result = builder.ins().bitcast(I64, cranelift_codegen::ir::MemFlags::new(), result_f);
                builder.def_var(vars[a_idx], result);
            }
            OP_ADD => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.add);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_SUB => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.sub);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MUL => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.mul);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_DIV => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.div);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_EQ => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.eq);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_NE => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.ne);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_GT => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.gt);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_LT => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.lt);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_GE => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.ge);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_LE => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.le);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MOVE => {
                if a_idx != b_idx {
                    let bv = builder.use_var(vars[b_idx]);
                    let fref = get_func_ref(&mut builder, &mut module, helpers.jit_move);
                    let call_inst = builder.ins().call(fref, &[bv]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                }
            }
            OP_NOT => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.not);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_NEG => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.neg);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_WRAPOK => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.wrapok);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_WRAPERR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.wraperr);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ISOK => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.isok);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ISERR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.iserr);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_UNWRAP => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.unwrap);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_LOADK => {
                let bx = (inst & 0xFFFF) as usize;
                let bits = nan_consts.get(bx)?.0;
                let kval = builder.ins().iconst(I64, bits as i64);
                // Clone RC for heap values
                let nv = NanVal(bits);
                if nv.is_heap() {
                    let fref = get_func_ref(&mut builder, &mut module, helpers.jit_move);
                    let call_inst = builder.ins().call(fref, &[kval]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                } else {
                    builder.def_var(vars[a_idx], kval);
                }
            }
            OP_JMP => {
                let sbx = (inst & 0xFFFF) as i16;
                let target = (ip as isize + 1 + sbx as isize) as usize;
                if let Some(&target_block) = block_map.get(&target) {
                    builder.ins().jump(target_block, &[]);
                    block_terminated = true;
                }
            }
            OP_JMPF => {
                let sbx = (inst & 0xFFFF) as i16;
                let target = (ip as isize + 1 + sbx as isize) as usize;
                let fallthrough = ip + 1;
                let av = builder.use_var(vars[a_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.truthy);
                let call_inst = builder.ins().call(fref, &[av]);
                let truthy_val = builder.inst_results(call_inst)[0];
                if let (Some(&target_block), Some(&fall_block)) = (block_map.get(&target), block_map.get(&fallthrough)) {
                    builder.ins().brif(truthy_val, fall_block, &[], target_block, &[]);
                    block_terminated = true;
                }
            }
            OP_JMPT => {
                let sbx = (inst & 0xFFFF) as i16;
                let target = (ip as isize + 1 + sbx as isize) as usize;
                let fallthrough = ip + 1;
                let av = builder.use_var(vars[a_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.truthy);
                let call_inst = builder.ins().call(fref, &[av]);
                let truthy_val = builder.inst_results(call_inst)[0];
                if let (Some(&target_block), Some(&fall_block)) = (block_map.get(&target), block_map.get(&fallthrough)) {
                    builder.ins().brif(truthy_val, target_block, &[], fall_block, &[]);
                    block_terminated = true;
                }
            }
            OP_JMPNN => {
                let sbx = (inst & 0xFFFF) as i16;
                let target = (ip as isize + 1 + sbx as isize) as usize;
                let fallthrough = ip + 1;
                let av = builder.use_var(vars[a_idx]);
                let nil_const = builder.ins().iconst(I64, TAG_NIL as i64);
                let is_nil = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, av, nil_const);
                if let (Some(&target_block), Some(&fall_block)) = (block_map.get(&target), block_map.get(&fallthrough)) {
                    // JMPNN: jump if NOT nil → brif(is_nil, fallthrough, target)
                    builder.ins().brif(is_nil, fall_block, &[], target_block, &[]);
                    block_terminated = true;
                }
            }
            OP_RET => {
                let av = builder.use_var(vars[a_idx]);
                builder.ins().return_(&[av]);
                block_terminated = true;
            }
            OP_LEN => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.len);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_STR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.str_fn);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_NUM => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.num);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ABS => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.abs);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MIN => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.min);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MAX => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.max);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_FLR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.flr);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_CEL => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.cel);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RND0 => {
                let fref = get_func_ref(&mut builder, &mut module, helpers.rnd0);
                let call_inst = builder.ins().call(fref, &[]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RND2 => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.rnd2);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_NOW => {
                let fref = get_func_ref(&mut builder, &mut module, helpers.now);
                let call_inst = builder.ins().call(fref, &[]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ENV => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.env);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_GET => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.get);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_SPL => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.spl);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_CAT => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.cat);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_HAS => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.has);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_HD => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.hd);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_TL => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.tl);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_REV => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.rev);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_SRT => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.srt);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_SLC => {
                // slc(R[B], R[C], R[C+1])
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let dv = builder.use_var(vars[c_idx + 1]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.slc);
                let call_inst = builder.ins().call(fref, &[bv, cv, dv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_LISTAPPEND => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.listappend);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_INDEX => {
                // R[A] = R[B][C] where C is a literal index
                let bv = builder.use_var(vars[b_idx]);
                let idx_val = builder.ins().iconst(I64, c_idx as i64);
                let fref = get_func_ref(&mut builder, &mut module, helpers.index);
                let call_inst = builder.ins().call(fref, &[bv, idx_val]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RECFLD => {
                // R[A] = R[B].fields[C]  — C is now a field index
                let bv = builder.use_var(vars[b_idx]);

                // Inline fast path for arena records:
                //   tag = bv & TAG_MASK
                //   if tag == TAG_ARENA_REC:
                //     ptr = bv & PTR_MASK
                //     result = load(ptr + 8 + C*8)  // skip ArenaRecord header
                //     call jit_move(result) // clone_rc for heap fields (no-op for numbers)
                //   else:
                //     result = call jit_recfld(bv, C)
                let tag_mask_val = builder.ins().iconst(I64, TAG_MASK as i64);
                let tag = builder.ins().band(bv, tag_mask_val);
                let arena_tag_val = builder.ins().iconst(I64, TAG_ARENA_REC as i64);
                let is_arena = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal, tag, arena_tag_val);

                let arena_block = builder.create_block();
                let heap_block = builder.create_block();
                let merge_block = builder.create_block();
                builder.append_block_param(merge_block, I64);

                builder.ins().brif(is_arena, arena_block, &[], heap_block, &[]);

                // Arena path: inline pointer math + inline clone_rc
                builder.switch_to_block(arena_block);
                let ptr_mask_val = builder.ins().iconst(I64, PTR_MASK as i64);
                let ptr = builder.ins().band(bv, ptr_mask_val);
                let field_offset = builder.ins().iconst(I64, (8 + c_idx * 8) as i64);
                let field_addr = builder.ins().iadd(ptr, field_offset);
                let field_val = builder.ins().load(I64, cranelift_codegen::ir::MemFlags::trusted(), field_addr, 0);
                // Inline is_heap check: (val & QNAN) == QNAN && val != NIL && val != TRUE && val != FALSE && tag != ARENA_REC
                // For numbers (the hot path), (val & QNAN) != QNAN → skip clone_rc entirely
                let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                let masked = builder.ins().band(field_val, qnan_val);
                let is_nan_tagged = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal, masked, qnan_val);
                let clone_block = builder.create_block();
                let skip_clone_block = builder.create_block();
                builder.ins().brif(is_nan_tagged, clone_block, &[], skip_clone_block, &[]);

                // Clone path: call jit_move for heap values
                builder.switch_to_block(clone_block);
                let fref_move = get_func_ref(&mut builder, &mut module, helpers.jit_move);
                let move_inst = builder.ins().call(fref_move, &[field_val]);
                let _cloned = builder.inst_results(move_inst)[0];
                builder.ins().jump(skip_clone_block, &[]);

                // Skip clone path: field_val is a number, no RC management needed
                builder.switch_to_block(skip_clone_block);
                builder.ins().jump(merge_block, &[field_val]);

                // Heap path: call jit_recfld
                builder.switch_to_block(heap_block);
                let field_idx_val = builder.ins().iconst(I64, c_idx as i64);
                let fref = get_func_ref(&mut builder, &mut module, helpers.recfld);
                let call_inst = builder.ins().call(fref, &[bv, field_idx_val]);
                let heap_result = builder.inst_results(call_inst)[0];
                builder.ins().jump(merge_block, &[heap_result]);

                // Merge
                builder.switch_to_block(merge_block);
                let result = builder.block_params(merge_block)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RECFLD_NAME => {
                // Dynamic field access by name — bail out of JIT
                return None;
            }
            OP_RECNEW => {
                let bx = (inst & 0xFFFF) as usize;
                let type_id = (bx >> 8) as u16;
                let n_fields = bx & 0xFF;

                // Build array of register values on the stack
                let slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                    cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                    (n_fields * 8) as u32,
                    0,
                ));
                for i in 0..n_fields {
                    let v = builder.use_var(vars[a_idx + 1 + i]);
                    builder.ins().stack_store(v, slot, (i * 8) as i32);
                }
                let regs_ptr = builder.ins().stack_addr(I64, slot, 0);
                // Pack type_id (upper 16 bits) | n_fields (lower 16 bits) into one i64
                let type_id_and_nfields = ((type_id as u64) << 16) | (n_fields as u64);
                let arena_ptr_val = builder.ins().iconst(I64, jit_arena_ptr() as i64);
                let type_id_nfields_val = builder.ins().iconst(I64, type_id_and_nfields as i64);
                let registry_ptr_val = builder.ins().iconst(I64, &program.type_registry as *const TypeRegistry as i64);
                let fref = get_func_ref(&mut builder, &mut module, helpers.recnew);
                let call_inst = builder.ins().call(fref, &[arena_ptr_val, type_id_nfields_val, regs_ptr, registry_ptr_val]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RECWITH => {
                let bx = (inst & 0xFFFF) as usize;
                let indices_idx = bx >> 8;
                let n_updates = bx & 0xFF;

                // Extract field indices from the constant pool
                let update_indices: Vec<u8> = match &chunk.constants[indices_idx] {
                    Value::List(items) => items.iter().map(|v| match v {
                        Value::Number(n) => *n as u8,
                        _ => 0,
                    }).collect(),
                    _ => return None,
                };
                let indices_bytes: &'static [u8] = Box::leak(update_indices.into_boxed_slice());

                let old_rec = builder.use_var(vars[a_idx]);
                let slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                    cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                    (n_updates * 8) as u32,
                    0,
                ));
                for i in 0..n_updates {
                    let v = builder.use_var(vars[a_idx + 1 + i]);
                    builder.ins().stack_store(v, slot, (i * 8) as i32);
                }
                let regs_ptr = builder.ins().stack_addr(I64, slot, 0);
                let indices_ptr_val = builder.ins().iconst(I64, indices_bytes.as_ptr() as i64);
                let n_updates_val = builder.ins().iconst(I64, n_updates as i64);
                let fref = get_func_ref(&mut builder, &mut module, helpers.recwith);
                let call_inst = builder.ins().call(fref, &[old_rec, indices_ptr_val, n_updates_val, regs_ptr]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_LISTNEW => {
                let n = (inst & 0xFFFF) as usize;
                if n == 0 {
                    // Empty list: still need valid ptr
                    let null_ptr = builder.ins().iconst(I64, 0i64);
                    let n_val = builder.ins().iconst(I64, 0i64);
                    let fref = get_func_ref(&mut builder, &mut module, helpers.listnew);
                    let call_inst = builder.ins().call(fref, &[null_ptr, n_val]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                } else {
                    let slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                        cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                        (n * 8) as u32,
                        0,
                    ));
                    for i in 0..n {
                        let v = builder.use_var(vars[a_idx + 1 + i]);
                        builder.ins().stack_store(v, slot, (i * 8) as i32);
                    }
                    let regs_ptr = builder.ins().stack_addr(I64, slot, 0);
                    let n_val = builder.ins().iconst(I64, n as i64);
                    let fref = get_func_ref(&mut builder, &mut module, helpers.listnew);
                    let call_inst = builder.ins().call(fref, &[regs_ptr, n_val]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                }
            }
            OP_LISTGET => {
                // LISTGET: R[A] = R[B][R[C]], skip next instruction if found
                // This is used for foreach loops.
                // Call jit_listget which returns Ok(item) if found, Nil if not.
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.listget);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];

                // Check if result is TAG_NIL (not found) → go to ip+1 (the JMP exit)
                // If found (result is Ok(item)) → unwrap and skip the JMP (go to ip+2)
                let nil_const = builder.ins().iconst(I64, TAG_NIL as i64);
                let is_nil = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, result, nil_const);

                let jmp_block = block_map.get(&(ip + 1)).copied();
                let body_block = block_map.get(&(ip + 2)).copied();
                if let (Some(jb), Some(bb)) = (jmp_block, body_block) {
                    // If nil → fall through to JMP block; if found → unwrap and go to body
                    let unwrap_block = builder.create_block();
                    builder.ins().brif(is_nil, jb, &[], unwrap_block, &[]);

                    builder.switch_to_block(unwrap_block);
                    builder.seal_block(unwrap_block);
                    // Unwrap the Ok wrapper, then drop the wrapper itself
                    let fref2 = get_func_ref(&mut builder, &mut module, helpers.unwrap);
                    let call_inst2 = builder.ins().call(fref2, &[result]);
                    let item = builder.inst_results(call_inst2)[0];
                    let fref_drop = get_func_ref(&mut builder, &mut module, helpers.drop_rc);
                    builder.ins().call(fref_drop, &[result]);
                    builder.def_var(vars[a_idx], item);
                    builder.ins().jump(bb, &[]);
                    block_terminated = true;
                } else {
                    // Fallback: just store result
                    builder.def_var(vars[a_idx], result);
                }
            }
            OP_CALL => {
                let a = ((inst >> 16) & 0xFF) as u8;
                let bx = (inst & 0xFFFF) as usize;
                let func_idx = (bx >> 8) as u16;
                let n_args = bx & 0xFF;

                // Build array of args on the stack
                if n_args > 0 {
                    let slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                        cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                        (n_args * 8) as u32,
                        0,
                    ));
                    for i in 0..n_args {
                        let v = builder.use_var(vars[a as usize + 1 + i]);
                        builder.ins().stack_store(v, slot, (i * 8) as i32);
                    }
                    let args_ptr = builder.ins().stack_addr(I64, slot, 0);
                    let prog_ptr = builder.ins().iconst(I64, program_ptr_val as i64);
                    let func_idx_val = builder.ins().iconst(I64, func_idx as i64);
                    let n_args_val = builder.ins().iconst(I64, n_args as i64);
                    let fref = get_func_ref(&mut builder, &mut module, helpers.call);
                    let call_inst = builder.ins().call(fref, &[prog_ptr, func_idx_val, args_ptr, n_args_val]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a as usize], result);
                } else {
                    let null_ptr = builder.ins().iconst(I64, 0i64);
                    let prog_ptr = builder.ins().iconst(I64, program_ptr_val as i64);
                    let func_idx_val = builder.ins().iconst(I64, func_idx as i64);
                    let n_args_val = builder.ins().iconst(I64, 0i64);
                    let fref = get_func_ref(&mut builder, &mut module, helpers.call);
                    let call_inst = builder.ins().call(fref, &[prog_ptr, func_idx_val, null_ptr, n_args_val]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a as usize], result);
                }
            }
            OP_JPTH => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.jpth);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_JDMP => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.jdmp);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_JPAR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, &mut module, helpers.jpar);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            _ => {
                // Unknown opcode — bail out
                return None;
            }
        }
    }

    // If the last block doesn't have a terminator, add a return with TAG_NIL
    if !block_terminated {
        let nil = builder.ins().iconst(I64, TAG_NIL as i64);
        builder.ins().return_(&[nil]);
    }

    builder.seal_all_blocks();
    builder.finalize();

    module.define_function(func_id, &mut ctx).ok()?;
    module.finalize_definitions().ok()?;

    let func_ptr = module.get_finalized_function(func_id);

    Some(JitFunction {
        _module: module,
        func_ptr,
        param_count: chunk.param_count as usize,
    })
}

/// Call a compiled NanVal JIT function with u64 args, returns u64.
fn call_raw(func: &JitFunction, args: &[u64]) -> Option<u64> {
    if args.len() != func.param_count { return None; }
    Some(match args.len() {
        0 => {
            let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(func.func_ptr) };
            f()
        }
        1 => {
            let f: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0])
        }
        2 => {
            let f: extern "C" fn(u64, u64) -> u64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1])
        }
        3 => {
            let f: extern "C" fn(u64, u64, u64) -> u64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2])
        }
        4 => {
            let f: extern "C" fn(u64, u64, u64, u64) -> u64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2], args[3])
        }
        5 => {
            let f: extern "C" fn(u64, u64, u64, u64, u64) -> u64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2], args[3], args[4])
        }
        6 => {
            let f: extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2], args[3], args[4], args[5])
        }
        7 => {
            let f: extern "C" fn(u64, u64, u64, u64, u64, u64, u64) -> u64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2], args[3], args[4], args[5], args[6])
        }
        8 => {
            let f: extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64 = unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7])
        }
        _ => return None,
    })
}

/// Call a compiled NanVal JIT function with u64 args, returns u64.
/// Resets the JIT arena after each call (promoting the result if arena-tagged).
pub(crate) fn call(func: &JitFunction, args: &[u64]) -> Option<u64> {
    let mut result = call_raw(func, args)?;

    // Promote arena result and reset arena
    let rv = NanVal(result);
    if rv.is_arena_record() {
        let registry_ptr = ACTIVE_REGISTRY.with(|r| r.get());
        if !registry_ptr.is_null() {
            let promoted = rv.promote_arena_to_heap(unsafe { &*registry_ptr });
            result = promoted.0;
        }
    }
    jit_arena_reset();

    Some(result)
}

/// Compile and call in one shot (convenience wrapper).
pub(crate) fn compile_and_call(chunk: &Chunk, nan_consts: &[NanVal], args: &[u64], program: &CompiledProgram) -> Option<u64> {
    // Set active registry for arena record field name resolution
    ACTIVE_REGISTRY.with(|r| r.set(&program.type_registry as *const TypeRegistry));
    let func = compile(chunk, nan_consts, program)?;
    call(&func, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    fn jit_run(source: &str, func_name: &str, args: &[Value]) -> Option<Value> {
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
        let nan_args: Vec<u64> = args.iter().map(|v| NanVal::from_value(v).0).collect();
        let result = compile_and_call(chunk, nan_consts, &nan_args, &compiled)?;
        Some(NanVal(result).to_value())
    }

    fn jit_run_numeric(source: &str, func_name: &str, args: &[f64]) -> Option<f64> {
        let val_args: Vec<Value> = args.iter().map(|n| Value::Number(*n)).collect();
        match jit_run(source, func_name, &val_args)? {
            Value::Number(n) => Some(n),
            _ => None,
        }
    }

    #[test]
    fn cranelift_sub_nn() {
        let result = jit_run_numeric("f a:n b:n>n;-a b", "f", &[10.0, 3.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn cranelift_div_nn() {
        let result = jit_run_numeric("f a:n b:n>n;/a b", "f", &[10.0, 2.0]);
        assert_eq!(result, Some(5.0));
    }

    #[test]
    fn cranelift_subk_n() {
        let result = jit_run_numeric("f x:n>n;-x 3", "f", &[10.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn cranelift_divk_n() {
        let result = jit_run_numeric("f x:n>n;/x 4", "f", &[20.0]);
        assert_eq!(result, Some(5.0));
    }

    #[test]
    fn cranelift_neg() {
        let result = jit_run_numeric("f x:n>n;-x", "f", &[5.0]);
        assert_eq!(result, Some(-5.0));
    }

    #[test]
    fn cranelift_zero_arg_function() {
        let result = jit_run_numeric("f>n;42", "f", &[]);
        assert_eq!(result, Some(42.0));
    }

    #[test]
    fn cranelift_add_k_n() {
        let result = jit_run_numeric("f x:n>n;+x 10", "f", &[5.0]);
        assert_eq!(result, Some(15.0));
    }

    #[test]
    fn cranelift_move_op() {
        let result = jit_run_numeric("f x:n>n;x", "f", &[7.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn cranelift_arg_count_mismatch() {
        let result = jit_run_numeric("f x:n y:n>n;+x y", "f", &[1.0]);
        assert_eq!(result, None);
    }

    #[test]
    fn cranelift_move_a_ne_b() {
        let result = jit_run_numeric("f x:n>n;y=x;y", "f", &[7.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn cranelift_4_args() {
        let result = jit_run_numeric("f a:n b:n c:n d:n>n;+a +b +c d", "f", &[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(result, Some(10.0));
    }

    #[test]
    fn cranelift_5_args() {
        let result = jit_run_numeric("f a:n b:n c:n d:n e:n>n;+a +b +c +d e", "f", &[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(result, Some(15.0));
    }

    #[test]
    fn cranelift_6_args() {
        let result = jit_run_numeric("f a:n b:n c:n d:n e:n f0:n>n;+a +b +c +d +e f0", "f", &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(result, Some(21.0));
    }

    #[test]
    fn cranelift_7_args() {
        let result = jit_run_numeric("f a:n b:n c:n d:n e:n f0:n g0:n>n;+a +b +c +d +e +f0 g0", "f", &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
        assert_eq!(result, Some(28.0));
    }

    #[test]
    fn cranelift_8_args() {
        let result = jit_run_numeric("f a:n b:n c:n d:n e:n f0:n g0:n h:n>n;+a +b +c +d +e +f0 +g0 h", "f", &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        assert_eq!(result, Some(36.0));
    }

    #[test]
    fn cranelift_9_args_hits_fallback() {
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex(
            "f a:n b:n c:n d:n e:n f0:n g0:n h:n i:n>n;+a +b +c +d +e +f0 +g0 +h i"
        ).unwrap().into_iter().map(|(t, _)| t).collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        if let Some(func) = compile(chunk, nan_consts, &compiled) {
            let args: Vec<u64> = (1..=9).map(|i| NanVal::number(i as f64).0).collect();
            let result = call(&func, &args);
            assert_eq!(result, None);
        }
    }

    // ── New tests for NanVal JIT (non-numeric functions) ──

    #[test]
    fn cranelift_string_concat() {
        let result = jit_run(r#"f a:t b:t>t;+ a b"#, "f", &[Value::Text("hello".into()), Value::Text(" world".into())]);
        assert_eq!(result, Some(Value::Text("hello world".into())));
    }

    #[test]
    fn cranelift_string_constant() {
        let result = jit_run(r#"f>t;"hello""#, "f", &[]);
        assert_eq!(result, Some(Value::Text("hello".into())));
    }

    #[test]
    fn cranelift_bool_true() {
        let result = jit_run("f>b;true", "f", &[]);
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_bool_false() {
        let result = jit_run("f>b;false", "f", &[]);
        assert_eq!(result, Some(Value::Bool(false)));
    }

    #[test]
    fn cranelift_equality() {
        let result = jit_run("f a:n b:n>b;= a b", "f", &[Value::Number(5.0), Value::Number(5.0)]);
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_inequality() {
        let result = jit_run("f a:n b:n>b;!= a b", "f", &[Value::Number(5.0), Value::Number(3.0)]);
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_guard_ternary() {
        let result = jit_run("f x:n>n;>x 0{x}{0}", "f", &[Value::Number(5.0)]);
        assert_eq!(result, Some(Value::Number(5.0)));
        let result2 = jit_run("f x:n>n;>x 0{x}{0}", "f", &[Value::Number(-1.0)]);
        assert_eq!(result2, Some(Value::Number(0.0)));
    }

    #[test]
    fn cranelift_wrapok() {
        let result = jit_run("f x:n>R n t;~x", "f", &[Value::Number(42.0)]);
        assert_eq!(result, Some(Value::Ok(Box::new(Value::Number(42.0)))));
    }

    #[test]
    fn cranelift_wraperr() {
        let result = jit_run(r#"f x:t>R n t;^"bad""#, "f", &[Value::Text("bad".into())]);
        assert_eq!(result, Some(Value::Err(Box::new(Value::Text("bad".into())))));
    }

    #[test]
    fn cranelift_len_string() {
        let result = jit_run(r#"f s:t>n;len s"#, "f", &[Value::Text("hello".into())]);
        assert_eq!(result, Some(Value::Number(5.0)));
    }

    #[test]
    fn cranelift_len_list() {
        let result = jit_run("f xs:L n>n;len xs", "f", &[Value::List(vec![Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)])]);
        assert_eq!(result, Some(Value::Number(3.0)));
    }

    #[test]
    fn cranelift_not() {
        let result = jit_run("f x:b>b;! x", "f", &[Value::Bool(true)]);
        assert_eq!(result, Some(Value::Bool(false)));
    }

    #[test]
    fn cranelift_comparison_gt() {
        let result = jit_run("f a:n b:n>b;> a b", "f", &[Value::Number(5.0), Value::Number(3.0)]);
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_comparison_lt() {
        let result = jit_run("f a:n b:n>b;< a b", "f", &[Value::Number(3.0), Value::Number(5.0)]);
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_while_loop() {
        // sum 1..n using while loop
        let result = jit_run("f n:n>n;s=0;i=1;wh <= i n{s=+s i;i=+i 1};s", "f", &[Value::Number(10.0)]);
        assert_eq!(result, Some(Value::Number(55.0)));
    }

    #[test]
    fn cranelift_str_builtin() {
        let result = jit_run("f x:n>t;str x", "f", &[Value::Number(42.0)]);
        assert_eq!(result, Some(Value::Text("42".into())));
    }

    #[test]
    fn cranelift_abs_builtin() {
        let result = jit_run("f x:n>n;abs x", "f", &[Value::Number(-5.0)]);
        assert_eq!(result, Some(Value::Number(5.0)));
    }

    #[test]
    fn cranelift_function_call() {
        let result = jit_run("double x:n>n;* x 2\nf x:n>n;double x", "f", &[Value::Number(5.0)]);
        assert_eq!(result, Some(Value::Number(10.0)));
    }
}
