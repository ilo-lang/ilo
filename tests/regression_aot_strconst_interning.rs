// Regression: AOT compile must not collide on data-section names when
// more than one function emits string constants (`OP_LOADK` of a Text
// constant, `OP_RECFLD_NAME`, `OP_RECFLD_NAME_SAFE`, or `OP_RECWITH`).
//
// Background:
//
// `compile_function_body` (src/vm/compile_cranelift.rs) is called once
// per ilo chunk and keeps a function-local `data_section_counter`
// reset to 0 every call. The data-section names it derives from that
// counter (`ilo_strconst_N`, `ilo_fldname_N`, `ilo_fldname_safe_N`,
// `ilo_recwith_indices_N`) are declared at module scope via
// `module.declare_data`, so the moment two functions in the same
// AOT program each emit any string constant, both try to declare
// `ilo_strconst_1` and cranelift's ObjectModule errors with
// `Duplicate definition of identifier: ilo_strconst_1`.
//
// JIT does not hit this: each JIT compile is its own one-function
// module so the per-function counter is also the module counter.
//
// Fix: prefix data-section names with the cranelift function name
// (already module-unique from the first declaration pass), so
// `pick`'s first string constant becomes `ilo_pick_strconst_1` and
// `main`'s first becomes `ilo_main_strconst_1`. No collision.
//
// Gated on `cranelift` because the AOT compile path requires it.

#![cfg(feature = "cranelift")]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn tmp_paths(tag: &str) -> (PathBuf, PathBuf) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let src = std::env::temp_dir().join(format!("ilo-strconst-{tag}-{pid}-{n}.ilo"));
    let bin = std::env::temp_dir().join(format!("ilo-strconst-{tag}-{pid}-{n}.bin"));
    (src, bin)
}

/// Compile, run, and assert the AOT binary matches expected stdout. Also
/// compares against tree / VM / JIT byte-for-byte so any future engine
/// drift surfaces here.
fn assert_aot_cross_engine(tag: &str, src: &str, entry: Option<&str>, expected_stdout: &[u8]) {
    let (src_path, bin_path) = tmp_paths(tag);
    std::fs::write(&src_path, src).expect("write ilo source");

    // AOT compile + run.
    let mut compile = ilo();
    compile
        .args(["compile"])
        .arg(&src_path)
        .arg("-o")
        .arg(&bin_path);
    if let Some(e) = entry {
        compile.arg(e);
    }
    let c = compile.output().expect("invoke ilo compile");
    assert!(
        c.status.success(),
        "{tag}: ilo compile failed: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&c.stdout),
        String::from_utf8_lossy(&c.stderr),
    );
    let aot = Command::new(&bin_path).output().expect("run AOT binary");
    assert_eq!(
        aot.stdout,
        expected_stdout,
        "{tag}: AOT stdout mismatch. got={:?} expected={:?}",
        String::from_utf8_lossy(&aot.stdout),
        String::from_utf8_lossy(expected_stdout),
    );

    // Cross-engine parity. Run the same source through VM, JIT and
    // require byte-identical stdout. Tree-walker is exercised via the VM
    // bridge for the ops that still dispatch through it.
    for engine in ["--vm", "--jit"] {
        let mut cmd = ilo();
        cmd.arg(&src_path).arg(engine);
        if let Some(e) = entry {
            cmd.arg(e);
        }
        let out = cmd.output().expect("run ilo in-process");
        assert_eq!(
            out.stdout,
            expected_stdout,
            "{tag}/{engine}: stdout diverges. got={:?} expected={:?}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(expected_stdout),
        );
    }

    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&bin_path);
}

// ── Two functions, two string constants ──────────────────────────────
//
// The minimal trigger: any program where two AOT chunks each emit at
// least one Text constant. Before the fix this errored with
// `Duplicate definition of identifier: ilo_strconst_1`.

#[test]
fn aot_two_functions_each_with_string_constant() {
    assert_aot_cross_engine(
        "two-fn-two-strconst",
        "g>t;\"hello\"\nm>t;\"world\"",
        Some("m"),
        b"world\n",
    );
}

// ── Sum-type variant via `pick` (the original repro from #413) ───────
//
// `S dog cat` is a sum type with tag strings `dog` and `cat`. The
// `pick` helper emits string constants for both arms. With `main`
// calling `pick`, the AOT module has two chunks both emitting
// strconsts.

#[test]
fn aot_sum_type_pick_compiles_and_runs() {
    assert_aot_cross_engine(
        "sum-type-pick",
        "pick a:S dog cat>t;?a{\"dog\":\"woof\";\"cat\":\"cat\"}\nmain>t;pick \"cat\"\n",
        Some("main"),
        b"cat\n",
    );
}

// ── Many functions, many string constants ────────────────────────────
//
// Stresses the counter past 1, ensuring `*_strconst_2`,
// `*_strconst_3`, etc. also stay unique across functions. Three
// functions, two string constants each.

#[test]
fn aot_many_functions_many_strconsts() {
    let src = "\
a>t;cat [\"a-\", \"1\"] \"\"\n\
b>t;cat [\"b-\", \"2\"] \"\"\n\
m>t;cat [(a), (b), \"!\"] \"\"\n\
";
    assert_aot_cross_engine("many-fn-many-strconst", src, Some("m"), b"a-1b-2!\n");
}
