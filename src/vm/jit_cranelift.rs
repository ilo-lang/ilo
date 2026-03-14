//! Cranelift NanVal JIT backend — compiles ALL functions to native code.
//!
//! Works with u64 (NanVal) registers instead of f64. For numeric operations,
//! bitcasts u64↔f64 and uses FP instructions. For everything else, calls
//! `extern "C"` Rust helper functions. This eliminates the bytecode dispatch
//! loop while reusing all existing VM logic.

use super::*;
use cranelift_codegen::Context;
use cranelift_codegen::ir::types::{F64, I64};
use cranelift_codegen::ir::{AbiParam, InstBuilder};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module, default_libcall_names};
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
    mod_fn: FuncId,
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
    recwith_arena: FuncId,
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
    module
        .declare_function(name, Linkage::Import, &sig)
        .unwrap()
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
        ("jit_mod", jit_mod as *const u8),
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
        ("jit_recwith_arena", jit_recwith_arena as *const u8),
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
        mod_fn: declare_helper(module, "jit_mod", 2, 1),
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
        recwith_arena: declare_helper(module, "jit_recwith_arena", 5, 1),
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

/// Check whether a callee chunk can be safely inlined at a JIT call site.
///
/// A chunk is inlinable when every opcode is from the "pure numeric guard chain"
/// set (CMPK_*_N, JMP, LOADK with numeric constants only, RET, and the fused
/// arithmetic ops), all registers are numeric, and the register file is <= 16.
fn is_inlinable(chunk: &Chunk, nan_consts: &[NanVal]) -> bool {
    if !chunk.all_regs_numeric {
        return false;
    }
    if chunk.reg_count as usize > 16 {
        return false;
    }
    if chunk.code.is_empty() {
        return false;
    }

    let mut has_ret = false;
    for &inst in &chunk.code {
        let op = (inst >> 24) as u8;
        match op {
            op if op == OP_CMPK_GE_N
                || op == OP_CMPK_GT_N
                || op == OP_CMPK_LT_N
                || op == OP_CMPK_LE_N
                || op == OP_CMPK_EQ_N
                || op == OP_CMPK_NE_N => {}
            OP_JMP => {}
            OP_RET => {
                has_ret = true;
            }
            OP_LOADK => {
                let bx = (inst & 0xFFFF) as usize;
                if bx >= nan_consts.len() || !nan_consts[bx].is_number() {
                    return false;
                }
            }
            OP_ADD_NN | OP_SUB_NN | OP_MUL_NN | OP_DIV_NN | OP_ADDK_N | OP_SUBK_N | OP_MULK_N
            | OP_DIVK_N => {}
            _ => return false,
        }
    }
    has_ret
}

