pub mod ansi;
pub mod json;
pub mod registry;

use crate::ast::Span;

// ---- Fix-plan types (Zero-style structured edit previews) ----

/// A single text replacement within a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixEdit {
    /// Line number (1-based) where the replacement starts.
    pub line_start: usize,
    /// Line number (1-based) where the replacement ends (inclusive).
    pub line_end: usize,
    /// The text that currently appears at this location (before-snippet).
    pub before: String,
    /// The replacement text (after-snippet).
    pub after: String,
}

/// A structured fix plan attached to a diagnostic.
///
/// Agents can apply the edit mechanically without re-parsing prose suggestions.
/// Schema mirrors Zero PR #137: `{ path, line_range, before, after }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixPlan {
    /// File path, if known (absent for inline code snippets).
    pub path: Option<String>,
    /// The edits to apply (MVP: always a single edit).
    pub edits: Vec<FixEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Label {
    pub span: Span,
    pub message: String,
    pub is_primary: bool,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Option<&'static str>,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub suggestion: Option<String>,
    pub source: Option<String>,
    /// File path for fix-plan `path` field; absent for inline code.
    pub path: Option<String>,
    /// Structured fix plan (Zero-style).  Present on diagnostics where the
    /// fix is a mechanical text replacement that an agent can apply blindly.
    pub fix_plan: Option<FixPlan>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            code: None,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
            suggestion: None,
            source: None,
            path: None,
            fix_plan: None,
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            code: None,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
            suggestion: None,
            source: None,
            path: None,
            fix_plan: None,
        }
    }

    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }

    pub fn with_span(mut self, span: Span, label: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: label.into(),
            is_primary: true,
        });
        self
    }

    #[allow(dead_code)] // forward infrastructure for multi-label diagnostics (C3+)
    pub fn with_secondary_span(mut self, span: Span, label: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: label.into(),
            is_primary: false,
        });
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn with_fix_plan(mut self, plan: FixPlan) -> Self {
        self.fix_plan = Some(plan);
        self
    }

    /// Derive a structured fix_plan from the diagnostic's code, span, and
    /// source text.  Must be called *after* `with_source()` has populated
    /// `self.source`.  Idempotent: a pre-existing `fix_plan` is not replaced.
    ///
    /// Covered codes (MVP):
    /// * `ILO-T004` — undefined variable; hint `"did you mean 'X'?"` → rename span
    /// * `ILO-T003` — undefined type; same hint pattern
    /// * `ILO-T032` — bare `fmt`/`fmt2` discarded → prefix `prnt ` before the call
    /// * `ILO-L002` — underscore identifier; suggestion contains the hyphenated form
    pub fn derive_fix_plan(mut self) -> Self {
        if self.fix_plan.is_some() {
            return self;
        }
        let Some(code) = self.code else {
            return self;
        };
        let Some(source) = &self.source else {
            return self;
        };

        let plan = match code {
            "ILO-T004" | "ILO-T003" => derive_typo_rename(&self, source),
            "ILO-T032" => derive_fmt_prefix(&self, source),
            "ILO-L002" => derive_underscore_hyphen(&self, source),
            "ILO-T008" => derive_return_type_cast(&self, source),
            "ILO-P011" => derive_reserved_rename(&self, source),
            "ILO-T041" => derive_nil_coalesce_result(&self, source),
            _ => None,
        };
        self.fix_plan = plan;
        self
    }
}

// ---- Fix-plan derivation helpers ----

use crate::ast::SourceMap;

/// Extract the suggested name from `"did you mean 'X'?"` and build an edit
/// that replaces the primary span with X.
fn derive_typo_rename(d: &Diagnostic, source: &str) -> Option<FixPlan> {
    let hint = d.suggestion.as_deref()?;
    // Match: did you mean 'X'?
    let after = hint
        .strip_prefix("did you mean '")?
        .strip_suffix("'?")?
        .to_string();

    let span = d.labels.iter().find(|l| l.is_primary).map(|l| l.span)?;
    if span.start >= source.len() || span.end > source.len() || span.start >= span.end {
        return None;
    }
    let before = source[span.start..span.end].to_string();
    if before.is_empty() || after.is_empty() {
        return None;
    }

    let sm = SourceMap::new(source);
    let (line_start, _) = sm.lookup(span.start);
    let (line_end, _) = sm.lookup(span.end.saturating_sub(1));

    Some(FixPlan {
        path: d.path.clone(),
        edits: vec![FixEdit {
            line_start,
            line_end,
            before,
            after,
        }],
    })
}

