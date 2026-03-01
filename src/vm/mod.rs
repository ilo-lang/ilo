use std::collections::HashMap;
use std::rc::Rc;
use crate::ast::*;
use crate::interpreter::Value;

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

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub(crate) mod jit_arm64;
#[cfg(feature = "cranelift")]
pub(crate) mod jit_cranelift;
#[cfg(feature = "llvm")]
pub(crate) mod jit_llvm;

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
pub(crate) const OP_ADDK_N: u8 = 33;  // R[A] = R[B] + K[C]
pub(crate) const OP_SUBK_N: u8 = 34;  // R[A] = R[B] - K[C]
pub(crate) const OP_MULK_N: u8 = 35;  // R[A] = R[B] * K[C]
pub(crate) const OP_DIVK_N: u8 = 36;  // R[A] = R[B] / K[C]

// ABC mode — builtins
pub(crate) const OP_LEN: u8 = 37;     // R[A] = len(R[B])
pub(crate) const OP_LISTAPPEND: u8 = 38; // R[A] = R[B] ++ [R[C]]
pub(crate) const OP_INDEX: u8 = 39;      // R[A] = R[B][C]  (C = literal index)
pub(crate) const OP_STR: u8 = 40;        // R[A] = str(R[B])  (number to text)
pub(crate) const OP_NUM: u8 = 41;        // R[A] = num(R[B])  (text to number, returns R n t)
pub(crate) const OP_ABS: u8 = 42;        // R[A] = abs(R[B])
pub(crate) const OP_MIN: u8 = 43;        // R[A] = min(R[B], R[C])
pub(crate) const OP_MAX: u8 = 44;        // R[A] = max(R[B], R[C])
pub(crate) const OP_FLR: u8 = 45;        // R[A] = floor(R[B])
pub(crate) const OP_CEL: u8 = 46;        // R[A] = ceil(R[B])
pub(crate) const OP_GET: u8 = 47;        // R[A] = http_get(R[B])  (returns R t t)
pub(crate) const OP_SPL: u8 = 48;        // R[A] = spl(R[B], R[C])  (split text by separator → L t)
pub(crate) const OP_CAT: u8 = 49;        // R[A] = cat(R[B], R[C])  (join list with separator → t)
pub(crate) const OP_HAS: u8 = 50;        // R[A] = has(R[B], R[C])  (membership test → b)
pub(crate) const OP_HD: u8 = 51;         // R[A] = hd(R[B])  (head of list/text)
pub(crate) const OP_TL: u8 = 52;         // R[A] = tl(R[B])  (tail of list/text)
pub(crate) const OP_REV: u8 = 53;        // R[A] = rev(R[B])  (reverse list or text)
pub(crate) const OP_SRT: u8 = 54;        // R[A] = srt(R[B])  (sort list or text)
pub(crate) const OP_SLC: u8 = 55;        // R[A] = slc(R[B], R[C], R[C+1])  (slice list or text)
pub(crate) const OP_JMPNN: u8 = 56;     // if R[A] is not nil, jump by signed Bx (ABx mode)

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

#[derive(Debug, Clone)]
pub struct Chunk {
    pub code: Vec<u32>,
    pub constants: Vec<Value>,
    #[allow(dead_code)] // available for introspection/debugging tools
    pub param_count: u8,
    pub reg_count: u8,
    pub spans: Vec<crate::ast::Span>,
}

impl Chunk {
    fn new(param_count: u8) -> Self {
        Chunk { code: Vec::new(), constants: Vec::new(), param_count, reg_count: param_count, spans: Vec::new() }
    }

    fn add_const(&mut self, val: Value) -> u16 {
        for (i, c) in self.constants.iter().enumerate() {
            match (c, &val) {
                (Value::Number(a), Value::Number(b)) if (a - b).abs() < f64::EPSILON => return i as u16,
                (Value::Text(a), Value::Text(b)) if a == b => return i as u16,
                (Value::Bool(a), Value::Bool(b)) if a == b => return i as u16,
                (Value::Nil, Value::Nil) => return i as u16,
                _ => {}
            }
        }
        let idx = self.constants.len();
        assert!(idx <= u16::MAX as usize, "constant pool overflow: more than 65535 constants in one function");
        self.constants.push(val);
        idx as u16
    }

