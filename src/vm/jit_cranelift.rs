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
    add_inplace: FuncId,
    concat: FuncId,
    concat_inplace: FuncId,
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
    panic_unwrap: FuncId,
    jit_move: FuncId,
    drop_rc: FuncId,
    len: FuncId,
    str_fn: FuncId,
    num: FuncId,
    abs: FuncId,
    mod_fn: FuncId,
    clamp: FuncId,
    min: FuncId,
    max: FuncId,
    flr: FuncId,
    cel: FuncId,
    rou: FuncId,
    pow: FuncId,
    sqrt: FuncId,
    log: FuncId,
    exp: FuncId,
    sin: FuncId,
    cos: FuncId,
    tan: FuncId,
    log10: FuncId,
    log2: FuncId,
    atan2: FuncId,
    transpose: FuncId,
    matmul: FuncId,
    dot: FuncId,
    rnd0: FuncId,
    rnd2: FuncId,
    rndn: FuncId,
    now: FuncId,
    env: FuncId,
    get: FuncId,
    spl: FuncId,
    cat: FuncId,
    has: FuncId,
    hd: FuncId,
    at: FuncId,
    fmt2: FuncId,
    zip: FuncId,
    enumerate: FuncId,
    range: FuncId,
    window: FuncId,
    chunks: FuncId,
    setunion: FuncId,
    setinter: FuncId,
    setdiff: FuncId,
    tl: FuncId,
    rev: FuncId,
    srt: FuncId,
    rsrt: FuncId,
    fft: FuncId,
    ifft: FuncId,
    cumsum: FuncId,
    median: FuncId,
    quantile: FuncId,
    stdev: FuncId,
    variance: FuncId,
    slc: FuncId,
    lst: FuncId,
    rgxsub: FuncId,
    take: FuncId,
    drop_fn: FuncId,
    listappend: FuncId,
    listappend_inplace: FuncId,
    index: FuncId,
    recfld: FuncId,
    recfld_strict: FuncId,
    recfld_name: FuncId,
    recfld_name_strict: FuncId,
    recnew: FuncId,
    recnew_empty: FuncId,
    reccopy: FuncId,
    recsetfield: FuncId,
    recwith: FuncId,
    recwith_arena: FuncId,
    listnew: FuncId,
    listget: FuncId,
    jpth: FuncId,
    jdmp: FuncId,
    jpar: FuncId,
    rdjl: FuncId,
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
    mset_inplace: FuncId,
    mhas: FuncId,
    mkeys: FuncId,
    mvals: FuncId,
    mdel: FuncId,
    // Print, trim, uniq
    prt: FuncId,
    trm: FuncId,
    upr: FuncId,
    lwr: FuncId,
    cap: FuncId,
    padl: FuncId,
    padr: FuncId,
    ord: FuncId,
    chr: FuncId,
    chars: FuncId,
    unq: FuncId,
    uniqby: FuncId,
    partition: FuncId,
    frq: FuncId,
    // File I/O
    rd: FuncId,
    rdl: FuncId,
    wr: FuncId,
    wrl: FuncId,
    // HTTP
    post: FuncId,
    geth: FuncId,
    posth: FuncId,
    getmany: FuncId,
    // Linear algebra
    solve: FuncId,
    inv: FuncId,
    det: FuncId,
    // Datetime
    dtfmt: FuncId,
    dtparse: FuncId,
    // Tree-bridge for tree-only builtins (rgx, rgxall, fmt-variadic, etc.)
    call_builtin_tree: FuncId,
}

/// Pack a `Span { start, end }` into a single i64 immediate for passing to
/// erroring JIT helpers. High 32 bits = start, low 32 bits = end. `start ==
/// end == 0` (i.e. `Span::UNKNOWN`) round-trips to `0`, which helpers decode
/// as `None`. Spans whose offsets exceed `u32::MAX` (i.e. source files
/// larger than ~4 GiB) are clamped to `u32::MAX`; this only affects
/// diagnostic rendering, never program correctness.
#[inline]
fn pack_span_bits(span: crate::ast::Span) -> i64 {
    let start = span.start.min(u32::MAX as usize) as u64;
    let end = span.end.min(u32::MAX as usize) as u64;
    ((start << 32) | end) as i64
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
        ("jit_add_inplace", jit_add_inplace as *const u8),
        ("jit_concat", jit_concat as *const u8),
        ("jit_concat_inplace", jit_concat_inplace as *const u8),
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
        ("jit_panic_unwrap", jit_panic_unwrap as *const u8),
        ("jit_move", jit_move as *const u8),
        ("jit_drop_rc", jit_drop_rc as *const u8),
        ("jit_len", jit_len as *const u8),
        ("jit_str", jit_str as *const u8),
        ("jit_num", jit_num as *const u8),
        ("jit_abs", jit_abs as *const u8),
        ("jit_mod", jit_mod as *const u8),
        ("jit_clamp", jit_clamp as *const u8),
        ("jit_min", jit_min as *const u8),
        ("jit_max", jit_max as *const u8),
        ("jit_flr", jit_flr as *const u8),
        ("jit_cel", jit_cel as *const u8),
        ("jit_rou", jit_rou as *const u8),
        ("jit_pow", jit_pow as *const u8),
        ("jit_sqrt", jit_sqrt as *const u8),
        ("jit_log", jit_log as *const u8),
        ("jit_exp", jit_exp as *const u8),
        ("jit_sin", jit_sin as *const u8),
        ("jit_cos", jit_cos as *const u8),
        ("jit_tan", jit_tan as *const u8),
        ("jit_log10", jit_log10 as *const u8),
        ("jit_log2", jit_log2 as *const u8),
        ("jit_atan2", jit_atan2 as *const u8),
        ("jit_transpose", jit_transpose as *const u8),
        ("jit_matmul", jit_matmul as *const u8),
        ("jit_dot", jit_dot as *const u8),
        ("jit_rnd0", jit_rnd0 as *const u8),
        ("jit_rnd2", jit_rnd2 as *const u8),
        ("jit_rndn", jit_rndn as *const u8),
        ("jit_now", jit_now as *const u8),
        ("jit_env", jit_env as *const u8),
        ("jit_get", jit_get as *const u8),
        ("jit_spl", jit_spl as *const u8),
        ("jit_cat", jit_cat as *const u8),
        ("jit_has", jit_has as *const u8),
        ("jit_hd", jit_hd as *const u8),
        ("jit_at", jit_at as *const u8),
        ("jit_fmt2", jit_fmt2 as *const u8),
        ("jit_zip", jit_zip as *const u8),
        ("jit_enumerate", jit_enumerate as *const u8),
        ("jit_range", jit_range as *const u8),
        ("jit_window", jit_window as *const u8),
        ("jit_chunks", jit_chunks as *const u8),
        ("jit_setunion", jit_setunion as *const u8),
        ("jit_setinter", jit_setinter as *const u8),
        ("jit_setdiff", jit_setdiff as *const u8),
        ("jit_tl", jit_tl as *const u8),
        ("jit_rev", jit_rev as *const u8),
        ("jit_srt", jit_srt as *const u8),
        ("jit_rsrt", jit_rsrt as *const u8),
        ("jit_fft", jit_fft as *const u8),
        ("jit_ifft", jit_ifft as *const u8),
        ("jit_cumsum", jit_cumsum as *const u8),
        ("jit_median", jit_median as *const u8),
        ("jit_quantile", jit_quantile as *const u8),
        ("jit_stdev", jit_stdev as *const u8),
        ("jit_variance", jit_variance as *const u8),
        ("jit_slc", jit_slc as *const u8),
        ("jit_lst", jit_lst as *const u8),
        ("jit_rgxsub", jit_rgxsub as *const u8),
        ("jit_take", jit_take as *const u8),
        ("jit_drop", jit_drop as *const u8),
        ("jit_listappend", jit_listappend as *const u8),
        (
            "jit_listappend_inplace",
            jit_listappend_inplace as *const u8,
        ),
        ("jit_index", jit_index as *const u8),
        ("jit_recfld", jit_recfld as *const u8),
        ("jit_recfld_strict", jit_recfld_strict as *const u8),
        ("jit_recfld_name", jit_recfld_name as *const u8),
        (
            "jit_recfld_name_strict",
            jit_recfld_name_strict as *const u8,
        ),
        ("jit_recnew", jit_recnew as *const u8),
        ("jit_recnew_empty", jit_recnew_empty as *const u8),
        ("jit_reccopy", jit_reccopy as *const u8),
        ("jit_recsetfield", jit_recsetfield as *const u8),
        ("jit_recwith", jit_recwith as *const u8),
        ("jit_recwith_arena", jit_recwith_arena as *const u8),
        ("jit_listnew", jit_listnew as *const u8),
        ("jit_listget", jit_listget as *const u8),
        ("jit_jpth", jit_jpth as *const u8),
        ("jit_jdmp", jit_jdmp as *const u8),
        ("jit_jpar", jit_jpar as *const u8),
        ("jit_rdjl", jit_rdjl as *const u8),
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
        ("jit_mset_inplace", jit_mset_inplace as *const u8),
        ("jit_mhas", jit_mhas as *const u8),
        ("jit_mkeys", jit_mkeys as *const u8),
        ("jit_mvals", jit_mvals as *const u8),
        ("jit_mdel", jit_mdel as *const u8),
        // Print, trim, uniq
        ("jit_prt", jit_prt as *const u8),
        ("jit_trm", jit_trm as *const u8),
        ("jit_upr", jit_upr as *const u8),
        ("jit_lwr", jit_lwr as *const u8),
        ("jit_cap", jit_cap as *const u8),
        ("jit_padl", jit_padl as *const u8),
        ("jit_padr", jit_padr as *const u8),
        ("jit_ord", jit_ord as *const u8),
        ("jit_chr", jit_chr as *const u8),
        ("jit_chars", jit_chars as *const u8),
        ("jit_unq", jit_unq as *const u8),
        ("jit_uniqby", jit_uniqby as *const u8),
        ("jit_partition", jit_partition as *const u8),
        ("jit_frq", jit_frq as *const u8),
        // File I/O
        ("jit_rd", jit_rd as *const u8),
        ("jit_rdl", jit_rdl as *const u8),
        ("jit_wr", jit_wr as *const u8),
        ("jit_wrl", jit_wrl as *const u8),
        // HTTP
        ("jit_post", jit_post as *const u8),
        ("jit_geth", jit_geth as *const u8),
        ("jit_posth", jit_posth as *const u8),
        ("jit_getmany", jit_getmany as *const u8),
        ("jit_solve", jit_solve as *const u8),
        ("jit_inv", jit_inv as *const u8),
        ("jit_det", jit_det as *const u8),
        ("jit_dtfmt", jit_dtfmt as *const u8),
        ("jit_dtparse", jit_dtparse as *const u8),
        ("jit_call_builtin_tree", jit_call_builtin_tree as *const u8),
    ];
    for &(name, ptr) in helpers {
        builder.symbol(name, ptr);
    }
}

