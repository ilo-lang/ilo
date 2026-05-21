//! HIR program — top-level container.

use crate::hir::decl::Decl;

/// A complete HIR program.
///
/// Produced by `hir::lower(ast, verify_out)`. Consumed by every backend in
/// Phase 5. Optional `source` mirrors `ast::Program::source` and is preserved
/// so diagnostics can quote the original code.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub decls: Vec<Decl>,
    pub source: Option<String>,
}

impl Program {
    pub fn new(decls: Vec<Decl>) -> Self {
        Program {
            decls,
            source: None,
        }
    }

    /// Find a function by name. Returns `None` if absent or if the named decl
    /// is a type def or tool decl.
    pub fn function(&self, name: &str) -> Option<&Decl> {
        self.decls.iter().find(|d| match d {
            Decl::Function { name: n, .. } => n == name,
            _ => false,
        })
    }

    /// First function decl, in source order. Used as the default entry point
    /// when no `--func` is specified (mirrors the tree interpreter).
    pub fn first_function(&self) -> Option<&Decl> {
        self.decls
            .iter()
            .find(|d| matches!(d, Decl::Function { .. }))
    }
}