    fn add_const_raw(&mut self, val: Value) -> u16 {
        let idx = self.constants.len();
        assert!(idx <= u16::MAX as usize, "constant pool overflow: more than 65535 constants in one function");
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

// ── Compiled program ─────────────────────────────────────────────────

pub struct CompiledProgram {
    pub chunks: Vec<Chunk>,
    pub func_names: Vec<String>,
    pub(crate) nan_constants: Vec<Vec<NanVal>>,
}

impl CompiledProgram {
    fn func_index(&self, name: &str) -> Option<u16> {
        self.func_names.iter().position(|n| n == name).map(|i| i as u16)
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
    reg_is_num: [bool; 256],  // track which registers are known numeric
    first_error: Option<CompileError>,
    current_span: crate::ast::Span,
    loop_stack: Vec<LoopContext>,
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
            first_error: None,
            current_span: crate::ast::Span::UNKNOWN,
            loop_stack: Vec::new(),
        }
    }

    fn alloc_reg(&mut self) -> u8 {
        assert!(self.next_reg < 255, "register overflow: function uses more than 255 registers");
        let r = self.next_reg;
        self.next_reg += 1;
        if self.next_reg > self.max_reg {
            self.max_reg = self.next_reg;
        }
        self.reg_is_num[r as usize] = false;
        r
    }


    fn resolve_local(&self, name: &str) -> Option<u8> {
        self.locals.iter().rev().find(|(n, _)| n == name).map(|(_, r)| *r)
    }

    fn add_local(&mut self, name: &str, reg: u8) {
        self.locals.push((name.to_string(), reg));
    }

    fn emit_abc(&mut self, op: u8, a: u8, b: u8, c: u8) -> usize {
        self.current.emit(encode_abc(op, a, b, c), self.current_span)
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

    fn compile_program(mut self, program: &Program) -> Result<CompiledProgram, CompileError> {
        for decl in &program.declarations {
            match decl {
                Decl::Function { name, .. } | Decl::Tool { name, .. } => {
                    self.func_names.push(name.clone());
                }
                Decl::TypeDef { .. } | Decl::Error { .. } => {}
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
                for (i, p) in params.iter().enumerate() {
                    self.add_local(&p.name, i as u8);
                    if p.ty == Type::Number {
                        self.reg_is_num[i] = true;
                    }
                }

                let result = self.compile_body(body);

                let ret_reg = result.unwrap_or_else(|| {
                    let r = self.alloc_reg();
                    let ki = self.current.add_const(Value::Nil);
                    self.emit_abx(OP_LOADK, r, ki);
                    r
                });

                // Only emit RET if last instruction isn't already RET
                let last_is_ret = self.current.code.last()
                    .map(|inst| (inst >> 24) as u8 == OP_RET)
                    .unwrap_or(false);
                if !last_is_ret {
                    self.emit_abx(OP_RET, ret_reg, 0);
                }

                self.current.reg_count = self.max_reg;
                self.chunks.push(self.current.clone());
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
                self.chunks.push(self.current.clone());
            } else {
                self.chunks.push(Chunk::new(0));
            }
        }

        if let Some(e) = self.first_error {
            return Err(e);
        }
        Ok(CompiledProgram { chunks: self.chunks, func_names: self.func_names, nan_constants: Vec::new() })
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
                    // Re-binding: compile value and move to existing register
                    let reg = self.compile_expr(value);
                    if reg != existing_reg {
                        self.emit_abc(OP_MOVE, existing_reg, reg, 0);
                    }
                } else {
                    let reg = self.compile_expr(value);
                    self.add_local(name, reg);
                }
                None
            }

            Stmt::Guard { condition, negated, body, else_body } => {
                let saved_next = self.next_reg;
                let cond_reg = self.compile_expr(condition);
                let jump = if *negated {
                    self.emit_jmpt(cond_reg)
                } else {
                    self.emit_jmpf(cond_reg)
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
                } else {
                    // Guard: cond{body} — early return
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

            Stmt::ForEach { binding, collection, body } => {
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

                let one_reg = self.alloc_reg();
                let one_ki = self.current.add_const(Value::Number(1.0));
                self.emit_abx(OP_LOADK, one_reg, one_ki);

                // Loop top
                let loop_top = self.current.code.len();
                self.emit_abc(OP_LISTGET, bind_reg, coll_reg, idx_reg);
                let exit_jump = self.emit_jmp_placeholder();

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
                    && br != last_reg {
                        self.emit_abc(OP_MOVE, last_reg, br, 0);
                    }

                // Patch continue jumps to idx increment
                let continue_target = self.current.code.len();
                if let Some(patches) = &self.loop_stack.last().unwrap().continue_patches {
                    let patches: Vec<usize> = patches.clone();
                    for patch in patches {
                        let offset = continue_target as isize - patch as isize - 1;
                        let encoded = encode_abx(OP_JMP, 0, offset as i16 as u16);
                        self.current.code[patch] = encoded;
                    }
                }

                // idx += 1
                self.emit_abc(OP_ADD, idx_reg, idx_reg, one_reg);

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
                    && br != last_reg {
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
                    self.loop_stack.last_mut().unwrap().break_patches.push(jmp);
                }
                None
            }

            Stmt::Continue => {
                if let Some(ctx) = self.loop_stack.last() {
                    if ctx.continue_patches.is_some() {
                        // Foreach: emit placeholder, patch later
                        let jmp = self.emit_jmp_placeholder();
                        self.loop_stack.last_mut().unwrap()
                            .continue_patches.as_mut().unwrap().push(jmp);
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
                        && br != result_reg {
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
                        && br != result_reg {
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
                        && br != result_reg {
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
                    };
                    let const_reg = self.alloc_reg();
                    let ki = self.current.add_const(val);
                    self.emit_abx(OP_LOADK, const_reg, ki);
                    let eq_reg = self.alloc_reg();
                    self.emit_abc(OP_EQ, eq_reg, sub_reg, const_reg);
                    let skip = self.emit_jmpf(eq_reg);

                    let body_result = self.compile_body(&arm.body);
                    if let Some(br) = body_result
                        && br != result_reg {
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
            && let Some(ref val) = Self::try_const_fold(expr) {
                let is_num = matches!(val, Value::Number(_));
                let reg = self.alloc_reg();
                let ki = self.current.add_const(val.clone());
                self.emit_abx(OP_LOADK, reg, ki);
                if is_num { self.reg_is_num[reg as usize] = true; }
                return reg;
            }

        match expr {
            Expr::Literal(lit) => {
                let is_num = matches!(lit, Literal::Number(_));
                let val = match lit {
                    Literal::Number(n) => Value::Number(*n),
                    Literal::Text(s) => Value::Text(s.clone()),
                    Literal::Bool(b) => Value::Bool(*b),
                };
                let reg = self.alloc_reg();
                let ki = self.current.add_const(val);
                self.emit_abx(OP_LOADK, reg, ki);
                if is_num { self.reg_is_num[reg as usize] = true; }
                reg
            }

            Expr::Ref(name) => {
                if let Some(reg) = self.resolve_local(name) {
                    reg // FREE — no instruction needed!
                } else {
                    self.first_error.get_or_insert(CompileError::UndefinedVariable { name: name.clone() });
                    0 // dummy register; compile continues to surface more errors
                }
            }

            Expr::Field { object, field, safe } => {
                let obj_reg = self.compile_expr(object);
                let ki = self.current.add_const(Value::Text(field.clone()));
                assert!(ki <= 255, "constant pool overflow: field name index {} exceeds 8-bit limit in OP_RECFLD", ki);
                if *safe {
                    // Safe nav: if nil, skip field access (obj_reg stays nil)
                    self.emit_abx(OP_JMPNN, obj_reg, 1); // not nil → skip JMP
                    self.emit_abx(OP_JMP, 0, 1);         // nil → skip RECFLD
                    self.emit_abc(OP_RECFLD, obj_reg, obj_reg, ki as u8);
                    obj_reg
                } else {
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_RECFLD, ra, obj_reg, ki as u8);
                    ra
                }
            }

            Expr::Index { object, index, safe } => {
                let obj_reg = self.compile_expr(object);
                assert!(*index <= 255, "index literal {} exceeds 8-bit limit in OP_INDEX", index);
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

            Expr::Call { function, args, unwrap } => {
                // Builtins — compile to dedicated opcodes
                if function == "len" && args.len() == 1 {
                    let rb = self.compile_expr(&args[0]);
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_LEN, ra, rb, 0);
                    self.reg_is_num[ra as usize] = true;
                    return ra;
                }
                if function == "str" && args.len() == 1 {
                    let rb = self.compile_expr(&args[0]);
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_STR, ra, rb, 0);
                    return ra;
                }
                if function == "num" && args.len() == 1 {
                    let rb = self.compile_expr(&args[0]);
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_NUM, ra, rb, 0);
                    return ra;
                }
                if function == "abs" && args.len() == 1 {
                    let rb = self.compile_expr(&args[0]);
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_ABS, ra, rb, 0);
                    self.reg_is_num[ra as usize] = true;
                    return ra;
                }
                if (function == "min" || function == "max") && args.len() == 2 {
                    let rb = self.compile_expr(&args[0]);
                    let rc = self.compile_expr(&args[1]);
                    let ra = self.alloc_reg();
                    let op = if function == "min" { OP_MIN } else { OP_MAX };
                    self.emit_abc(op, ra, rb, rc);
                    self.reg_is_num[ra as usize] = true;
                    return ra;
                }
                if (function == "flr" || function == "cel") && args.len() == 1 {
                    let rb = self.compile_expr(&args[0]);
                    let ra = self.alloc_reg();
                    let op = if function == "flr" { OP_FLR } else { OP_CEL };
                    self.emit_abc(op, ra, rb, 0);
                    self.reg_is_num[ra as usize] = true;
                    return ra;
                }
                if function == "spl" && args.len() == 2 {
                    let rb = self.compile_expr(&args[0]);
                    let rc = self.compile_expr(&args[1]);
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_SPL, ra, rb, rc);
                    return ra;
                }
                if function == "cat" && args.len() == 2 {
                    let rb = self.compile_expr(&args[0]);
                    let rc = self.compile_expr(&args[1]);
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_CAT, ra, rb, rc);
                    return ra;
                }
                if function == "has" && args.len() == 2 {
                    let rb = self.compile_expr(&args[0]);
                    let rc = self.compile_expr(&args[1]);
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_HAS, ra, rb, rc);
                    return ra;
                }
                if function == "hd" && args.len() == 1 {
                    let rb = self.compile_expr(&args[0]);
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_HD, ra, rb, 0);
                    return ra;
                }
                if function == "tl" && args.len() == 1 {
                    let rb = self.compile_expr(&args[0]);
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_TL, ra, rb, 0);
                    return ra;
                }
                if function == "rev" && args.len() == 1 {
                    let rb = self.compile_expr(&args[0]);
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_REV, ra, rb, 0);
                    return ra;
                }
                if function == "srt" && args.len() == 1 {
                    let rb = self.compile_expr(&args[0]);
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_SRT, ra, rb, 0);
                    return ra;
                }
                if function == "slc" && args.len() == 3 {
                    let rb = self.compile_expr(&args[0]);
                    let rc = self.compile_expr(&args[1]);
                    let rd = self.compile_expr(&args[2]);
                    debug_assert_eq!(rd, rc + 1, "slc args should be consecutive regs");
                    let ra = self.alloc_reg();
                    self.emit_abc(OP_SLC, ra, rb, rc);
                    return ra;
                }
                if function == "get" && args.len() == 1 {
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

                let arg_regs: Vec<u8> = args.iter().map(|a| self.compile_expr(a)).collect();
                let func_idx = self.func_names.iter().position(|n| n == function)
                    .unwrap_or_else(|| {
                        self.first_error.get_or_insert(CompileError::UndefinedFunction { name: function.clone() });
                        0 // dummy index; compile continues to surface more errors
                    });

                let a = self.alloc_reg(); // result register
                // Reserve slots for args
                let args_base = self.next_reg;
                assert!((self.next_reg as usize) + args.len() <= 255, "register overflow: call requires too many register slots");
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

                assert!(func_idx <= 255, "too many functions: function index {} exceeds 8-bit limit in OP_CALL", func_idx);
                let bx = ((func_idx as u16) << 8) | args.len() as u16;
                self.emit_abx(OP_CALL, a, bx);

                // After call, only the result register is live
                self.next_reg = a + 1;

                // Auto-unwrap: Ok(v)→v, Err(e)→return Err to caller
                if *unwrap {
                    let check_reg = self.alloc_reg();
                    self.emit_abc(OP_ISOK, check_reg, a, 0);
                    let skip_ret = self.emit_jmpt(check_reg);
                    self.emit_abx(OP_RET, a, 0);        // propagate Err
                    self.current.patch_jump(skip_ret);
                    self.emit_abc(OP_UNWRAP, a, a, 0);   // extract Ok inner
                    self.next_reg = a + 1; // only result register live
                }

                a
            }