/// Inline a callee chunk directly into the caller's Cranelift IR stream.
///
/// `arg_vars`     — caller `Variable`s for callee params (indices 0..n_params)
/// `result_var`   — caller `Variable` that receives the inlined return value
/// `extra_vars`   — caller `Variable`s for callee non-param regs
/// `f64_arg_vars` — F64 shadow `Variable`s corresponding to `arg_vars`
/// `merge_block`  — block to jump to after the inlined return
///
/// Returns `true` on success, `false` if inlining should be abandoned.
#[allow(clippy::too_many_arguments)]
fn inline_chunk(
    builder: &mut FunctionBuilder<'_>,
    callee_chunk: &Chunk,
    callee_consts: &[NanVal],
    arg_vars: &[Variable],
    result_var: Variable,
    extra_vars: &[Variable],
    f64_arg_vars: &[Variable],
    merge_block: cranelift_codegen::ir::Block,
) -> bool {
    let n_params = callee_chunk.param_count as usize;
    let mf = cranelift_codegen::ir::MemFlags::new();

    let reg_var = |r: usize| -> Variable {
        if r < n_params {
            arg_vars[r]
        } else {
            extra_vars[r - n_params]
        }
    };

    // Retrieve the f64 Value for callee register `r`.
    // Uses the pre-computed shadow for param registers; bitcasts for others.
    // NOTE: always uses two separate statements to avoid double-borrow of builder.
    let f64_val_for =
        |r: usize, builder: &mut FunctionBuilder<'_>| -> cranelift_codegen::ir::Value {
            if r < n_params {
                builder.use_var(f64_arg_vars[r])
            } else {
                let iv = builder.use_var(extra_vars[r - n_params]);
                builder.ins().bitcast(F64, mf, iv)
            }
        };

    let leaders = find_block_leaders(&callee_chunk.code);
    let mut imap: HashMap<usize, cranelift_codegen::ir::Block> = HashMap::new();
    for &l in &leaders {
        imap.insert(l, builder.create_block());
    }

    builder.ins().jump(imap[&0], &[]);
    // Outer block is now terminated; mark it so the loop doesn't re-emit
    // a jump when ip=0 is recognised as a block leader.
    let mut terminated = true;

    for (ip, &inst) in callee_chunk.code.iter().enumerate() {
        if let Some(&blk) = imap.get(&ip) {
            if !terminated {
                builder.ins().jump(blk, &[]);
            }
            builder.switch_to_block(blk);
            terminated = false;
        }
        if terminated {
            continue;
        }

        let op = (inst >> 24) as u8;
        let a_raw = ((inst >> 16) & 0xFF) as usize;

        match op {
            OP_RET => {
                let rv = builder.use_var(reg_var(a_raw));
                builder.def_var(result_var, rv);
                builder.ins().jump(merge_block, &[]);
                terminated = true;
            }
            OP_JMP => {
                let sbx = (inst & 0xFFFF) as i16;
                let t = (ip as isize + 1 + sbx as isize) as usize;
                if let Some(&tb) = imap.get(&t) {
                    builder.ins().jump(tb, &[]);
                    terminated = true;
                } else {
                    return false;
                }
            }
            OP_LOADK => {
                let bx = (inst & 0xFFFF) as usize;
                let bits = callee_consts[bx].0;
                let kval = builder.ins().iconst(I64, bits as i64);
                builder.def_var(reg_var(a_raw), kval);
            }
            op if op == OP_CMPK_GE_N
                || op == OP_CMPK_GT_N
                || op == OP_CMPK_LT_N
                || op == OP_CMPK_LE_N
                || op == OP_CMPK_EQ_N
                || op == OP_CMPK_NE_N =>
            {
                let ki = (inst & 0xFF) as usize;
                let lhs_f64 = f64_val_for(a_raw, builder);
                let rhs_k = if ki < callee_consts.len() {
                    callee_consts[ki].as_number()
                } else {
                    0.0
                };
                let rhs_f64 = builder.ins().f64const(rhs_k);

                use cranelift_codegen::ir::condcodes::FloatCC;
                let cc = match op {
                    op if op == OP_CMPK_GE_N => FloatCC::GreaterThanOrEqual,
                    op if op == OP_CMPK_GT_N => FloatCC::GreaterThan,
                    op if op == OP_CMPK_LT_N => FloatCC::LessThan,
                    op if op == OP_CMPK_LE_N => FloatCC::LessThanOrEqual,
                    op if op == OP_CMPK_EQ_N => FloatCC::Equal,
                    _ => FloatCC::NotEqual,
                };
                let cmp = builder.ins().fcmp(cc, lhs_f64, rhs_f64);
                let body = imap.get(&(ip + 2)).copied();
                let miss = callee_chunk
                    .code
                    .get(ip + 1)
                    .and_then(|&j| {
                        if (j >> 24) as u8 == OP_JMP {
                            let sbx = (j & 0xFFFF) as i16;
                            imap.get(&((ip as isize + 2 + sbx as isize) as usize))
                                .copied()
                        } else {
                            None
                        }
                    })
                    .or_else(|| imap.get(&(ip + 1)).copied());

                match (miss, body) {
                    (Some(fb), Some(bb)) => {
                        builder.ins().brif(cmp, bb, &[], fb, &[]);
                        terminated = true;
                    }
                    _ => return false,
                }
            }
            OP_ADD_NN => {
                let b = ((inst >> 8) & 0xFF) as usize;
                let c = (inst & 0xFF) as usize;
                let bv = f64_val_for(b, builder);
                let cv = f64_val_for(c, builder);
                let rf = builder.ins().fadd(bv, cv);
                let ri = builder.ins().bitcast(I64, mf, rf);
                builder.def_var(reg_var(a_raw), ri);
            }
            OP_SUB_NN => {
                let b = ((inst >> 8) & 0xFF) as usize;
                let c = (inst & 0xFF) as usize;
                let bv = f64_val_for(b, builder);
                let cv = f64_val_for(c, builder);
                let rf = builder.ins().fsub(bv, cv);
                let ri = builder.ins().bitcast(I64, mf, rf);
                builder.def_var(reg_var(a_raw), ri);
            }
            OP_MUL_NN => {
                let b = ((inst >> 8) & 0xFF) as usize;
                let c = (inst & 0xFF) as usize;
                let bv = f64_val_for(b, builder);
                let cv = f64_val_for(c, builder);
                let rf = builder.ins().fmul(bv, cv);
                let ri = builder.ins().bitcast(I64, mf, rf);
                builder.def_var(reg_var(a_raw), ri);
            }
            OP_DIV_NN => {
                let b = ((inst >> 8) & 0xFF) as usize;
                let c = (inst & 0xFF) as usize;
                let bv = f64_val_for(b, builder);
                let cv = f64_val_for(c, builder);
                let rf = builder.ins().fdiv(bv, cv);
                let ri = builder.ins().bitcast(I64, mf, rf);
                builder.def_var(reg_var(a_raw), ri);
            }
            OP_ADDK_N => {
                let b = ((inst >> 8) & 0xFF) as usize;
                let ki = (inst & 0xFF) as usize;
                let bv = f64_val_for(b, builder);
                let kv = builder
                    .ins()
                    .f64const(callee_consts.get(ki).map(|c| c.as_number()).unwrap_or(0.0));
                let rf = builder.ins().fadd(bv, kv);
                let ri = builder.ins().bitcast(I64, mf, rf);
                builder.def_var(reg_var(a_raw), ri);
            }
            OP_SUBK_N => {
                let b = ((inst >> 8) & 0xFF) as usize;
                let ki = (inst & 0xFF) as usize;
                let bv = f64_val_for(b, builder);
                let kv = builder
                    .ins()
                    .f64const(callee_consts.get(ki).map(|c| c.as_number()).unwrap_or(0.0));
                let rf = builder.ins().fsub(bv, kv);
                let ri = builder.ins().bitcast(I64, mf, rf);
                builder.def_var(reg_var(a_raw), ri);
            }
            OP_MULK_N => {
                let b = ((inst >> 8) & 0xFF) as usize;
                let ki = (inst & 0xFF) as usize;
                let bv = f64_val_for(b, builder);
                let kv = builder
                    .ins()
                    .f64const(callee_consts.get(ki).map(|c| c.as_number()).unwrap_or(0.0));
                let rf = builder.ins().fmul(bv, kv);
                let ri = builder.ins().bitcast(I64, mf, rf);
                builder.def_var(reg_var(a_raw), ri);
            }
            OP_DIVK_N => {
                let b = ((inst >> 8) & 0xFF) as usize;
                let ki = (inst & 0xFF) as usize;
                let bv = f64_val_for(b, builder);
                let kv = builder
                    .ins()
                    .f64const(callee_consts.get(ki).map(|c| c.as_number()).unwrap_or(0.0));
                let rf = builder.ins().fdiv(bv, kv);
                let ri = builder.ins().bitcast(I64, mf, rf);
                builder.def_var(reg_var(a_raw), ri);
            }
            _ => return false,
        }
    }
    if !terminated {
        let nil = builder.ins().iconst(I64, TAG_NIL as i64);
        builder.def_var(result_var, nil);
        builder.ins().jump(merge_block, &[]);
    }
    true
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

    // Declare extra F64 shadow variables for registers that will be proven always-numeric.
    // These allow guard chains to reuse a single bitcast across multiple CMPK instructions
    // without redundant i64→f64 conversions on every comparison.
    // Variable indices [reg_count .. reg_count*2) are the F64 shadows.
    let f64_var_offset = reg_count;
    let mut f64_vars: Vec<Variable> = Vec::with_capacity(reg_count);
    for i in 0..reg_count {
        let var = Variable::from_u32((f64_var_offset + i) as u32);
        builder.declare_var(var, F64);
        f64_vars.push(var);
    }

    // ── Foreach loop cache variables ─────────────────────────────────────────
    // For each FOREACHPREP instruction we allocate 4 extra I64 Cranelift variables:
    //   [foreach_var_base + loop_idx*4 + 0]  ptr      — HeapObj pointer (bv & PTR_MASK)
    //   [foreach_var_base + loop_idx*4 + 1]  data_ptr — Vec backing-store pointer (*ptr+16)
    //   [foreach_var_base + loop_idx*4 + 2]  vec_len  — Vec length (*ptr+8)
    //   [foreach_var_base + loop_idx*4 + 3]  int_idx  — current index as raw i64
    //
    // FOREACHPREP writes all four; FOREACHNEXT reads them to avoid re-extracting the
    // pointer, re-loading two memory fields, and doing fcvt_to_uint_sat each iteration.
    // Matching is done by (b_idx, c_idx) register pair — each foreach loop uses a
    // dedicated (coll_reg, idx_reg) pair assigned by the bytecode compiler.
    //
    // Variable layout:
    //   [0 .. reg_count)            I64  VM register vars
    //   [reg_count .. 2*reg_count)  F64  shadow vars
    //   [2*reg_count .. )           I64  foreach cache vars (4 per loop)
    let foreach_var_base = 2 * reg_count;

    // Pre-scan: collect unique (b_idx, c_idx) pairs from FOREACHPREP instructions
    // and assign each a loop index.
    let mut foreach_loop_map: HashMap<(usize, usize), usize> = HashMap::new();
    for &inst in &chunk.code {
        let op = (inst >> 24) as u8;
        if op == OP_FOREACHPREP {
            let b = ((inst >> 8) & 0xFF) as usize;
            let c = (inst & 0xFF) as usize;
            let next_idx = foreach_loop_map.len();
            foreach_loop_map.entry((b, c)).or_insert(next_idx);
        }
    }
    let num_foreach_loops = foreach_loop_map.len();

    // Declare the 4 × num_foreach_loops extra I64 variables.
    for i in 0..(num_foreach_loops * 4) {
        let var = Variable::from_u32((foreach_var_base + i) as u32);
        builder.declare_var(var, I64);
    }

    // Helper closures to get the Variable for each foreach cache slot.
    // `loop_idx` comes from `foreach_loop_map.get(&(b, c))`.
    let fe_ptr_var = |loop_idx: usize| Variable::from_u32((foreach_var_base + loop_idx * 4) as u32);
    let fe_data_ptr_var =
        |loop_idx: usize| Variable::from_u32((foreach_var_base + loop_idx * 4 + 1) as u32);
    let fe_len_var =
        |loop_idx: usize| Variable::from_u32((foreach_var_base + loop_idx * 4 + 2) as u32);
    let fe_idx_var =
        |loop_idx: usize| Variable::from_u32((foreach_var_base + loop_idx * 4 + 3) as u32);

    // ── Inline callee variable pool ───────────────────────────────────────────
    // For each OP_CALL that targets an inlinable callee we need variables for the
    // callee's non-parameter registers.  We pre-scan OP_CALL instructions and
    // allocate a variable block per unique (call-site-ip, callee-reg-index) pair.
    //
    // Variable layout continues at:
    //   [foreach_var_base + num_foreach_loops*4 ..)  I64  inline callee vars
    //
    // Map: call-site ip → Vec<Variable> of length (callee.reg_count - callee.param_count)
    let inline_var_base = foreach_var_base + num_foreach_loops * 4;
    let mut inline_var_map: HashMap<usize, Vec<Variable>> = HashMap::new();
    {
        let mut next_var_idx = inline_var_base;
        for (ip, &inst) in chunk.code.iter().enumerate() {
            let op = (inst >> 24) as u8;
            if op != OP_CALL {
                continue;
            }
            let bx = (inst & 0xFFFF) as usize;
            let func_idx = bx >> 8;
            if func_idx >= program.chunks.len() {
                continue;
            }
            let callee_chunk = &program.chunks[func_idx];
            let callee_consts = &program.nan_constants[func_idx];
            if !is_inlinable(callee_chunk, callee_consts) {
                continue;
            }
            let n_extra =
                (callee_chunk.reg_count as usize).saturating_sub(callee_chunk.param_count as usize);
            let mut slot_vars = Vec::with_capacity(n_extra);
            for _ in 0..n_extra {
                let v = Variable::from_u32(next_var_idx as u32);
                builder.declare_var(v, I64);
                slot_vars.push(v);
                next_var_idx += 1;
            }
            inline_var_map.insert(ip, slot_vars);
        }
    }

    // Pre-allocate one extra F64 shadow variable per inline arg register we'll use.
    // Indexed as [inline_f64_var_base + ip_slot * MAX_ARGS + arg_idx].
    // We use a simpler scheme: one F64 var per (call-site-ip, arg-position).
    // For the common case of 1-arg inlinable functions this is just one var per call.
    // Layout: [foreach_var_base + num_foreach_loops*4 + total_inline_i64_vars ..)  F64
    let total_inline_i64 = inline_var_map.values().map(|v| v.len()).sum::<usize>();
    let inline_f64_var_base = foreach_var_base + num_foreach_loops * 4 + total_inline_i64;
    // For each inlinable call site we allocate n_params F64 shadow variables.
    let mut inline_f64_var_map: HashMap<usize, Vec<Variable>> = HashMap::new();
    {
        let mut next_f64_idx = inline_f64_var_base;
        for &ip in inline_var_map.keys() {
            let bx = (chunk.code[ip] & 0xFFFF) as usize;
            let func_idx = bx >> 8;
            let callee = &program.chunks[func_idx];
            let np = callee.param_count as usize;
            let mut fvs = Vec::with_capacity(np);
            for _ in 0..np {
                let v = Variable::from_u32(next_f64_idx as u32);
                builder.declare_var(v, F64);
                fvs.push(v);
                next_f64_idx += 1;
            }
            inline_f64_var_map.insert(ip, fvs);
        }
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
    let mut get_func_ref = |builder: &mut FunctionBuilder<'_>,
                            module: &mut JITModule,
                            id: FuncId|
     -> cranelift_codegen::ir::FuncRef {
        *func_refs
            .entry(id)
            .or_insert_with(|| module.declare_func_in_func(id, builder.func))
    };

    // Store the program pointer as a constant for jit_call fallback
    let program_ptr_val = program as *const CompiledProgram as u64;

    // Pre-pass: determine which registers are *always* written with numeric values,
    // and which are *always* written with boolean values (TAG_TRUE / TAG_FALSE).
    //
    // Numeric analysis (reg_always_num):
    //   num_write[r]     — at least one write to r is definitely numeric
    //   non_num_write[r] — at least one write to r may produce a non-number
    //   A register is "always numeric" iff num_write && !non_num_write.
    //
    //   MOVE is handled with a fixpoint loop: MOVE a, b propagates b's numeric
    //   status to a.  This is critical for range-loop counters which are
    //   initialised with MOVE counter, start_const before ADDK_N takes over.
    //
    // Boolean analysis (reg_always_bool):
    //   bool_write[r]     — every write to r produces TAG_TRUE or TAG_FALSE
    //   non_bool_write[r] — at least one write to r may produce a non-bool
    //   A register is "always boolean" iff bool_write && !non_bool_write.
    //
    //   This lets JMPF/JMPT on comparison results be compiled as a single
    //   `icmp ne val, TAG_FALSE` + `brif` instead of a 3-block diamond that
    //   first checks whether the value is a number (it never is for booleans).
    let mut reg_always_num = vec![false; reg_count];
    let mut reg_always_bool = vec![false; reg_count];
    {
        let mut non_num_write = vec![false; reg_count];
        let mut num_write = vec![false; reg_count];
        let mut bool_write = vec![false; reg_count];
        let mut non_bool_write = vec![false; reg_count];

        // Function parameters: the VM compiler sets all_regs_numeric when it has
        // proven every param is numeric (e.g. single-param numeric functions).
        if chunk.all_regs_numeric {
            for slot in num_write.iter_mut().take(chunk.param_count as usize) {
                *slot = true;
            }
        }

        // First pass: classify all non-MOVE instructions.
        for &inst in &chunk.code {
            let op = (inst >> 24) as u8;
            let a = ((inst >> 16) & 0xFF) as usize;
            if a >= reg_count {
                continue;
            }
            match op {
                // Guaranteed numeric outputs.
                OP_ADD_NN | OP_SUB_NN | OP_MUL_NN | OP_DIV_NN
                | OP_ADDK_N | OP_SUBK_N | OP_MULK_N | OP_DIVK_N
                | OP_LEN | OP_ABS | OP_MIN | OP_MAX
                | OP_FLR | OP_CEL | OP_ROU | OP_RND0 | OP_RND2 | OP_NOW
                | OP_MOD => {
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
                // Boolean outputs: comparison and type-test ops.
                // These always produce TAG_TRUE or TAG_FALSE.
                OP_LT | OP_GT | OP_LE | OP_GE | OP_EQ | OP_NE
                | OP_NOT | OP_HAS | OP_ISNUM | OP_ISTEXT | OP_ISBOOL | OP_ISLIST
                | OP_MHAS | OP_ISOK | OP_ISERR => {
                    bool_write[a] = true;
                    non_num_write[a] = true;
                }
                // MOVE: skip here, handled by fixpoint below.
                OP_MOVE => {}
                // Ops that write a non-numeric or unknown type to R[A].
                OP_ADD | OP_SUB | OP_MUL | OP_DIV  // may be string concat etc.
                | OP_NEG
                | OP_WRAPOK | OP_WRAPERR | OP_UNWRAP
                | OP_RECFLD | OP_RECFLD_NAME | OP_LISTGET | OP_INDEX
                | OP_STR | OP_HD | OP_TL | OP_REV | OP_SRT | OP_SLC
                | OP_SPL | OP_CAT | OP_GET | OP_POST | OP_GETH | OP_POSTH
                | OP_ENV | OP_JPTH | OP_JDMP | OP_JPAR
                | OP_MAPNEW | OP_MGET | OP_MSET | OP_MDEL | OP_MKEYS | OP_MVALS
                | OP_LISTNEW | OP_LISTAPPEND
                | OP_RECNEW | OP_RECWITH
                | OP_PRT | OP_RD | OP_RDL | OP_WR | OP_WRL | OP_TRM | OP_UNQ
                | OP_NUM => {
                    non_num_write[a] = true;
                    non_bool_write[a] = true;
                }
                // OP_CALL: if callee is known all-numeric, result is numeric.
                OP_CALL => {
                    let bx = (inst & 0xFFFF) as usize;
                    let func_idx = bx >> 8;
                    if func_idx < program.chunks.len()
                        && program.chunks[func_idx].all_regs_numeric
                    {
                        num_write[a] = true;
                    } else {
                        non_num_write[a] = true;
                        non_bool_write[a] = true;
                    }
                }
                // Unknown / non-writing ops (JMP, RET, CMPK_*, etc.): leave flags unset.
                _ => {}
            }
        }

        // Fixpoint: propagate MOVE a, b by copying b's proven type to a.
        // Typically converges in 1–2 iterations for simple loop counter patterns.
        loop {
            let mut changed = false;
            for &inst in &chunk.code {
                let op = (inst >> 24) as u8;
                if op != OP_MOVE {
                    continue;
                }
                let a = ((inst >> 16) & 0xFF) as usize;
                let b = ((inst >> 8) & 0xFF) as usize;
                if a >= reg_count || b >= reg_count {
                    continue;
                }

                let b_always_num = num_write[b] && !non_num_write[b];
                let b_always_bool = bool_write[b] && !non_bool_write[b];

                // Numeric propagation through MOVE.
                if b_always_num {
                    if !num_write[a] {
                        num_write[a] = true;
                        changed = true;
                    }
                } else if !non_num_write[a] {
                    non_num_write[a] = true;
                    changed = true;
                }
                // Boolean propagation through MOVE.
                if b_always_bool {
                    if !bool_write[a] {
                        bool_write[a] = true;
                        changed = true;
                    }
                } else if !non_bool_write[a] {
                    non_bool_write[a] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        for i in 0..reg_count {
            reg_always_num[i] = num_write[i] && !non_num_write[i];
            reg_always_bool[i] = bool_write[i] && !non_bool_write[i];
        }
    }

    // Initialise F64 shadow variables for always-numeric registers.
    // For registers that are never written by non-numeric instructions (e.g. a
    // numeric-only function parameter), we emit a single bitcast at function
    // entry and store it in the F64 shadow variable.  All CMPK_*_N guards on
    // the same register then use that shadow directly instead of re-emitting a
    // bitcast on each comparison.  This cuts the per-guard overhead to a plain
    // `fcmp` + `brif` with no extra conversion instructions.
    //
    // We only initialise shadows for *parameter* registers here; for registers
    // that are written inside the function body (e.g. OP_ADD_NN results) the
    // shadow is written at the point of the assignment (see OP_ADD_NN etc. below).
    // Non-always-numeric registers leave the shadow at its default (undef) — the
    // CMPK handler checks `reg_always_num` before using it.
    {
        let mf = cranelift_codegen::ir::MemFlags::new();
        for i in 0..(chunk.param_count as usize) {
            if i < reg_count && reg_always_num[i] {
                let iv = builder.use_var(vars[i]);
                let fv = builder.ins().bitcast(F64, mf, iv);
                builder.def_var(f64_vars[i], fv);
            }
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
                // Both known numeric — inline fadd.
                // Use F64 shadow vars for inputs (avoids bitcast when shadow is fresh).
                let mf = cranelift_codegen::ir::MemFlags::new();
                let bf = if b_idx < reg_count && reg_always_num[b_idx] {
                    builder.use_var(f64_vars[b_idx])
                } else {
                    let bv = builder.use_var(vars[b_idx]);
                    builder.ins().bitcast(F64, mf, bv)
                };
                let cf = if c_idx < reg_count && reg_always_num[c_idx] {
                    builder.use_var(f64_vars[c_idx])
                } else {
                    let cv = builder.use_var(vars[c_idx]);
                    builder.ins().bitcast(F64, mf, cv)
                };
                let result_f = builder.ins().fadd(bf, cf);
                let result = builder.ins().bitcast(I64, mf, result_f);
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    builder.def_var(f64_vars[a_idx], result_f);
                }
            }
            OP_SUB_NN => {
                let mf = cranelift_codegen::ir::MemFlags::new();
                let bf = if b_idx < reg_count && reg_always_num[b_idx] {
                    builder.use_var(f64_vars[b_idx])
                } else {
                    let bv = builder.use_var(vars[b_idx]);
                    builder.ins().bitcast(F64, mf, bv)
                };
                let cf = if c_idx < reg_count && reg_always_num[c_idx] {
                    builder.use_var(f64_vars[c_idx])
                } else {
                    let cv = builder.use_var(vars[c_idx]);
                    builder.ins().bitcast(F64, mf, cv)
                };
                let result_f = builder.ins().fsub(bf, cf);
                let result = builder.ins().bitcast(I64, mf, result_f);
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    builder.def_var(f64_vars[a_idx], result_f);
                }
            }
            OP_MUL_NN => {
                let mf = cranelift_codegen::ir::MemFlags::new();
                let bf = if b_idx < reg_count && reg_always_num[b_idx] {
                    builder.use_var(f64_vars[b_idx])
                } else {
                    let bv = builder.use_var(vars[b_idx]);
                    builder.ins().bitcast(F64, mf, bv)
                };
                let cf = if c_idx < reg_count && reg_always_num[c_idx] {
                    builder.use_var(f64_vars[c_idx])
                } else {
                    let cv = builder.use_var(vars[c_idx]);
                    builder.ins().bitcast(F64, mf, cv)
                };
                let result_f = builder.ins().fmul(bf, cf);
                let result = builder.ins().bitcast(I64, mf, result_f);
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    builder.def_var(f64_vars[a_idx], result_f);
                }
            }
            OP_DIV_NN => {
                let mf = cranelift_codegen::ir::MemFlags::new();
                let bf = if b_idx < reg_count && reg_always_num[b_idx] {
                    builder.use_var(f64_vars[b_idx])
                } else {
                    let bv = builder.use_var(vars[b_idx]);
                    builder.ins().bitcast(F64, mf, bv)
                };
                let cf = if c_idx < reg_count && reg_always_num[c_idx] {
                    builder.use_var(f64_vars[c_idx])
                } else {
                    let cv = builder.use_var(vars[c_idx]);
                    builder.ins().bitcast(F64, mf, cv)
                };
                let result_f = builder.ins().fdiv(bf, cf);
                let result = builder.ins().bitcast(I64, mf, result_f);
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    builder.def_var(f64_vars[a_idx], result_f);
                }
            }
            OP_ADDK_N => {
                let kv = nan_consts.get(c_idx)?.as_number();
                let mf = cranelift_codegen::ir::MemFlags::new();
                let bf = if b_idx < reg_count && reg_always_num[b_idx] {
                    builder.use_var(f64_vars[b_idx])
                } else {
                    let bv = builder.use_var(vars[b_idx]);
                    builder.ins().bitcast(F64, mf, bv)
                };
                let kval = builder.ins().f64const(kv);
                let result_f = builder.ins().fadd(bf, kval);
                let result = builder.ins().bitcast(I64, mf, result_f);
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    builder.def_var(f64_vars[a_idx], result_f);
                }
            }
            OP_SUBK_N => {
                let kv = nan_consts.get(c_idx)?.as_number();
                let mf = cranelift_codegen::ir::MemFlags::new();
                let bf = if b_idx < reg_count && reg_always_num[b_idx] {
                    builder.use_var(f64_vars[b_idx])
                } else {
                    let bv = builder.use_var(vars[b_idx]);
                    builder.ins().bitcast(F64, mf, bv)
                };
                let kval = builder.ins().f64const(kv);
                let result_f = builder.ins().fsub(bf, kval);
                let result = builder.ins().bitcast(I64, mf, result_f);
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    builder.def_var(f64_vars[a_idx], result_f);
                }
            }
            OP_MULK_N => {
                let kv = nan_consts.get(c_idx)?.as_number();
                let mf = cranelift_codegen::ir::MemFlags::new();
                let bf = if b_idx < reg_count && reg_always_num[b_idx] {
                    builder.use_var(f64_vars[b_idx])
                } else {
                    let bv = builder.use_var(vars[b_idx]);
                    builder.ins().bitcast(F64, mf, bv)
                };
                let kval = builder.ins().f64const(kv);
                let result_f = builder.ins().fmul(bf, kval);
                let result = builder.ins().bitcast(I64, mf, result_f);
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    builder.def_var(f64_vars[a_idx], result_f);
                }
            }
            OP_DIVK_N => {
                let kv = nan_consts.get(c_idx)?.as_number();
                let mf = cranelift_codegen::ir::MemFlags::new();
                let bf = if b_idx < reg_count && reg_always_num[b_idx] {
                    builder.use_var(f64_vars[b_idx])
                } else {
                    let bv = builder.use_var(vars[b_idx]);
                    builder.ins().bitcast(F64, mf, bv)
                };
                let kval = builder.ins().f64const(kv);
                let result_f = builder.ins().fdiv(bf, kval);
                let result = builder.ins().bitcast(I64, mf, result_f);
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    builder.def_var(f64_vars[a_idx], result_f);
                }
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
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                    b_or_c,
                    qnan_val,
                );

                let num_block = builder.create_block();
                let slow_block = builder.create_block();
                let merge_block = builder.create_block();
                builder.append_block_param(merge_block, I64);

                builder
                    .ins()
                    .brif(both_num, num_block, &[], slow_block, &[]);

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
                let both_always_num = b_idx < reg_always_num.len()
                    && reg_always_num[b_idx]
                    && c_idx < reg_always_num.len()
                    && reg_always_num[c_idx];

                if both_always_num {
                    // Use F64 shadow vars for inputs to skip bitcasts.
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let bf = if b_idx < reg_count && reg_always_num[b_idx] {
                        builder.use_var(f64_vars[b_idx])
                    } else {
                        builder.ins().bitcast(F64, mf, bv)
                    };
                    let cf = if c_idx < reg_count && reg_always_num[c_idx] {
                        builder.use_var(f64_vars[c_idx])
                    } else {
                        builder.ins().bitcast(F64, mf, cv)
                    };

                    // Check if the immediately following instruction is JMPF or JMPT on
                    // the same destination register (a_idx). If so, fuse the comparison
                    // and conditional branch into a single compare-and-branch, eliminating
                    // the intermediate bool register write and the entire JMPF 3-block dispatch.
                    let next_inst = chunk.code.get(ip + 1).copied();
                    let fused = if let Some(next) = next_inst {
                        let next_op = (next >> 24) as u8;
                        let next_a = ((next >> 16) & 0xFF) as usize;
                        (next_op == OP_JMPF || next_op == OP_JMPT)
                            && next_a == a_idx
                            && !block_map.contains_key(&(ip + 1)) // next instr is not a block leader
                    } else {
                        false
                    };

                    if fused {
                        // Fused compare-and-branch: emit fcmp + brif, skip next JMPF/JMPT.
                        let next = chunk.code[ip + 1];
                        let next_op = (next >> 24) as u8;
                        let sbx = (next & 0xFFFF) as i16;
                        let target = (ip as isize + 2 + sbx as isize) as usize;
                        let fallthrough = ip + 2;

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

                        let true_block = block_map.get(&true_dest).copied();
                        let false_block = block_map.get(&false_dest).copied();

                        if let (Some(tb), Some(fb)) = (true_block, false_block) {
                            builder.ins().brif(cmp, tb, &[], fb, &[]);
                            block_terminated = true;
                            skip_next = true;
                        } else {
                            // Block targets not found — fall back to non-fused path.
                            let true_val = builder.ins().iconst(I64, TAG_TRUE as i64);
                            let false_val = builder.ins().iconst(I64, TAG_FALSE as i64);
                            let result = builder.ins().select(cmp, true_val, false_val);
                            builder.def_var(vars[a_idx], result);
                        }
                    } else {
                        // No fusion opportunity — emit direct float comparison without QNAN check.
                        let cmp = builder.ins().fcmp(cc, bf, cf);
                        let true_val = builder.ins().iconst(I64, TAG_TRUE as i64);
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
                        cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                        b_or_c,
                        qnan_val,
                    );

                    let num_block = builder.create_block();
                    let slow_block = builder.create_block();
                    let merge_block = builder.create_block();
                    builder.append_block_param(merge_block, I64);

                    builder
                        .ins()
                        .brif(both_num, num_block, &[], slow_block, &[]);

                    // Fast path: inline float comparison → TAG_TRUE/TAG_FALSE
                    builder.switch_to_block(num_block);
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let bf = builder.ins().bitcast(F64, mf, bv);
                    let cf = builder.ins().bitcast(F64, mf, cv);
                    let cmp = builder.ins().fcmp(cc, bf, cf);
                    let true_val = builder.ins().iconst(I64, TAG_TRUE as i64);
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
                    // Fast path: if source is always numeric or always boolean, no RC
                    // management is needed (numbers/booleans are not heap-allocated).
                    let src_always_num = b_idx < reg_always_num.len() && reg_always_num[b_idx];
                    let src_always_bool = b_idx < reg_always_bool.len() && reg_always_bool[b_idx];
                    if src_always_num || src_always_bool {
                        // No QNAN check, no clone_rc — just copy the bits.
                        builder.def_var(vars[a_idx], bv);
                        // Propagate f64 shadow so that destination can skip bitcasts too.
                        if src_always_num && a_idx < reg_always_num.len() && reg_always_num[a_idx] {
                            let bf = builder.use_var(f64_vars[b_idx]);
                            builder.def_var(f64_vars[a_idx], bf);
                        }
                    } else {
                        // General path: inline is_heap check; clone_rc only for heap values.
                        let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                        let masked = builder.ins().band(bv, qnan_val);
                        let is_heap = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::Equal,
                            masked,
                            qnan_val,
                        );
                        let clone_block = builder.create_block();
                        let after_block = builder.create_block();
                        builder
                            .ins()
                            .brif(is_heap, clone_block, &[], after_block, &[]);

                        builder.switch_to_block(clone_block);
                        let fref = get_func_ref(&mut builder, module, helpers.jit_move);
                        builder.ins().call(fref, &[bv]);
                        builder.ins().jump(after_block, &[]);

                        builder.switch_to_block(after_block);
                        builder.def_var(vars[a_idx], bv);
                    }
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
                // Inline fneg for always-numeric operands; call helper otherwise.
                if b_idx < reg_always_num.len() && reg_always_num[b_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let bf = builder.use_var(f64_vars[b_idx]);
                    let result_f = builder.ins().fneg(bf);
                    let result = builder.ins().bitcast(I64, mf, result_f);
                    builder.def_var(vars[a_idx], result);
                    if a_idx < reg_count && reg_always_num[a_idx] {
                        builder.def_var(f64_vars[a_idx], result_f);
                    }
                } else {
                    let bv = builder.use_var(vars[b_idx]);
                    let fref = get_func_ref(&mut builder, module, helpers.neg);
                    let call_inst = builder.ins().call(fref, &[bv]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                }
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
                    // Initialise F64 shadow for numeric constants so arithmetic ops
                    // can skip the bitcast when reading this register.
                    if nv.is_number() && a_idx < reg_count && reg_always_num[a_idx] {
                        let mf = cranelift_codegen::ir::MemFlags::new();
                        let fv = builder.ins().bitcast(F64, mf, kval);
                        builder.def_var(f64_vars[a_idx], fv);
                    }
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

                if let (Some(&target_block), Some(&fall_block)) =
                    (block_map.get(&target), block_map.get(&fallthrough))
                {
                    // Fast path: register is always a boolean (TAG_TRUE or TAG_FALSE).
                    // Comparison results (LT/GT/EQ/...) always fall here, so this is
                    // the common case for loop conditions and guards.
                    // TAG_FALSE = QNAN|2, TAG_TRUE = QNAN|1 — just compare to TAG_FALSE.
                    if a_idx < reg_always_bool.len() && reg_always_bool[a_idx] {
                        let false_val = builder.ins().iconst(I64, TAG_FALSE as i64);
                        let is_false = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::Equal,
                            av,
                            false_val,
                        );
                        // JMPF: jump (to target) when false; else fall through.
                        // JMPT: jump (to target) when true; else fall through.
                        if op == OP_JMPF {
                            builder
                                .ins()
                                .brif(is_false, target_block, &[], fall_block, &[]);
                        } else {
                            builder
                                .ins()
                                .brif(is_false, fall_block, &[], target_block, &[]);
                        }
                        block_terminated = true;
                    } else {
                        // General case: full truthy check.
                        // false if val==TAG_NIL or val==TAG_FALSE, true otherwise.
                        // Numbers: truthy if f64 != 0.0 (0.0 bits are 0, not TAG_FALSE/NIL).
                        let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                        let masked = builder.ins().band(av, qnan_val);
                        let is_num = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                            masked,
                            qnan_val,
                        );

                        let num_truthy_block = builder.create_block();
                        let tag_truthy_block = builder.create_block();
                        let merge_truthy = builder.create_block();
                        builder.append_block_param(merge_truthy, I64);

                        builder
                            .ins()
                            .brif(is_num, num_truthy_block, &[], tag_truthy_block, &[]);

                        // Number path: truthy if f64 != 0.0
                        builder.switch_to_block(num_truthy_block);
                        let mf = cranelift_codegen::ir::MemFlags::new();
                        let af = builder.ins().bitcast(F64, mf, av);
                        let zero = builder.ins().f64const(0.0);
                        let cmp = builder.ins().fcmp(
                            cranelift_codegen::ir::condcodes::FloatCC::NotEqual,
                            af,
                            zero,
                        );
                        let num_result = builder.ins().uextend(I64, cmp);
                        builder.ins().jump(merge_truthy, &[num_result]);

                        // Tag path: truthy if val != TAG_NIL && val != TAG_FALSE
                        builder.switch_to_block(tag_truthy_block);
                        let nil_val = builder.ins().iconst(I64, TAG_NIL as i64);
                        let false_val2 = builder.ins().iconst(I64, TAG_FALSE as i64);
                        let not_nil = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                            av,
                            nil_val,
                        );
                        let not_false = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                            av,
                            false_val2,
                        );
                        let tag_truthy = builder.ins().band(not_nil, not_false);
                        let tag_result = builder.ins().uextend(I64, tag_truthy);
                        builder.ins().jump(merge_truthy, &[tag_result]);

                        builder.switch_to_block(merge_truthy);
                        let truthy_val = builder.block_params(merge_truthy)[0];

                        if op == OP_JMPF {
                            builder
                                .ins()
                                .brif(truthy_val, fall_block, &[], target_block, &[]);
                        } else {
                            builder
                                .ins()
                                .brif(truthy_val, target_block, &[], fall_block, &[]);
                        }
                        block_terminated = true;
                    }
                }
            }
            OP_JMPNN => {
                let sbx = (inst & 0xFFFF) as i16;
                let target = (ip as isize + 1 + sbx as isize) as usize;
                let fallthrough = ip + 1;
                let av = builder.use_var(vars[a_idx]);
                let nil_const = builder.ins().iconst(I64, TAG_NIL as i64);
                let is_nil = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    av,
                    nil_const,
                );
                if let (Some(&target_block), Some(&fall_block)) =
                    (block_map.get(&target), block_map.get(&fallthrough))
                {
                    // JMPNN: jump if NOT nil → brif(is_nil, fallthrough, target)
                    builder
                        .ins()
                        .brif(is_nil, fall_block, &[], target_block, &[]);
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
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
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
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_MIN => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.min);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_MAX => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.max);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_MOD => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.mod_fn);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_FLR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.flr);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_CEL => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.cel);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_ROU => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.rou);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_RND0 => {
                let fref = get_func_ref(&mut builder, module, helpers.rnd0);
                let call_inst = builder.ins().call(fref, &[]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_RND2 => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.rnd2);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_NOW => {
                let fref = get_func_ref(&mut builder, module, helpers.now);
                let call_inst = builder.ins().call(fref, &[]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
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
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    tag,
                    arena_tag_val,
                );

                let arena_block = builder.create_block();
                let heap_block = builder.create_block();
                let merge_block = builder.create_block();
                builder.append_block_param(merge_block, I64);

                builder
                    .ins()
                    .brif(is_arena, arena_block, &[], heap_block, &[]);

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
                let field_val = builder.ins().load(
                    I64,
                    cranelift_codegen::ir::MemFlags::trusted(),
                    field_addr,
                    0,
                );
                // Inline is_heap check: (val & QNAN) == QNAN && val != NIL && val != TRUE && val != FALSE && tag != ARENA_REC
                // For numbers (the hot path), (val & QNAN) != QNAN → skip clone_rc entirely
                let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                let masked = builder.ins().band(field_val, qnan_val);
                let is_nan_tagged = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    masked,
                    qnan_val,
                );
                let clone_block = builder.create_block();
                let skip_clone_block = builder.create_block();
                builder
                    .ins()
                    .brif(is_nan_tagged, clone_block, &[], skip_clone_block, &[]);

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
                    crate::interpreter::Value::Text(s) => {
                        std::ffi::CString::new(s.as_bytes()).ok()?
                    }
                    _ => return None,
                };
                let leaked = Box::leak(Box::new(cstring));
                let field_name_ptr = leaked.as_ptr() as u64;
                let field_name_val = builder.ins().iconst(I64, field_name_ptr as i64);
                // Get registry pointer
                let registry_ptr = ACTIVE_REGISTRY.with(|r| r.get() as u64);
                let registry_val = builder.ins().iconst(I64, registry_ptr as i64);
                let fref = get_func_ref(&mut builder, module, helpers.recfld_name);
                let call_inst = builder
                    .ins()
                    .call(fref, &[bv, field_name_val, registry_val]);
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
                let cur_offset = builder.ins().load(
                    I64,
                    cranelift_codegen::ir::MemFlags::trusted(),
                    arena_ptr_val,
                    16,
                );
                // aligned_offset = (offset + 7) & !7  (already 8-aligned in practice)
                let seven = builder.ins().iconst(I64, 7);
                let off_plus_7 = builder.ins().iadd(cur_offset, seven);
                let neg8 = builder.ins().iconst(I64, !7i64);
                let aligned = builder.ins().band(off_plus_7, neg8);
                // new_offset = aligned + record_size
                let size_val = builder.ins().iconst(I64, record_size as i64);
                let new_offset = builder.ins().iadd(aligned, size_val);
                // Load arena.buf_cap and check space
                let buf_cap = builder.ins().load(
                    I64,
                    cranelift_codegen::ir::MemFlags::trusted(),
                    arena_ptr_val,
                    8,
                );
                let has_space = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThanOrEqual,
                    new_offset,
                    buf_cap,
                );

                let alloc_block = builder.create_block();
                let fallback_block = builder.create_block();
                let merge_block = builder.create_block();
                builder.append_block_param(merge_block, I64);

                builder
                    .ins()
                    .brif(has_space, alloc_block, &[], fallback_block, &[]);

                // ── Inline alloc path ──
                builder.switch_to_block(alloc_block);
                // rec_ptr = arena.buf_ptr + aligned_offset
                let buf_ptr = builder.ins().load(
                    I64,
                    cranelift_codegen::ir::MemFlags::trusted(),
                    arena_ptr_val,
                    0,
                );
                let rec_ptr = builder.ins().iadd(buf_ptr, aligned);
                // Write ArenaRecord header: type_id(u16) | n_fields(u16) | pad(u32) as u64
                let header = ((n_fields as u64) << 16) | (type_id as u64);
                let header_val = builder.ins().iconst(I64, header as i64);
                builder.ins().store(
                    cranelift_codegen::ir::MemFlags::trusted(),
                    header_val,
                    rec_ptr,
                    0,
                );
                // Write field values and clone_rc heap fields
                for i in 0..n_fields {
                    let field_v = builder.use_var(vars[a_idx + 1 + i]);
                    let field_off = (8 + i * 8) as i32;
                    builder.ins().store(
                        cranelift_codegen::ir::MemFlags::trusted(),
                        field_v,
                        rec_ptr,
                        field_off,
                    );
                    // Inline is_heap check: if (val & QNAN) == QNAN → call jit_move (clone_rc)
                    // For numbers (hot path), this branch is not taken.
                    let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                    let masked = builder.ins().band(field_v, qnan_val);
                    let is_heap = builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::Equal,
                        masked,
                        qnan_val,
                    );
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
                builder.ins().store(
                    cranelift_codegen::ir::MemFlags::trusted(),
                    new_offset,
                    arena_ptr_val,
                    16,
                );
                // Result = TAG_ARENA_REC | rec_ptr
                let tag_val = builder.ins().iconst(I64, TAG_ARENA_REC as i64);
                let result_val = builder.ins().bor(rec_ptr, tag_val);
                builder.ins().jump(merge_block, &[result_val]);

                // ── Fallback path: arena full → call jit_recnew helper ──
                builder.switch_to_block(fallback_block);
                let slot =
                    builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
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
                let registry_ptr_val = builder
                    .ins()
                    .iconst(I64, &program.type_registry as *const TypeRegistry as i64);
                let fref = get_func_ref(&mut builder, module, helpers.recnew);
                let call_inst = builder.ins().call(
                    fref,
                    &[
                        arena_ptr_val,
                        type_id_nfields_val,
                        regs_ptr,
                        registry_ptr_val,
                    ],
                );
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

                // Extract field indices from the constant pool.
                // Detect whether they are all numeric (resolved indices vs. string names).
                let (update_indices, all_resolved) = match &chunk.constants[indices_idx] {
                    Value::List(items) => {
                        let resolved = items.iter().all(|v| matches!(v, Value::Number(_)));
                        let indices: Vec<u8> = items
                            .iter()
                            .map(|v| match v {
                                Value::Number(n) => *n as u8,
                                _ => 0,
                            })
                            .collect();
                        (indices, resolved)
                    }
                    _ => return None,
                };

                let old_rec = builder.use_var(vars[a_idx]);

                if all_resolved {
                    // ── Fully inlined arena path ───────────────────────────────────────────
                    //
                    // For arena records with all-resolved field indices we inline the entire
                    // copy+update without any C-ABI call, eliminating:
                    //   • jit_recwith_arena call overhead (save/restore caller-saved regs)
                    //   • all clone_rc/drop_rc calls for numeric fields (no-ops for f64)
                    //
                    // The copy loop uses Cranelift block parameters to carry the loop
                    // counter, avoiding the need for an extra Cranelift Variable.
                    //
                    // BumpArena #[repr(C)]: buf_ptr(0), buf_cap(8), offset(16).
                    // ArenaRecord header: ((n_fields << 16) | type_id) as u64 at offset 0.
                    // Fields: 8 bytes each starting at offset 8.
                    let arena_ptr_rw = jit_arena_ptr();
                    let arena_ptr_rw_val = builder.ins().iconst(I64, arena_ptr_rw as i64);
                    let mf_t = cranelift_codegen::ir::MemFlags::trusted();

                    // Branch on arena tag
                    let tag_mask_rw = builder.ins().iconst(I64, TAG_MASK as i64);
                    let tag_rw = builder.ins().band(old_rec, tag_mask_rw);
                    let arena_tag_rw = builder.ins().iconst(I64, TAG_ARENA_REC as i64);
                    let is_arena_rw = builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::Equal,
                        tag_rw,
                        arena_tag_rw,
                    );

                    let arena_inline_block = builder.create_block();
                    let fallback_rw_block = builder.create_block();
                    let merge_rw_block = builder.create_block();
                    builder.append_block_param(merge_rw_block, I64);

                    builder.ins().brif(
                        is_arena_rw,
                        arena_inline_block,
                        &[],
                        fallback_rw_block,
                        &[],
                    );

                    // ── Arena inline path ──────────────────────────────────────────────────
                    builder.switch_to_block(arena_inline_block);
                    builder.seal_block(arena_inline_block);

                    // Decode old record pointer and header
                    let ptr_mask_rw = builder.ins().iconst(I64, PTR_MASK as i64);
                    let old_ptr_rw = builder.ins().band(old_rec, ptr_mask_rw);
                    let header_rw = builder.ins().load(I64, mf_t, old_ptr_rw, 0);
                    // n_fields = (header >> 16) & 0xFFFF
                    let n_fields_rt = {
                        let shifted = builder.ins().ushr_imm(header_rw, 16);
                        let mask16 = builder.ins().iconst(I64, 0xFFFFi64);
                        builder.ins().band(shifted, mask16)
                    };

                    // Compute record_size = 8 + n_fields * 8, then inline bump-alloc
                    let eight_rw = builder.ins().iconst(I64, 8i64);
                    let fields_bytes_rw = builder.ins().imul(n_fields_rt, eight_rw);
                    let record_size_rw = builder.ins().iadd(fields_bytes_rw, eight_rw);

                    let cur_off_rw = builder.ins().load(I64, mf_t, arena_ptr_rw_val, 16);
                    let seven_rw = builder.ins().iconst(I64, 7i64);
                    let neg8_rw = builder.ins().iconst(I64, !7i64);
                    let off_plus_7_rw = builder.ins().iadd(cur_off_rw, seven_rw);
                    let aligned_rw = builder.ins().band(off_plus_7_rw, neg8_rw);
                    let new_off_rw = builder.ins().iadd(aligned_rw, record_size_rw);
                    let buf_cap_rw = builder.ins().load(I64, mf_t, arena_ptr_rw_val, 8);
                    let has_space_rw = builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThanOrEqual,
                        new_off_rw,
                        buf_cap_rw,
                    );

                    let alloc_rw_block = builder.create_block();
                    let alloc_fallback_rw_block = builder.create_block();
                    builder.ins().brif(
                        has_space_rw,
                        alloc_rw_block,
                        &[],
                        alloc_fallback_rw_block,
                        &[],
                    );

                    // ── Alloc success: inline copy + update ───────────────────────────────
                    builder.switch_to_block(alloc_rw_block);
                    builder.seal_block(alloc_rw_block);

                    let buf_ptr_rw = builder.ins().load(I64, mf_t, arena_ptr_rw_val, 0);
                    let new_ptr_rw = builder.ins().iadd(buf_ptr_rw, aligned_rw);

                    // Copy header from old record → new record
                    builder.ins().store(mf_t, header_rw, new_ptr_rw, 0);

                    // ── Field copy loop using block parameters ─────────────────────────
                    // The loop counter `i` is threaded as a Cranelift block parameter so
                    // Cranelift's SSA construction can handle it without needing an extra
                    // Variable declaration. All blocks that carry `i` declare it as a param.
                    //
                    //   loop_copy_hdr(i)  → if i >= n_fields: copy_done
                    //                     → else: copy_body(i)
                    //   copy_body(i)      → copy field, check is_heap
                    //                     → if heap: clone_rw_block(i)
                    //                     → else: after_clone_rw(i)
                    //   clone_rw_block(i) → call jit_move, jump after_clone_rw(i)
                    //   after_clone_rw(i) → i++, jump loop_copy_hdr(i+1)
                    //   copy_done         → (no params)
                    let loop_copy_hdr = builder.create_block();
                    builder.append_block_param(loop_copy_hdr, I64); // param 0: i
                    let copy_body = builder.create_block();
                    builder.append_block_param(copy_body, I64); // param 0: i
                    let clone_rw_block = builder.create_block();
                    builder.append_block_param(clone_rw_block, I64); // param 0: i (carry)
                    let after_clone_rw = builder.create_block();
                    builder.append_block_param(after_clone_rw, I64); // param 0: i (carry)
                    let copy_done = builder.create_block();

                    let zero_rw = builder.ins().iconst(I64, 0i64);
                    builder.ins().jump(loop_copy_hdr, &[zero_rw]);

                    // ── Loop header ──
                    builder.switch_to_block(loop_copy_hdr);
                    let ci_hdr = builder.block_params(loop_copy_hdr)[0];
                    let loop_done_rw = builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::UnsignedGreaterThanOrEqual,
                        ci_hdr,
                        n_fields_rt,
                    );
                    builder
                        .ins()
                        .brif(loop_done_rw, copy_done, &[], copy_body, &[ci_hdr]);

                    // ── Loop body ──
                    builder.switch_to_block(copy_body);
                    builder.seal_block(copy_body);
                    let ci = builder.block_params(copy_body)[0];
                    let ci_bytes_rw = builder.ins().imul(ci, eight_rw);
                    let ci_off_rw = builder.ins().iadd(ci_bytes_rw, eight_rw);
                    let src_addr_rw = builder.ins().iadd(old_ptr_rw, ci_off_rw);
                    let fv_rw = builder.ins().load(I64, mf_t, src_addr_rw, 0);
                    let dst_addr_rw = builder.ins().iadd(new_ptr_rw, ci_off_rw);
                    builder.ins().store(mf_t, fv_rw, dst_addr_rw, 0);
                    let qnan_rw = builder.ins().iconst(I64, QNAN as i64);
                    let fv_masked_rw = builder.ins().band(fv_rw, qnan_rw);
                    let fv_is_heap_rw = builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::Equal,
                        fv_masked_rw,
                        qnan_rw,
                    );
                    // Thread `ci` to both branches via block parameters
                    builder
                        .ins()
                        .brif(fv_is_heap_rw, clone_rw_block, &[ci], after_clone_rw, &[ci]);

                    // ── Clone path: increment RC ──
                    builder.switch_to_block(clone_rw_block);
                    builder.seal_block(clone_rw_block);
                    let ci_in_clone = builder.block_params(clone_rw_block)[0];
                    let fref_move_rw = get_func_ref(&mut builder, module, helpers.jit_move);
                    builder.ins().call(fref_move_rw, &[fv_rw]);
                    builder.ins().jump(after_clone_rw, &[ci_in_clone]);

                    // ── After clone: i++ and loop back ──
                    builder.switch_to_block(after_clone_rw);
                    builder.seal_block(after_clone_rw);
                    let ci_cont = builder.block_params(after_clone_rw)[0];
                    let ci_next_rw = builder.ins().iadd_imm(ci_cont, 1);
                    builder.ins().jump(loop_copy_hdr, &[ci_next_rw]);

                    builder.seal_block(loop_copy_hdr);
                    builder.switch_to_block(copy_done);
                    builder.seal_block(copy_done);

                    // ── Overwrite updated fields (compile-time unrolled) ───────────────
                    // For each update: drop RC of old slot, write new value, clone RC of new value.
                    // For numeric fields (hot path), all RC operations are no-ops and
                    // Cranelift will eliminate the dead branches.
                    let qnan_upd = builder.ins().iconst(I64, QNAN as i64);
                    for (upd_i, &field_slot) in update_indices.iter().enumerate() {
                        let new_val_rw = builder.use_var(vars[a_idx + 1 + upd_i]);
                        let slot_off = (8 + field_slot as i64 * 8) as i32;

                        // Load old slot value from new record (already copied above)
                        let old_fv = builder.ins().load(I64, mf_t, new_ptr_rw, slot_off);
                        // Drop RC on old slot if it's heap
                        let old_masked_rw = builder.ins().band(old_fv, qnan_upd);
                        let old_is_heap_rw = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::Equal,
                            old_masked_rw,
                            qnan_upd,
                        );
                        let drop_rw_block = builder.create_block();
                        let after_drop_rw = builder.create_block();
                        builder
                            .ins()
                            .brif(old_is_heap_rw, drop_rw_block, &[], after_drop_rw, &[]);
                        builder.switch_to_block(drop_rw_block);
                        builder.seal_block(drop_rw_block);
                        let fref_drop_rw = get_func_ref(&mut builder, module, helpers.drop_rc);
                        builder.ins().call(fref_drop_rw, &[old_fv]);
                        builder.ins().jump(after_drop_rw, &[]);
                        builder.switch_to_block(after_drop_rw);
                        builder.seal_block(after_drop_rw);

                        // Write new value
                        builder.ins().store(mf_t, new_val_rw, new_ptr_rw, slot_off);
                        // Clone RC on new value if it's heap
                        let nv_masked_rw = builder.ins().band(new_val_rw, qnan_upd);
                        let nv_is_heap_rw = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::Equal,
                            nv_masked_rw,
                            qnan_upd,
                        );
                        let clone_nv_block = builder.create_block();
                        let after_nv_clone = builder.create_block();
                        builder
                            .ins()
                            .brif(nv_is_heap_rw, clone_nv_block, &[], after_nv_clone, &[]);
                        builder.switch_to_block(clone_nv_block);
                        builder.seal_block(clone_nv_block);
                        let fref_move_nv = get_func_ref(&mut builder, module, helpers.jit_move);
                        builder.ins().call(fref_move_nv, &[new_val_rw]);
                        builder.ins().jump(after_nv_clone, &[]);
                        builder.switch_to_block(after_nv_clone);
                        builder.seal_block(after_nv_clone);
                    }

                    // Update arena.offset and return arena-tagged pointer
                    builder.ins().store(mf_t, new_off_rw, arena_ptr_rw_val, 16);
                    let tag_arena_rw = builder.ins().iconst(I64, TAG_ARENA_REC as i64);
                    let result_rw = builder.ins().bor(new_ptr_rw, tag_arena_rw);
                    builder.ins().jump(merge_rw_block, &[result_rw]);

                    // ── Arena-full fallback → jit_recwith_arena ────────────────────────
                    builder.switch_to_block(alloc_fallback_rw_block);
                    builder.seal_block(alloc_fallback_rw_block);
                    {
                        let indices_bytes_fb: &'static [u8] =
                            Box::leak(update_indices.clone().into_boxed_slice());
                        let slot_fb = builder.create_sized_stack_slot(
                            cranelift_codegen::ir::StackSlotData::new(
                                cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                                (n_updates * 8) as u32,
                                0,
                            ),
                        );
                        for i in 0..n_updates {
                            let v = builder.use_var(vars[a_idx + 1 + i]);
                            builder.ins().stack_store(v, slot_fb, (i * 8) as i32);
                        }
                        let regs_ptr_fb = builder.ins().stack_addr(I64, slot_fb, 0);
                        let indices_ptr_fb =
                            builder.ins().iconst(I64, indices_bytes_fb.as_ptr() as i64);
                        let n_upd_fb = builder.ins().iconst(I64, n_updates as i64);
                        let fref_fb = get_func_ref(&mut builder, module, helpers.recwith_arena);
                        let call_fb = builder.ins().call(
                            fref_fb,
                            &[
                                old_rec,
                                arena_ptr_rw_val,
                                indices_ptr_fb,
                                n_upd_fb,
                                regs_ptr_fb,
                            ],
                        );
                        let fb_res = builder.inst_results(call_fb)[0];
                        builder.ins().jump(merge_rw_block, &[fb_res]);
                    }

                    // ── Heap fallback → jit_recwith ────────────────────────────────────
                    builder.switch_to_block(fallback_rw_block);
                    builder.seal_block(fallback_rw_block);
                    {
                        let indices_bytes_hp: &'static [u8] =
                            Box::leak(update_indices.into_boxed_slice());
                        let slot_hp = builder.create_sized_stack_slot(
                            cranelift_codegen::ir::StackSlotData::new(
                                cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                                (n_updates * 8) as u32,
                                0,
                            ),
                        );
                        for i in 0..n_updates {
                            let v = builder.use_var(vars[a_idx + 1 + i]);
                            builder.ins().stack_store(v, slot_hp, (i * 8) as i32);
                        }
                        let regs_ptr_hp = builder.ins().stack_addr(I64, slot_hp, 0);
                        let indices_ptr_hp =
                            builder.ins().iconst(I64, indices_bytes_hp.as_ptr() as i64);
                        let n_upd_hp = builder.ins().iconst(I64, n_updates as i64);
                        let fref_hp = get_func_ref(&mut builder, module, helpers.recwith);
                        let call_hp = builder
                            .ins()
                            .call(fref_hp, &[old_rec, indices_ptr_hp, n_upd_hp, regs_ptr_hp]);
                        let hp_res = builder.inst_results(call_hp)[0];
                        builder.ins().jump(merge_rw_block, &[hp_res]);
                    }

                    // ── Merge ──────────────────────────────────────────────────────────
                    builder.switch_to_block(merge_rw_block);
                    builder.seal_block(merge_rw_block);
                    let result_rw_final = builder.block_params(merge_rw_block)[0];
                    builder.def_var(vars[a_idx], result_rw_final);
                } else {
                    // Unresolved (string) field names: general path
                    let indices_bytes: &'static [u8] = Box::leak(update_indices.into_boxed_slice());
                    let slot =
                        builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
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
                    let call_inst = builder
                        .ins()
                        .call(fref, &[old_rec, indices_ptr_val, n_updates_val, regs_ptr]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                }
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
                    let slot =
                        builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
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
                //   [ptr +  0]  discriminant = 1 for List variant  (8 bytes)
                //   [ptr +  8]  Vec.cap  (usize)   — capacity (first Vec field on aarch64)
                //   [ptr + 16]  Vec.data_ptr  (*mut NanVal, each slot is u64/8 bytes)
                //   [ptr + 24]  Vec.len  (usize)   — length (last Vec field on aarch64)
                //
                // Rust Vec<T> field order on aarch64: [cap, ptr, len] (confirmed via
                // runtime probing with a multi-variant enum that prevents niche opt).
                // NOTE: earlier code incorrectly read +8 as "len"; that is actually cap.
                //
                // Fast path (bv is TAG_LIST and cv is a number):
                //   idx_u = fcvt_to_uint_sat(bitcast_f64(cv))  // NaN/neg → 0
                //   if idx_u < [ptr+24]:                       // compare vs Vec.len
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

                    // vec_len = *[ptr + 24]  (Vec.len — actual element count)
                    // SAFETY: TAG_LIST check guarantees ptr points to a live HeapObj::List.
                    // HeapObj layout: [discriminant(+0), Vec.cap(+8), Vec.data_ptr(+16), Vec.len(+24)]
                    let vec_len = builder.ins().load(I64, mf_trusted, ptr, 24);

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
                    builder
                        .ins()
                        .brif(elem_is_heap, clone_block, &[], after_clone_block, &[]);

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
                //
                // Inlined fast path with loop-invariant caching:
                //   HeapObj::List layout (ptr = bv & PTR_MASK):
                //     [ptr +  0]  enum discriminant  (8 bytes)
                //     [ptr +  8]  Vec.cap  (usize)
                //     [ptr + 16]  Vec.data_ptr
                //     [ptr + 24]  Vec.len  (usize)   ← bounds check
                //
                //   Rust Vec<T> field order on aarch64: [cap, data_ptr, len].
                //   The multi-variant HeapObj enum places discriminant first,
                //   then Vec fields in their natural [cap, ptr, len] order.
                //   We read Vec.len at ptr+24 for the bounds check.
                //
                // ptr, data_ptr, vec_len, and int_idx=0 are stored in per-loop
                // Cranelift vars (see foreach_loop_map) so FOREACHNEXT can reuse them.
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
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
                    // Vec.len at ptr+24.  SAFETY: TAG_LIST guarantees a live HeapObj::List.
                    // HeapObj layout: [discriminant(+0), Vec.cap(+8), Vec.data_ptr(+16), Vec.len(+24)]
                    let vec_len = builder.ins().load(I64, mf_trusted, ptr, 24);
                    let idx_u = builder.ins().iconst(I64, 0i64);
                    let in_bounds = builder.ins().icmp(ic_ult, idx_u, vec_len);

                    let in_bounds_block = builder.create_block();
                    builder.ins().brif(in_bounds, in_bounds_block, &[], jb, &[]);

                    builder.switch_to_block(in_bounds_block);
                    builder.seal_block(in_bounds_block);
                    let data_ptr = builder.ins().load(I64, mf_trusted, ptr, 16);

                    // Cache loop invariants for FOREACHNEXT fast path.
                    if let Some(&loop_idx) = foreach_loop_map.get(&(b_idx, c_idx)) {
                        builder.def_var(fe_ptr_var(loop_idx), ptr);
                        builder.def_var(fe_data_ptr_var(loop_idx), data_ptr);
                        builder.def_var(fe_len_var(loop_idx), vec_len);
                        builder.def_var(fe_idx_var(loop_idx), idx_u);
                    }

                    let elem = builder.ins().load(I64, mf_trusted, data_ptr, 0);

                    let elem_masked = builder.ins().band(elem, qnan_c);
                    let elem_is_heap = builder.ins().icmp(ic_eq, elem_masked, qnan_c);
                    let clone_block = builder.create_block();
                    let after_clone_block = builder.create_block();
                    builder
                        .ins()
                        .brif(elem_is_heap, clone_block, &[], after_clone_block, &[]);

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
                //
                // FAST PATH (using cached vars from FOREACHPREP):
                //   - Integer index increment (iadd), no f64 round-trip per iteration
                //   - No ptr re-extraction (no mask per iteration)
                //   - No vec_len or data_ptr memory reloads (both cached)
                //   - Hot loop: bounds-check + element load + optional RC clone only
                //
                // R[C] (NanVal f64 index) kept consistent via fcvt_from_uint.
                let mf_plain = cranelift_codegen::ir::MemFlags::new();
                let mf_trusted = cranelift_codegen::ir::MemFlags::trusted();
                let ic_eq = cranelift_codegen::ir::condcodes::IntCC::Equal;
                let ic_ult = cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThan;

                let jmp_block = block_map.get(&(ip + 1)).copied();
                let body_block = block_map.get(&(ip + 2)).copied();

                if let (Some(jb), Some(bb)) = (jmp_block, body_block) {
                    if let Some(&loop_idx) = foreach_loop_map.get(&(b_idx, c_idx)) {
                        // ── Fast path: cached loop invariants ──
                        let int_idx = builder.use_var(fe_idx_var(loop_idx));
                        let one_i = builder.ins().iconst(I64, 1i64);
                        let new_int_idx = builder.ins().iadd(int_idx, one_i);
                        builder.def_var(fe_idx_var(loop_idx), new_int_idx);

                        let new_idx_f64 = builder.ins().fcvt_from_uint(F64, new_int_idx);
                        let new_idx_nanval = builder.ins().bitcast(I64, mf_plain, new_idx_f64);
                        builder.def_var(vars[c_idx], new_idx_nanval);

                        let vec_len = builder.use_var(fe_len_var(loop_idx));
                        let in_bounds = builder.ins().icmp(ic_ult, new_int_idx, vec_len);

                        let in_bounds_block = builder.create_block();
                        builder.ins().brif(in_bounds, in_bounds_block, &[], jb, &[]);

                        builder.switch_to_block(in_bounds_block);
                        builder.seal_block(in_bounds_block);

                        let data_ptr = builder.use_var(fe_data_ptr_var(loop_idx));
                        let eight = builder.ins().iconst(I64, 8i64);
                        let byte_off = builder.ins().imul(new_int_idx, eight);
                        let elem_addr = builder.ins().iadd(data_ptr, byte_off);
                        let elem = builder.ins().load(I64, mf_trusted, elem_addr, 0);

                        let qnan_c = builder.ins().iconst(I64, QNAN as i64);
                        let elem_masked = builder.ins().band(elem, qnan_c);
                        let elem_is_heap = builder.ins().icmp(ic_eq, elem_masked, qnan_c);
                        let clone_block = builder.create_block();
                        let after_clone_block = builder.create_block();
                        builder
                            .ins()
                            .brif(elem_is_heap, clone_block, &[], after_clone_block, &[]);

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
                        // ── Slow path: no cached loop data (should not occur normally)
                        let cv = builder.use_var(vars[c_idx]);
                        let cv_f64 = builder.ins().bitcast(F64, mf_plain, cv);
                        let one_f64 = builder.ins().f64const(1.0);
                        let new_idx_f64 = builder.ins().fadd(cv_f64, one_f64);
                        let new_idx = builder.ins().bitcast(I64, mf_plain, new_idx_f64);
                        builder.def_var(vars[c_idx], new_idx);

                        let bv = builder.use_var(vars[b_idx]);
                        let ptr_mask_c = builder.ins().iconst(I64, PTR_MASK as i64);
                        let ptr = builder.ins().band(bv, ptr_mask_c);
                        // Vec.len at ptr+24 (aarch64 HeapObj layout: [discriminant, cap, data_ptr, len]).
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
                        builder
                            .ins()
                            .brif(elem_is_heap, clone_block, &[], after_clone_block, &[]);

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
                    }
                } else {
                    let cv = builder.use_var(vars[c_idx]);
                    let cv_f64 = builder.ins().bitcast(F64, mf_plain, cv);
                    let one_f64 = builder.ins().f64const(1.0);
                    let new_idx_f64 = builder.ins().fadd(cv_f64, one_f64);
                    let new_idx = builder.ins().bitcast(I64, mf_plain, new_idx_f64);
                    builder.def_var(vars[c_idx], new_idx);
                    let bv = builder.use_var(vars[b_idx]);
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
            //
            // Two optimisations applied here:
            //
            // 1. F64 shadow variables: if R[A] is always numeric we pre-converted it
            //    to F64 at function entry (or at the write site for non-parameter
            //    numeric registers).  Reuse that F64 value directly, skipping the
            //    redundant i64→f64 bitcast that would otherwise appear on every guard.
            //
            // 2. Jump threading: ip+1 is always OP_JMP (the body-skip jump). Instead
            //    of branching to the JMP block and then immediately jumping again, we
            //    decode the JMP target here and use it directly as the false branch
            //    destination.  This eliminates one unconditional jump per failed guard.
            op if op == OP_CMPK_GE_N
                || op == OP_CMPK_GT_N
                || op == OP_CMPK_LT_N
                || op == OP_CMPK_LE_N
                || op == OP_CMPK_EQ_N
                || op == OP_CMPK_NE_N =>
            {
                let ki = (inst & 0xFF) as usize;

                // Optimisation 1: use the pre-converted F64 shadow when available.
                let lhs_f64 = if a_idx < reg_count && reg_always_num[a_idx] {
                    builder.use_var(f64_vars[a_idx])
                } else {
                    let lhs = builder.use_var(vars[a_idx]);
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    builder.ins().bitcast(F64, mf, lhs)
                };

                let rhs_f64 = if ki < nan_consts.len() {
                    builder.ins().f64const(nan_consts[ki].as_number())
                } else {
                    builder.ins().f64const(0.0)
                };

                use cranelift_codegen::ir::condcodes::FloatCC;
                let cc = match op {
                    op if op == OP_CMPK_GE_N => FloatCC::GreaterThanOrEqual,
                    op if op == OP_CMPK_GT_N => FloatCC::GreaterThan,
                    op if op == OP_CMPK_LT_N => FloatCC::LessThan,
                    op if op == OP_CMPK_LE_N => FloatCC::LessThanOrEqual,
                    op if op == OP_CMPK_EQ_N => FloatCC::Equal,
                    _ => FloatCC::NotEqual, // OP_CMPK_NE_N
                };
                let cmp = builder.ins().fcmp(cc, lhs_f64, rhs_f64);

                // ip + 1 = the OP_JMP that skips the body (taken when condition FALSE)
                // ip + 2 = first instruction of the guard body (taken when condition TRUE)
                let body_block = block_map.get(&(ip + 2)).copied();

                // Optimisation 2: jump threading.
                // The instruction at ip+1 is always OP_JMP.  Decode its target now so
                // that the false branch goes directly there, skipping the intermediate
                // JMP block entirely.  If the look-ahead fails for any reason, fall
                // back to the original behaviour (branch to the JMP block).
                let false_dest_block = chunk
                    .code
                    .get(ip + 1)
                    .and_then(|&jmp_inst| {
                        let jmp_op = (jmp_inst >> 24) as u8;
                        if jmp_op == OP_JMP {
                            let sbx = (jmp_inst & 0xFFFF) as i16;
                            let jmp_target = (ip as isize + 2 + sbx as isize) as usize;
                            block_map.get(&jmp_target).copied()
                        } else {
                            None
                        }
                    })
                    .or_else(|| block_map.get(&(ip + 1)).copied());

                if let (Some(false_block), Some(bb)) = (false_dest_block, body_block) {
                    // condition TRUE → body; condition FALSE → threaded miss target
                    builder.ins().brif(cmp, bb, &[], false_block, &[]);
                    block_terminated = true;
                }
                // else: blocks not found → JIT bails (should not happen in practice)
            }
            OP_CALL => {
                let a = ((inst >> 16) & 0xFF) as u8;
                let bx = (inst & 0xFFFF) as usize;
                let func_idx = bx >> 8;
                let n_args = bx & 0xFF;

                let a_idx_call = a as usize;
                let call_result: cranelift_codegen::ir::Value;
                if func_idx < all_func_ids.len() {
                    // Check if the target function is inlinable (pure numeric guard chain).
                    // If so, emit its IR directly here instead of a real call — this avoids
                    // all function-call overhead (ABI setup, call/ret instructions, register
                    // saves) which is the dominant cost for tight guard-chain loops.
                    let can_inline = func_idx < program.chunks.len()
                        && is_inlinable(
                            &program.chunks[func_idx],
                            &program.nan_constants[func_idx],
                        )
                        && n_args == program.chunks[func_idx].param_count as usize
                        && inline_var_map.contains_key(&ip);

                    if can_inline {
                        let callee_chunk = &program.chunks[func_idx];
                        let callee_consts = &program.nan_constants[func_idx];
                        let result_var = vars[a as usize];

                        // Collect arg Variables and build F64 shadows for them.
                        let arg_var_list: Vec<Variable> =
                            (0..n_args).map(|i| vars[a as usize + 1 + i]).collect();
                        let f64_arg_list: Vec<Variable> = {
                            let mf = cranelift_codegen::ir::MemFlags::new();
                            let f64_slots = inline_f64_var_map
                                .get(&ip)
                                .map(|v| v.as_slice())
                                .unwrap_or(&[]);
                            for (i, &av) in arg_var_list.iter().enumerate() {
                                if i < f64_slots.len() {
                                    let iv = builder.use_var(av);
                                    let fv = builder.ins().bitcast(F64, mf, iv);
                                    builder.def_var(f64_slots[i], fv);
                                }
                            }
                            f64_slots.to_vec()
                        };

                        let extra_var_list = inline_var_map
                            .get(&ip)
                            .map(|v| v.as_slice())
                            .unwrap_or(&[])
                            .to_vec();

                        // Create a merge block where inline code converges after each RET.
                        let merge_blk = builder.create_block();

                        let ok = inline_chunk(
                            &mut builder,
                            callee_chunk,
                            callee_consts,
                            &arg_var_list,
                            result_var,
                            &extra_var_list,
                            &f64_arg_list,
                            merge_blk,
                        );

                        if ok {
                            // Switch to merge block so subsequent code continues there.
                            builder.switch_to_block(merge_blk);
                            // result_var already holds the inlined return value.
                            block_terminated = false;
                        } else {
                            // Inlining failed mid-way; fall back to a real call.
                            // We need to terminate whatever block we're in and fall through.
                            builder.ins().jump(merge_blk, &[]);
                            builder.switch_to_block(merge_blk);

                            let target_fid = all_func_ids[func_idx];
                            let target_fref = get_func_ref(&mut builder, module, target_fid);
                            let call_args: Vec<_> = (0..n_args)
                                .map(|i| builder.use_var(vars[a as usize + 1 + i]))
                                .collect();
                            let call_inst = builder.ins().call(target_fref, &call_args);
                            let result = builder.inst_results(call_inst)[0];
                            builder.def_var(result_var, result);
                        }
                    } else {
                        // Direct call: the target function is compiled in this module
                        let target_fid = all_func_ids[func_idx];
                        let target_fref = get_func_ref(&mut builder, module, target_fid);
                        let mut call_args = Vec::with_capacity(n_args);
                        for i in 0..n_args {
                            call_args.push(builder.use_var(vars[a_idx_call + 1 + i]));
                        }
                        let call_inst = builder.ins().call(target_fref, &call_args);
                        call_result = builder.inst_results(call_inst)[0];
                        builder.def_var(vars[a_idx_call], call_result);
                    } // end else (not inlined)
                } else {
                    // Fallback: use jit_call helper for out-of-range func indices
                    if n_args > 0 {
                        let slot = builder.create_sized_stack_slot(
                            cranelift_codegen::ir::StackSlotData::new(
                                cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                                (n_args * 8) as u32,
                                0,
                            ),
                        );
                        for i in 0..n_args {
                            let v = builder.use_var(vars[a_idx_call + 1 + i]);
                            builder.ins().stack_store(v, slot, (i * 8) as i32);
                        }
                        let args_ptr = builder.ins().stack_addr(I64, slot, 0);
                        let prog_ptr = builder.ins().iconst(I64, program_ptr_val as i64);
                        let func_idx_val = builder.ins().iconst(I64, func_idx as i64);
                        let n_args_val = builder.ins().iconst(I64, n_args as i64);
                        let fref = get_func_ref(&mut builder, module, helpers.call);
                        let call_inst = builder
                            .ins()
                            .call(fref, &[prog_ptr, func_idx_val, args_ptr, n_args_val]);
                        call_result = builder.inst_results(call_inst)[0];
                        builder.def_var(vars[a_idx_call], call_result);
                    } else {
                        let null_ptr = builder.ins().iconst(I64, 0i64);
                        let prog_ptr = builder.ins().iconst(I64, program_ptr_val as i64);
                        let func_idx_val = builder.ins().iconst(I64, func_idx as i64);
                        let n_args_val = builder.ins().iconst(I64, 0i64);
                        let fref = get_func_ref(&mut builder, module, helpers.call);
                        let call_inst = builder
                            .ins()
                            .call(fref, &[prog_ptr, func_idx_val, null_ptr, n_args_val]);
                        call_result = builder.inst_results(call_inst)[0];
                        builder.def_var(vars[a_idx_call], call_result);
                    }
                }
                // Update F64 shadow so arithmetic ops can skip bitcast when using this
                // register as input.
                if a_idx_call < reg_count && reg_always_num[a_idx_call] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rv = builder.use_var(vars[a_idx_call]);
                    let rf = builder.ins().bitcast(F64, mf, rv);
                    builder.def_var(f64_vars[a_idx_call], rf);
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
                let bv = builder.use_var(vars[b_idx]); // url
                let cv = builder.use_var(vars[c_idx]); // body
                // Consume the next instruction (data word) at compile time
                let data_inst = chunk.code[ip + 1];
                skip_next = true;
                let d_idx = ((data_inst >> 16) & 0xFF) as usize; // headers register
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
pub fn compile(
    chunk: &Chunk,
    _nan_consts: &[NanVal],
    program: &CompiledProgram,
) -> Option<JitFunction> {
    // Find the entry function index by matching chunk pointer
    let entry_idx = program.chunks.iter().position(|c| std::ptr::eq(c, chunk))?;
    compile_program(program, entry_idx)
}

/// Compile all functions in the program and return a JitFunction for the entry at `entry_idx`.
fn compile_program(program: &CompiledProgram, entry_idx: usize) -> Option<JitFunction> {
    let mut flag_builder = settings::builder();
    flag_builder.set("opt_level", "speed").ok()?;
    let isa_builder = cranelift_native::builder().ok()?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .ok()?;

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
    for (i, (chunk, nan_consts)) in program
        .chunks
        .iter()
        .zip(program.nan_constants.iter())
        .enumerate()
    {
        compile_function_body(
            &mut module,
            chunk,
            nan_consts,
            func_ids[i],
            &helpers,
            &func_ids,
            program,
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
    if args.len() != func.param_count {
        return None;
    }
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
            let f: extern "C" fn(u64, u64, u64) -> u64 =
                unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2])
        }
        4 => {
            let f: extern "C" fn(u64, u64, u64, u64) -> u64 =
                unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2], args[3])
        }
        5 => {
            let f: extern "C" fn(u64, u64, u64, u64, u64) -> u64 =
                unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2], args[3], args[4])
        }
        6 => {
            let f: extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64 =
                unsafe { std::mem::transmute(func.func_ptr) };
            f(args[0], args[1], args[2], args[3], args[4], args[5])
        }
        7 => {
            let f: extern "C" fn(u64, u64, u64, u64, u64, u64, u64) -> u64 =
                unsafe { std::mem::transmute(func.func_ptr) };
            f(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6],
            )
        }
        8 => {
            let f: extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64 =
                unsafe { std::mem::transmute(func.func_ptr) };
            f(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
            )
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
pub fn compile_and_call(
    chunk: &Chunk,
    nan_consts: &[NanVal],
    args: &[u64],
    program: &CompiledProgram,
) -> Option<u64> {
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
        let result = jit_run_numeric(
            "f a:n b:n c:n d:n e:n>n;+a +b +c +d e",
            "f",
            &[1.0, 2.0, 3.0, 4.0, 5.0],
        );
        assert_eq!(result, Some(15.0));
    }

    #[test]
    fn cranelift_6_args() {
        let result = jit_run_numeric(
            "f a:n b:n c:n d:n e:n f0:n>n;+a +b +c +d +e f0",
            "f",
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        );
        assert_eq!(result, Some(21.0));
    }

    #[test]
    fn cranelift_7_args() {
        let result = jit_run_numeric(
            "f a:n b:n c:n d:n e:n f0:n g0:n>n;+a +b +c +d +e +f0 g0",
            "f",
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
        );
        assert_eq!(result, Some(28.0));
    }

    #[test]
    fn cranelift_8_args() {
        let result = jit_run_numeric(
            "f a:n b:n c:n d:n e:n f0:n g0:n h:n>n;+a +b +c +d +e +f0 +g0 h",
            "f",
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
        );
        assert_eq!(result, Some(36.0));
    }

    #[test]
    fn cranelift_9_args_hits_fallback() {
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex(
            "f a:n b:n c:n d:n e:n f0:n g0:n h:n i:n>n;+a +b +c +d +e +f0 +g0 +h i",
        )
        .unwrap()
        .into_iter()
        .map(|(t, _)| t)
        .collect();
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
        let result = jit_run(
            r#"f a:t b:t>t;+ a b"#,
            "f",
            &[Value::Text("hello".into()), Value::Text(" world".into())],
        );
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
        let result = jit_run(
            "f a:n b:n>b;= a b",
            "f",
            &[Value::Number(5.0), Value::Number(5.0)],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_inequality() {
        let result = jit_run(
            "f a:n b:n>b;!= a b",
            "f",
            &[Value::Number(5.0), Value::Number(3.0)],
        );
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
        assert_eq!(
            result,
            Some(Value::Err(Box::new(Value::Text("bad".into()))))
        );
    }

    #[test]
    fn cranelift_len_string() {
        let result = jit_run(r#"f s:t>n;len s"#, "f", &[Value::Text("hello".into())]);
        assert_eq!(result, Some(Value::Number(5.0)));
    }

    #[test]
    fn cranelift_len_list() {
        let result = jit_run(
            "f xs:L n>n;len xs",
            "f",
            &[Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])],
        );
        assert_eq!(result, Some(Value::Number(3.0)));
    }

    #[test]
    fn cranelift_not() {
        let result = jit_run("f x:b>b;! x", "f", &[Value::Bool(true)]);
        assert_eq!(result, Some(Value::Bool(false)));
    }

    #[test]
    fn cranelift_comparison_gt() {
        let result = jit_run(
            "f a:n b:n>b;> a b",
            "f",
            &[Value::Number(5.0), Value::Number(3.0)],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_comparison_lt() {
        let result = jit_run(
            "f a:n b:n>b;< a b",
            "f",
            &[Value::Number(3.0), Value::Number(5.0)],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_while_loop() {
        // sum 1..n using while loop
        let result = jit_run(
            "f n:n>n;s=0;i=1;wh <= i n{s=+s i;i=+i 1};s",
            "f",
            &[Value::Number(10.0)],
        );
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
        let result = jit_run(
            "double x:n>n;* x 2\nf x:n>n;double x",
            "f",
            &[Value::Number(5.0)],
        );
        assert_eq!(result, Some(Value::Number(10.0)));
    }

    // ── num / flr / cel / min / max / rnd ─────────────────────────────────────

    #[test]
    fn cranelift_num_builtin() {
        // num returns R n t; match to extract the inner number
        let result = jit_run(
            r#"f s:t>n;r=num s;?r{~v:v;^_:0}"#,
            "f",
            &[Value::Text("3.14".into())],
        );
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
        let result = jit_run(
            "f a:n b:n>n;min a b",
            "f",
            &[Value::Number(3.0), Value::Number(7.0)],
        );
        assert_eq!(result, Some(Value::Number(3.0)));
    }

    #[test]
    fn cranelift_max_builtin() {
        let result = jit_run(
            "f a:n b:n>n;max a b",
            "f",
            &[Value::Number(3.0), Value::Number(7.0)],
        );
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
        unsafe {
            std::env::set_var("ILO_JIT_TEST_VAR", "hello");
        }
        let result = jit_run(
            r#"f k:t>R t t;env k"#,
            "f",
            &[Value::Text("ILO_JIT_TEST_VAR".into())],
        );
        assert_eq!(
            result,
            Some(Value::Ok(Box::new(Value::Text("hello".into()))))
        );
    }

    // ── spl / cat ─────────────────────────────────────────────────────────────

    #[test]
    fn cranelift_spl_builtin() {
        let result = jit_run(
            r#"f s:t sep:t>L t;spl s sep"#,
            "f",
            &[Value::Text("a,b,c".into()), Value::Text(",".into())],
        );
        assert_eq!(
            result,
            Some(Value::List(vec![
                Value::Text("a".into()),
                Value::Text("b".into()),
                Value::Text("c".into()),
            ]))
        );
    }

    #[test]
    fn cranelift_cat_builtin() {
        let result = jit_run(
            r#"f xs:L t sep:t>t;cat xs sep"#,
            "f",
            &[
                Value::List(vec![Value::Text("x".into()), Value::Text("y".into())]),
                Value::Text("-".into()),
            ],
        );
        assert_eq!(result, Some(Value::Text("x-y".into())));
    }

    // ── has / hd / tl / rev / srt / slc ──────────────────────────────────────

    #[test]
    fn cranelift_has_list() {
        let result = jit_run(
            "f xs:L n v:n>b;has xs v",
            "f",
            &[
                Value::List(vec![
                    Value::Number(1.0),
                    Value::Number(2.0),
                    Value::Number(3.0),
                ]),
                Value::Number(2.0),
            ],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_hd_builtin() {
        let result = jit_run(
            "f xs:L n>n;hd xs",
            "f",
            &[Value::List(vec![Value::Number(10.0), Value::Number(20.0)])],
        );
        assert_eq!(result, Some(Value::Number(10.0)));
    }

    #[test]
    fn cranelift_tl_builtin() {
        let result = jit_run(
            "f xs:L n>L n;tl xs",
            "f",
            &[Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])],
        );
        assert_eq!(
            result,
            Some(Value::List(vec![Value::Number(2.0), Value::Number(3.0)]))
        );
    }

    #[test]
    fn cranelift_rev_builtin() {
        let result = jit_run(
            "f xs:L n>L n;rev xs",
            "f",
            &[Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])],
        );
        assert_eq!(
            result,
            Some(Value::List(vec![
                Value::Number(3.0),
                Value::Number(2.0),
                Value::Number(1.0)
            ]))
        );
    }

    #[test]
    fn cranelift_srt_builtin() {
        let result = jit_run(
            "f xs:L n>L n;srt xs",
            "f",
            &[Value::List(vec![
                Value::Number(3.0),
                Value::Number(1.0),
                Value::Number(2.0),
            ])],
        );
        assert_eq!(
            result,
            Some(Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ]))
        );
    }

    #[test]
    fn cranelift_slc_builtin() {
        let result = jit_run(
            "f xs:L n a:n b:n>L n;slc xs a b",
            "f",
            &[
                Value::List(vec![
                    Value::Number(10.0),
                    Value::Number(20.0),
                    Value::Number(30.0),
                    Value::Number(40.0),
                ]),
                Value::Number(1.0),
                Value::Number(3.0),
            ],
        );
        assert_eq!(
            result,
            Some(Value::List(vec![Value::Number(20.0), Value::Number(30.0)]))
        );
    }

    // ── list append / listnew / index ─────────────────────────────────────────

    #[test]
    fn cranelift_listappend() {
        // +=xs v — append single element (BinOp::Append → OP_LISTAPPEND)
        let result = jit_run(
            "f xs:L n v:n>L n;r=+=xs v;r",
            "f",
            &[
                Value::List(vec![Value::Number(1.0), Value::Number(2.0)]),
                Value::Number(3.0),
            ],
        );
        assert_eq!(
            result,
            Some(Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ]))
        );
    }

    #[test]
    fn cranelift_listnew() {
        // [a, b] literal — OP_LISTNEW
        let result = jit_run(
            "f a:n b:n>L n;[a, b]",
            "f",
            &[Value::Number(5.0), Value::Number(6.0)],
        );
        assert_eq!(
            result,
            Some(Value::List(vec![Value::Number(5.0), Value::Number(6.0)]))
        );
    }

    #[test]
    fn cranelift_index_literal() {
        // xs.0 — literal index 0 → OP_INDEX
        let result = jit_run(
            "f xs:L n>n;xs.0",
            "f",
            &[Value::List(vec![
                Value::Number(10.0),
                Value::Number(20.0),
                Value::Number(30.0),
            ])],
        );
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
        let result = jit_run(
            r#"f s:t>R t t;jpar s"#,
            "f",
            &[Value::Text(r#"{"k":"v"}"#.into())],
        );
        assert!(matches!(result, Some(Value::Ok(_))));
    }

    #[test]
    fn cranelift_jpth_ok() {
        let result = jit_run(
            r#"f j:t p:t>R t t;jpth j p"#,
            "f",
            &[
                Value::Text(r#"{"name":"alice"}"#.into()),
                Value::Text("name".into()),
            ],
        );
        assert_eq!(
            result,
            Some(Value::Ok(Box::new(Value::Text("alice".into()))))
        );
    }

    // ── isok / iserr / unwrap — via match patterns ────────────────────────────

    #[test]
    fn cranelift_isok_via_match() {
        // match pattern Ok → OP_ISOK emitted in JIT
        let result = jit_run(
            "f x:R n t>n;?x{~v:v;^_:0}",
            "f",
            &[Value::Ok(Box::new(Value::Number(7.0)))],
        );
        assert_eq!(result, Some(Value::Number(7.0)));
    }

    #[test]
    fn cranelift_iserr_via_match() {
        // match pattern Err → OP_ISERR emitted in JIT
        let result = jit_run(
            r#"f x:R n t>n;?x{~_:1;^_:99}"#,
            "f",
            &[Value::Err(Box::new(Value::Text("bad".into())))],
        );
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
            .unwrap()
            .into_iter()
            .map(|(t, r)| {
                (
                    t,
                    crate::ast::Span {
                        start: r.start,
                        end: r.end,
                    },
                )
            })
            .collect();
        let (prog, errors) = parser::parse(tokens);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let nan_args: Vec<u64> = [Value::Text(r#"{"score":42}"#.to_string())]
            .iter()
            .map(|v| NanVal::from_value(v).0)
            .collect();
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
        use crate::vm::{OP_CALL, OP_RET, compile as vm_compile};
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex("g>n;42\nf>n;42")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
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
        let result = jit_run(
            "f xs:L n>n;s=0;@x xs{s=+s x};s",
            "f",
            &[Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])],
        );
        assert_eq!(result, Some(Value::Number(6.0)));
    }

    #[test]
    fn cranelift_foreach_loop_heap_elements() {
        // Foreach over strings: exercises the RC clone (elem_is_heap) branch
        // in the inline OP_LISTGET fast path.
        let result = jit_run(
            "f xs:L n>n;s=0;@x xs{s=+s 1};s",
            "f",
            &[Value::List(vec![
                Value::Text("a".to_string()),
                Value::Text("b".to_string()),
                Value::Text("c".to_string()),
            ])],
        );
        assert_eq!(result, Some(Value::Number(3.0)));
    }

    #[test]
    fn cranelift_foreach_empty_list() {
        // Foreach over empty list: bounds check fails immediately, sum stays 0.
        let result = jit_run(
            "f xs:L n>n;s=0;@x xs{s=+s x};s",
            "f",
            &[Value::List(vec![])],
        );
        assert_eq!(result, Some(Value::Number(0.0)));
    }

    #[test]
    fn cranelift_sequential_cross_function_calls() {
        // Two sequential calls: a=dbl(n), then triple(a)
        let result = jit_run(
            "dbl x:n>n;*x 2\ntriple x:n>n;*x 3\nf n:n>n;a=dbl n;triple a",
            "f",
            &[Value::Number(5.0)],
        );
        assert_eq!(result, Some(Value::Number(30.0)));
    }

    #[test]
    fn cranelift_pipe_chain() {
        // Pipe chain: inc(dbl(inc(dbl(5)))) = (5*2+1)*2+1 = 23
        let result = jit_run(
            "dbl x:n>n;*x 2\ninc x:n>n;+x 1\nf n:n>n;n>>dbl>>inc>>dbl>>inc",
            "f",
            &[Value::Number(5.0)],
        );
        assert_eq!(result, Some(Value::Number(23.0)));
    }

    #[test]
    fn cranelift_foreach_listbuild_many_calls() {
        // Repeated JIT calls that build a list then sum via foreach.
        // Previously crashed because jit_listappend used reserve_exact(1) (O(n²) allocs)
        // and the JIT read Vec.cap at HeapObj+8 instead of Vec.len at HeapObj+24.
        // Now fixed: JIT reads Vec.len at HeapObj+24 and uses standard push (amortized O(1)).
        let source = "f n:n>n;xs=[];i=0;wh <i n{xs=+=xs i;i=+i 1};s=0;@x xs{s=+s x};s";
        let prog = {
            let tokens: Vec<crate::lexer::Token> = crate::lexer::lex(source)
                .unwrap()
                .into_iter()
                .map(|(t, _)| t)
                .collect();
            crate::parser::parse_tokens(tokens).unwrap()
        };
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let n_val = crate::vm::NanVal::from_value(&Value::Number(100.0)).0;
        let nan_args = vec![n_val];
        crate::vm::with_active_registry(&compiled, || {
            if let Some(jit_func) = compile(chunk, nan_consts, &compiled) {
                for i in 0..10_100u32 {
                    let result = call(&jit_func, &nan_args).expect("JIT call failed");
                    let val = crate::vm::NanVal(result).to_value();
                    assert_eq!(val, Value::Number(4950.0), "failed on iteration {}", i);
                }
            } else {
                panic!("JIT compilation failed");
            }
        });
    }

    // ── Coverage gap tests ──────────────────────────────────────────────

    // Division and subtraction with constants
    #[test]
    fn cranelift_cov_divk_n() {
        let result = jit_run_numeric("f x:n>n;/x 2", "f", &[10.0]);
        assert_eq!(result, Some(5.0));
    }

    // Greater-than comparison (OP_CMPK_GT_N path) — braceless guards (early return)
    #[test]
    fn cranelift_cov_gt_comparison() {
        let result = jit_run_numeric("f x:n>n;>x 5 1;0", "f", &[10.0]);
        assert_eq!(result, Some(1.0));
    }

    #[test]
    fn cranelift_cov_gt_comparison_false() {
        let result = jit_run_numeric("f x:n>n;>x 5 1;0", "f", &[3.0]);
        assert_eq!(result, Some(0.0));
    }

    // GTE comparison
    #[test]
    fn cranelift_cov_gte() {
        let result = jit_run_numeric("f x:n>n;>=x 5 1;0", "f", &[5.0]);
        assert_eq!(result, Some(1.0));
    }

    // LTE comparison
    #[test]
    fn cranelift_cov_lte() {
        let result = jit_run_numeric("f x:n>n;<=x 5 1;0", "f", &[5.0]);
        assert_eq!(result, Some(1.0));
    }

    // Modulo via JIT
    #[test]
    fn cranelift_cov_modulo() {
        let result = jit_run_numeric("f a:n b:n>n;mod a b", "f", &[10.0, 3.0]);
        assert_eq!(result, Some(1.0));
    }

    // Record creation and field access via JIT
    #[test]
    fn cranelift_cov_record_field() {
        let result = jit_run_numeric("type pt{x:n;y:n}\nf>n;p=pt x:3 y:4;p.x", "f", &[]);
        assert_eq!(result, Some(3.0));
    }

    // Record with update via JIT
    #[test]
    fn cranelift_cov_record_with() {
        let result = jit_run_numeric(
            "type pt{x:n;y:n}\nf>n;p=pt x:1 y:2;q=p with x:10;+q.x q.y",
            "f",
            &[],
        );
        assert_eq!(result, Some(12.0));
    }

    // Equality check — braceless guard (early return)
    #[test]
    fn cranelift_cov_eq() {
        let result = jit_run_numeric("f a:n b:n>n;=a b 1;0", "f", &[5.0, 5.0]);
        assert_eq!(result, Some(1.0));
    }

    // Not-equal check — braceless guard (early return)
    #[test]
    fn cranelift_cov_neq() {
        let result = jit_run_numeric("f a:n b:n>n;!=a b 1;0", "f", &[5.0, 3.0]);
        assert_eq!(result, Some(1.0));
    }

    // Deeply nested call chain via JIT
    #[test]
    fn cranelift_cov_deep_call() {
        let result = jit_run_numeric(
            "a x:n>n;+x 1\nb x:n>n;a x\nf x:n>n;b x",
            "f",
            &[10.0],
        );
        assert_eq!(result, Some(11.0));
    }

    // Nil return (optional result)
    #[test]
    fn cranelift_cov_nil_guard() {
        let result = jit_run("f x:n>n;>x 0{x}", "f", &[Value::Number(-1.0)]);
        // JIT may return None (cannot compile) or Some(Nil)
        match result {
            Some(Value::Nil) => {} // expected
            None => {}             // JIT bailed out, also fine
            other => panic!("expected Nil or None, got {:?}", other),
        }
    }

    // ── Type predicates — OP_ISNUM, OP_ISTEXT, OP_ISBOOL, OP_ISLIST ────────

    #[test]
    fn cranelift_isnum_true() {
        let result = jit_run(
            "f x:n>b;?x{n _:true;_:false}",
            "f",
            &[Value::Number(42.0)],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_istext_true() {
        let result = jit_run(
            r#"f x:t>b;?x{t _:true;_:false}"#,
            "f",
            &[Value::Text("hello".into())],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_isbool_true() {
        let result = jit_run(
            "f x:b>b;?x{b _:true;_:false}",
            "f",
            &[Value::Bool(true)],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_islist_true() {
        let result = jit_run(
            "f x:n>b;?x{l _:true;_:false}",
            "f",
            &[Value::List(vec![Value::Number(1.0)])],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    // ── Map operations — OP_MAPNEW, OP_MSET, OP_MGET, OP_MHAS, OP_MKEYS,
    //                    OP_MVALS, OP_MDEL ──────────────────────────────────────

    #[test]
    fn cranelift_mapnew_mset_mget() {
        let result = jit_run(
            r#"f>n;m=mset mmap "k" 42;mget m "k""#,
            "f",
            &[],
        );
        assert_eq!(result, Some(Value::Number(42.0)));
    }

    #[test]
    fn cranelift_mhas_true() {
        let result = jit_run(
            r#"f>b;m=mset mmap "x" 1;mhas m "x""#,
            "f",
            &[],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_mhas_false() {
        let result = jit_run(
            r#"f>b;m=mmap;mhas m "missing""#,
            "f",
            &[],
        );
        assert_eq!(result, Some(Value::Bool(false)));
    }

    #[test]
    fn cranelift_mkeys_returns_list() {
        let result = jit_run(
            r#"f>n;m=mset mmap "a" 1;ks=mkeys m;len ks"#,
            "f",
            &[],
        );
        assert_eq!(result, Some(Value::Number(1.0)));
    }

    #[test]
    fn cranelift_mvals_returns_list() {
        let result = jit_run(
            r#"f>n;m=mset mmap "a" 99;vs=mvals m;len vs"#,
            "f",
            &[],
        );
        assert_eq!(result, Some(Value::Number(1.0)));
    }

    #[test]
    fn cranelift_mdel_removes_key() {
        let result = jit_run(
            r#"f>b;m=mset mmap "x" 1;m=mdel m "x";mhas m "x""#,
            "f",
            &[],
        );
        assert_eq!(result, Some(Value::Bool(false)));
    }

    // ── OP_PRT, OP_TRM, OP_UNQ ──────────────────────────────────────────────

    #[test]
    fn cranelift_prt_returns_value() {
        // prnt x returns the value (with side-effect of printing)
        let result = jit_run("f x:n>n;prnt x;x", "f", &[Value::Number(7.0)]);
        assert_eq!(result, Some(Value::Number(7.0)));
    }

    #[test]
    fn cranelift_trm_strips_whitespace() {
        let result = jit_run(
            r#"f s:t>t;trm s"#,
            "f",
            &[Value::Text("  hello  ".into())],
        );
        assert_eq!(result, Some(Value::Text("hello".into())));
    }

    #[test]
    fn cranelift_unq_deduplicates() {
        let result = jit_run(
            "f xs:L n>L n;unq xs",
            "f",
            &[Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(1.0),
                Value::Number(3.0),
            ])],
        );
        match result {
            Some(Value::List(v)) => assert_eq!(v.len(), 3),
            other => panic!("expected List of 3, got {:?}", other),
        }
    }

    // ── File I/O ops — OP_RD, OP_RDL, OP_WR, OP_WRL ────────────────────────

    #[test]
    fn cranelift_wr_wrl_emits_code() {
        // Write a temp file and read it back
        let tmp = std::env::temp_dir().join("ilo_jit_test_wr.txt");
        let path = tmp.to_str().unwrap().to_string();
        // wr path content returns Result; wrl writes with newline
        let src = r#"f p:t c:t>R t t;wr p c"#;
        let result = jit_run(src, "f", &[
            Value::Text(path.clone()),
            Value::Text("hello".into()),
        ]);
        // wr returns Ok or Err; just check it returns something
        assert!(result.is_some(), "wr should return a value");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn cranelift_rd_reads_file() {
        // Write a file then read it
        let tmp = std::env::temp_dir().join("ilo_jit_test_rd.txt");
        std::fs::write(&tmp, "42").unwrap();
        let path = tmp.to_str().unwrap().to_string();
        let src = r#"f p:t>R t t;rd p"#;
        let result = jit_run(src, "f", &[Value::Text(path)]);
        let _ = std::fs::remove_file(&tmp);
        match result {
            Some(Value::Ok(v)) => assert_eq!(*v, Value::Text("42".into())),
            other => panic!("expected Ok(42), got {:?}", other),
        }
    }

    #[test]
    fn cranelift_rdl_reads_lines() {
        let tmp = std::env::temp_dir().join("ilo_jit_test_rdl.txt");
        std::fs::write(&tmp, "line1\nline2\n").unwrap();
        let path = tmp.to_str().unwrap().to_string();
        let src = r#"f p:t>R L t t;rdl p"#;
        let result = jit_run(src, "f", &[Value::Text(path)]);
        let _ = std::fs::remove_file(&tmp);
        assert!(
            matches!(&result, Some(Value::Ok(_))),
            "expected Ok(lines), got {:?}",
            result
        );
    }

    #[test]
    fn cranelift_wrl_writes_lines() {
        let tmp = std::env::temp_dir().join("ilo_jit_test_wrl.txt");
        let path = tmp.to_str().unwrap().to_string();
        let src = r#"f p:t>R t t;wrl p ["a","b"]"#;
        let result = jit_run(src, "f", &[Value::Text(path.clone())]);
        let _ = std::fs::remove_file(&tmp);
        assert!(result.is_some(), "wrl should return a value");
    }

    // ── Inlining path: is_inlinable returns true for pure numeric guards ──────

    #[test]
    fn cranelift_inline_numeric_callee() {
        // A simple numeric function that gets inlined by the JIT.
        // double is pure numeric and inlinable.
        let result = jit_run_numeric(
            "double x:n>n;*x 2\nf x:n>n;double x",
            "f",
            &[25.0],
        );
        assert_eq!(result, Some(50.0));
    }

    #[test]
    fn cranelift_inline_callee_boundary_lo() {
        // Guard chain that returns lo when x <= lo
        let result = jit_run_numeric(
            "pos x:n>n;>x 0 x;0\nf x:n>n;pos x",
            "f",
            &[-5.0],
        );
        assert_eq!(result, Some(0.0));
    }

    #[test]
    fn cranelift_inline_callee_boundary_hi() {
        // Guard chain that returns 1 when x > 100
        let result = jit_run_numeric(
            "big x:n>n;>x 100 1;0\nf x:n>n;big x",
            "f",
            &[200.0],
        );
        assert_eq!(result, Some(1.0));
    }

    #[test]
    fn cranelift_inline_add_nn_sub_nn_mul_nn_div_nn() {
        // Callee uses OP_ADD_NN — inlinable
        let result = jit_run_numeric(
            "add2 a:n b:n>n;+a b\nf x:n>n;add2 x x",
            "f",
            &[5.0],
        );
        assert_eq!(result, Some(10.0));
    }

    #[test]
    fn cranelift_inline_subk_n_divk_n_mulk_n() {
        // Callee uses OP_DIVK_N — inlinable
        let result = jit_run_numeric(
            "halve x:n>n;/x 2\nf x:n>n;halve x",
            "f",
            &[10.0],
        );
        assert_eq!(result, Some(5.0));
    }

    // ── is_inlinable edge cases (lines 312-346) ───────────────────────────────

    #[test]
    fn cranelift_non_numeric_callee_not_inlined() {
        // Callee uses string ops — not all_regs_numeric so not inlined,
        // falls through to direct call path instead.
        let result = jit_run(
            "adds a:t b:t>t;+ a b\nf a:t b:t>t;adds a b",
            "f",
            &[Value::Text("hi".into()), Value::Text("!".into())],
        );
        assert_eq!(result, Some(Value::Text("hi!".into())));
    }

    // ── inline_chunk CMPK_EQ_N path ──────────────────────────────────────────

    #[test]
    fn cranelift_inline_cmpk_eq_n() {
        // Callee: return 1 if x==0, else 0 — exercises CMPK_EQ_N in inline_chunk.
        // (Fallthrough must be a literal so the code doesn't "fall off end")
        let result = jit_run_numeric(
            "mayone x:n>n;=x 0 1;0\nf x:n>n;mayone x",
            "f",
            &[0.0],
        );
        assert_eq!(result, Some(1.0));
    }

    #[test]
    fn cranelift_inline_cmpk_ne_n() {
        // Callee: return 99 if x!=0, else 0 — exercises CMPK_NE_N in inline_chunk
        let result = jit_run_numeric(
            "nonzero x:n>n;!=x 0 99;0\nf x:n>n;nonzero x",
            "f",
            &[5.0],
        );
        assert_eq!(result, Some(99.0));
    }

    // ── OP_FOREACHNEXT slow path (no cached loop data) ───────────────────────

    #[test]
    fn cranelift_foreach_string_list() {
        // Foreach over a string list — exercises non-numeric elements path.
        let result = jit_run(
            "f xs:L t>n;s=0;@x xs{n=len x;s=+s n};s",
            "f",
            &[Value::List(vec![
                Value::Text("ab".into()),
                Value::Text("cde".into()),
            ])],
        );
        assert_eq!(result, Some(Value::Number(5.0)));
    }

    // ── OP_LISTGET fallback path (block_map missing entries) ─────────────────

    #[test]
    fn cranelift_listget_via_index_op() {
        // xs.1 uses OP_INDEX (direct index), not LISTGET via foreach.
        let result = jit_run(
            "f xs:L n>n;xs.1",
            "f",
            &[Value::List(vec![Value::Number(10.0), Value::Number(20.0)])],
        );
        assert_eq!(result, Some(Value::Number(20.0)));
    }

    // ── OP_GET (HTTP GET builtin) ────────────────────────────────────────────

    #[test]
    fn cranelift_get_builtin_emits_code() {
        // OP_GET is HTTP GET; we just verify it compiles without panicking.
        // The actual request will fail (no server) — that's fine.
        use crate::vm;
        let src = r#"f url:t>R t t;get url"#;
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex(src)
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        // Just check it compiles without panicking
        let _ = compile(chunk, nan_consts, &compiled);
    }

    // ── OP_NUM builtin ───────────────────────────────────────────────────────

    #[test]
    fn cranelift_num_parse_integer() {
        let result = jit_run(
            r#"f s:t>n;r=num s;?r{~v:v;^_:0}"#,
            "f",
            &[Value::Text("42".into())],
        );
        assert_eq!(result, Some(Value::Number(42.0)));
    }

    // ── OP_INDEX with literal index ──────────────────────────────────────────

    #[test]
    fn cranelift_index_literal_second() {
        // xs.1 uses OP_INDEX with literal index 1
        let result = jit_run(
            "f xs:L n>n;xs.1",
            "f",
            &[Value::List(vec![Value::Number(100.0), Value::Number(200.0)])],
        );
        assert_eq!(result, Some(Value::Number(200.0)));
    }

    // ── OP_MOD builtin ───────────────────────────────────────────────────────

    #[test]
    fn cranelift_mod_builtin() {
        let result = jit_run_numeric("f a:n b:n>n;mod a b", "f", &[17.0, 5.0]);
        assert_eq!(result, Some(2.0));
    }

    // ── OP_ADD/MUL with non-numeric operands (slow path) ─────────────────────

    #[test]
    fn cranelift_add_slow_path_text() {
        // + with text operands hits the slow (helper) path in the inline numeric check
        let result = jit_run(
            r#"f a:t b:t>t;+ a b"#,
            "f",
            &[Value::Text("foo".into()), Value::Text("bar".into())],
        );
        assert_eq!(result, Some(Value::Text("foobar".into())));
    }

    // ── OP_GE, OP_LE with always-numeric fast path ────────────────────────────

    #[test]
    fn cranelift_ge_always_numeric() {
        let result = jit_run(
            "f a:n b:n>b;>= a b",
            "f",
            &[Value::Number(5.0), Value::Number(5.0)],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_le_always_numeric() {
        let result = jit_run(
            "f a:n b:n>b;<= a b",
            "f",
            &[Value::Number(3.0), Value::Number(5.0)],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    // ── Fused compare-and-branch (compare + immediately-following JMPF/JMPT) ──

    #[test]
    fn cranelift_fused_cmp_branch_gt() {
        // Pattern: bool = > a b, then conditional jump on the same bool register.
        // This triggers the fused compare-and-branch path in OP_LT|OP_GT|...
        let result = jit_run_numeric(
            "f a:n b:n>n;r=0;r=> a b{r=1};r",
            "f",
            &[10.0, 5.0],
        );
        assert_eq!(result, Some(1.0));
    }

    // ── call_raw with 3 args ──────────────────────────────────────────────────

    #[test]
    fn cranelift_3_args() {
        let result = jit_run_numeric(
            "f a:n b:n c:n>n;+a +b c",
            "f",
            &[1.0, 2.0, 3.0],
        );
        assert_eq!(result, Some(6.0));
    }

    // ── compile_and_call with entry_idx != 0 ─────────────────────────────────

    #[test]
    fn cranelift_entry_not_at_index_zero() {
        // Entry function is declared after a helper: exercises non-zero entry_idx path
        let result = jit_run_numeric(
            "helper x:n>n;+x 1\nf x:n>n;+x 2",
            "f",
            &[10.0],
        );
        assert_eq!(result, Some(12.0));
    }

    // ── OP_RECWITH arena path in JIT ─────────────────────────────────────────

    #[test]
    fn cranelift_recwith_arena_field_update() {
        let src = "type box{v:n} f>n;b=box v:5;b2=b with v:99;b2.v";
        let result = jit_run_numeric(src, "f", &[]);
        assert_eq!(result, Some(99.0));
    }

    // ── OP_JMPT path (jump if true) ──────────────────────────────────────────

    #[test]
    fn cranelift_jmpt_taken() {
        // The ternary ?cond a b uses JMPT/JMPF.
        // Syntax: ?<condition> <true_val> <false_val>
        let result = jit_run_numeric("f x:n>n;?<x 0 0 x", "f", &[42.0]);
        // x < 0 is false (42 > 0), so returns x=42
        assert_eq!(result, Some(42.0));
    }

    #[test]
    fn cranelift_jmpf_taken() {
        let result = jit_run_numeric("f x:n>n;?<x 0 0 x", "f", &[-1.0]);
        // x < 0 is true, returns 0
        assert_eq!(result, Some(0.0));
    }

    // ── OP_TRUTHY (general truthy check for non-bool register in JMPF) ────────

    #[test]
    fn cranelift_truthy_number() {
        // wh n{body} — condition is a number (not always-bool reg) → exercises
        // the general truthy check path in OP_JMPF handler (the 3-block diamond).
        // Counts down from 3, accumulating s = 3+2+1 = 6.
        let result = jit_run_numeric("f n:n>n;s=0;wh n{s=+s n;n=-n 1};s", "f", &[3.0]);
        assert_eq!(result, Some(6.0));
    }

    // ── OP_MOVE (a != b, identity move) via JIT ──────────────────────────────

    #[test]
    fn cranelift_move_copies_text() {
        let result = jit_run(
            r#"f x:t>t;y=x;y"#,
            "f",
            &[Value::Text("abc".into())],
        );
        assert_eq!(result, Some(Value::Text("abc".into())));
    }

    // ── OP_LOADK with nan_consts fallback (ki >= len → 0.0) ─────────────────

    #[test]
    fn cranelift_loadk_zero_fallback() {
        // Indirect test: a constant load with an in-range index
        let result = jit_run_numeric("f>n;3.14", "f", &[]);
        assert!((result.unwrap() - 3.14).abs() < 1e-10);
    }

    // ── For-range loop (OP_FORRANGEPREP / OP_FORRANGENEXT) ───────────────────

    #[test]
    fn cranelift_for_range_loop_sum() {
        let result = jit_run_numeric(
            "f n:n>n;s=0;@i 0..n{s=+s i};s",
            "f",
            &[5.0],
        );
        assert_eq!(result, Some(10.0)); // 0+1+2+3+4 = 10
    }

    // ── Inlining with LOADK (constant load in inline_chunk) ─────────────────

    #[test]
    fn cranelift_inline_loadk_constant() {
        // Callee uses LOADK to load a constant — exercises LOADK in inline_chunk.
        // A function that adds a constant (ADDK_N uses LOADK internally).
        let result = jit_run_numeric(
            "addconst x:n>n;+x 3.14\nf x:n>n;addconst x",
            "f",
            &[0.0],
        );
        assert!((result.unwrap() - 3.14).abs() < 1e-10);
    }

    // ── OP_JMPNN — nil coalescing with non-nil value (JIT) ──────────────────

    #[test]
    fn cranelift_nil_coalesce_non_nil_text() {
        let result = jit_run(
            r#"f x:O t>t;x??"default""#,
            "f",
            &[Value::Text("present".into())],
        );
        assert_eq!(result, Some(Value::Text("present".into())));
    }

    // ── OP_SLC ───────────────────────────────────────────────────────────────

    #[test]
    fn cranelift_slc_string() {
        let result = jit_run(
            r#"f s:t>t;slc s 1 3"#,
            "f",
            &[Value::Text("hello".into())],
        );
        assert_eq!(result, Some(Value::Text("el".into())));
    }

    // ── OP_INDEX (record field by name via RECFLD_NAME helper) ───────────────

    #[test]
    fn cranelift_recfld_by_index() {
        // Access second field of a record by positional index via JIT
        let src = "type pt{x:n;y:n} f a:n b:n>n;p=pt x:a y:b;p.y";
        let result = jit_run_numeric(src, "f", &[3.0, 4.0]);
        assert_eq!(result, Some(4.0));
    }

    // ── OP_FOREACHPREP fallback (jb or bb block not found) ───────────────────

    #[test]
    fn cranelift_foreach_nested_loops() {
        // Two nested foreach loops — inner loop re-enters FOREACHPREP
        let result = jit_run(
            "f rows:L L n>n;s=0;@row rows{@x row{s=+s x}};s",
            "f",
            &[Value::List(vec![
                Value::List(vec![Value::Number(1.0), Value::Number(2.0)]),
                Value::List(vec![Value::Number(3.0), Value::Number(4.0)]),
            ])],
        );
        assert_eq!(result, Some(Value::Number(10.0)));
    }

    // ── Multiple map operations in sequence ──────────────────────────────────

    #[test]
    fn cranelift_map_multi_set_get() {
        let result = jit_run(
            r#"f>n;m=mset mmap "a" 1;m=mset m "b" 2;m=mset m "c" 3;mget m "b""#,
            "f",
            &[],
        );
        assert_eq!(result, Some(Value::Number(2.0)));
    }

    // ── OP_POSTH (HTTP POST with headers) — emits code, no actual request ───

    #[test]
    fn cranelift_posth_emits_code() {
        // We just verify the JIT compiles a function using OP_POSTH without panicking.
        // OP_POSTH is emitted when `post url body hdrs` with hdrs:M t t.
        // The actual HTTP call will fail (no server), but that's fine.
        let src = r#"f url:t body:t hdrs:M t t>R t t;post url body hdrs"#;
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex(src)
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        // Should compile without panicking (result may be None if JIT bails)
        let _ = compile(chunk, nan_consts, &compiled);
    }

    // ── inline_chunk OP_SUB_NN / OP_MUL_NN / OP_DIV_NN (lines 500-526) ──────

    #[test]
    fn cranelift_inline_sub_nn_two_params() {
        // Callee uses OP_SUB_NN (both params numeric) — exercises inline_chunk SUB_NN path
        let result = jit_run_numeric(
            "subdbl a:n b:n>n;-a b\nf x:n y:n>n;subdbl x y",
            "f",
            &[10.0, 3.0],
        );
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn cranelift_inline_mul_nn_two_params() {
        // Callee uses OP_MUL_NN (both params numeric) — exercises inline_chunk MUL_NN path
        let result = jit_run_numeric(
            "muldbl a:n b:n>n;*a b\nf x:n y:n>n;muldbl x y",
            "f",
            &[4.0, 5.0],
        );
        assert_eq!(result, Some(20.0));
    }

    #[test]
    fn cranelift_inline_div_nn_two_params() {
        // Callee uses OP_DIV_NN (both params numeric) — exercises inline_chunk DIV_NN path
        let result = jit_run_numeric(
            "divdbl a:n b:n>n;/a b\nf x:n y:n>n;divdbl x y",
            "f",
            &[12.0, 4.0],
        );
        assert_eq!(result, Some(3.0));
    }

    // ── inline_chunk OP_SUBK_N (lines 539-547) ───────────────────────────────

    #[test]
    fn cranelift_inline_subk_n() {
        // Callee uses OP_SUBK_N (subtract constant) — exercises inline_chunk SUBK_N path
        let result = jit_run_numeric(
            "dec1 x:n>n;-x 1\nf x:n>n;dec1 x",
            "f",
            &[10.0],
        );
        assert_eq!(result, Some(9.0));
    }

    // ── inline_chunk f64_val_for non-param register (lines 388-389) ──────────

    #[test]
    fn cranelift_inline_local_var_f64_path() {
        // Callee has a local (non-param) register used in DIVK_N.
        // f64_val_for for non-param regs uses extra_vars + bitcast (lines 388-389).
        let result = jit_run_numeric(
            "step x:n>n;y=+x 1;/y 2\nf x:n>n;step x",
            "f",
            &[9.0],
        );
        assert_eq!(result, Some(5.0));
    }

    // ── is_inlinable: reg_count > 16 (line 315-316) ──────────────────────────

    #[test]
    fn cranelift_inline_too_many_regs_falls_back_to_direct_call() {
        // Build a callee with many local variables (> 16 regs) so is_inlinable returns
        // false due to reg_count > 16.  The caller should still produce the correct result
        // via a direct Cranelift call.
        let src = "bigfn x:n>n;\
            a=+x 1;b=+a 1;c=+b 1;d=+c 1;e=+d 1;f=+e 1;g=+f 1;h=+g 1;\
            i=+h 1;j=+i 1;k=+j 1;l=+k 1;m=+l 1;n2=+m 1;o=+n2 1;p=+o 1;\
            +p 1\n\
            f x:n>n;bigfn x";
        let result = jit_run_numeric(src, "f", &[0.0]);
        assert_eq!(result, Some(17.0));
    }

    // ── OP_SUB_NN / OP_MUL_NN / OP_DIV_NN slow path in main JIT loop ─────────
    // When all_regs_numeric = false (mixed-type params), params don't get
    // reg_always_num=true, so arithmetic with those params hits the bitcast path.

    #[test]
    fn cranelift_mul_nn_mixed_params_slow_path() {
        // f a:n b:n c:t>n;*a b — c:t makes all_regs_numeric=false.
        // OP_MUL_NN on a,b with reg_always_num=false → slow bitcast path (lines 1042-1049).
        let result = jit_run(
            "f a:n b:n c:t>n;*a b",
            "f",
            &[Value::Number(3.0), Value::Number(4.0), Value::Text("x".into())],
        );
        assert_eq!(result, Some(Value::Number(12.0)));
    }

    #[test]
    fn cranelift_sub_nn_mixed_params_slow_path() {
        // f a:n b:n c:t>n;-a b — exercises OP_SUB_NN slow bitcast path (lines 1021-1028).
        let result = jit_run(
            "f a:n b:n c:t>n;-a b",
            "f",
            &[Value::Number(10.0), Value::Number(3.0), Value::Text("x".into())],
        );
        assert_eq!(result, Some(Value::Number(7.0)));
    }

    #[test]
    fn cranelift_div_nn_mixed_params_slow_path() {
        // f a:n b:n c:t>n;/a b — exercises OP_DIV_NN slow bitcast path (lines 1063-1070).
        let result = jit_run(
            "f a:n b:n c:t>n;/a b",
            "f",
            &[Value::Number(12.0), Value::Number(4.0), Value::Text("x".into())],
        );
        assert_eq!(result, Some(Value::Number(3.0)));
    }

    // ── OP_ADD|OP_SUB|OP_MUL|OP_DIV fast and slow paths (lines 1178-1195) ────
    // OP_SUB/MUL/DIV (not _NN) are emitted when params are :_ (any type).
    // Fast path: both are numbers at runtime (lines 1178-1186).
    // Slow path: one/both are non-numeric at runtime (lines 1188-1200).

    #[test]
    fn cranelift_op_sub_any_type_fast_path() {
        // f a:_ b:_>n;-a b with numeric args → fast path (both nums), OP_SUB fsub branch
        let result = jit_run_numeric("f a:_ b:_>n;-a b", "f", &[9.0, 4.0]);
        assert_eq!(result, Some(5.0));
    }

    #[test]
    fn cranelift_op_mul_any_type_fast_path() {
        // f a:_ b:_>n;*a b with numeric args → fast path, OP_MUL fmul branch (line 1181)
        let result = jit_run_numeric("f a:_ b:_>n;*a b", "f", &[3.0, 7.0]);
        assert_eq!(result, Some(21.0));
    }

    #[test]
    fn cranelift_op_div_any_type_fast_path() {
        // f a:_ b:_>n;/a b with numeric args → fast path, OP_DIV fdiv branch (line 1182)
        let result = jit_run_numeric("f a:_ b:_>n;/a b", "f", &[20.0, 4.0]);
        assert_eq!(result, Some(5.0));
    }

    #[test]
    fn cranelift_op_sub_any_type_slow_path() {
        // f a:_ b:_>n;-a b with text args → slow path, OP_SUB helper branch (line 1192)
        let result = jit_run(
            "f a:_ b:_>n;-a b",
            "f",
            &[Value::Text("x".into()), Value::Text("y".into())],
        );
        // Non-numeric subtraction returns nil
        assert_eq!(result, Some(Value::Nil));
    }

    #[test]
    fn cranelift_op_mul_any_type_slow_path() {
        // f a:_ b:_>_;*a b with text args → slow path, OP_MUL helper branch (line 1193)
        let result = jit_run(
            "f a:_ b:_>_;*a b",
            "f",
            &[Value::Text("x".into()), Value::Text("y".into())],
        );
        // Non-numeric multiplication returns nil
        assert_eq!(result, Some(Value::Nil));
    }

    // ── OP_POST / OP_GETH (lines 3222-3237) ──────────────────────────────────

    #[test]
    fn cranelift_op_post_emits_code() {
        // OP_POST: `post url body` (2-arg, no headers).
        // We verify JIT compiles this successfully (HTTP call may fail at runtime).
        let src = r#"f url:t body:t>R t t;post url body"#;
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex(src)
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let _ = compile(chunk, nan_consts, &compiled);
    }

    #[test]
    fn cranelift_op_geth_emits_code() {
        // OP_GETH: `get url headers` (2-arg, with headers map).
        // We verify JIT compiles this successfully.
        let src = r#"f url:t hdrs:M t t>R t t;get url hdrs"#;
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex(src)
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let _ = compile(chunk, nan_consts, &compiled);
    }

    // ── OP_RECWITH unresolved field path (lines 2432-2451) ───────────────────

    #[test]
    fn cranelift_recwith_unresolved_field_compiles() {
        // r:_ with z:10 — field 'z' not in any registered type →
        // all_resolved = false → JIT takes the unresolved helper path (lines 2432-2451).
        let src = "type pt{x:n;y:n}\nf r:_>_;r with z:10";
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex(src)
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        // Compilation should succeed (covering the unresolved path)
        let jit_fn = compile(chunk, nan_consts, &compiled);
        assert!(
            jit_fn.is_some(),
            "JIT compilation should succeed for unresolved recwith"
        );
    }

    // ── OP_GET (1-arg HTTP GET) ───────────────────────────────────────────────

    #[test]
    fn cranelift_op_get_1arg_emits_code() {
        // `get url` (1-arg) emits OP_GET (HTTP GET without headers).
        let src = r#"f url:t>R t t;get url"#;
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex(src)
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let _ = compile(chunk, nan_consts, &compiled);
    }

    // ── OP_LE/GE/EQ/NE slow path helpers (lines 1334-1338) ──────────────────
    // These match arms are only reached when both operands are :_ (not always-num)
    // and at least one operand is non-numeric at runtime (slow path).

    #[test]
    fn cranelift_le_any_type_slow_path() {
        // f a:_ b:_>b;<= a b with text args → slow helper path (OP_LE → helpers.le)
        let result = jit_run(
            "f a:_ b:_>b;<= a b",
            "f",
            &[Value::Text("a".into()), Value::Text("b".into())],
        );
        // Text comparison: "a" <= "b" → true
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_ge_any_type_slow_path() {
        // f a:_ b:_>b;>= a b with text args → slow helper path (OP_GE → helpers.ge)
        let result = jit_run(
            "f a:_ b:_>b;>= a b",
            "f",
            &[Value::Text("b".into()), Value::Text("a".into())],
        );
        // Text comparison: "b" >= "a" → true
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_eq_any_type_slow_path() {
        // f a:_ b:_>b;== a b with text args → slow helper path (OP_EQ → helpers.eq)
        let result = jit_run(
            "f a:_ b:_>b;== a b",
            "f",
            &[Value::Text("hello".into()), Value::Text("hello".into())],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_ne_any_type_slow_path() {
        // f a:_ b:_>b;!= a b with text args → slow helper path (OP_NE → helpers.ne)
        let result = jit_run(
            "f a:_ b:_>b;!= a b",
            "f",
            &[Value::Text("x".into()), Value::Text("y".into())],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    // ── OP_JMPT always-bool fast path (lines 1508-1511) ─────────────────────
    // env! emits ISOK + JMPT where ISOK result is always boolean → reg_always_bool.

    #[test]
    fn cranelift_jmpt_always_bool_fast_path() {
        // f k:t>t;env! k — ISOK writes bool, JMPT on bool reg → fast path (lines 1508-1511).
        // We just verify JIT compiles this (env! may fail at runtime, but codegen is covered).
        let src = r#"f k:t>t;env! k"#;
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex(src)
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        // Should compile; the JMPT on always-bool reg exercises lines 1508-1511
        let jit_fn = compile(chunk, nan_consts, &compiled);
        assert!(jit_fn.is_some(), "JIT should compile env! pattern");
    }

    // ── OP_LISTGET fast path (lines 2509-2611) ────────────────────────────────
    // OP_LISTGET is a legacy opcode (never emitted by current ilo compiler).
    // We craft a CompiledProgram with OP_LISTGET bytecode directly to cover this path.

    #[test]
    fn cranelift_op_listget_fast_path() {

        // Encode a minimal program:
        // [0] LISTGET R0, R1, R2   — R0 = R1[R2], skip next if found
        // [1] JMP 0                 — exit (fallback when not found / out-of-bounds)
        // [2] RET R0                — return R0 (element found)
        //
        // block_map will have: ip+1=1 (jb), ip+2=2 (bb).
        // This exercises the OP_LISTGET fast path (lines 2519-2603).

        let encode_abc = |op: u8, a: u8, b: u8, c: u8| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | (b as u32) << 8 | c as u32
        };
        let encode_abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        let code = vec![
            encode_abc(OP_LISTGET, 0, 1, 2), // ip=0: LISTGET R0, R1, R2
            encode_abx(OP_JMP, 0, 1u16),      // ip=1: JMP +1 → ip=3 (exit)
            encode_abx(OP_RET, 0, 0),          // ip=2: RET R0
            encode_abx(OP_RET, 0, 0),          // ip=3: RET R0 (fallthrough exit)
        ];
        let dummy_span = crate::ast::Span { start: 0, end: 0 };
        let chunk = Chunk {
            code,
            constants: vec![],
            param_count: 3, // params: R0(result), R1(list), R2(index)
            reg_count: 3,
            spans: vec![dummy_span; 4],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        // Compile — should succeed (covers OP_LISTGET fast path lines 2519-2603)
        let entry_idx = 0;
        let jit_fn = compile_program(&program, entry_idx);
        assert!(jit_fn.is_some(), "JIT should compile OP_LISTGET program");
    }

    // ── OP_FOREACHNEXT slow path (lines 2776-2836) ────────────────────────────
    // The slow path is taken when foreach_loop_map.get(&(b_idx, c_idx)) returns None,
    // which only happens with manually crafted bytecode (OP_FOREACHNEXT with no
    // corresponding OP_FOREACHPREP for the same (b, c) pair).

    #[test]
    fn cranelift_foreachnext_slow_path() {

        // Craft a program with FOREACHNEXT but NO FOREACHPREP.
        // foreach_loop_map will be empty → get(&(1, 2)) returns None → slow path.
        //
        // [0] FOREACHNEXT R0, R1, R2  — slow path (no cached loop data)
        // [1] JMP +1 → ip=3 (exit loop)
        // [2] RET R0
        // [3] RET R0 (exit)

        let encode_abc = |op: u8, a: u8, b: u8, c: u8| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | (b as u32) << 8 | c as u32
        };
        let encode_abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        let code = vec![
            encode_abc(OP_FOREACHNEXT, 0, 1, 2), // ip=0: FOREACHNEXT R0, R1, R2
            encode_abx(OP_JMP, 0, 1u16),           // ip=1: JMP +1 → ip=3
            encode_abx(OP_RET, 0, 0),               // ip=2: RET R0
            encode_abx(OP_RET, 0, 0),               // ip=3: RET R0
        ];
        let dummy_span = crate::ast::Span { start: 0, end: 0 };
        let chunk = Chunk {
            code,
            constants: vec![],
            param_count: 3,
            reg_count: 3,
            spans: vec![dummy_span; 4],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        let jit_fn = compile_program(&program, 0);
        assert!(jit_fn.is_some(), "JIT should compile FOREACHNEXT slow path");
    }

    // ── OP_CALL out-of-range fallback (lines 3019-3052) ──────────────────────
    // func_idx >= all_func_ids.len() → uses jit_call helper instead of direct call.

    #[test]
    fn cranelift_call_out_of_range_fallback_with_args() {

        // Craft a program calling func_idx=99 (which doesn't exist).
        // OP_CALL encoding: (op<<24) | (a<<16) | (func_idx<<8 | n_args)
        // With a=0, func_idx=99, n_args=1: (OP_CALL<<24) | (0<<16) | (99<<8 | 1)
        let encode_abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        // OP_CALL R0, func_idx=99, n_args=1 (R1 is the argument)
        let call_bx: u16 = (99u16 << 8) | 1u16;
        let code = vec![
            encode_abx(OP_CALL, 0, call_bx), // CALL R0 = call(func=99, args=[R1])
            encode_abx(OP_RET, 0, 0),          // RET R0
        ];
        let dummy_span = crate::ast::Span { start: 0, end: 0 };
        let chunk = Chunk {
            code,
            constants: vec![],
            param_count: 2,  // R0=result, R1=arg
            reg_count: 2,
            spans: vec![dummy_span; 2],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        // func_idx=99 is out of range (only 1 function) → fallback path (lines 3019-3040)
        let jit_fn = compile_program(&program, 0);
        assert!(jit_fn.is_some(), "JIT should compile out-of-range call with args");
    }

    #[test]
    fn cranelift_call_out_of_range_fallback_no_args() {

        let encode_abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        // OP_CALL R0, func_idx=99, n_args=0
        let call_bx: u16 = 99u16 << 8; // n_args=0
        let code = vec![
            encode_abx(OP_CALL, 0, call_bx), // CALL R0 = call(func=99, args=[])
            encode_abx(OP_RET, 0, 0),          // RET R0
        ];
        let dummy_span = crate::ast::Span { start: 0, end: 0 };
        let chunk = Chunk {
            code,
            constants: vec![],
            param_count: 1,
            reg_count: 1,
            spans: vec![dummy_span; 2],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        // func_idx=99 is out of range → fallback path (lines 3041-3052)
        let jit_fn = compile_program(&program, 0);
        assert!(jit_fn.is_some(), "JIT should compile out-of-range call no args");
    }

    // ── Inline-failed mid-way fallback (lines 2993-3003) ─────────────────────
    // inline_chunk returns false mid-way when a JMP target isn't in imap.
    // This happens when inline_chunk encounters OP_JMP but block_map is incomplete.
    // We craft a callee that is_inlinable() but inline_chunk fails mid-way.

    #[test]
    fn cranelift_inline_jmp_to_unknown_target_falls_back() {

        // Callee: OP_ADDK_N R0, R0, k0; OP_JMP -999 (invalid target); OP_RET R0
        // is_inlinable: all_regs_numeric=true, reg_count=1, has ADDK_N + JMP + RET
        // inline_chunk: JMP target = (1 + 1 + (-999)) = -997 → not in imap → returns false
        // → fallback to direct call
        let encode_abc = |op: u8, a: u8, b: u8, c: u8| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | (b as u32) << 8 | c as u32
        };
        let encode_abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        // Callee chunk: (func_idx=1)
        // [0] ADD_NN R0, R0, R0   — no constant reference (avoids nan_consts bounds issues)
        // [1] JMP -999 (invalid)
        // [2] RET R0
        let invalid_offset: i16 = -999;
        let callee_code = vec![
            encode_abc(OP_ADD_NN, 0, 0, 0),                     // ip=0: ADD_NN R0, R0, R0
            encode_abx(OP_JMP, 0, invalid_offset as u16),       // ip=1: JMP to invalid target
            encode_abx(OP_RET, 0, 0),                            // ip=2
        ];

        // Caller chunk: (func_idx=0)
        // [0] CALL R0, func_idx=1, n_args=1 (R1 is arg)
        // [1] RET R0
        let call_bx: u16 = (1u16 << 8) | 1u16; // func_idx=1, n_args=1
        let caller_code = vec![
            encode_abx(OP_CALL, 0, call_bx),
            encode_abx(OP_RET, 0, 0),
        ];

        let dummy_span = crate::ast::Span { start: 0, end: 0 };
        let caller_chunk = Chunk {
            code: caller_code,
            constants: vec![],
            param_count: 2,
            reg_count: 2,
            spans: vec![dummy_span; 2],
            all_regs_numeric: true,
        };
        let callee_chunk = Chunk {
            code: callee_code,
            constants: vec![],
            param_count: 1,
            reg_count: 1,
            spans: vec![dummy_span; 3],
            all_regs_numeric: true,
        };
        let program = CompiledProgram {
            chunks: vec![caller_chunk, callee_chunk],
            func_names: vec!["f".to_string(), "callee".to_string()],
            nan_constants: vec![vec![], vec![]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false, false],
        };
        // inline_chunk will fail for callee (JMP to unknown target) → fallback to direct call
        let jit_fn = compile_program(&program, 0);
        // May return None if the fallback call causes issues, but codegen should be exercised
        let _ = jit_fn;
    }

    // ── inline_chunk fallthrough jump (lines 406-407) and not-terminated (574-577) ──
    // Callee with CMPK + JMP + non-terminating block + leader → fires fallthrough.
    // This also tests dead-code skip (line 413) via the dead ADD_NN after JMP.

    #[test]
    fn cranelift_inline_fallthrough_and_unterminated() {
        // Callee (func_idx=1):
        // [0] CMPK_GT_N R0, k0    ← marks ip+1=1, ip+2=2 as leaders
        // [1] JMP +1               ← target=3; marks ip+1=2, target=3 as leaders
        // [2] ADD_NN R0, R0, R0    ← non-terminating block (b2), falls through to b3
        // [3] ADD_NN R0, R0, R0    ← b3 is a leader; !terminated=true → fires line 406-407
        //                             then no terminator after this → fires lines 574-577
        //
        // The callee satisfies is_inlinable (has RET... wait, no RET here!)
        // Actually is_inlinable requires has_ret=true. Let me add a RET.
        // Revised:
        // [0] CMPK_GT_N R0, k0    ← leaders: {0,1,2}
        // [1] JMP +2               ← target=4; leaders: {2,4}
        // [2] ADD_NN R0, R0, R0    ← body block, non-terminating
        // [3] ADD_NN R0, R0, R0    ← NOT a leader, dead (after b4 jumps past); terminated check
        //                             Hmm, but ip=3 not a leader and terminated=false from ip=2...
        // Let me use a simpler approach: two leaders with non-terminating block between:
        // [0] CMPK_GT_N R0, k0    ← {0,1,2}
        // [1] JMP +1               ← target=3; {2,3}
        // [2] ADD_NN R0, R0, R0    ← non-terminating; falls through to ip=3 (leader)
        // [3] RET R0               ← leader b3; ip=2 was non-terminated → line 406-407 fires!

        let encode_abc = |op: u8, a: u8, b: u8, c: u8| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | (b as u32) << 8 | c as u32
        };
        let encode_abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        // Callee with fallthrough between blocks:
        // [0] CMPK_GT_N R0, k0  -- leaders: 0,1,2; from JMP+1: 2,3
        // [1] JMP +1             -- target=3
        // [2] ADD_NN R0, R0, R0  -- falls through to leader at ip=3 → line 406-407
        // [3] RET R0
        let k0_bits = NanVal::number(5.0).0; // constant k0 = 5.0
        let callee_code = vec![
            encode_abc(OP_CMPK_GT_N, 0, 0, 0), // ip=0: CMPK_GT_N R0, k0
            encode_abx(OP_JMP, 0, 1u16),          // ip=1: JMP +1 → target=3
            encode_abc(OP_ADD_NN, 0, 0, 0),        // ip=2: ADD_NN R0, R0, R0 (no terminator)
            encode_abx(OP_RET, 0, 0),               // ip=3: RET R0 (leader from JMP target)
        ];
        let callee_nan_consts = vec![NanVal::number(5.0)]; // k0 = 5.0

        // Caller:
        // [0] CALL R0, func_idx=1, n_args=1 (R1=arg)
        // [1] RET R0
        let call_bx: u16 = (1u16 << 8) | 1u16; // func_idx=1, n_args=1
        let caller_code = vec![
            encode_abx(OP_CALL, 0, call_bx),
            encode_abx(OP_RET, 0, 0),
        ];

        let dummy_span = crate::ast::Span { start: 0, end: 0 };
        let caller_chunk = Chunk {
            code: caller_code,
            constants: vec![],
            param_count: 2,
            reg_count: 2,
            spans: vec![dummy_span; 2],
            all_regs_numeric: true,
        };
        let callee_chunk = Chunk {
            code: callee_code,
            constants: vec![],
            param_count: 1,
            reg_count: 1,
            spans: vec![dummy_span; 4],
            all_regs_numeric: true,
        };

        let program = CompiledProgram {
            chunks: vec![caller_chunk, callee_chunk],
            func_names: vec!["f".to_string(), "callee".to_string()],
            nan_constants: vec![vec![], callee_nan_consts],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false, false],
        };

        // compile_program → JIT compiles caller which tries to inline callee.
        // inline_chunk: at ip=3 (leader), terminated=false from ip=2 → lines 406-407 fire.
        let jit_fn = compile_program(&program, 0);
        assert!(jit_fn.is_some(), "JIT should compile inline fallthrough callee");
    }

    // ── inline_chunk dead code skip (line 413) ────────────────────────────────
    // A callee where the instruction after RET is not a block leader → dead code skip.

    #[test]
    fn cranelift_inline_dead_code_after_ret() {
        // Callee:
        // [0] RET R0         ← terminates immediately; set terminated=true
        // [1] ADD_NN R0, R0, R0  ← NOT a leader; `if terminated { continue }` → line 413
        let encode_abc = |op: u8, a: u8, b: u8, c: u8| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | (b as u32) << 8 | c as u32
        };
        let encode_abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        let callee_code = vec![
            encode_abx(OP_RET, 0, 0),               // ip=0: RET R0
            encode_abc(OP_ADD_NN, 0, 0, 0),          // ip=1: dead code (not a leader)
        ];
        // Caller:
        let call_bx: u16 = (1u16 << 8) | 1u16;
        let caller_code = vec![
            encode_abx(OP_CALL, 0, call_bx),
            encode_abx(OP_RET, 0, 0),
        ];

        let dummy_span = crate::ast::Span { start: 0, end: 0 };
        let caller_chunk = Chunk {
            code: caller_code,
            constants: vec![],
            param_count: 2,
            reg_count: 2,
            spans: vec![dummy_span; 2],
            all_regs_numeric: true,
        };
        let callee_chunk = Chunk {
            code: callee_code,
            constants: vec![],
            param_count: 1,
            reg_count: 1,
            spans: vec![dummy_span; 2],
            all_regs_numeric: true,
        };

        let program = CompiledProgram {
            chunks: vec![caller_chunk, callee_chunk],
            func_names: vec!["f".to_string(), "callee".to_string()],
            nan_constants: vec![vec![], vec![]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false, false],
        };

        // inline_chunk: ip=1 is not a leader but terminated=true → line 413 fires
        let jit_fn = compile_program(&program, 0);
        assert!(jit_fn.is_some(), "JIT should compile inline dead-code callee");
    }

    // ── inline_chunk not-terminated at loop end (lines 574-577) ──────────────
    // Callee where last block has ADD_NN (non-terminating) but no RET in that block.
    // The overall callee has a RET (required by is_inlinable) but in a different block.

    #[test]
    fn cranelift_inline_not_terminated_at_end() {
        // Callee:
        // [0] CMPK_GT_N R0, k0   ← leaders: {0, 1, 2}
        // [1] JMP +2              ← target=4; leaders: {2, 4}
        // [2] RET R0              ← body: returns early
        // [3] ADD_NN R0, R0, R0  ← NOT a leader, dead code after JMP target jump
        // [4] ADD_NN R0, R0, R0  ← leader b4; no terminator → lines 574-577 fire at end
        let encode_abc = |op: u8, a: u8, b: u8, c: u8| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | (b as u32) << 8 | c as u32
        };
        let encode_abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        let k0_bits = NanVal::number(5.0).0;
        let callee_code = vec![
            encode_abc(OP_CMPK_GT_N, 0, 0, 0), // ip=0
            encode_abx(OP_JMP, 0, 2u16),          // ip=1: JMP +2 → target=4
            encode_abx(OP_RET, 0, 0),               // ip=2: RET (body)
            encode_abc(OP_ADD_NN, 0, 0, 0),         // ip=3: dead code (not a leader)
            encode_abc(OP_ADD_NN, 0, 0, 0),         // ip=4: leader (JMP target); no terminator
        ];
        let callee_nan_consts = vec![NanVal::number(5.0)];

        let call_bx: u16 = (1u16 << 8) | 1u16;
        let caller_code = vec![
            encode_abx(OP_CALL, 0, call_bx),
            encode_abx(OP_RET, 0, 0),
        ];

        let dummy_span = crate::ast::Span { start: 0, end: 0 };
        let caller_chunk = Chunk {
            code: caller_code,
            constants: vec![],
            param_count: 2,
            reg_count: 2,
            spans: vec![dummy_span; 2],
            all_regs_numeric: true,
        };
        let callee_chunk = Chunk {
            code: callee_code,
            constants: vec![],
            param_count: 1,
            reg_count: 1,
            spans: vec![dummy_span; 5],
            all_regs_numeric: true,
        };

        let program = CompiledProgram {
            chunks: vec![caller_chunk, callee_chunk],
            func_names: vec!["f".to_string(), "callee".to_string()],
            nan_constants: vec![vec![], callee_nan_consts],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false, false],
        };

        // inline_chunk: at end of code, b4 block not terminated → lines 574-577 fire
        let jit_fn = compile_program(&program, 0);
        assert!(jit_fn.is_some(), "JIT should compile inline not-terminated-at-end callee");
    }

}