/// Build a fix that prepends `prnt ` before the bare `fmt`/`fmt2` call.
/// The span covers the entire call expression; we replace `fmt args` with
/// `prnt fmt args`.
fn derive_fmt_prefix(d: &Diagnostic, source: &str) -> Option<FixPlan> {
    let span = d.labels.iter().find(|l| l.is_primary).map(|l| l.span)?;
    if span.start >= source.len() || span.end > source.len() || span.start >= span.end {
        return None;
    }
    let before = source[span.start..span.end].to_string();

    // Identify which name is used (fmt / fmt2).
    let prefix = if before.starts_with("fmt2") {
        "fmt2"
    } else if before.starts_with("fmt") {
        "fmt"
    } else {
        return None;
    };
    let _ = prefix; // used implicitly via before

    let after = format!("prnt {before}");
    let sm = SourceMap::new(source);
    let (line_start, _) = sm.lookup(span.start);
    let (line_end, _) = sm.lookup(span.end.saturating_sub(1));

    Some(FixPlan {
        path: d.path.clone(),
        edits: vec![FixEdit {
            line_start,
            line_end,
            before,
            after,
        }],
    })
}

/// L002: suggestion text contains the corrected hyphenated form.
/// Pattern: `"underscores are not allowed in identifiers; use hyphens (e.g. \`X\`)"`.
fn derive_underscore_hyphen(d: &Diagnostic, source: &str) -> Option<FixPlan> {
    let suggestion = d.suggestion.as_deref()?;
    // Extract the backtick-quoted corrected form.
    let after = suggestion.rsplit('`').nth(1)?.to_string();
    if after.is_empty() {
        return None;
    }

    let span = d.labels.iter().find(|l| l.is_primary).map(|l| l.span)?;
    if span.start >= source.len() || span.end > source.len() || span.start >= span.end {
        return None;
    }
    let before = source[span.start..span.end].to_string();

    let sm = SourceMap::new(source);
    let (line_start, _) = sm.lookup(span.start);
    let (line_end, _) = sm.lookup(span.end.saturating_sub(1));

    Some(FixPlan {
        path: d.path.clone(),
        edits: vec![FixEdit {
            line_start,
            line_end,
            before,
            after,
        }],
    })
}

/// T008: return type mismatch.
///
/// When the hint is `"use 'str' to convert: str <expr>"` or `"use 'num' to
/// parse text …"` we wrap the offending expression with the cast function.
/// For the generic `"change the return expression to T, or …"` pattern we
/// extract T and do the same.
fn derive_return_type_cast(d: &Diagnostic, source: &str) -> Option<FixPlan> {
    let hint = d.suggestion.as_deref()?;
    let span = d.labels.iter().find(|l| l.is_primary).map(|l| l.span)?;
    if span.start >= source.len() || span.end > source.len() || span.start >= span.end {
        return None;
    }
    let before = source[span.start..span.end].to_string();
    if before.is_empty() {
        return None;
    }

    // Determine the cast function from the hint text.
    let cast_fn: Option<&str> = if hint.starts_with("use 'str' to convert") {
        Some("str")
    } else if hint.starts_with("use 'num' to parse text") {
        Some("num")
    } else if let Some(rest) = hint.strip_prefix("change the return expression to ") {
        // Extract the type token before the first comma or space.
        let ty = rest.split([',', ' ']).next().unwrap_or("").trim();
        match ty {
            "n" | "num" | "number" => Some("num"),
            "t" | "str" | "text" => Some("str"),
            _ => None,
        }
    } else {
        None
    };

    let cast_fn = cast_fn?;
    let after = format!("{cast_fn} {before}");

    let sm = SourceMap::new(source);
    let (line_start, _) = sm.lookup(span.start);
    let (line_end, _) = sm.lookup(span.end.saturating_sub(1));

    Some(FixPlan {
        path: d.path.clone(),
        edits: vec![FixEdit {
            line_start,
            line_end,
            before,
            after,
        }],
    })
}

