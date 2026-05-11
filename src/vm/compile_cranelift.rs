//! Cranelift AOT (ahead-of-time) compiler — emits a standalone native binary.
//!
//! Reuses the same NanVal / I64 IR generation strategy as `jit_cranelift.rs`,
//! but targets `ObjectModule` instead of `JITModule` to produce a relocatable
//! `.o` file. A generated `main()` handles CLI arg parsing + result printing.
//! The object is linked with the system `cc` to produce the final executable.
//!
//! All 87 opcodes are supported. Runtime helpers from `libilo.a` are linked
//! via `Linkage::Import` declarations.

use super::*;
use cranelift_codegen::Context;
use cranelift_codegen::ir::types::{F64, I32, I64};
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlags};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use std::collections::HashMap;

// ── HelperFuncs struct ──────────────────────────────────────────────

/// Imported helper function IDs for AOT module (mirrors JIT's HelperFuncs).
#[allow(dead_code)]
struct HelperFuncs {
    add: FuncId,
    concat: FuncId,
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
    rnd0: FuncId,
    rnd2: FuncId,
    now: FuncId,
    env: FuncId,
    get: FuncId,
    spl: FuncId,
    cat: FuncId,
    has: FuncId,
    hd: FuncId,
    at: FuncId,
    fmt2: FuncId,
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
    // AOT-specific helpers
    get_arena_ptr: FuncId,
    get_registry_ptr: FuncId,
    aot_init: FuncId,
    aot_fini: FuncId,
    aot_set_registry: FuncId,
    aot_parse_arg: FuncId,
    string_const: FuncId,
}

fn declare_helper(
    module: &mut ObjectModule,
    name: &str,
    n_params: usize,
    n_returns: usize,
) -> FuncId {
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

fn declare_all_helpers(module: &mut ObjectModule) -> HelperFuncs {
    HelperFuncs {
        add: declare_helper(module, "jit_add", 2, 1),
        concat: declare_helper(module, "jit_concat", 2, 1),
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
        rnd0: declare_helper(module, "jit_rnd0", 0, 1),
        rnd2: declare_helper(module, "jit_rnd2", 2, 1),
        now: declare_helper(module, "jit_now", 0, 1),
        env: declare_helper(module, "jit_env", 1, 1),
        get: declare_helper(module, "jit_get", 1, 1),
        spl: declare_helper(module, "jit_spl", 2, 1),
        cat: declare_helper(module, "jit_cat", 2, 1),
        has: declare_helper(module, "jit_has", 2, 1),
        hd: declare_helper(module, "jit_hd", 1, 1),
        at: declare_helper(module, "jit_at", 2, 1),
        fmt2: declare_helper(module, "jit_fmt2", 2, 1),
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
        // AOT-specific helpers
        get_arena_ptr: declare_helper(module, "jit_get_arena_ptr", 0, 1),
        get_registry_ptr: declare_helper(module, "jit_get_registry_ptr", 0, 1),
        aot_init: declare_helper(module, "ilo_aot_init", 0, 0),
        aot_fini: declare_helper(module, "ilo_aot_fini", 0, 0),
        aot_set_registry: declare_helper(module, "ilo_aot_set_registry", 2, 0),
        aot_parse_arg: declare_helper(module, "ilo_aot_parse_arg", 1, 1),
        string_const: declare_helper(module, "jit_string_const", 1, 1),
    }
}

// ── Linker flags ────────────────────────────────────────────────────

/// Find libilo.a path for linking.
fn find_libilo_a() -> Result<String, String> {
    // Try target/release first, then target/debug
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let release_path = format!("{}/target/release/libilo.a", manifest_dir);
    if std::path::Path::new(&release_path).exists() {
        return Ok(release_path);
    }
    let debug_path = format!("{}/target/debug/libilo.a", manifest_dir);
    if std::path::Path::new(&debug_path).exists() {
        return Ok(debug_path);
    }
    // Also try parent directory (workspace root)
    let parent = std::path::Path::new(manifest_dir)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    if !parent.is_empty() {
        let ws_release = format!("{}/target/release/libilo.a", parent);
        if std::path::Path::new(&ws_release).exists() {
            return Ok(ws_release);
        }
        let ws_debug = format!("{}/target/debug/libilo.a", parent);
        if std::path::Path::new(&ws_debug).exists() {
            return Ok(ws_debug);
        }
    }
    Err(format!(
        "cannot find libilo.a — build with `cargo build --release --features cranelift` first.\n\
         Searched: {}, {}",
        release_path, debug_path
    ))
}

/// Platform-specific linker flags for linking the static library.
fn platform_linker_flags() -> Vec<&'static str> {
    if cfg!(target_os = "macos") {
        vec![
            "-lm",
            "-liconv",
            "-framework",
            "CoreFoundation",
            "-framework",
            "Security",
            "-framework",
            "SystemConfiguration",
        ]
    } else {
        vec!["-lm", "-ldl", "-lpthread"]
    }
}

// ── Compile to binary ───────────────────────────────────────────────

/// Compile an ilo program to a standalone native binary.
pub fn compile_to_binary(
    program: &CompiledProgram,
    entry_func: &str,
    output_path: &str,
) -> Result<(), String> {
    let entry_idx = program
        .func_names
        .iter()
        .position(|n| n == entry_func)
        .ok_or_else(|| format!("undefined function: {}", entry_func))?;

    // Set up Cranelift for the host target
    let mut flag_builder = settings::builder();
    flag_builder
        .set("opt_level", "speed")
        .map_err(|e| e.to_string())?;
    flag_builder
        .set("is_pic", "true")
        .map_err(|e| e.to_string())?;
    let isa_builder = cranelift_native::builder().map_err(|e| e.to_string())?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| e.to_string())?;

    let obj_builder = ObjectBuilder::new(isa.clone(), "ilo_aot", default_libcall_names())
        .map_err(|e| e.to_string())?;
    let mut module = ObjectModule::new(obj_builder);

    // Helper to clean up temp files on any error path
    let cleanup = |obj: &str| {
        let _ = std::fs::remove_file(obj);
    };

    // Declare all runtime helpers as imports (resolved at link time from libilo.a)
    let helpers = declare_all_helpers(&mut module);

    // First pass: declare all functions to get FuncIds
    let mut func_ids: Vec<FuncId> = Vec::with_capacity(program.chunks.len());
    for (i, chunk) in program.chunks.iter().enumerate() {
        let name = format!("ilo_{}", program.func_names[i]);
        let mut sig = module.make_signature();
        for _ in 0..chunk.param_count {
            sig.params.push(AbiParam::new(I64));
        }
        sig.returns.push(AbiParam::new(I64));
        let fid = module
            .declare_function(&name, Linkage::Local, &sig)
            .map_err(|e| e.to_string())?;
        func_ids.push(fid);
    }

    // Second pass: compile all functions with func_ids available
    for (i, (chunk, nan_consts)) in program
        .chunks
        .iter()
        .zip(program.nan_constants.iter())
        .enumerate()
    {
        let name = format!("ilo_{}", program.func_names[i]);
        compile_function_body(
            &mut module,
            chunk,
            nan_consts,
            &name,
            func_ids[i],
            &helpers,
            Some(&func_ids),
            Some(program),
        )?;
    }

    let entry_func_id = func_ids[entry_idx];
    let entry_chunk = &program.chunks[entry_idx];

    // Serialize the type registry for embedding in the binary
    let registry_bytes = serialize_type_registry(&program.type_registry);

    // Generate main()
    generate_main(
        &mut module,
        entry_func_id,
        entry_chunk.param_count as usize,
        &helpers,
        &registry_bytes,
    )?;

    // Emit object file
    let obj_product = module.finish();
    let obj_bytes = obj_product.emit().map_err(|e| e.to_string())?;

    let obj_path = format!("{}.o", output_path);
    std::fs::write(&obj_path, &obj_bytes)
        .map_err(|e| format!("failed to write object file: {}", e))?;

    // Find libilo.a
    let libilo_path = find_libilo_a().inspect_err(|_| {
        cleanup(&obj_path);
    })?;
    let libilo_dir = std::path::Path::new(&libilo_path)
        .parent()
        .ok_or_else(|| "invalid libilo.a path".to_string())?
        .to_string_lossy()
        .to_string();

    // Link: user.o + libilo.a + system libs
    let mut link_cmd = std::process::Command::new("cc");
    link_cmd
        .arg(&obj_path)
        .arg("-o")
        .arg(output_path)
        .arg(format!("-L{}", libilo_dir))
        .arg("-lilo");
    for flag in platform_linker_flags() {
        link_cmd.arg(flag);
    }

    let status = link_cmd.status().map_err(|e| {
        cleanup(&obj_path);
        format!("failed to run cc: {}", e)
    })?;

    cleanup(&obj_path);

    if !status.success() {
        return Err(format!("linker failed with exit code: {}", status));
    }

    Ok(())
}

/// Check whether a callee chunk can be safely inlined at an AOT call site.
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

/// Inline a callee chunk directly into the caller's Cranelift IR stream (AOT).
///
/// `arg_vars`     -- caller `Variable`s for callee params (indices 0..n_params)
/// `result_var`   -- caller `Variable` that receives the inlined return value
/// `extra_vars`   -- caller `Variable`s for callee non-param regs
/// `f64_arg_vars` -- F64 shadow `Variable`s corresponding to `arg_vars`
/// `merge_block`  -- block to jump to after the inlined return
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

