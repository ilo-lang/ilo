pub mod ansi;
pub mod json;
pub mod registry;

use crate::ast::Span;

/// Hints that mark narrow, type-proven automatic fixes. These strings are
/// shared with the verifier so code actions are only offered when the type
/// checker has established the required preconditions.
pub const FIX_HINT_JOIN_TO_CAT: &str =
    "`join` is not an ilo builtin; for a list of text and a separator, use `cat list separator`";
pub const FIX_HINT_SWAP_AT_ARGS: &str =
    "the first argument is numeric and the second is a list/text; use `at collection index`";
pub const FIX_HINT_NOT_TO_BANG: &str =
    "`not` is not an ilo function; for this boolean argument use the prefix operator `!`";
pub const FIX_HINT_RANGE_ZERO_START: &str =
    "`range` takes two arguments; for `range n` (0..n) write `range 0 n`";
pub const FIX_HINT_TERNARY_BOOL_LITERALS: &str =
    "ternary branches are numeric literals in a bool-returning expression; write `true`/`false`";

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

/// A structured, machine-applicable fix.
///
/// The language layer (parser / verifier / lexer) knows the intended form at
/// the raise site, so it hands the fix layer *facts* rather than English prose
/// that would have to be re-parsed. `derive_fix_plan` turns one of these plus
/// the source text into a concrete byte edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fix {
    /// Replace the identifier `from` with `to`.
    ///
    /// Covers typo renames (T003 / T004 / T005) and the underscore→hyphen
    /// rewrite (L002). `from` is matched as a whole token inside the primary
    /// span, so a span that covers a wider expression still yields a
    /// single-token edit.
    Rename { from: String, to: String },
    /// The primary span holds a bare zero-arg function name; rewrite it to
    /// `name()` so the reference becomes a call (T039).
    CallZeroArg { name: String },
    /// The primary span holds a discarded `fmt` / `fmt2` call; prefix it with
    /// `prnt ` so the result is printed instead of dropped (T032).
    PrefixPrnt,
    /// Swap `at index collection` to `at collection index` when types prove
    /// the two argument roles.
    SwapAtArgs,
    /// Rewrite the one-argument Python-style `not expr` call to ilo `!expr`
    /// when the verifier proves `expr` is boolean.
    NotCallToBang,
    /// Rewrite the single-argument `range n` to `range 0 n` (0..n).
    RangeAddZeroStart,
    /// Rewrite numeric literal ternary branches `1`/`0` to `true`/`false`
    /// when the enclosing expression must be bool.
    TernaryBoolLiterals,
    /// Wrap an expression in braces (for guard bodies)
    WrapInBraces,
    /// Remove Python-style `let X = Y in E` syntax
    RemoveLet,
}

impl Fix {
    /// Derive a structured fix for a verification error.
    ///
    /// The verifier already knows the intended form when it raises: the
    /// offending name appears in the message and the replacement appears in
    /// the hint. Recovering both here — in the language layer, next to the
    /// code that produced them — keeps the LSP-facing fix layer free of
    /// prose-scraping and keeps the two in sync automatically.
    ///
    /// Returns `None` when the error has no mechanical fix (the vast
    /// majority): only pure renames and call/print corrections qualify.
    pub fn for_verify_error(error: &crate::verify::VerifyError) -> Option<Fix> {
        let message = error.message.as_str();
        let hint = error.hint.as_deref().unwrap_or("");

        match error.code {
            // `undefined type 'X'` + `did you mean 'Y'?` -> rename X to Y.
            "ILO-T003" | "ILO-T004" | "ILO-T005" => {
                let from = quoted_name(message)?;
                if error.code == "ILO-T005" && from == "join" && hint == FIX_HINT_JOIN_TO_CAT {
                    return Some(Fix::Rename {
                        from: "join".to_string(),
                        to: "cat".to_string(),
                    });
                }
                if error.code == "ILO-T005" && from == "not" && hint == FIX_HINT_NOT_TO_BANG {
                    return Some(Fix::NotCallToBang);
                }
                let to = did_you_mean(hint)?;
                if from == to {
                    return None;
                }
                Some(Fix::Rename {
                    from: from.to_string(),
                    to: to.to_string(),
                })
            }
            "ILO-T013" if hint == FIX_HINT_SWAP_AT_ARGS => Some(Fix::SwapAtArgs),
            "ILO-T006" if hint == FIX_HINT_RANGE_ZERO_START => Some(Fix::RangeAddZeroStart),
            "ILO-T008" if hint == FIX_HINT_TERNARY_BOOL_LITERALS => Some(Fix::TernaryBoolLiterals),
            // `'X' is a 0-arg function used as a value reference` -> call it.
            "ILO-T039" => {
                let name = quoted_name(message)?;
                Some(Fix::CallZeroArg {
                    name: name.to_string(),
                })
            }
            // `bare 'fmt' result is discarded` -> print it instead.
            "ILO-T032" => Some(Fix::PrefixPrnt),
            _ => None,
        }
    }
}

