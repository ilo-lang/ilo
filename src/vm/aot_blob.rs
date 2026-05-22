//! Serialised `CompiledProgram` blob embedded into AOT binaries.
//!
//! AOT-compiled binaries need the full `CompiledProgram` at runtime so the
//! Cranelift HOF / closure dispatch helpers (`jit_call_dyn`, `jit_call_builtin_tree`)
//! can re-enter the VM on user-fn callbacks and resolve FnRef names. Before this
//! module existed, the AOT runtime had no `CompiledProgram` published in
//! `ACTIVE_PROGRAM` / `ACTIVE_AST_PROGRAM`, so every HOF callback hit the null-program
//! guard and silently returned `TAG_NIL` — manifesting as `[nil, nil, ...]` for
//! `map (lambda) xs`, `nil` for `fld add xs 0`, `nil` for `grp/uniqby`, and so on
//! (engine audit PR #413 gap #1).
//!
//! The fix: serialise the `CompiledProgram` with `postcard`, embed the byte blob
//! in a `.rodata` section of the AOT object file, and at `main` entry call
//! `ilo_aot_publish_program(ptr, len)` to deserialise + leak a static
//! `CompiledProgram` and publish its pointers into the `with_active_registry`
//! TLS slots for the lifetime of the process. The chunk constant pool is narrow
//! at compile time (`Number` / `Text` / `Bool` / `Nil`, plus `List` from the
//! record-`with` lowering) so the wire format keeps the constant variant set
//! tight rather than serialising the full `Value` enum. The `From<&Value>`
//! impl on `WireConst` panics with a clear message if a future compiler change
//! starts emitting a new variant — the existing AOT coverage suite catches it
//! before any binary ships.
//!
//! `schema_version` is the first field so a future change to the wire format
//! can be detected and rejected with a clean error rather than a silent
//! mis-deserialisation.

use serde::{Deserialize, Serialize};

use super::{Chunk, CompiledProgram, TypeRegistry};
use crate::ast::{Program, Span};
use crate::interpreter::Value;
use std::sync::Arc;