/// P011: reserved keyword used as identifier.
///
/// Extracts the offending name from the primary span and offers `<name>2`
/// as the replacement (safe, mechanical rename).
fn derive_reserved_rename(d: &Diagnostic, source: &str) -> Option<FixPlan> {
    let span = d.labels.iter().find(|l| l.is_primary).map(|l| l.span)?;
    if span.start >= source.len() || span.end > source.len() || span.start >= span.end {
        return None;
    }
    let before = source[span.start..span.end].to_string();
    if before.is_empty() {
        return None;
    }
    // Only offer the mechanical rename when the span looks like a plain
    // identifier / keyword (no whitespace or operators).
    if before.contains(|c: char| c.is_whitespace()) {
        return None;
    }
    let after = format!("{before}2");

    let sm = SourceMap::new(source);
    let (line_start, _) = sm.lookup(span.start);
    let (line_end, _) = sm.lookup(span.end.saturating_sub(1));

    Some(FixPlan {
        path: d.path.clone(),
        edits: vec![FixEdit {
            line_start,
            line_end,
            before,
            after,
        }],
    })
}

/// T041: `??` nil-coalesce applied to a `R T E` (Result) type.
///
/// The primary span covers the full `<value> ?? <default>` expression.
/// We rewrite it to `?val{~v:v;^_:<default>}` which is the idiomatic
/// full-control pattern, splitting on ` ?? ` to recover the two operands.
fn derive_nil_coalesce_result(d: &Diagnostic, source: &str) -> Option<FixPlan> {
    let span = d.labels.iter().find(|l| l.is_primary).map(|l| l.span)?;
    if span.start >= source.len() || span.end > source.len() || span.start >= span.end {
        return None;
    }
    let before = source[span.start..span.end].to_string();
    if before.is_empty() {
        return None;
    }

    // Split on the nil-coalesce operator to recover value and default.
    // Use the first occurrence to handle nested `??`.
    let (val_part, default_part) = before.split_once(" ?? ")?;
    let val_part = val_part.trim();
    let default_part = default_part.trim();

    let after = format!("?{val_part}{{~v:v;^_:{default_part}}}");

    let sm = SourceMap::new(source);
    let (line_start, _) = sm.lookup(span.start);
    let (line_end, _) = sm.lookup(span.end.saturating_sub(1));

    Some(FixPlan {
        path: d.path.clone(),
        edits: vec![FixEdit {
            line_start,
            line_end,
            before,
            after,
        }],
    })
}

// ---- From impls for existing error types ----

impl From<&crate::lexer::LexError> for Diagnostic {
    fn from(e: &crate::lexer::LexError) -> Self {
        let span = Span {
            start: e.position,
            end: e.position + e.snippet.len().max(1),
        };
        let mut d = Diagnostic::error(format!("unexpected token '{}'", e.snippet))
            .with_code(e.code)
            .with_span(span, "here");
        if !e.suggestion.is_empty() {
            d = d.with_suggestion(e.suggestion.clone());
        }
        d
    }
}

impl From<&crate::parser::ParseError> for Diagnostic {
    fn from(e: &crate::parser::ParseError) -> Self {
        let mut d = Diagnostic::error(&e.message)
            .with_code(e.code)
            .with_span(e.span, "here");
        if let Some(hint) = &e.hint {
            d = d.with_suggestion(hint.clone());
        }
        d
    }
}

impl From<&crate::verify::VerifyError> for Diagnostic {
    fn from(e: &crate::verify::VerifyError) -> Self {
        let mut d = if e.is_warning {
            Diagnostic::warning(&e.message)
        } else {
            Diagnostic::error(&e.message)
        }
        .with_code(e.code)
        .with_note(format!("in function '{}'", e.function));
        if let Some(span) = e.span {
            d = d.with_span(span, "");
        }
        if let Some(hint) = &e.hint {
            d = d.with_suggestion(hint.clone());
        }
        d
    }
}

impl From<&crate::runtime::RuntimeError> for Diagnostic {
    fn from(e: &crate::runtime::RuntimeError) -> Self {
        let mut d = Diagnostic::error(&e.message).with_code(e.code);
        if let Some(span) = e.span {
            d = d.with_span(span, "here");
        }
        for name in &e.call_stack {
            d = d.with_note(format!("called from '{name}'"));
        }
        d
    }
}

