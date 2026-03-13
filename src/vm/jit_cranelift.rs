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
pub struct JitFunction {
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
    rou: FuncId,
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
    recfld_name: FuncId,
    recnew: FuncId,
    recwith: FuncId,
    listnew: FuncId,
    listget: FuncId,
    jpth: FuncId,
    jdmp: FuncId,
    jpar: FuncId,
    call: FuncId,
    // Type predicates
    isnum: FuncId,
    istext: FuncId,
    isbool: FuncId,
    islist: FuncId,
    // Map operations
    mapnew: FuncId,
    mget: FuncId,
    mset: FuncId,
    mhas: FuncId,
    mkeys: FuncId,
    mvals: FuncId,
    mdel: FuncId,
    // Print, trim, uniq
    prt: FuncId,
    trm: FuncId,
    unq: FuncId,
    // File I/O
    rd: FuncId,
    rdl: FuncId,
    wr: FuncId,
    wrl: FuncId,
    // HTTP
    post: FuncId,
    geth: FuncId,
    posth: FuncId,
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
        ("jit_rou", jit_rou as *const u8),
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
        ("jit_recfld_name", jit_recfld_name as *const u8),
        ("jit_recnew", jit_recnew as *const u8),
        ("jit_recwith", jit_recwith as *const u8),
        ("jit_listnew", jit_listnew as *const u8),
        ("jit_listget", jit_listget as *const u8),
        ("jit_jpth", jit_jpth as *const u8),
        ("jit_jdmp", jit_jdmp as *const u8),
        ("jit_jpar", jit_jpar as *const u8),
        ("jit_call", jit_call as *const u8),
        // Type predicates
        ("jit_isnum", jit_isnum as *const u8),
        ("jit_istext", jit_istext as *const u8),
        ("jit_isbool", jit_isbool as *const u8),
        ("jit_islist", jit_islist as *const u8),
        // Map operations
        ("jit_mapnew", jit_mapnew as *const u8),
        ("jit_mget", jit_mget as *const u8),
        ("jit_mset", jit_mset as *const u8),
        ("jit_mhas", jit_mhas as *const u8),
        ("jit_mkeys", jit_mkeys as *const u8),
        ("jit_mvals", jit_mvals as *const u8),
        ("jit_mdel", jit_mdel as *const u8),
        // Print, trim, uniq
        ("jit_prt", jit_prt as *const u8),
        ("jit_trm", jit_trm as *const u8),
        ("jit_unq", jit_unq as *const u8),
        // File I/O
        ("jit_rd", jit_rd as *const u8),
        ("jit_rdl", jit_rdl as *const u8),
        ("jit_wr", jit_wr as *const u8),
        ("jit_wrl", jit_wrl as *const u8),
        // HTTP
        ("jit_post", jit_post as *const u8),
        ("jit_geth", jit_geth as *const u8),
        ("jit_posth", jit_posth as *const u8),
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
        rou: declare_helper(module, "jit_rou", 1, 1),
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
        recfld_name: declare_helper(module, "jit_recfld_name", 3, 1),
        recnew: declare_helper(module, "jit_recnew", 4, 1),
        recwith: declare_helper(module, "jit_recwith", 4, 1),
        listnew: declare_helper(module, "jit_listnew", 2, 1),
        listget: declare_helper(module, "jit_listget", 2, 1),
        jpth: declare_helper(module, "jit_jpth", 2, 1),
        jdmp: declare_helper(module, "jit_jdmp", 1, 1),
        jpar: declare_helper(module, "jit_jpar", 1, 1),
        call: declare_helper(module, "jit_call", 4, 1),
        // Type predicates
        isnum: declare_helper(module, "jit_isnum", 1, 1),
        istext: declare_helper(module, "jit_istext", 1, 1),
        isbool: declare_helper(module, "jit_isbool", 1, 1),
        islist: declare_helper(module, "jit_islist", 1, 1),
        // Map operations
        mapnew: declare_helper(module, "jit_mapnew", 0, 1),
        mget: declare_helper(module, "jit_mget", 2, 1),
        mset: declare_helper(module, "jit_mset", 3, 1),
        mhas: declare_helper(module, "jit_mhas", 2, 1),
        mkeys: declare_helper(module, "jit_mkeys", 1, 1),
        mvals: declare_helper(module, "jit_mvals", 1, 1),
        mdel: declare_helper(module, "jit_mdel", 2, 1),
        // Print, trim, uniq
        prt: declare_helper(module, "jit_prt", 1, 1),
        trm: declare_helper(module, "jit_trm", 1, 1),
        unq: declare_helper(module, "jit_unq", 1, 1),
        // File I/O
        rd: declare_helper(module, "jit_rd", 1, 1),
        rdl: declare_helper(module, "jit_rdl", 1, 1),
        wr: declare_helper(module, "jit_wr", 2, 1),
        wrl: declare_helper(module, "jit_wrl", 2, 1),
        // HTTP
        post: declare_helper(module, "jit_post", 2, 1),
        geth: declare_helper(module, "jit_geth", 2, 1),
        posth: declare_helper(module, "jit_posth", 3, 1),
    }
}