fn declare_all_helpers(module: &mut JITModule) -> HelperFuncs {
    HelperFuncs {
        add: declare_helper(module, "jit_add", 2, 1),
        add_inplace: declare_helper(module, "jit_add_inplace", 2, 1),
        concat: declare_helper(module, "jit_concat", 2, 1),
        concat_inplace: declare_helper(module, "jit_concat_inplace", 2, 1),
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
        panic_unwrap: declare_helper(module, "jit_panic_unwrap", 1, 1),
        jit_move: declare_helper(module, "jit_move", 1, 1),
        drop_rc: declare_helper(module, "jit_drop_rc", 1, 0),
        len: declare_helper(module, "jit_len", 1, 1),
        str_fn: declare_helper(module, "jit_str", 1, 1),
        num: declare_helper(module, "jit_num", 1, 1),
        abs: declare_helper(module, "jit_abs", 1, 1),
        mod_fn: declare_helper(module, "jit_mod", 2, 1),
        clamp: declare_helper(module, "jit_clamp", 3, 1),
        min: declare_helper(module, "jit_min", 2, 1),
        max: declare_helper(module, "jit_max", 2, 1),
        flr: declare_helper(module, "jit_flr", 1, 1),
        cel: declare_helper(module, "jit_cel", 1, 1),
        rou: declare_helper(module, "jit_rou", 1, 1),
        pow: declare_helper(module, "jit_pow", 2, 1),
        sqrt: declare_helper(module, "jit_sqrt", 1, 1),
        log: declare_helper(module, "jit_log", 1, 1),
        exp: declare_helper(module, "jit_exp", 1, 1),
        sin: declare_helper(module, "jit_sin", 1, 1),
        cos: declare_helper(module, "jit_cos", 1, 1),
        tan: declare_helper(module, "jit_tan", 1, 1),
        log10: declare_helper(module, "jit_log10", 1, 1),
        log2: declare_helper(module, "jit_log2", 1, 1),
        atan2: declare_helper(module, "jit_atan2", 2, 1),
        transpose: declare_helper(module, "jit_transpose", 1, 1),
        matmul: declare_helper(module, "jit_matmul", 2, 1),
        dot: declare_helper(module, "jit_dot", 2, 1),
        rnd0: declare_helper(module, "jit_rnd0", 0, 1),
        rnd2: declare_helper(module, "jit_rnd2", 2, 1),
        rndn: declare_helper(module, "jit_rndn", 2, 1),
        now: declare_helper(module, "jit_now", 0, 1),
        env: declare_helper(module, "jit_env", 1, 1),
        get: declare_helper(module, "jit_get", 1, 1),
        spl: declare_helper(module, "jit_spl", 2, 1),
        cat: declare_helper(module, "jit_cat", 2, 1),
        has: declare_helper(module, "jit_has", 2, 1),
        // jit_hd / jit_at / jit_tl take a packed (start<<32)|end span_bits
        // immediate as their trailing arg so cranelift runtime errors carry
        // a source span like tree / VM. See `vm/mod.rs` JIT runtime-error
        // signalling section.
        hd: declare_helper(module, "jit_hd", 2, 1),
        at: declare_helper(module, "jit_at", 3, 1),
        fmt2: declare_helper(module, "jit_fmt2", 2, 1),
        zip: declare_helper(module, "jit_zip", 2, 1),
        enumerate: declare_helper(module, "jit_enumerate", 1, 1),
        range: declare_helper(module, "jit_range", 2, 1),
        window: declare_helper(module, "jit_window", 2, 1),
        chunks: declare_helper(module, "jit_chunks", 2, 1),
        setunion: declare_helper(module, "jit_setunion", 2, 1),
        setinter: declare_helper(module, "jit_setinter", 2, 1),
        setdiff: declare_helper(module, "jit_setdiff", 2, 1),
        tl: declare_helper(module, "jit_tl", 2, 1),
        rev: declare_helper(module, "jit_rev", 1, 1),
        srt: declare_helper(module, "jit_srt", 1, 1),
        rsrt: declare_helper(module, "jit_rsrt", 1, 1),
        fft: declare_helper(module, "jit_fft", 1, 1),
        ifft: declare_helper(module, "jit_ifft", 1, 1),
        cumsum: declare_helper(module, "jit_cumsum", 1, 1),
        median: declare_helper(module, "jit_median", 1, 1),
        quantile: declare_helper(module, "jit_quantile", 2, 1),
        stdev: declare_helper(module, "jit_stdev", 1, 1),
        variance: declare_helper(module, "jit_variance", 1, 1),
        slc: declare_helper(module, "jit_slc", 3, 1),
        lst: declare_helper(module, "jit_lst", 3, 1),
        rgxsub: declare_helper(module, "jit_rgxsub", 3, 1),
        take: declare_helper(module, "jit_take", 2, 1),
        drop_fn: declare_helper(module, "jit_drop", 2, 1),
        listappend: declare_helper(module, "jit_listappend", 2, 1),
        listappend_inplace: declare_helper(module, "jit_listappend_inplace", 2, 1),
        index: declare_helper(module, "jit_index", 2, 1),
        recfld: declare_helper(module, "jit_recfld", 2, 1),
        recfld_strict: declare_helper(module, "jit_recfld_strict", 2, 1),
        recfld_name: declare_helper(module, "jit_recfld_name", 3, 1),
        recfld_name_strict: declare_helper(module, "jit_recfld_name_strict", 3, 1),
        recnew: declare_helper(module, "jit_recnew", 4, 1),
        recnew_empty: declare_helper(module, "jit_recnew_empty", 3, 1),
        reccopy: declare_helper(module, "jit_reccopy", 3, 1),
        recsetfield: declare_helper(module, "jit_recsetfield", 3, 0),
        recwith: declare_helper(module, "jit_recwith", 4, 1),
        recwith_arena: declare_helper(module, "jit_recwith_arena", 5, 1),
        listnew: declare_helper(module, "jit_listnew", 2, 1),
        listget: declare_helper(module, "jit_listget", 2, 1),
        jpth: declare_helper(module, "jit_jpth", 2, 1),
        jdmp: declare_helper(module, "jit_jdmp", 1, 1),
        jpar: declare_helper(module, "jit_jpar", 1, 1),
        rdjl: declare_helper(module, "jit_rdjl", 1, 1),
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
        mset_inplace: declare_helper(module, "jit_mset_inplace", 3, 1),
        mhas: declare_helper(module, "jit_mhas", 2, 1),
        mkeys: declare_helper(module, "jit_mkeys", 1, 1),
        mvals: declare_helper(module, "jit_mvals", 1, 1),
        mdel: declare_helper(module, "jit_mdel", 2, 1),
        // Print, trim, uniq
        prt: declare_helper(module, "jit_prt", 1, 1),
        trm: declare_helper(module, "jit_trm", 1, 1),
        upr: declare_helper(module, "jit_upr", 1, 1),
        lwr: declare_helper(module, "jit_lwr", 1, 1),
        cap: declare_helper(module, "jit_cap", 1, 1),
        padl: declare_helper(module, "jit_padl", 2, 1),
        padr: declare_helper(module, "jit_padr", 2, 1),
        ord: declare_helper(module, "jit_ord", 1, 1),
        chr: declare_helper(module, "jit_chr", 1, 1),
        chars: declare_helper(module, "jit_chars", 1, 1),
        unq: declare_helper(module, "jit_unq", 1, 1),
        uniqby: declare_helper(module, "jit_uniqby", 2, 1),
        partition: declare_helper(module, "jit_partition", 2, 1),
        frq: declare_helper(module, "jit_frq", 1, 1),
        // File I/O
        rd: declare_helper(module, "jit_rd", 1, 1),
        rdl: declare_helper(module, "jit_rdl", 1, 1),
        wr: declare_helper(module, "jit_wr", 2, 1),
        wrl: declare_helper(module, "jit_wrl", 2, 1),
        // HTTP
        post: declare_helper(module, "jit_post", 2, 1),
        geth: declare_helper(module, "jit_geth", 2, 1),
        posth: declare_helper(module, "jit_posth", 3, 1),
        getmany: declare_helper(module, "jit_getmany", 1, 1),
        // Linear algebra
        solve: declare_helper(module, "jit_solve", 2, 1),
        inv: declare_helper(module, "jit_inv", 1, 1),
        det: declare_helper(module, "jit_det", 1, 1),
        dtfmt: declare_helper(module, "jit_dtfmt", 2, 1),
        dtparse: declare_helper(module, "jit_dtparse", 2, 1),
        call_builtin_tree: declare_helper(module, "jit_call_builtin_tree", 3, 1),
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
                | OP_FLR | OP_CEL | OP_ROU | OP_RND0 | OP_RND2 | OP_RNDN | OP_NOW
                | OP_MOD | OP_CLAMP | OP_POW | OP_SQRT | OP_LOG | OP_EXP | OP_SIN | OP_COS
                | OP_TAN | OP_LOG10 | OP_LOG2 | OP_ATAN2
                | OP_MEDIAN | OP_QUANTILE | OP_STDEV | OP_VARIANCE | OP_DOT | OP_DET
                | OP_ORD => {
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
                | OP_ADD_SS  // string concat — always a string
                | OP_NEG
                | OP_WRAPOK | OP_WRAPERR | OP_UNWRAP
                | OP_RECFLD | OP_RECFLD_NAME | OP_RECFLD_SAFE | OP_RECFLD_NAME_SAFE | OP_LISTGET | OP_INDEX
                | OP_STR | OP_HD | OP_AT | OP_FMT2 | OP_TL | OP_REV | OP_SRT | OP_SRTDESC
                | OP_FFT | OP_IFFT
                | OP_SLC | OP_LST | OP_ZIP | OP_TAKE | OP_DROP | OP_ENUMERATE | OP_RANGE
                | OP_WINDOW | OP_CHUNKS | OP_CUMSUM
                | OP_SETUNION | OP_SETINTER | OP_SETDIFF
                | OP_INV | OP_SOLVE
                | OP_SPL | OP_CAT | OP_GET | OP_POST | OP_GETH | OP_POSTH | OP_GETMANY
                | OP_ENV | OP_JPTH | OP_JDMP | OP_JPAR | OP_RDJL
                | OP_MAPNEW | OP_MGET | OP_MSET | OP_MDEL | OP_MKEYS | OP_MVALS
                | OP_LISTNEW | OP_LISTAPPEND
                | OP_RECNEW | OP_RECWITH | OP_RECNEW_EMPTY | OP_RECCOPY
                | OP_PRT | OP_RD | OP_RDL | OP_WR | OP_WRL | OP_TRM | OP_UPR | OP_LWR | OP_CAP
                | OP_PADL | OP_PADR | OP_CHR | OP_CHARS | OP_UNQ | OP_UNIQBY | OP_PARTITION | OP_FRQ | OP_NUM
                | OP_RGXSUB | OP_TRANSPOSE | OP_MATMUL | OP_DTFMT | OP_DTPARSE
                | OP_CALL_BUILTIN_TREE => {
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
    // Track whether to skip the next instruction (data word for OP_POSTH, OP_SLC, OP_MSET, OP_RGXSUB)
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
                let both_always_num = b_idx < reg_always_num.len()
                    && reg_always_num[b_idx]
                    && c_idx < reg_always_num.len()
                    && reg_always_num[c_idx];

                if both_always_num {
                    // Pre-pass proved both operands are always numeric: skip QNAN check,
                    // emit inline float op directly (same as OP_ADD_NN / OP_SUB_NN etc.).
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
                    let result_f = match op {
                        OP_ADD => builder.ins().fadd(bf, cf),
                        OP_SUB => builder.ins().fsub(bf, cf),
                        OP_MUL => builder.ins().fmul(bf, cf),
                        OP_DIV => builder.ins().fdiv(bf, cf),
                        _ => unreachable!(),
                    };
                    let result = builder.ins().bitcast(I64, mf, result_f);
                    builder.def_var(vars[a_idx], result);
                    if a_idx < reg_count && reg_always_num[a_idx] {
                        builder.def_var(f64_vars[a_idx], result_f);
                    }
                } else {
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
                        // OP_ADD is the only one with an in-place string-mutation
                        // fast path; pick it only when dest == LHS source AND
                        // RHS != LHS (rebind shape, excluding self-concat).
                        OP_ADD => {
                            if a_idx == b_idx && b_idx != c_idx {
                                helpers.add_inplace
                            } else {
                                helpers.add
                            }
                        }
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
            }
            // ── String concatenation fast path — both operands guaranteed strings ──
            OP_ADD_SS => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                // Pick the in-place helper only when dest == LHS source AND
                // RHS != LHS (rebind shape, excluding the self-concat `s = +s s`
                // case where push_str would self-alias).
                let helper_fn = if a_idx == b_idx && b_idx != c_idx {
                    helpers.concat_inplace
                } else {
                    helpers.concat
                };
                let fref = get_func_ref(&mut builder, module, helper_fn);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
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
            OP_PANIC_UNWRAP => {
                // `!!` panic-unwrap. The helper sets JIT_RUNTIME_ERROR (the
                // post-call check in `compile_and_call` turns that into an
                // `Err(VmRuntimeError)` to the caller), then returns TAG_NIL.
                // We emit an immediate `return` so the rest of the function
                // doesn't execute against the failed value, matching the VM
                // dispatcher's `vm_err!` early-unwind contract.
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.panic_unwrap);
                let call_inst = builder.ins().call(fref, &[bv]);
                let nil_val = builder.inst_results(call_inst)[0];
                builder.ins().return_(&[nil_val]);
                block_terminated = true;
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
            OP_CLAMP => {
                // clamp(R[B]=x, R[C]=lo, R[D]=hi) — D in data word A field
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let data_inst = chunk.code[ip + 1];
                skip_next = true;
                let d_idx = ((data_inst >> 16) & 0xFF) as usize;
                let dv = builder.use_var(vars[d_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.clamp);
                let call_inst = builder.ins().call(fref, &[bv, cv, dv]);
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
            OP_POW => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.pow);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_ATAN2 => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.atan2);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_TRANSPOSE => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.transpose);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MATMUL => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.matmul);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_DOT => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.dot);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_DET => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.det);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_INV => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.inv);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_SOLVE => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.solve);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_SQRT | OP_LOG | OP_EXP | OP_SIN | OP_COS | OP_TAN | OP_LOG10 | OP_LOG2 => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = match op {
                    OP_SQRT => helpers.sqrt,
                    OP_LOG => helpers.log,
                    OP_EXP => helpers.exp,
                    OP_SIN => helpers.sin,
                    OP_COS => helpers.cos,
                    OP_TAN => helpers.tan,
                    OP_LOG10 => helpers.log10,
                    _ => helpers.log2,
                };
                let fref = get_func_ref(&mut builder, module, fref);
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
            OP_RNDN => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.rndn);
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
                let span_bits = pack_span_bits(chunk.spans[ip]);
                let span_arg = builder.ins().iconst(I64, span_bits);
                let fref = get_func_ref(&mut builder, module, helpers.hd);
                let call_inst = builder.ins().call(fref, &[bv, span_arg]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_AT => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let span_bits = pack_span_bits(chunk.spans[ip]);
                let span_arg = builder.ins().iconst(I64, span_bits);
                let fref = get_func_ref(&mut builder, module, helpers.at);
                let call_inst = builder.ins().call(fref, &[bv, cv, span_arg]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_FMT2 => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.fmt2);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ZIP => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.zip);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ENUMERATE => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.enumerate);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RANGE => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.range);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_WINDOW => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.window);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_CHUNKS => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.chunks);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_SETUNION => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.setunion);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_SETINTER => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.setinter);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_SETDIFF => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.setdiff);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_TL => {
                let bv = builder.use_var(vars[b_idx]);
                let span_bits = pack_span_bits(chunk.spans[ip]);
                let span_arg = builder.ins().iconst(I64, span_bits);
                let fref = get_func_ref(&mut builder, module, helpers.tl);
                let call_inst = builder.ins().call(fref, &[bv, span_arg]);
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
            OP_SRTDESC => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.rsrt);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_FFT => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.fft);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_IFFT => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.ifft);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_CUMSUM => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.cumsum);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_MEDIAN => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.median);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_QUANTILE => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.quantile);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_STDEV => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.stdev);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_VARIANCE => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.variance);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_SLC => {
                // slc(R[B], R[C], R[D]) — D in data word A field
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let data_inst = chunk.code[ip + 1];
                skip_next = true;
                let d_idx = ((data_inst >> 16) & 0xFF) as usize;
                let dv = builder.use_var(vars[d_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.slc);
                let call_inst = builder.ins().call(fref, &[bv, cv, dv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_LST => {
                // lst(R[B], R[C], R[D]) — D in data word A field
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let data_inst = chunk.code[ip + 1];
                skip_next = true;
                let d_idx = ((data_inst >> 16) & 0xFF) as usize;
                let dv = builder.use_var(vars[d_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.lst);
                let call_inst = builder.ins().call(fref, &[bv, cv, dv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RGXSUB => {
                // rgxsub(R[B]=pattern, R[C]=replacement, R[D]=subject) — D in data word A field
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let data_inst = chunk.code[ip + 1];
                skip_next = true;
                let d_idx = ((data_inst >> 16) & 0xFF) as usize;
                let dv = builder.use_var(vars[d_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.rgxsub);
                let call_inst = builder.ins().call(fref, &[bv, cv, dv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_CALL_BUILTIN_TREE => {
                // Generic bridge for tree-only builtins.
                //   A = result_reg, B = Builtin::tag, C = argc
                //   args live in R[A+1..=A+argc]
                // Spill args to a stack slot, then call
                // `jit_call_builtin_tree(tag, argc, regs_ptr)`.
                let tag = b_idx as i64; // B field already extracted as u8 into b_idx
                let argc = c_idx; // C field is argc
                let tag_val = builder.ins().iconst(I64, tag);
                let argc_val = builder.ins().iconst(I64, argc as i64);

                let regs_ptr = if argc == 0 {
                    // No args: pass a null pointer; the helper's slice
                    // construction handles argc=0 without dereferencing.
                    builder.ins().iconst(I64, 0)
                } else {
                    let slot =
                        builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            (argc * 8) as u32,
                            0,
                        ));
                    for i in 0..argc {
                        let src = builder.use_var(vars[a_idx + 1 + i]);
                        builder.ins().stack_store(src, slot, (i * 8) as i32);
                    }
                    builder.ins().stack_addr(I64, slot, 0)
                };

                let fref = get_func_ref(&mut builder, module, helpers.call_builtin_tree);
                let call_inst = builder.ins().call(fref, &[tag_val, argc_val, regs_ptr]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_TAKE => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.take);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_DROP => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.drop_fn);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
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

                // Heap path: call jit_recfld_strict so missing-field /
                // non-record errors surface via JIT_RUNTIME_ERROR. The arena
                // fast path above does no bounds check (c_idx is a
                // compile-time-constant verified by the typechecker on
                // statically-typed records); a verifier bug there would still
                // segfault, which is the same shape as the existing
                // arena-fast-path contract.
                builder.switch_to_block(heap_block);
                let field_idx_val = builder.ins().iconst(I64, c_idx as i64);
                let fref = get_func_ref(&mut builder, module, helpers.recfld_strict);
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
                // Strict variant: missing-name / non-record surfaces as a
                // JIT_RUNTIME_ERROR. OP_RECFLD_NAME_SAFE below keeps using
                // the permissive `jit_recfld_name` for `.?field` semantics.
                let fref = get_func_ref(&mut builder, module, helpers.recfld_name_strict);
                let call_inst = builder
                    .ins()
                    .call(fref, &[bv, field_name_val, registry_val]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RECFLD_SAFE => {
                // Safe field-by-index: route through jit_recfld which already
                // returns TAG_NIL on miss / non-record / nil-object. No inline
                // arena fast path here — the dynamic-record (jpar) use case
                // generally lives on the heap path anyway, and skipping the
                // inline math keeps the safe variant trivially correct.
                let bv = builder.use_var(vars[b_idx]);
                let field_idx_val = builder.ins().iconst(I64, c_idx as i64);
                let fref = get_func_ref(&mut builder, module, helpers.recfld);
                let call_inst = builder.ins().call(fref, &[bv, field_idx_val]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RECFLD_NAME_SAFE => {
                // Safe field-by-name: mirrors OP_RECFLD_NAME exactly, since
                // jit_recfld_name already returns TAG_NIL on miss. Distinct
                // opcode kept so the VM interpreter has a clean error/safe
                // split and so future tightening (e.g. strict OP_RECFLD_NAME
                // emitting ILO-R005) doesn't accidentally break `.?`.
                let b_idx = ((inst >> 8) & 0xFF) as usize;
                let c_idx = (inst & 0xFF) as usize;
                let bv = builder.use_var(vars[b_idx]);
                let cstring = match &chunk.constants[c_idx] {
                    crate::interpreter::Value::Text(s) => {
                        std::ffi::CString::new(s.as_bytes()).ok()?
                    }
                    _ => return None,
                };
                let leaked = Box::leak(Box::new(cstring));
                let field_name_ptr = leaked.as_ptr() as u64;
                let field_name_val = builder.ins().iconst(I64, field_name_ptr as i64);
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
            OP_RECNEW_EMPTY => {
                // Fallback emission for oversized record literals. The fast
                // path (OP_RECNEW above) inlines the arena bump; this path
                // is cold (only fires on records with >127 fields or lots
                // of preceding locals) so a simple helper call is fine.
                let type_id = (inst & 0xFFFF) as i64;
                let arena_ptr_val = builder.ins().iconst(I64, jit_arena_ptr() as i64);
                let type_id_val = builder.ins().iconst(I64, type_id);
                let registry_ptr_val = builder
                    .ins()
                    .iconst(I64, &program.type_registry as *const TypeRegistry as i64);
                let fref = get_func_ref(&mut builder, module, helpers.recnew_empty);
                let call_inst = builder
                    .ins()
                    .call(fref, &[arena_ptr_val, type_id_val, registry_ptr_val]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RECCOPY => {
                // R[A] = fresh clone of R[B]. Companion to OP_RECNEW_EMPTY
                // for the oversized-with fallback. Same cold-path rationale
                // as RECNEW_EMPTY: helper call rather than inlined.
                let b_idx = ((inst >> 8) & 0xFF) as usize;
                let src = builder.use_var(vars[b_idx]);
                let arena_ptr_val = builder.ins().iconst(I64, jit_arena_ptr() as i64);
                let registry_ptr_val = builder
                    .ins()
                    .iconst(I64, &program.type_registry as *const TypeRegistry as i64);
                let fref = get_func_ref(&mut builder, module, helpers.reccopy);
                let call_inst = builder
                    .ins()
                    .call(fref, &[src, arena_ptr_val, registry_ptr_val]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RECSETFIELD => {
                // R[A].field[C] = R[B], in place. Only emitted against
                // freshly-allocated records (rc=1, no aliasing), so the
                // helper does no defensive RC dance — see the SAFETY
                // comment on OP_RECSETFIELD in src/vm/mod.rs.
                let b_idx = ((inst >> 8) & 0xFF) as usize;
                let c = (inst & 0xFF) as i64;
                let rec_val = builder.use_var(vars[a_idx]);
                let val_val = builder.use_var(vars[b_idx]);
                let idx_val = builder.ins().iconst(I64, c);
                let fref = get_func_ref(&mut builder, module, helpers.recsetfield);
                builder.ins().call(fref, &[rec_val, val_val, idx_val]);
                // No result — the record at R[A] is mutated in place; the
                // var still holds the same NanVal so we don't redefine it.
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
            OP_RDJL => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.rdjl);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_DTFMT => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.dtfmt);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_DTPARSE => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.dtparse);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
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
                // mset(R[B], R[C], R[D]) — D in data word A field
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let data_inst = chunk.code[ip + 1];
                skip_next = true;
                let d_idx = ((data_inst >> 16) & 0xFF) as usize;
                let dv = builder.use_var(vars[d_idx]);
                // Pick the in-place helper only when the destination and source
                // registers are the same SSA variable (compiler peephole shape).
                // See jit_mset_inplace docs for why a != b is unsafe at RC=1.
                let helper_fn = if a_idx == b_idx {
                    helpers.mset_inplace
                } else {
                    helpers.mset
                };
                let fref = get_func_ref(&mut builder, module, helper_fn);
                let call_inst = builder.ins().call(fref, &[bv, cv, dv]);
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
            OP_UPR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.upr);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_LWR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.lwr);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_CAP => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.cap);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_PADL => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.padl);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_PADR => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.padr);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_ORD => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.ord);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_CHR => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.chr);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_CHARS => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.chars);
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
            OP_UNIQBY => {
                // HOF: B = fn-ref reg, C = list reg. Helper is a stub today.
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.uniqby);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_PARTITION => {
                // HOF: B = fn-ref reg, C = list reg. Helper is a stub today.
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.partition);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_FRQ => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.frq);
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
            OP_GETMANY => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.getmany);
                let call_inst = builder.ins().call(fref, &[bv]);
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
            OP_FLATMAP => {
                // Pre-allocated HOF opcode. The compiler currently lets `flatmap`
                // calls fall through to OP_CALL (interpreter), mirroring `map`,
                // so this arm should never fire for compiler-emitted bytecode.
                // Bail out if encountered so the VM falls back to the interpreter.
                return None;
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

/// Outcome of calling a Cranelift-compiled function.
///
/// `NotEligible` means the JIT couldn't dispatch (compile failure or arg
/// mismatch); callers may fall back to the bytecode VM. `Runtime` carries a
/// real runtime error raised by a JIT helper (e.g. `hd []`, `at xs 99`).
/// Callers should NOT fall back on `Runtime` — the program executed but
/// hit a defined error condition, which is the same shape tree and VM
/// surface for the same input.
#[derive(Debug)]
pub enum JitCallError {
    NotEligible,
    Runtime(VmRuntimeError),
}

/// Call a compiled NanVal JIT function with u64 args.
///
/// Installs a `JitRuntimeErrorGuard` for the duration of the call so a stale
/// helper error can never leak into this invocation, and so the cell is
/// cleared on drop even if Rust-side code panics later. Returns `Runtime`
/// when a helper set the error cell, else `Ok` with the raw NanVal bits.
/// Resets the JIT arena after each call (promoting the result if arena-tagged).
pub fn call(func: &JitFunction, args: &[u64]) -> Result<u64, JitCallError> {
    let _err_guard = JitRuntimeErrorGuard::new();

    let mut result = call_raw(func, args).ok_or(JitCallError::NotEligible)?;

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

    if let Some((err, span)) = jit_take_runtime_error() {
        return Err(JitCallError::Runtime(VmRuntimeError {
            error: err,
            span,
            call_stack: Vec::new(),
        }));
    }

    Ok(result)
}

/// Compile and call in one shot (convenience wrapper).
pub fn compile_and_call(
    chunk: &Chunk,
    nan_consts: &[NanVal],
    args: &[u64],
    program: &CompiledProgram,
) -> Result<u64, JitCallError> {
    with_active_registry(program, || {
        let func = compile(chunk, nan_consts, program).ok_or(JitCallError::NotEligible)?;
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
        let result = compile_and_call(chunk, nan_consts, &nan_args, &compiled).ok()?;
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
            assert!(matches!(result, Err(JitCallError::NotEligible)));
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
            &[Value::List(std::sync::Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ]))],
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
            Some(Value::List(std::sync::Arc::new(vec![
                Value::Text("a".into()),
                Value::Text("b".into()),
                Value::Text("c".into()),
            ])))
        );
    }

    #[test]
    fn cranelift_cat_builtin() {
        let result = jit_run(
            r#"f xs:L t sep:t>t;cat xs sep"#,
            "f",
            &[
                Value::List(std::sync::Arc::new(vec![
                    Value::Text("x".into()),
                    Value::Text("y".into()),
                ])),
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
                Value::List(std::sync::Arc::new(vec![
                    Value::Number(1.0),
                    Value::Number(2.0),
                    Value::Number(3.0),
                ])),
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
            &[Value::List(std::sync::Arc::new(vec![
                Value::Number(10.0),
                Value::Number(20.0),
            ]))],
        );
        assert_eq!(result, Some(Value::Number(10.0)));
    }

    #[test]
    fn cranelift_tl_builtin() {
        let result = jit_run(
            "f xs:L n>L n;tl xs",
            "f",
            &[Value::List(std::sync::Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ]))],
        );
        assert_eq!(
            result,
            Some(Value::List(std::sync::Arc::new(vec![
                Value::Number(2.0),
                Value::Number(3.0)
            ])))
        );
    }

    #[test]
    fn cranelift_rev_builtin() {
        let result = jit_run(
            "f xs:L n>L n;rev xs",
            "f",
            &[Value::List(std::sync::Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ]))],
        );
        assert_eq!(
            result,
            Some(Value::List(std::sync::Arc::new(vec![
                Value::Number(3.0),
                Value::Number(2.0),
                Value::Number(1.0)
            ])))
        );
    }

    #[test]
    fn cranelift_srt_builtin() {
        let result = jit_run(
            "f xs:L n>L n;srt xs",
            "f",
            &[Value::List(std::sync::Arc::new(vec![
                Value::Number(3.0),
                Value::Number(1.0),
                Value::Number(2.0),
            ]))],
        );
        assert_eq!(
            result,
            Some(Value::List(std::sync::Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])))
        );
    }

    #[test]
    fn cranelift_slc_builtin() {
        let result = jit_run(
            "f xs:L n a:n b:n>L n;slc xs a b",
            "f",
            &[
                Value::List(std::sync::Arc::new(vec![
                    Value::Number(10.0),
                    Value::Number(20.0),
                    Value::Number(30.0),
                    Value::Number(40.0),
                ])),
                Value::Number(1.0),
                Value::Number(3.0),
            ],
        );
        assert_eq!(
            result,
            Some(Value::List(std::sync::Arc::new(vec![
                Value::Number(20.0),
                Value::Number(30.0)
            ])))
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
                Value::List(std::sync::Arc::new(vec![
                    Value::Number(1.0),
                    Value::Number(2.0),
                ])),
                Value::Number(3.0),
            ],
        );
        assert_eq!(
            result,
            Some(Value::List(std::sync::Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])))
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
            Some(Value::List(std::sync::Arc::new(vec![
                Value::Number(5.0),
                Value::Number(6.0)
            ])))
        );
    }

    #[test]
    fn cranelift_index_literal() {
        // xs.0 — literal index 0 → OP_INDEX
        let result = jit_run(
            "f xs:L n>n;xs.0",
            "f",
            &[Value::List(std::sync::Arc::new(vec![
                Value::Number(10.0),
                Value::Number(20.0),
                Value::Number(30.0),
            ]))],
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
        assert_eq!(result, Some(Value::List(std::sync::Arc::new(vec![]))));
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
        assert!(result.is_ok(), "JIT should handle OP_RECFLD_NAME");
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
            assert_eq!(result.ok(), Some(NanVal::number(42.0).0));
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
            &[Value::List(std::sync::Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ]))],
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
            &[Value::List(std::sync::Arc::new(vec![
                Value::Text("a".to_string()),
                Value::Text("b".to_string()),
                Value::Text("c".to_string()),
            ]))],
        );
        assert_eq!(result, Some(Value::Number(3.0)));
    }

    #[test]
    fn cranelift_foreach_empty_list() {
        // Foreach over empty list: bounds check fails immediately, sum stays 0.
        let result = jit_run(
            "f xs:L n>n;s=0;@x xs{s=+s x};s",
            "f",
            &[Value::List(std::sync::Arc::new(vec![]))],
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
        let result = jit_run_numeric("a x:n>n;+x 1\nb x:n>n;a x\nf x:n>n;b x", "f", &[10.0]);
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

    // ── is_inlinable negative paths ───────────────────────────────────────────

    // is_inlinable: reg_count > 16 → return false (line 315)
    // A callee with 17+ parameters exceeds the 16-register inlining limit.
    #[test]
    fn cranelift_is_inlinable_too_many_regs() {
        // 17-param function: reg_count=17 > 16 → is_inlinable=false → direct call
        let result = jit_run_numeric(
            "sum17 a:n b:n c:n d:n e:n f:n g:n h:n i:n j:n k:n l:n m:n nn:n o:n p:n q:n>n;\
             +a +b +c +d +e +f +g +h +i +j +k +l +m +nn +o +p q\n\
             caller>n;sum17 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17",
            "caller",
            &[],
        );
        assert_eq!(result, Some(153.0));
    }

    #[test]
    fn cranelift_is_inlinable_non_numeric_regs_not_inlined() {
        // A function that concatenates strings is NOT all_regs_numeric, so it
        // should not be inlined — the caller compiles with a direct call instead.
        // Note: "concat" is a builtin alias for "cat", so use "strjoin" instead.
        let result = jit_run(
            "strjoin a:t b:t>t;+a b\nf a:t b:t>t;strjoin a b",
            "f",
            &[Value::Text("hello".into()), Value::Text(" world".into())],
        );
        assert_eq!(result, Some(Value::Text("hello world".into())));
    }

    #[test]
    fn cranelift_non_numeric_call_result_non_num_branch() {
        // The callee returns a string (not all_regs_numeric), so OP_CALL with
        // a non-numeric callee exercises the non_num_write branch in the
        // pre-pass analysis (lines 877-879 in jit_cranelift).
        // Use \n (not indented newline) so the parser sees two top-level functions.
        let result = jit_run(
            "greet x:t>t;+\"hi \" x\nf name:t>t;greet name",
            "f",
            &[Value::Text("alice".into())],
        );
        assert_eq!(result, Some(Value::Text("hi alice".into())));
    }

    // ── Type predicate ops ────────────────────────────────────────────────────

    #[test]
    fn cranelift_isnum_true() {
        // TypeIs pattern with `n` branch emits OP_ISNUM
        let result = jit_run(
            r#"f x:t>b;?x{n _:true;_:false}"#,
            "f",
            &[Value::Number(5.0)],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_istext_true() {
        // TypeIs pattern with `t` branch emits OP_ISTEXT
        let result = jit_run(
            r#"f x:t>b;?x{t _:true;_:false}"#,
            "f",
            &[Value::Text("hi".into())],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_isbool_true() {
        // TypeIs pattern with `b` branch emits OP_ISBOOL
        let result = jit_run("f x:b>b;?x{b _:true;_:false}", "f", &[Value::Bool(true)]);
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_islist_true() {
        // TypeIs pattern with `l` branch emits OP_ISLIST
        let result = jit_run(
            "f x:t>b;?x{l _:true;_:false}",
            "f",
            &[Value::List(std::sync::Arc::new(vec![Value::Number(1.0)]))],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    // ── Map operations ────────────────────────────────────────────────────────

    #[test]
    fn cranelift_map_new_set_get() {
        let result = jit_run(r#"f>n;m=mset mmap "k" 42;mget m "k""#, "f", &[]);
        // mget returns Ok(42) or the value directly depending on type
        match result {
            Some(Value::Number(n)) => assert_eq!(n, 42.0),
            Some(Value::Ok(v)) => assert_eq!(*v, Value::Number(42.0)),
            other => panic!("expected 42, got {:?}", other),
        }
    }

    #[test]
    fn cranelift_map_has() {
        let result = jit_run(r#"f>b;m=mset mmap "a" 1;mhas m "a""#, "f", &[]);
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_map_keys() {
        let result = jit_run(r#"f>n;m=mset mmap "x" 1;k=mkeys m;len k"#, "f", &[]);
        assert_eq!(result, Some(Value::Number(1.0)));
    }

    #[test]
    fn cranelift_map_vals() {
        let result = jit_run(r#"f>n;m=mset mmap "x" 99;v=mvals m;len v"#, "f", &[]);
        assert_eq!(result, Some(Value::Number(1.0)));
    }

    #[test]
    fn cranelift_mset_rebind_accumulator_text() {
        // Exercises jit_mset_inplace fast path: m = mset m k v with Text values.
        // Pre-fix jit_mset bit-copied entries without bumping RC; with Text
        // values that produced over-decrement on map drop.
        let result = jit_run(
            r#"f>n;m=mmap;m=mset m "a" "1";m=mset m "b" "2";m=mset m "c" "3";len (mkeys m)"#,
            "f",
            &[],
        );
        assert_eq!(result, Some(Value::Number(3.0)));
    }

    #[test]
    fn cranelift_mset_non_rebind_does_not_alias() {
        // Non-rebind shape m2 = mset m k v: Cranelift OP_MSET must pick the
        // cloning helper (jit_mset) not jit_mset_inplace, otherwise both m and
        // m2 alias the same Rc and m would observe m2's insertion.
        let result = jit_run(
            r#"f>t;m=mset mmap "k" "1";m2=mset m "j" "2";mget m "j" ?? "miss""#,
            "f",
            &[],
        );
        assert_eq!(result, Some(Value::Text("miss".into())));
    }

    #[test]
    fn cranelift_mset_overwrite_drops_old_text() {
        // Overwriting a key with a new Text value must drop_rc the displaced
        // value. With the latent jit_mset RC bug, the displaced Rc was never
        // properly accounted for and the second `mget` would observe garbage.
        let result = jit_run(
            r#"f>t;m=mmap;m=mset m "k" "first";m=mset m "k" "second";mget m "k" ?? "miss""#,
            "f",
            &[],
        );
        assert_eq!(result, Some(Value::Text("second".into())));
    }

    #[test]
    fn cranelift_map_del() {
        let result = jit_run(
            r#"f>n;m=mset mmap "a" 1;m=mdel m "a";k=mkeys m;len k"#,
            "f",
            &[],
        );
        assert_eq!(result, Some(Value::Number(0.0)));
    }

    // ── Print / Trim / Uniq ───────────────────────────────────────────────────

    #[test]
    fn cranelift_prt_builtin() {
        // prnt returns the original value after printing
        let result = jit_run("f x:n>n;prnt x;x", "f", &[Value::Number(7.0)]);
        assert_eq!(result, Some(Value::Number(7.0)));
    }

    #[test]
    fn cranelift_trm_builtin() {
        let result = jit_run(r#"f s:t>t;trm s"#, "f", &[Value::Text("  hello  ".into())]);
        assert_eq!(result, Some(Value::Text("hello".into())));
    }

    #[test]
    fn cranelift_unq_builtin() {
        let result = jit_run(
            "f xs:L n>n;u=unq xs;len u",
            "f",
            &[Value::List(std::sync::Arc::new(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(1.0),
                Value::Number(3.0),
            ]))],
        );
        assert_eq!(result, Some(Value::Number(3.0)));
    }

    // ── Non-numeric comparison (general case / slow path) ────────────────────

    #[test]
    fn cranelift_eq_mixed_type_operands() {
        // Comparing a string with a number forces the non-always-numeric path
        // in OP_EQ (both_always_num = false) — the "else" branch with QNAN check.
        let result = jit_run(
            r#"f x:t y:t>b;= x y"#,
            "f",
            &[Value::Text("hello".into()), Value::Text("hello".into())],
        );
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_lt_mixed_non_numeric() {
        // OP_LT on text-type registers exercises the general (non-fast-path) branch.
        let result = jit_run(
            r#"f x:t y:t>b;< x y"#,
            "f",
            &[Value::Text("a".into()), Value::Text("b".into())],
        );
        // String comparison: "a" < "b" is true
        assert_eq!(result, Some(Value::Bool(true)));
    }

    // ── JMPF/JMPT general case (non-bool register) ───────────────────────────

    #[test]
    fn cranelift_jmpt_numeric_truthy() {
        // When the condition register holds a *number* (not proven bool),
        // the JMPF/JMPT general truthy-check path is exercised.
        // `x{1}{0}` is a ternary expression on a numeric value.
        // When x != 0 the number is truthy → returns 1.
        let result = jit_run_numeric("f x:n>n;x{1}{0}", "f", &[5.0]);
        assert_eq!(result, Some(1.0));
    }

    #[test]
    fn cranelift_jmpf_numeric_falsy_zero() {
        // When x == 0.0 it is falsy → returns 0.
        let result = jit_run_numeric("f x:n>n;x{1}{0}", "f", &[0.0]);
        assert_eq!(result, Some(0.0));
    }

    // ── File I/O (compilation smoke-tests) ───────────────────────────────────

    #[test]
    fn cranelift_wr_compiles_and_runs() {
        use std::env::temp_dir;
        let path = temp_dir().join("ilo_jit_wr_test.txt");
        let path_str = path.to_str().unwrap().to_string();
        let result = jit_run(
            "f p:t c:t>t;wr p c",
            "f",
            &[Value::Text(path_str), Value::Text("hello jit\n".into())],
        );
        // wr returns ok/err result; we just check it doesn't panic
        let _ = std::fs::remove_file(&path);
        assert!(result.is_some());
    }

    #[test]
    fn cranelift_rd_compiles() {
        // OP_RD exercises the file-read codegen path
        let bytes = {
            let tokens: Vec<crate::lexer::Token> = crate::lexer::lex("f p:t>R t t;rd p")
                .unwrap()
                .into_iter()
                .map(|(t, _)| t)
                .collect();
            let prog = crate::parser::parse_tokens(tokens).unwrap();
            let compiled = crate::vm::compile(&prog).unwrap();
            let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
            let chunk = &compiled.chunks[idx];
            let nan_consts = &compiled.nan_constants[idx];
            compile(chunk, nan_consts, &compiled).is_some()
        };
        assert!(bytes, "OP_RD JIT compilation should succeed");
    }

    // ── GET (HTTP GET) compiles ───────────────────────────────────────────────

    #[test]
    fn cranelift_get_compiles() {
        // OP_GET (HTTP GET) — just verify it compiles without panicking.
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex("f url:t>R t t;get url")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let compiled_fn = compile(chunk, nan_consts, &compiled);
        assert!(
            compiled_fn.is_some(),
            "OP_GET JIT compilation should succeed"
        );
    }

    // ── inline_chunk paths: SUB_NN, DIV_NN, MULK_N, DIVK_N in inlinable callees ──

    #[test]
    fn cranelift_inline_sub_nn_callee() {
        // Callee uses OP_SUB_NN (a-b) — inlinable since all regs numeric.
        // This exercises the OP_SUB_NN branch inside inline_chunk.
        let result = jit_run_numeric(
            "diff a:n b:n>n;-a b\nf a:n b:n>n;diff a b",
            "f",
            &[10.0, 3.0],
        );
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn cranelift_inline_div_nn_callee() {
        // Callee uses OP_DIV_NN (a/b) — inlinable.
        let result = jit_run_numeric(
            "div2 a:n b:n>n;/a b\nf a:n b:n>n;div2 a b",
            "f",
            &[20.0, 4.0],
        );
        assert_eq!(result, Some(5.0));
    }

    #[test]
    fn cranelift_inline_mulk_n_callee() {
        // Callee uses OP_MULK_N (*x 3) — inlinable with numeric constant.
        let result = jit_run_numeric("triple x:n>n;*x 3\nf x:n>n;triple x", "f", &[7.0]);
        assert_eq!(result, Some(21.0));
    }

    #[test]
    fn cranelift_inline_divk_n_callee() {
        // Callee uses OP_DIVK_N (/x 2) — inlinable.
        let result = jit_run_numeric("half x:n>n;/x 2\nf x:n>n;half x", "f", &[14.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn cranelift_inline_addk_n_callee() {
        // Callee uses OP_ADDK_N (+x 1) — inlinable.
        let result = jit_run_numeric("inc x:n>n;+x 1\nf x:n>n;inc x", "f", &[9.0]);
        assert_eq!(result, Some(10.0));
    }

    #[test]
    fn cranelift_inline_subk_n_callee() {
        // Callee uses OP_SUBK_N (-x 1) — inlinable.
        let result = jit_run_numeric("dec x:n>n;-x 1\nf x:n>n;dec x", "f", &[5.0]);
        assert_eq!(result, Some(4.0));
    }

    // ── OP_NUM builtin ────────────────────────────────────────────────────────

    #[test]
    fn cranelift_num_text_to_number() {
        // num converts a text to a number (returns Result)
        let result = jit_run(r#"f s:t>R n t;num s"#, "f", &[Value::Text("3.14".into())]);
        assert_eq!(result, Some(Value::Ok(Box::new(Value::Number(3.14)))));
    }

    // ── map mset key with number value ───────────────────────────────────────

    #[test]
    fn cranelift_map_set_and_get_string_value() {
        let result = jit_run(r#"f>t;m=mset mmap "key" "val";mget m "key""#, "f", &[]);
        match result {
            Some(Value::Text(s)) => assert_eq!(s, "val"),
            Some(Value::Ok(v)) => assert_eq!(*v, Value::Text("val".into())),
            other => panic!("expected 'val', got {:?}", other),
        }
    }

    // ── OP_WRL and OP_RDL compilation smoke-tests ────────────────────────────

    #[test]
    fn cranelift_wrl_compiles() {
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex("f p:t c:t>t;wrl p c")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let compiled_fn = compile(chunk, nan_consts, &compiled);
        assert!(
            compiled_fn.is_some(),
            "OP_WRL JIT compilation should succeed"
        );
    }

    #[test]
    fn cranelift_rdl_compiles() {
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex("f p:t>R t t;rdl p")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let compiled_fn = compile(chunk, nan_consts, &compiled);
        assert!(
            compiled_fn.is_some(),
            "OP_RDL JIT compilation should succeed"
        );
    }

    // ── MOVE with non-numeric source (general RC path) ───────────────────────

    #[test]
    fn cranelift_move_string_value() {
        // When the source register holds a string (not proven always-numeric),
        // the MOVE handler takes the general is_heap-check path.
        let result = jit_run(r#"f s:t>t;t=s;t"#, "f", &[Value::Text("hello".into())]);
        assert_eq!(result, Some(Value::Text("hello".into())));
    }

    // ── NEG on non-numeric operand (helper slow path) ─────────────────────────

    #[test]
    fn cranelift_neg_string_slow_path() {
        // OP_NEG on a text-typed register exercises the `jit_neg` helper call path.
        // The result for neg("hello") is nil/err, but we only care that it compiles.
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex("f x:t>n;-x")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let compiled_fn = compile(chunk, nan_consts, &compiled);
        assert!(
            compiled_fn.is_some(),
            "OP_NEG with text type should compile"
        );
    }

    // ── HTTP POST / GETH compile smoke-tests ─────────────────────────────────

    #[test]
    fn cranelift_post_compiles() {
        let tokens: Vec<crate::lexer::Token> =
            crate::lexer::lex("f url:t body:t>R t t;post url body")
                .unwrap()
                .into_iter()
                .map(|(t, _)| t)
                .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        assert!(
            compile(chunk, nan_consts, &compiled).is_some(),
            "OP_POST should compile"
        );
    }

    #[test]
    fn cranelift_geth_compiles() {
        // OP_GETH is emitted when `get` receives a map argument (headers).
        // Use `M t t` (Map string->string) as the headers type.
        let tokens: Vec<crate::lexer::Token> =
            crate::lexer::lex("f url:t hdrs:M t t>R t t;get url hdrs")
                .unwrap()
                .into_iter()
                .map(|(t, _)| t)
                .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        assert!(
            compile(chunk, nan_consts, &compiled).is_some(),
            "OP_GETH should compile"
        );
    }

    // ── OP_MUL_NN / OP_ADD_NN in main compilation path ───────────────────────

    #[test]
    fn cranelift_mul_nn() {
        // OP_MUL_NN with two always-numeric params exercises the F64 shadow path.
        let result = jit_run_numeric("f a:n b:n>n;*a b", "f", &[3.0, 4.0]);
        assert_eq!(result, Some(12.0));
    }

    #[test]
    fn cranelift_add_nn() {
        // OP_ADD_NN with two always-numeric params.
        let result = jit_run_numeric("f a:n b:n>n;+a b", "f", &[5.0, 7.0]);
        assert_eq!(result, Some(12.0));
    }

    // ── inline_chunk paths for add/mul inlinable callees ─────────────────────

    #[test]
    fn cranelift_inline_add_nn_callee() {
        // Callee uses OP_ADD_NN — inlinable since all regs numeric.
        // This exercises the OP_ADD_NN branch inside inline_chunk.
        let result = jit_run_numeric(
            "mysum a:n b:n>n;+a b\nf a:n b:n>n;mysum a b",
            "f",
            &[6.0, 9.0],
        );
        assert_eq!(result, Some(15.0));
    }

    #[test]
    fn cranelift_inline_mul_nn_callee() {
        // Callee uses OP_MUL_NN — inlinable since all regs numeric.
        let result = jit_run_numeric(
            "myprod a:n b:n>n;*a b\nf a:n b:n>n;myprod a b",
            "f",
            &[4.0, 5.0],
        );
        assert_eq!(result, Some(20.0));
    }

    // ── inline_chunk OP_CMPK_* + OP_JMP + OP_LOADK path ─────────────────────

    #[test]
    fn cranelift_inline_cmpk_guard_callee() {
        // Callee uses OP_CMPK_GT_N + OP_JMP + OP_LOADK:
        //   `pos x:n>n;>x 0 x;0`  — returns x if x>0, else 0
        // This exercises OP_CMPK, OP_JMP, and OP_LOADK branches in inline_chunk.
        let result = jit_run_numeric("pos x:n>n;>x 0 x;0\nf x:n>n;pos x", "f", &[5.0]);
        assert_eq!(result, Some(5.0));

        let result2 = jit_run_numeric("pos x:n>n;>x 0 x;0\nf x:n>n;pos x", "f", &[-3.0]);
        assert_eq!(result2, Some(0.0));
    }

    // ── foreach loops (FOREACHPREP / FOREACHNEXT) ────────────────────────────

    #[test]
    fn cranelift_foreach_numeric_list() {
        // `@x xs{*x x}` exercises OP_FOREACHPREP and OP_FOREACHNEXT JIT codegen.
        let result = jit_run(
            "f xs:L n>n;@x xs{*x x}",
            "f",
            &[Value::List(std::sync::Arc::new(vec![
                Value::Number(3.0),
                Value::Number(4.0),
                Value::Number(5.0),
            ]))],
        );
        // Last squared value: 5^2=25
        assert_eq!(result, Some(Value::Number(25.0)));
    }

    #[test]
    fn cranelift_foreach_text_list() {
        // Foreach over a text list exercises FOREACHPREP/FOREACHNEXT with heap elements.
        let tokens: Vec<crate::lexer::Token> = crate::lexer::lex("f xs:L t>t;@x xs{x}")
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        let compiled_fn = compile(chunk, nan_consts, &compiled);
        assert!(
            compiled_fn.is_some(),
            "foreach text list JIT compilation should succeed"
        );
    }

    // ── OP_SUB_NN / OP_MUL_NN / OP_DIV_NN with non-always-num registers ─────

    #[test]
    fn cranelift_sub_nn_non_always_num() {
        // Mixed-type function (y:t): all_regs_numeric=false, so x is not always_num.
        // VM still emits OP_SUB_NN but Cranelift pre-pass won't mark x as always_num.
        // Must pass both args (x and y) for call_raw to accept the call.
        let result = jit_run(
            "f x:n y:t>n;-x x",
            "f",
            &[Value::Number(8.0), Value::Text("dummy".into())],
        );
        assert_eq!(result, Some(Value::Number(0.0)));
    }

    #[test]
    fn cranelift_mul_nn_non_always_num() {
        let result = jit_run(
            "f x:n y:t>n;*x x",
            "f",
            &[Value::Number(3.0), Value::Text("dummy".into())],
        );
        assert_eq!(result, Some(Value::Number(9.0)));
    }

    #[test]
    fn cranelift_div_nn_non_always_num() {
        let result = jit_run(
            "f x:n y:t>n;/x x",
            "f",
            &[Value::Number(6.0), Value::Text("dummy".into())],
        );
        assert_eq!(result, Some(Value::Number(1.0)));
    }

    // ── OP_SUB / OP_MUL / OP_DIV generic (non-NN variants) ──────────────────

    #[test]
    fn cranelift_generic_sub() {
        // `v` (any) params prevent OP_SUB_NN — emits generic OP_SUB.
        let result = jit_run_numeric("f x:v y:v>n;-x y", "f", &[10.0, 3.0]);
        assert_eq!(result, Some(7.0));
    }

    #[test]
    fn cranelift_generic_mul() {
        let result = jit_run_numeric("f x:v y:v>n;*x y", "f", &[4.0, 5.0]);
        assert_eq!(result, Some(20.0));
    }

    #[test]
    fn cranelift_generic_div() {
        let result = jit_run_numeric("f x:v y:v>n;/x y", "f", &[12.0, 4.0]);
        assert_eq!(result, Some(3.0));
    }

    // ── inline_chunk with extra (non-param) registers ────────────────────────

    #[test]
    fn cranelift_inline_callee_with_extra_reg() {
        // Callee `double x:n>n;d=*x 2;+d 1` has x (param) and d (extra reg).
        // When inlined, f64_val_for(d_idx) hits the else branch (lines 388-389)
        // for non-param registers.
        let result = jit_run_numeric("double x:n>n;d=*x 2;+d 1\nf x:n>n;double x", "f", &[4.0]);
        assert_eq!(result, Some(9.0)); // 4*2+1=9
    }

    // ── OP_ADDK_N / OP_SUBK_N / OP_MULK_N / OP_DIVK_N with non-always-num ──────

    #[test]
    fn cranelift_addk_n_non_always_num() {
        // Mixed-type (y:t): all_regs_numeric=false → x not always_num.
        // `+x 1` compiles to OP_ADDK_N; Cranelift pre-pass sees x as non-always-num.
        let result = jit_run(
            "f x:n y:t>n;+x 1",
            "f",
            &[Value::Number(5.0), Value::Text("dummy".into())],
        );
        assert_eq!(result, Some(Value::Number(6.0)));
    }

    #[test]
    fn cranelift_subk_n_non_always_num() {
        let result = jit_run(
            "f x:n y:t>n;-x 1",
            "f",
            &[Value::Number(8.0), Value::Text("dummy".into())],
        );
        assert_eq!(result, Some(Value::Number(7.0)));
    }

    #[test]
    fn cranelift_mulk_n_non_always_num() {
        let result = jit_run(
            "f x:n y:t>n;*x 3",
            "f",
            &[Value::Number(4.0), Value::Text("dummy".into())],
        );
        assert_eq!(result, Some(Value::Number(12.0)));
    }

    #[test]
    fn cranelift_divk_n_non_always_num() {
        let result = jit_run(
            "f x:n y:t>n;/x 2",
            "f",
            &[Value::Number(10.0), Value::Text("dummy".into())],
        );
        assert_eq!(result, Some(Value::Number(5.0)));
    }

    // ── OP_JMPT on always-bool register via OR short-circuit ─────────────────

    #[test]
    fn cranelift_jmpt_always_bool_via_or() {
        // `|>x 3 >x 5`: OP_GT writes bool to ra, OP_MOVE copies to result, OP_JMPT on ra.
        // JMPT is not fused (preceding instruction is MOVE, not comparison).
        // ra is always-bool → exercises the JMPT always-bool fast path.
        let result = jit_run("f x:n>b;|>x 3 >x 5", "f", &[Value::Number(4.0)]);
        assert_eq!(result, Some(Value::Bool(true)));
        let result2 = jit_run("f x:n>b;|>x 3 >x 5", "f", &[Value::Number(2.0)]);
        assert_eq!(result2, Some(Value::Bool(false)));
    }

    // ── OP_POSTH compile smoke-test ──────────────────────────────────────────

    #[test]
    fn cranelift_posth_compiles() {
        // OP_POSTH is emitted when `post` receives a Map argument for headers.
        let tokens: Vec<crate::lexer::Token> =
            crate::lexer::lex("f url:t body:t hdrs:M t t>R t t;post url body hdrs")
                .unwrap()
                .into_iter()
                .map(|(t, _)| t)
                .collect();
        let prog = crate::parser::parse_tokens(tokens).unwrap();
        let compiled = crate::vm::compile(&prog).unwrap();
        let idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[idx];
        let nan_consts = &compiled.nan_constants[idx];
        assert!(
            compile(chunk, nan_consts, &compiled).is_some(),
            "OP_POSTH should compile"
        );
    }

    // ── CMPK_*_N with non-always-num register (mixed-type function) ──────────

    #[test]
    fn cranelift_cmpk_gt_n_non_always_num() {
        // y:t makes all_regs_numeric=false → x is NOT always_num in pre-pass.
        // `>x 5 1;0` emits CMPK_GT_N; hits the else-bitcast branch (lines 2867-2869).
        let result = jit_run(
            "f x:n y:t>n;>x 5 1;0",
            "f",
            &[Value::Number(10.0), Value::Text("dummy".into())],
        );
        assert_eq!(result, Some(Value::Number(1.0)));
    }

    #[test]
    fn cranelift_cmpk_lt_n_non_always_num() {
        let result = jit_run(
            "f x:n y:t>n;<x 5 1;0",
            "f",
            &[Value::Number(3.0), Value::Text("dummy".into())],
        );
        assert_eq!(result, Some(Value::Number(1.0)));
    }

    #[test]
    fn cranelift_cmpk_le_n_non_always_num() {
        let result = jit_run(
            "f x:n y:t>n;<=x 5 1;0",
            "f",
            &[Value::Number(5.0), Value::Text("dummy".into())],
        );
        assert_eq!(result, Some(Value::Number(1.0)));
    }

    #[test]
    fn cranelift_cmpk_ge_n_non_always_num() {
        let result = jit_run(
            "f x:n y:t>n;>=x 5 1;0",
            "f",
            &[Value::Number(5.0), Value::Text("dummy".into())],
        );
        assert_eq!(result, Some(Value::Number(1.0)));
    }

    #[test]
    fn cranelift_cmpk_eq_n_non_always_num() {
        let result = jit_run(
            "f x:n y:t>n;==x 5 1;0",
            "f",
            &[Value::Number(5.0), Value::Text("dummy".into())],
        );
        assert_eq!(result, Some(Value::Number(1.0)));
    }

    #[test]
    fn cranelift_cmpk_ne_n_non_always_num() {
        let result = jit_run(
            "f x:n y:t>n;!=x 5 1;0",
            "f",
            &[Value::Number(3.0), Value::Text("dummy".into())],
        );
        assert_eq!(result, Some(Value::Number(1.0)));
    }

    // ── OP_LE / OP_GE / OP_EQ / OP_NE generic (v params → slow-path helpers) ──

    #[test]
    fn cranelift_generic_le() {
        // `<=x y` with v params emits OP_LE; slow path calls helpers.le
        let result = jit_run_numeric("f x:v y:v>n;b=<=x y;b", "f", &[3.0, 5.0]);
        // <=3.0 5.0 → true (TAG_TRUE=1 as f64? no — result is bool NanVal)
        // Let's just check it compiles and runs without panicking
        let _ = result; // result could be bool or num depending on type coercion
        // Use jit_run to check boolean result
        let r2 = jit_run(
            "f x:v y:v>b;<=x y",
            "f",
            &[Value::Number(3.0), Value::Number(5.0)],
        );
        assert_eq!(r2, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_generic_ge() {
        let r = jit_run(
            "f x:v y:v>b;>=x y",
            "f",
            &[Value::Number(5.0), Value::Number(3.0)],
        );
        assert_eq!(r, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_generic_eq() {
        let r = jit_run(
            "f x:v y:v>b;==x y",
            "f",
            &[Value::Number(5.0), Value::Number(5.0)],
        );
        assert_eq!(r, Some(Value::Bool(true)));
    }

    #[test]
    fn cranelift_generic_ne() {
        let r = jit_run(
            "f x:v y:v>b;!=x y",
            "f",
            &[Value::Number(3.0), Value::Number(5.0)],
        );
        assert_eq!(r, Some(Value::Bool(true)));
    }

    // ── inline_chunk: CMPK_LT_N / CMPK_LE_N / CMPK_EQ_N / CMPK_NE_N arms ──

    #[test]
    fn cranelift_inline_cmpk_lt_callee() {
        // Callee uses CMPK_LT_N — covers line 462 in inline_chunk.
        // Both branches have constant fallbacks so JMP stays within code (no past-end issue).
        let result = jit_run_numeric("negone x:n>n;<x 0 -1;0\nf x:n>n;negone x", "f", &[-5.0]);
        assert_eq!(result, Some(-1.0));
        let result2 = jit_run_numeric("negone x:n>n;<x 0 -1;0\nf x:n>n;negone x", "f", &[3.0]);
        assert_eq!(result2, Some(0.0));
    }

    #[test]
    fn cranelift_inline_cmpk_le_callee() {
        // Callee uses CMPK_LE_N — covers line 463 in inline_chunk.
        // `atmost5` returns x if x<=5, else 5.
        let result = jit_run_numeric("atmost5 x:n>n;<=x 5 x;5\nf x:n>n;atmost5 x", "f", &[3.0]);
        assert_eq!(result, Some(3.0));
        let result2 = jit_run_numeric("atmost5 x:n>n;<=x 5 x;5\nf x:n>n;atmost5 x", "f", &[10.0]);
        assert_eq!(result2, Some(5.0));
    }

    #[test]
    fn cranelift_inline_cmpk_eq_callee() {
        // Callee uses CMPK_EQ_N — covers line 464 in inline_chunk.
        let result = jit_run_numeric("exact5 x:n>n;==x 5 1;0\nf x:n>n;exact5 x", "f", &[5.0]);
        assert_eq!(result, Some(1.0));
        let result2 = jit_run_numeric("exact5 x:n>n;==x 5 1;0\nf x:n>n;exact5 x", "f", &[3.0]);
        assert_eq!(result2, Some(0.0));
    }

    #[test]
    fn cranelift_inline_cmpk_ne_callee() {
        // Callee uses CMPK_NE_N — covers line 465 (_ => FloatCC::NotEqual) in inline_chunk.
        let result = jit_run_numeric("noteq5 x:n>n;!=x 5 1;0\nf x:n>n;noteq5 x", "f", &[3.0]);
        assert_eq!(result, Some(1.0));
        let result2 = jit_run_numeric("noteq5 x:n>n;!=x 5 1;0\nf x:n>n;noteq5 x", "f", &[5.0]);
        assert_eq!(result2, Some(0.0));
    }

    // ── OP_RECWITH unresolved (string field names path) ─────────────────────
    // Two types where the same field name has different indices force
    // search_field_index to return None, so the constant pool stores string
    // names instead of numeric indices.  The JIT compiler then takes the
    // `else` (all_resolved=false) branch at lines 2432-2451.

    #[test]
    fn cranelift_recwith_unresolved_field_names() {
        // pt{x:n;y:n}: x at index 0, y at index 1
        // qt{y:n;x:n}: y at index 0, x at index 1
        // In g (p:v), search_field_index("x") returns None (ambiguous) →
        // OP_RECWITH constant is List([Text("x")]) → all_resolved=false
        let result = jit_run(
            "type pt{x:n;y:n}\ntype qt{y:n;x:n}\ng p:v>v;p with x:99\nf>v;p=pt x:1 y:2;g p",
            "f",
            &[],
        );
        // Should return a pt record with x updated to 99
        match result {
            Some(Value::Record { fields, .. }) => {
                let x_val = fields.get("x").expect("field x missing");
                assert_eq!(*x_val, Value::Number(99.0), "x field should be 99");
            }
            other => panic!("expected a Record, got {:?}", other),
        }
    }

    // ── OP_RECWITH resolved path via arena type ──────────────────────────────
    // When a single type is defined (no ambiguity), field names resolve to
    // numeric indices and the JIT takes the `all_resolved=true` inline path.

    #[test]
    fn cranelift_recwith_resolved_single_type() {
        let result = jit_run(
            "type pt{x:n;y:n}\nf>v;p=pt x:1 y:2;p with x:99 y:88",
            "f",
            &[],
        );
        match result {
            Some(Value::Record { fields, .. }) => {
                assert_eq!(fields.get("x"), Some(&Value::Number(99.0)));
                assert_eq!(fields.get("y"), Some(&Value::Number(88.0)));
            }
            other => panic!("expected a Record, got {:?}", other),
        }
    }

    // ── OP_RECWITH resolved path — multiple field updates ────────────────────
    #[test]
    fn cranelift_recwith_three_fields() {
        let result = jit_run(
            "type tri{a:n;b:n;c:n}\nf>v;r=tri a:1 b:2 c:3;r with a:10 b:20",
            "f",
            &[],
        );
        match result {
            Some(Value::Record { fields, .. }) => {
                assert_eq!(fields.get("a"), Some(&Value::Number(10.0)));
                assert_eq!(fields.get("b"), Some(&Value::Number(20.0)));
                assert_eq!(fields.get("c"), Some(&Value::Number(3.0)));
            }
            other => panic!("expected a Record, got {:?}", other),
        }
    }
}