/// Extract the first single-quoted name from a diagnostic message.
///
/// The verifier formats these messages with the offending identifier in
/// single quotes (`undefined function 'uniq'`), so this is a stable
/// contract between the raise site and the fix layer — not prose scraping.
fn quoted_name(message: &str) -> Option<&str> {
    let start = message.find('\'')? + 1;
    let rest = &message[start..];
    let end = rest.find('\'')?;
    Some(&rest[..end])
}

/// Extract the replacement from a `did you mean 'X'?` hint.
fn did_you_mean(hint: &str) -> Option<&str> {
    let rest = hint.strip_prefix("did you mean '")?;
    let end = rest.find("'?")?;
    Some(&rest[..end])
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
    /// Structured fix produced by the language layer. `derive_fix_plan`
    /// turns this into a `fix_plan`; when present it takes precedence over
    /// the legacy prose-scraping derivations.
    pub fix: Option<Fix>,
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
            fix: None,
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
            fix: None,
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

    /// Attach a structured fix produced by the language layer.
    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }

    /// Derive a structured fix_plan from the diagnostic's code, span, and
    /// source text.  Must be called *after* `with_source()` has populated
    /// `self.source`.  Idempotent: a pre-existing `fix_plan` is not replaced.
    ///
    /// Covered codes (MVP):
    /// * `ILO-T004` — undefined variable; hint `"did you mean 'X'?"` → rename span
    /// * `ILO-T003` — undefined type; same hint pattern
    /// * `ILO-T005` — undefined function; same hint pattern
    /// * `ILO-T032` — bare `fmt`/`fmt2` discarded → prefix `prnt ` before the call
    /// * `ILO-L002` — underscore identifier; suggestion contains the hyphenated form
    /// * `ILO-T039` — zero-arg fn used as value → append `()` to make it a call
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

        let plan = if let Some(fix) = self.fix.clone() {
            // Structured fix from the language layer — the checker knew the
            // intended form at the raise site, so no prose has to be re-parsed.
            derive_structured_fix(&fix, &self, source)
        } else {
            match code {
                "ILO-T004" | "ILO-T003" | "ILO-T005" => derive_typo_rename(&self, source),
                "ILO-T032" => derive_fmt_prefix(&self, source),
                "ILO-L002" => derive_underscore_hyphen(&self, source),
                "ILO-T008" => derive_return_type_cast(&self, source),
                "ILO-P011" => derive_reserved_rename(&self, source),
                "ILO-T039" => derive_zero_arg_call_parens(&self, source),
                "ILO-T041" => derive_nil_coalesce_result(&self, source),
                "ILO-P009" => derive_let_in_rewrite(&self, source),
                _ => None,
            }
        };
        self.fix_plan = plan;
        self
    }
}

// ---- Fix-plan derivation helpers ----

use crate::ast::SourceMap;

/// P009: `let X = EXPR in REST` foreign syntax → binding chain
/// `X = EXPR; REST`. Fires when the offending token is a bare `let` whose
/// line carries the full `let ... in` construct — the biggest single source
/// of broken generator output.
fn derive_let_in_rewrite(d: &Diagnostic, source: &str) -> Option<FixPlan> {
    if d.message != "expected expression, got `let`" {
        return None;
    }
    let span = d.labels.iter().find(|l| l.is_primary).map(|l| l.span)?;
    if span.start >= source.len() || span.start >= span.end {
        return None;
    }
    let sm = SourceMap::new(source);
    let (line_start, _) = sm.lookup(span.start);
    let lines: Vec<&str> = source.split('\n').collect();
    let lo = line_start.saturating_sub(1);
    if lo >= lines.len() {
        return None;
    }
    let line = lines[lo];
    let rewritten = rewrite_let_in_line(line)?;
    if rewritten == line {
        return None;
    }
    Some(FixPlan {
        path: d.path.clone(),
        edits: vec![FixEdit {
            line_start,
            line_end: line_start,
            before: line.to_string(),
            after: rewritten,
        }],
    })
}

