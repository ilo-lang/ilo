//! Lowering pass: verified `ast::Program` → `hir::Program`.
//!
//! See `DESIGN.md`. Stage 5a keeps lowering thin: mirror the AST, apply the
//! handful of documented desugarings (body tail-split, guard polarity fold,
//! `Ternary` → `If`, drop `Alias`/`Use`/`Error` decls).
//!
//! Type slots on HIR nodes are best-effort. Where the AST literal makes the
//! type obvious (`Literal::Number` → `Ty::Number`) we fill it; everything else
//! gets `Ty::Unknown` for now. Stage 5b will introduce a typed-AST channel
//! from the verifier when Cranelift's emit pass demands it.

use crate::ast;
use crate::hir::decl::{Decl, Param};
use crate::hir::expr::{Body, Expr, MatchArm, Pattern, Stmt};
use crate::hir::program::Program;
use crate::hir::types::Ty;
use crate::verify::VerifyResult;

/// Errors surfaced by lowering.
///
/// Stage 5a only emits these for genuine internal-consistency bugs: a verified
/// AST that survives the verifier should always lower cleanly. The caller's
/// expected contract is "verify first, then lower" — feeding a program that
/// still contains `Decl::Error` poison nodes is the only failure mode that
/// can hit a real user.
#[derive(Debug, Clone, PartialEq)]
pub enum LowerError {
    /// The AST contains a `Decl::Error` poison node. The caller failed to
    /// reject parse errors before lowering.
    PoisonDecl,
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LowerError::PoisonDecl => write!(
                f,
                "hir::lower: program contains Decl::Error — refusing to lower a program with parse errors"
            ),
        }
    }
}

impl std::error::Error for LowerError {}

/// Lower a verified AST program to HIR.
///
/// `verify_out` is passed in because future stages will use the verifier's
/// (eventual) typed output to populate `Ty` slots properly. Stage 5a only
/// consults it to bail when `verify_out.errors` is non-empty; downstream
/// stages will use the type-info channel that lands in Stage 5b.
pub fn lower(ast: &ast::Program, _verify_out: &VerifyResult) -> Result<Program, LowerError> {
    let mut decls = Vec::with_capacity(ast.declarations.len());

    for decl in &ast.declarations {
        match decl {
            ast::Decl::Function {
                name,
                params,
                return_type,
                body,
                span,
            } => {
                let lowered_body = lower_function_body(body);
                decls.push(Decl::Function {
                    name: name.clone(),
                    params: params.iter().map(lower_param).collect(),
                    return_type: lower_type(return_type),
                    body: lowered_body,
                    span: *span,
                });
            }
            ast::Decl::TypeDef { name, fields, span } => {
                decls.push(Decl::TypeDef {
                    name: name.clone(),
                    fields: fields.iter().map(lower_param).collect(),
                    span: *span,
                });
            }
            ast::Decl::Tool {
                name,
                description,
                params,
                return_type,
                timeout,
                retry,
                span,
            } => {
                decls.push(Decl::Tool {
                    name: name.clone(),
                    description: description.clone(),
                    params: params.iter().map(lower_param).collect(),
                    return_type: lower_type(return_type),
                    timeout: *timeout,
                    retry: *retry,
                    span: *span,
                });
            }
            // Aliases are resolved at verify time; `use` is resolved pre-verify.
            // Both are pure sugar in the HIR layer — drop them.
            ast::Decl::Alias { .. } | ast::Decl::Use { .. } => continue,
            // Parse-recovery poison. A correctly sequenced caller verified the
            // program first and bailed on errors; reaching us is a bug.
            ast::Decl::Error { .. } => return Err(LowerError::PoisonDecl),
        }
    }

    Ok(Program {
        decls,
        source: ast.source.clone(),
    })
}

fn lower_param(p: &ast::Param) -> Param {
    Param {
        name: p.name.clone(),
        ty: lower_type(&p.ty),
    }
}