            Expr::BinOp { op, left, right } => {
                // Try superinstructions: register op constant (right is number literal)
                let is_arith = matches!(op, BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide);
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
                                    BinOp::Divide => OP_DIVK_N,
                                    _ => unreachable!(),
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
                        && let Expr::Literal(Literal::Number(n)) = left.as_ref() {
                            let rc = self.compile_expr(right);
                            if self.reg_is_num[rc as usize] {
                                let ki = self.current.add_const(Value::Number(*n));
                                if ki <= 255 {
                                    let ra = self.alloc_reg();
                                    let opcode = match op {
                                        BinOp::Add => OP_ADDK_N,
                                        BinOp::Multiply => OP_MULK_N,
                                        _ => unreachable!(),
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
                    let jump = if *op == BinOp::And {
                        self.emit_jmpf(ra)
                    } else {
                        self.emit_jmpt(ra)
                    };
                    let rb = self.compile_expr(right);
                    // Move result of right into ra so the result register is consistent
                    if rb != ra {
                        self.emit_abc(OP_MOVE, ra, rb, 0);
                    }
                    self.current.patch_jump(jump);
                    return ra;
                }

                let rb = self.compile_expr(left);
                let rc = self.compile_expr(right);
                let both_num = self.reg_is_num[rb as usize] && self.reg_is_num[rc as usize];

                // Use type-specialized opcodes when both operands are known numeric
                let (opcode, result_is_num) = match op {
                    BinOp::Add if both_num => (OP_ADD_NN, true),
                    BinOp::Subtract if both_num => (OP_SUB_NN, true),
                    BinOp::Multiply if both_num => (OP_MUL_NN, true),
                    BinOp::Divide if both_num => (OP_DIV_NN, true),
                    BinOp::Add => (OP_ADD, false),
                    BinOp::Subtract => (OP_SUB, false),
                    BinOp::Multiply => (OP_MUL, false),
                    BinOp::Divide => (OP_DIV, false),
                    BinOp::Equals => (OP_EQ, false),
                    BinOp::NotEquals => (OP_NE, false),
                    BinOp::GreaterThan => (OP_GT, false),
                    BinOp::LessThan => (OP_LT, false),
                    BinOp::GreaterOrEqual => (OP_GE, false),
                    BinOp::LessOrEqual => (OP_LE, false),
                    BinOp::Append => (OP_LISTAPPEND, false),
                    BinOp::And | BinOp::Or => unreachable!("handled above"),
                };
                let ra = self.alloc_reg();
                self.emit_abc(opcode, ra, rb, rc);
                if result_is_num { self.reg_is_num[ra as usize] = true; }
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
                assert!((self.next_reg as usize) + items.len() <= 255, "register overflow: list literal requires too many register slots");
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
                let field_regs: Vec<u8> = fields.iter()
                    .map(|(_, val_expr)| self.compile_expr(val_expr))
                    .collect();

                let desc = Value::List(vec![
                    Value::Text(type_name.clone()),
                    Value::List(fields.iter().map(|(n, _)| Value::Text(n.clone())).collect()),
                ]);
                let desc_idx = self.current.add_const_raw(desc);

                let a = self.alloc_reg(); // result register
                let fields_base = self.next_reg;
                assert!((self.next_reg as usize) + fields.len() <= 255, "register overflow: record literal requires too many register slots");
                self.next_reg += fields.len() as u8;
                if self.next_reg > self.max_reg {
                    self.max_reg = self.next_reg;
                }

                for (i, &field_reg) in field_regs.iter().enumerate() {
                    let target = fields_base + i as u8;
                    if field_reg != target {
                        self.emit_abc(OP_MOVE, target, field_reg, 0);
                    }
                }

                assert!(desc_idx <= 255, "constant pool overflow: record descriptor index {} exceeds 8-bit limit in OP_RECNEW", desc_idx);
                let bx = ((desc_idx as u16) << 8) | fields.len() as u16;
                self.emit_abx(OP_RECNEW, a, bx);
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
            Expr::With { object, updates } => {
                let obj_reg = self.compile_expr(object);
                let update_regs: Vec<u8> = updates.iter()
                    .map(|(_, val_expr)| self.compile_expr(val_expr))
                    .collect();

                let names = Value::List(
                    updates.iter().map(|(n, _)| Value::Text(n.clone())).collect()
                );
                let names_idx = self.current.add_const_raw(names);

                let a = self.alloc_reg(); // result register
                let updates_base = self.next_reg;
                assert!((self.next_reg as usize) + updates.len() <= 255, "register overflow: 'with' expression requires too many register slots");
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

                assert!(names_idx <= 255, "constant pool overflow: field names index {} exceeds 8-bit limit in OP_RECWITH", names_idx);
                let bx = (names_idx << 8) | updates.len() as u16;
                self.emit_abx(OP_RECWITH, a, bx);
                a
            }
        }
    }
}

// ── NaN-boxed value ──────────────────────────────────────────────────
//
// IEEE 754 quiet NaN has 51 unused payload bits. We use them to encode
// all ilo value types in a single Copy u64, making the VM stack
// Vec<u64>-equivalent with zero-cost number operations.

const QNAN: u64       = 0x7FFC_0000_0000_0000;
const TAG_NIL: u64    = QNAN;
const TAG_TRUE: u64   = QNAN | 1;
const TAG_FALSE: u64  = QNAN | 2;
const TAG_STRING: u64 = 0x7FFD_0000_0000_0000;
const TAG_LIST: u64   = 0x7FFE_0000_0000_0000;
const TAG_RECORD: u64 = 0x7FFF_0000_0000_0000;
const TAG_OK: u64     = 0xFFFC_0000_0000_0000;
const TAG_ERR: u64    = 0xFFFD_0000_0000_0000;
const PTR_MASK: u64   = 0x0000_FFFF_FFFF_FFFF;
const TAG_MASK: u64   = 0xFFFF_0000_0000_0000;

enum HeapObj {
    Str(String),
    List(Vec<NanVal>),
    Record { type_name: String, fields: HashMap<String, NanVal> },
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
            HeapObj::Record { fields, .. } => {
                for val in fields.values() {
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
pub(crate) struct NanVal(u64);

impl NanVal {
    #[inline]
    pub(crate) fn number(n: f64) -> Self {
        if n.is_nan() {
            NanVal(0x7FF8_0000_0000_0000) // canonical NaN, outside our tag space
        } else {
            NanVal(n.to_bits())
        }
    }

    #[inline]
    fn nil() -> Self { NanVal(TAG_NIL) }

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

    fn heap_record(type_name: String, fields: HashMap<String, NanVal>) -> Self {
        let rc = Rc::new(HeapObj::Record { type_name, fields });
        let ptr = Rc::into_raw(rc) as u64;
        NanVal(TAG_RECORD | (ptr & PTR_MASK))
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
        (self.0 & QNAN) == QNAN && self.0 != TAG_NIL && self.0 != TAG_TRUE && self.0 != TAG_FALSE
    }

    #[inline]
    fn is_string(self) -> bool {
        (self.0 & TAG_MASK) == TAG_STRING
    }

    /// # Safety
    /// Caller must ensure `self` was created via one of the `heap_*` constructors
    /// (i.e. `is_heap()` returns true) and that the underlying `Rc<HeapObj>` is
    /// still alive — i.e. the strong count has not reached zero. The returned
    /// reference borrows the heap allocation; its lifetime is bounded by the
    /// caller's knowledge of the RC lifetime, not by `'a`. Callers must not
    /// hold the reference across any operation that could decrement the RC to zero.
    #[inline]
    unsafe fn as_heap_ref<'a>(self) -> &'a HeapObj {
        let ptr = (self.0 & PTR_MASK) as *const HeapObj;
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
            unsafe { Rc::increment_strong_count(ptr); }
        }
    }

    #[inline(always)]
    fn drop_rc(self) {
        if self.is_heap() {
            let ptr = (self.0 & PTR_MASK) as *const HeapObj;
            // SAFETY: is_heap() guarantees this pointer was produced by Rc::into_raw.
            // Decrementing mirrors every clone_rc call; the VM is responsible for
            // pairing increments and decrements correctly.
            unsafe { Rc::decrement_strong_count(ptr); }
        }
    }

    pub(crate) fn from_value(val: &Value) -> Self {
        match val {
            Value::Number(n) => NanVal::number(*n),
            Value::Bool(b) => NanVal::boolean(*b),
            Value::Nil => NanVal::nil(),
            Value::Text(s) => NanVal::heap_string(s.clone()),
            Value::List(items) => {
                NanVal::heap_list(items.iter().map(NanVal::from_value).collect())
            }
            Value::Record { type_name, fields } => {
                NanVal::heap_record(
                    type_name.clone(),
                    fields.iter().map(|(k, v)| (k.clone(), NanVal::from_value(v))).collect(),
                )
            }
            Value::Ok(inner) => NanVal::heap_ok(NanVal::from_value(inner)),
            Value::Err(inner) => NanVal::heap_err(NanVal::from_value(inner)),
        }
    }

    pub(crate) fn to_value(self) -> Value {
        if self.is_number() {
            return Value::Number(self.as_number());
        }
        match self.0 {
            TAG_NIL => Value::Nil,
            TAG_TRUE => Value::Bool(true),
            TAG_FALSE => Value::Bool(false),
            _ => unsafe {
                // SAFETY: Not a number, nil, true, or false — must be a heap-tagged
                // pointer. The NanVal was created by a heap_* constructor so the
                // Rc is still live (we own this NanVal value).
                debug_assert!(self.is_heap(), "to_value: unexpected non-heap NanVal tag {:#018x}", self.0);
                match self.as_heap_ref() {
                    HeapObj::Str(s) => Value::Text(s.clone()),
                    HeapObj::List(items) => {
                        Value::List(items.iter().map(|v| v.to_value()).collect())
                    }
                    HeapObj::Record { type_name, fields } => Value::Record {
                        type_name: type_name.clone(),
                        fields: fields.iter().map(|(k, v)| (k.clone(), v.to_value())).collect(),
                    },
                    HeapObj::OkVal(inner) => Value::Ok(Box::new(inner.to_value())),
                    HeapObj::ErrVal(inner) => Value::Err(Box::new(inner.to_value())),
                }
            }
        }
    }
}

// ── VM ───────────────────────────────────────────────────────────────

pub fn compile(program: &Program) -> Result<CompiledProgram, CompileError> {
    let mut prog = RegCompiler::new().compile_program(program)?;
    prog.nan_constants = prog.chunks.iter()
        .map(|chunk| chunk.constants.iter().map(NanVal::from_value).collect())
        .collect();
    Ok(prog)
}

pub fn run(compiled: &CompiledProgram, func_name: Option<&str>, args: Vec<Value>) -> Result<Value, VmRuntimeError> {
    let target = match func_name {
        Some(name) => name.to_string(),
        None => compiled.func_names.first().ok_or_else(|| VmRuntimeError {
            error: VmError::NoFunctionsDefined,
            span: None,
            call_stack: Vec::new(),
        })?.clone(),
    };
    let func_idx = compiled.func_index(&target)
        .ok_or_else(|| VmRuntimeError {
            error: VmError::UndefinedFunction { name: target.clone() },
            span: None,
            call_stack: Vec::new(),
        })?;
    VM::new(compiled).call(func_idx, args)
}

#[cfg(test)]
pub fn compile_and_run(program: &Program, func_name: Option<&str>, args: Vec<Value>) -> Result<Value, Box<dyn std::error::Error>> {
    let compiled = compile(program)?;
    Ok(run(&compiled, func_name, args).map_err(|e| e.error)?)
}

/// Reusable VM handle — avoids re-allocating stack/frames per call.
pub struct VmState<'a> {
    vm: VM<'a>,
}

impl<'a> VmState<'a> {
    pub fn new(compiled: &'a CompiledProgram) -> Self {
        VmState { vm: VM::new(compiled) }
    }

    pub fn call(&mut self, func_name: &str, args: Vec<Value>) -> VmResult<Value> {
        for v in self.vm.stack.drain(..) {
            v.drop_rc();
        }
        self.vm.frames.clear();

        let func_idx = self.vm.program.func_index(func_name)
            .ok_or_else(|| VmError::UndefinedFunction { name: func_name.to_string() })?;
        let nan_args: Vec<NanVal> = args.iter().map(NanVal::from_value).collect();
        self.vm.setup_call(func_idx, nan_args, 0);
        self.vm.execute()  // returns VmError for bench compatibility
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
    /// Last dispatched instruction position — for error span capture.
    last_ci: usize,
    last_ip: usize,
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
        VM { program, stack: Vec::with_capacity(256), frames: Vec::with_capacity(64), last_ci: 0, last_ip: 0 }
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
        let span = self.program.chunks.get(self.last_ci)
            .and_then(|chunk| chunk.spans.get(self.last_ip))
            .copied()
            .filter(|s| *s != crate::ast::Span::UNKNOWN);
        let call_stack: Vec<String> = self.frames.iter()
            .filter_map(|f| self.program.func_names.get(f.chunk_idx as usize).cloned())
            .collect();
        VmRuntimeError { error, span, call_stack }
    }

    // reg!/reg_set! carry their own unsafe {} — clippy flags them as redundant when
    // expanded inside an outer unsafe {} site. The inner unsafe is intentional as
    // documentation; allow the lint here.
    #[allow(unused_unsafe)]
    fn execute(&mut self) -> VmResult<Value> {
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
                let target = f.stack_base + self.frames.last().map(|f| f.result_reg).unwrap_or(0) as usize;
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
            // Track position for error span capture (before incrementing ip).
            self.last_ci = ci;
            self.last_ip = ip;
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
                }
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
                }
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
                        let result = unsafe {
                            // SAFETY: is_string() confirmed both are heap-tagged string
                            // pointers with live RC counts (loaded from valid registers).
                            let sb = match bv.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() };
                            let sc = match cv.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() };
                            let mut out = String::with_capacity(sb.len() + sc.len());
                            out.push_str(sb);
                            out.push_str(sc);
                            NanVal::heap_string(out)
                        };
                        reg_set!(a, result);
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
                            return Err(VmError::Type("cannot add non-matching types"));
                        }
                    } else {
                        return Err(VmError::Type("cannot add non-matching types"));
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
                        return Err(VmError::Type("cannot subtract non-numbers"));
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
                        return Err(VmError::Type("cannot multiply non-numbers"));
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
                            return Err(VmError::DivisionByZero);
                        }
                        reg_set!(a, NanVal::number(bv.as_number() / dv));
                    } else {
                        return Err(VmError::Type("cannot divide non-numbers"));
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
                        let result = unsafe { nanval_str_cmp(bv, cv) == std::cmp::Ordering::Greater };
                        reg_set!(a, NanVal::boolean(result));
                    } else {
                        return Err(VmError::Type("cannot compare > : operands must be same type (n or t)"));
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
                        return Err(VmError::Type("cannot compare < : operands must be same type (n or t)"));
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
                        return Err(VmError::Type("cannot compare >= : operands must be same type (n or t)"));
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
                        let result = unsafe { nanval_str_cmp(bv, cv) != std::cmp::Ordering::Greater };
                        reg_set!(a, NanVal::boolean(result));
                    } else {
                        return Err(VmError::Type("cannot compare <= : operands must be same type (n or t)"));
                    }
                }
                OP_MOVE => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_number() { v.clone_rc(); }
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
                        return Err(VmError::Type("cannot negate non-number"));
                    }
                }
                OP_WRAPOK => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_number() { v.clone_rc(); }
                    reg_set!(a, NanVal::heap_ok(v));
                }
                OP_WRAPERR => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_number() { v.clone_rc(); }
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
                            _ => return Err(VmError::Type("unwrap on non-Ok/Err")),
                        }
                    };
                    reg_set!(a, inner);
                }
                OP_RECFLD => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize;
                    // SAFETY: ci is a valid chunk index (same invariant as loop header).
                    let chunk = unsafe { self.program.chunks.get_unchecked(ci) };
                    let field_name = match &chunk.constants[c] {
                        Value::Text(s) => s.as_str(),
                        _ => return Err(VmError::Type("RecordField expects string constant")),
                    };
                    let record = reg!(b);
                    // SAFETY: OP_RECFLD is only emitted by the compiler for record
                    // field accesses on values the type-checker knows are records.
                    // The debug_assert catches compiler bugs; the runtime _ arm
                    // provides a safe fallback for release builds with malformed bytecode.
                    debug_assert!(record.is_heap(), "OP_RECFLD on non-heap value");
                    let field_val = unsafe {
                        match record.as_heap_ref() {
                            HeapObj::Record { fields, .. } => {
                                match fields.get(field_name) {
                                    Some(&val) => {
                                        val.clone_rc();
                                        val
                                    }
                                    None => return Err(VmError::FieldNotFound { field: field_name.to_string() }),
                                }
                            }
                            _ => return Err(VmError::Type("field access on non-record")),
                        }
                    };
                    reg_set!(a, field_val);
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
                                    return Err(VmError::Type("list index out of bounds"));
                                }
                            }
                            _ => return Err(VmError::Type("index access on non-list")),
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
                        return Err(VmError::Type("foreach requires a list"));
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
                                _ => return Err(VmError::Type("foreach requires a list")),
                            }
                        }
                    } else {
                        return Err(VmError::Type("list index must be a number"));
                    }
                }
                OP_LOADK => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let bx = (inst & 0xFFFF) as usize;
                    // SAFETY: bx is the constant pool index encoded in the instruction;
                    // the compiler only emits indices < constants.len().
                    let v = unsafe { *nan_consts.get_unchecked(bx) };
                    if !v.is_number() { v.clone_rc(); }
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

                    let mut args = Vec::with_capacity(n_args);
                    for i in 0..n_args {
                        let v = reg!(base + a as usize + 1 + i);
                        if !v.is_number() { v.clone_rc(); }
                        args.push(v);
                    }

                    self.setup_call(func_idx, args, a);

                    // SAFETY: setup_call just pushed a new frame above.
                    let f = unsafe { self.frames.last().unwrap_unchecked() };
                    ci = f.chunk_idx as usize;
                    ip = f.ip;
                    base = f.stack_base;
                }
                OP_RET => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let result = reg!(a);
                    if !result.is_number() { result.clone_rc(); }

                    // SAFETY: frames is non-empty while execute() is running.
                    let result_reg = unsafe { self.frames.last().unwrap_unchecked() }.result_reg;

                    for i in base..self.stack.len() {
                        // SAFETY: i is in range base..self.stack.len() by loop bounds.
                        unsafe { self.stack.get_unchecked(i) }.drop_rc();
                    }
                    self.stack.truncate(base);
                    self.frames.pop();

                    if self.frames.is_empty() {
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
                OP_RECNEW => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let bx = (inst & 0xFFFF) as usize;
                    let desc_idx = bx >> 8;
                    // n_fields (bx & 0xFF) is encoded for future validation; field count
                    // is derived at runtime from field_names returned by unpack_record_desc.

                    // SAFETY: ci is a valid chunk index (same invariant as loop header).
                    let chunk = unsafe { self.program.chunks.get_unchecked(ci) };
                    let desc = chunk.constants[desc_idx].clone();
                    let (type_name, field_names) = unpack_record_desc(desc)?;

                    let mut fields = HashMap::new();
                    for (i, name) in field_names.into_iter().enumerate() {
                        let v = reg!(a + 1 + i);
                        v.clone_rc();
                        fields.insert(name, v);
                    }

                    reg_set!(a, NanVal::heap_record(type_name, fields));
                }
                OP_RECWITH => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let bx = (inst & 0xFFFF) as usize;
                    let names_idx = bx >> 8;
                    // n_updates (bx & 0xFF) is encoded for future validation; update count
                    // is derived at runtime from field_names returned by unpack_string_list.

                    // SAFETY: ci is a valid chunk index (same invariant as loop header).
                    let chunk = unsafe { self.program.chunks.get_unchecked(ci) };
                    let field_names = unpack_string_list(&chunk.constants[names_idx])?;

                    let old_record = reg!(a);
                    // SAFETY: OP_RECWITH is only emitted by the compiler for `with`
                    // expressions on values the type-checker knows are records.
                    // The debug_assert catches compiler bugs; the runtime _ arm
                    // provides a safe fallback for release builds with malformed bytecode.
                    debug_assert!(old_record.is_heap(), "OP_RECWITH on non-heap value");
                    let new_record = unsafe {
                        match old_record.as_heap_ref() {
                            HeapObj::Record { type_name, fields } => {
                                let mut new_fields = HashMap::new();
                                for (k, v) in fields {
                                    v.clone_rc();
                                    new_fields.insert(k.clone(), *v);
                                }
                                for (i, name) in field_names.into_iter().enumerate() {
                                    let val = reg!(a + 1 + i);
                                    val.clone_rc();
                                    if let Some(old_val) = new_fields.insert(name, val) {
                                        old_val.drop_rc();
                                    }
                                }
                                NanVal::heap_record(type_name.clone(), new_fields)
                            }
                            _ => return Err(VmError::Type("'with' requires a record")),
                        }
                    };
                    reg_set!(a, new_record);
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
                    let result = NanVal::number(reg!(b).as_number() + kv.as_number());
                    unsafe { *self.stack.as_mut_ptr().add(a) = result; }
                }
                OP_SUBK_N => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize;
                    // SAFETY: same as OP_ADDK_N.
                    let kv = unsafe { *nan_consts.get_unchecked(c) };
                    let result = NanVal::number(reg!(b).as_number() - kv.as_number());
                    unsafe { *self.stack.as_mut_ptr().add(a) = result; }
                }
                OP_MULK_N => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize;
                    // SAFETY: same as OP_ADDK_N.
                    let kv = unsafe { *nan_consts.get_unchecked(c) };
                    let result = NanVal::number(reg!(b).as_number() * kv.as_number());
                    unsafe { *self.stack.as_mut_ptr().add(a) = result; }
                }
                OP_DIVK_N => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize;
                    // SAFETY: same as OP_ADDK_N.
                    let kv = unsafe { *nan_consts.get_unchecked(c) };
                    let dv = kv.as_number();
                    if dv == 0.0 {
                        return Err(VmError::DivisionByZero);
                    }
                    let result = NanVal::number(reg!(b).as_number() / dv);
                    unsafe { *self.stack.as_mut_ptr().add(a) = result; }
                }
                OP_ADD_NN => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: a, b, c are all base + register offsets within pre-allocated stack slots.
                    let result = NanVal::number(reg!(b).as_number() + reg!(c).as_number());
                    unsafe { *self.stack.as_mut_ptr().add(a) = result; }
                }
                OP_SUB_NN => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: a, b, c are base + register offsets within pre-allocated stack slots.
                    let result = NanVal::number(reg!(b).as_number() - reg!(c).as_number());
                    unsafe { *self.stack.as_mut_ptr().add(a) = result; }
                }
                OP_MUL_NN => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: same as OP_SUB_NN.
                    let result = NanVal::number(reg!(b).as_number() * reg!(c).as_number());
                    unsafe { *self.stack.as_mut_ptr().add(a) = result; }
                }
                OP_DIV_NN => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    // SAFETY: same as OP_SUB_NN.
                    let dv = reg!(c).as_number();
                    if dv == 0.0 {
                        return Err(VmError::DivisionByZero);
                    }
                    let result = NanVal::number(reg!(b).as_number() / dv);
                    unsafe { *self.stack.as_mut_ptr().add(a) = result; }
                }
                OP_LEN => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    let length = if v.is_string() {
                        // SAFETY: is_string() confirmed heap-tagged string with live RC.
                        let s = unsafe { match v.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() } };
                        s.len() as f64
                    } else if v.is_heap() {
                        // SAFETY: is_heap() confirmed heap-tagged with live RC.
                        match unsafe { v.as_heap_ref() } {
                            HeapObj::List(items) => items.len() as f64,
                            _ => return Err(VmError::Type("len requires string or list")),
                        }
                    } else {
                        return Err(VmError::Type("len requires string or list"));
                    };
                    reg_set!(a, NanVal::number(length));
                }
                OP_STR => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_number() {
                        return Err(VmError::Type("str requires a number"));
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
                        return Err(VmError::Type("num requires a string"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let s = unsafe { match v.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() } };
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
                        return Err(VmError::Type("abs requires a number"));
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
                        return Err(VmError::Type("min/max require numbers"));
                    }
                    let nb = vb.as_number();
                    let nc = vc.as_number();
                    let result = if op == OP_MIN { nb.min(nc) } else { nb.max(nc) };
                    reg_set!(a, NanVal::number(result));
                }
                OP_FLR | OP_CEL => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_number() {
                        return Err(VmError::Type("flr/cel requires a number"));
                    }
                    let n = v.as_number();
                    let result = if op == OP_FLR { n.floor() } else { n.ceil() };
                    reg_set!(a, NanVal::number(result));
                }
                OP_GET => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if !v.is_string() {
                        return Err(VmError::Type("get requires a string"));
                    }
                    #[cfg(feature = "http")]
                    let result = {
                        // SAFETY: is_string() confirmed heap-tagged string with live RC.
                        let url = unsafe { match v.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() } };
                        match minreq::get(url.as_str()).send() {
                            Ok(resp) => match resp.as_str() {
                                Ok(body) => NanVal::heap_ok(NanVal::heap_string(body.to_string())),
                                Err(e) => NanVal::heap_err(NanVal::heap_string(format!("response is not valid UTF-8: {e}"))),
                            },
                            Err(e) => NanVal::heap_err(NanVal::heap_string(e.to_string())),
                        }
                    };
                    #[cfg(not(feature = "http"))]
                    let result = NanVal::heap_err(NanVal::heap_string("http feature not enabled".to_string()));
                    reg_set!(a, result);
                }
                OP_SPL => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    if !vb.is_string() || !vc.is_string() {
                        return Err(VmError::Type("spl requires two strings"));
                    }
                    // SAFETY: is_string() confirmed heap-tagged string with live RC.
                    let text = unsafe { match vb.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() } };
                    let sep = unsafe { match vc.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() } };
                    let items: Vec<NanVal> = text.split(sep.as_str())
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
                        return Err(VmError::Type("cat requires a text separator"));
                    }
                    if !vb.is_heap() {
                        return Err(VmError::Type("cat requires a list"));
                    }
                    let sep = unsafe { match vc.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() } };
                    let items = unsafe { match vb.as_heap_ref() { HeapObj::List(l) => l, _ => return Err(VmError::Type("cat requires a list")) } };
                    let mut parts = Vec::with_capacity(items.len());
                    for item in items {
                        if !item.is_string() {
                            return Err(VmError::Type("cat: list items must be text"));
                        }
                        let s = unsafe { match item.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() } };
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
                            return Err(VmError::Type("has: text search requires text needle"));
                        }
                        unsafe {
                            let haystack = match collection.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() };
                            let needle_s = match needle.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() };
                            haystack.contains(needle_s.as_str())
                        }
                    } else if collection.is_heap() {
                        match unsafe { collection.as_heap_ref() } {
                            HeapObj::List(items) => {
                                items.iter().any(|item| nanval_equal(*item, needle))
                            }
                            _ => return Err(VmError::Type("has requires a list or text")),
                        }
                    } else {
                        return Err(VmError::Type("has requires a list or text"));
                    };
                    reg_set!(a, NanVal::boolean(found));
                }
                OP_HD => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    let result = if v.is_string() {
                        let s = unsafe { match v.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() } };
                        if s.is_empty() {
                            return Err(VmError::Type("hd: empty text"));
                        }
                        NanVal::heap_string(s.chars().next().unwrap().to_string())
                    } else if v.is_heap() {
                        match unsafe { v.as_heap_ref() } {
                            HeapObj::List(items) => {
                                if items.is_empty() {
                                    return Err(VmError::Type("hd: empty list"));
                                }
                                items[0].clone_rc();
                                items[0]
                            }
                            _ => return Err(VmError::Type("hd requires a list or text")),
                        }
                    } else {
                        return Err(VmError::Type("hd requires a list or text"));
                    };
                    reg_set!(a, result);
                }
                OP_TL => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    let result = if v.is_string() {
                        let s = unsafe { match v.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() } };
                        if s.is_empty() {
                            return Err(VmError::Type("tl: empty text"));
                        }
                        let mut chars = s.chars();
                        chars.next();
                        NanVal::heap_string(chars.collect())
                    } else if v.is_heap() {
                        match unsafe { v.as_heap_ref() } {
                            HeapObj::List(items) => {
                                if items.is_empty() {
                                    return Err(VmError::Type("tl: empty list"));
                                }
                                let tail: Vec<NanVal> = items[1..].iter().map(|item| {
                                    item.clone_rc();
                                    *item
                                }).collect();
                                NanVal::heap_list(tail)
                            }
                            _ => return Err(VmError::Type("tl requires a list or text")),
                        }
                    } else {
                        return Err(VmError::Type("tl requires a list or text"));
                    };
                    reg_set!(a, result);
                }
                OP_REV => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    let result = if v.is_string() {
                        let s = unsafe { match v.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() } };
                        NanVal::heap_string(s.chars().rev().collect::<String>())
                    } else if v.is_heap() {
                        match unsafe { v.as_heap_ref() } {
                            HeapObj::List(items) => {
                                let mut reversed: Vec<NanVal> = items.iter().map(|item| { item.clone_rc(); *item }).collect();
                                reversed.reverse();
                                NanVal::heap_list(reversed)
                            }
                            _ => return Err(VmError::Type("rev requires a list or text")),
                        }
                    } else {
                        return Err(VmError::Type("rev requires a list or text"));
                    };
                    reg_set!(a, result);
                }
                OP_SRT => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let v = reg!(b);
                    if v.is_string() {
                        let s = unsafe { match v.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() } };
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
                                        let mut sorted: Vec<NanVal> = items.iter().map(|v| { v.clone_rc(); *v }).collect();
                                        sorted.sort_by(|a, b| {
                                            a.as_number().partial_cmp(&b.as_number()).unwrap_or(std::cmp::Ordering::Equal)
                                        });
                                        reg_set!(a, NanVal::heap_list(sorted));
                                    } else if all_strings {
                                        let mut sorted: Vec<NanVal> = items.iter().map(|v| { v.clone_rc(); *v }).collect();
                                        sorted.sort_by(|a, b| unsafe { nanval_str_cmp(*a, *b) });
                                        reg_set!(a, NanVal::heap_list(sorted));
                                    } else {
                                        return Err(VmError::Type("srt: list must contain all numbers or all text"));
                                    }
                                }
                            }
                            _ => return Err(VmError::Type("srt requires a list or text")),
                        }
                    } else {
                        return Err(VmError::Type("srt requires a list or text"));
                    }
                }
                OP_SLC => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let d = c + 1;
                    let vb = reg!(b);
                    let vc = reg!(c);
                    let vd = reg!(d);
                    if !vc.is_number() || !vd.is_number() {
                        return Err(VmError::Type("slc: indices must be numbers"));
                    }
                    let start = vc.as_number() as usize;
                    let end = vd.as_number() as usize;
                    if vb.is_string() {
                        let s = unsafe { match vb.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() } };
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
                            _ => return Err(VmError::Type("slc requires a list or text")),
                        }
                    } else {
                        return Err(VmError::Type("slc requires a list or text"));
                    }
                }
                OP_LISTAPPEND => {
                    let a = ((inst >> 16) & 0xFF) as usize + base;
                    let b = ((inst >> 8) & 0xFF) as usize + base;
                    let c = (inst & 0xFF) as usize + base;
                    let list_val = reg!(b);
                    let item_val = reg!(c);
                    if !list_val.is_heap() {
                        return Err(VmError::Type("+= requires a list"));
                    }
                    // SAFETY: is_heap() confirmed heap-tagged with live RC.
                    match unsafe { list_val.as_heap_ref() } {
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
                        _ => return Err(VmError::Type("+= requires a list")),
                    }
                }
                _ => return Err(VmError::UnknownOpcode { op }),
            }
        }
    }
}