impl From<&crate::vm::VmRuntimeError> for Diagnostic {
    fn from(e: &crate::vm::VmRuntimeError) -> Self {
        let mut d = Diagnostic::from(&e.error);
        if let Some(span) = e.span {
            d = d.with_span(span, "here");
        }
        for name in &e.call_stack {
            d = d.with_note(format!("called from '{name}'"));
        }
        d
    }
}

impl From<&crate::vm::VmError> for Diagnostic {
    fn from(e: &crate::vm::VmError) -> Self {
        use crate::vm::VmError;
        let code = match e {
            VmError::NoFunctionsDefined => "ILO-R012",
            VmError::UndefinedFunction { .. } => "ILO-R002",
            VmError::DivisionByZero => "ILO-R003",
            VmError::FieldNotFound { .. } => "ILO-R005",
            VmError::UnknownOpcode { .. } => "ILO-R013",
            VmError::Type(_) => "ILO-R004",
            VmError::Arity { .. } => "ILO-R004",
            // Tree-bridge surfaces the interpreter's RuntimeError verbatim;
            // R009 mirrors what the tree interpreter would produce for the
            // same call, keeping diagnostic codes consistent across engines.
            VmError::Runtime(_) => "ILO-R009",
        };
        Diagnostic::error(e.to_string()).with_code(code)
    }
}

impl From<&crate::vm::CompileError> for Diagnostic {
    fn from(e: &crate::vm::CompileError) -> Self {
        use crate::vm::CompileError;
        let code = match e {
            CompileError::UndefinedVariable { .. } => "ILO-R010",
            CompileError::UndefinedFunction { .. } => "ILO-R011",
            // Wide-capture (>255) VM-encoding cap. Phase 2 closure capture
            // itself is fully supported across tree / VM / Cranelift JIT;
            // this is the engine-specific encoding limit only.
            CompileError::UnsupportedClosureCapture { .. } => "ILO-E802",
            CompileError::RegisterOverflow { .. } => "ILO-T035",
            CompileError::CallRegisterOverflow { .. } => "ILO-T036",
            CompileError::SumTypeNotSupported { .. } => "ILO-E803",
        };
        let d = Diagnostic::error(e.to_string()).with_code(code);
        match e {
            CompileError::RegisterOverflow { span, .. }
            | CompileError::CallRegisterOverflow { span, .. }
                if span.start != span.end =>
            {
                d.with_span(*span, "in this function")
            }
            _ => d,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Span;

    #[test]
    fn diagnostic_error_builder() {
        let d = Diagnostic::error("something went wrong");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.message, "something went wrong");
        assert!(d.labels.is_empty());
        assert!(d.notes.is_empty());
        assert!(d.suggestion.is_none());
    }

    #[test]
    fn diagnostic_with_span() {
        let d = Diagnostic::error("bad token").with_span(Span { start: 5, end: 8 }, "here");
        assert_eq!(d.labels.len(), 1);
        assert_eq!(d.labels[0].span.start, 5);
        assert_eq!(d.labels[0].span.end, 8);
        assert!(d.labels[0].is_primary);
    }

    #[test]
    fn diagnostic_with_note_and_suggestion() {
        let d = Diagnostic::error("type mismatch")
            .with_note("in function 'foo'")
            .with_suggestion("use n instead of t");
        assert_eq!(d.notes, vec!["in function 'foo'"]);
        assert_eq!(d.suggestion.as_deref(), Some("use n instead of t"));
    }

    #[test]
    fn from_lex_error() {
        let e = crate::lexer::LexError {
            code: "ILO-L002",
            position: 3,
            snippet: "my_func".to_string(),
            suggestion: "Use hyphens: 'my-func'".to_string(),
        };
        let d = Diagnostic::from(&e);
        assert_eq!(d.severity, Severity::Error);
        assert!(d.message.contains("my_func"));
        assert_eq!(d.labels[0].span.start, 3);
        assert_eq!(d.labels[0].span.end, 10); // 3 + len("my_func")
        assert!(d.suggestion.is_some());
        assert_eq!(d.code, Some("ILO-L002"));
    }

    #[test]
    fn from_parse_error() {
        let e = crate::parser::ParseError {
            code: "ILO-P005",
            position: 2,
            span: Span { start: 10, end: 15 },
            message: "expected identifier".to_string(),
            hint: None,
        };
        let d = Diagnostic::from(&e);
        assert!(d.message.contains("expected identifier"));
        assert_eq!(d.labels[0].span, Span { start: 10, end: 15 });
        assert_eq!(d.code, Some("ILO-P005"));
        assert!(d.suggestion.is_none());
    }

    #[test]
    fn from_parse_error_with_hint() {
        let e = crate::parser::ParseError {
            code: "ILO-P001",
            position: 0,
            span: Span { start: 0, end: 8 },
            message: "expected declaration, got identifier `function`".to_string(),
            hint: Some("ilo function syntax: name param:type > return-type; body".to_string()),
        };
        let d = Diagnostic::from(&e);
        assert_eq!(
            d.suggestion.as_deref(),
            Some("ilo function syntax: name param:type > return-type; body")
        );
    }

    #[test]
    fn diagnostic_warning_constructor() {
        let d = Diagnostic::warning("cross-language syntax detected");
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.message, "cross-language syntax detected");
        assert!(d.code.is_none());
    }