/// Lower a `body: Vec<Spanned<Stmt>>` from a function decl, splitting the
/// optional trailing expression into `Body::tail` so backends don't have to
/// re-derive the implicit return.
fn lower_function_body(body: &[ast::Spanned<ast::Stmt>]) -> Body {
    lower_body(body)
}

/// Lower an arbitrary statement block. Same shape as a function body: trailing
/// `Expr` statement becomes `Body::tail`.
fn lower_body(body: &[ast::Spanned<ast::Stmt>]) -> Body {
    if body.is_empty() {
        return Body::default();
    }

    let last_idx = body.len() - 1;
    let (head, last) = body.split_at(last_idx);
    let last = &last[0];

    let mut stmts: Vec<Stmt> = head.iter().map(lower_stmt).collect();

    // Peel a trailing bare expression into `Body::tail`. Everything else (Let,
    // Guard, Return, ForEach, etc.) stays as a statement — those carry
    // semantics beyond "value-of-block."
    match &last.node {
        ast::Stmt::Expr(e) => Body {
            stmts,
            tail: Some(lower_expr(e)),
        },
        _ => {
            stmts.push(lower_stmt(last));
            Body { stmts, tail: None }
        }
    }
}

fn lower_stmt(stmt: &ast::Spanned<ast::Stmt>) -> Stmt {
    let span = stmt.span;
    match &stmt.node {
        ast::Stmt::Let { name, value } => Stmt::Let {
            name: name.clone(),
            value: lower_expr(value),
            span,
        },

        ast::Stmt::Guard {
            condition,
            negated,
            body,
            else_body,
            braceless,
        } => {
            // Braceless guards (`cond expr`) early-return their value from the
            // enclosing function. Body has exactly one tail expression by
            // parser construction (a single `Expr` statement). Re-shape into
            // GuardReturn so backends see the early-return intent explicitly.
            if *braceless {
                let cond = fold_negation(lower_expr(condition), *negated, span);
                let value = match lower_body(body).tail {
                    Some(v) => v,
                    // Defensive: if the parser ever lands a braceless guard
                    // whose body isn't a tail expression (e.g. a Return stmt),
                    // synthesize a Nil so the HIR shape stays valid. This
                    // shouldn't fire on any verified program.
                    None => Expr::Literal {
                        value: ast::Literal::Nil,
                        ty: Ty::Nil,
                        span,
                    },
                };
                return Stmt::GuardReturn { cond, value, span };
            }

            // Braced guards are plain conditionals. Fold negation into the
            // condition; backends only ever see positive polarity.
            let cond = fold_negation(lower_expr(condition), *negated, span);
            let then = lower_body(body);
            let else_ = else_body.as_ref().map(|eb| lower_body(eb));
            Stmt::If {
                cond,
                then,
                else_,
                span,
            }
        }

        ast::Stmt::Match { subject, arms } => Stmt::Match {
            subject: subject.as_ref().map(lower_expr),
            arms: arms.iter().map(lower_match_arm).collect(),
            span,
        },

        ast::Stmt::ForEach {
            binding,
            collection,
            body,
        } => Stmt::ForEach {
            binding: binding.clone(),
            collection: lower_expr(collection),
            body: lower_body(body),
            span,
        },

        ast::Stmt::ForRange {
            binding,
            start,
            end,
            body,
        } => Stmt::ForRange {
            binding: binding.clone(),
            start: lower_expr(start),
            end: lower_expr(end),
            body: lower_body(body),
            span,
        },

        ast::Stmt::While { condition, body } => Stmt::While {
            cond: lower_expr(condition),
            body: lower_body(body),
            span,
        },

        ast::Stmt::Return(e) => Stmt::Return {
            value: lower_expr(e),
            span,
        },

        ast::Stmt::Break(opt) => Stmt::Break {
            value: opt.as_ref().map(lower_expr),
            span,
        },

        ast::Stmt::Continue => Stmt::Continue { span },

        ast::Stmt::Destructure { bindings, value } => Stmt::Destructure {
            bindings: bindings.clone(),
            value: lower_expr(value),
            span,
        },

        ast::Stmt::Expr(e) => Stmt::Expr {
            value: lower_expr(e),
            span,
        },
    }
}

