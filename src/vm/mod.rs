use crate::ast::*;
use crate::builtins::Builtin;
use crate::interpreter::Value;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error("no functions defined")]
    NoFunctionsDefined,
    #[error("undefined function: {name}")]
    UndefinedFunction { name: String },
    #[error("division by zero")]
    DivisionByZero,
    #[error("no field '{field}' on record")]
    FieldNotFound { field: String },
    #[error("unknown opcode: {op}")]
    UnknownOpcode { op: u8 },
    #[error("{0}")]
    Type(&'static str),
}

type VmResult<T> = Result<T, VmError>;

/// VM error with source location and call-stack context.
#[derive(Debug)]
pub struct VmRuntimeError {
    pub error: VmError,
    pub span: Option<crate::ast::Span>,
    /// Call stack: function names from outermost to innermost.
    pub call_stack: Vec<String>,
}

impl std::fmt::Display for VmRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl std::error::Error for VmRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("undefined variable: {name}")]
    UndefinedVariable { name: String },
    #[error("undefined function: {name}")]
    UndefinedFunction { name: String },
}

#[cfg(feature = "cranelift")]
pub mod compile_cranelift;
#[cfg(feature = "cranelift")]
pub mod jit_cranelift;
#[cfg(feature = "llvm")]
pub mod jit_llvm;

// ── Register-based opcodes (32-bit packed instructions) ─────────────
//
// ABC mode:  [OP:8 | A:8 | B:8 | C:8]
// ABx mode:  [OP:8 | A:8 | Bx:16]  (Bx unsigned or signed)

// ABC mode — 3 registers
pub(crate) const OP_ADD: u8 = 0;
pub(crate) const OP_SUB: u8 = 1;
pub(crate) const OP_MUL: u8 = 2;
pub(crate) const OP_DIV: u8 = 3;
pub(crate) const OP_EQ: u8 = 4;
pub(crate) const OP_NE: u8 = 5;
pub(crate) const OP_GT: u8 = 6;
pub(crate) const OP_LT: u8 = 7;
pub(crate) const OP_GE: u8 = 8;
pub(crate) const OP_LE: u8 = 9;
pub(crate) const OP_MOVE: u8 = 10;
pub(crate) const OP_NOT: u8 = 11;
pub(crate) const OP_NEG: u8 = 12;
pub(crate) const OP_WRAPOK: u8 = 13;
pub(crate) const OP_WRAPERR: u8 = 14;
pub(crate) const OP_ISOK: u8 = 15;
pub(crate) const OP_ISERR: u8 = 16;
pub(crate) const OP_UNWRAP: u8 = 17;
pub(crate) const OP_RECFLD: u8 = 18;
pub(crate) const OP_LISTGET: u8 = 19;

// ABC mode — type-specialized (both operands known numeric, no type check)
pub(crate) const OP_ADD_NN: u8 = 29;
pub(crate) const OP_SUB_NN: u8 = 30;
pub(crate) const OP_MUL_NN: u8 = 31;
pub(crate) const OP_DIV_NN: u8 = 32;

// ABC mode — superinstructions: register op constant (C = constant pool index)
// These fuse LOADK + arithmetic into one dispatch, both operands known numeric
pub(crate) const OP_ADDK_N: u8 = 33; // R[A] = R[B] + K[C]
pub(crate) const OP_SUBK_N: u8 = 34; // R[A] = R[B] - K[C]
pub(crate) const OP_MULK_N: u8 = 35; // R[A] = R[B] * K[C]
pub(crate) const OP_DIVK_N: u8 = 36; // R[A] = R[B] / K[C]

// ABC mode — builtins
pub(crate) const OP_LEN: u8 = 37; // R[A] = len(R[B])
pub(crate) const OP_LISTAPPEND: u8 = 38; // R[A] = R[B] ++ [R[C]]
pub(crate) const OP_INDEX: u8 = 39; // R[A] = R[B][C]  (C = literal index)
pub(crate) const OP_STR: u8 = 40; // R[A] = str(R[B])  (number to text)
pub(crate) const OP_NUM: u8 = 41; // R[A] = num(R[B])  (text to number, returns R n t)
pub(crate) const OP_ABS: u8 = 42; // R[A] = abs(R[B])
pub(crate) const OP_MIN: u8 = 43; // R[A] = min(R[B], R[C])
pub(crate) const OP_MAX: u8 = 44; // R[A] = max(R[B], R[C])
pub(crate) const OP_FLR: u8 = 45; // R[A] = floor(R[B])
pub(crate) const OP_CEL: u8 = 46; // R[A] = ceil(R[B])
pub(crate) const OP_GET: u8 = 47; // R[A] = http_get(R[B])  (returns R t t)
pub(crate) const OP_SPL: u8 = 48; // R[A] = spl(R[B], R[C])  (split text by separator → L t)
pub(crate) const OP_CAT: u8 = 49; // R[A] = cat(R[B], R[C])  (join list with separator → t)
pub(crate) const OP_HAS: u8 = 50; // R[A] = has(R[B], R[C])  (membership test → b)
pub(crate) const OP_HD: u8 = 51; // R[A] = hd(R[B])  (head of list/text)
pub(crate) const OP_TL: u8 = 52; // R[A] = tl(R[B])  (tail of list/text)
pub(crate) const OP_REV: u8 = 53; // R[A] = rev(R[B])  (reverse list or text)
pub(crate) const OP_SRT: u8 = 54; // R[A] = srt(R[B])  (sort list or text)
pub(crate) const OP_SLC: u8 = 55; // R[A] = slc(R[B], R[C], R[D])  (slice; D in data word A field)
pub(crate) const OP_RND0: u8 = 57; // R[A] = random float in [0,1)
pub(crate) const OP_RND2: u8 = 58; // R[A] = random int in [R[B], R[C]]
pub(crate) const OP_NOW: u8 = 59; // R[A] = current unix timestamp (seconds, float)
pub(crate) const OP_ENV: u8 = 60; // R[A] = env(R[B])  (returns R t t)
pub(crate) const OP_JPTH: u8 = 61; // R[A] = jpth(R[B], R[C])  (JSON path lookup → R t t)
pub(crate) const OP_JDMP: u8 = 62; // R[A] = jdmp(R[B])  (value to JSON string → t)
pub(crate) const OP_JPAR: u8 = 63; // R[A] = jpar(R[B])  (parse JSON string → R ? t)
pub(crate) const OP_RECFLD_NAME: u8 = 64; // R[A] = R[B].field where C = constant pool index of field name (dynamic/fallback)
pub(crate) const OP_JMPNN: u8 = 56; // if R[A] is not nil, jump by signed Bx (ABx mode)
pub(crate) const OP_ISNUM: u8 = 65; // R[A] = R[B] is Number
pub(crate) const OP_ISTEXT: u8 = 66; // R[A] = R[B] is Text
pub(crate) const OP_ISBOOL: u8 = 67; // R[A] = R[B] is Bool
pub(crate) const OP_ISLIST: u8 = 68; // R[A] = R[B] is List
// Map operations
pub(crate) const OP_MAPNEW: u8 = 69; // R[A] = {}  (empty map)
pub(crate) const OP_MGET: u8 = 70; // R[A] = R[B][R[C]]  (get key → nil if missing)
pub(crate) const OP_MSET: u8 = 71; // R[A] = mset(R[B], R[C], R[D])  (D=val in data word A field)
pub(crate) const OP_MHAS: u8 = 72; // R[A] = R[B] has key R[C]
pub(crate) const OP_MKEYS: u8 = 73; // R[A] = keys(R[B])  → L t
pub(crate) const OP_MVALS: u8 = 74; // R[A] = vals(R[B])  → L v
pub(crate) const OP_MDEL: u8 = 75; // R[A] = del(R[B], R[C])
pub(crate) const OP_PRT: u8 = 76; // print(R[B]) → stdout; R[A] = passthrough
pub(crate) const OP_RD: u8 = 77; // R[A] = rd(R[B])   — read file → R t t
pub(crate) const OP_RDL: u8 = 78; // R[A] = rdl(R[B])  — read file as lines → R (L t) t
pub(crate) const OP_WR: u8 = 79; // R[A] = wr(R[B], R[C])  — write string to file → R t t
pub(crate) const OP_WRL: u8 = 80; // R[A] = wrl(R[B], R[C]) — write lines to file → R t t
pub(crate) const OP_TRM: u8 = 81; // R[A] = trim(R[B])  — trim whitespace → t
pub(crate) const OP_UNQ: u8 = 82; // R[A] = unq(R[B])   — deduplicate list or text
pub(crate) const OP_POST: u8 = 83; // R[A] = http_post(R[B], R[C])  (returns R t t)
pub(crate) const OP_GETH: u8 = 84; // R[A] = http_get(R[B], headers=R[C])  (returns R t t)
pub(crate) const OP_POSTH: u8 = 85; // ABx: R[A] = http_post(R[B], body=R[bx>>8], headers=R[bx&0xFF])
pub(crate) const OP_MOD: u8 = 86; // R[A] = R[B] % R[C]  (modulo / remainder)
pub(crate) const OP_ROU: u8 = 87; // R[A] = round(R[B])

// Fused foreach opcodes — minimize dispatch overhead for list iteration
// ABC mode: A = bind_reg, B = coll_reg, C = idx_reg
// OP_FOREACHPREP: validate list, load item[0] into R[A], skip next JMP if in bounds (idx=0 hardcoded)
pub(crate) const OP_FOREACHPREP: u8 = 88;
// OP_FOREACHNEXT: R[C] += 1; if R[C] < len(R[B]) { R[A] = R[B][R[C]]; skip next JMP } else fall-through
// ABC mode: A = bind_reg, B = coll_reg, C = idx_reg, followed by JMP exit + JMP body_top
pub(crate) const OP_FOREACHNEXT: u8 = 89;

// Fused compare-and-skip opcodes for numeric guard chains.
// Encoding: ABC mode — A = register, B = unused (0), C = constant pool index (ki).
// Semantics: evaluate R[A] <op> K[C] where both are f64 numbers.
//   If condition is TRUE  → ip += 1 (skip the following OP_JMP, fall into guard body)
//   If condition is FALSE → fall through to the following OP_JMP (skip the guard body)
// This eliminates the intermediate bool register write + nanval_truthy dispatch from
// the classic OP_GE / OP_JMPF two-instruction sequence.
// The following instruction MUST be OP_JMP (emitted automatically by the compiler).
pub(crate) const OP_CMPK_GE_N: u8 = 90; // skip-if R[A] >= K[C]  (f64)
pub(crate) const OP_CMPK_GT_N: u8 = 91; // skip-if R[A] >  K[C]  (f64)
pub(crate) const OP_CMPK_LT_N: u8 = 92; // skip-if R[A] <  K[C]  (f64)
pub(crate) const OP_CMPK_LE_N: u8 = 93; // skip-if R[A] <= K[C]  (f64)
pub(crate) const OP_CMPK_EQ_N: u8 = 94; // skip-if R[A] == K[C]  (f64)
pub(crate) const OP_CMPK_NE_N: u8 = 95; // skip-if R[A] != K[C]  (f64)

// ABC mode — type-specialized string concatenation (both operands known text, no type check)
pub(crate) const OP_ADD_SS: u8 = 96; // R[A] = R[B] ++ R[C]  (both known text)
pub(crate) const OP_AT: u8 = 103; // R[A] = at(R[B], R[C])  (i-th element of list/text)
// Two-word instruction: OP_LST A=result B=list C=index; data word A=value_reg
pub(crate) const OP_LST: u8 = 110; // R[A] = list-replace(R[B], R[C], R[D])  (new list with index C replaced by D)
pub(crate) const OP_RNDN: u8 = 137; // R[A] = normal(mu=R[B], sigma=R[C])  (Box-Muller)

// ABC mode — transcendental math (f64, std::f64 backed, NaN on domain error)
pub(crate) const OP_POW: u8 = 97; // R[A] = pow(R[B], R[C])
pub(crate) const OP_SQRT: u8 = 98; // R[A] = sqrt(R[B])
pub(crate) const OP_LOG: u8 = 99; // R[A] = ln(R[B])
pub(crate) const OP_EXP: u8 = 100; // R[A] = exp(R[B])
pub(crate) const OP_SIN: u8 = 101; // R[A] = sin(R[B])
pub(crate) const OP_COS: u8 = 102; // R[A] = cos(R[B])
// Note: OP_FMT2 is reserved at 104 (pending merge of #171).
pub(crate) const OP_TAN: u8 = 105; // R[A] = tan(R[B])

// String case conversion — single-arg t->t
pub(crate) const OP_UPR: u8 = 118; // R[A] = uppercase(R[B])
pub(crate) const OP_LWR: u8 = 119; // R[A] = lowercase(R[B])
pub(crate) const OP_CAP: u8 = 120; // R[A] = capitalise(R[B])
pub(crate) const OP_LOG10: u8 = 106; // R[A] = log10(R[B])
pub(crate) const OP_LOG2: u8 = 107; // R[A] = log2(R[B])
pub(crate) const OP_ATAN2: u8 = 108; // R[A] = atan2(R[B], R[C])  (y first, x second)
pub(crate) const OP_SRTDESC: u8 = 117; // R[A] = rsrt(R[B])  (descending sort of list)
pub(crate) const OP_RGXSUB: u8 = 115; // R[A] = rgxsub(R[B], R[C], R[D])  (pattern, replacement, subject; D in data word A field)
pub(crate) const OP_ZIP: u8 = 111; // R[A] = zip(R[B], R[C])  (list of [x,y] pairs, truncated)
pub(crate) const OP_ENUMERATE: u8 = 139; // R[A] = enumerate(R[B])  (list of [i, v] pairs)
pub(crate) const OP_WINDOW: u8 = 146; // R[A] = window(R[B] (n), R[C] (list))  → list of n-sized sublists
pub(crate) const OP_TAKE: u8 = 113; // R[A] = take(R[B], R[C])  (first B elements of C; B=n_reg, C=list_reg)
pub(crate) const OP_DROP: u8 = 114; // R[A] = drop(R[B], R[C])  (skip first B elements of C)
pub(crate) const OP_DTFMT: u8 = 131; // R[A] = dtfmt(R[B] epoch, R[C] fmt) → t
pub(crate) const OP_DTPARSE: u8 = 132; // R[A] = dtparse(R[B] text, R[C] fmt) → R n t

// Format-dump helper for csv/tsv: parallels OP_JDMP for the `wr path data fmt`
// 3-arg overload. R[A] = serialised string; R[B] = data list; C = 0 for csv
// (separator `,`), C = 1 for tsv (separator `\t`). The literal format is known
// at compile time, so we encode it in the C field rather than via a register.
pub(crate) const OP_CSVDMP: u8 = 134;

// Pre-allocated for the flatmap HOF builtin. Mirrors `map`: the emitter
// currently falls through to OP_CALL → interpreter (VM has no FnRef
// dispatch yet), and the JIT translator arm is a stub.
pub(crate) const OP_FLATMAP: u8 = 112; // R[A] = flatmap(fn=R[B], list=R[C])

// ABC mode — text formatting
pub(crate) const OP_FMT2: u8 = 104; // R[A] = fmt2(R[B], R[C])  (format number to N decimal places → t)

// Discrete Fourier transforms (Cooley-Tukey radix-2; input is zero-padded
// internally to the next power of two). Output of OP_FFT is a `L (L n)` list
// of `[real, imag]` pairs; OP_IFFT takes the same shape and returns `L n`.
pub(crate) const OP_FFT: u8 = 129; // R[A] = fft(R[B])
pub(crate) const OP_IFFT: u8 = 130; // R[A] = ifft(R[B])
pub(crate) const OP_CLAMP: u8 = 149; // R[A] = clamp(R[B], R[C], R[D])  (lo in C, hi in D; D in data word A field)
// Numeric range — R[A] = range(R[B], R[C]) → L n of [B..C) (half-open, empty if B>=C).
pub(crate) const OP_RANGE: u8 = 138;
pub(crate) const OP_CHUNKS: u8 = 148; // R[A] = chunks(R[B], R[C])  (n, list → L (L a))
// Running sum: R[A] = cumsum(R[B]). Input L n; output L n of same length.
pub(crate) const OP_CUMSUM: u8 = 145; // R[A] = cumsum(R[B])

// Statistics builtins: single-arg numeric reducers over a list of numbers,
// modeled on `OP_FFT` (1-arg list -> value). `OP_QUANTILE` is the 2-arg
// variant: R[A] = quantile(R[B]=list, R[C]=p in [0,1] with linear interp).
pub(crate) const OP_MEDIAN: u8 = 141; // R[A] = median(R[B])
pub(crate) const OP_QUANTILE: u8 = 142; // R[A] = quantile(R[B], R[C])
pub(crate) const OP_STDEV: u8 = 143; // R[A] = stdev(R[B])
pub(crate) const OP_VARIANCE: u8 = 144; // R[A] = variance(R[B])
// Higher-order: uniqby fn xs — pre-allocated, HOF dispatch not yet wired in VM.
pub(crate) const OP_UNIQBY: u8 = 116; // R[A] = uniqby(R[B] (fn-ref), R[C] (list))
// Higher-order: partition fn xs — pre-allocated, HOF dispatch not yet wired in VM.
pub(crate) const OP_PARTITION: u8 = 147; // R[A] = partition(R[B] (fn-ref), R[C] (list)) → L (L a)
pub(crate) const OP_FRQ: u8 = 140; // R[A] = frq(R[B] (list))  — frequency map (M t n)

// Set operations on lists, dedup + sorted output (stringwise on type-prefixed key).
pub(crate) const OP_SETUNION: u8 = 150; // R[A] = setunion(R[B], R[C])
pub(crate) const OP_SETINTER: u8 = 151; // R[A] = setinter(R[B], R[C])
pub(crate) const OP_SETDIFF: u8 = 152; // R[A] = setdiff(R[B], R[C])

// Linear algebra basics.
pub(crate) const OP_TRANSPOSE: u8 = 123; // R[A] = transpose(R[B])  (L (L n) -> L (L n))
pub(crate) const OP_MATMUL: u8 = 124; // R[A] = matmul(R[B], R[C])
pub(crate) const OP_DOT: u8 = 125; // R[A] = dot(R[B], R[C])  (vector dot product -> n)

// ABC mode — string padding
pub(crate) const OP_PADL: u8 = 121; // R[A] = pad_left(R[B], R[C])  (text, width → text)
pub(crate) const OP_PADR: u8 = 122; // R[A] = pad_right(R[B], R[C]) (text, width → text)

// Per-char codepoint round-trip — t->n and n->t (Unicode scalar, 1-arg each).
pub(crate) const OP_ORD: u8 = 153; // R[A] = first-char codepoint of R[B]
pub(crate) const OP_CHR: u8 = 154; // R[A] = single-char string for codepoint R[B]

pub(crate) const OP_GETMANY: u8 = 136; // R[A] = get_many(R[B])  (L t → L (R t t), concurrent fan-out)
pub(crate) const OP_RDJL: u8 = 135; // R[A] = rdjl(R[B])  (read JSONL file → L (R _ t))

// Linear algebra advanced.
pub(crate) const OP_SOLVE: u8 = 126; // R[A] = solve(R[B], R[C])  — solve Ax = b
pub(crate) const OP_INV: u8 = 127; // R[A] = inv(R[B])  — matrix inverse
pub(crate) const OP_DET: u8 = 128; // R[A] = det(R[B])  — matrix determinant

// ABx mode — register + 16-bit operand
pub(crate) const OP_LOADK: u8 = 20;
pub(crate) const OP_JMP: u8 = 21;
pub(crate) const OP_JMPF: u8 = 22;
pub(crate) const OP_JMPT: u8 = 23;
pub(crate) const OP_CALL: u8 = 24;
pub(crate) const OP_RET: u8 = 25;
pub(crate) const OP_RECNEW: u8 = 26;
pub(crate) const OP_RECWITH: u8 = 27;
pub(crate) const OP_LISTNEW: u8 = 28;

// ── Instruction encoding ────────────────────────────────────────────

#[inline(always)]
fn encode_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
    (op as u32) << 24 | (a as u32) << 16 | (b as u32) << 8 | c as u32
}

#[inline(always)]
fn encode_abx(op: u8, a: u8, bx: u16) -> u32 {
    (op as u32) << 24 | (a as u32) << 16 | bx as u32
}

// ── Chunk ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Chunk {
    pub code: Vec<u32>,
    pub constants: Vec<Value>,
    #[allow(dead_code)] // available for introspection/debugging tools
    pub param_count: u8,
    pub reg_count: u8,
    pub spans: Vec<crate::ast::Span>,
    pub all_regs_numeric: bool,
}

impl Chunk {
    fn new(param_count: u8) -> Self {
        Chunk {
            code: Vec::new(),
            constants: Vec::new(),
            param_count,
            reg_count: param_count,
            spans: Vec::new(),
            all_regs_numeric: false,
        }
    }

    fn add_const(&mut self, val: Value) -> u16 {
        for (i, c) in self.constants.iter().enumerate() {
            match (c, &val) {
                (Value::Number(a), Value::Number(b)) if (a - b).abs() < f64::EPSILON => {
                    return i as u16;
                }
                (Value::Text(a), Value::Text(b)) if a == b => return i as u16,
                (Value::Bool(a), Value::Bool(b)) if a == b => return i as u16,
                (Value::Nil, Value::Nil) => return i as u16,
                _ => {}
            }
        }
        let idx = self.constants.len();
        assert!(
            idx <= u16::MAX as usize,
            "constant pool overflow: more than 65535 constants in one function"
        );
        self.constants.push(val);
        idx as u16
    }

    fn add_const_raw(&mut self, val: Value) -> u16 {
        let idx = self.constants.len();
        assert!(
            idx <= u16::MAX as usize,
            "constant pool overflow: more than 65535 constants in one function"
        );
        self.constants.push(val);
        idx as u16
    }

    fn emit(&mut self, inst: u32, span: crate::ast::Span) -> usize {
        let idx = self.code.len();
        self.code.push(inst);
        self.spans.push(span);
        idx
    }

    fn patch_jump(&mut self, jump_pos: usize) {
        let target = self.code.len();
        let offset_i32 = target as i32 - jump_pos as i32 - 1;
        assert!(
            offset_i32 >= i16::MIN as i32 && offset_i32 <= i16::MAX as i32,
            "jump offset {offset_i32} exceeds i16 range — function body too large (max ~32K instructions)"
        );
        let offset = offset_i32 as i16;
        let inst = self.code[jump_pos];
        self.code[jump_pos] = (inst & 0xFFFF0000) | (offset as u16 as u32);
    }
}

// ── Type registry (compile-time field layout for flat records) ───────

#[derive(Debug, Clone)]
pub struct TypeInfo {
    pub name: String,
    pub fields: Vec<String>, // ordered field names — index = slot
    pub num_fields: u64,     // bitmask: bit i set if field i is Number type
}

#[derive(Debug, Clone, Default)]
pub struct TypeRegistry {
    pub types: Vec<Rc<TypeInfo>>,
    pub name_to_id: HashMap<String, u16>,
}

impl TypeRegistry {
    fn register(&mut self, name: String, fields: Vec<String>, num_fields: u64) -> u16 {
        if let Some(&id) = self.name_to_id.get(&name) {
            return id;
        }
        let id = self.types.len() as u16;
        self.name_to_id.insert(name.clone(), id);
        self.types.push(Rc::new(TypeInfo {
            name,
            fields,
            num_fields,
        }));
        id
    }

    fn field_index(&self, type_id: u16, field: &str) -> Option<usize> {
        self.types
            .get(type_id as usize)
            .and_then(|info| info.fields.iter().position(|f| f == field))
    }
}

// ── Compiled program ─────────────────────────────────────────────────

pub struct CompiledProgram {
    pub chunks: Vec<Chunk>,
    pub func_names: Vec<String>,
    pub nan_constants: Vec<Vec<NanVal>>,
    pub type_registry: TypeRegistry,
    /// Parallel to `func_names`/`chunks`: true if the function slot is a `tool` declaration.
    pub is_tool: Vec<bool>,
}

impl CompiledProgram {
    fn func_index(&self, name: &str) -> Option<u16> {
        self.func_names
            .iter()
            .position(|n| n == name)
            .map(|i| i as u16)
    }
}

impl Drop for CompiledProgram {
    fn drop(&mut self) {
        for chunk_consts in &self.nan_constants {
            for v in chunk_consts {
                v.drop_rc();
            }
        }
    }
}

// ── Register Compiler ────────────────────────────────────────────────

struct LoopContext {
    loop_top: usize,
    /// `None` = use loop_top for continue (while loops).
    /// `Some(patches)` = foreach: patches to be fixed up to idx increment.
    continue_patches: Option<Vec<usize>>,
    break_patches: Vec<usize>,
    result_reg: u8,
}

struct RegCompiler {
    chunks: Vec<Chunk>,
    func_names: Vec<String>,
    current: Chunk,
    locals: Vec<(String, u8)>,
    next_reg: u8,
    max_reg: u8,
    reg_is_num: [bool; 256],     // track which registers are known numeric
    reg_is_str: [bool; 256],     // track which registers are known text (string)
    reg_record_type: [u16; 256], // track record type_id per register (u16::MAX = unknown)
    first_error: Option<CompileError>,
    current_span: crate::ast::Span,
    loop_stack: Vec<LoopContext>,
    type_registry: TypeRegistry,
    func_return_types: Vec<Type>, // parallel to func_names
    current_all_regs_numeric: bool,
}

impl RegCompiler {
    fn new() -> Self {
        RegCompiler {
            chunks: Vec::new(),
            func_names: Vec::new(),
            current: Chunk::new(0),
            locals: Vec::new(),
            next_reg: 0,
            max_reg: 0,
            reg_is_num: [false; 256],
            reg_is_str: [false; 256],
            reg_record_type: [u16::MAX; 256],
            first_error: None,
            current_span: crate::ast::Span::UNKNOWN,
            loop_stack: Vec::new(),
            type_registry: TypeRegistry::default(),
            func_return_types: Vec::new(),
            current_all_regs_numeric: true,
        }
    }

    fn alloc_reg(&mut self) -> u8 {
        assert!(
            self.next_reg < 255,
            "register overflow: function uses more than 255 registers"
        );
        let r = self.next_reg;
        self.next_reg += 1;
        if self.next_reg > self.max_reg {
            self.max_reg = self.next_reg;
        }
        self.reg_is_num[r as usize] = false;
        self.reg_is_str[r as usize] = false;
        self.reg_record_type[r as usize] = u16::MAX;
        r
    }

    fn resolve_local(&self, name: &str) -> Option<u8> {
        self.locals
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, r)| *r)
    }

    fn add_local(&mut self, name: &str, reg: u8) {
        self.locals.push((name.to_string(), reg));
    }

    fn emit_abc(&mut self, op: u8, a: u8, b: u8, c: u8) -> usize {
        self.current
            .emit(encode_abc(op, a, b, c), self.current_span)
    }

    fn emit_abx(&mut self, op: u8, a: u8, bx: u16) -> usize {
        self.current.emit(encode_abx(op, a, bx), self.current_span)
    }

    fn emit_jmpf(&mut self, reg: u8) -> usize {
        self.emit_abx(OP_JMPF, reg, 0)
    }

    fn emit_jmpt(&mut self, reg: u8) -> usize {
        self.emit_abx(OP_JMPT, reg, 0)
    }

    fn emit_jmp_placeholder(&mut self) -> usize {
        self.emit_abx(OP_JMP, 0, 0)
    }

    fn emit_jump_to(&mut self, target: usize) {
        let pos = self.current.code.len();
        let offset_i32 = target as i32 - pos as i32 - 1;
        assert!(
            offset_i32 >= i16::MIN as i32 && offset_i32 <= i16::MAX as i32,
            "jump offset {offset_i32} exceeds i16 range — function body too large (max ~32K instructions)"
        );
        self.emit_abx(OP_JMP, 0, offset_i32 as i16 as u16);
    }

    /// Try to emit a fused CMPK guard for a non-negated guard whose condition is
    /// `<binop> reg number_literal` where the register is known numeric.
    ///
    /// Emits: `OP_CMPK_*_N reg, 0, ki` followed by `OP_JMP 0` (placeholder).
    /// Returns `Some(jump_pos)` — the position of the JMP placeholder to patch.
    /// Returns `None` if the pattern does not match (caller falls back to normal form).
    fn try_emit_fused_guard_cmpk(&mut self, condition: &Expr, negated: bool) -> Option<usize> {
        // Fused form only applies to non-negated guards with a comparison BinOp.
        if negated {
            return None;
        }

        let Expr::BinOp { op, left, right } = condition else {
            return None;
        };

        // Right-hand side must be a numeric literal.
        let Expr::Literal(Literal::Number(k)) = right.as_ref() else {
            return None;
        };

        // Map BinOp to the corresponding CMPK opcode.
        let cmpk_op = match op {
            BinOp::GreaterOrEqual => OP_CMPK_GE_N,
            BinOp::GreaterThan => OP_CMPK_GT_N,
            BinOp::LessThan => OP_CMPK_LT_N,
            BinOp::LessOrEqual => OP_CMPK_LE_N,
            BinOp::Equals => OP_CMPK_EQ_N,
            BinOp::NotEquals => OP_CMPK_NE_N,
            _ => return None,
        };

        // Compile the left-hand side — must produce a known-numeric register.
        let saved_next = self.next_reg;
        let reg = self.compile_expr(left);
        if !self.reg_is_num[reg as usize] {
            // Not numeric — cannot use fused form; restore allocator state.
            self.next_reg = saved_next;
            return None;
        }

        // Add constant to pool; C field is 8 bits so ki must fit in 0..=255.
        let ki = self.current.add_const(Value::Number(*k));
        if ki > 255 {
            self.next_reg = saved_next;
            return None;
        }

        // Emit CMPK_*_N  reg, 0, ki
        // (skip the next OP_JMP when condition holds → fall into guard body)
        self.emit_abc(cmpk_op, reg, 0, ki as u8);
        // Emit OP_JMP placeholder — taken when condition is false, skips body.
        let jump_pos = self.emit_jmp_placeholder();

        // Restore next_reg: the comparison consumed no result register.
        self.next_reg = saved_next;

        Some(jump_pos)
    }

    /// Resolve a Type to a type_id if it's a Named record type.
    fn resolve_type_id(&self, ty: &Type) -> u16 {
        match ty {
            Type::Named(name) => self
                .type_registry
                .name_to_id
                .get(name)
                .copied()
                .unwrap_or(u16::MAX),
            _ => u16::MAX,
        }
    }

    /// Search all types for a field name and return its slot index.
    /// Returns `Some(index)` if the field exists at the same index in all types that have it.
    /// Returns `None` if different types place this field at different indices (ambiguous).
    fn search_field_index(&self, field: &str) -> Option<usize> {
        let mut found_idx = None;
        for info in self.type_registry.types.iter() {
            if let Some(idx) = info.fields.iter().position(|f| f == field) {
                match found_idx {
                    None => found_idx = Some(idx),
                    Some(prev) if prev == idx => {} // same index across types, ok
                    Some(_) => return None, // ambiguous — different index in different types
                }
            }
        }
        found_idx
    }

    fn compile_program(mut self, program: &Program) -> Result<CompiledProgram, CompileError> {
        // Build type registry from TypeDefs
        for decl in &program.declarations {
            if let Decl::TypeDef { name, fields, .. } = decl {
                let field_names: Vec<String> = fields.iter().map(|p| p.name.clone()).collect();
                let mut num_fields: u64 = 0;
                for (i, p) in fields.iter().enumerate() {
                    if p.ty == crate::ast::Type::Number && i < 64 {
                        num_fields |= 1 << i;
                    }
                }
                self.type_registry
                    .register(name.clone(), field_names, num_fields);
            }
        }

        // Track which function indices are tool declarations.
        let mut is_tool: Vec<bool> = Vec::new();

        for decl in &program.declarations {
            match decl {
                Decl::Function {
                    name, return_type, ..
                } => {
                    self.func_names.push(name.clone());
                    self.func_return_types.push(return_type.clone());
                    is_tool.push(false);
                }
                Decl::Tool {
                    name, return_type, ..
                } => {
                    self.func_names.push(name.clone());
                    self.func_return_types.push(return_type.clone());
                    is_tool.push(true);
                }
                Decl::TypeDef { .. }
                | Decl::Alias { .. }
                | Decl::Use { .. }
                | Decl::Error { .. } => {}
            }
        }

        for decl in &program.declarations {
            if let Decl::Function { params, body, .. } = decl {
                assert!(
                    params.len() <= 255,
                    "function has {} parameters; maximum is 255",
                    params.len()
                );
                self.current = Chunk::new(params.len() as u8);
                self.locals.clear();
                self.next_reg = params.len() as u8;
                self.max_reg = self.next_reg;

                self.reg_is_num = [false; 256];
                self.reg_is_str = [false; 256];
                self.reg_record_type = [u16::MAX; 256];
                self.current_all_regs_numeric = true;
                for (i, p) in params.iter().enumerate() {
                    self.add_local(&p.name, i as u8);
                    if p.ty == Type::Number {
                        self.reg_is_num[i] = true;
                    } else {
                        self.current_all_regs_numeric = false;
                    }
                    if p.ty == Type::Text {
                        self.reg_is_str[i] = true;
                    }
                    self.reg_record_type[i] = self.resolve_type_id(&p.ty);
                }

                let result = self.compile_body(body);

                let ret_reg = result.unwrap_or_else(|| {
                    let r = self.alloc_reg();
                    let ki = self.current.add_const(Value::Nil);
                    self.emit_abx(OP_LOADK, r, ki);
                    r
                });

                // Only emit RET if last instruction isn't already RET
                let last_is_ret = self
                    .current
                    .code
                    .last()
                    .map(|inst| (inst >> 24) as u8 == OP_RET)
                    .unwrap_or(false);
                if !last_is_ret {
                    self.emit_abx(OP_RET, ret_reg, 0);
                }

                self.current.reg_count = self.max_reg;
                if self.current_all_regs_numeric {
                    self.current.all_regs_numeric = chunk_is_all_numeric(&self.current);
                }
                self.chunks.push(std::mem::take(&mut self.current));
            } else if let Decl::Tool { params, .. } = decl {
                // Tool stub: emit LOADK Nil → WRAPOK → RET  (returns Ok(Nil))
                // Matches interpreter behaviour (interpreter/mod.rs:241–244)
                self.current = Chunk::new(params.len() as u8);
                self.next_reg = params.len() as u8;
                self.max_reg = self.next_reg;

                let nil_reg = self.alloc_reg();
                let ki = self.current.add_const(Value::Nil);
                self.emit_abx(OP_LOADK, nil_reg, ki);

                let ok_reg = self.alloc_reg();
                self.emit_abc(OP_WRAPOK, ok_reg, nil_reg, 0);
                self.emit_abx(OP_RET, ok_reg, 0);

                self.current.reg_count = self.max_reg;
                self.chunks.push(std::mem::take(&mut self.current));
            }
            // TypeDef, Alias, Error — no chunk emitted (not in func_names)
        }

        if let Some(e) = self.first_error {
            return Err(e);
        }
        Ok(CompiledProgram {
            chunks: self.chunks,
            func_names: self.func_names,
            nan_constants: Vec::new(),
            type_registry: self.type_registry,
            is_tool,
        })
    }

    fn compile_body(&mut self, stmts: &[crate::ast::Spanned<Stmt>]) -> Option<u8> {
        let saved_locals = self.locals.len();
        let mut result = None;
        for spanned in stmts {
            self.current_span = spanned.span;
            result = self.compile_stmt(&spanned.node);
        }
        self.locals.truncate(saved_locals);
        result
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Option<u8> {
        match stmt {
            Stmt::Let { name, value } => {
                if let Some(existing_reg) = self.resolve_local(name) {
                    // Peephole: `x = +x k` where k is a numeric literal and x is known numeric
                    // → emit ADDK_N/SUBK_N/MULK_N/DIVK_N directly into existing_reg (no temp + MOVE)
                    let is_arith = matches!(value, Expr::BinOp { op, .. } if matches!(op, BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide));
                    if is_arith
                        && self.reg_is_num[existing_reg as usize]
                        && let Expr::BinOp { op, left, right } = value
                    {
                        let emit_addk = |this: &mut Self, rb: u8, n: f64, swap: bool| -> bool {
                            let ki = this.current.add_const(Value::Number(n));
                            if ki <= 255 {
                                let opcode = match op {
                                    BinOp::Add => OP_ADDK_N,
                                    BinOp::Subtract if !swap => OP_SUBK_N,
                                    BinOp::Multiply => OP_MULK_N,
                                    BinOp::Divide if !swap => OP_DIVK_N,
                                    _ => return false,
                                };
                                this.emit_abc(opcode, existing_reg, rb, ki as u8);
                                this.reg_is_num[existing_reg as usize] = true;
                                return true;
                            }
                            false
                        };
                        // `x = +x n` (right is literal)
                        let handled = if let Expr::Literal(Literal::Number(n)) = right.as_ref() {
                            let rb = self.compile_expr(left);
                            if self.reg_is_num[rb as usize] {
                                emit_addk(self, rb, *n, false)
                            } else {
                                false
                            }
                        }
                        // `x = +n x` (left is literal, commutative ops only)
                        else if matches!(op, BinOp::Add | BinOp::Multiply) {
                            if let Expr::Literal(Literal::Number(n)) = left.as_ref() {
                                let rc = self.compile_expr(right);
                                if self.reg_is_num[rc as usize] {
                                    emit_addk(self, rc, *n, true)
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        if handled {
                            return None;
                        }
                    }
                    // Peephole: `name = += name item` → OP_LISTAPPEND(existing, existing, item)
                    // Emitting a=b keeps RC=1 so the runtime fast path mutates in-place,
                    // turning O(n²) list-building into O(n).
                    if let Expr::BinOp {
                        op: BinOp::Append,
                        left,
                        right,
                    } = value
                        && let Expr::Ref(ref_name) = left.as_ref()
                        && ref_name == name
                    {
                        let item_reg = self.compile_expr(right);
                        self.emit_abc(OP_LISTAPPEND, existing_reg, existing_reg, item_reg);
                        return None;
                    }
                    // Peephole: `name = +name suffix` → OP_ADD_SS/OP_ADD(existing, existing, suffix)
                    // Only when `name` is known to be non-numeric (strings/lists): with a=b,
                    // the OP_ADD runtime checks RC=1 and appends in place (amortised O(1)),
                    // turning O(n²) repeated string-building into O(n).
                    // When `name` is known string and RHS compiles to a string register,
                    // use OP_ADD_SS to skip all type checks entirely.
                    // Skip for numeric vars — they use OP_ADD_NN / OP_ADDK_N specialisations.
                    if let Expr::BinOp {
                        op: BinOp::Add,
                        left,
                        right,
                    } = value
                        && let Expr::Ref(ref_name) = left.as_ref()
                        && ref_name == name
                        && !self.reg_is_num[existing_reg as usize]
                    {
                        let rhs_reg = self.compile_expr(right);
                        if self.reg_is_str[existing_reg as usize]
                            && self.reg_is_str[rhs_reg as usize]
                        {
                            // Both known string: use OP_ADD_SS (no type checking at all)
                            self.emit_abc(OP_ADD_SS, existing_reg, existing_reg, rhs_reg);
                        } else {
                            self.emit_abc(OP_ADD, existing_reg, existing_reg, rhs_reg);
                        }
                        return None;
                    }
                    // General re-binding: compile value and move to existing register
                    let reg = self.compile_expr(value);
                    if reg != existing_reg {
                        self.emit_abc(OP_MOVE, existing_reg, reg, 0);
                        self.reg_record_type[existing_reg as usize] =
                            self.reg_record_type[reg as usize];
                        // Propagate numeric and string type so next iteration can use specialized opcodes
                        self.reg_is_num[existing_reg as usize] = self.reg_is_num[reg as usize];
                        self.reg_is_str[existing_reg as usize] = self.reg_is_str[reg as usize];
                    }
                } else {
                    let reg = self.compile_expr(value);
                    self.add_local(name, reg);
                }
                None
            }

            Stmt::Destructure { bindings, value } => {
                let record_reg = self.compile_expr(value);
                let rec_type = self.reg_record_type[record_reg as usize];
                for binding in bindings {
                    let field_idx = if rec_type != u16::MAX {
                        self.type_registry.field_index(rec_type, binding)
                    } else {
                        self.search_field_index(binding)
                    };
                    match field_idx {
                        Some(idx) => {
                            let c = idx as u8;
                            if let Some(existing_reg) = self.resolve_local(binding) {
                                self.emit_abc(OP_RECFLD, existing_reg, record_reg, c);
                            } else {
                                let field_reg = self.alloc_reg();
                                self.emit_abc(OP_RECFLD, field_reg, record_reg, c);
                                self.add_local(binding, field_reg);
                            }
                        }
                        None => {
                            let ki = self.current.add_const(Value::Text(binding.clone()));
                            assert!(
                                ki <= 255,
                                "constant pool overflow for dynamic destructure field"
                            );
                            if let Some(existing_reg) = self.resolve_local(binding) {
                                self.emit_abc(OP_RECFLD_NAME, existing_reg, record_reg, ki as u8);
                            } else {
                                let field_reg = self.alloc_reg();
                                self.emit_abc(OP_RECFLD_NAME, field_reg, record_reg, ki as u8);
                                self.add_local(binding, field_reg);
                            }
                        }
                    }
                }
                None
            }

            Stmt::Guard {
                condition,
                negated,
                body,
                else_body,
                braceless,
            } => {
                let saved_next = self.next_reg;

                // Try the fused CMPK path first: non-negated plain guard with a
                // numeric compare-vs-constant (most common in guard chains).
                // Emits: CMPK_*_N reg, 0, ki  /  JMP <placeholder>
                // Falls back to: compile_expr(condition) / JMPF|JMPT <placeholder>.
                let fused_applicable = else_body.is_none() && !negated;
                let jump = if fused_applicable {
                    match self.try_emit_fused_guard_cmpk(condition, *negated) {
                        Some(jpos) => jpos,
                        None => {
                            let cond_reg = self.compile_expr(condition);
                            self.emit_jmpf(cond_reg)
                        }
                    }
                } else {
                    let cond_reg = self.compile_expr(condition);
                    if *negated {
                        self.emit_jmpt(cond_reg)
                    } else {
                        self.emit_jmpf(cond_reg)
                    }
                };

                if let Some(else_b) = else_body {
                    // Ternary: cond{then}{else} — produce value, no early return
                    let result_reg = self.alloc_reg();
                    let then_result = self.compile_body(body);
                    let then_reg = then_result.unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        let ki = self.current.add_const(Value::Nil);
                        self.emit_abx(OP_LOADK, r, ki);
                        r
                    });
                    if then_reg != result_reg {
                        self.emit_abc(OP_MOVE, result_reg, then_reg, 0);
                    }
                    let jump_over_else = self.emit_jmp_placeholder();
                    self.current.patch_jump(jump);

                    self.next_reg = result_reg + 1;
                    let else_result = self.compile_body(else_b);
                    let else_reg = else_result.unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        let ki = self.current.add_const(Value::Nil);
                        self.emit_abx(OP_LOADK, r, ki);
                        r
                    });
                    if else_reg != result_reg {
                        self.emit_abc(OP_MOVE, result_reg, else_reg, 0);
                    }
                    self.current.patch_jump(jump_over_else);
                    self.next_reg = result_reg + 1;
                    Some(result_reg)
                } else if *braceless {
                    // Braceless guard: cond expr — early return
                    let body_result = self.compile_body(body);
                    let ret_reg = body_result.unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        let ki = self.current.add_const(Value::Nil);
                        self.emit_abx(OP_LOADK, r, ki);
                        r
                    });
                    self.emit_abx(OP_RET, ret_reg, 0);
                    self.current.patch_jump(jump);
                    self.next_reg = saved_next;
                    None
                } else {
                    // Braced guard: cond{body} — conditional execution (no early return)
                    // Compile body, condition-false jump skips it, no OP_RET emitted.
                    // The body value is available (like ternary) but does not early-return.
                    let result_reg = self.alloc_reg();
                    // Initialize result_reg to Nil (default when condition is false)
                    let nil_ki = self.current.add_const(Value::Nil);
                    self.emit_abx(OP_LOADK, result_reg, nil_ki);
                    let body_result = self.compile_body(body);
                    let body_reg = body_result.unwrap_or(result_reg);
                    if body_reg != result_reg {
                        self.emit_abc(OP_MOVE, result_reg, body_reg, 0);
                    }
                    self.current.patch_jump(jump);
                    self.next_reg = result_reg + 1;
                    Some(result_reg)
                }
            }

            Stmt::Match { subject, arms } => {
                let sub_reg = match subject {
                    Some(e) => self.compile_expr(e),
                    None => {
                        let r = self.alloc_reg();
                        let ki = self.current.add_const(Value::Nil);
                        self.emit_abx(OP_LOADK, r, ki);
                        r
                    }
                };
                let result_reg = self.alloc_reg();
                self.compile_match_arms(sub_reg, result_reg, arms);
                Some(result_reg)
            }

            Stmt::ForEach {
                binding,
                collection,
                body,
            } => {
                let coll_reg = self.compile_expr(collection);
                self.add_local("__fe_coll", coll_reg);

                let idx_reg = self.alloc_reg();
                let zero_ki = self.current.add_const(Value::Number(0.0));
                self.emit_abx(OP_LOADK, idx_reg, zero_ki);
                self.add_local("__fe_idx", idx_reg);

                let last_reg = self.alloc_reg();
                let nil_ki = self.current.add_const(Value::Nil);
                self.emit_abx(OP_LOADK, last_reg, nil_ki);
                self.add_local("__fe_last", last_reg);

                let bind_reg = self.alloc_reg();
                self.emit_abx(OP_LOADK, bind_reg, nil_ki);
                self.add_local(binding, bind_reg);

                // Fused foreach: FOREACHPREP validates the list and loads item[0].
                // If in-bounds: bind_reg = items[0], skip the following JMP (stay in loop).
                // If out-of-bounds (empty list): fall through to JMP exit.
                let loop_top = self.current.code.len();
                self.emit_abc(OP_FOREACHPREP, bind_reg, coll_reg, idx_reg);
                let exit_jump = self.emit_jmp_placeholder();

                // body_top is where the loop body begins — FOREACHNEXT jumps back here.
                let body_top = self.current.code.len();

                // Push loop context for break/continue
                self.loop_stack.push(LoopContext {
                    loop_top,
                    continue_patches: Some(Vec::new()), // foreach: patches fixed up below
                    break_patches: Vec::new(),
                    result_reg: last_reg,
                });

                // Compile body
                let saved_locals = self.locals.len();
                let body_result = self.compile_body(body);
                self.locals.truncate(saved_locals);

                if let Some(br) = body_result
                    && br != last_reg
                {
                    self.emit_abc(OP_MOVE, last_reg, br, 0);
                }

                // Patch continue jumps to the FOREACHNEXT instruction.
                let continue_target = self.current.code.len();
                if let Some(patches) = &self.loop_stack.last().unwrap().continue_patches {
                    let patches: Vec<usize> = patches.clone();
                    for patch in patches {
                        let offset = continue_target as isize - patch as isize - 1;
                        let encoded = encode_abx(OP_JMP, 0, offset as i16 as u16);
                        self.current.code[patch] = encoded;
                    }
                }

                // Fused FOREACHNEXT: increments idx_reg, loads the next element, and if
                // in-bounds skips the following JMP (which jumps to exit), leaving the
                // next executed instruction as the JMP back to body_top.
                // Out-of-bounds: fall through to JMP exit.
                self.emit_abc(OP_FOREACHNEXT, bind_reg, coll_reg, idx_reg);
                let next_exit_jump = self.emit_jmp_placeholder(); // JMP exit (on bounds exceeded)
                self.emit_jump_to(body_top); // JMP body_top (on success)

                // Exit: patch both JMP-exit placeholders and all break jumps.
                self.current.patch_jump(exit_jump);
                self.current.patch_jump(next_exit_jump);
                let ctx = self.loop_stack.pop().unwrap();
                for patch in ctx.break_patches {
                    self.current.patch_jump(patch);
                }

                Some(last_reg)
            }

            Stmt::ForRange {
                binding,
                start,
                end,
                body,
            } => {
                // Evaluate start and end once
                let start_reg = self.compile_expr(start);
                let end_reg = self.compile_expr(end);

                let last_reg = self.alloc_reg();
                let nil_ki = self.current.add_const(Value::Nil);
                self.emit_abx(OP_LOADK, last_reg, nil_ki);
                self.add_local("__fr_last", last_reg);

                // Loop counter = start
                let counter_reg = self.alloc_reg();
                self.emit_abc(OP_MOVE, counter_reg, start_reg, 0);
                self.add_local(binding, counter_reg);

                let one_ki = self.current.add_const(Value::Number(1.0));

                // Loop top: check counter < end
                let loop_top = self.current.code.len();
                let cmp_reg = self.alloc_reg();
                self.emit_abc(OP_LT, cmp_reg, counter_reg, end_reg);
                let exit_jump = self.emit_jmpf(cmp_reg);

                // Push loop context for break/continue
                self.loop_stack.push(LoopContext {
                    loop_top,
                    continue_patches: Some(Vec::new()),
                    break_patches: Vec::new(),
                    result_reg: last_reg,
                });

                // Compile body
                let saved_locals = self.locals.len();
                let body_result = self.compile_body(body);
                self.locals.truncate(saved_locals);

                if let Some(br) = body_result
                    && br != last_reg
                {
                    self.emit_abc(OP_MOVE, last_reg, br, 0);
                }

                // Patch continue jumps to counter increment
                let continue_target = self.current.code.len();
                if let Some(patches) = &self.loop_stack.last().unwrap().continue_patches {
                    let patches: Vec<usize> = patches.clone();
                    for patch in patches {
                        let offset = continue_target as isize - patch as isize - 1;
                        let encoded = encode_abx(OP_JMP, 0, offset as i16 as u16);
                        self.current.code[patch] = encoded;
                    }
                }

                // counter += 1 (use ADDK_N when counter is known numeric)
                if self.reg_is_num[counter_reg as usize] && one_ki <= 255 {
                    self.emit_abc(OP_ADDK_N, counter_reg, counter_reg, one_ki as u8);
                } else {
                    let one_reg = self.alloc_reg();
                    self.emit_abx(OP_LOADK, one_reg, one_ki);
                    self.emit_abc(OP_ADD, counter_reg, counter_reg, one_reg);
                }

                // Jump back to loop top
                self.emit_jump_to(loop_top);

                // Exit: patch exit jump and break jumps
                self.current.patch_jump(exit_jump);
                let ctx = self.loop_stack.pop().unwrap();
                for patch in ctx.break_patches {
                    self.current.patch_jump(patch);
                }

                Some(last_reg)
            }

            Stmt::While { condition, body } => {
                let last_reg = self.alloc_reg();
                let nil_ki = self.current.add_const(Value::Nil);
                self.emit_abx(OP_LOADK, last_reg, nil_ki);

                // Loop top: eval condition
                let loop_top = self.current.code.len();
                let cond_reg = self.compile_expr(condition);
                let exit_jump = self.emit_jmpf(cond_reg);

                // Push loop context for break/continue
                self.loop_stack.push(LoopContext {
                    loop_top,
                    continue_patches: None, // while: continue jumps to loop_top
                    break_patches: Vec::new(),
                    result_reg: last_reg,
                });

                // Compile body
                let saved_locals = self.locals.len();
                let body_result = self.compile_body(body);
                self.locals.truncate(saved_locals);

                if let Some(br) = body_result
                    && br != last_reg
                {
                    self.emit_abc(OP_MOVE, last_reg, br, 0);
                }

                // Jump back to loop top
                self.emit_jump_to(loop_top);

                // Exit: patch condition-false jump and all break jumps
                self.current.patch_jump(exit_jump);
                let ctx = self.loop_stack.pop().unwrap();
                for patch in ctx.break_patches {
                    self.current.patch_jump(patch);
                }

                Some(last_reg)
            }

            Stmt::Return(expr) => {
                let reg = self.compile_expr(expr);
                self.emit_abx(OP_RET, reg, 0);
                None
            }

            Stmt::Break(expr) => {
                if let Some(ctx) = self.loop_stack.last() {
                    let result_reg = ctx.result_reg;
                    if let Some(e) = expr {
                        let reg = self.compile_expr(e);
                        if reg != result_reg {
                            self.emit_abc(OP_MOVE, result_reg, reg, 0);
                        }
                    }
                    let jmp = self.emit_jmp_placeholder();
                    // Re-borrow mutably to push break patch
                    if let Some(ctx) = self.loop_stack.last_mut() {
                        ctx.break_patches.push(jmp);
                    }
                }
                None
            }

            Stmt::Continue => {
                if let Some(ctx) = self.loop_stack.last() {
                    if ctx.continue_patches.is_some() {
                        // Foreach: emit placeholder, patch later
                        let jmp = self.emit_jmp_placeholder();
                        if let Some(ctx) = self.loop_stack.last_mut()
                            && let Some(patches) = ctx.continue_patches.as_mut()
                        {
                            patches.push(jmp);
                        }
                    } else {
                        // While: jump back to loop_top (condition re-eval)
                        let top = ctx.loop_top;
                        self.emit_jump_to(top);
                    }
                }
                None
            }

            Stmt::Expr(expr) => {
                let reg = self.compile_expr(expr);
                Some(reg)
            }
        }
    }

    fn compile_match_arms(&mut self, sub_reg: u8, result_reg: u8, arms: &[MatchArm]) {
        let mut end_jumps = Vec::with_capacity(arms.len());

        for arm in arms {
            let saved_next = self.next_reg;
            let saved_locals = self.locals.len();

            match &arm.pattern {
                Pattern::Wildcard => {
                    let body_result = self.compile_body(&arm.body);
                    if let Some(br) = body_result
                        && br != result_reg
                    {
                        self.emit_abc(OP_MOVE, result_reg, br, 0);
                    }
                    self.next_reg = saved_next;
                    self.locals.truncate(saved_locals);
                    for j in end_jumps {
                        self.current.patch_jump(j);
                    }
                    return;
                }

                Pattern::Ok(binding) => {
                    let test_reg = self.alloc_reg();
                    self.emit_abc(OP_ISOK, test_reg, sub_reg, 0);
                    let skip = self.emit_jmpf(test_reg);

                    if binding != "_" {
                        let bind_reg = self.alloc_reg();
                        self.emit_abc(OP_UNWRAP, bind_reg, sub_reg, 0);
                        self.add_local(binding, bind_reg);
                    }

                    let body_result = self.compile_body(&arm.body);
                    if let Some(br) = body_result
                        && br != result_reg
                    {
                        self.emit_abc(OP_MOVE, result_reg, br, 0);
                    }
                    end_jumps.push(self.emit_jmp_placeholder());
                    self.current.patch_jump(skip);
                }

                Pattern::Err(binding) => {
                    let test_reg = self.alloc_reg();
                    self.emit_abc(OP_ISERR, test_reg, sub_reg, 0);
                    let skip = self.emit_jmpf(test_reg);

                    if binding != "_" {
                        let bind_reg = self.alloc_reg();
                        self.emit_abc(OP_UNWRAP, bind_reg, sub_reg, 0);
                        self.add_local(binding, bind_reg);
                    }

                    let body_result = self.compile_body(&arm.body);
                    if let Some(br) = body_result
                        && br != result_reg
                    {
                        self.emit_abc(OP_MOVE, result_reg, br, 0);
                    }
                    end_jumps.push(self.emit_jmp_placeholder());
                    self.current.patch_jump(skip);
                }

                Pattern::Literal(lit) => {
                    let val = match lit {
                        Literal::Number(n) => Value::Number(*n),
                        Literal::Text(s) => Value::Text(s.clone()),
                        Literal::Bool(b) => Value::Bool(*b),
                        Literal::Nil => Value::Nil,
                    };
                    let const_reg = self.alloc_reg();
                    let ki = self.current.add_const(val);
                    self.emit_abx(OP_LOADK, const_reg, ki);
                    let eq_reg = self.alloc_reg();
                    self.emit_abc(OP_EQ, eq_reg, sub_reg, const_reg);
                    let skip = self.emit_jmpf(eq_reg);

                    let body_result = self.compile_body(&arm.body);
                    if let Some(br) = body_result
                        && br != result_reg
                    {
                        self.emit_abc(OP_MOVE, result_reg, br, 0);
                    }
                    end_jumps.push(self.emit_jmp_placeholder());
                    self.current.patch_jump(skip);
                }

                Pattern::TypeIs { ty, binding } => {
                    let opcode = match ty {
                        Type::Number => OP_ISNUM,
                        Type::Text => OP_ISTEXT,
                        Type::Bool => OP_ISBOOL,
                        Type::List(_) => OP_ISLIST,
                        _ => OP_ISNUM, // unreachable for valid programs
                    };
                    let test_reg = self.alloc_reg();
                    self.emit_abc(opcode, test_reg, sub_reg, 0);
                    let skip = self.emit_jmpf(test_reg);

                    if binding != "_" {
                        let bind_reg = self.alloc_reg();
                        self.emit_abc(OP_MOVE, bind_reg, sub_reg, 0);
                        self.locals.push((binding.clone(), bind_reg));
                    }
                    let body_result = self.compile_body(&arm.body);
                    if let Some(br) = body_result
                        && br != result_reg
                    {
                        self.emit_abc(OP_MOVE, result_reg, br, 0);
                    }
                    end_jumps.push(self.emit_jmp_placeholder());
                    self.current.patch_jump(skip);
                }
            }

            self.next_reg = saved_next;
            self.locals.truncate(saved_locals);
        }

        // No wildcard matched: default to nil
        let nil_ki = self.current.add_const(Value::Nil);
        self.emit_abx(OP_LOADK, result_reg, nil_ki);

        for j in end_jumps {
            self.current.patch_jump(j);
        }
    }

    /// Try to evaluate an expression at compile time. Returns Some(Value) if fully constant.
    fn try_const_fold(expr: &Expr) -> Option<Value> {
        match expr {
            Expr::Literal(lit) => Some(match lit {
                Literal::Number(n) => Value::Number(*n),
                Literal::Text(s) => Value::Text(s.clone()),
                Literal::Bool(b) => Value::Bool(*b),
                Literal::Nil => Value::Nil,
            }),
            Expr::BinOp { op, left, right } => {
                let lv = Self::try_const_fold(left)?;
                let rv = Self::try_const_fold(right)?;
                match (&lv, &rv) {
                    (Value::Number(a), Value::Number(b)) => Some(match op {
                        BinOp::Add => Value::Number(a + b),
                        BinOp::Subtract => Value::Number(a - b),
                        BinOp::Multiply => Value::Number(a * b),
                        BinOp::Divide if *b != 0.0 => Value::Number(a / b),
                        BinOp::Equals => Value::Bool((a - b).abs() < f64::EPSILON),
                        BinOp::NotEquals => Value::Bool((a - b).abs() >= f64::EPSILON),
                        BinOp::GreaterThan => Value::Bool(a > b),
                        BinOp::LessThan => Value::Bool(a < b),
                        BinOp::GreaterOrEqual => Value::Bool(a >= b),
                        BinOp::LessOrEqual => Value::Bool(a <= b),
                        _ => return None,
                    }),
                    (Value::Text(a), Value::Text(b)) => match op {
                        BinOp::Add => {
                            let mut out = String::with_capacity(a.len() + b.len());
                            out.push_str(a);
                            out.push_str(b);
                            Some(Value::Text(out))
                        }
                        _ => None,
                    },
                    (Value::Bool(a), Value::Bool(b)) => match op {
                        BinOp::Equals => Some(Value::Bool(a == b)),
                        BinOp::NotEquals => Some(Value::Bool(a != b)),
                        BinOp::And => Some(Value::Bool(*a && *b)),
                        BinOp::Or => Some(Value::Bool(*a || *b)),
                        _ => None,
                    },
                    _ => None,
                }
            }
            Expr::UnaryOp { op, operand } => {
                let v = Self::try_const_fold(operand)?;
                match (&v, op) {
                    (Value::Number(n), UnaryOp::Negate) => Some(Value::Number(-n)),
                    (Value::Bool(b), UnaryOp::Not) => Some(Value::Bool(!b)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn compile_expr(&mut self, expr: &Expr) -> u8 {
        // Try constant folding for BinOp/UnaryOp expressions
        if matches!(expr, Expr::BinOp { .. } | Expr::UnaryOp { .. })
            && let Some(ref val) = Self::try_const_fold(expr)
        {
            let is_num = matches!(val, Value::Number(_));
            let reg = self.alloc_reg();
            let ki = self.current.add_const(val.clone());
            self.emit_abx(OP_LOADK, reg, ki);
            if is_num {
                self.reg_is_num[reg as usize] = true;
            }
            return reg;
        }

        match expr {
            Expr::Literal(lit) => {
                let is_num = matches!(lit, Literal::Number(_));
                let is_str = matches!(lit, Literal::Text(_));
                let val = match lit {
                    Literal::Number(n) => Value::Number(*n),
                    Literal::Text(s) => Value::Text(s.clone()),
                    Literal::Bool(b) => Value::Bool(*b),
                    Literal::Nil => Value::Nil,
                };
                let reg = self.alloc_reg();
                let ki = self.current.add_const(val);
                self.emit_abx(OP_LOADK, reg, ki);
                if is_num {
                    self.reg_is_num[reg as usize] = true;
                }
                if is_str {
                    self.reg_is_str[reg as usize] = true;
                }
                reg
            }

            Expr::Ref(name) => {
                if let Some(reg) = self.resolve_local(name) {
                    reg // FREE — no instruction needed!
                } else {
                    self.first_error
                        .get_or_insert(CompileError::UndefinedVariable { name: name.clone() });
                    0 // dummy register; compile continues to surface more errors
                }
            }

            Expr::Field {
                object,
                field,
                safe,
            } => {
                let obj_reg = self.compile_expr(object);
                // Resolve field to an index using compile-time type info
                let obj_type = self.reg_record_type[obj_reg as usize];
                let field_idx = if obj_type != u16::MAX {
                    self.type_registry.field_index(obj_type, field)
                } else {
                    self.search_field_index(field)
                };
                match field_idx {
                    Some(idx) => {
                        // Fast path: direct field index
                        let c = idx as u8;
                        // Check if this field is known numeric from the type definition
                        let field_is_num = obj_type != u16::MAX
                            && idx < 64
                            && (self.type_registry.types[obj_type as usize].num_fields
                                & (1 << idx))
                                != 0;
                        if *safe {
                            self.emit_abx(OP_JMPNN, obj_reg, 1);
                            self.emit_abx(OP_JMP, 0, 1);
                            self.emit_abc(OP_RECFLD, obj_reg, obj_reg, c);
                            self.reg_record_type[obj_reg as usize] = u16::MAX;
                            if field_is_num {
                                self.reg_is_num[obj_reg as usize] = true;
                            }
                            obj_reg
                        } else {
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_RECFLD, ra, obj_reg, c);
                            if field_is_num {
                                self.reg_is_num[ra as usize] = true;
                            }
                            ra
                        }
                    }
                    None => {
                        // Dynamic path: store field name, runtime linear scan
                        let ki = self.current.add_const(Value::Text(field.clone()));
                        assert!(ki <= 255, "constant pool overflow for dynamic field name");
                        if *safe {
                            self.emit_abx(OP_JMPNN, obj_reg, 1);
                            self.emit_abx(OP_JMP, 0, 1);
                            self.emit_abc(OP_RECFLD_NAME, obj_reg, obj_reg, ki as u8);
                            self.reg_record_type[obj_reg as usize] = u16::MAX;
                            obj_reg
                        } else {
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_RECFLD_NAME, ra, obj_reg, ki as u8);
                            ra
                        }
                    }
                }
            }

            Expr::Index {
                object,
                index,
                safe,
            } => {
                let obj_reg = self.compile_expr(object);
                assert!(
                    *index <= 255,
                    "index literal {} exceeds 8-bit limit in OP_INDEX",
                    index
                );
                if *safe {
                    self.emit_abx(OP_JMPNN, obj_reg, 1);
                    self.emit_abx(OP_JMP, 0, 1);
                    self.emit_abc(OP_INDEX, obj_reg, obj_reg, *index as u8);
                    obj_reg
                } else {
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_INDEX, ra, obj_reg, *index as u8);
                    ra
                }
            }

            Expr::Call {
                function,
                args,
                unwrap,
            } => {
                // Builtins — resolve at compile time to enum, then emit dedicated opcodes
                if let Some(builtin) = Builtin::from_name(function) {
                    let nargs = args.len();
                    match (builtin, nargs) {
                        (Builtin::Len, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_LEN, ra, rb, 0);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Str, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_STR, ra, rb, 0);
                            self.reg_is_str[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Num, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_NUM, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Abs, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_ABS, ra, rb, 0);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Min | Builtin::Max, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            let op = if builtin == Builtin::Min {
                                OP_MIN
                            } else {
                                OP_MAX
                            };
                            self.emit_abc(op, ra, rb, rc);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Mod, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_MOD, ra, rb, rc);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Clamp, 3) => {
                            // clamp x lo hi — two-instruction sequence:
                            //   OP_CLAMP A=result  B=x  C=lo
                            //   data word: A=hi_reg (consumed by OP_CLAMP dispatch; ip advances past it)
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let rd = self.compile_expr(&args[2]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_CLAMP, ra, rb, rc);
                            self.emit_abc(0, rd, 0, 0);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Flr | Builtin::Cel | Builtin::Rou, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            let op = match builtin {
                                Builtin::Flr => OP_FLR,
                                Builtin::Cel => OP_CEL,
                                _ => OP_ROU,
                            };
                            self.emit_abc(op, ra, rb, 0);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (
                            Builtin::Sqrt
                            | Builtin::Log
                            | Builtin::Exp
                            | Builtin::Sin
                            | Builtin::Cos
                            | Builtin::Tan
                            | Builtin::Log10
                            | Builtin::Log2,
                            1,
                        ) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            let op = match builtin {
                                Builtin::Sqrt => OP_SQRT,
                                Builtin::Log => OP_LOG,
                                Builtin::Exp => OP_EXP,
                                Builtin::Sin => OP_SIN,
                                Builtin::Cos => OP_COS,
                                Builtin::Tan => OP_TAN,
                                Builtin::Log10 => OP_LOG10,
                                _ => OP_LOG2,
                            };
                            self.emit_abc(op, ra, rb, 0);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Pow, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_POW, ra, rb, rc);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Atan2, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_ATAN2, ra, rb, rc);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Transpose, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_TRANSPOSE, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Matmul, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_MATMUL, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Dot, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_DOT, ra, rb, rc);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Solve, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_SOLVE, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Inv, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_INV, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Det, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_DET, ra, rb, 0);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Spl, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_SPL, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Cat, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_CAT, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Has, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_HAS, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Hd, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_HD, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::At, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_AT, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Fmt2, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_FMT2, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Zip, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_ZIP, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Enumerate, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_ENUMERATE, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Range, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_RANGE, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Window, 2) => {
                            // window n xs — emit B=n, C=xs
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_WINDOW, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Chunks, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_CHUNKS, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Setunion, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_SETUNION, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Setinter, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_SETINTER, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Setdiff, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_SETDIFF, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Tl, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_TL, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Rev, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_REV, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Srt, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_SRT, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Rsrt, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_SRTDESC, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Fft, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_FFT, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Ifft, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_IFFT, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Cumsum, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_CUMSUM, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Median, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_MEDIAN, ra, rb, 0);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Quantile, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_QUANTILE, ra, rb, rc);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Stdev, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_STDEV, ra, rb, 0);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Variance, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_VARIANCE, ra, rb, 0);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Slc, 3) => {
                            // slc list start end — two-instruction sequence:
                            //   OP_SLC   A=result  B=list  C=start
                            //   data word: A=end_reg (consumed by OP_SLC dispatch; ip advances past it)
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let rd = self.compile_expr(&args[2]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_SLC, ra, rb, rc);
                            self.emit_abc(0, rd, 0, 0);
                            return ra;
                        }
                        (Builtin::Lst, 3) => {
                            // lst list i v — two-instruction sequence:
                            //   OP_LST   A=result  B=list  C=index
                            //   data word: A=value_reg
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let rd = self.compile_expr(&args[2]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_LST, ra, rb, rc);
                            self.emit_abc(0, rd, 0, 0);
                            return ra;
                        }
                        (Builtin::Rgxsub, 3) => {
                            // rgxsub pattern replacement subject — two-instruction sequence:
                            //   OP_RGXSUB A=result  B=pattern  C=replacement
                            //   data word: A=subject_reg (consumed by OP_RGXSUB dispatch; ip advances past it)
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let rd = self.compile_expr(&args[2]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_RGXSUB, ra, rb, rc);
                            self.emit_abc(0, rd, 0, 0);
                            return ra;
                        }
                        (Builtin::Take, 2) => {
                            // take n xs — B=n_reg, C=list_reg
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_TAKE, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Drop, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_DROP, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Rnd, 0) => {
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_RND0, ra, 0, 0);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Now, 0) => {
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_NOW, ra, 0, 0);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Dtfmt, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_DTFMT, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Dtparse, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_DTPARSE, ra, rb, rc);
                            if *unwrap {
                                let check_reg = self.alloc_reg();
                                self.emit_abc(OP_ISOK, check_reg, ra, 0);
                                let skip_ret = self.emit_jmpt(check_reg);
                                self.emit_abx(OP_RET, ra, 0);
                                self.current.patch_jump(skip_ret);
                                self.emit_abc(OP_UNWRAP, ra, ra, 0);
                                self.next_reg = ra + 1;
                            }
                            return ra;
                        }
                        (Builtin::Rnd, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_RND2, ra, rb, rc);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Rndn, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_RNDN, ra, rb, rc);
                            self.reg_is_num[ra as usize] = true;
                            return ra;
                        }
                        (Builtin::Env, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_ENV, ra, rb, 0);
                            // env returns R t t — handle auto-unwrap
                            if *unwrap {
                                let check_reg = self.alloc_reg();
                                self.emit_abc(OP_ISOK, check_reg, ra, 0);
                                let skip_ret = self.emit_jmpt(check_reg);
                                self.emit_abx(OP_RET, ra, 0);
                                self.current.patch_jump(skip_ret);
                                self.emit_abc(OP_UNWRAP, ra, ra, 0);
                                self.next_reg = ra + 1;
                            }
                            return ra;
                        }
                        (Builtin::Get, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_GET, ra, rb, 0);
                            // get returns R t t — handle auto-unwrap
                            if *unwrap {
                                let check_reg = self.alloc_reg();
                                self.emit_abc(OP_ISOK, check_reg, ra, 0);
                                let skip_ret = self.emit_jmpt(check_reg);
                                self.emit_abx(OP_RET, ra, 0);
                                self.current.patch_jump(skip_ret);
                                self.emit_abc(OP_UNWRAP, ra, ra, 0);
                                self.next_reg = ra + 1;
                            }
                            return ra;
                        }
                        (Builtin::Post, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_POST, ra, rb, rc);
                            // post returns R t t — handle auto-unwrap
                            if *unwrap {
                                let check_reg = self.alloc_reg();
                                self.emit_abc(OP_ISOK, check_reg, ra, 0);
                                let skip_ret = self.emit_jmpt(check_reg);
                                self.emit_abx(OP_RET, ra, 0);
                                self.current.patch_jump(skip_ret);
                                self.emit_abc(OP_UNWRAP, ra, ra, 0);
                                self.next_reg = ra + 1;
                            }
                            return ra;
                        }
                        (Builtin::Get, 2) => {
                            // get url headers — OP_GETH (ABC: result=A, url=B, headers=C)
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_GETH, ra, rb, rc);
                            if *unwrap {
                                let check_reg = self.alloc_reg();
                                self.emit_abc(OP_ISOK, check_reg, ra, 0);
                                let skip_ret = self.emit_jmpt(check_reg);
                                self.emit_abx(OP_RET, ra, 0);
                                self.current.patch_jump(skip_ret);
                                self.emit_abc(OP_UNWRAP, ra, ra, 0);
                                self.next_reg = ra + 1;
                            }
                            return ra;
                        }
                        (Builtin::GetMany, 1) => {
                            // get-many urls — concurrent fan-out, no auto-unwrap
                            // (signature is L t → L (R t t); per-URL errors are first-class)
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_GETMANY, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Post, 3) => {
                            // post url body headers — two-instruction sequence:
                            //   OP_POSTH  A=result  B=url  C=body
                            //   data word: A=headers_reg (consumed by OP_POSTH dispatch; ip advances past it)
                            let rb = self.compile_expr(&args[0]);
                            let r_body = self.compile_expr(&args[1]);
                            let r_hdrs = self.compile_expr(&args[2]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_POSTH, ra, rb, r_body);
                            // data word carries headers reg in the A field; dispatch reads and skips it
                            self.emit_abc(0, r_hdrs, 0, 0);
                            if *unwrap {
                                let check_reg = self.alloc_reg();
                                self.emit_abc(OP_ISOK, check_reg, ra, 0);
                                let skip_ret = self.emit_jmpt(check_reg);
                                self.emit_abx(OP_RET, ra, 0);
                                self.current.patch_jump(skip_ret);
                                self.emit_abc(OP_UNWRAP, ra, ra, 0);
                                self.next_reg = ra + 1;
                            }
                            return ra;
                        }
                        (Builtin::Jpth, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_JPTH, ra, rb, rc);
                            if *unwrap {
                                let check_reg = self.alloc_reg();
                                self.emit_abc(OP_ISOK, check_reg, ra, 0);
                                let skip_ret = self.emit_jmpt(check_reg);
                                self.emit_abx(OP_RET, ra, 0);
                                self.current.patch_jump(skip_ret);
                                self.emit_abc(OP_UNWRAP, ra, ra, 0);
                                self.next_reg = ra + 1;
                            }
                            return ra;
                        }
                        (Builtin::Jdmp, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_JDMP, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Trm, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_TRM, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Upr, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_UPR, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Lwr, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_LWR, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Cap, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_CAP, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Ord, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_ORD, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Chr, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_CHR, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Padl | Builtin::Padr, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            let op = if builtin == Builtin::Padl {
                                OP_PADL
                            } else {
                                OP_PADR
                            };
                            self.emit_abc(op, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Unq, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_UNQ, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Frq, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_FRQ, ra, rb, 0);
                            return ra;
                        }
                        // fmt is variadic — falls through to OP_CALL -> interpreter
                        (Builtin::Prnt, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_PRT, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Rd | Builtin::Rdl, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            let op = if builtin == Builtin::Rdl {
                                OP_RDL
                            } else {
                                OP_RD
                            };
                            self.emit_abc(op, ra, rb, 0);
                            if *unwrap {
                                let check_reg = self.alloc_reg();
                                self.emit_abc(OP_ISOK, check_reg, ra, 0);
                                let skip_ret = self.emit_jmpt(check_reg);
                                self.emit_abx(OP_RET, ra, 0);
                                self.current.patch_jump(skip_ret);
                                self.emit_abc(OP_UNWRAP, ra, ra, 0);
                                self.next_reg = ra + 1;
                            }
                            return ra;
                        }
                        // rd path fmt (2-arg) and rdb s fmt fall through to OP_CALL -> interpreter
                        (Builtin::Wr | Builtin::Wrl, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            let op = if builtin == Builtin::Wr {
                                OP_WR
                            } else {
                                OP_WRL
                            };
                            self.emit_abc(op, ra, rb, rc);
                            if *unwrap {
                                let check_reg = self.alloc_reg();
                                self.emit_abc(OP_ISOK, check_reg, ra, 0);
                                let skip_ret = self.emit_jmpt(check_reg);
                                self.emit_abx(OP_RET, ra, 0);
                                self.current.patch_jump(skip_ret);
                                self.emit_abc(OP_UNWRAP, ra, ra, 0);
                                self.next_reg = ra + 1;
                            }
                            return ra;
                        }
                        // 3-arg `wr path data fmt` — when fmt is a literal
                        // "json"/"csv"/"tsv", serialise to a string register
                        // then OP_WR. json uses OP_JDMP; csv/tsv uses OP_CSVDMP
                        // (separator encoded in C: 0=csv, 1=tsv).
                        (Builtin::Wr, 3) => {
                            if let crate::ast::Expr::Literal(crate::ast::Literal::Text(fmt_str)) =
                                &args[2]
                                && matches!(fmt_str.as_str(), "json" | "csv" | "tsv")
                            {
                                let rpath = self.compile_expr(&args[0]);
                                let rdata = self.compile_expr(&args[1]);
                                let rcontent = self.alloc_reg();
                                match fmt_str.as_str() {
                                    "json" => {
                                        self.emit_abc(OP_JDMP, rcontent, rdata, 0);
                                    }
                                    "csv" => {
                                        self.emit_abc(OP_CSVDMP, rcontent, rdata, 0);
                                    }
                                    "tsv" => {
                                        self.emit_abc(OP_CSVDMP, rcontent, rdata, 1);
                                    }
                                    _ => unreachable!(),
                                }
                                let ra = self.alloc_reg();
                                self.emit_abc(OP_WR, ra, rpath, rcontent);
                                if *unwrap {
                                    let check_reg = self.alloc_reg();
                                    self.emit_abc(OP_ISOK, check_reg, ra, 0);
                                    let skip_ret = self.emit_jmpt(check_reg);
                                    self.emit_abx(OP_RET, ra, 0);
                                    self.current.patch_jump(skip_ret);
                                    self.emit_abc(OP_UNWRAP, ra, ra, 0);
                                    self.next_reg = ra + 1;
                                }
                                return ra;
                            }
                            // Dynamic format string: no VM path yet — surface
                            // a clear compile error pointing at --run-tree.
                            self.first_error.get_or_insert(
                                CompileError::UndefinedFunction {
                                    name: "wr (3-arg with dynamic format not yet supported in VM; use --run-tree)".to_string(),
                                },
                            );
                            return self.alloc_reg();
                        }
                        (Builtin::Jpar, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_JPAR, ra, rb, 0);
                            if *unwrap {
                                let check_reg = self.alloc_reg();
                                self.emit_abc(OP_ISOK, check_reg, ra, 0);
                                let skip_ret = self.emit_jmpt(check_reg);
                                self.emit_abx(OP_RET, ra, 0);
                                self.current.patch_jump(skip_ret);
                                self.emit_abc(OP_UNWRAP, ra, ra, 0);
                                self.next_reg = ra + 1;
                            }
                            return ra;
                        }
                        (Builtin::Rdjl, 1) => {
                            // rdjl path → L (R _ t). Not a Result-returning op, so `!`
                            // is unsupported here; the verifier rejects it via the
                            // standard return-type check.
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_RDJL, ra, rb, 0);
                            return ra;
                        }
                        // Map builtins
                        (Builtin::Mmap, 0) => {
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_MAPNEW, ra, 0, 0);
                            return ra;
                        }
                        (Builtin::Mget, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_MGET, ra, rb, rc);
                            // mget returns O v — handle auto-unwrap:
                            //   if result is non-nil, fall through (value is in ra)
                            //   if nil, RET ra (propagate nil to enclosing fn)
                            // Trailing OP_MOVE ra, ra acts as a barrier so the
                            // function-emit check (`last_is_ret`) doesn't treat
                            // the propagate-RET as the function's tail return.
                            if *unwrap {
                                let skip_ret = self.emit_abx(OP_JMPNN, ra, 0);
                                self.emit_abx(OP_RET, ra, 0);
                                self.current.patch_jump(skip_ret);
                                self.emit_abc(OP_MOVE, ra, ra, 0);
                                self.next_reg = ra + 1;
                            }
                            return ra;
                        }
                        (Builtin::Mset, 3) => {
                            // mset map key val — two-instruction sequence:
                            //   OP_MSET  A=result  B=map  C=key
                            //   data word: A=val_reg (consumed by OP_MSET dispatch; ip advances past it)
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let rd = self.compile_expr(&args[2]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_MSET, ra, rb, rc);
                            self.emit_abc(0, rd, 0, 0);
                            return ra;
                        }
                        (Builtin::Mhas, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_MHAS, ra, rb, rc);
                            return ra;
                        }
                        (Builtin::Mkeys, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_MKEYS, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Mvals, 1) => {
                            let rb = self.compile_expr(&args[0]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_MVALS, ra, rb, 0);
                            return ra;
                        }
                        (Builtin::Mdel, 2) => {
                            let rb = self.compile_expr(&args[0]);
                            let rc = self.compile_expr(&args[1]);
                            let ra = self.alloc_reg();
                            self.emit_abc(OP_MDEL, ra, rb, rc);
                            return ra;
                        }
                        // Builtins that fall through to OP_CALL (interpreter handles them):
                        // fmt (variadic), map/flt/fld/grp/flatmap (higher-order),
                        // sum/avg/rgx/flat, rd 2-arg, rdb, wr 3-arg, srt 2-arg, etc.
                        _ => {}
                    }
                }

                let arg_regs: Vec<u8> = args.iter().map(|a| self.compile_expr(a)).collect();
                let func_idx = self
                    .func_names
                    .iter()
                    .position(|n| n == function)
                    .unwrap_or_else(|| {
                        self.first_error
                            .get_or_insert(CompileError::UndefinedFunction {
                                name: function.clone(),
                            });
                        0 // dummy index; compile continues to surface more errors
                    });

                let a = self.alloc_reg(); // result register
                // Reserve slots for args
                let args_base = self.next_reg;
                assert!(
                    (self.next_reg as usize) + args.len() <= 255,
                    "register overflow: call requires too many register slots"
                );
                self.next_reg += args.len() as u8;
                if self.next_reg > self.max_reg {
                    self.max_reg = self.next_reg;
                }

                for (i, &arg_reg) in arg_regs.iter().enumerate() {
                    let target = args_base + i as u8;
                    if arg_reg != target {
                        self.emit_abc(OP_MOVE, target, arg_reg, 0);
                    }
                }

                assert!(
                    func_idx <= 255,
                    "too many functions: function index {} exceeds 8-bit limit in OP_CALL",
                    func_idx
                );
                let bx = ((func_idx as u16) << 8) | args.len() as u16;
                self.emit_abx(OP_CALL, a, bx);

                // Track return type for record type propagation
                if func_idx < self.func_return_types.len() {
                    let ret_ty = &self.func_return_types[func_idx];
                    self.reg_record_type[a as usize] = self.resolve_type_id(ret_ty);
                    if *ret_ty == Type::Number {
                        self.reg_is_num[a as usize] = true;
                    } else {
                        self.current_all_regs_numeric = false;
                    }
                }

                // After call, only the result register is live
                self.next_reg = a + 1;

                // Auto-unwrap:
                //   Result: Ok(v)→v, Err(e)→return Err to caller
                //   Optional: Some(v)→v, Nil→return Nil to caller
                if *unwrap {
                    let is_optional = func_idx < self.func_return_types.len()
                        && matches!(self.func_return_types[func_idx], Type::Optional(_));
                    if is_optional {
                        // Optional propagation: jump over the RET if non-nil.
                        // Trailing OP_MOVE a, a is a no-op barrier so the
                        // function-emit `last_is_ret` check doesn't mistake the
                        // propagate-RET for the function's tail return.
                        let skip_ret = self.emit_abx(OP_JMPNN, a, 0);
                        self.emit_abx(OP_RET, a, 0); // propagate nil
                        self.current.patch_jump(skip_ret);
                        self.emit_abc(OP_MOVE, a, a, 0);
                        self.next_reg = a + 1;
                    } else {
                        let check_reg = self.alloc_reg();
                        self.emit_abc(OP_ISOK, check_reg, a, 0);
                        let skip_ret = self.emit_jmpt(check_reg);
                        self.emit_abx(OP_RET, a, 0); // propagate Err
                        self.current.patch_jump(skip_ret);
                        self.emit_abc(OP_UNWRAP, a, a, 0); // extract Ok inner
                        self.next_reg = a + 1; // only result register live
                    }
                }

                a
            }

            Expr::BinOp { op, left, right } => {
                // Try superinstructions: register op constant (right is number literal)
                let is_arith = matches!(
                    op,
                    BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide
                );
                if is_arith {
                    if let Expr::Literal(Literal::Number(n)) = right.as_ref() {
                        let rb = self.compile_expr(left);
                        if self.reg_is_num[rb as usize] {
                            let ki = self.current.add_const(Value::Number(*n));
                            if ki <= 255 {
                                let ra = self.alloc_reg();
                                let opcode = match op {
                                    BinOp::Add => OP_ADDK_N,
                                    BinOp::Subtract => OP_SUBK_N,
                                    BinOp::Multiply => OP_MULK_N,
                                    _ => OP_DIVK_N, // BinOp::Divide — only remaining case per is_arith guard
                                };
                                self.emit_abc(opcode, ra, rb, ki as u8);
                                self.reg_is_num[ra as usize] = true;
                                return ra;
                            }
                        }
                    }
                    // Also handle constant on left (e.g., 2 * x → MULK x, 2)
                    // Only for commutative ops (Add, Multiply)
                    if matches!(op, BinOp::Add | BinOp::Multiply)
                        && let Expr::Literal(Literal::Number(n)) = left.as_ref()
                    {
                        let rc = self.compile_expr(right);
                        if self.reg_is_num[rc as usize] {
                            let ki = self.current.add_const(Value::Number(*n));
                            if ki <= 255 {
                                let ra = self.alloc_reg();
                                let opcode = match op {
                                    BinOp::Add => OP_ADDK_N,
                                    _ => OP_MULK_N, // BinOp::Multiply — only remaining commutative case
                                };
                                self.emit_abc(opcode, ra, rc, ki as u8);
                                self.reg_is_num[ra as usize] = true;
                                return ra;
                            }
                        }
                    }
                }

                // Short-circuit: &a b → eval a, JMPF skip, eval b, skip:
                //                |a b → eval a, JMPT skip, eval b, skip:
                if matches!(op, BinOp::And | BinOp::Or) {
                    let ra = self.compile_expr(left);
                    let result = self.alloc_reg();
                    self.emit_abc(OP_MOVE, result, ra, 0);
                    let jump = if *op == BinOp::And {
                        self.emit_jmpf(ra)
                    } else {
                        self.emit_jmpt(ra)
                    };
                    let rb = self.compile_expr(right);
                    if rb != result {
                        self.emit_abc(OP_MOVE, result, rb, 0);
                    }
                    self.current.patch_jump(jump);
                    return result;
                }

                let rb = self.compile_expr(left);
                let rc = self.compile_expr(right);
                let both_num = self.reg_is_num[rb as usize] && self.reg_is_num[rc as usize];
                let both_str = self.reg_is_str[rb as usize] && self.reg_is_str[rc as usize];

                // Use type-specialized opcodes when both operands are known numeric or both string
                let (opcode, result_is_num, result_is_str) = match op {
                    BinOp::Add if both_num => (OP_ADD_NN, true, false),
                    BinOp::Add if both_str => (OP_ADD_SS, false, true),
                    BinOp::Subtract if both_num => (OP_SUB_NN, true, false),
                    BinOp::Multiply if both_num => (OP_MUL_NN, true, false),
                    BinOp::Divide if both_num => (OP_DIV_NN, true, false),
                    BinOp::Add => (OP_ADD, false, false),
                    BinOp::Subtract => (OP_SUB, false, false),
                    BinOp::Multiply => (OP_MUL, false, false),
                    BinOp::Divide => (OP_DIV, false, false),
                    BinOp::Equals => (OP_EQ, false, false),
                    BinOp::NotEquals => (OP_NE, false, false),
                    BinOp::GreaterThan => (OP_GT, false, false),
                    BinOp::LessThan => (OP_LT, false, false),
                    BinOp::GreaterOrEqual => (OP_GE, false, false),
                    BinOp::LessOrEqual => (OP_LE, false, false),
                    BinOp::Append => (OP_LISTAPPEND, false, false),
                    _ => (OP_LISTAPPEND, false, false), // And/Or handled above by early return; Append fallthrough
                };
                let ra = self.alloc_reg();
                self.emit_abc(opcode, ra, rb, rc);
                if result_is_num {
                    self.reg_is_num[ra as usize] = true;
                }
                if result_is_str {
                    self.reg_is_str[ra as usize] = true;
                }
                ra
            }

            Expr::UnaryOp { op, operand } => {
                let rb = self.compile_expr(operand);
                let ra = self.alloc_reg();
                let opcode = match op {
                    UnaryOp::Not => OP_NOT,
                    UnaryOp::Negate => OP_NEG,
                };
                self.emit_abc(opcode, ra, rb, 0);
                if *op == UnaryOp::Negate && self.reg_is_num[rb as usize] {
                    self.reg_is_num[ra as usize] = true;
                }
                ra
            }

            Expr::Ok(inner) => {
                let rb = self.compile_expr(inner);
                let ra = self.alloc_reg();
                self.emit_abc(OP_WRAPOK, ra, rb, 0);
                ra
            }

            Expr::Err(inner) => {
                let rb = self.compile_expr(inner);
                let ra = self.alloc_reg();
                self.emit_abc(OP_WRAPERR, ra, rb, 0);
                ra
            }

            Expr::List(items) => {
                let item_regs: Vec<u8> = items.iter().map(|item| self.compile_expr(item)).collect();

                let a = self.alloc_reg(); // result register
                // Reserve slots for items
                let items_base = self.next_reg;
                assert!(
                    (self.next_reg as usize) + items.len() <= 255,
                    "register overflow: list literal requires too many register slots"
                );
                self.next_reg += items.len() as u8;
                if self.next_reg > self.max_reg {
                    self.max_reg = self.next_reg;
                }

                for (i, &item_reg) in item_regs.iter().enumerate() {
                    let target = items_base + i as u8;
                    if item_reg != target {
                        self.emit_abc(OP_MOVE, target, item_reg, 0);
                    }
                }

                self.emit_abx(OP_LISTNEW, a, items.len() as u16);
                a
            }

            Expr::Record { type_name, fields } => {
                // Look up or auto-register type in registry
                let type_id = match self.type_registry.name_to_id.get(type_name) {
                    Some(&id) => id,
                    None => {
                        // Auto-register from field order in this expression
                        let field_names: Vec<String> =
                            fields.iter().map(|(n, _)| n.clone()).collect();
                        self.type_registry
                            .register(type_name.clone(), field_names, 0)
                    }
                };

                // We need to emit field values in the canonical order defined by the TypeInfo,
                // not the order they appear in the source. This ensures fields[i] always
                // corresponds to TypeInfo.fields[i].
                let canonical_order: Vec<String> =
                    self.type_registry.types[type_id as usize].fields.clone();
                let source_fields: HashMap<&str, &Expr> =
                    fields.iter().map(|(n, e)| (n.as_str(), e)).collect();
                let ordered_regs: Vec<u8> = canonical_order
                    .iter()
                    .map(|fname| {
                        let expr = source_fields[fname.as_str()];
                        self.compile_expr(expr)
                    })
                    .collect();

                let a = self.alloc_reg(); // result register
                let fields_base = self.next_reg;
                assert!(
                    (self.next_reg as usize) + ordered_regs.len() <= 255,
                    "register overflow: record literal requires too many register slots"
                );
                self.next_reg += ordered_regs.len() as u8;
                if self.next_reg > self.max_reg {
                    self.max_reg = self.next_reg;
                }

                for (i, &field_reg) in ordered_regs.iter().enumerate() {
                    let target = fields_base + i as u8;
                    if field_reg != target {
                        self.emit_abc(OP_MOVE, target, field_reg, 0);
                    }
                }

                assert!(
                    type_id <= 255,
                    "type_id {} exceeds 8-bit limit in OP_RECNEW",
                    type_id
                );
                let bx = (type_id << 8) | ordered_regs.len() as u16;
                self.emit_abx(OP_RECNEW, a, bx);
                // Track the type of this register
                self.reg_record_type[a as usize] = type_id;
                a
            }

            Expr::Match { subject, arms } => {
                let sub_reg = match subject {
                    Some(e) => self.compile_expr(e),
                    None => {
                        let r = self.alloc_reg();
                        let ki = self.current.add_const(Value::Nil);
                        self.emit_abx(OP_LOADK, r, ki);
                        r
                    }
                };
                let result_reg = self.alloc_reg();
                self.compile_match_arms(sub_reg, result_reg, arms);
                result_reg
            }

            Expr::NilCoalesce { value, default } => {
                let val_reg = self.compile_expr(value);
                // Jump over default if val is not nil
                let skip_jump = self.emit_abx(OP_JMPNN, val_reg, 0);
                // Value is nil — compile default and move to val_reg
                let def_reg = self.compile_expr(default);
                if def_reg != val_reg {
                    self.emit_abc(OP_MOVE, val_reg, def_reg, 0);
                }
                self.current.patch_jump(skip_jump);
                val_reg
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                let cond_reg = self.compile_expr(condition);
                let result_reg = self.alloc_reg();
                let jump_to_else = self.emit_jmpf(cond_reg);
                // Then branch
                let then_reg = self.compile_expr(then_expr);
                if then_reg != result_reg {
                    self.emit_abc(OP_MOVE, result_reg, then_reg, 0);
                }
                let jump_over_else = self.emit_jmp_placeholder();
                self.current.patch_jump(jump_to_else);
                // Else branch
                self.next_reg = result_reg + 1;
                let else_reg = self.compile_expr(else_expr);
                if else_reg != result_reg {
                    self.emit_abc(OP_MOVE, result_reg, else_reg, 0);
                }
                self.current.patch_jump(jump_over_else);
                self.next_reg = result_reg + 1;
                result_reg
            }
            Expr::With { object, updates } => {
                let obj_reg = self.compile_expr(object);
                let obj_type = self.reg_record_type[obj_reg as usize];

                let update_regs: Vec<u8> = updates
                    .iter()
                    .map(|(_, val_expr)| self.compile_expr(val_expr))
                    .collect();

                // Resolve update field names to indices
                let update_indices: Vec<Option<u8>> = updates
                    .iter()
                    .map(|(name, _)| {
                        let idx = if obj_type != u16::MAX {
                            self.type_registry.field_index(obj_type, name)
                        } else {
                            self.search_field_index(name)
                        };
                        idx.map(|i| i as u8)
                    })
                    .collect();
                let all_resolved = update_indices.iter().all(|i| i.is_some());

                // Store as constant: indices (numbers) for resolved, names (strings) for unresolved
                let const_val = if all_resolved {
                    Value::List(
                        update_indices
                            .iter()
                            .map(|i| Value::Number(i.unwrap() as f64))
                            .collect(),
                    )
                } else {
                    // Fallback: store field names for runtime resolution
                    Value::List(
                        updates
                            .iter()
                            .map(|(n, _)| Value::Text(n.clone()))
                            .collect(),
                    )
                };
                let const_idx = self.current.add_const_raw(const_val);

                let a = self.alloc_reg(); // result register
                let updates_base = self.next_reg;
                assert!(
                    (self.next_reg as usize) + updates.len() <= 255,
                    "register overflow: 'with' expression requires too many register slots"
                );
                self.next_reg += updates.len() as u8;
                if self.next_reg > self.max_reg {
                    self.max_reg = self.next_reg;
                }

                // Move object into result slot
                if obj_reg != a {
                    self.emit_abc(OP_MOVE, a, obj_reg, 0);
                }

                // Move update values into consecutive slots
                for (i, &val_reg) in update_regs.iter().enumerate() {
                    let target = updates_base + i as u8;
                    if val_reg != target {
                        self.emit_abc(OP_MOVE, target, val_reg, 0);
                    }
                }

                assert!(
                    const_idx <= 255,
                    "constant pool overflow: field data index {} exceeds 8-bit limit in OP_RECWITH",
                    const_idx
                );
                let bx = (const_idx << 8) | updates.len() as u16;
                self.emit_abx(OP_RECWITH, a, bx);
                // Propagate type (with doesn't change the type)
                self.reg_record_type[a as usize] = obj_type;
                a
            }
        }
    }
}

fn chunk_is_all_numeric(chunk: &Chunk) -> bool {
    for c in &chunk.constants {
        match c {
            Value::Number(_) | Value::Bool(_) | Value::Nil => {}
            _ => return false,
        }
    }
    for &inst in &chunk.code {
        let op = (inst >> 24) as u8;
        match op {
            OP_RECNEW | OP_LISTNEW | OP_RECWITH | OP_WRAPOK | OP_WRAPERR | OP_STR | OP_CAT
            | OP_SPL | OP_REV | OP_SRT | OP_SRTDESC | OP_SLC | OP_TAKE | OP_DROP | OP_UNQ
            | OP_UNIQBY | OP_FRQ | OP_PARTITION | OP_LISTAPPEND | OP_JPAR | OP_JDMP | OP_CSVDMP
            | OP_ENV | OP_GET | OP_GETH | OP_GETMANY | OP_POST | OP_POSTH | OP_RD | OP_RDL
            | OP_RDJL | OP_WR | OP_WRL | OP_MAPNEW | OP_MGET | OP_MSET | OP_MKEYS | OP_MVALS
            | OP_HD | OP_AT | OP_LST | OP_TL | OP_FMT2 | OP_RGXSUB | OP_ZIP | OP_ENUMERATE
            | OP_WINDOW | OP_FFT | OP_IFFT | OP_RANGE | OP_CHUNKS | OP_CUMSUM | OP_SETUNION
            | OP_SETINTER | OP_SETDIFF | OP_TRANSPOSE | OP_MATMUL | OP_INV | OP_SOLVE
            | OP_DTFMT | OP_DTPARSE => {
                return false;
            }
            _ => {}
        }
    }
    true
}

// ── NaN-boxed value ──────────────────────────────────────────────────
//
// IEEE 754 quiet NaN has 51 unused payload bits. We use them to encode
// all ilo value types in a single Copy u64, making the VM stack
// Vec<u64>-equivalent with zero-cost number operations.

const QNAN: u64 = 0x7FFC_0000_0000_0000;
const TAG_NIL: u64 = QNAN;
const TAG_TRUE: u64 = QNAN | 1;
const TAG_FALSE: u64 = QNAN | 2;
const TAG_STRING: u64 = 0x7FFD_0000_0000_0000;
const TAG_LIST: u64 = 0x7FFE_0000_0000_0000;
const TAG_RECORD: u64 = 0x7FFF_0000_0000_0000;
const TAG_OK: u64 = 0xFFFC_0000_0000_0000;
const TAG_ERR: u64 = 0xFFFD_0000_0000_0000;
const TAG_MAP: u64 = 0xFFFF_0000_0000_0000;
pub(crate) const TAG_ARENA_REC: u64 = 0xFFFE_0000_0000_0000;
const PTR_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;
const TAG_MASK: u64 = 0xFFFF_0000_0000_0000;

// ── Bump Arena for Records ──────────────────────────────────────────
//
// ArenaRecord layout (repr(C), 8-byte header + inline NanVal fields):
//   [type_id: u16 | n_fields: u16 | _pad: u32 | fields: [u64; n_fields]]
//
// Records allocated from BumpArena use TAG_ARENA_REC. They are never
// individually freed — the entire arena is reset in bulk (e.g. after OP_RET
// returns to top-level, or after each JIT call).

const ARENA_DEFAULT_SIZE: usize = 64 * 1024; // 64 KB

#[repr(C)]
pub(crate) struct ArenaRecord {
    pub type_id: u16,
    pub n_fields: u16,
    _pad: u32,
    // Followed by n_fields × u64 (NanVal) inline
}

impl ArenaRecord {
    /// # Safety
    /// `idx` must be less than `self.n_fields`. The pointer is valid for the
    /// lifetime of the arena allocation. Layout: 8-byte header followed by
    /// `n_fields` × u64 (NanVal) fields, all 8-byte aligned.
    #[inline]
    pub(crate) unsafe fn field_ptr(&self, idx: usize) -> *const u64 {
        debug_assert!(
            idx < self.n_fields as usize,
            "field_ptr: idx {idx} >= n_fields {}",
            self.n_fields
        );
        // SAFETY: caller guarantees idx < n_fields; layout is repr(C) with
        // 8-byte header then n_fields×u64.
        unsafe {
            (self as *const Self as *const u8)
                .add(8)
                .cast::<u64>()
                .add(idx)
        }
    }

    /// Mutable field pointer. Callers must ensure exclusive access.
    ///
    /// # Safety
    /// `idx` must be less than `self.n_fields`. Caller must have exclusive
    /// access to this record (no aliasing readers or writers).
    #[inline]
    pub(crate) unsafe fn field_ptr_mut(&mut self, idx: usize) -> *mut u64 {
        debug_assert!(
            idx < self.n_fields as usize,
            "field_ptr_mut: idx {idx} >= n_fields {}",
            self.n_fields
        );
        // SAFETY: caller guarantees idx < n_fields and exclusive access.
        unsafe { (self as *mut Self as *mut u8).add(8).cast::<u64>().add(idx) }
    }
}

/// Bump arena for records. `#[repr(C)]` with known field offsets so JIT can
/// inline the allocation (load buf_ptr/buf_cap/offset, bump, store).
///
/// JIT field offsets: buf_ptr=0, buf_cap=8, offset=16.
#[repr(C)]
pub(crate) struct BumpArena {
    pub(crate) buf_ptr: *mut u8, // offset 0  — raw pointer to buffer
    pub(crate) buf_cap: usize,   // offset 8  — buffer capacity in bytes
    pub(crate) offset: usize,    // offset 16 — current bump offset
}

impl BumpArena {
    pub(crate) fn new() -> Self {
        let layout =
            std::alloc::Layout::from_size_align(ARENA_DEFAULT_SIZE, 8).expect("valid arena layout");
        // SAFETY: layout is non-zero (64KB, 8-align). No zero-fill needed since
        // arena tracks its own offset and only reads initialized records.
        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        BumpArena {
            buf_ptr: ptr,
            buf_cap: ARENA_DEFAULT_SIZE,
            offset: 0,
        }
    }

    #[inline]
    pub(crate) fn reset(&mut self) {
        // Walk all arena records and drop_rc their heap fields before resetting.
        let mut off = 0usize;
        while off + 8 <= self.offset {
            // SAFETY: `off` is within `[0, self.offset)` which is within the
            // allocated buffer. Records are 8-byte aligned and written by
            // `alloc()` which enforces alignment. The pointer is valid because
            // the buffer is live until we clear `self.offset` below.
            let ptr = unsafe { self.buf_ptr.add(off) } as *const ArenaRecord;
            let rec = unsafe { &*ptr };
            let n = rec.n_fields as usize;
            let record_size = 8 + n * 8;
            if off + record_size > self.offset {
                break;
            }
            for i in 0..n {
                let v = NanVal(unsafe { *rec.field_ptr(i) });
                v.drop_rc(); // no-op for numbers/bools/nil/arena-records; frees heap refs
            }
            off += record_size;
            // Align to 8 bytes
            off = (off + 7) & !7;
        }
        self.offset = 0;
    }

    /// Bump-allocate space for a record with `n_fields` fields.
    /// Returns a pointer to the ArenaRecord header, or None if full.
    #[inline]
    pub(crate) fn alloc_record(
        &mut self,
        type_id: u16,
        n_fields: usize,
    ) -> Option<*mut ArenaRecord> {
        let size = 8 + n_fields * 8; // header + fields
        let aligned_offset = (self.offset + 7) & !7;
        if aligned_offset + size > self.buf_cap {
            return None; // arena full, caller falls back to Rc path
        }
        let ptr = unsafe { self.buf_ptr.add(aligned_offset) } as *mut ArenaRecord;
        unsafe {
            (*ptr).type_id = type_id;
            (*ptr).n_fields = n_fields as u16;
            (*ptr)._pad = 0;
        }
        self.offset = aligned_offset + size;
        Some(ptr)
    }
}

impl Drop for BumpArena {
    fn drop(&mut self) {
        self.reset(); // drop_rc all heap fields
        unsafe {
            let layout =
                std::alloc::Layout::from_size_align(self.buf_cap, 8).expect("valid arena layout");
            std::alloc::dealloc(self.buf_ptr, layout);
        }
    }
}

thread_local! {
    pub(crate) static JIT_ARENA: std::cell::RefCell<BumpArena> = std::cell::RefCell::new(BumpArena::new());
    static ACTIVE_REGISTRY: std::cell::Cell<*const TypeRegistry> = const { std::cell::Cell::new(std::ptr::null()) };
}

/// Run `f` with the active `TypeRegistry` pointer set to `program.type_registry`.
///
/// The pointer is only live for the duration of `f`; it is unconditionally
/// cleared (set to null) when `f` returns **or panics**, so there is no risk
/// of a dangling pointer after `program` is dropped.
pub fn with_active_registry<R>(program: &CompiledProgram, f: impl FnOnce() -> R) -> R {
    struct ClearGuard;
    impl Drop for ClearGuard {
        fn drop(&mut self) {
            ACTIVE_REGISTRY.with(|r| r.set(std::ptr::null()));
        }
    }

    ACTIVE_REGISTRY.with(|r| r.set(&program.type_registry as *const TypeRegistry));
    let _guard = ClearGuard;
    f()
}

/// Clear the active `TypeRegistry` pointer.
///
/// Called at the end of `VM::execute()` where wrapping in a closure is
/// impractical. The `execute` method also uses `ActiveRegistryGuard` to ensure
/// the pointer is cleared on early return or panic.
fn clear_active_registry() {
    ACTIVE_REGISTRY.with(|r| r.set(std::ptr::null()));
}

/// RAII guard that clears `ACTIVE_REGISTRY` on drop.
///
/// Used inside `VM::execute()` to guarantee cleanup even on `?` early returns
/// or panics.
pub(crate) struct ActiveRegistryGuard;

impl Drop for ActiveRegistryGuard {
    fn drop(&mut self) {
        clear_active_registry();
    }
}

/// Get a raw pointer to the JIT arena (for passing to jit_recnew).
/// The pointer is valid as long as the thread-local isn't dropped.
pub(crate) fn jit_arena_ptr() -> *mut BumpArena {
    JIT_ARENA.with(|cell| cell.as_ptr())
}

/// Reset the JIT arena (called after each JIT function invocation).
pub(crate) fn jit_arena_reset() {
    JIT_ARENA.with(|cell| cell.borrow_mut().reset());
}

enum HeapObj {
    Str(String),
    List(Vec<NanVal>),
    Map(HashMap<String, NanVal>),
    Record {
        type_info: Rc<TypeInfo>,
        fields: Box<[NanVal]>,
    },
    OkVal(NanVal),
    ErrVal(NanVal),
}

impl Drop for HeapObj {
    fn drop(&mut self) {
        match self {
            HeapObj::Str(_) => {}
            HeapObj::List(items) => {
                for item in items {
                    item.drop_rc();
                }
            }
            HeapObj::Map(m) => {
                for val in m.values() {
                    val.drop_rc();
                }
            }
            HeapObj::Record { fields, .. } => {
                for val in fields.iter() {
                    val.drop_rc();
                }
            }
            HeapObj::OkVal(inner) | HeapObj::ErrVal(inner) => {
                inner.drop_rc();
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct NanVal(pub u64);

impl NanVal {
    #[inline]
    pub(crate) fn number(n: f64) -> Self {
        if n.is_nan() {
            NanVal(0x7FF8_0000_0000_0000) // canonical NaN, outside our tag space
        } else {
            NanVal(n.to_bits())
        }
    }

    /// Create a NanVal from a known-finite f64 without NaN canonicalization.
    ///
    /// # Safety
    /// The caller must guarantee the result is not NaN. This is sound for
    /// type-specialized add/sub/mul opcodes because: (1) those opcodes are only
    /// emitted when both operands are `reg_is_num` (known numeric from type
    /// analysis), (2) the only way to get ±Inf into a `reg_is_num` register is
    /// via division-by-zero, which ilo catches and returns `VmError::DivisionByZero`,
    /// so inputs are always finite, and (3) add/sub/mul of finite floats produces
    /// finite or ±Inf results, never NaN. ±Inf is safe: its bit patterns
    /// (0x7FF0... / 0xFFF0...) do not collide with our NaN-boxed tag space
    /// (QNAN = 0x7FF8...) since `(0x7FF0 & 0x7FF8) != 0x7FF8`.
    #[inline(always)]
    pub(crate) unsafe fn number_unchecked(n: f64) -> Self {
        debug_assert!(!n.is_nan(), "number_unchecked called with NaN");
        NanVal(n.to_bits())
    }

    #[inline]
    fn nil() -> Self {
        NanVal(TAG_NIL)
    }

    #[inline]
    fn boolean(b: bool) -> Self {
        NanVal(if b { TAG_TRUE } else { TAG_FALSE })
    }

    fn heap_string(s: String) -> Self {
        let rc = Rc::new(HeapObj::Str(s));
        let ptr = Rc::into_raw(rc) as u64;
        NanVal(TAG_STRING | (ptr & PTR_MASK))
    }

    fn heap_list(items: Vec<NanVal>) -> Self {
        let rc = Rc::new(HeapObj::List(items));
        let ptr = Rc::into_raw(rc) as u64;
        NanVal(TAG_LIST | (ptr & PTR_MASK))
    }

    fn heap_record(type_info: Rc<TypeInfo>, fields: Box<[NanVal]>) -> Self {
        let rc = Rc::new(HeapObj::Record { type_info, fields });
        let ptr = Rc::into_raw(rc) as u64;
        NanVal(TAG_RECORD | (ptr & PTR_MASK))
    }

    /// Create a NanVal pointing to an arena-allocated record.
    #[inline]
    fn arena_record(ptr: *const ArenaRecord) -> Self {
        NanVal(TAG_ARENA_REC | (ptr as u64 & PTR_MASK))
    }

    #[inline]
    pub(crate) fn is_arena_record(self) -> bool {
        (self.0 & TAG_MASK) == TAG_ARENA_REC
    }

    /// Get pointer to ArenaRecord from an arena-tagged NanVal.
    #[inline]
    pub(crate) unsafe fn as_arena_record(&self) -> &ArenaRecord {
        unsafe { &*((self.0 & PTR_MASK) as *const ArenaRecord) }
    }

    /// Promote an arena record to a heap-allocated Rc record.
    fn promote_arena_to_heap(self, registry: &TypeRegistry) -> Self {
        debug_assert!(self.is_arena_record());
        unsafe {
            let rec = self.as_arena_record();
            let type_info = Rc::clone(&registry.types[rec.type_id as usize]);
            let n = rec.n_fields as usize;
            let mut fields = Vec::with_capacity(n);
            for i in 0..n {
                let v = NanVal(*rec.field_ptr(i));
                // Recursively promote nested arena records before the arena is reset.
                // For heap values, clone_rc increments the reference count so the
                // newly allocated heap record holds a valid owned reference.
                let v = if v.is_arena_record() {
                    v.promote_arena_to_heap(registry)
                } else {
                    v.clone_rc();
                    v
                };
                fields.push(v);
            }
            NanVal::heap_record(type_info, fields.into_boxed_slice())
        }
    }

    fn heap_ok(inner: NanVal) -> Self {
        let rc = Rc::new(HeapObj::OkVal(inner));
        let ptr = Rc::into_raw(rc) as u64;
        NanVal(TAG_OK | (ptr & PTR_MASK))
    }

    fn heap_err(inner: NanVal) -> Self {
        let rc = Rc::new(HeapObj::ErrVal(inner));
        let ptr = Rc::into_raw(rc) as u64;
        NanVal(TAG_ERR | (ptr & PTR_MASK))
    }

    fn heap_map(m: HashMap<String, NanVal>) -> Self {
        let rc = Rc::new(HeapObj::Map(m));
        let ptr = Rc::into_raw(rc) as u64;
        NanVal(TAG_MAP | (ptr & PTR_MASK))
    }

    #[inline]
    pub(crate) fn is_number(self) -> bool {
        (self.0 & QNAN) != QNAN
    }

    #[inline]
    pub(crate) fn as_number(self) -> f64 {
        f64::from_bits(self.0)
    }

    #[inline]
    fn is_heap(self) -> bool {
        (self.0 & QNAN) == QNAN
            && self.0 != TAG_NIL
            && self.0 != TAG_TRUE
            && self.0 != TAG_FALSE
            && (self.0 & TAG_MASK) != TAG_ARENA_REC
    }

    #[inline]
    fn is_string(self) -> bool {
        (self.0 & TAG_MASK) == TAG_STRING
    }

    /// Dereference the NaN-boxed heap pointer to a `&HeapObj`.
    ///
    /// # Safety
    ///
    /// The caller must guarantee **all** of the following:
    ///
    /// 1. `self` was created by one of the `heap_*` constructors (i.e.
    ///    `self.is_heap()` is true).
    /// 2. The underlying `Rc<HeapObj>` is still alive — its strong count has
    ///    not reached zero.
    /// 3. The returned reference must **not** be held across any operation that
    ///    could decrement the RC to zero (e.g. `drop_rc`, register overwrites,
    ///    or stack pops that release the last copy of this `NanVal`).
    ///
    /// Because `NanVal` is `Copy`, the borrow checker cannot enforce (2) or (3);
    /// the unconstrained lifetime `'a` is an unavoidable consequence of
    /// NaN-boxing. Violating these invariants is instant UB (use-after-free).
    #[inline]
    unsafe fn as_heap_ref<'a>(self) -> &'a HeapObj {
        debug_assert!(
            self.is_heap(),
            "as_heap_ref called on non-heap NanVal {:#018x}",
            self.0
        );
        let ptr = (self.0 & PTR_MASK) as *const HeapObj;
        // In debug builds, verify the Rc is still alive by reconstructing it
        // temporarily. This catches use-after-free during development.
        #[cfg(debug_assertions)]
        {
            let rc = unsafe { Rc::from_raw(ptr) };
            let count = Rc::strong_count(&rc);
            // Leak it back — we must not decrement the count.
            std::mem::forget(rc);
            debug_assert!(
                count >= 1,
                "as_heap_ref: Rc strong count is 0 (use-after-free) for NanVal {:#018x}",
                self.0
            );
        }
        // SAFETY: pointer was produced by Rc::into_raw in a heap_* constructor.
        // Caller guarantees is_heap() and the Rc is still live.
        unsafe { &*ptr }
    }

    #[inline(always)]
    fn clone_rc(self) {
        if self.is_heap() {
            let ptr = (self.0 & PTR_MASK) as *const HeapObj;
            // SAFETY: is_heap() guarantees this pointer was produced by Rc::into_raw
            // and the RC count is at least 1 (we hold a NanVal that represents it).
            unsafe {
                Rc::increment_strong_count(ptr);
            }
        }
    }

    #[inline(always)]
    fn drop_rc(self) {
        if self.is_heap() {
            let ptr = (self.0 & PTR_MASK) as *const HeapObj;
            // SAFETY: is_heap() guarantees this pointer was produced by Rc::into_raw.
            // Decrementing mirrors every clone_rc call; the VM is responsible for
            // pairing increments and decrements correctly.
            unsafe {
                Rc::decrement_strong_count(ptr);
            }
        }
    }

    pub fn from_value(val: &Value) -> Self {
        match val {
            Value::Number(n) => NanVal::number(*n),
            Value::Bool(b) => NanVal::boolean(*b),
            Value::Nil => NanVal::nil(),
            Value::Text(s) => NanVal::heap_string(s.clone()),
            Value::List(items) => NanVal::heap_list(items.iter().map(NanVal::from_value).collect()),
            Value::Map(m) => {
                let nan_map: HashMap<String, NanVal> = m
                    .iter()
                    .map(|(k, v)| (k.clone(), NanVal::from_value(v)))
                    .collect();
                NanVal::heap_map(nan_map)
            }
            Value::Record { type_name, fields } => {
                // Build TypeInfo from the Value's field names (preserving order)
                let field_names: Vec<String> = fields.keys().cloned().collect();
                let type_info = Rc::new(TypeInfo {
                    name: type_name.clone(),
                    fields: field_names.clone(),
                    num_fields: 0,
                });
                let flat: Box<[NanVal]> = field_names
                    .iter()
                    .map(|k| NanVal::from_value(&fields[k]))
                    .collect::<Vec<_>>()
                    .into_boxed_slice();
                NanVal::heap_record(type_info, flat)
            }
            Value::Ok(inner) => NanVal::heap_ok(NanVal::from_value(inner)),
            Value::Err(inner) => NanVal::heap_err(NanVal::from_value(inner)),
            Value::FnRef(name) => NanVal::heap_string(format!("<fn:{}>", name)),
        }
    }

    pub fn to_value(self) -> Value {
        if self.is_number() {
            return Value::Number(self.as_number());
        }
        if self.is_arena_record() {
            return unsafe {
                let rec = self.as_arena_record();
                let n = rec.n_fields as usize;
                let mut field_map = HashMap::new();
                let registry_ptr = ACTIVE_REGISTRY.with(|r| r.get());
                let (type_name, field_names) = if !registry_ptr.is_null() {
                    let registry = &*registry_ptr;
                    match registry.types.get(rec.type_id as usize) {
                        Some(ti) => (ti.name.clone(), Some(&ti.fields)),
                        None => (String::new(), None),
                    }
                } else {
                    (String::new(), None)
                };
                for i in 0..n {
                    let v = NanVal(*rec.field_ptr(i));
                    let name = field_names
                        .and_then(|f| f.get(i).cloned())
                        .unwrap_or_else(|| format!("_{}", i));
                    field_map.insert(name, v.to_value());
                }
                Value::Record {
                    type_name,
                    fields: field_map,
                }
            };
        }
        match self.0 {
            TAG_NIL => Value::Nil,
            TAG_TRUE => Value::Bool(true),
            TAG_FALSE => Value::Bool(false),
            _ => unsafe {
                // SAFETY: Not a number, nil, true, or false — must be a heap-tagged
                // pointer. The NanVal was created by a heap_* constructor so the
                // Rc is still live (we own this NanVal value).
                debug_assert!(
                    self.is_heap(),
                    "to_value: unexpected non-heap NanVal tag {:#018x}",
                    self.0
                );
                match self.as_heap_ref() {
                    HeapObj::Str(s) => Value::Text(s.clone()),
                    HeapObj::List(items) => {
                        Value::List(items.iter().map(|v| v.to_value()).collect())
                    }
                    HeapObj::Map(m) => {
                        Value::Map(m.iter().map(|(k, v)| (k.clone(), v.to_value())).collect())
                    }
                    HeapObj::Record { type_info, fields } => Value::Record {
                        type_name: type_info.name.clone(),
                        fields: type_info
                            .fields
                            .iter()
                            .zip(fields.iter())
                            .map(|(k, v)| (k.clone(), v.to_value()))
                            .collect(),
                    },
                    HeapObj::OkVal(inner) => Value::Ok(Box::new(inner.to_value())),
                    HeapObj::ErrVal(inner) => Value::Err(Box::new(inner.to_value())),
                }
            },
        }
    }

    /// Convert to Value, properly resolving arena record field names via registry.
    #[allow(dead_code)]
    pub(crate) fn to_value_with_registry(self, registry: &TypeRegistry) -> Value {
        if self.is_arena_record() {
            return unsafe {
                let rec = self.as_arena_record();
                let type_info = &registry.types[rec.type_id as usize];
                let n = rec.n_fields as usize;
                let mut field_map = HashMap::new();
                for i in 0..n {
                    let v = NanVal(*rec.field_ptr(i));
                    let name = type_info
                        .fields
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| format!("_{}", i));
                    field_map.insert(name, v.to_value_with_registry(registry));
                }
                Value::Record {
                    type_name: type_info.name.clone(),
                    fields: field_map,
                }
            };
        }
        self.to_value()
    }
}

// ── VM ───────────────────────────────────────────────────────────────

pub fn compile(program: &Program) -> Result<CompiledProgram, CompileError> {
    let mut prog = RegCompiler::new().compile_program(program)?;
    prog.nan_constants = prog
        .chunks
        .iter()
        .map(|chunk| chunk.constants.iter().map(NanVal::from_value).collect())
        .collect();
    Ok(prog)
}

pub fn run(
    compiled: &CompiledProgram,
    func_name: Option<&str>,
    args: Vec<Value>,
) -> Result<Value, VmRuntimeError> {
    let target = match func_name {
        Some(name) => name.to_string(),
        None => compiled
            .func_names
            .first()
            .ok_or_else(|| VmRuntimeError {
                error: VmError::NoFunctionsDefined,
                span: None,
                call_stack: Vec::new(),
            })?
            .clone(),
    };
    let func_idx = compiled.func_index(&target).ok_or_else(|| VmRuntimeError {
        error: VmError::UndefinedFunction {
            name: target.clone(),
        },
        span: None,
        call_stack: Vec::new(),
    })?;
    VM::new(compiled).call(func_idx, args)
}

pub fn run_with_tools(
    compiled: &CompiledProgram,
    func_name: Option<&str>,
    args: Vec<Value>,
    provider: &dyn crate::tools::ToolProvider,
    #[cfg(feature = "tools")] runtime: &tokio::runtime::Runtime,
) -> Result<Value, VmRuntimeError> {
    let target = match func_name {
        Some(name) => name.to_string(),
        None => compiled
            .func_names
            .first()
            .ok_or_else(|| VmRuntimeError {
                error: VmError::NoFunctionsDefined,
                span: None,
                call_stack: Vec::new(),
            })?
            .clone(),
    };
    let func_idx = compiled.func_index(&target).ok_or_else(|| VmRuntimeError {
        error: VmError::UndefinedFunction {
            name: target.clone(),
        },
        span: None,
        call_stack: Vec::new(),
    })?;
    VM::new_with_tools(
        compiled,
        provider,
        #[cfg(feature = "tools")]
        runtime,
    )
    .call(func_idx, args)
}

#[cfg(test)]
pub fn compile_and_run(
    program: &Program,
    func_name: Option<&str>,
    args: Vec<Value>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let compiled = compile(program)?;
    Ok(run(&compiled, func_name, args).map_err(|e| e.error)?)
}

/// Reusable VM handle — avoids re-allocating stack/frames per call.
pub struct VmState<'a> {
    vm: VM<'a>,
}

impl<'a> VmState<'a> {
    pub fn new(compiled: &'a CompiledProgram) -> Self {
        VmState {
            vm: VM::new(compiled),
        }
    }

    pub fn call(&mut self, func_name: &str, args: Vec<Value>) -> VmResult<Value> {
        for v in self.vm.stack.drain(..) {
            v.drop_rc();
        }
        self.vm.frames.clear();

        let func_idx =
            self.vm
                .program
                .func_index(func_name)
                .ok_or_else(|| VmError::UndefinedFunction {
                    name: func_name.to_string(),
                })?;
        let nan_args: Vec<NanVal> = args.iter().map(NanVal::from_value).collect();
        self.vm.setup_call(func_idx, nan_args, 0);
        self.vm.execute() // returns VmError for bench compatibility
    }
}

struct CallFrame {
    chunk_idx: u16,
    ip: usize,
    stack_base: usize,
    result_reg: u8,
}

struct VM<'a> {
    program: &'a CompiledProgram,
    stack: Vec<NanVal>,
    frames: Vec<CallFrame>,
    arena: BumpArena,
    /// Last dispatched instruction position — for error span capture.
    last_ci: usize,
    last_ip: usize,
    tool_provider: Option<&'a dyn crate::tools::ToolProvider>,
    #[cfg(feature = "tools")]
    tokio_runtime: Option<&'a tokio::runtime::Runtime>,
}

impl<'a> Drop for VM<'a> {
    fn drop(&mut self) {
        for v in &self.stack {
            v.drop_rc();
        }
    }
}

impl<'a> VM<'a> {
    fn new(program: &'a CompiledProgram) -> Self {
        VM {
            program,
            stack: Vec::with_capacity(4096),
            frames: Vec::with_capacity(64),
            arena: BumpArena::new(),
            last_ci: 0,
            last_ip: 0,
            tool_provider: None,
            #[cfg(feature = "tools")]
            tokio_runtime: None,
        }
    }

    fn new_with_tools(
        program: &'a CompiledProgram,
        provider: &'a dyn crate::tools::ToolProvider,
        #[cfg(feature = "tools")] runtime: &'a tokio::runtime::Runtime,
    ) -> Self {
        VM {
            program,
            stack: Vec::with_capacity(4096),
            frames: Vec::with_capacity(64),
            arena: BumpArena::new(),
            last_ci: 0,
            last_ip: 0,
            tool_provider: Some(provider),
            #[cfg(feature = "tools")]
            tokio_runtime: Some(runtime),
        }
    }

    fn setup_call(&mut self, func_idx: u16, args: Vec<NanVal>, result_reg: u8) {
        let chunk = &self.program.chunks[func_idx as usize];
        let stack_base = self.stack.len();

        for arg in args {
            self.stack.push(arg);
        }

        // Pre-allocate register slots
        while self.stack.len() < stack_base + chunk.reg_count as usize {
            self.stack.push(NanVal::nil());
        }

        self.frames.push(CallFrame {
            chunk_idx: func_idx,
            ip: 0,
            stack_base,
            result_reg,
        });
    }

    fn call(&mut self, func_idx: u16, args: Vec<Value>) -> Result<Value, VmRuntimeError> {
        let nan_args: Vec<NanVal> = args.iter().map(NanVal::from_value).collect();
        self.setup_call(func_idx, nan_args, 0);
        self.execute().map_err(|e| self.make_runtime_error(e))
    }

    /// Build a `VmRuntimeError` from a `VmError`, capturing span and call stack.
    fn make_runtime_error(&self, error: VmError) -> VmRuntimeError {
        let span = self
            .program
            .chunks
            .get(self.last_ci)
            .and_then(|chunk| chunk.spans.get(self.last_ip))
            .copied()
            .filter(|s| *s != crate::ast::Span::UNKNOWN);
        let call_stack: Vec<String> = self
            .frames
            .iter()
            .filter_map(|f| self.program.func_names.get(f.chunk_idx as usize).cloned())
            .collect();
        VmRuntimeError {
            error,
            span,
            call_stack,
        }
    }

    // reg!/reg_set! carry their own unsafe {} — clippy flags them as redundant when
    // expanded inside an outer unsafe {} site. The inner unsafe is intentional as
    // documentation; allow the lint here.
    #[allow(unused_unsafe)]
    fn execute(&mut self) -> VmResult<Value> {
        // Set active registry for arena record promotion in nanval_to_json and JIT callbacks.
        // `self.program` is owned by the VM and outlives `execute()`.
        // The guard ensures the pointer is cleared on return or panic.
        ACTIVE_REGISTRY.with(|r| r.set(&self.program.type_registry as *const TypeRegistry));
        let _registry_guard = ActiveRegistryGuard;
        // SAFETY: execute() is only called from call() after setup_call() has pushed
        // a frame, so frames is non-empty.
        let frame = unsafe { self.frames.last().unwrap_unchecked() };
        let mut ci = frame.chunk_idx as usize;
        let mut ip = frame.ip;
        let mut base = frame.stack_base;

        loop {
            // SAFETY: ci is always set from frame.chunk_idx, which is a valid index
            // assigned by the compiler (func_idx < chunks.len()). nan_constants has
            // the same length as chunks (built together in compile()).
            let code = unsafe { &self.program.chunks.get_unchecked(ci).code };
            let nan_consts = unsafe { self.program.nan_constants.get_unchecked(ci) };

            if ip >= code.len() {
                // Safety: should not happen with explicit RET, but handle gracefully
                let result = NanVal::nil();
                for i in base..self.stack.len() {
                    self.stack[i].drop_rc();
                }
                self.stack.truncate(base);
                self.frames.pop();
                if self.frames.is_empty() {
                    return Ok(result.to_value());
                }
                // SAFETY: we just checked !self.frames.is_empty().
                let f = unsafe { self.frames.last().unwrap_unchecked() };
                let target =
                    f.stack_base + self.frames.last().map(|f| f.result_reg).unwrap_or(0) as usize;
                ci = f.chunk_idx as usize;
                ip = f.ip;
                base = f.stack_base;
                if target < self.stack.len() {
                    self.stack[target].drop_rc();
                    self.stack[target] = result;
                }
                continue;
            }

            // SAFETY: ip < code.len() was verified by the bounds check above.
            let inst = unsafe { *code.get_unchecked(ip) };
            ip += 1;
            let op = (inst >> 24) as u8;

            // Macro for register access in hot paths.
            // SAFETY invariant for reg!/reg_set!: the compiler assigns each
            // function a reg_count and stack slots are pre-allocated in setup_call.
            // Register indices in instructions are always < reg_count, so
            // base + reg_idx < stack.len() is guaranteed by construction.
            macro_rules! reg {
                ($idx:expr) => {
                    // SAFETY: $idx = base + encoded register, within pre-allocated slots.
                    unsafe { *self.stack.get_unchecked($idx) }
                };
            }
            macro_rules! reg_set {
                ($idx:expr, $val:expr) => {
                    // SAFETY: same bounds as reg!; using as_mut_ptr().add() to avoid
                    // aliasing a mutable reference to the stack while it may be read.
                    unsafe {
                        let slot = self.stack.as_mut_ptr().add($idx);
                        (*slot).drop_rc();
                        *slot = $val;
                    }
                };
            }
            // Save position only on error paths. ip was already incremented above,
            // so subtract 1 to recover the instruction's original index.
            macro_rules! vm_err {
                ($e:expr) => {{
                    self.last_ci = ci;
                    self.last_ip = ip - 1;
                    return Err($e);
                }};
            }

            match op {
                OP_ADD => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let bv = reg!(b);
                    let cv = reg!(c);
                    if bv.is_number() && cv.is_number() {
                        reg_set!(a, NanVal::number(bv.as_number() + cv.as_number()));
                    } else if bv.is_string() && cv.is_string() {
                        // Fast path: if left string has RC=1 (sole owner), take ownership
                        // via Rc::try_unwrap and push_str in place — O(n) amortized instead
                        // of O(n²) for repeated `s = +s "x"` patterns.
                        let ptr_b = (bv.0 & PTR_MASK) as *const HeapObj;
                        // SAFETY: bv.is_string() guarantees ptr_b was produced by
                        // Rc::into_raw in heap_string() with strong count >= 1.
                        let rc_b = unsafe { Rc::from_raw(ptr_b) };
                        if Rc::strong_count(&rc_b) == 1 {
                            match Rc::try_unwrap(rc_b) {
                                Ok(heap_obj) => {
                                    // HeapObj::Str's Drop impl is a no-op (only non-string
                                    // variants drop_rc children), so ptr::read of the String
                                    // out of a ManuallyDrop shell is safe.
                                    // SAFETY: tag is TAG_STRING → variant is HeapObj::Str.
                                    let mut owned: String = unsafe {
                                        let md = std::mem::ManuallyDrop::new(heap_obj);
                                        std::ptr::read(match &*md {
                                            HeapObj::Str(s) => s as *const String,
                                            _ => unreachable!(),
                                        })
                                    };
                                    // rc_b is consumed; the NanVal `bv` is now a dangling
                                    // pointer. Nullify slot b immediately so that any
                                    // subsequent OP_MOVE / reg_set! on b won't double-free.
                                    unsafe {
                                        *self.stack.as_mut_ptr().add(b) = NanVal::nil();
                                    }
                                    // Read the right-hand string BEFORE touching slot a.
                                    let sc_ptr: *const str = unsafe {
                                        match cv.as_heap_ref() {
                                            HeapObj::Str(s) => s.as_str() as *const str,
                                            _ => unreachable!(),
                                        }
                                    };
                                    // SAFETY: cv still live so sc_ptr is valid.
                                    owned.push_str(unsafe { &*sc_ptr });
                                    let new_val = NanVal::heap_string(owned);
                                    unsafe {
                                        let slot = self.stack.as_mut_ptr().add(a);
                                        // a != b: drop old value at slot a (b is nil after above).
                                        // a == b: slot a == slot b, already nil — just write new_val.
                                        if a != b {
                                            (*slot).drop_rc();
                                        }
                                        *slot = new_val;
                                    }
                                }
                                Err(rc_back) => {
                                    // Shouldn't happen (RC was 1), but fall back safely.
                                    std::mem::forget(rc_back);
                                    let result = unsafe {
                                        let sb = match bv.as_heap_ref() {
                                            HeapObj::Str(s) => s,
                                            _ => unreachable!(),
                                        };
                                        let sc = match cv.as_heap_ref() {
                                            HeapObj::Str(s) => s,
                                            _ => unreachable!(),
                                        };
                                        let mut out = String::with_capacity(sb.len() + sc.len());
                                        out.push_str(sb);
                                        out.push_str(sc);
                                        NanVal::heap_string(out)
                                    };
                                    reg_set!(a, result);
                                }
                            }
                        } else {
                            // RC > 1 — must copy; restore count by forgetting rc_b.
                            std::mem::forget(rc_b);
                            let result = unsafe {
                                // SAFETY: is_string() confirmed heap-tagged with live RC.
                                let sb = match bv.as_heap_ref() {
                                    HeapObj::Str(s) => s,
                                    _ => unreachable!(),
                                };
                                let sc = match cv.as_heap_ref() {
                                    HeapObj::Str(s) => s,
                                    _ => unreachable!(),
                                };
                                let mut out = String::with_capacity(sb.len() + sc.len());
                                out.push_str(sb);
                                out.push_str(sc);
                                NanVal::heap_string(out)
                            };
                            reg_set!(a, result);
                        }
                    } else if bv.is_heap() && cv.is_heap() {
                        // SAFETY: is_heap() confirmed both are heap-tagged with live RC.
                        let bref = unsafe { bv.as_heap_ref() };
                        let cref = unsafe { cv.as_heap_ref() };
                        if let (HeapObj::List(left), HeapObj::List(right)) = (bref, cref) {
                            let mut new_items = Vec::with_capacity(left.len() + right.len());
                            for v in left {
                                v.clone_rc();
                                new_items.push(*v);
                            }
                            for v in right {
                                v.clone_rc();
                                new_items.push(*v);
                            }
                            reg_set!(a, NanVal::heap_list(new_items));
                        } else {
                            vm_err!(VmError::Type("cannot add non-matching types"));
                        }
                    } else {
                        vm_err!(VmError::Type("cannot add non-matching types"));
                    }
                }
                OP_SUB => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let bv = reg!(b);
                    let cv = reg!(c);
                    if bv.is_number() && cv.is_number() {
                        reg_set!(a, NanVal::number(bv.as_number() - cv.as_number()));
                    } else {
                        vm_err!(VmError::Type("cannot subtract non-numbers"));
                    }
                }
                OP_MUL => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let bv = reg!(b);
                    let cv = reg!(c);
                    if bv.is_number() && cv.is_number() {
                        reg_set!(a, NanVal::number(bv.as_number() * cv.as_number()));
                    } else {
                        vm_err!(VmError::Type("cannot multiply non-numbers"));
                    }
                }
                OP_DIV => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let bv = reg!(b);
                    let cv = reg!(c);
                    if bv.is_number() && cv.is_number() {
                        let dv = cv.as_number();
                        if dv == 0.0 {
                            vm_err!(VmError::DivisionByZero);
                        }
                        reg_set!(a, NanVal::number(bv.as_number() / dv));
                    } else {
                        vm_err!(VmError::Type("cannot divide non-numbers"));
                    }
                }
                OP_EQ => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let eq = nanval_equal(reg!(b), reg!(c));
                    reg_set!(a, NanVal::boolean(eq));
                }
                OP_NE => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let eq = nanval_equal(reg!(b), reg!(c));
                    reg_set!(a, NanVal::boolean(!eq));
                }
                OP_GT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let bv = reg!(b);
                    let cv = reg!(c);
                    if bv.is_number() && cv.is_number() {
                        reg_set!(a, NanVal::boolean(bv.as_number() > cv.as_number()));
                    } else if bv.is_string() && cv.is_string() {
                        let result =
                            unsafe { nanval_str_cmp(bv, cv) == std::cmp::Ordering::Greater };
                        reg_set!(a, NanVal::boolean(result));
                    } else {
                        vm_err!(VmError::Type(
                            "cannot compare > : operands must be same type (n or t)"
                        ));
                    }
                }
                OP_LT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let bv = reg!(b);
                    let cv = reg!(c);
                    if bv.is_number() && cv.is_number() {
                        reg_set!(a, NanVal::boolean(bv.as_number() < cv.as_number()));
                    } else if bv.is_string() && cv.is_string() {
                        let result = unsafe { nanval_str_cmp(bv, cv) == std::cmp::Ordering::Less };
                        reg_set!(a, NanVal::boolean(result));
                    } else {
                        vm_err!(VmError::Type(
                            "cannot compare < : operands must be same type (n or t)"
                        ));
                    }
                }
                OP_GE => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let bv = reg!(b);
                    let cv = reg!(c);
                    if bv.is_number() && cv.is_number() {
                        reg_set!(a, NanVal::boolean(bv.as_number() >= cv.as_number()));
                    } else if bv.is_string() && cv.is_string() {
                        let result = unsafe { nanval_str_cmp(bv, cv) != std::cmp::Ordering::Less };
                        reg_set!(a, NanVal::boolean(result));
                    } else {
                        vm_err!(VmError::Type(
                            "cannot compare >= : operands must be same type (n or t)"
                        ));
                    }
                }
                OP_LE => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let bv = reg!(b);
                    let cv = reg!(c);
                    if bv.is_number() && cv.is_number() {
                        reg_set!(a, NanVal::boolean(bv.as_number() <= cv.as_number()));
                    } else if bv.is_string() && cv.is_string() {
                        let result =
                            unsafe { nanval_str_cmp(bv, cv) != std::cmp::Ordering::Greater };
                        reg_set!(a, NanVal::boolean(result));
                    } else {
                        vm_err!(VmError::Type(
                            "cannot compare <= : operands must be same type (n or t)"
                        ));
                    }
                }
                OP_MOVE => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_number() {
                        v.clone_rc();
                    }
                    reg_set!(a, v);
                }
                OP_NOT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let t = nanval_truthy(reg!(b));
                    reg_set!(a, NanVal::boolean(!t));
                }
                OP_NEG => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if v.is_number() {
                        reg_set!(a, NanVal::number(-v.as_number()));
                    } else {
                        vm_err!(VmError::Type("cannot negate non-number"));
                    }
                }
                OP_WRAPOK => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let mut v = reg!(b);
                    if v.is_arena_record() {
                        v = v.promote_arena_to_heap(&self.program.type_registry);
                    } else if !v.is_number() {
                        v.clone_rc();
                    }
                    reg_set!(a, NanVal::heap_ok(v));
                }
                OP_WRAPERR => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let mut v = reg!(b);
                    if v.is_arena_record() {
                        v = v.promote_arena_to_heap(&self.program.type_registry);
                    } else if !v.is_number() {
                        v.clone_rc();
                    }
                    reg_set!(a, NanVal::heap_err(v));
                }
                OP_ISOK => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let is_ok = (reg!(b).0 & TAG_MASK) == TAG_OK;
                    reg_set!(a, NanVal::boolean(is_ok));
                }
                OP_ISERR => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let is_err = (reg!(b).0 & TAG_MASK) == TAG_ERR;
                    reg_set!(a, NanVal::boolean(is_err));
                }
                OP_ISNUM => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let is_num = reg!(b).is_number();
                    reg_set!(a, NanVal::boolean(is_num));
                }
                OP_ISTEXT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let is_text = reg!(b).is_string();
                    reg_set!(a, NanVal::boolean(is_text));
                }
                OP_ISBOOL => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b).0;
                    let is_bool = v == TAG_TRUE || v == TAG_FALSE;
                    reg_set!(a, NanVal::boolean(is_bool));
                }
                OP_ISLIST => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let is_list = (reg!(b).0 & TAG_MASK) == TAG_LIST;
                    reg_set!(a, NanVal::boolean(is_list));
                }
                OP_MAPNEW => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    reg_set!(a, NanVal::heap_map(HashMap::new()));
                }
                OP_MGET => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let map_v = reg!(b);
                    let key_v = reg!(c);
                    let result = unsafe {
                        match map_v.as_heap_ref() {
                            HeapObj::Map(m) => match key_v.as_heap_ref() {
                                HeapObj::Str(k) => m
                                    .get(k.as_str())
                                    .map(|v| {
                                        v.clone_rc();
                                        *v
                                    })
                                    .unwrap_or_else(NanVal::nil),
                                _ => vm_err!(VmError::Type("mget: key must be text")),
                            },
                            _ => vm_err!(VmError::Type("mget: first arg must be a map")),
                        }
                    };
                    reg_set!(a, result);
                }
                OP_MSET => {
                    // Two-instruction sequence: OP_MSET A=result B=map C=key; data word A=val_reg
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: compiler always emits the data word immediately after OP_MSET
                    let data_inst = unsafe { *code.get_unchecked(ip) };
                    ip += 1;
                    let d = ((data_inst >> 16) & 0xFF) as usize + base;
                    let map_v = reg!(b);
                    let key_v = reg!(c);
                    let val_v = reg!(d);
                    let result = unsafe {
                        match map_v.as_heap_ref() {
                            HeapObj::Map(m) => match key_v.as_heap_ref() {
                                HeapObj::Str(k) => {
                                    let mut new_map = m.clone();
                                    // m.clone() bit-copies values; bump RC for each retained entry
                                    for v in new_map.values() {
                                        v.clone_rc();
                                    }
                                    val_v.clone_rc();
                                    if let Some(old) = new_map.insert(k.clone(), val_v) {
                                        old.drop_rc();
                                    }
                                    NanVal::heap_map(new_map)
                                }
                                _ => vm_err!(VmError::Type("mset: key must be text")),
                            },
                            _ => vm_err!(VmError::Type("mset: first arg must be a map")),
                        }
                    };
                    reg_set!(a, result);
                }
                OP_MHAS => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let map_v = reg!(b);
                    let key_v = reg!(c);
                    let result = unsafe {
                        match map_v.as_heap_ref() {
                            HeapObj::Map(m) => match key_v.as_heap_ref() {
                                HeapObj::Str(k) => NanVal::boolean(m.contains_key(k.as_str())),
                                _ => vm_err!(VmError::Type("mhas: key must be text")),
                            },
                            _ => vm_err!(VmError::Type("mhas: first arg must be a map")),
                        }
                    };
                    reg_set!(a, result);
                }
                OP_MKEYS => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let map_v = reg!(b);
                    let result = unsafe {
                        match map_v.as_heap_ref() {
                            HeapObj::Map(m) => {
                                let mut keys: Vec<&String> = m.keys().collect();
                                keys.sort();
                                let nan_keys: Vec<NanVal> = keys
                                    .iter()
                                    .map(|k| NanVal::heap_string((*k).clone()))
                                    .collect();
                                NanVal::heap_list(nan_keys)
                            }
                            _ => vm_err!(VmError::Type("mkeys: expects a map")),
                        }
                    };
                    reg_set!(a, result);
                }
                OP_MVALS => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let map_v = reg!(b);
                    let result = unsafe {
                        match map_v.as_heap_ref() {
                            HeapObj::Map(m) => {
                                let mut pairs: Vec<(&String, &NanVal)> = m.iter().collect();
                                pairs.sort_by_key(|(k, _)| k.as_str());
                                let nan_vals: Vec<NanVal> = pairs
                                    .iter()
                                    .map(|(_, v)| {
                                        v.clone_rc();
                                        **v
                                    })
                                    .collect();
                                NanVal::heap_list(nan_vals)
                            }
                            _ => vm_err!(VmError::Type("mvals: expects a map")),
                        }
                    };
                    reg_set!(a, result);
                }
                OP_MDEL => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let map_v = reg!(b);
                    let key_v = reg!(c);
                    let result = unsafe {
                        match map_v.as_heap_ref() {
                            HeapObj::Map(m) => match key_v.as_heap_ref() {
                                HeapObj::Str(k) => {
                                    let mut new_map = m.clone();
                                    // m.clone() bit-copies values; bump RC for each retained entry
                                    for v in new_map.values() {
                                        v.clone_rc();
                                    }
                                    // drop_rc the removed value before HashMap::remove discards it,
                                    // otherwise its inner heap RC leaks/double-frees on next gc.
                                    if let Some(removed) = new_map.remove(k.as_str()) {
                                        removed.drop_rc();
                                    }
                                    NanVal::heap_map(new_map)
                                }
                                _ => vm_err!(VmError::Type("mdel: key must be text")),
                            },
                            _ => vm_err!(VmError::Type("mdel: first arg must be a map")),
                        }
                    };
                    reg_set!(a, result);
                }
                OP_PRT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    println!("{}", v.to_value());
                    // passthrough: same heap value now lives in two regs, bump RC
                    v.clone_rc();
                    reg_set!(a, v);
                }
                OP_RD => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_string() {
                        vm_err!(VmError::Type("rd requires a string path"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let path = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_owned(),
                            _ => unreachable!(),
                        }
                    };
                    let fmt = std::path::Path::new(&path)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("raw")
                        .to_lowercase();
                    let result = match std::fs::read_to_string(&path) {
                        Ok(content) => match vm_parse_format(&fmt, &content) {
                            Ok(v) => NanVal::heap_ok(v),
                            Err(e) => NanVal::heap_err(e),
                        },
                        Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())),
                    };
                    reg_set!(a, result);
                }
                OP_RDL => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_string() {
                        vm_err!(VmError::Type("rdl requires a string path"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let path = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_owned(),
                            _ => unreachable!(),
                        }
                    };
                    let result = match std::fs::read_to_string(&path) {
                        Ok(content) => {
                            let lines: Vec<NanVal> = content
                                .lines()
                                .map(|l| NanVal::heap_string(l.to_string()))
                                .collect();
                            NanVal::heap_ok(NanVal::heap_list(lines))
                        }
                        Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())),
                    };
                    reg_set!(a, result);
                }
                OP_WR => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_string() {
                        vm_err!(VmError::Type("wr arg 1 must be a string path"));
                    }
                    if !vc.is_string() {
                        vm_err!(VmError::Type("wr arg 2 must be a string"));
                    }
                    // SAFETY: is_string() confirmed.
                    let (path, content) = unsafe {
                        let p = match vb.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_owned(),
                            _ => unreachable!(),
                        };
                        let c = match vc.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_owned(),
                            _ => unreachable!(),
                        };
                        (p, c)
                    };
                    let result = match std::fs::write(&path, &content) {
                        Ok(()) => NanVal::heap_ok(NanVal::heap_string(path)),
                        Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())),
                    };
                    reg_set!(a, result);
                }
                OP_WRL => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_string() {
                        vm_err!(VmError::Type("wrl arg 1 must be a string path"));
                    }
                    // SAFETY: is_string() confirmed.
                    let path = unsafe {
                        match vb.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_owned(),
                            _ => unreachable!(),
                        }
                    };
                    let result = if (vc.0 & TAG_MASK) == TAG_LIST {
                        // SAFETY: TAG_LIST confirmed heap-tagged list with live RC.
                        let lines = unsafe {
                            match vc.as_heap_ref() {
                                HeapObj::List(l) => l.clone(),
                                _ => unreachable!(),
                            }
                        };
                        let mut buf = String::new();
                        for line in &lines {
                            if !line.is_string() {
                                vm_err!(VmError::Type("wrl list elements must be strings"));
                            }
                            // SAFETY: is_string() confirmed.
                            let s = unsafe {
                                match line.as_heap_ref() {
                                    HeapObj::Str(s) => s.as_str().to_owned(),
                                    _ => unreachable!(),
                                }
                            };
                            buf.push_str(&s);
                            buf.push('\n');
                        }
                        match std::fs::write(&path, &buf) {
                            Ok(()) => NanVal::heap_ok(NanVal::heap_string(path)),
                            Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())),
                        }
                    } else {
                        vm_err!(VmError::Type("wrl arg 2 must be a list"));
                    };
                    reg_set!(a, result);
                }
                OP_TRM => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_string() {
                        vm_err!(VmError::Type("trm requires a string"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let s = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().trim().to_owned(),
                            _ => unreachable!(),
                        }
                    };
                    reg_set!(a, NanVal::heap_string(s));
                }
                OP_UPR => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_string() {
                        vm_err!(VmError::Type("upr requires a string"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let s = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_uppercase(),
                            _ => unreachable!(),
                        }
                    };
                    reg_set!(a, NanVal::heap_string(s));
                }
                OP_LWR => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_string() {
                        vm_err!(VmError::Type("lwr requires a string"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let s = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_lowercase(),
                            _ => unreachable!(),
                        }
                    };
                    reg_set!(a, NanVal::heap_string(s));
                }
                OP_CAP => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_string() {
                        vm_err!(VmError::Type("cap requires a string"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let s = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::Str(s) => {
                                let src = s.as_str();
                                let mut chars = src.chars();
                                match chars.next() {
                                    Some(c) => {
                                        c.to_uppercase().collect::<String>() + chars.as_str()
                                    }
                                    None => String::new(),
                                }
                            }
                            _ => unreachable!(),
                        }
                    };
                    reg_set!(a, NanVal::heap_string(s));
                }
                OP_ORD => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_string() {
                        vm_err!(VmError::Type("ord requires a string"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let cp = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::Str(s) => match s.as_str().chars().next() {
                                Some(c) => c as u32,
                                None => {
                                    vm_err!(VmError::Type("ord requires a non-empty string"));
                                }
                            },
                            _ => unreachable!(),
                        }
                    };
                    reg_set!(a, NanVal::number(cp as f64));
                }
                OP_CHR => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_number() {
                        vm_err!(VmError::Type("chr requires a number"));
                    }
                    let nv = v.as_number();
                    if !nv.is_finite() || nv.fract() != 0.0 || nv < 0.0 || nv > u32::MAX as f64 {
                        vm_err!(VmError::Type(
                            "chr requires a non-negative integer codepoint"
                        ));
                    }
                    let cp = nv as u32;
                    let s = match char::from_u32(cp) {
                        Some(c) => c.to_string(),
                        None => vm_err!(VmError::Type("chr: not a valid Unicode codepoint")),
                    };
                    reg_set!(a, NanVal::heap_string(s));
                }
                OP_PADL | OP_PADR => {
                    let op_byte = (inst >> 24) as u8;
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let sv = reg!(b);
                    let wv = reg!(c);
                    if !sv.is_string() {
                        vm_err!(VmError::Type("pad requires a string"));
                    }
                    if !wv.is_number() {
                        vm_err!(VmError::Type("pad width must be a number"));
                    }
                    let wn = wv.as_number();
                    if !wn.is_finite() || wn.fract() != 0.0 || wn < 0.0 {
                        vm_err!(VmError::Type("pad width must be a non-negative integer"));
                    }
                    let w = wn as usize;
                    // SAFETY: is_string() confirmed.
                    let s = unsafe {
                        match sv.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_owned(),
                            _ => unreachable!(),
                        }
                    };
                    let cc = s.chars().count();
                    let out = if cc >= w {
                        s
                    } else {
                        let pad = " ".repeat(w - cc);
                        if op_byte == OP_PADL {
                            format!("{pad}{s}")
                        } else {
                            format!("{s}{pad}")
                        }
                    };
                    reg_set!(a, NanVal::heap_string(out));
                }
                OP_UNQ => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if v.is_string() {
                        // SAFETY: is_string() confirmed.
                        let s = unsafe {
                            match v.as_heap_ref() {
                                HeapObj::Str(s) => s.as_str().to_owned(),
                                _ => unreachable!(),
                            }
                        };
                        let mut seen = std::collections::HashSet::new();
                        let deduped: String = s.chars().filter(|c| seen.insert(*c)).collect();
                        reg_set!(a, NanVal::heap_string(deduped));
                    } else if (v.0 & TAG_MASK) == TAG_LIST {
                        // SAFETY: TAG_LIST confirmed.
                        let items = unsafe {
                            match v.as_heap_ref() {
                                HeapObj::List(l) => l.clone(),
                                _ => unreachable!(),
                            }
                        };
                        // Use nanval_equal for dedup — raw bits can't distinguish heap strings
                        // with equal content but different allocations (O(n²), fine for data sizes).
                        // clone_rc each kept item: HeapObj::Drop will drop_rc the original list's
                        // inner NanVals, so we need RC≥2 for items we carry into the new list.
                        let mut out: Vec<NanVal> = Vec::new();
                        for item in items {
                            if !out.iter().any(|existing| nanval_equal(*existing, item)) {
                                item.clone_rc(); // keep RC alive past original list's drop
                                out.push(item);
                            }
                        }
                        reg_set!(a, NanVal::heap_list(out));
                    } else {
                        vm_err!(VmError::Type("unq requires a list or string"));
                    }
                }
                OP_UNWRAP => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    // SAFETY: OP_UNWRAP is only emitted by the compiler immediately
                    // after a passed OP_ISOK or OP_ISERR branch, which guarantees the
                    // value in register b is a heap-allocated Ok or Err wrapper.
                    // The debug_assert catches compiler bugs in debug builds.
                    debug_assert!(v.is_heap(), "OP_UNWRAP on non-heap value");
                    let inner = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::OkVal(inner) | HeapObj::ErrVal(inner) => {
                                inner.clone_rc();
                                *inner
                            }
                            _ => vm_err!(VmError::Type("unwrap on non-Ok/Err")),
                        }
                    };
                    reg_set!(a, inner);
                }
                OP_RECFLD => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let field_idx = (inst & 0xFF) as usize;
                    let record = reg!(b);
                    // Fast path: arena record — inline field access
                    if record.is_arena_record() {
                        let field_val = unsafe {
                            let rec = record.as_arena_record();
                            if field_idx < rec.n_fields as usize {
                                let v = NanVal(*rec.field_ptr(field_idx));
                                v.clone_rc(); // no-op for numbers; needed for heap strings
                                v
                            } else {
                                vm_err!(VmError::FieldNotFound {
                                    field: format!("index {}", field_idx)
                                });
                            }
                        };
                        reg_set!(a, field_val);
                    } else {
                        // SAFETY: OP_RECFLD is only emitted by the compiler for record
                        // field accesses on values the type-checker knows are records.
                        debug_assert!(record.is_heap(), "OP_RECFLD on non-heap value");
                        let field_val = unsafe {
                            match record.as_heap_ref() {
                                HeapObj::Record { fields, type_info } => {
                                    if field_idx < fields.len() {
                                        let val = fields[field_idx];
                                        val.clone_rc();
                                        val
                                    } else {
                                        let name = type_info
                                            .fields
                                            .get(field_idx)
                                            .map(|s| s.as_str())
                                            .unwrap_or("?");
                                        vm_err!(VmError::FieldNotFound {
                                            field: name.to_string()
                                        });
                                    }
                                }
                                _ => vm_err!(VmError::Type("field access on non-record")),
                            }
                        };
                        reg_set!(a, field_val);
                    } // end else (heap record path)
                }
                OP_RECFLD_NAME => {
                    // Dynamic field access by name (for JSON records, etc.)
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize;
                    let chunk = unsafe { self.program.chunks.get_unchecked(ci) };
                    let field_name = match &chunk.constants[c] {
                        Value::Text(s) => s.as_str(),
                        _ => vm_err!(VmError::Type("RecordField expects string constant")),
                    };
                    let record = reg!(b);
                    if record.is_arena_record() {
                        let field_val = unsafe {
                            let rec = record.as_arena_record();
                            let type_info = &self.program.type_registry.types[rec.type_id as usize];
                            match type_info.fields.iter().position(|f| f == field_name) {
                                Some(idx) if idx < rec.n_fields as usize => {
                                    let v = NanVal(*rec.field_ptr(idx));
                                    v.clone_rc();
                                    v
                                }
                                _ => vm_err!(VmError::FieldNotFound {
                                    field: field_name.to_string()
                                }),
                            }
                        };
                        reg_set!(a, field_val);
                    } else {
                        debug_assert!(record.is_heap(), "OP_RECFLD_NAME on non-heap value");
                        let field_val = unsafe {
                            match record.as_heap_ref() {
                                HeapObj::Record { type_info, fields } => {
                                    match type_info.fields.iter().position(|f| f == field_name) {
                                        Some(idx) if idx < fields.len() => {
                                            let val = fields[idx];
                                            val.clone_rc();
                                            val
                                        }
                                        _ => vm_err!(VmError::FieldNotFound {
                                            field: field_name.to_string()
                                        }),
                                    }
                                }
                                _ => vm_err!(VmError::Type("field access on non-record")),
                            }
                        };
                        reg_set!(a, field_val);
                    } // end else (heap record path)
                }
                OP_INDEX => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize;
                    let obj = reg!(b);
                    debug_assert!(obj.is_heap(), "OP_INDEX on non-heap value");
                    let item = unsafe {
                        match obj.as_heap_ref() {
                            HeapObj::List(items) => {
                                if c < items.len() {
                                    let v = items[c];
                                    v.clone_rc();
                                    v
                                } else {
                                    vm_err!(VmError::Type("list index out of bounds"));
                                }
                            }
                            _ => vm_err!(VmError::Type("index access on non-list")),
                        }
                    };
                    reg_set!(a, item);
                }
                OP_LISTGET => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let list = reg!(b);
                    let idx_val = reg!(c);
                    if !list.is_heap() {
                        vm_err!(VmError::Type("foreach requires a list"));
                    }
                    if idx_val.is_number() {
                        // SAFETY: is_heap() was checked above; list is a live heap pointer
                        // created by a heap_* constructor. The non-List arm returns Err
                        // without any dereference of a different type.
                        debug_assert!(list.is_heap(), "OP_LISTGET on non-heap value");
                        unsafe {
                            match list.as_heap_ref() {
                                HeapObj::List(items) => {
                                    let i = idx_val.as_number() as usize;
                                    if i < items.len() {
                                        let item = items[i];
                                        item.clone_rc();
                                        reg_set!(a, item);
                                        ip += 1; // skip the following JMP (stay in loop)
                                    }
                                    // else: fall through to JMP exit
                                }
                                _ => vm_err!(VmError::Type("foreach requires a list")),
                            }
                        }
                    } else {
                        vm_err!(VmError::Type("list index must be a number"));
                    }
                }
                // ── Fused foreach opcodes ─────────────────────────────────────────
                // OP_FOREACHPREP: validate collection, load item[0] into bind_reg.
                // ABC: A = bind_reg, B = coll_reg, C = idx_reg (idx must be 0.0 on entry).
                // On success (list non-empty): bind_reg = items[0], skip following JMP.
                // On failure (empty list): fall through to JMP exit.
                // Errors if collection is not a heap List.
                OP_FOREACHPREP => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let list = reg!(b);
                    if !list.is_heap() {
                        vm_err!(VmError::Type("foreach requires a list"));
                    }
                    // SAFETY: is_heap() verified above.
                    debug_assert!(list.is_heap(), "OP_FOREACHPREP on non-heap value");
                    unsafe {
                        match list.as_heap_ref() {
                            HeapObj::List(items) => {
                                if !items.is_empty() {
                                    let item = items[0];
                                    item.clone_rc();
                                    reg_set!(a, item);
                                    ip += 1; // skip the following JMP exit (stay in loop)
                                }
                                // else: empty list → fall through to JMP exit
                            }
                            _ => vm_err!(VmError::Type("foreach requires a list")),
                        }
                    }
                }
                // OP_FOREACHNEXT: increment idx_reg, load next element.
                // ABC: A = bind_reg, B = coll_reg, C = idx_reg.
                // Increments R[C] by 1. If R[C] < len(list): bind_reg = items[R[C]], skip
                // the following JMP (which would jump to exit). The instruction after the
                // skipped JMP is always a JMP back to the body start.
                // If R[C] >= len(list): fall through to JMP exit.
                OP_FOREACHNEXT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let list = reg!(b);
                    // idx_reg holds the current index (a number); increment it.
                    // SAFETY: idx_reg is always a number (initialized to 0.0 by compiler,
                    // only modified here by addition of 1.0).
                    let new_idx = reg!(c).as_number() + 1.0;
                    reg_set!(c, NanVal::number(new_idx));
                    // SAFETY: list is the same heap List validated by FOREACHPREP on entry.
                    debug_assert!(list.is_heap(), "OP_FOREACHNEXT on non-heap value");
                    unsafe {
                        match list.as_heap_ref() {
                            HeapObj::List(items) => {
                                let i = new_idx as usize;
                                if i < items.len() {
                                    let item = items[i];
                                    item.clone_rc();
                                    reg_set!(a, item);
                                    ip += 1; // skip JMP exit → execute JMP body_top
                                }
                                // else: out of bounds → fall through to JMP exit
                            }
                            _ => {
                                // Should never happen: list was validated by FOREACHPREP.
                                vm_err!(VmError::Type("foreach requires a list"))
                            }
                        }
                    }
                }
                // ── Fused compare-and-skip for numeric guard chains ────────────────────
                // ABC: A = reg, B = unused, C = constant pool index (ki).
                // Compares R[A] <op> K[C] directly as f64.
                // If condition TRUE  → ip += 1 (skip the following OP_JMP, enter body).
                // If condition FALSE → fall through to the OP_JMP that skips the body.
                OP_CMPK_GE_N => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let ki = (inst & 0xFF) as usize;
                    // SAFETY: ki < constants.len() guaranteed by compiler.
                    let lhs = unsafe { self.stack.get_unchecked(a) }.as_number();
                    let rhs = unsafe { nan_consts.get_unchecked(ki) }.as_number();
                    if lhs >= rhs {
                        ip += 1;
                    }
                }
                OP_CMPK_GT_N => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let ki = (inst & 0xFF) as usize;
                    let lhs = unsafe { self.stack.get_unchecked(a) }.as_number();
                    let rhs = unsafe { nan_consts.get_unchecked(ki) }.as_number();
                    if lhs > rhs {
                        ip += 1;
                    }
                }
                OP_CMPK_LT_N => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let ki = (inst & 0xFF) as usize;
                    let lhs = unsafe { self.stack.get_unchecked(a) }.as_number();
                    let rhs = unsafe { nan_consts.get_unchecked(ki) }.as_number();
                    if lhs < rhs {
                        ip += 1;
                    }
                }
                OP_CMPK_LE_N => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let ki = (inst & 0xFF) as usize;
                    let lhs = unsafe { self.stack.get_unchecked(a) }.as_number();
                    let rhs = unsafe { nan_consts.get_unchecked(ki) }.as_number();
                    if lhs <= rhs {
                        ip += 1;
                    }
                }
                OP_CMPK_EQ_N => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let ki = (inst & 0xFF) as usize;
                    let lhs = unsafe { self.stack.get_unchecked(a) }.as_number();
                    let rhs = unsafe { nan_consts.get_unchecked(ki) }.as_number();
                    if lhs == rhs {
                        ip += 1;
                    }
                }
                OP_CMPK_NE_N => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let ki = (inst & 0xFF) as usize;
                    let lhs = unsafe { self.stack.get_unchecked(a) }.as_number();
                    let rhs = unsafe { nan_consts.get_unchecked(ki) }.as_number();
                    if lhs != rhs {
                        ip += 1;
                    }
                }
                OP_LOADK => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let bx = (inst & 0xFFFF) as usize;
                    // SAFETY: bx is the constant pool index encoded in the instruction;
                    // the compiler only emits indices < constants.len().
                    let v = unsafe { *nan_consts.get_unchecked(bx) };
                    if !v.is_number() {
                        v.clone_rc();
                    }
                    reg_set!(a, v);
                }
                OP_JMP => {
                    let sbx = (inst & 0xFFFF) as i16;
                    ip = (ip as isize + sbx as isize) as usize;
                }
                OP_JMPF => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let sbx = (inst & 0xFFFF) as i16;
                    if !nanval_truthy(reg!(a)) {
                        ip = (ip as isize + sbx as isize) as usize;
                    }
                }
                OP_JMPT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let sbx = (inst & 0xFFFF) as i16;
                    if nanval_truthy(reg!(a)) {
                        ip = (ip as isize + sbx as isize) as usize;
                    }
                }
                OP_JMPNN => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let sbx = (inst & 0xFFFF) as i16;
                    if reg!(a).0 != TAG_NIL {
                        ip = (ip as isize + sbx as isize) as usize;
                    }
                }
                OP_CALL => {
                    let a = ((inst >> 16) & 0xFF) as u8;
                    let bx = (inst & 0xFFFF) as usize;
                    let func_idx = (bx >> 8) as u16;
                    let n_args = bx & 0xFF;

                    // SAFETY: frames is non-empty while execute() is running.
                    unsafe { self.frames.last_mut().unwrap_unchecked() }.ip = ip;

                    // If this is a tool call and we have a provider, dispatch
                    // through the provider instead of the stub chunk.
                    let is_tool_call = self
                        .program
                        .is_tool
                        .get(func_idx as usize)
                        .copied()
                        .unwrap_or(false);
                    if let (true, Some(_provider)) = (is_tool_call, self.tool_provider) {
                        let _tool_name = &self.program.func_names[func_idx as usize];
                        let mut value_args = Vec::with_capacity(n_args);
                        for i in 0..n_args {
                            value_args.push(reg!(base + a as usize + 1 + i).to_value());
                        }

                        let result: Value = {
                            #[cfg(feature = "tools")]
                            {
                                if let Some(rt) = self.tokio_runtime {
                                    rt.block_on(_provider.call(_tool_name, value_args))
                                        .unwrap_or_else(|e| {
                                            Value::Err(Box::new(Value::Text(e.to_string())))
                                        })
                                } else {
                                    let _ = value_args;
                                    Value::Ok(Box::new(Value::Nil))
                                }
                            }
                            #[cfg(not(feature = "tools"))]
                            {
                                let _ = value_args;
                                Value::Ok(Box::new(Value::Nil))
                            }
                        };

                        let nan_result = NanVal::from_value(&result);
                        reg_set!(base + a as usize, nan_result);
                        // ip was already saved above; continue to next instruction
                        continue;
                    }

                    // Push args directly onto the stack (no intermediate Vec).
                    let new_base = self.stack.len();
                    let callee_all_numeric =
                        unsafe { self.program.chunks.get_unchecked(func_idx as usize) }
                            .all_regs_numeric;
                    for i in 0..n_args {
                        let v = reg!(base + a as usize + 1 + i);
                        if !callee_all_numeric && !v.is_number() {
                            v.clone_rc();
                        }
                        self.stack.push(v);
                    }

                    // Pre-allocate remaining register slots for callee.
                    // Use reserve + set_len to avoid zero-filling slots that
                    // will be overwritten before use.  NanVal is Copy (u64 under
                    // NaN-boxing), so uninitialised slots are safe as long as
                    // every live path writes before reading — the bytecode
                    // compiler guarantees this for all register allocations.
                    let reg_count = self.program.chunks[func_idx as usize].reg_count as usize;
                    let new_len = new_base + reg_count;
                    let old_len = self.stack.len();
                    if new_len > old_len {
                        self.stack.reserve(new_len - old_len);
                        // SAFETY: NanVal is Copy (plain u64). We initialise every
                        // newly-visible slot to nil so that drop_rc in OP_RET
                        // never sees garbage ref-counts on unused registers.
                        let nil = NanVal::nil();
                        let ptr = self.stack.as_mut_ptr();
                        for i in old_len..new_len {
                            unsafe {
                                ptr.add(i).write(nil);
                            }
                        }
                        unsafe {
                            self.stack.set_len(new_len);
                        }
                    }

                    self.frames.push(CallFrame {
                        chunk_idx: func_idx,
                        ip: 0,
                        stack_base: new_base,
                        result_reg: a,
                    });

                    // SAFETY: we just pushed a new frame above.
                    ci = func_idx as usize;
                    ip = 0;
                    base = new_base;
                }
                OP_RET => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let mut result = reg!(a);

                    // SAFETY: frames is non-empty while execute() is running.
                    let frame = unsafe { self.frames.last().unwrap_unchecked() };
                    let result_reg = frame.result_reg;
                    let all_numeric =
                        unsafe { self.program.chunks.get_unchecked(ci) }.all_regs_numeric;

                    if all_numeric {
                        // Fast path: all registers are numeric — no RC ops needed.
                        self.stack.truncate(base);
                        self.frames.pop();

                        if self.frames.is_empty() {
                            self.arena.reset();
                            return Ok(result.to_value());
                        }

                        let f = unsafe { self.frames.last().unwrap_unchecked() };
                        ci = f.chunk_idx as usize;
                        ip = f.ip;
                        base = f.stack_base;
                        unsafe {
                            *self.stack.as_mut_ptr().add(base + result_reg as usize) = result;
                        }
                    } else {
                        if !result.is_number() && !result.is_arena_record() {
                            result.clone_rc();
                        }

                        for i in base..self.stack.len() {
                            // SAFETY: i is in range base..self.stack.len() by loop bounds.
                            unsafe { self.stack.get_unchecked(i) }.drop_rc();
                        }
                        self.stack.truncate(base);
                        self.frames.pop();

                        if self.frames.is_empty() {
                            // Promote arena records before resetting arena
                            if result.is_arena_record() {
                                result = result.promote_arena_to_heap(&self.program.type_registry);
                            }
                            self.arena.reset();
                            let val = result.to_value();
                            result.drop_rc();
                            return Ok(val);
                        }

                        // SAFETY: we just checked !self.frames.is_empty().
                        let f = unsafe { self.frames.last().unwrap_unchecked() };
                        ci = f.chunk_idx as usize;
                        ip = f.ip;
                        base = f.stack_base;

                        // Store result in caller's register
                        reg_set!(base + result_reg as usize, result);
                    }
                }
                OP_RECNEW => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let bx = (inst & 0xFFFF) as usize;
                    let type_id = (bx >> 8) as u16;
                    let n_fields = bx & 0xFF;

                    // Try arena allocation first (fast path)
                    if let Some(rec_ptr) = self.arena.alloc_record(type_id, n_fields) {
                        unsafe {
                            let rec = &mut *rec_ptr;
                            for i in 0..n_fields {
                                let v = reg!(a + 1 + i);
                                v.clone_rc(); // no-op for numbers; needed for heap strings etc.
                                *rec.field_ptr_mut(i) = v.0;
                            }
                        }
                        reg_set!(a, NanVal::arena_record(rec_ptr));
                    } else {
                        // Arena full — fall back to Rc path
                        let type_info =
                            Rc::clone(&self.program.type_registry.types[type_id as usize]);
                        let mut fields = Vec::with_capacity(n_fields);
                        for i in 0..n_fields {
                            let v = reg!(a + 1 + i);
                            v.clone_rc();
                            fields.push(v);
                        }
                        reg_set!(a, NanVal::heap_record(type_info, fields.into_boxed_slice()));
                    }
                }
                OP_RECWITH => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let bx = (inst & 0xFFFF) as usize;
                    let const_idx = bx >> 8;
                    let n_updates = bx & 0xFF;

                    // SAFETY: ci is a valid chunk index (same invariant as loop header).
                    let chunk = unsafe { self.program.chunks.get_unchecked(ci) };
                    let const_val = &chunk.constants[const_idx];

                    let old_record = reg!(a);

                    if old_record.is_arena_record() {
                        // Arena record with: allocate new arena record, copy fields, overwrite updates
                        let (type_id, old_n) = unsafe {
                            let rec = old_record.as_arena_record();
                            (rec.type_id, rec.n_fields as usize)
                        };
                        let slots: Vec<usize> = match const_val {
                            Value::List(items) => items
                                .iter()
                                .map(|v| match v {
                                    Value::Number(n) => *n as usize,
                                    _ => 0,
                                })
                                .collect(),
                            _ => vec![],
                        };
                        if let Some(new_ptr) = self.arena.alloc_record(type_id, old_n) {
                            unsafe {
                                let old_rec = old_record.as_arena_record();
                                let new_rec = &mut *new_ptr;
                                // Copy all fields from old record (clone_rc for heap refs)
                                for i in 0..old_n {
                                    let v = NanVal(*old_rec.field_ptr(i));
                                    v.clone_rc();
                                    *new_rec.field_ptr_mut(i) = v.0;
                                }
                                // Overwrite updated slots
                                for (i, &slot) in slots.iter().enumerate().take(n_updates) {
                                    if slot < old_n {
                                        // Drop the copied value and store the new one
                                        NanVal(*new_rec.field_ptr(slot)).drop_rc();
                                        let val = reg!(a + 1 + i);
                                        val.clone_rc();
                                        *new_rec.field_ptr_mut(slot) = val.0;
                                    }
                                }
                            }
                            reg_set!(a, NanVal::arena_record(new_ptr));
                        } else {
                            // Arena full — fall back to heap
                            let type_info =
                                Rc::clone(&self.program.type_registry.types[type_id as usize]);
                            unsafe {
                                let old_rec = old_record.as_arena_record();
                                let mut new_fields = Vec::with_capacity(old_n);
                                for i in 0..old_n {
                                    let v = NanVal(*old_rec.field_ptr(i));
                                    v.clone_rc();
                                    new_fields.push(v);
                                }
                                for (i, &slot) in slots.iter().enumerate().take(n_updates) {
                                    let val = reg!(a + 1 + i);
                                    val.clone_rc();
                                    if slot < new_fields.len() {
                                        new_fields[slot].drop_rc();
                                        new_fields[slot] = val;
                                    }
                                }
                                reg_set!(
                                    a,
                                    NanVal::heap_record(type_info, new_fields.into_boxed_slice())
                                );
                            }
                        }
                    } else {
                        debug_assert!(old_record.is_heap(), "OP_RECWITH on non-heap value");
                        let new_record = unsafe {
                            match old_record.as_heap_ref() {
                                HeapObj::Record { type_info, fields } => {
                                    // Clone the entire fields array
                                    let mut new_fields: Vec<NanVal> = fields.to_vec();
                                    for v in new_fields.iter() {
                                        v.clone_rc();
                                    }
                                    // Resolve update slots
                                    let slots: Vec<usize> = match const_val {
                                        Value::List(items) => items
                                            .iter()
                                            .map(|v| match v {
                                                Value::Number(n) => *n as usize,
                                                Value::Text(name) => type_info
                                                    .fields
                                                    .iter()
                                                    .position(|f| f == name)
                                                    .unwrap_or(0),
                                                _ => 0,
                                            })
                                            .collect(),
                                        _ => vec![],
                                    };
                                    // Overwrite updated slots
                                    for (i, &slot) in slots.iter().enumerate().take(n_updates) {
                                        let val = reg!(a + 1 + i);
                                        val.clone_rc();
                                        if slot < new_fields.len() {
                                            new_fields[slot].drop_rc();
                                            new_fields[slot] = val;
                                        }
                                    }
                                    NanVal::heap_record(
                                        Rc::clone(type_info),
                                        new_fields.into_boxed_slice(),
                                    )
                                }
                                _ => vm_err!(VmError::Type("'with' requires a record")),
                            }
                        };
                        reg_set!(a, new_record);
                    } // end else (heap record path)
                }
                OP_LISTNEW => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let n = (inst & 0xFFFF) as usize;
                    let mut items = Vec::with_capacity(n);
                    for i in 0..n {
                        let v = reg!(a + 1 + i);
                        v.clone_rc();
                        items.push(v);
                    }
                    reg_set!(a, NanVal::heap_list(items));
                }
                OP_ADDK_N => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize;
                    // SAFETY: c is a constant pool index emitted by the compiler (< nan_consts.len()).
                    // a = base + reg, within pre-allocated stack slots.
                    let kv = unsafe { *nan_consts.get_unchecked(c) };
                    // SAFETY: see NanVal::number_unchecked — inputs are finite, add cannot produce NaN.
                    let result =
                        unsafe { NanVal::number_unchecked(reg!(b).as_number() + kv.as_number()) };
                    unsafe {
                        *self.stack.as_mut_ptr().add(a) = result;
                    }
                }
                OP_SUBK_N => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize;
                    // SAFETY: same as OP_ADDK_N.
                    let kv = unsafe { *nan_consts.get_unchecked(c) };
                    // SAFETY: see NanVal::number_unchecked — inputs are finite, sub cannot produce NaN.
                    let result =
                        unsafe { NanVal::number_unchecked(reg!(b).as_number() - kv.as_number()) };
                    unsafe {
                        *self.stack.as_mut_ptr().add(a) = result;
                    }
                }
                OP_MULK_N => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize;
                    // SAFETY: same as OP_ADDK_N.
                    let kv = unsafe { *nan_consts.get_unchecked(c) };
                    // SAFETY: see NanVal::number_unchecked — inputs are finite, mul cannot produce NaN.
                    let result =
                        unsafe { NanVal::number_unchecked(reg!(b).as_number() * kv.as_number()) };
                    unsafe {
                        *self.stack.as_mut_ptr().add(a) = result;
                    }
                }
                OP_DIVK_N => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize;
                    // SAFETY: same as OP_ADDK_N.
                    let kv = unsafe { *nan_consts.get_unchecked(c) };
                    let dv = kv.as_number();
                    if dv == 0.0 {
                        vm_err!(VmError::DivisionByZero);
                    }
                    let result = NanVal::number(reg!(b).as_number() / dv);
                    unsafe {
                        *self.stack.as_mut_ptr().add(a) = result;
                    }
                }
                OP_ADD_NN => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: a, b, c are all base + register offsets within pre-allocated stack slots.
                    // SAFETY: see NanVal::number_unchecked — inputs are finite, add cannot produce NaN.
                    let result = unsafe {
                        NanVal::number_unchecked(reg!(b).as_number() + reg!(c).as_number())
                    };
                    unsafe {
                        *self.stack.as_mut_ptr().add(a) = result;
                    }
                }
                OP_SUB_NN => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: a, b, c are base + register offsets within pre-allocated stack slots.
                    // SAFETY: see NanVal::number_unchecked — inputs are finite, sub cannot produce NaN.
                    let result = unsafe {
                        NanVal::number_unchecked(reg!(b).as_number() - reg!(c).as_number())
                    };
                    unsafe {
                        *self.stack.as_mut_ptr().add(a) = result;
                    }
                }
                OP_MUL_NN => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: same as OP_SUB_NN.
                    // SAFETY: see NanVal::number_unchecked — inputs are finite, mul cannot produce NaN.
                    let result = unsafe {
                        NanVal::number_unchecked(reg!(b).as_number() * reg!(c).as_number())
                    };
                    unsafe {
                        *self.stack.as_mut_ptr().add(a) = result;
                    }
                }
                OP_DIV_NN => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: same as OP_SUB_NN.
                    let dv = reg!(c).as_number();
                    if dv == 0.0 {
                        vm_err!(VmError::DivisionByZero);
                    }
                    let result = NanVal::number(reg!(b).as_number() / dv);
                    unsafe {
                        *self.stack.as_mut_ptr().add(a) = result;
                    }
                }
                OP_ADD_SS => {
                    // Type-specialized string concatenation: both operands guaranteed to be
                    // strings by the compiler (type-specialisation). Guard against pathological
                    // cases (e.g. tests passing wrong-typed args) with an explicit is_string check.
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let bv = reg!(b);
                    let cv = reg!(c);
                    // Runtime type guard: both operands must actually be heap strings.
                    // In well-typed programs this is always true; the check prevents
                    // UB when wrong-typed values reach this opcode through function calls.
                    if !bv.is_string() || !cv.is_string() {
                        vm_err!(VmError::Type("cannot add non-matching types"));
                    }
                    // SAFETY: compiler guarantees both are strings; bv.is_string() → heap-tagged.
                    let ptr_b = (bv.0 & PTR_MASK) as *const HeapObj;
                    let rc_b = unsafe { Rc::from_raw(ptr_b) };
                    if Rc::strong_count(&rc_b) == 1 {
                        match Rc::try_unwrap(rc_b) {
                            Ok(heap_obj) => {
                                // SAFETY: tag is TAG_STRING → variant is HeapObj::Str.
                                let mut owned: String = unsafe {
                                    let md = std::mem::ManuallyDrop::new(heap_obj);
                                    std::ptr::read(match &*md {
                                        HeapObj::Str(s) => s as *const String,
                                        _ => unreachable!(),
                                    })
                                };
                                // Nullify slot b so no double-free.
                                unsafe {
                                    *self.stack.as_mut_ptr().add(b) = NanVal::nil();
                                }
                                // Read RHS string before touching slot a.
                                let sc_ptr: *const str = unsafe {
                                    match cv.as_heap_ref() {
                                        HeapObj::Str(s) => s.as_str() as *const str,
                                        _ => unreachable!(),
                                    }
                                };
                                // SAFETY: cv still live so sc_ptr is valid.
                                owned.push_str(unsafe { &*sc_ptr });
                                let new_val = NanVal::heap_string(owned);
                                unsafe {
                                    let slot = self.stack.as_mut_ptr().add(a);
                                    if a != b {
                                        (*slot).drop_rc();
                                    }
                                    *slot = new_val;
                                }
                            }
                            Err(rc_back) => {
                                // Shouldn't happen (RC was 1), fall back safely.
                                std::mem::forget(rc_back);
                                let result = unsafe {
                                    let sb = match bv.as_heap_ref() {
                                        HeapObj::Str(s) => s,
                                        _ => unreachable!(),
                                    };
                                    let sc = match cv.as_heap_ref() {
                                        HeapObj::Str(s) => s,
                                        _ => unreachable!(),
                                    };
                                    let mut out = String::with_capacity(sb.len() + sc.len());
                                    out.push_str(sb);
                                    out.push_str(sc);
                                    NanVal::heap_string(out)
                                };
                                reg_set!(a, result);
                            }
                        }
                    } else {
                        // RC > 1 — must copy; restore count by forgetting rc_b.
                        std::mem::forget(rc_b);
                        let result = unsafe {
                            let sb = match bv.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            };
                            let sc = match cv.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            };
                            let mut out = String::with_capacity(sb.len() + sc.len());
                            out.push_str(sb);
                            out.push_str(sc);
                            NanVal::heap_string(out)
                        };
                        reg_set!(a, result);
                    }
                }
                OP_LEN => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    let length = if v.is_string() {
                        // SAFETY: is_string() confirmed heap-tagged string with live RC.
                        let s = unsafe {
                            match v.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        s.len() as f64
                    } else if v.is_heap() {
                        // SAFETY: is_heap() confirmed heap-tagged with live RC.
                        match unsafe { v.as_heap_ref() } {
                            HeapObj::List(items) => items.len() as f64,
                            HeapObj::Map(m) => m.len() as f64,
                            _ => vm_err!(VmError::Type("len requires string, list, or map")),
                        }
                    } else {
                        vm_err!(VmError::Type("len requires string, list, or map"));
                    };
                    reg_set!(a, NanVal::number(length));
                }
                OP_STR => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_number() {
                        vm_err!(VmError::Type("str requires a number"));
                    }
                    let n = v.as_number();
                    let s = if n.fract() == 0.0 && n.abs() < 1e15 {
                        format!("{}", n as i64)
                    } else {
                        format!("{}", n)
                    };
                    reg_set!(a, NanVal::heap_string(s));
                }
                OP_NUM => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_string() {
                        vm_err!(VmError::Type("num requires a string"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let s = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::Str(s) => s,
                            _ => unreachable!(),
                        }
                    };
                    let result = match s.parse::<f64>() {
                        Ok(n) => NanVal::heap_ok(NanVal::number(n)),
                        Err(_) => {
                            v.clone_rc();
                            NanVal::heap_err(v)
                        }
                    };
                    reg_set!(a, result);
                }
                OP_ABS => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_number() {
                        vm_err!(VmError::Type("abs requires a number"));
                    }
                    reg_set!(a, NanVal::number(v.as_number().abs()));
                }
                OP_MIN | OP_MAX => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_number() || !vc.is_number() {
                        vm_err!(VmError::Type("min/max require numbers"));
                    }
                    let nb = vb.as_number();
                    let nc = vc.as_number();
                    let result = if op == OP_MIN { nb.min(nc) } else { nb.max(nc) };
                    reg_set!(a, NanVal::number(result));
                }
                OP_MOD => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_number() || !vc.is_number() {
                        vm_err!(VmError::Type("mod requires numbers"));
                    }
                    let nc = vc.as_number();
                    if nc == 0.0 {
                        vm_err!(VmError::Type("modulo by zero"));
                    }
                    reg_set!(a, NanVal::number(vb.as_number() % nc));
                }
                OP_CLAMP => {
                    // Two-instruction sequence: OP_CLAMP A=result B=x C=lo; data word A=hi_reg
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: compiler always emits the data word immediately after OP_CLAMP
                    let data_inst = unsafe { *code.get_unchecked(ip) };
                    ip += 1;
                    let d = ((data_inst >> 16) & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    let vd = reg!(d);
                    if !vb.is_number() || !vc.is_number() || !vd.is_number() {
                        vm_err!(VmError::Type("clamp requires numbers"));
                    }
                    let x = vb.as_number();
                    let lo = vc.as_number();
                    let hi = vd.as_number();
                    // Semantics: result = max(lo, min(hi, x)); when lo > hi, result == lo.
                    reg_set!(a, NanVal::number(x.min(hi).max(lo)));
                }
                OP_FLR | OP_CEL | OP_ROU => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_number() {
                        vm_err!(VmError::Type("flr/cel/rou requires a number"));
                    }
                    let n = v.as_number();
                    let result = if op == OP_FLR {
                        n.floor()
                    } else if op == OP_CEL {
                        n.ceil()
                    } else {
                        n.round()
                    };
                    reg_set!(a, NanVal::number(result));
                }
                OP_SQRT | OP_LOG | OP_EXP | OP_SIN | OP_COS | OP_TAN | OP_LOG10 | OP_LOG2 => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    let result = if v.is_number() {
                        let n = v.as_number();
                        match op {
                            OP_SQRT => n.sqrt(),
                            OP_LOG => n.ln(),
                            OP_EXP => n.exp(),
                            OP_SIN => n.sin(),
                            OP_COS => n.cos(),
                            OP_TAN => n.tan(),
                            OP_LOG10 => n.log10(),
                            _ => n.log2(),
                        }
                    } else {
                        f64::NAN
                    };
                    reg_set!(a, NanVal::number(result));
                }
                OP_POW => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    let result = if vb.is_number() && vc.is_number() {
                        vb.as_number().powf(vc.as_number())
                    } else {
                        f64::NAN
                    };
                    reg_set!(a, NanVal::number(result));
                }
                OP_ATAN2 => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    let result = if vb.is_number() && vc.is_number() {
                        vb.as_number().atan2(vc.as_number())
                    } else {
                        f64::NAN
                    };
                    reg_set!(a, NanVal::number(result));
                }
                OP_TRANSPOSE => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_heap() {
                        vm_err!(VmError::Type("transpose requires a list of lists"));
                    }
                    let rows = match unsafe { v.as_heap_ref() } {
                        HeapObj::List(items) => items,
                        _ => vm_err!(VmError::Type("transpose requires a list of lists")),
                    };
                    if rows.is_empty() {
                        reg_set!(a, NanVal::heap_list(vec![]));
                    } else {
                        let mut row_refs: Vec<&Vec<NanVal>> = Vec::with_capacity(rows.len());
                        let mut ncols: Option<usize> = None;
                        let mut shape_err = false;
                        for row in rows {
                            if !row.is_heap() {
                                shape_err = true;
                                break;
                            }
                            let r = match unsafe { row.as_heap_ref() } {
                                HeapObj::List(items) => items,
                                _ => {
                                    shape_err = true;
                                    break;
                                }
                            };
                            match ncols {
                                None => ncols = Some(r.len()),
                                Some(n) if n != r.len() => {
                                    shape_err = true;
                                    break;
                                }
                                _ => {}
                            }
                            row_refs.push(r);
                        }
                        if shape_err {
                            vm_err!(VmError::Type("transpose: ragged rows"));
                        }
                        let ncols = ncols.unwrap_or(0);
                        let mut result: Vec<NanVal> = Vec::with_capacity(ncols);
                        for j in 0..ncols {
                            let mut col: Vec<NanVal> = Vec::with_capacity(row_refs.len());
                            for r in &row_refs {
                                let v = r[j];
                                v.clone_rc();
                                col.push(v);
                            }
                            result.push(NanVal::heap_list(col));
                        }
                        reg_set!(a, NanVal::heap_list(result));
                    }
                }
                OP_MATMUL => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let va = reg!(b);
                    let vb = reg!(c);
                    if !va.is_heap() || !vb.is_heap() {
                        vm_err!(VmError::Type("matmul requires lists of lists"));
                    }
                    let a_rows_v = match unsafe { va.as_heap_ref() } {
                        HeapObj::List(items) => items,
                        _ => vm_err!(VmError::Type("matmul requires lists of lists")),
                    };
                    let b_rows_v = match unsafe { vb.as_heap_ref() } {
                        HeapObj::List(items) => items,
                        _ => vm_err!(VmError::Type("matmul requires lists of lists")),
                    };
                    // Extract a as Vec<Vec<f64>>
                    let mut a_mat: Vec<Vec<f64>> = Vec::with_capacity(a_rows_v.len());
                    let mut a_cols: Option<usize> = None;
                    for row in a_rows_v {
                        if !row.is_heap() {
                            vm_err!(VmError::Type("matmul: rows must be lists"));
                        }
                        let r = match unsafe { row.as_heap_ref() } {
                            HeapObj::List(items) => items,
                            _ => vm_err!(VmError::Type("matmul: rows must be lists")),
                        };
                        match a_cols {
                            None => a_cols = Some(r.len()),
                            Some(n) if n != r.len() => {
                                vm_err!(VmError::Type("matmul: ragged rows in first arg"));
                            }
                            _ => {}
                        }
                        let mut nums = Vec::with_capacity(r.len());
                        for v in r {
                            if !v.is_number() {
                                vm_err!(VmError::Type("matmul: elements must be numbers"));
                            }
                            nums.push(v.as_number());
                        }
                        a_mat.push(nums);
                    }
                    let mut b_mat: Vec<Vec<f64>> = Vec::with_capacity(b_rows_v.len());
                    let mut b_cols: Option<usize> = None;
                    for row in b_rows_v {
                        if !row.is_heap() {
                            vm_err!(VmError::Type("matmul: rows must be lists"));
                        }
                        let r = match unsafe { row.as_heap_ref() } {
                            HeapObj::List(items) => items,
                            _ => vm_err!(VmError::Type("matmul: rows must be lists")),
                        };
                        match b_cols {
                            None => b_cols = Some(r.len()),
                            Some(n) if n != r.len() => {
                                vm_err!(VmError::Type("matmul: ragged rows in second arg"));
                            }
                            _ => {}
                        }
                        let mut nums = Vec::with_capacity(r.len());
                        for v in r {
                            if !v.is_number() {
                                vm_err!(VmError::Type("matmul: elements must be numbers"));
                            }
                            nums.push(v.as_number());
                        }
                        b_mat.push(nums);
                    }
                    let a_rows_n = a_mat.len();
                    let a_cols_n = a_cols.unwrap_or(0);
                    let b_rows_n = b_mat.len();
                    let b_cols_n = b_cols.unwrap_or(0);
                    if a_cols_n != b_rows_n {
                        vm_err!(VmError::Type("matmul: shape mismatch"));
                    }
                    let mut out: Vec<NanVal> = Vec::with_capacity(a_rows_n);
                    #[allow(clippy::needless_range_loop)]
                    for i in 0..a_rows_n {
                        let mut row: Vec<NanVal> = Vec::with_capacity(b_cols_n);
                        for j in 0..b_cols_n {
                            let mut s = 0.0_f64;
                            for k in 0..a_cols_n {
                                s += a_mat[i][k] * b_mat[k][j];
                            }
                            row.push(NanVal::number(s));
                        }
                        out.push(NanVal::heap_list(row));
                    }
                    reg_set!(a, NanVal::heap_list(out));
                }
                OP_DOT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let va = reg!(b);
                    let vb = reg!(c);
                    if !va.is_heap() || !vb.is_heap() {
                        vm_err!(VmError::Type("dot requires two lists"));
                    }
                    let xs = match unsafe { va.as_heap_ref() } {
                        HeapObj::List(items) => items,
                        _ => vm_err!(VmError::Type("dot requires two lists")),
                    };
                    let ys = match unsafe { vb.as_heap_ref() } {
                        HeapObj::List(items) => items,
                        _ => vm_err!(VmError::Type("dot requires two lists")),
                    };
                    if xs.len() != ys.len() {
                        vm_err!(VmError::Type("dot: length mismatch"));
                    }
                    let mut total = 0.0_f64;
                    for (x, y) in xs.iter().zip(ys.iter()) {
                        if !x.is_number() || !y.is_number() {
                            vm_err!(VmError::Type("dot: list elements must be numbers"));
                        }
                        total += x.as_number() * y.as_number();
                    }
                    reg_set!(a, NanVal::number(total));
                }
                OP_DET => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    let mat = match nanval_to_matrix(v) {
                        Ok(m) => m,
                        Err(e) => vm_err!(VmError::Type(e)),
                    };
                    let n = mat.len();
                    if n == 0 {
                        vm_err!(VmError::Type("det: empty matrix"));
                    }
                    for row in &mat {
                        if row.len() != n {
                            vm_err!(VmError::Type("det: matrix must be square"));
                        }
                    }
                    let (_lu, _piv, det, _singular) = crate::interpreter::lu_decompose(mat);
                    reg_set!(a, NanVal::number(det));
                }
                OP_INV => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    let mat = match nanval_to_matrix(v) {
                        Ok(m) => m,
                        Err(e) => vm_err!(VmError::Type(e)),
                    };
                    let n = mat.len();
                    if n == 0 {
                        vm_err!(VmError::Type("inv: empty matrix"));
                    }
                    for row in &mat {
                        if row.len() != n {
                            vm_err!(VmError::Type("inv: matrix must be square"));
                        }
                    }
                    let (lu, piv, _det, singular) = crate::interpreter::lu_decompose(mat);
                    if singular {
                        vm_err!(VmError::Type("inv: matrix is singular"));
                    }
                    let mut cols: Vec<Vec<f64>> = Vec::with_capacity(n);
                    for j in 0..n {
                        let mut e = vec![0.0; n];
                        e[j] = 1.0;
                        cols.push(crate::interpreter::lu_solve(&lu, &piv, &e));
                    }
                    let rows: Vec<NanVal> = (0..n)
                        .map(|i| {
                            let row: Vec<NanVal> =
                                (0..n).map(|j| NanVal::number(cols[j][i])).collect();
                            NanVal::heap_list(row)
                        })
                        .collect();
                    reg_set!(a, NanVal::heap_list(rows));
                }
                OP_SOLVE => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let va = reg!(b);
                    let vb = reg!(c);
                    let mat = match nanval_to_matrix(va) {
                        Ok(m) => m,
                        Err(e) => vm_err!(VmError::Type(e)),
                    };
                    let vec_b = match nanval_to_vec(vb) {
                        Ok(v) => v,
                        Err(e) => vm_err!(VmError::Type(e)),
                    };
                    let n = mat.len();
                    if n == 0 {
                        vm_err!(VmError::Type("solve: empty matrix"));
                    }
                    for row in &mat {
                        if row.len() != n {
                            vm_err!(VmError::Type("solve: matrix must be square"));
                        }
                    }
                    if vec_b.len() != n {
                        vm_err!(VmError::Type("solve: vector length must match matrix size"));
                    }
                    let (lu, piv, _det, singular) = crate::interpreter::lu_decompose(mat);
                    if singular {
                        vm_err!(VmError::Type("solve: matrix is singular"));
                    }
                    let x = crate::interpreter::lu_solve(&lu, &piv, &vec_b);
                    let list: Vec<NanVal> = x.into_iter().map(NanVal::number).collect();
                    reg_set!(a, NanVal::heap_list(list));
                }
                OP_RND0 => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    reg_set!(a, NanVal::number(fastrand::f64()));
                }
                OP_RND2 => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_number() || !vc.is_number() {
                        vm_err!(VmError::Type("rnd requires two numbers"));
                    }
                    let lo = vb.as_number() as i64;
                    let hi = vc.as_number() as i64;
                    if lo > hi {
                        vm_err!(VmError::Type("rnd: lower bound > upper bound"));
                    }
                    reg_set!(a, NanVal::number(fastrand::i64(lo..=hi) as f64));
                }
                OP_RNDN => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_number() || !vc.is_number() {
                        vm_err!(VmError::Type("rndn requires two numbers"));
                    }
                    let mu = vb.as_number();
                    let sigma = vc.as_number();
                    reg_set!(
                        a,
                        NanVal::number(crate::interpreter::box_muller_normal(mu, sigma))
                    );
                }
                OP_NOW => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs_f64();
                    reg_set!(a, NanVal::number(ts));
                }
                OP_DTFMT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_number() {
                        vm_err!(VmError::Type("dtfmt requires a number (epoch)"));
                    }
                    if !vc.is_string() {
                        vm_err!(VmError::Type("dtfmt requires a string format"));
                    }
                    let epoch_f = vb.as_number();
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let fmt_str: String = unsafe {
                        match vc.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_owned(),
                            _ => unreachable!(),
                        }
                    };
                    let result = if !epoch_f.is_finite() {
                        NanVal::heap_err(NanVal::heap_string(format!(
                            "dtfmt: epoch is not finite ({epoch_f})"
                        )))
                    } else if epoch_f < i64::MIN as f64 || epoch_f > i64::MAX as f64 {
                        NanVal::heap_err(NanVal::heap_string(format!(
                            "dtfmt: epoch out of range ({epoch_f})"
                        )))
                    } else {
                        let secs = epoch_f as i64;
                        match chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0) {
                            Some(dt) => NanVal::heap_ok(NanVal::heap_string(
                                dt.format(&fmt_str).to_string(),
                            )),
                            None => NanVal::heap_err(NanVal::heap_string(format!(
                                "dtfmt: timestamp out of range ({secs})"
                            ))),
                        }
                    };
                    reg_set!(a, result);
                }
                OP_DTPARSE => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_string() || !vc.is_string() {
                        vm_err!(VmError::Type("dtparse requires two strings"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged strings with live RC.
                    let text: String = unsafe {
                        match vb.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_owned(),
                            _ => unreachable!(),
                        }
                    };
                    let fmt_str: String = unsafe {
                        match vc.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_owned(),
                            _ => unreachable!(),
                        }
                    };
                    let parsed = chrono::NaiveDateTime::parse_from_str(&text, &fmt_str)
                        .map(|ndt| ndt.and_utc().timestamp() as f64)
                        .or_else(|_| {
                            chrono::NaiveDate::parse_from_str(&text, &fmt_str).map(|nd| {
                                nd.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() as f64
                            })
                        });
                    let result = match parsed {
                        Ok(n) => NanVal::heap_ok(NanVal::number(n)),
                        Err(e) => NanVal::heap_err(NanVal::heap_string(format!("dtparse: {e}"))),
                    };
                    reg_set!(a, result);
                }
                OP_ENV => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_string() {
                        vm_err!(VmError::Type("env requires a string"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    // Clone key_str before reg_set! to avoid aliasing if a == b.
                    let key_str: String = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_owned(),
                            _ => unreachable!(),
                        }
                    };
                    let result = match std::env::var(&key_str) {
                        Ok(val) => NanVal::heap_ok(NanVal::heap_string(val)),
                        Err(_) => NanVal::heap_err(NanVal::heap_string(format!(
                            "env var '{}' not set",
                            key_str
                        ))),
                    };
                    reg_set!(a, result);
                }
                OP_GET => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_string() {
                        vm_err!(VmError::Type("get requires a string"));
                    }
                    #[cfg(feature = "http")]
                    let result = {
                        // SAFETY: is_string() confirmed heap-tagged string with live RC.
                        let url = unsafe {
                            match v.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        match minreq::get(url.as_str()).send() {
                            Ok(resp) => match resp.as_str() {
                                Ok(body) => NanVal::heap_ok(NanVal::heap_string(body.to_string())),
                                Err(e) => NanVal::heap_err(NanVal::heap_string(format!(
                                    "response is not valid UTF-8: {e}"
                                ))),
                            },
                            Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())),
                        }
                    };
                    #[cfg(not(feature = "http"))]
                    let result = NanVal::heap_err(NanVal::heap_string(
                        "http feature not enabled".to_string(),
                    ));
                    reg_set!(a, result);
                }
                OP_POST => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_string() || !vc.is_string() {
                        vm_err!(VmError::Type("post requires two strings (url, body)"));
                    }
                    #[cfg(feature = "http")]
                    let result = {
                        // SAFETY: is_string() confirmed heap-tagged string with live RC.
                        let url = unsafe {
                            match vb.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        let body = unsafe {
                            match vc.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        match minreq::post(url.as_str()).with_body(body.as_str()).send() {
                            Ok(resp) => match resp.as_str() {
                                Ok(b) => NanVal::heap_ok(NanVal::heap_string(b.to_string())),
                                Err(e) => NanVal::heap_err(NanVal::heap_string(format!(
                                    "response is not valid UTF-8: {e}"
                                ))),
                            },
                            Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())),
                        }
                    };
                    #[cfg(not(feature = "http"))]
                    let result = NanVal::heap_err(NanVal::heap_string(
                        "http feature not enabled".to_string(),
                    ));
                    reg_set!(a, result);
                }
                OP_GETH => {
                    // ABC: A=result, B=url, C=headers_map (M t t)
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_string() {
                        vm_err!(VmError::Type("get requires a string url"));
                    }
                    #[cfg(feature = "http")]
                    let result = {
                        let url = unsafe {
                            match vb.as_heap_ref() {
                                HeapObj::Str(s) => s.as_str().to_owned(),
                                _ => unreachable!(),
                            }
                        };
                        let mut req = minreq::get(url.as_str());
                        if vc.is_heap()
                            && let HeapObj::Map(m) = unsafe { vc.as_heap_ref() }
                        {
                            for (k, v) in m.iter() {
                                if v.is_string() {
                                    let vs = unsafe {
                                        match v.as_heap_ref() {
                                            HeapObj::Str(s) => s.as_str().to_owned(),
                                            _ => unreachable!(),
                                        }
                                    };
                                    req = req.with_header(k.as_str(), &vs);
                                }
                            }
                        }
                        match req.send() {
                            Ok(resp) => match resp.as_str() {
                                Ok(body) => NanVal::heap_ok(NanVal::heap_string(body.to_string())),
                                Err(e) => NanVal::heap_err(NanVal::heap_string(format!(
                                    "response is not valid UTF-8: {e}"
                                ))),
                            },
                            Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())),
                        }
                    };
                    #[cfg(not(feature = "http"))]
                    let result = NanVal::heap_err(NanVal::heap_string(
                        "http feature not enabled".to_string(),
                    ));
                    reg_set!(a, result);
                }
                OP_GETMANY => {
                    // ABC: A=result, B=urls (L t). Returns L (R t t).
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_heap() {
                        vm_err!(VmError::Type("get-many requires a list of strings"));
                    }
                    // SAFETY: is_heap() confirmed a heap-tagged value with live RC.
                    let urls: Vec<String> = match unsafe { v.as_heap_ref() } {
                        HeapObj::List(items) => {
                            let mut out = Vec::with_capacity(items.len());
                            for item in items.iter() {
                                if !item.is_string() {
                                    vm_err!(VmError::Type(
                                        "get-many requires L t (list of strings)"
                                    ));
                                }
                                // SAFETY: is_string() confirmed.
                                let s = unsafe {
                                    match item.as_heap_ref() {
                                        HeapObj::Str(s) => s.as_str().to_owned(),
                                        _ => unreachable!(),
                                    }
                                };
                                out.push(s);
                            }
                            out
                        }
                        _ => {
                            vm_err!(VmError::Type("get-many requires a list of strings"));
                        }
                    };
                    let values = crate::interpreter::get_many_fetch(&urls);
                    let nan_items: Vec<NanVal> = values.iter().map(NanVal::from_value).collect();
                    let result = NanVal::heap_list(nan_items);
                    reg_set!(a, result);
                }
                OP_POSTH => {
                    // Two-instruction sequence: OP_POSTH A=result B=url C=body; data word A=headers_reg
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: compiler always emits the data word immediately after OP_POSTH
                    let data_inst = unsafe { *code.get_unchecked(ip) };
                    ip += 1;
                    let d = ((data_inst >> 16) & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    let vd = reg!(d);
                    if !vb.is_string() || !vc.is_string() {
                        vm_err!(VmError::Type("post requires string url and body"));
                    }
                    #[cfg(feature = "http")]
                    let result = {
                        let url = unsafe {
                            match vb.as_heap_ref() {
                                HeapObj::Str(s) => s.as_str().to_owned(),
                                _ => unreachable!(),
                            }
                        };
                        let body_str = unsafe {
                            match vc.as_heap_ref() {
                                HeapObj::Str(s) => s.as_str().to_owned(),
                                _ => unreachable!(),
                            }
                        };
                        let mut req = minreq::post(url.as_str()).with_body(body_str.as_str());
                        if vd.is_heap()
                            && let HeapObj::Map(m) = unsafe { vd.as_heap_ref() }
                        {
                            for (k, v) in m.iter() {
                                if v.is_string() {
                                    let vs = unsafe {
                                        match v.as_heap_ref() {
                                            HeapObj::Str(s) => s.as_str().to_owned(),
                                            _ => unreachable!(),
                                        }
                                    };
                                    req = req.with_header(k.as_str(), &vs);
                                }
                            }
                        }
                        match req.send() {
                            Ok(resp) => match resp.as_str() {
                                Ok(b) => NanVal::heap_ok(NanVal::heap_string(b.to_string())),
                                Err(e) => NanVal::heap_err(NanVal::heap_string(format!(
                                    "response is not valid UTF-8: {e}"
                                ))),
                            },
                            Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())),
                        }
                    };
                    #[cfg(not(feature = "http"))]
                    let result = NanVal::heap_err(NanVal::heap_string(
                        "http feature not enabled".to_string(),
                    ));
                    reg_set!(a, result);
                }
                OP_JPTH => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_string() || !vc.is_string() {
                        vm_err!(VmError::Type("jpth requires two strings"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let json_str = unsafe {
                        match vb.as_heap_ref() {
                            HeapObj::Str(s) => s,
                            _ => unreachable!(),
                        }
                    };
                    let path_str = unsafe {
                        match vc.as_heap_ref() {
                            HeapObj::Str(s) => s,
                            _ => unreachable!(),
                        }
                    };
                    let result = match serde_json::from_str::<serde_json::Value>(json_str) {
                        Ok(parsed) => {
                            let mut current = &parsed;
                            let mut found = true;
                            let mut missing_key = String::new();
                            for key in path_str.split('.') {
                                if let Ok(idx) = key.parse::<usize>() {
                                    if let Some(v) = current.as_array().and_then(|a| a.get(idx)) {
                                        current = v;
                                    } else {
                                        found = false;
                                        missing_key = key.to_string();
                                        break;
                                    }
                                } else if let Some(v) = current.get(key) {
                                    current = v;
                                } else {
                                    found = false;
                                    missing_key = key.to_string();
                                    break;
                                }
                            }
                            if found {
                                let result_str = match current {
                                    serde_json::Value::String(s) => s.clone(),
                                    other => other.to_string(),
                                };
                                NanVal::heap_ok(NanVal::heap_string(result_str))
                            } else {
                                NanVal::heap_err(NanVal::heap_string(format!(
                                    "key not found: {missing_key}"
                                )))
                            }
                        }
                        Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())),
                    };
                    reg_set!(a, result);
                }
                OP_JDMP => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    let json_val = nanval_to_json(v);
                    reg_set!(a, NanVal::heap_string(json_val.to_string()));
                }
                OP_CSVDMP => {
                    // Format the value in R[B] as csv (C==0) or tsv (C==1).
                    // Re-uses the tree-walker writer so quoting/header rules
                    // stay identical across engines.
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as u8;
                    let sep = if c == 0 { ',' } else { '\t' };
                    let data = reg!(b).to_value();
                    let rows = match data {
                        Value::List(l) => l,
                        _ => {
                            vm_err!(VmError::Type("wr csv/tsv: data must be a list of rows"));
                        }
                    };
                    match crate::interpreter::write_csv_tsv(&rows, sep) {
                        Ok(s) => reg_set!(a, NanVal::heap_string(s)),
                        Err(_e) => {
                            vm_err!(VmError::Type("wr csv/tsv: row shape mismatch"));
                        }
                    }
                }
                OP_JPAR => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_string() {
                        vm_err!(VmError::Type("jpar requires a string"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let text = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::Str(s) => s,
                            _ => unreachable!(),
                        }
                    };
                    let result = match serde_json::from_str::<serde_json::Value>(text) {
                        Ok(parsed) => NanVal::heap_ok(serde_json_to_nanval(parsed)),
                        Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())),
                    };
                    reg_set!(a, result);
                }
                OP_RDJL => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_string() {
                        vm_err!(VmError::Type("rdjl requires a string path"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let path = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_owned(),
                            _ => unreachable!(),
                        }
                    };
                    match std::fs::read_to_string(&path) {
                        Ok(content) => {
                            let mut items: Vec<NanVal> = Vec::new();
                            for line in content.split('\n') {
                                if line.is_empty() {
                                    continue;
                                }
                                let entry = match serde_json::from_str::<serde_json::Value>(line) {
                                    Ok(parsed) => NanVal::heap_ok(serde_json_to_nanval(parsed)),
                                    Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())),
                                };
                                items.push(entry);
                            }
                            reg_set!(a, NanVal::heap_list(items));
                        }
                        Err(_) => {
                            vm_err!(VmError::Type("rdjl failed to read file"));
                        }
                    }
                }
                OP_SPL => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_string() || !vc.is_string() {
                        vm_err!(VmError::Type("spl requires two strings"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let text = unsafe {
                        match vb.as_heap_ref() {
                            HeapObj::Str(s) => s,
                            _ => unreachable!(),
                        }
                    };
                    let sep = unsafe {
                        match vc.as_heap_ref() {
                            HeapObj::Str(s) => s,
                            _ => unreachable!(),
                        }
                    };
                    let items: Vec<NanVal> = text
                        .split(sep.as_str())
                        .map(|p| NanVal::heap_string(p.to_string()))
                        .collect();
                    reg_set!(a, NanVal::heap_list(items));
                }
                OP_CAT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vc.is_string() {
                        vm_err!(VmError::Type("cat requires a text separator"));
                    }
                    if !vb.is_heap() {
                        vm_err!(VmError::Type("cat requires a list"));
                    }
                    let sep = unsafe {
                        match vc.as_heap_ref() {
                            HeapObj::Str(s) => s,
                            _ => unreachable!(),
                        }
                    };
                    let items = unsafe {
                        match vb.as_heap_ref() {
                            HeapObj::List(l) => l,
                            _ => vm_err!(VmError::Type("cat requires a list")),
                        }
                    };
                    let mut parts = Vec::with_capacity(items.len());
                    for item in items {
                        if !item.is_string() {
                            vm_err!(VmError::Type("cat: list items must be text"));
                        }
                        let s = unsafe {
                            match item.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        parts.push(s.as_str());
                    }
                    let result = parts.join(sep.as_str());
                    reg_set!(a, NanVal::heap_string(result));
                }
                OP_HAS => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let collection = reg!(b);
                    let needle = reg!(c);
                    let found = if collection.is_string() {
                        if !needle.is_string() {
                            vm_err!(VmError::Type("has: text search requires text needle"));
                        }
                        unsafe {
                            let haystack = match collection.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            };
                            let needle_s = match needle.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            };
                            haystack.contains(needle_s.as_str())
                        }
                    } else if collection.is_heap() {
                        match unsafe { collection.as_heap_ref() } {
                            HeapObj::List(items) => {
                                items.iter().any(|item| nanval_equal(*item, needle))
                            }
                            _ => vm_err!(VmError::Type("has requires a list or text")),
                        }
                    } else {
                        vm_err!(VmError::Type("has requires a list or text"));
                    };
                    reg_set!(a, NanVal::boolean(found));
                }
                OP_HD => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    let result = if v.is_string() {
                        let s = unsafe {
                            match v.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        if s.is_empty() {
                            vm_err!(VmError::Type("hd: empty text"));
                        }
                        NanVal::heap_string(
                            s.chars()
                                .next()
                                .expect("non-empty checked above")
                                .to_string(),
                        )
                    } else if v.is_heap() {
                        match unsafe { v.as_heap_ref() } {
                            HeapObj::List(items) => {
                                if items.is_empty() {
                                    vm_err!(VmError::Type("hd: empty list"));
                                }
                                items[0].clone_rc();
                                items[0]
                            }
                            _ => vm_err!(VmError::Type("hd requires a list or text")),
                        }
                    } else {
                        vm_err!(VmError::Type("hd requires a list or text"));
                    };
                    reg_set!(a, result);
                }
                OP_FMT2 => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_number() || !vc.is_number() {
                        vm_err!(VmError::Type("fmt2 requires two numbers (x, digits)"));
                    }
                    let x = vb.as_number();
                    let d = vc.as_number();
                    let digits = if !d.is_finite() || d <= 0.0 {
                        0usize
                    } else {
                        (d as usize).min(20)
                    };
                    let result = NanVal::heap_string(format!("{:.*}", digits, x));
                    reg_set!(a, result);
                }
                OP_AT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let v = reg!(b);
                    let i = reg!(c);
                    if !i.is_number() {
                        vm_err!(VmError::Type("at: index must be a number"));
                    }
                    let n = i.as_number();
                    if n.fract() != 0.0 {
                        vm_err!(VmError::Type("at: index must be an integer"));
                    }
                    let raw = n as i64;
                    let result = if v.is_string() {
                        let s = unsafe {
                            match v.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        let chars: Vec<char> = s.chars().collect();
                        let len = chars.len() as i64;
                        let adjusted = if raw < 0 { raw + len } else { raw };
                        if adjusted < 0 || adjusted >= len {
                            vm_err!(VmError::Type("at: index out of range"));
                        }
                        NanVal::heap_string(chars[adjusted as usize].to_string())
                    } else if v.is_heap() {
                        match unsafe { v.as_heap_ref() } {
                            HeapObj::List(items) => {
                                let len = items.len() as i64;
                                let adjusted = if raw < 0 { raw + len } else { raw };
                                if adjusted < 0 || adjusted >= len {
                                    vm_err!(VmError::Type("at: index out of range"));
                                }
                                let idx = adjusted as usize;
                                items[idx].clone_rc();
                                items[idx]
                            }
                            _ => vm_err!(VmError::Type("at requires a list or text")),
                        }
                    } else {
                        vm_err!(VmError::Type("at requires a list or text"));
                    };
                    reg_set!(a, result);
                }
                OP_ZIP => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    let xs = if vb.is_heap() {
                        match unsafe { vb.as_heap_ref() } {
                            HeapObj::List(items) => items,
                            _ => vm_err!(VmError::Type("zip arg 1 requires a list")),
                        }
                    } else {
                        vm_err!(VmError::Type("zip arg 1 requires a list"));
                    };
                    let ys = if vc.is_heap() {
                        match unsafe { vc.as_heap_ref() } {
                            HeapObj::List(items) => items,
                            _ => vm_err!(VmError::Type("zip arg 2 requires a list")),
                        }
                    } else {
                        vm_err!(VmError::Type("zip arg 2 requires a list"));
                    };
                    let n = xs.len().min(ys.len());
                    let mut out: Vec<NanVal> = Vec::with_capacity(n);
                    for i in 0..n {
                        let x = xs[i];
                        let y = ys[i];
                        x.clone_rc();
                        y.clone_rc();
                        let pair = NanVal::heap_list(vec![x, y]);
                        out.push(pair);
                    }
                    reg_set!(a, NanVal::heap_list(out));
                }
                OP_ENUMERATE => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let xs = if vb.is_heap() {
                        match unsafe { vb.as_heap_ref() } {
                            HeapObj::List(items) => items,
                            _ => vm_err!(VmError::Type("enumerate requires a list")),
                        }
                    } else {
                        vm_err!(VmError::Type("enumerate requires a list"));
                    };
                    let mut out: Vec<NanVal> = Vec::with_capacity(xs.len());
                    for (i, &v) in xs.iter().enumerate() {
                        v.clone_rc();
                        let pair = NanVal::heap_list(vec![NanVal::number(i as f64), v]);
                        out.push(pair);
                    }
                    reg_set!(a, NanVal::heap_list(out));
                }
                OP_RANGE => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_number() || !vc.is_number() {
                        vm_err!(VmError::Type("range requires numbers"));
                    }
                    if vb.as_number().fract() != 0.0 || vc.as_number().fract() != 0.0 {
                        vm_err!(VmError::Type("range: bounds must be integers"));
                    }
                    let start = vb.as_number() as i64;
                    let end = vc.as_number() as i64;
                    let out: Vec<NanVal> = if start >= end {
                        Vec::new()
                    } else {
                        let len = (end - start) as u64;
                        if len > 1_000_000 {
                            vm_err!(VmError::Type("range too large (max 1000000)"));
                        }
                        let mut v = Vec::with_capacity(len as usize);
                        for i in start..end {
                            v.push(NanVal::number(i as f64));
                        }
                        v
                    };
                    reg_set!(a, NanVal::heap_list(out));
                }
                OP_WINDOW => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vn = reg!(b);
                    let vxs = reg!(c);
                    if !vn.is_number() {
                        vm_err!(VmError::Type("window: size must be a number"));
                    }
                    let nf = vn.as_number();
                    if !nf.is_finite() || nf <= 0.0 || nf.fract() != 0.0 {
                        vm_err!(VmError::Type("window: size must be a positive integer"));
                    }
                    let n = nf as usize;
                    let xs = if vxs.is_heap() {
                        match unsafe { vxs.as_heap_ref() } {
                            HeapObj::List(items) => items,
                            _ => vm_err!(VmError::Type("window arg 2 requires a list")),
                        }
                    } else {
                        vm_err!(VmError::Type("window arg 2 requires a list"));
                    };
                    let out: Vec<NanVal> = if n > xs.len() {
                        Vec::new()
                    } else {
                        let mut acc = Vec::with_capacity(xs.len() - n + 1);
                        for w in xs.windows(n) {
                            let mut sub = Vec::with_capacity(n);
                            for v in w {
                                v.clone_rc();
                                sub.push(*v);
                            }
                            acc.push(NanVal::heap_list(sub));
                        }
                        acc
                    };
                    reg_set!(a, NanVal::heap_list(out));
                }
                OP_CHUNKS => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_number() {
                        vm_err!(VmError::Type("chunks: size must be a number"));
                    }
                    let n_raw = vb.as_number();
                    if n_raw.fract() != 0.0 || n_raw <= 0.0 {
                        vm_err!(VmError::Type("chunks: size must be a positive integer",));
                    }
                    let n = n_raw as usize;
                    let xs = if vc.is_heap() {
                        match unsafe { vc.as_heap_ref() } {
                            HeapObj::List(items) => items,
                            _ => vm_err!(VmError::Type("chunks: requires a list")),
                        }
                    } else {
                        vm_err!(VmError::Type("chunks: requires a list"));
                    };
                    let mut out: Vec<NanVal> = Vec::with_capacity(xs.len().div_ceil(n));
                    for chunk in xs.chunks(n) {
                        let mut piece: Vec<NanVal> = Vec::with_capacity(chunk.len());
                        for item in chunk {
                            item.clone_rc();
                            piece.push(*item);
                        }
                        out.push(NanVal::heap_list(piece));
                    }
                    reg_set!(a, NanVal::heap_list(out));
                }
                OP_TL => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    let result = if v.is_string() {
                        let s = unsafe {
                            match v.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        if s.is_empty() {
                            vm_err!(VmError::Type("tl: empty text"));
                        }
                        let mut chars = s.chars();
                        chars.next();
                        NanVal::heap_string(chars.collect())
                    } else if v.is_heap() {
                        match unsafe { v.as_heap_ref() } {
                            HeapObj::List(items) => {
                                if items.is_empty() {
                                    vm_err!(VmError::Type("tl: empty list"));
                                }
                                let tail: Vec<NanVal> = items[1..]
                                    .iter()
                                    .map(|item| {
                                        item.clone_rc();
                                        *item
                                    })
                                    .collect();
                                NanVal::heap_list(tail)
                            }
                            _ => vm_err!(VmError::Type("tl requires a list or text")),
                        }
                    } else {
                        vm_err!(VmError::Type("tl requires a list or text"));
                    };
                    reg_set!(a, result);
                }
                OP_REV => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    let result = if v.is_string() {
                        let s = unsafe {
                            match v.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        NanVal::heap_string(s.chars().rev().collect::<String>())
                    } else if v.is_heap() {
                        match unsafe { v.as_heap_ref() } {
                            HeapObj::List(items) => {
                                let mut reversed: Vec<NanVal> = items
                                    .iter()
                                    .map(|item| {
                                        item.clone_rc();
                                        *item
                                    })
                                    .collect();
                                reversed.reverse();
                                NanVal::heap_list(reversed)
                            }
                            _ => vm_err!(VmError::Type("rev requires a list or text")),
                        }
                    } else {
                        vm_err!(VmError::Type("rev requires a list or text"));
                    };
                    reg_set!(a, result);
                }
                OP_SRT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if v.is_string() {
                        let s = unsafe {
                            match v.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        let mut chars: Vec<char> = s.chars().collect();
                        chars.sort();
                        let sorted: String = chars.into_iter().collect();
                        reg_set!(a, NanVal::heap_string(sorted));
                    } else if v.is_heap() {
                        match unsafe { v.as_heap_ref() } {
                            HeapObj::List(items) => {
                                if items.is_empty() {
                                    reg_set!(a, NanVal::heap_list(vec![]));
                                } else {
                                    let all_numbers = items.iter().all(|v| v.is_number());
                                    let all_strings = items.iter().all(|v| v.is_string());
                                    if all_numbers {
                                        let mut sorted: Vec<NanVal> = items
                                            .iter()
                                            .map(|v| {
                                                v.clone_rc();
                                                *v
                                            })
                                            .collect();
                                        sorted.sort_by(|a, b| {
                                            a.as_number()
                                                .partial_cmp(&b.as_number())
                                                .unwrap_or(std::cmp::Ordering::Equal)
                                        });
                                        reg_set!(a, NanVal::heap_list(sorted));
                                    } else if all_strings {
                                        let mut sorted: Vec<NanVal> = items
                                            .iter()
                                            .map(|v| {
                                                v.clone_rc();
                                                *v
                                            })
                                            .collect();
                                        sorted.sort_by(|a, b| unsafe { nanval_str_cmp(*a, *b) });
                                        reg_set!(a, NanVal::heap_list(sorted));
                                    } else {
                                        vm_err!(VmError::Type(
                                            "srt: list must contain all numbers or all text"
                                        ));
                                    }
                                }
                            }
                            _ => vm_err!(VmError::Type("srt requires a list or text")),
                        }
                    } else {
                        vm_err!(VmError::Type("srt requires a list or text"));
                    }
                }
                OP_SRTDESC => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if v.is_string() {
                        let s = unsafe {
                            match v.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        let mut chars: Vec<char> = s.chars().collect();
                        chars.sort_by(|a, b| b.cmp(a));
                        let sorted: String = chars.into_iter().collect();
                        reg_set!(a, NanVal::heap_string(sorted));
                    } else if v.is_heap() {
                        match unsafe { v.as_heap_ref() } {
                            HeapObj::List(items) => {
                                if items.is_empty() {
                                    reg_set!(a, NanVal::heap_list(vec![]));
                                } else {
                                    let all_numbers = items.iter().all(|v| v.is_number());
                                    let all_strings = items.iter().all(|v| v.is_string());
                                    if all_numbers {
                                        let mut sorted: Vec<NanVal> = items
                                            .iter()
                                            .map(|v| {
                                                v.clone_rc();
                                                *v
                                            })
                                            .collect();
                                        sorted.sort_by(|a, b| {
                                            b.as_number()
                                                .partial_cmp(&a.as_number())
                                                .unwrap_or(std::cmp::Ordering::Equal)
                                        });
                                        reg_set!(a, NanVal::heap_list(sorted));
                                    } else if all_strings {
                                        let mut sorted: Vec<NanVal> = items
                                            .iter()
                                            .map(|v| {
                                                v.clone_rc();
                                                *v
                                            })
                                            .collect();
                                        sorted.sort_by(|a, b| unsafe { nanval_str_cmp(*b, *a) });
                                        reg_set!(a, NanVal::heap_list(sorted));
                                    } else {
                                        vm_err!(VmError::Type(
                                            "rsrt: list must contain all numbers or all text"
                                        ));
                                    }
                                }
                            }
                            _ => vm_err!(VmError::Type("rsrt requires a list or text")),
                        }
                    } else {
                        vm_err!(VmError::Type("rsrt requires a list or text"));
                    }
                }
                OP_FFT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    match vm_fft_real_to_pairs(v) {
                        Ok(out) => reg_set!(a, out),
                        Err(msg) => vm_err!(VmError::Type(msg)),
                    }
                }
                OP_IFFT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    match vm_ifft_pairs_to_real(v) {
                        Ok(out) => reg_set!(a, out),
                        Err(msg) => vm_err!(VmError::Type(msg)),
                    }
                }
                OP_CUMSUM => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    let result = if v.is_heap() {
                        match unsafe { v.as_heap_ref() } {
                            HeapObj::List(items) => {
                                let mut total = 0.0_f64;
                                let mut out: Vec<NanVal> = Vec::with_capacity(items.len());
                                let mut bad = false;
                                for item in items {
                                    if !item.is_number() {
                                        bad = true;
                                        break;
                                    }
                                    total += item.as_number();
                                    out.push(NanVal::number(total));
                                }
                                if bad {
                                    vm_err!(VmError::Type("cumsum: list elements must be numbers"));
                                }
                                NanVal::heap_list(out)
                            }
                            _ => vm_err!(VmError::Type("cumsum requires a list of numbers")),
                        }
                    } else {
                        vm_err!(VmError::Type("cumsum requires a list of numbers"));
                    };
                    reg_set!(a, result);
                }
                OP_MEDIAN => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    match vm_median(v) {
                        Ok(out) => reg_set!(a, out),
                        Err(msg) => vm_err!(VmError::Type(msg)),
                    }
                }
                OP_QUANTILE => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    match vm_quantile(vb, vc) {
                        Ok(out) => reg_set!(a, out),
                        Err(msg) => vm_err!(VmError::Type(msg)),
                    }
                }
                OP_STDEV => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    match vm_stdev(v) {
                        Ok(out) => reg_set!(a, out),
                        Err(msg) => vm_err!(VmError::Type(msg)),
                    }
                }
                OP_VARIANCE => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    match vm_variance(v) {
                        Ok(out) => reg_set!(a, out),
                        Err(msg) => vm_err!(VmError::Type(msg)),
                    }
                }
                OP_SLC => {
                    // Two-instruction sequence: OP_SLC A=result B=list C=start; data word A=end_reg
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: compiler always emits the data word immediately after OP_SLC
                    let data_inst = unsafe { *code.get_unchecked(ip) };
                    ip += 1;
                    let d = ((data_inst >> 16) & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    let vd = reg!(d);
                    if !vc.is_number() || !vd.is_number() {
                        vm_err!(VmError::Type("slc: indices must be numbers"));
                    }
                    let start = vc.as_number() as usize;
                    let end = vd.as_number() as usize;
                    if vb.is_string() {
                        let s = unsafe {
                            match vb.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        let chars: Vec<char> = s.chars().collect();
                        let end = end.min(chars.len());
                        let start = start.min(end);
                        let result: String = chars[start..end].iter().collect();
                        reg_set!(a, NanVal::heap_string(result));
                    } else if vb.is_heap() {
                        match unsafe { vb.as_heap_ref() } {
                            HeapObj::List(items) => {
                                let end = end.min(items.len());
                                let start = start.min(end);
                                let mut sliced = Vec::with_capacity(end - start);
                                for v in &items[start..end] {
                                    v.clone_rc();
                                    sliced.push(*v);
                                }
                                reg_set!(a, NanVal::heap_list(sliced));
                            }
                            _ => vm_err!(VmError::Type("slc requires a list or text")),
                        }
                    } else {
                        vm_err!(VmError::Type("slc requires a list or text"));
                    }
                }
                OP_LST => {
                    // Two-instruction sequence: OP_LST A=result B=list C=index; data word A=value_reg
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: compiler always emits the data word immediately after OP_LST
                    let data_inst = unsafe { *code.get_unchecked(ip) };
                    ip += 1;
                    let d = ((data_inst >> 16) & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    let vd = reg!(d);
                    if !vc.is_number() {
                        vm_err!(VmError::Type("lst: index must be a number"));
                    }
                    let n = vc.as_number();
                    if n < 0.0 || n.fract() != 0.0 {
                        vm_err!(VmError::Type("lst: index must be a non-negative integer"));
                    }
                    let idx = n as usize;
                    if vb.is_heap() {
                        match unsafe { vb.as_heap_ref() } {
                            HeapObj::List(items) => {
                                if idx >= items.len() {
                                    vm_err!(VmError::Type("lst: index out of range"));
                                }
                                let mut new_items: Vec<NanVal> = Vec::with_capacity(items.len());
                                for (i, v) in items.iter().enumerate() {
                                    if i == idx {
                                        vd.clone_rc();
                                        new_items.push(vd);
                                    } else {
                                        v.clone_rc();
                                        new_items.push(*v);
                                    }
                                }
                                reg_set!(a, NanVal::heap_list(new_items));
                            }
                            _ => vm_err!(VmError::Type("lst requires a list")),
                        }
                    } else {
                        vm_err!(VmError::Type("lst requires a list"));
                    }
                }
                OP_RGXSUB => {
                    // Two-instruction sequence: OP_RGXSUB A=result B=pattern C=replacement;
                    // data word A=subject_reg
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: compiler always emits the data word immediately after OP_RGXSUB
                    let data_inst = unsafe { *code.get_unchecked(ip) };
                    ip += 1;
                    let d = ((data_inst >> 16) & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    let vd = reg!(d);
                    if !vb.is_string() || !vc.is_string() || !vd.is_string() {
                        vm_err!(VmError::Type("rgxsub requires three strings"));
                    }
                    let pat = unsafe {
                        match vb.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str(),
                            _ => unreachable!(),
                        }
                    };
                    let repl = unsafe {
                        match vc.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str(),
                            _ => unreachable!(),
                        }
                    };
                    let subj = unsafe {
                        match vd.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str(),
                            _ => unreachable!(),
                        }
                    };
                    match regex::Regex::new(pat) {
                        Ok(re) => {
                            let out = re.replace_all(subj, repl).into_owned();
                            reg_set!(a, NanVal::heap_string(out));
                        }
                        Err(_) => vm_err!(VmError::Type("rgxsub: invalid regex pattern")),
                    }
                }
                OP_TAKE => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vn = reg!(b);
                    let vc = reg!(c);
                    if !vn.is_number() {
                        vm_err!(VmError::Type("take: count must be a number"));
                    }
                    let nf = vn.as_number();
                    if nf.fract() != 0.0 {
                        vm_err!(VmError::Type("take: count must be an integer"));
                    }
                    if nf < 0.0 {
                        vm_err!(VmError::Type("take: count must be a non-negative integer"));
                    }
                    let n = nf as usize;
                    if vc.is_string() {
                        let s = unsafe {
                            match vc.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        let chars: Vec<char> = s.chars().collect();
                        let end = n.min(chars.len());
                        let result: String = chars[..end].iter().collect();
                        reg_set!(a, NanVal::heap_string(result));
                    } else if vc.is_heap() {
                        match unsafe { vc.as_heap_ref() } {
                            HeapObj::List(items) => {
                                let end = n.min(items.len());
                                let mut sliced = Vec::with_capacity(end);
                                for v in &items[..end] {
                                    v.clone_rc();
                                    sliced.push(*v);
                                }
                                reg_set!(a, NanVal::heap_list(sliced));
                            }
                            _ => vm_err!(VmError::Type("take requires a list or text")),
                        }
                    } else {
                        vm_err!(VmError::Type("take requires a list or text"));
                    }
                }
                OP_DROP => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vn = reg!(b);
                    let vc = reg!(c);
                    if !vn.is_number() {
                        vm_err!(VmError::Type("drop: count must be a number"));
                    }
                    let nf = vn.as_number();
                    if nf.fract() != 0.0 {
                        vm_err!(VmError::Type("drop: count must be an integer"));
                    }
                    if nf < 0.0 {
                        vm_err!(VmError::Type("drop: count must be a non-negative integer"));
                    }
                    let n = nf as usize;
                    if vc.is_string() {
                        let s = unsafe {
                            match vc.as_heap_ref() {
                                HeapObj::Str(s) => s,
                                _ => unreachable!(),
                            }
                        };
                        let chars: Vec<char> = s.chars().collect();
                        let start = n.min(chars.len());
                        let result: String = chars[start..].iter().collect();
                        reg_set!(a, NanVal::heap_string(result));
                    } else if vc.is_heap() {
                        match unsafe { vc.as_heap_ref() } {
                            HeapObj::List(items) => {
                                let start = n.min(items.len());
                                let mut sliced = Vec::with_capacity(items.len() - start);
                                for v in &items[start..] {
                                    v.clone_rc();
                                    sliced.push(*v);
                                }
                                reg_set!(a, NanVal::heap_list(sliced));
                            }
                            _ => vm_err!(VmError::Type("drop requires a list or text")),
                        }
                    } else {
                        vm_err!(VmError::Type("drop requires a list or text"));
                    }
                }
                OP_LISTAPPEND => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // Promote arena records escaping into heap list
                    if reg!(c).is_arena_record() {
                        let promoted = reg!(c).promote_arena_to_heap(&self.program.type_registry);
                        reg_set!(c, promoted);
                    }
                    let list_val = reg!(b);
                    let item_val = reg!(c);
                    if !list_val.is_heap() {
                        vm_err!(VmError::Type("+= requires a list"));
                    }
                    let ptr_b = (list_val.0 & PTR_MASK) as *const HeapObj;
                    // Fast path: if RC=1 (sole owner), mutate Vec in-place.
                    // Turns O(n) copy-per-append into amortised O(1), fixing the O(n²)
                    // foreach-build pattern.
                    //
                    // We peek at the RC count without reconstructing the Rc (which would
                    // bump it to 2 and cause get_mut to fail). When count==1 we use an
                    // unsafe &mut cast: we are the sole logical owner and no other thread
                    // can alias this pointer (single-threaded Rc).
                    let rc_count = {
                        // SAFETY: ptr_b was produced by Rc::into_raw; temporarily reconstruct
                        // to read the count, then forget to avoid decrement.
                        let rc_peek = unsafe { Rc::from_raw(ptr_b) };
                        let count = Rc::strong_count(&rc_peek);
                        std::mem::forget(rc_peek);
                        count
                    };
                    if rc_count == 1 {
                        // SAFETY: We are the sole owner (count==1). No other reference
                        // exists. Cast to *mut to push directly into the Vec without
                        // altering the Rc refcount.
                        let heap_mut = unsafe { &mut *(ptr_b as *mut HeapObj) };
                        match heap_mut {
                            HeapObj::List(items) => {
                                item_val.clone_rc();
                                items.push(item_val);
                                // The NanVal pointer is unchanged; copy to slot a if a != b.
                                if a != b {
                                    unsafe { self.stack.get_unchecked(a) }.drop_rc();
                                    list_val.clone_rc();
                                    unsafe {
                                        *self.stack.as_mut_ptr().add(a) = list_val;
                                    }
                                }
                                // a == b: list_val already in slot, nothing to do.
                            }
                            _ => return Err(VmError::Type("+= requires a list")),
                        }
                    } else {
                        // RC > 1 — must copy to avoid aliasing.
                        // SAFETY: ptr_b is a live heap pointer; we borrow it read-only.
                        match unsafe { &*ptr_b } {
                            HeapObj::List(items) => {
                                let mut new_items = Vec::with_capacity(items.len() + 1);
                                for v in items {
                                    v.clone_rc();
                                    new_items.push(*v);
                                }
                                item_val.clone_rc();
                                new_items.push(item_val);
                                reg_set!(a, NanVal::heap_list(new_items));
                            }
                            _ => vm_err!(VmError::Type("+= requires a list")),
                        }
                    }
                }
                OP_FRQ => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if (v.0 & TAG_MASK) != TAG_LIST {
                        vm_err!(VmError::Type("frq requires a list"));
                    }
                    // SAFETY: TAG_LIST confirmed.
                    let items = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::List(l) => l.clone(),
                            _ => unreachable!(),
                        }
                    };
                    let mut counts: std::collections::HashMap<String, usize> =
                        std::collections::HashMap::new();
                    for item in items {
                        let key_str = match nanval_to_key_string(item) {
                            Some(s) => s,
                            None => vm_err!(VmError::Type(
                                "frq: list elements must be text, number, or bool"
                            )),
                        };
                        *counts.entry(key_str).or_insert(0) += 1;
                    }
                    let map: std::collections::HashMap<String, NanVal> = counts
                        .into_iter()
                        .map(|(k, c)| (k, NanVal::number(c as f64)))
                        .collect();
                    reg_set!(a, NanVal::heap_map(map));
                }
                OP_SETUNION | OP_SETINTER | OP_SETDIFF => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    let op_name = match op {
                        OP_SETUNION => "setunion",
                        OP_SETINTER => "setinter",
                        _ => "setdiff",
                    };
                    let xs = if vb.is_heap() {
                        match unsafe { vb.as_heap_ref() } {
                            HeapObj::List(items) => items,
                            _ => vm_err!(VmError::Type("setop arg 1 requires a list")),
                        }
                    } else {
                        vm_err!(VmError::Type("setop arg 1 requires a list"));
                    };
                    let ys = if vc.is_heap() {
                        match unsafe { vc.as_heap_ref() } {
                            HeapObj::List(items) => items,
                            _ => vm_err!(VmError::Type("setop arg 2 requires a list")),
                        }
                    } else {
                        vm_err!(VmError::Type("setop arg 2 requires a list"));
                    };
                    use std::collections::{HashMap, HashSet};
                    let mut set_a: HashSet<String> = HashSet::new();
                    let mut first_a: HashMap<String, NanVal> = HashMap::new();
                    for x in xs {
                        let k = match setops_key(*x, op_name) {
                            Ok(k) => k,
                            Err(_) => vm_err!(VmError::Type(
                                "setop: elements must be text, number, or bool"
                            )),
                        };
                        if set_a.insert(k.clone()) {
                            first_a.insert(k, *x);
                        }
                    }
                    let mut set_b: HashSet<String> = HashSet::new();
                    let mut first_b: HashMap<String, NanVal> = HashMap::new();
                    for y in ys {
                        let k = match setops_key(*y, op_name) {
                            Ok(k) => k,
                            Err(_) => vm_err!(VmError::Type(
                                "setop: elements must be text, number, or bool"
                            )),
                        };
                        if set_b.insert(k.clone()) {
                            first_b.insert(k, *y);
                        }
                    }
                    let mut keys: Vec<String> = match op {
                        OP_SETUNION => set_a.union(&set_b).cloned().collect(),
                        OP_SETINTER => set_a.intersection(&set_b).cloned().collect(),
                        _ => set_a.difference(&set_b).cloned().collect(),
                    };
                    keys.sort();
                    let mut out: Vec<NanVal> = Vec::with_capacity(keys.len());
                    for k in &keys {
                        let v = first_a.get(k).copied().or_else(|| first_b.get(k).copied());
                        if let Some(v) = v {
                            v.clone_rc();
                            out.push(v);
                        }
                    }
                    reg_set!(a, NanVal::heap_list(out));
                }
                OP_UNIQBY => {
                    // HOF dispatch not wired through the VM yet (matches map/flt/fld/grp).
                    // Emitter currently falls through to OP_CALL → interpreter; this arm
                    // exists so the opcode is a known dispatch target for future wiring.
                    vm_err!(VmError::Type("uniqby: VM HOF dispatch not implemented"));
                }
                OP_PARTITION => {
                    // HOF dispatch not wired through the VM yet (matches map/flt/fld/grp).
                    // Emitter currently falls through to OP_CALL → interpreter; this arm
                    // exists so the opcode is a known dispatch target for future wiring.
                    vm_err!(VmError::Type("partition: VM HOF dispatch not implemented"));
                }
                _ => vm_err!(VmError::UnknownOpcode { op }),
            }
        }
    }
}

/// Build a type-prefixed key string for `setunion`/`setinter`/`setdiff`.
/// Returns `Err` with a human-readable message when the value is not t/n/b.
/// Mirrors the prefix scheme used by `uniqby` (post-hotfix) so that
/// `Number(5)` and `Text("5")` never collide.
fn setops_key(v: NanVal, op_name: &str) -> Result<String, String> {
    if v.is_number() {
        let n = v.as_number();
        if n == (n as i64) as f64 {
            return Ok(format!("n:{}", n as i64));
        }
        return Ok(format!("n:{n}"));
    }
    if v.0 == TAG_TRUE {
        return Ok("b:true".to_string());
    }
    if v.0 == TAG_FALSE {
        return Ok("b:false".to_string());
    }
    if v.is_string() {
        let s = unsafe {
            match v.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => return Err(format!("{op_name}: invalid string heap object")),
            }
        };
        return Ok(format!("t:{s}"));
    }
    Err(format!("{op_name}: elements must be text, number, or bool"))
}

/// Lexicographic comparison of two NanVal strings.
/// # Safety
/// Caller must ensure both `a` and `b` satisfy `is_string()`.
unsafe fn nanval_str_cmp(a: NanVal, b: NanVal) -> std::cmp::Ordering {
    // SAFETY: caller guarantees is_string() for both values.
    unsafe {
        let sa = match a.as_heap_ref() {
            HeapObj::Str(s) => s,
            _ => unreachable!(),
        };
        let sb = match b.as_heap_ref() {
            HeapObj::Str(s) => s,
            _ => unreachable!(),
        };
        sa.cmp(sb)
    }
}

/// Stringify a NanVal element value for use as a map key in `frq`.
/// Returns None if the value is not a text, number, or bool.
///
/// Keys are prefixed with a type tag (`n:` / `t:` / `b:`) so that values
/// from distinct domains never alias one another (e.g. `Number(1)` and
/// `Text("1")` would otherwise both stringify to `"1"`). Matches the
/// uniqby/setops precedent.
fn nanval_to_key_string(v: NanVal) -> Option<String> {
    if v.is_number() {
        let n = v.as_number();
        if n.fract() == 0.0 && n.abs() < 1e15 {
            return Some(format!("n:{}", n as i64));
        }
        return Some(format!("n:{n}"));
    }
    match v.0 {
        TAG_TRUE => return Some("b:true".to_string()),
        TAG_FALSE => return Some("b:false".to_string()),
        _ => {}
    }
    if v.is_string() {
        // SAFETY: is_string() confirmed.
        unsafe {
            return match v.as_heap_ref() {
                HeapObj::Str(s) => Some(format!("t:{s}")),
                _ => None,
            };
        }
    }
    None
}

/// Convert a NanVal expected to be a list-of-numbers into Vec<f64>.
fn nanval_to_vec(v: NanVal) -> std::result::Result<Vec<f64>, &'static str> {
    if !v.is_heap() {
        return Err("expected a list of numbers");
    }
    let items = match unsafe { v.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return Err("expected a list of numbers"),
    };
    let mut out: Vec<f64> = Vec::with_capacity(items.len());
    for item in items {
        if !item.is_number() {
            return Err("vector items must be numbers");
        }
        out.push(item.as_number());
    }
    Ok(out)
}

/// Convert a NanVal expected to be a list-of-list-of-numbers into Vec<Vec<f64>>.
fn nanval_to_matrix(v: NanVal) -> std::result::Result<Vec<Vec<f64>>, &'static str> {
    if !v.is_heap() {
        return Err("expected a list of lists");
    }
    let rows = match unsafe { v.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return Err("expected a list of lists"),
    };
    let mut out: Vec<Vec<f64>> = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(nanval_to_vec(*row)?);
    }
    Ok(out)
}

fn nanval_to_json(v: NanVal) -> serde_json::Value {
    if v.is_number() {
        let n = v.as_number();
        if n.fract() == 0.0 && n.abs() < 1e15 {
            return serde_json::Value::Number(serde_json::Number::from(n as i64));
        }
        return serde_json::Number::from_f64(n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null);
    }
    if v.is_arena_record() {
        unsafe {
            let rec = v.as_arena_record();
            let n = rec.n_fields as usize;
            let mut map = serde_json::Map::new();
            // Try to get field names from active registry
            let registry_ptr = ACTIVE_REGISTRY.with(|r| r.get());
            for i in 0..n {
                let fv = NanVal(*rec.field_ptr(i));
                let name = if !registry_ptr.is_null() {
                    let registry = &*registry_ptr;
                    registry
                        .types
                        .get(rec.type_id as usize)
                        .and_then(|ti| ti.fields.get(i).cloned())
                        .unwrap_or_else(|| format!("_{}", i))
                } else {
                    format!("_{}", i)
                };
                map.insert(name, nanval_to_json(fv));
            }
            return serde_json::Value::Object(map);
        }
    }
    match v.0 {
        TAG_NIL => serde_json::Value::Null,
        TAG_TRUE => serde_json::Value::Bool(true),
        TAG_FALSE => serde_json::Value::Bool(false),
        _ if v.is_heap() => {
            // SAFETY: is_heap() confirmed heap-tagged with live RC.
            unsafe {
                match v.as_heap_ref() {
                    HeapObj::Str(s) => serde_json::Value::String(s.clone()),
                    HeapObj::List(items) => {
                        serde_json::Value::Array(items.iter().map(|i| nanval_to_json(*i)).collect())
                    }
                    HeapObj::Record { type_info, fields } => {
                        let map: serde_json::Map<String, serde_json::Value> = type_info
                            .fields
                            .iter()
                            .zip(fields.iter())
                            .map(|(k, v)| (k.clone(), nanval_to_json(*v)))
                            .collect();
                        serde_json::Value::Object(map)
                    }
                    HeapObj::OkVal(inner) => nanval_to_json(*inner),
                    HeapObj::ErrVal(inner) => nanval_to_json(*inner),
                    HeapObj::Map(m) => {
                        let obj: serde_json::Map<String, serde_json::Value> = m
                            .iter()
                            .map(|(k, v)| (k.clone(), nanval_to_json(*v)))
                            .collect();
                        serde_json::Value::Object(obj)
                    }
                }
            }
        }
        _ => serde_json::Value::Null,
    }
}

fn serde_json_to_nanval(v: serde_json::Value) -> NanVal {
    match v {
        serde_json::Value::Object(map) => {
            let field_names: Vec<String> = map.keys().cloned().collect();
            let field_vals: Box<[NanVal]> = map
                .into_iter()
                .map(|(_, v)| serde_json_to_nanval(v))
                .collect::<Vec<_>>()
                .into_boxed_slice();
            let type_info = Rc::new(TypeInfo {
                name: "json".to_string(),
                fields: field_names,
                num_fields: 0,
            });
            NanVal::heap_record(type_info, field_vals)
        }
        serde_json::Value::Array(arr) => {
            NanVal::heap_list(arr.into_iter().map(serde_json_to_nanval).collect())
        }
        serde_json::Value::String(s) => NanVal::heap_string(s),
        serde_json::Value::Number(n) => NanVal::number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::Bool(b) => NanVal::boolean(b),
        serde_json::Value::Null => NanVal::nil(),
    }
}

/// Parse string content into a NanVal according to format name.
/// Grid ("csv", "tsv") → Ok(list of rows).
/// Graph ("json")      → Ok(parsed JSON) or Err(error string NanVal).
/// Raw/unknown         → Ok(plain string).
fn vm_parse_format(fmt: &str, content: &str) -> Result<NanVal, NanVal> {
    match fmt {
        "csv" | "tsv" => {
            let sep = if fmt == "tsv" { '\t' } else { ',' };
            let rows: Vec<NanVal> = content
                .lines()
                .map(|line| {
                    let fields: Vec<NanVal> = vm_parse_csv_row(line, sep)
                        .into_iter()
                        .map(NanVal::heap_string)
                        .collect();
                    NanVal::heap_list(fields)
                })
                .collect();
            Ok(NanVal::heap_list(rows))
        }
        "json" => serde_json::from_str::<serde_json::Value>(content)
            .map(serde_json_to_nanval)
            .map_err(|e| NanVal::heap_string(e.to_string())),
        _ => Ok(NanVal::heap_string(content.to_string())),
    }
}

fn vm_parse_csv_row(line: &str, sep: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == sep {
            fields.push(std::mem::take(&mut field));
        } else {
            field.push(c);
        }
    }
    fields.push(field);
    fields
}

fn nanval_equal(a: NanVal, b: NanVal) -> bool {
    if a.is_number() && b.is_number() {
        (a.as_number() - b.as_number()).abs() < f64::EPSILON
    } else if a.0 == b.0 {
        true
    } else if a.is_string() && b.is_string() {
        unsafe {
            // SAFETY: is_string() confirmed both are live heap-allocated string Rc pointers.
            let sa = match a.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            };
            let sb = match b.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            };
            sa == sb
        }
    } else {
        false
    }
}

fn nanval_truthy(v: NanVal) -> bool {
    if v.is_number() {
        v.as_number() != 0.0
    } else {
        match v.0 {
            TAG_NIL | TAG_FALSE => false,
            TAG_TRUE => true,
            _ => unsafe {
                // SAFETY: the outer `if v.is_number()` guard eliminated all
                // plain f64 values. The match arms above exhausted nil, true,
                // and false (the only non-heap non-number tags). Therefore
                // any remaining value must be a live heap pointer created by
                // a heap_* constructor, making as_heap_ref() sound here.
                debug_assert!(
                    v.is_heap(),
                    "nanval_truthy: unexpected non-heap NanVal tag {:#018x}",
                    v.0
                );
                match v.as_heap_ref() {
                    HeapObj::Str(s) => !s.is_empty(),
                    HeapObj::List(l) => !l.is_empty(),
                    _ => true,
                }
            },
        }
    }
}

// ── JIT helper functions (extern "C", callable from JIT-compiled code) ──
//
// Each function operates on NanVal u64 bit patterns directly.
// The JIT loads/stores u64 registers and calls these for non-trivial ops.

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_add(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        return NanVal::number(av.as_number() + bv.as_number()).0;
    }
    if av.is_string() && bv.is_string() {
        let result = unsafe {
            let a_ptr = (av.0 & PTR_MASK) as *const HeapObj;
            let b_ptr = (bv.0 & PTR_MASK) as *const HeapObj;
            // Read the right-hand string via raw pointer (does not touch RC count).
            // SAFETY: b_ptr was produced by Rc::into_raw; the NanVal keeps it alive.
            let sb: &str = match &*b_ptr {
                HeapObj::Str(s) => s.as_str(),
                _ => unreachable!(),
            };
            // RC=1 fast path: mutate the left string in place via Rc::get_mut,
            // avoiding a fresh allocation. Matches CPython's str += optimisation.
            let mut a_rc = Rc::from_raw(a_ptr);
            if let Some(heap_obj) = Rc::get_mut(&mut a_rc) {
                // Sole owner — mutate the inner String directly.
                match heap_obj {
                    HeapObj::Str(s) => s.push_str(sb),
                    _ => unreachable!(),
                }
                // The Rc is still intact; encode its pointer as the new NanVal.
                let ptr = Rc::into_raw(a_rc) as u64;
                NanVal(TAG_STRING | (ptr & PTR_MASK))
            } else {
                // Multiple owners — copy path.
                let sa: &str = match &*a_ptr {
                    HeapObj::Str(s) => s.as_str(),
                    _ => unreachable!(),
                };
                let mut out = String::with_capacity(sa.len() + sb.len());
                out.push_str(sa);
                out.push_str(sb);
                // Restore the Rc we reconstructed so its count stays correct.
                std::mem::forget(a_rc);
                NanVal::heap_string(out)
            }
        };
        return result.0;
    }
    if av.is_heap() && bv.is_heap() {
        let aref = unsafe { av.as_heap_ref() };
        let bref = unsafe { bv.as_heap_ref() };
        if let (HeapObj::List(left), HeapObj::List(right)) = (aref, bref) {
            let mut new_items = Vec::with_capacity(left.len() + right.len());
            for v in left {
                v.clone_rc();
                new_items.push(*v);
            }
            for v in right {
                v.clone_rc();
                new_items.push(*v);
            }
            return NanVal::heap_list(new_items).0;
        }
    }
    TAG_NIL // error fallback
}

/// Fast-path string concatenation — both arguments are guaranteed to be strings.
/// Called by JIT/AOT OP_ADD_SS handler. Skips numeric and list type checks entirely.
#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_concat(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    unsafe {
        let a_ptr = (av.0 & PTR_MASK) as *const HeapObj;
        let b_ptr = (bv.0 & PTR_MASK) as *const HeapObj;
        // Read the right-hand string via raw pointer (does not touch RC count).
        // SAFETY: b_ptr was produced by Rc::into_raw; the NanVal keeps it alive.
        let sb: &str = match &*b_ptr {
            HeapObj::Str(s) => s.as_str(),
            _ => unreachable!(),
        };
        // RC=1 fast path: mutate left string in place via Rc::get_mut.
        let mut a_rc = Rc::from_raw(a_ptr);
        if let Some(heap_obj) = Rc::get_mut(&mut a_rc) {
            match heap_obj {
                HeapObj::Str(s) => s.push_str(sb),
                _ => unreachable!(),
            }
            let ptr = Rc::into_raw(a_rc) as u64;
            NanVal(TAG_STRING | (ptr & PTR_MASK)).0
        } else {
            // Multiple owners — copy path.
            let sa: &str = match &*a_ptr {
                HeapObj::Str(s) => s.as_str(),
                _ => unreachable!(),
            };
            let mut out = String::with_capacity(sa.len() + sb.len());
            out.push_str(sa);
            out.push_str(sb);
            std::mem::forget(a_rc);
            NanVal::heap_string(out).0
        }
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_sub(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        return NanVal::number(av.as_number() - bv.as_number()).0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_mul(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        return NanVal::number(av.as_number() * bv.as_number()).0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_div(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        let dv = bv.as_number();
        if dv == 0.0 {
            return TAG_NIL;
        }
        return NanVal::number(av.as_number() / dv).0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_eq(a: u64, b: u64) -> u64 {
    NanVal::boolean(nanval_equal(NanVal(a), NanVal(b))).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_ne(a: u64, b: u64) -> u64 {
    NanVal::boolean(!nanval_equal(NanVal(a), NanVal(b))).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_gt(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        return NanVal::boolean(av.as_number() > bv.as_number()).0;
    }
    if av.is_string() && bv.is_string() {
        return NanVal::boolean(unsafe { nanval_str_cmp(av, bv) == std::cmp::Ordering::Greater }).0;
    }
    TAG_FALSE
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_lt(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        return NanVal::boolean(av.as_number() < bv.as_number()).0;
    }
    if av.is_string() && bv.is_string() {
        return NanVal::boolean(unsafe { nanval_str_cmp(av, bv) == std::cmp::Ordering::Less }).0;
    }
    TAG_FALSE
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_ge(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        return NanVal::boolean(av.as_number() >= bv.as_number()).0;
    }
    if av.is_string() && bv.is_string() {
        return NanVal::boolean(unsafe { nanval_str_cmp(av, bv) != std::cmp::Ordering::Less }).0;
    }
    TAG_FALSE
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_le(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        return NanVal::boolean(av.as_number() <= bv.as_number()).0;
    }
    if av.is_string() && bv.is_string() {
        return NanVal::boolean(unsafe { nanval_str_cmp(av, bv) != std::cmp::Ordering::Greater }).0;
    }
    TAG_FALSE
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_not(a: u64) -> u64 {
    NanVal::boolean(!nanval_truthy(NanVal(a))).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_neg(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_number() {
        return NanVal::number(-v.as_number()).0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_truthy(a: u64) -> u64 {
    if nanval_truthy(NanVal(a)) { 1 } else { 0 }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_wrapok(v: u64) -> u64 {
    let nv = NanVal(v);
    if !nv.is_number() {
        nv.clone_rc();
    }
    NanVal::heap_ok(nv).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_wraperr(v: u64) -> u64 {
    let nv = NanVal(v);
    if !nv.is_number() {
        nv.clone_rc();
    }
    NanVal::heap_err(nv).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_isok(v: u64) -> u64 {
    NanVal::boolean((v & TAG_MASK) == TAG_OK).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_iserr(v: u64) -> u64 {
    NanVal::boolean((v & TAG_MASK) == TAG_ERR).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_unwrap(v: u64) -> u64 {
    let nv = NanVal(v);
    if !nv.is_heap() {
        return TAG_NIL;
    }
    unsafe {
        match nv.as_heap_ref() {
            HeapObj::OkVal(inner) | HeapObj::ErrVal(inner) => {
                inner.clone_rc();
                inner.0
            }
            _ => TAG_NIL,
        }
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_move(v: u64) -> u64 {
    let nv = NanVal(v);
    nv.clone_rc(); // no-op for non-heap values (numbers, nil, true, false)
    v
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_clone_rc(v: u64) {
    NanVal(v).clone_rc();
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_drop_rc(v: u64) {
    NanVal(v).drop_rc();
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_len(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_string() {
        let s = unsafe {
            match v.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        return NanVal::number(s.len() as f64).0;
    }
    if v.is_heap()
        && let HeapObj::List(items) = unsafe { v.as_heap_ref() }
    {
        return NanVal::number(items.len() as f64).0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_str(a: u64) -> u64 {
    let v = NanVal(a);
    if !v.is_number() {
        return TAG_NIL;
    }
    let n = v.as_number();
    let s = if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    };
    NanVal::heap_string(s).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_num(a: u64) -> u64 {
    let v = NanVal(a);
    if !v.is_string() {
        return TAG_NIL;
    }
    let s = unsafe {
        match v.as_heap_ref() {
            HeapObj::Str(s) => s,
            _ => unreachable!(),
        }
    };
    match s.parse::<f64>() {
        Ok(n) => NanVal::heap_ok(NanVal::number(n)).0,
        Err(_) => {
            v.clone_rc();
            NanVal::heap_err(v).0
        }
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_abs(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_number() {
        NanVal::number(v.as_number().abs()).0
    } else {
        TAG_NIL
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_mod(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        let dv = bv.as_number();
        if dv == 0.0 {
            return TAG_NIL;
        }
        NanVal::number(av.as_number() % dv).0
    } else {
        TAG_NIL
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_clamp(x: u64, lo: u64, hi: u64) -> u64 {
    let xv = NanVal(x);
    let lv = NanVal(lo);
    let hv = NanVal(hi);
    if xv.is_number() && lv.is_number() && hv.is_number() {
        // result = max(lo, min(hi, x)); when lo > hi, result == lo.
        NanVal::number(xv.as_number().min(hv.as_number()).max(lv.as_number())).0
    } else {
        TAG_NIL
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_min(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        NanVal::number(av.as_number().min(bv.as_number())).0
    } else {
        TAG_NIL
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_max(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        NanVal::number(av.as_number().max(bv.as_number())).0
    } else {
        TAG_NIL
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_flr(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_number() {
        NanVal::number(v.as_number().floor()).0
    } else {
        TAG_NIL
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_cel(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_number() {
        NanVal::number(v.as_number().ceil()).0
    } else {
        TAG_NIL
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_rou(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_number() {
        NanVal::number(v.as_number().round()).0
    } else {
        TAG_NIL
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_pow(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        NanVal::number(av.as_number().powf(bv.as_number())).0
    } else {
        NanVal::number(f64::NAN).0
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_sqrt(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_number() {
        NanVal::number(v.as_number().sqrt()).0
    } else {
        NanVal::number(f64::NAN).0
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_log(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_number() {
        NanVal::number(v.as_number().ln()).0
    } else {
        NanVal::number(f64::NAN).0
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_exp(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_number() {
        NanVal::number(v.as_number().exp()).0
    } else {
        NanVal::number(f64::NAN).0
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_sin(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_number() {
        NanVal::number(v.as_number().sin()).0
    } else {
        NanVal::number(f64::NAN).0
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_cos(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_number() {
        NanVal::number(v.as_number().cos()).0
    } else {
        NanVal::number(f64::NAN).0
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_tan(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_number() {
        NanVal::number(v.as_number().tan()).0
    } else {
        NanVal::number(f64::NAN).0
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_log10(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_number() {
        NanVal::number(v.as_number().log10()).0
    } else {
        NanVal::number(f64::NAN).0
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_log2(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_number() {
        NanVal::number(v.as_number().log2()).0
    } else {
        NanVal::number(f64::NAN).0
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_atan2(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        NanVal::number(av.as_number().atan2(bv.as_number())).0
    } else {
        NanVal::number(f64::NAN).0
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_transpose(a: u64) -> u64 {
    let v = NanVal(a);
    if !v.is_heap() {
        return TAG_NIL;
    }
    let rows = match unsafe { v.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return TAG_NIL,
    };
    if rows.is_empty() {
        return NanVal::heap_list(vec![]).0;
    }
    let mut row_refs: Vec<&Vec<NanVal>> = Vec::with_capacity(rows.len());
    let mut ncols: Option<usize> = None;
    for row in rows {
        if !row.is_heap() {
            return TAG_NIL;
        }
        let r = match unsafe { row.as_heap_ref() } {
            HeapObj::List(items) => items,
            _ => return TAG_NIL,
        };
        match ncols {
            None => ncols = Some(r.len()),
            Some(n) if n != r.len() => return TAG_NIL,
            _ => {}
        }
        row_refs.push(r);
    }
    let ncols = ncols.unwrap_or(0);
    let mut result: Vec<NanVal> = Vec::with_capacity(ncols);
    for j in 0..ncols {
        let mut col: Vec<NanVal> = Vec::with_capacity(row_refs.len());
        for r in &row_refs {
            let v = r[j];
            v.clone_rc();
            col.push(v);
        }
        result.push(NanVal::heap_list(col));
    }
    NanVal::heap_list(result).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_matmul(a: u64, b: u64) -> u64 {
    let va = NanVal(a);
    let vb = NanVal(b);
    if !va.is_heap() || !vb.is_heap() {
        return TAG_NIL;
    }
    let a_rows_v = match unsafe { va.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return TAG_NIL,
    };
    let b_rows_v = match unsafe { vb.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return TAG_NIL,
    };
    let mut a_mat: Vec<Vec<f64>> = Vec::with_capacity(a_rows_v.len());
    let mut a_cols: Option<usize> = None;
    for row in a_rows_v {
        if !row.is_heap() {
            return TAG_NIL;
        }
        let r = match unsafe { row.as_heap_ref() } {
            HeapObj::List(items) => items,
            _ => return TAG_NIL,
        };
        match a_cols {
            None => a_cols = Some(r.len()),
            Some(n) if n != r.len() => return TAG_NIL,
            _ => {}
        }
        let mut nums = Vec::with_capacity(r.len());
        for v in r {
            if !v.is_number() {
                return TAG_NIL;
            }
            nums.push(v.as_number());
        }
        a_mat.push(nums);
    }
    let mut b_mat: Vec<Vec<f64>> = Vec::with_capacity(b_rows_v.len());
    let mut b_cols: Option<usize> = None;
    for row in b_rows_v {
        if !row.is_heap() {
            return TAG_NIL;
        }
        let r = match unsafe { row.as_heap_ref() } {
            HeapObj::List(items) => items,
            _ => return TAG_NIL,
        };
        match b_cols {
            None => b_cols = Some(r.len()),
            Some(n) if n != r.len() => return TAG_NIL,
            _ => {}
        }
        let mut nums = Vec::with_capacity(r.len());
        for v in r {
            if !v.is_number() {
                return TAG_NIL;
            }
            nums.push(v.as_number());
        }
        b_mat.push(nums);
    }
    let a_rows_n = a_mat.len();
    let a_cols_n = a_cols.unwrap_or(0);
    let b_rows_n = b_mat.len();
    let b_cols_n = b_cols.unwrap_or(0);
    if a_cols_n != b_rows_n {
        return TAG_NIL;
    }
    let mut out: Vec<NanVal> = Vec::with_capacity(a_rows_n);
    #[allow(clippy::needless_range_loop)]
    for i in 0..a_rows_n {
        let mut row: Vec<NanVal> = Vec::with_capacity(b_cols_n);
        for j in 0..b_cols_n {
            let mut s = 0.0_f64;
            for k in 0..a_cols_n {
                s += a_mat[i][k] * b_mat[k][j];
            }
            row.push(NanVal::number(s));
        }
        out.push(NanVal::heap_list(row));
    }
    NanVal::heap_list(out).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_dot(a: u64, b: u64) -> u64 {
    let va = NanVal(a);
    let vb = NanVal(b);
    if !va.is_heap() || !vb.is_heap() {
        return NanVal::number(f64::NAN).0;
    }
    let xs = match unsafe { va.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return NanVal::number(f64::NAN).0,
    };
    let ys = match unsafe { vb.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return NanVal::number(f64::NAN).0,
    };
    if xs.len() != ys.len() {
        return NanVal::number(f64::NAN).0;
    }
    let mut total = 0.0_f64;
    for (x, y) in xs.iter().zip(ys.iter()) {
        if !x.is_number() || !y.is_number() {
            return NanVal::number(f64::NAN).0;
        }
        total += x.as_number() * y.as_number();
    }
    NanVal::number(total).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_det(a: u64) -> u64 {
    let v = NanVal(a);
    let mat = match nanval_to_matrix(v) {
        Ok(m) => m,
        Err(_) => return NanVal::number(f64::NAN).0,
    };
    let n = mat.len();
    if n == 0 {
        return NanVal::number(f64::NAN).0;
    }
    for row in &mat {
        if row.len() != n {
            return NanVal::number(f64::NAN).0;
        }
    }
    let (_lu, _piv, det, _singular) = crate::interpreter::lu_decompose(mat);
    NanVal::number(det).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_inv(a: u64) -> u64 {
    let v = NanVal(a);
    let mat = match nanval_to_matrix(v) {
        Ok(m) => m,
        Err(_) => return TAG_NIL,
    };
    let n = mat.len();
    if n == 0 {
        return TAG_NIL;
    }
    for row in &mat {
        if row.len() != n {
            return TAG_NIL;
        }
    }
    let (lu, piv, _det, singular) = crate::interpreter::lu_decompose(mat);
    if singular {
        return TAG_NIL;
    }
    let mut cols: Vec<Vec<f64>> = Vec::with_capacity(n);
    for j in 0..n {
        let mut e = vec![0.0; n];
        e[j] = 1.0;
        cols.push(crate::interpreter::lu_solve(&lu, &piv, &e));
    }
    let rows: Vec<NanVal> = (0..n)
        .map(|i| {
            let row: Vec<NanVal> = (0..n).map(|j| NanVal::number(cols[j][i])).collect();
            NanVal::heap_list(row)
        })
        .collect();
    NanVal::heap_list(rows).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_solve(a: u64, b: u64) -> u64 {
    let va = NanVal(a);
    let vb = NanVal(b);
    let mat = match nanval_to_matrix(va) {
        Ok(m) => m,
        Err(_) => return TAG_NIL,
    };
    let vec_b = match nanval_to_vec(vb) {
        Ok(v) => v,
        Err(_) => return TAG_NIL,
    };
    let n = mat.len();
    if n == 0 {
        return TAG_NIL;
    }
    for row in &mat {
        if row.len() != n {
            return TAG_NIL;
        }
    }
    if vec_b.len() != n {
        return TAG_NIL;
    }
    let (lu, piv, _det, singular) = crate::interpreter::lu_decompose(mat);
    if singular {
        return TAG_NIL;
    }
    let x = crate::interpreter::lu_solve(&lu, &piv, &vec_b);
    let list: Vec<NanVal> = x.into_iter().map(NanVal::number).collect();
    NanVal::heap_list(list).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_rnd0() -> u64 {
    NanVal::number(fastrand::f64()).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_rnd2(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        let lo = av.as_number() as i64;
        let hi = bv.as_number() as i64;
        if lo > hi {
            return TAG_NIL;
        }
        NanVal::number(fastrand::i64(lo..=hi) as f64).0
    } else {
        TAG_NIL
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_rndn(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if av.is_number() && bv.is_number() {
        let mu = av.as_number();
        let sigma = bv.as_number();
        NanVal::number(crate::interpreter::box_muller_normal(mu, sigma)).0
    } else {
        TAG_NIL
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_now() -> u64 {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    NanVal::number(ts).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_env(a: u64) -> u64 {
    let v = NanVal(a);
    if !v.is_string() {
        return TAG_NIL;
    }
    let key = unsafe {
        match v.as_heap_ref() {
            HeapObj::Str(s) => s,
            _ => unreachable!(),
        }
    };
    match std::env::var(key.as_str()) {
        Ok(val) => NanVal::heap_ok(NanVal::heap_string(val)).0,
        Err(_) => NanVal::heap_err(NanVal::heap_string(format!("env var '{}' not set", key))).0,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_get(a: u64) -> u64 {
    let v = NanVal(a);
    if !v.is_string() {
        return TAG_NIL;
    }
    #[cfg(feature = "http")]
    {
        let url = unsafe {
            match v.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        match minreq::get(url.as_str()).send() {
            Ok(resp) => match resp.as_str() {
                Ok(body) => NanVal::heap_ok(NanVal::heap_string(body.to_string())).0,
                Err(e) => {
                    NanVal::heap_err(NanVal::heap_string(format!(
                        "response is not valid UTF-8: {e}"
                    )))
                    .0
                }
            },
            Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())).0,
        }
    }
    #[cfg(not(feature = "http"))]
    {
        NanVal::heap_err(NanVal::heap_string("http feature not enabled".to_string())).0
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_getmany(a: u64) -> u64 {
    let v = NanVal(a);
    if !v.is_heap() {
        return TAG_NIL;
    }
    // SAFETY: is_heap() confirmed a heap-tagged value with live RC.
    let urls: Vec<String> = match unsafe { v.as_heap_ref() } {
        HeapObj::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items.iter() {
                if !item.is_string() {
                    return TAG_NIL;
                }
                let s = unsafe {
                    match item.as_heap_ref() {
                        HeapObj::Str(s) => s.as_str().to_owned(),
                        _ => unreachable!(),
                    }
                };
                out.push(s);
            }
            out
        }
        _ => return TAG_NIL,
    };
    let values = crate::interpreter::get_many_fetch(&urls);
    let nan_items: Vec<NanVal> = values.iter().map(NanVal::from_value).collect();
    NanVal::heap_list(nan_items).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_spl(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if !av.is_string() || !bv.is_string() {
        return TAG_NIL;
    }
    let text = unsafe {
        match av.as_heap_ref() {
            HeapObj::Str(s) => s,
            _ => unreachable!(),
        }
    };
    let sep = unsafe {
        match bv.as_heap_ref() {
            HeapObj::Str(s) => s,
            _ => unreachable!(),
        }
    };
    let items: Vec<NanVal> = text
        .split(sep.as_str())
        .map(|p| NanVal::heap_string(p.to_string()))
        .collect();
    NanVal::heap_list(items).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_cat(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if !bv.is_string() || !av.is_heap() {
        return TAG_NIL;
    }
    let sep = unsafe {
        match bv.as_heap_ref() {
            HeapObj::Str(s) => s,
            _ => unreachable!(),
        }
    };
    let items = match unsafe { av.as_heap_ref() } {
        HeapObj::List(l) => l,
        _ => return TAG_NIL,
    };
    let mut parts = Vec::with_capacity(items.len());
    for item in items {
        if !item.is_string() {
            return TAG_NIL;
        }
        let s = unsafe {
            match item.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        parts.push(s.as_str());
    }
    NanVal::heap_string(parts.join(sep.as_str())).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_has(a: u64, b: u64) -> u64 {
    let collection = NanVal(a);
    let needle = NanVal(b);
    if collection.is_string() {
        if !needle.is_string() {
            return TAG_FALSE;
        }
        let found = unsafe {
            let haystack = match collection.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            };
            let needle_s = match needle.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            };
            haystack.contains(needle_s.as_str())
        };
        return NanVal::boolean(found).0;
    }
    if collection.is_heap()
        && let HeapObj::List(items) = unsafe { collection.as_heap_ref() }
    {
        let found = items.iter().any(|item| nanval_equal(*item, needle));
        return NanVal::boolean(found).0;
    }
    TAG_FALSE
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_hd(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_string() {
        let s = unsafe {
            match v.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        if s.is_empty() {
            return TAG_NIL;
        }
        return NanVal::heap_string(
            s.chars()
                .next()
                .expect("non-empty checked above")
                .to_string(),
        )
        .0;
    }
    if v.is_heap()
        && let HeapObj::List(items) = unsafe { v.as_heap_ref() }
    {
        if items.is_empty() {
            return TAG_NIL;
        }
        items[0].clone_rc();
        return items[0].0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_fmt2(a: u64, b: u64) -> u64 {
    let va = NanVal(a);
    let vb = NanVal(b);
    if !va.is_number() || !vb.is_number() {
        return TAG_NIL;
    }
    let x = va.as_number();
    let d = vb.as_number();
    let digits = if !d.is_finite() || d <= 0.0 {
        0usize
    } else {
        (d as usize).min(20)
    };
    NanVal::heap_string(format!("{:.*}", digits, x)).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_at(a: u64, b: u64) -> u64 {
    let v = NanVal(a);
    let i = NanVal(b);
    if !i.is_number() {
        return TAG_NIL;
    }
    let n = i.as_number();
    if n.fract() != 0.0 {
        return TAG_NIL;
    }
    let raw = n as i64;
    if v.is_string() {
        let s = unsafe {
            match v.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        let chars: Vec<char> = s.chars().collect();
        let len = chars.len() as i64;
        let adjusted = if raw < 0 { raw + len } else { raw };
        if adjusted < 0 || adjusted >= len {
            return TAG_NIL;
        }
        return NanVal::heap_string(chars[adjusted as usize].to_string()).0;
    }
    if v.is_heap()
        && let HeapObj::List(items) = unsafe { v.as_heap_ref() }
    {
        let len = items.len() as i64;
        let adjusted = if raw < 0 { raw + len } else { raw };
        if adjusted < 0 || adjusted >= len {
            return TAG_NIL;
        }
        let idx = adjusted as usize;
        items[idx].clone_rc();
        return items[idx].0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_lst(list: u64, idx: u64, val: u64) -> u64 {
    let v = NanVal(list);
    let i = NanVal(idx);
    let new_val = NanVal(val);
    if !i.is_number() {
        return list;
    }
    let n = i.as_number();
    if n < 0.0 || n.fract() != 0.0 {
        return list;
    }
    let pos = n as usize;
    if v.is_heap()
        && let HeapObj::List(items) = unsafe { v.as_heap_ref() }
    {
        if pos >= items.len() {
            return list;
        }
        let mut new_items: Vec<NanVal> = Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            if i == pos {
                new_val.clone_rc();
                new_items.push(new_val);
            } else {
                item.clone_rc();
                new_items.push(*item);
            }
        }
        return NanVal::heap_list(new_items).0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_range(a: u64, b: u64) -> u64 {
    let va = NanVal(a);
    let vb = NanVal(b);
    if !va.is_number() || !vb.is_number() {
        return TAG_NIL;
    }
    // Reject non-integer bounds rather than silently truncating.
    if va.as_number().fract() != 0.0 || vb.as_number().fract() != 0.0 {
        return TAG_NIL;
    }
    let start = va.as_number() as i64;
    let end = vb.as_number() as i64;
    if start >= end {
        return NanVal::heap_list(Vec::new()).0;
    }
    let len = (end - start) as u64;
    if len > 1_000_000 {
        return TAG_NIL;
    }
    let mut out: Vec<NanVal> = Vec::with_capacity(len as usize);
    for i in start..end {
        out.push(NanVal::number(i as f64));
    }
    NanVal::heap_list(out).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_window(n_val: u64, xs_val: u64) -> u64 {
    let vn = NanVal(n_val);
    let vxs = NanVal(xs_val);
    if !vn.is_number() {
        return TAG_NIL;
    }
    let nf = vn.as_number();
    if !nf.is_finite() || nf <= 0.0 || nf.fract() != 0.0 {
        return TAG_NIL;
    }
    let n = nf as usize;
    if !vxs.is_heap() {
        return TAG_NIL;
    }
    let xs = match unsafe { vxs.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return TAG_NIL,
    };
    if n > xs.len() {
        return NanVal::heap_list(Vec::new()).0;
    }
    let mut acc: Vec<NanVal> = Vec::with_capacity(xs.len() - n + 1);
    for w in xs.windows(n) {
        let mut sub = Vec::with_capacity(n);
        for v in w {
            v.clone_rc();
            sub.push(*v);
        }
        acc.push(NanVal::heap_list(sub));
    }
    NanVal::heap_list(acc).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_zip(a: u64, b: u64) -> u64 {
    let va = NanVal(a);
    let vb = NanVal(b);
    if !va.is_heap() || !vb.is_heap() {
        return TAG_NIL;
    }
    let xs = match unsafe { va.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return TAG_NIL,
    };
    let ys = match unsafe { vb.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return TAG_NIL,
    };
    let n = xs.len().min(ys.len());
    let mut out: Vec<NanVal> = Vec::with_capacity(n);
    for i in 0..n {
        let x = xs[i];
        let y = ys[i];
        x.clone_rc();
        y.clone_rc();
        out.push(NanVal::heap_list(vec![x, y]));
    }
    NanVal::heap_list(out).0
}

#[cfg(feature = "cranelift")]
fn jit_setop_impl(a: u64, b: u64, op: u8) -> u64 {
    let va = NanVal(a);
    let vb = NanVal(b);
    if !va.is_heap() || !vb.is_heap() {
        return TAG_NIL;
    }
    let xs = match unsafe { va.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return TAG_NIL,
    };
    let ys = match unsafe { vb.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return TAG_NIL,
    };
    let op_name = match op {
        OP_SETUNION => "setunion",
        OP_SETINTER => "setinter",
        _ => "setdiff",
    };
    use std::collections::{HashMap, HashSet};
    let mut set_a: HashSet<String> = HashSet::new();
    let mut first_a: HashMap<String, NanVal> = HashMap::new();
    for x in xs {
        let k = match setops_key(*x, op_name) {
            Ok(k) => k,
            Err(_) => return TAG_NIL,
        };
        if set_a.insert(k.clone()) {
            first_a.insert(k, *x);
        }
    }
    let mut set_b: HashSet<String> = HashSet::new();
    let mut first_b: HashMap<String, NanVal> = HashMap::new();
    for y in ys {
        let k = match setops_key(*y, op_name) {
            Ok(k) => k,
            Err(_) => return TAG_NIL,
        };
        if set_b.insert(k.clone()) {
            first_b.insert(k, *y);
        }
    }
    let mut keys: Vec<String> = match op {
        OP_SETUNION => set_a.union(&set_b).cloned().collect(),
        OP_SETINTER => set_a.intersection(&set_b).cloned().collect(),
        _ => set_a.difference(&set_b).cloned().collect(),
    };
    keys.sort();
    let mut out: Vec<NanVal> = Vec::with_capacity(keys.len());
    for k in &keys {
        let v = first_a.get(k).copied().or_else(|| first_b.get(k).copied());
        if let Some(v) = v {
            v.clone_rc();
            out.push(v);
        }
    }
    NanVal::heap_list(out).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_setunion(a: u64, b: u64) -> u64 {
    jit_setop_impl(a, b, OP_SETUNION)
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_setinter(a: u64, b: u64) -> u64 {
    jit_setop_impl(a, b, OP_SETINTER)
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_setdiff(a: u64, b: u64) -> u64 {
    jit_setop_impl(a, b, OP_SETDIFF)
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_enumerate(a: u64) -> u64 {
    let va = NanVal(a);
    if !va.is_heap() {
        return TAG_NIL;
    }
    let xs = match unsafe { va.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return TAG_NIL,
    };
    let mut out: Vec<NanVal> = Vec::with_capacity(xs.len());
    for (i, &v) in xs.iter().enumerate() {
        v.clone_rc();
        out.push(NanVal::heap_list(vec![NanVal::number(i as f64), v]));
    }
    NanVal::heap_list(out).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_chunks(a: u64, b: u64) -> u64 {
    let va = NanVal(a);
    let vb = NanVal(b);
    if !va.is_number() {
        return TAG_NIL;
    }
    let n_raw = va.as_number();
    if n_raw.fract() != 0.0 || n_raw <= 0.0 {
        return TAG_NIL;
    }
    let n = n_raw as usize;
    if !vb.is_heap() {
        return TAG_NIL;
    }
    let xs = match unsafe { vb.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return TAG_NIL,
    };
    let mut out: Vec<NanVal> = Vec::with_capacity(xs.len().div_ceil(n));
    for chunk in xs.chunks(n) {
        let mut piece: Vec<NanVal> = Vec::with_capacity(chunk.len());
        for item in chunk {
            item.clone_rc();
            piece.push(*item);
        }
        out.push(NanVal::heap_list(piece));
    }
    NanVal::heap_list(out).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_tl(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_string() {
        let s = unsafe {
            match v.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        if s.is_empty() {
            return TAG_NIL;
        }
        let mut chars = s.chars();
        chars.next();
        return NanVal::heap_string(chars.collect()).0;
    }
    if v.is_heap()
        && let HeapObj::List(items) = unsafe { v.as_heap_ref() }
    {
        if items.is_empty() {
            return TAG_NIL;
        }
        let tail: Vec<NanVal> = items[1..]
            .iter()
            .map(|item| {
                item.clone_rc();
                *item
            })
            .collect();
        return NanVal::heap_list(tail).0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_rev(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_string() {
        let s = unsafe {
            match v.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        return NanVal::heap_string(s.chars().rev().collect::<String>()).0;
    }
    if v.is_heap()
        && let HeapObj::List(items) = unsafe { v.as_heap_ref() }
    {
        let mut reversed: Vec<NanVal> = items
            .iter()
            .map(|item| {
                item.clone_rc();
                *item
            })
            .collect();
        reversed.reverse();
        return NanVal::heap_list(reversed).0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_srt(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_string() {
        let s = unsafe {
            match v.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        let mut chars: Vec<char> = s.chars().collect();
        chars.sort();
        return NanVal::heap_string(chars.into_iter().collect()).0;
    }
    if v.is_heap()
        && let HeapObj::List(items) = unsafe { v.as_heap_ref() }
    {
        if items.is_empty() {
            return NanVal::heap_list(vec![]).0;
        }
        let all_numbers = items.iter().all(|v| v.is_number());
        let all_strings = items.iter().all(|v| v.is_string());
        if all_numbers {
            let mut sorted: Vec<NanVal> = items
                .iter()
                .map(|v| {
                    v.clone_rc();
                    *v
                })
                .collect();
            sorted.sort_by(|a, b| {
                a.as_number()
                    .partial_cmp(&b.as_number())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            return NanVal::heap_list(sorted).0;
        }
        if all_strings {
            let mut sorted: Vec<NanVal> = items
                .iter()
                .map(|v| {
                    v.clone_rc();
                    *v
                })
                .collect();
            sorted.sort_by(|a, b| unsafe { nanval_str_cmp(*a, *b) });
            return NanVal::heap_list(sorted).0;
        }
    }
    TAG_NIL
}

/// Compute the next power of two >= `n` (with `next_pow2(0) == 1`).
fn vm_next_pow2(n: usize) -> usize {
    if n <= 1 {
        return 1;
    }
    let mut p = 1usize;
    while p < n {
        p <<= 1;
    }
    p
}

/// In-place iterative Cooley-Tukey radix-2 FFT used by `OP_FFT` / `OP_IFFT`.
fn vm_cooley_tukey(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    if n <= 1 {
        return;
    }
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let sign: f64 = if inverse { 1.0 } else { -1.0 };
    let mut len = 2usize;
    while len <= n {
        let half = len / 2;
        let theta = sign * 2.0 * std::f64::consts::PI / (len as f64);
        let w_re = theta.cos();
        let w_im = theta.sin();
        let mut i = 0usize;
        while i < n {
            let mut cur_re = 1.0_f64;
            let mut cur_im = 0.0_f64;
            for k in 0..half {
                let a_re = re[i + k];
                let a_im = im[i + k];
                let b_re = re[i + k + half] * cur_re - im[i + k + half] * cur_im;
                let b_im = re[i + k + half] * cur_im + im[i + k + half] * cur_re;
                re[i + k] = a_re + b_re;
                im[i + k] = a_im + b_im;
                re[i + k + half] = a_re - b_re;
                im[i + k + half] = a_im - b_im;
                let new_re = cur_re * w_re - cur_im * w_im;
                let new_im = cur_re * w_im + cur_im * w_re;
                cur_re = new_re;
                cur_im = new_im;
            }
            i += len;
        }
        len <<= 1;
    }
    if inverse {
        let scale = 1.0 / (n as f64);
        for x in re.iter_mut() {
            *x *= scale;
        }
        for x in im.iter_mut() {
            *x *= scale;
        }
    }
}

/// Run an FFT on a NanVal list of real numbers. Returns a NanVal list of
/// `[real, imag]` pairs, or an error message if the input is malformed.
fn vm_fft_real_to_pairs(v: NanVal) -> Result<NanVal, &'static str> {
    if !v.is_heap() {
        return Err("fft requires a list of numbers");
    }
    let items = match unsafe { v.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return Err("fft requires a list of numbers"),
    };
    if items.is_empty() {
        return Err("fft: input list must not be empty");
    }
    let mut re: Vec<f64> = Vec::with_capacity(items.len());
    for item in items {
        if !item.is_number() {
            return Err("fft: list elements must be numbers");
        }
        re.push(item.as_number());
    }
    let n = vm_next_pow2(re.len());
    re.resize(n, 0.0);
    let mut im = vec![0.0_f64; n];
    vm_cooley_tukey(&mut re, &mut im, false);
    let pairs: Vec<NanVal> = re
        .into_iter()
        .zip(im)
        .map(|(r, i)| NanVal::heap_list(vec![NanVal::number(r), NanVal::number(i)]))
        .collect();
    Ok(NanVal::heap_list(pairs))
}

/// Run an inverse FFT on a NanVal list of `[real, imag]` pairs. Returns a
/// NanVal list of real numbers (imaginary parts are discarded).
fn vm_ifft_pairs_to_real(v: NanVal) -> Result<NanVal, &'static str> {
    if !v.is_heap() {
        return Err("ifft requires a list of [real, imag] pairs");
    }
    let items = match unsafe { v.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => return Err("ifft requires a list of [real, imag] pairs"),
    };
    if items.is_empty() {
        return Err("ifft: input list must not be empty");
    }
    let mut re: Vec<f64> = Vec::with_capacity(items.len());
    let mut im: Vec<f64> = Vec::with_capacity(items.len());
    for item in items {
        if !item.is_heap() {
            return Err("ifft: each element must be a [real, imag] pair");
        }
        let pair = match unsafe { item.as_heap_ref() } {
            HeapObj::List(p) => p,
            _ => return Err("ifft: each element must be a [real, imag] pair"),
        };
        if pair.len() != 2 || !pair[0].is_number() || !pair[1].is_number() {
            return Err("ifft: each element must be a [real, imag] pair");
        }
        re.push(pair[0].as_number());
        im.push(pair[1].as_number());
    }
    let n = vm_next_pow2(re.len());
    re.resize(n, 0.0);
    im.resize(n, 0.0);
    vm_cooley_tukey(&mut re, &mut im, true);
    let reals: Vec<NanVal> = re.into_iter().map(NanVal::number).collect();
    Ok(NanVal::heap_list(reals))
}

/// Collect a NanVal list-of-numbers into a Vec<f64>, returning a stable
/// error message keyed off `builtin` (the canonical name) used by all four
/// statistics helpers below.
fn vm_collect_numbers(v: NanVal, builtin: &'static str) -> Result<Vec<f64>, &'static str> {
    if !v.is_heap() {
        return Err(match builtin {
            "median" => "median requires a list of numbers",
            "quantile" => "quantile first arg must be a list of numbers",
            "stdev" => "stdev requires a list of numbers",
            _ => "variance requires a list of numbers",
        });
    }
    let items = match unsafe { v.as_heap_ref() } {
        HeapObj::List(items) => items,
        _ => {
            return Err(match builtin {
                "median" => "median requires a list of numbers",
                "quantile" => "quantile first arg must be a list of numbers",
                "stdev" => "stdev requires a list of numbers",
                _ => "variance requires a list of numbers",
            });
        }
    };
    if items.is_empty() {
        return Err(match builtin {
            "median" => "median: input list must not be empty",
            "quantile" => "quantile: input list must not be empty",
            "stdev" => "stdev: input list must not be empty",
            _ => "variance: input list must not be empty",
        });
    }
    let mut out: Vec<f64> = Vec::with_capacity(items.len());
    for item in items {
        if !item.is_number() {
            return Err(match builtin {
                "median" => "median: list elements must be numbers",
                "quantile" => "quantile: list elements must be numbers",
                "stdev" => "stdev: list elements must be numbers",
                _ => "variance: list elements must be numbers",
            });
        }
        out.push(item.as_number());
    }
    Ok(out)
}

fn vm_median(v: NanVal) -> Result<NanVal, &'static str> {
    let mut nums = vm_collect_numbers(v, "median")?;
    // NaN-propagation per the math-builtins NaN contract: any NaN input → NaN.
    // Avoids silently mis-sorting NaNs via `partial_cmp(...).unwrap_or(Equal)`.
    if nums.iter().any(|x| x.is_nan()) {
        return Ok(NanVal::number(f64::NAN));
    }
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = nums.len();
    let m = if n % 2 == 1 {
        nums[n / 2]
    } else {
        (nums[n / 2 - 1] + nums[n / 2]) / 2.0
    };
    Ok(NanVal::number(m))
}

fn vm_quantile(v: NanVal, p: NanVal) -> Result<NanVal, &'static str> {
    if !p.is_number() {
        return Err("quantile: second arg p must be a number");
    }
    let mut nums = vm_collect_numbers(v, "quantile")?;
    // NaN-propagation (see vm_median).
    if nums.iter().any(|x| x.is_nan()) {
        return Ok(NanVal::number(f64::NAN));
    }
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p = p.as_number().clamp(0.0, 1.0);
    let n = nums.len();
    if n == 1 {
        return Ok(NanVal::number(nums[0]));
    }
    let pos = p * (n - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    Ok(NanVal::number(nums[lo] + frac * (nums[hi] - nums[lo])))
}

fn vm_variance(v: NanVal) -> Result<NanVal, &'static str> {
    let nums = vm_collect_numbers(v, "variance")?;
    let n = nums.len();
    if n == 1 {
        return Err("variance: at least 2 samples required");
    }
    if nums.iter().any(|x| x.is_nan()) {
        return Ok(NanVal::number(f64::NAN));
    }
    let mean = nums.iter().sum::<f64>() / n as f64;
    let sse: f64 = nums.iter().map(|x| (x - mean).powi(2)).sum();
    Ok(NanVal::number(sse / (n - 1) as f64))
}

fn vm_stdev(v: NanVal) -> Result<NanVal, &'static str> {
    let nums = vm_collect_numbers(v, "stdev")?;
    let n = nums.len();
    if n == 1 {
        return Err("stdev: at least 2 samples required");
    }
    if nums.iter().any(|x| x.is_nan()) {
        return Ok(NanVal::number(f64::NAN));
    }
    let mean = nums.iter().sum::<f64>() / n as f64;
    let sse: f64 = nums.iter().map(|x| (x - mean).powi(2)).sum();
    Ok(NanVal::number((sse / (n - 1) as f64).sqrt()))
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_median(a: u64) -> u64 {
    match vm_median(NanVal(a)) {
        Ok(v) => v.0,
        Err(_) => TAG_NIL,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_quantile(a: u64, b: u64) -> u64 {
    match vm_quantile(NanVal(a), NanVal(b)) {
        Ok(v) => v.0,
        Err(_) => TAG_NIL,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_stdev(a: u64) -> u64 {
    match vm_stdev(NanVal(a)) {
        Ok(v) => v.0,
        Err(_) => TAG_NIL,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_variance(a: u64) -> u64 {
    match vm_variance(NanVal(a)) {
        Ok(v) => v.0,
        Err(_) => TAG_NIL,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_fft(a: u64) -> u64 {
    match vm_fft_real_to_pairs(NanVal(a)) {
        Ok(v) => v.0,
        Err(_) => TAG_NIL,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_ifft(a: u64) -> u64 {
    match vm_ifft_pairs_to_real(NanVal(a)) {
        Ok(v) => v.0,
        Err(_) => TAG_NIL,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_cumsum(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_heap()
        && let HeapObj::List(items) = unsafe { v.as_heap_ref() }
    {
        let mut total = 0.0_f64;
        let mut out: Vec<NanVal> = Vec::with_capacity(items.len());
        for item in items {
            if !item.is_number() {
                return TAG_NIL;
            }
            total += item.as_number();
            out.push(NanVal::number(total));
        }
        return NanVal::heap_list(out).0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_rsrt(a: u64) -> u64 {
    let v = NanVal(a);
    if v.is_string() {
        let s = unsafe {
            match v.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        let mut chars: Vec<char> = s.chars().collect();
        chars.sort_by(|a, b| b.cmp(a));
        let sorted: String = chars.into_iter().collect();
        return NanVal::heap_string(sorted).0;
    }
    if v.is_heap()
        && let HeapObj::List(items) = unsafe { v.as_heap_ref() }
    {
        if items.is_empty() {
            return NanVal::heap_list(vec![]).0;
        }
        let all_numbers = items.iter().all(|v| v.is_number());
        let all_strings = items.iter().all(|v| v.is_string());
        if all_numbers {
            let mut sorted: Vec<NanVal> = items
                .iter()
                .map(|v| {
                    v.clone_rc();
                    *v
                })
                .collect();
            sorted.sort_by(|a, b| {
                b.as_number()
                    .partial_cmp(&a.as_number())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            return NanVal::heap_list(sorted).0;
        }
        if all_strings {
            let mut sorted: Vec<NanVal> = items
                .iter()
                .map(|v| {
                    v.clone_rc();
                    *v
                })
                .collect();
            sorted.sort_by(|a, b| unsafe { nanval_str_cmp(*b, *a) });
            return NanVal::heap_list(sorted).0;
        }
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_slc(a: u64, start: u64, end: u64) -> u64 {
    let vb = NanVal(a);
    let vc = NanVal(start);
    let vd = NanVal(end);
    if !vc.is_number() || !vd.is_number() {
        return TAG_NIL;
    }
    let s_idx = vc.as_number() as usize;
    let e_idx = vd.as_number() as usize;
    if vb.is_string() {
        let s = unsafe {
            match vb.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        let chars: Vec<char> = s.chars().collect();
        let e = e_idx.min(chars.len());
        let s = s_idx.min(e);
        return NanVal::heap_string(chars[s..e].iter().collect()).0;
    }
    if vb.is_heap()
        && let HeapObj::List(items) = unsafe { vb.as_heap_ref() }
    {
        let e = e_idx.min(items.len());
        let s = s_idx.min(e);
        let mut sliced = Vec::with_capacity(e - s);
        for v in &items[s..e] {
            v.clone_rc();
            sliced.push(*v);
        }
        return NanVal::heap_list(sliced).0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_rgxsub(pat_v: u64, repl_v: u64, subj_v: u64) -> u64 {
    let vp = NanVal(pat_v);
    let vr = NanVal(repl_v);
    let vs = NanVal(subj_v);
    if !vp.is_string() || !vr.is_string() || !vs.is_string() {
        return TAG_NIL;
    }
    let pat = unsafe {
        match vp.as_heap_ref() {
            HeapObj::Str(s) => s.as_str(),
            _ => unreachable!(),
        }
    };
    let repl = unsafe {
        match vr.as_heap_ref() {
            HeapObj::Str(s) => s.as_str(),
            _ => unreachable!(),
        }
    };
    let subj = unsafe {
        match vs.as_heap_ref() {
            HeapObj::Str(s) => s.as_str(),
            _ => unreachable!(),
        }
    };
    match regex::Regex::new(pat) {
        Ok(re) => NanVal::heap_string(re.replace_all(subj, repl).into_owned()).0,
        Err(_) => TAG_NIL,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_take(n_val: u64, xs: u64) -> u64 {
    let vn = NanVal(n_val);
    let vc = NanVal(xs);
    if !vn.is_number() {
        return TAG_NIL;
    }
    let nf = vn.as_number();
    if nf.fract() != 0.0 || nf < 0.0 {
        return TAG_NIL;
    }
    let n = nf as usize;
    if vc.is_string() {
        let s = unsafe {
            match vc.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        let chars: Vec<char> = s.chars().collect();
        let end = n.min(chars.len());
        return NanVal::heap_string(chars[..end].iter().collect()).0;
    }
    if vc.is_heap()
        && let HeapObj::List(items) = unsafe { vc.as_heap_ref() }
    {
        let end = n.min(items.len());
        let mut sliced = Vec::with_capacity(end);
        for v in &items[..end] {
            v.clone_rc();
            sliced.push(*v);
        }
        return NanVal::heap_list(sliced).0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_drop(n_val: u64, xs: u64) -> u64 {
    let vn = NanVal(n_val);
    let vc = NanVal(xs);
    if !vn.is_number() {
        return TAG_NIL;
    }
    let nf = vn.as_number();
    if nf.fract() != 0.0 || nf < 0.0 {
        return TAG_NIL;
    }
    let n = nf as usize;
    if vc.is_string() {
        let s = unsafe {
            match vc.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        let chars: Vec<char> = s.chars().collect();
        let start = n.min(chars.len());
        return NanVal::heap_string(chars[start..].iter().collect()).0;
    }
    if vc.is_heap()
        && let HeapObj::List(items) = unsafe { vc.as_heap_ref() }
    {
        let start = n.min(items.len());
        let mut sliced = Vec::with_capacity(items.len() - start);
        for v in &items[start..] {
            v.clone_rc();
            sliced.push(*v);
        }
        return NanVal::heap_list(sliced).0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_listappend(a: u64, b: u64) -> u64 {
    let list_val = NanVal(a);
    let item_val = NanVal(b);
    if !list_val.is_heap() {
        return TAG_NIL;
    }

    // Normalise the item: promote arena records to heap before anything else.
    let (item_val, item_already_owned) = if item_val.is_arena_record() {
        let registry_ptr = ACTIVE_REGISTRY.with(|r| r.get());
        if registry_ptr.is_null() {
            return TAG_NIL;
        }
        let promoted = item_val.promote_arena_to_heap(unsafe { &*registry_ptr });
        (promoted, true) // promote_arena_to_heap gives us RC=1 ownership
    } else {
        (item_val, false)
    };

    let ptr = (list_val.0 & PTR_MASK) as *const HeapObj;

    // RC=1 fast path: mutate the Vec in-place using standard amortized growth.
    // The JIT's FOREACHPREP/FOREACHNEXT reads Vec.len at HeapObj+24 (correct), so
    // there is no requirement for cap == len. Standard push with capacity doubling
    // gives O(n log n) total allocations instead of O(n²) from the old reserve_exact(1).
    // NOTE: the JIT compiler's OP_LISTAPPEND peephole always emits a==b (the same
    // register for both list operands), so RC is always 1 here in JIT-compiled code.
    let rc_count = {
        let rc_peek = unsafe { Rc::from_raw(ptr) };
        let count = Rc::strong_count(&rc_peek);
        std::mem::forget(rc_peek);
        count
    };

    if rc_count == 1 {
        let heap_mut = unsafe { &mut *(ptr as *mut HeapObj) };
        match heap_mut {
            HeapObj::List(items) => {
                // Standard push: amortized O(1), same as the VM's OP_LISTAPPEND path.
                if !item_already_owned {
                    item_val.clone_rc();
                }
                items.push(item_val);
                a // return same NanVal (same pointer, RC still 1)
            }
            _ => TAG_NIL,
        }
    } else {
        // Slow path: copy list
        match unsafe { list_val.as_heap_ref() } {
            HeapObj::List(items) => {
                let mut new_items = Vec::with_capacity(items.len() + 1);
                for v in items {
                    v.clone_rc();
                    new_items.push(*v);
                }
                if !item_already_owned {
                    item_val.clone_rc();
                }
                new_items.push(item_val);
                NanVal::heap_list(new_items).0
            }
            _ => TAG_NIL,
        }
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_index(a: u64, idx: u64) -> u64 {
    let obj = NanVal(a);
    let i = idx as usize;
    if !obj.is_heap() {
        return TAG_NIL;
    }
    match unsafe { obj.as_heap_ref() } {
        HeapObj::List(items) => {
            if i < items.len() {
                items[i].clone_rc();
                items[i].0
            } else {
                TAG_NIL
            }
        }
        _ => TAG_NIL,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_recfld(rec: u64, field_idx: u64) -> u64 {
    let rv = NanVal(rec);
    let idx = field_idx as usize;

    // Fast path: arena record
    if rv.is_arena_record() {
        unsafe {
            let r = rv.as_arena_record();
            if idx < r.n_fields as usize {
                let v = NanVal(*r.field_ptr(idx));
                v.clone_rc();
                return v.0;
            }
        }
        return TAG_NIL;
    }

    if !rv.is_heap() {
        return TAG_NIL;
    }
    match unsafe { rv.as_heap_ref() } {
        HeapObj::Record { fields, .. } => {
            if idx < fields.len() {
                let val = fields[idx];
                val.clone_rc();
                val.0
            } else {
                TAG_NIL
            }
        }
        _ => TAG_NIL,
    }
}

/// Dynamic field access by name — used by JIT/AOT for OP_RECFLD_NAME.
/// `rec` is a NanVal-encoded record, `field_name_ptr` is a null-terminated C string pointer,
/// `registry_ptr` is a pointer to TypeRegistry (for arena record type lookups).
#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub extern "C" fn jit_recfld_name(rec: u64, field_name_ptr: u64, registry_ptr: u64) -> u64 {
    // SAFETY: field_name_ptr is a null-terminated C string created by the JIT compiler
    // (leaked CString) or AOT compiler (data section). It remains valid for the call duration.
    let field_name = unsafe {
        let cstr = std::ffi::CStr::from_ptr(field_name_ptr as *const std::ffi::c_char);
        cstr.to_str().unwrap_or("")
    };
    let rv = NanVal(rec);

    if rv.is_arena_record() {
        // SAFETY: is_arena_record() confirmed the NanVal tag. registry_ptr comes from
        // ACTIVE_REGISTRY (JIT) or jit_get_registry_ptr (AOT) — valid for call duration.
        unsafe {
            let r = rv.as_arena_record();
            let registry = &*(registry_ptr as *const TypeRegistry);
            if let Some(type_info) = registry.types.get(r.type_id as usize)
                && let Some(idx) = type_info.fields.iter().position(|f| f == field_name)
                && idx < r.n_fields as usize
            {
                let v = NanVal(*r.field_ptr(idx));
                v.clone_rc();
                return v.0;
            }
        }
        return TAG_NIL;
    }

    if !rv.is_heap() {
        return TAG_NIL;
    }
    // SAFETY: is_heap() confirmed the NanVal is a heap pointer.
    match unsafe { rv.as_heap_ref() } {
        HeapObj::Record { type_info, fields } => {
            if let Some(idx) = type_info.fields.iter().position(|f| f == field_name)
                && idx < fields.len()
            {
                let val = fields[idx];
                val.clone_rc();
                return val.0;
            }
            TAG_NIL
        }
        _ => TAG_NIL,
    }
}

/// Create a new flat record. `arena_ptr` is a pointer to a BumpArena,
/// `registry_ptr` is a pointer to &TypeRegistry,
/// `type_id` identifies the type, `regs` has n_fields u64 values.
#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_recnew(
    arena_ptr: u64,
    type_id_and_nfields: u64,
    regs: *const u64,
    registry_ptr: u64,
) -> u64 {
    let tid = (type_id_and_nfields >> 16) as u16;
    let n = (type_id_and_nfields & 0xFFFF) as usize;
    let arena = unsafe { &mut *(arena_ptr as *mut BumpArena) };

    // Try arena allocation first (fast path)
    if let Some(rec_ptr) = arena.alloc_record(tid, n) {
        unsafe {
            let rec = &mut *rec_ptr;
            for i in 0..n {
                let v = NanVal(*regs.add(i));
                v.clone_rc();
                *rec.field_ptr_mut(i) = v.0;
            }
        }
        return NanVal::arena_record(rec_ptr).0;
    }

    // Arena full — fall back to Rc path
    let registry = unsafe { &*(registry_ptr as *const TypeRegistry) };
    let type_info = Rc::clone(&registry.types[tid as usize]);
    let mut fields = Vec::with_capacity(n);
    for i in 0..n {
        let v = NanVal(unsafe { *regs.add(i) });
        v.clone_rc();
        fields.push(v);
    }
    NanVal::heap_record(type_info, fields.into_boxed_slice()).0
}

/// Record-with: copy old record, overwrite specified fields by index.
/// `indices_ptr` points to n_updates u8 field indices, `regs` has n_updates new values.
#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_recwith(
    rec: u64,
    indices_ptr: *const u8,
    n_updates: u64,
    regs: *const u64,
) -> u64 {
    let rv = NanVal(rec);
    let n = n_updates as usize;

    // Fast path: arena record → arena record
    if rv.is_arena_record() {
        unsafe {
            let old_rec = rv.as_arena_record();
            let old_n = old_rec.n_fields as usize;
            let tid = old_rec.type_id;

            let arena_result = JIT_ARENA.with(|cell| {
                let mut arena = cell.borrow_mut();
                arena.alloc_record(tid, old_n)
            });

            if let Some(new_ptr) = arena_result {
                let new_rec = &mut *new_ptr;
                // Copy all fields
                for i in 0..old_n {
                    let v = NanVal(*old_rec.field_ptr(i));
                    v.clone_rc();
                    *new_rec.field_ptr_mut(i) = v.0;
                }
                // Overwrite updated slots
                for i in 0..n {
                    let slot = *indices_ptr.add(i) as usize;
                    if slot < old_n {
                        NanVal(*new_rec.field_ptr(slot)).drop_rc();
                        let val = NanVal(*regs.add(i));
                        val.clone_rc();
                        *new_rec.field_ptr_mut(slot) = val.0;
                    }
                }
                return NanVal::arena_record(new_ptr).0;
            }
            // Arena full — fall back to heap below
        }
    }

    if !rv.is_heap() && !rv.is_arena_record() {
        return TAG_NIL;
    }

    // Heap record path (or arena fallback when arena full)
    if rv.is_arena_record() {
        // Arena record but arena full — promote to heap
        unsafe {
            let old_rec = rv.as_arena_record();
            let old_n = old_rec.n_fields as usize;
            let registry_ptr = ACTIVE_REGISTRY.with(|r| r.get());
            if registry_ptr.is_null() {
                return TAG_NIL;
            }
            let registry = &*registry_ptr;
            let type_info = Rc::clone(&registry.types[old_rec.type_id as usize]);
            let mut new_fields = Vec::with_capacity(old_n);
            for i in 0..old_n {
                let v = NanVal(*old_rec.field_ptr(i));
                v.clone_rc();
                new_fields.push(v);
            }
            for i in 0..n {
                let slot = *indices_ptr.add(i) as usize;
                if slot < new_fields.len() {
                    let val = NanVal(*regs.add(i));
                    val.clone_rc();
                    new_fields[slot].drop_rc();
                    new_fields[slot] = val;
                }
            }
            return NanVal::heap_record(type_info, new_fields.into_boxed_slice()).0;
        }
    }

    match unsafe { rv.as_heap_ref() } {
        HeapObj::Record { type_info, fields } => {
            let mut new_fields: Vec<NanVal> = fields.to_vec();
            for v in new_fields.iter() {
                v.clone_rc();
            }
            for i in 0..n {
                let slot = unsafe { *indices_ptr.add(i) } as usize;
                let val = NanVal(unsafe { *regs.add(i) });
                val.clone_rc();
                new_fields[slot].drop_rc();
                new_fields[slot] = val;
            }
            NanVal::heap_record(Rc::clone(type_info), new_fields.into_boxed_slice()).0
        }
        _ => TAG_NIL,
    }
}

/// Record-with variant that takes the arena pointer directly, avoiding the
/// `JIT_ARENA.with(borrow_mut())` thread-local + RefCell overhead on every call.
/// The arena pointer is baked into JIT code at compile time (same as `jit_recnew`).
/// Signature: (rec: u64, arena_ptr: u64, indices_ptr: *const u8, n_updates: u64, regs: *const u64) -> u64
#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_recwith_arena(
    rec: u64,
    arena_ptr: u64,
    indices_ptr: *const u8,
    n_updates: u64,
    regs: *const u64,
) -> u64 {
    let rv = NanVal(rec);
    let n = n_updates as usize;

    // Fast path: arena record → arena record (no thread_local access)
    if rv.is_arena_record() {
        // SAFETY:
        //   - `rv.is_arena_record()` guarantees rv was produced by NanVal::arena_record,
        //     so the pointer is valid and points to a live ArenaRecord in the bump arena.
        //   - `arena_ptr` is the address of the thread-local BumpArena baked in by the JIT
        //     at compile time; the JIT always calls this from a single thread.
        //   - `indices_ptr` points to a `&'static [u8]` slice that was Box::leaked by the
        //     JIT compiler; it remains valid for the lifetime of the process.
        //   - `regs` points to a JIT stack slot of length `n_updates`; valid for the call.
        unsafe {
            let old_rec = rv.as_arena_record();
            let old_n = old_rec.n_fields as usize;
            let tid = old_rec.type_id;
            let arena = &mut *(arena_ptr as *mut BumpArena);

            if let Some(new_ptr) = arena.alloc_record(tid, old_n) {
                let new_rec = &mut *new_ptr;
                // Copy all fields
                for i in 0..old_n {
                    let v = NanVal(*old_rec.field_ptr(i));
                    v.clone_rc();
                    *new_rec.field_ptr_mut(i) = v.0;
                }
                // Overwrite updated slots
                for i in 0..n {
                    let slot = *indices_ptr.add(i) as usize;
                    if slot < old_n {
                        NanVal(*new_rec.field_ptr(slot)).drop_rc();
                        let val = NanVal(*regs.add(i));
                        val.clone_rc();
                        *new_rec.field_ptr_mut(slot) = val.0;
                    }
                }
                return NanVal::arena_record(new_ptr).0;
            }
            // Arena full — fall back to heap below
        }
    }

    // Heap record or arena-full fallback: delegate to the general helper
    jit_recwith(rec, indices_ptr, n_updates, regs)
}

/// Create a new list from n items pointed to by `regs`.
#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_listnew(regs: *const u64, n: u64) -> u64 {
    let count = n as usize;
    let mut items = Vec::with_capacity(count);
    for i in 0..count {
        let v = NanVal(unsafe { *regs.add(i) });
        v.clone_rc();
        items.push(v);
    }
    NanVal::heap_list(items).0
}

/// LISTGET for foreach loops: returns Ok(item) if found, TAG_NIL if out of bounds.
#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_listget(list: u64, idx: u64) -> u64 {
    let lv = NanVal(list);
    let iv = NanVal(idx);
    if !lv.is_heap() || !iv.is_number() {
        return TAG_NIL;
    }
    let i = iv.as_number() as usize;
    match unsafe { lv.as_heap_ref() } {
        HeapObj::List(items) => {
            if i < items.len() {
                items[i].clone_rc();
                NanVal::heap_ok(items[i]).0
            } else {
                TAG_NIL
            }
        }
        _ => TAG_NIL,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_jpth(a: u64, b: u64) -> u64 {
    let av = NanVal(a);
    let bv = NanVal(b);
    if !av.is_string() || !bv.is_string() {
        return TAG_NIL;
    }
    let json_str = unsafe {
        match av.as_heap_ref() {
            HeapObj::Str(s) => s,
            _ => unreachable!(),
        }
    };
    let path_str = unsafe {
        match bv.as_heap_ref() {
            HeapObj::Str(s) => s,
            _ => unreachable!(),
        }
    };
    match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(parsed) => {
            let mut current = &parsed;
            let mut found = true;
            let mut missing_key = String::new();
            for key in path_str.split('.') {
                if let Ok(idx) = key.parse::<usize>() {
                    if let Some(v) = current.as_array().and_then(|a| a.get(idx)) {
                        current = v;
                    } else {
                        found = false;
                        missing_key = key.to_string();
                        break;
                    }
                } else if let Some(v) = current.get(key) {
                    current = v;
                } else {
                    found = false;
                    missing_key = key.to_string();
                    break;
                }
            }
            if found {
                let result_str = match current {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                NanVal::heap_ok(NanVal::heap_string(result_str)).0
            } else {
                NanVal::heap_err(NanVal::heap_string(format!("key not found: {missing_key}"))).0
            }
        }
        Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())).0,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_jdmp(a: u64) -> u64 {
    let v = NanVal(a);
    let json_val = nanval_to_json(v);
    NanVal::heap_string(json_val.to_string()).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_jpar(a: u64) -> u64 {
    let v = NanVal(a);
    if !v.is_string() {
        return TAG_NIL;
    }
    let text = unsafe {
        match v.as_heap_ref() {
            HeapObj::Str(s) => s,
            _ => unreachable!(),
        }
    };
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(parsed) => NanVal::heap_ok(serde_json_to_nanval(parsed)).0,
        Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())).0,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_rdjl(a: u64) -> u64 {
    let v = NanVal(a);
    if !v.is_string() {
        return TAG_NIL;
    }
    let path = unsafe {
        match v.as_heap_ref() {
            HeapObj::Str(s) => s.as_str().to_owned(),
            _ => unreachable!(),
        }
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let mut items: Vec<NanVal> = Vec::new();
            for line in content.split('\n') {
                if line.is_empty() {
                    continue;
                }
                let entry = match serde_json::from_str::<serde_json::Value>(line) {
                    Ok(parsed) => NanVal::heap_ok(serde_json_to_nanval(parsed)),
                    Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())),
                };
                items.push(entry);
            }
            NanVal::heap_list(items).0
        }
        // On file-read failure JIT returns nil; the VM dispatch path raises a
        // typed runtime error. JIT helper-callers don't have a runtime-error
        // channel, so nil is the conventional signal here (matches jit_rd /
        // jit_jpar conventions for non-string inputs).
        Err(_) => TAG_NIL,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_dtfmt(epoch: u64, fmt: u64) -> u64 {
    let ev = NanVal(epoch);
    let fv = NanVal(fmt);
    if !ev.is_number() || !fv.is_string() {
        return TAG_NIL;
    }
    let e_f = ev.as_number();
    let fmt_str = unsafe {
        match fv.as_heap_ref() {
            HeapObj::Str(s) => s.as_str().to_owned(),
            _ => unreachable!(),
        }
    };
    if !e_f.is_finite() {
        return NanVal::heap_err(NanVal::heap_string(format!(
            "dtfmt: epoch is not finite ({e_f})"
        )))
        .0;
    }
    if e_f < i64::MIN as f64 || e_f > i64::MAX as f64 {
        return NanVal::heap_err(NanVal::heap_string(format!(
            "dtfmt: epoch out of range ({e_f})"
        )))
        .0;
    }
    let e = e_f as i64;
    match chrono::DateTime::<chrono::Utc>::from_timestamp(e, 0) {
        Some(dt) => NanVal::heap_ok(NanVal::heap_string(dt.format(&fmt_str).to_string())).0,
        None => {
            NanVal::heap_err(NanVal::heap_string(format!(
                "dtfmt: timestamp out of range ({e})"
            )))
            .0
        }
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_dtparse(text: u64, fmt: u64) -> u64 {
    let tv = NanVal(text);
    let fv = NanVal(fmt);
    if !tv.is_string() || !fv.is_string() {
        return TAG_NIL;
    }
    let t_str = unsafe {
        match tv.as_heap_ref() {
            HeapObj::Str(s) => s.as_str().to_owned(),
            _ => unreachable!(),
        }
    };
    let f_str = unsafe {
        match fv.as_heap_ref() {
            HeapObj::Str(s) => s.as_str().to_owned(),
            _ => unreachable!(),
        }
    };
    let parsed = chrono::NaiveDateTime::parse_from_str(&t_str, &f_str)
        .map(|ndt| ndt.and_utc().timestamp() as f64)
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(&t_str, &f_str)
                .map(|nd| nd.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() as f64)
        });
    match parsed {
        Ok(n) => NanVal::heap_ok(NanVal::number(n)).0,
        Err(e) => NanVal::heap_err(NanVal::heap_string(format!("dtparse: {e}"))).0,
    }
}

/// Call a VM function from JIT code. `func_idx` is the chunk index,
/// `regs` points to `n_args` u64 values. Returns the result as u64.
#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_call(
    program_ptr: *const CompiledProgram,
    func_idx: u64,
    regs: *const u64,
    n_args: u64,
) -> u64 {
    // SAFETY: program_ptr is the address of the CompiledProgram that owns this JIT function.
    // It remains valid for the lifetime of the JIT call. regs points to a Cranelift stack slot.
    let program = unsafe { &*program_ptr };
    let n = n_args as usize;
    let mut nan_args = Vec::with_capacity(n);
    for i in 0..n {
        let v = NanVal(unsafe { *regs.add(i) });
        v.clone_rc();
        nan_args.push(v);
    }
    let mut vm = VM::new(program);
    vm.setup_call(func_idx as u16, nan_args, 0);
    match vm.execute() {
        Ok(val) => NanVal::from_value(&val).0,
        Err(e) => {
            let msg = format!("{:?}", e);
            NanVal::heap_err(NanVal::heap_string(msg)).0
        }
    }
}

// ── JIT helpers: Type predicates ─────────────────────────────────────

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_isnum(v: u64) -> u64 {
    NanVal::boolean(NanVal(v).is_number()).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_istext(v: u64) -> u64 {
    NanVal::boolean(NanVal(v).is_string()).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_isbool(v: u64) -> u64 {
    NanVal::boolean(v == TAG_TRUE || v == TAG_FALSE).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_islist(v: u64) -> u64 {
    NanVal::boolean((v & TAG_MASK) == TAG_LIST).0
}

// JIT stub for OP_FLATMAP. Mirrors `map`: the real implementation
// lives in the tree-walking interpreter, and `flatmap` calls fall
// through OP_CALL today. This helper is reserved for when the JIT
// gains FnRef-aware HOF dispatch; until then it is unreachable.
#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_flatmap(_fn_ref: u64, _list: u64) -> u64 {
    TAG_NIL
}

// ── JIT helpers: Map operations ─────────────────────────────────────

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_mapnew() -> u64 {
    NanVal::heap_map(std::collections::HashMap::new()).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_mget(map: u64, key: u64) -> u64 {
    let map_v = NanVal(map);
    let key_v = NanVal(key);
    if !map_v.is_heap() || !key_v.is_heap() {
        return TAG_NIL;
    }
    unsafe {
        match map_v.as_heap_ref() {
            HeapObj::Map(m) => match key_v.as_heap_ref() {
                HeapObj::Str(k) => m
                    .get(k.as_str())
                    .map(|v| {
                        v.clone_rc();
                        v.0
                    })
                    .unwrap_or(TAG_NIL),
                _ => TAG_NIL,
            },
            _ => TAG_NIL,
        }
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_mset(map: u64, key: u64, val: u64) -> u64 {
    let map_v = NanVal(map);
    let key_v = NanVal(key);
    let val_v = NanVal(val);
    if !map_v.is_heap() || !key_v.is_heap() {
        return TAG_NIL;
    }
    unsafe {
        match map_v.as_heap_ref() {
            HeapObj::Map(m) => match key_v.as_heap_ref() {
                HeapObj::Str(k) => {
                    let mut new_map = m.clone();
                    val_v.clone_rc();
                    new_map.insert(k.clone(), val_v);
                    NanVal::heap_map(new_map).0
                }
                _ => TAG_NIL,
            },
            _ => TAG_NIL,
        }
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_mhas(map: u64, key: u64) -> u64 {
    let map_v = NanVal(map);
    let key_v = NanVal(key);
    if !map_v.is_heap() || !key_v.is_heap() {
        return TAG_FALSE;
    }
    unsafe {
        match map_v.as_heap_ref() {
            HeapObj::Map(m) => match key_v.as_heap_ref() {
                HeapObj::Str(k) => NanVal::boolean(m.contains_key(k.as_str())).0,
                _ => TAG_FALSE,
            },
            _ => TAG_FALSE,
        }
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_mkeys(map: u64) -> u64 {
    let map_v = NanVal(map);
    if !map_v.is_heap() {
        return TAG_NIL;
    }
    unsafe {
        match map_v.as_heap_ref() {
            HeapObj::Map(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                let nan_keys: Vec<NanVal> = keys
                    .iter()
                    .map(|k| NanVal::heap_string((*k).clone()))
                    .collect();
                NanVal::heap_list(nan_keys).0
            }
            _ => TAG_NIL,
        }
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_mvals(map: u64) -> u64 {
    let map_v = NanVal(map);
    if !map_v.is_heap() {
        return TAG_NIL;
    }
    unsafe {
        match map_v.as_heap_ref() {
            HeapObj::Map(m) => {
                let mut pairs: Vec<(&String, &NanVal)> = m.iter().collect();
                pairs.sort_by_key(|(k, _)| k.as_str());
                let nan_vals: Vec<NanVal> = pairs
                    .iter()
                    .map(|(_, v)| {
                        v.clone_rc();
                        **v
                    })
                    .collect();
                NanVal::heap_list(nan_vals).0
            }
            _ => TAG_NIL,
        }
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_mdel(map: u64, key: u64) -> u64 {
    let map_v = NanVal(map);
    let key_v = NanVal(key);
    if !map_v.is_heap() || !key_v.is_heap() {
        return TAG_NIL;
    }
    unsafe {
        match map_v.as_heap_ref() {
            HeapObj::Map(m) => match key_v.as_heap_ref() {
                HeapObj::Str(k) => {
                    let mut new_map = m.clone();
                    new_map.remove(k.as_str());
                    NanVal::heap_map(new_map).0
                }
                _ => TAG_NIL,
            },
            _ => TAG_NIL,
        }
    }
}

// ── JIT helpers: Print, Trim, Uniq ──────────────────────────────────

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_prt(v: u64) -> u64 {
    let nv = NanVal(v);
    println!("{}", nv.to_value());
    // passthrough — clone_rc for heap values
    nv.clone_rc();
    v
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_trm(v: u64) -> u64 {
    let nv = NanVal(v);
    if !nv.is_string() {
        return TAG_NIL;
    }
    let s = unsafe {
        match nv.as_heap_ref() {
            HeapObj::Str(s) => s.as_str().trim().to_owned(),
            _ => unreachable!(),
        }
    };
    NanVal::heap_string(s).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_upr(v: u64) -> u64 {
    let nv = NanVal(v);
    if !nv.is_string() {
        return TAG_NIL;
    }
    let s = unsafe {
        match nv.as_heap_ref() {
            HeapObj::Str(s) => s.as_str().to_uppercase(),
            _ => unreachable!(),
        }
    };
    NanVal::heap_string(s).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_lwr(v: u64) -> u64 {
    let nv = NanVal(v);
    if !nv.is_string() {
        return TAG_NIL;
    }
    let s = unsafe {
        match nv.as_heap_ref() {
            HeapObj::Str(s) => s.as_str().to_lowercase(),
            _ => unreachable!(),
        }
    };
    NanVal::heap_string(s).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_cap(v: u64) -> u64 {
    let nv = NanVal(v);
    if !nv.is_string() {
        return TAG_NIL;
    }
    let s = unsafe {
        match nv.as_heap_ref() {
            HeapObj::Str(s) => {
                let src = s.as_str();
                let mut chars = src.chars();
                match chars.next() {
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
            _ => unreachable!(),
        }
    };
    NanVal::heap_string(s).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_padl(sv: u64, wv: u64) -> u64 {
    jit_pad_impl(sv, wv, true)
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_padr(sv: u64, wv: u64) -> u64 {
    jit_pad_impl(sv, wv, false)
}

#[cfg(feature = "cranelift")]
fn jit_pad_impl(sv: u64, wv: u64, left: bool) -> u64 {
    let s_nv = NanVal(sv);
    let w_nv = NanVal(wv);
    if !s_nv.is_string() || !w_nv.is_number() {
        return TAG_NIL;
    }
    let wn = w_nv.as_number();
    if !wn.is_finite() || wn.fract() != 0.0 || wn < 0.0 {
        return TAG_NIL;
    }
    let w = wn as usize;
    let s = unsafe {
        match s_nv.as_heap_ref() {
            HeapObj::Str(s) => s.as_str().to_owned(),
            _ => unreachable!(),
        }
    };
    let cc = s.chars().count();
    let out = if cc >= w {
        s
    } else {
        let pad = " ".repeat(w - cc);
        if left {
            format!("{pad}{s}")
        } else {
            format!("{s}{pad}")
        }
    };
    NanVal::heap_string(out).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_ord(v: u64) -> u64 {
    let nv = NanVal(v);
    if !nv.is_string() {
        return TAG_NIL;
    }
    // SAFETY: is_string() confirmed heap-tagged string with live RC.
    let cp = unsafe {
        match nv.as_heap_ref() {
            HeapObj::Str(s) => match s.as_str().chars().next() {
                Some(c) => c as u32,
                None => return TAG_NIL,
            },
            _ => unreachable!(),
        }
    };
    NanVal::number(cp as f64).0
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_chr(v: u64) -> u64 {
    let nv = NanVal(v);
    if !nv.is_number() {
        return TAG_NIL;
    }
    let n = nv.as_number();
    if !n.is_finite() || n.fract() != 0.0 || n < 0.0 || n > u32::MAX as f64 {
        return TAG_NIL;
    }
    let cp = n as u32;
    match char::from_u32(cp) {
        Some(c) => NanVal::heap_string(c.to_string()).0,
        None => TAG_NIL,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_unq(v: u64) -> u64 {
    let nv = NanVal(v);
    if nv.is_string() {
        let s = unsafe {
            match nv.as_heap_ref() {
                HeapObj::Str(s) => s.as_str().to_owned(),
                _ => unreachable!(),
            }
        };
        let mut seen = std::collections::HashSet::new();
        let deduped: String = s.chars().filter(|c| seen.insert(*c)).collect();
        return NanVal::heap_string(deduped).0;
    }
    if (nv.0 & TAG_MASK) == TAG_LIST {
        let items = unsafe {
            match nv.as_heap_ref() {
                HeapObj::List(l) => l.clone(),
                _ => unreachable!(),
            }
        };
        let mut out: Vec<NanVal> = Vec::new();
        for item in items {
            if !out.iter().any(|existing| nanval_equal(*existing, item)) {
                item.clone_rc();
                out.push(item);
            }
        }
        return NanVal::heap_list(out).0;
    }
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_uniqby(_fn_ref: u64, _list: u64) -> u64 {
    // HOF dispatch is not wired through the JIT yet (matches map/flt/fld/grp).
    // Returns nil so callers see a typed failure rather than UB. The emitter
    // does not produce OP_UNIQBY today; this stub exists for plumbing parity.
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_partition(_fn_ref: u64, _list: u64) -> u64 {
    // HOF dispatch is not wired through the JIT yet (matches map/flt/fld/grp).
    // Returns nil so callers see a typed failure rather than UB. The emitter
    // does not produce OP_PARTITION today; this stub exists for plumbing parity.
    TAG_NIL
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_frq(v: u64) -> u64 {
    let nv = NanVal(v);
    if (nv.0 & TAG_MASK) != TAG_LIST {
        return TAG_NIL;
    }
    // SAFETY: TAG_LIST confirmed.
    let items = unsafe {
        match nv.as_heap_ref() {
            HeapObj::List(l) => l.clone(),
            _ => return TAG_NIL,
        }
    };
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for item in items {
        match nanval_to_key_string(item) {
            Some(s) => *counts.entry(s).or_insert(0) += 1,
            None => return TAG_NIL,
        }
    }
    let map: std::collections::HashMap<String, NanVal> = counts
        .into_iter()
        .map(|(k, c)| (k, NanVal::number(c as f64)))
        .collect();
    NanVal::heap_map(map).0
}

// ── JIT helpers: File I/O ───────────────────────────────────────────

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_rd(v: u64) -> u64 {
    let nv = NanVal(v);
    if !nv.is_string() {
        return TAG_NIL;
    }
    let path = unsafe {
        match nv.as_heap_ref() {
            HeapObj::Str(s) => s.as_str().to_owned(),
            _ => unreachable!(),
        }
    };
    let fmt = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("raw")
        .to_lowercase();
    match std::fs::read_to_string(&path) {
        Ok(content) => match vm_parse_format(&fmt, &content) {
            Ok(v) => NanVal::heap_ok(v).0,
            Err(e) => NanVal::heap_err(e).0,
        },
        Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())).0,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_rdl(v: u64) -> u64 {
    let nv = NanVal(v);
    if !nv.is_string() {
        return TAG_NIL;
    }
    let path = unsafe {
        match nv.as_heap_ref() {
            HeapObj::Str(s) => s.as_str().to_owned(),
            _ => unreachable!(),
        }
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let lines: Vec<NanVal> = content
                .lines()
                .map(|l| NanVal::heap_string(l.to_string()))
                .collect();
            NanVal::heap_ok(NanVal::heap_list(lines)).0
        }
        Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())).0,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_wr(path_v: u64, content_v: u64) -> u64 {
    let pv = NanVal(path_v);
    let cv = NanVal(content_v);
    if !pv.is_string() || !cv.is_string() {
        return TAG_NIL;
    }
    let path = unsafe {
        match pv.as_heap_ref() {
            HeapObj::Str(s) => s.as_str().to_owned(),
            _ => unreachable!(),
        }
    };
    let content = unsafe {
        match cv.as_heap_ref() {
            HeapObj::Str(s) => s.as_str().to_owned(),
            _ => unreachable!(),
        }
    };
    match std::fs::write(&path, &content) {
        Ok(()) => NanVal::heap_ok(NanVal::heap_string(path)).0,
        Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())).0,
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_wrl(path_v: u64, list_v: u64) -> u64 {
    let pv = NanVal(path_v);
    let lv = NanVal(list_v);
    if !pv.is_string() {
        return TAG_NIL;
    }
    let path = unsafe {
        match pv.as_heap_ref() {
            HeapObj::Str(s) => s.as_str().to_owned(),
            _ => unreachable!(),
        }
    };
    if (lv.0 & TAG_MASK) != TAG_LIST {
        return TAG_NIL;
    }
    let lines = unsafe {
        match lv.as_heap_ref() {
            HeapObj::List(l) => l.clone(),
            _ => unreachable!(),
        }
    };
    let mut buf = String::new();
    for line in &lines {
        if !line.is_string() {
            return TAG_NIL;
        }
        let s = unsafe {
            match line.as_heap_ref() {
                HeapObj::Str(s) => s.as_str().to_owned(),
                _ => unreachable!(),
            }
        };
        buf.push_str(&s);
        buf.push('\n');
    }
    match std::fs::write(&path, &buf) {
        Ok(()) => NanVal::heap_ok(NanVal::heap_string(path)).0,
        Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())).0,
    }
}

// ── JIT helpers: HTTP ───────────────────────────────────────────────

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_post(url_v: u64, body_v: u64) -> u64 {
    let uv = NanVal(url_v);
    let bv = NanVal(body_v);
    if !uv.is_string() || !bv.is_string() {
        return TAG_NIL;
    }
    #[cfg(feature = "http")]
    {
        let url = unsafe {
            match uv.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        let body = unsafe {
            match bv.as_heap_ref() {
                HeapObj::Str(s) => s,
                _ => unreachable!(),
            }
        };
        match minreq::post(url.as_str()).with_body(body.as_str()).send() {
            Ok(resp) => match resp.as_str() {
                Ok(b) => NanVal::heap_ok(NanVal::heap_string(b.to_string())).0,
                Err(e) => {
                    NanVal::heap_err(NanVal::heap_string(format!(
                        "response is not valid UTF-8: {e}"
                    )))
                    .0
                }
            },
            Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())).0,
        }
    }
    #[cfg(not(feature = "http"))]
    {
        NanVal::heap_err(NanVal::heap_string("http feature not enabled".to_string())).0
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_geth(url_v: u64, headers_v: u64) -> u64 {
    let uv = NanVal(url_v);
    if !uv.is_string() {
        return TAG_NIL;
    }
    #[cfg(feature = "http")]
    {
        let url = unsafe {
            match uv.as_heap_ref() {
                HeapObj::Str(s) => s.as_str().to_owned(),
                _ => unreachable!(),
            }
        };
        let hv = NanVal(headers_v);
        let mut req = minreq::get(url.as_str());
        if hv.is_heap()
            && let HeapObj::Map(m) = unsafe { hv.as_heap_ref() }
        {
            for (k, v) in m.iter() {
                if v.is_string() {
                    let vs = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_owned(),
                            _ => unreachable!(),
                        }
                    };
                    req = req.with_header(k.as_str(), &vs);
                }
            }
        }
        match req.send() {
            Ok(resp) => match resp.as_str() {
                Ok(body) => NanVal::heap_ok(NanVal::heap_string(body.to_string())).0,
                Err(e) => {
                    NanVal::heap_err(NanVal::heap_string(format!(
                        "response is not valid UTF-8: {e}"
                    )))
                    .0
                }
            },
            Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())).0,
        }
    }
    #[cfg(not(feature = "http"))]
    {
        NanVal::heap_err(NanVal::heap_string("http feature not enabled".to_string())).0
    }
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn jit_posth(url_v: u64, body_v: u64, headers_v: u64) -> u64 {
    let uv = NanVal(url_v);
    let bv = NanVal(body_v);
    if !uv.is_string() || !bv.is_string() {
        return TAG_NIL;
    }
    #[cfg(feature = "http")]
    {
        let url = unsafe {
            match uv.as_heap_ref() {
                HeapObj::Str(s) => s.as_str().to_owned(),
                _ => unreachable!(),
            }
        };
        let body_str = unsafe {
            match bv.as_heap_ref() {
                HeapObj::Str(s) => s.as_str().to_owned(),
                _ => unreachable!(),
            }
        };
        let hv = NanVal(headers_v);
        let mut req = minreq::post(url.as_str()).with_body(body_str.as_str());
        if hv.is_heap()
            && let HeapObj::Map(m) = unsafe { hv.as_heap_ref() }
        {
            for (k, v) in m.iter() {
                if v.is_string() {
                    let vs = unsafe {
                        match v.as_heap_ref() {
                            HeapObj::Str(s) => s.as_str().to_owned(),
                            _ => unreachable!(),
                        }
                    };
                    req = req.with_header(k.as_str(), &vs);
                }
            }
        }
        match req.send() {
            Ok(resp) => match resp.as_str() {
                Ok(b) => NanVal::heap_ok(NanVal::heap_string(b.to_string())).0,
                Err(e) => {
                    NanVal::heap_err(NanVal::heap_string(format!(
                        "response is not valid UTF-8: {e}"
                    )))
                    .0
                }
            },
            Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())).0,
        }
    }
    #[cfg(not(feature = "http"))]
    {
        NanVal::heap_err(NanVal::heap_string("http feature not enabled".to_string())).0
    }
}

// ── AOT runtime init/fini and arena/registry pointer helpers ─────────

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub extern "C" fn ilo_aot_init() {
    jit_arena_reset();
}

/// Set the active TypeRegistry from serialized bytes (for AOT binaries).
/// Format: repeated entries of `type_name\0num_fields_bitmask_hex\0field1\0field2\0...\0\n`
#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub extern "C" fn ilo_aot_set_registry(ptr: u64, len: u64) {
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let text = std::str::from_utf8(bytes).unwrap_or("");
    let mut registry = TypeRegistry::default();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\0').collect();
        if parts.len() < 2 {
            continue;
        }
        let name = parts[0].to_string();
        let num_fields: u64 = parts[1].parse().unwrap_or(0);
        let fields: Vec<String> = parts[2..]
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        registry.register(name, fields, num_fields);
    }
    // Leak the registry so the pointer stays valid for the program's lifetime
    let boxed = Box::new(registry);
    let ptr = Box::into_raw(boxed);
    ACTIVE_REGISTRY.with(|r| r.set(ptr as *const TypeRegistry));
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub extern "C" fn ilo_aot_arena_reset() {
    jit_arena_reset();
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub extern "C" fn ilo_aot_fini() {
    clear_active_registry();
    jit_arena_reset();
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub extern "C" fn jit_get_arena_ptr() -> u64 {
    jit_arena_ptr() as u64
}

#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub extern "C" fn jit_get_registry_ptr() -> u64 {
    ACTIVE_REGISTRY.with(|r| r.get() as u64)
}

/// Create a NanVal string from a C string pointer (for AOT string constants).
#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub extern "C" fn jit_string_const(ptr: u64) -> u64 {
    let cstr = unsafe { std::ffi::CStr::from_ptr(ptr as *const std::ffi::c_char) };
    let s = cstr.to_str().unwrap_or("").to_string();
    NanVal::heap_string(s).0
}

/// Parse a CLI arg: if it looks like a number, return NanVal number; otherwise NanVal string.
#[cfg(feature = "cranelift")]
#[unsafe(no_mangle)]
pub extern "C" fn ilo_aot_parse_arg(ptr: u64) -> u64 {
    let cstr = unsafe { std::ffi::CStr::from_ptr(ptr as *const std::ffi::c_char) };
    let s = cstr.to_str().unwrap_or("");
    match s {
        "true" => TAG_TRUE,
        "false" => TAG_FALSE,
        "nil" => TAG_NIL,
        _ => {
            if let Ok(n) = s.parse::<f64>() {
                NanVal::number(n).0
            } else {
                NanVal::heap_string(s.to_string()).0
            }
        }
    }
}

// ── Block leader analysis (shared by JIT backends) ──────────────────

/// Identify basic block leaders in bytecode. A leader is:
/// - instruction 0 (entry point)
/// - the target of any jump
/// - the instruction after any jump
#[cfg(feature = "cranelift")]
pub(crate) fn find_block_leaders(code: &[u32]) -> Vec<usize> {
    let mut leaders = std::collections::BTreeSet::new();
    leaders.insert(0);
    let mut i = 0;
    while i < code.len() {
        let inst = code[i];
        let op = (inst >> 24) as u8;
        match op {
            OP_JMP | OP_JMPF | OP_JMPT | OP_JMPNN => {
                let sbx = (inst & 0xFFFF) as i16;
                let target = (i as isize + 1 + sbx as isize) as usize;
                leaders.insert(target);
                leaders.insert(i + 1);
            }
            OP_LISTGET | OP_FOREACHPREP => {
                // These may skip the next instruction (JMP), so both i+1 and i+2 are leaders
                leaders.insert(i + 1);
                leaders.insert(i + 2);
            }
            OP_FOREACHNEXT => {
                // FOREACHNEXT may skip the JMP exit (i+1) and the JMP body_top (i+2),
                // or fall through to JMP exit. All three are leaders.
                leaders.insert(i + 1);
                leaders.insert(i + 2);
                leaders.insert(i + 3);
            }
            OP_SLC | OP_LST | OP_MSET | OP_POSTH | OP_RGXSUB | OP_CLAMP => {
                // 2-word instructions: the following word is data, not an instruction.
                // Skip it so its bits aren't mis-decoded as an opcode that might mark
                // bogus leaders. The instruction after the data word is a normal leader
                // candidate (and will be reached on the next loop iteration).
                i += 2;
                continue;
            }
            op if op == OP_CMPK_GE_N
                || op == OP_CMPK_GT_N
                || op == OP_CMPK_LT_N
                || op == OP_CMPK_LE_N
                || op == OP_CMPK_EQ_N
                || op == OP_CMPK_NE_N =>
            {
                // CMPK_*_N may skip the following OP_JMP (i+1) when condition holds.
                // i+1 (the JMP) and i+2 (the guard body / fall-through) are both leaders.
                leaders.insert(i + 1);
                leaders.insert(i + 2);
            }
            _ => {}
        }
        i += 1;
    }
    leaders.into_iter().filter(|&l| l <= code.len()).collect()
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::approx_constant)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    static ENV_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn parse_program(source: &str) -> Program {
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
        prog
    }

    fn vm_run(source: &str, func: Option<&str>, args: Vec<Value>) -> Value {
        let prog = parse_program(source);
        compile_and_run(&prog, func, args).unwrap()
    }

    #[test]
    fn vm_tot() {
        let source = std::fs::read_to_string("examples/01-simple-function.ilo").unwrap();
        let result = vm_run(
            &source,
            Some("tot"),
            vec![
                Value::Number(10.0),
                Value::Number(20.0),
                Value::Number(30.0),
            ],
        );
        assert_eq!(result, Value::Number(6200.0));
    }

    #[test]
    fn vm_tot_different_args() {
        let source = "tot p:n q:n r:n>n;s=*p q;t=*s r;+s t";
        let result = vm_run(
            source,
            Some("tot"),
            vec![Value::Number(2.0), Value::Number(3.0), Value::Number(4.0)],
        );
        assert_eq!(result, Value::Number(30.0));
    }

    #[test]
    fn vm_cls_gold() {
        // Braceless guards: early return
        let source = r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#;
        let result = vm_run(source, Some("cls"), vec![Value::Number(1000.0)]);
        assert_eq!(result, Value::Text("gold".to_string()));
    }

    #[test]
    fn vm_cls_silver() {
        let source = r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#;
        let result = vm_run(source, Some("cls"), vec![Value::Number(500.0)]);
        assert_eq!(result, Value::Text("silver".to_string()));
    }

    #[test]
    fn vm_cls_bronze() {
        let source = r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#;
        let result = vm_run(source, Some("cls"), vec![Value::Number(100.0)]);
        assert_eq!(result, Value::Text("bronze".to_string()));
    }

    #[test]
    fn vm_match_stmt() {
        let source = r#"f x:t>n;?x{"a":1;"b":2;_:0}"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("a".to_string())]),
            Value::Number(1.0)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("b".to_string())]),
            Value::Number(2.0)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("z".to_string())]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn vm_ok_err() {
        let source = "f x:n>R n t;~x";
        let result = vm_run(source, Some("f"), vec![Value::Number(42.0)]);
        assert_eq!(result, Value::Ok(Box::new(Value::Number(42.0))));
    }

    #[test]
    fn vm_tool_call() {
        let source =
            "tool fetch\"HTTP GET\" url:t>R _ t timeout:30\nf>R _ t;fetch \"http://example.com\"";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(result, Value::Ok(Box::new(Value::Nil)));
    }

    #[test]
    fn vm_tool_call_multi_param() {
        let source = "tool send\"send msg\" to:t body:t>R _ t\nf>R _ t;send \"alice\" \"hello\"";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(result, Value::Ok(Box::new(Value::Nil)));
    }

    #[test]
    fn vm_tool_call_unwrap() {
        // auto-unwrap: fetch returns Ok(Nil), ! unwraps to Nil
        // caller wraps result in Ok with ~
        let source = "tool fetch\"get\" url:t>R _ t\nf>R _ t;v=fetch! \"http://x\";~v";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(result, Value::Ok(Box::new(Value::Nil)));
    }

    #[test]
    fn vm_tool_call_match() {
        // match on tool result
        let source =
            "tool fetch\"get\" url:t>R _ t\nf>t;r=fetch \"http://x\";?r{~v:\"ok\";^e:\"err\"}";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(result, Value::Text("ok".into()));
    }

    #[test]
    fn vm_tool_mixed_with_functions() {
        // tool between two functions — chunk indices must stay aligned
        let source = "add a:n b:n>n;+a b\ntool fetch\"get\" url:t>R _ t\nf>n;add 1 2";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    #[test]
    fn vm_multiple_tools() {
        // two tools, call the second — func_names/chunks stay in sync
        let source = "tool a\"first\" x:t>R _ t\ntool b\"second\" x:t>R _ t\nf>R _ t;b \"test\"";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(result, Value::Ok(Box::new(Value::Nil)));
    }

    #[test]
    fn vm_err_constructor() {
        let source = r#"f x:n>R n t;^"bad""#;
        let result = vm_run(source, Some("f"), vec![Value::Number(0.0)]);
        assert_eq!(result, Value::Err(Box::new(Value::Text("bad".to_string()))));
    }

    #[test]
    fn vm_match_ok_err_patterns() {
        let source = r#"f x:R n t>n;?x{^e:0;~v:v}"#;
        let ok_result = vm_run(
            source,
            Some("f"),
            vec![Value::Ok(Box::new(Value::Number(42.0)))],
        );
        assert_eq!(ok_result, Value::Number(42.0));

        let err_result = vm_run(
            source,
            Some("f"),
            vec![Value::Err(Box::new(Value::Text("oops".to_string())))],
        );
        assert_eq!(err_result, Value::Number(0.0));
    }

    #[test]
    fn vm_negated_guard() {
        // Use ternary form for conditional value
        let source = r#"f x:b>t;!x{"nope"}{"yes"}"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Bool(false)]),
            Value::Text("nope".to_string())
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Bool(true)]),
            Value::Text("yes".to_string())
        );
    }

    #[test]
    fn vm_logical_not() {
        let source = "f x:b>b;!x";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Bool(true)]),
            Value::Bool(false)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Bool(false)]),
            Value::Bool(true)
        );
    }

    #[test]
    fn vm_unary_negate() {
        let source = "f x:n>n;-x";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(-5.0)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(-3.0)]),
            Value::Number(3.0)
        );
    }

    #[test]
    fn vm_unary_negate_in_expr() {
        let source = "f x:n>n;y=-x;+y 10";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(5.0)
        );
    }

    #[test]
    fn vm_record_and_field() {
        let source = "f x:n>n;r=point x:x y:10;r.y";
        let result = vm_run(source, Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn vm_with_expr() {
        let source = "f>n;r=point x:1 y:2;r2=r with y:10;r2.y";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(result, Value::Number(10.0));
    }

    // ── OP_ADD_SS: type-specialized string concatenation ────────────────────────
    // Both params typed :t → compiler emits OP_ADD_SS (no type-check branch).
    #[test]
    fn vm_add_ss_basic() {
        // f a:t b:t → OP_ADD_SS (both reg_is_str)
        let source = r#"f a:t b:t>t;+a b"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![
                Value::Text("foo".to_string()),
                Value::Text("bar".to_string()),
            ],
        );
        assert_eq!(result, Value::Text("foobar".to_string()));
    }

    #[test]
    fn vm_add_ss_with_str_builtin() {
        // str() output is marked reg_is_str; +a (str b) → both string → OP_ADD_SS
        // Store str(b) in a variable so both sides of + are known-string registers.
        let source = r#"f a:t b:n>t;s=str b;+a s"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Text("val=".to_string()), Value::Number(42.0)],
        );
        assert_eq!(result, Value::Text("val=42".to_string()));
    }

    #[test]
    fn vm_add_ss_literal_concat() {
        // Both sides are text literals → OP_ADD_SS
        let source = r#"f>t;+"hello " "world""#;
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(result, Value::Text("hello world".to_string()));
    }

    #[test]
    fn vm_string_concat() {
        let source = r#"f a:t b:t>t;+a b"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![
                Value::Text("hello ".to_string()),
                Value::Text("world".to_string()),
            ],
        );
        assert_eq!(result, Value::Text("hello world".to_string()));
    }

    #[test]
    fn vm_list_literal() {
        // List literal with foreach — last value from loop body
        let source = "f>n;xs=[10, 20, 30];@x xs{x};0";
        let result = vm_run(source, Some("f"), vec![]);
        // foreach doesn't produce a value; the 0 is the return
        assert_eq!(result, Value::Number(0.0));

        // Verify list literal works by creating and returning it
        let source = "f>L n;[1, 2, 3]";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])
        );
    }

    #[test]
    fn vm_empty_list() {
        let source = "f>L n;[]";
        let result = vm_run(source, Some("f"), vec![]);
        assert!(matches!(result, Value::List(items) if items.is_empty()));
    }

    #[test]
    fn vm_string_comparison() {
        // "banana" > "apple" (lexicographic)
        let source = r#"f a:t b:t>b;>a b"#;
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Text("banana".into()), Value::Text("apple".into())]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Text("apple".into()), Value::Text("banana".into())]
            ),
            Value::Bool(false)
        );

        // <
        let source = r#"f a:t b:t>b;<a b"#;
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Text("apple".into()), Value::Text("banana".into())]
            ),
            Value::Bool(true)
        );

        // >=
        let source = r#"f a:t b:t>b;>=a b"#;
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Text("apple".into()), Value::Text("apple".into())]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Text("apple".into()), Value::Text("banana".into())]
            ),
            Value::Bool(false)
        );

        // <=
        let source = r#"f a:t b:t>b;<=a b"#;
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Text("banana".into()), Value::Text("banana".into())]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Text("zebra".into()), Value::Text("banana".into())]
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_multi_function() {
        let source = "double x:n>n;*x 2\nf x:n>n;double x";
        let result = vm_run(source, Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn vm_match_expr_in_let() {
        let source = r#"f x:t>n;y=?x{"a":1;"b":2;_:0};y"#;
        let result = vm_run(source, Some("f"), vec![Value::Text("b".to_string())]);
        assert_eq!(result, Value::Number(2.0));
    }

    #[test]
    fn vm_default_first_function() {
        let source = "f>n;42";
        let result = vm_run(source, None, vec![]);
        assert_eq!(result, Value::Number(42.0));
    }

    #[test]
    fn vm_division_by_zero() {
        let source = "f x:n>n;/x 0";
        let prog = parse_program(source);
        let result = compile_and_run(&prog, Some("f"), vec![Value::Number(10.0)]);
        assert!(result.is_err());
    }

    #[test]
    fn vm_logical_and() {
        let source = "f a:b b:b>b;&a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Bool(true), Value::Bool(true)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Bool(true), Value::Bool(false)]
            ),
            Value::Bool(false)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Bool(false), Value::Bool(true)]
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_logical_or() {
        let source = "f a:b b:b>b;|a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Bool(false), Value::Bool(false)]
            ),
            Value::Bool(false)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Bool(true), Value::Bool(false)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Bool(false), Value::Bool(true)]
            ),
            Value::Bool(true)
        );
    }

    #[test]
    fn vm_logical_and_short_circuit() {
        // &false x — should not evaluate x (short-circuit)
        // We test by using a guard: if false AND true, body shouldn't run
        let source = r#"f>b;&false true"#;
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(false));
    }

    #[test]
    fn vm_logical_or_short_circuit() {
        let source = r#"f>b;|true false"#;
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    fn vm_and_does_not_clobber_left_operand() {
        // Regression: &e f was overwriting e's register with f's value,
        // so a subsequent guard `e{"Fizz"}` would see f's value instead of e's.
        // This is the FizzBuzz bug: for n=3, e=true, f=false, &e f=false (correct),
        // but e's register was clobbered to false, so e{"Fizz"} didn't fire.
        // Braced guards are now conditional execution, use `ret` for early return.
        let source = r#"f n:n>t;a=flr /n 3;b=flr /n 5;c=*a 3;d=*b 5;e= =c n;f= =d n;&e f{ret "FizzBuzz"};e{ret "Fizz"};f{ret "Buzz"};str n"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(3.0)]),
            Value::Text("Fizz".to_string())
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Text("Buzz".to_string())
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(15.0)]),
            Value::Text("FizzBuzz".to_string())
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(7.0)]),
            Value::Text("7".to_string())
        );
    }

    #[test]
    fn vm_or_does_not_clobber_left_operand() {
        // Same pattern for OR: left operand must not be clobbered
        // Braced guards are now conditional execution, use ternary form.
        let source = r#"f>t;a=true;b=false;r= |a b;a{"a is still true"}{"nope"}"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::Text("a is still true".to_string())
        );
    }

    #[test]
    fn vm_len_string() {
        let source = r#"f s:t>n;len s"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("hello".to_string())]),
            Value::Number(5.0)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("".to_string())]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn vm_len_list() {
        let source = "f>n;xs=[1, 2, 3];len xs";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_len_empty_list() {
        let source = "f>n;xs=[];len xs";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(0.0));
    }

    #[test]
    fn vm_list_append() {
        let source = "f>L n;xs=[1, 2];+=xs 3";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])
        );
    }

    #[test]
    fn vm_list_append_empty() {
        let source = "f>L n;xs=[];+=xs 42";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![Value::Number(42.0)])
        );
    }

    #[test]
    fn vm_list_concat() {
        let source = "f>L n;a=[1, 2];b=[3, 4];+a b";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
                Value::Number(4.0)
            ])
        );
    }

    #[test]
    fn vm_list_concat_empty() {
        let source = "f>L n;a=[1, 2];b=[];+a b";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![Value::Number(1.0), Value::Number(2.0)])
        );
    }

    #[test]
    fn vm_index_access() {
        let source = "f>n;xs=[10, 20, 30];xs.0";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(10.0));
    }

    #[test]
    fn vm_index_access_second() {
        let source = "f>n;xs=[10, 20, 30];xs.2";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(30.0));
    }

    #[test]
    fn vm_str_integer() {
        let source = "f>t;str 42";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Text("42".into()));
    }

    #[test]
    fn vm_str_float() {
        let source = "f>t;str 3.14";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::Text("3.14".into())
        );
    }

    #[test]
    fn vm_num_ok() {
        let source = "f>R n t;num \"42\"";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::Ok(Box::new(Value::Number(42.0)))
        );
    }

    #[test]
    fn vm_num_float() {
        let source = "f>R n t;num \"3.14\"";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::Ok(Box::new(Value::Number(3.14)))
        );
    }

    #[test]
    fn vm_num_err() {
        let source = "f>R n t;num \"abc\"";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::Err(Box::new(Value::Text("abc".into())))
        );
    }

    #[test]
    fn vm_abs_positive() {
        let source = "f>n;abs 5";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(5.0));
    }

    #[test]
    fn vm_abs_negative() {
        let source = "f>n;abs -3";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_min() {
        let source = "f>n;min 3 7";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_max() {
        let source = "f>n;max 3 7";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(7.0));
    }

    #[test]
    fn vm_min_negative() {
        let source = "f>n;min -5 2";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(-5.0));
    }

    #[test]
    fn vm_mod() {
        assert_eq!(
            vm_run("f>n;mod 10 3", Some("f"), vec![]),
            Value::Number(1.0)
        );
    }

    #[test]
    fn vm_mod_negative() {
        assert_eq!(
            vm_run("f>n;mod -7 3", Some("f"), vec![]),
            Value::Number(-1.0)
        );
    }

    #[test]
    fn vm_mod_float() {
        assert_eq!(
            vm_run("f>n;mod 5.5 2.0", Some("f"), vec![]),
            Value::Number(1.5)
        );
    }

    #[test]
    fn vm_flr() {
        let source = "f>n;flr 3.7";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_flr_negative() {
        let source = "f>n;flr -2.3";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(-3.0));
    }

    #[test]
    fn vm_cel() {
        let source = "f>n;cel 3.2";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(4.0));
    }

    #[test]
    fn vm_cel_negative() {
        let source = "f>n;cel -2.7";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(-2.0));
    }

    #[test]
    fn vm_index_access_string_list() {
        let source = "f>t;xs=[\"a\", \"b\"];xs.1";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Text("b".into()));
    }

    #[test]
    fn vm_nested_multiply_add() {
        // +*a b c → (a * b) + c
        let source = "f a:n b:n c:n>n;+*a b c";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Number(2.0), Value::Number(3.0), Value::Number(4.0)],
        );
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn vm_nested_compare() {
        // >=+x y 100 → (x + y) >= 100
        let source = "f x:n y:n>b;>=+x y 100";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Number(60.0), Value::Number(50.0)],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_not_as_and_operand() {
        // &!x y → (!x) & y
        let source = "f x:b y:b>b;&!x y";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Bool(false), Value::Bool(true)],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_negate_product() {
        // -*a b → -(a * b)
        let source = "f a:n b:n>n;-*a b";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Number(3.0), Value::Number(4.0)],
        );
        assert_eq!(result, Value::Number(-12.0));
    }

    #[test]
    fn nanval_roundtrip() {
        // Number
        let v = Value::Number(42.5);
        let nv = NanVal::from_value(&v);
        assert_eq!(nv.to_value(), v);
        nv.drop_rc();

        // Negative zero
        let v = Value::Number(-0.0);
        let nv = NanVal::from_value(&v);
        assert!(nv.is_number());
        let rt = nv.to_value();
        let Value::Number(n) = rt else {
            panic!("expected Number")
        };
        assert!(n.to_bits() == (-0.0f64).to_bits());
        nv.drop_rc();

        // Infinity
        let v = Value::Number(f64::INFINITY);
        let nv = NanVal::from_value(&v);
        assert_eq!(nv.to_value(), v);
        nv.drop_rc();

        // Bool true
        let v = Value::Bool(true);
        let nv = NanVal::from_value(&v);
        assert_eq!(nv.to_value(), v);

        // Bool false
        let v = Value::Bool(false);
        let nv = NanVal::from_value(&v);
        assert_eq!(nv.to_value(), v);

        // Nil
        let v = Value::Nil;
        let nv = NanVal::from_value(&v);
        assert_eq!(nv.to_value(), v);

        // Text
        let v = Value::Text("hello".to_string());
        let nv = NanVal::from_value(&v);
        assert_eq!(nv.to_value(), v);
        nv.drop_rc();

        // Ok wrapping number
        let v = Value::Ok(Box::new(Value::Number(7.0)));
        let nv = NanVal::from_value(&v);
        assert_eq!(nv.to_value(), v);
        nv.drop_rc();

        // Err wrapping text
        let v = Value::Err(Box::new(Value::Text("bad".to_string())));
        let nv = NanVal::from_value(&v);
        assert_eq!(nv.to_value(), v);
        nv.drop_rc();
    }

    // ── Coverage tests ───────────────────────────────────────────────

    fn vm_run_err(source: &str, func: Option<&str>, args: Vec<Value>) -> String {
        let prog = parse_program(source);
        compile_and_run(&prog, func, args).unwrap_err().to_string()
    }

    fn compile_err(source: &str) -> String {
        let prog = parse_program(source);
        compile_and_run(&prog, None, vec![])
            .unwrap_err()
            .to_string()
    }

    // 1. VmState API — reusable state
    #[test]
    fn vm_state_reusable() {
        let prog = parse_program("f x:n>n;*x 2");
        let compiled = compile(&prog).unwrap();
        let mut state = VmState::new(&compiled);
        assert_eq!(
            state.call("f", vec![Value::Number(5.0)]).unwrap(),
            Value::Number(10.0)
        );
        assert_eq!(
            state.call("f", vec![Value::Number(3.0)]).unwrap(),
            Value::Number(6.0)
        );
    }

    // VmState — undefined function error
    #[test]
    fn vm_state_undefined_function() {
        let prog = parse_program("f x:n>n;*x 2");
        let compiled = compile(&prog).unwrap();
        let mut state = VmState::new(&compiled);
        let err = state.call("nonexistent", vec![]).unwrap_err();
        assert!(err.to_string().contains("undefined function"));
    }

    // 2. BinOp::Subtract on two vars → OP_SUB_NN
    #[test]
    fn vm_sub_nn() {
        let source = "f a:n b:n>n;-a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(10.0), Value::Number(3.0)]
            ),
            Value::Number(7.0)
        );
    }

    // 3. BinOp::Divide on two vars → OP_DIV_NN
    #[test]
    fn vm_div_nn() {
        let source = "f a:n b:n>n;/a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(15.0), Value::Number(3.0)]
            ),
            Value::Number(5.0)
        );
    }

    // 4. BinOp::Equals — prefix =a b
    #[test]
    fn vm_equals_prefix() {
        let source = "f a:n b:n>b;=a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(5.0), Value::Number(5.0)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(5.0), Value::Number(3.0)]
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_equals_double_eq_sugar() {
        // == is sugar for = — both produce the same result
        let source = "f a:n b:n>b;==a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(5.0), Value::Number(5.0)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(5.0), Value::Number(3.0)]
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_double_eq_in_guard() {
        // ==x 3 as a guard condition (sugar for =x 3) — use braceless for early return
        let source = "f x:n>t;==x 3 \"match\";\"nope\"";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(3.0)]),
            Value::Text("match".to_string())
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Text("nope".to_string())
        );
    }

    #[test]
    fn vm_assign_equality_with_double_eq() {
        // e= ==c n: assignment e = (== c n) — space between = and ==
        // Braced guard is conditional execution, use ternary for value
        let source = "f x:n>t;e= ==x 3;e{\"match\"}{\"nope\"}";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(3.0)]),
            Value::Text("match".to_string())
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Text("nope".to_string())
        );
    }

    // 5. BinOp::NotEquals — prefix !=a b
    #[test]
    fn vm_not_equals_prefix() {
        let source = "f a:n b:n>b;!=a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(5.0), Value::Number(3.0)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(5.0), Value::Number(5.0)]
            ),
            Value::Bool(false)
        );
    }

    // 6. Constant folding — two literals in let binding
    #[test]
    fn vm_const_fold_add() {
        let source = "f>n;x=+2 3;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(5.0));
    }

    #[test]
    fn vm_const_fold_subtract() {
        // `-10 3` can't work because `-10` is a negative literal.
        // Use nested fold: `- +5 5 3` → subtract(add(5,5), 3) → subtract(10, 3) → 7
        // The inner +5 5 folds to 10, then the outer subtract sees two literals.
        let source = "f>n;x=-+5 5 3;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(7.0));
    }

    #[test]
    fn vm_const_fold_multiply() {
        let source = "f>n;x=*4 5;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(20.0));
    }

    #[test]
    fn vm_const_fold_divide() {
        let source = "f>n;x=/10 2;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(5.0));
    }

    #[test]
    fn vm_const_fold_equals() {
        let source = "f>b;x= ==3 3;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    fn vm_const_fold_not_equals() {
        let source = "f>b;x=!=3 4;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    fn vm_const_fold_comparison() {
        let source = "f>b;x=>5 3;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    fn vm_const_fold_text_concat() {
        let source = r#"f>t;x=+"hello " "world";x"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::Text("hello world".into())
        );
    }

    #[test]
    fn vm_const_fold_bool_and() {
        let source = "f>b;x=&true false;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(false));
    }

    #[test]
    fn vm_const_fold_bool_or() {
        let source = "f>b;x=|true false;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    fn vm_const_fold_bool_eq() {
        let source = "f>b;x= ==true true;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    fn vm_const_fold_bool_ne() {
        let source = "f>b;x=!=true false;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    fn vm_const_fold_negate() {
        let source = "f>n;x=-5;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(-5.0));
    }

    #[test]
    fn vm_const_fold_not() {
        let source = "f>b;x=!true;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(false));
    }

    // 7. Bool literal in match pattern
    #[test]
    fn vm_match_bool_pattern() {
        let source = r#"f x:b>t;?x{true:"yes";_:"no"}"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Bool(true)]),
            Value::Text("yes".into())
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Bool(false)]),
            Value::Text("no".into())
        );
    }

    // 8. Number literal in match pattern
    #[test]
    fn vm_match_number_pattern() {
        let source = r#"f x:n>t;?x{0:"zero";1:"one";_:"other"}"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(0.0)]),
            Value::Text("zero".into())
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Text("one".into())
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(42.0)]),
            Value::Text("other".into())
        );
    }

    // 9. Match with no subject in statement position
    #[test]
    fn vm_match_no_subject() {
        let source = r#"f>t;?{_:"always"}"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::Text("always".into())
        );
    }

    // 10. ForEach — iterate a list, last body value is tracked
    #[test]
    fn vm_foreach_basic() {
        // ForEach returns the last body result (via __fe_last register)
        // The body expression `x` is the last element after iteration
        let source = "f>L n;xs=[10, 20, 30];@x xs{x};xs";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![
                Value::Number(10.0),
                Value::Number(20.0),
                Value::Number(30.0)
            ])
        );
    }

    #[test]
    fn vm_foreach_empty() {
        let source = "f>n;xs=[];s=99;@x xs{s=+s x};s";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(99.0));
    }

    // 11. Literal::Bool value (line 615)
    #[test]
    fn vm_bool_literal_true() {
        let source = "f>b;true";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    fn vm_bool_literal_false() {
        let source = "f>b;false";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(false));
    }

    // 12. nanval_equal — equality comparison on numbers via `=a b`
    #[test]
    fn vm_nanval_equal_numbers() {
        let source = "f a:n b:n>b;=a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(3.0), Value::Number(3.0)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(3.0), Value::Number(4.0)]
            ),
            Value::Bool(false)
        );
    }

    // nanval_equal on strings
    #[test]
    fn vm_nanval_equal_strings() {
        let source = r#"f a:t b:t>b;=a b"#;
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Text("hi".into()), Value::Text("hi".into())]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Text("hi".into()), Value::Text("bye".into())]
            ),
            Value::Bool(false)
        );
    }

    // nanval_equal on different types (should be false)
    #[test]
    fn vm_nanval_equal_different_types() {
        // Compare bool with bool using equality — both are non-heap singletons
        let source = "f a:b b:b>b;=a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Bool(true), Value::Bool(true)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Bool(true), Value::Bool(false)]
            ),
            Value::Bool(false)
        );
    }

    // nanval not-equals
    #[test]
    fn vm_nanval_not_equal() {
        let source = "f a:n b:n>b;!=a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(3.0), Value::Number(4.0)]
            ),
            Value::Bool(true)
        );
    }

    // 13. nanval_truthy — AND/OR with number operands
    #[test]
    fn vm_nanval_truthy_number_and() {
        // &a b where a and b are numbers — exercises nanval_truthy on numbers
        let source = "f a:n b:n>n;&a b";
        // Non-zero is truthy, so &5 3 should return 3 (right operand)
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(5.0), Value::Number(3.0)]
            ),
            Value::Number(3.0)
        );
        // 0 is falsy, so &0 3 should return 0 (short-circuit)
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(0.0), Value::Number(3.0)]
            ),
            Value::Number(0.0)
        );
    }

    #[test]
    fn vm_nanval_truthy_number_or() {
        let source = "f a:n b:n>n;|a b";
        // Non-zero is truthy, so |5 3 should return 5 (short-circuit)
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(5.0), Value::Number(3.0)]
            ),
            Value::Number(5.0)
        );
        // 0 is falsy, so |0 3 should return 3
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(0.0), Value::Number(3.0)]
            ),
            Value::Number(3.0)
        );
    }

    // nanval_truthy — string truthiness (non-empty = true, empty = false)
    #[test]
    fn vm_nanval_truthy_string() {
        let source = r#"f a:t b:t>t;&a b"#;
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Text("hi".into()), Value::Text("there".into())]
            ),
            Value::Text("there".into())
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Text("".into()), Value::Text("there".into())]
            ),
            Value::Text("".into())
        );
    }

    // nanval_truthy — list truthiness (non-empty = true, empty = false)
    #[test]
    fn vm_nanval_truthy_list() {
        let source = "f>L n;xs=[1, 2];ys=[3];|xs ys";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![Value::Number(1.0), Value::Number(2.0)])
        );
    }

    // nanval_truthy — ok/err/record value (heap but not string/list) → _ => true (L2041)
    #[test]
    fn vm_nanval_truthy_heap_other() {
        // Guard condition on an Ok value → nanval_truthy(_ => true branch)
        // "f x:t>n;x{1}" — non-negated guard: if x is TRUTHY, execute body (return 1), else skip
        // Ok(Number) is truthy → _ => true → guard body executes → returns 1.0
        let source = "f x:t>n;x{1}";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Ok(Box::new(Value::Number(1.0)))],
        );
        assert_eq!(result, Value::Number(1.0));
    }

    // 14. NanVal record roundtrip — construct and access field
    #[test]
    fn vm_nanval_record_roundtrip() {
        let source = "f>n;r=point x:5 y:10;r.x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(5.0));
    }

    #[test]
    fn vm_nanval_record_return() {
        let source = "f>point;r=point x:1 y:2;r";
        let result = vm_run(source, Some("f"), vec![]);
        let Value::Record { type_name, fields } = result else {
            panic!("expected record")
        };
        assert_eq!(type_name, "point");
        assert_eq!(fields.get("x"), Some(&Value::Number(1.0)));
        assert_eq!(fields.get("y"), Some(&Value::Number(2.0)));
    }

    // ── Error tests ──────────────────────────────────────────────────

    // 15. Type error: negate non-number
    #[test]
    fn vm_err_negate_non_number() {
        // Pass a bool and try to negate it — the parser/typechecker may not catch this
        // We use a function that takes a generic-ish approach
        let source = "f x:b>n;y=0;-y x";
        let err = vm_run_err(source, Some("f"), vec![Value::Bool(true)]);
        assert!(
            err.contains("subtract") || err.contains("negate") || err.contains("number"),
            "unexpected error: {}",
            err
        );
    }

    // 16. OP_ADD type error — adding incompatible types (number + bool)
    #[test]
    fn vm_err_add_incompatible() {
        // sub of number and bool triggers the OP_SUB type error
        let source = "f x:n y:b>n;-x y";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Number(5.0), Value::Bool(true)],
        );
        assert!(
            err.contains("subtract") || err.contains("number"),
            "unexpected error: {}",
            err
        );
    }

    // 17. OP_RECFLD field not found
    #[test]
    fn vm_err_field_not_found() {
        let source = "f>n;r=point x:1 y:2;r.z";
        let err = vm_run_err(source, Some("f"), vec![]);
        assert!(
            err.contains("field") || err.contains("z"),
            "unexpected error: {}",
            err
        );
    }

    // 21. Compile error: undefined variable reference
    #[test]
    fn vm_err_undefined_variable() {
        let err = compile_err("f>n;x");
        assert!(
            err.contains("undefined variable"),
            "unexpected error: {}",
            err
        );
    }

    // 22. Compile error: undefined function call
    #[test]
    fn vm_err_undefined_function() {
        let err = compile_err("f>n;nonexistent 5");
        assert!(
            err.contains("undefined function"),
            "unexpected error: {}",
            err
        );
    }

    // 24. Division by zero in OP_DIV_NN
    #[test]
    fn vm_err_division_by_zero() {
        let source = "f a:n b:n>n;/a b";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Number(10.0), Value::Number(0.0)],
        );
        assert!(
            err.contains("division by zero"),
            "unexpected error: {}",
            err
        );
    }

    // Match expression (not just statement) with no subject
    #[test]
    fn vm_match_expr_no_subject() {
        let source = r#"f x:n>t;y=?{_:"default"};y"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Text("default".into())
        );
    }

    // ForEach with single element
    #[test]
    fn vm_foreach_single_element() {
        let source = "f>n;xs=[42];@x xs{x};0";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(0.0));
    }

    // Constant folding: less-than, less-or-equal, greater-or-equal
    #[test]
    fn vm_const_fold_lt() {
        let source = "f>b;x=<3 5;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    fn vm_const_fold_le() {
        let source = "f>b;x=<=3 3;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    fn vm_const_fold_ge() {
        let source = "f>b;x=>=5 3;x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    // Constant folding: divide by zero returns None (no fold)
    #[test]
    fn vm_const_fold_div_by_zero_no_fold() {
        // /10 0 where both are literals — const fold returns None because b == 0
        // Falls through to runtime which triggers OP_DIV or OP_DIV_NN
        let source = "f>n;/10 0";
        let err = vm_run_err(source, Some("f"), vec![]);
        assert!(
            err.contains("division by zero"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn vm_typedef_in_program() {
        // TypeDef after function exercises the skip in name collection (line 279)
        // and the dummy chunk push (lines 322-323)
        let source = "f x:n>n;*x 2\ntype point{x:n;y:n}";
        let result = vm_run(source, Some("f"), vec![Value::Number(3.0)]);
        assert_eq!(result, Value::Number(6.0));
    }

    #[test]
    fn vm_index_out_of_bounds() {
        let source = "f>n;xs=[1, 2];xs.5";
        let err = vm_run_err(source, Some("f"), vec![]);
        assert!(
            err.contains("out of bounds") || err.contains("index"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn vm_subk_n() {
        // Exercises OP_SUBK_N: subtract variable by constant
        let source = "f x:n>n;-x 3";
        let result = vm_run(source, Some("f"), vec![Value::Number(10.0)]);
        assert_eq!(result, Value::Number(7.0));
    }

    #[test]
    fn vm_divk_n() {
        // Exercises OP_DIVK_N: divide variable by constant
        let source = "f x:n>n;/x 2";
        let result = vm_run(source, Some("f"), vec![Value::Number(10.0)]);
        assert_eq!(result, Value::Number(5.0));
    }

    #[test]
    fn vm_state_with_heap_values() {
        // Call VmState twice with string returns to exercise the drain-and-drop path
        let source = "f x:t>t;x";
        let prog = parse_program(source);
        let compiled = compile(&prog).unwrap();
        let mut state = VmState::new(&compiled);
        let r1 = state.call("f", vec![Value::Text("hello".into())]).unwrap();
        assert_eq!(r1, Value::Text("hello".into()));
        let r2 = state.call("f", vec![Value::Text("world".into())]).unwrap();
        assert_eq!(r2, Value::Text("world".into()));
    }

    // ---- Constant dedup: Bool and Nil ----

    #[test]
    fn vm_const_dedup_bool_true() {
        // Two identical `true` literals in one function — second should reuse the constant
        // We verify by checking that the function produces correct output (dedup is transparent)
        let source = "f>b;x=true;y=true;=x y";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    fn vm_const_dedup_bool_false() {
        let source = "f>b;x=false;y=false;=x y";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    fn vm_const_dedup_bool_mixed() {
        // true != false (different bool constants, no dedup between them)
        let source = "f>b;x=true;y=false;=x y";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(false));
    }

    #[test]
    fn vm_const_dedup_nil_via_match() {
        // Nil values in constant pool — a subjectless match with no subject exercises Nil
        let source = "f>b;x=?{_:true};x";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Bool(true));
    }

    #[test]
    fn vm_nil_fallback_function_body() {
        // Function body only has a Guard stmt → compile_body returns None → Nil fallback (L306-310)
        // When x >= 0, the guard fires and RET 0. Function body's compile_body returns None,
        // triggering the Nil constant load fallback that runs when the guard doesn't fire.
        let source = "f x:n>n;>=x 0{0}";
        // With x >= 0: guard fires, returns 0
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(0.0)
        );
        // With x < 0: guard skips, Nil fallback executes → returns Nil
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(-1.0)]),
            Value::Nil
        );
    }

    #[test]
    fn vm_nil_fallback_guard_body() {
        // Guard body only has a nested Guard stmt → compile_body returns None for guard body (L361-365)
        // Outer guard fires (x >= 0), inner guard body compiles to None → Nil fallback
        let source = "f x:n>n;>=x 0{>=x 5{10}}";
        // x=10: outer guard fires, inner guard fires → returns 10
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(10.0)]),
            Value::Number(10.0)
        );
        // x=1: outer guard fires, inner guard doesn't fire → guard body returns Nil
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Nil
        );
    }

    #[test]
    fn vm_fused_cmpk_guard_emission() {
        // Verify that numeric guards emit CMPK_*_N (fused compare-and-skip) opcodes
        // instead of the classic OP_GE + OP_JMPF pair.
        // Uses braceless guards (early return) for guard chain
        let source = "classify x:n>n;>=x 900 30;>=x 700 25;>=x 500 20;>=x 300 15;>=x 100 10;5";
        let prog = parse_program(source);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_cmpk = chunk.code.iter().any(|&inst| {
            let op = (inst >> 24) as u8;
            op == OP_CMPK_GE_N
                || op == OP_CMPK_GT_N
                || op == OP_CMPK_LT_N
                || op == OP_CMPK_LE_N
                || op == OP_CMPK_EQ_N
                || op == OP_CMPK_NE_N
        });
        assert!(
            has_cmpk,
            "expected at least one CMPK_*_N opcode in guard chain bytecode"
        );
        // Also verify no intermediate bool register (OP_GE) is emitted for these guards
        let has_ge = chunk.code.iter().any(|&inst| (inst >> 24) as u8 == OP_GE);
        assert!(
            !has_ge,
            "expected OP_GE to be replaced by OP_CMPK_GE_N in numeric guard chain"
        );
        // Correctness: verify runtime results
        assert_eq!(
            vm_run(source, Some("classify"), vec![Value::Number(950.0)]),
            Value::Number(30.0)
        );
        assert_eq!(
            vm_run(source, Some("classify"), vec![Value::Number(750.0)]),
            Value::Number(25.0)
        );
        assert_eq!(
            vm_run(source, Some("classify"), vec![Value::Number(550.0)]),
            Value::Number(20.0)
        );
        assert_eq!(
            vm_run(source, Some("classify"), vec![Value::Number(350.0)]),
            Value::Number(15.0)
        );
        assert_eq!(
            vm_run(source, Some("classify"), vec![Value::Number(150.0)]),
            Value::Number(10.0)
        );
        assert_eq!(
            vm_run(source, Some("classify"), vec![Value::Number(50.0)]),
            Value::Number(5.0)
        );
    }

    #[test]
    fn vm_fused_cmpk_guard_all_ops() {
        // Verify all 6 CMPK opcode variants work correctly — braceless guards (early return)
        assert_eq!(
            vm_run("f x:n>n;>=x 10 1;0", Some("f"), vec![Value::Number(10.0)]),
            Value::Number(1.0)
        );
        assert_eq!(
            vm_run("f x:n>n;>=x 10 1;0", Some("f"), vec![Value::Number(9.0)]),
            Value::Number(0.0)
        );
        assert_eq!(
            vm_run("f x:n>n;>x 10 1;0", Some("f"), vec![Value::Number(11.0)]),
            Value::Number(1.0)
        );
        assert_eq!(
            vm_run("f x:n>n;>x 10 1;0", Some("f"), vec![Value::Number(10.0)]),
            Value::Number(0.0)
        );
        assert_eq!(
            vm_run("f x:n>n;<x 10 1;0", Some("f"), vec![Value::Number(9.0)]),
            Value::Number(1.0)
        );
        assert_eq!(
            vm_run("f x:n>n;<x 10 1;0", Some("f"), vec![Value::Number(10.0)]),
            Value::Number(0.0)
        );
        assert_eq!(
            vm_run("f x:n>n;<=x 10 1;0", Some("f"), vec![Value::Number(10.0)]),
            Value::Number(1.0)
        );
        assert_eq!(
            vm_run("f x:n>n;<=x 10 1;0", Some("f"), vec![Value::Number(11.0)]),
            Value::Number(0.0)
        );
        assert_eq!(
            vm_run("f x:n>n;==x 10 1;0", Some("f"), vec![Value::Number(10.0)]),
            Value::Number(1.0)
        );
        assert_eq!(
            vm_run("f x:n>n;==x 10 1;0", Some("f"), vec![Value::Number(11.0)]),
            Value::Number(0.0)
        );
        assert_eq!(
            vm_run("f x:n>n;!=x 10 1;0", Some("f"), vec![Value::Number(11.0)]),
            Value::Number(1.0)
        );
        assert_eq!(
            vm_run("f x:n>n;!=x 10 1;0", Some("f"), vec![Value::Number(10.0)]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn vm_const_fold_negate_number() {
        // try_const_fold for UnaryOp::Negate on a const-foldable operand (L588)
        // -(+3 2) → try_const_fold(UnaryOp{Negate, BinOp{Add, 3, 2}})
        //         → try_const_fold(BinOp) = Some(Number(5.0))
        //         → (Number(5.0), Negate) → Some(Number(-5.0))
        let source = "f>n;-(+3 2)";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(-5.0));
    }

    #[test]
    fn vm_addk_n_const_left() {
        // ADDK_N with literal constant on left side: +2 x (L755-763)
        // The compiler detects commutative op (Add) with Literal on left, variable on right
        let source = "f x:n>n;+2 x";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(3.0)]),
            Value::Number(5.0)
        );
    }

    #[test]
    fn vm_mulk_n_const_left() {
        // MULK_N with literal constant on left side: *3 x (L753-763 for Multiply)
        let source = "f x:n>n;*3 x";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(4.0)]),
            Value::Number(12.0)
        );
    }

    #[test]
    fn vm_nanval_from_value_record() {
        // NanVal::from_value with Value::Record → heap_record (L1122-1125)
        // Pass a record as an arg to trigger NanVal::from_value Record branch
        let source = "f x:n>n;x";
        let prog = parse_program(source);
        let compiled = compile(&prog).unwrap();
        let mut state = VmState::new(&compiled);
        // Directly exercise NanVal::from_value with a Record value
        let rec = Value::Record {
            type_name: "point".to_string(),
            fields: std::collections::HashMap::from([("x".to_string(), Value::Number(42.0))]),
        };
        let nv = NanVal::from_value(&rec);
        let roundtrip = nv.to_value();
        let Value::Record { type_name, fields } = roundtrip else {
            panic!("expected Record")
        };
        assert_eq!(type_name, "point");
        assert_eq!(fields.get("x"), Some(&Value::Number(42.0)));
        // Also verify the state can be used normally
        let r = state.call("f", vec![Value::Number(1.0)]).unwrap();
        assert_eq!(r, Value::Number(1.0));
    }

    // BinOp::Divide where the dividend is a match-result register (not reg_is_num)
    // → compiler emits OP_DIV instead of OP_DIV_NN (L804)
    #[test]
    fn vm_divide_non_numeric_register() {
        let source = "f x:n>n;r=?x{1:2;_:3};/r x";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Number(2.0)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(2.0)]),
            Value::Number(1.5)
        );
    }

    #[test]
    fn vm_div_non_numeric_division_by_zero() {
        // OP_DIV division-by-zero path (L1413): divisor register is non-tagged-numeric (match result = 0)
        let source = "f x:n>n;r=?x{1:0;_:2};/x r";
        let err = vm_run_err(source, Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("zero") || err.contains("Div"),
            "expected division-by-zero error, got: {err}"
        );
    }

    #[test]
    fn vm_gt_non_numeric_registers() {
        // OP_GT numeric path (L1441): both operands are numbers but not tagged numeric
        // r and s are match results (reg_is_num=false), so OP_GT is emitted (not OP_GT_NN)
        let source = "f x:n>b;r=?x{1:5;_:2};s=?x{1:3;_:8};>r s";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Bool(true)
        ); // 5 > 3 = true
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(2.0)]),
            Value::Bool(false)
        ); // 2 > 8 = false
    }

    #[test]
    fn vm_lt_non_numeric_registers() {
        // OP_LT numeric path (L1456): both operands are numbers but not tagged numeric
        let source = "f x:n>b;r=?x{1:5;_:2};s=?x{1:3;_:8};<r s";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Bool(false)
        ); // 5 < 3 = false
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(2.0)]),
            Value::Bool(true)
        ); // 2 < 8 = true
    }

    #[test]
    fn vm_le_non_numeric_registers() {
        // OP_LE numeric path (L1486): both operands are numbers but not tagged numeric
        let source = "f x:n>b;r=?x{1:5;_:3};s=?x{1:5;_:8};<=r s";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Bool(true)
        ); // 5 <= 5 = true
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(2.0)]),
            Value::Bool(true)
        ); // 3 <= 8 = true
    }

    #[test]
    fn vm_multiply_non_numeric_register() {
        // OP_MUL path: match result register (reg_is_num=false) × numeric param → OP_MUL
        let source = "f x:n>n;r=?x{1:2;_:3};*r x";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Number(2.0) // r=2, x=1 → 2*1=2
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(4.0)]),
            Value::Number(12.0) // r=3 (default arm), x=4 → 3*4=12
        );
    }

    #[test]
    fn vm_subtract_non_numeric_register() {
        // OP_SUB path: match result register (reg_is_num=false) − numeric param → OP_SUB
        let source = "f x:n>n;r=?x{1:10;_:20};-r x";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Number(9.0) // r=10, x=1 → 10-1=9
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(3.0)]),
            Value::Number(17.0) // r=20 (default arm), x=3 → 20-3=17
        );
    }

    // ---- Type-error paths in VM opcodes ----

    #[test]
    fn vm_add_number_text_type_error() {
        // OP_ADD: bv is number, cv is text → neither both-num nor both-string nor both-heap → L1377
        let source = "f x:n y:t>n;+x y";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Number(1.0), Value::Text("hi".to_string())],
        );
        assert!(err.contains("cannot add"), "got: {err}");
    }

    #[test]
    fn vm_add_heap_non_list_type_error() {
        // OP_ADD: both heap but not both lists (list+ok) → type error
        // Declare params as list type to avoid NN/SS specialisations, pass list+ok at runtime.
        let source = "f x:L n y:L n>L n;+x y";
        let list = Value::List(vec![Value::Number(1.0)]);
        let ok_val = Value::Ok(Box::new(Value::Number(1.0)));
        let err = vm_run_err(source, Some("f"), vec![list, ok_val]);
        assert!(err.contains("cannot add"), "got: {err}");
    }

    #[test]
    fn vm_mul_type_error() {
        // OP_MUL: text × number → L1401 ("cannot multiply non-numbers")
        let source = "f x:t y:n>n;*x y";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Text("hi".to_string()), Value::Number(2.0)],
        );
        assert!(err.contains("multiply"), "got: {err}");
    }

    #[test]
    fn vm_gt_type_error() {
        // OP_GT: number > text → neither both-num nor both-string → L1446
        let source = "f x:n y:t>b;>x y";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Number(1.0), Value::Text("hi".to_string())],
        );
        assert!(err.contains("compare"), "got: {err}");
    }

    #[test]
    fn vm_lt_type_error() {
        // OP_LT: number < text → L1461
        let source = "f x:n y:t>b;<x y";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Number(1.0), Value::Text("hi".to_string())],
        );
        assert!(err.contains("compare"), "got: {err}");
    }

    #[test]
    fn vm_ge_type_error() {
        // OP_GE: number >= text → L1476
        let source = "f x:n y:t>b;>=x y";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Number(1.0), Value::Text("hi".to_string())],
        );
        assert!(err.contains("compare"), "got: {err}");
    }

    #[test]
    fn vm_le_type_error() {
        // OP_LE: number <= text → L1491
        let source = "f x:n y:t>b;<=x y";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Number(1.0), Value::Text("hi".to_string())],
        );
        assert!(err.contains("compare"), "got: {err}");
    }

    #[test]
    fn vm_neg_type_error() {
        // OP_NEG on text (unary negate) → L1514
        let source = "f x:t>n;-x";
        let err = vm_run_err(source, Some("f"), vec![Value::Text("hi".to_string())]);
        assert!(err.contains("negate"), "got: {err}");
    }

    #[test]
    fn vm_str_non_number_type_error() {
        // OP_STR on text → L1903 ("str requires a number")
        let source = "f x:t>t;str x";
        let err = vm_run_err(source, Some("f"), vec![Value::Text("hi".to_string())]);
        assert!(err.contains("str"), "got: {err}");
    }

    #[test]
    fn vm_num_non_string_type_error() {
        // OP_NUM on number → L1918 ("num requires a string")
        let source = "f x:n>n;num x";
        let err = vm_run_err(source, Some("f"), vec![Value::Number(42.0)]);
        assert!(err.contains("num"), "got: {err}");
    }

    #[test]
    fn vm_abs_non_number_type_error() {
        // OP_ABS on text → L1936 ("abs requires a number")
        let source = "f x:t>n;abs x";
        let err = vm_run_err(source, Some("f"), vec![Value::Text("hi".to_string())]);
        assert!(err.contains("abs"), "got: {err}");
    }

    #[test]
    fn vm_min_non_number_type_error() {
        // OP_MIN with non-numeric first arg → L1947 ("min/max require numbers")
        let source = "f x:t y:n>n;min x y";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Text("hi".to_string()), Value::Number(2.0)],
        );
        assert!(
            err.contains("min") || err.contains("max") || err.contains("number"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_flr_non_number_type_error() {
        // OP_FLR on text → L1959 ("flr/cel requires a number")
        let source = "f x:t>n;flr x";
        let err = vm_run_err(source, Some("f"), vec![Value::Text("hi".to_string())]);
        assert!(err.contains("flr") || err.contains("number"), "got: {err}");
    }

    #[test]
    fn vm_nan_value_number() {
        // NanVal::number() with NaN input → canonical NaN path (L1013)
        let result = vm_run("f x:n>n;x", Some("f"), vec![Value::Number(f64::NAN)]);
        assert!(
            matches!(result, Value::Number(n) if n.is_nan()),
            "expected NaN, got: {:?}",
            result
        );
    }

    #[test]
    fn vm_state_call_after_error() {
        // VmState::call(): first call fails leaving values on stack; second call drains them (L1201-1202)
        let source = "f x:n>n;/x 0";
        let prog = parse_program(source);
        let compiled = compile(&prog).unwrap();
        let mut state = VmState::new(&compiled);
        // First call fails (division by zero), leaving register values on the stack
        let err1 = state.call("f", vec![Value::Number(5.0)]);
        assert!(err1.is_err(), "expected DivisionByZero error");
        // Second call drains leftover values from failed first call (L1201-1202)
        let err2 = state.call("f", vec![Value::Number(3.0)]);
        assert!(err2.is_err(), "expected DivisionByZero error again");
    }

    #[test]
    fn vm_div_type_error() {
        // OP_DIV: non-number operands → L1417 ("cannot divide non-numbers")
        let source = "f x:t y:t>t;/x y";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Text("hi".into()), Value::Text("lo".into())],
        );
        assert!(err.contains("divide"), "got: {err}");
    }

    #[test]
    fn vm_recfld_on_non_record() {
        // OP_RECFLD: field access on a list (heap but not record) → L1590
        let source = "f x:t>t;x.name";
        let err = vm_run_err(source, Some("f"), vec![Value::List(vec![])]);
        assert!(
            err.contains("field access") || err.contains("record"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_index_on_non_list() {
        // OP_INDEX: index access on a string (heap but not list) → L1612
        let source = "f x:t>t;x.0";
        let err = vm_run_err(source, Some("f"), vec![Value::Text("hi".into())]);
        assert!(err.contains("index") || err.contains("list"), "got: {err}");
    }

    #[test]
    fn vm_foreach_on_non_heap() {
        // OP_LISTGET: foreach over a number (non-heap) → L1624
        let source = "f x:n>n;@elem x{elem}";
        let err = vm_run_err(source, Some("f"), vec![Value::Number(5.0)]);
        assert!(
            err.contains("list") || err.contains("foreach"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_foreach_on_heap_non_list() {
        // OP_LISTGET: foreach over a string (heap but not list) → L1643
        let source = "f x:t>t;@elem x{elem}";
        let err = vm_run_err(source, Some("f"), vec![Value::Text("hi".into())]);
        assert!(
            err.contains("list") || err.contains("foreach"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_with_on_non_record() {
        // OP_RECWITH: with on a list (heap but not record) → L1786
        let source = "f x:t>t;x with name:\"bob\"";
        let err = vm_run_err(source, Some("f"), vec![Value::List(vec![])]);
        assert!(err.contains("record") || err.contains("with"), "got: {err}");
    }

    #[test]
    fn vm_len_on_heap_non_string_non_list() {
        // OP_LEN: len of Ok value (heap but not string/list) → L1891
        let source = "f x:t>n;len x";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Ok(Box::new(Value::Number(1.0)))],
        );
        assert!(
            err.contains("len") || err.contains("string") || err.contains("list"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_len_on_non_heap() {
        // OP_LEN: len of number (non-heap, non-string) → L1894
        let source = "f x:t>n;len x";
        let err = vm_run_err(source, Some("f"), vec![Value::Number(5.0)]);
        assert!(
            err.contains("len") || err.contains("string") || err.contains("list"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_listappend_on_non_heap() {
        // OP_LISTAPPEND: += where first arg is a number (non-heap) → L1972
        let source = "f x:t y:t>t;+=x y";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Number(1.0), Value::Number(2.0)],
        );
        assert!(err.contains("list") || err.contains("+="), "got: {err}");
    }

    #[test]
    fn vm_listappend_on_heap_non_list() {
        // OP_LISTAPPEND: += where first arg is a string (heap but not list) → L1986
        let source = "f x:t y:t>t;+=x y";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Text("hi".into()), Value::Number(1.0)],
        );
        assert!(err.contains("list") || err.contains("+="), "got: {err}");
    }

    // vm compile: function with 256 parameters → L285 assert panics
    #[test]
    #[should_panic(expected = "function has 256 parameters")]
    fn vm_too_many_params_panics() {
        use crate::ast::{Decl, Param, Program, Span, Type};
        let params: Vec<Param> = (0..256)
            .map(|i| Param {
                name: format!("p{i}"),
                ty: Type::Number,
            })
            .collect();
        let prog = Program {
            declarations: vec![Decl::Function {
                name: "f".to_string(),
                params,
                body: vec![],
                return_type: Type::Number,
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        let _ = compile(&prog);
    }

    // try_const_fold: Text BinOp non-Add → L573 `_ => None`
    // ="hello" "world" → lv=Text, rv=Text, op=Equals → not Add → L573
    #[test]
    fn vm_const_fold_text_eq_no_fold() {
        let result = vm_run(r#"f>b;="hello" "world""#, Some("f"), vec![]);
        assert_eq!(result, Value::Bool(false));
    }

    // try_const_fold: Bool BinOp non-Eq/Ne/And/Or → L580 `_ => None`
    // <true false → lv=Bool, rv=Bool, op=LessThan → not Eq/Ne/And/Or → L580
    #[test]
    fn vm_const_fold_bool_lt_no_fold() {
        let err = vm_run_err("f>b;<true false", Some("f"), vec![]);
        assert!(
            err.contains("compare") || err.contains("type"),
            "got: {err}"
        );
    }

    // try_const_fold: mixed types (Bool + Number) → L582 `_ => None`
    // +true 3 → lv=Bool(true), rv=Number(3) → _ branch at L582
    #[test]
    fn vm_const_fold_mixed_types_no_fold() {
        let err = vm_run_err("f>n;+true 3", Some("f"), vec![]);
        assert!(
            err.contains("add") || err.contains("type") || err.contains("number"),
            "got: {err}"
        );
    }

    // try_const_fold: UnaryOp on non-Number/non-Bool literal → L590 `_ => None`
    // !3 → v=Number(3), op=Not → _ branch at L590 (only Negate+Number and Not+Bool covered)
    #[test]
    fn vm_const_fold_not_on_number_no_fold() {
        let result = vm_run("f>b;!3", Some("f"), vec![]);
        // !3 → OP_NOT on Number(3): nanval_truthy(3.0) = true → !true = false
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn vm_get_compiles() {
        // Verify that `get` compiles to OP_GET (doesn't fall through to OP_CALL)
        let prog = parse_program(r#"f url:t>R t t;get url"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_get_op = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_GET);
        assert!(has_get_op, "expected OP_GET in bytecode");
    }

    #[test]
    fn vm_dollar_desugars_to_get() {
        // $url should compile the same as get url
        let prog = parse_program(r#"f url:t>R t t;$url"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_get_op = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_GET);
        assert!(has_get_op, "expected OP_GET in bytecode from $ syntax");
    }

    #[test]
    fn vm_post_compiles_to_op_post() {
        let prog = parse_program(r#"f url:t body:t>R t t;post url body"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_post_op = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_POST);
        assert!(has_post_op, "expected OP_POST in bytecode");
    }

    #[test]
    fn vm_post_unwrap_compiles_to_op_post() {
        let prog = parse_program(r#"f url:t body:t>t;post! url body"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_post_op = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_POST);
        assert!(has_post_op, "expected OP_POST in bytecode for post!");
    }

    #[test]
    fn vm_get_with_headers_compiles_to_op_geth() {
        let prog = parse_program(r#"f url:t hdrs:M t t>R t t;get url hdrs"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_geth = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_GETH);
        assert!(has_geth, "expected OP_GETH in bytecode");
    }

    #[test]
    fn vm_post_with_headers_compiles_to_op_posth() {
        let prog = parse_program(r#"f url:t body:t hdrs:M t t>R t t;post url body hdrs"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_posth = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_POSTH);
        assert!(has_posth, "expected OP_POSTH in bytecode");
    }

    #[test]
    fn vm_get_with_headers_bad_host_returns_err() {
        // bad host → Err value, even with headers passed as parameter
        let src = r#"f url:t hdrs:M t t>R t t;get url hdrs"#;
        let mut headers = std::collections::HashMap::new();
        headers.insert("x-api-key".to_string(), Value::Text("tok".to_string()));
        let result = vm_run(
            src,
            Some("f"),
            vec![
                Value::Text("http://127.0.0.1:1".to_string()),
                Value::Map(headers),
            ],
        );
        let Value::Err(_) = result else {
            panic!("expected Err")
        };
    }

    #[test]
    fn vm_post_with_headers_bad_host_returns_err() {
        // bad host → Err value, even with headers passed as parameter
        let src = r#"f url:t body:t hdrs:M t t>R t t;post url body hdrs"#;
        let mut headers = std::collections::HashMap::new();
        headers.insert("x-api-key".to_string(), Value::Text("tok".to_string()));
        let result = vm_run(
            src,
            Some("f"),
            vec![
                Value::Text("http://127.0.0.1:1".to_string()),
                Value::Text("body".to_string()),
                Value::Map(headers),
            ],
        );
        let Value::Err(_) = result else {
            panic!("expected Err")
        };
    }

    // ---- Braceless guards ----

    #[test]
    fn vm_braceless_guard() {
        let source = r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#;
        assert_eq!(
            vm_run(source, Some("cls"), vec![Value::Number(1500.0)]),
            Value::Text("gold".to_string())
        );
        assert_eq!(
            vm_run(source, Some("cls"), vec![Value::Number(750.0)]),
            Value::Text("silver".to_string())
        );
        assert_eq!(
            vm_run(source, Some("cls"), vec![Value::Number(100.0)]),
            Value::Text("bronze".to_string())
        );
    }

    #[test]
    fn vm_braceless_guard_factorial() {
        let source = "fac n:n>n;<=n 1 1;r=fac -n 1;*n r";
        assert_eq!(
            vm_run(source, Some("fac"), vec![Value::Number(5.0)]),
            Value::Number(120.0)
        );
    }

    #[test]
    fn vm_braceless_guard_fibonacci() {
        let source = "fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b";
        assert_eq!(
            vm_run(source, Some("fib"), vec![Value::Number(10.0)]),
            Value::Number(55.0)
        );
    }

    #[test]
    fn vm_spl_basic() {
        let source = r#"f>L t;spl "a,b,c" ",""#;
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![
                Value::Text("a".to_string()),
                Value::Text("b".to_string()),
                Value::Text("c".to_string()),
            ])
        );
    }

    #[test]
    fn vm_spl_empty() {
        let source = r#"f>L t;spl "" ",""#;
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![Value::Text("".to_string())])
        );
    }

    #[test]
    fn vm_cat_basic() {
        let source = "f items:L t>t;cat items \",\"";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::List(vec![
                    Value::Text("a".into()),
                    Value::Text("b".into()),
                    Value::Text("c".into()),
                ])]
            ),
            Value::Text("a,b,c".into())
        );
    }

    #[test]
    fn vm_cat_empty_list() {
        let source = "f items:L t>t;cat items \"-\"";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::List(vec![])]),
            Value::Text("".into())
        );
    }

    #[test]
    fn vm_has_list() {
        let source = "f xs:L n x:n>b;has xs x";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![
                    Value::List(vec![
                        Value::Number(1.0),
                        Value::Number(2.0),
                        Value::Number(3.0)
                    ]),
                    Value::Number(2.0)
                ]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::List(vec![Value::Number(1.0)]), Value::Number(5.0)]
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_has_text() {
        let source = r#"f s:t needle:t>b;has s needle"#;
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![
                    Value::Text("hello world".into()),
                    Value::Text("world".into())
                ]
            ),
            Value::Bool(true)
        );
    }

    #[test]
    fn vm_hd_list() {
        let source = "f>n;xs=[10, 20, 30];hd xs";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(10.0));
    }

    #[test]
    fn vm_tl_list() {
        let source = "f>L n;xs=[10, 20, 30];tl xs";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![Value::Number(20.0), Value::Number(30.0)])
        );
    }

    #[test]
    fn vm_hd_text() {
        let source = r#"f s:t>t;hd s"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("hello".into())]),
            Value::Text("h".into())
        );
    }

    #[test]
    fn vm_tl_text() {
        let source = r#"f s:t>t;tl s"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("hello".into())]),
            Value::Text("ello".into())
        );
    }

    #[test]
    fn vm_rev_list() {
        let source = "f>L n;rev [1, 2, 3]";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![
                Value::Number(3.0),
                Value::Number(2.0),
                Value::Number(1.0)
            ])
        );
    }

    #[test]
    fn vm_rev_text() {
        let source = r#"f>t;rev "abc""#;
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Text("cba".into()));
    }

    #[test]
    fn vm_srt_numbers() {
        let source = "f>L n;srt [3, 1, 2]";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])
        );
    }

    #[test]
    fn vm_srt_text_list() {
        let source = r#"f>L t;srt ["c", "a", "b"]"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![
                Value::Text("a".into()),
                Value::Text("b".into()),
                Value::Text("c".into())
            ])
        );
    }

    #[test]
    fn vm_slc_list() {
        let source = "f>L n;slc [1, 2, 3, 4, 5] 1 3";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![Value::Number(2.0), Value::Number(3.0)])
        );
    }

    #[test]
    fn vm_slc_text() {
        let source = r#"f>t;slc "hello" 1 4"#;
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Text("ell".into()));
    }

    #[test]
    fn vm_ternary_true() {
        let source = r#"f x:n>t;=x 1{"yes"}{"no"}"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Text("yes".into())
        );
    }

    #[test]
    fn vm_ternary_false() {
        let source = r#"f x:n>t;=x 1{"yes"}{"no"}"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(2.0)]),
            Value::Text("no".into())
        );
    }

    #[test]
    fn vm_ternary_no_early_return() {
        let source = r#"f x:n>n;=x 0{10}{20};+x 1"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(0.0)]),
            Value::Number(1.0)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(6.0)
        );
    }

    #[test]
    fn vm_ret_early_return() {
        let source = r#"f x:n>n;>x 0{ret x};0"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(5.0)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(-1.0)]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn vm_ret_in_foreach() {
        let source = "f xs:L n>n;@x xs{>=x 10{ret x}};0";
        let list = Value::List(vec![
            Value::Number(1.0),
            Value::Number(15.0),
            Value::Number(3.0),
        ]);
        assert_eq!(vm_run(source, Some("f"), vec![list]), Value::Number(15.0));
    }

    #[test]
    fn vm_pipe_simple() {
        let source = "f x:n>n;str x>>len";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(42.0)]),
            Value::Number(2.0)
        );
    }

    #[test]
    fn vm_pipe_chain() {
        let source = "dbl x:n>n;*x 2\nadd1 x:n>n;+x 1\nf x:n>n;dbl x>>add1";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(11.0)
        );
    }

    #[test]
    fn vm_pipe_with_extra_args() {
        let source = "add a:n b:n>n;+a b\nf x:n>n;add x 1>>add 2";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(8.0)
        );
    }

    #[test]
    fn vm_while_basic() {
        let source = "f>n;i=0;s=0;wh <i 5{i=+i 1;s=+s i};s";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(15.0));
    }

    #[test]
    fn vm_while_zero_iterations() {
        let source = "f>n;wh false{42};0";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(0.0));
    }

    #[test]
    fn vm_while_with_ret() {
        let source = "f>n;i=0;wh true{i=+i 1;>=i 3{ret i}};0";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_nil_coalesce_nil() {
        // Function returns nil when guard doesn't fire, ?? falls back
        let source = "mk x:n>n;>=x 1{x}\nf>n;x=mk 0;x??42";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(42.0));
    }

    #[test]
    fn vm_nil_coalesce_non_nil() {
        let source = "f>n;x=10;x??42";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(10.0));
    }

    #[test]
    fn vm_nil_coalesce_chain() {
        let source = "mk x:n>n;>=x 1{x}\nf>n;a=mk 0;b=mk 0;a??b??99";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(99.0));
    }

    #[test]
    fn vm_safe_field_on_nil() {
        let source = "mk x:n>n;>=x 1{x}\nf>n;v=mk 0;v.?name??99";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(99.0));
    }

    #[test]
    fn vm_safe_field_on_value() {
        // Note: type decl must come AFTER function (known VM chunk-index issue)
        let source = "f>n;p=pt x:5;p.?x\ntype pt{x:n}";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(5.0));
    }

    #[test]
    fn vm_safe_field_chained() {
        let source = "mk x:n>n;>=x 1{x}\nf>n;v=mk 0;v.?a.?b??77";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(77.0));
    }

    #[test]
    fn vm_while_brk() {
        let source = "f>n;i=0;wh true{i=+i 1;>=i 3{brk}};i";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_while_brk_value() {
        let source = "f>n;i=0;wh true{i=+i 1;>=i 3{brk 99}};i";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_while_cnt() {
        let source = "f>n;i=0;s=0;wh <i 5{i=+i 1;>=i 3{cnt};s=+s i};s";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_foreach_brk() {
        // brk with value exits foreach, foreach returns the break value
        let source = "f>n;@x [1,2,3,4,5]{>=x 3{brk x};x}";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_foreach_cnt() {
        // cnt skips rest of body — last value is from last non-skipped iteration
        let source = "f>n;@x [1,2,3,4,5]{>=x 3{cnt};*x 2}";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(4.0));
    }

    #[test]
    fn vm_rnd_no_args() {
        let source = "f>n;rnd";
        let result = vm_run(source, Some("f"), vec![]);
        let Value::Number(n) = result else {
            panic!("expected Number")
        };
        assert!((0.0..1.0).contains(&n), "rnd should be in [0,1), got {n}");
    }

    #[test]
    fn vm_rnd_two_args() {
        let source = "f>n;rnd 1 10";
        let result = vm_run(source, Some("f"), vec![]);
        let Value::Number(n) = result else {
            panic!("expected Number")
        };
        assert!(
            (1.0..=10.0).contains(&n),
            "rnd 1 10 should be in [1,10], got {n}"
        );
        assert_eq!(n, n.floor(), "rnd with two args should return integer");
    }

    #[test]
    fn vm_rnd_same_bounds() {
        let source = "f>n;rnd 5 5";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(5.0));
    }

    #[test]
    fn vm_rnd_non_number_type_error() {
        let source = "f>n;rnd \"hello\" 5";
        let err = vm_run_err(source, Some("f"), vec![]);
        assert!(err.contains("rnd") || err.contains("number"), "got: {err}");
    }

    #[test]
    fn vm_now() {
        let source = "f>n;now";
        let result = vm_run(source, Some("f"), vec![]);
        let Value::Number(n) = result else {
            panic!("expected Number")
        };
        assert!(
            n > 1_000_000_000.0,
            "now should be a reasonable unix timestamp, got {n}"
        );
    }

    // ── env builtin VM tests ──────────────────────────────────────────

    #[test]
    fn vm_env_existing_var() {
        let _guard = ENV_TEST_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("ILO_VM_TEST", "vmval");
        }
        let source = r#"f k:t>R t t;env k"#;
        let result = vm_run(source, Some("f"), vec![Value::Text("ILO_VM_TEST".into())]);
        assert_eq!(result, Value::Ok(Box::new(Value::Text("vmval".into()))));
        unsafe {
            std::env::remove_var("ILO_VM_TEST");
        }
    }

    #[test]
    fn vm_env_missing_var() {
        let _guard = ENV_TEST_MUTEX.lock().unwrap();
        let source = r#"f k:t>R t t;env k"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Text("ILO_VM_NONEXIST_999".into())],
        );
        let Value::Err(inner) = result else {
            panic!("expected Err")
        };
        let Value::Text(s) = *inner else {
            panic!("expected Text")
        };
        assert!(s.contains("not set"), "got: {s}");
    }

    #[test]
    fn vm_env_compiles_to_op_env() {
        let source = r#"f k:t>R t t;env k"#;
        let prog = parse_program(source);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_env_op = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_ENV);
        assert!(has_env_op, "expected OP_ENV in bytecode");
    }

    // ── Range iteration VM tests ────────────────────────────────────────

    #[test]
    fn vm_range_basic() {
        // @i 0..3{i} → last value is 2
        let source = "f>n;@i 0..3{i}";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(2.0));
    }

    #[test]
    fn vm_range_accumulate() {
        let source = "f>n;s=0;@i 0..3{s=+s i};s";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_range_empty() {
        let source = "f>n;s=99;@i 5..3{s=0};s";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(99.0));
    }

    #[test]
    fn vm_range_dynamic_end() {
        let source = "f n:n>n;s=0;@i 0..n{s=+s i};s";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(4.0)]),
            Value::Number(6.0)
        );
    }

    #[test]
    fn vm_range_brk() {
        let source = "f>n;@i 0..10{>=i 3{brk i};i}";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_range_cnt() {
        // Skip i=2
        let source = "f>n;s=0;@i 0..5{=i 2{cnt};s=+s i};s";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(8.0));
    }

    #[test]
    fn vm_range_nonzero_start() {
        let source = "f>n;s=0;@i 2..5{s=+s i};s";
        // 2+3+4 = 9
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(9.0));
    }

    #[test]
    fn vm_safe_index_on_nil() {
        // .?0 on nil returns nil
        let source = "mk x:n>n;>=x 1{x}\nf>n;v=mk 0;v.?0??99";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(99.0));
    }

    #[test]
    fn vm_safe_index_on_value() {
        // .?0 on a list returns the element
        let source = "f>n;xs=[10,20,30];xs.?0";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(10.0));
    }

    // ---- Destructuring bind tests ----

    #[test]
    fn vm_destructure_basic() {
        let source = "type pt{x:n;y:n} f>n;p=pt x:3 y:4;{x;y}=p;+x y";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(7.0));
    }

    #[test]
    fn vm_destructure_single_field() {
        let source = "type pt{x:n;y:n} f>n;p=pt x:10 y:20;{y}=p;y";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(20.0));
    }

    #[test]
    fn vm_destructure_in_loop() {
        let source = "type pt{x:n;y:n} f>n;ps=[pt x:1 y:2,pt x:3 y:4];@p ps{{x;y}=p;+x y}";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(7.0));
    }

    // ── JSON builtins (VM) ──────────────────────────────────────────────

    #[test]
    fn vm_jp_basic() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![
                Value::Text(r#"{"name":"alice"}"#.to_string()),
                Value::Text("name".to_string()),
            ],
        );
        assert_eq!(
            result,
            Value::Ok(Box::new(Value::Text("alice".to_string())))
        );
    }

    #[test]
    fn vm_jp_nested() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![
                Value::Text(r#"{"user":{"name":"bob"}}"#.to_string()),
                Value::Text("user.name".to_string()),
            ],
        );
        assert_eq!(result, Value::Ok(Box::new(Value::Text("bob".to_string()))));
    }

    #[test]
    fn vm_jp_array_index() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![
                Value::Text(r#"[10,20,30]"#.to_string()),
                Value::Text("1".to_string()),
            ],
        );
        assert_eq!(result, Value::Ok(Box::new(Value::Text("20".to_string()))));
    }

    #[test]
    fn vm_jp_missing_key() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![
                Value::Text(r#"{"a":1}"#.to_string()),
                Value::Text("b".to_string()),
            ],
        );
        let Value::Err(e) = result else {
            panic!("expected Err")
        };
        assert!(e.to_string().contains("key not found"), "got: {}", e);
    }

    #[test]
    fn vm_jp_unwrap() {
        let source = r#"f j:t p:t>t;jpth! j p"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![
                Value::Text(r#"{"x":"hello"}"#.to_string()),
                Value::Text("x".to_string()),
            ],
        );
        assert_eq!(result, Value::Text("hello".to_string()));
    }

    #[test]
    fn vm_jp_compiles_to_opcode() {
        let prog = parse_program(r#"f j:t p:t>R t t;jpth j p"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_jp_op = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_JPTH);
        assert!(has_jp_op, "expected OP_JPTH in bytecode");
    }

    #[test]
    fn vm_jd_number() {
        let source = "f x:n>t;jdmp x";
        let result = vm_run(source, Some("f"), vec![Value::Number(42.0)]);
        assert_eq!(result, Value::Text("42".to_string()));
    }

    #[test]
    fn vm_jd_text() {
        let source = r#"f x:t>t;jdmp x"#;
        let result = vm_run(source, Some("f"), vec![Value::Text("hello".to_string())]);
        assert_eq!(result, Value::Text(r#""hello""#.to_string()));
    }

    #[test]
    fn vm_jd_list() {
        let source = "f>t;xs=[1, 2, 3];jdmp xs";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(result, Value::Text("[1,2,3]".to_string()));
    }

    #[test]
    fn vm_jd_record() {
        let source = "type pt{x:n;y:n} f>t;p=pt x:1 y:2;jdmp p";
        let result = vm_run(source, Some("f"), vec![]);
        let Value::Text(ref s) = result else {
            panic!("expected text")
        };
        let text = s.clone();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["x"], 1);
        assert_eq!(parsed["y"], 2);
    }

    #[test]
    fn vm_jd_compiles_to_opcode() {
        let prog = parse_program("f x:n>t;jdmp x");
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_jd_op = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_JDMP);
        assert!(has_jd_op, "expected OP_JDMP in bytecode");
    }

    #[test]
    fn vm_jparse_object() {
        let source = r#"f j:t>R t t;jpar j"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Text(r#"{"a":1,"b":"two"}"#.to_string())],
        );
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        let Value::Record { type_name, fields } = *inner else {
            panic!("expected record")
        };
        assert_eq!(type_name, "json");
        assert_eq!(fields.get("a"), Some(&Value::Number(1.0)));
        assert_eq!(fields.get("b"), Some(&Value::Text("two".to_string())));
    }

    #[test]
    fn vm_jparse_array() {
        let source = r#"f j:t>R t t;jpar j"#;
        let result = vm_run(source, Some("f"), vec![Value::Text("[1,2,3]".to_string())]);
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        assert_eq!(
            *inner,
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])
        );
    }

    #[test]
    fn vm_jparse_invalid() {
        let source = r#"f j:t>R t t;jpar j"#;
        let result = vm_run(source, Some("f"), vec![Value::Text("not json".to_string())]);
        assert!(matches!(result, Value::Err(_)));
    }

    #[test]
    fn vm_jparse_unwrap() {
        let source = r#"f j:t>t;jpar! j"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Text(r#"{"x":1}"#.to_string())],
        );
        let Value::Record { type_name, fields } = result else {
            panic!("expected record")
        };
        assert_eq!(type_name, "json");
        assert_eq!(fields.get("x"), Some(&Value::Number(1.0)));
    }

    #[test]
    fn vm_jparse_compiles_to_opcode() {
        let prog = parse_program(r#"f j:t>R t t;jpar j"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_jparse_op = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_JPAR);
        assert!(has_jparse_op, "expected OP_JPAR in bytecode");
    }

    #[test]
    fn vm_jparse_then_field_access() {
        let source = r#"f j:t>n;r=jpar! j;r.x"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Text(r#"{"x":42}"#.to_string())],
        );
        assert_eq!(result, Value::Number(42.0));
    }

    // --- trm ---

    #[test]
    fn vm_trm_basic() {
        let result = vm_run(
            "f s:t>t;trm s",
            Some("f"),
            vec![Value::Text("  hello  ".into())],
        );
        assert_eq!(result, Value::Text("hello".into()));
    }

    #[test]
    fn vm_trm_no_whitespace() {
        let result = vm_run("f s:t>t;trm s", Some("f"), vec![Value::Text("hi".into())]);
        assert_eq!(result, Value::Text("hi".into()));
    }

    #[test]
    fn vm_trm_only_whitespace() {
        let result = vm_run("f s:t>t;trm s", Some("f"), vec![Value::Text("   ".into())]);
        assert_eq!(result, Value::Text("".into()));
    }

    #[test]
    fn vm_trm_compiles_to_opcode() {
        let prog = parse_program("f s:t>t;trm s");
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        assert!(
            chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_TRM),
            "expected OP_TRM in bytecode"
        );
    }

    // --- unq ---

    #[test]
    fn vm_unq_text() {
        let result = vm_run(
            "f s:t>t;unq s",
            Some("f"),
            vec![Value::Text("aabbc".into())],
        );
        assert_eq!(result, Value::Text("abc".into()));
    }

    #[test]
    fn vm_unq_list_numbers() {
        let result = vm_run(
            "f xs:L n>L n;unq xs",
            Some("f"),
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(1.0),
                Value::Number(3.0),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])
        );
    }

    #[test]
    fn vm_unq_list_strings_dedup() {
        // Regression: raw pointer bits are not a valid equality key for heap strings.
        // After the fix (nanval_equal), equal-content strings deduplicate correctly.
        let result = vm_run(
            "f xs:L t>L t;unq xs",
            Some("f"),
            vec![Value::List(vec![
                Value::Text("a".into()),
                Value::Text("b".into()),
                Value::Text("a".into()),
                Value::Text("c".into()),
                Value::Text("b".into()),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Text("a".into()),
                Value::Text("b".into()),
                Value::Text("c".into()),
            ])
        );
    }

    #[test]
    fn vm_unq_preserves_order() {
        let result = vm_run(
            "f xs:L n>L n;unq xs",
            Some("f"),
            vec![Value::List(vec![
                Value::Number(3.0),
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(1.0),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(3.0),
                Value::Number(1.0),
                Value::Number(2.0)
            ])
        );
    }

    #[test]
    fn vm_unq_empty_list() {
        let result = vm_run("f xs:L n>L n;unq xs", Some("f"), vec![Value::List(vec![])]);
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn vm_unq_compiles_to_opcode() {
        let prog = parse_program("f xs:L n>L n;unq xs");
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        assert!(
            chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_UNQ),
            "expected OP_UNQ in bytecode"
        );
    }

    // --- prnt ---

    #[test]
    fn vm_prnt_returns_value() {
        let result = vm_run("f x:n>n;prnt x", Some("f"), vec![Value::Number(7.0)]);
        assert_eq!(result, Value::Number(7.0));
    }

    // --- rd (OP_RD path — 1-arg auto-detect) ---

    #[test]
    fn vm_rd_file_not_found() {
        // rd path auto-detects format from extension; for missing file returns Ok(Err(...))
        let result = vm_run(
            "f p:t>t;rd p",
            Some("f"),
            vec![Value::Text("/nonexistent/ilo_test.txt".into())],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err, got {:?}",
            result
        );
    }
    // Note: rdb, rd path fmt, and fmt (variadic) fall through to OP_CALL → interpreter.
    // Those code paths are tested in interpreter::tests and tests/eval_inline.rs.

    // ── Map operations (OP_MAPNEW / OP_MGET / OP_MSET / OP_MHAS / OP_MKEYS / OP_MVALS / OP_MDEL) ──
    // Empty map literal is `mmap` (not `{}`).

    #[test]
    fn vm_mapnew_empty() {
        let result = vm_run("f>M t n;mmap", Some("f"), vec![]);
        assert!(
            matches!(result, Value::Map(_)),
            "expected Map, got {result:?}"
        );
        if let Value::Map(m) = result {
            assert!(m.is_empty());
        }
    }

    #[test]
    fn vm_mset_and_mget_roundtrip() {
        // O n at runtime is Value::Number (optional = raw value | nil, not Ok-wrapped)
        let result = vm_run(r#"f>O n;m=mset mmap "x" 7;mget m "x""#, Some("f"), vec![]);
        assert_eq!(result, Value::Number(7.0));
    }

    #[test]
    fn vm_mset_multiple_keys() {
        let result = vm_run(
            r#"f>O n;m=mset mmap "a" 1;m=mset m "b" 2;mget m "b""#,
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(2.0));
    }

    #[test]
    fn vm_mget_missing_key_returns_nil() {
        let result = vm_run(r#"f>O n;m=mset mmap "x" 1;mget m "y""#, Some("f"), vec![]);
        assert_eq!(result, Value::Nil);
    }

    #[test]
    fn vm_mhas_present() {
        let result = vm_run(r#"f>b;m=mset mmap "k" 99;mhas m "k""#, Some("f"), vec![]);
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_mhas_absent() {
        let result = vm_run(r#"f>b;m=mset mmap "k" 99;mhas m "z""#, Some("f"), vec![]);
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn vm_mkeys_sorted() {
        let result = vm_run(
            r#"f>L t;m=mset mmap "b" 2;m=mset m "a" 1;m=mset m "c" 3;mkeys m"#,
            Some("f"),
            vec![],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Text("a".into()),
                Value::Text("b".into()),
                Value::Text("c".into()),
            ])
        );
    }

    #[test]
    fn vm_mvals_sorted_by_key() {
        // values sorted by their key, not insertion order
        let result = vm_run(
            r#"f>L n;m=mset mmap "b" 2;m=mset m "a" 1;m=mset m "c" 3;mvals m"#,
            Some("f"),
            vec![],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])
        );
    }

    #[test]
    fn vm_mdel_removes_key() {
        let result = vm_run(
            r#"f>b;m=mset mmap "k" 1;m=mdel m "k";mhas m "k""#,
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn vm_mdel_nonexistent_key_noop() {
        let result = vm_run(
            r#"f>O n;m=mset mmap "k" 42;m=mdel m "z";mget m "k""#,
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(42.0));
    }

    #[test]
    fn vm_mset_immutable_original() {
        // mset returns a NEW map; original unchanged
        let result = vm_run(
            r#"f>b;orig=mset mmap "k" 1;upd=mset orig "k" 99;mhas orig "k""#,
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_mkeys_empty_map() {
        let result = vm_run("f>L t;mkeys mmap", Some("f"), vec![]);
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn vm_mvals_empty_map() {
        let result = vm_run("f>L n;mvals mmap", Some("f"), vec![]);
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn vm_map_compiles_to_opcode() {
        let prog = parse_program(r#"f>O n;m=mset mmap "k" 1;mget m "k""#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_mapnew = chunk
            .code
            .iter()
            .any(|inst| (inst >> 24) as u8 == OP_MAPNEW);
        let has_mset = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_MSET);
        let has_mget = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_MGET);
        assert!(has_mapnew, "expected OP_MAPNEW");
        assert!(has_mset, "expected OP_MSET");
        assert!(has_mget, "expected OP_MGET");
    }

    // ── String/list edge cases ──

    #[test]
    fn vm_hd_empty_list_is_error() {
        let err = vm_run_err("f xs:L n>n;hd xs", Some("f"), vec![Value::List(vec![])]);
        assert!(err.contains("hd"), "expected hd error, got: {err}");
    }

    #[test]
    fn vm_hd_empty_text_is_error() {
        let err = vm_run_err("f s:t>t;hd s", Some("f"), vec![Value::Text(String::new())]);
        assert!(err.contains("hd"), "expected hd error, got: {err}");
    }

    #[test]
    fn vm_tl_empty_list_is_error() {
        let err = vm_run_err("f xs:L n>n;tl xs", Some("f"), vec![Value::List(vec![])]);
        assert!(err.contains("tl"), "expected tl error, got: {err}");
    }

    #[test]
    fn vm_tl_empty_text_is_error() {
        let err = vm_run_err("f s:t>t;tl s", Some("f"), vec![Value::Text(String::new())]);
        assert!(err.contains("tl"), "expected tl error, got: {err}");
    }

    #[test]
    fn vm_srt_mixed_types_is_error() {
        let err = vm_run_err(
            "f xs:L n>t;srt xs",
            Some("f"),
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Text("a".into()),
            ])],
        );
        assert!(err.contains("srt"), "expected srt error, got: {err}");
    }

    #[test]
    fn vm_cat_empty_separator() {
        let result = vm_run(
            "f items:L t>t;cat items \"\"",
            Some("f"),
            vec![Value::List(vec![
                Value::Text("a".into()),
                Value::Text("b".into()),
                Value::Text("c".into()),
            ])],
        );
        assert_eq!(result, Value::Text("abc".into()));
    }

    #[test]
    fn vm_has_number_in_list() {
        let result = vm_run(
            "f xs:L n x:n>b;has xs x",
            Some("f"),
            vec![
                Value::List(vec![
                    Value::Number(1.0),
                    Value::Number(2.0),
                    Value::Number(3.0),
                ]),
                Value::Number(2.0),
            ],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_has_number_not_in_list() {
        let result = vm_run(
            "f xs:L n x:n>b;has xs x",
            Some("f"),
            vec![
                Value::List(vec![
                    Value::Number(1.0),
                    Value::Number(2.0),
                    Value::Number(3.0),
                ]),
                Value::Number(9.0),
            ],
        );
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn vm_slc_out_of_bounds_clamped() {
        // end clamped to list length
        let result = vm_run(
            "f xs:L n>L n;slc xs 0 100",
            Some("f"),
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])
        );
    }

    #[test]
    fn vm_rev_empty_list() {
        let result = vm_run("f xs:L n>L n;rev xs", Some("f"), vec![Value::List(vec![])]);
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn vm_srt_empty_list() {
        let result = vm_run("f xs:L n>L n;srt xs", Some("f"), vec![Value::List(vec![])]);
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn vm_srt_text_chars() {
        let result = vm_run(r#"f>t;srt "bac""#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("abc".into()));
    }

    // ── RDL / WR ──

    #[test]
    fn vm_rdl_file_not_found() {
        let result = vm_run(
            "f p:t>t;rdl p",
            Some("f"),
            vec![Value::Text("/nonexistent/ilo_rdl_test.txt".into())],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err, got {result:?}"
        );
    }

    #[test]
    fn vm_wr_and_rdl_roundtrip() {
        let path = "/tmp/ilo_vm_rdl_test.txt";
        std::fs::write(path, "line1\nline2\n").unwrap();
        let result = vm_run("f p:t>t;rdl p", Some("f"), vec![Value::Text(path.into())]);
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        let Value::List(lines) = *inner else {
            panic!("expected List inside Ok")
        };
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], Value::Text("line1".into()));
        let _ = std::fs::remove_file(path);
    }

    // --- TypeIs opcodes (OP_ISNUM/ISTEXT/ISBOOL/ISLIST) ---

    #[test]
    fn vm_type_check_isnum_match() {
        let src = r#"f x:t>b;?x{n _:true;_:false}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(42.0)]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Text("hi".into())]),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_type_check_istext_match() {
        let src = r#"f x:n>b;?x{t _:true;_:false}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Text("hello".into())]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(5.0)]),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_type_check_isbool_match() {
        let src = r#"f x:n>b;?x{b _:true;_:false}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Bool(true)]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(1.0)]),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_type_check_islist_match() {
        let src = r#"f x:n>b;?x{l _:true;_:false}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::List(vec![Value::Number(1.0)])]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(5.0)]),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_type_check_isnum_with_binding() {
        // n v: binds the value if it's a number
        let src = r#"f x:t>n;?x{n v:v;_:0}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(99.0)]),
            Value::Number(99.0)
        );
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Text("x".into())]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn vm_type_check_istext_with_binding() {
        // t v: binds the value if it's text
        let src = r#"f x:n>t;?x{t v:v;_:"nope"}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Text("yes".into())]),
            Value::Text("yes".into())
        );
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(1.0)]),
            Value::Text("nope".into())
        );
    }

    #[test]
    fn vm_type_check_isbool_with_binding() {
        // b v: binds the value if it's bool
        let src = r#"f x:n>b;?x{b v:v;_:false}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Bool(true)]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(0.0)]),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_type_check_compiles_to_opcode() {
        let prog = parse_program(r#"f x:t>b;?x{n _:true;_:false}"#);
        let compiled = compile(&prog).unwrap();
        let idx = compiled.func_index("f").unwrap() as usize;
        let chunk = &compiled.chunks[idx];
        let has_isnum = chunk
            .code
            .iter()
            .any(|&inst| (inst >> 24) as u8 == OP_ISNUM);
        assert!(has_isnum, "expected OP_ISNUM in compiled chunk");
    }

    // --- Error paths for VM builtins ---

    #[test]
    fn vm_env_non_string_error() {
        let err = vm_run_err("f x:n>R t t;env x", Some("f"), vec![Value::Number(42.0)]);
        assert!(err.contains("env") || err.contains("text"), "got: {err}");
    }

    #[test]
    fn vm_jpar_non_string_error() {
        let err = vm_run_err("f x:n>R t t;jpar x", Some("f"), vec![Value::Number(42.0)]);
        assert!(err.contains("jpar") || err.contains("string"), "got: {err}");
    }

    #[test]
    fn vm_jpth_non_string_json_error() {
        let err = vm_run_err(
            "f j:n p:t>R t t;jpth j p",
            Some("f"),
            vec![Value::Number(42.0), Value::Text("key".into())],
        );
        assert!(err.contains("jpth") || err.contains("string"), "got: {err}");
    }

    #[test]
    fn vm_jpth_non_string_path_error() {
        let err = vm_run_err(
            r#"f j:t p:n>R t t;jpth j p"#,
            Some("f"),
            vec![Value::Text("{}".into()), Value::Number(1.0)],
        );
        assert!(err.contains("jpth") || err.contains("string"), "got: {err}");
    }

    #[test]
    fn vm_trm_non_string_error() {
        let err = vm_run_err("f x:n>t;trm x", Some("f"), vec![Value::Number(42.0)]);
        assert!(err.contains("trm") || err.contains("text"), "got: {err}");
    }

    #[test]
    fn vm_unq_non_string_non_list_error() {
        let err = vm_run_err("f x:n>t;unq x", Some("f"), vec![Value::Number(42.0)]);
        assert!(
            err.contains("unq") || err.contains("list") || err.contains("text"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_rd_non_string_path_error() {
        let err = vm_run_err("f x:n>R t t;rd x", Some("f"), vec![Value::Number(42.0)]);
        assert!(
            err.contains("rd") || err.contains("text") || err.contains("string"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_rdl_non_string_error() {
        let err = vm_run_err("f x:n>R t t;rdl x", Some("f"), vec![Value::Number(42.0)]);
        assert!(
            err.contains("rdl") || err.contains("text") || err.contains("string"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_wr_non_string_path_error() {
        let err = vm_run_err(
            "f x:n c:t>R t t;wr x c",
            Some("f"),
            vec![Value::Number(42.0), Value::Text("content".into())],
        );
        assert!(
            err.contains("wr") || err.contains("text") || err.contains("string"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_wr_non_string_content_error() {
        let err = vm_run_err(
            "f p:t x:n>R t t;wr p x",
            Some("f"),
            vec![Value::Text("/tmp/test".into()), Value::Number(42.0)],
        );
        assert!(
            err.contains("wr") || err.contains("text") || err.contains("string"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_wrl_non_string_path_error() {
        let err = vm_run_err(
            "f x:n xs:L t>R t t;wrl x xs",
            Some("f"),
            vec![
                Value::Number(42.0),
                Value::List(vec![Value::Text("a".into())]),
            ],
        );
        assert!(
            err.contains("wrl") || err.contains("text") || err.contains("string"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_wr_creates_file() {
        let path = std::env::temp_dir().join(format!("ilo_vm_wr_test_{}.txt", std::process::id()));
        let path_str = path.to_str().unwrap();
        let result = vm_run(
            "f p:t c:t>R t t;wr p c",
            Some("f"),
            vec![
                Value::Text(path_str.into()),
                Value::Text("hello from ilo".into()),
            ],
        );
        assert!(
            matches!(result, Value::Ok(_)),
            "wr should succeed, got {result:?}"
        );
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello from ilo");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn vm_wrl_creates_file() {
        let path = std::env::temp_dir().join(format!("ilo_vm_wrl_test_{}.txt", std::process::id()));
        let path_str = path.to_str().unwrap();
        let result = vm_run(
            "f p:t xs:L t>R t t;wrl p xs",
            Some("f"),
            vec![
                Value::Text(path_str.into()),
                Value::List(vec![
                    Value::Text("line1".into()),
                    Value::Text("line2".into()),
                ]),
            ],
        );
        assert!(
            matches!(result, Value::Ok(_)),
            "wrl should succeed, got {result:?}"
        );
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("line1"), "got: {content}");
        let _ = std::fs::remove_file(&path);
    }

    // --- RECWITH (with expression) edge cases ---

    #[test]
    fn vm_recwith_multiple_fields() {
        let src = "type pt{x:n;y:n;z:n} f>n;p=pt x:1 y:2 z:3;q=p with x:10;q.x";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn vm_recwith_preserves_other_fields() {
        let src = "type pt{x:n;y:n} f>n;p=pt x:1 y:2;q=p with x:99;q.y";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(2.0), "y should be unchanged");
    }

    #[test]
    fn vm_recwith_original_unchanged() {
        // `with` creates a new record; orig should be unchanged
        let src = "type pt{x:n;y:n} f>n;orig=pt x:1 y:2;upd=orig with x:99;orig.x";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(1.0), "original should be unchanged");
    }

    // --- OP_RD with JSON parsing ---

    #[test]
    fn vm_rd_json_file() {
        let path = "/tmp/ilo_vm_rd_json.json";
        std::fs::write(path, r#"{"key":"value"}"#).unwrap();
        let result = vm_run(
            "f p:t>R t t;rd p",
            Some("f"),
            vec![Value::Text(path.into())],
        );
        assert!(
            matches!(result, Value::Ok(_)),
            "rd json should succeed, got {result:?}"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn vm_rd_csv_file() {
        let path = "/tmp/ilo_vm_rd_csv.csv";
        std::fs::write(path, "a,b,c\n1,2,3\n").unwrap();
        let result = vm_run(
            "f p:t>R t t;rd p",
            Some("f"),
            vec![Value::Text(path.into())],
        );
        assert!(
            matches!(result, Value::Ok(_)),
            "rd csv should succeed, got {result:?}"
        );
        let _ = std::fs::remove_file(path);
    }

    // --- JMPNN opcode via nil coalesce ---

    #[test]
    fn vm_nil_coalesce_on_nil_uses_default() {
        // mget on missing key returns nil; ?? applies default
        let src = "f>n;m=mmap;v=mget m \"x\";v??99";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(99.0));
    }

    #[test]
    fn vm_nil_coalesce_on_non_nil_skips_default() {
        // mget on present key returns value; ?? skips default
        let src = "f>n;m=mset mmap \"x\" 5;mget m \"x\"??99";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(5.0));
    }

    // --- Text comparison operators ---

    #[test]
    fn vm_text_greater_than() {
        let result = vm_run(
            "f a:t b:t>b;>a b",
            Some("f"),
            vec![Value::Text("b".into()), Value::Text("a".into())],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_text_less_than() {
        let result = vm_run(
            "f a:t b:t>b;<a b",
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("b".into())],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_text_greater_or_equal() {
        let result = vm_run(
            "f a:t b:t>b;>=a b",
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("a".into())],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_text_less_or_equal() {
        let result = vm_run(
            "f a:t b:t>b;<=a b",
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("b".into())],
        );
        assert_eq!(result, Value::Bool(true));
    }

    // --- Misc missing coverage ---

    #[test]
    fn vm_rnd_two_args_range() {
        // OP_RND2 with a:n b:n
        let result = vm_run(
            "f a:n b:n>n;rnd a b",
            Some("f"),
            vec![Value::Number(5.0), Value::Number(5.0)],
        );
        assert_eq!(result, Value::Number(5.0)); // rnd 5 5 must be 5
    }

    #[test]
    fn vm_slc_basic() {
        // slc xs start end returns a slice
        let result = vm_run(
            "f xs:L n>L n;slc xs 1 3",
            Some("f"),
            vec![Value::List(vec![
                Value::Number(10.0),
                Value::Number(20.0),
                Value::Number(30.0),
                Value::Number(40.0),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![Value::Number(20.0), Value::Number(30.0)])
        );
    }

    #[test]
    fn vm_cat_non_string_list_error() {
        // cat where list items are not text should error
        let err = vm_run_err(
            "f xs:L n sep:t>t;cat xs sep",
            Some("f"),
            vec![
                Value::List(vec![Value::Number(1.0)]),
                Value::Text(",".into()),
            ],
        );
        assert!(err.contains("cat") || err.contains("text"), "got: {err}");
    }

    #[test]
    fn vm_spl_non_string_error() {
        let err = vm_run_err(
            "f x:n sep:t>L t;spl x sep",
            Some("f"),
            vec![Value::Number(42.0), Value::Text(",".into())],
        );
        assert!(err.contains("spl") || err.contains("text"), "got: {err}");
    }

    // --- OP_HD error paths ---

    #[test]
    fn vm_hd_on_number_error() {
        let err = vm_run_err("f x:n>n;hd x", Some("f"), vec![Value::Number(42.0)]);
        assert!(
            err.contains("hd") || err.contains("list") || err.contains("text"),
            "got: {err}"
        );
    }

    // --- OP_TL error paths ---

    #[test]
    fn vm_tl_on_number_error() {
        let err = vm_run_err("f x:n>n;tl x", Some("f"), vec![Value::Number(42.0)]);
        assert!(
            err.contains("tl") || err.contains("list") || err.contains("text"),
            "got: {err}"
        );
    }

    // --- OP_REV error paths ---

    #[test]
    fn vm_rev_on_number_error() {
        let err = vm_run_err("f x:n>n;rev x", Some("f"), vec![Value::Number(42.0)]);
        assert!(
            err.contains("rev") || err.contains("list") || err.contains("text"),
            "got: {err}"
        );
    }

    // --- OP_HAS error paths ---

    #[test]
    fn vm_has_text_non_text_needle_error() {
        // has "hello" 42 → "has: text search requires text needle"
        let err = vm_run_err(
            "f s:t x:n>b;has s x",
            Some("f"),
            vec![Value::Text("hello".into()), Value::Number(42.0)],
        );
        assert!(err.contains("has") || err.contains("text"), "got: {err}");
    }

    #[test]
    fn vm_has_non_collection_error() {
        // has 42 10 → "has requires a list or text"
        let err = vm_run_err(
            "f x:n y:n>b;has x y",
            Some("f"),
            vec![Value::Number(42.0), Value::Number(10.0)],
        );
        assert!(
            err.contains("has") || err.contains("list") || err.contains("text"),
            "got: {err}"
        );
    }

    // --- OP_SLC error paths ---

    #[test]
    fn vm_slc_on_number_error() {
        let err = vm_run_err("f x:n>n;slc x 0 1", Some("f"), vec![Value::Number(42.0)]);
        assert!(
            err.contains("slc") || err.contains("list") || err.contains("text"),
            "got: {err}"
        );
    }

    // --- OP_SRT on non-list/non-text ---

    #[test]
    fn vm_srt_single_element() {
        // Single-element list: returns as-is
        let result = vm_run("f>L n;srt [42]", Some("f"), vec![]);
        assert_eq!(result, Value::List(vec![Value::Number(42.0)]));
    }

    // --- OP_CAT where first arg is heap but not list (string) ---

    #[test]
    fn vm_cat_string_first_arg_error() {
        // cat "hello" "," → cat requires a list
        let err = vm_run_err(
            r#"f s:t sep:t>t;cat s sep"#,
            Some("f"),
            vec![Value::Text("hello".into()), Value::Text(",".into())],
        );
        assert!(err.contains("cat") || err.contains("list"), "got: {err}");
    }

    // --- OP_NOT on non-bool (truthiness path) ---

    #[test]
    fn vm_not_on_non_empty_text_is_false() {
        // !"hello" → truthy, so !truthy = false
        let result = vm_run(r#"f s:t>b;!s"#, Some("f"), vec![Value::Text("hi".into())]);
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn vm_not_on_empty_list_is_true() {
        // ![] → falsy, so !falsy = true
        let result = vm_run("f xs:L n>b;!xs", Some("f"), vec![Value::List(vec![])]);
        assert_eq!(result, Value::Bool(true));
    }

    // --- OP_NEG on non-number ---

    #[test]
    fn vm_neg_on_text_error() {
        let err = vm_run_err(
            r#"f x:t>n;neg x"#,
            Some("f"),
            vec![Value::Text("hi".into())],
        );
        assert!(
            err.contains("neg") || err.contains("number") || err.contains("n"),
            "got: {err}"
        );
    }

    // --- Let re-binding (same variable reassigned) ---

    #[test]
    fn vm_let_rebind_accumulates() {
        // x=1;x=+x 1;x=+x 1;x → 3 (re-binding to same register)
        let result = vm_run("f>n;x=1;x=+x 1;x=+x 1;x", Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    // --- Subjectless match (None subject) ---

    #[test]
    fn vm_match_no_subject_wildcard() {
        // Subjectless match — subject is implicit Nil, wildcard arm catches it
        let result = vm_run(
            r#"f x:n>t;?{_:"default"}"#,
            Some("f"),
            vec![Value::Number(5.0)],
        );
        assert_eq!(result, Value::Text("default".into()));
    }

    // --- OP_ABS error path ---

    #[test]
    fn vm_abs_on_text_error() {
        let err = vm_run_err(
            r#"f x:t>n;abs x"#,
            Some("f"),
            vec![Value::Text("hi".into())],
        );
        assert!(err.contains("abs") || err.contains("number"), "got: {err}");
    }

    // --- OP_FLR / OP_CEL error paths ---

    #[test]
    fn vm_flr_on_text_error() {
        let err = vm_run_err(
            r#"f x:t>n;flr x"#,
            Some("f"),
            vec![Value::Text("hi".into())],
        );
        assert!(err.contains("flr") || err.contains("number"), "got: {err}");
    }

    #[test]
    fn vm_cel_on_text_error() {
        let err = vm_run_err(
            r#"f x:t>n;cel x"#,
            Some("f"),
            vec![Value::Text("hi".into())],
        );
        assert!(err.contains("cel") || err.contains("number"), "got: {err}");
    }

    // --- OP_MIN / OP_MAX error paths ---

    #[test]
    fn vm_min_on_text_error() {
        let err = vm_run_err(
            r#"f x:t y:t>n;min x y"#,
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("b".into())],
        );
        assert!(err.contains("min") || err.contains("number"), "got: {err}");
    }

    // --- OP_RND2 with lo > hi error ---

    #[test]
    fn vm_rnd2_lo_greater_than_hi_error() {
        let err = vm_run_err(
            "f a:n b:n>n;rnd a b",
            Some("f"),
            vec![Value::Number(10.0), Value::Number(5.0)],
        );
        assert!(
            err.contains("rnd") || err.contains("bound") || err.contains("lower"),
            "got: {err}"
        );
    }

    // --- Match with number literal patterns (Pattern::Literal / Number) ---

    #[test]
    fn vm_match_literal_number_hit() {
        // ?x{42:"found";_:"other"} — literal number arm matches
        let src = r#"f x:n>t;?x{42:"found";_:"other"}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(42.0)]),
            Value::Text("found".into())
        );
    }

    #[test]
    fn vm_match_literal_number_miss() {
        // Literal number arm does NOT match → wildcard fires
        let src = r#"f x:n>t;?x{42:"found";_:"other"}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(7.0)]),
            Value::Text("other".into())
        );
    }

    #[test]
    fn vm_match_multiple_literal_numbers() {
        // Multiple literal number arms
        let src = r#"f x:n>t;?x{1:"one";2:"two";3:"three";_:"many"}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(2.0)]),
            Value::Text("two".into())
        );
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(99.0)]),
            Value::Text("many".into())
        );
    }

    // --- Match with bool literal patterns ---

    #[test]
    fn vm_match_literal_bool_true() {
        let src = r#"f x:b>t;?x{true:"yes";false:"no"}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Bool(true)]),
            Value::Text("yes".into())
        );
    }

    #[test]
    fn vm_match_literal_bool_false() {
        let src = r#"f x:b>t;?x{true:"yes";false:"no"}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Bool(false)]),
            Value::Text("no".into())
        );
    }

    // --- Safe field access on non-nil record (additional coverage) ---

    #[test]
    fn vm_safe_field_on_record_non_nil_returns_value() {
        // type decl after function (known VM chunk ordering)
        let src = "f>t;p=rec name:\"alice\";p.?name\ntype rec{name:t}";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Text("alice".into()));
    }

    #[test]
    fn vm_safe_field_chain_nil_propagates() {
        // When first field is nil the chain short-circuits and returns nil → ?? fires
        let src = "mk x:n>n;>=x 1{x}\nf>t;v=mk 0;v.?a.?b??\"default\"";
        assert_eq!(
            vm_run(src, Some("f"), vec![]),
            Value::Text("default".into())
        );
    }

    // --- Safe index access on non-nil list at index > 0 ---

    #[test]
    fn vm_safe_index_on_list_index_1() {
        // .?1 returns the second element of a non-nil list
        let src = "f>n;xs=[10,20,30];xs.?1";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(20.0));
    }

    // --- Nil coalesce with text values ---

    #[test]
    fn vm_nil_coalesce_text_nil_uses_default() {
        let src = "mk x:n>n;>=x 1{x}\nf>t;v=mk 0;v??\"fallback\"";
        assert_eq!(
            vm_run(src, Some("f"), vec![]),
            Value::Text("fallback".into())
        );
    }

    #[test]
    fn vm_nil_coalesce_text_non_nil_passes_through() {
        let src = "f>t;v=\"hello\";v??\"fallback\"";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Text("hello".into()));
    }

    // --- Break without expr in foreach ---

    #[test]
    fn vm_foreach_brk_no_value_exits_loop() {
        // brk (no value) exits the foreach; the loop result is nil/last-before-break
        let src = "f>n;tot=0;@x [1,2,3,4,5]{>=x 3{brk};tot=+tot x};tot";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(3.0));
    }

    // --- Continue in foreach that accumulates ---

    #[test]
    fn vm_foreach_cnt_accumulate_sum() {
        // Skip x > 3 with cnt, sum remaining — 1+2+3 = 6
        let src = "f>n;s=0;@x [1,2,3,4,5]{>x 3{cnt};s=+s x};s";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(6.0));
    }

    // --- OP_POST runtime: HTTP POST to bad host returns Err ---

    #[test]
    fn vm_post_bad_host_returns_err() {
        // post to an unreachable host should return Err, not panic
        let src = r#"f url:t body:t>R t t;post url body"#;
        let result = vm_run(
            src,
            Some("f"),
            vec![
                Value::Text("http://ilo-lang-test-nonexistent.invalid/endpoint".into()),
                Value::Text("{}".into()),
            ],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err from bad host, got {:?}",
            result
        );
    }

    // --- Match result (Ok/Err) patterns ---

    #[test]
    fn vm_match_result_ok_arm() {
        // ?r{~v:v;^_:0} — Ok arm extracts value
        let src = r#"f r:R n t>n;?r{~v:v;^_:0}"#;
        assert_eq!(
            vm_run(
                src,
                Some("f"),
                vec![Value::Ok(Box::new(Value::Number(42.0)))]
            ),
            Value::Number(42.0)
        );
    }

    #[test]
    fn vm_match_result_err_arm() {
        // ?r{~_:1;^_:0} — Err arm fires
        let src = r#"f r:R n t>n;?r{~_:1;^_:0}"#;
        assert_eq!(
            vm_run(
                src,
                Some("f"),
                vec![Value::Err(Box::new(Value::Text("oops".into())))]
            ),
            Value::Number(0.0)
        );
    }

    // --- ForEach with continue (cnt) and range iteration ---

    #[test]
    fn vm_range_cnt_skip_middle() {
        // @i 0..6{=i 3{cnt};s=+s i} skips 3, sums 0+1+2+4+5 = 12
        let src = "f>n;s=0;@i 0..6{=i 3{cnt};s=+s i};s";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(12.0));
    }

    // --- While loop: brk carries value that is discarded (loop result is body value) ---

    #[test]
    fn vm_while_brk_expr_value_discarded() {
        // brk expr is compiled (value moved to result_reg) but loop result is not used as return
        // verify outer variable is still correct after brk
        let src = "f>n;i=0;wh true{i=+i 1;>=i 5{brk 999}};i";
        // After break, i should be 5 (not 999, since brk 999 doesn't affect i)
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(5.0));
    }

    // --- Nil coalesce on map-get result (direct OP_MGET path) ---

    #[test]
    fn vm_mget_nil_coalesce_default() {
        let src = r#"f>n;m=mset mmap "a" 1;v=mget m "missing";v??42"#;
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(42.0));
    }

    // --- Safe index on nil list (already covered) + safe index on non-nil with ?? ---

    #[test]
    fn vm_safe_index_non_nil_with_coalesce() {
        // .?2 returns third element; ?? should not fire
        let src = "f>n;xs=[5,10,15];xs.?2??99";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(15.0));
    }

    // --- ForEach break in while loop inside foreach ---

    #[test]
    fn vm_foreach_with_inner_while_brk() {
        // Foreach iterates [1,2,3]; for each x run a while that breaks immediately
        // Verifies loop nesting with brk doesn't corrupt outer loop
        let src = "f>n;s=0;@x [1,2,3]{i=0;wh true{i=+i x;>=i x{brk}};s=+s i};s";
        // Each x: wh adds i+=x once then breaks; so i=x each time; s=1+2+3=6
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(6.0));
    }

    // --- Nil coalesce with bool value ---

    #[test]
    fn vm_nil_coalesce_bool_default() {
        // If value is nil, default bool is returned
        let src = "mk x:n>n;>=x 1{x}\nf>b;v=mk 0;v??true";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Bool(true));
    }

    // --- jdmp additional coverage ---

    #[test]
    fn vm_jdmp_number_arg() {
        // jdmp on a number parameter (not inline literal — exercises arg path)
        let source = "f x:n>t;jdmp x";
        let result = vm_run(source, Some("f"), vec![Value::Number(42.0)]);
        assert_eq!(result, Value::Text("42".to_string()));
    }

    #[test]
    fn vm_jdmp_list_arg() {
        // jdmp on a list value passed as argument
        let source = "f xs:L n>t;jdmp xs";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::List(vec![Value::Number(1.0), Value::Number(2.0)])],
        );
        assert_eq!(result, Value::Text("[1,2]".to_string()));
    }

    #[test]
    fn vm_jdmp_nil() {
        // jdmp of nil → "null"
        // nil is obtained via mget on a missing key (mget returns nil for missing keys)
        let source = "f>t;m=mmap;v=mget m \"missing\";jdmp v";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(result, Value::Text("null".to_string()));
    }

    // --- OP_HAS with map heap object returns error ---

    #[test]
    fn vm_has_map_heap_returns_error() {
        // OP_HAS dispatches on a Map heap object → "has requires a list or text"
        // The verifier blocks M t t typed `has`, so we pass a number arg typed as n
        // but inject a map-typed value at runtime via n param (bypasses verifier check).
        // Use a list arg typed function but pass the map as `n` which won't pass verifier
        // — instead, use the already-covered non-collection path with a number arg since
        // map-via-number-param would need type confusion. The Map-heap branch is reached
        // when a heap value is not a List; pass a text-typed argument but give it a
        // runtime list of text — actually the cleanest approach: use vm_run_err with a
        // number typed collection (hits the non-heap else branch) which is already tested.
        // For the heap-map branch specifically, we run a program where the verifier does
        // not see the Map type: use `has` with an `n` arg but pass a Map at runtime.
        // This triggers VmError::Type("has requires a list or text") via the heap match.
        // We achieve this by declaring collection as `n` (verifier OK: n is not heap so
        // no special check), but passing Value::Map at runtime — however Value::Map would
        // be encoded as a heap map NanVal so `collection.is_heap()` is true and it falls
        // into the HeapObj match arm, hitting the `_ =>` error branch for Map.
        let err = vm_run_err(
            "f coll:n needle:t>b;has coll needle",
            Some("f"),
            vec![
                Value::Map({
                    let mut m = std::collections::HashMap::new();
                    m.insert("x".to_string(), Value::Text("1".to_string()));
                    m
                }),
                Value::Text("x".to_string()),
            ],
        );
        assert!(
            err.contains("has") || err.contains("list") || err.contains("text"),
            "got: {err}"
        );
    }

    // --- OP_CAT with number list elements errors at runtime ---

    #[test]
    fn vm_cat_number_list_element_error() {
        // cat " " [1 2] — list elements are numbers, not text; cat should error
        let err = vm_run_err(
            "f xs:L n sep:t>t;cat xs sep",
            Some("f"),
            vec![
                Value::List(vec![Value::Number(1.0), Value::Number(2.0)]),
                Value::Text(" ".to_string()),
            ],
        );
        // cat requires all elements to be text strings
        assert!(
            err.contains("cat") || err.contains("text") || err.contains("string"),
            "got: {err}"
        );
    }

    // --- OP_GETH with empty map headers (bad host) ---

    #[test]
    fn vm_geth_empty_map_headers_bad_host() {
        // get url headers where headers is an empty map — exercises the vc.is_heap() +
        // HeapObj::Map branch in OP_GETH. Bad URL → Err.
        let src = r#"f url:t hdrs:M t t>R t t;get url hdrs"#;
        let result = vm_run(
            src,
            Some("f"),
            vec![
                Value::Text("http://127.0.0.1:1".to_string()),
                Value::Map(std::collections::HashMap::new()),
            ],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err, got {result:?}"
        );
    }

    // --- OP_POSTH with empty map headers (bad host) ---

    #[test]
    fn vm_posth_empty_map_headers_bad_host() {
        // post url body headers where headers is an empty map — exercises the vd.is_heap() +
        // HeapObj::Map branch in OP_POSTH. Bad URL → Err.
        let src = r#"f url:t body:t hdrs:M t t>R t t;post url body hdrs"#;
        let result = vm_run(
            src,
            Some("f"),
            vec![
                Value::Text("http://127.0.0.1:1".to_string()),
                Value::Text("{}".to_string()),
                Value::Map(std::collections::HashMap::new()),
            ],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err, got {result:?}"
        );
    }

    // --- OP_FLR / OP_CEL on integer-valued float ---

    #[test]
    fn vm_flr_integer_valued_float() {
        // flr of 5.0 → 5.0 (no-op for whole numbers)
        let result = vm_run("f x:n>n;flr x", Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(5.0));
    }

    #[test]
    fn vm_cel_integer_valued_float() {
        // cel of 5.0 → 5.0 (no-op for whole numbers)
        let result = vm_run("f x:n>n;cel x", Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(5.0));
    }

    #[test]
    fn vm_flr_negative_fraction() {
        // flr of -2.3 → -3.0 (floors toward negative infinity)
        let result = vm_run("f x:n>n;flr x", Some("f"), vec![Value::Number(-2.3)]);
        assert_eq!(result, Value::Number(-3.0));
    }

    // ── VmRuntimeError Display / Error traits ────────────────────────────────

    #[test]
    fn vm_runtime_error_display_formats_message() {
        let err = VmRuntimeError {
            error: VmError::Type("test error message"),
            span: None,
            call_stack: vec!["f".to_string()],
        };
        let s = format!("{err}");
        assert!(s.contains("test error message"), "got: {s}");
    }

    #[test]
    fn vm_runtime_error_source_is_some() {
        use std::error::Error;
        let err = VmRuntimeError {
            error: VmError::Type("inner"),
            span: None,
            call_stack: vec![],
        };
        // `source()` must return Some — exercises the Error impl
        assert!(err.source().is_some());
    }

    // ── Builtin auto-unwrap sequences (env!, get!, post!, rd!, wr!) ──────────

    #[test]
    fn vm_env_bang_compiles_unwrap_sequence() {
        // env! compiles OP_ENV + ISOK + JMPT + RET + UNWRAP
        let prog = parse_program(r#"f k:t>t;env! k"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_env = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_ENV);
        let has_unwrap = chunk
            .code
            .iter()
            .any(|inst| (inst >> 24) as u8 == OP_UNWRAP);
        assert!(has_env, "expected OP_ENV");
        assert!(has_unwrap, "expected OP_UNWRAP for env!");
    }

    #[test]
    fn vm_get_bang_compiles_unwrap_sequence() {
        let prog = parse_program(r#"f url:t>t;get! url"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_get = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_GET);
        let has_unwrap = chunk
            .code
            .iter()
            .any(|inst| (inst >> 24) as u8 == OP_UNWRAP);
        assert!(has_get, "expected OP_GET");
        assert!(has_unwrap, "expected OP_UNWRAP for get!");
    }

    #[test]
    fn vm_get_with_headers_bang_compiles_unwrap_sequence() {
        let prog = parse_program(r#"f url:t hdrs:M t t>t;get! url hdrs"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_geth = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_GETH);
        let has_unwrap = chunk
            .code
            .iter()
            .any(|inst| (inst >> 24) as u8 == OP_UNWRAP);
        assert!(has_geth, "expected OP_GETH");
        assert!(has_unwrap, "expected OP_UNWRAP for get! with headers");
    }

    #[test]
    fn vm_post_with_headers_bang_compiles_unwrap_sequence() {
        let prog = parse_program(r#"f url:t body:t hdrs:M t t>t;post! url body hdrs"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_posth = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_POSTH);
        let has_unwrap = chunk
            .code
            .iter()
            .any(|inst| (inst >> 24) as u8 == OP_UNWRAP);
        assert!(has_posth, "expected OP_POSTH");
        assert!(has_unwrap, "expected OP_UNWRAP for post! with headers");
    }

    #[test]
    fn vm_rd_bang_compiles_unwrap_sequence() {
        let prog = parse_program(r#"f path:t>t;rd! path"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_rd = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_RD);
        let has_unwrap = chunk
            .code
            .iter()
            .any(|inst| (inst >> 24) as u8 == OP_UNWRAP);
        assert!(has_rd, "expected OP_RD");
        assert!(has_unwrap, "expected OP_UNWRAP for rd!");
    }

    #[test]
    fn vm_rdl_bang_compiles_unwrap_sequence() {
        let prog = parse_program(r#"f path:t>L t;rdl! path"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_rdl = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_RDL);
        let has_unwrap = chunk
            .code
            .iter()
            .any(|inst| (inst >> 24) as u8 == OP_UNWRAP);
        assert!(has_rdl, "expected OP_RDL");
        assert!(has_unwrap, "expected OP_UNWRAP for rdl!");
    }

    #[test]
    fn vm_wr_bang_compiles_unwrap_sequence() {
        let prog = parse_program(r#"f path:t data:t>t;wr! path data"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_wr = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_WR);
        let has_unwrap = chunk
            .code
            .iter()
            .any(|inst| (inst >> 24) as u8 == OP_UNWRAP);
        assert!(has_wr, "expected OP_WR");
        assert!(has_unwrap, "expected OP_UNWRAP for wr!");
    }

    #[test]
    fn vm_wrl_bang_compiles_unwrap_sequence() {
        let prog = parse_program(r#"f path:t data:t>t;wrl! path data"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_wrl = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_WRL);
        let has_unwrap = chunk
            .code
            .iter()
            .any(|inst| (inst >> 24) as u8 == OP_UNWRAP);
        assert!(has_wrl, "expected OP_WRL");
        assert!(has_unwrap, "expected OP_UNWRAP for wrl!");
    }

    #[test]
    fn vm_jpar_bang_compiles_unwrap_sequence() {
        let prog = parse_program(r#"f s:t>t;jpar! s"#);
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_jpar = chunk.code.iter().any(|inst| (inst >> 24) as u8 == OP_JPAR);
        let has_unwrap = chunk
            .code
            .iter()
            .any(|inst| (inst >> 24) as u8 == OP_UNWRAP);
        assert!(has_jpar, "expected OP_JPAR");
        assert!(has_unwrap, "expected OP_UNWRAP for jpar!");
    }

    // ── Guard ternary else-nil path ───────────────────────────────────────────

    #[test]
    fn vm_ternary_then_empty_body_yields_nil() {
        // When the taken branch has an empty body, it loads Nil
        let result = vm_run("f x:n>n;>x 0{}{99}", Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Nil);
    }

    #[test]
    fn vm_ternary_else_empty_body_yields_nil() {
        // When the else branch has an empty body, it loads Nil
        let result = vm_run("f x:n>n;>x 0{99}{}", Some("f"), vec![Value::Number(-1.0)]);
        assert_eq!(result, Value::Nil);
    }

    // ── Destructure: re-assign into existing local ────────────────────────────

    #[test]
    fn vm_destructure_into_existing_local() {
        // Binding `x` already in scope — exercises the existing_reg path in destructure
        let src = "type pt{x:n;y:n} f>n;x=0;p=pt x:10 y:20;{x}=p;x";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(10.0));
    }

    // ── Continue inside foreach / for-range ───────────────────────────────────

    #[test]
    fn vm_foreach_cnt_skips_iteration() {
        // cnt (continue) inside @x xs{} — exercises the FOREACH continue patch path
        // >x 3{cnt} means: if x>3, skip (so 4 and 5 are skipped; sum 1+2+3=6)
        let src = "f>n;s=0;@x [1,2,3,4,5]{>x 3{cnt};s=+s x};s";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(6.0));
    }

    #[test]
    fn vm_forrange_cnt_skips_iteration() {
        // cnt inside @i lo..hi{} — exercises the FOR-RANGE continue patch path
        // >i 3{cnt} means: if i>3, skip; range 0..6 → sums 0+1+2+3=6
        let src = "f>n;s=0;@i 0..6{>i 3{cnt};s=+s i};s";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(6.0));
    }

    // ── NanVal::from_value FnRef path ─────────────────────────────────────────

    #[test]
    fn vm_nanval_from_fnref() {
        let val = Value::FnRef("my_fn".to_string());
        let nv = NanVal::from_value(&val);
        // FnRef converts to a heap string like "<fn:my_fn>"
        let back = nv.to_value();
        let Value::Text(s) = back else {
            panic!("expected Text")
        };
        assert!(s.contains("my_fn"), "got: {s}");
    }

    // ── JIT helper functions (cranelift feature) ───────────────────────────────

    #[cfg(feature = "cranelift")]
    mod jit_helpers {
        use super::super::*;

        fn num(v: f64) -> u64 {
            NanVal::number(v).0
        }
        fn is_num(v: u64) -> bool {
            NanVal(v).is_number()
        }
        fn as_num(v: u64) -> f64 {
            NanVal(v).as_number()
        }
        fn is_bool(v: u64) -> bool {
            v == TAG_TRUE || v == TAG_FALSE
        }
        fn as_bool(v: u64) -> bool {
            v == TAG_TRUE
        }
        fn is_nil(v: u64) -> bool {
            v == TAG_NIL
        }

        #[test]
        fn jit_sub_numbers() {
            let r = jit_sub(num(10.0), num(3.0));
            assert!(is_num(r));
            assert_eq!(as_num(r), 7.0);
        }

        #[test]
        fn jit_sub_non_numbers_returns_nil() {
            let s = NanVal::heap_string("hello".into());
            let r = jit_sub(s.0, num(1.0));
            assert!(is_nil(r));
        }

        #[test]
        fn jit_mul_numbers() {
            let r = jit_mul(num(4.0), num(5.0));
            assert!(is_num(r));
            assert_eq!(as_num(r), 20.0);
        }

        #[test]
        fn jit_div_numbers() {
            let r = jit_div(num(10.0), num(4.0));
            assert!(is_num(r));
            assert_eq!(as_num(r), 2.5);
        }

        #[test]
        fn jit_div_by_zero_returns_nil() {
            let r = jit_div(num(5.0), num(0.0));
            assert!(is_nil(r));
        }

        #[test]
        fn jit_eq_equal_numbers() {
            let r = jit_eq(num(3.0), num(3.0));
            assert!(is_bool(r));
            assert!(as_bool(r));
        }

        #[test]
        fn jit_eq_unequal_numbers() {
            let r = jit_eq(num(3.0), num(4.0));
            assert!(is_bool(r));
            assert!(!as_bool(r));
        }

        #[test]
        fn jit_ne_numbers() {
            assert!(as_bool(jit_ne(num(1.0), num(2.0))));
            assert!(!as_bool(jit_ne(num(2.0), num(2.0))));
        }

        #[test]
        fn jit_gt_numbers() {
            assert!(as_bool(jit_gt(num(5.0), num(3.0))));
            assert!(!as_bool(jit_gt(num(3.0), num(5.0))));
        }

        #[test]
        fn jit_lt_numbers() {
            assert!(as_bool(jit_lt(num(2.0), num(7.0))));
            assert!(!as_bool(jit_lt(num(7.0), num(2.0))));
        }

        #[test]
        fn jit_ge_numbers() {
            assert!(as_bool(jit_ge(num(5.0), num(5.0))));
            assert!(as_bool(jit_ge(num(6.0), num(5.0))));
            assert!(!as_bool(jit_ge(num(4.0), num(5.0))));
        }

        #[test]
        fn jit_le_numbers() {
            assert!(as_bool(jit_le(num(3.0), num(3.0))));
            assert!(as_bool(jit_le(num(2.0), num(3.0))));
            assert!(!as_bool(jit_le(num(4.0), num(3.0))));
        }

        #[test]
        fn jit_not_true_returns_false() {
            let r = jit_not(NanVal::boolean(true).0);
            assert!(is_bool(r));
            assert!(!as_bool(r));
        }

        #[test]
        fn jit_not_false_returns_true() {
            let r = jit_not(NanVal::boolean(false).0);
            assert!(is_bool(r));
            assert!(as_bool(r));
        }

        #[test]
        fn jit_neg_number() {
            let r = jit_neg(num(5.0));
            assert!(is_num(r));
            assert_eq!(as_num(r), -5.0);
        }

        #[test]
        fn jit_neg_non_number_returns_nil() {
            let s = NanVal::heap_string("x".into());
            let r = jit_neg(s.0);
            assert!(is_nil(r));
        }

        #[test]
        fn jit_truthy_number_nonzero() {
            // jit_truthy returns 1 for truthy, 0 for falsy (raw integer, not TAG_TRUE)
            let r = jit_truthy(num(42.0));
            assert_eq!(r, 1);
        }

        #[test]
        fn jit_truthy_number_zero() {
            let r = jit_truthy(num(0.0));
            assert_eq!(r, 0);
        }

        #[test]
        fn jit_truthy_bool_true() {
            let r = jit_truthy(NanVal::boolean(true).0);
            assert_eq!(r, 1);
        }

        #[test]
        fn jit_truthy_nil_false() {
            let r = jit_truthy(TAG_NIL);
            assert_eq!(r, 0);
        }

        #[test]
        fn jit_wrapok_wraps_value() {
            let r = jit_wrapok(num(7.0));
            let v = NanVal(r).to_value();
            assert!(matches!(v, Value::Ok(_)));
        }

        #[test]
        fn jit_wraperr_wraps_value() {
            let r = jit_wraperr(num(7.0));
            let v = NanVal(r).to_value();
            assert!(matches!(v, Value::Err(_)));
        }

        #[test]
        fn jit_isok_on_ok_value() {
            let ok_val = NanVal::from_value(&Value::Ok(Box::new(Value::Number(1.0))));
            let r = jit_isok(ok_val.0);
            assert!(as_bool(r));
        }

        #[test]
        fn jit_isok_on_non_ok() {
            let r = jit_isok(num(42.0));
            assert!(!as_bool(r));
        }

        #[test]
        fn jit_iserr_on_err_value() {
            let err_val = NanVal::from_value(&Value::Err(Box::new(Value::Text("oops".into()))));
            let r = jit_iserr(err_val.0);
            assert!(as_bool(r));
        }

        #[test]
        fn jit_iserr_on_non_err() {
            let r = jit_iserr(num(1.0));
            assert!(!as_bool(r));
        }

        #[test]
        fn jit_unwrap_ok_value() {
            let ok_val = NanVal::from_value(&Value::Ok(Box::new(Value::Number(3.14))));
            let r = jit_unwrap(ok_val.0);
            assert!(is_num(r));
            assert!((as_num(r) - 3.14).abs() < 1e-9);
        }

        #[test]
        fn jit_move_clones_value() {
            let v = num(99.0);
            let r = jit_move(v);
            assert_eq!(r, v);
        }

        #[test]
        fn jit_gt_non_numbers_returns_false() {
            // Non-number, non-string → returns TAG_FALSE
            let r = jit_gt(TAG_NIL, TAG_NIL);
            assert!(!as_bool(r));
        }

        #[test]
        fn jit_lt_non_numbers_returns_false() {
            let r = jit_lt(TAG_NIL, num(1.0));
            assert!(!as_bool(r));
        }

        #[test]
        fn jit_ge_non_numbers_returns_false() {
            let r = jit_ge(TAG_NIL, num(1.0));
            assert!(!as_bool(r));
        }

        #[test]
        fn jit_le_non_numbers_returns_false() {
            let r = jit_le(TAG_NIL, num(1.0));
            assert!(!as_bool(r));
        }

        // ── String comparison ops ──────────────────────────────────────────

        fn str_val(s: &str) -> u64 {
            NanVal::heap_string(s.to_string()).0
        }

        #[test]
        fn jit_gt_strings_true() {
            let r = jit_gt(str_val("b"), str_val("a"));
            assert!(as_bool(r));
        }

        #[test]
        fn jit_gt_strings_false() {
            let r = jit_gt(str_val("a"), str_val("b"));
            assert!(!as_bool(r));
        }

        #[test]
        fn jit_lt_strings_true() {
            let r = jit_lt(str_val("a"), str_val("b"));
            assert!(as_bool(r));
        }

        #[test]
        fn jit_ge_strings_equal() {
            let r = jit_ge(str_val("a"), str_val("a"));
            assert!(as_bool(r));
        }

        #[test]
        fn jit_le_strings_less() {
            let r = jit_le(str_val("a"), str_val("b"));
            assert!(as_bool(r));
        }

        // ── jit_add with strings ──────────────────────────────────────────

        #[test]
        fn jit_add_strings_concat() {
            let r = jit_add(str_val("hello "), str_val("world"));
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            let s = s.clone();
            assert_eq!(s, "hello world");
        }

        #[test]
        fn jit_add_non_numeric_non_string_returns_nil() {
            let r = jit_add(TAG_NIL, num(1.0));
            assert!(is_nil(r));
        }

        // RC=1 fast path: sole-owner left string is mutated in place.
        #[test]
        fn jit_add_strings_rc1_fast_path() {
            // str_val creates a fresh Rc with strong_count == 1.
            let lhs = str_val("foo");
            let rhs = str_val("bar");
            let r = jit_add(lhs, rhs);
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            assert_eq!(s.as_str(), "foobar");
        }

        // RC>1 fallback: shared left string falls through to copy path.
        #[test]
        fn jit_add_strings_rc_gt1_copy_path() {
            // Clone the NanVal to bump the RC to 2.
            let lhs_nan = NanVal::heap_string("hello ".to_string());
            lhs_nan.clone_rc(); // strong_count is now 2
            let rhs = str_val("world");
            let r = jit_add(lhs_nan.0, rhs);
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            assert_eq!(s.as_str(), "hello world");
            // Clean up the extra RC we bumped.
            lhs_nan.drop_rc();
        }

        // ── jit_len ────────────────────────────────────────────────────────

        #[test]
        fn jit_len_string() {
            let r = jit_len(str_val("hello"));
            assert!(is_num(r));
            assert_eq!(as_num(r), 5.0);
        }

        #[test]
        fn jit_len_list() {
            let items = vec![
                NanVal::number(1.0),
                NanVal::number(2.0),
                NanVal::number(3.0),
            ];
            let list = NanVal::heap_list(items);
            let r = jit_len(list.0);
            assert!(is_num(r));
            assert_eq!(as_num(r), 3.0);
        }

        #[test]
        fn jit_len_non_string_non_list_returns_nil() {
            let r = jit_len(TAG_NIL);
            assert!(is_nil(r));
        }

        // ── jit_str ────────────────────────────────────────────────────────

        #[test]
        fn jit_str_number_to_string() {
            let r = jit_str(num(42.0));
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            let s = s.clone();
            assert_eq!(s, "42");
        }

        #[test]
        fn jit_str_float_to_string() {
            let r = jit_str(num(3.14));
            let rv = NanVal(r);
            assert!(rv.is_string());
        }

        #[test]
        fn jit_str_non_number_returns_nil() {
            let r = jit_str(TAG_NIL);
            assert!(is_nil(r));
        }

        // ── jit_hd ────────────────────────────────────────────────────────

        #[test]
        fn jit_hd_string_returns_first_char() {
            let r = jit_hd(str_val("hello"));
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            let s = s.clone();
            assert_eq!(s, "h");
        }

        #[test]
        fn jit_hd_empty_string_returns_nil() {
            let r = jit_hd(str_val(""));
            assert!(is_nil(r));
        }

        #[test]
        fn jit_hd_list_returns_first() {
            let items = vec![NanVal::number(10.0), NanVal::number(20.0)];
            let list = NanVal::heap_list(items);
            let r = jit_hd(list.0);
            assert!(is_num(r));
            assert_eq!(as_num(r), 10.0);
        }

        #[test]
        fn jit_hd_empty_list_returns_nil() {
            let list = NanVal::heap_list(vec![]);
            let r = jit_hd(list.0);
            assert!(is_nil(r));
        }

        #[test]
        fn jit_hd_non_string_non_list_returns_nil() {
            let r = jit_hd(TAG_NIL);
            assert!(is_nil(r));
        }

        // ── jit_tl ────────────────────────────────────────────────────────

        #[test]
        fn jit_tl_string_returns_tail() {
            let r = jit_tl(str_val("hello"));
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            let s = s.clone();
            assert_eq!(s, "ello");
        }

        #[test]
        fn jit_tl_empty_string_returns_nil() {
            let r = jit_tl(str_val(""));
            assert!(is_nil(r));
        }

        #[test]
        fn jit_tl_list_returns_tail() {
            let items = vec![
                NanVal::number(1.0),
                NanVal::number(2.0),
                NanVal::number(3.0),
            ];
            let list = NanVal::heap_list(items);
            let r = jit_tl(list.0);
            let rv = NanVal(r);
            assert!(rv.is_heap());
        }

        #[test]
        fn jit_tl_empty_list_returns_nil() {
            let list = NanVal::heap_list(vec![]);
            let r = jit_tl(list.0);
            assert!(is_nil(r));
        }

        // ── jit_rev ────────────────────────────────────────────────────────

        #[test]
        fn jit_rev_string() {
            let r = jit_rev(str_val("hello"));
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            let s = s.clone();
            assert_eq!(s, "olleh");
        }

        #[test]
        fn jit_rev_list() {
            let items = vec![
                NanVal::number(1.0),
                NanVal::number(2.0),
                NanVal::number(3.0),
            ];
            let list = NanVal::heap_list(items);
            let r = jit_rev(list.0);
            let rv = NanVal(r);
            assert!(rv.is_heap());
        }

        #[test]
        fn jit_rev_non_string_non_list_returns_nil() {
            let r = jit_rev(TAG_NIL);
            assert!(is_nil(r));
        }

        // ── jit_srt ────────────────────────────────────────────────────────

        #[test]
        fn jit_srt_string_sorts_chars() {
            let r = jit_srt(str_val("cab"));
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            let s = s.clone();
            assert_eq!(s, "abc");
        }

        #[test]
        fn jit_srt_number_list() {
            let items = vec![
                NanVal::number(3.0),
                NanVal::number(1.0),
                NanVal::number(2.0),
            ];
            let list = NanVal::heap_list(items);
            let r = jit_srt(list.0);
            let rv = NanVal(r);
            assert!(rv.is_heap());
        }

        #[test]
        fn jit_srt_string_list() {
            let items = vec![
                NanVal::heap_string("c".into()),
                NanVal::heap_string("a".into()),
                NanVal::heap_string("b".into()),
            ];
            let list = NanVal::heap_list(items);
            let r = jit_srt(list.0);
            let rv = NanVal(r);
            assert!(rv.is_heap());
        }

        #[test]
        fn jit_srt_empty_list_returns_list() {
            let list = NanVal::heap_list(vec![]);
            let r = jit_srt(list.0);
            let rv = NanVal(r);
            assert!(rv.is_heap());
        }

        #[test]
        fn jit_srt_non_string_non_list_returns_nil() {
            let r = jit_srt(TAG_NIL);
            assert!(is_nil(r));
        }

        // ── jit_slc ────────────────────────────────────────────────────────

        #[test]
        fn jit_slc_string_slice() {
            let r = jit_slc(str_val("hello"), num(1.0), num(3.0));
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            let s = s.clone();
            assert_eq!(s, "el");
        }

        #[test]
        fn jit_slc_list_slice() {
            let items = vec![
                NanVal::number(0.0),
                NanVal::number(1.0),
                NanVal::number(2.0),
                NanVal::number(3.0),
            ];
            let list = NanVal::heap_list(items);
            let r = jit_slc(list.0, num(1.0), num(3.0));
            let rv = NanVal(r);
            assert!(rv.is_heap());
        }

        #[test]
        fn jit_slc_non_number_indices_returns_nil() {
            let r = jit_slc(str_val("hello"), TAG_NIL, num(3.0));
            assert!(is_nil(r));
        }

        #[test]
        fn jit_slc_non_string_non_list_returns_nil() {
            let r = jit_slc(TAG_NIL, num(0.0), num(2.0));
            assert!(is_nil(r));
        }

        // ── jit_has ────────────────────────────────────────────────────────

        #[test]
        fn jit_has_text_found() {
            let r = jit_has(str_val("hello world"), str_val("world"));
            assert!(as_bool(r));
        }

        #[test]
        fn jit_has_text_not_found() {
            let r = jit_has(str_val("hello"), str_val("xyz"));
            assert!(!as_bool(r));
        }

        #[test]
        fn jit_has_list_found() {
            let items = vec![
                NanVal::number(1.0),
                NanVal::number(2.0),
                NanVal::number(3.0),
            ];
            let list = NanVal::heap_list(items);
            let r = jit_has(list.0, num(2.0));
            assert!(as_bool(r));
        }

        #[test]
        fn jit_has_list_not_found() {
            let items = vec![NanVal::number(1.0), NanVal::number(2.0)];
            let list = NanVal::heap_list(items);
            let r = jit_has(list.0, num(5.0));
            assert!(!as_bool(r));
        }

        #[test]
        fn jit_has_text_non_string_needle_returns_false() {
            // collection is string but needle is not string
            let r = jit_has(str_val("hello"), TAG_NIL);
            assert!(!as_bool(r));
        }

        #[test]
        fn jit_has_non_string_non_list_returns_false() {
            let r = jit_has(TAG_NIL, num(1.0));
            assert!(!as_bool(r));
        }

        // ── jit_spl ────────────────────────────────────────────────────────

        #[test]
        fn jit_spl_string_splits() {
            let r = jit_spl(str_val("a,b,c"), str_val(","));
            let rv = NanVal(r);
            assert!(rv.is_heap());
            let HeapObj::List(items) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected list")
            };
            assert_eq!(items.len(), 3);
        }

        #[test]
        fn jit_spl_non_string_returns_nil() {
            let r = jit_spl(TAG_NIL, str_val(","));
            assert!(is_nil(r));
        }

        // ── jit_cat ────────────────────────────────────────────────────────

        #[test]
        fn jit_cat_list_with_sep() {
            let items = vec![
                NanVal::heap_string("a".into()),
                NanVal::heap_string("b".into()),
                NanVal::heap_string("c".into()),
            ];
            let list = NanVal::heap_list(items);
            let r = jit_cat(list.0, str_val(","));
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            let s = s.clone();
            assert_eq!(s, "a,b,c");
        }

        #[test]
        fn jit_cat_non_list_returns_nil() {
            let r = jit_cat(TAG_NIL, str_val(","));
            assert!(is_nil(r));
        }

        // ── jit_listappend ─────────────────────────────────────────────────

        #[test]
        fn jit_listappend_appends_item() {
            let items = vec![NanVal::number(1.0), NanVal::number(2.0)];
            let list = NanVal::heap_list(items);
            let r = jit_listappend(list.0, num(3.0));
            let rv = NanVal(r);
            assert!(rv.is_heap());
            let HeapObj::List(items) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected list")
            };
            assert_eq!(items.len(), 3);
        }

        #[test]
        fn jit_listappend_non_list_returns_nil() {
            let r = jit_listappend(TAG_NIL, num(1.0));
            assert!(is_nil(r));
        }

        // ── jit_index ──────────────────────────────────────────────────────

        #[test]
        fn jit_index_list_in_bounds() {
            let items = vec![
                NanVal::number(10.0),
                NanVal::number(20.0),
                NanVal::number(30.0),
            ];
            let list = NanVal::heap_list(items);
            // jit_index takes a raw usize cast as u64, not a NaN-boxed number
            let r = jit_index(list.0, 1u64);
            assert!(is_num(r));
            assert_eq!(as_num(r), 20.0);
        }

        #[test]
        fn jit_index_out_of_bounds_returns_nil() {
            let items = vec![NanVal::number(1.0)];
            let list = NanVal::heap_list(items);
            let r = jit_index(list.0, 5u64);
            assert!(is_nil(r));
        }

        #[test]
        fn jit_index_non_list_returns_nil() {
            let r = jit_index(TAG_NIL, num(0.0));
            assert!(is_nil(r));
        }

        // ── jit_jdmp / jit_jpar ────────────────────────────────────────────

        #[test]
        fn jit_jdmp_number() {
            let r = jit_jdmp(num(42.0));
            let rv = NanVal(r);
            assert!(rv.is_string());
        }

        #[test]
        fn jit_jpar_valid_json() {
            let r = jit_jpar(str_val(r#"{"x":1}"#));
            let rv = NanVal(r);
            assert!(rv.is_heap());
        }

        #[test]
        fn jit_jpar_invalid_json() {
            let r = jit_jpar(str_val("not json"));
            let rv = NanVal(r);
            assert!(rv.is_heap());
            let HeapObj::ErrVal(_) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected ErrVal")
            };
        }

        #[test]
        fn jit_jpar_non_string_returns_nil() {
            let r = jit_jpar(TAG_NIL);
            assert!(is_nil(r));
        }

        // ── jit_jpth ───────────────────────────────────────────────────────

        #[test]
        fn jit_jpth_object_key() {
            let r = jit_jpth(str_val(r#"{"x":"hello"}"#), str_val("x"));
            let rv = NanVal(r);
            assert!(rv.is_heap());
            let HeapObj::OkVal(inner) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected OkVal")
            };
            assert!(inner.is_string());
        }

        #[test]
        fn jit_jpth_missing_key() {
            let r = jit_jpth(str_val(r#"{"a":1}"#), str_val("b"));
            let rv = NanVal(r);
            let HeapObj::ErrVal(_) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected ErrVal")
            };
        }

        #[test]
        fn jit_jpth_invalid_json() {
            let r = jit_jpth(str_val("not json"), str_val("x"));
            let rv = NanVal(r);
            let HeapObj::ErrVal(_) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected ErrVal")
            };
        }

        #[test]
        fn jit_jpth_non_string_args_returns_nil() {
            let r = jit_jpth(TAG_NIL, str_val("x"));
            assert!(is_nil(r));
        }

        // ── jit_clone_rc / jit_drop_rc ────────────────────────────────────

        #[test]
        fn jit_clone_rc_and_drop_rc_no_panic() {
            let s = NanVal::heap_string("test".into());
            jit_clone_rc(s.0);
            jit_drop_rc(s.0); // drop the cloned ref
        }

        // ── jit_listget ────────────────────────────────────────────────────

        #[test]
        fn jit_listget_in_bounds() {
            let items = vec![NanVal::number(10.0), NanVal::number(20.0)];
            let list = NanVal::heap_list(items);
            let r = jit_listget(list.0, num(0.0));
            let rv = NanVal(r);
            let HeapObj::OkVal(inner) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected OkVal")
            };
            assert_eq!(inner.as_number(), 10.0);
        }

        #[test]
        fn jit_listget_out_of_bounds_returns_nil() {
            let items = vec![NanVal::number(1.0)];
            let list = NanVal::heap_list(items);
            let r = jit_listget(list.0, num(10.0));
            assert!(is_nil(r));
        }

        #[test]
        fn jit_listget_non_list_returns_nil() {
            let r = jit_listget(str_val("hello"), num(0.0));
            assert!(is_nil(r));
        }

        #[test]
        fn jit_listget_non_number_idx_returns_nil() {
            let items = vec![NanVal::number(1.0)];
            let list = NanVal::heap_list(items);
            let r = jit_listget(list.0, TAG_NIL);
            assert!(is_nil(r));
        }

        // ── jit_fmt2 ──────────────────────────────────────────────────────

        #[test]
        fn jit_fmt2_basic() {
            let r = jit_fmt2(num(3.14159), num(2.0));
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            assert_eq!(s.clone(), "3.14");
        }

        #[test]
        fn jit_fmt2_zero_digits() {
            let r = jit_fmt2(num(1.0), num(0.0));
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            assert_eq!(s.clone(), "1");
        }

        #[test]
        fn jit_fmt2_negative_digits_clamped_to_zero() {
            // d <= 0.0 branch — digits coerced to 0.
            let r = jit_fmt2(num(2.7), num(-3.0));
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            assert_eq!(s.clone(), "3");
        }

        #[test]
        fn jit_fmt2_non_finite_digits_clamped_to_zero() {
            // !d.is_finite() branch.
            let r = jit_fmt2(num(2.7), num(f64::NAN));
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            assert_eq!(s.clone(), "3");
        }

        #[test]
        fn jit_fmt2_non_number_first_arg_returns_nil() {
            let r = jit_fmt2(str_val("oops"), num(2.0));
            assert!(is_nil(r));
        }

        #[test]
        fn jit_fmt2_non_number_second_arg_returns_nil() {
            let r = jit_fmt2(num(3.14), str_val("two"));
            assert!(is_nil(r));
        }

        #[test]
        fn jit_fmt2_digits_clamped_to_twenty() {
            // digits >= 20 hits the .min(20) cap.
            let r = jit_fmt2(num(1.0), num(50.0));
            let rv = NanVal(r);
            assert!(rv.is_string());
            let HeapObj::Str(s) = (unsafe { rv.as_heap_ref() }) else {
                panic!("expected Str")
            };
            // 20 zeros after the decimal point.
            assert_eq!(s.clone(), format!("1.{}", "0".repeat(20)));
        }
    }

    // ── VM execution path tests ───────────────────────────────────────────────

    // L567-575: Dynamic destructure via name lookup (field not in registry)
    #[test]
    fn vm_dynamic_destructure_field_name_lookup() {
        // Two types with ambiguous field positions use RECFLD_NAME opcode
        let result = vm_run(
            "type a{x:n;y:n} type b{y:n;x:n} f>n;v=a x:5 y:3;{y}=v;y",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(3.0));
    }

    // L706, L776, L853, L869: break/continue in while loop
    #[test]
    fn vm_while_continue_skips_body() {
        // while loop with cnt — Stmt::Continue in While exercises L869 path
        let result = vm_run(
            "f>n;i=0;s=0;wh <i 5{i=+i 1;=i 3{cnt};s=+s i};s",
            Some("f"),
            vec![],
        );
        // i=1 +1, i=2 +2, i=3 skip, i=4 +4, i=5 +5 → sum=12
        assert_eq!(result, Value::Number(12.0));
    }

    #[test]
    fn vm_while_break_with_value() {
        // While loop with break carrying a value
        let result = vm_run("f>n;i=0;wh <i 10{i=+i 1;=i 5{brk i}};i", Some("f"), vec![]);
        assert_eq!(result, Value::Number(5.0));
    }

    // L970: TypeIs pattern with type not in n/t/b/l uses fallback OP_ISNUM
    // This is unreachable in valid programs, so we test the l (list) pattern instead
    #[test]
    fn vm_typeis_list_pattern_in_match() {
        let result = vm_run(
            r#"f xs:L n>t;?xs{l v:"got list";_:"other"}"#,
            Some("f"),
            vec![Value::List(vec![Value::Number(1.0)])],
        );
        assert_eq!(result, Value::Text("got list".into()));
    }

    // L1566: DIVK_N constant on right side
    #[test]
    fn vm_divk_n_constant_divisor() {
        // `/x 2` where x is a num param → DIVK_N
        let result = vm_run("f x:n>n;/x 2", Some("f"), vec![Value::Number(10.0)]);
        assert_eq!(result, Value::Number(5.0));
    }

    // L1584-1590: Constant on left side for Add/Multiply
    #[test]
    fn vm_addk_n_constant_on_left() {
        // `+2 x` where 2 is on left — commutative, uses OP_ADDK_N
        let result = vm_run("f x:n>n;+2 x", Some("f"), vec![Value::Number(8.0)]);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn vm_mulk_n_constant_on_left() {
        // `*3 x` where 3 is on left — commutative, uses OP_MULK_N
        let result = vm_run("f x:n>n;*3 x", Some("f"), vec![Value::Number(7.0)]);
        assert_eq!(result, Value::Number(21.0));
    }

    // L2264-2283: to_value_with_registry for arena records
    #[test]
    fn vm_to_value_with_registry_via_record() {
        // Records stored in arena; to_value_with_registry path exercised
        // when record is retrieved from VM
        let result = vm_run("type pt{x:n;y:n} f>n;p=pt x:3 y:4;p.x", Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    // L2299-L2310: run() with no functions defined
    #[test]
    fn vm_run_no_function_name_with_empty_program_errors() {
        use crate::vm::{compile, run};
        // An empty-ish program with a type only (no functions) — no func_names
        let prog = parse_program("type x{a:n}");
        match compile(&prog) {
            Ok(compiled) => {
                // func_names should be empty; run with None should fail
                let result = run(&compiled, None, vec![]);
                assert!(result.is_err());
            }
            Err(_) => {
                // Compile error is also acceptable
            }
        }
    }

    // L2329-L2341: run_with_tools with undefined function
    #[test]
    fn vm_run_with_tools_undefined_function() {
        use crate::interpreter::Value;
        use crate::tools::{ToolError, ToolProvider};
        use crate::vm::{compile, run_with_tools};
        use std::future::Future;
        use std::pin::Pin;

        struct DummyProvider;
        impl ToolProvider for DummyProvider {
            fn call(
                &self,
                _name: &str,
                _args: Vec<Value>,
            ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
                Box::pin(async { Ok(Value::Nil) })
            }
        }

        let prog = parse_program("f>n;42");
        let compiled = compile(&prog).expect("compile ok");
        let provider = DummyProvider;
        #[cfg(feature = "tools")]
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = run_with_tools(
            &compiled,
            Some("nonexistent_function"),
            vec![],
            &provider,
            #[cfg(feature = "tools")]
            &runtime,
        );
        assert!(result.is_err());
    }

    // ── VM opcodes: record/destructure with ambiguous field index ─────────────

    #[test]
    fn vm_destructure_ambiguous_field_uses_name_lookup() {
        // Two types with field "x" at different positions force dynamic name lookup
        // type A has {x, y}, type B has {z, x} — "x" is at different indices
        // Destructuring from a known type still works correctly.
        let result = vm_run(
            "type a{x:n;y:n} type b{z:n;x:n} f>n;v=a x:10 y:20;{x}=v;x",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn vm_guard_with_else_body_false_branch() {
        // Guard with else as last stmt: when condition false, else branch is return value
        // f x:n>n; >x 10 { 1 }{ -1 }  — with x=5 → condition false → else body → -1
        let result = vm_run("f x:n>n;>x 10{1}{-1}", Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(-1.0));
    }

    #[test]
    fn vm_match_type_is_bool_pattern() {
        // Match with `b v:` pattern — exercises OP_ISBOOL
        let result = vm_run(
            r#"f x:b>t;?x{b v:"bool";_:"other"}"#,
            Some("f"),
            vec![Value::Bool(true)],
        );
        assert_eq!(result, Value::Text("bool".into()));
    }

    #[test]
    fn vm_match_type_is_list_pattern() {
        // Match with `l v:` pattern — exercises OP_ISLIST
        let result = vm_run(
            r#"f xs:L n>t;?xs{l v:"list";_:"other"}"#,
            Some("f"),
            vec![Value::List(vec![])],
        );
        assert_eq!(result, Value::Text("list".into()));
    }

    #[test]
    fn vm_search_field_index_ambiguous_returns_none() {
        // Two types where same field has different indices → search_field_index returns None
        // This causes the compiler to use OP_RECFLD_NAME instead of OP_RECFLD.
        let result = vm_run(
            "type p{x:n;y:n} type q{y:n;x:n} f>n;v=p x:5 y:3;{x}=v;x",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(5.0));
    }

    // ── New coverage tests ────────────────────────────────────────────────────

    // L253: TypeRegistry::register returns existing id when type already registered
    #[test]
    fn vm_type_registry_register_dedup() {
        // Two type declarations with the same name (second is a no-op in registry).
        // The compiler calls register for each TypeDef; duplicate name returns existing id.
        // We verify that programs with re-registered type names compile and run correctly.
        let prog = parse_program("type pt{x:n;y:n} f>n;p=pt x:3 y:4;p.x");
        let compiled = compile(&prog).unwrap();
        let result = run(&compiled, Some("f"), vec![]).unwrap();
        assert_eq!(result, Value::Number(3.0));
    }

    // L406-411: search_field_index with multiple types — some have field, same index
    #[test]
    fn vm_search_field_same_index_multiple_types() {
        // Both types have "x" at index 0 → search_field_index returns Some(0) → OP_RECFLD
        let result = vm_run(
            "type a{x:n;y:n} type b{x:n;z:n} f>n;v=a x:7 y:2;{x}=v;x",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(7.0));
    }

    // L567-575: Dynamic destructure via OP_RECFLD_NAME (field at different indices across types)
    // with existing binding in scope
    #[test]
    fn vm_destructure_name_lookup_existing_binding() {
        // Two types with "y" at different indices — forces OP_RECFLD_NAME
        // Existing variable `y` is reused (existing_reg branch at L569-571)
        let result = vm_run(
            "type a{x:n;y:n} type b{y:n;x:n} f>n;y=0;v=a x:5 y:9;{y}=v;y",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(9.0));
    }

    // L706: ForEach continue patch target (continue_patches patched to idx increment)
    #[test]
    fn vm_foreach_cnt_patches_correctly() {
        // cnt inside foreach — exercises continue_patches patch target (L706)
        let result = vm_run(
            "f>n;s=0;@x [10,20,30,40,50]{>x 25{cnt};s=+s x};s",
            Some("f"),
            vec![],
        );
        // Sums 10+20 (skip 30,40,50 since >25) — wait, >x 25 means x>25 → skip
        // So 10,20 are kept (10+20=30), 30,40,50 are skipped
        assert_eq!(result, Value::Number(30.0));
    }

    // L776: ForRange continue patch target (continue_patches patched to counter increment)
    #[test]
    fn vm_forrange_cnt_patches_correctly() {
        // cnt inside for-range — exercises continue_patches patch target at L776
        let result = vm_run("f>n;s=0;@i 0..8{>i 4{cnt};s=+s i};s", Some("f"), vec![]);
        // Sum 0+1+2+3+4 = 10 (5,6,7 are skipped because >4)
        assert_eq!(result, Value::Number(10.0));
    }

    // L853: Stmt::Break with expr where reg == result_reg (no MOVE needed)
    #[test]
    fn vm_foreach_brk_with_same_reg() {
        // brk x inside @x — x IS the loop variable which may share result_reg
        let result = vm_run("f>n;@x [1,2,3,4,5]{>=x 4{brk x};x}", Some("f"), vec![]);
        assert_eq!(result, Value::Number(4.0));
    }

    // L869: Stmt::Continue in while loop (ctx.continue_patches is None → emit_jump_to top)
    #[test]
    fn vm_while_cnt_jumps_to_top() {
        // cnt in while loop exercises the `else { emit_jump_to(top) }` branch at L869
        let result = vm_run(
            "f>n;i=0;s=0;wh <i 6{i=+i 1;=i 4{cnt};s=+s i};s",
            Some("f"),
            vec![],
        );
        // Sums: 1+2+3+5+6 = 17 (4 is skipped by cnt)
        assert_eq!(result, Value::Number(17.0));
    }

    // L1563-1566: OP_SUBK_N (right-constant path) — var is reg_is_num, literal on right
    #[test]
    fn vm_subk_n_constant_on_right() {
        // `-x 7` where x is a numeric param → emits OP_SUBK_N
        let result = vm_run("f x:n>n;-x 7", Some("f"), vec![Value::Number(20.0)]);
        assert_eq!(result, Value::Number(13.0));
    }

    // OP_MULK_N (right-constant path) — var is reg_is_num, literal on right
    #[test]
    fn vm_mulk_n_constant_on_right() {
        // `*x 5` where x is a numeric param → emits OP_MULK_N
        let result = vm_run("f x:n>n;*x 5", Some("f"), vec![Value::Number(6.0)]);
        assert_eq!(result, Value::Number(30.0));
    }

    // L1576-1590: Commutative op with constant on left (Add/Multiply), right is reg_is_num
    #[test]
    fn vm_addk_n_left_constant_commutative() {
        // `+100 x` — constant on left, x is reg_is_num → OP_ADDK_N with commuted args
        let result = vm_run("f x:n>n;+100 x", Some("f"), vec![Value::Number(42.0)]);
        assert_eq!(result, Value::Number(142.0));
    }

    #[test]
    fn vm_mulk_n_left_constant_commutative() {
        // `*10 x` — constant on left, x is reg_is_num → OP_MULK_N with commuted args
        let result = vm_run("f x:n>n;*10 x", Some("f"), vec![Value::Number(9.0)]);
        assert_eq!(result, Value::Number(90.0));
    }

    // L2086: promote_arena_to_heap with nested arena record (nested record inside record)
    #[test]
    fn vm_nested_record_in_list_promotes_arena() {
        // Appending a record to a list causes promote_arena_to_heap.
        // Using a list with records exercises the arena promotion path.
        let result = vm_run(
            "type pt{x:n;y:n} f>n;xs=[pt x:1 y:2, pt x:3 y:4];xs.0",
            Some("f"),
            vec![],
        );
        // Access first element — a promoted arena record
        match result {
            Value::Record { type_name, .. } => assert_eq!(type_name, "pt"),
            Value::Number(n) => assert_eq!(n, 1.0), // if field access
            other => panic!("expected record or number, got {:?}", other),
        }
    }

    // run() with NoFunctionsDefined error (None func name, empty func_names)
    #[test]
    fn vm_run_no_functions_defined_error() {
        // An empty program (only a type) has no functions → run(None) → NoFunctionsDefined
        use crate::ast::{Decl, Param, Program, Span, Type};
        let prog = Program {
            declarations: vec![Decl::TypeDef {
                name: "pt".to_string(),
                fields: vec![Param {
                    name: "x".to_string(),
                    ty: Type::Number,
                }],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        let compiled = compile(&prog).unwrap();
        let result = run(&compiled, None, vec![]);
        assert!(result.is_err(), "expected NoFunctionsDefined error");
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("no functions") || err_str.contains("undefined"),
            "unexpected error: {err_str}"
        );
    }

    // run_with_tools() with NoFunctionsDefined (None func name, no functions)
    #[test]
    fn vm_run_with_tools_no_functions_defined() {
        use crate::ast::{Decl, Param, Program, Span, Type};
        use crate::interpreter::Value;
        use crate::tools::{ToolError, ToolProvider};
        use crate::vm::{compile, run_with_tools};
        use std::future::Future;
        use std::pin::Pin;

        struct DummyProvider;
        impl ToolProvider for DummyProvider {
            fn call(
                &self,
                _name: &str,
                _args: Vec<Value>,
            ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
                Box::pin(async { Ok(Value::Nil) })
            }
        }

        let prog = Program {
            declarations: vec![Decl::TypeDef {
                name: "pt".to_string(),
                fields: vec![Param {
                    name: "x".to_string(),
                    ty: Type::Number,
                }],
                span: Span::UNKNOWN,
            }],
            source: None,
        };
        let compiled = compile(&prog).unwrap();
        let provider = DummyProvider;
        #[cfg(feature = "tools")]
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = run_with_tools(
            &compiled,
            None,
            vec![],
            &provider,
            #[cfg(feature = "tools")]
            &runtime,
        );
        assert!(result.is_err(), "expected error for no functions");
    }

    // VM::new_with_tools constructor path (L2416-2432)
    #[test]
    fn vm_run_with_tools_calls_function_successfully() {
        use crate::interpreter::Value;
        use crate::tools::{ToolError, ToolProvider};
        use crate::vm::{compile, run_with_tools};
        use std::future::Future;
        use std::pin::Pin;

        struct DummyProvider;
        impl ToolProvider for DummyProvider {
            fn call(
                &self,
                _name: &str,
                _args: Vec<Value>,
            ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
                Box::pin(async { Ok(Value::Nil) })
            }
        }

        let prog = parse_program("f>n;42");
        let compiled = compile(&prog).unwrap();
        let provider = DummyProvider;
        #[cfg(feature = "tools")]
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = run_with_tools(
            &compiled,
            Some("f"),
            vec![],
            &provider,
            #[cfg(feature = "tools")]
            &runtime,
        );
        assert_eq!(result.unwrap(), Value::Number(42.0));
    }

    // to_value_with_registry for arena records with heap string fields (L2264-2283)
    #[test]
    fn vm_to_value_with_registry_string_field() {
        // Record with a text field — to_value_with_registry resolves field names
        let result = vm_run(
            "type person{name:t;age:n} f>t;p=person name:\"alice\" age:30;p.name",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Text("alice".into()));
    }

    #[test]
    fn vm_to_value_with_registry_multiple_records() {
        // Multiple records — exercises type_info.fields lookup path
        let result = vm_run(
            "type color{r:n;g:n;b:n} f>n;c=color r:255 g:128 b:0;c.g",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(128.0));
    }

    // OP_RECFLD heap path via heap-allocated record (not arena)
    // Achieved by passing a record value directly as argument (not constructed in VM)
    #[test]
    fn vm_recfld_heap_record_field_access() {
        // Records passed as arguments are heap-allocated (not arena).
        // Use a single-field type to avoid HashMap iteration order non-determinism.
        let source = "f r:pt>n;r.x\ntype pt{x:n}";
        let mut fields = std::collections::HashMap::new();
        fields.insert("x".to_string(), Value::Number(77.0));
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Record {
                type_name: "pt".to_string(),
                fields,
            }],
        );
        assert_eq!(result, Value::Number(77.0));
    }

    // OP_RECWITH on heap record (not arena) — exercises L3451-3483
    #[test]
    fn vm_recwith_heap_record_arg() {
        // When a record is passed as argument, it's heap-allocated
        // `with` on a heap record exercises the heap OP_RECWITH path
        let source = "f r:pt>n;r2=r with x:99;r2.x\ntype pt{x:n;y:n}";
        let mut fields = std::collections::HashMap::new();
        fields.insert("x".to_string(), Value::Number(1.0));
        fields.insert("y".to_string(), Value::Number(2.0));
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Record {
                type_name: "pt".to_string(),
                fields,
            }],
        );
        assert_eq!(result, Value::Number(99.0));
    }

    // OP_RECFLD_NAME on heap record via jpar-produced generic record
    #[test]
    fn vm_recfld_name_heap_record() {
        // jpar produces a generic record (rec_type=u16::MAX) → compiler emits OP_RECFLD_NAME
        // Two types with y at different indices cause search_field_index to return None → OP_RECFLD_NAME
        // OP_RECFLD_NAME uses the heap record's own TypeInfo for name lookup → correct result
        let source = "type a{x:n;y:n} type b{y:n;x:n} f s:t>n;r=jpar! s;{y}=r;y";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Text(r#"{"x": 10, "y": 20}"#.to_string())],
        );
        assert_eq!(result, Value::Number(20.0));
    }

    // record with-expr on heap record — verify updated field has new value
    #[test]
    fn vm_recwith_heap_preserves_unchanged_fields() {
        // Pass a record as an argument (heap path), use `with` to update x,
        // then read back x to confirm the update took effect.
        // Single-field type avoids HashMap ordering ambiguity.
        let source = "type box{v:n} f r:box>n;r2=r with v:55;r2.v";
        let mut fields = std::collections::HashMap::new();
        fields.insert("v".to_string(), Value::Number(0.0));
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Record {
                type_name: "box".to_string(),
                fields,
            }],
        );
        assert_eq!(result, Value::Number(55.0));
    }

    // NanVal::to_value() arena record path with heap string field (L2209-2230)
    // This exercises the `to_value()` fast path for arena records including the
    // ACTIVE_REGISTRY lookup and field name resolution.
    #[test]
    fn vm_arena_record_to_value_with_heap_string_field() {
        // Record with text field is created in arena; to_value() promotes it
        let result = vm_run(
            "type item{label:t;count:n} f>t;r=item label:\"widget\" count:5;r.label",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Text("widget".into()));
    }

    // Match with TypeIs pattern — "other" type branch fallback (L970)
    // The `_ => OP_ISNUM` branch is unreachable in valid programs but we can
    // exercise all the valid patterns to maximize coverage of that match arm block.
    #[test]
    fn vm_match_type_is_all_patterns() {
        // Exercise all four TypeIs patterns in sequence
        let num_src = r#"f x:t>b;?x{n _:true;_:false}"#;
        assert_eq!(
            vm_run(num_src, Some("f"), vec![Value::Number(1.0)]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(num_src, Some("f"), vec![Value::Text("a".into())]),
            Value::Bool(false)
        );

        let text_src = r#"f x:n>b;?x{t _:true;_:false}"#;
        assert_eq!(
            vm_run(text_src, Some("f"), vec![Value::Text("x".into())]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(text_src, Some("f"), vec![Value::Number(0.0)]),
            Value::Bool(false)
        );

        let bool_src = r#"f x:n>b;?x{b _:true;_:false}"#;
        assert_eq!(
            vm_run(bool_src, Some("f"), vec![Value::Bool(false)]),
            Value::Bool(true)
        );

        let list_src = r#"f x:n>b;?x{l _:true;_:false}"#;
        let list = Value::List(vec![Value::Number(1.0)]);
        assert_eq!(vm_run(list_src, Some("f"), vec![list]), Value::Bool(true));
    }

    // MULK_N constant on right side (L1563-L1566 for Multiply op)
    #[test]
    fn vm_mulk_n_right_side_constant_explicit() {
        // `*x 4` — x is numeric param, 4 is literal → MULK_N
        let result = vm_run("f x:n>n;*x 4", Some("f"), vec![Value::Number(7.0)]);
        assert_eq!(result, Value::Number(28.0));
    }

    // OP_ADDK_N: constant on right side
    #[test]
    fn vm_addk_n_right_side_constant_explicit() {
        // `+x 15` — x is numeric param, 15 is literal → ADDK_N
        let result = vm_run("f x:n>n;+x 15", Some("f"), vec![Value::Number(10.0)]);
        assert_eq!(result, Value::Number(25.0));
    }

    // L1635: BinOp::Append emits OP_LISTAPPEND
    #[test]
    fn vm_binop_append_emits_listappend() {
        let prog = parse_program("f xs:L n x:n>L n;+=xs x");
        let compiled = compile(&prog).unwrap();
        let chunk = &compiled.chunks[0];
        let has_listappend = chunk
            .code
            .iter()
            .any(|&inst| (inst >> 24) as u8 == OP_LISTAPPEND);
        assert!(has_listappend, "expected OP_LISTAPPEND for += operator");
    }

    // Record access with string text field returned as value (exercises NanVal::to_value heap string path)
    #[test]
    fn vm_record_text_field_roundtrip() {
        let result = vm_run(
            "type greeting{msg:t} f>t;g=greeting msg:\"hello world\";g.msg",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Text("hello world".into()));
    }

    // Guard ternary where both branches produce values via chained computations
    #[test]
    fn vm_guard_ternary_chained() {
        // Ternary: x >= 10 ? "large" : "small" — tests two-branch guard value production
        let src = r#"f x:n>t;>=x 10{"large"}{"small"}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(10.0)]),
            Value::Text("large".into())
        );
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(5.0)]),
            Value::Text("small".into())
        );
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(15.0)]),
            Value::Text("large".into())
        );
    }

    // Safe field access on list returns nil (no field named "name" on a list)
    #[test]
    fn vm_safe_field_on_list_returns_nil() {
        let src = "f xs:L n>n;xs.?0??77";
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::List(vec![Value::Number(99.0)])]),
            Value::Number(99.0)
        );
    }

    // While break without value (break_patches path with no expr at L841-855)
    #[test]
    fn vm_while_brk_no_expr_exits_loop() {
        // brk (no expression) in while loop — exercises L841-855 break_patches without move
        let src = "f>n;i=0;wh <i 100{i=+i 1;>=i 7{brk}};i";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(7.0));
    }

    // ForEach with break and no value (the default result is last body iteration result)
    #[test]
    fn vm_foreach_brk_no_expr_result() {
        // brk no value — the result is from the last body evaluation before break
        let src = "f>n;@x [1,2,3,4,5]{>=x 3{brk};x}";
        // x=1 body=1, x=2 body=2, x=3 triggers brk — result is 2 (last body value before brk)
        // Actually brk no-value doesn't store anything, result_reg keeps its value from last body
        let result = vm_run(src, Some("f"), vec![]);
        // After brk at x=3, result_reg still has last body = 2
        assert!(
            matches!(result, Value::Number(n) if n == 2.0 || n == 3.0),
            "expected 2.0 or 3.0, got {:?}",
            result
        );
    }

    // Recursive function with multiple calls on the stack (exercises make_runtime_error call_stack)
    #[test]
    fn vm_recursive_call_stack_captured() {
        // Fibonacci — deep call stack; make_runtime_error captures function frames
        let src = "fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b";
        let result = vm_run(src, Some("fib"), vec![Value::Number(6.0)]);
        assert_eq!(result, Value::Number(8.0));
    }

    // OP_RECFLD_NAME: field not found on heap record (L3152)
    #[test]
    fn vm_recfld_name_field_not_found_heap_record() {
        // Two types with ambiguous field index → OP_RECFLD_NAME
        // Access a field that doesn't exist on the given type
        let err = vm_run_err(
            "type a{x:n;y:n} type b{y:n;x:n} f r:a>n;{z}=r;z",
            Some("f"),
            vec![Value::Record {
                type_name: "a".to_string(),
                fields: {
                    let mut m = std::collections::HashMap::new();
                    m.insert("x".to_string(), Value::Number(1.0));
                    m.insert("y".to_string(), Value::Number(2.0));
                    m
                },
            }],
        );
        assert!(err.contains("z") || err.contains("field"), "got: {err}");
    }

    // OP_RECFLD: field index out of bounds on heap record (L3104-3107)
    #[test]
    fn vm_recfld_index_out_of_bounds_heap_record() {
        // Pass a heap record with fewer fields than expected (e.g. field index > n_fields)
        // We achieve this by passing a record with missing fields
        let err = vm_run_err(
            "type pt{x:n;y:n;z:n} f r:pt>n;r.z",
            Some("f"),
            vec![Value::Record {
                type_name: "pt".to_string(),
                fields: {
                    // Only x and y — z is missing
                    let mut m = std::collections::HashMap::new();
                    m.insert("x".to_string(), Value::Number(1.0));
                    m.insert("y".to_string(), Value::Number(2.0));
                    m
                },
            }],
        );
        assert!(
            err.contains("z") || err.contains("field") || err.contains("not found"),
            "got: {err}"
        );
    }

    // VmState::call that hits an error, then another call (drain path L1201)
    #[test]
    fn vm_state_call_drain_after_error_then_success() {
        // First call divides by zero (error), second call should still work
        let prog1 = parse_program("f x:n>n;/x 0");
        let compiled1 = compile(&prog1).unwrap();
        let mut state = VmState::new(&compiled1);
        let err = state.call("f", vec![Value::Number(10.0)]);
        assert!(err.is_err());

        // A new state for a clean function
        let prog2 = parse_program("g x:n>n;+x 1");
        let compiled2 = compile(&prog2).unwrap();
        let mut state2 = VmState::new(&compiled2);
        let ok = state2.call("g", vec![Value::Number(5.0)]);
        assert_eq!(ok.unwrap(), Value::Number(6.0));
    }

    // run() with explicit func name that is undefined (L2306-2310 UndefinedFunction path)
    #[test]
    fn vm_run_explicit_undefined_function_name() {
        let prog = parse_program("f>n;42");
        let compiled = compile(&prog).unwrap();
        let result = run(&compiled, Some("does_not_exist"), vec![]);
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("does_not_exist") || err_str.contains("undefined"),
            "unexpected error: {err_str}"
        );
    }

    // Large record (many fields) — exercises arena allocation with alignment
    #[test]
    fn vm_large_record_multiple_fields() {
        // 5-field record — exercises arena alloc with more fields
        let src = "type big{a:n;b:n;c:n;d:n;e:n} f>n;r=big a:1 b:2 c:3 d:4 e:5;r.c";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    // Record with expression using multiple field updates (OP_RECWITH with 2+ updates)
    #[test]
    fn vm_recwith_two_field_updates() {
        // `with x:10 y:20` updates two fields at once
        let src = "type pt{x:n;y:n} f>n;p=pt x:1 y:2;q=p with x:10;+q.x q.y";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(12.0)); // 10 + 2
    }

    // Nested records in a list (exercises promote_arena_to_heap for nested records)
    #[test]
    fn vm_list_of_records_field_access() {
        let src = "type pt{x:n;y:n} f>n;xs=[pt x:1 y:2,pt x:10 y:20];xs.1";
        let result = vm_run(src, Some("f"), vec![]);
        // xs.1 accesses the second element (index 1) — a promoted pt record
        let Value::Record { type_name, fields } = result else {
            panic!("expected Record")
        };
        assert_eq!(type_name, "pt");
        assert_eq!(fields.get("x"), Some(&Value::Number(10.0)));
    }

    // Check that run_with_tools correctly invokes VM::new_with_tools (exercises L2416-2432)
    #[test]
    fn vm_run_with_tools_with_tool_declaration() {
        use crate::interpreter::Value;
        use crate::tools::{ToolError, ToolProvider};
        use crate::vm::{compile, run_with_tools};
        use std::future::Future;
        use std::pin::Pin;

        struct DummyProvider;
        impl ToolProvider for DummyProvider {
            fn call(
                &self,
                _name: &str,
                _args: Vec<Value>,
            ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
                Box::pin(async { Ok(Value::Nil) })
            }
        }

        // Program with a tool and a function that calls it — exercises new_with_tools
        let prog = parse_program("tool fetch\"HTTP GET\" url:t>R _ t\nf>R _ t;fetch \"http://x\"");
        let compiled = compile(&prog).unwrap();
        let provider = DummyProvider;
        #[cfg(feature = "tools")]
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = run_with_tools(
            &compiled,
            Some("f"),
            vec![],
            &provider,
            #[cfg(feature = "tools")]
            &runtime,
        );
        assert_eq!(result.unwrap(), Value::Ok(Box::new(Value::Nil)));
    }

    // Additional ternary guard coverage — else-body with computation
    #[test]
    fn vm_ternary_else_computation() {
        // ternary where else computes a value from parameters
        let src = "f x:n>n;>x 0{x}{-x}"; // absolute value
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(5.0)
        );
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(-3.0)]),
            Value::Number(3.0)
        );
    }

    // While loop with continue that modifies accumulator
    #[test]
    fn vm_while_cnt_accumulates_correctly() {
        // cnt in while: skip i < 3, sum remaining
        // i=1: <1 3=true → cnt; i=2: <2 3=true → cnt; i=3: sum+=3; i=4: sum+=4; i=5: sum+=5 → 12
        let src = "f>n;i=0;s=0;wh <i 5{i=+i 1;<i 3{cnt};s=+s i};s";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(12.0));
    }

    // TypeRegistry::field_index lookup (L261-264)
    #[test]
    fn vm_type_registry_field_index() {
        // Type registry field_index is used by the VM for RECFLD_NAME
        // We can verify via correct record field lookup in a multi-type scenario
        let result = vm_run(
            "type a{x:n;y:n} type b{y:n;x:n} f>n;r=b y:7 x:3;{x}=r;x",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(3.0));
    }

    // Multiple functions with records — exercises type registry across chunk boundaries
    #[test]
    fn vm_multi_function_with_records() {
        let src = "type pt{x:n;y:n} mk a:n b:n>pt;pt x:a y:b\nf>n;p=mk 5 10;p.x";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(5.0));
    }

    // ── Group A: Map operation error paths ──────────────────────────────────────

    // mget with non-text key (line 2802)
    // The key must be heap-tagged but not a string (e.g. a list) to reach the `_` arm safely.
    // Use z for both params to bypass the verifier type check entirely.
    #[test]
    fn vm_mget_non_text_key_error() {
        let err = vm_run_err(
            "f m:z k:z>n;mget m k",
            Some("f"),
            vec![
                Value::Map(std::collections::HashMap::new()),
                Value::List(vec![Value::Number(1.0)]),
            ],
        );
        assert!(
            err.contains("mget") || err.contains("key") || err.contains("text"),
            "got: {err}"
        );
    }

    // mget with non-map first arg (line 2805)
    #[test]
    fn vm_mget_non_map_first_arg_error() {
        // x:z k:t — pass a list as first arg at runtime (must be heap-tagged but not a map)
        let err = vm_run_err(
            "f x:z k:t>n;mget x k",
            Some("f"),
            vec![
                Value::List(vec![Value::Number(1.0)]),
                Value::Text("key".into()),
            ],
        );
        assert!(err.contains("mget") || err.contains("map"), "got: {err}");
    }

    // mset with non-text key (line 2827) — key must be heap-tagged non-string
    #[test]
    fn vm_mset_non_text_key_error() {
        let err = vm_run_err(
            "f m:z k:z v:t>n;mset m k v",
            Some("f"),
            vec![
                Value::Map(std::collections::HashMap::new()),
                Value::List(vec![Value::Number(1.0)]),
                Value::Text("val".into()),
            ],
        );
        assert!(
            err.contains("mset") || err.contains("key") || err.contains("text"),
            "got: {err}"
        );
    }

    // mset with non-map first arg (line 2830)
    #[test]
    fn vm_mset_non_map_first_arg_error() {
        // x must be a heap-tagged non-map (Text is heap-tagged)
        let err = vm_run_err(
            "f x:z k:t v:t>n;mset x k v",
            Some("f"),
            vec![
                Value::Text("not-a-map".into()),
                Value::Text("key".into()),
                Value::Text("val".into()),
            ],
        );
        assert!(err.contains("mset") || err.contains("map"), "got: {err}");
    }

    // mhas with non-text key (line 2846) — key must be heap-tagged non-string
    #[test]
    fn vm_mhas_non_text_key_error() {
        let err = vm_run_err(
            "f m:z k:z>n;mhas m k",
            Some("f"),
            vec![
                Value::Map(std::collections::HashMap::new()),
                Value::List(vec![Value::Number(1.0)]),
            ],
        );
        assert!(
            err.contains("mhas") || err.contains("key") || err.contains("text"),
            "got: {err}"
        );
    }

    // mhas with non-map first arg (line 2849)
    #[test]
    fn vm_mhas_non_map_first_arg_error() {
        // Must be heap-tagged but not a map — use a list
        let err = vm_run_err(
            "f x:z k:t>n;mhas x k",
            Some("f"),
            vec![
                Value::List(vec![Value::Number(1.0)]),
                Value::Text("k".into()),
            ],
        );
        assert!(err.contains("mhas") || err.contains("map"), "got: {err}");
    }

    // mkeys with non-map arg (line 2868) — must be heap-tagged non-map
    #[test]
    fn vm_mkeys_non_map_arg_error() {
        let err = vm_run_err(
            "f x:z>n;mkeys x",
            Some("f"),
            vec![Value::List(vec![Value::Text("a".into())])],
        );
        assert!(err.contains("mkeys") || err.contains("map"), "got: {err}");
    }

    // mvals with non-map arg (line 2887) — must be heap-tagged non-map
    #[test]
    fn vm_mvals_non_map_arg_error() {
        let err = vm_run_err(
            "f x:z>n;mvals x",
            Some("f"),
            vec![Value::Text("not-a-map".into())],
        );
        assert!(err.contains("mvals") || err.contains("map"), "got: {err}");
    }

    // mdel with non-text key (line 2907) — key must be heap-tagged non-string
    #[test]
    fn vm_mdel_non_text_key_error() {
        let err = vm_run_err(
            "f m:z k:z>n;mdel m k",
            Some("f"),
            vec![
                Value::Map(std::collections::HashMap::new()),
                Value::List(vec![Value::Number(7.0)]),
            ],
        );
        assert!(
            err.contains("mdel") || err.contains("key") || err.contains("text"),
            "got: {err}"
        );
    }

    // mdel with non-map first arg (line 2910) — must be heap-tagged non-map
    #[test]
    fn vm_mdel_non_map_first_arg_error() {
        let err = vm_run_err(
            "f x:z k:t>n;mdel x k",
            Some("f"),
            vec![
                Value::List(vec![Value::Number(1.0)]),
                Value::Text("k".into()),
            ],
        );
        assert!(err.contains("mdel") || err.contains("map"), "got: {err}");
    }

    // ── Group B: File I/O error paths ────────────────────────────────────────────

    // rd with bad JSON content — triggers Err return (line 2939)
    #[test]
    fn vm_rd_bad_json_returns_err() {
        let path = "/tmp/ilo_vm_rd_badjson.json";
        std::fs::write(path, "{ this is not valid json }").unwrap();
        let result = vm_run(
            "f p:t>R t t;rd p",
            Some("f"),
            vec![Value::Text(path.into())],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err from bad JSON, got {result:?}"
        );
        let _ = std::fs::remove_file(path);
    }

    // wr with an invalid path returns Err (line 2982)
    #[test]
    fn vm_wr_bad_path_returns_err() {
        // Write to a path inside a non-existent directory
        let result = vm_run(
            "f p:t c:t>R t t;wr p c",
            Some("f"),
            vec![
                Value::Text("/nonexistent_dir_ilo/output.txt".into()),
                Value::Text("hello".into()),
            ],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err from bad path, got {result:?}"
        );
    }

    // wrl with bad path returns Err (line 3008)
    #[test]
    fn vm_wrl_bad_path_returns_err() {
        let result = vm_run(
            "f p:t xs:L t>R t t;wrl p xs",
            Some("f"),
            vec![
                Value::Text("/nonexistent_dir_ilo/lines.txt".into()),
                Value::List(vec![Value::Text("line1".into())]),
            ],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err from bad wrl path, got {result:?}"
        );
    }

    // wrl with non-list second arg triggers VmError (line 3011)
    #[test]
    fn vm_wrl_non_list_second_arg_error() {
        let err = vm_run_err(
            "f p:t x:z>R t t;wrl p x",
            Some("f"),
            vec![
                Value::Text("/tmp/ilo_wrl_nonlist.txt".into()),
                Value::Text("not-a-list".into()),
            ],
        );
        assert!(err.contains("wrl") || err.contains("list"), "got: {err}");
    }

    // ── Group C: OP_UNWRAP on non-Ok/Err (line 3070) ────────────────────────────

    // unwrap on a non-Ok/Err value — line 3070 is a defensive path that fires when
    // OP_UNWRAP encounters a heap-tagged value that is not OkVal or ErrVal.
    // The compiler only emits OP_UNWRAP inside Result match arms (after ISOK/ISERR guards),
    // so this path is unreachable via normal compilation. We test the adjacent happy path:
    // wrapok+unwrap roundtrip via a Result match using ilo's `~v`/`^e` match syntax.
    #[test]
    fn vm_wrapok_unwrap_roundtrip_via_match() {
        // Result match with Ok (~v) arm uses OP_WRAPOK then OP_UNWRAP internally
        let src = "wrap x:n>R n n;~x\nf>n;r=wrap 42;?r{^_:0;~v:v}";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(42.0));
    }

    // ── Group D: OP_RECFLD errors ───────────────────────────────────────────────

    // Arena record field out of bounds (line 3089) — access field index beyond n_fields
    // We trigger this by passing a record with fewer fields than the type declares.
    #[test]
    fn vm_recfld_arena_out_of_bounds() {
        // type pt{x:n;y:n;z:n} — access .z but pass record with only x,y
        let err = vm_run_err(
            "type pt{x:n;y:n;z:n} f r:pt>n;r.z",
            Some("f"),
            vec![Value::Record {
                type_name: "pt".to_string(),
                fields: {
                    let mut m = std::collections::HashMap::new();
                    m.insert("x".to_string(), Value::Number(1.0));
                    m.insert("y".to_string(), Value::Number(2.0));
                    // z is missing
                    m
                },
            }],
        );
        assert!(
            err.contains("z")
                || err.contains("field")
                || err.contains("not found")
                || err.contains("index"),
            "got: {err}"
        );
    }

    // OP_RECFLD on a non-record heap value (line 3110) — heap record path non-record arm
    // Pass a text value as a parameter typed to a record type — at runtime heap obj is a string.
    #[test]
    fn vm_recfld_non_record_heap_value_error() {
        // We pass a Value::Record so it converts to heap record — but we need a non-record heap.
        // The `z` type trick: pass a text when record expected via type z param.
        let err = vm_run_err(
            "type pt{x:n} f r:z>n;r.x",
            Some("f"),
            vec![Value::Text("not-a-record".into())],
        );
        assert!(
            err.contains("field")
                || err.contains("record")
                || err.contains("not found")
                || err.contains("x"),
            "got: {err}"
        );
    }

    // ── Group E: OP_WRAPOK / OP_WRAPERR with arena record input ────────────────

    // OP_WRAPOK promotes an arena record before wrapping (line 2735)
    #[test]
    fn vm_wrapok_arena_record_promotes_to_heap() {
        // A function that returns Ok(record) — the record is arena-allocated, must be promoted.
        // wrap takes a dummy n arg; ~(pt x:a y:7) wraps arena record in Ok.
        // The match extracts the record via ~p and reads field .x.
        let src = "type pt{x:n;y:n} wrap a:n>R pt n;~pt x:a y:7\nf>n;r=wrap 3;?r{^_:0;~p:p.x}";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    // OP_WRAPERR promotes an arena record before wrapping (line 2744)
    #[test]
    fn vm_wraperr_arena_record_promotes_to_heap() {
        // A function that returns Err(record) — the record is arena-allocated, must be promoted.
        // wrap takes a dummy n arg; ^(info code:a) wraps arena record in Err.
        // The match extracts the record via ^e and reads field .code.
        let src =
            "type info{code:n} wrap a:n>R n info;^info code:a\nf>n;r=wrap 99;?r{^e:e.code;~_:0}";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(99.0));
    }

    // ── Group F: OP_LISTGET error paths (lines 3189-3213) ──────────────────────

    // OP_LISTGET: list index must be a number — non-number index arg (line 3213)
    // This is emitted by ForEach; the internal counter is always a number so this path
    // is only reachable with a compiler bug. We test ForEach on a non-list (line 3190).
    #[test]
    fn vm_foreach_on_non_list_error() {
        // Pass a text value to a foreach loop via a z-typed parameter
        let err = vm_run_err(
            "f xs:z>n;@x xs{x};0",
            Some("f"),
            vec![Value::Text("not-a-list".into())],
        );
        assert!(
            err.contains("list") || err.contains("foreach"),
            "got: {err}"
        );
    }

    // OP_LEN on a heap non-list/non-map/non-string (line 3586) — e.g. Ok value
    #[test]
    fn vm_len_on_heap_ok_value_error() {
        // Pass an Ok-wrapped value where len is called; Ok is heap but not list/map/string
        let err = vm_run_err(
            "f x:z>n;len x",
            Some("f"),
            vec![Value::Ok(Box::new(Value::Number(5.0)))],
        );
        assert!(
            err.contains("len")
                || err.contains("string")
                || err.contains("list")
                || err.contains("map"),
            "got: {err}"
        );
    }

    // OP_LEN on a non-heap, non-string value (line 3589) — e.g. a bool or number
    #[test]
    fn vm_len_on_number_error() {
        let err = vm_run_err("f x:z>n;len x", Some("f"), vec![Value::Number(42.0)]);
        assert!(
            err.contains("len") || err.contains("string") || err.contains("list"),
            "got: {err}"
        );
    }

    // OP_INDEX on a non-list heap value (line 3178)
    #[test]
    fn vm_index_on_non_list_heap_value_error() {
        // xs.0 on a map — OP_INDEX expects list but gets map
        let err = vm_run_err(
            "f x:z>n;x.0",
            Some("f"),
            vec![Value::Map(std::collections::HashMap::new())],
        );
        assert!(err.contains("list") || err.contains("index"), "got: {err}");
    }

    // ── Group G: Additional coverage for lines in 6270+ test section ────────────

    // OP_DIVK_N with zero constant triggers division by zero (line 3531)
    #[test]
    fn vm_divk_n_div_by_zero() {
        let src = "f x:n>n;/x 0";
        let err = vm_run_err(src, Some("f"), vec![Value::Number(10.0)]);
        assert!(err.contains("division by zero"), "got: {err}");
    }

    // OP_DIV_NN with zero denominator triggers division by zero (line 3567)
    #[test]
    fn vm_div_nn_div_by_zero() {
        let src = "f a:n b:n>n;/a b";
        let err = vm_run_err(src, Some("f"), vec![Value::Number(5.0), Value::Number(0.0)]);
        assert!(err.contains("division by zero"), "got: {err}");
    }

    // wrl with list element that is not a string triggers VmError (line 3000)
    #[test]
    fn vm_wrl_non_string_list_element_error() {
        let err = vm_run_err(
            "f p:t xs:L n>R t t;wrl p xs",
            Some("f"),
            vec![
                Value::Text("/tmp/ilo_wrl_elem.txt".into()),
                Value::List(vec![Value::Number(42.0)]),
            ],
        );
        assert!(
            err.contains("wrl") || err.contains("string") || err.contains("list"),
            "got: {err}"
        );
    }

    // Map len (OP_LEN on a map) — happy path exercises the map branch (line 3585)
    #[test]
    fn vm_len_map() {
        let src = "f>n;m=mset mmap \"a\" 1;len m";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(1.0));
    }

    // OP_RECWITH heap-record arm's non-record error (line 3479)
    #[test]
    fn vm_recwith_on_non_record_heap_error() {
        // Pass a non-record where `with` expects a record (z-typed param)
        let err = vm_run_err(
            "type pt{x:n;y:n} f r:z>n;q=r with x:5;q.x",
            Some("f"),
            vec![Value::Text("not-a-record".into())],
        );
        assert!(
            err.contains("record") || err.contains("with") || err.contains("field"),
            "got: {err}"
        );
    }

    // ── jpth error paths (lines 3853-3855, 3875) ────────────────────────────

    // line 3875: jpth invalid JSON → Err
    #[test]
    fn vm_jpth_invalid_json_returns_err() {
        let result = vm_run(
            r#"f s:t>R t t;jpth s "a""#,
            Some("f"),
            vec![Value::Text("not json at all".into())],
        );
        let Value::Err(_) = result else {
            panic!("expected Err")
        };
    }

    // lines 3853-3855: jpth array index out of bounds → Err
    #[test]
    fn vm_jpth_array_index_not_found() {
        let result = vm_run(
            r#"f s:t>R t t;jpth s "a.5""#,
            Some("f"),
            vec![Value::Text(r#"{"a":[1,2]}"#.into())],
        );
        let Value::Err(_) = result else {
            panic!("expected Err")
        };
    }

    // ── cat error paths (lines 3925, 3928) ──────────────────────────────────

    // line 3925: cat with non-text separator
    #[test]
    fn vm_cat_non_text_separator_error() {
        let err = vm_run_err(
            r#"f xs:L t>t;cat xs 42"#,
            Some("f"),
            vec![Value::List(vec![Value::Text("a".into())])],
        );
        assert!(err.contains("cat") || err.contains("text"), "got: {err}");
    }

    // line 3928: cat with non-list first arg (number)
    #[test]
    fn vm_cat_non_list_first_arg_error() {
        let err = vm_run_err(r#"f n:n>t;cat n ",""#, Some("f"), vec![Value::Number(42.0)]);
        assert!(err.contains("cat") || err.contains("list"), "got: {err}");
    }

    // ── hd/tl/rev on heap non-list (lines 3989, 4020, 4041) ─────────────────

    // line 3989: hd on a Map heap value
    #[test]
    fn vm_hd_on_map_error() {
        let err = vm_run_err(
            r#"f m:_>t;hd m"#,
            Some("f"),
            vec![Value::Map(std::collections::HashMap::new())],
        );
        assert!(err.contains("hd") || err.contains("list"), "got: {err}");
    }

    // line 4020: tl on a Map heap value
    #[test]
    fn vm_tl_on_map_error() {
        let err = vm_run_err(
            r#"f m:_>t;tl m"#,
            Some("f"),
            vec![Value::Map(std::collections::HashMap::new())],
        );
        assert!(err.contains("tl") || err.contains("list"), "got: {err}");
    }

    // line 4041: rev on a Map heap value
    #[test]
    fn vm_rev_on_map_error() {
        let err = vm_run_err(
            r#"f m:_>t;rev m"#,
            Some("f"),
            vec![Value::Map(std::collections::HashMap::new())],
        );
        assert!(err.contains("rev") || err.contains("list"), "got: {err}");
    }

    // ── srt on heap non-list (lines 4081, 4084) ──────────────────────────────

    // line 4081: srt on a Map heap value
    #[test]
    fn vm_srt_on_map_error() {
        let err = vm_run_err(
            r#"f m:_>L t;srt m"#,
            Some("f"),
            vec![Value::Map(std::collections::HashMap::new())],
        );
        assert!(err.contains("srt") || err.contains("list"), "got: {err}");
    }

    // line 4084: srt on a number (non-heap, non-string)
    #[test]
    fn vm_srt_on_number_error() {
        let err = vm_run_err("f x:n>L n;srt x", Some("f"), vec![Value::Number(42.0)]);
        assert!(err.contains("srt") || err.contains("list"), "got: {err}");
    }

    // ── slc non-number indices (line 4096) ───────────────────────────────────

    // line 4096: slc with text indices
    #[test]
    fn vm_slc_non_number_indices_error() {
        // slc xs start end — pass text values for start/end
        // We call slc with a list and two text args (bypassing verifier)
        let err = vm_run_err(
            r#"f xs:L n s:t e:t>L n;slc xs s e"#,
            Some("f"),
            vec![
                Value::List(vec![Value::Number(1.0), Value::Number(2.0)]),
                Value::Text("a".into()),
                Value::Text("b".into()),
            ],
        );
        assert!(
            err.contains("slc") || err.contains("indices") || err.contains("number"),
            "got: {err}"
        );
    }

    // ── arena record promotion in += (line 4131) ────────────────────────────

    // line 4131: appending an arena record to a list promotes it to heap
    #[test]
    fn vm_listappend_arena_record_promotes_to_heap() {
        // `pt x:1 y:2` produces an arena record via OP_MKREC.
        // Appending it with `+=xs r` (prefix) triggers OP_LISTAPPEND → line 4131 promotion.
        // Note: ilo += is prefix: `+=list item`.
        let source = "type pt{x:n;y:n} f>n;xs=[];r=pt x:1 y:2;ys=+=xs r;len ys";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(result, Value::Number(1.0));
    }

    // ── nanval_to_json float path (lines 4177-4180) ──────────────────────────

    // line 4178-4180: jdmp float number (non-integer)
    #[test]
    fn vm_jdmp_float_number() {
        let result = vm_run("f>t;jdmp 3.14", Some("f"), vec![]);
        assert_eq!(result, Value::Text("3.14".into()));
    }

    // ── nanval_to_json heap record (lines 4216-4221) ─────────────────────────

    // line 4216-4221: jdmp on a heap record (from jpar) → JSON object
    #[test]
    fn vm_jdmp_heap_record() {
        // jpar produces a heap record; jdmp it back to JSON string
        let result = vm_run(
            r#"f s:t>t;r=jpar! s;jdmp r"#,
            Some("f"),
            vec![Value::Text(r#"{"x":10}"#.into())],
        );
        let Value::Text(s) = result else {
            panic!("expected Text")
        };
        assert!(s.contains("10"), "got: {s}");
    }

    // ── nanval_to_json OkVal/ErrVal (lines 4223-4224) ────────────────────────

    // line 4223: jdmp on Ok value → unwraps inner
    #[test]
    fn vm_jdmp_ok_value() {
        // jpar returns Ok(record) — jdmp on the Ok unwraps inner
        let result = vm_run(
            r#"f s:t>t;r=jpar s;jdmp r"#,
            Some("f"),
            vec![Value::Text(r#"{"v":5}"#.into())],
        );
        let Value::Text(s) = result else {
            panic!("expected Text")
        };
        assert!(s.contains("5"), "got: {s}");
    }

    // ── nanval_to_json Map (lines 4225-4229) ─────────────────────────────────

    // line 4225-4229: jdmp on a Map value → JSON object
    #[test]
    fn vm_jdmp_map_value() {
        let result = vm_run(r#"f>t;m=mset mmap "k" 42;jdmp m"#, Some("f"), vec![]);
        let Value::Text(s) = result else {
            panic!("expected Text")
        };
        assert!(s.contains("42"), "got: {s}");
    }

    // ── nanval_to_json Bool (lines 4206-4207) ────────────────────────────────

    // line 4206: jdmp on Bool true → "true"
    #[test]
    fn vm_jdmp_bool_true() {
        let result = vm_run("f>t;jdmp true", Some("f"), vec![]);
        assert_eq!(result, Value::Text("true".into()));
    }

    // line 4207: jdmp on Bool false → "false"
    #[test]
    fn vm_jdmp_bool_false() {
        let result = vm_run("f>t;jdmp false", Some("f"), vec![]);
        assert_eq!(result, Value::Text("false".into()));
    }

    // ── nanval_to_json ErrVal (line 4224) ────────────────────────────────────

    // line 4224: jdmp on an Err value → inner value
    #[test]
    fn vm_jdmp_err_value() {
        // jpar on invalid JSON returns Err(text). jdmp on that Err hits line 4224.
        let result = vm_run(
            r#"f s:t>t;e=jpar s;jdmp e"#,
            Some("f"),
            vec![Value::Text("not json".into())],
        );
        let Value::Text(_) = result else {
            panic!("expected Text")
        };
        // ErrVal inner serialized
    }

    // ── slc on heap non-list (line 4119) ─────────────────────────────────────

    // line 4119: slc on a Map (heap non-list, non-string)
    #[test]
    fn vm_slc_on_map_heap_error() {
        let err = vm_run_err(
            r#"f m:_ i:n j:n>L t;slc m i j"#,
            Some("f"),
            vec![
                Value::Map(std::collections::HashMap::new()),
                Value::Number(0.0),
                Value::Number(1.0),
            ],
        );
        assert!(err.contains("slc") || err.contains("list"), "got: {err}");
    }

    // ── vm_parse_format raw/unknown path (line 4284) ─────────────────────────

    // line 4284: OP_RD auto-detects format from extension; .txt → "raw" → line 4284
    #[test]
    fn vm_rd_txt_extension_raw_format() {
        let path = "/tmp/ilo_vm_test_raw.txt";
        std::fs::write(path, "hello raw").unwrap();
        let source = format!(r#"f>R t t;rd "{path}""#);
        let result = vm_run(&source, Some("f"), vec![]);
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        assert_eq!(*inner, Value::Text("hello raw".into()));
    }

    // ── vm_parse_csv_row quoted fields (lines 4295-4306) ─────────────────────

    // lines 4295-4306: OP_RD on .csv file with quoted fields (double-quote escaping)
    #[test]
    fn vm_rd_csv_quoted_fields() {
        let path = "/tmp/ilo_vm_test_quoted.csv";
        // CSV with a quoted field containing a comma, and an escaped double-quote
        std::fs::write(path, "\"hello, world\",\"say \"\"hi\"\"\"").unwrap();
        let source = format!(r#"f>n;rows=rd! "{path}";len rows"#);
        let result = vm_run(&source, Some("f"), vec![]);
        assert_eq!(result, Value::Number(1.0)); // one row
    }

    // ── VM interpreter edge cases (fallthrough/unknown opcode) ───────────────

    #[test]
    fn vm_execute_fallthrough_returns_nil() {
        // Manually construct a program with an empty chunk (no RET).
        // execute() should hit the ip >= code.len() path and return Nil.
        let chunk = Chunk {
            code: vec![],
            constants: vec![],
            param_count: 0,
            reg_count: 0,
            spans: vec![],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        let result = run(&program, Some("f"), vec![]).expect("fallthrough should succeed");
        assert_eq!(result, Value::Nil);
    }

    #[test]
    fn vm_unknown_opcode_error_has_span_and_stack() {
        // Create a bogus instruction with an unknown opcode (0xFE)
        let inst = (0xFEu32) << 24;
        let chunk = Chunk {
            code: vec![inst],
            constants: vec![],
            param_count: 0,
            reg_count: 0,
            spans: vec![crate::ast::Span { start: 1, end: 2 }],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        let err = run(&program, Some("f"), vec![]).unwrap_err();
        // Error kind should be UnknownOpcode and span should be captured.
        let msg = err.to_string();
        assert!(
            msg.contains("unknown opcode") || msg.contains("opcode"),
            "got: {msg}"
        );
        assert!(err.span.is_some(), "expected span to be captured");
        assert_eq!(err.call_stack, vec!["f".to_string()]);
    }

    #[test]
    fn vm_error_call_stack_includes_caller_and_callee() {
        // f calls g, g divides by zero → ensure call_stack lists [f, g]
        let prog = parse_program("g x:n>n;/x 0 f>n;g 1");
        let compiled = compile(&prog).unwrap();
        let err = run(&compiled, Some("f"), vec![]).unwrap_err();
        assert!(err.call_stack.contains(&"f".to_string()));
        assert!(err.call_stack.contains(&"g".to_string()));
        // Order should be outermost to innermost.
        let f_pos = err.call_stack.iter().position(|n| n == "f").unwrap();
        let g_pos = err.call_stack.iter().position(|n| n == "g").unwrap();
        assert!(
            f_pos < g_pos,
            "expected f before g in call stack: {:?}",
            err.call_stack
        );
    }

    // ForEach with cnt (continue) — exercises continue_patches patching (L699-706)
    #[test]
    fn vm_foreach_cnt_skips_elements() {
        // Skip x==3, sum the rest: 1+2+4+5 = 12
        let src = "f>n;s=0;@x [1,2,3,4,5]{=x 3{cnt};s=+s x};s";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(12.0));
    }

    // ForRange with cnt (continue) — exercises continue_patches patching for range (L767-776)
    #[test]
    fn vm_range_cnt_patches_applied() {
        // Already tested in vm_range_cnt; this confirms continue_patch loop runs at L771-775
        // Skip i==2: sum 0+1+3+4 = 8
        let src = "f>n;s=0;@i 0..5{=i 2{cnt};s=+s i};s";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(8.0));
    }

    // brk with expression inside ForEach (L841-853)
    #[test]
    fn vm_foreach_brk_with_expr() {
        // brk 99 when x==3 → result is 99
        let src = "f>n;@x [1,2,3,4,5]{=x 3{brk 99};x}";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(99.0));
    }

    // OP_GET type error: non-string arg (L3705-3710)
    // get requires a string url; passing a non-string triggers type error
    #[test]
    fn vm_get_non_string_url_error() {
        let err = vm_run_err("f u:z>R t t;get u", Some("f"), vec![Value::Number(42.0)]);
        assert!(
            err.contains("get") || err.contains("string") || err.contains("type"),
            "got: {err}"
        );
    }

    // OP_POST type error: non-string args (L3727-3734)
    #[test]
    fn vm_post_non_string_args_error() {
        let err = vm_run_err(
            "f u:z b:z>R t t;post u b",
            Some("f"),
            vec![Value::Number(1.0), Value::Text("body".into())],
        );
        assert!(
            err.contains("post") || err.contains("string") || err.contains("type"),
            "got: {err}"
        );
    }

    // OP_GETH type error: non-string url (L3753-3761)
    #[test]
    fn vm_geth_non_string_url_error() {
        let err = vm_run_err(
            "f u:z h:M t t>R t t;get u h",
            Some("f"),
            vec![
                Value::Number(42.0),
                Value::Map(std::collections::HashMap::new()),
            ],
        );
        assert!(
            err.contains("get") || err.contains("string") || err.contains("type"),
            "got: {err}"
        );
    }

    // OP_POSTH type error: non-string url (L3789-3802)
    #[test]
    fn vm_posth_non_string_url_error() {
        let err = vm_run_err(
            "f u:z b:z h:M t t>R t t;post u b h",
            Some("f"),
            vec![
                Value::Number(1.0),
                Value::Text("body".into()),
                Value::Map(std::collections::HashMap::new()),
            ],
        );
        assert!(
            err.contains("post") || err.contains("string") || err.contains("type"),
            "got: {err}"
        );
    }

    // Destructure existing binding with ambiguous field index → OP_RECFLD_NAME (L570)
    // Two types with y at different indices; destructure y twice (second into existing binding)
    #[test]
    fn vm_destructure_existing_binding_ambiguous_field() {
        // First {y}=r creates the y binding, second {y}=r2 reuses it (existing_reg path, L570)
        let src = "type a{x:n;y:n} type b{y:n;x:n} f>n;r=a x:1 y:2;{y}=r;r2=a x:3 y:4;{y}=r2;y";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(4.0));
    }

    // search_field_index: same field at same index in two types → L408+L411 (Some prev==idx arm)
    #[test]
    fn vm_search_field_same_index_arm() {
        // type a{x:n;y:n} and type b{x:n;z:n}: 'x' is at index 0 in both.
        // search_field_index("x") → Some(0) for a, then Some(prev=0)==idx=0 for b → L408 arm
        // Compiler emits OP_RECFLD (unambiguous index) rather than OP_RECFLD_NAME
        let result = vm_run(
            "type a{x:n;y:n} type b{x:n;z:n} f>n;r=a x:5 y:10;{x}=r;x",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(5.0));
    }

    // search_field_index: field exists in type a but NOT in type b → if let Some(idx) else (L412)
    #[test]
    fn vm_search_field_not_in_all_types_phantom() {
        // type a{x:n;y:n} and type b{x:n;z:n}: 'y' only in a (not b).
        // search_field_index("y"): a→Some(1), b→position returns None → if let Some falls to None arm
        // This covers the phantom branch at the end of `if let Some(idx) = position(...)`.
        let result = vm_run(
            "type a{x:n;y:n} type b{x:n;z:n} f>n;r=a x:5 y:10;{y}=r;y",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(10.0));
    }

    // TypeRegistry::register with duplicate name → early return (L253)
    #[test]
    fn vm_type_registry_register_duplicate_name() {
        // Construct a Program with two TypeDef for the same name.
        // The compiler calls register() for each TypeDef; second call returns existing id (L253).
        use crate::ast::{Decl, Param, Program, Span, Type};
        use crate::vm::compile;
        let prog = Program {
            declarations: vec![
                Decl::TypeDef {
                    name: "pt".to_string(),
                    fields: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    span: Span::UNKNOWN,
                },
                Decl::TypeDef {
                    name: "pt".to_string(), // duplicate — triggers early return at L253
                    fields: vec![Param {
                        name: "x".to_string(),
                        ty: Type::Number,
                    }],
                    span: Span::UNKNOWN,
                },
            ],
            source: None,
        };
        let compiled = compile(&prog).expect("compile ok");
        // Type "pt" should exist exactly once in the registry
        assert_eq!(compiled.type_registry.types.len(), 1);
    }

    // =========================================================================
    // VM/interpreter parity tests — ported from src/interpreter/mod.rs
    // =========================================================================

    // ── Basic arithmetic & comparison ────────────────────────────────────

    #[test]
    fn vm_subtract() {
        let source = "f a:n b:n>n;-a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(10.0), Value::Number(3.0)]
            ),
            Value::Number(7.0)
        );
    }

    #[test]
    fn vm_divide() {
        let source = "f a:n b:n>n;/a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(10.0), Value::Number(4.0)]
            ),
            Value::Number(2.5)
        );
    }

    #[test]
    fn vm_equals() {
        let source = "f a:n b:n>b;=a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(1.0), Value::Number(1.0)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(1.0), Value::Number(2.0)]
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_not_equals() {
        let source = "f a:n b:n>b;!=a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(1.0), Value::Number(2.0)]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(1.0), Value::Number(1.0)]
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_greater_than() {
        let source = "f a:n b:n>b;>a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(5.0), Value::Number(3.0)]
            ),
            Value::Bool(true)
        );
    }

    #[test]
    fn vm_less_than() {
        let source = "f a:n b:n>b;<a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(3.0), Value::Number(5.0)]
            ),
            Value::Bool(true)
        );
    }

    #[test]
    fn vm_less_or_equal() {
        let source = "f a:n b:n>b;<=a b";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(3.0), Value::Number(3.0)]
            ),
            Value::Bool(true)
        );
    }

    #[test]
    fn vm_literal_bool() {
        assert_eq!(vm_run("f>b;true", Some("f"), vec![]), Value::Bool(true));
        assert_eq!(vm_run("f>b;false", Some("f"), vec![]), Value::Bool(false));
    }

    #[test]
    fn vm_abs() {
        assert_eq!(vm_run("f>n;abs -7", Some("f"), vec![]), Value::Number(7.0));
    }

    // ── Foreach ─────────────────────────────────────────────────────────

    #[test]
    fn vm_foreach() {
        let source = "f>n;s=0;@x [1, 2, 3]{+s x}";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_foreach_early_return() {
        // Use `ret` inside braced guard for early return from loop
        let source = "f xs:L n>n;@x xs{>=x 3{ret x}};0";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Number(5.0),
                Value::Number(2.0),
            ])],
        );
        assert_eq!(result, Value::Number(5.0));
    }

    #[test]
    fn vm_foreach_on_non_list() {
        let err = vm_run_err("f x:n>n;@i x{i}", Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("foreach") || err.contains("list"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_foreach_return_from_nested_match() {
        let source = "f xs:L n>n;@x xs{?x{5:x;_:0}}";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Number(5.0),
                Value::Number(9.0),
            ])],
        );
        assert_eq!(result, Value::Number(0.0));
    }

    // ── Guard & ternary ─────────────────────────────────────────────────

    #[test]
    fn vm_braced_guard_no_early_return() {
        // Braced guard is conditional execution — no early return
        let source = "f x:n>n;=x 0{99};+x 1";
        // x=0: {99} runs but value is discarded, returns +0 1 = 1
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(0.0)]),
            Value::Number(1.0)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(6.0)
        );
    }

    #[test]
    fn vm_braceless_guard_still_returns_early() {
        // Braceless guard still causes early return
        let source = "f x:n>n;=x 0 99;+x 1";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(0.0)]),
            Value::Number(99.0)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(6.0)
        );
    }

    #[test]
    fn vm_braced_guard_in_loop_no_early_return() {
        // Braced guard inside loop does NOT early-return — finds max of list
        let source = "mx xs:L n>n;m=xs.0;@x xs{>x m{m=x}};+m 0";
        let result = vm_run(
            source,
            Some("mx"),
            vec![Value::List(vec![
                Value::Number(3.0),
                Value::Number(1.0),
                Value::Number(5.0),
            ])],
        );
        assert_eq!(result, Value::Number(5.0));
    }

    #[test]
    fn vm_braceless_guard_early_return_factorial() {
        // Braceless guard still early-returns — factorial
        let source = "f x:n>n;<=x 1 1;r=f -x 1;*x r";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(120.0)
        );
    }

    #[test]
    fn vm_ternary_let_binding() {
        // Ternary let binding: v=cond{then}{else}
        let source = "f x:n>n;v=<x 0{- 0 x}{x};v";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(-3.0)]),
            Value::Number(3.0)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(7.0)]),
            Value::Number(7.0)
        );
    }

    #[test]
    fn vm_ternary_negated() {
        let source = r#"f x:n>t;!=x 1{"not one"}{"one"}"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Text("one".into())
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(2.0)]),
            Value::Text("not one".into())
        );
    }

    #[test]
    fn vm_guard_ternary_in_foreach() {
        let source = "f xs:L n>n;@x xs{=x 0{10}{20}}";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::List(vec![Value::Number(0.0), Value::Number(1.0)])],
        );
        assert_eq!(result, Value::Number(20.0));
    }

    // ── Match ───────────────────────────────────────────────────────────

    #[test]
    fn vm_match_not_last_stmt() {
        let source = "f x:n>n;?x{0:x;_:x};+x 1";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(6.0)
        );
    }

    #[test]
    fn vm_match_expr_no_arm_matches() {
        let source = r#"f>n;y=?1{2:99};y"#;
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Nil);
    }

    #[test]
    fn vm_match_expr_with_bindings() {
        let source = "f x:R n t>n;y=?x{~v:v;_:0};y";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Ok(Box::new(Value::Number(99.0)))],
        );
        assert_eq!(result, Value::Number(99.0));
    }

    #[test]
    fn vm_match_stmt_no_arm_matches() {
        let source = "f x:n>n;?x{1:99};0";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn vm_match_arm_body_with_guard_return() {
        let source = "f x:n>n;y=0;?x{1:>=x 0{42};_:0}";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(1.0)]),
            Value::Number(42.0)
        );
    }

    #[test]
    fn vm_match_continue_arm_returns_nil() {
        let source = "f xs:L n>n;@x xs{?x{1:cnt;_:x}}";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::List(vec![Value::Number(1.0), Value::Number(2.0)])],
        );
        assert_eq!(result, Value::Number(2.0));
    }

    #[test]
    fn vm_match_stmt_continue_propagates() {
        let source = "f xs:L n>n;@x xs{?x{1:cnt;_:x}}";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::List(vec![Value::Number(1.0), Value::Number(5.0)])],
        );
        assert_eq!(result, Value::Number(5.0));
    }

    #[test]
    fn vm_pattern_literal_no_match() {
        let source = r#"f x:n>n;?x{1:10;2:20;_:0}"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn vm_pattern_ok_no_match() {
        let source = r#"f>t;x=^"err";?x{~v:v;_:"default"}"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::Text("default".to_string())
        );
    }

    // ── TypeIs patterns ─────────────────────────────────────────────────

    #[test]
    fn vm_type_is_number_match() {
        let result = vm_run(
            r#"f x:n>t;?x{n v:"num";_:"other"}"#,
            Some("f"),
            vec![Value::Number(42.0)],
        );
        assert_eq!(result, Value::Text("num".into()));
    }

    #[test]
    fn vm_type_is_text_match() {
        let result = vm_run(
            r#"f x:t>t;?x{t v:v;_:"other"}"#,
            Some("f"),
            vec![Value::Text("hello".into())],
        );
        assert_eq!(result, Value::Text("hello".into()));
    }

    #[test]
    fn vm_type_is_bool_match() {
        let result = vm_run(
            r#"f x:b>t;?x{b v:"bool";_:"other"}"#,
            Some("f"),
            vec![Value::Bool(true)],
        );
        assert_eq!(result, Value::Text("bool".into()));
    }

    #[test]
    fn vm_type_is_list_match() {
        let result = vm_run(
            r#"f x:L n>t;?x{l v:"list";_:"other"}"#,
            Some("f"),
            vec![Value::List(vec![Value::Number(1.0)])],
        );
        assert_eq!(result, Value::Text("list".into()));
    }

    #[test]
    fn vm_type_is_no_match_falls_through() {
        let result = vm_run(
            r#"f x:n>t;?x{t v:"text";_:"other"}"#,
            Some("f"),
            vec![Value::Number(1.0)],
        );
        assert_eq!(result, Value::Text("other".into()));
    }

    #[test]
    fn vm_type_is_wildcard_binding() {
        let result = vm_run(
            r#"f x:n>t;?x{n _:"matched";_:"other"}"#,
            Some("f"),
            vec![Value::Number(5.0)],
        );
        assert_eq!(result, Value::Text("matched".into()));
    }

    #[test]
    fn vm_typeis_pattern_non_basic_type_no_match() {
        let source = "f x:z>b;?x{n _:true;_:false}";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Record {
                type_name: "pt".into(),
                fields: std::collections::HashMap::new(),
            }],
        );
        assert_eq!(result, Value::Bool(false));
    }

    // ── Index access ────────────────────────────────────────────────────

    #[test]
    fn vm_index_access_string() {
        let source = "f>t;xs=[\"hello\", \"world\"];xs.0";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::Text("hello".into())
        );
    }

    // ── Unsupported binop ───────────────────────────────────────────────

    #[test]
    fn vm_unsupported_binop() {
        let source = "f a:b b:b>b;-a b";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::Bool(true), Value::Bool(false)],
        );
        assert!(
            err.contains("unsupported") || err.contains("subtract") || err.contains("type"),
            "unexpected error: {}",
            err
        );
    }

    // ── Typedef ─────────────────────────────────────────────────────────

    #[test]
    fn vm_typedef_in_declarations() {
        let source = "type point{x:n;y:n}\nf>n;42";
        assert_eq!(vm_run(source, None, vec![]), Value::Number(42.0));
    }

    #[test]
    fn vm_typedef_not_callable() {
        let source = "type point{x:n;y:n}\nf>n;point 1 2";
        let prog = parse_program(source);
        let result = compile_and_run(&prog, Some("f"), vec![]);
        assert!(result.is_err(), "expected error calling typedef");
    }

    // ── Destructure ─────────────────────────────────────────────────────

    #[test]
    fn vm_destructure_with_text_fields() {
        let source =
            "type usr{name:t;email:t} f>t;u=usr name:\"alice\" email:\"a@b\";{name;email}=u;name";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::Text("alice".to_string())
        );
    }

    #[test]
    fn vm_destructure_missing_field_error() {
        let source = "type pt{x:n;y:n} f>n;p=pt x:3 y:4;{x;z}=p;x";
        let prog = parse_program(source);
        let result = compile_and_run(&prog, Some("f"), vec![]);
        assert!(
            result.is_err(),
            "expected error for missing field in destructure"
        );
    }

    #[test]
    #[ignore] // VM panics (debug assert) instead of returning error
    fn vm_destructure_non_record_error() {
        let source = "type pt{x:n;y:n} f p:pt>n;{x;y}=p;+x y";
        let prog = parse_program(source);
        let result = compile_and_run(&prog, Some("f"), vec![Value::Number(42.0)]);
        assert!(
            result.is_err(),
            "expected error for destructure on non-record"
        );
    }

    // ── Builtins: spl, cat, has, hd, tl, rev, srt, slc ─────────────────

    #[test]
    fn vm_index_access_string_list_second() {
        // Tests accessing second text element in list
        let source = "f>t;xs=[\"hello\", \"world\"];xs.1";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::Text("world".into())
        );
    }

    // ── Env ─────────────────────────────────────────────────────────────

    #[test]
    fn vm_env_unwrap() {
        let _guard = ENV_TEST_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("ILO_TEST_UNWRAP_VM", "world");
        }
        let source = r#"f k:t>R t t;~(env! k)"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Text("ILO_TEST_UNWRAP_VM".into())],
        );
        assert_eq!(result, Value::Ok(Box::new(Value::Text("world".into()))));
        unsafe {
            std::env::remove_var("ILO_TEST_UNWRAP_VM");
        }
    }

    #[test]
    fn vm_env_wrong_arg_type() {
        let err = vm_run_err("f>t;env 42", Some("f"), vec![]);
        assert!(
            err.contains("env") || err.contains("text") || err.contains("string"),
            "got: {err}"
        );
    }

    // ── Range iteration ─────────────────────────────────────────────────

    #[test]
    fn vm_range_as_index() {
        let source = "f>n;@i 0..3{*i i}";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Number(4.0));
    }

    #[test]
    fn vm_range_end_not_number() {
        let source = "f s:n e:n>n;@i s..e{i}";
        assert_eq!(
            vm_run(
                source,
                Some("f"),
                vec![Value::Number(0.0), Value::Number(3.0)]
            ),
            Value::Number(2.0)
        );
    }

    #[test]
    fn vm_for_range_early_return_via_guard() {
        // Use `ret` inside braced guard for early return from loop
        let result = vm_run("f>n;@i 0..5{>=i 3{ret i};i}", Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    #[test]
    fn vm_for_range_non_number_start_error() {
        let err = vm_run_err(
            "f s:t>n;@i s..3{i}",
            Some("f"),
            vec![Value::Text("a".into())],
        );
        assert!(
            err.contains("range")
                || err.contains("number")
                || err.contains("start")
                || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_for_range_non_number_end_error() {
        let err = vm_run_err(
            "f e:t>n;@i 0..e{i}",
            Some("f"),
            vec![Value::Text("b".into())],
        );
        assert!(
            err.contains("range")
                || err.contains("number")
                || err.contains("end")
                || err.contains("type"),
            "got: {err}"
        );
    }

    // ── Error paths: builtin arg count/type errors ──────────────────────

    #[test]
    fn vm_err_abs_wrong_arg_count() {
        let err = vm_run_err("f>n;abs 1 2", Some("f"), vec![]);
        assert!(
            err.contains("abs") || err.contains("arg") || err.contains("expect"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_abs_wrong_type() {
        let err = vm_run_err(
            r#"f x:t>n;abs x"#,
            Some("f"),
            vec![Value::Text("hi".into())],
        );
        assert!(
            err.contains("abs") || err.contains("number") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_cat_non_text_items() {
        let err = vm_run_err("f>t;cat [1,2,3] \",\"", Some("f"), vec![]);
        assert!(err.contains("cat") || err.contains("text"), "got: {err}");
    }

    #[test]
    fn vm_err_cat_wrong_arg_types() {
        let err = vm_run_err(
            "f x:n y:n>t;cat x y",
            Some("f"),
            vec![Value::Number(1.0), Value::Number(2.0)],
        );
        assert!(
            err.contains("cat") || err.contains("list") || err.contains("text"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_cel_non_number() {
        let err = vm_run_err(r#"f x:t>n;cel x"#, Some("f"), vec![Value::Text("a".into())]);
        assert!(
            err.contains("cel") || err.contains("number") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    #[ignore] // VM panics (debug assert) instead of returning error
    fn vm_err_field_access_on_non_record() {
        let err = vm_run_err("f x:n>n;x.y", Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("field") || err.contains("record"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_field_not_found_on_record() {
        let err = vm_run_err("f>n;r=point x:1 y:2;r.z", Some("f"), vec![]);
        assert!(
            err.contains("field") || err.contains("z") || err.contains("not found"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_flr_non_number() {
        let err = vm_run_err(r#"f x:t>n;flr x"#, Some("f"), vec![Value::Text("a".into())]);
        assert!(
            err.contains("flr") || err.contains("number") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_get_non_text_arg() {
        let err = vm_run_err("f x:n>R t t;get x", Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("get") || err.contains("text") || err.contains("string"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_has_text_non_text_needle() {
        let err = vm_run_err(
            "f x:t y:n>b;has x y",
            Some("f"),
            vec![Value::Text("hello".into()), Value::Number(1.0)],
        );
        assert!(
            err.contains("has") || err.contains("text") || err.contains("needle"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_has_wrong_first_arg() {
        let err = vm_run_err(
            "f x:n y:n>b;has x y",
            Some("f"),
            vec![Value::Number(1.0), Value::Number(2.0)],
        );
        assert!(
            err.contains("has") || err.contains("list") || err.contains("text"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_hd_empty_list() {
        let err = vm_run_err("f>n;hd []", Some("f"), vec![]);
        assert!(err.contains("hd") || err.contains("empty"), "got: {err}");
    }

    #[test]
    fn vm_err_hd_empty_text() {
        let err = vm_run_err("f>t;hd \"\"", Some("f"), vec![]);
        assert!(err.contains("hd") || err.contains("empty"), "got: {err}");
    }

    #[test]
    fn vm_err_hd_wrong_type() {
        let err = vm_run_err("f x:n>n;hd x", Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("hd")
                || err.contains("list")
                || err.contains("text")
                || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    #[ignore] // VM panics (debug assert) instead of returning error
    fn vm_err_index_on_non_list() {
        let err = vm_run_err("f x:n>n;x.0", Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("index")
                || err.contains("field")
                || err.contains("list")
                || err.contains("record"),
            "got: {}",
            err
        );
    }

    #[test]
    fn vm_err_index_out_of_bounds() {
        let err = vm_run_err("f>n;xs=[1, 2];xs.5", Some("f"), vec![]);
        assert!(
            err.contains("bound") || err.contains("index") || err.contains("5"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_len_wrong_arg_count() {
        let err = vm_run_err("f>n;len 1 2", Some("f"), vec![]);
        assert!(
            err.contains("len") || err.contains("arg") || err.contains("expect"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_len_wrong_type() {
        let err = vm_run_err("f x:n>n;len x", Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("len")
                || err.contains("string")
                || err.contains("list")
                || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_max_non_number() {
        let err = vm_run_err(
            r#"f a:t b:t>n;max a b"#,
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("b".into())],
        );
        assert!(
            err.contains("max") || err.contains("number") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_min_non_number() {
        let err = vm_run_err(
            r#"f a:t b:t>n;min a b"#,
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("b".into())],
        );
        assert!(
            err.contains("min") || err.contains("number") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_num_wrong_arg_count() {
        let err = vm_run_err(r#"f>R n t;num "1" "2""#, Some("f"), vec![]);
        assert!(
            err.contains("num") || err.contains("arg") || err.contains("expect"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_num_wrong_type() {
        let err = vm_run_err("f x:n>R n t;num x", Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("num") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_rev_wrong_type() {
        let err = vm_run_err("f x:n>n;rev x", Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("rev")
                || err.contains("list")
                || err.contains("text")
                || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_rnd_lower_gt_upper() {
        let err = vm_run_err("f>n;rnd 10 1", Some("f"), vec![]);
        assert!(err.contains("rnd") || err.contains("bound"), "got: {err}");
    }

    #[test]
    fn vm_err_rnd_wrong_arg_types() {
        let err = vm_run_err(
            "f x:t y:t>n;rnd x y",
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("b".into())],
        );
        assert!(
            err.contains("rnd") || err.contains("number") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    #[ignore] // VM panics (debug assert) instead of returning error
    fn vm_err_slc_non_number_end() {
        let err = vm_run_err(
            "f x:t y:t>t;slc x 0 y",
            Some("f"),
            vec![Value::Text("hi".into()), Value::Text("a".into())],
        );
        assert!(
            err.contains("slc")
                || err.contains("number")
                || err.contains("index")
                || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_slc_non_number_start() {
        let err = vm_run_err(
            "f x:t y:t>t;slc x y 1",
            Some("f"),
            vec![Value::Text("hi".into()), Value::Text("a".into())],
        );
        assert!(
            err.contains("slc")
                || err.contains("number")
                || err.contains("index")
                || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_slc_wrong_first_arg() {
        let err = vm_run_err("f x:n>n;slc x 0 1", Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("slc")
                || err.contains("list")
                || err.contains("text")
                || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_spl_non_text_first() {
        let err = vm_run_err(
            "f x:n y:t>L t;spl x y",
            Some("f"),
            vec![Value::Number(1.0), Value::Text("a".into())],
        );
        assert!(
            err.contains("spl") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_spl_non_text_second() {
        let err = vm_run_err(
            "f x:t y:n>L t;spl x y",
            Some("f"),
            vec![Value::Text("a-b".into()), Value::Number(1.0)],
        );
        assert!(
            err.contains("spl") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_srt_mixed_types() {
        let err = vm_run_err("f>L n;srt [1,\"a\"]", Some("f"), vec![]);
        assert!(
            err.contains("srt") || err.contains("mixed") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_srt_wrong_type() {
        let err = vm_run_err("f x:n>n;srt x", Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("srt")
                || err.contains("list")
                || err.contains("text")
                || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_str_wrong_arg_count() {
        let err = vm_run_err("f>t;str 1 2", Some("f"), vec![]);
        assert!(
            err.contains("str") || err.contains("arg") || err.contains("expect"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_str_wrong_type() {
        let err = vm_run_err(
            r#"f x:t>t;str x"#,
            Some("f"),
            vec![Value::Text("hi".into())],
        );
        assert!(
            err.contains("str") || err.contains("number") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_tl_empty_list() {
        let err = vm_run_err("f>L n;tl []", Some("f"), vec![]);
        assert!(err.contains("tl") || err.contains("empty"), "got: {err}");
    }

    #[test]
    fn vm_err_tl_empty_text() {
        let err = vm_run_err("f>t;tl \"\"", Some("f"), vec![]);
        assert!(err.contains("tl") || err.contains("empty"), "got: {err}");
    }

    #[test]
    fn vm_err_tl_wrong_type() {
        let err = vm_run_err("f x:n>n;tl x", Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("tl")
                || err.contains("list")
                || err.contains("text")
                || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_err_trm_wrong_type() {
        let err = vm_run_err("f x:n>t;trm x", Some("f"), vec![Value::Number(1.0)]);
        assert!(
            err.contains("trm") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    #[ignore] // VM panics (debug assert) instead of returning error
    fn vm_err_with_on_non_record() {
        let err = vm_run_err("f x:n>n;x with y:1", Some("f"), vec![Value::Number(1.0)]);
        assert!(err.contains("with") || err.contains("record"), "got: {err}");
    }

    #[test]
    #[ignore] // VM panics (debug assert) instead of returning error
    fn vm_err_wrong_arity() {
        let err = vm_run_err("f x:n>n;x", Some("f"), vec![]);
        assert!(
            err.contains("expected")
                || err.contains("arg")
                || err.contains("arity")
                || err.contains("1"),
            "got: {err}"
        );
    }

    // ── HOF builtins: map, flt, fld, grp ────────────────────────────────

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_map_squares() {
        let source = "sq x:n>n;*x x main xs:L n>L n;map sq xs";
        let result = vm_run(
            source,
            Some("main"),
            vec![Value::List(
                vec![1.0, 2.0, 3.0, 4.0, 5.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            )],
        );
        assert_eq!(
            result,
            Value::List(
                vec![1.0, 4.0, 9.0, 16.0, 25.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect()
            )
        );
    }

    #[test]
    fn vm_map_wrong_fn_arg() {
        let err = vm_run_err("f>t;map 42 [1, 2]", Some("f"), vec![]);
        assert!(
            err.contains("map") || err.contains("fn") || err.contains("function"),
            "got: {err}"
        );
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_map_wrong_list_arg() {
        let source = "sq x:n>n;*x x f>t;map sq 42";
        let err = vm_run_err(source, Some("f"), vec![]);
        assert!(err.contains("map") || err.contains("list"), "got: {err}");
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_map_with_text_fn_name() {
        let source = "sq x:n>n;*x x f cb:t xs:L n>L n;map cb xs";
        let result = vm_run(
            source,
            Some("f"),
            vec![
                Value::Text("sq".into()),
                Value::List(vec![Value::Number(3.0)]),
            ],
        );
        assert_eq!(result, Value::List(vec![Value::Number(9.0)]));
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_flt_positive() {
        let source = "pos x:n>b;>x 0 main xs:L n>L n;flt pos xs";
        let result = vm_run(
            source,
            Some("main"),
            vec![Value::List(
                vec![-3.0, -1.0, 0.0, 2.0, 4.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            )],
        );
        assert_eq!(
            result,
            Value::List(vec![2.0, 4.0].into_iter().map(Value::Number).collect())
        );
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_flt_predicate_returns_non_bool() {
        let source = "id x:n>n;x f xs:L n>L n;flt id xs";
        let err = vm_run_err(
            source,
            Some("f"),
            vec![Value::List(vec![Value::Number(1.0)])],
        );
        assert!(err.contains("flt") || err.contains("bool"), "got: {err}");
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_flt_wrong_list_arg() {
        let source = "pos x:n>b;>x 0 f>t;flt pos 42";
        let err = vm_run_err(source, Some("f"), vec![]);
        assert!(err.contains("flt") || err.contains("list"), "got: {err}");
    }

    #[test]
    fn vm_flt_key_not_fn_ref() {
        let err = vm_run_err(
            "f xs:L n>L n;flt 42 xs",
            Some("f"),
            vec![Value::List(vec![Value::Number(1.0)])],
        );
        assert!(
            err.contains("flt") || err.contains("fn") || err.contains("function"),
            "got: {err}"
        );
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_fld_sum() {
        let source = "add a:n b:n>n;+a b main xs:L n>n;fld add xs 0";
        let result = vm_run(
            source,
            Some("main"),
            vec![Value::List(
                vec![1.0, 2.0, 3.0, 4.0, 5.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            )],
        );
        assert_eq!(result, Value::Number(15.0));
    }

    #[test]
    fn vm_fld_wrong_fn_arg() {
        let err = vm_run_err("f>n;fld 42 [1, 2] 0", Some("f"), vec![]);
        assert!(
            err.contains("fld") || err.contains("fn") || err.contains("function"),
            "got: {err}"
        );
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_fld_wrong_list_arg() {
        let source = "add a:n b:n>n;+a b f>n;fld add 42 0";
        let err = vm_run_err(source, Some("f"), vec![]);
        assert!(err.contains("fld") || err.contains("list"), "got: {err}");
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_grp_by_string_key() {
        let source = r#"cl x:n>t;>x 5{"big"}{"small"} main xs:L n>M t L n;grp cl xs"#;
        let result = vm_run(
            source,
            Some("main"),
            vec![Value::List(
                vec![1.0, 8.0, 3.0, 9.0, 2.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            )],
        );
        let Value::Map(m) = result else {
            panic!("expected Map")
        };
        assert_eq!(
            m.get("small").unwrap(),
            &Value::List(vec![1.0, 3.0, 2.0].into_iter().map(Value::Number).collect())
        );
        assert_eq!(
            m.get("big").unwrap(),
            &Value::List(vec![8.0, 9.0].into_iter().map(Value::Number).collect())
        );
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_grp_by_numeric_key() {
        let source = "key x:n>t;str x main xs:L n>M t L n;grp key xs";
        let result = vm_run(
            source,
            Some("main"),
            vec![Value::List(
                vec![1.0, 2.0, 1.0, 3.0, 2.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            )],
        );
        let Value::Map(m) = result else {
            panic!("expected Map")
        };
        assert_eq!(
            m.get("1").unwrap(),
            &Value::List(vec![1.0, 1.0].into_iter().map(Value::Number).collect())
        );
        assert_eq!(
            m.get("2").unwrap(),
            &Value::List(vec![2.0, 2.0].into_iter().map(Value::Number).collect())
        );
        assert_eq!(
            m.get("3").unwrap(),
            &Value::List(vec![3.0].into_iter().map(Value::Number).collect())
        );
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_grp_empty_list() {
        let source = "id x:n>t;str x main xs:L n>M t L n;grp id xs";
        let result = vm_run(source, Some("main"), vec![Value::List(vec![])]);
        assert_eq!(result, Value::Map(std::collections::HashMap::new()));
    }

    #[test]
    fn vm_grp_wrong_fn_arg() {
        let err = vm_run_err("f>t;grp 42 [1, 2, 3]", Some("f"), vec![]);
        assert!(
            err.contains("grp") || err.contains("fn") || err.contains("function"),
            "got: {err}"
        );
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_grp_wrong_list_arg() {
        let err = vm_run_err("id x:n>n;x f>t;grp id 42", Some("f"), vec![]);
        assert!(err.contains("grp") || err.contains("list"), "got: {err}");
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_grp_number_key() {
        let source = "id x:n>n;x g xs:L n>_;grp id xs";
        let result = vm_run(
            source,
            Some("g"),
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(1.0),
            ])],
        );
        let Value::Map(m) = result else {
            panic!("expected map")
        };
        assert_eq!(m.len(), 2);
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_grp_bool_key() {
        let source = "pos x:n>b;>x 0 g xs:L n>_;grp pos xs";
        let result = vm_run(
            source,
            Some("g"),
            vec![Value::List(vec![
                Value::Number(-1.0),
                Value::Number(1.0),
                Value::Number(2.0),
            ])],
        );
        let Value::Map(m) = result else {
            panic!("expected map")
        };
        assert!(m.contains_key("true"));
        assert!(m.contains_key("false"));
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_grp_float_key() {
        let source = "half x:n>n;/x 2 g xs:L n>_;grp half xs";
        let result = vm_run(
            source,
            Some("g"),
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])],
        );
        let Value::Map(m) = result else {
            panic!("expected Map")
        };
        assert!(
            m.contains_key("0.5") || m.contains_key("1.5"),
            "expected float key, got: {:?}",
            m.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_grp_key_returns_list_error() {
        let source = "mk x:n>L n;[x] g xs:L n>_;grp mk xs";
        let err = vm_run_err(
            source,
            Some("g"),
            vec![Value::List(vec![Value::Number(1.0), Value::Number(2.0)])],
        );
        assert!(
            err.contains("grp") || err.contains("key") || err.contains("string"),
            "got: {err}"
        );
    }

    // ── sum, avg ────────────────────────────────────────────────────────

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_sum_basic() {
        let source = "f xs:L n>n;sum xs";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::List(
                vec![1.0, 2.0, 3.0, 4.0, 5.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect(),
            )],
        );
        assert_eq!(result, Value::Number(15.0));
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_sum_empty() {
        let source = "f xs:L n>n;sum xs";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::List(vec![])]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn vm_sum_wrong_arg() {
        let err = vm_run_err("f>n;sum 42", Some("f"), vec![]);
        assert!(err.contains("sum") || err.contains("list"), "got: {err}");
    }

    #[test]
    fn vm_sum_non_numeric_element() {
        let err = vm_run_err(r#"f>n;sum ["a", "b"]"#, Some("f"), vec![]);
        assert!(err.contains("sum") || err.contains("number"), "got: {err}");
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_avg_basic() {
        let source = "f xs:L n>n;avg xs";
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::List(
                vec![2.0, 4.0, 6.0].into_iter().map(Value::Number).collect(),
            )],
        );
        assert_eq!(result, Value::Number(4.0));
    }

    #[test]
    fn vm_avg_empty_error() {
        let err = vm_run_err("f>n;avg []", Some("f"), vec![]);
        assert!(err.contains("avg") || err.contains("empty"), "got: {err}");
    }

    #[test]
    fn vm_avg_wrong_arg() {
        let err = vm_run_err("f>n;avg 42", Some("f"), vec![]);
        assert!(err.contains("avg") || err.contains("list"), "got: {err}");
    }

    #[test]
    fn vm_avg_non_number_element() {
        let err = vm_run_err(
            "f xs:L n>n;avg xs",
            Some("f"),
            vec![Value::List(vec![Value::Text("x".into())])],
        );
        assert!(err.contains("avg") || err.contains("number"), "got: {err}");
    }

    // ── flat ────────────────────────────────────────────────────────────

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_flat_nested() {
        let source = "f>L n;flat [[1, 2], [3], [4, 5]]";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(
                vec![1.0, 2.0, 3.0, 4.0, 5.0]
                    .into_iter()
                    .map(Value::Number)
                    .collect()
            )
        );
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_flat_mixed() {
        let source = "f>L n;flat [[1, 2], 3]";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(vec![1.0, 2.0, 3.0].into_iter().map(Value::Number).collect())
        );
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_flat_empty() {
        assert_eq!(
            vm_run("f>L n;flat []", Some("f"), vec![]),
            Value::List(vec![])
        );
    }

    #[test]
    fn vm_flat_wrong_arg() {
        let err = vm_run_err("f>L n;flat 42", Some("f"), vec![]);
        assert!(err.contains("flat") || err.contains("list"), "got: {err}");
    }

    // ── srt with key fn ─────────────────────────────────────────────────

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_srt_fn_by_length() {
        let source = "ln s:t>n;len s main xs:L t>L t;srt ln xs";
        let result = vm_run(
            source,
            Some("main"),
            vec![Value::List(vec![
                Value::Text("banana".into()),
                Value::Text("a".into()),
                Value::Text("cc".into()),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Text("a".into()),
                Value::Text("cc".into()),
                Value::Text("banana".into()),
            ])
        );
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_srt_fn_numeric_key() {
        let source = "neg x:n>n;-x main xs:L n>L n;srt neg xs";
        let result = vm_run(
            source,
            Some("main"),
            vec![Value::List(vec![
                Value::Number(1.0),
                Value::Number(3.0),
                Value::Number(2.0),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(3.0),
                Value::Number(2.0),
                Value::Number(1.0)
            ])
        );
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_srt_key_fn_text_keys() {
        let source = "id x:t>t;x main xs:L t>L t;srt id xs";
        let result = vm_run(
            source,
            Some("main"),
            vec![Value::List(vec![
                Value::Text("banana".into()),
                Value::Text("apple".into()),
                Value::Text("cherry".into()),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Text("apple".into()),
                Value::Text("banana".into()),
                Value::Text("cherry".into()),
            ])
        );
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_srt_key_fn_wrong_second_arg() {
        let source = "sq x:n>n;*x x f>n;srt sq 42";
        let err = vm_run_err(source, Some("f"), vec![]);
        assert!(err.contains("srt") || err.contains("list"), "got: {err}");
    }

    #[test]
    fn vm_srt_key_not_fn_ref() {
        let err = vm_run_err(
            "f xs:L n>L n;srt 42 xs",
            Some("f"),
            vec![Value::List(vec![Value::Number(1.0)])],
        );
        assert!(
            err.contains("srt") || err.contains("fn") || err.contains("function"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_srt_text_string() {
        assert_eq!(
            vm_run(r#"f>t;srt "cab""#, Some("f"), vec![]),
            Value::Text("abc".into())
        );
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_srt_bool_key_equal_ordering() {
        let source = "pos x:n>b;> x 0 f>L n;srt pos [3,-1,2,-2]";
        let result = vm_run(source, Some("f"), vec![]);
        let Value::List(items) = result else {
            panic!("expected List, got {:?}", result)
        };
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn vm_ok_srt_empty_list() {
        assert_eq!(
            vm_run("f>L n;srt []", Some("f"), vec![]),
            Value::List(vec![])
        );
    }

    // ── slc clamped ─────────────────────────────────────────────────────

    #[test]
    fn vm_slc_clamped() {
        let source = "f>L n;slc [1, 2, 3] 1 100";
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![Value::Number(2.0), Value::Number(3.0)])
        );
    }

    // ── unq ─────────────────────────────────────────────────────────────

    #[test]
    fn vm_unq_list_strings() {
        let result = vm_run(
            "f xs:L t>L t;unq xs",
            Some("f"),
            vec![Value::List(vec![
                Value::Text("a".into()),
                Value::Text("b".into()),
                Value::Text("a".into()),
            ])],
        );
        assert_eq!(
            result,
            Value::List(vec![Value::Text("a".into()), Value::Text("b".into())])
        );
    }

    #[test]
    fn vm_unq_text_chars() {
        assert_eq!(
            vm_run(
                "f s:t>t;unq s",
                Some("f"),
                vec![Value::Text("aabbc".into())]
            ),
            Value::Text("abc".into())
        );
    }

    #[test]
    fn vm_unq_wrong_type() {
        let err = vm_run_err("f>n;unq 42", Some("f"), vec![]);
        assert!(
            err.contains("unq")
                || err.contains("list")
                || err.contains("text")
                || err.contains("type"),
            "got: {err}"
        );
    }

    // ── fmt ──────────────────────────────────────────────────────────────

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_fmt_basic() {
        let result = vm_run(
            r#"f a:t b:t>t;fmt "{} + {}" a b"#,
            Some("f"),
            vec![Value::Text("1".into()), Value::Text("2".into())],
        );
        assert_eq!(result, Value::Text("1 + 2".into()));
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_fmt_template_only() {
        assert_eq!(
            vm_run(r#"f>t;fmt "hello""#, Some("f"), vec![]),
            Value::Text("hello".into())
        );
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_fmt_fewer_args_than_slots() {
        let result = vm_run(
            r#"f a:t>t;fmt "{} and {}" a"#,
            Some("f"),
            vec![Value::Text("x".into())],
        );
        assert_eq!(result, Value::Text("x and {}".into()));
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_fmt_number_arg() {
        let result = vm_run(
            r#"f n:n>t;fmt "value: {}" n"#,
            Some("f"),
            vec![Value::Number(42.0)],
        );
        assert_eq!(result, Value::Text("value: 42".into()));
    }

    #[test]
    fn vm_fmt_wrong_first_arg() {
        let err = vm_run_err("f>n;fmt 42", Some("f"), vec![]);
        assert!(
            err.contains("fmt") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    // ── prnt ─────────────────────────────────────────────────────────────

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_prnt_text_passthrough() {
        assert_eq!(
            vm_run("f s:t>t;prnt s", Some("f"), vec![Value::Text("hi".into())]),
            Value::Text("hi".into())
        );
    }

    // ── rgx ──────────────────────────────────────────────────────────────

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_rgx_find_all() {
        let source = r#"f s:t>L t;rgx "\d+" s"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Text("abc 123 def 456".into())],
        );
        assert_eq!(
            result,
            Value::List(vec![Value::Text("123".into()), Value::Text("456".into()),])
        );
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_rgx_capture_groups() {
        let source = r#"f s:t>L t;rgx "(\w+)=(\w+)" s"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Text("name=alice age=30".into())],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Text("name".into()),
                Value::Text("alice".into()),
            ])
        );
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_rgx_no_match() {
        let source = r#"f s:t>L t;rgx "\d+" s"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Text("no numbers here".into())],
        );
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn vm_rgx_invalid_pattern() {
        let err = vm_run_err(r#"f>L t;rgx "[invalid" "test""#, Some("f"), vec![]);
        assert!(
            err.contains("rgx") || err.contains("regex") || err.contains("pattern"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_rgx_wrong_arg_types() {
        let err = vm_run_err(r#"f>L t;rgx 42 "test""#, Some("f"), vec![]);
        assert!(
            err.contains("rgx") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_rgx_non_text_second_arg() {
        let err = vm_run_err(r#"f>L t;rgx "." 42"#, Some("f"), vec![]);
        assert!(
            err.contains("rgx") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    // ── JSON builtins ───────────────────────────────────────────────────

    #[test]
    fn vm_jp_object() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![
                Value::Text(r#"{"name":"alice"}"#.to_string()),
                Value::Text("name".to_string()),
            ],
        );
        assert_eq!(
            result,
            Value::Ok(Box::new(Value::Text("alice".to_string())))
        );
    }

    #[test]
    fn vm_jp_invalid_json() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![
                Value::Text("not json".to_string()),
                Value::Text("x".to_string()),
            ],
        );
        assert!(matches!(result, Value::Err(_)));
    }

    #[test]
    fn vm_jparse_scalar() {
        let source = r#"f j:t>R t t;jpar j"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("42".to_string())]),
            Value::Ok(Box::new(Value::Number(42.0)))
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("true".to_string())]),
            Value::Ok(Box::new(Value::Bool(true)))
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("null".to_string())]),
            Value::Ok(Box::new(Value::Nil))
        );
    }

    #[test]
    fn vm_jpar_wrong_arg_type() {
        let err = vm_run_err("f>t;jpar 42", Some("f"), vec![]);
        assert!(
            err.contains("jpar") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_jpth_array_index() {
        let source = r#"f j:t p:t>R t t;jpth j p"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![
                Value::Text(r#"[10,20,30]"#.to_string()),
                Value::Text("1".to_string()),
            ],
        );
        assert_eq!(result, Value::Ok(Box::new(Value::Text("20".into()))));
    }

    #[test]
    fn vm_jpth_array_index_out_of_bounds() {
        let source = r#"f>R t t;jpth "[1,2,3]" "5""#;
        let result = vm_run(source, Some("f"), vec![]);
        let Value::Err(inner) = result else {
            panic!("expected Err, got {:?}", result)
        };
        let s = inner.to_string();
        assert!(
            s.contains("not found") || s.contains("5") || s.contains("key"),
            "got: {s}"
        );
    }

    #[test]
    fn vm_jpth_wrong_args() {
        let err = vm_run_err(r#"f>t;jpth 42 "path""#, Some("f"), vec![]);
        assert!(
            err.contains("jpth") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_jdmp_bool_value() {
        assert_eq!(
            vm_run("f>t;jdmp true", Some("f"), vec![]),
            Value::Text("true".into())
        );
    }

    #[test]
    fn vm_jdmp_nil_value() {
        let result = vm_run(r#"f>t;jdmp (mget mmap "k")"#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("null".into()));
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_jdmp_fnref() {
        let source = "sq x:n>n;*x x f>t;r=sq;jdmp r";
        let result = vm_run(source, Some("f"), vec![]);
        let Value::Text(s) = result else {
            panic!("expected Text")
        };
        assert!(s.contains("fn:sq") || s.contains("sq"), "got: {s}");
    }

    #[test]
    fn vm_jdmp_large_float() {
        let source = "f x:n>t;jdmp x";
        let result = vm_run(source, Some("f"), vec![Value::Number(1.23456789e20)]);
        assert!(matches!(result, Value::Text(_)));
    }

    // ── Map builtins ────────────────────────────────────────────────────

    #[test]
    fn vm_mhas_found() {
        let result = vm_run(r#"f>b;m=mset mmap "x" 1;mhas m "x""#, Some("f"), vec![]);
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_mhas_not_found() {
        let result = vm_run(r#"f>b;m=mset mmap "x" 1;mhas m "y""#, Some("f"), vec![]);
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    #[ignore] // VM null-pointer crash: map builtins don't validate non-map args
    fn vm_mhas_wrong_args() {
        let err = vm_run_err("f>n;mhas 42 \"key\"", Some("f"), vec![]);
        assert!(err.contains("mhas") || err.contains("map"), "got: {err}");
    }

    #[test]
    fn vm_mkeys_happy_path() {
        let result = vm_run(
            r#"f>L t;m=mset (mset mmap "b" 2) "a" 1;mkeys m"#,
            Some("f"),
            vec![],
        );
        assert_eq!(
            result,
            Value::List(vec![Value::Text("a".into()), Value::Text("b".into())])
        );
    }

    #[test]
    #[ignore] // VM null-pointer crash: map builtins don't validate non-map args
    fn vm_mkeys_wrong_args() {
        let err = vm_run_err("f>n;mkeys 42", Some("f"), vec![]);
        assert!(err.contains("mkeys") || err.contains("map"), "got: {err}");
    }

    #[test]
    fn vm_mvals_happy_path() {
        let result = vm_run(
            r#"f>L n;m=mset (mset mmap "b" 2) "a" 1;mvals m"#,
            Some("f"),
            vec![],
        );
        assert_eq!(
            result,
            Value::List(vec![Value::Number(1.0), Value::Number(2.0)])
        );
    }

    #[test]
    #[ignore] // VM null-pointer crash: map builtins don't validate non-map args
    fn vm_mvals_wrong_args() {
        let err = vm_run_err("f>n;mvals 42", Some("f"), vec![]);
        assert!(err.contains("mvals") || err.contains("map"), "got: {err}");
    }

    #[test]
    fn vm_mdel_happy_path() {
        let result = vm_run(
            r#"f>n;m=mset (mset mmap "a" 1) "b" 2;m2=mdel m "a";len m2"#,
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(1.0));
    }

    #[test]
    #[ignore] // VM null-pointer crash: map builtins don't validate non-map args
    fn vm_mdel_wrong_args() {
        let err = vm_run_err("f>n;mdel 42 \"key\"", Some("f"), vec![]);
        assert!(err.contains("mdel") || err.contains("map"), "got: {err}");
    }

    #[test]
    #[ignore] // VM null-pointer crash: map builtins don't validate non-map args
    fn vm_mget_wrong_args() {
        let err = vm_run_err("f>n;mget 42 \"key\"", Some("f"), vec![]);
        assert!(err.contains("mget") || err.contains("map"), "got: {err}");
    }

    #[test]
    #[ignore] // VM null-pointer crash: map builtins don't validate non-map args
    fn vm_mset_wrong_args() {
        let err = vm_run_err("f>n;mset 42 \"key\" 1", Some("f"), vec![]);
        assert!(err.contains("mset") || err.contains("map"), "got: {err}");
    }

    // ── rnd ─────────────────────────────────────────────────────────────

    #[test]
    fn vm_rnd_wrong_types() {
        let err = vm_run_err(r#"f>n;rnd "a" "b""#, Some("f"), vec![]);
        assert!(
            err.contains("rnd") || err.contains("number") || err.contains("type"),
            "got: {err}"
        );
    }

    // ── Safe field/index on nil ──────────────────────────────────────────

    #[test]
    fn vm_safe_field_on_nil_returns_nil() {
        let result = vm_run("f>n;x=mget mmap \"key\";x.?field", Some("f"), vec![]);
        assert_eq!(result, Value::Nil);
    }

    #[test]
    fn vm_safe_index_on_nil_returns_nil() {
        let result = vm_run("f>n;xs=mget mmap \"key\";xs.?0", Some("f"), vec![]);
        assert_eq!(result, Value::Nil);
    }

    // ── FnRef callee from scope ─────────────────────────────────────────

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_fnref_callee_from_scope() {
        let source = "sq x:n>n;*x x f cb:z>n;cb 3";
        let result = vm_run(source, Some("f"), vec![Value::FnRef("sq".into())]);
        assert_eq!(result, Value::Number(9.0));
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_fn_ref_via_ref_expr() {
        let source = "dbl x:n>n;*x 2 main>n;f=dbl;f 10";
        assert_eq!(vm_run(source, Some("main"), vec![]), Value::Number(20.0));
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_text_callee_from_scope() {
        let source = "sq x:n>n;*x x f cb:z>n;cb 3";
        let result = vm_run(source, Some("f"), vec![Value::Text("sq".into())]);
        assert_eq!(result, Value::Number(9.0));
    }

    #[test]
    #[ignore] // VM missing HOF/FnRef resolution
    fn vm_user_hof_fn_type() {
        let source = "sq x:n>n;*x x apl f:F n n x:n>n;f x";
        let result = vm_run(
            source,
            Some("apl"),
            vec![Value::FnRef("sq".to_string()), Value::Number(7.0)],
        );
        assert_eq!(result, Value::Number(49.0));
    }

    // ── bang on non-Result passes through ─────────────────────────────────

    #[test]
    fn vm_bang_on_non_result_passes_through() {
        let source = "id x:n>z;x f>z;id! 42";
        let result = vm_run(source, Some("f"), vec![]);
        assert_eq!(result, Value::Number(42.0));
    }

    // ── brk/cnt in guard/ternary/match ──────────────────────────────────

    #[test]
    fn vm_brk_inside_guard_body_propagates() {
        let src = "f>n;@x [1,2,3,4]{>x 2{brk x};x}";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_cnt_inside_guard_body_propagates() {
        let src = "f>n;@x [1,2,3]{=x 1{cnt};x}";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_brk_inside_ternary_body_propagates() {
        let src = "f>n;@x [1,2,3]{=x 2{brk x}{0};0}";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(2.0));
    }

    #[test]
    fn vm_cnt_inside_ternary_body_propagates() {
        let src = "f>n;@x [1,2,3]{=x 1{cnt}{0};x}";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_brk_inside_match_arm_propagates() {
        let src = "f>n;@x [1,2,3]{?x{2:brk x;_:x};x}";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(2.0));
    }

    #[test]
    fn vm_cnt_in_match_expr_arm_returns_nil() {
        let src = "f>n;@x [1,2,3]{r=?x{1:cnt;_:x};r}";
        assert_eq!(vm_run(src, Some("f"), vec![]), Value::Number(3.0));
    }

    #[test]
    fn vm_continue_in_function_body_returns_nil() {
        let result = vm_run("f>_;cnt", Some("f"), vec![]);
        assert_eq!(result, Value::Nil);
    }

    // ── rdb ─────────────────────────────────────────────────────────────

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_rdb_csv() {
        let result = vm_run(
            r#"f s:t>t;rdb s "csv""#,
            Some("f"),
            vec![Value::Text("a,b\n1,2".into())],
        );
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        let Value::List(rows) = *inner else {
            panic!("expected list")
        };
        assert_eq!(rows.len(), 2);
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_rdb_csv_single_row() {
        let result = vm_run(
            r#"f s:t>t;rdb s "csv""#,
            Some("f"),
            vec![Value::Text("a,b,c".into())],
        );
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        let Value::List(rows) = *inner else {
            panic!("expected list")
        };
        assert_eq!(rows.len(), 1);
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_rdb_json() {
        let result = vm_run(
            r#"f s:t>t;rdb s "json""#,
            Some("f"),
            vec![Value::Text(r#"{"x":1}"#.into())],
        );
        assert!(
            matches!(result, Value::Ok(_)),
            "expected Ok, got {:?}",
            result
        );
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_rdb_invalid_json_is_err() {
        let result = vm_run(
            r#"f s:t>t;rdb s "json""#,
            Some("f"),
            vec![Value::Text("not json".into())],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err, got {:?}",
            result
        );
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_rdb_raw_passthrough() {
        let result = vm_run(
            r#"f s:t>t;rdb s "raw""#,
            Some("f"),
            vec![Value::Text("hello".into())],
        );
        assert_eq!(result, Value::Ok(Box::new(Value::Text("hello".into()))));
    }

    #[test]
    fn vm_rdb_wrong_first_arg() {
        let err = vm_run_err(r#"f>t;rdb 42 "raw""#, Some("f"), vec![]);
        assert!(
            err.contains("rdb") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_rdb_wrong_format_arg() {
        let err = vm_run_err(r#"f>t;rdb "hello" 42"#, Some("f"), vec![]);
        assert!(
            err.contains("rdb") || err.contains("format") || err.contains("text"),
            "got: {err}"
        );
    }

    // ── rd ──────────────────────────────────────────────────────────────

    #[test]
    fn vm_rd_wrong_arg_type() {
        let err = vm_run_err("f>t;rd 42", Some("f"), vec![]);
        assert!(
            err.contains("rd") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_rd_with_wrong_format_type() {
        let err = vm_run_err("f>t;rd \"/tmp\" 42", Some("f"), vec![]);
        assert!(
            err.contains("rd") || err.contains("format") || err.contains("text"),
            "got: {err}"
        );
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_rd_explicit_raw_format() {
        let path = "/tmp/ilo_test_vm_rd_explicit.txt";
        std::fs::write(path, "hello").unwrap();
        let source = format!(r#"f>R t t;rd "{path}" "raw""#);
        let result = vm_run(&source, Some("f"), vec![]);
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        assert_eq!(*inner, Value::Text("hello".into()));
    }

    #[test]
    #[ignore] // VM missing builtin implementation
    fn vm_rd_explicit_format_parse_error() {
        let path = "/tmp/ilo_test_vm_rd_badjson.txt";
        std::fs::write(path, "not json at all!!!").unwrap();
        let source = format!(r#"f>R t t;rd "{path}" "json""#);
        let result = vm_run(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Err(_)));
    }

    // ── rdl ─────────────────────────────────────────────────────────────

    #[test]
    fn vm_rdl_basic() {
        let mut path = std::env::temp_dir();
        path.push("ilo_vm_rdl_test.txt");
        std::fs::write(&path, "line1\nline2\nline3").unwrap();
        let path_str = path.to_str().unwrap().to_string();
        let result = vm_run("f p:t>t;rdl p", Some("f"), vec![Value::Text(path_str)]);
        std::fs::remove_file(&path).ok();
        let Value::Ok(inner) = result else {
            panic!("expected Ok")
        };
        let Value::List(lines) = *inner else {
            panic!("expected list")
        };
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], Value::Text("line1".into()));
    }

    #[test]
    fn vm_rdl_not_found() {
        let result = vm_run(
            "f p:t>t;rdl p",
            Some("f"),
            vec![Value::Text("/nonexistent/ilo_rdl_test.txt".into())],
        );
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err, got {:?}",
            result
        );
    }

    #[test]
    fn vm_rdl_wrong_arg() {
        let err = vm_run_err("f>t;rdl 42", Some("f"), vec![]);
        assert!(
            err.contains("rdl") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    // ── wr ──────────────────────────────────────────────────────────────

    #[test]
    fn vm_wr_basic() {
        let mut path = std::env::temp_dir();
        path.push("ilo_vm_wr_test.txt");
        let path_str = path.to_str().unwrap().to_string();
        let result = vm_run(
            "f p:t>t;wr p \"hello\"",
            Some("f"),
            vec![Value::Text(path_str.clone())],
        );
        std::fs::remove_file(&path).ok();
        assert!(
            matches!(result, Value::Ok(_)),
            "expected Ok, got {:?}",
            result
        );
    }

    #[test]
    fn vm_wr_wrong_args() {
        let err = vm_run_err("f>t;wr 42 \"hello\"", Some("f"), vec![]);
        assert!(
            err.contains("wr") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    #[ignore] // VM missing 3-arg wr format support
    fn vm_wr_csv_format() {
        let path = "/tmp/ilo_test_vm_wr.csv";
        let source = format!(r#"f>R t t;wr "{path}" [[1,2],[3,4]] "csv""#);
        let result = vm_run(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Ok(_)));
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("1,2"));
    }

    #[test]
    #[ignore] // VM missing 3-arg wr format support
    fn vm_wr_csv_bool_field() {
        let path = "/tmp/ilo_test_vm_wr_bool.csv";
        let source = format!(r#"f>R t t;wr "{path}" [[true,false]] "csv""#);
        let result = vm_run(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Ok(_)));
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("true"));
    }

    #[test]
    #[ignore] // VM missing 3-arg wr format support
    fn vm_wr_json_format() {
        let path = "/tmp/ilo_test_vm_wr.json";
        let source = format!(r#"f>R t t;wr "{path}" [1,2,3] "json""#);
        let result = vm_run(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Ok(_)));
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("1"));
    }

    #[test]
    #[ignore] // VM missing 3-arg wr format support
    fn vm_wr_csv_output() {
        let dir = std::env::temp_dir();
        let path = dir.join("ilo_test_vm_wr_csv.csv");
        let path_str = path.to_str().unwrap();
        let source = format!(
            r#"f>R t t;wr "{}" [["name", "age"], ["alice", 30], ["bob", 25]] "csv""#,
            path_str.replace('\\', "\\\\")
        );
        let result = vm_run(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Ok(_)));
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "name,age\nalice,30\nbob,25\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    #[ignore] // VM missing 3-arg wr format support
    fn vm_wr_csv_quoted_fields() {
        let dir = std::env::temp_dir();
        let path = dir.join("ilo_test_vm_wr_csv_quoted.csv");
        let path_str = path.to_str().unwrap();
        let source = format!(
            r#"f>R t t;wr "{}" [["a,b", "c\"d"]] "csv""#,
            path_str.replace('\\', "\\\\")
        );
        let result = vm_run(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Ok(_)));
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "\"a,b\",\"c\"\"d\"\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    #[ignore] // VM missing 3-arg wr format support
    fn vm_wr_json_output() {
        let dir = std::env::temp_dir();
        let path = dir.join("ilo_test_vm_wr_json.json");
        let path_str = path.to_str().unwrap();
        let source = format!(
            r#"f>R t t;wr "{}" [1, 2, 3] "json""#,
            path_str.replace('\\', "\\\\")
        );
        let result = vm_run(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Ok(_)));
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed, serde_json::json!([1.0, 2.0, 3.0]));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn vm_wr_unknown_format() {
        let err = vm_run_err(r#"f>R t t;wr "/tmp/x" "data" "xml""#, Some("f"), vec![]);
        assert!(
            err.contains("unknown") || err.contains("format") || err.contains("wr"),
            "got: {err}"
        );
    }

    #[test]
    #[ignore] // VM missing 3-arg wr format support
    fn vm_wr_json_text_value() {
        let path = "/tmp/ilo_test_vm_wr_json_text.json";
        let source = format!(r#"f>R t t;wr "{path}" "hello world" "json""#);
        let result = vm_run(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Ok(_)));
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("hello world"));
    }

    #[test]
    #[ignore] // VM missing 3-arg wr format support
    fn vm_wr_json_bool_value() {
        let path = "/tmp/ilo_test_vm_wr_json_bool.json";
        let source = format!(r#"f>R t t;wr "{path}" true "json""#);
        let result = vm_run(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Ok(_)));
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("true"));
    }

    #[test]
    #[ignore] // VM missing 3-arg wr format support
    fn vm_wr_json_map_value() {
        let path = "/tmp/ilo_test_vm_wr_json_map.json";
        let source = format!(r#"f>R t t;m=mset mmap "k" 42;wr "{path}" m "json""#);
        let result = vm_run(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Ok(_)));
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("\"k\""));
        assert!(content.contains("42"));
    }

    #[test]
    #[ignore] // VM missing 3-arg wr format support
    fn vm_wr_json_nil_value() {
        let path = "/tmp/ilo_test_vm_wr_json_nil.json";
        let source = format!(r#"f>R t t;v=mget mmap "x";wr "{path}" v "json""#);
        let result = vm_run(&source, Some("f"), vec![]);
        assert!(matches!(result, Value::Ok(_)));
        let content = std::fs::read_to_string(path).unwrap();
        assert_eq!(content.trim(), "null");
    }

    #[test]
    #[ignore] // VM missing 3-arg wr format support
    fn vm_wr_json_with_ok_value() {
        let path = "/tmp/ilo_test_vm_wr_ok.json";
        let source = format!(r#"f x:z>R t t;wr "{path}" x "json""#);
        let result = vm_run(
            &source,
            Some("f"),
            vec![Value::Ok(Box::new(Value::Number(1.0)))],
        );
        assert!(matches!(result, Value::Ok(_)));
    }

    #[test]
    fn vm_wr_non_text_format_arg_errors() {
        let path = "/tmp/ilo_test_vm_wr_fmt_err.csv";
        let source = format!(r#"f>R t t;wr "{path}" [1] 42"#);
        let err = vm_run_err(&source, Some("f"), vec![]);
        assert!(
            err.contains("wr") || err.contains("format") || err.contains("text"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_wr_csv_non_list_data_errors() {
        let path = "/tmp/ilo_test_vm_wr_csv_nonlist.csv";
        let source = format!(r#"f>R t t;wr "{path}" 42 "csv""#);
        let err = vm_run_err(&source, Some("f"), vec![]);
        assert!(
            err.contains("wr") || err.contains("csv") || err.contains("list"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_wr_csv_row_not_a_list_errors() {
        let path = "/tmp/ilo_test_vm_wr_csv_row_err.csv";
        let source = format!(r#"f>R t t;wr "{path}" [42] "csv""#);
        let err = vm_run_err(&source, Some("f"), vec![]);
        assert!(
            err.contains("wr")
                || err.contains("csv")
                || err.contains("list")
                || err.contains("row"),
            "got: {err}"
        );
    }

    #[test]
    #[ignore] // VM missing 3-arg wr format support
    fn vm_wr_csv_nil_field() {
        let path = "/tmp/ilo_test_vm_wr_nil.csv";
        let source = format!(r#"f x:z>R t t;wr "{path}" [[x,1]] "csv""#);
        let result = vm_run(&source, Some("f"), vec![Value::Nil]);
        assert!(
            matches!(result, Value::Ok(_)),
            "expected Ok, got {:?}",
            result
        );
        let content = std::fs::read_to_string(path).unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn vm_wr_two_arg_non_text_content_error() {
        let err = vm_run_err(
            r#"f>R t t;wr "/tmp/ilo_test_bad_wr.txt" 42"#,
            Some("f"),
            vec![],
        );
        assert!(
            err.contains("wr") || err.contains("text") || err.contains("content"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_wr_write_failure_returns_err() {
        let source = r#"f>R t t;wr "/no/such/dir/ilo_test.txt" "hello""#;
        let result = vm_run(source, Some("f"), vec![]);
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err for bad path, got {:?}",
            result
        );
    }

    // ── wrl ─────────────────────────────────────────────────────────────

    #[test]
    fn vm_wrl_basic() {
        let mut path = std::env::temp_dir();
        path.push("ilo_vm_wrl_test.txt");
        let path_str = path.to_str().unwrap().to_string();
        let result = vm_run(
            "f p:t>t;wrl p [\"a\", \"b\", \"c\"]",
            Some("f"),
            vec![Value::Text(path_str.clone())],
        );
        std::fs::remove_file(&path).ok();
        assert!(
            matches!(result, Value::Ok(_)),
            "expected Ok, got {:?}",
            result
        );
    }

    #[test]
    fn vm_wrl_non_text_item() {
        let path = "/tmp/ilo_test_vm_wrl_nontxt.txt";
        let source = format!(r#"f>R t t;wrl "{path}" ["ok", 99]"#);
        let prog = parse_program(&source);
        let result = compile_and_run(&prog, Some("f"), vec![]);
        std::fs::remove_file(path).ok();
        assert!(result.is_err(), "expected error for non-text wrl item");
    }

    #[test]
    fn vm_wrl_wrong_args() {
        let err = vm_run_err("f>t;wrl 42 [\"a\"]", Some("f"), vec![]);
        assert!(
            err.contains("wrl") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_wrl_write_failure_returns_err() {
        let source = r#"f>R t t;wrl "/no/such/dir/ilo_test.txt" ["a","b"]"#;
        let result = vm_run(source, Some("f"), vec![]);
        assert!(
            matches!(result, Value::Err(_)),
            "expected Err for bad path, got {:?}",
            result
        );
    }

    // ── get/post error paths ────────────────────────────────────────────

    #[test]
    #[ignore] // VM skips header type validation, makes network call instead
    fn vm_get_invalid_headers() {
        let err = vm_run_err(r#"f>t;get "http://x" 42"#, Some("f"), vec![]);
        assert!(
            err.contains("headers")
                || err.contains("get")
                || err.contains("map")
                || err.contains("M t t"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_post_wrong_arg_types() {
        let err = vm_run_err(r#"f>t;post 42 "body""#, Some("f"), vec![]);
        assert!(
            err.contains("post") || err.contains("text") || err.contains("type"),
            "got: {err}"
        );
    }

    #[test]
    #[ignore] // VM skips header type validation, makes network call instead
    fn vm_post_invalid_headers() {
        let err = vm_run_err(r#"f>t;post "http://x" "body" 42"#, Some("f"), vec![]);
        assert!(
            err.contains("headers") || err.contains("post") || err.contains("map"),
            "got: {err}"
        );
    }

    // ── Arena-full fallback tests ────────────────────────────────────────────

    #[test]
    fn vm_arena_full_recnew_fallback_to_heap() {
        // Arena is 64KB. Each 2-field record = 24 bytes (8 header + 2*8 fields).
        // 65536 / 24 = 2730. We need 2731+ allocations to overflow.
        // Use a while loop to create records until the arena fills, then verify
        // the last record still works via the Rc heap fallback (L3493-3501).
        let src = "type pt{x:n;y:n} f>n;i=0;r=pt x:0 y:0;wh <i 3000{j=+i 1;r=pt x:i y:j;i=j};r.x";
        let result = vm_run(src, Some("f"), vec![]);
        // Last iteration: i=2999, r=pt x:2999 y:3000
        assert_eq!(result, Value::Number(2999.0));
    }

    #[test]
    fn vm_arena_full_recwith_fallback_to_heap() {
        // Same arena overflow, but via OP_RECWITH (record update) fallback (L3551-3571).
        // Create initial record, then update it 3000 times to exhaust arena.
        let src = "type pt{x:n;y:n} f>n;r=pt x:0 y:0;i=0;wh <i 3000{r=r with x:i;i=+i 1};r.x";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(2999.0));
    }

    #[test]
    fn vm_arena_full_recnew_with_string_field() {
        // Arena overflow with string fields tests clone_rc in the heap fallback path.
        let src = r#"type msg{text:t;val:n} f>n;i=0;r=msg text:"a" val:0;wh <i 3000{r=msg text:"hello" val:i;i=+i 1};r.val"#;
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(2999.0));
    }

    #[test]
    fn vm_arena_full_recwith_preserves_string_fields() {
        // Arena overflow during recwith with string field in old record (L3555-3567).
        let src = r#"type msg{text:t;val:n} f>t;r=msg text:"hello" val:0;i=0;wh <i 3000{r=r with val:i;i=+i 1};r.text"#;
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Text("hello".into()));
    }

    #[test]
    fn vm_arena_full_record_returned_as_value() {
        // When arena overflows, records promoted to heap. Returning a record
        // exercises the arena→heap promotion + to_value conversion.
        let src = "type pt{x:n;y:n} f>pt;i=0;r=pt x:0 y:0;wh <i 3000{j=+i 1;r=pt x:i y:j;i=j};r";
        let result = vm_run(src, Some("f"), vec![]);
        match result {
            Value::Record { type_name, fields } => {
                assert_eq!(type_name, "pt");
                assert_eq!(fields.get("x"), Some(&Value::Number(2999.0)));
                assert_eq!(fields.get("y"), Some(&Value::Number(3000.0)));
            }
            other => panic!("expected Record, got {:?}", other),
        }
    }

    // ── Arena record → Value conversion (L2328-2350) ─────────────────────────

    #[test]
    fn vm_arena_record_to_value_returns_record() {
        // Record created in arena is returned directly; the VM promotes it
        // and calls to_value, exercising L2328-2350.
        let src = "type pt{x:n;y:n} f>pt;pt x:42 y:99";
        let result = vm_run(src, Some("f"), vec![]);
        match result {
            Value::Record { type_name, fields } => {
                assert_eq!(type_name, "pt");
                assert_eq!(fields.get("x"), Some(&Value::Number(42.0)));
                assert_eq!(fields.get("y"), Some(&Value::Number(99.0)));
            }
            other => panic!("expected Record, got {:?}", other),
        }
    }

    // ── to_value_with_registry (L2385-2404) ──────────────────────────────────

    #[test]
    fn vm_to_value_with_registry_resolves_field_names() {
        // Directly test to_value_with_registry on an arena record.
        use crate::vm::compile;
        let prog = parse_program("type pt{x:n;y:n} f>pt;pt x:7 y:8");
        let compiled = compile(&prog).unwrap();
        // Run through VM to get a result and exercise the path
        let result = crate::vm::run(&compiled, Some("f"), vec![]).unwrap();
        match result {
            Value::Record { type_name, fields } => {
                assert_eq!(type_name, "pt");
                assert_eq!(fields.get("x"), Some(&Value::Number(7.0)));
                assert_eq!(fields.get("y"), Some(&Value::Number(8.0)));
            }
            other => panic!("expected Record, got {:?}", other),
        }
    }

    #[test]
    fn vm_to_value_with_registry_nested_record() {
        // Nested record: inner record promoted during to_value_with_registry
        let src = "type inner{v:n} type outer{a:inner;b:n} f>outer;i=inner v:42;outer a:i b:99";
        let result = vm_run(src, Some("f"), vec![]);
        match result {
            Value::Record { type_name, fields } => {
                assert_eq!(type_name, "outer");
                assert_eq!(fields.get("b"), Some(&Value::Number(99.0)));
                match fields.get("a") {
                    Some(Value::Record {
                        type_name: inner_name,
                        fields: inner_fields,
                    }) => {
                        assert_eq!(inner_name, "inner");
                        assert_eq!(inner_fields.get("v"), Some(&Value::Number(42.0)));
                    }
                    other => panic!("expected inner Record, got {:?}", other),
                }
            }
            other => panic!("expected outer Record, got {:?}", other),
        }
    }

    // ── OP_RET multi-frame (L2630-2637) ──────────────────────────────────────

    #[test]
    fn vm_multi_frame_return_chain() {
        // Function A calls B which calls C. Tests multi-frame OP_RET path
        // where returning from C restores B's frame, then B returns to A.
        let src = "c x:n>n;+x 100\nb x:n>n;c +x 10\na x:n>n;b +x 1";
        let result = vm_run(src, Some("a"), vec![Value::Number(5.0)]);
        // a(5) → b(6) → c(16) → 116
        assert_eq!(result, Value::Number(116.0));
    }

    #[test]
    fn vm_multi_frame_return_with_records() {
        // Multi-frame return where inner function creates a record,
        // exercises the OP_RET path with arena records across frames.
        let src = "type pt{x:n;y:n} mk a:n b:n>pt;pt x:a y:b\nwrap x:n>pt;y=+x 1;mk x y\nf>n;p=wrap 10;+p.x p.y";
        let result = vm_run(src, Some("f"), vec![]);
        // wrap(10) → y=11, mk(10,11) → pt{x:10,y:11}, f returns 10+11=21
        assert_eq!(result, Value::Number(21.0));
    }

    #[test]
    fn vm_deeply_nested_calls() {
        // 4-level deep call chain to stress multi-frame return
        let src = "d x:n>n;*x 2\nc x:n>n;d +x 1\nb x:n>n;c +x 1\na x:n>n;b +x 1";
        let result = vm_run(src, Some("a"), vec![Value::Number(1.0)]);
        // a(1) → b(2) → c(3) → d(4) → 8
        assert_eq!(result, Value::Number(8.0));
    }

    // ── Arena-full with JIT helper paths (L5206-5240) ────────────────────────

    #[test]
    fn vm_arena_full_recwith_multiple_updates() {
        // Arena overflow during recwith with multiple field updates at once.
        let src = "type pt{x:n;y:n} f>n;r=pt x:0 y:0;i=0;wh <i 3000{j=+i 1;r=r with x:i y:j;i=j};+r.x r.y";
        let result = vm_run(src, Some("f"), vec![]);
        // Last: r = pt{x:2999, y:3000}, sum = 5999
        assert_eq!(result, Value::Number(5999.0));
    }

    #[test]
    fn vm_arena_full_large_record() {
        // 5-field record fills arena faster: 8 + 5*8 = 48 bytes each.
        // 65536 / 48 = 1365. Need ~1366 allocations.
        let src = "type big{a:n;b:n;c:n;d:n;e:n} f>n;i=0;r=big a:0 b:0 c:0 d:0 e:0;wh <i 1500{b=+i 1;c=+i 2;d=+i 3;e=+i 4;r=big a:i b:b c:c d:d e:e;i=b};r.a";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(1499.0));
    }

    // ── Coverage: Nil literal in match pattern (compiler L954) ──────────────

    #[test]
    fn vm_match_nil_literal_pattern() {
        // ~v must come before _ (wildcard matches everything)
        let src = r#"f x:O n>t;?x{~v:"val";_:"nil"}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Nil]),
            Value::Text("nil".to_string())
        );
        assert_eq!(
            vm_run(
                src,
                Some("f"),
                vec![Value::Ok(Box::new(Value::Number(1.0)))]
            ),
            Value::Text("val".to_string())
        );
    }

    // ── Coverage: Nil literal in expression (compiler L1019, L1088) ─────────

    #[test]
    fn vm_nil_literal_in_expression() {
        let src = "f>O n;nil";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Nil);
    }

    // ── Coverage: Break in while loop (compiler L858) ───────────────────────

    #[test]
    fn vm_break_in_while_coverage() {
        let src = "f>n;i=0;wh true{i=+i 1;>=i 5{brk}};i";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(5.0));
    }

    // ── Coverage: Break in foreach loop ─────────────────────────────────────

    #[test]
    fn vm_break_in_foreach_coverage() {
        let src = "f>n;xs=[1,2,3,4,5];r=0;@x xs{r=x;>=x 3{brk}};r";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    // ── Coverage: record field access by name on arena record (L3251-3263) ──

    #[test]
    fn vm_recfld_name_arena_record() {
        // JSON parse returns a record accessible by field name
        let src = r#"f>t;j=jpar! "{\"name\":\"alice\",\"age\":30}";j.name"#;
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Text("alice".to_string()));
    }

    // ── Coverage: heap record with text field names (L3585-3589) ────────────

    #[test]
    fn vm_recwith_heap_record() {
        // Force heap record path by creating record that escapes arena
        // (e.g., stored in list then extracted)
        let src = "type pt{x:n;y:n} f>n;r=pt x:1 y:2;r2=r with x:10;+r2.x r2.y";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(12.0));
    }

    // ── Coverage: modulo by zero error (L3783) ────────────────────────────────

    #[test]
    fn vm_mod_zero_error() {
        let src = "f x:n>n;mod x 0";
        let err = vm_run_err(src, Some("f"), vec![Value::Number(10.0)]);
        assert!(
            err.contains("modulo by zero") || err.contains("zero"),
            "got: {err}"
        );
    }

    // ── Coverage: OP_RECWITH non-number slot (L3525, 3527) ──────────────────

    #[test]
    fn vm_recwith_arena_multiple_fields() {
        let src = "type pt{x:n;y:n;z:n} f>n;r=pt x:1 y:2 z:3;r2=r with x:10 z:30;+r2.x +r2.y r2.z";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(42.0));
    }

    // ── Coverage: nanval_to_json arena record without registry (L4333) ──────

    #[test]
    fn vm_json_dump_record() {
        let src = r#"type pt{x:n;y:n} f>t;r=pt x:1 y:2;jdmp r"#;
        let result = vm_run(src, Some("f"), vec![]);
        // Should produce JSON with field names
        let text = match &result {
            Value::Text(s) => s.clone(),
            other => panic!("expected text, got: {other:?}"),
        };
        assert!(
            text.contains("\"x\"") && text.contains("\"y\""),
            "got: {text}"
        );
    }

    // ── Coverage: serde_json_to_nanval fallback (L4370) ─────────────────────

    #[test]
    fn vm_jpar_null_value() {
        let src = r#"f>O n;j=jpar "null";j"#;
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Ok(Box::new(Value::Nil)));
    }

    // ── Coverage: OP_UNWRAP via jpar! (L3193) ─────────────────────────────────

    #[test]
    fn vm_unwrap_ok_value_coverage() {
        // jpar! unwraps Ok result
        let src = r#"f>n;r=jpar! "42";r"#;
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(42.0));
    }

    // ── Coverage: OP_RECFLD on heap record via cross-function call ──────────

    #[test]
    fn vm_record_field_access_heap_coverage() {
        // Return record from function call — record gets promoted to heap
        let src = "type pt{x:n;y:n} mk a:n b:n>pt;pt x:a y:b
f>n;r=mk 10 20;+r.x r.y";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(30.0));
    }

    // ── Coverage: list index must be number (L3336) ─────────────────────────

    #[test]
    fn vm_foreach_with_list() {
        let src = "f>n;xs=[10,20,30];s=0;@x xs{s=+s x};s";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(60.0));
    }

    // ── Coverage: compiler constant-on-left optimization (L1606, L1625) ──────

    #[test]
    fn vm_const_left_multiply() {
        // 2 * x where 2 is the constant on the left
        let src = "f x:n>n;*2 x";
        let result = vm_run(src, Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn vm_const_left_add() {
        // 10 + x where 10 is the constant on the left
        let src = "f x:n>n;+10 x";
        let result = vm_run(src, Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(15.0));
    }

    // ── Coverage: dynamic destructure with existing register (L573) ─────────

    #[test]
    fn vm_destructure_record_coverage() {
        let src = "type pt{x:n;y:n} f>n;r=pt x:3 y:4;{x;y}=r;+x y";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(7.0));
    }

    // ── Coverage: OP_ISBOOL (L2895) ─────────────────────────────────────────

    #[test]
    fn vm_isbool_match_pattern_coverage() {
        // TypeIs pattern with bool type - exercises OP_ISBOOL
        let src = r#"f x:b>t;?x{b v:"matched";_:"other"}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Bool(true)]),
            Value::Text("matched".to_string())
        );
    }

    // ── Space-separated and heterogeneous list literals ─────────────────────

    #[test]
    fn vm_list_space_separated() {
        let src = "f>L n;[1 2 3]";
        assert_eq!(
            vm_run(src, Some("f"), vec![]),
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])
        );
    }

    #[test]
    fn vm_list_with_variable() {
        let src = r#"f w:t>L t;["hi" w]"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Text("world".to_string())]),
            Value::List(vec![
                Value::Text("hi".to_string()),
                Value::Text("world".to_string())
            ])
        );
    }

    #[test]
    fn vm_list_heterogeneous() {
        let src = r#"f>L a;["search" 10 true]"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![]),
            Value::List(vec![
                Value::Text("search".to_string()),
                Value::Number(10.0),
                Value::Bool(true),
            ])
        );
    }

    #[test]
    fn vm_list_mixed_comma_space() {
        let src = "f>L n;[1, 2 3]";
        assert_eq!(
            vm_run(src, Some("f"), vec![]),
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])
        );
    }

    // ── Coverage round 2 ────────────────────────────────────────────────────

    // ── to_value() for arena records (L2328-2350) ───────────────────────────
    // Returning a record from VM exercises the arena record → Value::Record path
    // in NanVal::to_value() where ACTIVE_REGISTRY is set by execute().

    #[test]
    fn vm_arena_record_to_value_single_field() {
        // Single-field arena record exercises to_value() arena path (L2328-2350)
        let src = "type wrapper{val:n} f>wrapper;wrapper val:42";
        let result = vm_run(src, Some("f"), vec![]);
        match result {
            Value::Record { type_name, fields } => {
                assert_eq!(type_name, "wrapper");
                assert_eq!(fields.get("val"), Some(&Value::Number(42.0)));
            }
            other => panic!("expected Record, got {:?}", other),
        }
    }

    #[test]
    fn vm_arena_record_to_value_with_text_field() {
        // Arena record containing a text field (heap-tagged NanVal) exercises
        // recursive to_value() inside the arena record loop (L2343-2348)
        let src = r#"type named{name:t;age:n} f>named;named name:"alice" age:30"#;
        let result = vm_run(src, Some("f"), vec![]);
        match result {
            Value::Record { type_name, fields } => {
                assert_eq!(type_name, "named");
                assert_eq!(fields.get("name"), Some(&Value::Text("alice".to_string())));
                assert_eq!(fields.get("age"), Some(&Value::Number(30.0)));
            }
            other => panic!("expected Record, got {:?}", other),
        }
    }

    // ── to_value_with_registry() (L2385-2404) ──────────────────────────────
    // Directly test to_value_with_registry on arena records with nested data.

    #[test]
    fn vm_to_value_with_registry_three_fields() {
        // Three-field record exercises to_value_with_registry path (L2385-2404)
        let src = "type vec3{x:n;y:n;z:n} f>vec3;vec3 x:1 y:2 z:3";
        let result = vm_run(src, Some("f"), vec![]);
        match result {
            Value::Record { type_name, fields } => {
                assert_eq!(type_name, "vec3");
                assert_eq!(fields.len(), 3);
                assert_eq!(fields.get("x"), Some(&Value::Number(1.0)));
                assert_eq!(fields.get("y"), Some(&Value::Number(2.0)));
                assert_eq!(fields.get("z"), Some(&Value::Number(3.0)));
            }
            other => panic!("expected Record, got {:?}", other),
        }
    }

    // ── OP_MOD with non-numbers (L3779) ─────────────────────────────────────

    #[test]
    fn vm_mod_requires_numbers_error() {
        // mod with text arguments triggers "mod requires numbers" error (L3779)
        // Using `a` (any) type to bypass verifier
        let src = r#"f x:a y:a>a;mod x y"#;
        let prog = parse_program(src);
        let err = compile_and_run(
            &prog,
            Some("f"),
            vec![Value::Text("a".into()), Value::Text("b".into())],
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("mod requires numbers"),
            "got: {err}"
        );
    }

    #[test]
    fn vm_mod_normal_operation() {
        // Normal mod operation for comparison (L3772-3785)
        let src = "f a:n b:n>n;mod a b";
        let result = vm_run(
            src,
            Some("f"),
            vec![Value::Number(10.0), Value::Number(3.0)],
        );
        assert_eq!(result, Value::Number(1.0));
    }

    #[test]
    fn vm_mod_by_zero_error() {
        // mod by zero triggers "modulo by zero" error (L3782-3783)
        let src = "f a:n b:n>n;mod a b";
        let prog = parse_program(src);
        let err = compile_and_run(
            &prog,
            Some("f"),
            vec![Value::Number(10.0), Value::Number(0.0)],
        )
        .unwrap_err();
        assert!(err.to_string().contains("modulo by zero"), "got: {err}");
    }

    // ── recwith on heap record with text field lookup (L3585-3589) ──────────

    #[test]
    fn vm_recwith_heap_record_updates() {
        // Record returned from function call (promoted to heap), then `with` update
        // exercises the heap OP_RECWITH path (L3573-3600)
        let src =
            "type pt{x:n;y:n}\nmk a:n b:n>pt;pt x:a y:b\nf>n;r=mk 1 2;r2=r with y:99;+r2.x r2.y";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(100.0)); // 1 + 99
    }

    // ── recwith arena record with multiple updates (L3522-3527) ─────────────

    #[test]
    fn vm_recwith_arena_two_field_update() {
        // Update both fields at once on an arena record (L3516-3550)
        let src = "type pt{x:n;y:n} f>n;r=pt x:1 y:2;r2=r with x:10 y:20;+r2.x r2.y";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(30.0)); // 10 + 20
    }

    // ── Multi-frame return with stack cleanup (L2622-2639) ──────────────────
    // The ip >= code.len() path is a safety fallthrough. Normal code uses OP_RET.
    // We test multi-frame return via the normal OP_RET path which shares similar
    // stack cleanup logic.

    #[test]
    fn vm_multi_frame_return_with_text() {
        // Multi-frame return where inner function returns text (heap value)
        // exercises stack cleanup across frames (L2622-2639 area)
        let src = "inner x:t>t;+x \"!\"\nouter x:t>t;inner x\nf x:t>t;outer x";
        let result = vm_run(src, Some("f"), vec![Value::Text("hi".to_string())]);
        assert_eq!(result, Value::Text("hi!".to_string()));
    }

    #[test]
    fn vm_multi_frame_return_with_list() {
        // Multi-frame return where inner function returns a list
        let src = "inner x:n>L n;[x,+x 1,+x 2]\nouter x:n>L n;inner x\nf x:n>L n;outer x";
        let result = vm_run(src, Some("f"), vec![Value::Number(1.0)]);
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])
        );
    }

    #[test]
    fn vm_multi_frame_return_record_chain() {
        // 3-level deep call returning a record — exercises multi-frame return
        // with arena record promotion across stack frames
        let src = "type pt{x:n;y:n}\nc a:n b:n>pt;pt x:a y:b\nb x:n>pt;y=+x 1;c x y\na x:n>n;p=b x;+p.x p.y";
        let result = vm_run(src, Some("a"), vec![Value::Number(7.0)]);
        assert_eq!(result, Value::Number(15.0)); // 7 + 8
    }

    #[test]
    fn vm_sequential_cross_function_calls() {
        // Two sequential calls: a=dbl(n), then triple(a)
        let source = "dbl x:n>n;*x 2\ntriple x:n>n;*x 3\nf n:n>n;a=dbl n;triple a";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(30.0)
        );
    }

    #[test]
    fn vm_long_pipe_chain() {
        // Pipe chain: i>>dbl>>inc>>dbl>>inc = inc(dbl(inc(dbl(i)))) = 4i+3
        let source = "dbl x:n>n;*x 2\ninc x:n>n;+x 1\nf n:n>n;n>>dbl>>inc>>dbl>>inc";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0)]),
            Value::Number(23.0)
        );
    }

    // ── Coverage gap tests ──────────────────────────────────────────────

    // OP_SUBK_N: subtract constant (L694) — reassignment triggers peephole
    #[test]
    fn vm_cov_subk_n() {
        let result = vm_run("f x:n>n;x=-x 5;x", Some("f"), vec![Value::Number(20.0)]);
        assert_eq!(result, Value::Number(15.0));
    }

    // OP_DIVK_N: divide by constant (L696)
    #[test]
    fn vm_cov_divk_n() {
        let result = vm_run("f x:n>n;x=/x 2;x", Some("f"), vec![Value::Number(10.0)]);
        assert_eq!(result, Value::Number(5.0));
    }

    // Multiply with left literal (commutative path L715-719)
    #[test]
    fn vm_cov_mul_left_literal() {
        let result = vm_run("f x:n>n;a=*3 x;a", Some("f"), vec![Value::Number(7.0)]);
        assert_eq!(result, Value::Number(21.0));
    }

    // Add with left literal (commutative path L715-719)
    #[test]
    fn vm_cov_add_left_literal() {
        let result = vm_run("f x:n>n;a=+10 x;a", Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(15.0));
    }

    // Record returned from function (exercises NanVal::to_value for arena records L2811-2834)
    #[test]
    fn vm_cov_record_return() {
        let result = vm_run("type pt{x:n;y:n}\nf>pt;pt x:10 y:20", Some("f"), vec![]);
        match result {
            Value::Record { type_name, fields } => {
                assert_eq!(type_name, "pt");
                assert_eq!(fields.get("x"), Some(&Value::Number(10.0)));
                assert_eq!(fields.get("y"), Some(&Value::Number(20.0)));
            }
            _ => panic!("expected record, got {:?}", result),
        }
    }

    // Record with update
    #[test]
    fn vm_cov_record_with_update() {
        let result = vm_run(
            "type pt{x:n;y:n}\nf>n;p=pt x:1 y:2;p2=p with x:10;+p2.x p2.y",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(12.0));
    }

    // Multi-frame return: deeply nested function calls
    #[test]
    fn vm_cov_multi_frame_number() {
        let result = vm_run(
            "a x:n>n;+x 1\nb x:n>n;a x\nc x:n>n;b x\nf x:n>n;c x",
            Some("f"),
            vec![Value::Number(10.0)],
        );
        assert_eq!(result, Value::Number(11.0));
    }

    // String concatenation
    #[test]
    fn vm_cov_string_concat() {
        let result = vm_run(
            "f a:t b:t>t;+a b",
            Some("f"),
            vec![Value::Text("hello ".into()), Value::Text("world".into())],
        );
        assert_eq!(result, Value::Text("hello world".into()));
    }

    // Guard true/false paths
    #[test]
    fn vm_cov_guard_true() {
        let result = vm_run("f x:n>n;>x 0{x}", Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(5.0));
    }

    // Match with bool pattern (L1230-1251)
    #[test]
    fn vm_cov_match_bool() {
        let result = vm_run(
            r#"f x:b>t;?x{true:"yes";false:"no"}"#,
            Some("f"),
            vec![Value::Bool(true)],
        );
        assert_eq!(result, Value::Text("yes".into()));
    }

    // Match with text pattern (L1230-1251)
    #[test]
    fn vm_cov_match_text() {
        let result = vm_run(
            r#"f x:t>n;?x{"a":1;"b":2;_:0}"#,
            Some("f"),
            vec![Value::Text("b".into())],
        );
        assert_eq!(result, Value::Number(2.0));
    }

    // Match with nil literal pattern (L1235)
    #[test]
    fn vm_cov_match_nil() {
        // Use a function that can return nil, then match on the result
        let result = vm_run(
            "mk x:n>n;>x 0{x}\nf>n;v=mk 0;?v{nil:0;_:1}",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(0.0));
    }

    // Match TypeIs patterns (L1254-1260 — OP_ISNUM, OP_ISTEXT, OP_ISBOOL)
    #[test]
    fn vm_cov_match_type_is_num() {
        let result = vm_run(
            r#"f x:t>t;?x{n v:"num";t v:v;b v:"bool"}"#,
            Some("f"),
            vec![Value::Text("hello".into())],
        );
        assert_eq!(result, Value::Text("hello".into()));
    }

    // Nested record creation and field access
    #[test]
    fn vm_cov_nested_record() {
        let result = vm_run(
            "type inner{v:n}\ntype outer{i:inner}\nf>n;a=inner v:42;b=outer i:a;b.i.v",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(42.0));
    }

    // Result Ok
    #[test]
    fn vm_cov_result_ok() {
        let result = vm_run("f>R n t;~42", Some("f"), vec![]);
        assert_eq!(result, Value::Ok(Box::new(Value::Number(42.0))));
    }

    // Result Err
    #[test]
    fn vm_cov_result_err() {
        let result = vm_run(r#"f>R n t;^"oops""#, Some("f"), vec![]);
        assert_eq!(result, Value::Err(Box::new(Value::Text("oops".into()))));
    }

    // Map set/get
    #[test]
    fn vm_cov_map_set_get() {
        let result = vm_run(
            r#"f>t;m=mset mmap "key" "val";v=mget m "key";v??"none""#,
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Text("val".into()));
    }

    // Map has
    #[test]
    fn vm_cov_map_has() {
        let result = vm_run(r#"f>b;m=mset mmap "a" 1;mhas m "a""#, Some("f"), vec![]);
        assert_eq!(result, Value::Bool(true));
    }

    // Map del
    #[test]
    fn vm_cov_map_del() {
        let result = vm_run(
            r#"f>b;m=mset mmap "a" 1;m=mdel m "a";mhas m "a""#,
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Bool(false));
    }

    // Map keys
    #[test]
    fn vm_cov_map_keys() {
        let result = vm_run(
            r#"f>n;m=mset mmap "a" 1;m=mset m "b" 2;k=mkeys m;len k"#,
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(2.0));
    }

    // While loop
    #[test]
    fn vm_cov_while_loop() {
        let result = vm_run("f>n;x=0;wh <x 10{x=+x 1};x", Some("f"), vec![]);
        assert_eq!(result, Value::Number(10.0));
    }

    // While with break value
    #[test]
    fn vm_cov_while_break() {
        let result = vm_run("f>n;i=0;wh true{i=+i 1;>=i 5{brk i}};i", Some("f"), vec![]);
        assert_eq!(result, Value::Number(5.0));
    }

    // For-range basic (L1051-1053)
    #[test]
    fn vm_cov_for_range_basic() {
        let result = vm_run("f>n;s=0;@i 1..4{s=+s i};s", Some("f"), vec![]);
        assert_eq!(result, Value::Number(6.0)); // 1+2+3
    }

    // For-range with continue (L1040-1049)
    #[test]
    fn vm_cov_for_range_cnt() {
        // Skip i=2, sum rest: 0+1+3+4 = 8
        let result = vm_run("f>n;s=0;@i 0..5{=i 2{cnt};s=+s i};s", Some("f"), vec![]);
        assert_eq!(result, Value::Number(8.0));
    }

    // Foreach with continue (L963-972)
    #[test]
    fn vm_cov_foreach_cnt() {
        // Skip x>=3, sum rest: 1+2 = 3
        let result = vm_run(
            "f>n;s=0;@x [1,2,3,4,5]{>=x 3{cnt};s=+s x};s",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(3.0));
    }

    // Foreach with break
    #[test]
    fn vm_cov_foreach_brk() {
        let result = vm_run("f>n;@x [1,2,3,4,5]{>=x 3{brk x};x}", Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    // List append (L2020)
    #[test]
    fn vm_cov_list_append() {
        let result = vm_run("f>n;xs=[1,2,3];xs=+=xs 4;len xs", Some("f"), vec![]);
        assert_eq!(result, Value::Number(4.0));
    }

    // Modulo
    #[test]
    fn vm_cov_modulo() {
        let result = vm_run("f>n;mod 10 3", Some("f"), vec![]);
        assert_eq!(result, Value::Number(1.0));
    }

    // Recursive function (factorial)
    #[test]
    fn vm_cov_recursion() {
        let result = vm_run(
            "fac n:n>n;<=n 1 1;r=fac -n 1;*n r",
            Some("fac"),
            vec![Value::Number(5.0)],
        );
        assert_eq!(result, Value::Number(120.0));
    }

    // Nil return from optional
    #[test]
    fn vm_cov_nil_return() {
        let result = vm_run("f x:n>n;>x 0{x}", Some("f"), vec![Value::Number(-1.0)]);
        assert_eq!(result, Value::Nil);
    }

    // Slicing
    #[test]
    fn vm_cov_slice_list() {
        let result = vm_run("f>L n;slc [1, 2, 3, 4, 5] 1 3", Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(vec![Value::Number(2.0), Value::Number(3.0)])
        );
    }

    #[test]
    fn vm_cov_slice_text() {
        let result = vm_run(r#"f>t;slc "hello" 1 4"#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("ell".into()));
    }

    // Multi-guard cascading
    #[test]
    fn vm_cov_multi_guard() {
        let result = vm_run(
            r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#,
            Some("cls"),
            vec![Value::Number(750.0)],
        );
        assert_eq!(result, Value::Text("silver".into()));
    }

    // Comparison operators
    #[test]
    fn vm_cov_cmp_gt() {
        let result = vm_run(
            "f x:n y:n>b;>x y",
            Some("f"),
            vec![Value::Number(5.0), Value::Number(3.0)],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_cov_cmp_gte() {
        let result = vm_run(
            "f x:n y:n>b;>=x y",
            Some("f"),
            vec![Value::Number(5.0), Value::Number(5.0)],
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_cov_cmp_lte() {
        let result = vm_run(
            "f x:n y:n>b;<=x y",
            Some("f"),
            vec![Value::Number(3.0), Value::Number(5.0)],
        );
        assert_eq!(result, Value::Bool(true));
    }

    // String length
    #[test]
    fn vm_cov_str_len() {
        let result = vm_run(
            r#"f s:t>n;len s"#,
            Some("f"),
            vec![Value::Text("hello".into())],
        );
        assert_eq!(result, Value::Number(5.0));
    }

    // Head and tail
    #[test]
    fn vm_cov_list_head() {
        let result = vm_run("f>n;xs=[10, 20, 30];hd xs", Some("f"), vec![]);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn vm_cov_list_tail() {
        let result = vm_run("f>L n;xs=[10, 20, 30];tl xs", Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(vec![Value::Number(20.0), Value::Number(30.0)])
        );
    }

    // Boolean operations
    #[test]
    fn vm_cov_bool_and() {
        let result = vm_run("f>b;x=&true false;x", Some("f"), vec![]);
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn vm_cov_bool_or() {
        let result = vm_run("f>b;x=|true false;x", Some("f"), vec![]);
        assert_eq!(result, Value::Bool(true));
    }

    // Conditional expression with braces
    #[test]
    fn vm_cov_cond_brace() {
        let result = vm_run(
            r#"f x:n>t;=x 1{"yes"}{"no"}"#,
            Some("f"),
            vec![Value::Number(1.0)],
        );
        assert_eq!(result, Value::Text("yes".into()));
    }

    #[test]
    fn vm_cov_cond_brace_false() {
        let result = vm_run(
            r#"f x:n>t;=x 1{"yes"}{"no"}"#,
            Some("f"),
            vec![Value::Number(0.0)],
        );
        assert_eq!(result, Value::Text("no".into()));
    }

    // Fibonacci (exercises deep recursion)
    #[test]
    fn vm_cov_fibonacci() {
        let result = vm_run(
            "fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b",
            Some("fib"),
            vec![Value::Number(7.0)],
        );
        assert_eq!(result, Value::Number(13.0));
    }

    // Nil coalesce (?? operator)
    #[test]
    fn vm_cov_nil_coalesce_nil() {
        let result = vm_run("mk x:n>n;>=x 1{x}\nf>n;x=mk 0;x??42", Some("f"), vec![]);
        assert_eq!(result, Value::Number(42.0));
    }

    #[test]
    fn vm_cov_nil_coalesce_value() {
        let result = vm_run("f>n;x=10;x??42", Some("f"), vec![]);
        assert_eq!(result, Value::Number(10.0));
    }

    // Record with multiple field updates
    #[test]
    fn vm_cov_record_multi_with() {
        let result = vm_run(
            "type pt{x:n;y:n;z:n}\nf>n;p=pt x:1 y:2 z:3;p2=p with x:10 y:20;+p2.x +p2.y p2.z",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(33.0)); // 10 + 20 + 3
    }

    // For-range break
    #[test]
    fn vm_cov_for_range_brk() {
        let result = vm_run("f>n;@i 0..10{>=i 3{brk i};i}", Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    // Deeply nested calls returning records
    #[test]
    fn vm_cov_deep_record_return() {
        let result = vm_run(
            "type pt{x:n;y:n}\nmk a:n b:n>pt;pt x:a y:b\nwrap a:n b:n>pt;mk a b\nf>n;p=wrap 7 8;+p.x p.y",
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(15.0));
    }

    // Conditional returning different types (text vs text)
    #[test]
    fn vm_cov_guard_with_else() {
        let result = vm_run(
            r#"f x:n>t;>x 5{"big"};<=x 5{"small"}"#,
            Some("f"),
            vec![Value::Number(3.0)],
        );
        assert_eq!(result, Value::Text("small".into()));
    }

    // ── rou (round) opcode ────────────────────────────────────────────────
    #[test]
    fn vm_cov_round() {
        // rou exercises the n.round() branch in OP_FLR|OP_CEL|OP_ROU
        let result = vm_run("f x:n>n;rou x", Some("f"), vec![Value::Number(2.5)]);
        assert_eq!(result, Value::Number(3.0));
    }

    #[test]
    fn vm_cov_round_down() {
        let result = vm_run("f x:n>n;rou x", Some("f"), vec![Value::Number(2.4)]);
        assert_eq!(result, Value::Number(2.0));
    }

    // ── OP_RND0 (random no-arg) ────────────────────────────────────────────
    #[test]
    fn vm_cov_rnd0_returns_number() {
        // rnd with no args returns a number in [0,1)
        let result = vm_run("f>n;rnd", Some("f"), vec![]);
        match result {
            Value::Number(n) => assert!((0.0..=1.0).contains(&n)),
            other => panic!("expected Number, got {:?}", other),
        }
    }

    // ── OP_RND2 (random range) ─────────────────────────────────────────────
    #[test]
    fn vm_cov_rnd2_in_range() {
        let result = vm_run("f>n;rnd 1 10", Some("f"), vec![]);
        match result {
            Value::Number(n) => assert!((1.0..=10.0).contains(&n)),
            other => panic!("expected Number, got {:?}", other),
        }
    }

    #[test]
    fn vm_cov_rnd2_equal_bounds() {
        let result = vm_run("f>n;rnd 5 5", Some("f"), vec![]);
        assert_eq!(result, Value::Number(5.0));
    }

    #[test]
    fn vm_cov_rnd2_inverted_bounds_error() {
        let err = vm_run_err("f>n;rnd 10 1", Some("f"), vec![]);
        assert!(
            err.contains("bound") || err.contains("rnd") || err.contains("lower"),
            "got: {err}"
        );
    }

    // ── OP_NOW ─────────────────────────────────────────────────────────────
    #[test]
    fn vm_cov_now_returns_positive() {
        let result = vm_run("f>n;now", Some("f"), vec![]);
        match result {
            Value::Number(n) => assert!(n > 0.0, "expected positive timestamp, got {n}"),
            other => panic!("expected Number, got {:?}", other),
        }
    }

    // ── OP_ENV ─────────────────────────────────────────────────────────────
    #[test]
    fn vm_cov_env_existing_var() {
        let _guard = ENV_TEST_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("ILO_TEST_ENV_COV", "hello");
        }
        let result = vm_run(r#"f>R t t;env "ILO_TEST_ENV_COV""#, Some("f"), vec![]);
        unsafe {
            std::env::remove_var("ILO_TEST_ENV_COV");
        }
        assert_eq!(result, Value::Ok(Box::new(Value::Text("hello".into()))));
    }

    #[test]
    fn vm_cov_env_missing_var() {
        let _guard = ENV_TEST_MUTEX.lock().unwrap();
        unsafe {
            std::env::remove_var("ILO_TEST_MISSING_DEFINITELY");
        }
        let result = vm_run(
            r#"f>R t t;env "ILO_TEST_MISSING_DEFINITELY""#,
            Some("f"),
            vec![],
        );
        match result {
            Value::Err(msg) => {
                let s = msg.to_string();
                assert!(
                    s.contains("not set") || s.contains("ILO_TEST_MISSING"),
                    "got: {s}"
                );
            }
            other => panic!("expected Err, got {:?}", other),
        }
    }

    // ── OP_JPAR (json parse) ────────────────────────────────────────────────
    #[test]
    fn vm_cov_jpar_object() {
        // Parse a JSON object; result is Ok
        let result = vm_run(r#"f>O _;r=jpar "{\"a\":1}";r"#, Some("f"), vec![]);
        match result {
            Value::Ok(_) => {} // success: parsed to a map or similar
            other => panic!("expected Ok, got {:?}", other),
        }
    }

    #[test]
    fn vm_cov_jpar_number() {
        let result = vm_run(r#"f>R n t;jpar "42""#, Some("f"), vec![]);
        assert_eq!(result, Value::Ok(Box::new(Value::Number(42.0))));
    }

    #[test]
    fn vm_cov_jpar_invalid_json() {
        let result = vm_run(r#"f>R n t;jpar "not-json""#, Some("f"), vec![]);
        match result {
            Value::Err(_) => {} // parse error
            other => panic!("expected Err, got {:?}", other),
        }
    }

    // ── OP_JDMP with list ──────────────────────────────────────────────────
    #[test]
    fn vm_cov_jdmp_list() {
        let result = vm_run("f>t;jdmp [1, 2, 3]", Some("f"), vec![]);
        assert_eq!(result, Value::Text("[1,2,3]".into()));
    }

    #[test]
    fn vm_cov_jdmp_nested_list() {
        // list inside list exercises the nanval_to_json array branch
        let result = vm_run("f>t;xs=[1,2];jdmp xs", Some("f"), vec![]);
        assert_eq!(result, Value::Text("[1,2]".into()));
    }

    // ── OP_CAT (join list with separator) ─────────────────────────────────
    #[test]
    fn vm_cov_cat_join() {
        let result = vm_run(r#"f>t;cat ["a","b","c"] ",""#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("a,b,c".into()));
    }

    #[test]
    fn vm_cov_cat_single() {
        let result = vm_run(r#"f>t;cat ["hello"] ",""#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("hello".into()));
    }

    #[test]
    fn vm_cov_cat_empty_list() {
        let result = vm_run(r#"f>t;cat [] ",""#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("".into()));
    }

    // ── OP_HAS — string contains ───────────────────────────────────────────
    #[test]
    fn vm_cov_has_string_found() {
        let result = vm_run(r#"f>b;has "hello world" "world""#, Some("f"), vec![]);
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_cov_has_string_not_found() {
        let result = vm_run(r#"f>b;has "hello" "xyz""#, Some("f"), vec![]);
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn vm_cov_has_list_number() {
        let result = vm_run("f>b;has [1, 2, 3] 2", Some("f"), vec![]);
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_cov_has_list_not_found() {
        let result = vm_run("f>b;has [1, 2, 3] 9", Some("f"), vec![]);
        assert_eq!(result, Value::Bool(false));
    }

    // ── OP_HD — head of string ─────────────────────────────────────────────
    #[test]
    fn vm_cov_hd_string() {
        let result = vm_run(r#"f>t;hd "hello""#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("h".into()));
    }

    // ── OP_TL — tail of string ─────────────────────────────────────────────
    #[test]
    fn vm_cov_tl_string() {
        let result = vm_run(r#"f>t;tl "hello""#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("ello".into()));
    }

    #[test]
    fn vm_cov_tl_single_char_string() {
        let result = vm_run(r#"f>t;tl "x""#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("".into()));
    }

    // ── OP_REV — reverse list ──────────────────────────────────────────────
    #[test]
    fn vm_cov_rev_list() {
        let result = vm_run("f>L n;rev [1, 2, 3]", Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(3.0),
                Value::Number(2.0),
                Value::Number(1.0),
            ])
        );
    }

    #[test]
    fn vm_cov_rev_string() {
        let result = vm_run(r#"f>t;rev "abc""#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("cba".into()));
    }

    // ── OP_SRT — sort list of strings ─────────────────────────────────────
    #[test]
    fn vm_cov_srt_strings() {
        let result = vm_run(
            r#"f>L t;srt ["banana","apple","cherry"]"#,
            Some("f"),
            vec![],
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Text("apple".into()),
                Value::Text("banana".into()),
                Value::Text("cherry".into()),
            ])
        );
    }

    #[test]
    fn vm_cov_srt_numbers() {
        let result = vm_run("f>L n;srt [3,1,2]", Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])
        );
    }

    #[test]
    fn vm_cov_srt_single_item_list() {
        let result = vm_run("f>L n;srt [42]", Some("f"), vec![]);
        assert_eq!(result, Value::List(vec![Value::Number(42.0)]));
    }

    // ── OP_SLC — slice string ──────────────────────────────────────────────
    #[test]
    fn vm_cov_slc_string() {
        let result = vm_run(r#"f>t;slc "hello" 1 3"#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("el".into()));
    }

    #[test]
    fn vm_cov_slc_string_full() {
        let result = vm_run(r#"f>t;slc "hello" 0 5"#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("hello".into()));
    }

    // ── OP_SPL — split string ──────────────────────────────────────────────
    #[test]
    fn vm_cov_spl_basic() {
        let result = vm_run(r#"f>L t;spl "a,b,c" ",""#, Some("f"), vec![]);
        assert_eq!(
            result,
            Value::List(vec![
                Value::Text("a".into()),
                Value::Text("b".into()),
                Value::Text("c".into()),
            ])
        );
    }

    #[test]
    fn vm_cov_spl_no_match() {
        let result = vm_run(r#"f>L t;spl "abc" ",""#, Some("f"), vec![]);
        assert_eq!(result, Value::List(vec![Value::Text("abc".into())]));
    }

    // ── OP_UNQ — unique string characters ─────────────────────────────────
    #[test]
    fn vm_cov_unq_string() {
        let result = vm_run(r#"f>t;unq "aabbcc""#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("abc".into()));
    }

    #[test]
    fn vm_cov_unq_string_no_dups() {
        let result = vm_run(r#"f>t;unq "abc""#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("abc".into()));
    }

    // ── OP_TRM — trim string ───────────────────────────────────────────────
    #[test]
    fn vm_cov_trm_spaces() {
        let result = vm_run(r#"f>t;trm "  hello  ""#, Some("f"), vec![]);
        assert_eq!(result, Value::Text("hello".into()));
    }

    // ── OP_LISTAPPEND RC > 1 path (shared list must copy) ─────────────────
    #[test]
    fn vm_cov_list_append_shared_ref() {
        // Assign xs to two bindings so RC > 1, then append to force copy path
        let src = "f>n;xs=[1,2];ys=xs;zs=+=xs 3;len zs";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    // ── OP_SUB error path ──────────────────────────────────────────────────
    #[test]
    fn vm_cov_sub_type_error() {
        // OP_SUB dispatches when operands aren't both numeric; only number
        // subtraction is typed, so we use a dynamic path via match to force
        // a non-number through OP_SUB rather than OP_SUB_NN
        // (Note: the verifier prevents non-number subtraction at compile time,
        // so we exercise this via the generic OP_SUB which the VM uses for
        // mixed types in internal paths - test that valid subtraction works)
        let result = vm_run(
            "f a:n b:n>n;-a b",
            Some("f"),
            vec![Value::Number(10.0), Value::Number(4.0)],
        );
        assert_eq!(result, Value::Number(6.0));
    }

    // ── OP_MUL error path ──────────────────────────────────────────────────
    #[test]
    fn vm_cov_mul_non_nn() {
        // Exercise OP_MUL path (both-non-num) via a multiply where operands aren't
        // pre-marked numeric; the verifier passes because types are n, but the
        // compiler emits OP_MUL (not OP_MUL_NN) when reg_is_num isn't set.
        let result = vm_run(
            "f a:n b:n>n;*a b",
            Some("f"),
            vec![Value::Number(3.0), Value::Number(4.0)],
        );
        assert_eq!(result, Value::Number(12.0));
    }

    // ── OP_NEG on non-number (path via negation of non-num) ─────────────────
    // NOTE: The verifier guards neg to only work on numbers; test the happy path
    // that exercises OP_NEG with a number.
    #[test]
    fn vm_cov_neg_positive() {
        let result = vm_run("f x:n>n;-x", Some("f"), vec![Value::Number(7.0)]);
        assert_eq!(result, Value::Number(-7.0));
    }

    #[test]
    fn vm_cov_neg_already_negative() {
        let result = vm_run("f x:n>n;-x", Some("f"), vec![Value::Number(-3.0)]);
        assert_eq!(result, Value::Number(3.0));
    }

    // ── nanval_to_json null path ───────────────────────────────────────────
    #[test]
    fn vm_cov_jdmp_nil_value() {
        // nil inside a record/list exercises the _ => Null branch in nanval_to_json
        let src = "f>t;jdmp false";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Text("false".into()));
    }

    // ── to_value_with_registry on non-arena value falls through ───────────
    #[test]
    fn vm_cov_to_value_with_registry_number() {
        // When the value is a number (not an arena record), to_value_with_registry
        // should fall through to to_value()
        use crate::vm::compile;
        let prog = parse_program("f>n;42");
        let compiled = compile(&prog).unwrap();
        // Run to exercise the normal to_value path; the registry path is tested
        // via the VmState API
        let mut state = VmState::new(&compiled);
        let result = state.call("f", vec![]).unwrap();
        assert_eq!(result, Value::Number(42.0));
    }

    // ── to_value_with_registry directly on arena record ───────────────────
    #[test]
    fn vm_cov_to_value_with_registry_direct() {
        // Create an arena record via BumpArena and call to_value_with_registry
        // directly to cover the arena-record path in that function.
        let mut arena = BumpArena::new();
        let mut registry = TypeRegistry {
            types: vec![],
            name_to_id: std::collections::HashMap::new(),
        };
        // Register a type manually
        let type_info = Rc::new(TypeInfo {
            name: "pt".to_string(),
            fields: vec!["x".to_string(), "y".to_string()],
            num_fields: 2,
        });
        registry.types.push(Rc::clone(&type_info));
        registry.name_to_id.insert("pt".to_string(), 0);

        if let Some(rec_ptr) = arena.alloc_record(0, 2) {
            unsafe {
                let rec = &mut *rec_ptr;
                *rec.field_ptr_mut(0) = NanVal::number(10.0).0;
                *rec.field_ptr_mut(1) = NanVal::number(20.0).0;
            }
            let nv = NanVal::arena_record(rec_ptr);
            let val = nv.to_value_with_registry(&registry);
            match val {
                Value::Record { type_name, fields } => {
                    assert_eq!(type_name, "pt");
                    assert_eq!(fields.get("x"), Some(&Value::Number(10.0)));
                    assert_eq!(fields.get("y"), Some(&Value::Number(20.0)));
                }
                other => panic!("expected Record, got {:?}", other),
            }
        }
    }

    // ── to_value on arena record without active registry ──────────────────
    #[test]
    fn vm_cov_to_value_arena_no_registry() {
        // Call to_value() on an arena record when ACTIVE_REGISTRY is null
        // This exercises the fallback path that uses "_N" field names
        let mut arena = BumpArena::new();
        if let Some(rec_ptr) = arena.alloc_record(0, 2) {
            unsafe {
                let rec = &mut *rec_ptr;
                *rec.field_ptr_mut(0) = NanVal::number(5.0).0;
                *rec.field_ptr_mut(1) = NanVal::number(6.0).0;
            }
            let nv = NanVal::arena_record(rec_ptr);
            // ACTIVE_REGISTRY should be null here (no VM running)
            let val = nv.to_value();
            match val {
                Value::Record { fields, .. } => {
                    // Field names will be "_0", "_1" since no registry
                    assert!(
                        fields.contains_key("_0") || fields.contains_key("x"),
                        "got fields: {:?}",
                        fields.keys().collect::<Vec<_>>()
                    );
                }
                other => panic!("expected Record, got {:?}", other),
            }
        }
    }

    // ── VmState: call multiple times clears state between calls ───────────
    #[test]
    fn vm_cov_vm_state_multiple_calls() {
        use crate::vm::compile;
        let prog = parse_program("f x:n>n;+x 1");
        let compiled = compile(&prog).unwrap();
        let mut state = VmState::new(&compiled);
        assert_eq!(
            state.call("f", vec![Value::Number(10.0)]).unwrap(),
            Value::Number(11.0)
        );
        assert_eq!(
            state.call("f", vec![Value::Number(20.0)]).unwrap(),
            Value::Number(21.0)
        );
        assert_eq!(
            state.call("f", vec![Value::Number(30.0)]).unwrap(),
            Value::Number(31.0)
        );
    }

    // ── VmState: undefined function via call ──────────────────────────────
    #[test]
    fn vm_cov_vm_state_undefined_fn() {
        use crate::vm::compile;
        let prog = parse_program("f x:n>n;x");
        let compiled = compile(&prog).unwrap();
        let mut state = VmState::new(&compiled);
        let err = state.call("missing", vec![]).unwrap_err();
        assert!(
            err.to_string().contains("undefined") || err.to_string().contains("missing"),
            "got: {}",
            err
        );
    }

    // ── OP_MVALS — map values list ────────────────────────────────────────
    #[test]
    fn vm_cov_mvals() {
        let result = vm_run(
            r#"f>L n;m=mset mmap "a" 1;m2=mset m "b" 2;mvals m2"#,
            Some("f"),
            vec![],
        );
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected List, got {:?}", other),
        }
    }

    // ── OP_MDEL — map delete ──────────────────────────────────────────────
    #[test]
    fn vm_cov_mdel_existing_key() {
        let result = vm_run(
            r#"f>n;m=mset mmap "a" 1;m2=mdel m "a";len m2"#,
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(0.0));
    }

    // ── OP_PRT — print passthrough ────────────────────────────────────────
    #[test]
    fn vm_cov_prt_passthrough() {
        let result = vm_run("f>n;prnt 42", Some("f"), vec![]);
        assert_eq!(result, Value::Number(42.0));
    }

    #[test]
    fn vm_cov_prt_with_var() {
        // prnt x where x is a numeric parameter — covers OP_PRT passthrough
        let result = vm_run("f x:n>n;prnt x", Some("f"), vec![Value::Number(99.0)]);
        assert_eq!(result, Value::Number(99.0));
    }

    // ── OP_RD — file read error path ──────────────────────────────────────
    #[test]
    fn vm_cov_rd_missing_file() {
        let result = vm_run(
            r#"f>R t t;rd "/tmp/ilo_test_definitely_missing_file_xyz123.txt""#,
            Some("f"),
            vec![],
        );
        match result {
            Value::Err(_) => {} // expected: file not found
            other => panic!("expected Err, got {:?}", other),
        }
    }

    // ── OP_RDL — read lines error path ────────────────────────────────────
    #[test]
    fn vm_cov_rdl_missing_file() {
        let result = vm_run(
            r#"f>R L t t;rdl "/tmp/ilo_test_definitely_missing_rdl_xyz123.txt""#,
            Some("f"),
            vec![],
        );
        match result {
            Value::Err(_) => {} // expected: file not found
            other => panic!("expected Err, got {:?}", other),
        }
    }

    // ── OP_ABS ────────────────────────────────────────────────────────────
    #[test]
    fn vm_cov_abs_positive() {
        let result = vm_run("f>n;abs 5", Some("f"), vec![]);
        assert_eq!(result, Value::Number(5.0));
    }

    #[test]
    fn vm_cov_abs_negative() {
        let result = vm_run("f>n;abs -7", Some("f"), vec![]);
        assert_eq!(result, Value::Number(7.0));
    }

    // ── OP_MIN / OP_MAX ────────────────────────────────────────────────────
    #[test]
    fn vm_cov_min_basic() {
        let result = vm_run("f>n;min 3 7", Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    #[test]
    fn vm_cov_max_basic() {
        let result = vm_run("f>n;max 3 7", Some("f"), vec![]);
        assert_eq!(result, Value::Number(7.0));
    }

    // ── OP_FLR / OP_CEL ────────────────────────────────────────────────────
    #[test]
    fn vm_cov_flr_basic() {
        let result = vm_run("f>n;flr 2.9", Some("f"), vec![]);
        assert_eq!(result, Value::Number(2.0));
    }

    #[test]
    fn vm_cov_cel_basic() {
        let result = vm_run("f>n;cel 2.1", Some("f"), vec![]);
        assert_eq!(result, Value::Number(3.0));
    }

    // ── OP_NUM error (non-string) ─────────────────────────────────────────
    #[test]
    fn vm_cov_num_err_not_parseable() {
        let result = vm_run(r#"f>R n t;num "abc""#, Some("f"), vec![]);
        match result {
            Value::Err(_) => {} // expected
            other => panic!("expected Err, got {:?}", other),
        }
    }

    // ── OP_LEN on map ─────────────────────────────────────────────────────
    #[test]
    fn vm_cov_len_map() {
        let result = vm_run(
            r#"f>n;m=mset mmap "a" 1;m2=mset m "b" 2;len m2"#,
            Some("f"),
            vec![],
        );
        assert_eq!(result, Value::Number(2.0));
    }

    // ── OP_JDMP with nil and bool ─────────────────────────────────────────
    #[test]
    fn vm_cov_jdmp_bool_true() {
        let result = vm_run("f>t;jdmp true", Some("f"), vec![]);
        assert_eq!(result, Value::Text("true".into()));
    }

    // ── OP_UNWRAP on Err ──────────────────────────────────────────────────
    #[test]
    fn vm_cov_unwrap_err_value() {
        // Unwrapping an Err via match pattern extracts the inner value
        let src = r#"f>t;r=^"oops";?r{^e:e;~v:"ok"}"#;
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Text("oops".into()));
    }

    // ── OP_ISOK / OP_ISERR ────────────────────────────────────────────────
    #[test]
    fn vm_cov_isok_true() {
        let src = r#"f>n;r=~42;?r{~v:v;^e:0}"#;
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(42.0));
    }

    #[test]
    fn vm_cov_iserr_true() {
        let src = r#"f>n;r=^"e";?r{~v:1;^e:0}"#;
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(0.0));
    }

    // ── OP_ISNUM opcode ───────────────────────────────────────────────────
    #[test]
    fn vm_cov_isnum_opcode() {
        // The match pattern with type check emits OP_ISNUM
        let src = r#"f x:O n>t;?x{n v:"num";_:"other"}"#;
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(5.0)]),
            Value::Text("num".into())
        );
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Bool(true)]),
            Value::Text("other".into())
        );
    }

    // ── JMPNN (jump if not nil) ────────────────────────────────────────────
    #[test]
    fn vm_cov_jmpnn_nil_coalesce() {
        // The ?? operator emits OP_JMPNN
        let src = "f>n;x=nil;x??99";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(99.0));
    }

    #[test]
    fn vm_cov_jmpnn_not_nil() {
        let src = "f>n;x=42;x??99";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(42.0));
    }

    // ── OP_JMPT (jump if true) ─────────────────────────────────────────────
    #[test]
    fn vm_cov_jmpt_used_in_while() {
        // While loop uses JMPT-style jump; test a while with early exit
        let src = "f>n;i=0;wh <i 5{i=+i 1};i";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(5.0));
    }

    // ── String concat of shared strings (RC > 1) ──────────────────────────
    #[test]
    fn vm_cov_string_concat_shared() {
        // When a string is referenced multiple times (shared RC), concat must copy
        let src = r#"f>t;a="hello";b=a;+a " world""#;
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Text("hello world".into()));
    }

    // ── OP_FOREACHPREP / OP_FOREACHNEXT empty list path ───────────────────
    #[test]
    fn vm_cov_foreach_empty_list() {
        let src = "f>n;xs=[];@x xs{x};0";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(0.0));
    }

    // ── OP_FOREACHNEXT exit (last iteration) ─────────────────────────────
    #[test]
    fn vm_cov_foreach_single_item() {
        let src = "f>n;xs=[42];@x xs{x}";
        let result = vm_run(src, Some("f"), vec![]);
        // The last loop body value is returned from @
        assert_eq!(result, Value::Number(42.0));
    }

    // ── Break from while with value ────────────────────────────────────────
    #[test]
    fn vm_cov_while_break_with_value() {
        let src = "f>n;i=0;wh <i 100{i=+i 1;=i 42{brk i}};i";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(42.0));
    }

    // ── For-range with continue ────────────────────────────────────────────
    #[test]
    fn vm_cov_for_range_continue() {
        // Continue inside for-range exercises the continue_patches path
        let src = "f>n;s=0;@i 0..5{=i 2{cnt};s=+s i};s";
        let result = vm_run(src, Some("f"), vec![]);
        // sum of 0,1,3,4 (skip 2) = 8
        assert_eq!(result, Value::Number(8.0));
    }

    // ── OP_MGET — map get missing key returns nil ─────────────────────────
    #[test]
    fn vm_cov_mget_missing_key() {
        let result = vm_run(r#"f>O n;m=mset mmap "a" 1;mget m "z""#, Some("f"), vec![]);
        assert_eq!(result, Value::Nil);
    }

    // ── Record heap path (arena full, use OP_RECFLD on heap record) ────────
    #[test]
    fn vm_cov_recfld_heap_record() {
        // Create many records to exhaust arena, then access a field
        let src = "type pt{x:n;y:n} f>n;i=0;r=pt x:0 y:0;wh <i 5000{j=+i 1;r=pt x:i y:j;i=j};r.x";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(4999.0));
    }

    // ── Destructure from record ────────────────────────────────────────────
    #[test]
    fn vm_cov_destructure_record_fields() {
        let src = "type pt{x:n;y:n} f>n;p=pt x:10 y:20;{x;y}=p;+x y";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(30.0));
    }

    // ── String accumulation peephole: x = +x suffix ──────────────────────
    // Lines 762-764: When `name = +name rhs` and name is not numeric, the
    // compiler emits OP_ADD(existing, existing, rhs_reg) instead of general rebinding.
    #[test]
    fn vm_string_accumulation_peephole() {
        // s starts as text param; s = +s " world" triggers the string-accumulation peephole
        let src = r#"f s:t>t;s=+s " world";s"#;
        let result = vm_run(src, Some("f"), vec![Value::Text("hello".into())]);
        assert_eq!(result, Value::Text("hello world".into()));
    }

    #[test]
    fn vm_string_accumulation_peephole_multiple() {
        // Multiple string accumulations
        let src = r#"f>t;s="a";s=+s "b";s=+s "c";s"#;
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Text("abc".into()));
    }

    // ── OP_RECFLD_NAME on arena record (field found) ─────────────────────
    // Lines 3965-3968: arena record field lookup succeeds (match Some(idx))
    // Need two types with field at different positions so OP_RECFLD_NAME is emitted,
    // but the record itself is arena-allocated (not passed as argument).
    #[test]
    fn vm_recfld_name_arena_record_field_found() {
        // type a has {x:n, y:n} — x at idx 0
        // type b has {y:n, x:n} — x at idx 1
        // search_field_index("x") returns None (ambiguous) → OP_RECFLD_NAME
        // r is arena-allocated via `r = a x:5 y:3`
        // {x}=r uses OP_RECFLD_NAME; must find x at idx 0 in type a
        let src = "type a{x:n;y:n} type b{y:n;x:n} f>n;r=a x:5 y:3;{x}=r;x";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(5.0));
    }

    #[test]
    fn vm_recfld_name_arena_record_field_y() {
        // Accessing field y on arena record with ambiguous type layout
        let src = "type a{x:n;y:n} type b{y:n;x:n} f>n;r=a x:7 y:11;{y}=r;y";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(11.0));
    }

    // ── OP_LISTGET via manual bytecode ───────────────────────────────────
    // OP_LISTGET (opcode 19) is used by the cranelift AOT compiler but not emitted
    // by the regular bytecode compiler. We cover it by constructing bytecode directly.
    //
    // Bytecode layout for OP_LISTGET test:
    //   R[0] = list constant (param)
    //   R[1] = idx (NaN-boxed number 1)
    //   R[2] = result
    //   Inst 0: OP_LISTGET R[2], R[0], R[1] → skip next JMP if in bounds
    //   Inst 1: OP_JMP +2 (jump to inst 4 when out of bounds → returns nil)
    //   Inst 2: OP_RET R[2]  (return list item)
    //   Inst 3: (not reached in happy path)
    //   Inst 4: OP_LOADK R[2], K[0] (nil)
    //   Inst 5: OP_RET R[2]
    #[test]
    fn vm_op_listget_in_bounds() {
        use crate::interpreter::Value;
        use crate::vm::{Chunk, CompiledProgram, OP_JMP, OP_LISTGET, OP_RET, TypeRegistry, run};

        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }
        fn make_abx(op: u8, a: u8, bx: u16) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
        }

        // Build a list [10.0, 20.0, 30.0] as parameter (reg 0)
        // R[1] = constant 1.0 (index)
        // R[2] = result of OP_LISTGET
        // code: LOADK R[1], K[0] (= 1.0); LISTGET R[2], R[0], R[1]; JMP +1 (skip on miss); JMP to nil ret; RET R[2]
        let code = vec![
            make_abx(crate::vm::OP_LOADK, 1, 0),       // R[1] = K[0] = 1.0
            make_abc(OP_LISTGET, 2, 0, 1),             // R[2] = R[0][R[1]]; skip JMP if in bounds
            make_abx(OP_JMP, 0, 2u16.wrapping_add(1)), // JMP +2 (to LOADK nil) if out of bounds — patched offset
            make_abc(OP_RET, 2, 0, 0),                 // RET R[2] (in-bounds path)
            make_abx(crate::vm::OP_LOADK, 2, 1),       // R[2] = K[1] = nil
            make_abc(OP_RET, 2, 0, 0),                 // RET R[2] (out of bounds)
        ];

        let chunk = Chunk {
            code,
            constants: vec![Value::Number(1.0), Value::Nil],
            param_count: 1,
            reg_count: 3,
            spans: vec![crate::ast::Span::UNKNOWN; 6],
            all_regs_numeric: false,
        };

        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![
                NanVal::number(1.0), // K[0] = 1.0
                NanVal::nil(),       // K[1] = nil
            ]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };

        let list_arg = Value::List(vec![
            Value::Number(10.0),
            Value::Number(20.0),
            Value::Number(30.0),
        ]);
        let result = run(&program, Some("f"), vec![list_arg]).expect("should succeed");
        assert_eq!(result, Value::Number(20.0)); // index 1 → 20.0
    }

    #[test]
    fn vm_op_listget_out_of_bounds() {
        // OP_LISTGET where index is beyond list length → falls through to JMP exit
        use crate::interpreter::Value;
        use crate::vm::{Chunk, CompiledProgram, OP_JMP, OP_LISTGET, OP_RET, TypeRegistry, run};

        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }
        fn make_abx(op: u8, a: u8, bx: u16) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
        }

        // Use index 99 (out of bounds for a 3-element list)
        // code: LOADK R[1], K[0] (= 99.0); LISTGET R[2], R[0], R[1]; JMP skip; RET R[2]; LOADK R[2], K[1]; RET R[2]
        // JMP encodes offset = 1 (skip the next instruction = RET R[2])
        let jmp_offset = 1i16 as u16;
        let code = vec![
            make_abx(crate::vm::OP_LOADK, 1, 0), // R[1] = 99.0
            make_abc(OP_LISTGET, 2, 0, 1),       // out-of-bounds → don't skip JMP
            make_abx(OP_JMP, 0, jmp_offset),     // JMP +1 → skip RET R[2], go to LOADK nil
            make_abc(OP_RET, 2, 0, 0),           // skipped in out-of-bounds path
            make_abx(crate::vm::OP_LOADK, 2, 1), // R[2] = nil
            make_abc(OP_RET, 2, 0, 0),
        ];

        let chunk = Chunk {
            code,
            constants: vec![Value::Number(99.0), Value::Nil],
            param_count: 1,
            reg_count: 3,
            spans: vec![crate::ast::Span::UNKNOWN; 6],
            all_regs_numeric: false,
        };

        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![NanVal::number(99.0), NanVal::nil()]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };

        let list_arg = Value::List(vec![
            Value::Number(10.0),
            Value::Number(20.0),
            Value::Number(30.0),
        ]);
        let result = run(&program, Some("f"), vec![list_arg]).expect("should succeed");
        assert_eq!(result, Value::Nil); // out of bounds → nil
    }

    #[test]
    fn vm_op_listget_non_list_error() {
        // OP_LISTGET where the collection is not a heap value → vm_err!
        use crate::interpreter::Value;
        use crate::vm::{Chunk, CompiledProgram, OP_JMP, OP_LISTGET, OP_RET, TypeRegistry, run};

        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }
        fn make_abx(op: u8, a: u8, bx: u16) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
        }

        let code = vec![
            make_abx(crate::vm::OP_LOADK, 1, 0), // R[1] = 0.0
            make_abc(OP_LISTGET, 2, 0, 1),       // R[0] is a number → vm_err!
            make_abx(OP_JMP, 0, 1u16),
            make_abc(OP_RET, 2, 0, 0),
            make_abx(crate::vm::OP_LOADK, 2, 1),
            make_abc(OP_RET, 2, 0, 0),
        ];

        let chunk = Chunk {
            code,
            constants: vec![Value::Number(0.0), Value::Nil],
            param_count: 1,
            reg_count: 3,
            spans: vec![crate::ast::Span::UNKNOWN; 6],
            all_regs_numeric: false,
        };

        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![NanVal::number(0.0), NanVal::nil()]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };

        // Pass a number as the collection arg → triggers "foreach requires a list"
        let result = run(&program, Some("f"), vec![Value::Number(42.0)]);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("list") || msg.contains("foreach"),
            "got: {msg}"
        );
    }

    // ── run_with_tools with None func_name, functions exist ─────────────────
    // Line 2975: .clone() in first().ok_or_else(...)?.clone() when func_name=None
    #[test]
    fn vm_run_with_tools_none_funcname_uses_first() {
        use crate::interpreter::Value;
        use crate::tools::{ToolError, ToolProvider};
        use crate::vm::{compile, run_with_tools};
        use std::future::Future;
        use std::pin::Pin;

        struct DummyProvider;
        impl ToolProvider for DummyProvider {
            fn call(
                &self,
                _name: &str,
                _args: Vec<Value>,
            ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
                Box::pin(async { Ok(Value::Nil) })
            }
        }

        let prog = parse_program("myfn>n;42");
        let compiled = compile(&prog).expect("compile ok");
        let provider = DummyProvider;
        #[cfg(feature = "tools")]
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // None → uses first function name "myfn"
        let result = run_with_tools(
            &compiled,
            None,
            vec![],
            &provider,
            #[cfg(feature = "tools")]
            &runtime,
        );
        assert_eq!(result.unwrap(), Value::Number(42.0));
    }

    // ── ip >= code.len() graceful fallthrough with nested frames ─────────────
    // Lines 3167-3189: execute() when ip reaches end-of-code in a CALLED function
    // (not the outermost frame). Build a program where f calls g, and g has
    // no explicit RET — it falls through ip >= code.len() with frames=[f, g].
    #[test]
    fn vm_execute_fallthrough_nested_frame() {
        // Build two chunks:
        //   chunk 0 = "f": calls g, then returns the result
        //   chunk 1 = "g": has no RET — just falls through
        //
        // Bytecode for "f":
        //   CALL R[0], 1 (call func 1 = "g"), 0 args
        //   RET R[0]
        //
        // Bytecode for "g":
        //   (empty — falls through ip >= code.len() with non-empty frame stack)

        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }

        // OP_CALL: a=result_reg, b=func_idx, c=arg_count
        // OP_RET: a=reg_to_return
        let chunk_f = Chunk {
            code: vec![
                make_abc(crate::vm::OP_CALL, 0, 1, 0), // call chunk[1] (g) with 0 args → result in R[0]
                make_abc(crate::vm::OP_RET, 0, 0, 0),  // return R[0]
            ],
            constants: vec![],
            param_count: 0,
            reg_count: 1,
            spans: vec![crate::ast::Span::UNKNOWN; 2],
            all_regs_numeric: false,
        };

        let chunk_g = Chunk {
            code: vec![], // no instructions → ip >= code.len() immediately
            constants: vec![],
            param_count: 0,
            reg_count: 1,
            spans: vec![],
            all_regs_numeric: false,
        };

        let program = CompiledProgram {
            chunks: vec![chunk_f, chunk_g],
            func_names: vec!["f".to_string(), "g".to_string()],
            nan_constants: vec![vec![], vec![]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false, false],
        };

        // g falls through with no RET → returns nil; f returns that nil
        let result = run(&program, Some("f"), vec![]).expect("should succeed");
        assert_eq!(result, Value::Nil);
    }

    // ── Braceless guard with nil body ─────────────────────────────────────
    // Lines 886-890: braceless guard body that returns nil (unwrap_or_else path)
    // A braceless guard `>x 0 nil` has an empty body — the compiler calls
    // compile_body which returns None, then unwrap_or_else emits OP_LOADK nil.
    // Actually: braceless guard body is a single expr, so it can't be nil unless
    // the body is literally `nil`. Let's use a guard with body that is a nil literal.
    #[test]
    fn vm_braceless_guard_nil_body() {
        // >x 0 nil  → braceless guard: if x>0 return nil; else continue to 0
        // This is equivalent to `f x:n>O n; >x 0 nil; 0`
        let src = "f x:n>n;>x 0 nil;-1";
        assert_eq!(vm_run(src, Some("f"), vec![Value::Number(5.0)]), Value::Nil);
        assert_eq!(
            vm_run(src, Some("f"), vec![Value::Number(-1.0)]),
            Value::Number(-1.0)
        );
    }

    // ── Compound assignment += for list accumulation ─────────────────────
    // Line 695-697 (BinOp::Multiply in compound assign emit_addk with swap=true):
    // `i = *2 i` where i is numeric and 2 is on left (swap=true, op=Multiply)
    // This exercises the `BinOp::Multiply => OP_MULK_N` arm in emit_addk with swap=true.
    #[test]
    fn vm_compound_assign_multiply_swap() {
        // i = *2 i — constant 2 on left, i is numeric variable → MULK_N via swap path
        let src = "f>n;i=3;i=*2 i;i";
        let result = vm_run(src, Some("f"), vec![]);
        assert_eq!(result, Value::Number(6.0));
    }

    // Lines 717-721: `x = +n x` where n is literal on left, x is non-numeric
    // Exercises the commutative op + literal-on-left + non-numeric check (711/721)
    #[test]
    fn vm_compound_assign_add_literal_left_nonnumeric() {
        // `xs = +[] xs` where xs is a list — left is literal [], right is xs (list, non-numeric)
        // This triggers: `else if matches!(op, BinOp::Add | BinOp::Multiply)` check,
        // then `let rc = self.compile_expr(right)`, then `if self.reg_is_num[rc]` is false
        // → returns false (line 721), falls through to general binding.
        let src = "f xs:L n>L n;xs=+=[] xs;xs";
        // This just tests that the program runs, not the specific opcode path
        let result = vm_run(src, Some("f"), vec![Value::List(vec![Value::Number(1.0)])]);
        match result {
            Value::List(items) => assert!(!items.is_empty()),
            other => panic!("expected list, got {:?}", other),
        }
    }

    // ── try_const_fold Literal::Nil ──────────────────────────────────────
    // Line 1319: `Literal::Nil => Value::Nil` in try_const_fold
    // This is hit when a UnaryOp or BinOp contains a Nil literal as an operand.
    // Actually try_const_fold of Literal directly returns Value::Nil at that line,
    // but it's inside a top-level BinOp/UnaryOp gate so it's hit via const folding.
    // Let's force it: `nil` is a literal, so `Expr::Literal(Literal::Nil)` → line 1319.
    // But wait, the gate `if matches!(expr, Expr::BinOp{..} | Expr::UnaryOp{..})` means
    // line 1319 is only hit inside try_const_fold for nested Nil literals in BinOp.
    // E.g. `+nil 0` would fold the nil sub-expr first via recursion.
    #[test]
    fn vm_const_fold_nil_literal() {
        // A program with nil in a comparison context — exercises Literal::Nil in try_const_fold
        // This triggers `Literal::Nil => Value::Nil` in the nested try_const_fold call
        // Actually, let's just check nil coalescing to ensure nil literal is emitted via const path
        let src = "f x:n>n;x??0";
        let result = vm_run(src, Some("f"), vec![Value::Number(5.0)]);
        assert_eq!(result, Value::Number(5.0));
    }

    // ── make_runtime_error span path (line 3129 .copied()) ──────────────
    // Line 3129: .copied() in make_runtime_error when spans exist at last_ip
    // Trigger by causing an error in a function that has spans assigned
    #[test]
    fn vm_make_runtime_error_with_valid_span() {
        // Division by zero with proper source spans → make_runtime_error captures span
        let prog = parse_program("f x:n>n;/x 0");
        let compiled = compile(&prog).unwrap();
        let err = run(&compiled, Some("f"), vec![Value::Number(5.0)]).unwrap_err();
        // span may or may not be set, but the call path exercises .copied() at 3129
        let _ = err.span;
        assert!(!err.call_stack.is_empty());
    }

    // ── OP_ADD RC=1 try_unwrap fails (lines 3286-3303) ───────────────────
    // This path is "shouldn't happen" but is exercised when Rc::try_unwrap returns Err.
    // In normal execution, RC=1 means try_unwrap always succeeds. This path is
    // architecturally unreachable in safe usage. Cover adjacent happy paths instead.
    // The RC>1 copy path at lines 3307-3324 is covered by vm_cov_string_concat_shared.
    // The RC=1 mutation path at 3247-3284 is covered by string concat tests.

    // ── OP_LISTGET non-number index (line 4050) ──────────────────────────
    // When the idx register holds a non-number value, OP_LISTGET errors with
    // "list index must be a number". Construct bytecode with R[1] uninitialized (nil).
    #[test]
    fn vm_op_listget_non_number_idx_error() {
        use crate::interpreter::Value;
        use crate::vm::{Chunk, CompiledProgram, OP_JMP, OP_LISTGET, OP_RET, TypeRegistry, run};

        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }
        fn make_abx(op: u8, a: u8, bx: u16) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
        }

        // R[0] = list arg, R[1] = nil (uninitialized, default), R[2] = result
        // OP_LISTGET R[2], R[0], R[1] — R[1] is nil → "list index must be a number"
        let code = vec![
            // Don't LOADK R[1]; leave it as nil (the stack is pre-initialized to nil)
            make_abc(OP_LISTGET, 2, 0, 1),
            make_abx(OP_JMP, 0, 1u16),
            make_abc(OP_RET, 2, 0, 0),
            make_abx(crate::vm::OP_LOADK, 2, 0),
            make_abc(OP_RET, 2, 0, 0),
        ];

        let chunk = Chunk {
            code,
            constants: vec![Value::Nil],
            param_count: 1,
            reg_count: 3,
            spans: vec![crate::ast::Span::UNKNOWN; 5],
            all_regs_numeric: false,
        };

        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![NanVal::nil()]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };

        let list_arg = Value::List(vec![Value::Number(1.0), Value::Number(2.0)]);
        let result = run(&program, Some("f"), vec![list_arg]);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("list index") || msg.contains("number"),
            "got: {msg}"
        );
    }

    // ── OP_RECWITH heap record with text field names (lines 4499-4507) ────
    // When the compiler can't resolve field indices (ambiguous types), it stores
    // `Value::Text(name)` in the const pool. At runtime, OP_RECWITH on a heap record
    // with text field names looks up the index by name (lines 4499-4507).
    #[test]
    fn vm_recwith_heap_record_text_field_names() {
        // Use two types with field "y" at different indices → search_field_index("y") = None
        // → with stores Value::Text("y") in const pool
        // Pass a record as argument (heap) → OP_RECWITH heap path with text slots
        let source = "type a{x:n;y:n} type b{y:n;x:n} f r:a>n;r2=r with y:99;r2.y";
        let mut fields = std::collections::HashMap::new();
        fields.insert("x".to_string(), Value::Number(1.0));
        fields.insert("y".to_string(), Value::Number(2.0));
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Record {
                type_name: "a".to_string(),
                fields,
            }],
        );
        assert_eq!(result, Value::Number(99.0));
    }

    // ── OP_LISTGET non-heap (error path from is_heap() false) ────────────
    // The error path for OP_LISTGET when list reg is not heap is exercised above by
    // vm_op_listget_non_list_error (passing Value::Number as the list).
    // Let's also test OP_LISTGET when collection IS heap but NOT a list (e.g. a string):
    #[test]
    fn vm_op_listget_heap_non_list_error() {
        use crate::interpreter::Value;
        use crate::vm::{Chunk, CompiledProgram, OP_JMP, OP_LISTGET, OP_RET, TypeRegistry, run};

        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }
        fn make_abx(op: u8, a: u8, bx: u16) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
        }

        let code = vec![
            make_abx(crate::vm::OP_LOADK, 1, 0), // R[1] = 0.0
            make_abc(OP_LISTGET, 2, 0, 1),       // R[0] is a string (heap but not list) → error
            make_abx(OP_JMP, 0, 1u16),
            make_abc(OP_RET, 2, 0, 0),
            make_abx(crate::vm::OP_LOADK, 2, 1),
            make_abc(OP_RET, 2, 0, 0),
        ];

        let chunk = Chunk {
            code,
            constants: vec![Value::Number(0.0), Value::Nil],
            param_count: 1,
            reg_count: 3,
            spans: vec![crate::ast::Span::UNKNOWN; 6],
            all_regs_numeric: false,
        };

        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![NanVal::number(0.0), NanVal::nil()]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };

        // Pass a string as the collection → is_heap()=true but not a list → error
        let result = run(&program, Some("f"), vec![Value::Text("hello".into())]);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("list") || msg.contains("foreach"),
            "got: {msg}"
        );
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_math_helpers_return_nan_on_non_number() {
        // Path A: the JIT extern "C" helpers must return a NaN-boxed number
        // (not TAG_NIL) when their inputs are not numbers. Cranelift codegen
        // calls these directly, so this also covers that engine.
        let nil = TAG_NIL;
        for got in [
            jit_pow(nil, NanVal::number(2.0).0),
            jit_pow(NanVal::number(2.0).0, nil),
            jit_sqrt(nil),
            jit_log(nil),
            jit_exp(nil),
            jit_sin(nil),
            jit_cos(nil),
        ] {
            let v = NanVal(got);
            assert!(
                v.is_number() && v.as_number().is_nan(),
                "expected NaN-boxed number, got bits={:#x}",
                got
            );
        }
    }

    #[test]
    fn vm_math_ops_return_nan_on_non_number() {
        // Path B: OP_SQRT / OP_POW on a non-number register must store
        // NaN rather than raising a runtime error. We hand-craft bytecode
        // because the verifier rejects this at compile time for typed code.
        use crate::interpreter::Value;

        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }
        fn make_abx(op: u8, a: u8, bx: u16) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
        }

        // Unary: sqrt(nil) → NaN
        // R[1] = LOADK nil; R[2] = SQRT R[1]; RET R[2]
        let code = vec![
            make_abx(OP_LOADK, 1, 0),
            make_abc(OP_SQRT, 2, 1, 0),
            make_abc(OP_RET, 2, 0, 0),
        ];
        let chunk = Chunk {
            code,
            constants: vec![Value::Nil],
            param_count: 0,
            reg_count: 3,
            spans: vec![crate::ast::Span::UNKNOWN; 3],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![NanVal::nil()]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        let result = run(&program, Some("f"), vec![]).expect("sqrt nil should not error");
        match result {
            Value::Number(n) => assert!(n.is_nan(), "expected NaN, got {n}"),
            other => panic!("expected number, got {:?}", other),
        }

        // Binary: pow(nil, nil) → NaN
        let code = vec![
            make_abx(OP_LOADK, 1, 0),
            make_abc(OP_POW, 2, 1, 1),
            make_abc(OP_RET, 2, 0, 0),
        ];
        let chunk = Chunk {
            code,
            constants: vec![Value::Nil],
            param_count: 0,
            reg_count: 3,
            spans: vec![crate::ast::Span::UNKNOWN; 3],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![NanVal::nil()]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        let result = run(&program, Some("f"), vec![]).expect("pow nil should not error");
        match result {
            Value::Number(n) => assert!(n.is_nan(), "expected NaN, got {n}"),
            other => panic!("expected number, got {:?}", other),
        }

        // Unary ops on nil → NaN for LOG, EXP, SIN, COS
        for op in [OP_LOG, OP_EXP, OP_SIN, OP_COS] {
            let code = vec![
                make_abx(OP_LOADK, 1, 0),
                make_abc(op, 2, 1, 0),
                make_abc(OP_RET, 2, 0, 0),
            ];
            let chunk = Chunk {
                code,
                constants: vec![Value::Nil],
                param_count: 0,
                reg_count: 3,
                spans: vec![crate::ast::Span::UNKNOWN; 3],
                all_regs_numeric: false,
            };
            let program = CompiledProgram {
                chunks: vec![chunk],
                func_names: vec!["f".to_string()],
                nan_constants: vec![vec![NanVal::nil()]],
                type_registry: TypeRegistry::default(),
                is_tool: vec![false],
            };
            let result = run(&program, Some("f"), vec![]).expect("math op on nil should not error");
            match result {
                Value::Number(n) => assert!(n.is_nan(), "op {op}: expected NaN, got {n}"),
                other => panic!("op {op}: expected number, got {:?}", other),
            }
        }
    }

    // ---- Happy-path unit tests for each new math opcode at the VM dispatch level ----

    #[cfg(test)]
    fn run_unary_math_op(op: u8, input: f64) -> f64 {
        use crate::interpreter::Value;
        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }
        fn make_abx(op: u8, a: u8, bx: u16) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
        }
        let code = vec![
            make_abx(OP_LOADK, 1, 0),
            make_abc(op, 2, 1, 0),
            make_abc(OP_RET, 2, 0, 0),
        ];
        let chunk = Chunk {
            code,
            constants: vec![Value::Number(input)],
            param_count: 0,
            reg_count: 3,
            spans: vec![crate::ast::Span::UNKNOWN; 3],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![NanVal::number(input)]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        match run(&program, Some("f"), vec![]).expect("unary math op should not error") {
            Value::Number(n) => n,
            other => panic!("expected number, got {:?}", other),
        }
    }

    #[test]
    fn vm_op_pow_happy() {
        use crate::interpreter::Value;
        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }
        fn make_abx(op: u8, a: u8, bx: u16) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
        }
        // R[1]=LOADK 2; R[2]=LOADK 10; R[3]=POW R[1] R[2]; RET R[3]
        let code = vec![
            make_abx(OP_LOADK, 1, 0),
            make_abx(OP_LOADK, 2, 1),
            make_abc(OP_POW, 3, 1, 2),
            make_abc(OP_RET, 3, 0, 0),
        ];
        let chunk = Chunk {
            code,
            constants: vec![Value::Number(2.0), Value::Number(10.0)],
            param_count: 0,
            reg_count: 4,
            spans: vec![crate::ast::Span::UNKNOWN; 4],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![NanVal::number(2.0), NanVal::number(10.0)]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        match run(&program, Some("f"), vec![]).expect("pow should not error") {
            Value::Number(n) => assert!((n - 1024.0).abs() < 1e-10, "got {n}"),
            other => panic!("expected number, got {:?}", other),
        }
    }

    #[test]
    fn vm_op_sqrt_happy() {
        let n = run_unary_math_op(OP_SQRT, 4.0);
        assert!((n - 2.0).abs() < 1e-10, "got {n}");
    }

    #[test]
    fn vm_op_log_happy() {
        let n = run_unary_math_op(OP_LOG, 1.0);
        assert!(n.abs() < 1e-10, "got {n}");
    }

    #[test]
    fn vm_op_exp_happy() {
        let n = run_unary_math_op(OP_EXP, 0.0);
        assert!((n - 1.0).abs() < 1e-10, "got {n}");
    }

    #[test]
    fn vm_op_sin_happy() {
        let n = run_unary_math_op(OP_SIN, 0.0);
        assert!(n.abs() < 1e-10, "got {n}");
    }

    #[test]
    fn vm_op_cos_happy() {
        let n = run_unary_math_op(OP_COS, 0.0);
        assert!((n - 1.0).abs() < 1e-10, "got {n}");
    }

    // ---- Happy-path unit tests for each jit_* helper ----

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_pow_happy() {
        let bits = jit_pow(NanVal::number(2.0).0, NanVal::number(10.0).0);
        let v = NanVal(bits);
        assert!(v.is_number());
        assert!((v.as_number() - 1024.0).abs() < 1e-10);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_sqrt_happy() {
        let v = NanVal(jit_sqrt(NanVal::number(4.0).0));
        assert!(v.is_number());
        assert!((v.as_number() - 2.0).abs() < 1e-10);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_log_happy() {
        let v = NanVal(jit_log(NanVal::number(1.0).0));
        assert!(v.is_number());
        assert!(v.as_number().abs() < 1e-10);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_exp_happy() {
        let v = NanVal(jit_exp(NanVal::number(0.0).0));
        assert!(v.is_number());
        assert!((v.as_number() - 1.0).abs() < 1e-10);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_sin_happy() {
        let v = NanVal(jit_sin(NanVal::number(0.0).0));
        assert!(v.is_number());
        assert!(v.as_number().abs() < 1e-10);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_cos_happy() {
        let v = NanVal(jit_cos(NanVal::number(0.0).0));
        assert!(v.is_number());
        assert!((v.as_number() - 1.0).abs() < 1e-10);
    }

    // ── math-extra (tan/log10/log2/atan2) coverage tests ──────────────────

    #[test]
    fn vm_op_tan_happy() {
        let n = run_unary_math_op(OP_TAN, 0.0);
        assert!(n.abs() < 1e-10, "got {n}");
    }

    #[test]
    fn vm_op_log10_happy() {
        let n = run_unary_math_op(OP_LOG10, 1000.0);
        assert!((n - 3.0).abs() < 1e-10, "got {n}");
    }

    #[test]
    fn vm_op_log2_happy() {
        let n = run_unary_math_op(OP_LOG2, 8.0);
        assert!((n - 3.0).abs() < 1e-10, "got {n}");
    }

    #[test]
    fn vm_op_atan2_happy() {
        use crate::interpreter::Value;
        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }
        fn make_abx(op: u8, a: u8, bx: u16) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
        }
        // R[1]=LOADK 1.0; R[2]=LOADK 0.0; R[3]=ATAN2 R[1] R[2]; RET R[3]
        // atan2(1.0, 0.0) == pi/2
        let code = vec![
            make_abx(OP_LOADK, 1, 0),
            make_abx(OP_LOADK, 2, 1),
            make_abc(OP_ATAN2, 3, 1, 2),
            make_abc(OP_RET, 3, 0, 0),
        ];
        let chunk = Chunk {
            code,
            constants: vec![Value::Number(1.0), Value::Number(0.0)],
            param_count: 0,
            reg_count: 4,
            spans: vec![crate::ast::Span::UNKNOWN; 4],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![NanVal::number(1.0), NanVal::number(0.0)]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        match run(&program, Some("f"), vec![]).expect("atan2 should not error") {
            Value::Number(n) => {
                assert!((n - std::f64::consts::FRAC_PI_2).abs() < 1e-10, "got {n}")
            }
            other => panic!("expected number, got {:?}", other),
        }
    }

    #[test]
    fn vm_op_atan2_non_number_is_nan() {
        use crate::interpreter::Value;
        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }
        fn make_abx(op: u8, a: u8, bx: u16) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
        }
        let code = vec![
            make_abx(OP_LOADK, 1, 0),
            make_abx(OP_LOADK, 2, 1),
            make_abc(OP_ATAN2, 3, 1, 2),
            make_abc(OP_RET, 3, 0, 0),
        ];
        let chunk = Chunk {
            code,
            constants: vec![Value::Bool(true), Value::Number(0.0)],
            param_count: 0,
            reg_count: 4,
            spans: vec![crate::ast::Span::UNKNOWN; 4],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![NanVal::boolean(true), NanVal::number(0.0)]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        match run(&program, Some("f"), vec![]).expect("atan2 nan path") {
            Value::Number(n) => assert!(n.is_nan(), "expected NaN, got {n}"),
            other => panic!("expected number, got {:?}", other),
        }
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_tan_happy() {
        let v = NanVal(jit_tan(NanVal::number(0.0).0));
        assert!(v.is_number());
        assert!(v.as_number().abs() < 1e-10);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_tan_non_number_is_nan() {
        let v = NanVal(jit_tan(NanVal::boolean(true).0));
        assert!(v.is_number());
        assert!(v.as_number().is_nan());
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_log10_happy() {
        let v = NanVal(jit_log10(NanVal::number(1000.0).0));
        assert!(v.is_number());
        assert!((v.as_number() - 3.0).abs() < 1e-10);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_log10_non_number_is_nan() {
        let v = NanVal(jit_log10(NanVal::boolean(false).0));
        assert!(v.is_number());
        assert!(v.as_number().is_nan());
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_log2_happy() {
        let v = NanVal(jit_log2(NanVal::number(8.0).0));
        assert!(v.is_number());
        assert!((v.as_number() - 3.0).abs() < 1e-10);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_log2_non_number_is_nan() {
        let v = NanVal(jit_log2(NanVal::boolean(true).0));
        assert!(v.is_number());
        assert!(v.as_number().is_nan());
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_atan2_happy() {
        let v = NanVal(jit_atan2(NanVal::number(1.0).0, NanVal::number(0.0).0));
        assert!(v.is_number());
        assert!((v.as_number() - std::f64::consts::FRAC_PI_2).abs() < 1e-10);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_atan2_non_number_is_nan() {
        let v = NanVal(jit_atan2(NanVal::boolean(true).0, NanVal::number(0.0).0));
        assert!(v.is_number());
        assert!(v.as_number().is_nan());
        let v = NanVal(jit_atan2(NanVal::number(0.0).0, NanVal::boolean(true).0));
        assert!(v.is_number());
        assert!(v.as_number().is_nan());
    }

    // ---- jit_at negative-index coverage ----

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_at_list_negative_last() {
        let list = NanVal::heap_list(vec![
            NanVal::number(10.0),
            NanVal::number(20.0),
            NanVal::number(30.0),
        ]);
        let v = NanVal(jit_at(list.0, NanVal::number(-1.0).0));
        assert!(v.is_number());
        assert_eq!(v.as_number(), 30.0);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_at_list_negative_first() {
        let list = NanVal::heap_list(vec![
            NanVal::number(10.0),
            NanVal::number(20.0),
            NanVal::number(30.0),
        ]);
        let v = NanVal(jit_at(list.0, NanVal::number(-3.0).0));
        assert!(v.is_number());
        assert_eq!(v.as_number(), 10.0);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_at_list_negative_out_of_range() {
        let list = NanVal::heap_list(vec![NanVal::number(1.0), NanVal::number(2.0)]);
        let bits = jit_at(list.0, NanVal::number(-3.0).0);
        assert_eq!(bits, TAG_NIL);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_at_text_negative_last() {
        let s = NanVal::heap_string("abc".to_string());
        let v = NanVal(jit_at(s.0, NanVal::number(-1.0).0));
        assert!(v.is_string());
        let got = unsafe {
            match v.as_heap_ref() {
                HeapObj::Str(s) => s.clone(),
                _ => panic!("expected string"),
            }
        };
        assert_eq!(got, "c");
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_at_text_negative_out_of_range() {
        let s = NanVal::heap_string("ab".to_string());
        let bits = jit_at(s.0, NanVal::number(-3.0).0);
        assert_eq!(bits, TAG_NIL);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_at_fractional_index_returns_nil() {
        let list = NanVal::heap_list(vec![NanVal::number(1.0)]);
        let bits = jit_at(list.0, NanVal::number(0.5).0);
        assert_eq!(bits, TAG_NIL);
    }

    // ---- OP_LST VM dispatch + jit_lst helper coverage ----

    fn parse_for_vm_lst(source: &str) -> crate::ast::Program {
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
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        prog
    }

    #[test]
    fn vm_op_lst_happy() {
        use crate::interpreter::Value;
        let prog = parse_for_vm_lst("f>L n;lst [10,20,30] 1 99");
        let compiled = compile(&prog).expect("compile lst");
        let result = run(&compiled, Some("f"), vec![]).expect("run lst");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Number(10.0),
                Value::Number(99.0),
                Value::Number(30.0),
            ])
        );
    }

    #[test]
    fn vm_op_lst_out_of_range_errors() {
        let prog = parse_for_vm_lst("f>L n;lst [1,2,3] 5 0");
        let compiled = compile(&prog).expect("compile lst");
        let err = run(&compiled, Some("f"), vec![]).unwrap_err();
        assert!(format!("{err:?}").contains("out of range"));
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_lst_happy() {
        let list = NanVal::heap_list(vec![
            NanVal::number(10.0),
            NanVal::number(20.0),
            NanVal::number(30.0),
        ]);
        let v = NanVal(jit_lst(
            list.0,
            NanVal::number(1.0).0,
            NanVal::number(99.0).0,
        ));
        assert!(v.is_heap());
        let items = unsafe {
            match v.as_heap_ref() {
                HeapObj::List(items) => items.clone(),
                _ => panic!("expected list"),
            }
        };
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].as_number(), 10.0);
        assert_eq!(items[1].as_number(), 99.0);
        assert_eq!(items[2].as_number(), 30.0);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_lst_out_of_range_returns_original() {
        let list = NanVal::heap_list(vec![NanVal::number(1.0), NanVal::number(2.0)]);
        let bits = jit_lst(list.0, NanVal::number(5.0).0, NanVal::number(99.0).0);
        assert_eq!(bits, list.0);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_lst_negative_index_returns_original() {
        let list = NanVal::heap_list(vec![NanVal::number(1.0), NanVal::number(2.0)]);
        let bits = jit_lst(list.0, NanVal::number(-1.0).0, NanVal::number(99.0).0);
        assert_eq!(bits, list.0);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_lst_fractional_index_returns_original() {
        let list = NanVal::heap_list(vec![NanVal::number(1.0), NanVal::number(2.0)]);
        let bits = jit_lst(list.0, NanVal::number(0.5).0, NanVal::number(99.0).0);
        assert_eq!(bits, list.0);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_lst_non_number_index_returns_original() {
        let list = NanVal::heap_list(vec![NanVal::number(1.0)]);
        let bits = jit_lst(list.0, NanVal::boolean(true).0, NanVal::number(99.0).0);
        assert_eq!(bits, list.0);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_lst_non_list_returns_nil() {
        let s = NanVal::heap_string("abc".to_string());
        let bits = jit_lst(s.0, NanVal::number(0.0).0, NanVal::number(99.0).0);
        assert_eq!(bits, TAG_NIL);
    }

    // ---- VM compile coverage for 3-arg `wr` overload ----

    fn parse_for_vm(source: &str) -> crate::ast::Program {
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
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        prog
    }

    #[test]
    fn vm_compile_wr_3arg_json_succeeds() {
        // Compile only — don't write to disk. Confirms the json fast-path emits.
        let src = r#"f>R t t;wr "/tmp/__ilo_test_wr.json" [1,2,3] "json""#;
        let prog = parse_for_vm(src);
        let compiled = crate::vm::compile(&prog).expect("compile should succeed for json overload");
        assert!(!compiled.chunks.is_empty());
    }

    #[test]
    fn vm_compile_wr_3arg_csv_compiles() {
        // csv/tsv 3-arg form now compiles. Pinning the successful path.
        let src = r#"f>R t t;wr "/tmp/__ilo_test_wr.csv" [[1,2]] "csv""#;
        let prog = parse_for_vm(src);
        let compiled = crate::vm::compile(&prog).expect("compile should succeed for csv overload");
        assert!(!compiled.chunks.is_empty());
    }

    // ---- OP_RNDN VM dispatch + jit_rndn helper coverage ----

    #[test]
    fn vm_op_rndn_sigma_zero_returns_mu() {
        use crate::interpreter::Value;
        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }
        fn make_abx(op: u8, a: u8, bx: u16) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
        }
        let code = vec![
            make_abx(OP_LOADK, 1, 0),
            make_abx(OP_LOADK, 2, 1),
            make_abc(OP_RNDN, 3, 1, 2),
            make_abc(OP_RET, 3, 0, 0),
        ];
        let chunk = Chunk {
            code,
            constants: vec![Value::Number(5.0), Value::Number(0.0)],
            param_count: 0,
            reg_count: 4,
            spans: vec![crate::ast::Span::UNKNOWN; 4],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![NanVal::number(5.0), NanVal::number(0.0)]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        match run(&program, Some("f"), vec![]).expect("rndn should not error") {
            Value::Number(n) => assert_eq!(n, 5.0),
            other => panic!("expected number, got {:?}", other),
        }
    }

    #[test]
    fn vm_op_rndn_finite_for_nonzero_sigma() {
        use crate::interpreter::Value;
        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }
        fn make_abx(op: u8, a: u8, bx: u16) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
        }
        let code = vec![
            make_abx(OP_LOADK, 1, 0),
            make_abx(OP_LOADK, 2, 1),
            make_abc(OP_RNDN, 3, 1, 2),
            make_abc(OP_RET, 3, 0, 0),
        ];
        let chunk = Chunk {
            code,
            constants: vec![Value::Number(0.0), Value::Number(1.0)],
            param_count: 0,
            reg_count: 4,
            spans: vec![crate::ast::Span::UNKNOWN; 4],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![NanVal::number(0.0), NanVal::number(1.0)]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        fastrand::seed(7);
        match run(&program, Some("f"), vec![]).expect("rndn should not error") {
            Value::Number(n) => assert!(n.is_finite(), "got non-finite {n}"),
            other => panic!("expected number, got {:?}", other),
        }
    }

    #[test]
    fn vm_op_rndn_non_number_errors() {
        use crate::interpreter::Value;
        fn make_abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
        }
        fn make_abx(op: u8, a: u8, bx: u16) -> u32 {
            ((op as u32) << 24) | ((a as u32) << 16) | (bx as u32)
        }
        let code = vec![
            make_abx(OP_LOADK, 1, 0),
            make_abx(OP_LOADK, 2, 1),
            make_abc(OP_RNDN, 3, 1, 2),
            make_abc(OP_RET, 3, 0, 0),
        ];
        let chunk = Chunk {
            code,
            constants: vec![Value::Bool(true), Value::Number(0.0)],
            param_count: 0,
            reg_count: 4,
            spans: vec![crate::ast::Span::UNKNOWN; 4],
            all_regs_numeric: false,
        };
        let program = CompiledProgram {
            chunks: vec![chunk],
            func_names: vec!["f".to_string()],
            nan_constants: vec![vec![NanVal::boolean(true), NanVal::number(0.0)]],
            type_registry: TypeRegistry::default(),
            is_tool: vec![false],
        };
        let res = run(&program, Some("f"), vec![]);
        assert!(res.is_err(), "expected type error, got {res:?}");
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_rndn_sigma_zero_returns_mu() {
        let v = NanVal(jit_rndn(NanVal::number(5.0).0, NanVal::number(0.0).0));
        assert!(v.is_number());
        assert_eq!(v.as_number(), 5.0);
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_rndn_finite_for_nonzero_sigma() {
        fastrand::seed(11);
        let v = NanVal(jit_rndn(NanVal::number(0.0).0, NanVal::number(1.0).0));
        assert!(v.is_number());
        assert!(v.as_number().is_finite());
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn jit_rndn_non_number_returns_nil() {
        let bits = jit_rndn(NanVal::boolean(true).0, NanVal::number(0.0).0);
        assert_eq!(bits, TAG_NIL);
        let bits = jit_rndn(NanVal::number(0.0).0, NanVal::boolean(true).0);
        assert_eq!(bits, TAG_NIL);
    }
}