    #[test]
    fn from_verify_error() {
        let e = crate::verify::VerifyError {
            code: "ILO-T004",
            function: "myFunc".to_string(),
            message: "undefined variable 'x'".to_string(),
            hint: Some("did you mean 'y'?".to_string()),
            span: None,
            is_warning: false,
        };
        let d = Diagnostic::from(&e);
        assert_eq!(d.severity, Severity::Error);
        assert!(d.message.contains("undefined variable"));
        assert!(d.notes.iter().any(|n| n.contains("myFunc")));
        assert!(d.suggestion.is_some());
        assert_eq!(d.code, Some("ILO-T004"));
    }

    #[test]
    fn from_verify_warning() {
        let e = crate::verify::VerifyError {
            code: "ILO-T029",
            function: "f".to_string(),
            message: "unreachable code after 'ret'".to_string(),
            hint: None,
            span: Some(Span { start: 10, end: 15 }),
            is_warning: true,
        };
        let d = Diagnostic::from(&e);
        assert_eq!(d.severity, Severity::Warning);
        assert!(d.message.contains("unreachable"));
        assert_eq!(d.code, Some("ILO-T029"));
    }

    #[test]
    fn from_runtime_error() {
        let e = crate::runtime::RuntimeError {
            code: "ILO-R003",
            message: "division by zero".to_string(),
            span: None,
            call_stack: Vec::new(),
            propagate_value: None,
        };
        let d = Diagnostic::from(&e);
        assert!(d.message.contains("division by zero"));
        assert!(d.labels.is_empty()); // no span when RuntimeError.span is None
        assert_eq!(d.code, Some("ILO-R003"));
    }

    #[test]
    fn from_runtime_error_with_span() {
        use crate::ast::Span;
        let e = crate::runtime::RuntimeError {
            code: "ILO-R003",
            message: "division by zero".to_string(),
            span: Some(Span { start: 5, end: 10 }),
            call_stack: vec!["f".to_string()],
            propagate_value: None,
        };
        let d = Diagnostic::from(&e);
        assert!(d.message.contains("division by zero"));
        assert_eq!(d.labels.len(), 1);
        assert_eq!(d.labels[0].span, Span { start: 5, end: 10 });
        assert!(d.notes.iter().any(|n| n.contains("'f'")));
    }

    #[test]
    fn from_vm_runtime_error() {
        use crate::ast::Span;
        let e = crate::vm::VmRuntimeError {
            error: crate::vm::VmError::DivisionByZero,
            span: Some(Span { start: 3, end: 6 }),
            call_stack: vec!["g".to_string()],
        };
        let d = Diagnostic::from(&e);
        assert_eq!(d.code, Some("ILO-R003"));
        assert_eq!(d.labels.len(), 1);
        assert!(d.notes.iter().any(|n| n.contains("'g'")));
    }

    #[test]
    fn from_vm_error() {
        let e = crate::vm::VmError::UndefinedFunction {
            name: "foo".to_string(),
        };
        let d = Diagnostic::from(&e);
        assert!(d.message.contains("foo"));
    }

    #[test]
    fn from_compile_error() {
        let e = crate::vm::CompileError::UndefinedVariable {
            name: "x".to_string(),
        };
        let d = Diagnostic::from(&e);
        assert!(d.message.contains("x"));
    }

    // ---- Uncovered VmError variants ----

    #[test]
    fn from_vm_error_no_functions_defined() {
        let e = crate::vm::VmError::NoFunctionsDefined;
        let d = Diagnostic::from(&e);
        assert_eq!(d.code, Some("ILO-R012"));
        assert!(d.message.contains("no functions defined"));
    }

