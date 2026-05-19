// Regression: cross-engine error message + call-stack parity.
//
// db-analyst rerun8 on v0.11.7 reported three divergences in runtime error
// shape between the tree walker, VM, and Cranelift JIT:
//
//   (a) Error codes diverged: tree emitted ILO-R009 for at-OOB while VM and
//       Cranelift emitted ILO-R004 for the same condition.
//   (b) VM and Cranelift dropped the rich values: tree said "index 99 out
//       of range for list of length 3"; VM/Cranelift said "index out of
//       range" with no values, forcing the agent to add `prnt` to discover
//       what bad index actually fired.
//   (c) Cranelift dropped the call-stack notes entirely: tree and VM
//       produced `notes:["called from 'main'", "called from 'g'"]`,
//       Cranelift produced `notes:[]`.
//
// These tests assert the first-pass fix:
//   - (b) at/lst OOB sites in VM and Cranelift now produce the full
//     `"<builtin>: index N out of range for list/text of length M"` message,
//     matching tree's wording.
//   - (a) Since the OOB sites switched from `VmError::Type` (R004) to
//     `VmError::Runtime` (R009), the code is also reconciled with tree.
//   - (c) Cranelift now seeds the JIT call stack with the entry function
//     name and pushes/pops frames around each direct OP_CALL, so a runtime
//     error surfaces with `notes` matching VM/tree.
//
// Parked (filed as follow-ups, not covered here):
//   - mget-on-non-map code reconciliation (tree R009 vs VM/Cranelift R004).
//   - jpth missing-key panic-unwrap code reconciliation (tree R026 vs
//     VM/Cranelift R009).
//   - num!! parse-fail code reconciliation (same shape).

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--vm"];

/// Run `src` on every engine and return the JSON stderr line for each, so
/// the caller can assert on shape parity directly (message, code, notes).
fn run_on_all_engines(src_path: &str, entry: &str) -> Vec<(String, String)> {
    ENGINES_ALL
        .iter()
        .map(|engine| {
            let out = ilo()
                .args(["run", "--json", engine, src_path, entry])
                .output()
                .expect("failed to spawn ilo");
            assert!(
                !out.status.success(),
                "engine={engine}: expected non-zero exit on runtime error\nstdout={}\nstderr={}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
            );
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            ((*engine).to_string(), stderr)
        })
        .collect()
}

fn write_src(src: &str, name: &str) -> String {
    let dir = std::env::temp_dir().join("ilo-cross-engine-error-parity");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, src).unwrap();
    path.to_str().unwrap().to_owned()
}

// (a) + (b): `at` OOB on a list surfaces ILO-R009 with the full
// "index N out of range for list of length M" message on every engine.
#[test]
fn at_list_oob_has_rich_message_and_r009_on_every_engine() {
    let src = "g xs:L n>n;at xs 99\nmain>n;xs=[1,2,3];g xs\n";
    let path = write_src(src, "at_list_oob.ilo");
    for (engine, stderr) in run_on_all_engines(&path, "main") {
        assert!(
            stderr.contains("ILO-R009"),
            "engine={engine}: expected ILO-R009 (matching tree), got:\n{stderr}"
        );
        assert!(
            stderr.contains("at: index 99 out of range for list of length 3"),
            "engine={engine}: expected rich tree-style message, got:\n{stderr}"
        );
    }
}

// (b): `at` OOB on text surfaces with the same rich shape, mentioning text
// length rather than list length.
#[test]
fn at_text_oob_has_rich_message_and_r009_on_every_engine() {
    let src = "g s:t>t;at s 50\nmain>t;s=\"hi\";g s\n";
    let path = write_src(src, "at_text_oob.ilo");
    for (engine, stderr) in run_on_all_engines(&path, "main") {
        assert!(
            stderr.contains("ILO-R009"),
            "engine={engine}: expected ILO-R009, got:\n{stderr}"
        );
        assert!(
            stderr.contains("at: index 50 out of range for text of length 2"),
            "engine={engine}: expected rich text message, got:\n{stderr}"
        );
    }
}

// (a) + (b): `lst` OOB likewise reports the full "index N out of range for
// list of length M" wording with R009 across engines.
#[test]
fn lst_oob_has_rich_message_and_r009_on_every_engine() {
    let src = "g xs:L n>L n;lst xs 99 0\nmain>L n;xs=[1,2,3];g xs\n";
    let path = write_src(src, "lst_oob.ilo");
    for (engine, stderr) in run_on_all_engines(&path, "main") {
        assert!(
            stderr.contains("ILO-R009"),
            "engine={engine}: expected ILO-R009, got:\n{stderr}"
        );
        assert!(
            stderr.contains("lst: index 99 out of range for list of length 3"),
            "engine={engine}: expected rich lst message, got:\n{stderr}"
        );
    }
}

// (c): Cranelift's call_stack now matches tree/VM. The repro is the
// db-analyst report's exact shape: main calls g, g raises OOB, every
// engine reports notes=["called from 'main'", "called from 'g'"].
#[test]
fn call_stack_notes_match_across_engines_two_levels() {
    let src = "g xs:L n>n;at xs 99\nmain>n;xs=[1,2,3];g xs\n";
    let path = write_src(src, "callstack_two_levels.ilo");
    for (engine, stderr) in run_on_all_engines(&path, "main") {
        assert!(
            stderr.contains("\"called from 'main'\""),
            "engine={engine}: expected note for 'main', got:\n{stderr}"
        );
        assert!(
            stderr.contains("\"called from 'g'\""),
            "engine={engine}: expected note for 'g', got:\n{stderr}"
        );
    }
}

// (c) stress: three-level call chain (main → h → g). Cranelift's
// pre-fix behaviour was `notes:[]`; tree/VM produced all three. With the
// per-thread JIT call-stack snapshot at error-set time, Cranelift now
// reports all three names too.
#[test]
fn call_stack_notes_match_across_engines_three_levels() {
    let src = "g xs:L n>n;at xs 99\nh xs:L n>n;a=g xs;+ a 1\nmain>n;xs=[1,2,3];h xs\n";
    let path = write_src(src, "callstack_three_levels.ilo");
    for (engine, stderr) in run_on_all_engines(&path, "main") {
        for expected in [
            "\"called from 'main'\"",
            "\"called from 'h'\"",
            "\"called from 'g'\"",
        ] {
            assert!(
                stderr.contains(expected),
                "engine={engine}: missing note {expected}, got:\n{stderr}"
            );
        }
    }
}

// Sanity: when the JIT entry function itself raises (no nested call),
// the notes still mention the entry. Previously Cranelift produced an
// empty notes array because no call stack was tracked at all.
#[test]
fn call_stack_notes_present_when_entry_errors_directly() {
    let src = "main>n;xs=[1,2,3];at xs 99\n";
    let path = write_src(src, "callstack_entry_only.ilo");
    for (engine, stderr) in run_on_all_engines(&path, "main") {
        assert!(
            stderr.contains("\"called from 'main'\""),
            "engine={engine}: expected entry-frame note, got:\n{stderr}"
        );
    }
}