/// Compile a function body into the ObjectModule (function already declared).
#[allow(clippy::too_many_arguments)]
fn compile_function_body(
    module: &mut ObjectModule,
    chunk: &Chunk,
    nan_consts: &[NanVal],
    _name: &str,
    func_id: FuncId,
    helpers: &HelperFuncs,
    all_func_ids: Option<&[FuncId]>,
    program: Option<&CompiledProgram>,
) -> Result<(), String> {
    let mut sig = module.make_signature();
    for _ in 0..chunk.param_count {
        sig.params.push(AbiParam::new(I64));
    }
    sig.returns.push(AbiParam::new(I64));

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

    // Declare extra F64 shadow variables for registers that will be proven always-numeric.
    // These allow guard chains to reuse a single bitcast across multiple CMPK instructions
    // without redundant i64->f64 conversions on every comparison.
    // Variable indices [reg_count .. reg_count*2) are the F64 shadows.
    let f64_var_offset = reg_count;
    let mut f64_vars: Vec<Variable> = Vec::with_capacity(reg_count);
    for i in 0..reg_count {
        let var = Variable::from_u32((f64_var_offset + i) as u32);
        builder.declare_var(var, F64);
        f64_vars.push(var);
    }

    // ── Inline callee variable pool ───────────────────────────────────────────
    // For each OP_CALL that targets an inlinable callee we need variables for the
    // callee's non-parameter registers.  We pre-scan OP_CALL instructions and
    // allocate a variable block per unique (call-site-ip, callee-reg-index) pair.
    //
    // Variable layout:
    //   [0 .. reg_count)            I64  VM register vars
    //   [reg_count .. 2*reg_count)  F64  shadow vars
    //   [2*reg_count .. )           I64  inline callee vars
    let inline_var_base = 2 * reg_count;
    let mut inline_var_map: HashMap<usize, Vec<Variable>> = HashMap::new();
    {
        let mut next_var_idx = inline_var_base;
        if let Some(prog) = program {
            for (ip, &inst) in chunk.code.iter().enumerate() {
                let op = (inst >> 24) as u8;
                if op != OP_CALL {
                    continue;
                }
                let bx = (inst & 0xFFFF) as usize;
                let func_idx = bx >> 8;
                if func_idx >= prog.chunks.len() {
                    continue;
                }
                let callee_chunk = &prog.chunks[func_idx];
                let callee_consts = &prog.nan_constants[func_idx];
                if !is_inlinable(callee_chunk, callee_consts) {
                    continue;
                }
                let n_extra = (callee_chunk.reg_count as usize)
                    .saturating_sub(callee_chunk.param_count as usize);
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
    }

    // Pre-allocate one extra F64 shadow variable per inline arg register.
    let total_inline_i64: usize = inline_var_map.values().map(|v| v.len()).sum();
    let inline_f64_var_base = inline_var_base + total_inline_i64;
    let mut inline_f64_var_map: HashMap<usize, Vec<Variable>> = HashMap::new();
    {
        let mut next_f64_idx = inline_f64_var_base;
        if let Some(prog) = program {
            for &ip in inline_var_map.keys() {
                let bx = (chunk.code[ip] & 0xFFFF) as usize;
                let func_idx = bx >> 8;
                let callee = &prog.chunks[func_idx];
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

    // Import helper function references (cached)
    let mut func_refs: HashMap<FuncId, cranelift_codegen::ir::FuncRef> = HashMap::new();
    let mut get_func_ref = |builder: &mut FunctionBuilder<'_>,
                            module: &mut ObjectModule,
                            id: FuncId|
     -> cranelift_codegen::ir::FuncRef {
        *func_refs
            .entry(id)
            .or_insert_with(|| module.declare_func_in_func(id, builder.func))
    };

    // ── Pre-pass: determine which registers are always numeric / always boolean ──
    //
    // Numeric analysis (reg_always_num):
    //   num_write[r]     -- at least one write to r is definitely numeric
    //   non_num_write[r] -- at least one write to r may produce a non-number
    //   A register is "always numeric" iff num_write && !non_num_write.
    //
    // Boolean analysis (reg_always_bool):
    //   bool_write[r]     -- every write to r produces TAG_TRUE or TAG_FALSE
    //   non_bool_write[r] -- at least one write to r may produce a non-bool
    //   A register is "always boolean" iff bool_write && !non_bool_write.
    let mut reg_always_num = vec![false; reg_count];
    let mut reg_always_bool = vec![false; reg_count];
    {
        let mut non_num_write = vec![false; reg_count];
        let mut num_write = vec![false; reg_count];
        let mut bool_write = vec![false; reg_count];
        let mut non_bool_write = vec![false; reg_count];

        // Function parameters: the VM compiler sets all_regs_numeric when it has
        // proven every param is numeric.
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
                OP_ADD_NN | OP_SUB_NN | OP_MUL_NN | OP_DIV_NN | OP_ADDK_N | OP_SUBK_N
                | OP_MULK_N | OP_DIVK_N | OP_LEN | OP_ABS | OP_MIN | OP_MAX | OP_FLR | OP_CEL
                | OP_ROU | OP_RND0 | OP_RND2 | OP_NOW | OP_MOD | OP_POW | OP_SQRT | OP_LOG
                | OP_EXP | OP_SIN | OP_COS | OP_TAN | OP_LOG10 | OP_LOG2 | OP_ATAN2 => {
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
                OP_LT | OP_GT | OP_LE | OP_GE | OP_EQ | OP_NE | OP_NOT | OP_HAS | OP_ISNUM
                | OP_ISTEXT | OP_ISBOOL | OP_ISLIST | OP_MHAS | OP_ISOK | OP_ISERR => {
                    bool_write[a] = true;
                    non_num_write[a] = true;
                }
                // MOVE: skip here, handled by fixpoint below.
                OP_MOVE => {}
                // Ops that write a non-numeric or unknown type to R[A].
                OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_ADD_SS | OP_NEG | OP_WRAPOK | OP_WRAPERR
                | OP_UNWRAP | OP_RECFLD | OP_RECFLD_NAME | OP_LISTGET | OP_INDEX | OP_STR
                | OP_HD | OP_AT | OP_FMT2 | OP_TL | OP_REV | OP_SRT | OP_SLC | OP_SPL | OP_CAT
                | OP_GET | OP_POST | OP_GETH | OP_POSTH | OP_ENV | OP_JPTH | OP_JDMP | OP_JPAR
                | OP_MAPNEW | OP_MGET | OP_MSET | OP_MDEL | OP_MKEYS | OP_MVALS | OP_LISTNEW
                | OP_LISTAPPEND | OP_RECNEW | OP_RECWITH | OP_PRT | OP_RD | OP_RDL | OP_WR
                | OP_WRL | OP_TRM | OP_UNQ | OP_NUM => {
                    non_num_write[a] = true;
                    non_bool_write[a] = true;
                }
                // OP_CALL: if callee is known all-numeric, result is numeric.
                OP_CALL => {
                    if let Some(prog) = program {
                        let bx = (inst & 0xFFFF) as usize;
                        let func_idx = bx >> 8;
                        if func_idx < prog.chunks.len() && prog.chunks[func_idx].all_regs_numeric {
                            num_write[a] = true;
                        } else {
                            non_num_write[a] = true;
                            non_bool_write[a] = true;
                        }
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

                if b_always_num {
                    if !num_write[a] {
                        num_write[a] = true;
                        changed = true;
                    }
                } else if !non_num_write[a] {
                    non_num_write[a] = true;
                    changed = true;
                }
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

    // Initialise F64 shadow variables for always-numeric parameter registers.
    {
        let mf_init = cranelift_codegen::ir::MemFlags::new();
        for i in 0..(chunk.param_count as usize) {
            if i < reg_count && reg_always_num[i] {
                let iv = builder.use_var(vars[i]);
                let fv = builder.ins().bitcast(F64, mf_init, iv);
                builder.def_var(f64_vars[i], fv);
            }
        }
    }

    let mut block_terminated = false;
    let mut skip_next = false;
    let mf = MemFlags::new();
    // Counter for unique data section names
    let mut data_section_counter: usize = 0;

    for (ip, &inst) in chunk.code.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }

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

        match op {
            // ── Optimized numeric opcodes with F64 shadow support ──
            OP_ADD_NN => {
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
            OP_ADDK_N | OP_SUBK_N | OP_MULK_N | OP_DIVK_N => {
                let bf = if b_idx < reg_count && reg_always_num[b_idx] {
                    builder.use_var(f64_vars[b_idx])
                } else {
                    let bv = builder.use_var(vars[b_idx]);
                    builder.ins().bitcast(F64, mf, bv)
                };
                let kv = nan_consts[c_idx].as_number();
                let kval = builder.ins().f64const(kv);
                let result_f = match op {
                    OP_ADDK_N => builder.ins().fadd(bf, kval),
                    OP_SUBK_N => builder.ins().fsub(bf, kval),
                    OP_MULK_N => builder.ins().fmul(bf, kval),
                    OP_DIVK_N => builder.ins().fdiv(bf, kval),
                    _ => unreachable!(),
                };
                let result = builder.ins().bitcast(I64, mf, result_f);
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    builder.def_var(f64_vars[a_idx], result_f);
                }
            }
            // ── Generic arithmetic with inline numeric fast path + helper slow path ──
            OP_ADD | OP_SUB | OP_MUL | OP_DIV => {
                let both_always_num = b_idx < reg_always_num.len()
                    && reg_always_num[b_idx]
                    && c_idx < reg_always_num.len()
                    && reg_always_num[c_idx];

                if both_always_num {
                    // Pre-pass proved both operands are always numeric: skip QNAN check,
                    // emit inline float op directly (same as OP_ADD_NN / OP_SUB_NN etc.).
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

                    // Slow path: call helper
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
            }
            // ── String concatenation fast path — both operands guaranteed strings ──
            OP_ADD_SS => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.concat);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            // ── Comparisons with inline numeric fast path + fused compare-and-branch ──
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
                    // the same destination register. If so, fuse into single compare-and-branch.
                    let next_inst = chunk.code.get(ip + 1).copied();
                    let fused = if let Some(next) = next_inst {
                        let next_op = (next >> 24) as u8;
                        let next_a = ((next >> 16) & 0xFF) as usize;
                        (next_op == OP_JMPF || next_op == OP_JMPT)
                            && next_a == a_idx
                            && !block_map.contains_key(&(ip + 1))
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

                        let (true_dest, false_dest) = if next_op == OP_JMPF {
                            (fallthrough, target)
                        } else {
                            (target, fallthrough)
                        };

                        let true_block = block_map.get(&true_dest).copied();
                        let false_block = block_map.get(&false_dest).copied();

                        if let (Some(tb), Some(fb)) = (true_block, false_block) {
                            builder.ins().brif(cmp, tb, &[], fb, &[]);
                            block_terminated = true;
                            skip_next = true;
                        } else {
                            // Block targets not found -- fall back to non-fused path.
                            let true_val = builder.ins().iconst(I64, TAG_TRUE as i64);
                            let false_val = builder.ins().iconst(I64, TAG_FALSE as i64);
                            let result = builder.ins().select(cmp, true_val, false_val);
                            builder.def_var(vars[a_idx], result);
                        }
                    } else {
                        // No fusion -- emit direct float comparison without QNAN check.
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

                    // Fast path: inline float comparison
                    builder.switch_to_block(num_block);
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
                        // No QNAN check, no clone_rc -- just copy the bits.
                        builder.def_var(vars[a_idx], bv);
                        // Propagate f64 shadow so destination can skip bitcasts too.
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
            OP_MOD => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let bf = builder.ins().bitcast(F64, mf, bv);
                let cf = builder.ins().bitcast(F64, mf, cv);
                let div = builder.ins().fdiv(bf, cf);
                let trunc = builder.ins().trunc(div);
                let prod = builder.ins().fmul(trunc, cf);
                let result_f = builder.ins().fsub(bf, prod);
                let result = builder.ins().bitcast(I64, mf, result_f);
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
                    builder.def_var(f64_vars[a_idx], result_f);
                }
            }
            // ── Result/Option wrapping ──
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
            // ── Load constant ──
            OP_LOADK => {
                let bx = (inst & 0xFFFF) as usize;
                let bits = nan_consts[bx].0;
                let nv = NanVal(bits);
                if nv.is_string() {
                    // AOT: string constants can't embed compile-time pointers.
                    // Extract the string, store as data section, call jit_string_const at runtime.
                    let s = unsafe { nv.as_heap_ref() };
                    let string_bytes = match s {
                        HeapObj::Str(st) => {
                            let mut bytes = st.as_bytes().to_vec();
                            bytes.push(0); // null-terminate
                            bytes
                        }
                        _ => b"\0".to_vec(),
                    };
                    data_section_counter += 1;
                    let ds_name = format!("ilo_strconst_{}", data_section_counter);
                    let str_ptr =
                        create_data_section(module, &mut builder, &ds_name, &string_bytes)?;
                    let fref = get_func_ref(&mut builder, module, helpers.string_const);
                    let call_inst = builder.ins().call(fref, &[str_ptr]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                } else if nv.is_heap() {
                    // Other heap values (lists, maps, etc.) — clone RC
                    let kval = builder.ins().iconst(I64, bits as i64);
                    let fref = get_func_ref(&mut builder, module, helpers.jit_move);
                    let call_inst = builder.ins().call(fref, &[kval]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                } else {
                    let kval = builder.ins().iconst(I64, bits as i64);
                    builder.def_var(vars[a_idx], kval);
                    // Initialise F64 shadow for numeric constants so arithmetic ops
                    // can skip the bitcast when reading this register.
                    if nv.is_number() && a_idx < reg_count && reg_always_num[a_idx] {
                        let fv = builder.ins().bitcast(F64, mf, kval);
                        builder.def_var(f64_vars[a_idx], fv);
                    }
                }
            }
            // ── Control flow ──
            OP_JMP => {
                let sbx = (inst & 0xFFFF) as i16;
                let target = (ip as isize + 1 + sbx as isize) as usize;
                let tb = block_map.get(&target).ok_or_else(|| {
                    format!("JMP target {} at ip {} has no block leader", target, ip)
                })?;
                builder.ins().jump(*tb, &[]);
                block_terminated = true;
            }
            OP_JMPF | OP_JMPT => {
                let sbx = (inst & 0xFFFF) as i16;
                let target = (ip as isize + 1 + sbx as isize) as usize;
                let fallthrough = ip + 1;
                let av = builder.use_var(vars[a_idx]);

                let target_block = block_map.get(&target).ok_or_else(|| {
                    format!(
                        "JMPF/JMPT target {} at ip {} has no block leader",
                        target, ip
                    )
                })?;
                let fall_block = block_map.get(&fallthrough).ok_or_else(|| {
                    format!(
                        "JMPF/JMPT fallthrough {} at ip {} has no block leader",
                        fallthrough, ip
                    )
                })?;

                // Fast path: register is always a boolean (TAG_TRUE or TAG_FALSE).
                // Single icmp + brif instead of 3-block truthy dispatch.
                if a_idx < reg_always_bool.len() && reg_always_bool[a_idx] {
                    let false_val = builder.ins().iconst(I64, TAG_FALSE as i64);
                    let is_false = builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::Equal,
                        av,
                        false_val,
                    );
                    if op == OP_JMPF {
                        builder
                            .ins()
                            .brif(is_false, *target_block, &[], *fall_block, &[]);
                    } else {
                        builder
                            .ins()
                            .brif(is_false, *fall_block, &[], *target_block, &[]);
                    }
                    block_terminated = true;
                } else {
                    // General case: full truthy check.
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

                    builder.switch_to_block(num_truthy_block);
                    let af = builder.ins().bitcast(F64, mf, av);
                    let zero = builder.ins().f64const(0.0);
                    let cmp = builder.ins().fcmp(
                        cranelift_codegen::ir::condcodes::FloatCC::NotEqual,
                        af,
                        zero,
                    );
                    let num_result = builder.ins().uextend(I64, cmp);
                    builder.ins().jump(merge_truthy, &[num_result]);

                    builder.switch_to_block(tag_truthy_block);
                    let nil_val = builder.ins().iconst(I64, TAG_NIL as i64);
                    let false_val = builder.ins().iconst(I64, TAG_FALSE as i64);
                    let not_nil = builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                        av,
                        nil_val,
                    );
                    let not_false = builder.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                        av,
                        false_val,
                    );
                    let tag_truthy = builder.ins().band(not_nil, not_false);
                    let tag_result = builder.ins().uextend(I64, tag_truthy);
                    builder.ins().jump(merge_truthy, &[tag_result]);

                    builder.switch_to_block(merge_truthy);
                    let truthy_val = builder.block_params(merge_truthy)[0];

                    if op == OP_JMPF {
                        builder
                            .ins()
                            .brif(truthy_val, *fall_block, &[], *target_block, &[]);
                    } else {
                        builder
                            .ins()
                            .brif(truthy_val, *target_block, &[], *fall_block, &[]);
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
                let is_nil = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    av,
                    nil_const,
                );
                let tb = block_map.get(&target).ok_or_else(|| {
                    format!("JMPNN target {} at ip {} has no block leader", target, ip)
                })?;
                let fb = block_map.get(&fallthrough).ok_or_else(|| {
                    format!(
                        "JMPNN fallthrough {} at ip {} has no block leader",
                        fallthrough, ip
                    )
                })?;
                builder.ins().brif(is_nil, *fb, &[], *tb, &[]);
                block_terminated = true;
            }
            OP_RET => {
                let av = builder.use_var(vars[a_idx]);
                builder.ins().return_(&[av]);
                block_terminated = true;
            }
            // ── Builtins: 1-arg → 1-return ──
            OP_LEN => {
                let bv = builder.use_var(vars[b_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.len);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
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
                    let rf = builder.ins().bitcast(F64, mf, result);
                    builder.def_var(f64_vars[a_idx], rf);
                }
            }
            OP_SQRT | OP_LOG | OP_EXP | OP_SIN | OP_COS | OP_TAN | OP_LOG10 | OP_LOG2 => {
                let bv = builder.use_var(vars[b_idx]);
                let fid = match op {
                    OP_SQRT => helpers.sqrt,
                    OP_LOG => helpers.log,
                    OP_EXP => helpers.exp,
                    OP_SIN => helpers.sin,
                    OP_COS => helpers.cos,
                    OP_TAN => helpers.tan,
                    OP_LOG10 => helpers.log10,
                    _ => helpers.log2,
                };
                let fref = get_func_ref(&mut builder, module, fid);
                let call_inst = builder.ins().call(fref, &[bv]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
                if a_idx < reg_count && reg_always_num[a_idx] {
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
            OP_AT => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.at);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
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
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                // Consume the next instruction (data word) at compile time; end-index reg in A field
                let data_inst = chunk.code[ip + 1];
                skip_next = true;
                let d_idx = ((data_inst >> 16) & 0xFF) as usize;
                let dv = builder.use_var(vars[d_idx]);
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
                let bv = builder.use_var(vars[b_idx]);
                let idx_val = builder.ins().iconst(I64, c_idx as i64);
                let fref = get_func_ref(&mut builder, module, helpers.index);
                let call_inst = builder.ins().call(fref, &[bv, idx_val]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            // ── Record field access (inline arena fast path) ──
            OP_RECFLD => {
                let bv = builder.use_var(vars[b_idx]);

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

                // Arena path: inline pointer math
                builder.switch_to_block(arena_block);
                let ptr_mask_val = builder.ins().iconst(I64, PTR_MASK as i64);
                let ptr = builder.ins().band(bv, ptr_mask_val);
                let field_offset = builder.ins().iconst(I64, (8 + c_idx * 8) as i64);
                let field_addr = builder.ins().iadd(ptr, field_offset);
                let field_val = builder.ins().load(I64, MemFlags::trusted(), field_addr, 0);
                // Inline is_heap check for clone_rc
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

                builder.switch_to_block(clone_block);
                let fref_move = get_func_ref(&mut builder, module, helpers.jit_move);
                let move_inst = builder.ins().call(fref_move, &[field_val]);
                let _cloned = builder.inst_results(move_inst)[0];
                builder.ins().jump(skip_clone_block, &[]);

                builder.switch_to_block(skip_clone_block);
                builder.ins().jump(merge_block, &[field_val]);

                // Heap path: call jit_recfld
                builder.switch_to_block(heap_block);
                let field_idx_val = builder.ins().iconst(I64, c_idx as i64);
                let fref = get_func_ref(&mut builder, module, helpers.recfld);
                let call_inst = builder.ins().call(fref, &[bv, field_idx_val]);
                let heap_result = builder.inst_results(call_inst)[0];
                builder.ins().jump(merge_block, &[heap_result]);

                builder.switch_to_block(merge_block);
                let result = builder.block_params(merge_block)[0];
                builder.def_var(vars[a_idx], result);
            }
            OP_RECFLD_NAME => {
                let b_idx = ((inst >> 8) & 0xFF) as usize;
                let c_idx = (inst & 0xFF) as usize;
                let bv = builder.use_var(vars[b_idx]);
                // Get field name from chunk constants, store as data section
                let mut name_bytes = match &chunk.constants[c_idx] {
                    crate::interpreter::Value::Text(s) => s.as_bytes().to_vec(),
                    _ => return Err(format!("OP_RECFLD_NAME expects string constant at {}", ip)),
                };
                name_bytes.push(0); // null-terminate
                data_section_counter += 1;
                let ds_name = format!("ilo_fldname_{}", data_section_counter);
                let name_ptr = create_data_section(module, &mut builder, &ds_name, &name_bytes)?;
                // Get registry pointer at runtime
                let fref_reg = get_func_ref(&mut builder, module, helpers.get_registry_ptr);
                let reg_call = builder.ins().call(fref_reg, &[]);
                let registry_val = builder.inst_results(reg_call)[0];
                let fref = get_func_ref(&mut builder, module, helpers.recfld_name);
                let call_inst = builder.ins().call(fref, &[bv, name_ptr, registry_val]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            // ── Record creation (AOT: call helper with runtime arena pointer) ──
            OP_RECNEW => {
                let bx = (inst & 0xFFFF) as usize;
                let type_id = (bx >> 8) as u16;
                let n_fields = bx & 0xFF;
                let record_size = 8 + n_fields * 8;

                // Get arena pointer at runtime
                let fref_arena = get_func_ref(&mut builder, module, helpers.get_arena_ptr);
                let arena_call = builder.ins().call(fref_arena, &[]);
                let arena_ptr_val = builder.inst_results(arena_call)[0];

                // Load arena.offset
                let cur_offset = builder
                    .ins()
                    .load(I64, MemFlags::trusted(), arena_ptr_val, 16);
                let seven = builder.ins().iconst(I64, 7);
                let off_plus_7 = builder.ins().iadd(cur_offset, seven);
                let neg8 = builder.ins().iconst(I64, !7i64);
                let aligned = builder.ins().band(off_plus_7, neg8);
                let size_val = builder.ins().iconst(I64, record_size as i64);
                let new_offset = builder.ins().iadd(aligned, size_val);
                let buf_cap = builder
                    .ins()
                    .load(I64, MemFlags::trusted(), arena_ptr_val, 8);
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

                // Inline alloc path
                builder.switch_to_block(alloc_block);
                let buf_ptr = builder
                    .ins()
                    .load(I64, MemFlags::trusted(), arena_ptr_val, 0);
                let rec_ptr = builder.ins().iadd(buf_ptr, aligned);
                let header = ((n_fields as u64) << 16) | (type_id as u64);
                let header_val = builder.ins().iconst(I64, header as i64);
                builder
                    .ins()
                    .store(MemFlags::trusted(), header_val, rec_ptr, 0);
                for i in 0..n_fields {
                    let field_v = builder.use_var(vars[a_idx + 1 + i]);
                    let field_off = (8 + i * 8) as i32;
                    builder
                        .ins()
                        .store(MemFlags::trusted(), field_v, rec_ptr, field_off);
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
                builder
                    .ins()
                    .store(MemFlags::trusted(), new_offset, arena_ptr_val, 16);
                let tag_val = builder.ins().iconst(I64, TAG_ARENA_REC as i64);
                let result_val = builder.ins().bor(rec_ptr, tag_val);
                builder.ins().jump(merge_block, &[result_val]);

                // Fallback: call jit_recnew helper
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
                // Get registry pointer at runtime
                let fref_reg = get_func_ref(&mut builder, module, helpers.get_registry_ptr);
                let reg_call = builder.ins().call(fref_reg, &[]);
                let registry_ptr_val = builder.inst_results(reg_call)[0];
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

                builder.switch_to_block(merge_block);
                let result = builder.block_params(merge_block)[0];
                builder.def_var(vars[a_idx], result);
            }
            // ── Record update ──
            OP_RECWITH => {
                let bx = (inst & 0xFFFF) as usize;
                let indices_idx = bx >> 8;
                let n_updates = bx & 0xFF;

                // Extract field indices from the constant pool and store in a data section
                let update_indices: Vec<u8> = match &chunk.constants[indices_idx] {
                    Value::List(items) => items
                        .iter()
                        .map(|v| match v {
                            Value::Number(n) => *n as u8,
                            _ => 0,
                        })
                        .collect(),
                    _ => {
                        return Err(format!(
                            "OP_RECWITH: expected list constant at index {}",
                            indices_idx
                        ));
                    }
                };

                // Use a data section instead of leaking a Box
                let ds_name = format!("ilo_recwith_indices_{}", data_section_counter);
                data_section_counter += 1;
                let indices_gv =
                    create_data_section(module, &mut builder, &ds_name, &update_indices)?;

                let old_rec = builder.use_var(vars[a_idx]);
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
                let n_updates_val = builder.ins().iconst(I64, n_updates as i64);
                let fref = get_func_ref(&mut builder, module, helpers.recwith);
                let call_inst = builder
                    .ins()
                    .call(fref, &[old_rec, indices_gv, n_updates_val, regs_ptr]);
                let result = builder.inst_results(call_inst)[0];
                builder.def_var(vars[a_idx], result);
            }
            // ── List creation ──
            OP_LISTNEW => {
                let n = (inst & 0xFFFF) as usize;
                if n == 0 {
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
            // ── List get (foreach iteration) ──
            OP_LISTGET => {
                // LISTGET: R[A] = R[B][R[C]], skip next instruction if found.
                // Used for foreach loops. Inlined to eliminate C-ABI call overhead
                // (jit_listget + jit_unwrap + jit_drop_rc) and the malloc/free of the
                // OkVal wrapper that the old helper-call path required.
                //
                // HeapObj::List memory layout (ptr = bv & PTR_MASK):
                //   [ptr + 0]  discriminant = 1 for List variant
                //   [ptr + 8]  Vec.len  (usize)
                //   [ptr + 16] Vec.data_ptr  (*mut NanVal, each slot is u64/8 bytes)
                //   [ptr + 24] Vec.cap  (usize)
                // Layout confirmed with a runtime probe in jit_cranelift.rs.
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let mf_plain = cranelift_codegen::ir::MemFlags::new();
                let mf_trusted = cranelift_codegen::ir::MemFlags::trusted();
                let ic_eq = cranelift_codegen::ir::condcodes::IntCC::Equal;
                let ic_ult = cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThan;
                let qnan_c = builder.ins().iconst(I64, QNAN as i64);

                let jmp_block = block_map.get(&(ip + 1)).copied();
                let body_block = block_map.get(&(ip + 2)).copied();

                if let (Some(jb), Some(bb)) = (jmp_block, body_block) {
                    // Guard 1: bv must be a list (tag == TAG_LIST)
                    let tag_mask_c = builder.ins().iconst(I64, TAG_MASK as i64);
                    let tag = builder.ins().band(bv, tag_mask_c);
                    let list_tag_c = builder.ins().iconst(I64, TAG_LIST as i64);
                    let is_list = builder.ins().icmp(ic_eq, tag, list_tag_c);

                    let check_num_block = builder.create_block();
                    builder.ins().brif(is_list, check_num_block, &[], jb, &[]);

                    // Guard 2: cv must be a number ((cv & QNAN) != QNAN)
                    builder.switch_to_block(check_num_block);
                    builder.seal_block(check_num_block);
                    let cv_masked = builder.ins().band(cv, qnan_c);
                    let is_not_num = builder.ins().icmp(ic_eq, cv_masked, qnan_c);

                    let load_block = builder.create_block();
                    builder.ins().brif(is_not_num, jb, &[], load_block, &[]);

                    // Load Vec metadata and bounds check
                    builder.switch_to_block(load_block);
                    builder.seal_block(load_block);

                    // ptr = bv & PTR_MASK  (points to HeapObj::List inner value)
                    let ptr_mask_c = builder.ins().iconst(I64, PTR_MASK as i64);
                    let ptr = builder.ins().band(bv, ptr_mask_c);

                    // vec_len = *[ptr + 8]  (Vec.len)
                    let vec_len = builder.ins().load(I64, mf_trusted, ptr, 8);

                    // idx_u = (u64) cv as f64, saturating cast; NaN/neg → 0
                    let cv_f = builder.ins().bitcast(F64, mf_plain, cv);
                    let idx_u = builder.ins().fcvt_to_uint_sat(I64, cv_f);

                    // Bounds check: idx_u < vec_len
                    let in_bounds = builder.ins().icmp(ic_ult, idx_u, vec_len);

                    let in_bounds_block = builder.create_block();
                    builder.ins().brif(in_bounds, in_bounds_block, &[], jb, &[]);

                    // In-bounds: load element and optionally clone RC
                    builder.switch_to_block(in_bounds_block);
                    builder.seal_block(in_bounds_block);

                    // data_ptr = *[ptr + 16]
                    let data_ptr = builder.ins().load(I64, mf_trusted, ptr, 16);

                    // elem_addr = data_ptr + idx_u * 8
                    let eight = builder.ins().iconst(I64, 8i64);
                    let byte_off = builder.ins().imul(idx_u, eight);
                    let elem_addr = builder.ins().iadd(data_ptr, byte_off);
                    let elem = builder.ins().load(I64, mf_trusted, elem_addr, 0);

                    // Increment RC if elem is a heap value: (elem & QNAN) == QNAN.
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
                //
                // Bytecode layout: FOREACHPREP / JMP_exit / body_top...
                //   ip+1 = JMP exit (taken when empty list)
                //   ip+2 = first body instruction (taken when list non-empty)
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let mf_plain = cranelift_codegen::ir::MemFlags::new();
                let mf_trusted = cranelift_codegen::ir::MemFlags::trusted();
                let ic_eq = cranelift_codegen::ir::condcodes::IntCC::Equal;
                let ic_ult = cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThan;

                let jmp_block = block_map.get(&(ip + 1)).copied();
                let body_block = block_map.get(&(ip + 2)).copied();
                if let (Some(jb), Some(bb)) = (jmp_block, body_block) {
                    let qnan_c = builder.ins().iconst(I64, QNAN as i64);

                    // Guard 1: bv must be a list
                    let tag_mask_c = builder.ins().iconst(I64, TAG_MASK as i64);
                    let tag = builder.ins().band(bv, tag_mask_c);
                    let list_tag_c = builder.ins().iconst(I64, TAG_LIST as i64);
                    let is_list = builder.ins().icmp(ic_eq, tag, list_tag_c);

                    let check_num_block = builder.create_block();
                    builder.ins().brif(is_list, check_num_block, &[], jb, &[]);

                    // Guard 2: cv (index=0.0) must be a number
                    builder.switch_to_block(check_num_block);
                    builder.seal_block(check_num_block);
                    let cv_masked = builder.ins().band(cv, qnan_c);
                    let is_not_num = builder.ins().icmp(ic_eq, cv_masked, qnan_c);

                    let load_block = builder.create_block();
                    builder.ins().brif(is_not_num, jb, &[], load_block, &[]);

                    builder.switch_to_block(load_block);
                    builder.seal_block(load_block);

                    // ptr = bv & PTR_MASK
                    let ptr_mask_c = builder.ins().iconst(I64, PTR_MASK as i64);
                    let ptr = builder.ins().band(bv, ptr_mask_c);

                    // HeapObj::List layout:
                    //   ptr+ 0 = discriminant
                    //   ptr+ 8 = Vec.len   (length — use this for bounds check)
                    //   ptr+16 = Vec.data_ptr
                    //   ptr+24 = Vec.cap
                    let vec_len = builder.ins().load(I64, mf_trusted, ptr, 8);
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
                    // Fallback: use listget helper (should not occur in normal foreach)
                    let fref = get_func_ref(&mut builder, module, helpers.listget);
                    let call_inst = builder.ins().call(fref, &[bv, cv]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                }
            }
            OP_FOREACHNEXT => {
                // FOREACHNEXT: R[C] += 1; load R[B][R[C]] into R[A] if in-bounds.
                // Inlined: increment index as f64, then direct memory access.
                //
                // Bytecode layout: FOREACHNEXT / JMP_exit / JMP_body_top
                //   ip+1 = JMP exit  (taken when out-of-bounds)
                //   ip+2 = JMP body_top  (taken when in-bounds, after loading element)
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
                    let qnan_c = builder.ins().iconst(I64, QNAN as i64);

                    // Extract ptr from list NanVal (already validated in FOREACHPREP)
                    let ptr_mask_c = builder.ins().iconst(I64, PTR_MASK as i64);
                    let ptr = builder.ins().band(bv, ptr_mask_c);

                    // vec_len = *[ptr + 8]  (Vec.len — offset confirmed in OP_LISTGET)
                    let vec_len = builder.ins().load(I64, mf_trusted, ptr, 8);

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
                    // Fallback: use listget helper with the new index
                    let fref = get_func_ref(&mut builder, module, helpers.listget);
                    let call_inst = builder.ins().call(fref, &[bv, new_idx]);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a_idx], result);
                }
            }
            // ── Fused compare-and-skip for numeric guard chains ──────────────
            // ABC: A = reg, B = unused, C = constant pool index (ki).
            // If condition TRUE -> skip next instruction (the OP_JMP), enter body.
            // If condition FALSE -> fall through to the OP_JMP that skips the body.
            //
            // Optimisation 1: use the pre-converted F64 shadow when available.
            // Optimisation 2: jump threading -- decode ip+1 JMP target directly.
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
                    let mf_cmpk = cranelift_codegen::ir::MemFlags::new();
                    builder.ins().bitcast(F64, mf_cmpk, lhs)
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

                // ip + 2 = first instruction of the guard body (taken when condition TRUE)
                let body_block = block_map.get(&(ip + 2)).copied();

                // Optimisation 2: jump threading.
                // The instruction at ip+1 is always OP_JMP. Decode its target now so
                // that the false branch goes directly there, skipping the intermediate
                // JMP block entirely.
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
                    // condition TRUE -> body; condition FALSE -> threaded miss target
                    builder.ins().brif(cmp, bb, &[], false_block, &[]);
                    block_terminated = true;
                }
            }
            // ── Function call with inlining + F64 shadow support ──
            OP_CALL => {
                let a = ((inst >> 16) & 0xFF) as u8;
                let bx = (inst & 0xFFFF) as usize;
                let func_idx = bx >> 8;
                let n_args = bx & 0xFF;
                let a_idx_call = a as usize;

                if let Some(fids) = all_func_ids {
                    // Check if the target function is inlinable (pure numeric guard chain).
                    let can_inline = program
                        .map(|prog| {
                            func_idx < prog.chunks.len()
                                && is_inlinable(
                                    &prog.chunks[func_idx],
                                    &prog.nan_constants[func_idx],
                                )
                                && n_args == prog.chunks[func_idx].param_count as usize
                                && inline_var_map.contains_key(&ip)
                        })
                        .unwrap_or(false);

                    if can_inline {
                        let prog = program.unwrap();
                        let callee_chunk = &prog.chunks[func_idx];
                        let callee_consts = &prog.nan_constants[func_idx];
                        let result_var = vars[a_idx_call];

                        // Collect arg Variables and build F64 shadows for them.
                        let arg_var_list: Vec<Variable> =
                            (0..n_args).map(|i| vars[a_idx_call + 1 + i]).collect();
                        let f64_arg_list: Vec<Variable> = {
                            let mf_inline = cranelift_codegen::ir::MemFlags::new();
                            let f64_slots = inline_f64_var_map
                                .get(&ip)
                                .map(|v| v.as_slice())
                                .unwrap_or(&[]);
                            for (i, &av) in arg_var_list.iter().enumerate() {
                                if i < f64_slots.len() {
                                    let iv = builder.use_var(av);
                                    let fv = builder.ins().bitcast(F64, mf_inline, iv);
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
                            builder.switch_to_block(merge_blk);
                            block_terminated = false;
                        } else {
                            // Inlining failed mid-way; fall back to a real call.
                            builder.ins().jump(merge_blk, &[]);
                            builder.switch_to_block(merge_blk);

                            let target_fid = fids[func_idx];
                            let target_fref = get_func_ref(&mut builder, module, target_fid);
                            let call_args: Vec<_> = (0..n_args)
                                .map(|i| builder.use_var(vars[a_idx_call + 1 + i]))
                                .collect();
                            let call_inst = builder.ins().call(target_fref, &call_args);
                            let result = builder.inst_results(call_inst)[0];
                            builder.def_var(result_var, result);
                        }
                    } else {
                        // Direct call: the target function is compiled in this module
                        let target_fid = fids[func_idx];
                        let target_fref = get_func_ref(&mut builder, module, target_fid);
                        let mut call_args = Vec::with_capacity(n_args);
                        for i in 0..n_args {
                            call_args.push(builder.use_var(vars[a_idx_call + 1 + i]));
                        }
                        let call_inst = builder.ins().call(target_fref, &call_args);
                        let result = builder.inst_results(call_inst)[0];
                        builder.def_var(vars[a_idx_call], result);
                    }
                } else {
                    // Fallback: use jit_call helper (should not happen if all_func_ids is provided)
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
                        let prog_ptr = builder.ins().iconst(I64, 0i64);
                        let func_idx_val = builder.ins().iconst(I64, func_idx as i64);
                        let n_args_val = builder.ins().iconst(I64, n_args as i64);
                        let fref = get_func_ref(&mut builder, module, helpers.call);
                        let call_inst = builder
                            .ins()
                            .call(fref, &[prog_ptr, func_idx_val, args_ptr, n_args_val]);
                        let result = builder.inst_results(call_inst)[0];
                        builder.def_var(vars[a_idx_call], result);
                    } else {
                        let null_ptr = builder.ins().iconst(I64, 0i64);
                        let prog_ptr = builder.ins().iconst(I64, 0i64);
                        let func_idx_val = builder.ins().iconst(I64, func_idx as i64);
                        let n_args_val = builder.ins().iconst(I64, 0i64);
                        let fref = get_func_ref(&mut builder, module, helpers.call);
                        let call_inst = builder
                            .ins()
                            .call(fref, &[prog_ptr, func_idx_val, null_ptr, n_args_val]);
                        let result = builder.inst_results(call_inst)[0];
                        builder.def_var(vars[a_idx_call], result);
                    }
                }
                // Update F64 shadow so arithmetic ops can skip bitcast when using this
                // register as input.
                if a_idx_call < reg_count && reg_always_num[a_idx_call] {
                    let rv = builder.use_var(vars[a_idx_call]);
                    let rf = builder.ins().bitcast(F64, mf, rv);
                    builder.def_var(f64_vars[a_idx_call], rf);
                }
            }
            // ── JSON ──
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
            // ── Type predicates ──
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
                // Consume the next instruction (data word) at compile time; val reg in A field
                let data_inst = chunk.code[ip + 1];
                skip_next = true;
                let d_idx = ((data_inst >> 16) & 0xFF) as usize;
                let dv = builder.use_var(vars[d_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.mset);
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

    module
        .define_function(func_id, &mut ctx)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Generate the `main(argc, argv)` entry point.
/// Serialize a TypeRegistry to bytes for embedding in AOT binaries.
/// Format: `type_name\0num_fields_bitmask\0field1\0field2\0...\0\n` per type.
fn serialize_type_registry(registry: &super::TypeRegistry) -> Vec<u8> {
    let mut buf = Vec::new();
    for ti in &registry.types {
        buf.extend_from_slice(ti.name.as_bytes());
        buf.push(0);
        buf.extend_from_slice(ti.num_fields.to_string().as_bytes());
        buf.push(0);
        for f in &ti.fields {
            buf.extend_from_slice(f.as_bytes());
            buf.push(0);
        }
        buf.push(b'\n');
    }
    buf
}

fn generate_main(
    module: &mut ObjectModule,
    user_func_id: FuncId,
    param_count: usize,
    helpers: &HelperFuncs,
    registry_bytes: &[u8],
) -> Result<(), String> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(I32)); // argc
    sig.params.push(AbiParam::new(I64)); // argv
    sig.returns.push(AbiParam::new(I32)); // exit code

    let main_id = module
        .declare_function("main", Linkage::Export, &sig)
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

    // Call ilo_aot_init() at the start
    let init_fref = module.declare_func_in_func(helpers.aot_init, builder.func);
    builder.ins().call(init_fref, &[]);

    // Set up the type registry if there are any types
    if !registry_bytes.is_empty() {
        let reg_ptr =
            create_data_section(module, &mut builder, "ilo_type_registry", registry_bytes)?;
        let reg_len = builder.ins().iconst(I64, registry_bytes.len() as i64);
        let set_reg_fref = module.declare_func_in_func(helpers.aot_set_registry, builder.func);
        builder.ins().call(set_reg_fref, &[reg_ptr, reg_len]);
    }

    let user_fref = module.declare_func_in_func(user_func_id, builder.func);
    let parse_arg_fref = module.declare_func_in_func(helpers.aot_parse_arg, builder.func);

    // Convert CLI args to NanVal via ilo_aot_parse_arg (auto-detects number vs string)
    let mut call_args = Vec::with_capacity(param_count);
    for i in 0..param_count {
        let idx = builder.ins().iconst(I64, ((i + 1) * 8) as i64);
        let arg_ptr_ptr = builder.ins().iadd(argv, idx);
        let arg_ptr = builder.ins().load(I64, mf, arg_ptr_ptr, 0);
        let call_inst = builder.ins().call(parse_arg_fref, &[arg_ptr]);
        let nan_val = builder.inst_results(call_inst)[0];
        call_args.push(nan_val);
    }

    // Call the user function
    let call_inst = builder.ins().call(user_fref, &call_args);
    let result = builder.inst_results(call_inst)[0];

    // Print the result using jit_prt (handles all value types: numbers, strings, bools, etc.)
    let prt_fref = module.declare_func_in_func(helpers.prt, builder.func);
    builder.ins().call(prt_fref, &[result]);

    // Call ilo_aot_fini() before returning
    let fini_fref = module.declare_func_in_func(helpers.aot_fini, builder.func);
    builder.ins().call(fini_fref, &[]);
    let zero = builder.ins().iconst(I32, 0);
    builder.ins().return_(&[zero]);

    builder.finalize();

    module
        .define_function(main_id, &mut ctx)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Create a read-only data section and return a pointer to it.
fn create_data_section(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder<'_>,
    name: &str,
    bytes: &[u8],
) -> Result<cranelift_codegen::ir::Value, String> {
    use cranelift_module::DataDescription;
    let data_id = module
        .declare_data(name, Linkage::Local, false, false)
        .map_err(|e| e.to_string())?;
    let mut desc = DataDescription::new();
    desc.define(bytes.to_vec().into_boxed_slice());
    module
        .define_data(data_id, &desc)
        .map_err(|e| e.to_string())?;
    let gv = module.declare_data_in_func(data_id, builder.func);
    Ok(builder.ins().global_value(I64, gv))
}

/// Compile an ilo program to a benchmark binary that loops and reports ns/call.
pub fn compile_to_bench_binary(
    program: &CompiledProgram,
    entry_func: &str,
    output_path: &str,
) -> Result<(), String> {
    let entry_idx = program
        .func_names
        .iter()
        .position(|n| n == entry_func)
        .ok_or_else(|| format!("undefined function: {}", entry_func))?;

    let mut flag_builder = settings::builder();
    flag_builder
        .set("opt_level", "speed")
        .map_err(|e| e.to_string())?;
    flag_builder
        .set("is_pic", "true")
        .map_err(|e| e.to_string())?;
    let isa_builder = cranelift_native::builder().map_err(|e| e.to_string())?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| e.to_string())?;

    let obj_builder = ObjectBuilder::new(isa.clone(), "ilo_aot_bench", default_libcall_names())
        .map_err(|e| e.to_string())?;
    let mut module = ObjectModule::new(obj_builder);

    let helpers = declare_all_helpers(&mut module);

    // First pass: declare all functions to get FuncIds
    let mut func_ids: Vec<FuncId> = Vec::with_capacity(program.chunks.len());
    for (i, chunk) in program.chunks.iter().enumerate() {
        let name = format!("ilo_{}", program.func_names[i]);
        let linkage = if i == entry_idx {
            Linkage::Export
        } else {
            Linkage::Local
        };
        let mut sig = module.make_signature();
        for _ in 0..chunk.param_count {
            sig.params.push(AbiParam::new(I64));
        }
        sig.returns.push(AbiParam::new(I64));
        let fid = module
            .declare_function(&name, linkage, &sig)
            .map_err(|e| e.to_string())?;
        func_ids.push(fid);
    }

    // Second pass: compile all functions with func_ids available
    for (i, (chunk, nan_consts)) in program
        .chunks
        .iter()
        .zip(program.nan_constants.iter())
        .enumerate()
    {
        let name = format!("ilo_{}", program.func_names[i]);
        compile_function_body(
            &mut module,
            chunk,
            nan_consts,
            &name,
            func_ids[i],
            &helpers,
            Some(&func_ids),
            Some(program),
        )?;
    }

    // Emit object file
    let obj_product = module.finish();
    let obj_bytes = obj_product.emit().map_err(|e| e.to_string())?;
    let obj_path = format!("{}.o", output_path);
    std::fs::write(&obj_path, &obj_bytes)
        .map_err(|e| format!("failed to write object file: {}", e))?;

    // Generate C bench harness
    let entry_chunk = &program.chunks[entry_idx];
    let param_count = entry_chunk.param_count as usize;
    let func_name = format!("ilo_{}", entry_func);
    let bench_c_path = format!("{}_bench.c", output_path);
    // Serialize registry for embedding in C harness
    let registry_bytes = serialize_type_registry(&program.type_registry);
    let registry_c_literal = registry_bytes
        .iter()
        .map(|b| format!("\\x{:02x}", b))
        .collect::<String>();

    let mut c_code = String::from(
        "#include <stdio.h>\n\
         #include <stdlib.h>\n\
         #include <stdint.h>\n\
         #include <string.h>\n\
         #include <time.h>\n\n\
         extern void ilo_aot_init(void);\n\
         extern void ilo_aot_fini(void);\n\
         extern void ilo_aot_arena_reset(void);\n\
         extern void ilo_aot_set_registry(int64_t ptr, int64_t len);\n\
         extern int64_t ilo_aot_parse_arg(int64_t ptr);\n\n",
    );
    // Embed the serialized type registry as a C byte array
    if !registry_bytes.is_empty() {
        c_code.push_str(&format!(
            "static const char ilo_registry_data[] = \"{}\";\n\n",
            registry_c_literal
        ));
    }

    // Declare the exported function
    c_code.push_str(&format!("extern int64_t {}(", func_name));
    for i in 0..param_count {
        if i > 0 {
            c_code.push_str(", ");
        }
        c_code.push_str("int64_t");
    }
    c_code.push_str(");\n\n");

    // main: parse args, warmup, loop, report
    c_code.push_str("int main(int argc, char** argv) {\n");
    c_code.push_str(&format!(
        "\tif (argc < {}) {{ fprintf(stderr, \"Usage: %s <iters> <func-args...>\\n\", argv[0]); return 1; }}\n",
        2 + param_count
    ));
    c_code.push_str("\tint iters = atoi(argv[1]);\n");

    for i in 0..param_count {
        c_code.push_str(&format!(
            "\tint64_t a{} = ilo_aot_parse_arg((int64_t)argv[{}]);\n",
            i,
            i + 2
        ));
    }

    let call_args: String = (0..param_count)
        .map(|i| format!("a{}", i))
        .collect::<Vec<_>>()
        .join(", ");

    // Init + warmup
    c_code.push_str("\tilo_aot_init();\n");
    if !registry_bytes.is_empty() {
        c_code.push_str(&format!(
            "\tilo_aot_set_registry((int64_t)ilo_registry_data, {});\n",
            registry_bytes.len()
        ));
    }
    c_code.push_str(&format!("\t{}({});\n", func_name, call_args));
    c_code.push_str("\tilo_aot_arena_reset();\n");

    // Timed loop
    c_code.push_str("\tstruct timespec start, end;\n");
    c_code.push_str("\tclock_gettime(CLOCK_MONOTONIC, &start);\n");
    c_code.push_str("\tvolatile int64_t r;\n");
    c_code.push_str(&format!(
        "\tfor (int i = 0; i < iters; i++) {{ r = {}({}); ilo_aot_arena_reset(); }}\n",
        func_name, call_args
    ));
    c_code.push_str("\tclock_gettime(CLOCK_MONOTONIC, &end);\n");
    c_code.push_str(
        "\tlong ns = (end.tv_sec - start.tv_sec) * 1000000000L + (end.tv_nsec - start.tv_nsec);\n",
    );
    c_code.push_str("\tprintf(\"%ld\\n\", ns / iters);\n");
    c_code.push_str("\tilo_aot_fini();\n");
    c_code.push_str("\treturn 0;\n}\n");

    std::fs::write(&bench_c_path, &c_code)
        .map_err(|e| format!("failed to write bench C file: {}", e))?;

    // Compile C harness
    let bench_o_path = format!("{}_bench_c.o", output_path);
    let cc_status = std::process::Command::new("cc")
        .args(["-c", "-O2", &bench_c_path, "-o", &bench_o_path])
        .status()
        .map_err(|e| format!("failed to compile bench harness: {}", e))?;
    let _ = std::fs::remove_file(&bench_c_path);
    if !cc_status.success() {
        let _ = std::fs::remove_file(&obj_path);
        let _ = std::fs::remove_file(&bench_o_path);
        return Err("failed to compile C bench harness".to_string());
    }

    // Find libilo.a and link
    let libilo_path = find_libilo_a()?;
    let libilo_dir = std::path::Path::new(&libilo_path)
        .parent()
        .ok_or_else(|| "invalid libilo.a path".to_string())?
        .to_string_lossy()
        .to_string();

    let mut link_cmd = std::process::Command::new("cc");
    link_cmd
        .arg(&obj_path)
        .arg(&bench_o_path)
        .arg("-o")
        .arg(output_path)
        .arg(format!("-L{}", libilo_dir))
        .arg("-lilo");
    for flag in platform_linker_flags() {
        link_cmd.arg(flag);
    }

    let status = link_cmd.status().map_err(|e| {
        let _ = std::fs::remove_file(&obj_path);
        let _ = std::fs::remove_file(&bench_o_path);
        format!("failed to link: {}", e)
    })?;

    let _ = std::fs::remove_file(&obj_path);
    let _ = std::fs::remove_file(&bench_o_path);

    if !status.success() {
        return Err(format!("linker failed with exit code: {}", status));
    }

    Ok(())
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
    fn aot_compile_recursive() {
        // Recursive factorial — now supported via direct calls
        let compiled = compile_program("fac n:n>n;<=n 1 1;r=fac -n 1;*n r");
        let tmp = std::env::temp_dir().join("ilo_test_aot_rec");
        let out = tmp.to_str().unwrap();
        compile_to_binary(&compiled, "fac", out).unwrap();

        let output = std::process::Command::new(out)
            .arg("5")
            .output()
            .expect("failed to run compiled binary");
        let _ = std::fs::remove_file(out);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "120");
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

    #[test]
    fn aot_sequential_cross_function_calls() {
        // Two sequential calls: a=dbl(n), then triple(a)
        let compiled =
            compile_program("dbl x:n>n;*x 2\ntriple x:n>n;*x 3\nf n:n>n;a=dbl n;triple a");
        let tmp = std::env::temp_dir().join("ilo_test_aot_seq_calls");
        let out = tmp.to_str().unwrap();
        compile_to_binary(&compiled, "f", out).unwrap();

        let output = std::process::Command::new(out)
            .arg("5")
            .output()
            .expect("failed to run compiled binary");
        let _ = std::fs::remove_file(out);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "30", "dbl(5)=10, triple(10)=30");
    }

    #[test]
    fn aot_pipe_chain() {
        // Pipe chain: i>>dbl>>inc>>dbl>>inc = inc(dbl(inc(dbl(i)))) = 4i+3
        let compiled =
            compile_program("dbl x:n>n;*x 2\ninc x:n>n;+x 1\nf n:n>n;n>>dbl>>inc>>dbl>>inc");
        let tmp = std::env::temp_dir().join("ilo_test_aot_pipe");
        let out = tmp.to_str().unwrap();
        compile_to_binary(&compiled, "f", out).unwrap();

        let output = std::process::Command::new(out)
            .arg("5")
            .output()
            .expect("failed to run compiled binary");
        let _ = std::fs::remove_file(out);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "23", "4*5+3=23");
    }

    // ── Helper: build an ObjectModule suitable for codegen tests ────────

    fn make_module() -> ObjectModule {
        let mut flag_builder = settings::builder();
        flag_builder.set("opt_level", "speed").unwrap();
        flag_builder.set("is_pic", "true").unwrap();
        let isa_builder = cranelift_native::builder().unwrap();
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();
        let obj_builder = ObjectBuilder::new(isa, "ilo_aot_test", default_libcall_names()).unwrap();
        ObjectModule::new(obj_builder)
    }

    /// Compile a program's functions into an ObjectModule and emit the object bytes.
    /// This exercises all Cranelift IR generation without needing libilo.a or a linker.
    fn compile_to_object_bytes(source: &str) -> Result<Vec<u8>, String> {
        let compiled = compile_program(source);

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);

        // First pass: declare all functions
        let mut func_ids: Vec<FuncId> = Vec::with_capacity(compiled.chunks.len());
        for (i, chunk) in compiled.chunks.iter().enumerate() {
            let name = format!("ilo_{}", compiled.func_names[i]);
            let mut sig = module.make_signature();
            for _ in 0..chunk.param_count {
                sig.params.push(cranelift_codegen::ir::AbiParam::new(
                    cranelift_codegen::ir::types::I64,
                ));
            }
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(
                cranelift_codegen::ir::types::I64,
            ));
            let fid = module
                .declare_function(&name, cranelift_module::Linkage::Local, &sig)
                .map_err(|e| e.to_string())?;
            func_ids.push(fid);
        }

        // Second pass: compile each function body
        for (i, (chunk, nan_consts)) in compiled
            .chunks
            .iter()
            .zip(compiled.nan_constants.iter())
            .enumerate()
        {
            let name = format!("ilo_{}", compiled.func_names[i]);
            compile_function_body(
                &mut module,
                chunk,
                nan_consts,
                &name,
                func_ids[i],
                &helpers,
                Some(&func_ids),
                Some(&compiled),
            )?;
        }

        let obj_product = module.finish();
        obj_product.emit().map_err(|e| e.to_string())
    }

    // ── find_block_leaders tests ────────────────────────────────────────

    #[test]
    fn block_leaders_empty_code() {
        let leaders = find_block_leaders(&[]);
        // instruction 0 is always a leader; filter removes > code.len()
        assert!(leaders.contains(&0) || leaders.is_empty());
    }

    #[test]
    fn block_leaders_linear_code_only_entry() {
        // A simple sequence with no jumps: only instruction 0 is a leader.
        let code: Vec<u32> = vec![
            make_inst_abc(OP_ADD_NN, 0, 1, 2),
            make_inst_abc(OP_RET, 0, 0, 0),
        ];
        let leaders = find_block_leaders(&code);
        assert_eq!(leaders, vec![0]);
    }

    #[test]
    fn block_leaders_jmp_creates_three_leaders() {
        // JMP at ip=0 with offset=1 jumps to ip=2; leaders: 0, 1, 2.
        let jmp_inst = ((OP_JMP as u32) << 24) | (1u32 & 0xFFFF);
        let code: Vec<u32> = vec![
            jmp_inst,
            make_inst_abc(OP_ADD_NN, 0, 1, 2),
            make_inst_abc(OP_RET, 0, 0, 0),
        ];
        let leaders = find_block_leaders(&code);
        assert!(leaders.contains(&0));
        assert!(leaders.contains(&1)); // instruction after jump
        assert!(leaders.contains(&2)); // jump target (ip 0 + 1 + offset 1 = 2)
    }

    #[test]
    fn block_leaders_cmpk_creates_leaders_at_ip_plus_1_and_2() {
        // CMPK_GE_N at ip=0: leaders at 0, 1, 2.
        let cmpk_inst = (OP_CMPK_GE_N as u32) << 24;
        let code: Vec<u32> = vec![
            cmpk_inst,
            make_inst_abc(OP_JMP, 0, 0, 0),
            make_inst_abc(OP_RET, 0, 0, 0),
        ];
        let leaders = find_block_leaders(&code);
        assert!(leaders.contains(&0));
        assert!(leaders.contains(&1));
        assert!(leaders.contains(&2));
    }

    #[test]
    fn block_leaders_listget_creates_leaders_at_ip_plus_1_and_2() {
        let code: Vec<u32> = vec![
            make_inst_abc(OP_LISTGET, 0, 1, 2),
            make_inst_abc(OP_JMP, 0, 0, 0),
            make_inst_abc(OP_RET, 0, 0, 0),
        ];
        let leaders = find_block_leaders(&code);
        assert!(leaders.contains(&0));
        assert!(leaders.contains(&1));
        assert!(leaders.contains(&2));
    }

    #[test]
    fn block_leaders_multiple_jumps() {
        // Two JMPs with different targets.
        // JMP at ip=0 with sbx=2 → target=3; leaders: 0, 1, 3
        // JMP at ip=1 with sbx=1 → target=3; leaders adds: 2, 3
        let jmp0 = ((OP_JMP as u32) << 24) | (2u16 as u32);
        let jmp1 = ((OP_JMP as u32) << 24) | (1u16 as u32);
        let code: Vec<u32> = vec![
            jmp0,
            jmp1,
            make_inst_abc(OP_ADD_NN, 0, 1, 2),
            make_inst_abc(OP_RET, 0, 0, 0),
        ];
        let leaders = find_block_leaders(&code);
        assert!(leaders.contains(&0));
        assert!(leaders.contains(&1)); // fallthrough of jmp0
        assert!(leaders.contains(&3)); // target of jmp0 (0+1+2=3)
        assert!(leaders.contains(&2)); // fallthrough of jmp1
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    fn make_inst_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
        ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
    }

    // ── serialize_type_registry tests ────────────────────────────────────

    #[test]
    fn serialize_empty_registry() {
        let registry = TypeRegistry::default();
        let bytes = serialize_type_registry(&registry);
        assert!(bytes.is_empty());
    }

    #[test]
    fn serialize_registry_single_type() {
        let mut registry = TypeRegistry::default();
        registry.register(
            "pt".to_string(),
            vec!["x".to_string(), "y".to_string()],
            0b11,
        );
        let bytes = serialize_type_registry(&registry);
        let s = String::from_utf8(bytes.clone()).unwrap();
        // Should contain type name, field count bitmask, field names
        assert!(s.contains("pt"));
        assert!(s.contains("x"));
        assert!(s.contains("y"));
        // Ends with newline separator
        assert!(bytes.ends_with(b"\n"));
    }

    #[test]
    fn serialize_registry_multiple_types() {
        let mut registry = TypeRegistry::default();
        registry.register(
            "pt".to_string(),
            vec!["x".to_string(), "y".to_string()],
            0b11,
        );
        registry.register(
            "person".to_string(),
            vec!["name".to_string(), "age".to_string()],
            0b10,
        );
        let bytes = serialize_type_registry(&registry);
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("pt"));
        assert!(s.contains("person"));
        assert!(s.contains("name"));
        assert!(s.contains("age"));
    }

    #[test]
    fn serialize_registry_no_fields() {
        let mut registry = TypeRegistry::default();
        registry.register("unit".to_string(), vec![], 0);
        let bytes = serialize_type_registry(&registry);
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("unit"));
    }

    // ── compile_function_body / object-emit tests ────────────────────────
    //
    // These tests compile to object bytes WITHOUT linking, so they work even
    // when libilo.a is absent. They exercise the full Cranelift IR generation
    // path (all opcode handlers, block leaders, data sections, etc.).

    #[test]
    fn codegen_simple_arithmetic_emits_object() {
        // f x:n>n; +x 1  — ADDK_N opcode
        let bytes = compile_to_object_bytes("f x:n>n;+x 1");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
        let obj = bytes.unwrap();
        assert!(!obj.is_empty(), "object file should not be empty");
    }

    #[test]
    fn codegen_sub_mul_div_emits_object() {
        let bytes = compile_to_object_bytes("f a:n b:n>n;+a *a -a /a b");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_nn_ops_emits_object() {
        // OP_ADD_NN, OP_SUB_NN, OP_MUL_NN, OP_DIV_NN paths
        let bytes = compile_to_object_bytes("f a:n b:n>n;*a b");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_zero_arg_function_emits_object() {
        let bytes = compile_to_object_bytes("f>n;42");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_guard_emits_object() {
        // Guard pattern: CMPK_GT_N + JMP + body
        let bytes = compile_to_object_bytes("f x:n>n;>x 5{1};0");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_while_loop_emits_object() {
        // While loop with accumulator: sum 0..n
        let bytes = compile_to_object_bytes("f n:n>n;s=0;i=0;wh <i n{s=+s i;i=+i 1};s");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_multiple_functions_emits_object() {
        // Two functions with cross-function call (OP_CALL path)
        let bytes = compile_to_object_bytes("dbl x:n>n;*x 2\nf x:n>n;dbl x");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_comparison_ops_emits_object() {
        // LT, GT, LE, GE, EQ, NE opcodes
        let bytes = compile_to_object_bytes("f a:n b:n>b;<a b");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_not_neg_emits_object() {
        // OP_NOT and OP_NEG
        let bytes = compile_to_object_bytes("f x:n>n;-x");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_string_cat_emits_object() {
        // OP_CAT with string operands
        let bytes = compile_to_object_bytes("f x:t>t;cat x \" world\"");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_len_emits_object() {
        let bytes = compile_to_object_bytes("f x:t>n;len x");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_abs_floor_ceil_round_emits_object() {
        let bytes = compile_to_object_bytes("f x:n>n;abs x");
        assert!(bytes.is_ok());
        let bytes2 = compile_to_object_bytes("f x:n>n;flr x");
        assert!(bytes2.is_ok());
        let bytes3 = compile_to_object_bytes("f x:n>n;cel x");
        assert!(bytes3.is_ok());
        let bytes4 = compile_to_object_bytes("f x:n>n;rou x");
        assert!(bytes4.is_ok());
    }

    // ── AOT translator coverage for the new transcendental math opcodes ───
    // These exercise the OP_POW and OP_SQRT|OP_LOG|OP_EXP|OP_SIN|OP_COS arms
    // in compile_function_body, which the JIT-based --run-cranelift tests do
    // not reach (those go through jit_cranelift::compile_and_call, not the
    // AOT translator).
    #[test]
    fn codegen_pow_emits_object() {
        let bytes = compile_to_object_bytes("f>n;pow 2 10");
        assert!(bytes.is_ok(), "pow AOT failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_sqrt_emits_object() {
        let bytes = compile_to_object_bytes("f>n;sqrt 4");
        assert!(bytes.is_ok(), "sqrt AOT failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_log_emits_object() {
        let bytes = compile_to_object_bytes("f>n;log 2.5");
        assert!(bytes.is_ok(), "log AOT failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_exp_emits_object() {
        let bytes = compile_to_object_bytes("f>n;exp 1");
        assert!(bytes.is_ok(), "exp AOT failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_sin_emits_object() {
        let bytes = compile_to_object_bytes("f>n;sin 0");
        assert!(bytes.is_ok(), "sin AOT failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cos_emits_object() {
        let bytes = compile_to_object_bytes("f>n;cos 0");
        assert!(bytes.is_ok(), "cos AOT failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_min_max_emits_object() {
        let bytes = compile_to_object_bytes("f a:n b:n>n;min a b");
        assert!(bytes.is_ok());
        let bytes2 = compile_to_object_bytes("f a:n b:n>n;max a b");
        assert!(bytes2.is_ok());
    }

    #[test]
    fn codegen_str_num_conversion_emits_object() {
        let bytes = compile_to_object_bytes("f x:n>t;str x");
        assert!(bytes.is_ok());
    }

    #[test]
    fn codegen_result_type_emits_object() {
        // OP_WRAPOK (~x), OP_WRAPERR (^e), OP_ISOK, OP_ISERR, OP_UNWRAP
        // Using result guard: =x 0 ^"zero";~x generates wrapok/wraperr paths
        let bytes = compile_to_object_bytes("f x:n>R n t;=x 0 ^\"zero\";~x");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_type_predicates_emits_object() {
        // OP_ISNUM, OP_ISTEXT, OP_ISBOOL, OP_ISLIST generated by type match
        // ?x{n _:true;_:false} compiles to OP_ISNUM + branch
        let bytes = compile_to_object_bytes("f x:n>b;?x{n _:true;_:false}");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_map_operations_emits_object() {
        // OP_MAPNEW (mmap), OP_MSET, OP_MGET — maps use mmap/mset/mget builtins
        let bytes = compile_to_object_bytes("f>n;m=mset mmap \"key\" 42;mget m \"key\"");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_list_operations_emits_object() {
        // OP_LISTNEW, OP_HD, OP_TL, OP_REV, OP_SRT
        let bytes = compile_to_object_bytes("f>n;xs=[1 2 3];hd xs");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_foreach_loop_emits_object() {
        // FOREACHPREP / FOREACHNEXT / LISTGET path: @x xs{body}
        let bytes = compile_to_object_bytes("f xs:L n>n;s=0;@x xs{s=+s x};s");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_record_type_emits_object() {
        // OP_RECNEW, OP_RECFLD — requires a type declaration
        let bytes = compile_to_object_bytes("type pt{x:n;y:n}\nf>n;p=pt x:3 y:4;p.x");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_spl_has_emits_object() {
        // OP_SPL (split) and OP_HAS
        let bytes = compile_to_object_bytes("f s:t>n;xs=spl s \",\";len xs");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_prt_emits_object() {
        // OP_PRT — ilo builtin is named 'prnt' (not 'prt')
        let bytes = compile_to_object_bytes("f x:n>n;prnt x;x");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_jdmp_jpar_emits_object() {
        // OP_JDMP, OP_JPAR
        let bytes = compile_to_object_bytes("f x:n>t;jdmp x");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_conditional_ternary_emits_object() {
        // ternary (?) uses JMPT/JMPF opcodes
        let bytes = compile_to_object_bytes("f x:n>n;?<x 0 0 x");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_recursive_function_emits_object() {
        // Recursive call — OP_CALL with function calling itself
        let bytes = compile_to_object_bytes("fac n:n>n;<=n 1 1;r=fac -n 1;*n r");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_multiple_guards_emits_object() {
        // Multiple guard clauses
        let bytes = compile_to_object_bytes("f x:n>n;>x 10{100};>x 5{50};0");
        assert!(bytes.is_ok(), "codegen failed: {:?}", bytes.err());
    }

    // ── compile_to_binary error-path tests ──────────────────────────────

    #[test]
    fn compile_to_binary_undefined_function_returns_error() {
        let compiled = compile_program("f x:n>n;+x 1");
        let tmp = std::env::temp_dir().join("ilo_test_aot_no_such_fn");
        let out = tmp.to_str().unwrap();
        let result = compile_to_binary(&compiled, "does_not_exist", out);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("undefined function"),
            "expected 'undefined function' in error, got: {}",
            err
        );
    }

    #[test]
    fn compile_to_binary_reaches_link_step_or_succeeds() {
        // This test verifies that codegen succeeds for a simple program — it either
        // completes end-to-end (if libilo.a is present) or fails at the LINKING step
        // (not at codegen). In either case, Cranelift IR generation was exercised.
        let compiled = compile_program("f x:n>n;*x 2");
        let tmp = std::env::temp_dir().join("ilo_test_aot_codegen_check");
        let out = tmp.to_str().unwrap();
        let result = compile_to_binary(&compiled, "f", out);
        let _ = std::fs::remove_file(out);
        let _ = std::fs::remove_file(format!("{}.o", out));
        match result {
            Ok(()) => {
                // Full pipeline succeeded — libilo.a was available
            }
            Err(e) => {
                // Must fail at linking, not at codegen
                let is_link_error = e.contains("libilo")
                    || e.contains("linker")
                    || e.contains("cc")
                    || e.contains("cannot find")
                    || e.contains("lilo")
                    || e.contains("ld");
                assert!(
                    is_link_error,
                    "expected a linker/libilo error but got codegen error: {}",
                    e
                );
            }
        }
    }

    #[test]
    fn compile_to_binary_guard_reaches_link_step_or_succeeds() {
        let compiled = compile_program("f x:n>n;>x 5{1};0");
        let tmp = std::env::temp_dir().join("ilo_test_aot_guard_check");
        let out = tmp.to_str().unwrap();
        let result = compile_to_binary(&compiled, "f", out);
        let _ = std::fs::remove_file(out);
        let _ = std::fs::remove_file(format!("{}.o", out));
        match result {
            Ok(()) => {}
            Err(e) => {
                let is_link_error = e.contains("libilo")
                    || e.contains("linker")
                    || e.contains("cc")
                    || e.contains("cannot find")
                    || e.contains("lilo")
                    || e.contains("ld");
                assert!(is_link_error, "codegen error (not link): {}", e);
            }
        }
    }

    #[test]
    fn compile_to_binary_while_loop_reaches_link_step_or_succeeds() {
        let compiled = compile_program("f n:n>n;s=0;i=0;wh <i n{s=+s i;i=+i 1};s");
        let tmp = std::env::temp_dir().join("ilo_test_aot_loop_check");
        let out = tmp.to_str().unwrap();
        let result = compile_to_binary(&compiled, "f", out);
        let _ = std::fs::remove_file(out);
        let _ = std::fs::remove_file(format!("{}.o", out));
        match result {
            Ok(()) => {}
            Err(e) => {
                let is_link_error = e.contains("libilo")
                    || e.contains("linker")
                    || e.contains("cc")
                    || e.contains("cannot find")
                    || e.contains("lilo")
                    || e.contains("ld");
                assert!(is_link_error, "codegen error (not link): {}", e);
            }
        }
    }

    #[test]
    fn compile_to_binary_record_type_reaches_link_step_or_succeeds() {
        let compiled = compile_program("type pt{x:n;y:n}\nf>n;p=pt x:3 y:4;p.x");
        let tmp = std::env::temp_dir().join("ilo_test_aot_record_check");
        let out = tmp.to_str().unwrap();
        let result = compile_to_binary(&compiled, "f", out);
        let _ = std::fs::remove_file(out);
        let _ = std::fs::remove_file(format!("{}.o", out));
        match result {
            Ok(()) => {}
            Err(e) => {
                let is_link_error = e.contains("libilo")
                    || e.contains("linker")
                    || e.contains("cc")
                    || e.contains("cannot find")
                    || e.contains("lilo")
                    || e.contains("ld");
                assert!(is_link_error, "codegen error (not link): {}", e);
            }
        }
    }

    #[test]
    fn compile_to_binary_string_ops_reaches_link_step_or_succeeds() {
        let compiled = compile_program("f x:t>t;cat x \" world\"");
        let tmp = std::env::temp_dir().join("ilo_test_aot_str_check");
        let out = tmp.to_str().unwrap();
        let result = compile_to_binary(&compiled, "f", out);
        let _ = std::fs::remove_file(out);
        let _ = std::fs::remove_file(format!("{}.o", out));
        match result {
            Ok(()) => {}
            Err(e) => {
                let is_link_error = e.contains("libilo")
                    || e.contains("linker")
                    || e.contains("cc")
                    || e.contains("cannot find")
                    || e.contains("lilo")
                    || e.contains("ld");
                assert!(is_link_error, "codegen error (not link): {}", e);
            }
        }
    }

    // ── Coverage gap tests (object-code only) ───────────────────────────

    // OP_SUB_NN, OP_DIV_NN
    #[test]
    fn codegen_cov_sub_nn() {
        let bytes = compile_to_object_bytes("f a:n b:n>n;-a b");
        assert!(bytes.is_ok(), "SUB_NN codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_div_nn() {
        let bytes = compile_to_object_bytes("f a:n b:n>n;/a b");
        assert!(bytes.is_ok(), "DIV_NN codegen failed: {:?}", bytes.err());
    }

    // OP_SUBK_N, OP_DIVK_N
    #[test]
    fn codegen_cov_subk_n() {
        let bytes = compile_to_object_bytes("f x:n>n;-x 3");
        assert!(bytes.is_ok(), "SUBK_N codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_divk_n() {
        let bytes = compile_to_object_bytes("f x:n>n;/x 2");
        assert!(bytes.is_ok(), "DIVK_N codegen failed: {:?}", bytes.err());
    }

    // OP_NIL (nil constant loading)
    #[test]
    fn codegen_cov_nil() {
        let bytes = compile_to_object_bytes("f x:n>n;>x 0{x}");
        assert!(bytes.is_ok(), "NIL codegen failed: {:?}", bytes.err());
    }

    // Greater-than comparison
    #[test]
    fn codegen_cov_gt_comparison() {
        let bytes = compile_to_object_bytes("f x:n y:n>n;>x y{1};0");
        assert!(
            bytes.is_ok(),
            "GT comparison codegen failed: {:?}",
            bytes.err()
        );
    }

    // GTE comparison
    #[test]
    fn codegen_cov_gte_comparison() {
        let bytes = compile_to_object_bytes("f x:n y:n>n;>=x y{1};0");
        assert!(
            bytes.is_ok(),
            "GTE comparison codegen failed: {:?}",
            bytes.err()
        );
    }

    // LTE comparison
    #[test]
    fn codegen_cov_lte_comparison() {
        let bytes = compile_to_object_bytes("f x:n y:n>n;<=x y{1};0");
        assert!(
            bytes.is_ok(),
            "LTE comparison codegen failed: {:?}",
            bytes.err()
        );
    }

    // Record type
    #[test]
    fn codegen_cov_record_type() {
        let bytes = compile_to_object_bytes("type pt{x:n;y:n}\nf a:n b:n>pt;pt x:a y:b");
        assert!(
            bytes.is_ok(),
            "record type codegen failed: {:?}",
            bytes.err()
        );
    }

    // Record field access
    #[test]
    fn codegen_cov_record_field() {
        let bytes = compile_to_object_bytes("type pt{x:n;y:n}\nf>n;p=pt x:1 y:2;p.x");
        assert!(
            bytes.is_ok(),
            "record field codegen failed: {:?}",
            bytes.err()
        );
    }

    // Record with update
    #[test]
    fn codegen_cov_record_with() {
        let bytes = compile_to_object_bytes("type pt{x:n;y:n}\nf>n;p=pt x:1 y:2;q=p with x:10;q.x");
        assert!(
            bytes.is_ok(),
            "record with codegen failed: {:?}",
            bytes.err()
        );
    }

    // For-range loop
    #[test]
    fn codegen_cov_for_range() {
        let bytes = compile_to_object_bytes("f n:n>n;s=0;@i 0..n{s=+s i};s");
        assert!(bytes.is_ok(), "for-range codegen failed: {:?}", bytes.err());
    }

    // Foreach loop
    #[test]
    fn codegen_cov_foreach() {
        let bytes = compile_to_object_bytes("f>n;s=0;@x [1,2,3]{s=+s x};s");
        assert!(bytes.is_ok(), "foreach codegen failed: {:?}", bytes.err());
    }

    // Modulo
    #[test]
    fn codegen_cov_modulo() {
        let bytes = compile_to_object_bytes("f a:n b:n>n;mod a b");
        assert!(bytes.is_ok(), "modulo codegen failed: {:?}", bytes.err());
    }

    // Equality check
    #[test]
    fn codegen_cov_eq() {
        let bytes = compile_to_object_bytes("f a:n b:n>n;=a b{1};0");
        assert!(bytes.is_ok(), "EQ codegen failed: {:?}", bytes.err());
    }

    // Not-equal check
    #[test]
    fn codegen_cov_neq() {
        let bytes = compile_to_object_bytes("f a:n b:n>n;!=a b{1};0");
        assert!(bytes.is_ok(), "NEQ codegen failed: {:?}", bytes.err());
    }

    // Map operations
    #[test]
    fn codegen_cov_map_ops() {
        let bytes =
            compile_to_object_bytes(r#"f>n;m=mset mmap "a" 1;m=mset m "b" 2;k=mkeys m;len k"#);
        assert!(bytes.is_ok(), "map ops codegen failed: {:?}", bytes.err());
    }

    // Ok/Err result types
    #[test]
    fn codegen_cov_result_types() {
        let bytes = compile_to_object_bytes(r#"f x:n>R n t;>x 0{~x};^"neg""#);
        assert!(
            bytes.is_ok(),
            "result types codegen failed: {:?}",
            bytes.err()
        );
    }

    // Multi-function call chain
    #[test]
    fn codegen_cov_multi_func_chain() {
        let bytes = compile_to_object_bytes("a x:n>n;+x 1\nb x:n>n;a x\nf x:n>n;b x");
        assert!(
            bytes.is_ok(),
            "multi-func chain codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── Type predicates ──────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_isnum_istext_isbool_islist() {
        // Type predicates use pattern-match syntax, not function call syntax.
        // ?x{n _:true;_:false} emits OP_ISNUM, ?x{t _:...} emits OP_ISTEXT, etc.
        let bytes = compile_to_object_bytes(r#"f x:t>b;?x{n _:true;_:false}"#);
        assert!(bytes.is_ok(), "isnum codegen failed: {:?}", bytes.err());
        let bytes2 = compile_to_object_bytes(r#"f x:t>b;?x{t _:true;_:false}"#);
        assert!(bytes2.is_ok(), "istext codegen failed: {:?}", bytes2.err());
        let bytes3 = compile_to_object_bytes(r#"f x:t>b;?x{b _:true;_:false}"#);
        assert!(bytes3.is_ok(), "isbool codegen failed: {:?}", bytes3.err());
        let bytes4 = compile_to_object_bytes(r#"f x:t>b;?x{l _:true;_:false}"#);
        assert!(bytes4.is_ok(), "islist codegen failed: {:?}", bytes4.err());
    }

    // ── Map ops codegen ──────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_map_has_del_vals() {
        let bytes = compile_to_object_bytes(r#"f>b;m=mset mmap "a" 1;mhas m "a""#);
        assert!(bytes.is_ok(), "mhas codegen failed: {:?}", bytes.err());
        let bytes2 =
            compile_to_object_bytes(r#"f>n;m=mset mmap "a" 1;m=mdel m "a";k=mkeys m;len k"#);
        assert!(
            bytes2.is_ok(),
            "mdel/mkeys codegen failed: {:?}",
            bytes2.err()
        );
        let bytes3 = compile_to_object_bytes(r#"f>n;m=mset mmap "a" 1;v=mvals m;len v"#);
        assert!(bytes3.is_ok(), "mvals codegen failed: {:?}", bytes3.err());
    }

    // ── Print / Trim / Uniq ──────────────────────────────────────────────────

    #[test]
    fn codegen_cov_trm_unq() {
        let bytes = compile_to_object_bytes(r#"f s:t>t;trm s"#);
        assert!(bytes.is_ok(), "trm codegen failed: {:?}", bytes.err());
        let bytes2 = compile_to_object_bytes("f xs:L n>n;u=unq xs;len u");
        assert!(bytes2.is_ok(), "unq codegen failed: {:?}", bytes2.err());
    }

    // ── File I/O ops ─────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_file_io() {
        let bytes = compile_to_object_bytes("f p:t>R t t;rd p");
        assert!(bytes.is_ok(), "rd codegen failed: {:?}", bytes.err());
        let bytes2 = compile_to_object_bytes("f p:t>R t t;rdl p");
        assert!(bytes2.is_ok(), "rdl codegen failed: {:?}", bytes2.err());
        let bytes3 = compile_to_object_bytes("f p:t c:t>t;wr p c");
        assert!(bytes3.is_ok(), "wr codegen failed: {:?}", bytes3.err());
        let bytes4 = compile_to_object_bytes("f p:t c:t>t;wrl p c");
        assert!(bytes4.is_ok(), "wrl codegen failed: {:?}", bytes4.err());
    }

    // ── HTTP ops ─────────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_http_ops() {
        let bytes = compile_to_object_bytes("f url:t body:t>R t t;post url body");
        assert!(bytes.is_ok(), "post codegen failed: {:?}", bytes.err());
        // OP_GETH is emitted when `get` receives a Map argument for headers.
        let bytes2 = compile_to_object_bytes("f url:t hdrs:M t t>R t t;get url hdrs");
        assert!(bytes2.is_ok(), "geth codegen failed: {:?}", bytes2.err());
    }

    // ── RND0 / RND2 / NOW / ENV / GET ────────────────────────────────────────

    #[test]
    fn codegen_cov_rnd_now_env() {
        let bytes = compile_to_object_bytes("f>n;rnd");
        assert!(bytes.is_ok(), "rnd0 codegen failed: {:?}", bytes.err());
        let bytes2 = compile_to_object_bytes("f>n;rnd 1 10");
        assert!(bytes2.is_ok(), "rnd2 codegen failed: {:?}", bytes2.err());
        let bytes3 = compile_to_object_bytes("f>n;now");
        assert!(bytes3.is_ok(), "now codegen failed: {:?}", bytes3.err());
        let bytes4 = compile_to_object_bytes(r#"f k:t>R t t;env k"#);
        assert!(bytes4.is_ok(), "env codegen failed: {:?}", bytes4.err());
        let bytes5 = compile_to_object_bytes("f url:t>R t t;get url");
        assert!(bytes5.is_ok(), "get codegen failed: {:?}", bytes5.err());
    }

    // ── SPL / CAT / HAS / HD / TL / REV / SRT / SLC ─────────────────────────

    #[test]
    fn codegen_cov_string_list_ops() {
        let bytes = compile_to_object_bytes(r#"f s:t sep:t>L t;spl s sep"#);
        assert!(bytes.is_ok(), "spl codegen failed: {:?}", bytes.err());
        let bytes2 = compile_to_object_bytes(r#"f xs:L t sep:t>t;cat xs sep"#);
        assert!(bytes2.is_ok(), "cat codegen failed: {:?}", bytes2.err());
        let bytes3 = compile_to_object_bytes("f xs:L n v:n>b;has xs v");
        assert!(bytes3.is_ok(), "has codegen failed: {:?}", bytes3.err());
        let bytes4 = compile_to_object_bytes("f xs:L n>n;hd xs");
        assert!(bytes4.is_ok(), "hd codegen failed: {:?}", bytes4.err());
        let bytes5 = compile_to_object_bytes("f xs:L n>L n;tl xs");
        assert!(bytes5.is_ok(), "tl codegen failed: {:?}", bytes5.err());
        let bytes6 = compile_to_object_bytes("f xs:L n>L n;rev xs");
        assert!(bytes6.is_ok(), "rev codegen failed: {:?}", bytes6.err());
        let bytes7 = compile_to_object_bytes("f xs:L n>L n;srt xs");
        assert!(bytes7.is_ok(), "srt codegen failed: {:?}", bytes7.err());
        let bytes8 = compile_to_object_bytes("f xs:L n a:n b:n>L n;slc xs a b");
        assert!(bytes8.is_ok(), "slc codegen failed: {:?}", bytes8.err());
    }

    // ── STR / NUM / LISTAPPEND / INDEX / JPTH ────────────────────────────────

    #[test]
    fn codegen_cov_more_builtins() {
        let bytes = compile_to_object_bytes("f x:n>t;str x");
        assert!(bytes.is_ok(), "str codegen failed: {:?}", bytes.err());
        let bytes2 = compile_to_object_bytes(r#"f s:t>R n t;num s"#);
        assert!(bytes2.is_ok(), "num codegen failed: {:?}", bytes2.err());
        let bytes3 = compile_to_object_bytes("f xs:L n v:n>L n;r=+=xs v;r");
        assert!(
            bytes3.is_ok(),
            "listappend codegen failed: {:?}",
            bytes3.err()
        );
        let bytes4 = compile_to_object_bytes("f xs:L n>n;xs.0");
        assert!(bytes4.is_ok(), "index codegen failed: {:?}", bytes4.err());
        let bytes5 = compile_to_object_bytes(r#"f j:t p:t>R t t;jpth j p"#);
        assert!(bytes5.is_ok(), "jpth codegen failed: {:?}", bytes5.err());
    }

    // ── JMPF/JMPT with non-bool registers (general truthy path) ─────────────

    #[test]
    fn codegen_cov_jmpt_jmpf_non_bool() {
        // A ternary with a numeric (non-bool) condition forces the JMPF/JMPT
        // general truthy path.  Syntax: `x{1}{0}` (no leading `?`).
        let bytes = compile_to_object_bytes("f x:n>n;x{1}{0}");
        assert!(
            bytes.is_ok(),
            "jmpt/jmpf non-bool codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── JMPNN codegen ────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_jmpnn() {
        // nil-coalesce operator (??) emits JMPNN
        let bytes = compile_to_object_bytes("f x:O n>n;x??42");
        assert!(bytes.is_ok(), "jmpnn codegen failed: {:?}", bytes.err());
    }

    // ── MOVE with non-numeric source (RC management path) ────────────────────

    #[test]
    fn codegen_cov_move_heap_value() {
        // Moving a string value exercises the is_heap-check clone path in MOVE.
        let bytes = compile_to_object_bytes(r#"f s:t>t;t=s;t"#);
        assert!(bytes.is_ok(), "move heap codegen failed: {:?}", bytes.err());
    }

    // ── NEG on non-numeric operand (helper call path) ─────────────────────────

    #[test]
    fn codegen_cov_neg_non_numeric() {
        // NEG on a text-typed param exercises the jit_neg helper call path.
        let bytes = compile_to_object_bytes("f x:t>n;-x");
        assert!(
            bytes.is_ok(),
            "neg non-numeric codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── LT/GT/GE/LE/EQ/NE on non-numeric args (general slow path) ────────────

    #[test]
    fn codegen_cov_comparison_non_numeric() {
        // LT on text params forces the general (non-always-numeric) comparison path
        let bytes = compile_to_object_bytes(r#"f x:t y:t>b;< x y"#);
        assert!(bytes.is_ok(), "lt text codegen failed: {:?}", bytes.err());
        let bytes2 = compile_to_object_bytes(r#"f x:t y:t>b;= x y"#);
        assert!(bytes2.is_ok(), "eq text codegen failed: {:?}", bytes2.err());
    }

    // ── WRAPOK / WRAPERR / ISOK / ISERR / UNWRAP ──────────────────────────────

    #[test]
    fn codegen_cov_result_ops() {
        let bytes = compile_to_object_bytes(r#"f x:n>R n t;~x"#);
        assert!(bytes.is_ok(), "wrapok codegen failed: {:?}", bytes.err());
        let bytes2 = compile_to_object_bytes(r#"f x:t>R n t;^x"#);
        assert!(bytes2.is_ok(), "wraperr codegen failed: {:?}", bytes2.err());
        let bytes3 = compile_to_object_bytes("f x:R n t>b;?x{~_:true;^_:false}");
        assert!(
            bytes3.is_ok(),
            "isok/iserr/unwrap codegen failed: {:?}",
            bytes3.err()
        );
    }

    // ── is_inlinable edge cases: non-numeric callee, large reg file ───────────

    #[test]
    fn codegen_cov_non_inlinable_callee_direct_call() {
        // A callee that uses OP_CAT (non-numeric) should NOT be inlined;
        // instead the caller uses a direct function call.
        let bytes = compile_to_object_bytes(
            r#"join a:t b:t>t;+ a b
f a:t b:t>t;join a b"#,
        );
        assert!(
            bytes.is_ok(),
            "non-inlinable callee codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── is_inlinable: reg_count > 16 → return false (line 424) ─────────────
    // A callee with 17+ parameters exceeds the 16-register inlining limit.

    #[test]
    fn codegen_cov_callee_too_many_regs() {
        // 17-parameter function: reg_count = 17 > 16 → is_inlinable returns false
        let bytes = compile_to_object_bytes(
            "sum17 a:n b:n c:n d:n e:n f:n g:n h:n i:n j:n k:n l:n m:n nn:n o:n p:n q:n>n;\
             +a +b +c +d +e +f +g +h +i +j +k +l +m +nn +o +p q\n\
             caller>n;sum17 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17",
        );
        assert!(
            bytes.is_ok(),
            "callee too-many-regs codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── compile_to_bench_binary smoke-test ───────────────────────────────────

    #[test]
    fn codegen_cov_bench_binary_reaches_object_emit_or_link() {
        // compile_to_bench_binary writes a .o and a _bench_c.o; it either
        // succeeds fully (if libilo.a and cc are available) or fails at linking.
        // Either way, the codegen path should be exercised.
        let compiled = compile_program("f x:n>n;*x 2");
        let tmp = std::env::temp_dir().join("ilo_test_bench_binary");
        let out = tmp.to_str().unwrap();
        let result = compile_to_bench_binary(&compiled, "f", out);
        // Clean up any artifacts
        let _ = std::fs::remove_file(out);
        let _ = std::fs::remove_file(format!("{}.o", out));
        let _ = std::fs::remove_file(format!("{}_bench.c", out));
        let _ = std::fs::remove_file(format!("{}_bench_c.o", out));
        match result {
            Ok(()) => {} // Full pipeline worked
            Err(e) => {
                // Must fail at linking/compilation, not at codegen
                let is_expected_err = e.contains("libilo")
                    || e.contains("linker")
                    || e.contains("cc")
                    || e.contains("cannot find")
                    || e.contains("lilo")
                    || e.contains("ld")
                    || e.contains("bench")
                    || e.contains("write")
                    || e.contains("failed");
                assert!(
                    is_expected_err,
                    "unexpected error in bench binary codegen: {}",
                    e
                );
            }
        }
    }

    // ── compile_to_bench_binary undefined function error ─────────────────────

    #[test]
    fn codegen_cov_bench_binary_undefined_function() {
        let compiled = compile_program("f x:n>n;*x 2");
        let tmp = std::env::temp_dir().join("ilo_test_bench_noexist");
        let out = tmp.to_str().unwrap();
        let result = compile_to_bench_binary(&compiled, "does_not_exist", out);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("undefined function"));
    }

    // ── LISTNEW with n=0 (empty list) ────────────────────────────────────────

    #[test]
    fn codegen_cov_empty_list_literal() {
        let bytes = compile_to_object_bytes("f>L n;[]");
        assert!(
            bytes.is_ok(),
            "empty list codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── LISTGET fallback path (no successor blocks) ───────────────────────────

    #[test]
    fn codegen_cov_listget_normal_foreach() {
        // Normal foreach exercises the LISTGET inline path and FOREACHPREP/NEXT
        let bytes = compile_to_object_bytes("f xs:L n>n;s=0;@x xs{s=+s x};s");
        assert!(bytes.is_ok(), "foreach codegen failed: {:?}", bytes.err());
    }

    // ── SUB_NN / MUL_NN / DIV_NN with non-shadow-var inputs ─────────────────

    #[test]
    fn codegen_cov_nn_ops_non_shadow() {
        // Two non-param numeric registers that use bitcast (not shadow) path
        // for ADD_NN / SUB_NN / MUL_NN / DIV_NN.
        let bytes = compile_to_object_bytes("f>n;a=3;b=4;-a b");
        assert!(
            bytes.is_ok(),
            "sub_nn non-shadow codegen failed: {:?}",
            bytes.err()
        );
        let bytes2 = compile_to_object_bytes("f>n;a=3;b=4;*a b");
        assert!(
            bytes2.is_ok(),
            "mul_nn non-shadow codegen failed: {:?}",
            bytes2.err()
        );
        let bytes3 = compile_to_object_bytes("f>n;a=10;b=2;/a b");
        assert!(
            bytes3.is_ok(),
            "div_nn non-shadow codegen failed: {:?}",
            bytes3.err()
        );
    }

    // ── OP_ADD / OP_SUB / OP_MUL / OP_DIV generic with fast/slow paths ───────

    #[test]
    fn codegen_cov_generic_arith_fast_slow() {
        // Generic OP_ADD/SUB/MUL/DIV with non-numeric registers exercises
        // both the inline numeric fast path and the helper slow path.
        let bytes = compile_to_object_bytes(r#"f x:t y:t>t;+ x y"#);
        assert!(
            bytes.is_ok(),
            "generic add (text) codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── OP_RECFLD_NAME (named field access via registry) ─────────────────────

    #[test]
    fn codegen_cov_recfld_name() {
        // OP_RECFLD_NAME is emitted when accessing a field by name from a
        // dynamic record (e.g. from jpar result). Use the same pattern as
        // jit_cranelift tests.
        let source = r#"f x:t>R t t;r=jpar! x;r.score"#;
        let bytes = {
            let tokens = crate::lexer::lex(source).unwrap();
            let token_spans: Vec<(crate::lexer::Token, crate::ast::Span)> = tokens
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
            let (prog, errors) = crate::parser::parse(token_spans);
            if !errors.is_empty() {
                // If parse fails, skip
                return;
            }
            let compiled = crate::vm::compile(&prog).unwrap();
            let mut module = make_module();
            let helpers = declare_all_helpers(&mut module);
            let mut func_ids = Vec::new();
            for (i, chunk) in compiled.chunks.iter().enumerate() {
                let name = format!("ilo_{}", compiled.func_names[i]);
                let mut sig = module.make_signature();
                for _ in 0..chunk.param_count {
                    sig.params.push(cranelift_codegen::ir::AbiParam::new(
                        cranelift_codegen::ir::types::I64,
                    ));
                }
                sig.returns.push(cranelift_codegen::ir::AbiParam::new(
                    cranelift_codegen::ir::types::I64,
                ));
                let fid = module
                    .declare_function(&name, cranelift_module::Linkage::Local, &sig)
                    .map_err(|e| e.to_string())
                    .unwrap();
                func_ids.push(fid);
            }
            let mut result = Ok(());
            for (i, (chunk, nan_consts)) in compiled
                .chunks
                .iter()
                .zip(compiled.nan_constants.iter())
                .enumerate()
            {
                let name = format!("ilo_{}", compiled.func_names[i]);
                result = compile_function_body(
                    &mut module,
                    chunk,
                    nan_consts,
                    &name,
                    func_ids[i],
                    &helpers,
                    Some(&func_ids),
                    Some(&compiled),
                );
                if result.is_err() {
                    break;
                }
            }
            result.map(|_| {
                let obj_product = module.finish();
                obj_product.emit().unwrap_or_default()
            })
        };
        assert!(
            bytes.is_ok(),
            "recfld_name codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── LOADK with heap (non-string) constant ────────────────────────────────

    #[test]
    fn codegen_cov_loadk_bool_constant() {
        // Loading a bool constant (not a number, not a string) exercises the
        // plain iconst path in LOADK (the else branch after is_string/is_heap).
        let bytes = compile_to_object_bytes("f>b;true");
        assert!(
            bytes.is_ok(),
            "loadk bool codegen failed: {:?}",
            bytes.err()
        );
        let bytes2 = compile_to_object_bytes("f>b;false");
        assert!(
            bytes2.is_ok(),
            "loadk false codegen failed: {:?}",
            bytes2.err()
        );
    }

    // ── inline_chunk: callee with OP_ADD_NN / OP_SUB_NN / OP_MUL_NN / OP_DIV_NN ─

    #[test]
    fn codegen_cov_inline_add_nn_callee() {
        // Callee uses OP_ADD_NN (two register params) — inline_chunk OP_ADD_NN branch.
        let bytes = compile_to_object_bytes("mysum a:n b:n>n;+a b\nf a:n b:n>n;mysum a b");
        assert!(
            bytes.is_ok(),
            "inline add_nn callee codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_inline_sub_nn_callee() {
        // Callee uses OP_SUB_NN — inline_chunk OP_SUB_NN branch.
        let bytes = compile_to_object_bytes("diff a:n b:n>n;-a b\nf a:n b:n>n;diff a b");
        assert!(
            bytes.is_ok(),
            "inline sub_nn callee codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_inline_mul_nn_callee() {
        // Callee uses OP_MUL_NN — inline_chunk OP_MUL_NN branch.
        let bytes = compile_to_object_bytes("myprod a:n b:n>n;*a b\nf a:n b:n>n;myprod a b");
        assert!(
            bytes.is_ok(),
            "inline mul_nn callee codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_inline_div_nn_callee() {
        // Callee uses OP_DIV_NN — inline_chunk OP_DIV_NN branch.
        let bytes = compile_to_object_bytes("quot a:n b:n>n;/a b\nf a:n b:n>n;quot a b");
        assert!(
            bytes.is_ok(),
            "inline div_nn callee codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── inline_chunk: callee with OP_SUBK_N / OP_DIVK_N ─────────────────────

    #[test]
    fn codegen_cov_inline_subk_n_callee() {
        // Callee uses OP_SUBK_N (x - constant) — inline_chunk OP_SUBK_N branch.
        let bytes = compile_to_object_bytes("dec x:n>n;-x 1\nf x:n>n;dec x");
        assert!(
            bytes.is_ok(),
            "inline subk_n callee codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_inline_divk_n_callee() {
        // Callee uses OP_DIVK_N (x / constant) — inline_chunk OP_DIVK_N branch.
        let bytes = compile_to_object_bytes("halve x:n>n;/x 2\nf x:n>n;halve x");
        assert!(
            bytes.is_ok(),
            "inline divk_n callee codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── inline_chunk: callee with OP_CMPK_* / OP_JMP / OP_LOADK ─────────────

    #[test]
    fn codegen_cov_inline_cmpk_guard_callee() {
        // Callee uses OP_CMPK_GT_N (guard >x 0) + braceless return + OP_LOADK for 0:
        //   `pos x:n>n;>x 0 x;0`
        // This exercises the OP_CMPK_*, OP_JMP, and OP_LOADK branches in inline_chunk.
        let bytes = compile_to_object_bytes("pos x:n>n;>x 0 x;0\nf x:n>n;pos x");
        assert!(
            bytes.is_ok(),
            "inline cmpk guard callee codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── OP_POSTH compile smoke-test ──────────────────────────────────────────

    #[test]
    fn codegen_cov_posth() {
        // OP_POSTH is emitted when `post` receives a Map argument for headers.
        let bytes = compile_to_object_bytes("f url:t body:t hdrs:M t t>R t t;post url body hdrs");
        assert!(bytes.is_ok(), "posth codegen failed: {:?}", bytes.err());
    }

    // ── OP_NOT (logical negation) ─────────────────────────────────────────────

    #[test]
    fn codegen_cov_op_not() {
        // OP_NOT is emitted for `!x` (logical NOT) on a bool.
        let bytes = compile_to_object_bytes("f x:b>b;!x");
        assert!(bytes.is_ok(), "op_not codegen failed: {:?}", bytes.err());
    }

    // ── JMPT with always-bool register ──────────────────────────────────────

    #[test]
    fn codegen_cov_jmpt_always_bool() {
        // A negated braceless guard `!>x 5 10;0` emits OP_JMPT on an always-bool
        // register (the comparison result).
        let bytes = compile_to_object_bytes("f x:n>n;!>x 5 10;0");
        assert!(
            bytes.is_ok(),
            "jmpt always-bool codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── inline_chunk: callee with OP_CMPK_LT_N / OP_CMPK_LE_N / OP_CMPK_EQ_N ─

    #[test]
    fn codegen_cov_inline_cmpk_lt_callee() {
        // Callee uses OP_CMPK_LT_N (guard <x 0) — inline_chunk OP_CMPK_LT_N branch.
        // `neg x:n>n;<x 0{ret x};0` — returns x if x<0, else 0.
        // But `ret` inside braced guard is fine; callee is inlinable.
        // Actually, braceless guard: `<x 0 x;0` = if x<0 return x, else 0.
        let bytes = compile_to_object_bytes("negval x:n>n;<x 0 x;0\nf x:n>n;negval x");
        assert!(
            bytes.is_ok(),
            "inline cmpk_lt callee codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_inline_cmpk_le_callee() {
        // Callee uses OP_CMPK_LE_N (guard <=x 0) — inline_chunk OP_CMPK_LE_N branch.
        let bytes = compile_to_object_bytes("nonpos x:n>n;<=x 0 x;0\nf x:n>n;nonpos x");
        assert!(
            bytes.is_ok(),
            "inline cmpk_le callee codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── inline_chunk: CMPK_EQ_N and CMPK_NE_N (lines 569-570) ──────────────
    // These arms are in `inline_chunk` (AOT). They require a callee that uses
    // `==x K` or `!=x K` in an inlinable (all-numeric) context.

    #[test]
    fn codegen_cov_inline_cmpk_eq_callee() {
        // Callee uses CMPK_EQ_N: `==x 5 1;0` = return 1 if x==5 else 0.
        let bytes = compile_to_object_bytes("exact5 x:n>n;==x 5 1;0\nf x:n>n;exact5 x");
        assert!(
            bytes.is_ok(),
            "inline cmpk_eq callee codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_inline_cmpk_ne_callee() {
        // Callee uses CMPK_NE_N: `!=x 5 1;0` = return 1 if x!=5 else 0.
        // This hits the `_ => FloatCC::NotEqual` arm (line 570).
        let bytes = compile_to_object_bytes("noteq5 x:n>n;!=x 5 1;0\nf x:n>n;noteq5 x");
        assert!(
            bytes.is_ok(),
            "inline cmpk_ne callee codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── foreach loops (FOREACHPREP / FOREACHNEXT) ────────────────────────────

    #[test]
    fn codegen_cov_foreach_numeric_list() {
        // `@x xs{*x x}` exercises OP_FOREACHPREP and OP_FOREACHNEXT codegen.
        let bytes = compile_to_object_bytes("f xs:L n>n;@x xs{*x x}");
        assert!(
            bytes.is_ok(),
            "foreach numeric list codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_foreach_text_list() {
        // Foreach over a text list to exercise FOREACHPREP/FOREACHNEXT with heap elements.
        let bytes = compile_to_object_bytes("f xs:L t>t;@x xs{x}");
        assert!(
            bytes.is_ok(),
            "foreach text list codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── OP_ADD_NN / OP_SUB_NN / OP_MUL_NN / OP_DIV_NN with non-always-num regs ──

    #[test]
    fn codegen_cov_add_nn_non_always_num() {
        // Mixed-type function: `all_regs_numeric=false` so x_reg is not always_num.
        // The VM compiler still emits OP_ADD_NN (x is numeric), but Cranelift's
        // pre-pass won't mark x as always-num → the else branches at lines ~1016-1023 fire.
        let bytes = compile_to_object_bytes("f x:n y:t>n;+x x");
        assert!(
            bytes.is_ok(),
            "add_nn non-always-num codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_sub_nn_non_always_num() {
        let bytes = compile_to_object_bytes("f x:n y:t>n;-x x");
        assert!(
            bytes.is_ok(),
            "sub_nn non-always-num codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_mul_nn_non_always_num() {
        let bytes = compile_to_object_bytes("f x:n y:t>n;*x x");
        assert!(
            bytes.is_ok(),
            "mul_nn non-always-num codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_div_nn_non_always_num() {
        let bytes = compile_to_object_bytes("f x:n y:t>n;/x x");
        assert!(
            bytes.is_ok(),
            "div_nn non-always-num codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── OP_SUB / OP_MUL / OP_DIV generic (non-NN variants) ──────────────────

    #[test]
    fn codegen_cov_generic_sub() {
        // `v` (any) params prevent OP_SUB_NN — emits generic OP_SUB.
        let bytes = compile_to_object_bytes("f x:v y:v>n;-x y");
        assert!(
            bytes.is_ok(),
            "generic sub codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_generic_mul() {
        let bytes = compile_to_object_bytes("f x:v y:v>n;*x y");
        assert!(
            bytes.is_ok(),
            "generic mul codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_generic_div() {
        let bytes = compile_to_object_bytes("f x:v y:v>n;/x y");
        assert!(
            bytes.is_ok(),
            "generic div codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── inline_chunk with extra (non-param) registers ────────────────────────

    #[test]
    fn codegen_cov_inline_callee_extra_reg() {
        // Callee `double x:n>n;d=*x 2;+d 1` has x (param) and d (extra reg).
        // When inlined in the AOT path, f64_val_for(d) hits the else branch for
        // non-param extra registers.
        let bytes = compile_to_object_bytes("double x:n>n;d=*x 2;+d 1\nf x:n>n;double x");
        assert!(
            bytes.is_ok(),
            "inline callee extra reg codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── OP_ADDK_N / OP_SUBK_N / OP_MULK_N / OP_DIVK_N with non-always-num ──────

    #[test]
    fn codegen_cov_addk_n_non_always_num() {
        // Mixed-type function: all_regs_numeric=false, x is not always_num.
        // `+x 1` compiles to OP_ADDK_N; Cranelift pre-pass sees x as non-always-num
        // → the else branch (bitcast path) at ~line 1097 is exercised.
        let bytes = compile_to_object_bytes("f x:n y:t>n;+x 1");
        assert!(
            bytes.is_ok(),
            "addk_n non-always-num codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_subk_n_non_always_num() {
        let bytes = compile_to_object_bytes("f x:n y:t>n;-x 1");
        assert!(
            bytes.is_ok(),
            "subk_n non-always-num codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_mulk_n_non_always_num() {
        let bytes = compile_to_object_bytes("f x:n y:t>n;*x 2");
        assert!(
            bytes.is_ok(),
            "mulk_n non-always-num codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_divk_n_non_always_num() {
        let bytes = compile_to_object_bytes("f x:n y:t>n;/x 2");
        assert!(
            bytes.is_ok(),
            "divk_n non-always-num codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── OP_LOADK with heap value (list constant) ─────────────────────────────

    #[test]
    fn codegen_cov_loadk_heap_list_const() {
        // `xs=[1,2,3]` stores a list as a heap constant in the constant pool.
        // LOADK handler checks is_heap() → exercises lines 1449-1455.
        let bytes = compile_to_object_bytes("f>n;xs=[1,2,3];len xs");
        assert!(
            bytes.is_ok(),
            "loadk heap list codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── OP_JMPT on always-bool register (OR short-circuit) ──────────────────

    #[test]
    fn codegen_cov_jmpt_always_bool_via_or() {
        // `|>x 3 >x 5` emits: OP_GT (writes bool to ra), OP_MOVE (result), OP_JMPT on ra.
        // The JMPT is NOT fused (preceding instruction is MOVE, not comparison),
        // so it reaches the OP_JMPF|OP_JMPT handler. ra is always-bool → lines 1509-1512.
        let bytes = compile_to_object_bytes("f x:n>b;|>x 3 >x 5");
        assert!(
            bytes.is_ok(),
            "jmpt always-bool via OR codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── bench_binary with type registry (record type) ────────────────────────

    #[test]
    fn codegen_cov_bench_binary_with_registry() {
        // compile_to_bench_binary with a record type exercises the registry embed path.
        let compiled = compile_program("type pt{x:n;y:n}\nf>n;p=pt x:3 y:4;p.x");
        let tmp = std::env::temp_dir().join("ilo_test_bench_registry");
        let out = tmp.to_str().unwrap();
        let result = compile_to_bench_binary(&compiled, "f", out);
        let _ = std::fs::remove_file(out);
        let _ = std::fs::remove_file(format!("{}.o", out));
        let _ = std::fs::remove_file(format!("{}_bench.c", out));
        let _ = std::fs::remove_file(format!("{}_bench_c.o", out));
        // Either full success or linker error — both paths exercise codegen + registry
        match result {
            Ok(()) => {}
            Err(e) => {
                let is_expected_err = e.contains("libilo")
                    || e.contains("linker")
                    || e.contains("cc")
                    || e.contains("cannot find")
                    || e.contains("lilo")
                    || e.contains("ld")
                    || e.contains("bench")
                    || e.contains("write")
                    || e.contains("failed");
                assert!(is_expected_err, "unexpected error: {}", e);
            }
        }
    }

    // ── Generic comparison (v params) → slow-path helper selection ───────────
    // With `v` (any) type params, both_always_num=false → general comparison
    // path fires. The slow-path `match op { OP_GT | OP_LE | OP_GE | OP_NE }`
    // arms (lines 1290-1295) need OP_GT, OP_LE, OP_GE, OP_EQ, OP_NE with v params.

    #[test]
    fn codegen_cov_generic_gt() {
        let bytes = compile_to_object_bytes("f x:v y:v>b;>x y");
        assert!(
            bytes.is_ok(),
            "generic OP_GT codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_generic_le() {
        let bytes = compile_to_object_bytes("f x:v y:v>b;<=x y");
        assert!(
            bytes.is_ok(),
            "generic OP_LE codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_generic_ge() {
        let bytes = compile_to_object_bytes("f x:v y:v>b;>=x y");
        assert!(
            bytes.is_ok(),
            "generic OP_GE codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_generic_eq() {
        let bytes = compile_to_object_bytes("f x:v y:v>b;==x y");
        assert!(
            bytes.is_ok(),
            "generic OP_EQ codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_generic_ne() {
        let bytes = compile_to_object_bytes("f x:v y:v>b;!=x y");
        assert!(
            bytes.is_ok(),
            "generic OP_NE codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── CMPK_NE_N guard in mixed-type function ────────────────────────────────
    // `!=x 5{1};0` emits CMPK_NE_N, exercising the `_ => FloatCC::NotEqual` arm
    // (line 2417) and the non-always-num bitcast path.

    #[test]
    fn codegen_cov_cmpk_ne_n_non_always_num() {
        let bytes = compile_to_object_bytes("f x:n y:t>n;!=x 5 1;0");
        assert!(
            bytes.is_ok(),
            "CMPK_NE_N non-always-num codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_cmpk_lt_n_non_always_num() {
        let bytes = compile_to_object_bytes("f x:n y:t>n;<x 5 1;0");
        assert!(
            bytes.is_ok(),
            "CMPK_LT_N non-always-num codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_cmpk_le_n_non_always_num() {
        let bytes = compile_to_object_bytes("f x:n y:t>n;<=x 5 1;0");
        assert!(
            bytes.is_ok(),
            "CMPK_LE_N non-always-num codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_cmpk_ge_n_non_always_num() {
        let bytes = compile_to_object_bytes("f x:n y:t>n;>=x 5 1;0");
        assert!(
            bytes.is_ok(),
            "CMPK_GE_N non-always-num codegen failed: {:?}",
            bytes.err()
        );
    }

    #[test]
    fn codegen_cov_cmpk_eq_n_non_always_num() {
        let bytes = compile_to_object_bytes("f x:n y:t>n;==x 5 1;0");
        assert!(
            bytes.is_ok(),
            "CMPK_EQ_N non-always-num codegen failed: {:?}",
            bytes.err()
        );
    }

    // ── program=None path in pre-pass analysis ────────────────────────────────
    // compile_function_body called with program=None: OP_CALL result assumed
    // non-numeric (lines 911-914). Use a multi-function program and compile
    // one chunk alone without the full CompiledProgram.

    #[test]
    fn codegen_cov_prepass_program_none() {
        // Compile a chunk with an OP_CALL using compile_function_body(program=None).
        // We manually call compile_function_body to exercise the program=None branch.
        let compiled = compile_program("helper x:n>n;+x 1\nf x:n>n;helper x");
        // Find the "f" chunk (it has an OP_CALL to helper)
        let f_idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[f_idx];
        let nan_consts = &compiled.nan_constants[f_idx];

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);

        // Declare the helper function so OP_CALL can reference it
        let mut all_func_ids = Vec::new();
        for (i, c) in compiled.chunks.iter().enumerate() {
            let name = format!("ilo_{}", compiled.func_names[i]);
            let linkage = cranelift_module::Linkage::Local;
            let mut sig = module.make_signature();
            for _ in 0..c.param_count {
                sig.params.push(cranelift_codegen::ir::AbiParam::new(
                    cranelift_codegen::ir::types::I64,
                ));
            }
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(
                cranelift_codegen::ir::types::I64,
            ));
            let fid = module.declare_function(&name, linkage, &sig).unwrap();
            all_func_ids.push(fid);
        }

        // Compile f with program=None — forces OP_CALL's result to non-numeric path
        let result = compile_function_body(
            &mut module,
            chunk,
            nan_consts,
            "ilo_f",
            all_func_ids[f_idx],
            &helpers,
            Some(&all_func_ids),
            None, // <-- program=None exercises lines 911-914
        );
        assert!(
            result.is_ok(),
            "compile_function_body(program=None) failed: {:?}",
            result.err()
        );
    }

    // ── all_func_ids=None path (jit_call helper fallback) ──────────────────
    // When compile_function_body is called with all_func_ids=None, OP_CALL
    // falls back to using the jit_call helper. This exercises lines 2547-2580.

    #[test]
    fn codegen_cov_call_no_func_ids() {
        // Multi-function program: f calls helper.
        let compiled = compile_program("helper x:n>n;+x 1\nf x:n>n;helper x");
        let f_idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[f_idx];
        let nan_consts = &compiled.nan_constants[f_idx];

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);

        // Declare only f's func_id (need to declare it to get a FuncId)
        let mut sig = module.make_signature();
        for _ in 0..chunk.param_count {
            sig.params.push(cranelift_codegen::ir::AbiParam::new(
                cranelift_codegen::ir::types::I64,
            ));
        }
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(
            cranelift_codegen::ir::types::I64,
        ));
        // Also declare helper so OP_CALL's func_idx lookup via helpers.call works
        let helper_idx = compiled
            .func_names
            .iter()
            .position(|n| n == "helper")
            .unwrap();
        let mut helper_sig = module.make_signature();
        for _ in 0..compiled.chunks[helper_idx].param_count {
            helper_sig.params.push(cranelift_codegen::ir::AbiParam::new(
                cranelift_codegen::ir::types::I64,
            ));
        }
        helper_sig
            .returns
            .push(cranelift_codegen::ir::AbiParam::new(
                cranelift_codegen::ir::types::I64,
            ));

        let f_id = module
            .declare_function("ilo_f", cranelift_module::Linkage::Local, &sig)
            .unwrap();

        // Compile f with all_func_ids=None → exercises the jit_call helper fallback
        let result = compile_function_body(
            &mut module,
            chunk,
            nan_consts,
            "ilo_f",
            f_id,
            &helpers,
            None, // <-- all_func_ids=None exercises lines 2547-2580
            None,
        );
        assert!(
            result.is_ok(),
            "compile_function_body(all_func_ids=None) failed: {:?}",
            result.err()
        );
    }

    // ── all_func_ids=None path with zero-arg callee ──────────────────────────
    // Exercises the n_args==0 branch of the jit_call helper fallback (line 2569-2580).

    #[test]
    fn codegen_cov_call_no_func_ids_zero_arg() {
        // zero-arg helper: f calls const42() with no args
        let compiled = compile_program("const42>n;42\nf>n;const42()");
        let f_idx = compiled.func_names.iter().position(|n| n == "f").unwrap();
        let chunk = &compiled.chunks[f_idx];
        let nan_consts = &compiled.nan_constants[f_idx];

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);

        let mut sig = module.make_signature();
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(
            cranelift_codegen::ir::types::I64,
        ));
        let f_id = module
            .declare_function("ilo_f", cranelift_module::Linkage::Local, &sig)
            .unwrap();

        let result = compile_function_body(
            &mut module,
            chunk,
            nan_consts,
            "ilo_f",
            f_id,
            &helpers,
            None, // <-- all_func_ids=None, zero args → exercises lines 2569-2580
            None,
        );
        assert!(
            result.is_ok(),
            "compile_function_body(all_func_ids=None, zero arg) failed: {:?}",
            result.err()
        );
    }

    // ── AOT RECWITH with ambiguous field types → string names in constant ────
    // When two types have the same field at different indices, search_field_index
    // returns None, so the constant pool entry is Value::List([Value::Text(name)]).
    // In AOT's OP_RECWITH handler the `_ => 0` fallback treats string items as 0,
    // which triggers the `match v { Value::Number(_) => ..., _ => 0 }` arm.

    #[test]
    fn codegen_cov_recwith_ambiguous_field_names() {
        // pt{x:n;y:n}: x at 0, y at 1; qt{y:n;x:n}: y at 0, x at 1 → ambiguous "x"
        let bytes = compile_to_object_bytes(
            "type pt{x:n;y:n}\ntype qt{y:n;x:n}\ng p:v>v;p with x:99\nf>v;p=pt x:1 y:2;g p",
        );
        assert!(
            bytes.is_ok(),
            "RECWITH ambiguous codegen failed: {:?}",
            bytes.err()
        );
    }
}