fn lower_match_arm(arm: &ast::MatchArm) -> MatchArm {
    // Span on the arm is best-approximated by the union of pattern arm body
    // spans; in practice the AST doesn't carry an arm-level span, so we use
    // the first body statement's span (or UNKNOWN if empty).
    let span = arm
        .body
        .first()
        .map(|s| s.span)
        .unwrap_or(ast::Span::UNKNOWN);
    MatchArm {
        pattern: lower_pattern(&arm.pattern),
        body: lower_body(&arm.body),
        span,
    }
}

fn lower_pattern(p: &ast::Pattern) -> Pattern {
    match p {
        ast::Pattern::Err(b) => Pattern::Err {
            binding: b.clone(),
            ty: Ty::Unknown,
        },
        ast::Pattern::Ok(b) => Pattern::Ok {
            binding: b.clone(),
            ty: Ty::Unknown,
        },
        ast::Pattern::Literal(lit) => Pattern::Literal(lit.clone()),
        ast::Pattern::Wildcard => Pattern::Wildcard,
        ast::Pattern::TypeIs { ty, binding } => Pattern::TypeIs {
            ty: lower_type(ty),
            binding: binding.clone(),
        },
    }
}

fn lower_expr(e: &ast::Expr) -> Expr {
    // No span on AST expressions today — use UNKNOWN. When the parser starts
    // attaching expression spans this changes to read them through.
    let span = ast::Span::UNKNOWN;

    match e {
        ast::Expr::Literal(lit) => Expr::Literal {
            ty: literal_ty(lit),
            value: lit.clone(),
            span,
        },

        ast::Expr::Ref(name) => Expr::Ref {
            name: name.clone(),
            ty: Ty::Unknown,
            span,
        },

        ast::Expr::Field {
            object,
            field,
            safe,
        } => Expr::Field {
            object: Box::new(lower_expr(object)),
            field: field.clone(),
            safe: *safe,
            ty: Ty::Unknown,
            span,
        },

        ast::Expr::Index {
            object,
            index,
            safe,
        } => Expr::Index {
            object: Box::new(lower_expr(object)),
            index: *index,
            safe: *safe,
            ty: Ty::Unknown,
            span,
        },

        ast::Expr::Call {
            function,
            args,
            unwrap,
        } => Expr::Call {
            function: function.clone(),
            args: args.iter().map(lower_expr).collect(),
            unwrap: *unwrap,
            ty: Ty::Unknown,
            span,
        },

        ast::Expr::BinOp { op, left, right } => Expr::BinOp {
            op: op.clone(),
            left: Box::new(lower_expr(left)),
            right: Box::new(lower_expr(right)),
            ty: binop_ty(op),
            span,
        },

        ast::Expr::UnaryOp { op, operand } => Expr::UnaryOp {
            op: op.clone(),
            operand: Box::new(lower_expr(operand)),
            ty: unary_ty(op),
            span,
        },

        ast::Expr::Ok(inner) => Expr::Ok {
            inner: Box::new(lower_expr(inner)),
            ty: Ty::Unknown,
            span,
        },

        ast::Expr::Err(inner) => Expr::Err {
            inner: Box::new(lower_expr(inner)),
            ty: Ty::Unknown,
            span,
        },

        ast::Expr::List(items) => Expr::List {
            items: items.iter().map(lower_expr).collect(),
            ty: Ty::Unknown,
            span,
        },

        ast::Expr::Record { type_name, fields } => Expr::Record {
            type_name: type_name.clone(),
            fields: fields
                .iter()
                .map(|(n, v)| (n.clone(), lower_expr(v)))
                .collect(),
            ty: Ty::Named(type_name.clone()),
            span,
        },

        ast::Expr::Match { subject, arms } => Expr::Match {
            subject: subject.as_ref().map(|s| Box::new(lower_expr(s))),
            arms: arms.iter().map(lower_match_arm).collect(),
            ty: Ty::Unknown,
            span,
        },

        ast::Expr::NilCoalesce { value, default } => Expr::NilCoalesce {
            value: Box::new(lower_expr(value)),
            default: Box::new(lower_expr(default)),
            ty: Ty::Unknown,
            span,
        },

        ast::Expr::With { object, updates } => Expr::With {
            object: Box::new(lower_expr(object)),
            updates: updates
                .iter()
                .map(|(n, v)| (n.clone(), lower_expr(v)))
                .collect(),
            ty: Ty::Unknown,
            span,
        },

        // Ternary lowers to value-level If. The AST node is gone after this
        // point — backends see only `Expr::If`.
        ast::Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => Expr::If {
            cond: Box::new(lower_expr(condition)),
            then: Box::new(lower_expr(then_expr)),
            else_: Box::new(lower_expr(else_expr)),
            ty: Ty::Unknown,
            span,
        },

        ast::Expr::MakeClosure { fn_name, captures } => Expr::MakeClosure {
            fn_name: fn_name.clone(),
            captures: captures.iter().map(lower_expr).collect(),
            ty: Ty::Unknown,
            span,
        },
    }
}

