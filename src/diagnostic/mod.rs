pub mod ansi;
pub mod json;
pub mod registry;

use crate::ast::Span;

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
        }
    }

    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }

    pub fn with_span(mut self, span: Span, label: impl Into<String>) -> Self {
        self.labels.push(Label { span, message: label.into(), is_primary: true });
        self
    }

    #[allow(dead_code)] // forward infrastructure for multi-label diagnostics (C3+)
    pub fn with_secondary_span(mut self, span: Span, label: impl Into<String>) -> Self {
        self.labels.push(Label { span, message: label.into(), is_primary: false });
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
        let mut d = Diagnostic::error(&e.message).with_code(e.code).with_span(e.span, "here");
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

impl From<&crate::interpreter::RuntimeError> for Diagnostic {
    fn from(e: &crate::interpreter::RuntimeError) -> Self {
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
        };
        Diagnostic::error(e.to_string()).with_code(code)
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
        let d = Diagnostic::error("bad token")
            .with_span(Span { start: 5, end: 8 }, "here");
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
            message: "expected declaration, got Ident(\"function\")".to_string(),
            hint: Some("ilo function syntax: name param:type > return-type; body".to_string()),
        };
        let d = Diagnostic::from(&e);
        assert_eq!(d.suggestion.as_deref(), Some("ilo function syntax: name param:type > return-type; body"));
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
        let e = crate::interpreter::RuntimeError {
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
        let e = crate::interpreter::RuntimeError {
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
        let e = crate::vm::VmError::UndefinedFunction { name: "foo".to_string() };
        let d = Diagnostic::from(&e);
        assert!(d.message.contains("foo"));
    }

    #[test]
    fn from_compile_error() {
        let e = crate::vm::CompileError::UndefinedVariable { name: "x".to_string() };
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
    fn from_vm_error_division_by_zero() {
        let e = crate::vm::VmError::DivisionByZero;
        let d = Diagnostic::from(&e);
        assert_eq!(d.code, Some("ILO-R003"));
        assert!(d.message.contains("division by zero"));
    }

    #[test]
    fn from_vm_error_field_not_found() {
        let e = crate::vm::VmError::FieldNotFound { field: "foo".to_string() };
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
        let e = crate::vm::CompileError::UndefinedFunction { name: "bar".to_string() };
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
}