/// Compile a single function body into the JIT module.
fn compile_function_body(
    module: &mut JITModule,
    chunk: &Chunk,
    nan_consts: &[NanVal],
    func_id: FuncId,
    helpers: &HelperFuncs,
    all_func_ids: &[FuncId],
    program: &CompiledProgram,
) -> Option<()> {
    // Build function signature: (i64, i64, ...) -> i64
    let mut sig = module.make_signature();
    for _ in 0..chunk.param_count {
        sig.params.push(AbiParam::new(I64));
    }
    sig.returns.push(AbiParam::new(I64));

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
    let mut get_func_ref = |builder: &mut FunctionBuilder<'_>, module: &mut JITModule, id: FuncId| -> cranelift_codegen::ir::FuncRef {
        *func_refs.entry(id).or_insert_with(|| module.declare_func_in_func(id, builder.func))
    };

    // Store the program pointer as a constant for jit_call fallback
    let program_ptr_val = program as *const CompiledProgram as u64;

    // Pre-pass: determine which registers are *always* written with numeric values.
    // A register is "always numeric" if every instruction that writes it produces a
    // floating-point number (i.e. can be safely bitcast to f64 without a QNAN check).
    //
    // We track two flags per register:
    //   num_write[r]     — at least one write to r is definitely numeric
    //   non_num_write[r] — at least one write to r may produce a non-number
    //
    // A register is "always numeric" iff num_write && !non_num_write.
    let mut reg_always_num = vec![false; reg_count];
    {
        let mut non_num_write = vec![false; reg_count];
        let mut num_write     = vec![false; reg_count];

        // Function parameters: the VM compiler sets all_regs_numeric when it has
        // proven every param is numeric (e.g. single-param numeric functions).
        if chunk.all_regs_numeric {
            for (i, slot) in num_write.iter_mut().enumerate().take(chunk.param_count as usize) {
                if i < reg_count { *slot = true; }
            }
        }

        for &inst in &chunk.code {
            let op = (inst >> 24) as u8;
            let a  = ((inst >> 16) & 0xFF) as usize;
            if a >= reg_count { continue; }
            match op {
                // Guaranteed numeric outputs.
                OP_ADD_NN | OP_SUB_NN | OP_MUL_NN | OP_DIV_NN
                | OP_ADDK_N | OP_SUBK_N | OP_MULK_N | OP_DIVK_N => {
                    num_write[a] = true;
                }
                // LOADK: numeric only when the constant itself is a number.
                OP_LOADK => {
                    let bx = (inst & 0xFFFF) as usize;
                    if bx < nan_consts.len() && nan_consts[bx].is_number() {
                        num_write[a] = true;
                    } else {
                        non_num_write[a] = true;
                    }
                }
                // Ops that write a non-numeric or unknown type to R[A].
                // This list is conservative: an op not mentioned here simply leaves
                // num_write[a] false, so the register won't qualify as always-numeric.
                OP_LT | OP_GT | OP_LE | OP_GE | OP_EQ | OP_NE
                | OP_NOT | OP_HAS | OP_ISNUM | OP_ISTEXT | OP_ISBOOL | OP_ISLIST
                | OP_MHAS
                | OP_MOVE
                | OP_ADD | OP_SUB | OP_MUL | OP_DIV  // may be string concat etc.
                | OP_WRAPOK | OP_WRAPERR | OP_ISOK | OP_ISERR | OP_UNWRAP
                | OP_RECFLD | OP_RECFLD_NAME | OP_LISTGET | OP_INDEX
                | OP_STR | OP_HD | OP_TL | OP_REV | OP_SRT | OP_SLC
                | OP_SPL | OP_CAT | OP_GET | OP_POST | OP_GETH | OP_POSTH
                | OP_ENV | OP_JPTH | OP_JDMP | OP_JPAR
                | OP_MAPNEW | OP_MGET | OP_MSET | OP_MDEL | OP_MKEYS | OP_MVALS
                | OP_LISTNEW | OP_LISTAPPEND
                | OP_RECNEW | OP_RECWITH
                | OP_PRT | OP_RD | OP_RDL | OP_WR | OP_WRL | OP_TRM | OP_UNQ => {
                    non_num_write[a] = true;
                }
                // Numeric-output single-register ops left as conservative default
                // (neither flag set → not always-numeric, which is safe).
                _ => {}
            }
        }

        for i in 0..reg_count {
            reg_always_num[i] = num_write[i] && !non_num_write[i];
        }
    }

    // Track whether the current block has been terminated
    let mut block_terminated = false;
    // Track whether to skip the next instruction (used by OP_POSTH data word)
    let mut skip_next = false;

    // Translate bytecode instruction by instruction
    for (ip, &inst) in chunk.code.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
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
            OP_ADD | OP_SUB | OP_MUL | OP_DIV => {
                // Inline numeric fast path: check both are numbers, do float op,
                // fall back to helper for non-numeric (e.g. string concat for ADD).
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);

                let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                let b_masked = builder.ins().band(bv, qnan_val);
                let c_masked = builder.ins().band(cv, qnan_val);
                let b_or_c = builder.ins().bor(b_masked, c_masked);
                // If either has QNAN bits set, it's not a number
                let both_num = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual, b_or_c, qnan_val);

                let num_block = builder.create_block();
                let slow_block = builder.create_block();
                let merge_block = builder.create_block();
                builder.append_block_param(merge_block, I64);

                builder.ins().brif(both_num, num_block, &[], slow_block, &[]);

                // Fast path: inline float arithmetic
                builder.switch_to_block(num_block);
                let mf = cranelift_codegen::ir::MemFlags::new();
                let bf = builder.ins().bitcast(F64, mf, bv);
                let cf = builder.ins().bitcast(F64, mf, cv);
                let result_f = match op {
                    OP_ADD => builder.ins().fadd(bf, cf),
                    OP_SUB => builder.ins().fsub(bf, cf),
                    OP_MUL => builder.ins().fmul(bf, cf),
                    OP_DIV => builder.ins().fdiv(bf, cf),
                    _ => unreachable!(),
                };
                let fast_result = builder.ins().bitcast(I64, mf, result_f);
                builder.ins().jump(merge_block, &[fast_result]);

                // Slow path: call helper (handles string concat, etc.)
                builder.switch_to_block(slow_block);
                let helper = match op {
                    OP_ADD => helpers.add,
                    OP_SUB => helpers.sub,
                    OP_MUL => helpers.mul,
                    OP_DIV => helpers.div,
                    _ => unreachable!(),
                };
                let fref = get_func_ref(&mut builder, module, helper);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let slow_result = builder.inst_results(call_inst)[0];
                builder.ins().jump(merge_block, &[slow_result]);

                builder.switch_to_block(merge_block);
                let result = builder.block_params(merge_block)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_LT | OP_GT | OP_LE | OP_GE | OP_EQ | OP_NE => {
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
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);

                // Pre-pass proved both operands are always numeric?
                let both_always_num = b_idx < reg_always_num.len() && reg_always_num[b_idx]
                    && c_idx < reg_always_num.len() && reg_always_num[c_idx];

                if both_always_num {
                    // Check if the immediately following instruction is JMPF or JMPT on
                    // the same destination register (a_idx). If so, fuse the comparison
                    // and conditional branch into a single compare-and-branch, eliminating
                    // the intermediate bool register write and the entire JMPF 3-block dispatch.
                    let next_inst = chunk.code.get(ip + 1).copied();
                    let fused = if let Some(next) = next_inst {
                        let next_op  = (next >> 24) as u8;
                        let next_a   = ((next >> 16) & 0xFF) as usize;
                        (next_op == OP_JMPF || next_op == OP_JMPT) && next_a == a_idx
                            && !block_map.contains_key(&(ip + 1))  // next instr is not a block leader
                    } else {
                        false
                    };

                    if fused {
                        // Fused compare-and-branch: emit fcmp + brif, skip next JMPF/JMPT.
                        let next = chunk.code[ip + 1];
                        let next_op = (next >> 24) as u8;
                        let sbx     = (next & 0xFFFF) as i16;
                        let target      = (ip as isize + 2 + sbx as isize) as usize;
                        let fallthrough = ip + 2;

                        let mf = cranelift_codegen::ir::MemFlags::new();
                        let bf = builder.ins().bitcast(F64, mf, bv);
                        let cf = builder.ins().bitcast(F64, mf, cv);
                        let cmp = builder.ins().fcmp(cc, bf, cf);

                        // For JMPF: jump to target when cmp is FALSE (i.e. condition false → jump).
                        // For JMPT: jump to target when cmp is TRUE.
                        let (true_dest, false_dest) = if next_op == OP_JMPF {
                            // JMPF: cmp-true → fallthrough, cmp-false → target
                            (fallthrough, target)
                        } else {
                            // JMPT: cmp-true → target, cmp-false → fallthrough
                            (target, fallthrough)
                        };

                        let true_block  = block_map.get(&true_dest).copied();
                        let false_block = block_map.get(&false_dest).copied();

                        if let (Some(tb), Some(fb)) = (true_block, false_block) {
                            builder.ins().brif(cmp, tb, &[], fb, &[]);
                            block_terminated = true;
                            skip_next = true;
                        } else {
                            // Block targets not found — fall back to non-fused path.
                            let true_val  = builder.ins().iconst(I64, TAG_TRUE  as i64);
                            let false_val = builder.ins().iconst(I64, TAG_FALSE as i64);
                            let result = builder.ins().select(cmp, true_val, false_val);
                            builder.def_var(vars[a_idx], result);
                        }
                    } else {
                        // No fusion opportunity — emit direct float comparison without QNAN check.
                        let mf = cranelift_codegen::ir::MemFlags::new();
                        let bf = builder.ins().bitcast(F64, mf, bv);
                        let cf = builder.ins().bitcast(F64, mf, cv);
                        let cmp = builder.ins().fcmp(cc, bf, cf);
                        let true_val  = builder.ins().iconst(I64, TAG_TRUE  as i64);
                        let false_val = builder.ins().iconst(I64, TAG_FALSE as i64);
                        let result = builder.ins().select(cmp, true_val, false_val);
                        builder.def_var(vars[a_idx], result);
                    }
                } else {
                    // General case: check both are numeric at runtime before comparing.
                    let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                    let b_masked = builder.ins().band(bv, qnan_val);
                    let c_masked = builder.ins().band(cv, qnan_val);
                    let b_or_c = builder.ins().bor(b_masked, c_masked);
                    let both_num = builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::NotEqual, b_or_c, qnan_val);

                    let num_block   = builder.create_block();
                    let slow_block  = builder.create_block();
                    let merge_block = builder.create_block();
                    builder.append_block_param(merge_block, I64);

                    builder.ins().brif(both_num, num_block, &[], slow_block, &[]);

                    // Fast path: inline float comparison → TAG_TRUE/TAG_FALSE
                    builder.switch_to_block(num_block);
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let bf = builder.ins().bitcast(F64, mf, bv);
                    let cf = builder.ins().bitcast(F64, mf, cv);
                    let cmp = builder.ins().fcmp(cc, bf, cf);
                    let true_val  = builder.ins().iconst(I64, TAG_TRUE  as i64);
                    let false_val = builder.ins().iconst(I64, TAG_FALSE as i64);
                    let fast_result = builder.ins().select(cmp, true_val, false_val);
                    builder.ins().jump(merge_block, &[fast_result]);

                    // Slow path: call helper
                    builder.switch_to_block(slow_block);
                    let helper = match op {
                        OP_LT => helpers.lt,
                        OP_GT => helpers.gt,
                        OP_LE => helpers.le,
                        OP_GE => helpers.ge,
                        OP_EQ => helpers.eq,
                        OP_NE => helpers.ne,
                        _ => unreachable!(),
                    };
                    let fref = get_func_ref(&mut builder, module, helper);
                    let call_inst = builder.ins().call(fref, &[bv, cv]);
                    let slow_result = builder.inst_results(call_inst)[0];
                    builder.ins().jump(merge_block, &[slow_result]);

                    builder.switch_to_block(merge_block);
                    let result = builder.block_params(merge_block)[0];
                    builder.def_var(vars[a_idx], result);
                }
            }
            OP_MOVE => {
                if a_idx != b_idx {
                    let bv = builder.use_var(vars[b_idx]);
                    // Inline is_heap check: skip clone_rc for numbers (hot path)
                    let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                    let masked = builder.ins().band(bv, qnan_val);
                    let is_heap = builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::Equal, masked, qnan_val);
                    let clone_block = builder.create_block();
                    let after_block = builder.create_block();
                    builder.ins().brif(is_heap, clone_block, &[], after_block, &[]);

                    builder.switch_to_block(clone_block);
                    let fref = get_func_ref(&mut builder, module, helpers.jit_move);
                    builder.ins().call(fref, &[bv]);
                    builder.ins().jump(after_block, &[]);

                    builder.switch_to_block(after_block);
                    builder.def_var(vars[a_idx], bv);
                }
            }
            OP_NOT => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.not);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_NEG => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.neg);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_WRAPOK => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.wrapok);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_WRAPERR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.wraperr);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ISOK => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.isok);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ISERR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.iserr);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_UNWRAP => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.unwrap);
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
                    let fref = get_func_ref(&mut builder, module, helpers.jit_move);
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
            OP_JMPF | OP_JMPT => {
                let sbx = (inst & 0xFFFF) as i16;
                let target = (ip as isize + 1 + sbx as isize) as usize;
                let fallthrough = ip + 1;
                let av = builder.use_var(vars[a_idx]);

                // Inline truthy: false if val==TAG_NIL or val==TAG_FALSE, true otherwise.
                // This covers numbers (truthy when != 0.0, but 0.0 bits != TAG_NIL/TAG_FALSE),
                // booleans, and all heap values. For number 0.0 (bits=0), it's truthy=true here
                // but should be falsy — so we need a number check too.
                // Full inline: is_number ? (f64 != 0.0) : (val != TAG_NIL && val != TAG_FALSE)
                let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                let masked = builder.ins().band(av, qnan_val);
                let is_num = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual, masked, qnan_val);

                let num_truthy_block = builder.create_block();
                let tag_truthy_block = builder.create_block();
                let merge_truthy = builder.create_block();
                builder.append_block_param(merge_truthy, I64);

                builder.ins().brif(is_num, num_truthy_block, &[], tag_truthy_block, &[]);

                // Number path: truthy if f64 != 0.0
                builder.switch_to_block(num_truthy_block);
                let mf = cranelift_codegen::ir::MemFlags::new();
                let af = builder.ins().bitcast(F64, mf, av);
                let zero = builder.ins().f64const(0.0);
                let cmp = builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::NotEqual, af, zero);
                let num_result = builder.ins().uextend(I64, cmp);
                builder.ins().jump(merge_truthy, &[num_result]);

                // Tag path: truthy if val != TAG_NIL && val != TAG_FALSE
                builder.switch_to_block(tag_truthy_block);
                let nil_val = builder.ins().iconst(I64, TAG_NIL as i64);
                let false_val = builder.ins().iconst(I64, TAG_FALSE as i64);
                let not_nil = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual, av, nil_val);
                let not_false = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual, av, false_val);
                let tag_truthy = builder.ins().band(not_nil, not_false);
                let tag_result = builder.ins().uextend(I64, tag_truthy);
                builder.ins().jump(merge_truthy, &[tag_result]);

                builder.switch_to_block(merge_truthy);
                let truthy_val = builder.block_params(merge_truthy)[0];

                if let (Some(&target_block), Some(&fall_block)) = (block_map.get(&target), block_map.get(&fallthrough)) {
                    if op == OP_JMPF {
                        builder.ins().brif(truthy_val, fall_block, &[], target_block, &[]);
                    } else {
                        builder.ins().brif(truthy_val, target_block, &[], fall_block, &[]);
                    }
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
                let fref = get_func_ref(&mut builder, module, helpers.len);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_STR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.str_fn);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_NUM => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.num);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ABS => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.abs);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MIN => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.min);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MAX => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.max);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_FLR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.flr);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_CEL => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.cel);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ROU => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.rou);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RND0 => {
                let fref = get_func_ref(&mut builder, module, helpers.rnd0);
                let call_inst = builder.ins().call(fref, &[]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RND2 => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.rnd2);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_NOW => {
                let fref = get_func_ref(&mut builder, module, helpers.now);
                let call_inst = builder.ins().call(fref, &[]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ENV => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.env);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_GET => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.get);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_SPL => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.spl);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_CAT => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.cat);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_HAS => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.has);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_HD => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.hd);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_TL => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.tl);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_REV => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.rev);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_SRT => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.srt);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_SLC => {
                // slc(R[B], R[C], R[C+1])
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let dv = builder.use_var(vars[c_idx + 1]);
                let fref = get_func_ref(&mut builder, module, helpers.slc);
                let call_inst = builder.ins().call(fref, &[bv, cv, dv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_LISTAPPEND => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.listappend);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_INDEX => {
                // R[A] = R[B][C] where C is a literal index
                let bv = builder.use_var(vars[b_idx]);
                let idx_val = builder.ins().iconst(I64, c_idx as i64);
                let fref = get_func_ref(&mut builder, module, helpers.index);
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
                // SAFETY: MemFlags::trusted() is valid because:
                // (a) `ptr` was produced from a TAG_ARENA_REC NanVal so it points into
                //     the live bump arena buffer, and
                // (b) `c_idx` is a compile-time constant encoded by the register compiler
                //     from a type-checked field access, so it is always < n_fields.
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
                let fref_move = get_func_ref(&mut builder, module, helpers.jit_move);
                let move_inst = builder.ins().call(fref_move, &[field_val]);
                let _cloned = builder.inst_results(move_inst)[0];
                builder.ins().jump(skip_clone_block, &[]);

                // Skip clone path: field_val is a number, no RC management needed
                builder.switch_to_block(skip_clone_block);
                builder.ins().jump(merge_block, &[field_val]);

                // Heap path: call jit_recfld
                builder.switch_to_block(heap_block);
                let field_idx_val = builder.ins().iconst(I64, c_idx as i64);
                let fref = get_func_ref(&mut builder, module, helpers.recfld);
                let call_inst = builder.ins().call(fref, &[bv, field_idx_val]);
                let heap_result = builder.inst_results(call_inst)[0];
                builder.ins().jump(merge_block, &[heap_result]);

                // Merge
                builder.switch_to_block(merge_block);
                let result = builder.block_params(merge_block)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RECFLD_NAME => {
                let b_idx = ((inst >> 8) & 0xFF) as usize;
                let c_idx = (inst & 0xFF) as usize;
                let bv = builder.use_var(vars[b_idx]);
                // Get field name from chunk constants as a null-terminated C string
                let cstring = match &chunk.constants[c_idx] {
                    crate::interpreter::Value::Text(s) => std::ffi::CString::new(s.as_bytes()).ok()?,
                    _ => return None,
                };
                let leaked = Box::leak(Box::new(cstring));
                let field_name_ptr = leaked.as_ptr() as u64;
                let field_name_val = builder.ins().iconst(I64, field_name_ptr as i64);
                // Get registry pointer
                let registry_ptr = ACTIVE_REGISTRY.with(|r| r.get() as u64);
                let registry_val = builder.ins().iconst(I64, registry_ptr as i64);
                let fref = get_func_ref(&mut builder, module, helpers.recfld_name);
                let call_inst = builder.ins().call(fref, &[bv, field_name_val, registry_val]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RECNEW => {
                let bx = (inst & 0xFFFF) as usize;
                let type_id = (bx >> 8) as u16;
                let n_fields = bx & 0xFF;
                let record_size = 8 + n_fields * 8; // ArenaRecord header + inline fields

                // Inline bump allocation from arena.
                // BumpArena is #[repr(C)]: buf_ptr(0), buf_cap(8), offset(16).
                let arena_ptr = jit_arena_ptr();
                let arena_ptr_val = builder.ins().iconst(I64, arena_ptr as i64);

                // Load arena.offset
                let cur_offset = builder.ins().load(I64,
                    cranelift_codegen::ir::MemFlags::trusted(), arena_ptr_val, 16);
                // aligned_offset = (offset + 7) & !7  (already 8-aligned in practice)
                let seven = builder.ins().iconst(I64, 7);
                let off_plus_7 = builder.ins().iadd(cur_offset, seven);
                let neg8 = builder.ins().iconst(I64, !7i64);
                let aligned = builder.ins().band(off_plus_7, neg8);
                // new_offset = aligned + record_size
                let size_val = builder.ins().iconst(I64, record_size as i64);
                let new_offset = builder.ins().iadd(aligned, size_val);
                // Load arena.buf_cap and check space
                let buf_cap = builder.ins().load(I64,
                    cranelift_codegen::ir::MemFlags::trusted(), arena_ptr_val, 8);
                let has_space = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThanOrEqual,
                    new_offset, buf_cap);

                let alloc_block = builder.create_block();
                let fallback_block = builder.create_block();
                let merge_block = builder.create_block();
                builder.append_block_param(merge_block, I64);

                builder.ins().brif(has_space, alloc_block, &[], fallback_block, &[]);

                // ── Inline alloc path ──
                builder.switch_to_block(alloc_block);
                // rec_ptr = arena.buf_ptr + aligned_offset
                let buf_ptr = builder.ins().load(I64,
                    cranelift_codegen::ir::MemFlags::trusted(), arena_ptr_val, 0);
                let rec_ptr = builder.ins().iadd(buf_ptr, aligned);
                // Write ArenaRecord header: type_id(u16) | n_fields(u16) | pad(u32) as u64
                let header = ((n_fields as u64) << 16) | (type_id as u64);
                let header_val = builder.ins().iconst(I64, header as i64);
                builder.ins().store(cranelift_codegen::ir::MemFlags::trusted(),
                    header_val, rec_ptr, 0);
                // Write field values and clone_rc heap fields
                for i in 0..n_fields {
                    let field_v = builder.use_var(vars[a_idx + 1 + i]);
                    let field_off = (8 + i * 8) as i32;
                    builder.ins().store(cranelift_codegen::ir::MemFlags::trusted(),
                        field_v, rec_ptr, field_off);
                    // Inline is_heap check: if (val & QNAN) == QNAN → call jit_move (clone_rc)
                    // For numbers (hot path), this branch is not taken.
                    let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                    let masked = builder.ins().band(field_v, qnan_val);
                    let is_heap = builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::Equal, masked, qnan_val);
                    let do_clone = builder.create_block();
                    let after_clone = builder.create_block();
                    builder.ins().brif(is_heap, do_clone, &[], after_clone, &[]);

                    builder.switch_to_block(do_clone);
                    let fref_move = get_func_ref(&mut builder, module, helpers.jit_move);
                    builder.ins().call(fref_move, &[field_v]);
                    builder.ins().jump(after_clone, &[]);

                    builder.switch_to_block(after_clone);
                }
                // Update arena.offset = new_offset
                builder.ins().store(cranelift_codegen::ir::MemFlags::trusted(),
                    new_offset, arena_ptr_val, 16);
                // Result = TAG_ARENA_REC | rec_ptr
                let tag_val = builder.ins().iconst(I64, TAG_ARENA_REC as i64);
                let result_val = builder.ins().bor(rec_ptr, tag_val);
                builder.ins().jump(merge_block, &[result_val]);

                // ── Fallback path: arena full → call jit_recnew helper ──
                builder.switch_to_block(fallback_block);
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
                let type_id_and_nfields = ((type_id as u64) << 16) | (n_fields as u64);
                let type_id_nfields_val = builder.ins().iconst(I64, type_id_and_nfields as i64);
                let registry_ptr_val = builder.ins().iconst(I64, &program.type_registry as *const TypeRegistry as i64);
                let fref = get_func_ref(&mut builder, module, helpers.recnew);
                let call_inst = builder.ins().call(fref, &[arena_ptr_val, type_id_nfields_val, regs_ptr, registry_ptr_val]);
                let fb_result = builder.inst_results(call_inst)[0];
                builder.ins().jump(merge_block, &[fb_result]);

                // ── Merge ──
                builder.switch_to_block(merge_block);
                let result = builder.block_params(merge_block)[0];
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
                let fref = get_func_ref(&mut builder, module, helpers.recwith);
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
                    let fref = get_func_ref(&mut builder, module, helpers.listnew);
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
                    let fref = get_func_ref(&mut builder, module, helpers.listnew);
                    let call_inst = builder.ins().call(fref, &[regs_ptr, n_val]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                }
            }
            OP_LISTGET => {
                // LISTGET: R[A] = R[B][R[C]], skip next instruction if found.
                // Used for foreach loops. Inlined in Cranelift IR to eliminate
                // 3 C-ABI call overhead (jit_listget + jit_unwrap + jit_drop_rc)
                // and the malloc/free of the OkVal wrapper that the old path did.
                //
                // HeapObj::List memory layout (ptr = bv & PTR_MASK):
                //   [ptr + 0]  discriminant = 1 for List variant
                //   [ptr + 8]  Vec.len  (usize)
                //   [ptr + 16] Vec.data_ptr  (*mut NanVal, each slot is u64/8 bytes)
                //   [ptr + 24] Vec.cap  (usize)
                // This layout was confirmed with a runtime probe of the actual types.
                //
                // Fast path (bv is TAG_LIST and cv is a number):
                //   idx_u = fcvt_to_uint_sat(bitcast_f64(cv))  // NaN/neg → 0
                //   if idx_u < [ptr+8]:
                //     elem = load [ptr+16] + idx_u*8
                //     if (elem & QNAN) == QNAN: call jit_move(elem)  // clone RC
                //     R[A] = elem; jump → body_block
                //   else:
                //     jump → jmp_block  (exit loop, out-of-bounds)
                //
                // Any guard failure (not a list, not a number idx) → jmp_block.
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let mf_plain = cranelift_codegen::ir::MemFlags::new();
                let mf_trusted = cranelift_codegen::ir::MemFlags::trusted();
                let ic_eq = cranelift_codegen::ir::condcodes::IntCC::Equal;
                let ic_ult = cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThan;

                let jmp_block = block_map.get(&(ip + 1)).copied();
                let body_block = block_map.get(&(ip + 2)).copied();

                if let (Some(jb), Some(bb)) = (jmp_block, body_block) {
                    // ── Guard 1: bv must be a list (tag == TAG_LIST) ──
                    let tag_mask_c = builder.ins().iconst(I64, TAG_MASK as i64);
                    let tag = builder.ins().band(bv, tag_mask_c);
                    let list_tag_c = builder.ins().iconst(I64, TAG_LIST as i64);
                    let is_list = builder.ins().icmp(ic_eq, tag, list_tag_c);

                    let check_num_block = builder.create_block();
                    builder.ins().brif(is_list, check_num_block, &[], jb, &[]);

                    // ── Guard 2: cv must be a number ((cv & QNAN) != QNAN) ──
                    builder.switch_to_block(check_num_block);
                    builder.seal_block(check_num_block);
                    let qnan_c = builder.ins().iconst(I64, QNAN as i64);
                    let cv_masked = builder.ins().band(cv, qnan_c);
                    // is_not_num = (cv_masked == QNAN) — true means non-number, exit loop
                    let is_not_num = builder.ins().icmp(ic_eq, cv_masked, qnan_c);

                    let load_block = builder.create_block();
                    builder.ins().brif(is_not_num, jb, &[], load_block, &[]);

                    // ── Load Vec metadata and bounds check ──
                    builder.switch_to_block(load_block);
                    builder.seal_block(load_block);

                    // ptr = bv & PTR_MASK  (points to HeapObj::List inner value)
                    let ptr_mask_c = builder.ins().iconst(I64, PTR_MASK as i64);
                    let ptr = builder.ins().band(bv, ptr_mask_c);

                    // vec_len = *[ptr + 8]
                    // SAFETY: TAG_LIST check guarantees ptr points to a live HeapObj::List.
                    let vec_len = builder.ins().load(I64, mf_trusted, ptr, 8);

                    // idx_u = (u64) cv interpreted as f64, saturating cast
                    // fcvt_to_uint_sat: NaN → 0, negative → 0, overflow → u64::MAX
                    let cv_f = builder.ins().bitcast(F64, mf_plain, cv);
                    let idx_u = builder.ins().fcvt_to_uint_sat(I64, cv_f);

                    // Bounds check: idx_u < vec_len
                    let in_bounds = builder.ins().icmp(ic_ult, idx_u, vec_len);

                    let in_bounds_block = builder.create_block();
                    builder.ins().brif(in_bounds, in_bounds_block, &[], jb, &[]);

                    // ── In-bounds: load element and optionally clone RC ──
                    builder.switch_to_block(in_bounds_block);
                    builder.seal_block(in_bounds_block);

                    // data_ptr = *[ptr + 16]  (pointer to the Vec's backing heap allocation)
                    // SAFETY: in-bounds guarantee holds; HeapObj::List layout confirmed.
                    let data_ptr = builder.ins().load(I64, mf_trusted, ptr, 16);

                    // elem_addr = data_ptr + idx_u * 8
                    let eight = builder.ins().iconst(I64, 8i64);
                    let byte_off = builder.ins().imul(idx_u, eight);
                    let elem_addr = builder.ins().iadd(data_ptr, byte_off);
                    // SAFETY: idx_u < vec_len so elem_addr is within the allocation.
                    let elem = builder.ins().load(I64, mf_trusted, elem_addr, 0);

                    // Increment RC if elem is a heap value: (elem & QNAN) == QNAN.
                    // For numeric elements (the hot path) this branch is eliminated.
                    let elem_masked = builder.ins().band(elem, qnan_c);
                    let elem_is_heap = builder.ins().icmp(ic_eq, elem_masked, qnan_c);

                    let clone_block = builder.create_block();
                    let after_clone_block = builder.create_block();
                    builder.ins().brif(elem_is_heap, clone_block, &[], after_clone_block, &[]);

                    builder.switch_to_block(clone_block);
                    builder.seal_block(clone_block);
                    // jit_move clones the RC (no-op for numbers) and returns value unchanged.
                    // We discard the return value here; we already have `elem`.
                    let fref_move = get_func_ref(&mut builder, module, helpers.jit_move);
                    builder.ins().call(fref_move, &[elem]);
                    builder.ins().jump(after_clone_block, &[]);

                    builder.switch_to_block(after_clone_block);
                    builder.seal_block(after_clone_block);
                    builder.def_var(vars[a_idx], elem);
                    builder.ins().jump(bb, &[]);

                    block_terminated = true;
                } else {
                    // Fallback: call the extern helper when block_map doesn't have the
                    // expected successor blocks (should not occur in normal foreach).
                    let fref = get_func_ref(&mut builder, module, helpers.listget);
                    let call_inst = builder.ins().call(fref, &[bv, cv]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                }
            }
            OP_FOREACHPREP => {
                // FOREACHPREP: validate list and load item[0] into R[A].
                // R[C] (idx_reg) is 0.0 on entry (set by preceding LOADK).
                // Inlined: same direct memory access as OP_LISTGET above.
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let mf_plain = cranelift_codegen::ir::MemFlags::new();
                let mf_trusted = cranelift_codegen::ir::MemFlags::trusted();
                let ic_eq = cranelift_codegen::ir::condcodes::IntCC::Equal;
                let ic_ult = cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThan;

                let jmp_block = block_map.get(&(ip + 1)).copied();
                let body_block = block_map.get(&(ip + 2)).copied();
                if let (Some(jb), Some(bb)) = (jmp_block, body_block) {
                    let tag_mask_c = builder.ins().iconst(I64, TAG_MASK as i64);
                    let tag = builder.ins().band(bv, tag_mask_c);
                    let list_tag_c = builder.ins().iconst(I64, TAG_LIST as i64);
                    let is_list = builder.ins().icmp(ic_eq, tag, list_tag_c);

                    let check_num_block = builder.create_block();
                    builder.ins().brif(is_list, check_num_block, &[], jb, &[]);

                    builder.switch_to_block(check_num_block);
                    builder.seal_block(check_num_block);
                    let qnan_c = builder.ins().iconst(I64, QNAN as i64);
                    let cv_masked = builder.ins().band(cv, qnan_c);
                    let is_not_num = builder.ins().icmp(ic_eq, cv_masked, qnan_c);

                    let load_block = builder.create_block();
                    builder.ins().brif(is_not_num, jb, &[], load_block, &[]);

                    builder.switch_to_block(load_block);
                    builder.seal_block(load_block);
                    let ptr_mask_c = builder.ins().iconst(I64, PTR_MASK as i64);
                    let ptr = builder.ins().band(bv, ptr_mask_c);
                    // HeapObj layout (with 8-byte discriminant prefix):
                    //   ptr+ 8 = Vec.cap   (capacity)
                    //   ptr+16 = Vec.data_ptr
                    //   ptr+24 = Vec.len   (length — use this for bounds check)
                    let vec_len = builder.ins().load(I64, mf_trusted, ptr, 24);
                    let cv_f = builder.ins().bitcast(F64, mf_plain, cv);
                    let idx_u = builder.ins().fcvt_to_uint_sat(I64, cv_f);
                    let in_bounds = builder.ins().icmp(ic_ult, idx_u, vec_len);

                    let in_bounds_block = builder.create_block();
                    builder.ins().brif(in_bounds, in_bounds_block, &[], jb, &[]);

                    builder.switch_to_block(in_bounds_block);
                    builder.seal_block(in_bounds_block);
                    let data_ptr = builder.ins().load(I64, mf_trusted, ptr, 16);
                    let eight = builder.ins().iconst(I64, 8i64);
                    let byte_off = builder.ins().imul(idx_u, eight);
                    let elem_addr = builder.ins().iadd(data_ptr, byte_off);
                    let elem = builder.ins().load(I64, mf_trusted, elem_addr, 0);

                    let elem_masked = builder.ins().band(elem, qnan_c);
                    let elem_is_heap = builder.ins().icmp(ic_eq, elem_masked, qnan_c);
                    let clone_block = builder.create_block();
                    let after_clone_block = builder.create_block();
                    builder.ins().brif(elem_is_heap, clone_block, &[], after_clone_block, &[]);

                    builder.switch_to_block(clone_block);
                    builder.seal_block(clone_block);
                    let fref_move = get_func_ref(&mut builder, module, helpers.jit_move);
                    builder.ins().call(fref_move, &[elem]);
                    builder.ins().jump(after_clone_block, &[]);

                    builder.switch_to_block(after_clone_block);
                    builder.seal_block(after_clone_block);
                    builder.def_var(vars[a_idx], elem);
                    builder.ins().jump(bb, &[]);
                    block_terminated = true;
                } else {
                    let fref = get_func_ref(&mut builder, module, helpers.listget);
                    let call_inst = builder.ins().call(fref, &[bv, cv]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                }
            }
            OP_FOREACHNEXT => {
                // FOREACHNEXT: R[C] += 1; load R[B][R[C]] into R[A] if in-bounds.
                // Inlined: increment index as f64, then direct memory access.
                let cv = builder.use_var(vars[c_idx]);
                let mf_plain = cranelift_codegen::ir::MemFlags::new();
                let mf_trusted = cranelift_codegen::ir::MemFlags::trusted();
                let ic_eq = cranelift_codegen::ir::condcodes::IntCC::Equal;
                let ic_ult = cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThan;

                // Increment idx: bitcast i64→f64, add 1.0, bitcast f64→i64
                let cv_f64 = builder.ins().bitcast(F64, mf_plain, cv);
                let one_f64 = builder.ins().f64const(1.0);
                let new_idx_f64 = builder.ins().fadd(cv_f64, one_f64);
                let new_idx = builder.ins().bitcast(I64, mf_plain, new_idx_f64);
                builder.def_var(vars[c_idx], new_idx);

                let bv = builder.use_var(vars[b_idx]);

                let jmp_block = block_map.get(&(ip + 1)).copied();
                let body_block = block_map.get(&(ip + 2)).copied();
                if let (Some(jb), Some(bb)) = (jmp_block, body_block) {
                    // Extract ptr from list NanVal (already validated in FOREACHPREP)
                    let ptr_mask_c = builder.ins().iconst(I64, PTR_MASK as i64);
                    let ptr = builder.ins().band(bv, ptr_mask_c);
                    // ptr+24 = Vec.len (length); ptr+8 = Vec.cap (capacity)
                    let vec_len = builder.ins().load(I64, mf_trusted, ptr, 24);

                    let idx_u = builder.ins().fcvt_to_uint_sat(I64, new_idx_f64);
                    let in_bounds = builder.ins().icmp(ic_ult, idx_u, vec_len);

                    let in_bounds_block = builder.create_block();
                    builder.ins().brif(in_bounds, in_bounds_block, &[], jb, &[]);

                    builder.switch_to_block(in_bounds_block);
                    builder.seal_block(in_bounds_block);
                    let data_ptr = builder.ins().load(I64, mf_trusted, ptr, 16);
                    let eight = builder.ins().iconst(I64, 8i64);
                    let byte_off = builder.ins().imul(idx_u, eight);
                    let elem_addr = builder.ins().iadd(data_ptr, byte_off);
                    let elem = builder.ins().load(I64, mf_trusted, elem_addr, 0);

                    let qnan_c = builder.ins().iconst(I64, QNAN as i64);
                    let elem_masked = builder.ins().band(elem, qnan_c);
                    let elem_is_heap = builder.ins().icmp(ic_eq, elem_masked, qnan_c);
                    let clone_block = builder.create_block();
                    let after_clone_block = builder.create_block();
                    builder.ins().brif(elem_is_heap, clone_block, &[], after_clone_block, &[]);

                    builder.switch_to_block(clone_block);
                    builder.seal_block(clone_block);
                    let fref_move = get_func_ref(&mut builder, module, helpers.jit_move);
                    builder.ins().call(fref_move, &[elem]);
                    builder.ins().jump(after_clone_block, &[]);

                    builder.switch_to_block(after_clone_block);
                    builder.seal_block(after_clone_block);
                    builder.def_var(vars[a_idx], elem);
                    builder.ins().jump(bb, &[]);
                    block_terminated = true;
                } else {
                    let fref = get_func_ref(&mut builder, module, helpers.listget);
                    let call_inst = builder.ins().call(fref, &[bv, new_idx]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                }
            }
            // ── Fused compare-and-skip for numeric guard chains ──────────────
            // ABC: A = reg, B = unused, C = constant pool index (ki).
            // If condition TRUE → skip next instruction (the OP_JMP), enter body.
            // If condition FALSE → fall through to the OP_JMP that skips the body.
            op if op == OP_CMPK_GE_N || op == OP_CMPK_GT_N || op == OP_CMPK_LT_N
                || op == OP_CMPK_LE_N || op == OP_CMPK_EQ_N || op == OP_CMPK_NE_N => {
                let ki = (inst & 0xFF) as usize;
                let lhs = builder.use_var(vars[a_idx]);

                // Both operands are guaranteed numeric by the compiler; bitcast to f64.
                let mf = cranelift_codegen::ir::MemFlags::new();
                let lhs_f64 = builder.ins().bitcast(F64, mf, lhs);
                let rhs_f64 = if ki < nan_consts.len() {
                    builder.ins().f64const(nan_consts[ki].as_number())
                } else {
                    builder.ins().f64const(0.0)
                };

                use cranelift_codegen::ir::condcodes::FloatCC;
                let cc = if op == OP_CMPK_GE_N {
                    FloatCC::GreaterThanOrEqual
                } else if op == OP_CMPK_GT_N {
                    FloatCC::GreaterThan
                } else if op == OP_CMPK_LT_N {
                    FloatCC::LessThan
                } else if op == OP_CMPK_LE_N {
                    FloatCC::LessThanOrEqual
                } else if op == OP_CMPK_EQ_N {
                    FloatCC::Equal
                } else {
                    FloatCC::NotEqual  // OP_CMPK_NE_N
                };
                let cmp = builder.ins().fcmp(cc, lhs_f64, rhs_f64);

                // ip + 1 = the OP_JMP that skips the body (taken when condition FALSE)
                // ip + 2 = first instruction of the guard body (taken when condition TRUE)
                let jmp_block  = block_map.get(&(ip + 1)).copied();
                let body_block = block_map.get(&(ip + 2)).copied();
                if let (Some(jb), Some(bb)) = (jmp_block, body_block) {
                    // condition TRUE → body (skip the JMP); condition FALSE → JMP block
                    builder.ins().brif(cmp, bb, &[], jb, &[]);
                    block_terminated = true;
                }
                // else: blocks not found → JIT bails (should not happen in practice)
            }
            OP_CALL => {
                let a = ((inst >> 16) & 0xFF) as u8;
                let bx = (inst & 0xFFFF) as usize;
                let func_idx = bx >> 8;
                let n_args = bx & 0xFF;

                if func_idx < all_func_ids.len() {
                    // Direct call: the target function is compiled in this module
                    let target_fid = all_func_ids[func_idx];
                    let target_fref = get_func_ref(&mut builder, module, target_fid);
                    let mut call_args = Vec::with_capacity(n_args);
                    for i in 0..n_args {
                        call_args.push(builder.use_var(vars[a as usize + 1 + i]));
                    }
                    let call_inst = builder.ins().call(target_fref, &call_args);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a as usize], result);
                } else {
                    // Fallback: use jit_call helper for out-of-range func indices
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
                        let fref = get_func_ref(&mut builder, module, helpers.call);
                        let call_inst = builder.ins().call(fref, &[prog_ptr, func_idx_val, args_ptr, n_args_val]);
                        let result = builder.inst_results(call_inst)[0];
                        builder.def_var(vars[a as usize], result);
                    } else {
                        let null_ptr = builder.ins().iconst(I64, 0i64);
                        let prog_ptr = builder.ins().iconst(I64, program_ptr_val as i64);
                        let func_idx_val = builder.ins().iconst(I64, func_idx as i64);
                        let n_args_val = builder.ins().iconst(I64, 0i64);
                        let fref = get_func_ref(&mut builder, module, helpers.call);
                        let call_inst = builder.ins().call(fref, &[prog_ptr, func_idx_val, null_ptr, n_args_val]);
                        let result = builder.inst_results(call_inst)[0];
                        builder.def_var(vars[a as usize], result);
                    }
                }
            }
            OP_JPTH => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.jpth);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_JDMP => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.jdmp);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_JPAR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.jpar);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            // ── Type predicates (1-arg → 1 return) ──
            OP_ISNUM => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.isnum);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ISTEXT => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.istext);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ISBOOL => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.isbool);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ISLIST => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.islist);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            // ── Map operations ──
            OP_MAPNEW => {
                let fref = get_func_ref(&mut builder, module, helpers.mapnew);
                let call_inst = builder.ins().call(fref, &[]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MGET => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.mget);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MSET => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let cv1 = builder.use_var(vars[c_idx + 1]);
                let fref = get_func_ref(&mut builder, module, helpers.mset);
                let call_inst = builder.ins().call(fref, &[bv, cv, cv1]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MHAS => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.mhas);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MKEYS => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.mkeys);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MVALS => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.mvals);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MDEL => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.mdel);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            // ── Print, Trim, Uniq ──
            OP_PRT => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.prt);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_TRM => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.trm);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_UNQ => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.unq);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            // ── File I/O ──
            OP_RD => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.rd);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RDL => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.rdl);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_WR => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.wr);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_WRL => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.wrl);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            // ── HTTP ──
            OP_POST => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.post);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_GETH => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.geth);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_POSTH => {
                let bv = builder.use_var(vars[b_idx]);  // url
                let cv = builder.use_var(vars[c_idx]);  // body
                // Consume the next instruction (data word) at compile time
                let data_inst = chunk.code[ip + 1];
                skip_next = true;
                let d_idx = ((data_inst >> 16) & 0xFF) as usize;  // headers register
                let dv = builder.use_var(vars[d_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.posth);
                let call_inst = builder.ins().call(fref, &[bv, cv, dv]);
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
    Some(())
}

/// Compile ALL functions in the program into native code via Cranelift (two-pass).
///
/// First pass: declare all functions in the JIT module.
/// Second pass: compile all function bodies with direct cross-function calls.
/// Returns a JitFunction for the entry function at `entry_idx`.
pub fn compile(chunk: &Chunk, _nan_consts: &[NanVal], program: &CompiledProgram) -> Option<JitFunction> {
    // Find the entry function index by matching chunk pointer
    let entry_idx = program.chunks.iter().position(|c| std::ptr::eq(c, chunk))?;
    compile_program(program, entry_idx)
}

/// Compile all functions in the program and return a JitFunction for the entry at `entry_idx`.
fn compile_program(program: &CompiledProgram, entry_idx: usize) -> Option<JitFunction> {
    let mut flag_builder = settings::builder();
    flag_builder.set("opt_level", "speed").ok()?;
    let isa_builder = cranelift_native::builder().ok()?;
    let isa = isa_builder.finish(settings::Flags::new(flag_builder)).ok()?;

    let mut jit_builder = JITBuilder::with_isa(isa, default_libcall_names());
    register_helpers(&mut jit_builder);
    let mut module = JITModule::new(jit_builder);
    let helpers = declare_all_helpers(&mut module);

    // First pass: declare ALL functions in the module
    let mut func_ids = Vec::with_capacity(program.chunks.len());
    for (i, chunk) in program.chunks.iter().enumerate() {
        let name = format!("ilo_{}", program.func_names[i]);
        let mut sig = module.make_signature();
        for _ in 0..chunk.param_count {
            sig.params.push(AbiParam::new(I64));
        }
        sig.returns.push(AbiParam::new(I64));
        let fid = module.declare_function(&name, Linkage::Local, &sig).ok()?;
        func_ids.push(fid);
    }

    // Second pass: compile ALL function bodies with func_ids available for direct calls
    for (i, (chunk, nan_consts)) in program.chunks.iter().zip(program.nan_constants.iter()).enumerate() {
        compile_function_body(
            &mut module, chunk, nan_consts, func_ids[i], &helpers, &func_ids, program,
        )?;
    }

    module.finalize_definitions().ok()?;

    let entry_func_id = func_ids[entry_idx];
    let func_ptr = module.get_finalized_function(entry_func_id);
    let param_count = program.chunks[entry_idx].param_count as usize;

    Some(JitFunction {
        _module: module,
        func_ptr,
        param_count,
    })
}

/// Call a compiled NanVal JIT function with u64 args, returns u64.
///
/// # Safety (internal)
/// Each `transmute` casts the JIT function pointer to a typed `extern "C"` fn.
/// This is sound because:
/// 1. `compile()` generates code using the SystemV/Win64 C calling convention
///    (Cranelift's `CallConv::SystemV` / platform default).
/// 2. All parameters and the return value are `u64` (NanVal bit patterns),
///    matching the `I64` Cranelift type used for every parameter and return.
/// 3. The function pointer is obtained from `module.get_finalized_function()`
///    which returns executable memory with the correct entry point.
/// 4. `args.len() == func.param_count` is checked before dispatch.
fn call_raw(func: &JitFunction, args: &[u64]) -> Option<u64> {
    if args.len() != func.param_count { return None; }
    Some(match args.len() {
        0 => {
            // SAFETY: see call_raw doc — JIT compiled with 0 I64 params, returns I64.
            let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(func.func_ptr) };
            f()
        }
        1 => {
            // SAFETY: see call_raw doc — JIT compiled with 1 I64 param, returns I64.
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
pub fn call(func: &JitFunction, args: &[u64]) -> Option<u64> {
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
pub fn compile_and_call(chunk: &Chunk, nan_consts: &[NanVal], args: &[u64], program: &CompiledProgram) -> Option<u64> {
    with_active_registry(program, || {
        let func = compile(chunk, nan_consts, program)?;
        call(&func, args)
    })
}

#[cfg(test)]
#[allow(clippy::approx_constant)]
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

    // ── num / flr / cel / min / max / rnd ─────────────────────────────────────

    #[test]
    fn cranelift_num_builtin() {
        // num returns R n t; match to extract the inner number
        let result = jit_run(r#"f s:t>n;r=num s;?r{~v:v;^_:0}"#, "f", &[Value::Text("3.14".into())]);
        assert_eq!(result, Some(Value::Number(3.14)));
    }

    #[test]
    fn cranelift_flr_builtin() {
        let result = jit_run("f x:n>n;flr x", "f", &[Value::Number(4.7)]);
        assert_eq!(result, Some(Value::Number(4.0)));
    }

    #[test]
    fn cranelift_cel_builtin() {
        let result = jit_run("f x:n>n;cel x", "f", &[Value::Number(4.1)]);
        assert_eq!(result, Some(Value::Number(5.0)));
    }

    #[test]
    fn cranelift_min_builtin() {
        let result = jit_run("f a:n b:n>n;min a b", "f", &[Value::Number(3.0), Value::Number(7.0)]);
        assert_eq!(result, Some(Value::Number(3.0)));
    }

    #[test]
    fn cranelift_max_builtin() {
        let result = jit_run("f a:n b:n>n;max a b", "f", &[Value::Number(3.0), Value::Number(7.0)]);
        assert_eq!(result, Some(Value::Number(7.0)));
    }

    #[test]
    fn cranelift_rnd0_returns_number() {
        let result = jit_run("f>n;rnd", "f", &[]);
        assert!(matches!(result, Some(Value::Number(_))));
    }

    #[test]
    fn cranelift_rnd2_range_returns_number() {
        // rnd with two args: random integer in [1, 10]
        let result = jit_run("f>n;rnd 1 10", "f", &[]);
        assert!(matches!(result, Some(Value::Number(_))));
    }

    // ── env ───────────────────────────────────────────────────────────────────

    #[test]
    fn cranelift_env_builtin() {
        unsafe { std::env::set_var("ILO_JIT_TEST_VAR", "hello"); }
        let result = jit_run(r#"f k:t>R t t;env k"#, "f", &[Value::Text("ILO_JIT_TEST_VAR".into())]);
        assert_eq!(result, Some(Value::Ok(Box::new(Value::Text("hello".into())))));
    }

    // ── spl / cat ─────────────────────────────────────────────────────────────

    #[test]
    fn cranelift_spl_builtin() {
        let result = jit_run(r#"f s:t sep:t>L t;spl s sep"#, "f", &[
            Value::Text("a,b,c".into()),
            Value::Text(",".into()),
        ]);
        assert_eq!(result, Some(Value::List(vec![
            Value::Text("a".into()),
            Value::Text("b".into()),
            Value::Text("c".into()),
        ])));
    }

    #[test]
    fn cranelift_cat_builtin() {
        let result = jit_run(r#"f xs:L t sep:t>t;cat xs sep"#, "f", &[
            Value::List(vec![Value::Text("x".into()), Value::Text("y".into())]),
            Value::Text("-".into()),
        ]);
        assert_eq!(result, Some(Value::Text("x-y".into())));
    }

    // ── has / hd / tl / rev / srt / slc ──────────────────────────────────────

    #[test]
    fn cranelift_has_list() {
        let result = jit_run("f xs:L n v:n>b;has xs v", "f", &[
            Value::List(vec![Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)]),
            Value::Number(2.0),
        ]);
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_hd_builtin() {
        let result = jit_run("f xs:L n>n;hd xs", "f", &[
            Value::List(vec![Value::Number(10.0), Value::Number(20.0)]),
        ]);
        assert_eq!(result, Some(Value::Number(10.0)));
    }

    #[test]
    fn cranelift_tl_builtin() {
        let result = jit_run("f xs:L n>L n;tl xs", "f", &[
            Value::List(vec![Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)]),
        ]);
        assert_eq!(result, Some(Value::List(vec![Value::Number(2.0), Value::Number(3.0)])));
    }

    #[test]
    fn cranelift_rev_builtin() {
        let result = jit_run("f xs:L n>L n;rev xs", "f", &[
            Value::List(vec![Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)]),
        ]);
        assert_eq!(result, Some(Value::List(vec![Value::Number(3.0), Value::Number(2.0), Value::Number(1.0)])));
    }

    #[test]
    fn cranelift_srt_builtin() {
        let result = jit_run("f xs:L n>L n;srt xs", "f", &[
            Value::List(vec![Value::Number(3.0), Value::Number(1.0), Value::Number(2.0)]),
        ]);
        assert_eq!(result, Some(Value::List(vec![Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)])));
    }

    #[test]
    fn cranelift_slc_builtin() {
        let result = jit_run("f xs:L n a:n b:n>L n;slc xs a b", "f", &[
            Value::List(vec![Value::Number(10.0), Value::Number(20.0), Value::Number(30.0), Value::Number(40.0)]),
            Value::Number(1.0),
            Value::Number(3.0),
        ]);
        assert_eq!(result, Some(Value::List(vec![Value::Number(20.0), Value::Number(30.0)])));
    }

    // ── list append / listnew / index ─────────────────────────────────────────

    #[test]
    fn cranelift_listappend() {
        // +=xs v — append single element (BinOp::Append → OP_LISTAPPEND)
        let result = jit_run("f xs:L n v:n>L n;r=+=xs v;r", "f", &[
            Value::List(vec![Value::Number(1.0), Value::Number(2.0)]),
            Value::Number(3.0),
        ]);
        assert_eq!(result, Some(Value::List(vec![Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)])));
    }

    #[test]
    fn cranelift_listnew() {
        // [a, b] literal — OP_LISTNEW
        let result = jit_run("f a:n b:n>L n;[a, b]", "f", &[Value::Number(5.0), Value::Number(6.0)]);
        assert_eq!(result, Some(Value::List(vec![Value::Number(5.0), Value::Number(6.0)])));
    }

    #[test]
    fn cranelift_index_literal() {
        // xs.0 — literal index 0 → OP_INDEX
        let result = jit_run("f xs:L n>n;xs.0", "f", &[
            Value::List(vec![Value::Number(10.0), Value::Number(20.0), Value::Number(30.0)]),
        ]);
        assert_eq!(result, Some(Value::Number(10.0)));
    }

    // ── records ───────────────────────────────────────────────────────────────

    #[test]
    fn cranelift_recnew_and_field() {
        // record creation: `type pt{x:n;y:n} f a:n b:n>n;p=pt x:a y:b;p.x`
        let src = "type pt{x:n;y:n} f a:n b:n>n;p=pt x:a y:b;p.x";
        let result = jit_run(src, "f", &[Value::Number(3.0), Value::Number(4.0)]);
        assert_eq!(result, Some(Value::Number(3.0)));
    }

    #[test]
    fn cranelift_recwith() {
        // record update: `p with x:99`
        let src = "type pt{x:n;y:n} f a:n b:n>n;p=pt x:a y:b;q=p with x:99;q.x";
        let result = jit_run(src, "f", &[Value::Number(3.0), Value::Number(4.0)]);
        assert_eq!(result, Some(Value::Number(99.0)));
    }

    // ── json ─────────────────────────────────────────────────────────────────

    #[test]
    fn cranelift_jdmp_number() {
        // jdmp serialises numbers without trailing .0 (matches interpreter behaviour)
        let result = jit_run("f x:n>t;jdmp x", "f", &[Value::Number(42.0)]);
        assert_eq!(result, Some(Value::Text("42".into())));
    }

    #[test]
    fn cranelift_jpar_ok() {
        let result = jit_run(r#"f s:t>R t t;jpar s"#, "f", &[Value::Text(r#"{"k":"v"}"#.into())]);
        assert!(matches!(result, Some(Value::Ok(_))));
    }

    #[test]
    fn cranelift_jpth_ok() {
        let result = jit_run(r#"f j:t p:t>R t t;jpth j p"#, "f", &[
            Value::Text(r#"{"name":"alice"}"#.into()),
            Value::Text("name".into()),
        ]);
        assert_eq!(result, Some(Value::Ok(Box::new(Value::Text("alice".into())))));
    }

    // ── isok / iserr / unwrap — via match patterns ────────────────────────────

    #[test]
    fn cranelift_isok_via_match() {
        // match pattern Ok → OP_ISOK emitted in JIT
        let result = jit_run("f x:R n t>n;?x{~v:v;^_:0}", "f", &[Value::Ok(Box::new(Value::Number(7.0)))]);
        assert_eq!(result, Some(Value::Number(7.0)));
    }

    #[test]
    fn cranelift_iserr_via_match() {
        // match pattern Err → OP_ISERR emitted in JIT
        let result = jit_run(r#"f x:R n t>n;?x{~_:1;^_:99}"#, "f", &[Value::Err(Box::new(Value::Text("bad".into())))]);
        assert_eq!(result, Some(Value::Number(99.0)));
    }

    #[test]
    fn cranelift_unwrap_via_match() {
        // OP_UNWRAP emitted in Ok match arm — extracts the inner value
        let src = "f x:R n t>n;?x{~v:v;^_:0}";
        let result = jit_run(src, "f", &[Value::Ok(Box::new(Value::Number(42.0)))]);
        assert_eq!(result, Some(Value::Number(42.0)));
    }

    // ── now ───────────────────────────────────────────────────────────────────

    #[test]
    fn cranelift_now_returns_number() {
        let result = jit_run("f>n;now", "f", &[]);
        assert!(matches!(result, Some(Value::Number(_))));
    }

    // ── OP_JMPNN — nil coalesce (lines 652-662) ───────────────────────────────

    #[test]
    fn cranelift_nil_coalesce_with_value() {
        // x??42 — x is not nil, so returns x  (OP_JMPNN: jump if NOT nil)
        let result = jit_run("f x:O n>n;x??42", "f", &[Value::Number(7.0)]);
        assert_eq!(result, Some(Value::Number(7.0)));
    }

    #[test]
    fn cranelift_nil_coalesce_with_nil() {
        // x??42 — x is nil, so returns 42
        let result = jit_run("f x:O n>n;x??42", "f", &[Value::Nil]);
        assert_eq!(result, Some(Value::Number(42.0)));
    }

    // ── OP_LISTNEW n==0 — empty list literal (lines 1054-1061) ───────────────

    #[test]
    fn cranelift_empty_list_literal() {
        // [] compiles to OP_LISTNEW with n=0 — exercises the empty-list JIT path
        let result = jit_run("f>L n;[]", "f", &[]);
        assert_eq!(result, Some(Value::List(vec![])));
    }

    // ── jit_run_numeric _ => None (line 1300) ────────────────────────────────

    #[test]
    fn jit_run_numeric_non_number_returns_none() {
        // jit_run returns Some(Bool) but jit_run_numeric returns None for non-Number
        let result = jit_run_numeric("f>b;true", "f", &[]);
        assert_eq!(result, None);
    }

    // ── OP_RECFLD_NAME — JIT bails out returning None (line 913) ─────────────

    #[test]
    fn cranelift_recfld_name_works() {
        // OP_RECFLD_NAME is now supported in JIT via jit_recfld_name helper.
        // Must use parse() with spans (not parse_tokens) so jpar! adjacency check works.
        let source = r#"f x:t>R t t;r=jpar! x;r.score"#;
        let tokens: Vec<(crate::lexer::Token, crate::ast::Span)> = lexer::lex(source)
            .unwrap().into_iter()
            .map(|(t, r)| (t, crate::ast::Span { start: r.start, end: r.end }))
            .collect();
        let (prog, errors) = parser::parse(tokens);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let nan_args: Vec<u64> = [Value::Text(r#"{"score":42}"#.to_string())]
            .iter().map(|v| NanVal::from_value(v).0).collect();
        let result = compile_and_call(chunk, nan_consts, &nan_args, &compiled);
        assert!(result.is_some(), "JIT should handle OP_RECFLD_NAME");
        let val = NanVal(result.unwrap()).to_value();
        match val {
            Value::Number(n) => assert_eq!(n, 42.0),
            other => panic!("expected Number(42), got {:?}", other),
        }
    }

    // ── !block_terminated at function end (lines 1184-1185) ──────────────────

    #[test]
    fn cranelift_function_ends_without_explicit_terminator() {
        // A function whose last block doesn't have an explicit return
        // — the JIT inserts return TAG_NIL (lines 1184-1185)
        // A function with only a while loop that may break early fits this pattern
        let result = jit_run("f x:n>n;wh > x 0{x=-x};x", "f", &[Value::Number(5.0)]);
        // -5 < 0 so loop ends, returns -5
        assert_eq!(result, Some(Value::Number(-5.0)));
    }

    // ── OP_ROU — round builtin (JIT L730-735) ───────────────────────────────

    #[test]
    fn cranelift_rou_builtin() {
        let result = jit_run("f x:n>n;rou x", "f", &[Value::Number(4.5)]);
        assert_eq!(result, Some(Value::Number(5.0)));
    }

    #[test]
    fn cranelift_rou_down() {
        let result = jit_run("f x:n>n;rou x", "f", &[Value::Number(4.4)]);
        assert_eq!(result, Some(Value::Number(4.0)));
    }

    // ── OP_CALL with zero args (JIT L1153-1160) ─────────────────────────────

    #[test]
    fn cranelift_call_zero_args_injected() {
        // Inject OP_CALL with n_args=0 directly into bytecode to exercise the
        // JIT zero-args OP_CALL path (L1153-1160). This path is hard to reach
        // from normal source since bare identifiers parse as variable refs.
        use crate::vm::{compile as vm_compile, OP_CALL, OP_RET};
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex(
            "g>n;42\nf>n;42"
        ).unwrap().into_iter().map(|(t, _)| t).collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let mut compiled = vm_compile(&prog).unwrap();
        let f_idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let g_idx = compiled.func_names.iter().position(|n| n == "g").unwrap();
        // Replace f's code: OP_CALL r0 = g() [func_idx=g_idx, n_args=0], OP_RET r0
        let call_inst = (OP_CALL as u32) << 24 | ((g_idx as u32) << 8);
        let ret_inst = (OP_RET as u32) << 24;
        compiled.chunks[f_idx].code = vec![call_inst, ret_inst];
        let chunk = &compiled.chunks[f_idx];
        let nan_consts = &compiled.nan_constants[f_idx];
        // The JIT should compile this without panicking.
        let func = compile(chunk, nan_consts, &compiled);
        // If it compiled, try calling it (should call g() and return 42).
        if let Some(f) = func {
            let result = call(&f, &[]);
            assert_eq!(result, Some(NanVal::number(42.0).0));
        }
    }

    // ── JIT record return — arena promotion in call() (JIT L1278-1281) ──────

    #[test]
    fn cranelift_record_return_promotes_arena() {
        // Return a record from JIT — exercises the arena record promotion
        // in call() at L1277-1281.
        let src = "type pt{x:n;y:n} f a:n b:n>pt;pt x:a y:b";
        let result = jit_run(src, "f", &[Value::Number(10.0), Value::Number(20.0)]);
        match result {
            Some(Value::Record { type_name, fields }) => {
                assert_eq!(type_name, "pt");
                assert_eq!(fields.get("x"), Some(&Value::Number(10.0)));
                assert_eq!(fields.get("y"), Some(&Value::Number(20.0)));
            }
            other => panic!("expected Record, got {:?}", other),
        }
    }

    // ── OP_RECWITH in JIT (L1037-1039) ──────────────────────────────────────

    #[test]
    fn cranelift_recwith_update() {
        // Record update via JIT — exercises the JIT recwith path
        let src = "type pt{x:n;y:n} f>pt;p=pt x:1 y:2;p with x:99";
        let result = jit_run(src, "f", &[]);
        match result {
            Some(Value::Record { type_name, fields }) => {
                assert_eq!(type_name, "pt");
                assert_eq!(fields.get("x"), Some(&Value::Number(99.0)));
                assert_eq!(fields.get("y"), Some(&Value::Number(2.0)));
            }
            other => panic!("expected Record, got {:?}", other),
        }
    }

    // ── OP_LISTGET inline in JIT ─────────────────────────────────────────────

    #[test]
    fn cranelift_foreach_loop() {
        // Numeric foreach: exercises inline OP_LISTGET fast path (no RC clone).
        let result = jit_run("f xs:L n>n;s=0;@x xs{s=+s x};s", "f", &[
            Value::List(vec![Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)]),
        ]);
        assert_eq!(result, Some(Value::Number(6.0)));
    }

    #[test]
    fn cranelift_foreach_loop_heap_elements() {
        // Foreach over strings: exercises the RC clone (elem_is_heap) branch
        // in the inline OP_LISTGET fast path.
        let result = jit_run("f xs:L n>n;s=0;@x xs{s=+s 1};s", "f", &[
            Value::List(vec![
                Value::Text("a".to_string()),
                Value::Text("b".to_string()),
                Value::Text("c".to_string()),
            ]),
        ]);
        assert_eq!(result, Some(Value::Number(3.0)));
    }

    #[test]
    fn cranelift_foreach_empty_list() {
        // Foreach over empty list: bounds check fails immediately, sum stays 0.
        let result = jit_run("f xs:L n>n;s=0;@x xs{s=+s x};s", "f", &[
            Value::List(vec![]),
        ]);
        assert_eq!(result, Some(Value::Number(0.0)));
    }

    #[test]
    fn cranelift_sequential_cross_function_calls() {
        // Two sequential calls: a=dbl(n), then triple(a)
        let result = jit_run("dbl x:n>n;*x 2\ntriple x:n>n;*x 3\nf n:n>n;a=dbl n;triple a", "f", &[
            Value::Number(5.0),
        ]);
        assert_eq!(result, Some(Value::Number(30.0)));
    }

    #[test]
    fn cranelift_pipe_chain() {
        // Pipe chain: inc(dbl(inc(dbl(5)))) = (5*2+1)*2+1 = 23
        let result = jit_run(
            "dbl x:n>n;*x 2\ninc x:n>n;+x 1\nf n:n>n;n>>dbl>>inc>>dbl>>inc", "f", &[
            Value::Number(5.0),
        ]);
        assert_eq!(result, Some(Value::Number(23.0)));
    }
}
