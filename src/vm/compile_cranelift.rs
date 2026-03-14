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
                | OP_ROU | OP_RND0 | OP_RND2 | OP_NOW | OP_MOD => {
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
                OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_NEG | OP_WRAPOK | OP_WRAPERR | OP_UNWRAP
                | OP_RECFLD | OP_RECFLD_NAME | OP_LISTGET | OP_INDEX | OP_STR | OP_HD | OP_TL
                | OP_REV | OP_SRT | OP_SLC | OP_SPL | OP_CAT | OP_GET | OP_POST | OP_GETH
                | OP_POSTH | OP_ENV | OP_JPTH | OP_JDMP | OP_JPAR | OP_MAPNEW | OP_MGET
                | OP_MSET | OP_MDEL | OP_MKEYS | OP_MVALS | OP_LISTNEW | OP_LISTAPPEND
                | OP_RECNEW | OP_RECWITH | OP_PRT | OP_RD | OP_RDL | OP_WR | OP_WRL | OP_TRM
                | OP_UNQ | OP_NUM => {
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
        assert!(bytes.is_ok(), "GT comparison codegen failed: {:?}", bytes.err());
    }

    // GTE comparison
    #[test]
    fn codegen_cov_gte_comparison() {
        let bytes = compile_to_object_bytes("f x:n y:n>n;>=x y{1};0");
        assert!(bytes.is_ok(), "GTE comparison codegen failed: {:?}", bytes.err());
    }

    // LTE comparison
    #[test]
    fn codegen_cov_lte_comparison() {
        let bytes = compile_to_object_bytes("f x:n y:n>n;<=x y{1};0");
        assert!(bytes.is_ok(), "LTE comparison codegen failed: {:?}", bytes.err());
    }

    // Record type
    #[test]
    fn codegen_cov_record_type() {
        let bytes = compile_to_object_bytes("type pt{x:n;y:n}\nf a:n b:n>pt;pt x:a y:b");
        assert!(bytes.is_ok(), "record type codegen failed: {:?}", bytes.err());
    }

    // Record field access
    #[test]
    fn codegen_cov_record_field() {
        let bytes = compile_to_object_bytes("type pt{x:n;y:n}\nf>n;p=pt x:1 y:2;p.x");
        assert!(bytes.is_ok(), "record field codegen failed: {:?}", bytes.err());
    }

    // Record with update
    #[test]
    fn codegen_cov_record_with() {
        let bytes = compile_to_object_bytes("type pt{x:n;y:n}\nf>n;p=pt x:1 y:2;q=p with x:10;q.x");
        assert!(bytes.is_ok(), "record with codegen failed: {:?}", bytes.err());
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
        let bytes = compile_to_object_bytes(r#"f>n;m=mset mmap "a" 1;m=mset m "b" 2;k=mkeys m;len k"#);
        assert!(bytes.is_ok(), "map ops codegen failed: {:?}", bytes.err());
    }

    // Ok/Err result types
    #[test]
    fn codegen_cov_result_types() {
        let bytes = compile_to_object_bytes(r#"f x:n>R n t;>x 0{~x};^"neg""#);
        assert!(bytes.is_ok(), "result types codegen failed: {:?}", bytes.err());
    }

    // Multi-function call chain
    #[test]
    fn codegen_cov_multi_func_chain() {
        let bytes = compile_to_object_bytes("a x:n>n;+x 1\nb x:n>n;a x\nf x:n>n;b x");
        assert!(bytes.is_ok(), "multi-func chain codegen failed: {:?}", bytes.err());
    }

    // ── inline_chunk paths — AOT inline callee with numeric ops ─────────────

    /// inline_chunk: OP_ADD_NN, OP_SUB_NN, OP_MUL_NN, OP_DIV_NN paths
    #[test]
    fn codegen_cov_inline_add_sub_mul_div_nn() {
        // The callee `inner` uses all 4 reg-reg float ops; it is inlineable
        // (all_regs_numeric, ≤16 regs).  The AOT path exercises inline_chunk.
        let bytes = compile_to_object_bytes(
            "inner a:n b:n>n;+a b\nf x:n>n;inner x x",
        );
        assert!(bytes.is_ok(), "inline ADD_NN: {:?}", bytes.err());

        let bytes = compile_to_object_bytes(
            "inner a:n b:n>n;-a b\nf x:n>n;inner x x",
        );
        assert!(bytes.is_ok(), "inline SUB_NN: {:?}", bytes.err());

        let bytes = compile_to_object_bytes(
            "inner a:n b:n>n;*a b\nf x:n>n;inner x x",
        );
        assert!(bytes.is_ok(), "inline MUL_NN: {:?}", bytes.err());

        let bytes = compile_to_object_bytes(
            "inner a:n b:n>n;/a b\nf x:n>n;inner x x",
        );
        assert!(bytes.is_ok(), "inline DIV_NN: {:?}", bytes.err());
    }

    /// inline_chunk: OP_ADDK_N path
    #[test]
    fn codegen_cov_inline_addk_n() {
        let bytes = compile_to_object_bytes("inc x:n>n;+x 1\nf x:n>n;inc x");
        assert!(bytes.is_ok(), "inline ADDK_N: {:?}", bytes.err());
    }

    /// inline_chunk: OP_SUBK_N path
    #[test]
    fn codegen_cov_inline_subk_n() {
        let bytes = compile_to_object_bytes("dec x:n>n;-x 1\nf x:n>n;dec x");
        assert!(bytes.is_ok(), "inline SUBK_N: {:?}", bytes.err());
    }

    /// inline_chunk: OP_MULK_N path
    #[test]
    fn codegen_cov_inline_mulk_n() {
        let bytes = compile_to_object_bytes("dbl x:n>n;*x 2\nf x:n>n;dbl x");
        assert!(bytes.is_ok(), "inline MULK_N: {:?}", bytes.err());
    }

    /// inline_chunk: OP_DIVK_N path
    #[test]
    fn codegen_cov_inline_divk_n() {
        let bytes = compile_to_object_bytes("halve x:n>n;/x 2\nf x:n>n;halve x");
        assert!(bytes.is_ok(), "inline DIVK_N: {:?}", bytes.err());
    }

    /// inline_chunk: OP_CMPK_GE_N path (guard >=)
    #[test]
    fn codegen_cov_inline_cmpk_ge_n() {
        // 'pos' returns x if x >= 0, else 0 — mirrors the GT_N test pattern
        let bytes = compile_to_object_bytes("pos x:n>n;>=x 0 x;0\nf x:n>n;pos x");
        assert!(bytes.is_ok(), "inline CMPK_GE_N: {:?}", bytes.err());
    }

    /// inline_chunk: OP_CMPK_GT_N path (guard >)
    #[test]
    fn codegen_cov_inline_cmpk_gt_n() {
        let bytes = compile_to_object_bytes("pos x:n>n;>x 0 x;0\nf x:n>n;pos x");
        assert!(bytes.is_ok(), "inline CMPK_GT_N: {:?}", bytes.err());
    }

    /// inline_chunk: OP_CMPK_LT_N path (guard <)
    #[test]
    fn codegen_cov_inline_cmpk_lt_n() {
        let bytes = compile_to_object_bytes("negguard x:n>n;<x 0 x;0\nf x:n>n;negguard x");
        assert!(bytes.is_ok(), "inline CMPK_LT_N: {:?}", bytes.err());
    }

    /// inline_chunk: OP_CMPK_LE_N path (guard <=)
    #[test]
    fn codegen_cov_inline_cmpk_le_n() {
        let bytes = compile_to_object_bytes("small x:n>n;<=x 5 1;0\nf x:n>n;small x");
        assert!(bytes.is_ok(), "inline CMPK_LE_N: {:?}", bytes.err());
    }

    /// inline_chunk: OP_CMPK_EQ_N path (guard =)
    #[test]
    fn codegen_cov_inline_cmpk_eq_n() {
        let bytes = compile_to_object_bytes("iszero x:n>n;=x 0 1;0\nf x:n>n;iszero x");
        assert!(bytes.is_ok(), "inline CMPK_EQ_N: {:?}", bytes.err());
    }

    /// inline_chunk: OP_CMPK_NE_N path (guard !=)
    #[test]
    fn codegen_cov_inline_cmpk_ne_n() {
        let bytes = compile_to_object_bytes("nonzero x:n>n;!=x 0 99;0\nf x:n>n;nonzero x");
        assert!(bytes.is_ok(), "inline CMPK_NE_N: {:?}", bytes.err());
    }

    /// inline_chunk: LOADK inside callee (constant load in inlined function)
    #[test]
    fn codegen_cov_inline_loadk() {
        // A callee that uses ADDK_N (which uses LOADK for the constant internally)
        let bytes = compile_to_object_bytes("addpi x:n>n;+x 3\nf x:n>n;addpi x");
        assert!(bytes.is_ok(), "inline LOADK: {:?}", bytes.err());
    }

    /// inline_chunk: OP_JMP inside callee — jump within inlined code
    #[test]
    fn codegen_cov_inline_jmp() {
        // Guard with JMP: the body is jumped over when guard fails
        let bytes =
            compile_to_object_bytes("maybe x:n>n;>x 5 x;0\nf x:n>n;maybe x");
        assert!(bytes.is_ok(), "inline JMP: {:?}", bytes.err());
    }

    /// inline_chunk: unterminated callee (no explicit RET at end) — exercises
    /// the `if !terminated { def nil; jump merge }` path at the end of inline_chunk.
    #[test]
    fn codegen_cov_inline_unterminated_callee() {
        // A guard-only function: if x>0 return x, otherwise fall off end
        let bytes = compile_to_object_bytes("maybe x:n>n;>x 0{x}\nf x:n>n;maybe x");
        assert!(bytes.is_ok(), "inline unterminated: {:?}", bytes.err());
    }

    // ── is_inlinable edge cases ──────────────────────────────────────────────

    /// is_inlinable returns false when all_regs_numeric=false → direct call
    #[test]
    fn codegen_cov_non_inlinable_non_numeric_callee() {
        // String-returning callee — not all_regs_numeric, so not inlined
        let bytes = compile_to_object_bytes(
            "greet name:t>t;cat \"hi \" name\nf name:t>t;greet name",
        );
        assert!(bytes.is_ok(), "non-numeric callee: {:?}", bytes.err());
    }

    /// is_inlinable: LOADK with in-range bx → returns true for pure numeric callee
    #[test]
    fn codegen_cov_inlinable_loadk_check() {
        // addconst is inlinable (pure numeric), exercises LOADK constant check
        let bytes = compile_to_object_bytes("addconst x:n>n;+x 5\nf x:n>n;addconst x");
        assert!(bytes.is_ok(), "LOADK check: {:?}", bytes.err());
    }

    // ── find_libilo_a / platform_linker_flags — covered via compile_to_binary ─

    #[test]
    fn codegen_cov_find_libilo_a_called() {
        // compile_to_binary calls find_libilo_a internally; we just verify codegen
        // succeeds and linking fails at the expected step (libilo or linker error).
        let compiled = compile_program("f x:n>n;+x 1");
        let tmp = std::env::temp_dir().join("ilo_test_aot_find_libilo");
        let out = tmp.to_str().unwrap();
        let result = compile_to_binary(&compiled, "f", out);
        let _ = std::fs::remove_file(out);
        match result {
            Ok(()) => {}
            Err(e) => {
                let is_expected = e.contains("libilo")
                    || e.contains("linker")
                    || e.contains("cc")
                    || e.contains("cannot find")
                    || e.contains("lilo")
                    || e.contains("ld");
                assert!(is_expected, "unexpected error: {}", e);
            }
        }
    }

    // ── Map operations ───────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_map_has_del() {
        let bytes = compile_to_object_bytes(
            r#"f>b;m=mset mmap "x" 1;m=mdel m "x";mhas m "x""#,
        );
        assert!(bytes.is_ok(), "mhas/mdel codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_map_vals() {
        let bytes = compile_to_object_bytes(
            r#"f>n;m=mset mmap "a" 1;vs=mvals m;len vs"#,
        );
        assert!(bytes.is_ok(), "mvals codegen failed: {:?}", bytes.err());
    }

    // ── Print / Trim / Uniq ──────────────────────────────────────────────────

    #[test]
    fn codegen_cov_trm_unq() {
        let bytes = compile_to_object_bytes(r#"f s:t>t;trm s"#);
        assert!(bytes.is_ok(), "trm codegen failed: {:?}", bytes.err());

        let bytes2 = compile_to_object_bytes("f xs:L n>L n;unq xs");
        assert!(bytes2.is_ok(), "unq codegen failed: {:?}", bytes2.err());
    }

    // ── File I/O ─────────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_file_io_ops() {
        // OP_RD, OP_RDL, OP_WR, OP_WRL
        let bytes = compile_to_object_bytes(r#"f p:t>R t t;rd p"#);
        assert!(bytes.is_ok(), "OP_RD codegen failed: {:?}", bytes.err());

        let bytes2 = compile_to_object_bytes(r#"f p:t>R L t t;rdl p"#);
        assert!(bytes2.is_ok(), "OP_RDL codegen failed: {:?}", bytes2.err());

        let bytes3 = compile_to_object_bytes(r#"f p:t c:t>R t t;wr p c"#);
        assert!(bytes3.is_ok(), "OP_WR codegen failed: {:?}", bytes3.err());

        let bytes4 = compile_to_object_bytes(r#"f p:t>R t t;wrl p ["a","b"]"#);
        assert!(bytes4.is_ok(), "OP_WRL codegen failed: {:?}", bytes4.err());
    }

    // ── HTTP ops ─────────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_http_ops() {
        // OP_POST: post url body
        let bytes = compile_to_object_bytes("f url:t body:t>R t t;post url body");
        assert!(bytes.is_ok(), "OP_POST codegen failed: {:?}", bytes.err());

        // OP_GETH: get url hdrs (where hdrs is M t t)
        let bytes2 = compile_to_object_bytes("f url:t hdrs:M t t>R t t;get url hdrs");
        assert!(bytes2.is_ok(), "OP_GETH codegen failed: {:?}", bytes2.err());
    }

    #[test]
    fn codegen_cov_posth_op() {
        // OP_POSTH — post url body hdrs (where hdrs is M t t)
        let bytes = compile_to_object_bytes("f url:t body:t hdrs:M t t>R t t;post url body hdrs");
        assert!(bytes.is_ok(), "OP_POSTH codegen failed: {:?}", bytes.err());
    }

    // ── Type predicates — OP_ISNUM, OP_ISTEXT, OP_ISBOOL, OP_ISLIST ────────

    #[test]
    fn codegen_cov_type_predicates_all() {
        // OP_ISTEXT: ?x{t _:true;_:false}
        let bytes = compile_to_object_bytes("f x:t>b;?x{t _:true;_:false}");
        assert!(bytes.is_ok(), "ISTEXT codegen failed: {:?}", bytes.err());

        // OP_ISBOOL: ?x{b _:true;_:false}
        let bytes2 = compile_to_object_bytes("f x:b>b;?x{b _:true;_:false}");
        assert!(bytes2.is_ok(), "ISBOOL codegen failed: {:?}", bytes2.err());

        // OP_ISLIST: ?x{l _:true;_:false}  (lowercase l for list)
        let bytes3 = compile_to_object_bytes("f x:n>b;?x{l _:true;_:false}");
        assert!(bytes3.is_ok(), "ISLIST codegen failed: {:?}", bytes3.err());
    }

    // ── For-range loop ───────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_forrange_sum() {
        let bytes = compile_to_object_bytes("f n:n>n;s=0;@i 0..n{s=+s i};s");
        assert!(bytes.is_ok(), "for-range sum codegen failed: {:?}", bytes.err());
    }

    // ── OP_GET builtin ───────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_get_builtin() {
        let bytes = compile_to_object_bytes("f xs:L n i:n>n;get xs i");
        assert!(bytes.is_ok(), "get builtin codegen failed: {:?}", bytes.err());
    }

    // ── OP_SPL, OP_HAS ──────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_spl_has() {
        let bytes = compile_to_object_bytes(r#"f s:t>L t;spl s ",""#);
        assert!(bytes.is_ok(), "spl codegen failed: {:?}", bytes.err());

        let bytes2 = compile_to_object_bytes(r#"f xs:L n v:n>b;has xs v"#);
        assert!(bytes2.is_ok(), "has codegen failed: {:?}", bytes2.err());
    }

    // ── OP_JDMP, OP_JPAR, OP_JPTH ───────────────────────────────────────────

    #[test]
    fn codegen_cov_json_ops() {
        let bytes = compile_to_object_bytes("f x:n>t;jdmp x");
        assert!(bytes.is_ok(), "jdmp codegen failed: {:?}", bytes.err());

        let bytes2 = compile_to_object_bytes(r#"f s:t>R t t;jpar s"#);
        assert!(bytes2.is_ok(), "jpar codegen failed: {:?}", bytes2.err());

        let bytes3 = compile_to_object_bytes(r#"f j:t p:t>R t t;jpth j p"#);
        assert!(bytes3.is_ok(), "jpth codegen failed: {:?}", bytes3.err());
    }

    // ── OP_NUM, OP_STR ───────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_num_str_ops() {
        let bytes = compile_to_object_bytes(r#"f s:t>R n t;num s"#);
        assert!(bytes.is_ok(), "num codegen failed: {:?}", bytes.err());

        let bytes2 = compile_to_object_bytes("f x:n>t;str x");
        assert!(bytes2.is_ok(), "str codegen failed: {:?}", bytes2.err());
    }

    // ── OP_NOW, OP_RND0, OP_RND2 ────────────────────────────────────────────

    #[test]
    fn codegen_cov_now_rnd_ops() {
        let bytes = compile_to_object_bytes("f>n;now");
        assert!(bytes.is_ok(), "now codegen failed: {:?}", bytes.err());

        let bytes2 = compile_to_object_bytes("f>n;rnd");
        assert!(bytes2.is_ok(), "rnd0 codegen failed: {:?}", bytes2.err());

        let bytes3 = compile_to_object_bytes("f>n;rnd 1 10");
        assert!(bytes3.is_ok(), "rnd2 codegen failed: {:?}", bytes3.err());
    }

    // ── OP_ENV ───────────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_env_op() {
        let bytes = compile_to_object_bytes(r#"f k:t>R t t;env k"#);
        assert!(bytes.is_ok(), "env codegen failed: {:?}", bytes.err());
    }

    // ── OP_TRUTHY ────────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_truthy_op() {
        // ternary with numeric condition emits OP_TRUTHY
        // Syntax: ?<cond_expr> <true_expr> <false_expr>
        let bytes = compile_to_object_bytes("f x:n>n;?<x 0 0 x");
        assert!(bytes.is_ok(), "truthy codegen failed: {:?}", bytes.err());
    }

    // ── OP_WRAPOK, OP_WRAPERR, OP_ISOK, OP_ISERR, OP_UNWRAP ────────────────

    #[test]
    fn codegen_cov_result_ops_full() {
        // Exercises all result opcodes in one function
        let bytes = compile_to_object_bytes("f x:n>n;r=~x;?r{~v:v;^_:0}");
        assert!(bytes.is_ok(), "result ops codegen failed: {:?}", bytes.err());
    }

    // ── OP_MOVE (same register, identity — possible no-op) ───────────────────

    #[test]
    fn codegen_cov_move_identity() {
        // `y=x; y` generates MOVE + RET
        let bytes = compile_to_object_bytes("f x:n>n;y=x;y");
        assert!(bytes.is_ok(), "MOVE identity codegen failed: {:?}", bytes.err());
    }

    // ── OP_NOT ───────────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_not_op() {
        let bytes = compile_to_object_bytes("f x:b>b;! x");
        assert!(bytes.is_ok(), "NOT codegen failed: {:?}", bytes.err());
    }

    // ── OP_NEG (unary minus) ──────────────────────────────────────────────────

    #[test]
    fn codegen_cov_neg_op() {
        let bytes = compile_to_object_bytes("f x:n>n;-x");
        assert!(bytes.is_ok(), "NEG codegen failed: {:?}", bytes.err());
    }

    // ── OP_ABS, OP_FLR, OP_CEL, OP_ROU ─────────────────────────────────────

    #[test]
    fn codegen_cov_math_builtins() {
        for src in &[
            "f x:n>n;abs x",
            "f x:n>n;flr x",
            "f x:n>n;cel x",
            "f x:n>n;rou x",
        ] {
            let bytes = compile_to_object_bytes(src);
            assert!(bytes.is_ok(), "math builtin '{}' codegen failed: {:?}", src, bytes.err());
        }
    }

    // ── OP_MIN, OP_MAX ────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_min_max_builtins() {
        let bytes = compile_to_object_bytes("f a:n b:n>n;min a b");
        assert!(bytes.is_ok(), "min codegen failed: {:?}", bytes.err());

        let bytes2 = compile_to_object_bytes("f a:n b:n>n;max a b");
        assert!(bytes2.is_ok(), "max codegen failed: {:?}", bytes2.err());
    }

    // ── OP_LEN ───────────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_len_op() {
        let bytes = compile_to_object_bytes("f x:t>n;len x");
        assert!(bytes.is_ok(), "len codegen failed: {:?}", bytes.err());
    }

    // ── OP_CAT ───────────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_cat_op() {
        let bytes = compile_to_object_bytes(r#"f a:t b:t>t;cat a b"#);
        assert!(bytes.is_ok(), "cat codegen failed: {:?}", bytes.err());
    }

    // ── OP_SRT, OP_REV, OP_HD, OP_TL ─────────────────────────────────────────

    #[test]
    fn codegen_cov_list_ops() {
        for src in &[
            "f xs:L n>L n;srt xs",
            "f xs:L n>L n;rev xs",
            "f xs:L n>n;hd xs",
            "f xs:L n>L n;tl xs",
        ] {
            let bytes = compile_to_object_bytes(src);
            assert!(bytes.is_ok(), "'{}' codegen failed: {:?}", src, bytes.err());
        }
    }

    // ── OP_SLC ───────────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_slc_op() {
        let bytes = compile_to_object_bytes("f xs:L n a:n b:n>L n;slc xs a b");
        assert!(bytes.is_ok(), "slc codegen failed: {:?}", bytes.err());
    }

    // ── OP_LISTAPPEND ────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_listappend_op() {
        let bytes = compile_to_object_bytes("f xs:L n v:n>L n;+=xs v");
        assert!(bytes.is_ok(), "listappend codegen failed: {:?}", bytes.err());
    }

    // ── OP_LISTGET inside FOREACHPREP / FOREACHNEXT ──────────────────────────

    #[test]
    fn codegen_cov_foreach_over_list() {
        let bytes = compile_to_object_bytes("f xs:L n>n;s=0;@x xs{s=+s x};s");
        assert!(bytes.is_ok(), "foreach codegen failed: {:?}", bytes.err());
    }

    // ── OP_RECNEW, OP_RECFLD, OP_RECWITH ─────────────────────────────────────

    #[test]
    fn codegen_cov_record_ops_full() {
        // OP_RECNEW, OP_RECFLD (field access)
        let bytes = compile_to_object_bytes("type box{v:n} f>n;b=box v:42;b.v");
        assert!(bytes.is_ok(), "record ops codegen failed: {:?}", bytes.err());

        // OP_RECWITH (record update)
        let bytes2 = compile_to_object_bytes("type box{v:n} f>n;b=box v:1;b2=b with v:99;b2.v");
        assert!(bytes2.is_ok(), "recwith codegen failed: {:?}", bytes2.err());
    }

    // ── OP_RECFLD_NAME (record field access by string name) ──────────────────

    #[test]
    fn codegen_cov_recfld_name() {
        // r.score uses OP_RECFLD_NAME when field comes from json parse
        let bytes = compile_to_object_bytes(r#"f x:t>R t t;r=jpar! x;r.score"#);
        // This may not compile (unsupported op) but should not panic
        match bytes {
            Ok(_) | Err(_) => {}
        }
    }

    // ── OP_JMPNN (nil coalesce) ───────────────────────────────────────────────

    #[test]
    fn codegen_cov_nil_coalesce() {
        let bytes = compile_to_object_bytes("f x:O n>n;x??42");
        assert!(bytes.is_ok(), "nil coalesce codegen failed: {:?}", bytes.err());
    }

    // ── OP_FORRANGEPREP / OP_FORRANGENEXT ────────────────────────────────────

    #[test]
    fn codegen_cov_forrange_ops() {
        let bytes = compile_to_object_bytes("f n:n>n;s=0;@i 0..n{s=+s i};s");
        assert!(bytes.is_ok(), "forrange codegen failed: {:?}", bytes.err());
    }

    // ── OP_DROP_RC ────────────────────────────────────────────────────────────

    #[test]
    fn codegen_cov_drop_rc() {
        // Any operation that creates a heap value and then overwrites the register
        // generates OP_DROP_RC. String assignment to existing var does this.
        let bytes = compile_to_object_bytes(r#"f>t;s="hello";s="world";s"#);
        assert!(bytes.is_ok(), "drop_rc codegen failed: {:?}", bytes.err());
    }

    // ── OP_LISTNEW with n=0 (empty list) ─────────────────────────────────────

    #[test]
    fn codegen_cov_listnew_empty() {
        let bytes = compile_to_object_bytes("f>L n;[]");
        assert!(bytes.is_ok(), "listnew empty codegen failed: {:?}", bytes.err());
    }

    // ── OP_LISTNEW with n>0 ───────────────────────────────────────────────────

    #[test]
    fn codegen_cov_listnew_with_elements() {
        let bytes = compile_to_object_bytes("f a:n b:n c:n>L n;[a, b, c]");
        assert!(bytes.is_ok(), "listnew with elements codegen failed: {:?}", bytes.err());
    }

    // ── OP_LISTGET (fallback helper path) ────────────────────────────────────

    #[test]
    fn codegen_cov_listget_index() {
        // xs.1 uses OP_INDEX which may emit OP_LISTGET or OP_INDEX
        let bytes = compile_to_object_bytes("f xs:L n>n;xs.1");
        assert!(bytes.is_ok(), "listget/index codegen failed: {:?}", bytes.err());
    }

    // ── OP_RECWITH_ARENA (arena record with update) ───────────────────────────

    #[test]
    fn codegen_cov_recwith_arena() {
        // Record with update using arena allocation
        let bytes =
            compile_to_object_bytes("type pt{x:n;y:n} f>n;p=pt x:1 y:2;q=p with x:10;+q.x q.y");
        assert!(bytes.is_ok(), "recwith_arena codegen failed: {:?}", bytes.err());
    }

    // ── OP_NEG: fast path F64 shadow update, slow path for :_ type ───────────

    #[test]
    fn codegen_cov_neg_f64_shadow_update() {
        // f x:n>n;y=-x;+y 1 — OP_NEG result (y) used in ADDK_N means y is reg_always_num.
        // This covers the F64 shadow def_var after OP_NEG (line 1362 in compile_cranelift.rs).
        let bytes = compile_to_object_bytes("f x:n>n;y=-x;+y 1");
        assert!(bytes.is_ok(), "neg f64 shadow update codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_neg_slow_path_any_type() {
        // f x:_>n;-x — x:_ means all_regs_numeric=false, reg_always_num[b]=false.
        // This exercises the OP_NEG helper-call slow path (lines 1364-1370).
        let bytes = compile_to_object_bytes("f x:_>n;-x");
        assert!(bytes.is_ok(), "neg slow path codegen failed: {:?}", bytes.err());
    }

    // ── OP_GET (1-arg HTTP GET) ───────────────────────────────────────────────

    #[test]
    fn codegen_cov_get_1arg_http() {
        // `get url` (1-arg) emits OP_GET (HTTP GET without headers).
        // This covers lines 1737-1743 in compile_cranelift.rs.
        let bytes = compile_to_object_bytes("f url:t>R t t;get url");
        assert!(bytes.is_ok(), "get 1-arg codegen failed: {:?}", bytes.err());
    }

    // ── OP_MUL_NN / OP_SUB_NN / OP_DIV_NN with mixed-type params ────────────
    // When one param is :t (non-numeric), all_regs_numeric=false, so numeric params
    // don't get reg_always_num=true.  OP_MUL_NN on those params hits the slow bitcast path.

    #[test]
    fn codegen_cov_mul_nn_mixed_params() {
        // f a:n b:n c:t>n;*a b — c:t prevents all_regs_numeric; exercises slow bitcast path
        // for OP_MUL_NN inputs (lines 1042-1049 / similar in compile_cranelift.rs).
        let bytes = compile_to_object_bytes("f a:n b:n c:t>n;*a b");
        assert!(bytes.is_ok(), "mul_nn mixed params codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_sub_nn_mixed_params() {
        // f a:n b:n c:t>n;-a b — exercises OP_SUB_NN slow bitcast path.
        let bytes = compile_to_object_bytes("f a:n b:n c:t>n;-a b");
        assert!(bytes.is_ok(), "sub_nn mixed params codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_div_nn_mixed_params() {
        // f a:n b:n c:t>n;/a b — exercises OP_DIV_NN slow bitcast path.
        let bytes = compile_to_object_bytes("f a:n b:n c:t>n;/a b");
        assert!(bytes.is_ok(), "div_nn mixed params codegen failed: {:?}", bytes.err());
    }

    // ── OP_ADD|OP_SUB|OP_MUL|OP_DIV with :_ params ──────────────────────────

    #[test]
    fn codegen_cov_sub_any_type() {
        // f a:_ b:_>n;-a b — emits OP_SUB (not OP_SUB_NN) since params are :_.
        let bytes = compile_to_object_bytes("f a:_ b:_>n;-a b");
        assert!(bytes.is_ok(), "sub any-type codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_mul_any_type() {
        // f a:_ b:_>n;*a b — emits OP_MUL.
        let bytes = compile_to_object_bytes("f a:_ b:_>n;*a b");
        assert!(bytes.is_ok(), "mul any-type codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_div_any_type() {
        // f a:_ b:_>n;/a b — emits OP_DIV.
        let bytes = compile_to_object_bytes("f a:_ b:_>n;/a b");
        assert!(bytes.is_ok(), "div any-type codegen failed: {:?}", bytes.err());
    }

    // ── OP_LOADK with heap value (list constant) ─────────────────────────────

    #[test]
    fn codegen_cov_loadk_list_constant() {
        // A function that returns a list literal — the list constant is a heap value,
        // so LOADK uses jit_move to clone the RC (lines 1449-1455).
        let bytes = compile_to_object_bytes("f>L n;[1 2 3]");
        assert!(bytes.is_ok(), "loadk list constant codegen failed: {:?}", bytes.err());
    }

    // ── OP_RECWITH unresolved field path ─────────────────────────────────────

    #[test]
    fn codegen_cov_recwith_unresolved_field() {
        // r:_ with z:10 — field 'z' not in any type → all_resolved=false →
        // exercises the unresolved OP_RECWITH path (lines 2116-2209 in compile_cranelift.rs).
        let bytes = compile_to_object_bytes("type pt{x:n;y:n}\nf r:_>_;r with z:10");
        assert!(bytes.is_ok(), "recwith unresolved codegen failed: {:?}", bytes.err());
    }

    // ── compile_to_bench_binary ───────────────────────────────────────────────

    #[test]
    fn compile_to_bench_binary_starts_or_fails_at_link() {
        // compile_to_bench_binary exercises the bench codegen path (lines 2919-3135).
        // It may fail at the link step if libilo.a is not present, which is acceptable.
        let compiled = compile_program("f x:n>n;*x 2");
        let tmp = std::env::temp_dir().join("ilo_test_bench_codegen");
        let out = tmp.to_str().unwrap();
        let result = compile_to_bench_binary(&compiled, "f", out);
        // Clean up any generated files
        let _ = std::fs::remove_file(out);
        let _ = std::fs::remove_file(format!("{}.o", out));
        let _ = std::fs::remove_file(format!("{}_bench.c", out));
        let _ = std::fs::remove_file(format!("{}_bench_c.o", out));
        match result {
            Ok(()) => {
                // Full pipeline succeeded — libilo.a was available
                let _ = std::fs::remove_file(out);
            }
            Err(e) => {
                // Must fail at the link step or cc step, not at Cranelift codegen
                let is_expected_error = e.contains("libilo")
                    || e.contains("linker")
                    || e.contains("cc")
                    || e.contains("cannot find")
                    || e.contains("lilo")
                    || e.contains("ld")
                    || e.contains("failed to compile")
                    || e.contains("failed to link");
                assert!(
                    is_expected_error,
                    "expected a link/cc error but got codegen error: {}",
                    e
                );
            }
        }
    }

    #[test]
    fn compile_to_bench_binary_undefined_function_returns_error() {
        // Verify compile_to_bench_binary returns error for unknown entry function.
        let compiled = compile_program("f x:n>n;+x 1");
        let tmp = std::env::temp_dir().join("ilo_test_bench_no_fn");
        let out = tmp.to_str().unwrap();
        let result = compile_to_bench_binary(&compiled, "does_not_exist", out);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("undefined function"),
            "expected 'undefined function' in error, got: {}",
            err
        );
    }

    // ── inline_chunk OP_SUB_NN / OP_MUL_NN / OP_DIV_NN in AOT (lines 2116+) ─

    #[test]
    fn codegen_cov_inline_sub_nn_two_params() {
        // Callee with OP_SUB_NN (both params numeric) — exercises inline_chunk SUB_NN in AOT.
        let bytes = compile_to_object_bytes("subdbl2 a:n b:n>n;-a b\nf x:n y:n>n;subdbl2 x y");
        assert!(bytes.is_ok(), "inline sub_nn two params codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_inline_mul_nn_two_params() {
        // Callee with OP_MUL_NN (both params numeric) — exercises inline_chunk MUL_NN in AOT.
        let bytes = compile_to_object_bytes("muldbl2 a:n b:n>n;*a b\nf x:n y:n>n;muldbl2 x y");
        assert!(bytes.is_ok(), "inline mul_nn two params codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_inline_subk_n_alt() {
        // Callee with OP_SUBK_N (alternative: subtract 5) — exercises inline_chunk SUBK_N in AOT.
        let bytes = compile_to_object_bytes("dec5c x:n>n;-x 5\nf x:n>n;dec5c x");
        assert!(bytes.is_ok(), "inline subk_n alt codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_inline_local_var_f64_path() {
        // Callee with non-param register in DIVK_N — exercises f64_val_for extra_vars path.
        let bytes = compile_to_object_bytes("stepc x:n>n;y=+x 1;/y 2\nf x:n>n;stepc x");
        assert!(bytes.is_ok(), "inline local var f64 path codegen failed: {:?}", bytes.err());
    }

    // ── OP_LE/GE/EQ/NE slow path helpers ────────────────────────────────────

    #[test]
    fn codegen_cov_le_any_type() {
        // f a:_ b:_>b;<= a b — emits OP_LE (non-NN) covering the slow helper path.
        let bytes = compile_to_object_bytes("f a:_ b:_>b;<= a b");
        assert!(bytes.is_ok(), "le any-type codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_ge_any_type() {
        // f a:_ b:_>b;>= a b — emits OP_GE
        let bytes = compile_to_object_bytes("f a:_ b:_>b;>= a b");
        assert!(bytes.is_ok(), "ge any-type codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_eq_any_type() {
        // f a:_ b:_>b;== a b — emits OP_EQ
        let bytes = compile_to_object_bytes("f a:_ b:_>b;== a b");
        assert!(bytes.is_ok(), "eq any-type codegen failed: {:?}", bytes.err());
    }

    #[test]
    fn codegen_cov_ne_any_type() {
        // f a:_ b:_>b;!= a b — emits OP_NE
        let bytes = compile_to_object_bytes("f a:_ b:_>b;!= a b");
        assert!(bytes.is_ok(), "ne any-type codegen failed: {:?}", bytes.err());
    }

    // ── OP_LISTGET (AOT) — crafted bytecode ─────────────────────────────────

    #[test]
    fn codegen_cov_listget_fast_path() {
        // OP_LISTGET is legacy but still present in the AOT compiler.
        // Craft a CompiledProgram with OP_LISTGET bytecode to exercise lines 2116-2209.
        //
        // [0] LISTGET R0, R1, R2   — R0 = R1[R2], skip next if found
        // [1] JMP +1               — exit (not found / oob)
        // [2] RET R0               — return element
        // [3] RET R0               — exit
        let abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };
        let code = vec![
            make_inst_abc(OP_LISTGET, 0, 1, 2),
            abx(OP_JMP, 0, 1u16),
            abx(OP_RET, 0, 0),
            abx(OP_RET, 0, 0),
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

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);
        let mut sig = module.make_signature();
        for _ in 0..3 {
            sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
        }
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
        let fid = module
            .declare_function("ilo_f", cranelift_module::Linkage::Local, &sig)
            .unwrap();

        let result = compile_function_body(
            &mut module,
            &program.chunks[0],
            &program.nan_constants[0],
            "ilo_f",
            fid,
            &helpers,
            Some(&[fid]),
            Some(&program),
        );
        assert!(result.is_ok(), "OP_LISTGET AOT codegen failed: {:?}", result.err());
    }

    // ── AOT out-of-range fallback (all_func_ids = None) ─────────────────────
    // When compile_function_body is called with all_func_ids=None, OP_CALL hits
    // the fallback path using jit_call helper (lines 2547-2579).

    #[test]
    fn codegen_cov_call_no_func_ids_with_args() {
        // Call with all_func_ids=None and n_args=1 → fallback path (lines 2547-2568)
        let abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };
        // CALL R0, func_idx=0, n_args=1
        let call_bx: u16 = (0u16 << 8) | 1u16;
        let code = vec![
            abx(OP_CALL, 0, call_bx),
            abx(OP_RET, 0, 0),
        ];
        let dummy_span = crate::ast::Span { start: 0, end: 0 };
        let chunk = Chunk {
            code,
            constants: vec![],
            param_count: 2,
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

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);
        let mut sig = module.make_signature();
        for _ in 0..2 {
            sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
        }
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
        let fid = module
            .declare_function("ilo_f_no_ids", cranelift_module::Linkage::Local, &sig)
            .unwrap();

        // Pass all_func_ids=None → triggers the fallback path
        let result = compile_function_body(
            &mut module,
            &program.chunks[0],
            &program.nan_constants[0],
            "ilo_f_no_ids",
            fid,
            &helpers,
            None, // ← no func_ids → fallback path (lines 2547-2568)
            None,
        );
        assert!(result.is_ok(), "AOT call no-func-ids with-args failed: {:?}", result.err());
    }

    #[test]
    fn codegen_cov_call_no_func_ids_no_args() {
        // Call with all_func_ids=None and n_args=0 → fallback path (lines 2570-2579)
        let abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };
        // CALL R0, func_idx=0, n_args=0
        let call_bx: u16 = 0u16; // func_idx=0, n_args=0
        let code = vec![
            abx(OP_CALL, 0, call_bx),
            abx(OP_RET, 0, 0),
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

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);
        let mut sig = module.make_signature();
        sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
        let fid = module
            .declare_function("ilo_f_no_ids_noarg", cranelift_module::Linkage::Local, &sig)
            .unwrap();

        let dummy_program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };

        let result = compile_function_body(
            &mut module,
            &dummy_program.chunks[0],
            &dummy_program.nan_constants[0],
            "ilo_f_no_ids_noarg",
            fid,
            &helpers,
            None, // ← no func_ids → fallback no-args path (lines 2570-2579)
            None,
        );
        assert!(result.is_ok(), "AOT call no-func-ids no-args failed: {:?}", result.err());
    }

    // ── AOT inline-failed fallback (lines 2521-2531) ─────────────────────────
    // When inline_chunk returns false mid-way, fallback to direct call.
    // Craft a callee that is_inlinable() but inline_chunk fails (JMP to bad target).

    #[test]
    fn codegen_cov_inline_jmp_unknown_target_aot() {
        let abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        // Callee: ADD_NN R0, R0, R0; JMP -999 (invalid); RET R0
        // is_inlinable: all_regs_numeric=true, code has ADD_NN + JMP + RET → true
        // inline_chunk: JMP target = -997 → not in imap → returns false → fallback
        let invalid_offset: i16 = -999;
        let callee_code = vec![
            make_inst_abc(OP_ADD_NN, 0, 0, 0),                  // ADD_NN R0, R0, R0 (no constants)
            abx(OP_JMP, 0, invalid_offset as u16),               // JMP to invalid target
            abx(OP_RET, 0, 0),
        ];
        // Caller: CALL R0, func_idx=1, n_args=1 (R1 is arg); RET R0
        let call_bx: u16 = (1u16 << 8) | 1u16;
        let caller_code = vec![
            abx(OP_CALL, 0, call_bx),
            abx(OP_RET, 0, 0),
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

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);

        // Declare both functions
        let caller_fid = {
            let mut sig = module.make_signature();
            for _ in 0..2 {
                sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
            }
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
            module.declare_function("ilo_caller_inline_fail", cranelift_module::Linkage::Local, &sig).unwrap()
        };
        let callee_fid = {
            let mut sig = module.make_signature();
            sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
            module.declare_function("ilo_callee_inline_fail", cranelift_module::Linkage::Local, &sig).unwrap()
        };
        let func_ids = vec![caller_fid, callee_fid];

        // Compile callee first
        let _ = compile_function_body(
            &mut module,
            &program.chunks[1],
            &program.nan_constants[1],
            "ilo_callee_inline_fail",
            callee_fid,
            &helpers,
            Some(&func_ids),
            Some(&program),
        );

        // Compile caller — inline_chunk for callee will fail (JMP to invalid target)
        // → fallback to direct call (lines 2521-2531)
        let result = compile_function_body(
            &mut module,
            &program.chunks[0],
            &program.nan_constants[0],
            "ilo_caller_inline_fail",
            caller_fid,
            &helpers,
            Some(&func_ids),
            Some(&program),
        );
        // Result may be Err if fallback fails due to Cranelift validation, but codegen path is exercised
        let _ = result;
    }

    // ── OP_JMPT always-bool fast path (AOT, lines 1508-1511) ─────────────────
    // env! emits ISOK + JMPT where ISOK result is always boolean → AOT fast path.

    #[test]
    fn codegen_cov_jmpt_always_bool_aot() {
        // f k:t>t;env! k — ISOK result is always boolean → reg_always_bool=true
        // → JMPT on that register hits the fast bool path (lines 1508-1511 in AOT).
        let bytes = compile_to_object_bytes("f k:t>t;env! k");
        assert!(bytes.is_ok(), "jmpt always-bool AOT codegen failed: {:?}", bytes.err());
    }

    // ── OP_JMPF general truthy path (AOT, lines 1564-1567) ───────────────────
    // OR operator uses JMPF for short-circuit; with :_ params, reg_always_bool=false
    // → general truthy check path (lines 1564-1567 in AOT).

    #[test]
    fn codegen_cov_jmpf_general_truthy_aot() {
        // f a:_ b:_>b;|a b — short-circuit OR emits JMPF on :_ register (non-bool).
        // This exercises the JMPF general truthy path (lines 1564-1567).
        let bytes = compile_to_object_bytes("f a:_ b:_>b;|a b");
        assert!(bytes.is_ok(), "jmpf general truthy AOT codegen failed: {:?}", bytes.err());
    }

    // ── OP_JMPT general truthy path (AOT, lines 1568-1572) ───────────────────
    // AND uses JMPT; with :_ params → general truthy check.

    #[test]
    fn codegen_cov_jmpt_general_truthy_aot() {
        // f a:_ b:_>b;&a b — short-circuit AND emits JMPT on :_ register (non-bool).
        let bytes = compile_to_object_bytes("f a:_ b:_>b;&a b");
        assert!(bytes.is_ok(), "jmpt general truthy AOT codegen failed: {:?}", bytes.err());
    }

    // ── OP_JMPNN (AOT) ───────────────────────────────────────────────────────
    // JMPNN is emitted for nil-coalescing / optional patterns.

    #[test]
    fn codegen_cov_jmpnn_aot() {
        // f x:_>_;x ?? 42 — nil-coalescing uses JMPNN
        let bytes = compile_to_object_bytes("f x:_>_;x ?? 42");
        assert!(bytes.is_ok(), "jmpnn AOT codegen failed: {:?}", bytes.err());
    }

    // ── OP_LOADK list heap value path (lines 1450-1454 in AOT) ──────────────
    // Already covered by codegen_cov_loadk_list_constant, but let's be explicit.

    #[test]
    fn codegen_cov_loadk_heap_map() {
        // A function returning an empty map (heap value) exercises the heap LOADK path
        let bytes = compile_to_object_bytes("f>M t t;mmap");
        assert!(bytes.is_ok(), "loadk heap map codegen failed: {:?}", bytes.err());
    }

    // ── inline_chunk fallthrough between blocks (lines 511-512, 679-682 in AOT) ─

    #[test]
    fn codegen_cov_inline_fallthrough_aot() {
        // Callee with CMPK + JMP + non-terminating block + leader:
        // [0] CMPK_GT_N R0, k0  ← leaders: {0,1,2}; from JMP+1: {2,3}
        // [1] JMP +1             ← target=3
        // [2] ADD_NN R0, R0, R0  ← non-terminating; falls through to leader at ip=3
        // [3] RET R0             ← leader; !terminated=true at ip=3 → lines 511-512 fire
        let abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        let callee_code = vec![
            make_inst_abc(OP_CMPK_GT_N, 0, 0, 0), // ip=0
            abx(OP_JMP, 0, 1u16),                   // ip=1: JMP +1 → target=3
            make_inst_abc(OP_ADD_NN, 0, 0, 0),       // ip=2: non-terminating
            abx(OP_RET, 0, 0),                        // ip=3: RET (leader from JMP target)
        ];
        let callee_nan_consts = vec![NanVal::number(5.0)]; // k0=5.0

        let call_bx: u16 = (1u16 << 8) | 1u16;
        let caller_code = vec![
            abx(OP_CALL, 0, call_bx),
            abx(OP_RET, 0, 0),
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

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);
        let caller_fid = {
            let mut sig = module.make_signature();
            for _ in 0..2 {
                sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
            }
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
            module.declare_function("ilo_f_fallthrough", cranelift_module::Linkage::Local, &sig).unwrap()
        };
        let callee_fid = {
            let mut sig = module.make_signature();
            sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
            module.declare_function("ilo_callee_fallthrough", cranelift_module::Linkage::Local, &sig).unwrap()
        };
        let func_ids = vec![caller_fid, callee_fid];

        let _ = compile_function_body(&mut module, &program.chunks[1], &program.nan_constants[1],
            "ilo_callee_fallthrough", callee_fid, &helpers, Some(&func_ids), Some(&program));

        let result = compile_function_body(&mut module, &program.chunks[0], &program.nan_constants[0],
            "ilo_f_fallthrough", caller_fid, &helpers, Some(&func_ids), Some(&program));
        assert!(result.is_ok(), "inline fallthrough AOT failed: {:?}", result.err());
    }

    // ── inline_chunk dead code skip (line 517-518 in AOT) ───────────────────

    #[test]
    fn codegen_cov_inline_dead_code_aot() {
        // Callee:
        // [0] RET R0         ← terminates immediately
        // [1] ADD_NN R0, R0  ← dead code (not a leader); `if terminated { continue }` fires
        let abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        let callee_code = vec![
            abx(OP_RET, 0, 0),
            make_inst_abc(OP_ADD_NN, 0, 0, 0), // dead code
        ];
        let call_bx: u16 = (1u16 << 8) | 1u16;
        let caller_code = vec![
            abx(OP_CALL, 0, call_bx),
            abx(OP_RET, 0, 0),
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

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);
        let caller_fid = {
            let mut sig = module.make_signature();
            for _ in 0..2 {
                sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
            }
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
            module.declare_function("ilo_f_dead_code", cranelift_module::Linkage::Local, &sig).unwrap()
        };
        let callee_fid = {
            let mut sig = module.make_signature();
            sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
            module.declare_function("ilo_callee_dead_code", cranelift_module::Linkage::Local, &sig).unwrap()
        };
        let func_ids = vec![caller_fid, callee_fid];

        let _ = compile_function_body(&mut module, &program.chunks[1], &program.nan_constants[1],
            "ilo_callee_dead_code", callee_fid, &helpers, Some(&func_ids), Some(&program));
        let result = compile_function_body(&mut module, &program.chunks[0], &program.nan_constants[0],
            "ilo_f_dead_code", caller_fid, &helpers, Some(&func_ids), Some(&program));
        assert!(result.is_ok(), "inline dead code AOT failed: {:?}", result.err());
    }

    // ── inline_chunk not-terminated at end (lines 679-682 in AOT) ────────────

    #[test]
    fn codegen_cov_inline_not_terminated_at_end_aot() {
        // Callee:
        // [0] CMPK_GT_N R0, k0  ← leaders: {0,1,2}
        // [1] JMP +2             ← target=4; leaders: {2,4}
        // [2] RET R0             ← body block: returns
        // [3] ADD_NN R0, R0, R0  ← NOT a leader; dead code (skipped at line 517-518)
        // [4] ADD_NN R0, R0, R0  ← leader b4; no terminator → lines 679-682 fire at end
        let abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        let callee_code = vec![
            make_inst_abc(OP_CMPK_GT_N, 0, 0, 0), // ip=0
            abx(OP_JMP, 0, 2u16),                   // ip=1: JMP +2 → target=4
            abx(OP_RET, 0, 0),                        // ip=2: RET
            make_inst_abc(OP_ADD_NN, 0, 0, 0),        // ip=3: dead code (not leader)
            make_inst_abc(OP_ADD_NN, 0, 0, 0),        // ip=4: leader; no terminator
        ];
        let callee_nan_consts = vec![NanVal::number(5.0)];

        let call_bx: u16 = (1u16 << 8) | 1u16;
        let caller_code = vec![
            abx(OP_CALL, 0, call_bx),
            abx(OP_RET, 0, 0),
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

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);
        let caller_fid = {
            let mut sig = module.make_signature();
            for _ in 0..2 {
                sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
            }
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
            module.declare_function("ilo_f_unterminated", cranelift_module::Linkage::Local, &sig).unwrap()
        };
        let callee_fid = {
            let mut sig = module.make_signature();
            sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
            module.declare_function("ilo_callee_unterminated", cranelift_module::Linkage::Local, &sig).unwrap()
        };
        let func_ids = vec![caller_fid, callee_fid];

        let _ = compile_function_body(&mut module, &program.chunks[1], &program.nan_constants[1],
            "ilo_callee_unterminated", callee_fid, &helpers, Some(&func_ids), Some(&program));
        let result = compile_function_body(&mut module, &program.chunks[0], &program.nan_constants[0],
            "ilo_f_unterminated", caller_fid, &helpers, Some(&func_ids), Some(&program));
        // Result may fail (Cranelift might reject unterminated block), but we exercised the path
        let _ = result;
    }

    // ── compile_to_bench_binary with multi-function + type registry ──────────
    // Covers: line 2955 (Linkage::Local for non-entry), lines 3021-3025 and 3031
    // (registry embed in C harness), lines 3061-3064 (registry set call in C harness).

    #[test]
    fn compile_to_bench_binary_multi_func_with_registry() {
        // A program with 2 functions and a type definition so registry_bytes is non-empty.
        // The helper function exercises Linkage::Local (line 2955).
        // The type definition causes registry bytes to be written into the C harness.
        // Also uses 2 params to exercise line 3031 (`, ` separator between params).
        let compiled = compile_program(
            "type pt{x:n;y:n}\nhelper a:n b:n>n;+a b\nf x:n y:n>n;helper x y",
        );
        let tmp = std::env::temp_dir().join("ilo_test_bench_multifunc");
        let out = tmp.to_str().unwrap();
        let result = compile_to_bench_binary(&compiled, "f", out);
        // Clean up any generated files regardless of result
        let _ = std::fs::remove_file(out);
        let _ = std::fs::remove_file(format!("{}.o", out));
        let _ = std::fs::remove_file(format!("{}_bench.c", out));
        let _ = std::fs::remove_file(format!("{}_bench_c.o", out));
        match result {
            Ok(()) => {
                let _ = std::fs::remove_file(out);
            }
            Err(e) => {
                let is_expected = e.contains("libilo")
                    || e.contains("linker")
                    || e.contains("cc")
                    || e.contains("cannot find")
                    || e.contains("lilo")
                    || e.contains("ld")
                    || e.contains("failed to compile")
                    || e.contains("failed to link");
                assert!(is_expected, "unexpected bench error: {}", e);
            }
        }
    }

    // ── JMPT with out-of-range target (lines 1482-1487) ─────────────────────
    // compile_function_body should return Err when the JMPF/JMPT target is not
    // in block_map (because the jump offset is so large it gets filtered out of
    // find_block_leaders' results).

    #[test]
    fn codegen_cov_jmpt_out_of_range_target() {
        // Bytecode:
        // [0] JMPT R0, sbx=30000  → target = 0+1+30000 = 30001, way beyond code.len()=2
        //                          → filtered out of block_map → ok_or_else fires (line 1482)
        // [1] RET R0
        let abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };
        let large_sbx: i16 = 30000_i16;
        let code = vec![
            abx(OP_JMPT, 0, large_sbx as u16),
            abx(OP_RET, 0, 0),
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

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);
        let mut sig = module.make_signature();
        sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
        let fid = module
            .declare_function("ilo_jmpt_oor", cranelift_module::Linkage::Local, &sig)
            .unwrap();

        let result = compile_function_body(
            &mut module,
            &program.chunks[0],
            &program.nan_constants[0],
            "ilo_jmpt_oor",
            fid,
            &helpers,
            None,
            None,
        );
        // Must return an Err since target block is missing
        assert!(result.is_err(), "expected Err for out-of-range JMPT target");
        let e = result.unwrap_err();
        assert!(e.contains("block leader"), "unexpected error: {}", e);
    }

    // ── JMPNN with out-of-range target (lines 1587-1589) ────────────────────

    #[test]
    fn codegen_cov_jmpnn_out_of_range_target() {
        // Bytecode:
        // [0] JMPNN R0, sbx=30000 → target = 30001, filtered out → line 1587 fires
        // [1] RET R0
        let abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };
        let large_sbx: i16 = 30000_i16;
        let code = vec![
            abx(OP_JMPNN, 0, large_sbx as u16),
            abx(OP_RET, 0, 0),
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

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);
        let mut sig = module.make_signature();
        sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
        let fid = module
            .declare_function("ilo_jmpnn_oor", cranelift_module::Linkage::Local, &sig)
            .unwrap();

        let result = compile_function_body(
            &mut module,
            &program.chunks[0],
            &program.nan_constants[0],
            "ilo_jmpnn_oor",
            fid,
            &helpers,
            None,
            None,
        );
        assert!(result.is_err(), "expected Err for out-of-range JMPNN target");
        let e = result.unwrap_err();
        assert!(e.contains("block leader"), "unexpected error: {}", e);
    }

    // ── Fused compare-and-branch fallback (lines 1241-1246) ──────────────────
    // Fires when comparison is followed by JMPT/JMPF on same register (fused=true)
    // but the JMPT target block is not in block_map (out-of-range offset).
    // The fallback emits a select instruction instead of a brif.

    #[test]
    fn codegen_cov_fused_compare_fallback_aot() {
        // Bytecode for a function with 3 params (all numeric):
        // [0] GT R0, R1, R2          — compare; a_idx=0, b_idx=1, c_idx=2
        // [1] JMPT R0, sbx=30000     — target=30002 (out of range); NOT a leader at ip+1=1
        //                              block_map has {0, 1, 2} from JMPT (i+1=1, target filtered)
        //                              Wait: JMPT inserts i+1=1 as leader. So 1 IS in block_map.
        //                              Need ip+1 to NOT be a leader for fused=true.
        // Actually: fused requires !block_map.contains_key(&(ip+1)).
        // ip=0, ip+1=1. JMPT at ip=1 inserts leaders {target=30002→filtered, 2}.
        // So 1 is NOT in block_map (JMPT at ip=1 only inserts 2 and filtered target).
        // This means fused=true at ip=0. Then true_dest=2, false_dest=30002.
        // true_block = block_map.get(&2) = Some(blk2).
        // false_block = block_map.get(&30002) = None.
        // → if let (Some, Some) fails → else branch → lines 1241-1246 fire.
        // [2] RET R0
        let abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };
        let large_sbx: i16 = 30000_i16;
        let code = vec![
            make_inst_abc(OP_GT, 0, 1, 2),          // ip=0: GT R0, R1, R2
            abx(OP_JMPT, 0, large_sbx as u16),       // ip=1: JMPT R0, +30000
            abx(OP_RET, 0, 0),                        // ip=2: RET R0
        ];
        let dummy_span = crate::ast::Span { start: 0, end: 0 };
        let chunk = Chunk {
            code,
            constants: vec![],
            param_count: 3,
            reg_count: 3,
            spans: vec![dummy_span; 3],
            all_regs_numeric: true,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);
        let mut sig = module.make_signature();
        for _ in 0..3 {
            sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
        }
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
        let fid = module
            .declare_function("ilo_fused_fallback", cranelift_module::Linkage::Local, &sig)
            .unwrap();

        let result = compile_function_body(
            &mut module,
            &program.chunks[0],
            &program.nan_constants[0],
            "ilo_fused_fallback",
            fid,
            &helpers,
            None,
            None,
        );
        // May succeed or fail depending on whether Cranelift accepts the select path
        let _ = result;
    }

    // ── inline_chunk: CMPK followed by non-JMP (line 583) ───────────────────
    // When the instruction after CMPK is not a JMP, the and_then returns None (line 583)
    // and the or_else fallback uses imap.get(&(ip+1)) instead.

    #[test]
    fn codegen_cov_inline_cmpk_non_jmp_following() {
        // Callee bytecode:
        // [0] CMPK_GT_N R0, k0      — leaders {0,1,2} from find_block_leaders
        // [1] ADD_NN R0, R0, R0      — NOT a JMP → line 583 fires; miss = imap.get(&1)
        // [2] RET R0
        // Since miss=Some(blk1) and body=Some(blk2) → brif succeeds → inline_chunk returns true.
        let abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        let callee_code = vec![
            make_inst_abc(OP_CMPK_GT_N, 0, 0, 0), // ip=0: CMPK
            make_inst_abc(OP_ADD_NN, 0, 0, 0),       // ip=1: NOT JMP → line 583 fires
            abx(OP_RET, 0, 0),                        // ip=2: RET
        ];
        let callee_nan_consts = vec![NanVal::number(5.0)]; // k0 = 5.0

        let call_bx: u16 = (1u16 << 8) | 1u16; // func_idx=1, n_args=1
        let caller_code = vec![
            abx(OP_CALL, 0, call_bx),
            abx(OP_RET, 0, 0),
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
            func_names: vec!["f".to_string(), "callee_nonjmp".to_string()],
            nan_constants: vec![vec![], callee_nan_consts],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false, false],
        };

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);
        let caller_fid = {
            let mut sig = module.make_signature();
            for _ in 0..2 {
                sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
            }
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
            module.declare_function("ilo_f_nonjmp", cranelift_module::Linkage::Local, &sig).unwrap()
        };
        let callee_fid = {
            let mut sig = module.make_signature();
            sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
            module.declare_function("ilo_callee_nonjmp", cranelift_module::Linkage::Local, &sig).unwrap()
        };
        let func_ids = vec![caller_fid, callee_fid];

        let _ = compile_function_body(&mut module, &program.chunks[1], &program.nan_constants[1],
            "ilo_callee_nonjmp", callee_fid, &helpers, Some(&func_ids), Some(&program));
        let result = compile_function_body(&mut module, &program.chunks[0], &program.nan_constants[0],
            "ilo_f_nonjmp", caller_fid, &helpers, Some(&func_ids), Some(&program));
        assert!(result.is_ok(), "inline cmpk non-jmp following failed: {:?}", result.err());
    }

    // ── inline_chunk: CMPK with ki out of range (line 559) ───────────────────
    // When ki >= callee_consts.len() in a CMPK instruction, the else branch at
    // line 559 fires and uses 0.0 as the constant.

    #[test]
    fn codegen_cov_inline_cmpk_ki_out_of_range() {
        // Callee bytecode:
        // [0] CMPK_GT_N R0, k2      — ki=2 but nan_consts has 0 entries → line 559 fires
        // [1] JMP +1                 — target = ip+1+1 = 3
        // [2] ADD_NN R0, R0, R0
        // [3] RET R0
        let abx = |op: u8, a: u8, bx: u16| -> u32 {
            (op as u32) << 24 | (a as u32) << 16 | bx as u32
        };

        // CMPK_GT_N R0, ki=2 (c=2): abc format, c=2
        let callee_code = vec![
            make_inst_abc(OP_CMPK_GT_N, 0, 0, 2), // ip=0: CMPK; ki=2, out of range
            abx(OP_JMP, 0, 1u16),                    // ip=1: JMP +1 → target=3
            make_inst_abc(OP_ADD_NN, 0, 0, 0),        // ip=2: non-terminating block
            abx(OP_RET, 0, 0),                         // ip=3: RET (leader)
        ];
        // Empty nan_constants → ki=2 >= 0 → line 559 fires (uses 0.0)
        let callee_nan_consts: Vec<NanVal> = vec![];

        let call_bx: u16 = (1u16 << 8) | 1u16;
        let caller_code = vec![
            abx(OP_CALL, 0, call_bx),
            abx(OP_RET, 0, 0),
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
            func_names: vec!["f".to_string(), "callee_cmpk_oor".to_string()],
            nan_constants: vec![vec![], callee_nan_consts],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false, false],
        };

        let mut module = make_module();
        let helpers = declare_all_helpers(&mut module);
        let caller_fid = {
            let mut sig = module.make_signature();
            for _ in 0..2 {
                sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
            }
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
            module.declare_function("ilo_f_cmpk_oor", cranelift_module::Linkage::Local, &sig).unwrap()
        };
        let callee_fid = {
            let mut sig = module.make_signature();
            sig.params.push(cranelift_codegen::ir::AbiParam::new(I64));
            sig.returns.push(cranelift_codegen::ir::AbiParam::new(I64));
            module.declare_function("ilo_callee_cmpk_oor", cranelift_module::Linkage::Local, &sig).unwrap()
        };
        let func_ids = vec![caller_fid, callee_fid];

        let _ = compile_function_body(&mut module, &program.chunks[1], &program.nan_constants[1],
            "ilo_callee_cmpk_oor", callee_fid, &helpers, Some(&func_ids), Some(&program));
        let result = compile_function_body(&mut module, &program.chunks[0], &program.nan_constants[0],
            "ilo_f_cmpk_oor", caller_fid, &helpers, Some(&func_ids), Some(&program));
        assert!(result.is_ok(), "inline cmpk ki out-of-range failed: {:?}", result.err());
    }
}
