//! Raise HIR back to AST.
//!
//! Stage 5a uses this exclusively for the throwaway HIR walker
//! (`hir::walker::walk`). It proves the AST → HIR lowering preserves enough
//! information to reconstruct an equivalent AST and exercise the existing
//! tree interpreter. Stage 5f deletes this module once real backends drive
//! the HIR directly.
//!
//! The raise is _not_ a perfect inverse of `lower`. We don't try to recover
//! the original guard polarity (we folded it into `UnaryOp(Not)` deliberately)
//! or the source-level `Decl::Alias` / `Decl::Use` (they're gone). We only
//! guarantee that the raised AST has the same observable runtime behaviour.

use crate::ast;
use crate::hir;

/// Raise an HIR program back into AST form.
pub fn raise(hir: &hir::Program) -> ast::Program {
    let declarations = hir.decls.iter().map(raise_decl).collect();
    ast::Program {
        declarations,
        source: hir.source.clone(),
    }
}

fn raise_decl(d: &hir::Decl) -> ast::Decl {
    match d {
        hir::Decl::Function {
            name,
            params,
            return_type,
            body,
            span,
        } => ast::Decl::Function {
            name: name.clone(),
            params: params.iter().map(raise_param).collect(),
            return_type: raise_type(return_type),
            body: raise_body(body),
            span: *span,
        },
        hir::Decl::TypeDef { name, fields, span } => ast::Decl::TypeDef {
            name: name.clone(),
            fields: fields.iter().map(raise_param).collect(),
            span: *span,
        },
        hir::Decl::Tool {
            name,
            description,
            params,
            return_type,
            timeout,
            retry,
            span,
        } => ast::Decl::Tool {
            name: name.clone(),
            description: description.clone(),
            params: params.iter().map(raise_param).collect(),
            return_type: raise_type(return_type),
            timeout: *timeout,
            retry: *retry,
            span: *span,
        },
    }
}

fn raise_param(p: &hir::Param) -> ast::Param {
    ast::Param {
        name: p.name.clone(),
        ty: raise_type(&p.ty),
    }
}

/// Reverse of `lower_body`: re-attach the tail expression as a trailing
/// `Stmt::Expr` so the tree interpreter's "last expression is the return
/// value" rule fires correctly.
fn raise_body(body: &hir::Body) -> Vec<ast::Spanned<ast::Stmt>> {
    let mut out: Vec<ast::Spanned<ast::Stmt>> = body.stmts.iter().map(raise_stmt).collect();
    if let Some(tail) = &body.tail {
        let span = expr_span(tail);
        out.push(ast::Spanned::new(ast::Stmt::Expr(raise_expr(tail)), span));
    }
    out
}

fn raise_stmt(s: &hir::Stmt) -> ast::Spanned<ast::Stmt> {
    let (node, span) = match s {
        hir::Stmt::Let { name, value, span } => (
            ast::Stmt::Let {
                name: name.clone(),
                value: raise_expr(value),
            },
            *span,
        ),

        hir::Stmt::If {
            cond,
            then,
            else_,
            span,
        } => (
            ast::Stmt::Guard {
                condition: raise_expr(cond),
                negated: false,
                body: raise_body(then),
                else_body: else_.as_ref().map(raise_body),
                braceless: false,
            },
            *span,
        ),

        hir::Stmt::GuardReturn { cond, value, span } => (
            ast::Stmt::Guard {
                condition: raise_expr(cond),
                negated: false,
                body: vec![ast::Spanned::new(
                    ast::Stmt::Expr(raise_expr(value)),
                    expr_span(value),
                )],
                else_body: None,
                braceless: true,
            },
            *span,
        ),

        hir::Stmt::Match {
            subject,
            arms,
            span,
        } => (
            ast::Stmt::Match {
                subject: subject.as_ref().map(raise_expr),
                arms: arms.iter().map(raise_match_arm).collect(),
            },
            *span,
        ),

        hir::Stmt::ForEach {
            binding,
            collection,
            body,
            span,
        } => (
            ast::Stmt::ForEach {
                binding: binding.clone(),
                collection: raise_expr(collection),
                body: raise_body(body),
            },
            *span,
        ),

        hir::Stmt::ForRange {
            binding,
            start,
            end,
            body,
            span,
        } => (
            ast::Stmt::ForRange {
                binding: binding.clone(),
                start: raise_expr(start),
                end: raise_expr(end),
                body: raise_body(body),
            },
            *span,
        ),

        hir::Stmt::While { cond, body, span } => (
            ast::Stmt::While {
                condition: raise_expr(cond),
                body: raise_body(body),
            },
            *span,
        ),

        hir::Stmt::Return { value, span } => (ast::Stmt::Return(raise_expr(value)), *span),

        hir::Stmt::Break { value, span } => {
            (ast::Stmt::Break(value.as_ref().map(raise_expr)), *span)
        }

        hir::Stmt::Continue { span } => (ast::Stmt::Continue, *span),

        hir::Stmt::Destructure {
            bindings,
            value,
            span,
        } => (
            ast::Stmt::Destructure {
                bindings: bindings.clone(),
                value: raise_expr(value),
            },
            *span,
        ),

        hir::Stmt::Expr { value, span } => (ast::Stmt::Expr(raise_expr(value)), *span),
    };

    ast::Spanned::new(node, span)
}