    #[test]
    fn from_compile_error_unsupported_closure_capture_maps_to_e801() {
        // Phase 2 closure capture is fully supported on tree / VM / Cranelift JIT.
        // `CompileError::UnsupportedClosureCapture` now only fires on the
        // >255-capture VM-encoding cap, so it maps to the engine-specific
        // `ILO-E802`, not the misleading `ILO-R012` ("no functions defined")
        // it used to share. Regression guard so a future refactor doesn't
        // silently revive the old contract.
        let e = crate::vm::CompileError::UnsupportedClosureCapture {
            fn_name: "__lit_0".to_string(),
        };
        let d = Diagnostic::from(&e);
        assert_eq!(d.code, Some("ILO-E802"));
        assert!(d.message.contains("255-capture"));
    }

    #[test]
    fn from_vm_error_division_by_zero() {
        let e = crate::vm::VmError::DivisionByZero;
        let d = Diagnostic::from(&e);
        assert_eq!(d.code, Some("ILO-R003"));
        assert!(d.message.contains("division by zero"));
    }

    #[test]
    fn from_vm_error_field_not_found() {
        let e = crate::vm::VmError::FieldNotFound {
            field: "foo".to_string(),
        };
        let d = Diagnostic::from(&e);
        assert_eq!(d.code, Some("ILO-R005"));
        assert!(d.message.contains("foo"));
    }

    #[test]
    fn from_vm_error_unknown_opcode() {
        let e = crate::vm::VmError::UnknownOpcode { op: 99 };
        let d = Diagnostic::from(&e);
        assert_eq!(d.code, Some("ILO-R013"));
        assert!(d.message.contains("99"));
    }

    #[test]
    fn from_vm_error_type() {
        let e = crate::vm::VmError::Type("expected number");
        let d = Diagnostic::from(&e);
        assert_eq!(d.code, Some("ILO-R004"));
        assert!(d.message.contains("expected number"));
    }

    #[test]
    fn from_compile_error_undefined_function() {
        let e = crate::vm::CompileError::UndefinedFunction {
            name: "bar".to_string(),
        };
        let d = Diagnostic::from(&e);
        assert_eq!(d.code, Some("ILO-R011"));
        assert!(d.message.contains("bar"));
    }

    // ---- with_secondary_span ----

    #[test]
    fn diagnostic_with_secondary_span() {
        let d = Diagnostic::error("type mismatch")
            .with_secondary_span(Span { start: 10, end: 15 }, "secondary label");
        assert_eq!(d.labels.len(), 1);
        assert!(!d.labels[0].is_primary);
        assert_eq!(d.labels[0].message, "secondary label");
    }

    // ---- with_source ----

    #[test]
    fn diagnostic_with_source() {
        let d = Diagnostic::error("bad").with_source("f x:n>n;x".to_string());
        assert_eq!(d.source.as_deref(), Some("f x:n>n;x"));
    }

    // ---- fix_plan derivation ----

    #[test]
    fn derive_fix_plan_t004_typo_rename() {
        // ILO-T004: undefined variable with "did you mean 'X'?" hint
        let source = "f x:n>n;xyzz";
        let d = Diagnostic::error("undefined variable 'xyzz'")
            .with_code("ILO-T004")
            .with_span(Span { start: 8, end: 12 }, "")
            .with_suggestion("did you mean 'x'?")
            .with_source(source.to_string())
            .derive_fix_plan();
        let plan = d.fix_plan.expect("fix_plan should be derived for T004");
        assert_eq!(plan.edits.len(), 1);
        assert_eq!(plan.edits[0].before, "xyzz");
        assert_eq!(plan.edits[0].after, "x");
        assert_eq!(plan.edits[0].line_start, 1);
    }

    #[test]
    fn derive_fix_plan_t003_type_rename() {
        // ILO-T003: undefined type with "did you mean 'X'?" hint
        let source = "f x:Numbr>n;x";
        let d = Diagnostic::error("undefined type 'Numbr'")
            .with_code("ILO-T003")
            .with_span(Span { start: 4, end: 9 }, "")
            .with_suggestion("did you mean 'Number'?")
            .with_source(source.to_string())
            .derive_fix_plan();
        let plan = d.fix_plan.expect("fix_plan should be derived for T003");
        assert_eq!(plan.edits[0].before, "Numbr");
        assert_eq!(plan.edits[0].after, "Number");
    }