/// Rewrite a `let X = EXPR in REST` line into `X = EXPR; REST`.
///
/// The `REST` half is often an infix arithmetic expression borrowed from
/// another language (`s / len xs`). ilo is prefix-only, so a bare infix
/// operator would still not parse. Convert the simple `A op B` shape to
/// ilo's prefix form (`/A (B)`) so the rewritten line actually compiles.
fn rewrite_let_in_line(line: &str) -> Option<String> {
    let m = regex::Regex::new(
        r"^(\s*)let\s+([A-Za-z][A-Za-z0-9_-]*)\s*=\s*(.*?)\s+in\s+(.+)$",
    )
    .ok()?;
    let caps = m.captures(line)?;
    let indent = caps.get(1)?.as_str();
    let var = caps.get(2)?.as_str();
    let expr = caps.get(3)?.as_str();
    let rest = caps.get(4)?.as_str();
    let rest = infix_to_prefix(rest);
    Some(format!("{indent}{var}={expr};{rest}"))
}

/// Convert a simple infix arithmetic expression to ilo's prefix form.
///
/// Handles `A op B` where `A` and `B` are single atoms (identifiers,
/// numbers, or parenthesised groups): `s / len xs` -> `/s (len xs)`.
///
/// A compound left-hand side (`a + b / c`) is left alone — the caller
/// re-validates and falls back rather than guessing at precedence.
fn infix_to_prefix(expr: &str) -> String {
    // Only handle the single-operator case, and only when the operator is
    // surrounded by whitespace (so `a-b` as a name is not misread).
    //
    // The right-hand side may be a call (`len xs`) or a parenthesised group
    // (`(flt {...} xs)`), so the atom pattern allows a name followed by
    // space-separated arguments.
    let re = regex::Regex::new(
        r"^(?P<lhs>[A-Za-z0-9_().]+)\s+(?P<op>[-+*/])\s+(?P<rhs>[A-Za-z0-9_().]+(?:\s+[A-Za-z0-9_().{}]+)*)$",
    )
    .ok();
    let Some(re) = re else { return expr.to_string() };
    let Some(caps) = re.captures(expr.trim()) else {
        return expr.to_string();
    };
    let lhs = caps.name("lhs").map(|m| m.as_str()).unwrap_or("");
    let op = caps.name("op").map(|m| m.as_str()).unwrap_or("");
    let rhs = caps.name("rhs").map(|m| m.as_str()).unwrap_or("");
    if lhs.is_empty() || op.is_empty() || rhs.is_empty() {
        return expr.to_string();
    }
    // The right side is a grouped sub-expression: parenthesise it so the
    // prefix operator consumes exactly two operands.
    format!("{op}{lhs} ({rhs})")
}

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