fn raise_match_arm(arm: &hir::MatchArm) -> ast::MatchArm {
    ast::MatchArm {
        pattern: raise_pattern(&arm.pattern),
        body: raise_body(&arm.body),
    }
}

fn raise_pattern(p: &hir::Pattern) -> ast::Pattern {
    match p {
        hir::Pattern::Err { binding, .. } => ast::Pattern::Err(binding.clone()),
        hir::Pattern::Ok { binding, .. } => ast::Pattern::Ok(binding.clone()),
        hir::Pattern::Literal(lit) => ast::Pattern::Literal(lit.clone()),
        hir::Pattern::Wildcard => ast::Pattern::Wildcard,
        hir::Pattern::TypeIs { ty, binding } => ast::Pattern::TypeIs {
            ty: raise_type(ty),
            binding: binding.clone(),
        },
    }
}

fn raise_expr(e: &hir::Expr) -> ast::Expr {
    match e {
        hir::Expr::Literal { value, .. } => ast::Expr::Literal(value.clone()),

        hir::Expr::Ref { name, .. } => ast::Expr::Ref(name.clone()),

        hir::Expr::Field {
            object,
            field,
            safe,
            ..
        } => ast::Expr::Field {
            object: Box::new(raise_expr(object)),
            field: field.clone(),
            safe: *safe,
        },

        hir::Expr::Index {
            object,
            index,
            safe,
            ..
        } => ast::Expr::Index {
            object: Box::new(raise_expr(object)),
            index: *index,
            safe: *safe,
        },

        hir::Expr::Call {
            function,
            args,
            unwrap,
            ..
        } => ast::Expr::Call {
            function: function.clone(),
            args: args.iter().map(raise_expr).collect(),
            unwrap: *unwrap,
        },

        hir::Expr::BinOp {
            op, left, right, ..
        } => ast::Expr::BinOp {
            op: op.clone(),
            left: Box::new(raise_expr(left)),
            right: Box::new(raise_expr(right)),
        },

        hir::Expr::UnaryOp { op, operand, .. } => ast::Expr::UnaryOp {
            op: op.clone(),
            operand: Box::new(raise_expr(operand)),
        },

        hir::Expr::Ok { inner, .. } => ast::Expr::Ok(Box::new(raise_expr(inner))),
        hir::Expr::Err { inner, .. } => ast::Expr::Err(Box::new(raise_expr(inner))),

        hir::Expr::List { items, .. } => ast::Expr::List(items.iter().map(raise_expr).collect()),

        hir::Expr::Record {
            type_name, fields, ..
        } => ast::Expr::Record {
            type_name: type_name.clone(),
            fields: fields
                .iter()
                .map(|(n, v)| (n.clone(), raise_expr(v)))
                .collect(),
        },

        hir::Expr::Match { subject, arms, .. } => ast::Expr::Match {
            subject: subject.as_ref().map(|s| Box::new(raise_expr(s))),
            arms: arms.iter().map(raise_match_arm).collect(),
        },

        hir::Expr::NilCoalesce { value, default, .. } => ast::Expr::NilCoalesce {
            value: Box::new(raise_expr(value)),
            default: Box::new(raise_expr(default)),
        },

        hir::Expr::With {
            object, updates, ..
        } => ast::Expr::With {
            object: Box::new(raise_expr(object)),
            updates: updates
                .iter()
                .map(|(n, v)| (n.clone(), raise_expr(v)))
                .collect(),
        },

        hir::Expr::If {
            cond, then, else_, ..
        } => ast::Expr::Ternary {
            condition: Box::new(raise_expr(cond)),
            then_expr: Box::new(raise_expr(then)),
            else_expr: Box::new(raise_expr(else_)),
        },

        hir::Expr::MakeClosure {
            fn_name, captures, ..
        } => ast::Expr::MakeClosure {
            fn_name: fn_name.clone(),
            captures: captures.iter().map(raise_expr).collect(),
        },
    }
}

fn raise_type(t: &hir::Ty) -> ast::Type {
    use crate::verify::Ty;
    match t {
        Ty::Number => ast::Type::Number,
        Ty::Text => ast::Type::Text,
        Ty::Bool => ast::Type::Bool,
        Ty::Nil => ast::Type::Any,
        Ty::Optional(inner) => ast::Type::Optional(Box::new(raise_type(inner))),
        Ty::List(inner) => ast::Type::List(Box::new(raise_type(inner))),
        Ty::Map(k, v) => ast::Type::Map(Box::new(raise_type(k)), Box::new(raise_type(v))),
        Ty::Result(ok, err) => {
            ast::Type::Result(Box::new(raise_type(ok)), Box::new(raise_type(err)))
        }
        Ty::Sum(vs) => ast::Type::Sum(vs.clone()),
        Ty::Fn(params, ret) => ast::Type::Fn(
            params.iter().map(raise_type).collect(),
            Box::new(raise_type(ret)),
        ),
        Ty::Named(n) => ast::Type::Named(n.clone()),
        Ty::Unknown => ast::Type::Any,
    }
}

fn expr_span(_e: &hir::Expr) -> ast::Span {
    // HIR expressions all carry a span field, but most lowerings populate it
    // with `Span::UNKNOWN` today because AST expressions don't carry spans.
    // Use UNKNOWN for the raised statement wrapper too — the interpreter
    // doesn't read this field for non-error paths.
    ast::Span::UNKNOWN
}