    #[test]
    fn derive_fix_plan_t032_fmt_prefix() {
        // ILO-T032: bare fmt call → prnt fmt
        let source = r#"f x:n>n;fmt "{}" x;x"#;
        let fmt_start = source.find("fmt").unwrap();
        let fmt_end = source.find(";x").unwrap(); // up to next statement
        let d = Diagnostic::warning("bare 'fmt' result is discarded")
            .with_code("ILO-T032")
            .with_span(
                Span {
                    start: fmt_start,
                    end: fmt_end,
                },
                "",
            )
            .with_suggestion("did you mean `prnt fmt ...` to print?")
            .with_source(source.to_string())
            .derive_fix_plan();
        let plan = d.fix_plan.expect("fix_plan should be derived for T032");
        assert!(plan.edits[0].after.starts_with("prnt fmt"));
        assert_eq!(plan.edits[0].before, &source[fmt_start..fmt_end]);
    }

    #[test]
    fn derive_fix_plan_l002_hyphen() {
        // ILO-L002: underscore ident → hyphenated
        let source = "my_var=5;my_var";
        let d = Diagnostic::error("unexpected token 'my_var'")
            .with_code("ILO-L002")
            .with_span(Span { start: 0, end: 6 }, "here")
            .with_suggestion(
                "underscores are not allowed in identifiers; use hyphens (e.g. `my-var`)",
            )
            .with_source(source.to_string())
            .derive_fix_plan();
        let plan = d.fix_plan.expect("fix_plan should be derived for L002");
        assert_eq!(plan.edits[0].before, "my_var");
        assert_eq!(plan.edits[0].after, "my-var");
    }

    #[test]
    fn derive_fix_plan_absent_without_hint() {
        // T004 with no hint → no fix_plan
        let d = Diagnostic::error("undefined variable 'foo'")
            .with_code("ILO-T004")
            .with_span(Span { start: 0, end: 3 }, "")
            .with_source("foo".to_string())
            .derive_fix_plan();
        assert!(d.fix_plan.is_none());
    }

    #[test]
    fn derive_fix_plan_absent_without_source() {
        // No source → no fix_plan even with hint
        let d = Diagnostic::error("undefined variable 'xyzz'")
            .with_code("ILO-T004")
            .with_span(Span { start: 0, end: 4 }, "")
            .with_suggestion("did you mean 'x'?")
            .derive_fix_plan();
        assert!(d.fix_plan.is_none());
    }