/// Lexicographic comparison of two NanVal strings.
/// # Safety
/// Caller must ensure both `a` and `b` satisfy `is_string()`.
unsafe fn nanval_str_cmp(a: NanVal, b: NanVal) -> std::cmp::Ordering {
    // SAFETY: caller guarantees is_string() for both values.
    unsafe {
        let sa = match a.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() };
        let sb = match b.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() };
        sa.cmp(sb)
    }
}

fn nanval_equal(a: NanVal, b: NanVal) -> bool {
    if a.is_number() && b.is_number() {
        (a.as_number() - b.as_number()).abs() < f64::EPSILON
    } else if a.0 == b.0 {
        true
    } else if a.is_string() && b.is_string() {
        unsafe {
            // SAFETY: is_string() confirmed both are live heap-allocated string Rc pointers.
            let sa = match a.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() };
            let sb = match b.as_heap_ref() { HeapObj::Str(s) => s, _ => unreachable!() };
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
                debug_assert!(v.is_heap(), "nanval_truthy: unexpected non-heap NanVal tag {:#018x}", v.0);
                match v.as_heap_ref() {
                    HeapObj::Str(s) => !s.is_empty(),
                    HeapObj::List(l) => !l.is_empty(),
                    _ => true,
                }
            }
        }
    }
}

