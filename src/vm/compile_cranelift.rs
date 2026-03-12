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
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlags};
use cranelift_codegen::ir::types::{I32, I64, F64};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{default_libcall_names, Module, Linkage, FuncId};
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
    string_const: FuncId,
}

fn declare_helper(module: &mut ObjectModule, name: &str, n_params: usize, n_returns: usize) -> FuncId {
    let mut sig = module.make_signature();
    for _ in 0..n_params {
        sig.params.push(AbiParam::new(I64));
    }
    for _ in 0..n_returns {
        sig.returns.push(AbiParam::new(I64));
    }
    module.declare_function(name, Linkage::Import, &sig).unwrap()
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
    let parent = std::path::Path::new(manifest_dir).parent()
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
        vec!["-lm", "-liconv", "-framework", "CoreFoundation",
             "-framework", "Security", "-framework", "SystemConfiguration"]
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
    let entry_idx = program.func_names.iter().position(|n| n == entry_func)
        .ok_or_else(|| format!("undefined function: {}", entry_func))?;

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

    // Generate a C runtime helper file with ilo_atof wrapper.
    // Variadic functions like printf have different ABI on ARM64,
    // so we wrap atof in a regular C function.
    let runtime_c_path = format!("{}_rt.c", output_path);
    std::fs::write(&runtime_c_path, concat!(
        "#include <stdlib.h>\n",
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

    // Helper to clean up temp files on any error path
    let cleanup = |obj: &str, rt: &str| {
        let _ = std::fs::remove_file(obj);
        let _ = std::fs::remove_file(rt);
    };

    // Declare all runtime helpers as imports (resolved at link time from libilo.a)
    let helpers = declare_all_helpers(&mut module);

    // Declare ilo_atof for CLI arg parsing
    let ilo_atof = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(I64));
        sig.returns.push(AbiParam::new(F64));
        module.declare_function("ilo_atof", Linkage::Import, &sig).map_err(|e| e.to_string())?
    };

    // First pass: declare all functions to get FuncIds
    let mut func_ids: Vec<FuncId> = Vec::with_capacity(program.chunks.len());
    for (i, chunk) in program.chunks.iter().enumerate() {
        let name = format!("ilo_{}", program.func_names[i]);
        let mut sig = module.make_signature();
        for _ in 0..chunk.param_count {
            sig.params.push(AbiParam::new(I64));
        }
        sig.returns.push(AbiParam::new(I64));
        let fid = module.declare_function(&name, Linkage::Local, &sig)
            .map_err(|e| e.to_string())?;
        func_ids.push(fid);
    }

    // Second pass: compile all functions with func_ids available
    for (i, (chunk, nan_consts)) in program.chunks.iter().zip(program.nan_constants.iter()).enumerate() {
        let name = format!("ilo_{}", program.func_names[i]);
        compile_function_body(
            &mut module, chunk, nan_consts, &name, func_ids[i], &helpers, Some(&func_ids),
        ).inspect_err(|_| { cleanup("", &runtime_o_path); })?;
    }

    let entry_func_id = func_ids[entry_idx];
    let entry_chunk = &program.chunks[entry_idx];

    // Generate main()
    generate_main(
        &mut module,
        entry_func_id,
        entry_chunk.param_count as usize,
        ilo_atof,
        &helpers,
    ).inspect_err(|_| { cleanup("", &runtime_o_path); })?;

    // Emit object file
    let obj_product = module.finish();
    let obj_bytes = obj_product.emit().map_err(|e| { cleanup("", &runtime_o_path); e.to_string() })?;

    let obj_path = format!("{}.o", output_path);
    std::fs::write(&obj_path, &obj_bytes)
        .map_err(|e| { cleanup("", &runtime_o_path); format!("failed to write object file: {}", e) })?;

    // Find libilo.a
    let libilo_path = find_libilo_a()
        .inspect_err(|_| { cleanup(&obj_path, &runtime_o_path); })?;
    let libilo_dir = std::path::Path::new(&libilo_path).parent()
        .ok_or_else(|| "invalid libilo.a path".to_string())?
        .to_string_lossy().to_string();

    // Link: user.o + runtime.o + libilo.a + system libs
    let mut link_cmd = std::process::Command::new("cc");
    link_cmd.arg(&obj_path)
        .arg(&runtime_o_path)
        .arg("-o")
        .arg(output_path)
        .arg(format!("-L{}", libilo_dir))
        .arg("-lilo");
    for flag in platform_linker_flags() {
        link_cmd.arg(flag);
    }

    let status = link_cmd.status()
        .map_err(|e| { cleanup(&obj_path, &runtime_o_path); format!("failed to run cc: {}", e) })?;

    cleanup(&obj_path, &runtime_o_path);

    if !status.success() {
        return Err(format!("linker failed with exit code: {}", status));
    }

    Ok(())
}