    #[test]
    fn derive_fix_plan_json_shape() {
        // Verify fix_plan appears in JSON output with correct keys
        let source = "f x:n>n;xyzz";
        let d = Diagnostic::error("undefined variable 'xyzz'")
            .with_code("ILO-T004")
            .with_span(Span { start: 8, end: 12 }, "")
            .with_suggestion("did you mean 'x'?")
            .with_source(source.to_string())
            .derive_fix_plan();
        let json_str = super::json::render(&d);
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let plan = &v["fix_plan"];
        assert!(!plan.is_null());
        let edits = plan["edits"].as_array().unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["before"], "xyzz");
        assert_eq!(edits[0]["after"], "x");
        assert!(edits[0]["line_range"].is_array());
    }

    // ---- ILO-T008 fix_plan derivation ----

    #[test]
    fn derive_fix_plan_t008_str_cast() {
        // Hint: "use 'str' to convert: str <expr>" → wrap span with `str`
        let source = "f x:n>t;x";
        let d = Diagnostic::error("return type mismatch: expected t, got n")
            .with_code("ILO-T008")
            .with_span(Span { start: 8, end: 9 }, "")
            .with_suggestion("use 'str' to convert: str <expr>")
            .with_source(source.to_string())
            .derive_fix_plan();
        let plan = d.fix_plan.expect("fix_plan for T008 str cast");
        assert_eq!(plan.edits[0].before, "x");
        assert_eq!(plan.edits[0].after, "str x");
    }

    #[test]
    fn derive_fix_plan_t008_num_cast() {
        // Hint: "use 'num' to parse text …" → wrap span with `num`
        let source = "f x:t>n;x";
        let d = Diagnostic::error("return type mismatch: expected n, got t")
            .with_code("ILO-T008")
            .with_span(Span { start: 8, end: 9 }, "")
            .with_suggestion("use 'num' to parse text (returns R n t)")
            .with_source(source.to_string())
            .derive_fix_plan();
        let plan = d.fix_plan.expect("fix_plan for T008 num cast");
        assert_eq!(plan.edits[0].before, "x");
        assert_eq!(plan.edits[0].after, "num x");
    }

    #[test]
    fn derive_fix_plan_t008_generic_no_match() {
        // Generic hint where type is not num/str → no fix_plan
        let source = "f x:n>b;x";
        let d = Diagnostic::error("return type mismatch: expected b, got n")
            .with_code("ILO-T008")
            .with_span(Span { start: 8, end: 9 }, "")
            .with_suggestion(
                "change the return expression to b, or update the return type annotation",
            )
            .with_source(source.to_string())
            .derive_fix_plan();
        assert!(
            d.fix_plan.is_none(),
            "no fix_plan for non-cast generic type"
        );
    }

    // ---- ILO-P011 fix_plan derivation ----

    #[test]
    fn derive_fix_plan_p011_reserved_rename() {
        // Reserved keyword `var` used as binding name → rename to `var2`
        let source = "var=5;var";
        let d = Diagnostic::error("`var` is a reserved word and cannot be used as an identifier")
            .with_code("ILO-P011")
            .with_span(Span { start: 0, end: 3 }, "here")
            .with_suggestion("`var` is reserved; rename to e.g. `v`, `value`, or `varv`")
            .with_source(source.to_string())
            .derive_fix_plan();
        let plan = d.fix_plan.expect("fix_plan for P011");
        assert_eq!(plan.edits[0].before, "var");
        assert_eq!(plan.edits[0].after, "var2");
    }

    #[test]
    fn derive_fix_plan_p011_no_plan_for_whitespace_span() {
        // If the span somehow covers whitespace, don't emit a plan
        let source = "  fn=5";
        let d = Diagnostic::error("`fn` is a reserved word")
            .with_code("ILO-P011")
            .with_span(Span { start: 0, end: 4 }, "here") // covers "  fn"
            .with_source(source.to_string())
            .derive_fix_plan();
        assert!(d.fix_plan.is_none(), "no plan when span has whitespace");
    }

    // ---- ILO-T041 fix_plan derivation ----

    #[test]
    fn derive_fix_plan_t041_nil_coalesce_result() {
        // `num s ?? 0` on a Result → `?num s{~v:v;^_:0}`
        let source = "f s:t>n;num s ?? 0";
        let expr_start = source.find("num s").unwrap();
        let expr_end = source.len();
        let d = Diagnostic::error("`??` is nil-coalesce for `O T`, not `R T E`")
            .with_code("ILO-T041")
            .with_span(
                Span {
                    start: expr_start,
                    end: expr_end,
                },
                "",
            )
            .with_suggestion("use `default-on-err r d` or `?r{~v:v ^_:default}` for full control")
            .with_source(source.to_string())
            .derive_fix_plan();
        let plan = d.fix_plan.expect("fix_plan for T041");
        assert_eq!(plan.edits[0].before, "num s ?? 0");
        assert_eq!(plan.edits[0].after, "?num s{~v:v;^_:0}");
    }

    #[test]
    fn derive_fix_plan_t041_no_plan_when_no_double_question() {
        // If somehow the span text lacks ' ?? ', no plan emitted
        let source = "f s:t>n;num s";
        let d = Diagnostic::error("`??` is nil-coalesce for `O T`, not `R T E`")
            .with_code("ILO-T041")
            .with_span(Span { start: 8, end: 13 }, "")
            .with_source(source.to_string())
            .derive_fix_plan();
        assert!(d.fix_plan.is_none(), "no plan when span lacks ' ?? '");
    }

    #[test]
    fn fix_plan_with_path_in_json() {
        let source = "f x:n>n;xyzz";
        let d = Diagnostic::error("undefined variable 'xyzz'")
            .with_code("ILO-T004")
            .with_span(Span { start: 8, end: 12 }, "")
            .with_suggestion("did you mean 'x'?")
            .with_source(source.to_string())
            .with_path("/tmp/test.ilo")
            .derive_fix_plan();
        let json_str = super::json::render(&d);
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["fix_plan"]["path"], "/tmp/test.ilo");
    }
}