fn unpack_record_desc(desc: Value) -> VmResult<(String, Vec<String>)> {
    match desc {
        Value::List(items) if items.len() == 2 => {
            let tn = match &items[0] {
                Value::Text(s) => s.clone(),
                _ => return Err(VmError::Type("invalid record descriptor")),
            };
            let fns = unpack_string_list(&items[1])?;
            Ok((tn, fns))
        }
        _ => Err(VmError::Type("invalid record descriptor")),
    }
}

fn unpack_string_list(val: &Value) -> VmResult<Vec<String>> {
    match val {
        Value::List(items) => {
            items.iter().map(|v| match v {
                Value::Text(s) => Ok(s.clone()),
                _ => Err(VmError::Type("expected string in list")),
            }).collect()
        }
        _ => Err(VmError::Type("expected list")),
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    fn parse_program(source: &str) -> Program {
        let tokens = lexer::lex(source).unwrap();
        let token_spans: Vec<(crate::lexer::Token, crate::ast::Span)> = tokens
            .into_iter()
            .map(|(t, r)| (t, crate::ast::Span { start: r.start, end: r.end }))
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
        let source = std::fs::read_to_string("research/explorations/idea9-ultra-dense-short/01-simple-function.ilo").unwrap();
        let result = vm_run(
            &source,
            Some("tot"),
            vec![Value::Number(10.0), Value::Number(20.0), Value::Number(30.0)],
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
        let source = r#"cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze""#;
        let result = vm_run(source, Some("cls"), vec![Value::Number(1000.0)]);
        assert_eq!(result, Value::Text("gold".to_string()));
    }

    #[test]
    fn vm_cls_silver() {
        let source = r#"cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze""#;
        let result = vm_run(source, Some("cls"), vec![Value::Number(500.0)]);
        assert_eq!(result, Value::Text("silver".to_string()));
    }

    #[test]
    fn vm_cls_bronze() {
        let source = r#"cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze""#;
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
        let source = "tool fetch\"HTTP GET\" url:t>R _ t timeout:30\nf>R _ t;fetch \"http://example.com\"";
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
        let source = "tool fetch\"get\" url:t>R _ t\nf>t;r=fetch \"http://x\";?r{~v:\"ok\";^e:\"err\"}";
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
        let source = r#"f x:b>t;!x{"nope"};"yes""#;
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

    #[test]
    fn vm_string_concat() {
        let source = r#"f a:t b:t>t;+a b"#;
        let result = vm_run(
            source,
            Some("f"),
            vec![Value::Text("hello ".to_string()), Value::Text("world".to_string())],
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
        assert_eq!(result, Value::List(vec![
            Value::Number(1.0), Value::Number(2.0), Value::Number(3.0),
        ]));
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
            vm_run(source, Some("f"), vec![Value::Text("banana".into()), Value::Text("apple".into())]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("apple".into()), Value::Text("banana".into())]),
            Value::Bool(false)
        );

        // <
        let source = r#"f a:t b:t>b;<a b"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("apple".into()), Value::Text("banana".into())]),
            Value::Bool(true)
        );

        // >=
        let source = r#"f a:t b:t>b;>=a b"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("apple".into()), Value::Text("apple".into())]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("apple".into()), Value::Text("banana".into())]),
            Value::Bool(false)
        );

        // <=
        let source = r#"f a:t b:t>b;<=a b"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("banana".into()), Value::Text("banana".into())]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("zebra".into()), Value::Text("banana".into())]),
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
            vm_run(source, Some("f"), vec![Value::Bool(true), Value::Bool(true)]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Bool(true), Value::Bool(false)]),
            Value::Bool(false)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Bool(false), Value::Bool(true)]),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_logical_or() {
        let source = "f a:b b:b>b;|a b";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Bool(false), Value::Bool(false)]),
            Value::Bool(false)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Bool(true), Value::Bool(false)]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Bool(false), Value::Bool(true)]),
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
            Value::List(vec![Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)])
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
            Value::List(vec![Value::Number(1.0), Value::Number(2.0), Value::Number(3.0), Value::Number(4.0)])
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
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Text("3.14".into()));
    }

    #[test]
    fn vm_num_ok() {
        let source = "f>R n t;num \"42\"";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Ok(Box::new(Value::Number(42.0))));
    }

    #[test]
    fn vm_num_float() {
        let source = "f>R n t;num \"3.14\"";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Ok(Box::new(Value::Number(3.14))));
    }

    #[test]
    fn vm_num_err() {
        let source = "f>R n t;num \"abc\"";
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Err(Box::new(Value::Text("abc".into()))));
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
        let result = vm_run(source, Some("f"), vec![Value::Number(2.0), Value::Number(3.0), Value::Number(4.0)]);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn vm_nested_compare() {
        // >=+x y 100 → (x + y) >= 100
        let source = "f x:n y:n>b;>=+x y 100";
        let result = vm_run(source, Some("f"), vec![Value::Number(60.0), Value::Number(50.0)]);
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_not_as_and_operand() {
        // &!x y → (!x) & y
        let source = "f x:b y:b>b;&!x y";
        let result = vm_run(source, Some("f"), vec![Value::Bool(false), Value::Bool(true)]);
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_negate_product() {
        // -*a b → -(a * b)
        let source = "f a:n b:n>n;-*a b";
        let result = vm_run(source, Some("f"), vec![Value::Number(3.0), Value::Number(4.0)]);
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
        match rt { Value::Number(n) => assert!(n.to_bits() == (-0.0f64).to_bits()), _ => panic!() }
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
        compile_and_run(&prog, None, vec![]).unwrap_err().to_string()
    }

    // 1. VmState API — reusable state
    #[test]
    fn vm_state_reusable() {
        let prog = parse_program("f x:n>n;*x 2");
        let compiled = compile(&prog).unwrap();
        let mut state = VmState::new(&compiled);
        assert_eq!(state.call("f", vec![Value::Number(5.0)]).unwrap(), Value::Number(10.0));
        assert_eq!(state.call("f", vec![Value::Number(3.0)]).unwrap(), Value::Number(6.0));
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
            vm_run(source, Some("f"), vec![Value::Number(10.0), Value::Number(3.0)]),
            Value::Number(7.0)
        );
    }

    // 3. BinOp::Divide on two vars → OP_DIV_NN
    #[test]
    fn vm_div_nn() {
        let source = "f a:n b:n>n;/a b";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(15.0), Value::Number(3.0)]),
            Value::Number(5.0)
        );
    }

    // 4. BinOp::Equals — prefix =a b
    #[test]
    fn vm_equals_prefix() {
        let source = "f a:n b:n>b;=a b";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0), Value::Number(5.0)]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0), Value::Number(3.0)]),
            Value::Bool(false)
        );
    }

    // 5. BinOp::NotEquals — prefix !=a b
    #[test]
    fn vm_not_equals_prefix() {
        let source = "f a:n b:n>b;!=a b";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0), Value::Number(3.0)]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0), Value::Number(5.0)]),
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
        let source = "f>b;x==3 3;x";
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
        assert_eq!(vm_run(source, Some("f"), vec![]), Value::Text("hello world".into()));
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
        let source = "f>b;x==true true;x";
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
            Value::List(vec![Value::Number(10.0), Value::Number(20.0), Value::Number(30.0)])
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
            vm_run(source, Some("f"), vec![Value::Number(3.0), Value::Number(3.0)]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(3.0), Value::Number(4.0)]),
            Value::Bool(false)
        );
    }

    // nanval_equal on strings
    #[test]
    fn vm_nanval_equal_strings() {
        let source = r#"f a:t b:t>b;=a b"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("hi".into()), Value::Text("hi".into())]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("hi".into()), Value::Text("bye".into())]),
            Value::Bool(false)
        );
    }

    // nanval_equal on different types (should be false)
    #[test]
    fn vm_nanval_equal_different_types() {
        // Compare bool with bool using equality — both are non-heap singletons
        let source = "f a:b b:b>b;=a b";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Bool(true), Value::Bool(true)]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Bool(true), Value::Bool(false)]),
            Value::Bool(false)
        );
    }

    // nanval not-equals
    #[test]
    fn vm_nanval_not_equal() {
        let source = "f a:n b:n>b;!=a b";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(3.0), Value::Number(4.0)]),
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
            vm_run(source, Some("f"), vec![Value::Number(5.0), Value::Number(3.0)]),
            Value::Number(3.0)
        );
        // 0 is falsy, so &0 3 should return 0 (short-circuit)
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(0.0), Value::Number(3.0)]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn vm_nanval_truthy_number_or() {
        let source = "f a:n b:n>n;|a b";
        // Non-zero is truthy, so |5 3 should return 5 (short-circuit)
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(5.0), Value::Number(3.0)]),
            Value::Number(5.0)
        );
        // 0 is falsy, so |0 3 should return 3
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Number(0.0), Value::Number(3.0)]),
            Value::Number(3.0)
        );
    }

    // nanval_truthy — string truthiness (non-empty = true, empty = false)
    #[test]
    fn vm_nanval_truthy_string() {
        let source = r#"f a:t b:t>t;&a b"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("hi".into()), Value::Text("there".into())]),
            Value::Text("there".into())
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("".into()), Value::Text("there".into())]),
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
        let result = vm_run(source, Some("f"), vec![Value::Ok(Box::new(Value::Number(1.0)))]);
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
        match result {
            Value::Record { type_name, fields } => {
                assert_eq!(type_name, "point");
                assert_eq!(fields.get("x"), Some(&Value::Number(1.0)));
                assert_eq!(fields.get("y"), Some(&Value::Number(2.0)));
            }
            _ => panic!("expected record, got {:?}", result),
        }
    }

    // ── Error tests ──────────────────────────────────────────────────

    // 15. Type error: negate non-number
    #[test]
    fn vm_err_negate_non_number() {
        // Pass a bool and try to negate it — the parser/typechecker may not catch this
        // We use a function that takes a generic-ish approach
        let source = "f x:b>n;y=0;-y x";
        let err = vm_run_err(source, Some("f"), vec![Value::Bool(true)]);
        assert!(err.contains("subtract") || err.contains("negate") || err.contains("number"),
            "unexpected error: {}", err);
    }

    // 16. OP_ADD type error — adding incompatible types (number + bool)
    #[test]
    fn vm_err_add_incompatible() {
        // sub of number and bool triggers the OP_SUB type error
        let source = "f x:n y:b>n;-x y";
        let err = vm_run_err(source, Some("f"), vec![Value::Number(5.0), Value::Bool(true)]);
        assert!(err.contains("subtract") || err.contains("number"),
            "unexpected error: {}", err);
    }

    // 17. OP_RECFLD field not found
    #[test]
    fn vm_err_field_not_found() {
        let source = "f>n;r=point x:1 y:2;r.z";
        let err = vm_run_err(source, Some("f"), vec![]);
        assert!(err.contains("field") || err.contains("z"),
            "unexpected error: {}", err);
    }

    // 21. Compile error: undefined variable reference
    #[test]
    fn vm_err_undefined_variable() {
        let err = compile_err("f>n;x");
        assert!(err.contains("undefined variable"),
            "unexpected error: {}", err);
    }

    // 22. Compile error: undefined function call
    #[test]
    fn vm_err_undefined_function() {
        let err = compile_err("f>n;nonexistent 5");
        assert!(err.contains("undefined function"),
            "unexpected error: {}", err);
    }

    // 24. Division by zero in OP_DIV_NN
    #[test]
    fn vm_err_division_by_zero() {
        let source = "f a:n b:n>n;/a b";
        let err = vm_run_err(source, Some("f"), vec![Value::Number(10.0), Value::Number(0.0)]);
        assert!(err.contains("division by zero"),
            "unexpected error: {}", err);
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
        assert!(err.contains("division by zero"), "unexpected error: {}", err);
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
            "unexpected error: {}", err
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
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(5.0)]), Value::Number(0.0));
        // With x < 0: guard skips, Nil fallback executes → returns Nil
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(-1.0)]), Value::Nil);
    }

    #[test]
    fn vm_nil_fallback_guard_body() {
        // Guard body only has a nested Guard stmt → compile_body returns None for guard body (L361-365)
        // Outer guard fires (x >= 0), inner guard body compiles to None → Nil fallback
        let source = "f x:n>n;>=x 0{>=x 5{10}}";
        // x=10: outer guard fires, inner guard fires → returns 10
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(10.0)]), Value::Number(10.0));
        // x=1: outer guard fires, inner guard doesn't fire → guard body returns Nil
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(1.0)]), Value::Nil);
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
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(3.0)]), Value::Number(5.0));
    }

    #[test]
    fn vm_mulk_n_const_left() {
        // MULK_N with literal constant on left side: *3 x (L753-763 for Multiply)
        let source = "f x:n>n;*3 x";
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(4.0)]), Value::Number(12.0));
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
            fields: std::collections::HashMap::from([
                ("x".to_string(), Value::Number(42.0)),
            ]),
        };
        let nv = NanVal::from_value(&rec);
        let roundtrip = nv.to_value();
        match roundtrip {
            Value::Record { type_name, fields } => {
                assert_eq!(type_name, "point");
                assert_eq!(fields.get("x"), Some(&Value::Number(42.0)));
            }
            other => panic!("expected Record, got {:?}", other),
        }
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
        assert!(err.contains("zero") || err.contains("Div"), "expected division-by-zero error, got: {err}");
    }

    #[test]
    fn vm_gt_non_numeric_registers() {
        // OP_GT numeric path (L1441): both operands are numbers but not tagged numeric
        // r and s are match results (reg_is_num=false), so OP_GT is emitted (not OP_GT_NN)
        let source = "f x:n>b;r=?x{1:5;_:2};s=?x{1:3;_:8};>r s";
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(1.0)]), Value::Bool(true)); // 5 > 3 = true
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(2.0)]), Value::Bool(false)); // 2 > 8 = false
    }

    #[test]
    fn vm_lt_non_numeric_registers() {
        // OP_LT numeric path (L1456): both operands are numbers but not tagged numeric
        let source = "f x:n>b;r=?x{1:5;_:2};s=?x{1:3;_:8};<r s";
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(1.0)]), Value::Bool(false)); // 5 < 3 = false
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(2.0)]), Value::Bool(true)); // 2 < 8 = true
    }

    #[test]
    fn vm_le_non_numeric_registers() {
        // OP_LE numeric path (L1486): both operands are numbers but not tagged numeric
        let source = "f x:n>b;r=?x{1:5;_:3};s=?x{1:5;_:8};<=r s";
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(1.0)]), Value::Bool(true)); // 5 <= 5 = true
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(2.0)]), Value::Bool(true)); // 3 <= 8 = true
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
        let err = vm_run_err(source, Some("f"), vec![Value::Number(1.0), Value::Text("hi".to_string())]);
        assert!(err.contains("cannot add"), "got: {err}");
    }

    #[test]
    fn vm_add_heap_non_list_type_error() {
        // OP_ADD: both heap but not both lists (list+ok) → L1374
        // Declare params as text to avoid NN/superinstruction, pass list+ok at runtime.
        let source = "f x:t y:t>t;+x y";
        let list = Value::List(vec![Value::Number(1.0)]);
        let ok_val = Value::Ok(Box::new(Value::Number(1.0)));
        let err = vm_run_err(source, Some("f"), vec![list, ok_val]);
        assert!(err.contains("cannot add"), "got: {err}");
    }

    #[test]
    fn vm_mul_type_error() {
        // OP_MUL: text × number → L1401 ("cannot multiply non-numbers")
        let source = "f x:t y:n>n;*x y";
        let err = vm_run_err(source, Some("f"), vec![Value::Text("hi".to_string()), Value::Number(2.0)]);
        assert!(err.contains("multiply"), "got: {err}");
    }

    #[test]
    fn vm_gt_type_error() {
        // OP_GT: number > text → neither both-num nor both-string → L1446
        let source = "f x:n y:t>b;>x y";
        let err = vm_run_err(source, Some("f"), vec![Value::Number(1.0), Value::Text("hi".to_string())]);
        assert!(err.contains("compare"), "got: {err}");
    }

    #[test]
    fn vm_lt_type_error() {
        // OP_LT: number < text → L1461
        let source = "f x:n y:t>b;<x y";
        let err = vm_run_err(source, Some("f"), vec![Value::Number(1.0), Value::Text("hi".to_string())]);
        assert!(err.contains("compare"), "got: {err}");
    }

    #[test]
    fn vm_ge_type_error() {
        // OP_GE: number >= text → L1476
        let source = "f x:n y:t>b;>=x y";
        let err = vm_run_err(source, Some("f"), vec![Value::Number(1.0), Value::Text("hi".to_string())]);
        assert!(err.contains("compare"), "got: {err}");
    }

    #[test]
    fn vm_le_type_error() {
        // OP_LE: number <= text → L1491
        let source = "f x:n y:t>b;<=x y";
        let err = vm_run_err(source, Some("f"), vec![Value::Number(1.0), Value::Text("hi".to_string())]);
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
        let err = vm_run_err(source, Some("f"), vec![Value::Text("hi".to_string()), Value::Number(2.0)]);
        assert!(err.contains("min") || err.contains("max") || err.contains("number"), "got: {err}");
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
        assert!(matches!(result, Value::Number(n) if n.is_nan()), "expected NaN, got: {:?}", result);
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
        let err = vm_run_err(source, Some("f"), vec![Value::Text("hi".into()), Value::Text("lo".into())]);
        assert!(err.contains("divide"), "got: {err}");
    }

    #[test]
    fn vm_recfld_on_non_record() {
        // OP_RECFLD: field access on a list (heap but not record) → L1590
        let source = "f x:t>t;x.name";
        let err = vm_run_err(source, Some("f"), vec![Value::List(vec![])]);
        assert!(err.contains("field access") || err.contains("record"), "got: {err}");
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
        assert!(err.contains("list") || err.contains("foreach"), "got: {err}");
    }

    #[test]
    fn vm_foreach_on_heap_non_list() {
        // OP_LISTGET: foreach over a string (heap but not list) → L1643
        let source = "f x:t>t;@elem x{elem}";
        let err = vm_run_err(source, Some("f"), vec![Value::Text("hi".into())]);
        assert!(err.contains("list") || err.contains("foreach"), "got: {err}");
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
        let err = vm_run_err(source, Some("f"), vec![Value::Ok(Box::new(Value::Number(1.0)))]);
        assert!(err.contains("len") || err.contains("string") || err.contains("list"), "got: {err}");
    }

    #[test]
    fn vm_len_on_non_heap() {
        // OP_LEN: len of number (non-heap, non-string) → L1894
        let source = "f x:t>n;len x";
        let err = vm_run_err(source, Some("f"), vec![Value::Number(5.0)]);
        assert!(err.contains("len") || err.contains("string") || err.contains("list"), "got: {err}");
    }

    #[test]
    fn vm_listappend_on_non_heap() {
        // OP_LISTAPPEND: += where first arg is a number (non-heap) → L1972
        let source = "f x:t y:t>t;+=x y";
        let err = vm_run_err(source, Some("f"), vec![Value::Number(1.0), Value::Number(2.0)]);
        assert!(err.contains("list") || err.contains("+="), "got: {err}");
    }

    #[test]
    fn vm_listappend_on_heap_non_list() {
        // OP_LISTAPPEND: += where first arg is a string (heap but not list) → L1986
        let source = "f x:t y:t>t;+=x y";
        let err = vm_run_err(source, Some("f"), vec![Value::Text("hi".into()), Value::Number(1.0)]);
        assert!(err.contains("list") || err.contains("+="), "got: {err}");
    }

    // vm compile: function with 256 parameters → L285 assert panics
    #[test]
    #[should_panic(expected = "function has 256 parameters")]
    fn vm_too_many_params_panics() {
        use crate::ast::{Decl, Param, Program, Span, Type};
        let params: Vec<Param> = (0..256)
            .map(|i| Param { name: format!("p{i}"), ty: Type::Number })
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
        assert!(err.contains("compare") || err.contains("type"), "got: {err}");
    }

    // try_const_fold: mixed types (Bool + Number) → L582 `_ => None`
    // +true 3 → lv=Bool(true), rv=Number(3) → _ branch at L582
    #[test]
    fn vm_const_fold_mixed_types_no_fold() {
        let err = vm_run_err("f>n;+true 3", Some("f"), vec![]);
        assert!(err.contains("add") || err.contains("type") || err.contains("number"), "got: {err}");
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
            vm_run(source, Some("f"), vec![Value::List(vec![
                Value::Text("a".into()), Value::Text("b".into()), Value::Text("c".into()),
            ])]),
            Value::Text("a,b,c".into())
        );
    }

    #[test]
    fn vm_cat_empty_list() {
        let source = "f items:L t>t;cat items \"-\"";
        assert_eq!(vm_run(source, Some("f"), vec![Value::List(vec![])]), Value::Text("".into()));
    }

    #[test]
    fn vm_has_list() {
        let source = "f xs:L n x:n>b;has xs x";
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::List(vec![Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)]), Value::Number(2.0)]),
            Value::Bool(true)
        );
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::List(vec![Value::Number(1.0)]), Value::Number(5.0)]),
            Value::Bool(false)
        );
    }

    #[test]
    fn vm_has_text() {
        let source = r#"f s:t needle:t>b;has s needle"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![Value::Text("hello world".into()), Value::Text("world".into())]),
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
            Value::List(vec![Value::Number(3.0), Value::Number(2.0), Value::Number(1.0)])
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
            Value::List(vec![Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)])
        );
    }

    #[test]
    fn vm_srt_text_list() {
        let source = r#"f>L t;srt ["c", "a", "b"]"#;
        assert_eq!(
            vm_run(source, Some("f"), vec![]),
            Value::List(vec![Value::Text("a".into()), Value::Text("b".into()), Value::Text("c".into())])
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
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(1.0)]), Value::Text("yes".into()));
    }

    #[test]
    fn vm_ternary_false() {
        let source = r#"f x:n>t;=x 1{"yes"}{"no"}"#;
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(2.0)]), Value::Text("no".into()));
    }

    #[test]
    fn vm_ternary_no_early_return() {
        let source = r#"f x:n>n;=x 0{10}{20};+x 1"#;
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(0.0)]), Value::Number(1.0));
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(5.0)]), Value::Number(6.0));
    }

    #[test]
    fn vm_ret_early_return() {
        let source = r#"f x:n>n;>x 0{ret x};0"#;
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(5.0)]), Value::Number(5.0));
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(-1.0)]), Value::Number(0.0));
    }

    #[test]
    fn vm_ret_in_foreach() {
        let source = "f xs:L n>n;@x xs{>=x 10{ret x}};0";
        let list = Value::List(vec![Value::Number(1.0), Value::Number(15.0), Value::Number(3.0)]);
        assert_eq!(vm_run(source, Some("f"), vec![list]), Value::Number(15.0));
    }

    #[test]
    fn vm_pipe_simple() {
        let source = "f x:n>n;str x>>len";
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(42.0)]), Value::Number(2.0));
    }

    #[test]
    fn vm_pipe_chain() {
        let source = "dbl x:n>n;*x 2\nadd1 x:n>n;+x 1\nf x:n>n;dbl x>>add1";
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(5.0)]), Value::Number(11.0));
    }

    #[test]
    fn vm_pipe_with_extra_args() {
        let source = "add a:n b:n>n;+a b\nf x:n>n;add x 1>>add 2";
        assert_eq!(vm_run(source, Some("f"), vec![Value::Number(5.0)]), Value::Number(8.0));
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
}