/// Compile a function body into the ObjectModule (function already declared).
fn compile_function_body(
    module: &mut ObjectModule,
    chunk: &Chunk,
    nan_consts: &[NanVal],
    _name: &str,
    func_id: FuncId,
    helpers: &HelperFuncs,
    all_func_ids: Option<&[FuncId]>,
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
    let mut get_func_ref = |builder: &mut FunctionBuilder, module: &mut ObjectModule, id: FuncId| -> cranelift_codegen::ir::FuncRef {
        *func_refs.entry(id).or_insert_with(|| module.declare_func_in_func(id, builder.func))
    };

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
            // ── Optimized numeric opcodes (kept from original AOT) ──
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
            // ── Generic arithmetic with inline numeric fast path + helper slow path ──
            OP_ADD | OP_SUB | OP_MUL | OP_DIV => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);

                let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                let b_masked = builder.ins().band(bv, qnan_val);
                let c_masked = builder.ins().band(cv, qnan_val);
                let b_or_c = builder.ins().bor(b_masked, c_masked);
                let both_num = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual, b_or_c, qnan_val);

                let num_block = builder.create_block();
                let slow_block = builder.create_block();
                let merge_block = builder.create_block();
                builder.append_block_param(merge_block, I64);

                builder.ins().brif(both_num, num_block, &[], slow_block, &[]);

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
            // ── Comparisons with inline numeric fast path ──
            OP_LT | OP_GT | OP_LE | OP_GE | OP_EQ | OP_NE => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);

                let qnan_val = builder.ins().iconst(I64, QNAN as i64);
                let b_masked = builder.ins().band(bv, qnan_val);
                let c_masked = builder.ins().band(cv, qnan_val);
                let b_or_c = builder.ins().bor(b_masked, c_masked);
                let both_num = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual, b_or_c, qnan_val);

                let num_block = builder.create_block();
                let slow_block = builder.create_block();
                let merge_block = builder.create_block();
                builder.append_block_param(merge_block, I64);

                builder.ins().brif(both_num, num_block, &[], slow_block, &[]);

                // Fast path: inline float comparison
                builder.switch_to_block(num_block);
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
                        _ => { b"\0".to_vec() }
                    };
                    data_section_counter += 1;
                    let ds_name = format!("ilo_strconst_{}", data_section_counter);
                    let str_ptr = create_data_section(module, &mut builder, &ds_name, &string_bytes)?;
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
                }
            }
            // ── Control flow ──
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
                let not_nil = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual, av, nil_val);
                let not_false = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual, av, false_val);
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
            // ── Builtins: 1-arg → 1-return ──
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
                    cranelift_codegen::ir::condcodes::IntCC::Equal, tag, arena_tag_val);

                let arena_block = builder.create_block();
                let heap_block = builder.create_block();
                let merge_block = builder.create_block();
                builder.append_block_param(merge_block, I64);

                builder.ins().brif(is_arena, arena_block, &[], heap_block, &[]);

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
                    cranelift_codegen::ir::condcodes::IntCC::Equal, masked, qnan_val);
                let clone_block = builder.create_block();
                let skip_clone_block = builder.create_block();
                builder.ins().brif(is_nan_tagged, clone_block, &[], skip_clone_block, &[]);

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
                // Dynamic field access by name — unsupported in AOT
                return Err(format!("OP_RECFLD_NAME not supported in AOT at instruction {}", ip));
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
                let cur_offset = builder.ins().load(I64, MemFlags::trusted(), arena_ptr_val, 16);
                let seven = builder.ins().iconst(I64, 7);
                let off_plus_7 = builder.ins().iadd(cur_offset, seven);
                let neg8 = builder.ins().iconst(I64, !7i64);
                let aligned = builder.ins().band(off_plus_7, neg8);
                let size_val = builder.ins().iconst(I64, record_size as i64);
                let new_offset = builder.ins().iadd(aligned, size_val);
                let buf_cap = builder.ins().load(I64, MemFlags::trusted(), arena_ptr_val, 8);
                let has_space = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThanOrEqual,
                    new_offset, buf_cap);

                let alloc_block = builder.create_block();
                let fallback_block = builder.create_block();
                let merge_block = builder.create_block();
                builder.append_block_param(merge_block, I64);

                builder.ins().brif(has_space, alloc_block, &[], fallback_block, &[]);

                // Inline alloc path
                builder.switch_to_block(alloc_block);
                let buf_ptr = builder.ins().load(I64, MemFlags::trusted(), arena_ptr_val, 0);
                let rec_ptr = builder.ins().iadd(buf_ptr, aligned);
                let header = ((n_fields as u64) << 16) | (type_id as u64);
                let header_val = builder.ins().iconst(I64, header as i64);
                builder.ins().store(MemFlags::trusted(), header_val, rec_ptr, 0);
                for i in 0..n_fields {
                    let field_v = builder.use_var(vars[a_idx + 1 + i]);
                    let field_off = (8 + i * 8) as i32;
                    builder.ins().store(MemFlags::trusted(), field_v, rec_ptr, field_off);
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
                builder.ins().store(MemFlags::trusted(), new_offset, arena_ptr_val, 16);
                let tag_val = builder.ins().iconst(I64, TAG_ARENA_REC as i64);
                let result_val = builder.ins().bor(rec_ptr, tag_val);
                builder.ins().jump(merge_block, &[result_val]);

                // Fallback: call jit_recnew helper
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
                // Get registry pointer at runtime
                let fref_reg = get_func_ref(&mut builder, module, helpers.get_registry_ptr);
                let reg_call = builder.ins().call(fref_reg, &[]);
                let registry_ptr_val = builder.inst_results(reg_call)[0];
                let fref = get_func_ref(&mut builder, module, helpers.recnew);
                let call_inst = builder.ins().call(fref, &[arena_ptr_val, type_id_nfields_val, regs_ptr, registry_ptr_val]);
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
                    Value::List(items) => items.iter().map(|v| match v {
                        Value::Number(n) => *n as u8,
                        _ => 0,
                    }).collect(),
                    _ => return Err(format!("OP_RECWITH: expected list constant at index {}", indices_idx)),
                };

                // Use a data section instead of leaking a Box
                let ds_name = format!("ilo_recwith_indices_{}", data_section_counter);
                data_section_counter += 1;
                let indices_gv = create_data_section(module, &mut builder, &ds_name, &update_indices)?;

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
                let n_updates_val = builder.ins().iconst(I64, n_updates as i64);
                let fref = get_func_ref(&mut builder, module, helpers.recwith);
                let call_inst = builder.ins().call(fref, &[old_rec, indices_gv, n_updates_val, regs_ptr]);
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
            // ── List get (foreach iteration) ──
            OP_LISTGET => {
                let bv = builder.use_var(vars[b_idx]);
                let cv = builder.use_var(vars[c_idx]);
                let fref = get_func_ref(&mut builder, module, helpers.listget);
                let call_inst = builder.ins().call(fref, &[bv, cv]);
                let result = builder.inst_results(call_inst)[0];

                let nil_const = builder.ins().iconst(I64, TAG_NIL as i64);
                let is_nil = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, result, nil_const);

                let jmp_block = block_map.get(&(ip + 1)).copied();
                let body_block = block_map.get(&(ip + 2)).copied();
                if let (Some(jb), Some(bb)) = (jmp_block, body_block) {
                    let unwrap_block = builder.create_block();
                    builder.ins().brif(is_nil, jb, &[], unwrap_block, &[]);

                    builder.switch_to_block(unwrap_block);
                    builder.seal_block(unwrap_block);
                    let fref2 = get_func_ref(&mut builder, module, helpers.unwrap);
                    let call_inst2 = builder.ins().call(fref2, &[result]);
                    let item = builder.inst_results(call_inst2)[0];
                    let fref_drop = get_func_ref(&mut builder, module, helpers.drop_rc);
                    builder.ins().call(fref_drop, &[result]);
                    builder.def_var(vars[a_idx], item);
                    builder.ins().jump(bb, &[]);
                    block_terminated = true;
                } else {
                    builder.def_var(vars[a_idx], result);
                }
            }
            // ── Function call (direct Cranelift call within the same module) ──
            OP_CALL => {
                let a = ((inst >> 16) & 0xFF) as u8;
                let bx = (inst & 0xFFFF) as usize;
                let func_idx = bx >> 8;
                let n_args = bx & 0xFF;

                if let Some(fids) = all_func_ids {
                    // Direct call: the target function is compiled in this module
                    let target_fid = fids[func_idx];
                    let target_fref = get_func_ref(&mut builder, module, target_fid);
                    let mut call_args = Vec::with_capacity(n_args);
                    for i in 0..n_args {
                        call_args.push(builder.use_var(vars[a as usize + 1 + i]));
                    }
                    let call_inst = builder.ins().call(target_fref, &call_args);
                    let result = builder.inst_results(call_inst)[0];
                    builder.def_var(vars[a as usize], result);
                } else {
                    // Fallback: use jit_call helper (should not happen if all_func_ids is provided)
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
                        let prog_ptr = builder.ins().iconst(I64, 0i64); // null — not available in AOT
                        let func_idx_val = builder.ins().iconst(I64, func_idx as i64);
                        let n_args_val = builder.ins().iconst(I64, n_args as i64);
                        let fref = get_func_ref(&mut builder, module, helpers.call);
                        let call_inst = builder.ins().call(fref, &[prog_ptr, func_idx_val, args_ptr, n_args_val]);
                        let result = builder.inst_results(call_inst)[0];
                        builder.def_var(vars[a as usize], result);
                    } else {
                        let null_ptr = builder.ins().iconst(I64, 0i64);
                        let prog_ptr = builder.ins().iconst(I64, 0i64);
                        let func_idx_val = builder.ins().iconst(I64, func_idx as i64);
                        let n_args_val = builder.ins().iconst(I64, 0i64);
                        let fref = get_func_ref(&mut builder, module, helpers.call);
                        let call_inst = builder.ins().call(fref, &[prog_ptr, func_idx_val, null_ptr, n_args_val]);
                        let result = builder.inst_results(call_inst)[0];
                        builder.def_var(vars[a as usize], result);
                    }
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
    Ok(())
}