/// Choose the replacement name for a reserved-name diagnostic.
///
/// Preference order:
///  1. the first name proposed in the hint that is not itself reserved
///     (the hint is written by the parser, which knows the reserved set);
///  2. `<name>2`, unless that is also reserved;
///  3. `<name>v`, unless that is also reserved;
///  4. `<name>_`, which cannot collide with a keyword or builtin.
fn reserved_rename_target(name: &str, hint: Option<&str>) -> Option<String> {
    if let Some(hint) = hint {
        // Candidates are backticked in the hint: "rename the binding to
        // e.g. `v`, `value`, or `varv`".
        let mut parts = hint.split('`').skip(1);
        while let Some(candidate) = parts.next() {
            let _ = parts.next(); // skip the interleaved prose
            if candidate.is_empty() || candidate == name {
                continue;
            }
            if is_reserved_name(candidate) {
                continue;
            }
            if candidate
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                return Some(candidate.to_string());
            }
        }
    }
    for suffix in ["2", "v", "_"] {
        let candidate = format!("{name}{suffix}");
        if !is_reserved_name(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Is this name reserved (a builtin, a builtin alias, or a keyword)?
fn is_reserved_name(name: &str) -> bool {
    if crate::builtins::Builtin::is_builtin(name) {
        return true;
    }
    if crate::ast::all_builtin_aliases().any(|(alias, _)| alias == name) {
        return true;
    }
    matches!(
        name,
        "type"
            | "tool"
            | "use"
            | "with"
            | "timeout"
            | "retry"
            | "if"
            | "return"
            | "let"
            | "fn"
            | "def"
            | "var"
            | "const"
            | "true"
            | "false"
            | "nil"
            | "by"
            | "wh"
            | "ret"
            | "brk"
            | "cnt"
            | "defer"
            | "errdefer"
    )
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
/// Only the verifier's number-to-text conversion is mechanical. Confirm
/// that the primary span is an entire expression tail in the parsed AST:
/// T008 can also label a binding, loop, guard, or other statement.
/// `num` returns `R n t`, not `n`; choosing an unwrap or fallback would
/// invent error-handling semantics, so neither it nor generic hints qualify.
fn derive_return_type_cast(d: &Diagnostic, source: &str) -> Option<FixPlan> {
    let hint = d.suggestion.as_deref()?;
    let span = d.labels.iter().find(|l| l.is_primary).map(|l| l.span)?;
    if span.start >= source.len() || span.end > source.len() || span.start >= span.end {
        return None;
    }
    if hint != "use 'str' to convert: str <expr>" {
        return None;
    }
    let before = source.get(span.start..span.end)?.to_string();

    // Reparse without resolving imports: only a local function's expression
    // tail whose span exactly matches is eligible. Do not guess from text
    // such as `=` (which also appears in comparisons and string literals),
    // or from parser recovery on malformed source.
    let tokens = crate::lexer::lex(source).ok()?;
    let tokens = tokens
        .into_iter()
        .map(|(token, range)| (token, Span { start: range.start, end: range.end }))
        .collect();
    let (program, errors) = crate::parser::parse(tokens);
    if !errors.is_empty() {
        return None;
    }
    let expression_tail = program.declarations.iter().any(|decl| {
        matches!(decl, crate::ast::Decl::Function { return_type: crate::ast::Type::Text, body, .. }
            if body.last().is_some_and(|stmt|
                stmt.span == span && matches!(stmt.node, crate::ast::Stmt::Expr(_))))
    });
    if !expression_tail {
        return None;
    }
    let after = format!("str {before}");

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

fn ternary_bool_literals(call: &str) -> Option<String> {
    let trimmed = call.trim();
    let rest = trimmed.strip_prefix("?")?;
    let body = rest.trim_start();
    if body.is_empty() {
        return None;
    }
    // Only the two-branch numeric-literal ternary is handled. Parse the
    // condition up to the first top-level branch separator, then require
    // both branches to be the numeric literals 1 / 0.
    let mut depth = 0isize;
    let mut quoted = false;
    let mut escaped = false;
    let mut split = None;
    for (i, ch) in body.char_indices() {
        if quoted {
            if escaped { escaped = false; }
            else if ch == '\\' { escaped = true; }
            else if ch == '"' { quoted = false; }
            continue;
        }
        match ch {
            '"' => quoted = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if depth == 0 && ch.is_whitespace() && split.is_none() => {
                // first whitespace at depth 0 separates cond from branches
                split = Some(i);
                break;
            }
            _ => {}
        }
    }
    let split = split?;
    let cond = &body[..split];
    let branches = body[split..].split_whitespace().collect::<Vec<_>>();
    if branches.len() != 2 {
        return None;
    }
    let (a, b) = (branches[0], branches[1]);
    if !matches!((a, b), ("1", "0") | ("0", "1")) {
        return None;
    }
    let (ta, tb) = if a == "1" { ("true", "false") } else { ("false", "true") };
    Some(format!("?{cond} {ta} {tb}"))
}

fn range_add_zero_start(call: &str) -> Option<String> {
    let trimmed = call.trim();
    let mut matches = trimmed.match_indices("range").filter(|(index, _)| {
        let end = *index + 5;
        let before = trimmed[..*index].chars().next_back();
        let after = trimmed[end..].chars().next();
        before.is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '-')
            && after.is_some_and(char::is_whitespace)
    });
    let (start, _) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let prefix = &trimmed[..start];
    let rest = &trimmed[start + 5..];
    let operand = rest.trim_start();
    if operand.is_empty() {
        return None;
    }
    Some(format!("{prefix}range 0 {operand}"))
}

/// Rewrite a single-argument Python-style `not expr` call to ilo `!expr`.
/// Only atomic or fully parenthesized operands are accepted; this avoids
/// guessing the boundary of a multi-token prefix expression.
fn not_call_to_bang(call: &str) -> Option<String> {
    let trimmed = call.trim();
    let rest = trimmed.strip_prefix("not")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let operand = rest.trim_start();
    if operand.is_empty() {
        return None;
    }
    if operand.starts_with('(') {
        let mut depth = 0usize;
        let mut quoted = false;
        let mut escaped = false;
        let mut end = None;
        for (i, ch) in operand.char_indices() {
            if quoted {
                if escaped { escaped = false; }
                else if ch == '\\' { escaped = true; }
                else if ch == '"' { quoted = false; }
                continue;
            }
            match ch {
                '"' => quoted = true,
                '(' => depth += 1,
                ')' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 { end = Some(i + ch.len_utf8()); break; }
                }
                _ => {}
            }
        }
        if end? != operand.len() { return None; }
    } else if operand.split_whitespace().count() != 1 {
        return None;
    }
    Some(format!("!{operand}"))
}

/// Turn a structured `Fix` into a concrete byte edit over `source`.
///
/// The edit's `before` is always the full primary-span text — that is the
/// contract the repair loop enforces (`mid != edit.before` aborts) — so a
/// span that covers a wider expression still produces a single valid edit
/// with only the target token changed.
/// Swap verifier-proven reversed arguments in an `at` call.
///
/// The call may be embedded in a wider primary span, but only when that span
/// contains a unique `at` token. Grouped operands and known binary prefix
/// operators are parsed; ambiguous expression boundaries are declined.
pub(crate) fn swap_at_args(call: &str) -> Option<String> {
    fn take_expr(input: &str) -> Option<(&str, &str)> {
        let input = input.trim_start();
        if input.is_empty() { return None; }
        let bytes = input.as_bytes();
        let end = if matches!(bytes[0], b'(' | b'[' | b'{') {
            let mut depth = 0usize;
            let mut quoted = false;
            let mut escaped = false;
            let mut found = None;
            for (i, b) in bytes.iter().copied().enumerate() {
                if quoted {
                    if escaped { escaped = false; }
                    else if b == b'\\' { escaped = true; }
                    else if b == b'"' { quoted = false; }
                    continue;
                }
                match b {
                    b'"' => quoted = true,
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => {
                        depth = depth.checked_sub(1)?;
                        if depth == 0 { found = Some(i + 1); break; }
                    }
                    _ => {}
                }
            }
            found?
        } else if bytes[0] == b'"' {
            let mut escaped = false;
            let mut found = None;
            for (i, b) in bytes.iter().copied().enumerate().skip(1) {
                if escaped { escaped = false; }
                else if b == b'\\' { escaped = true; }
                else if b == b'"' { found = Some(i + 1); break; }
            }
            found?
        } else {
            let atom_end = input.find(char::is_whitespace).unwrap_or(input.len());
            let atom = &input[..atom_end];
            if matches!(atom, "+" | "-" | "*" | "/" | "%" | "mod") {
                let (_, after_left) = take_expr(&input[atom_end..])?;
                let (_, after_right) = take_expr(after_left)?;
                input.len() - after_right.len()
            } else {
                atom_end
            }
        };
        Some((&input[..end], &input[end..]))
    }

    let trimmed = call.trim();
    let mut matches = trimmed.match_indices("at").filter(|(index, _)| {
        let end = *index + 2;
        let before = trimmed[..*index].chars().next_back();
        let after = trimmed[end..].chars().next();
        before.is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '-')
            && after.is_some_and(char::is_whitespace)
    });
    let (start, _) = matches.next()?;
    if matches.next().is_some() { return None; }
    let prefix = &trimmed[..start];
    let rest = &trimmed[start + 2..];
    let (first, rest) = take_expr(rest)?;
    let (second, rest) = take_expr(rest)?;
    let tail = rest.trim_start();
    if !tail.is_empty()
        && !tail.chars().next().is_some_and(|c| matches!(c, ')' | ']' | '}' | ';' | ','))
    {
        return None;
    }
    Some(format!("{prefix}at {} {}{rest}", second.trim(), first.trim()))
}

fn derive_structured_fix(fix: &Fix, d: &Diagnostic, source: &str) -> Option<FixPlan> {
    let span = d.labels.iter().find(|l| l.is_primary).map(|l| l.span)?;
    if span.start >= source.len() || span.end > source.len() || span.start >= span.end {
        return None;
    }
    let before = source.get(span.start..span.end)?.to_string();
    if before.is_empty() {
        return None;
    }

    let after = match fix {
        Fix::Rename { from, to } => {
            let offset = locate_token(&before, from)?;
            let mut after = before.clone();
            after.replace_range(offset..offset + from.len(), to);
            after
        }
        Fix::CallZeroArg { name } => {
            let offset = locate_token(&before, name)?;
            let mut after = before.clone();
            after.replace_range(offset..offset + name.len(), &format!("{name}()"));
            after
        }
        Fix::PrefixPrnt => format!("prnt {before}"),
        Fix::SwapAtArgs => swap_at_args(&before)?,
        Fix::NotCallToBang => not_call_to_bang(&before)?,
        Fix::RangeAddZeroStart => range_add_zero_start(&before)?,
        Fix::TernaryBoolLiterals => ternary_bool_literals(&before)?,
        Fix::WrapInBraces => format!("{{ {before} }}"),
        Fix::RemoveLet => before.clone(), // handled at line scope in derive_let_in_rewrite
    };

    if after == before {
        return None; // no-op edit — not worth applying
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

/// Find `token` inside `haystack` as a whole identifier token.
///
/// Returns the byte offset of the *last* occurrence: in a span like
/// `field:name` the name is the trailing token, and for a bare `name`
/// reference there is only one occurrence anyway. A token that is only a
/// suffix of a longer identifier (`body-chunks` inside `my-body-chunks`) is
/// rejected.
fn locate_token(haystack: &str, token: &str) -> Option<usize> {
    if token.is_empty() {
        return None;
    }
    let mut offset = None;
    let mut from = 0usize;
    while let Some(rel) = haystack[from..].find(token) {
        let start = from + rel;
        let end = start + token.len();
        let before_char = haystack[..start].chars().last();
        let after_char = haystack[end..].chars().next();
        let left_ok = before_char.is_none_or(|c| !c.is_alphanumeric() && c != '-');
        let right_ok = after_char.is_none_or(|c| !c.is_alphanumeric() && c != '-');
        if left_ok && right_ok {
            offset = Some(start);
        }
        from = start + 1;
        if from >= haystack.len() {
            break;
        }
    }
    offset
}

/// T039: zero-arg function used as a value reference.
///
/// The suggestion reads `call it with 'X()' to get the return value; ...`.
/// Extract the quoted `X()` form, then rewrite the *name* inside the primary
/// span to `X()` — a pure mechanical edit that turns the fn-reference into a
/// call. The primary span can cover a wider expression (e.g. the whole
/// `field:name` record-construction tail), so the name is located inside the
/// span text rather than assumed to be the whole span.
///
/// The edit's `before` must equal the primary span verbatim — that is the
/// contract the repair loop enforces — so the whole span is rewritten with
/// just the name token changed.
fn derive_zero_arg_call_parens(d: &Diagnostic, source: &str) -> Option<FixPlan> {
    let hint = d.suggestion.as_deref()?;
    // Pattern: call it with 'X()' to get ...
    let rest = hint.strip_prefix("call it with '")?;
    let name = rest.split("()'").next()?;
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return None;
    }

    let span = d.labels.iter().find(|l| l.is_primary).map(|l| l.span)?;
    if span.start >= source.len() || span.end > source.len() || span.start >= span.end {
        return None;
    }
    let before = source.get(span.start..span.end)?.to_string();
    if before.is_empty() {
        return None;
    }

    // Locate the bare name as a whole token inside the span. Prefer the last
    // occurrence: in `field:name` the name is the trailing token, and in a
    // plain `name` reference there is only one occurrence anyway.
    let byte_off = before.rfind(name)?;
    // Guard against matching a suffix of a longer identifier (`body-chunks`
    // must not match inside `my-body-chunks`).
    let before_char = before[..byte_off].chars().last();
    let after_char = before[byte_off + name.len()..].chars().next();
    if before_char.is_some_and(|c| c.is_alphanumeric() || c == '-') {
        return None;
    }
    if after_char.is_some_and(|c| c.is_alphanumeric() || c == '-') {
        return None;
    }

    let mut after = before.clone();
    after.replace_range(byte_off..byte_off + name.len(), &format!("{name}()"));

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
/// The hint already proposes concrete replacement names ("rename the binding
/// to e.g. `v`, `value`, or `varv`"), so use the first of those that is not
/// itself reserved. The old `<name>2` fallback was actively harmful for
/// names like `fmt`, whose `<name>2` form (`fmt2`) is a real builtin — the
/// fix would trade one reserved-name error for another.
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
    let after = reserved_rename_target(&before, d.suggestion.as_deref())?;

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
///
/// The span may also cover a *binding* (`v = x ?? 0`), in which case the
/// binding has to stay outside the rewritten match — `?v = x{…}` would be
/// a parse error, not a fix.
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

    // A leading `<name> = ` is a binding, not part of the expression. Keep
    // it outside the match arm we generate.
    let (binding, val_part) = match val_part.split_once('=') {
        Some((lhs, rhs)) if !lhs.trim().is_empty() && !rhs.trim().is_empty() => {
            (format!("{}=", lhs.trim()), rhs.trim())
        }
        _ => (String::new(), val_part),
    };

    let after = format!("{binding}?{val_part}{{~v:v;^_:{default_part}}}");

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
        // The lexer knows the exact hyphenated identifier it wants; hand it
        // over as a structured fix instead of leaving the fix layer to
        // re-parse the suggestion prose.
        if e.code == "ILO-L002" {
            if let Some(hyphenated) = hyphenated_form(&e.suggestion) {
                let from = e.snippet.clone();
                let to = hyphenated.to_string();
                if from != to {
                    d = d.with_fix(Fix::Rename { from, to });
                }
            }
        }
        d
    }
}

/// Extract the corrected identifier from an L002 suggestion.
///
/// The lexer formats it as ``use hyphens (e.g. `max-len`)`` — the backticked
/// form is the identifier it wants.
fn hyphenated_form(suggestion: &str) -> Option<&str> {
    let rest = suggestion.rsplit('`').nth(1)?;
    if rest.is_empty() || rest.contains(char::is_whitespace) {
        return None;
    }
    Some(rest)
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
        // Attach the structured fix the language layer derived for this
        // error. `derive_fix_plan` turns it into a concrete byte edit; it
        // takes precedence over the legacy prose-scraping path.
        if let Some(fix) = Fix::for_verify_error(e) {
            d = d.with_fix(fix);
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
    fn derive_fix_plan_t005_undefined_fn_rename() {
        // ILO-T005: undefined function with "did you mean 'X'?" hint.
        // `uniq` -> `unq` is a real case from the saas-platform example.
        let source = "f xs:L t>L t;uniq xs";
        let d = Diagnostic::error("undefined function 'uniq' (called with 1 args)")
            .with_code("ILO-T005")
            .with_span(Span { start: 13, end: 17 }, "")
            .with_suggestion("did you mean 'unq'?")
            .with_source(source.to_string())
            .derive_fix_plan();
        let plan = d.fix_plan.expect("fix_plan should be derived for T005");
        assert_eq!(plan.edits.len(), 1);
        assert_eq!(plan.edits[0].before, "uniq");
        assert_eq!(plan.edits[0].after, "unq");
    }

    #[test]
    fn derive_fix_plan_t005_no_plan_without_typo_hint() {
        // The long call-vs-binop hint must NOT produce a rename plan.
        let source = "f x:n>n;-get-user uid";
        let d = Diagnostic::error("undefined function 'get-user'")
            .with_code("ILO-T005")
            .with_span(Span { start: 8, end: 16 }, "")
            .with_suggestion("this parsed as a call `get-user uid`")
            .with_source(source.to_string())
            .derive_fix_plan();
        assert!(d.fix_plan.is_none(), "non-typo T005 hint must not yield a plan");
    }

    #[test]
    fn derive_fix_plan_t039_zero_arg_call_parens() {
        // ILO-T039: 0-arg fn used as a value -> rewrite the name to `name()`.
        // The primary span covers the whole `body:name` tail, so the edit must
        // reproduce the span verbatim and change only the name token.
        let source = "h req:_>rsp;rsp status:200 body:body-chunks";
        let span_text = "body:body-chunks";
        let start = source.find(span_text).expect("span text present");
        let d = Diagnostic::error("'body-chunks' is a 0-arg function used as a value reference")
            .with_code("ILO-T039")
            .with_span(
                Span {
                    start,
                    end: start + span_text.len(),
                },
                "",
            )
            .with_suggestion(
                "call it with 'body-chunks()' to get the return value; bare 'body-chunks' in value position is a function reference, not a call result",
            )
            .with_source(source.to_string())
            .derive_fix_plan();
        let plan = d.fix_plan.expect("fix_plan should be derived for T039");
        assert_eq!(plan.edits.len(), 1);
        assert_eq!(plan.edits[0].before, "body:body-chunks");
        assert_eq!(plan.edits[0].after, "body:body-chunks()");
    }

    #[test]
    fn derive_fix_plan_t039_rejects_partial_name_match() {
        // The name must match as a whole token: `body-chunks` must not be
        // matched inside a longer identifier.
        let source = "h req:_>rsp;rsp status:200 body:my-body-chunks";
        let span_text = "body:my-body-chunks";
        let start = source.find(span_text).expect("span text present");
        let d = Diagnostic::error("'body-chunks' is a 0-arg function used as a value reference")
            .with_code("ILO-T039")
            .with_span(
                Span {
                    start,
                    end: start + span_text.len(),
                },
                "",
            )
            .with_suggestion(
                "call it with 'body-chunks()' to get the return value; bare 'body-chunks' in value position is a function reference, not a call result",
            )
            .with_source(source.to_string())
            .derive_fix_plan();
        assert!(
            d.fix_plan.is_none(),
            "partial identifier match must not yield a plan"
        );
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
    fn derive_fix_plan_t008_num_cast_is_not_safe() {
        // `num` returns R n t, so a bare cast cannot satisfy an n return.
        let source = "f x:t>n;x";
        let d = Diagnostic::error("return type mismatch: expected n, got t")
            .with_code("ILO-T008")
            .with_span(Span { start: 8, end: 9 }, "")
            .with_suggestion("use 'num' to parse text (returns R n t)")
            .with_source(source.to_string())
            .derive_fix_plan();
        assert!(d.fix_plan.is_none(), "num requires an error-handling decision");
    }

    #[test]
    fn derive_fix_plan_t008_requires_an_ast_expression_tail() {
        // Even a conversion hint cannot authorize prefixing a statement.
        for source in [
            "f>t;v=5",
            "f>t;ret 5",
            "f>t;@i 0..2{prnt i}",
            "f>t;wh false{prnt 5}",
            "f>t;true{5}{6}",
            "f>t;?true{true:5;false:6}",
            "f>t;defer prnt 5",
        ] {
            let start = source.find(';').unwrap() + 1;
            let d = Diagnostic::error("return type mismatch: expected t, got n")
                .with_code("ILO-T008")
                .with_span(Span { start, end: source.len() }, "")
                .with_suggestion("use 'str' to convert: str <expr>")
                .with_source(source)
                .derive_fix_plan();
            assert!(d.fix_plan.is_none(), "statement must not be cast: {source}");
        }
    }

    #[test]
    fn derive_fix_plan_t008_rejects_non_tail_and_malformed_source() {
        // NB: `f>t;5;` (trailing `;`) was removed from this list: the parser
        // now accepts a trailing semicolon, so the source is no longer
        // malformed and the expression-tail cast is legitimately derived.
        for (source, start, end) in [
            ("f>t;5;6", 4, 5),
            ("f>t;5\ng>n;var=1", 4, 5),
            ("f>t;5", 1, 5),
        ] {
            let d = Diagnostic::error("return type mismatch: expected t, got n")
                .with_code("ILO-T008")
                .with_span(Span { start, end }, "")
                .with_suggestion("use 'str' to convert: str <expr>")
                .with_source(source)
                .derive_fix_plan();
            assert!(d.fix_plan.is_none(), "{source}: {start}..{end}");
        }
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
        assert_eq!(plan.edits[0].after, "v");
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