/// Bump whenever the on-disk shape changes in a way old runtimes cannot read.
/// The runtime rejects blobs with a `schema_version` it does not recognise.
pub const BLOB_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProgramBlob {
    pub schema_version: u32,
    pub chunks: Vec<WireChunk>,
    pub func_names: Vec<String>,
    pub is_tool: Vec<bool>,
    /// Tuples of `(name, fields, num_fields_bitmask)` — same shape as
    /// `TypeRegistry::register` consumes.
    pub type_registry_entries: Vec<(String, Vec<String>, u64)>,
    /// AST is serialised as JSON inside the postcard envelope. Two reasons:
    /// (1) `Program::serialize_decls` is a custom `serialize_seq(None)` impl
    ///     that postcard's no-len-prefix encoding rejects with "The length
    ///     of a sequence must be known". serde_json doesn't require sized
    ///     sequences, and the AST already serialises cleanly via JSON for
    ///     the `--ast` flag.
    /// (2) The AST is only consumed by the tree-bridge in `call_builtin_for_bridge_with_program`,
    ///     which is rare — most HOF dispatch goes through OP_CALL_DYN's
    ///     VM re-entry which only needs `chunks` + `func_names`. Paying a
    ///     small encoding-mismatch tax to keep the AST surface intact
    ///     beats inventing a wire AST.
    pub ast_json: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WireChunk {
    pub code: Vec<u32>,
    pub constants: Vec<WireConst>,
    pub param_count: u8,
    pub reg_count: u8,
    pub spans: Vec<(u32, u32)>,
    pub all_regs_numeric: bool,
}

/// Constant pool entry. The `RegCompiler` emits `Nil` / `Number` / `Text` /
/// `Bool` from literals + match patterns + constant folding, plus `List` from
/// the record-`with` lowering (see `RegCompiler` lines ~5380-5394). If a future
/// compiler change adds a new constant variant the `From<&Value>` impl below
/// will panic with a clear message — caught by the existing test suite before
/// any binary ships.
#[derive(Debug, Serialize, Deserialize)]
pub enum WireConst {
    Nil,
    Number(f64),
    Text(String),
    Bool(bool),
    /// `Value::List` constant. Record-`with` lowering inlines either a list of
    /// numeric field indices or a list of field-name strings as a single
    /// constant; both cases use only the four scalar variants above so we
    /// reuse `WireConst` recursively.
    List(Vec<WireConst>),
}

impl From<&Value> for WireConst {
    fn from(v: &Value) -> Self {
        match v {
            Value::Nil => WireConst::Nil,
            Value::Number(n) => WireConst::Number(*n),
            Value::Text(s) => WireConst::Text(s.as_ref().clone()),
            Value::Bool(b) => WireConst::Bool(*b),
            Value::List(items) => WireConst::List(items.iter().map(WireConst::from).collect()),
            other => panic!(
                "aot_blob: unexpected chunk constant variant {:?} — only Nil/Number/Text/Bool/List are emitted by RegCompiler today; add a wire variant before lifting this",
                std::mem::discriminant(other)
            ),
        }
    }
}

impl From<WireConst> for Value {
    fn from(c: WireConst) -> Self {
        match c {
            WireConst::Nil => Value::Nil,
            WireConst::Number(n) => Value::Number(n),
            WireConst::Text(s) => Value::Text(Arc::new(s)),
            WireConst::Bool(b) => Value::Bool(b),
            WireConst::List(items) => {
                Value::List(Arc::new(items.into_iter().map(Value::from).collect()))
            }
        }
    }
}

impl WireChunk {
    pub fn from_chunk(chunk: &Chunk) -> Self {
        WireChunk {
            code: chunk.code.clone(),
            constants: chunk.constants.iter().map(WireConst::from).collect(),
            param_count: chunk.param_count,
            reg_count: chunk.reg_count,
            spans: chunk
                .spans
                .iter()
                .map(|s| (s.start as u32, s.end as u32))
                .collect(),
            all_regs_numeric: chunk.all_regs_numeric,
        }
    }

    pub fn into_chunk(self) -> Chunk {
        Chunk {
            code: self.code,
            constants: self.constants.into_iter().map(Value::from).collect(),
            param_count: self.param_count,
            reg_count: self.reg_count,
            spans: self
                .spans
                .into_iter()
                .map(|(s, e)| Span {
                    start: s as usize,
                    end: e as usize,
                })
                .collect(),
            all_regs_numeric: self.all_regs_numeric,
        }
    }
}

/// Serialise a `CompiledProgram` for embedding in an AOT binary. Errors
/// surface to the AOT compile path so a corrupt program never silently
/// ships.
pub fn serialize_program(program: &CompiledProgram) -> Result<Vec<u8>, String> {
    let chunks: Vec<WireChunk> = program.chunks.iter().map(WireChunk::from_chunk).collect();
    let type_registry_entries: Vec<(String, Vec<String>, u64)> = program
        .type_registry
        .types
        .iter()
        .map(|ti| (ti.name.clone(), ti.fields.clone(), ti.num_fields))
        .collect();
    let ast_json = match &program.ast {
        Some(ast) => serde_json::to_string(ast.as_ref())
            .map_err(|e| format!("serde_json serialize ast: {}", e))?,
        None => "{\"declarations\":[]}".to_string(),
    };
    let blob = ProgramBlob {
        schema_version: BLOB_SCHEMA_VERSION,
        chunks,
        func_names: program.func_names.clone(),
        is_tool: program.is_tool.clone(),
        type_registry_entries,
        ast_json,
    };
    postcard::to_allocvec(&blob).map_err(|e| format!("postcard serialize: {}", e))
}

/// Deserialise a blob into a fully-formed `CompiledProgram` ready to be
/// published by `with_active_registry`. The caller is responsible for
/// keeping the returned program alive for the lifetime of any code that
/// reads `ACTIVE_PROGRAM` (the AOT runtime leaks it for the process).
pub fn deserialize_program(bytes: &[u8]) -> Result<CompiledProgram, String> {
    let blob: ProgramBlob =
        postcard::from_bytes(bytes).map_err(|e| format!("postcard deserialize: {}", e))?;
    if blob.schema_version != BLOB_SCHEMA_VERSION {
        return Err(format!(
            "AOT program blob schema_version mismatch: binary embeds v{} but this runtime expects v{}. Recompile with this ilo version.",
            blob.schema_version, BLOB_SCHEMA_VERSION
        ));
    }
    let chunks: Vec<Chunk> = blob.chunks.into_iter().map(WireChunk::into_chunk).collect();
    let nan_constants: Vec<Vec<super::NanVal>> = chunks
        .iter()
        .map(|c| c.constants.iter().map(super::NanVal::from_value).collect())
        .collect();
    let mut type_registry = TypeRegistry::default();
    for (name, fields, num_fields) in blob.type_registry_entries {
        type_registry.register(name, fields, num_fields);
    }
    let ast: Program = serde_json::from_str(&blob.ast_json)
        .map_err(|e| format!("serde_json deserialize ast: {}", e))?;
    // Reconstruct is_defer_fn from the AST (parallel to compile_program logic).
    let n_fns = blob.func_names.len();
    let mut is_defer_fn = vec![false; n_fns];
    for (i, decl) in ast
        .declarations
        .iter()
        .filter(|d| {
            matches!(
                d,
                crate::ast::Decl::Function { .. } | crate::ast::Decl::Tool { .. }
            )
        })
        .enumerate()
    {
        if let crate::ast::Decl::Function { body, .. } = decl {
            if i < n_fns {
                is_defer_fn[i] = crate::vm::body_has_defer(body);
            }
        }
    }
    Ok(CompiledProgram {
        chunks,
        func_names: blob.func_names,
        nan_constants,
        type_registry,
        is_tool: blob.is_tool,
        is_defer_fn,
        ast: Some(Arc::new(ast)),
        defer_fns: std::collections::HashSet::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::compile;

    fn roundtrip(src: &str) -> CompiledProgram {
        let tokens = crate::lexer::lex(src).expect("lex");
        let token_spans: Vec<_> = tokens
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
        let (program, errors) = crate::parser::parse(token_spans);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let compiled = compile(&program).expect("compile");
        let bytes = serialize_program(&compiled).expect("serialize");
        deserialize_program(&bytes).expect("deserialize")
    }

    #[test]
    fn empty_program_roundtrips() {
        let r = roundtrip("main>n;42");
        assert_eq!(r.func_names, vec!["main".to_string()]);
        assert_eq!(r.chunks.len(), 1);
        assert!(r.ast.is_some());
    }

    #[test]
    fn schema_version_mismatch_is_rejected() {
        let blob = ProgramBlob {
            schema_version: 999,
            chunks: vec![],
            func_names: vec![],
            is_tool: vec![],
            type_registry_entries: vec![],
            ast_json: "{\"declarations\":[]}".to_string(),
        };
        let bytes = postcard::to_allocvec(&blob).unwrap();
        let err = match deserialize_program(&bytes) {
            Ok(_) => panic!("expected schema mismatch error, got Ok"),
            Err(e) => e,
        };
        assert!(err.contains("schema_version mismatch"), "got: {}", err);
    }

    #[test]
    fn map_lambda_program_roundtrips() {
        let r = roundtrip("main>L n;map (x:n>n;*x 2) [1,2,3]");
        let nv: Vec<&str> = r.func_names.iter().map(|s| s.as_str()).collect();
        assert!(nv.contains(&"main"));
        assert!(nv.iter().any(|n| n.starts_with("__lit_")));
    }

    #[test]
    fn fld_program_roundtrips() {
        let r = roundtrip("add a:n b:n>n;+a b\nmain>n;fld add [1,2,3,4] 0");
        assert!(r.func_names.contains(&"add".to_string()));
        assert!(r.func_names.contains(&"main".to_string()));
    }

    #[test]
    fn type_registry_roundtrips() {
        // Build a CompiledProgram with a populated type registry by hand and
        // exercise the serialise/deserialise path directly. The surface
        // syntax for record construction is finicky enough that doing it
        // through the full parser distracts from what we are checking here:
        // type registry entries make it through the blob round-trip.
        let mut tr = TypeRegistry::default();
        tr.register(
            "point".to_string(),
            vec!["x".to_string(), "y".to_string()],
            0b11, // both fields numeric
        );
        let prog = CompiledProgram {
            chunks: vec![],
            func_names: vec![],
            nan_constants: vec![],
            type_registry: tr,
            is_tool: vec![],
            is_defer_fn: vec![],
            ast: None,
            defer_fns: std::collections::HashSet::new(),
        };
        let bytes = serialize_program(&prog).expect("serialize");
        let r = deserialize_program(&bytes).expect("deserialize");
        assert!(r.type_registry.name_to_id.contains_key("point"));
        let id = r.type_registry.name_to_id["point"];
        let info = &r.type_registry.types[id as usize];
        assert_eq!(info.fields, vec!["x".to_string(), "y".to_string()]);
        assert_eq!(info.num_fields, 0b11);
    }
}