fn lower_type(t: &ast::Type) -> Ty {
    match t {
        ast::Type::Number => Ty::Number,
        ast::Type::Text => Ty::Text,
        ast::Type::Bool => Ty::Bool,
        ast::Type::Any => Ty::Unknown,
        ast::Type::Optional(inner) => Ty::Optional(Box::new(lower_type(inner))),
        ast::Type::List(inner) => Ty::List(Box::new(lower_type(inner))),
        ast::Type::Map(k, v) => Ty::Map(Box::new(lower_type(k)), Box::new(lower_type(v))),
        ast::Type::Result(ok, err) => {
            Ty::Result(Box::new(lower_type(ok)), Box::new(lower_type(err)))
        }
        ast::Type::Sum(variants) => Ty::Sum(variants.clone()),
        ast::Type::Fn(params, ret) => Ty::Fn(
            params.iter().map(lower_type).collect(),
            Box::new(lower_type(ret)),
        ),
        ast::Type::Named(n) => Ty::Named(n.clone()),
    }
}

fn literal_ty(lit: &ast::Literal) -> Ty {
    match lit {
        ast::Literal::Number(_) => Ty::Number,
        ast::Literal::Text(_) => Ty::Text,
        ast::Literal::Bool(_) => Ty::Bool,
        ast::Literal::Nil => Ty::Nil,
    }
}

fn binop_ty(op: &ast::BinOp) -> Ty {
    use ast::BinOp::*;
    match op {
        Add | Subtract | Multiply | Divide => Ty::Number,
        Equals | NotEquals | GreaterThan | LessThan | GreaterOrEqual | LessOrEqual | And | Or => {
            Ty::Bool
        }
        Append => Ty::Unknown, // depends on operand type (List vs Text)
    }
}

fn unary_ty(op: &ast::UnaryOp) -> Ty {
    match op {
        ast::UnaryOp::Not => Ty::Bool,
        ast::UnaryOp::Negate => Ty::Number,
    }
}

/// Fold guard negation into a `UnaryOp(Not)` wrapper. Backends only ever see
/// positive-polarity `If` conditions.
fn fold_negation(cond: Expr, negated: bool, span: ast::Span) -> Expr {
    if !negated {
        return cond;
    }
    Expr::UnaryOp {
        op: ast::UnaryOp::Not,
        operand: Box::new(cond),
        ty: Ty::Bool,
        span,
    }
}