/// Generate the `main(argc, argv)` entry point.
fn generate_main(
    module: &mut ObjectModule,
    user_func_id: FuncId,
    param_count: usize,
    ilo_atof: FuncId,
    helpers: &HelperFuncs,
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

    // Call ilo_aot_init() at the start
    let init_fref = module.declare_func_in_func(helpers.aot_init, builder.func);
    builder.ins().call(init_fref, &[]);

    let user_fref = module.declare_func_in_func(user_func_id, builder.func);
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

    // Print the result using jit_prt (handles all value types: numbers, strings, bools, etc.)
    let prt_fref = module.declare_func_in_func(helpers.prt, builder.func);
    builder.ins().call(prt_fref, &[result]);

    // Call ilo_aot_fini() before returning
    let fini_fref = module.declare_func_in_func(helpers.aot_fini, builder.func);
    builder.ins().call(fini_fref, &[]);
    let zero = builder.ins().iconst(I32, 0);
    builder.ins().return_(&[zero]);

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

/// Compile an ilo program to a benchmark binary that loops and reports ns/call.
pub fn compile_to_bench_binary(
    program: &CompiledProgram,
    entry_func: &str,
    output_path: &str,
) -> Result<(), String> {
    let entry_idx = program.func_names.iter().position(|n| n == entry_func)
        .ok_or_else(|| format!("undefined function: {}", entry_func))?;

    let mut flag_builder = settings::builder();
    flag_builder.set("opt_level", "speed").map_err(|e| e.to_string())?;
    flag_builder.set("is_pic", "true").map_err(|e| e.to_string())?;
    let isa_builder = cranelift_native::builder().map_err(|e| e.to_string())?;
    let isa = isa_builder.finish(settings::Flags::new(flag_builder)).map_err(|e| e.to_string())?;

    let obj_builder = ObjectBuilder::new(
        isa.clone(),
        "ilo_aot_bench",
        default_libcall_names(),
    ).map_err(|e| e.to_string())?;
    let mut module = ObjectModule::new(obj_builder);

    let helpers = declare_all_helpers(&mut module);

    // First pass: declare all functions to get FuncIds
    let mut func_ids: Vec<FuncId> = Vec::with_capacity(program.chunks.len());
    for (i, chunk) in program.chunks.iter().enumerate() {
        let name = format!("ilo_{}", program.func_names[i]);
        let linkage = if i == entry_idx { Linkage::Export } else { Linkage::Local };
        let mut sig = module.make_signature();
        for _ in 0..chunk.param_count {
            sig.params.push(AbiParam::new(I64));
        }
        sig.returns.push(AbiParam::new(I64));
        let fid = module.declare_function(&name, linkage, &sig)
            .map_err(|e| e.to_string())?;
        func_ids.push(fid);
    }

    // Second pass: compile all functions with func_ids available
    for (i, (chunk, nan_consts)) in program.chunks.iter().zip(program.nan_constants.iter()).enumerate() {
        let name = format!("ilo_{}", program.func_names[i]);
        compile_function_body(
            &mut module, chunk, nan_consts, &name, func_ids[i], &helpers, Some(&func_ids),
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
    let mut c_code = String::from(
        "#include <stdio.h>\n\
         #include <stdlib.h>\n\
         #include <stdint.h>\n\
         #include <string.h>\n\
         #include <time.h>\n\n\
         extern void ilo_aot_init(void);\n\
         extern void ilo_aot_fini(void);\n\n\
         static int64_t encode_arg(const char* s) {\n\
         \tdouble d = atof(s);\n\
         \tint64_t r;\n\
         \tmemcpy(&r, &d, 8);\n\
         \treturn r;\n\
         }\n\n"
    );

    // Declare the exported function
    c_code.push_str(&format!("extern int64_t {}(", func_name));
    for i in 0..param_count {
        if i > 0 { c_code.push_str(", "); }
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
        c_code.push_str(&format!("\tint64_t a{} = encode_arg(argv[{}]);\n", i, i + 2));
    }

    let call_args: String = (0..param_count)
        .map(|i| format!("a{}", i))
        .collect::<Vec<_>>()
        .join(", ");

    // Init + warmup
    c_code.push_str("\tilo_aot_init();\n");
    c_code.push_str(&format!("\t{}({});\n", func_name, call_args));

    // Timed loop
    c_code.push_str("\tstruct timespec start, end;\n");
    c_code.push_str("\tclock_gettime(CLOCK_MONOTONIC, &start);\n");
    c_code.push_str("\tvolatile int64_t r;\n");
    c_code.push_str(&format!(
        "\tfor (int i = 0; i < iters; i++) r = {}({});\n", func_name, call_args
    ));
    c_code.push_str("\tclock_gettime(CLOCK_MONOTONIC, &end);\n");
    c_code.push_str("\tlong ns = (end.tv_sec - start.tv_sec) * 1000000000L + (end.tv_nsec - start.tv_nsec);\n");
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
    let libilo_dir = std::path::Path::new(&libilo_path).parent()
        .ok_or_else(|| "invalid libilo.a path".to_string())?
        .to_string_lossy().to_string();

    let mut link_cmd = std::process::Command::new("cc");
    link_cmd.arg(&obj_path)
        .arg(&bench_o_path)
        .arg("-o")
        .arg(output_path)
        .arg(format!("-L{}", libilo_dir))
        .arg("-lilo");
    for flag in platform_linker_flags() {
        link_cmd.arg(flag);
    }

    let status = link_cmd.status()
        .map_err(|e| {
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
}
