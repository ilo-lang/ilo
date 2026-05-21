//! HIR top-level declarations.

use crate::ast::Span;
use crate::hir::expr::Body;
use crate::hir::types::Ty;

/// Function or tool parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Ty,
}

/// Top-level declaration in a HIR program.
///
/// Unlike `ast::Decl`, this enum has no `Alias`, `Use`, or `Error` variants:
/// aliases are resolved away at verify time, `use` is resolved before
/// verification, and `Error` is a parser poison node that verification rejects.
/// Stage 5a lowering drops all three.
#[derive(Debug, Clone, PartialEq)]
pub enum Decl {
    /// `name params > return ; body`
    Function {
        name: String,
        params: Vec<Param>,
        return_type: Ty,
        body: Body,
        span: Span,
    },

    /// `type name { field:type; ... }`
    TypeDef {
        name: String,
        fields: Vec<Param>,
        span: Span,
    },

    /// `tool name "desc" params > return timeout:n,retry:n`
    Tool {
        name: String,
        description: String,
        params: Vec<Param>,
        return_type: Ty,
        timeout: Option<f64>,
        retry: Option<f64>,
        span: Span,
    },
}
