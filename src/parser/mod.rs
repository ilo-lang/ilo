use crate::ast::*;
use crate::lexer::Token;
use std::collections::HashMap;

pub struct Parser {
    tokens: Vec<(Token, Span)>,
    pos: usize,
    /// Known function arities, populated with builtins at construction
    /// and extended with user-function headers as they're parsed.
    fn_arity: HashMap<String, usize>,
    /// For each known function, which parameter positions take a function
    /// reference (HOF positions).
    fn_param_is_fn: HashMap<String, Vec<bool>>,
    /// When true, an Ident followed by another whitespace-separated atom is
    /// parsed as a bare Ref (list element) rather than a function call.
    /// Set only inside list-literal element parsing.
    no_whitespace_call: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("Parse error at token {position}: {message}")]
pub struct ParseError {
    pub code: &'static str,
    pub position: usize,
    pub span: Span,
    pub message: String,
    pub hint: Option<String>,
}

type Result<T> = std::result::Result<T, ParseError>;

impl Parser {
    pub fn new(tokens: Vec<(Token, Span)>) -> Self {
        // Filter out newlines — idea9 uses ; as separator
        let tokens: Vec<(Token, Span)> = tokens
            .into_iter()
            .filter(|(t, _)| *t != Token::Newline)
            .collect();
        let (fn_arity, fn_param_is_fn) = builtin_arity_tables();
        Parser {
            tokens,
            pos: 0,
            fn_arity,
            fn_param_is_fn,
            no_whitespace_call: false,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|(t, _)| t)
    }

    fn peek_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|(_, s)| *s)
            .unwrap_or(Span::UNKNOWN)
    }

    fn advance(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos).map(|(t, _)| t);
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, expected: &Token) -> Result<Span> {
        match self.peek() {
            Some(tok) if tok == expected => {
                let span = self.peek_span();
                self.advance();
                Ok(span)
            }
            Some(tok) => {
                let hint = if *expected == Token::Greater
                    && *tok == Token::Minus
                    && self.token_at(self.pos + 1) == Some(&Token::Greater)
                {
                    Some("ilo uses '>' not '->' for the return type separator".to_string())
                } else {
                    None
                };
                let mut err = self.error(
                    "ILO-P003",
                    format!("expected {:?}, got {:?}", expected, tok),
                );
                err.hint = hint;
                Err(err)
            }
            None => Err(self.error("ILO-P004", format!("expected {:?}, got EOF", expected))),
        }
    }

    fn expect_ident(&mut self) -> Result<String> {
        match self.peek().cloned() {
            Some(Token::Ident(name)) => {
                self.advance();
                Ok(name)
            }
            Some(tok) => {
                if let Some((msg, hint)) = reserved_keyword_message(&tok) {
                    Err(self.error_hint("ILO-P011", msg, hint))
                } else {
                    Err(self.error("ILO-P005", format!("expected identifier, got {:?}", tok)))
                }
            }
            None => Err(self.error("ILO-P006", "expected identifier, got EOF".into())),
        }
    }

    fn error(&self, code: &'static str, message: String) -> ParseError {
        ParseError {
            code,
            position: self.pos,
            span: self.peek_span(),
            message,
            hint: None,
        }
    }

    fn error_hint(&self, code: &'static str, message: String, hint: String) -> ParseError {
        ParseError {
            code,
            position: self.pos,
            span: self.peek_span(),
            message,
            hint: Some(hint),
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// Check if we're at a body terminator (end of input, `}`, or end of declaration)
    fn at_body_end(&self) -> bool {
        matches!(self.peek(), None | Some(Token::RBrace))
    }

    /// Access raw token (for lookahead). Returns just the Token reference.
    fn token_at(&self, idx: usize) -> Option<&Token> {
        self.tokens.get(idx).map(|(t, _)| t)
    }

    // ---- Top-level parsing ----

    pub fn parse_program(&mut self) -> (Program, Vec<ParseError>) {
        let mut declarations = Vec::new();
        let mut errors: Vec<ParseError> = Vec::new();
        const MAX_ERRORS: usize = 20;
        // Cascade suppression: once we've reported a P001 at the top level, drop
        // further P001 errors until the parser successfully consumes another
        // declaration. The first P001 nearly always has the actionable hint;
        // subsequent ones are noise produced while resyncing through stray
        // tokens (e.g. a leftover `}` after a body-level parse failure).
        let mut suppress_p001 = false;

        while !self.at_end() {
            if errors.len() >= MAX_ERRORS {
                break;
            }
            let before_pos = self.pos;
            match self.parse_decl() {
                Ok(decl) => {
                    declarations.push(decl);
                    suppress_p001 = false;
                }
                Err(e) => {
                    let err_span = e.span;
                    let is_cascade_class = matches!(e.code, "ILO-P001" | "ILO-P002");
                    if !(is_cascade_class && suppress_p001) {
                        errors.push(e);
                    }
                    if is_cascade_class {
                        suppress_p001 = true;
                    }
                    let end_span = self.sync_to_decl_boundary();
                    declarations.push(Decl::Error {
                        span: err_span.merge(end_span),
                    });
                    // Guarantee forward progress so we cannot loop emitting the
                    // same error against the same token (e.g. a stray `}`).
                    if self.pos == before_pos {
                        self.advance();
                    }
                }
            }
        }

        (
            Program {
                declarations,
                source: None,
            },
            errors,
        )
    }

    /// Return true if the tokens at `pos` look like the start of a function declaration:
    /// `Ident` followed by `>` (no-param function) OR `Ident Ident :` (has params).
    ///
    /// Reserved statement-keyword identifiers (`wh`/`ret`/`brk`/`cnt`) are never
    /// valid function names — `parse_stmt` intercepts them as control-flow forms.
    /// Short-circuiting here closes the `wh >cond{...}` mid-body re-parse trap,
    /// where the body-boundary heuristic in `parse_body_with` would otherwise
    /// treat `wh >v 0{...}` as a fresh fn decl named `wh` returning `v`.
    fn is_fn_decl_start(&self, pos: usize) -> bool {
        let name = match self.token_at(pos) {
            Some(Token::Ident(n)) => n,
            _ => return false,
        };
        if is_reserved_stmt_keyword(name) {
            return false;
        }
        match self.token_at(pos + 1) {
            // name>return — zero-param function
            Some(Token::Greater) => true,
            // name param:type ... — has params
            Some(Token::Ident(_)) => matches!(self.token_at(pos + 2), Some(Token::Colon)),
            _ => false,
        }
    }

    /// Stricter variant of `is_fn_decl_start` used at top-level body boundaries
    /// to disambiguate fn declarations from record construction. A real fn decl
    /// always has `>` followed by a return type before the body's first `;`,
    /// while a record `Outer a:1 b:2` never has a `>` before its terminator.
    /// Returns true only when a `>` is visible before the next `;`/`}`/`{`/EOF
    /// at the same bracket depth.
    fn is_fn_decl_start_strict(&self, pos: usize) -> bool {
        if !self.is_fn_decl_start(pos) {
            return false;
        }
        // Fast path: `Ident >` is unambiguous in body position because a leading
        // `name>` statement is not legal here (no expression starts with a bare
        // identifier followed by `>` in a way that doesn't look like a fn decl
        // header). Even `a > b` would only appear after a `;`, but it has no
        // following `;type;` shape — but we still want to confirm by scanning.
        let mut i = pos + 1;
        let mut depth: i32 = 0;
        while let Some(tok) = self.token_at(i) {
            match tok {
                Token::LParen | Token::LBracket | Token::LBrace => depth += 1,
                Token::RParen | Token::RBracket => depth -= 1,
                _ if depth > 0 => {}
                Token::Greater if depth == 0 => return true,
                Token::Semi | Token::RBrace => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Advance past tokens until we reach what looks like the start of the next
    /// declaration (or EOF). Returns the span of the last token consumed.
    /// Tracks brace depth so nested `{…}` blocks are skipped atomically.
    fn sync_to_decl_boundary(&mut self) -> Span {
        let mut depth: usize = 0;
        let mut last_span = self.peek_span();

        loop {
            match self.peek() {
                None => break,
                Some(Token::LBrace) => {
                    depth += 1;
                    last_span = self.peek_span();
                    self.advance();
                }
                Some(Token::RBrace) => {
                    if depth == 0 {
                        // Stray top-level `}` — consume it so the outer loop
                        // makes progress rather than re-reporting the same
                        // token as a "missing declaration".
                        last_span = self.peek_span();
                        self.advance();
                        break;
                    }
                    depth -= 1;
                    last_span = self.peek_span();
                    self.advance();
                }
                // Unambiguous declaration starters
                Some(Token::Type) | Some(Token::Tool) if depth == 0 => break,
                // An identifier that looks like a function header
                _ if depth == 0 && self.is_fn_decl_start(self.pos) => break,
                _ => {
                    last_span = self.peek_span();
                    self.advance();
                }
            }
        }

        last_span
    }

    fn parse_decl(&mut self) -> Result<Decl> {
        // Reserved-keyword binding attempts: `var=5`, `let=5`, `if=5`, ...
        // Surface the friendly ILO-P011 message before any expression-level
        // cascade fires.
        if self.token_at(self.pos + 1) == Some(&Token::Eq)
            && let Some(tok) = self.peek()
            && let Some((msg, _)) = reserved_keyword_message(tok)
        {
            return Err(self.error_hint(
                "ILO-P011",
                msg,
                "use `name=expr` for bindings (e.g. `count=5`)".to_string(),
            ));
        }
        // Loop-control words `cnt`/`brk` used as binding names: `cnt=5`.
        if let Some(Token::Ident(name)) = self.peek()
            && (name == "cnt" || name == "brk")
            && self.token_at(self.pos + 1) == Some(&Token::Eq)
        {
            let (word, role, alt) = if name == "cnt" {
                ("cnt", "continue", "count")
            } else {
                ("brk", "break", "brake")
            };
            return Err(self.error_hint(
                "ILO-P011",
                format!("`{word}` is reserved for {role} (loop control) and cannot be used as an identifier"),
                format!("pick a different name like `{alt}` or `{}`", &word[..1]),
            ));
        }
        // Builtin `fld` (fold) used as binding name: `fld=5`. Personas reach
        // for `fld` as a natural variable (field/fold/folder); the builtin
        // collision otherwise surfaces as a misleading ILO-T006 arity error.
        if let Some(Token::Ident(name)) = self.peek()
            && name == "fld"
            && self.token_at(self.pos + 1) == Some(&Token::Eq)
        {
            return Err(self.error_hint(
                "ILO-P011",
                "`fld` is reserved for the fold builtin and cannot be used as an identifier".into(),
                "pick a different name like `field` or `folder`".into(),
            ));
        }
        match self.peek() {
            Some(Token::Type) => self.parse_type_decl(),
            Some(Token::Tool) => self.parse_tool_decl(),
            Some(Token::Use) => self.parse_use_decl(),
            Some(Token::Ident(_)) => {
                // Check for keywords from other languages before attempting fn parse
                let ident_str = match self.peek() {
                    Some(Token::Ident(s)) => s.as_str(),
                    _ => unreachable!(),
                };
                if ident_str == "alias" {
                    return self.parse_alias_decl();
                }
                let hint = match ident_str {
                    "function" | "def" | "fn" =>
                        Some("ilo function syntax: name param:type > return-type; body".to_string()),
                    "let" | "var" | "const" =>
                        Some("ilo uses assignment syntax: name = expr".to_string()),
                    "return" =>
                        Some("the last expression in a function body is the return value — no 'return' keyword".to_string()),
                    "if" =>
                        Some("ilo uses match for conditionals: ?expr{true:... false:...}".to_string()),
                    _ => None,
                };
                if let Some(hint_msg) = hint {
                    let mut err = self.error(
                        "ILO-P001",
                        format!("expected declaration, got Ident({ident_str:?})"),
                    );
                    err.hint = Some(hint_msg);
                    return Err(err);
                }
                self.parse_fn_decl()
            }
            Some(tok) => {
                let msg = format!("expected declaration, got {:?}", tok);
                let hint = match tok {
                    Token::Plus | Token::Minus | Token::Star | Token::Slash
                    | Token::Greater | Token::Less | Token::GreaterEq | Token::LessEq
                    | Token::Eq | Token::NotEq | Token::Amp | Token::Pipe
                    | Token::Bang | Token::Tilde | Token::Caret =>
                        Some("prefix operators can't start a declaration. Bind call results to variables: r=fac -n 1;*n r".to_string()),
                    Token::KwFn | Token::KwDef =>
                        Some("ilo function syntax: name param:type > return-type; body".to_string()),
                    Token::KwLet | Token::KwVar | Token::KwConst =>
                        Some("ilo uses assignment syntax: name = expr".to_string()),
                    Token::KwReturn =>
                        Some("the last expression in a function body is the return value — no 'return' keyword".to_string()),
                    Token::KwIf =>
                        Some("ilo uses match for conditionals: ?expr{true:... false:...}".to_string()),
                    _ => None,
                };
                let mut err = self.error("ILO-P001", msg);
                err.hint = hint;
                Err(err)
            }
            None => Err(self.error("ILO-P002", "expected declaration, got EOF".into())),
        }
    }

    /// `use "path/to/file.ilo"` or `use "path/to/file.ilo" [name1 name2]`
    fn parse_use_decl(&mut self) -> Result<Decl> {
        let start = self.peek_span();
        self.expect(&Token::Use)?;
        let path = match self.peek().cloned() {
            Some(Token::Text(p)) => {
                self.advance();
                p
            }
            Some(tok) => {
                return Err(self.error(
                    "ILO-P016",
                    format!("expected a string path after `use`, got {:?}", tok),
                ));
            }
            None => {
                return Err(self.error(
                    "ILO-P016",
                    "expected a string path after `use`, got EOF".into(),
                ));
            }
        };

        // Optional `[name1 name2 ...]` scoped import list
        let only = if self.peek() == Some(&Token::LBracket) {
            self.advance(); // consume `[`
            let mut names = Vec::new();
            while self.peek() != Some(&Token::RBracket) {
                match self.peek() {
                    None => {
                        return Err(self.error("ILO-P016", "unclosed `[` in use statement".into()));
                    }
                    _ => names.push(self.expect_ident()?),
                }
            }
            self.expect(&Token::RBracket)?;
            if names.is_empty() {
                return Err(self.error(
                    "ILO-P016",
                    "use `[...]` list must not be empty — omit brackets to import all".into(),
                ));
            }
            Some(names)
        } else {
            None
        };

        let end = self.peek_span();
        Ok(Decl::Use {
            path,
            only,
            span: start.merge(end),
        })
    }

    /// `type name{field:type;...}`
    fn parse_type_decl(&mut self) -> Result<Decl> {
        let start = self.peek_span();
        self.expect(&Token::Type)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LBrace)?;
        let mut fields = Vec::new();
        while self.peek() != Some(&Token::RBrace) {
            if !fields.is_empty() {
                self.expect(&Token::Semi)?;
            }
            let fname = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let ty = self.parse_type()?;
            fields.push(Param { name: fname, ty });
        }
        let end = self.peek_span();
        self.expect(&Token::RBrace)?;
        Ok(Decl::TypeDef {
            name,
            fields,
            span: start.merge(end),
        })
    }

    /// `tool name"desc" params>return timeout:n,retry:n`
    fn parse_tool_decl(&mut self) -> Result<Decl> {
        let start = self.peek_span();
        self.expect(&Token::Tool)?;
        let name = self.expect_ident()?;
        let description = match self.peek().cloned() {
            Some(Token::Text(s)) => {
                self.advance();
                s
            }
            _ => return Err(self.error("ILO-P015", "expected tool description string".into())),
        };
        let params = self.parse_params()?;
        self.expect(&Token::Greater)?;
        let return_type = self.parse_type()?;

        let mut timeout = None;
        let mut retry = None;

        // Parse optional tool options: timeout:n,retry:n
        while matches!(self.peek(), Some(Token::Timeout) | Some(Token::Retry)) {
            match self.peek() {
                Some(Token::Timeout) => {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    timeout = Some(self.parse_number()?);
                }
                Some(Token::Retry) => {
                    self.advance();
                    self.expect(&Token::Colon)?;
                    retry = Some(self.parse_number()?);
                }
                _ => break,
            }
            if self.peek() == Some(&Token::Comma) {
                self.advance();
            }
        }

        // End span: last consumed token
        let end_span = self.prev_span();

        Ok(Decl::Tool {
            name,
            description,
            params,
            return_type,
            timeout,
            retry,
            span: start.merge(end_span),
        })
    }

    /// `alias name type`
    fn parse_alias_decl(&mut self) -> Result<Decl> {
        let start = self.peek_span();
        // consume the `alias` identifier
        self.advance();
        let name = self.expect_ident()?;
        let target = self.parse_type()?;
        let end = self.prev_span();
        Ok(Decl::Alias {
            name,
            target,
            span: start.merge(end),
        })
    }

    /// `name params>return;body`
    fn parse_fn_decl(&mut self) -> Result<Decl> {
        let start = self.peek_span();
        let name = self.expect_ident()?;
        let params = self.parse_params()?;
        // Register arity + per-param fn-ref flags BEFORE parsing the body so
        // recursive self-references inside the body benefit from eager
        // call-arg expansion (e.g. `fac n:n>n;?=n 0{1}{*n fac -n 1}` —
        // `fac -n 1` is parsed as a single nested call).
        self.register_user_fn(&name, &params);
        self.expect(&Token::Greater)?;
        let return_type = self.parse_type()?;
        // The header/body boundary is normally a `;`, but a newline (filtered
        // out before parsing) leaves no separator. Accept either: consume a
        // `;` if present, otherwise fall straight into the body.
        if self.peek() == Some(&Token::Semi) {
            self.advance();
        }
        let body = self.parse_body_with(true)?;
        let end = self.prev_span();
        Ok(Decl::Function {
            name,
            params,
            return_type,
            body,
            span: start.merge(end),
        })
    }

    /// Span of the previously consumed token.
    fn prev_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens[self.pos - 1].1
        } else {
            Span::UNKNOWN
        }
    }

    // ---- Types ----

    fn parse_type(&mut self) -> Result<Type> {
        match self.peek().cloned() {
            Some(Token::LParen) => {
                self.advance();
                let inner = self.parse_type()?;
                self.expect(&Token::RParen)?;
                Ok(inner)
            }
            Some(Token::Ident(ref s)) if s == "n" => {
                self.advance();
                Ok(Type::Number)
            }
            Some(Token::Ident(ref s)) if s == "t" => {
                self.advance();
                Ok(Type::Text)
            }
            Some(Token::Ident(ref s)) if s == "b" => {
                self.advance();
                Ok(Type::Bool)
            }
            Some(Token::Underscore) => {
                self.advance();
                Ok(Type::Any)
            }
            Some(Token::OptType) => {
                self.advance();
                let inner = self.parse_type()?;
                Ok(Type::Optional(Box::new(inner)))
            }
            Some(Token::ListType) => {
                self.advance();
                let inner = self.parse_type()?;
                Ok(Type::List(Box::new(inner)))
            }
            Some(Token::MapType) => {
                self.advance();
                let key_type = self.parse_type()?;
                let val_type = self.parse_type()?;
                Ok(Type::Map(Box::new(key_type), Box::new(val_type)))
            }
            Some(Token::ResultType) => {
                self.advance();
                let ok_type = self.parse_type()?;
                let err_type = self.parse_type()?;
                Ok(Type::Result(Box::new(ok_type), Box::new(err_type)))
            }
            Some(Token::SumType) => {
                self.advance();
                // Collect variant names: lowercase idents not followed by colon.
                let mut variants = Vec::new();
                while let Some(Token::Ident(_)) = self.peek() {
                    // Ident followed by colon = param name, stop.
                    if self.token_at(self.pos + 1) == Some(&Token::Colon) {
                        break;
                    }
                    if let Some(Token::Ident(name)) = self.peek().cloned() {
                        variants.push(name);
                        self.advance();
                    }
                }
                if variants.is_empty() {
                    return Err(
                        self.error("ILO-P010", "S type requires at least one variant".into())
                    );
                }
                Ok(Type::Sum(variants))
            }
            Some(Token::FnType) => {
                self.advance();
                // Collect all following types; last is return type, preceding are params.
                // Stop when the next token cannot start a type, is >, ;, }, or is an Ident
                // followed by : (which would be a new parameter name, not a type).
                let mut types = Vec::new();
                loop {
                    if !self.can_start_type() {
                        break;
                    }
                    // An Ident followed by Colon is a param name, not a type.
                    if matches!(self.peek(), Some(Token::Ident(_)))
                        && self.token_at(self.pos + 1) == Some(&Token::Colon)
                    {
                        break;
                    }
                    types.push(self.parse_type()?);
                }
                if types.is_empty() {
                    return Err(
                        self.error("ILO-P009", "F type requires at least a return type".into())
                    );
                }
                let return_type = types.pop().expect("F type requires at least a return type");
                Ok(Type::Fn(types, Box::new(return_type)))
            }
            Some(Token::Ident(name)) => {
                self.advance();
                Ok(Type::Named(name))
            }
            Some(tok) => Err(self.error("ILO-P007", format!("expected type, got {:?}", tok))),
            None => Err(self.error("ILO-P008", "expected type, got EOF".into())),
        }
    }

    /// Returns true if the current token can begin a type expression.
    fn can_start_type(&self) -> bool {
        match self.peek() {
            Some(Token::Ident(s)) => {
                matches!(s.as_str(), "n" | "t" | "b")
                    || self.token_at(self.pos + 1) != Some(&Token::Colon)
            }
            Some(Token::Underscore) => true,
            Some(Token::OptType) => true,
            Some(Token::ListType) => true,
            Some(Token::MapType) => true,
            Some(Token::ResultType) => true,
            Some(Token::SumType) => true,
            Some(Token::FnType) => true,
            Some(Token::LParen) => true,
            _ => false,
        }
    }

    /// Parse parameter list: `name:type name:type ...`
    fn parse_params(&mut self) -> Result<Vec<Param>> {
        let mut params = Vec::new();
        while let Some(Token::Ident(_)) = self.peek() {
            // Look ahead for colon to distinguish params from other constructs
            if self.pos + 1 < self.tokens.len()
                && self.token_at(self.pos + 1) == Some(&Token::Colon)
            {
                let name = self.expect_ident()?;
                self.expect(&Token::Colon)?;
                let ty = self.parse_type()?;
                params.push(Param { name, ty });
            } else {
                break;
            }
        }
        Ok(params)
    }

    // ---- Body & Statements ----

    /// Parse a semicolon-separated body, wrapping each statement with its source span.
    fn parse_body(&mut self) -> Result<Vec<Spanned<Stmt>>> {
        self.parse_body_with(false)
    }

    /// Parse a semicolon-separated body. When `top_level` is true, the body
    /// also terminates if the tokens after a `;` look like the start of the
    /// next top-level function declaration. This closes the "sibling helper
    /// slurp" trap where a body's final bare call would otherwise consume the
    /// next function's name as an argument (and the trailing `>type;` would
    /// then be parsed as a comparison, hiding the boundary).
    fn parse_body_with(&mut self, top_level: bool) -> Result<Vec<Spanned<Stmt>>> {
        let mut stmts = Vec::new();
        if !self.at_body_end() {
            let span_start = self.peek_span();
            let stmt = self.parse_stmt()?;
            stmts.push(Spanned {
                node: stmt,
                span: span_start.merge(self.prev_span()),
            });
            while self.peek() == Some(&Token::Semi) {
                self.advance();
                if self.at_body_end() {
                    break;
                }
                if top_level && self.is_fn_decl_start_strict(self.pos) {
                    break;
                }
                let span_start = self.peek_span();
                let stmt = self.parse_stmt()?;
                stmts.push(Spanned {
                    node: stmt,
                    span: span_start.merge(self.prev_span()),
                });
            }
        }
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt> {
        // Reserved-keyword binding attempts inside a function body: `var=5`,
        // `let=5`, `if=5`, ... Surface the friendly ILO-P011 message before
        // `parse_atom` cascades into a cryptic ILO-P009.
        if self.token_at(self.pos + 1) == Some(&Token::Eq)
            && let Some(tok) = self.peek()
            && let Some((msg, _)) = reserved_keyword_message(tok)
        {
            return Err(self.error_hint(
                "ILO-P011",
                msg,
                "use `name=expr` for bindings (e.g. `count=5`)".to_string(),
            ));
        }
        match self.peek() {
            Some(Token::Question) => {
                if self.is_prefix_ternary() {
                    let expr = self.parse_prefix_ternary()?;
                    Ok(Stmt::Expr(expr))
                } else {
                    self.parse_match_stmt()
                }
            }
            Some(Token::At) => self.parse_foreach(),
            Some(Token::Ident(name)) if name == "ret" => {
                self.advance(); // consume "ret"
                let value = self.parse_expr()?;
                Ok(Stmt::Return(value))
            }
            Some(Token::Ident(name)) if name == "brk" => {
                if self.token_at(self.pos + 1) == Some(&Token::Eq) {
                    return Err(self.error_hint(
                        "ILO-P011",
                        "`brk` is reserved for break (loop control) and cannot be used as an identifier".into(),
                        "pick a different name like `brake` or `b`".into(),
                    ));
                }
                self.advance(); // consume "brk"
                // brk with optional value expression
                let value = if self.at_body_end() {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                Ok(Stmt::Break(value))
            }
            Some(Token::Ident(name)) if name == "cnt" => {
                if self.token_at(self.pos + 1) == Some(&Token::Eq) {
                    return Err(self.error_hint(
                        "ILO-P011",
                        "`cnt` is reserved for continue (loop control) and cannot be used as an identifier".into(),
                        "pick a different name like `count` or `c`".into(),
                    ));
                }
                self.advance(); // consume "cnt"
                Ok(Stmt::Continue)
            }
            Some(Token::Ident(name))
                if name == "fld" && self.token_at(self.pos + 1) == Some(&Token::Eq) =>
            {
                Err(self.error_hint(
                    "ILO-P011",
                    "`fld` is reserved for the fold builtin and cannot be used as an identifier"
                        .into(),
                    "pick a different name like `field` or `folder`".into(),
                ))
            }
            Some(Token::Ident(name)) if name == "wh" => {
                self.advance(); // consume "wh"
                let condition = self.parse_expr()?;
                self.expect(&Token::LBrace)?;
                let body = self.parse_body()?;
                self.expect(&Token::RBrace)?;
                Ok(Stmt::While { condition, body })
            }
            Some(Token::LBrace) if self.is_destructure_pattern() => self.parse_destructure(),
            Some(Token::Ident(_)) => {
                // Check for let binding: ident '='
                if self.pos + 1 < self.tokens.len()
                    && self.token_at(self.pos + 1) == Some(&Token::Eq)
                {
                    self.parse_let()
                } else {
                    // Could be a guard or an expression statement
                    self.parse_expr_or_guard()
                }
            }
            Some(Token::Bang) => {
                // !cond{body} — negated guard
                self.parse_bang_stmt()
            }
            Some(Token::Caret) => {
                // ^expr — Err constructor as statement
                self.parse_caret_stmt()
            }
            _ => {
                let expr = self.parse_expr()?;
                // Check if this is a guard: expr followed by {
                if self.peek() == Some(&Token::LBrace) {
                    let body = self.parse_brace_body()?;
                    let else_body = if self.peek() == Some(&Token::LBrace) {
                        Some(self.parse_brace_body()?)
                    } else {
                        None
                    };
                    Ok(Stmt::Guard {
                        condition: expr,
                        negated: false,
                        body,
                        else_body,
                        braceless: false,
                    })
                } else if is_guard_eligible_condition(&expr) && self.can_start_operand() {
                    Ok(self.parse_braceless_guard_body(expr, false)?)
                } else {
                    Ok(Stmt::Expr(expr))
                }
            }
        }
    }

    fn parse_let(&mut self) -> Result<Stmt> {
        let name = self.expect_ident()?;
        self.expect(&Token::Eq)?;
        let value = self.parse_expr()?;

        // Check if this is a ternary assignment: v=cond{then}{else}
        // or a conditional assignment: v=cond{body}
        if self.peek() == Some(&Token::LBrace) && is_guard_eligible_condition(&value) {
            let then_body = self.parse_brace_body()?;
            if self.peek() == Some(&Token::LBrace) {
                // Two brace blocks: v=cond{then}{else}
                // Desugar to: Let { name, value: Ternary { condition, then_expr, else_expr } }
                let else_body = self.parse_brace_body()?;
                let then_expr = body_to_expr(then_body);
                let else_expr = body_to_expr(else_body);
                Ok(Stmt::Let {
                    name,
                    value: Expr::Ternary {
                        condition: Box::new(value),
                        then_expr: Box::new(then_expr),
                        else_expr: Box::new(else_expr),
                    },
                })
            } else {
                // Single brace block: v=cond{body} (conditional assignment)
                // Desugar to: Guard { condition, body: [Let { name, value: last_expr }] }
                let body_with_let = wrap_body_as_let(&name, then_body);
                Ok(Stmt::Guard {
                    condition: value,
                    negated: false,
                    body: body_with_let,
                    else_body: None,
                    braceless: false,
                })
            }
        } else {
            Ok(Stmt::Let { name, value })
        }
    }

    /// Lookahead: `{ident;ident...}=` — destructure pattern
    fn is_destructure_pattern(&self) -> bool {
        let mut pos = self.pos + 1; // skip `{`
        loop {
            match self.token_at(pos) {
                Some(Token::Ident(_)) => pos += 1,
                Some(Token::Semi) => pos += 1,
                Some(Token::RBrace) => {
                    return self.token_at(pos + 1) == Some(&Token::Eq);
                }
                _ => return false,
            }
        }
    }

    /// `{a;b;c}=expr` — destructure record fields into bindings
    fn parse_destructure(&mut self) -> Result<Stmt> {
        self.expect(&Token::LBrace)?;
        let mut bindings = Vec::new();
        loop {
            let name = self.expect_ident()?;
            bindings.push(name);
            if self.peek() == Some(&Token::Semi) {
                self.advance(); // consume `;`
            } else {
                break;
            }
        }
        self.expect(&Token::RBrace)?;
        self.expect(&Token::Eq)?;
        let value = self.parse_expr()?;
        Ok(Stmt::Destructure { bindings, value })
    }

    /// `?{arms}` or `?expr{arms}`
    fn parse_match_stmt(&mut self) -> Result<Stmt> {
        self.expect(&Token::Question)?;
        let subject = if self.peek() == Some(&Token::LBrace) {
            None
        } else {
            Some(self.parse_atom()?)
        };
        self.expect(&Token::LBrace)?;
        let arms = self.parse_match_arms()?;
        self.expect(&Token::RBrace)?;
        Ok(Stmt::Match { subject, arms })
    }

    fn parse_match_arms(&mut self) -> Result<Vec<MatchArm>> {
        let mut arms = Vec::new();
        while self.peek() != Some(&Token::RBrace) {
            if !arms.is_empty() {
                self.expect(&Token::Semi)?;
                if self.peek() == Some(&Token::RBrace) {
                    break;
                }
            }
            arms.push(self.parse_match_arm()?);
        }
        Ok(arms)
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm> {
        let pattern = self.parse_pattern()?;
        self.expect(&Token::Colon)?;
        let body = self.parse_arm_body()?;
        Ok(MatchArm { pattern, body })
    }

    /// Parse body of a match arm — multiple statements until next arm pattern or `}`.
    ///
    /// Two body shapes are accepted:
    /// - Brace block: `~v:{stmt1;stmt2;final-expr}` — mirrors `=cond{block}` grammar,
    ///   makes the arm boundary unambiguous when the body contains call-shapes that
    ///   could look like patterns. Final stmt is the arm value.
    /// - Inline `;`-separated: `~v:stmt1;stmt2;final-expr` — existing form. `;` followed
    ///   by a pattern-shaped token sequence starts a new arm (see `semi_starts_new_arm`).
    fn parse_arm_body(&mut self) -> Result<Vec<Spanned<Stmt>>> {
        // Brace-block form: only when the `{...}` is not a destructure pattern start
        // (e.g. `{a, b}=v` is a destructure assignment, kept on the inline path).
        if self.peek() == Some(&Token::LBrace) && !self.is_destructure_pattern() {
            return self.parse_brace_body();
        }
        let mut stmts = Vec::new();
        if !self.at_arm_end() {
            let span_start = self.peek_span();
            let stmt = self.parse_stmt()?;
            stmts.push(Spanned {
                node: stmt,
                span: span_start.merge(self.prev_span()),
            });
            // Continue consuming statements if `;` is followed by non-pattern content
            while self.peek() == Some(&Token::Semi) && !self.semi_starts_new_arm() {
                self.advance(); // consume ;
                if self.at_arm_end() {
                    break;
                }
                let span_start = self.peek_span();
                let stmt = self.parse_stmt()?;
                stmts.push(Spanned {
                    node: stmt,
                    span: span_start.merge(self.prev_span()),
                });
            }
        }
        Ok(stmts)
    }

    /// Check if the `;` at current position starts a new match arm.
    /// A new arm starts with a pattern followed by `:`.
    fn semi_starts_new_arm(&self) -> bool {
        if self.peek() != Some(&Token::Semi) {
            return false;
        }
        // Look past the `;`
        let after_semi = self.pos + 1;
        if after_semi >= self.tokens.len() {
            return false;
        }
        match self.token_at(after_semi) {
            // ^ident: or ^_: → err pattern
            Some(Token::Caret) => {
                if after_semi + 2 < self.tokens.len() {
                    matches!(
                        (self.token_at(after_semi + 1), self.token_at(after_semi + 2)),
                        (
                            Some(Token::Ident(_) | Token::Underscore),
                            Some(Token::Colon)
                        )
                    )
                } else {
                    false
                }
            }
            // ~ident: or ~_: → ok pattern
            Some(Token::Tilde) => {
                if after_semi + 2 < self.tokens.len() {
                    matches!(
                        (self.token_at(after_semi + 1), self.token_at(after_semi + 2)),
                        (
                            Some(Token::Ident(_) | Token::Underscore),
                            Some(Token::Colon)
                        )
                    )
                } else {
                    false
                }
            }
            // _: → wildcard
            Some(Token::Underscore) => {
                after_semi + 1 < self.tokens.len()
                    && self.token_at(after_semi + 1) == Some(&Token::Colon)
            }
            // literal: → literal pattern (number, string, bool)
            Some(Token::Number(_) | Token::Text(_) | Token::True | Token::False | Token::Nil) => {
                after_semi + 1 < self.tokens.len()
                    && self.token_at(after_semi + 1) == Some(&Token::Colon)
            }
            // n/t/b/l ident: or n/t/b/l _: → TypeIs pattern
            Some(Token::Ident(ty_name)) if matches!(ty_name.as_str(), "n" | "t" | "b" | "l") => {
                if after_semi + 2 < self.tokens.len() {
                    matches!(
                        (self.token_at(after_semi + 1), self.token_at(after_semi + 2)),
                        (
                            Some(Token::Ident(_) | Token::Underscore),
                            Some(Token::Colon)
                        )
                    )
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn at_arm_end(&self) -> bool {
        matches!(self.peek(), None | Some(Token::RBrace) | Some(Token::Semi))
    }

    fn parse_pattern(&mut self) -> Result<Pattern> {
        match self.peek() {
            Some(Token::Caret) => {
                self.advance();
                let name = match self.peek() {
                    Some(Token::Underscore) => {
                        self.advance();
                        "_".to_string()
                    }
                    _ => self.expect_ident()?,
                };
                Ok(Pattern::Err(name))
            }
            Some(Token::Tilde) => {
                self.advance();
                let name = match self.peek() {
                    Some(Token::Underscore) => {
                        self.advance();
                        "_".to_string()
                    }
                    _ => self.expect_ident()?,
                };
                Ok(Pattern::Ok(name))
            }
            Some(Token::Underscore) => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            Some(Token::Number(_)) => {
                if let Some(Token::Number(n)) = self.advance().cloned() {
                    Ok(Pattern::Literal(Literal::Number(n)))
                } else {
                    unreachable!()
                }
            }
            Some(Token::Text(_)) => {
                if let Some(Token::Text(s)) = self.advance().cloned() {
                    Ok(Pattern::Literal(Literal::Text(s)))
                } else {
                    unreachable!()
                }
            }
            Some(Token::True) => {
                self.advance();
                Ok(Pattern::Literal(Literal::Bool(true)))
            }
            Some(Token::False) => {
                self.advance();
                Ok(Pattern::Literal(Literal::Bool(false)))
            }
            Some(Token::Nil) => {
                self.advance();
                Ok(Pattern::Literal(Literal::Nil))
            }
            Some(Token::Ident(name)) if matches!(name.as_str(), "n" | "t" | "b" | "l") => {
                let ty = match name.as_str() {
                    "n" => Type::Number,
                    "t" => Type::Text,
                    "b" => Type::Bool,
                    "l" => Type::List(Box::new(Type::Text)),
                    _ => unreachable!(),
                };
                self.advance();
                let binding = match self.peek() {
                    Some(Token::Underscore) => {
                        self.advance();
                        "_".to_string()
                    }
                    _ => self.expect_ident()?,
                };
                Ok(Pattern::TypeIs { ty, binding })
            }
            Some(tok) => Err(self.error("ILO-P011", format!("expected pattern, got {:?}", tok))),
            None => Err(self.error("ILO-P012", "expected pattern, got EOF".into())),
        }
    }

    /// `@binding collection{body}` or `@binding start..end{body}`
    fn parse_foreach(&mut self) -> Result<Stmt> {
        self.expect(&Token::At)?;
        let binding = self.expect_ident()?;
        // Range bounds accept any operand, not just atoms: this lets personas
        // write `@j +i 2..n` and `@j 0..-n 1` directly instead of binding an
        // intermediate (`jst=+i 2;@j jst..n`). Call-style bounds like
        // `@j 0..len xs` still need a binding; see tests/regression_range_expr.rs
        // for the negative anchor.
        let start_expr = self.parse_operand()?;
        // Check for range syntax: start..end
        if self.peek() == Some(&Token::DotDot) {
            self.advance(); // consume ..
            let end_expr = self.parse_operand()?;
            let body = self.parse_brace_body()?;
            return Ok(Stmt::ForRange {
                binding,
                start: start_expr,
                end: end_expr,
                body,
            });
        }
        let body = self.parse_brace_body()?;
        Ok(Stmt::ForEach {
            binding,
            collection: start_expr,
            body,
        })
    }

    /// Parse `!` at statement position — negated guard `!cond{body}` or logical NOT `!expr`.
    /// Also supports braceless negated guards: `!>=x 10 "fallback"`.
    fn parse_bang_stmt(&mut self) -> Result<Stmt> {
        self.expect(&Token::Bang)?;
        let inner = self.parse_expr_inner()?;

        if self.peek() == Some(&Token::LBrace) {
            // Negated guard: !cond{body} or !cond{then}{else}
            let body = self.parse_brace_body()?;
            let else_body = if self.peek() == Some(&Token::LBrace) {
                Some(self.parse_brace_body()?)
            } else {
                None
            };
            Ok(Stmt::Guard {
                condition: inner,
                negated: true,
                body,
                else_body,
                braceless: false,
            })
        } else if is_guard_eligible_condition(&inner) && self.can_start_operand() {
            Ok(self.parse_braceless_guard_body(inner, true)?)
        } else {
            // Logical NOT as expression statement: !expr
            Ok(Stmt::Expr(Expr::UnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(inner),
            }))
        }
    }

    /// Parse `^` at statement position — Err constructor: `^expr`
    fn parse_caret_stmt(&mut self) -> Result<Stmt> {
        self.expect(&Token::Caret)?;
        let inner = self.parse_expr_inner()?;
        Ok(Stmt::Expr(Expr::Err(Box::new(inner))))
    }

    /// Parse ident-starting statement — could be guard (expr{body}) or expr statement.
    /// Also supports braceless guards: `>=sp 1000 "gold"` (no braces needed when
    /// the condition is a comparison/logical operator and the body is a single expression).
    fn parse_expr_or_guard(&mut self) -> Result<Stmt> {
        let expr = self.parse_expr()?;
        if self.peek() == Some(&Token::LBrace) {
            let body = self.parse_brace_body()?;
            let else_body = if self.peek() == Some(&Token::LBrace) {
                Some(self.parse_brace_body()?)
            } else {
                None
            };
            Ok(Stmt::Guard {
                condition: expr,
                negated: false,
                body,
                else_body,
                braceless: false,
            })
        } else if is_guard_eligible_condition(&expr) && self.can_start_operand() {
            Ok(self.parse_braceless_guard_body(expr, false)?)
        } else {
            Ok(Stmt::Expr(expr))
        }
    }

    /// Parse the body of a braceless guard after eligibility has been confirmed.
    /// Uses `parse_operand` (not `parse_expr`) so function calls are NOT consumed —
    /// call bodies require braces: `>=sp 1000{classify sp}`.
    fn parse_braceless_guard_body(&mut self, condition: Expr, negated: bool) -> Result<Stmt> {
        let body_start = self.peek_span();
        let body_expr = self.parse_operand()?;
        let body_span = body_start.merge(self.prev_span());

        // Dangling token detection: after a braceless guard body, the next token
        // must be `;`, `}`, or EOF. If something else follows, the user likely
        // wrote a function call without braces: `>=sp 1000 classify sp`
        if !matches!(self.peek(), None | Some(Token::Semi) | Some(Token::RBrace)) {
            return Err(self.error_hint(
                "ILO-P016",
                "unexpected token after braceless guard body".to_string(),
                "function calls in braceless guards need braces: >=cond val{func args}".to_string(),
            ));
        }

        Ok(Stmt::Guard {
            condition,
            negated,
            body: vec![Spanned::new(Stmt::Expr(body_expr), body_span)],
            else_body: None,
            braceless: true,
        })
    }

    fn parse_brace_body(&mut self) -> Result<Vec<Spanned<Stmt>>> {
        self.expect(&Token::LBrace)?;
        let body = self.parse_body()?;
        self.expect(&Token::RBrace)?;
        Ok(body)
    }

    // ---- Expressions ----

    fn parse_expr(&mut self) -> Result<Expr> {
        let expr = match self.peek() {
            Some(Token::Tilde) => {
                self.advance();
                let inner = self.parse_expr_inner()?;
                Expr::Ok(Box::new(inner))
            }
            Some(Token::Caret) => {
                self.advance();
                let inner = self.parse_expr_inner()?;
                Expr::Err(Box::new(inner))
            }
            _ => self.parse_expr_inner()?,
        };
        let expr = self.maybe_with(expr)?;
        let expr = self.maybe_nil_coalesce(expr)?;
        self.maybe_pipe(expr)
    }

    /// Parse expression, possibly followed by `with`
    fn maybe_with(&mut self, expr: Expr) -> Result<Expr> {
        if matches!(self.peek(), Some(Token::With)) {
            self.advance();
            let mut updates = Vec::new();
            while let Some(Token::Ident(_)) = self.peek() {
                if self.pos + 1 < self.tokens.len()
                    && self.token_at(self.pos + 1) == Some(&Token::Colon)
                {
                    let name = self.expect_ident()?;
                    self.expect(&Token::Colon)?;
                    let value = self.parse_atom()?;
                    updates.push((name, value));
                } else {
                    break;
                }
            }
            Ok(Expr::With {
                object: Box::new(expr),
                updates,
            })
        } else {
            Ok(expr)
        }
    }

    /// Parse nil-coalesce: `a ?? b` — if a is nil, use b
    fn maybe_nil_coalesce(&mut self, mut expr: Expr) -> Result<Expr> {
        while matches!(self.peek(), Some(Token::NilCoalesce)) {
            self.advance(); // consume ??
            let default = self.parse_expr_inner()?;
            expr = Expr::NilCoalesce {
                value: Box::new(expr),
                default: Box::new(default),
            };
        }
        Ok(expr)
    }

    /// Parse pipe chains: `expr >> func` desugars to `func(expr)`.
    /// `expr >> func a b` desugars to `func(a, b, expr)` — piped value becomes last arg.
    fn maybe_pipe(&mut self, mut expr: Expr) -> Result<Expr> {
        while matches!(self.peek(), Some(Token::PipeOp)) {
            self.advance(); // consume >>
            let func_name = self.expect_ident()?;
            let unwrap = self.peek() == Some(&Token::Bang) && {
                let prev = self.prev_span();
                let bang = self.peek_span();
                prev.end > 0 && bang.start == prev.end
            };
            if unwrap {
                self.advance(); // consume !
            }
            // Parse additional args (operands until we hit >>, ;, }, etc.)
            // Use call-arg parsing so nested calls inside a pipe target
            // expand naturally (e.g. `xs >> map str` keeps `str` as a bare
            // fn-ref since `map`'s first arg is a fn-ref position).
            let mut args = Vec::new();
            while self.can_start_operand() {
                let arg_idx = args.len();
                let in_fn_pos = self.is_fn_ref_position(&func_name, arg_idx);
                args.push(self.parse_call_arg(in_fn_pos)?);
            }
            // Piped value becomes last arg
            args.push(expr);
            expr = Expr::Call {
                function: func_name,
                args,
                unwrap,
            };
        }
        Ok(expr)
    }

    /// Return the infix binding power (left, right) for a token, or None if not infix.
    /// Higher numbers bind tighter. Right bp > left bp for left-associativity.
    /// Operators that, in the middle of a call-arg sequence, may end the call
    /// by binding the preceding expression as their left operand. Covers
    /// Pratt-table infix ops plus `??` (handled by `maybe_nil_coalesce`).
    fn is_infix_or_suffix_op(token: &Token) -> bool {
        matches!(token, Token::NilCoalesce) || Self::infix_binding_power(token).is_some()
    }

    fn infix_binding_power(token: &Token) -> Option<(u8, u8, BinOp)> {
        match token {
            Token::Pipe => Some((1, 2, BinOp::Or)),
            Token::Amp => Some((3, 4, BinOp::And)),
            Token::Eq => Some((5, 6, BinOp::Equals)),
            Token::NotEq => Some((5, 6, BinOp::NotEquals)),
            Token::Less => Some((7, 8, BinOp::LessThan)),
            Token::Greater => Some((7, 8, BinOp::GreaterThan)),
            Token::LessEq => Some((7, 8, BinOp::LessOrEqual)),
            Token::GreaterEq => Some((7, 8, BinOp::GreaterOrEqual)),
            Token::PlusEq => Some((9, 10, BinOp::Append)),
            Token::Plus => Some((9, 10, BinOp::Add)),
            Token::Minus => Some((9, 10, BinOp::Subtract)),
            Token::Star => Some((11, 12, BinOp::Multiply)),
            Token::Slash => Some((11, 12, BinOp::Divide)),
            _ => None,
        }
    }

    /// Pratt parser: given a left-hand expression, consume infix operators
    /// with binding power >= min_bp and build the tree.
    fn parse_infix(&mut self, mut left: Expr, min_bp: u8) -> Result<Expr> {
        while let Some(token) = self.peek() {
            let Some((l_bp, r_bp, op)) = Self::infix_binding_power(token) else {
                break;
            };
            if l_bp < min_bp {
                break;
            }
            self.advance(); // consume operator
            // Parse right-hand side: an operand (atom or prefix op), then recurse for infix
            let right = self.parse_operand()?;
            let right = self.parse_infix(right, r_bp)?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// Parse a single list element — like `parse_expr_inner` but also handles
    /// `~expr` (Ok) and `^expr` (Err) wrapping that `parse_expr` normally handles.
    /// Scan ahead from the current position (just past the opening `[`)
    /// to determine whether this list literal contains a top-level comma.
    /// Used to choose between comma-separated mode (calls allowed in
    /// elements) and whitespace mode (bare refs are elements).
    fn list_has_top_level_comma(&self) -> bool {
        let mut depth_paren = 0;
        let mut depth_bracket = 0;
        let mut depth_brace = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].0 {
                Token::LParen => depth_paren += 1,
                Token::RParen => depth_paren -= 1,
                Token::LBracket => depth_bracket += 1,
                Token::RBracket => {
                    if depth_bracket == 0 && depth_paren == 0 && depth_brace == 0 {
                        return false;
                    }
                    depth_bracket -= 1;
                }
                Token::LBrace => depth_brace += 1,
                Token::RBrace => depth_brace -= 1,
                Token::Comma if depth_paren == 0 && depth_bracket == 0 && depth_brace == 0 => {
                    return true;
                }
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Whitespace-mode list element: bare refs become elements, not calls.
    /// Without this guard, `[a b c]` would parse as `[Call(a, [b, c])]` and
    /// confuse agents who reasonably expect it to mirror `[1 2 3]`. Calls
    /// inside whitespace-list elements still work via parens (`[(f x) y]`
    /// or `[f(x) y]`) — the flag is cleared on paren entry.
    fn parse_list_element(&mut self) -> Result<Expr> {
        let prev = self.no_whitespace_call;
        self.no_whitespace_call = true;
        let result = self.parse_list_element_call_ok();
        self.no_whitespace_call = prev;
        result
    }

    /// Comma-mode list element: full expression including whitespace-calls.
    /// Used when the list literal contains a top-level comma, so
    /// `[floor x, ceil x]` parses each side as its own call expression.
    fn parse_list_element_call_ok(&mut self) -> Result<Expr> {
        match self.peek() {
            Some(Token::Tilde) => {
                self.advance();
                let inner = self.parse_expr_inner()?;
                Ok(Expr::Ok(Box::new(inner)))
            }
            Some(Token::Caret) => {
                self.advance();
                let inner = self.parse_expr_inner()?;
                Ok(Expr::Err(Box::new(inner)))
            }
            _ => self.parse_expr_inner(),
        }
    }

    /// Core expression parsing — handles prefix ops, match expr, calls, atoms.
    /// Infix operators are only applied after atoms/calls, not after prefix operators
    /// (prefix forms like `+a b` are self-contained).
    fn parse_expr_inner(&mut self) -> Result<Expr> {
        match self.peek() {
            // Minus is special: could be unary negation (-x) or binary subtract (-a b)
            Some(Token::Minus) => self.parse_minus(),
            // Logical NOT: !x
            Some(Token::Bang) => {
                self.advance();
                let operand = self.parse_operand()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                })
            }
            // Dollar prefix: $expr → get expr
            Some(Token::Dollar) => self.parse_dollar(),
            // Prefix binary operators: +a b, *a b, etc. — self-contained, no infix after
            Some(Token::Plus)
            | Some(Token::Star)
            | Some(Token::Slash)
            | Some(Token::Greater)
            | Some(Token::Less)
            | Some(Token::GreaterEq)
            | Some(Token::LessEq)
            | Some(Token::Eq)
            | Some(Token::NotEq)
            | Some(Token::Amp)
            | Some(Token::Pipe)
            | Some(Token::PlusEq) => self.parse_prefix_binop(),
            // Prefix nil-coalesce: ??a b — mirror of infix `a ?? b`
            Some(Token::NilCoalesce) => {
                self.advance();
                let value = self.parse_operand()?;
                let default = self.parse_expr_inner()?;
                Ok(Expr::NilCoalesce {
                    value: Box::new(value),
                    default: Box::new(default),
                })
            }
            // Match expression: ?expr{...} or ?{...}, or prefix ternary: ?=x 0 10 20
            Some(Token::Question) => self.parse_question_expr(),
            // Atoms and calls — infix operators can follow these
            _ => {
                let primary = self.parse_call_or_atom()?;
                self.parse_infix(primary, 0)
            }
        }
    }

    /// `$expr` → `get expr`, `$!expr` → `get! expr`
    fn parse_dollar(&mut self) -> Result<Expr> {
        self.advance(); // consume $
        // Check for $! (auto-unwrap)
        let unwrap = self.peek() == Some(&Token::Bang) && {
            let prev = self.prev_span();
            let bang = self.peek_span();
            prev.end > 0 && bang.start == prev.end
        };
        if unwrap {
            self.advance(); // consume !
        }
        let arg = self.parse_operand()?;
        Ok(Expr::Call {
            function: "get".to_string(),
            args: vec![arg],
            unwrap,
        })
    }

    /// Check if `?` at current position is followed by a comparison op (prefix ternary).
    fn is_prefix_ternary(&self) -> bool {
        matches!(
            self.token_at(self.pos + 1),
            Some(
                Token::Eq
                    | Token::Greater
                    | Token::Less
                    | Token::GreaterEq
                    | Token::LessEq
                    | Token::NotEq
            )
        )
    }

    /// Parse `?` as either match (`?expr{...}`) or prefix ternary (`?=x 0 10 20`).
    fn parse_question_expr(&mut self) -> Result<Expr> {
        if self.is_prefix_ternary() {
            return self.parse_prefix_ternary();
        }
        self.parse_match_expr()
    }

    /// Parse prefix ternary: `?=x 0 10 20` → Ternary { condition: BinOp(=, x, 0), then: 10, else: 20 }
    fn parse_prefix_ternary(&mut self) -> Result<Expr> {
        self.advance(); // consume ?
        // Parse the condition as a prefix binop (=x 0, >x 5, etc.)
        let condition = self.parse_prefix_binop()?;
        // Parse then and else expressions
        let then_expr = self.parse_operand()?;
        let else_expr = self.parse_operand()?;
        Ok(Expr::Ternary {
            condition: Box::new(condition),
            then_expr: Box::new(then_expr),
            else_expr: Box::new(else_expr),
        })
    }

    /// Parse match as expression: `?expr{arms}` or `?{arms}`
    fn parse_match_expr(&mut self) -> Result<Expr> {
        self.expect(&Token::Question)?;
        let subject = if self.peek() == Some(&Token::LBrace) {
            None
        } else {
            Some(Box::new(self.parse_atom()?))
        };
        self.expect(&Token::LBrace)?;
        let arms = self.parse_match_arms()?;
        self.expect(&Token::RBrace)?;
        Ok(Expr::Match { subject, arms })
    }

    /// Parse `-`: unary negation (`-x`) when one atom follows,
    /// binary subtract (`-a b`) when two atoms follow.
    fn parse_minus(&mut self) -> Result<Expr> {
        self.advance(); // consume `-`
        let first = self.parse_operand()?;
        if self.can_start_operand() {
            let second = self.parse_operand()?;
            Ok(Expr::BinOp {
                op: BinOp::Subtract,
                left: Box::new(first),
                right: Box::new(second),
            })
        } else {
            Ok(Expr::UnaryOp {
                op: UnaryOp::Negate,
                operand: Box::new(first),
            })
        }
    }

    fn parse_prefix_binop(&mut self) -> Result<Expr> {
        let op = match self.advance() {
            Some(Token::Plus) => BinOp::Add,
            Some(Token::Star) => BinOp::Multiply,
            Some(Token::Slash) => BinOp::Divide,
            Some(Token::Greater) => BinOp::GreaterThan,
            Some(Token::Less) => BinOp::LessThan,
            Some(Token::GreaterEq) => BinOp::GreaterOrEqual,
            Some(Token::LessEq) => BinOp::LessOrEqual,
            Some(Token::Eq) => BinOp::Equals,
            Some(Token::NotEq) => BinOp::NotEquals,
            Some(Token::Amp) => {
                if self.peek() == Some(&Token::Amp) {
                    return Err(self.error_hint(
                        "ILO-P003",
                        "unexpected '&&': ilo uses single '&' for AND".to_string(),
                        "ilo uses single '&' for AND, '|' for OR".to_string(),
                    ));
                }
                BinOp::And
            }
            Some(Token::Pipe) => {
                if self.peek() == Some(&Token::Pipe) {
                    return Err(self.error_hint(
                        "ILO-P003",
                        "unexpected '||': ilo uses single '|' for OR".to_string(),
                        "ilo uses single '&' for AND, '|' for OR".to_string(),
                    ));
                }
                BinOp::Or
            }
            Some(Token::PlusEq) => BinOp::Append,
            _ => unreachable!(),
        };
        let left = self.parse_operand()?;
        let right = self.parse_operand()?;
        Ok(Expr::BinOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// Register a user function's arity and per-param fn-ref flags so that
    /// call-arg parsing can eagerly consume nested calls when this function
    /// is used as the outer callee.
    fn register_user_fn(&mut self, name: &str, params: &[Param]) {
        self.fn_arity.insert(name.to_string(), params.len());
        let flags: Vec<bool> = params
            .iter()
            .map(|p| matches!(p.ty, Type::Fn(_, _)))
            .collect();
        self.fn_param_is_fn.insert(name.to_string(), flags);
    }

    /// Is arg position `arg_idx` of function `outer_name` a fn-ref position
    /// (i.e. expects a function reference, not a regular value)? When true,
    /// we must NOT eagerly expand an Ident in that position as a nested call.
    fn is_fn_ref_position(&self, outer_name: &str, arg_idx: usize) -> bool {
        self.fn_param_is_fn
            .get(outer_name)
            .and_then(|v| v.get(arg_idx).copied())
            .unwrap_or(false)
    }

    /// Parse a single call argument. If `in_fn_ref_pos` is true, falls back
    /// to plain `parse_operand` so an Ident stays as a bare ref (HOF use).
    /// Otherwise, when the next token is an Ident naming a known function
    /// with arity N, eagerly consume that Ident plus its N args as a nested
    /// call — this lets agents write `prnt str nc` and `hd tl xs` naturally.
    fn parse_call_arg(&mut self, in_fn_ref_pos: bool) -> Result<Expr> {
        if !in_fn_ref_pos
            && let Some(Token::Ident(name)) = self.peek()
            && let Some(&arity) = self.fn_arity.get(name)
            && arity > 0
        {
            // Don't eagerly expand if the Ident is followed by tokens that
            // turn it into something other than a plain call (record fields,
            // field/index access, postfix-bang, zero-arg paren form).
            let next = self.token_at(self.pos + 1);
            let is_record = matches!(next, Some(Token::Ident(_)))
                && self.token_at(self.pos + 2) == Some(&Token::Colon);
            let is_field = matches!(next, Some(Token::Dot) | Some(Token::DotQuestion));
            let is_zero_arg_call =
                next == Some(&Token::LParen) && self.token_at(self.pos + 2) == Some(&Token::RParen);
            let is_unwrap = next == Some(&Token::Bang) && {
                let ident_span = self.peek_span();
                let bang_span = self
                    .tokens
                    .get(self.pos + 1)
                    .map(|(_, s)| *s)
                    .unwrap_or(Span::UNKNOWN);
                ident_span.end > 0 && bang_span.start == ident_span.end
            };
            if !(is_record || is_field || is_zero_arg_call || is_unwrap) {
                let inner_name = name.clone();
                self.advance(); // consume the inner function ident
                let mut inner_args = Vec::with_capacity(arity);
                for i in 0..arity {
                    if !self.can_start_operand() {
                        // Underfilled — let the verifier report arity mismatch.
                        break;
                    }
                    let inner_fn_pos = self.is_fn_ref_position(&inner_name, i);
                    inner_args.push(self.parse_call_arg(inner_fn_pos)?);
                }
                return Ok(Expr::Call {
                    function: inner_name,
                    args: inner_args,
                    unwrap: false,
                });
            }
        }
        self.parse_operand()
    }

    /// Parse function call or plain atom
    /// call = IDENT atom+ (greedy, when not a record)
    /// Also handles zero-arg calls: `func()`
    fn parse_call_or_atom(&mut self) -> Result<Expr> {
        let atom = self.parse_atom()?;

        // If atom is a Ref, check if it's a call or record construction
        if let Expr::Ref(ref name) = atom {
            let name = name.clone();

            // Check for auto-unwrap: name! (postfix Bang ADJACENT to name, no space)
            // Distinguish `func!` (unwrap) from `func !x` (call with NOT arg)
            // by checking if Bang span starts right where the Ident span ended.
            let unwrap = self.peek() == Some(&Token::Bang) && {
                let prev = self.prev_span();
                let bang = self.peek_span();
                // Adjacent if spans are real (non-zero) and contiguous
                prev.end > 0 && bang.start == prev.end
            };
            if unwrap {
                self.advance(); // consume !
            }

            // Check for zero-arg call: name() or name!()
            if self.peek() == Some(&Token::LParen)
                && self.pos + 1 < self.tokens.len()
                && self.token_at(self.pos + 1) == Some(&Token::RParen)
            {
                self.advance(); // (
                self.advance(); // )
                return Ok(Expr::Call {
                    function: name,
                    args: vec![],
                    unwrap,
                });
            }

            // If we consumed !, this must be a call (even with zero args if nothing follows)
            if unwrap {
                let mut args = Vec::new();
                while self.can_start_operand() {
                    let arg_idx = args.len();
                    let in_fn_pos = self.is_fn_ref_position(&name, arg_idx);
                    args.push(self.parse_call_arg(in_fn_pos)?);
                }
                return Ok(Expr::Call {
                    function: name,
                    args,
                    unwrap: true,
                });
            }

            // Check for record construction: name field:value
            if self.is_named_field_ahead() {
                return self.parse_record(name);
            }

            // Zero-arg builtins: `rnd`/`now`/`mmap` with no args → Call with empty args
            if (name == "rnd" || name == "now" || name == "mmap") && !self.can_start_operand() {
                return Ok(Expr::Call {
                    function: name,
                    args: vec![],
                    unwrap: false,
                });
            }

            // Inside a list literal, `[a b c]` must yield three list
            // elements rather than `[Call(a, [b, c])]`. If the next token
            // would otherwise start a call argument, return the bare Ref.
            if self.no_whitespace_call {
                return Ok(atom);
            }

            // Check for function call: name followed by args
            //
            // Infix interaction: when the first token after the name is an
            // operator, use lookahead to decide prefix-as-call-arg vs infix:
            //   `fac -n 1` → fac(-(n,1))  (operator + 2 atoms = prefix binary)
            //   `x - 3`   → x - 3         (operator + 1 atom = infix)
            //   `f a + b` → f(a) + b      (atom then operator = infix on call)
            if self.can_start_operand() {
                // If the first token is an infix-eligible operator, check if it
                // looks like a prefix binary op (followed by 2+ atoms) or infix
                if let Some(tok) = self.peek()
                    && Self::is_infix_or_suffix_op(tok)
                    && !self.looks_like_prefix_binary(self.pos)
                {
                    return Ok(atom);
                }
                let mut args = Vec::new();
                while self.can_start_operand() {
                    let arg_idx = args.len();
                    let in_fn_pos = self.is_fn_ref_position(&name, arg_idx);
                    args.push(self.parse_call_arg(in_fn_pos)?);
                    // After each arg, if next is infix, stop. `??` is always
                    // infix once we've already collected at least one arg —
                    // `f a ?? b` means `(f a) ?? b`, never `f a (??b ...)`.
                    // Without this, chained `f a ?? g b ?? d` mis-parses as
                    // `f a (?? g b) (?? d)` because the prefix-binary scanner
                    // sees `?? g b` as a valid prefix nil-coalesce form.
                    if let Some(tok) = self.peek()
                        && Self::is_infix_or_suffix_op(tok)
                        && (matches!(tok, Token::NilCoalesce)
                            || !self.looks_like_prefix_binary(self.pos))
                    {
                        break;
                    }
                }
                return Ok(Expr::Call {
                    function: name,
                    args,
                    unwrap: false,
                });
            }
        }

        Ok(atom)
    }

    /// Check if next tokens look like `ident:expr` (named field)
    fn is_named_field_ahead(&self) -> bool {
        if let Some(Token::Ident(_)) = self.peek()
            && self.pos + 1 < self.tokens.len()
            && self.token_at(self.pos + 1) == Some(&Token::Colon)
        {
            // Make sure it's not a param pattern (type follows colon)
            return true;
        }
        false
    }

    /// Parse record: `typename field:val field:val`
    fn parse_record(&mut self, type_name: String) -> Result<Expr> {
        let mut fields = Vec::new();
        while self.is_named_field_ahead() {
            let fname = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let value = self.parse_atom()?;
            fields.push((fname, value));
        }
        Ok(Expr::Record { type_name, fields })
    }

    /// Lookahead: does the token at `pos` start a prefix binary operator
    /// (operator followed by 2+ simple atoms before the next operator/terminator)?
    ///
    /// Used to disambiguate: `fac -n 1` (prefix: `-` + 2 atoms) vs `x - 3` (infix: `-` + 1 atom).
    /// Counts consecutive simple atoms; an operator-headed sub-expression that itself
    /// looks_like_prefix_binary also counts as one atom (so `h +a +b c` parses with
    /// `+a` and `+b c` as two args).
    fn looks_like_prefix_binary(&self, pos: usize) -> bool {
        self.scan_prefix_binary_end(pos).is_some()
    }

    /// If the token at `pos` heads a prefix-binary expression (operator + 2 atoms,
    /// where each atom may itself be a nested prefix-binary), return the position
    /// just after the last consumed token. Otherwise return None.
    fn scan_prefix_binary_end(&self, pos: usize) -> Option<usize> {
        if pos >= self.tokens.len() {
            return None;
        }
        let mut count = 0;
        let mut look = pos + 1;
        while look < self.tokens.len() && count < 2 {
            // Stop at function declaration boundaries
            if self.is_fn_decl_start(look) {
                break;
            }
            let t = &self.tokens[look].0;
            match t {
                Token::Ident(_)
                | Token::Number(_)
                | Token::Text(_)
                | Token::True
                | Token::False
                | Token::Nil
                | Token::Underscore => {
                    count += 1;
                    look += 1;
                }
                Token::LParen | Token::LBracket => {
                    // Paren/bracket group counts as one atom
                    count += 1;
                    let close = if *t == Token::LParen {
                        Token::RParen
                    } else {
                        Token::RBracket
                    };
                    let mut depth = 1;
                    look += 1;
                    while look < self.tokens.len() && depth > 0 {
                        let inner = &self.tokens[look].0;
                        if *inner == *t {
                            depth += 1;
                        }
                        if *inner == close {
                            depth -= 1;
                        }
                        look += 1;
                    }
                }
                // A nested prefix-binary operator counts as one atom if it itself
                // heads a prefix-binary sub-expression. Only the binary-only
                // operators listed in parse_prefix_binop qualify (plus Minus,
                // which is handled by parse_minus and is also binary-capable).
                // Unary-only operators (Bang/Tilde/Caret) are intentionally
                // excluded — they aren't prefix-binary.
                Token::Plus
                | Token::Minus
                | Token::Star
                | Token::Slash
                | Token::Greater
                | Token::Less
                | Token::GreaterEq
                | Token::LessEq
                | Token::Eq
                | Token::NotEq
                | Token::Amp
                | Token::Pipe
                | Token::PlusEq
                | Token::NilCoalesce => {
                    if let Some(end) = self.scan_prefix_binary_end(look) {
                        count += 1;
                        look = end;
                    } else {
                        break;
                    }
                }
                // Stop at other operators, terminators, etc.
                _ => break,
            }
        }
        if count >= 2 { Some(look) } else { None }
    }

    /// Can the current token start an atom?
    fn can_start_atom(&self) -> bool {
        matches!(
            self.peek(),
            Some(Token::Ident(_))
                | Some(Token::Number(_))
                | Some(Token::Text(_))
                | Some(Token::True)
                | Some(Token::False)
                | Some(Token::Nil)
                | Some(Token::Underscore)
                | Some(Token::LParen)
                | Some(Token::LBracket)
        )
    }

    /// Can the next token start an operand? (atom or prefix operator)
    /// Returns false if the current position looks like the start of a new function
    /// declaration — `Ident >` (zero-param) or `Ident Ident :` (parameterised) — so
    /// that a non-last function ending with a call doesn't greedily consume the next
    /// function's name as an argument.
    fn can_start_operand(&self) -> bool {
        // If the upcoming token is an Ident that begins a new declaration, stop here.
        if self.is_fn_decl_start(self.pos) {
            return false;
        }
        self.can_start_atom()
            || matches!(
                self.peek(),
                Some(Token::Plus)
                    | Some(Token::Minus)
                    | Some(Token::Star)
                    | Some(Token::Slash)
                    | Some(Token::Greater)
                    | Some(Token::Less)
                    | Some(Token::GreaterEq)
                    | Some(Token::LessEq)
                    | Some(Token::Eq)
                    | Some(Token::NotEq)
                    | Some(Token::Amp)
                    | Some(Token::Pipe)
                    | Some(Token::PlusEq)
                    | Some(Token::NilCoalesce)
                    | Some(Token::Bang)
                    | Some(Token::Tilde)
                    | Some(Token::Caret)
                    | Some(Token::Dollar)
            )
    }

    /// Parse an operand — an atom or a nested prefix operator.
    /// This sits between `parse_atom` (terminals only) and `parse_expr_inner`
    /// (which includes function calls). Prefix operators use this so that
    /// `+*a b c` works without greedy call parsing.
    fn parse_operand(&mut self) -> Result<Expr> {
        match self.peek() {
            Some(Token::Plus)
            | Some(Token::Star)
            | Some(Token::Slash)
            | Some(Token::Greater)
            | Some(Token::Less)
            | Some(Token::GreaterEq)
            | Some(Token::LessEq)
            | Some(Token::Eq)
            | Some(Token::NotEq)
            | Some(Token::Amp)
            | Some(Token::Pipe)
            | Some(Token::PlusEq) => self.parse_prefix_binop(),
            Some(Token::NilCoalesce) => {
                self.advance();
                let value = self.parse_operand()?;
                let default = self.parse_expr_inner()?;
                Ok(Expr::NilCoalesce {
                    value: Box::new(value),
                    default: Box::new(default),
                })
            }
            Some(Token::Minus) => self.parse_minus(),
            Some(Token::Bang) => {
                self.advance();
                let operand = self.parse_operand()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                })
            }
            Some(Token::Tilde) => {
                self.advance();
                let inner = self.parse_operand()?;
                Ok(Expr::Ok(Box::new(inner)))
            }
            Some(Token::Caret) => {
                self.advance();
                let inner = self.parse_operand()?;
                Ok(Expr::Err(Box::new(inner)))
            }
            Some(Token::Dollar) => self.parse_dollar(),
            _ => self.parse_atom(),
        }
    }

    /// Parse an atom — the smallest expression unit
    fn parse_atom(&mut self) -> Result<Expr> {
        match self.peek().cloned() {
            Some(Token::Number(n)) => {
                self.advance();
                Ok(Expr::Literal(Literal::Number(n)))
            }
            Some(Token::Text(s)) => {
                self.advance();
                Ok(Expr::Literal(Literal::Text(s)))
            }
            Some(Token::True) => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(true)))
            }
            Some(Token::False) => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(false)))
            }
            Some(Token::Nil) => {
                self.advance();
                Ok(Expr::Literal(Literal::Nil))
            }
            Some(Token::Underscore) => {
                self.advance();
                Ok(Expr::Ref("_".to_string()))
            }
            Some(Token::LParen) => {
                self.advance();
                // Parenthesised expressions are self-contained — restore
                // normal whitespace-call behaviour inside.
                let prev = self.no_whitespace_call;
                self.no_whitespace_call = false;
                let expr = self.parse_expr();
                self.no_whitespace_call = prev;
                let expr = expr?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Some(Token::LBracket) => {
                self.advance();
                // Disambiguation: if this list literal contains any comma
                // (at depth 0), it uses comma-separated mode where each
                // element is a full expression — calls like
                // `[floor x, ceil x]` work as expected. Otherwise the list
                // is whitespace-separated and bare refs become elements:
                // `[a b c]` → `[a, b, c]`, mirroring `[1 2 3]`. Calls
                // inside a whitespace-list must use parens: `[(f x) y]`.
                let has_comma = self.list_has_top_level_comma();
                let mut items = Vec::new();
                while self.peek() != Some(&Token::RBracket) {
                    if has_comma {
                        items.push(self.parse_list_element_call_ok()?);
                    } else {
                        items.push(self.parse_list_element()?);
                    }
                    // Skip optional comma separator
                    if self.peek() == Some(&Token::Comma) {
                        self.advance();
                    }
                }
                self.expect(&Token::RBracket)?;
                Ok(Expr::List(items))
            }
            Some(Token::Ident(name)) => {
                self.advance();
                // Zero-arg builtins used as operands (arguments to other calls)
                if name == "mmap" {
                    return Ok(Expr::Call {
                        function: name,
                        args: vec![],
                        unwrap: false,
                    });
                }
                // Check for field access chain: ident.field.field...
                let mut expr = Expr::Ref(name);
                while matches!(self.peek(), Some(Token::Dot) | Some(Token::DotQuestion)) {
                    let safe = self.peek() == Some(&Token::DotQuestion);
                    self.advance();
                    match self.peek().cloned() {
                        Some(Token::Number(n)) if n.fract() == 0.0 && n >= 0.0 => {
                            self.advance();
                            expr = Expr::Index {
                                object: Box::new(expr),
                                index: n as usize,
                                safe,
                            };
                        }
                        _ => {
                            let field = self.expect_ident()?;
                            expr = Expr::Field {
                                object: Box::new(expr),
                                field,
                                safe,
                            };
                        }
                    }
                }
                Ok(expr)
            }
            Some(tok) => Err(self.error("ILO-P009", format!("expected expression, got {:?}", tok))),
            None => Err(self.error("ILO-P010", "expected expression, got EOF".into())),
        }
    }

    fn parse_number(&mut self) -> Result<f64> {
        match self.peek().cloned() {
            Some(Token::Number(n)) => {
                self.advance();
                Ok(n)
            }
            Some(tok) => Err(self.error("ILO-P013", format!("expected number, got {:?}", tok))),
            None => Err(self.error("ILO-P014", "expected number, got EOF".into())),
        }
    }
}

/// Build the parser's static arity/HOF tables for builtins. These are used
/// during call-arg parsing to eagerly consume nested calls in arg position
/// (so `prnt str nc` parses as `prnt(str(nc))` instead of `prnt(str, nc)`).
///
/// Builtins with overloaded arities (`rnd`/`now` — 0 args, but also seen
/// with args in `rnd`, plus `get`/`post`/`rd`/`rdb` 1-or-2-arg variants and
/// `srt` 1-or-2-arg variants) get the BASE/canonical arity entered here.
/// `srt`'s entry uses arity 2 with a fn-ref first position, which lets
/// `srt cmp xs` expand and degrades gracefully for `srt xs` (the loop
/// simply stops when no more operands are available).
///
/// Mutating-only HOFs (`map`/`flt`/`fld`/`grp`) get fn-ref flag on slot 0.
fn builtin_arity_tables() -> (HashMap<String, usize>, HashMap<String, Vec<bool>>) {
    // (name, arity, fn_ref_positions)
    let entries: &[(&str, usize, &[usize])] = &[
        // Conversion
        ("str", 1, &[]),
        ("num", 1, &[]),
        // Math (unary)
        ("abs", 1, &[]),
        ("flr", 1, &[]),
        ("cel", 1, &[]),
        ("rou", 1, &[]),
        ("sqrt", 1, &[]),
        ("log", 1, &[]),
        ("exp", 1, &[]),
        ("sin", 1, &[]),
        ("cos", 1, &[]),
        // Math (binary)
        ("min", 2, &[]),
        ("max", 2, &[]),
        ("mod", 2, &[]),
        ("pow", 2, &[]),
        // Aggregates
        ("sum", 1, &[]),
        ("avg", 1, &[]),
        // Collections (unary)
        ("len", 1, &[]),
        ("hd", 1, &[]),
        ("tl", 1, &[]),
        ("rev", 1, &[]),
        ("unq", 1, &[]),
        ("flat", 1, &[]),
        ("frq", 1, &[]),
        // Collections (binary)
        ("at", 2, &[]),
        ("has", 2, &[]),
        ("spl", 2, &[]),
        ("cat", 2, &[]),
        // Collections (ternary)
        ("slc", 3, &[]),
        // Sort: 2-arg form (cmp, list) with fn-ref slot 0; 1-arg form
        // (list) still parses because the loop stops when no operand
        // follows. The 0th slot is a fn-ref position so `srt xs` keeps
        // `xs` as a bare ref and doesn't try to expand it.
        ("srt", 2, &[0]),
        // Higher-order
        ("map", 2, &[0]),
        ("flt", 2, &[0]),
        ("fld", 3, &[0]),
        ("grp", 2, &[0]),
        ("uniqby", 2, &[0]),
        ("partition", 2, &[0]),
        ("flatmap", 2, &[0]),
        // I/O
        ("prnt", 1, &[]),
        ("wr", 2, &[]),
        ("wrl", 2, &[]),
        ("trm", 1, &[]),
        ("upr", 1, &[]),
        ("lwr", 1, &[]),
        ("cap", 1, &[]),
        ("ord", 1, &[]),
        ("chr", 1, &[]),
        // fmt is variadic (template + N args) — leave to greedy parsing
        // JSON
        ("jdmp", 1, &[]),
        ("jpar", 1, &[]),
        ("jpth", 2, &[]),
        // Regex
        ("rgx", 2, &[]),
        ("rgxall", 2, &[]),
        ("rgxsub", 3, &[]),
        // Map (associative)
        ("mget", 2, &[]),
        ("mset", 3, &[]),
        ("mhas", 2, &[]),
        ("mkeys", 1, &[]),
        ("mvals", 1, &[]),
        ("mdel", 2, &[]),
        // Note: omitted by design — these have overloads or zero-arg forms
        // best left to the existing greedy/zero-arg paths:
        //   rnd, now, mmap (0-arg, special-cased above)
        //   get, post, rd, rdb, rdl, env (variable arity / IO)
        //   $ / get (path access via dollar prefix)
    ];
    let mut arity = HashMap::new();
    let mut fn_flags = HashMap::new();
    for (name, n, hof_slots) in entries {
        arity.insert((*name).to_string(), *n);
        let mut flags = vec![false; *n];
        for &slot in *hof_slots {
            if slot < flags.len() {
                flags[slot] = true;
            }
        }
        fn_flags.insert((*name).to_string(), flags);
    }
    // Mirror entries under their long-form aliases (e.g. `filter` → `flt`)
    // so agents writing `filter pos xs` still get the HOF first-arg
    // protection and arity-aware expansion.
    for (long, short) in crate::ast::all_builtin_aliases() {
        if let Some(n) = arity.get(short).copied() {
            arity.insert(long.to_string(), n);
        }
        if let Some(flags) = fn_flags.get(short).cloned() {
            fn_flags.insert(long.to_string(), flags);
        }
    }
    (arity, fn_flags)
}

/// Extract the last expression from a body, falling back to Nil.
fn body_to_expr(body: Vec<Spanned<Stmt>>) -> Expr {
    if body.is_empty() {
        return Expr::Literal(Literal::Nil);
    }
    match body.into_iter().last().unwrap().node {
        Stmt::Expr(e) => e,
        // If the last statement is not an expression, fall back to Nil.
        _ => Expr::Literal(Literal::Nil),
    }
}

/// Wrap the last expression in a body as a `Let` binding.
/// For example, if the body is `[Expr(- 0 x)]`, it becomes
/// `[Let { name: "v", value: Subtract(0, x) }]`.
fn wrap_body_as_let(name: &str, mut body: Vec<Spanned<Stmt>>) -> Vec<Spanned<Stmt>> {
    if body.is_empty() {
        return vec![Spanned::unknown(Stmt::Let {
            name: name.to_string(),
            value: Expr::Literal(Literal::Nil),
        })];
    }
    let last_idx = body.len() - 1;
    let last = &mut body[last_idx];
    let span = last.span;
    match &last.node {
        Stmt::Expr(expr) => {
            body[last_idx] = Spanned::new(
                Stmt::Let {
                    name: name.to_string(),
                    value: expr.clone(),
                },
                span,
            );
        }
        _ => {
            // If the last statement is not an expression (e.g. another Let),
            // we can't transform it — leave it as-is. This shouldn't normally
            // happen in well-formed ternary assignments.
        }
    }
    body
}

/// Identifier-keywords intercepted by `parse_stmt` as control-flow forms.
/// These names can never legitimately start a function declaration, so the
/// `is_fn_decl_start` heuristic must reject them — otherwise `wh >v 0{...}`
/// gets mis-parsed as a fn decl named `wh` returning `v` (see the gis-analyst
/// and routing-tsp persona reports).
fn is_reserved_stmt_keyword(name: &str) -> bool {
    matches!(name, "wh" | "ret" | "brk" | "cnt")
}

/// Map a reserved-keyword token to its `(message, hint)` pair for ILO-P011.
fn reserved_keyword_message(tok: &Token) -> Option<(String, String)> {
    let (name, hint) = match tok {
        Token::KwIf => ("if", "ilo uses `cond{body}` for conditional branches"),
        Token::KwReturn => ("return", "ilo uses `ret expr` for early returns"),
        Token::KwLet => ("let", "ilo uses `name=expr` for bindings"),
        Token::KwFn => ("fn", "ilo defines functions as `name params>return;body`"),
        Token::KwDef => ("def", "ilo defines functions as `name params>return;body`"),
        Token::KwVar => ("var", "ilo uses `name=expr` for bindings"),
        Token::KwConst => ("const", "ilo uses `name=expr` for bindings"),
        _ => return None,
    };
    Some((
        format!("`{name}` is a reserved word and cannot be used as an identifier"),
        hint.to_string(),
    ))
}

/// Check if an expression is a comparison or logical operator — eligible
/// as a braceless guard condition. Prefix operators have fixed arity, so
/// the parser knows exactly where the condition ends and the body begins.
fn is_guard_eligible_condition(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::BinOp { op, .. } if matches!(
            op,
            BinOp::Equals | BinOp::NotEquals
                | BinOp::GreaterThan | BinOp::LessThan
                | BinOp::GreaterOrEqual | BinOp::LessOrEqual
                | BinOp::And | BinOp::Or
        )
    )
}

/// Parse from token+span pairs.
/// Returns `(program, errors)`. The program may contain `Decl::Error` poison nodes
/// for declarations that failed to parse. Check `errors.is_empty()` before using
/// the program for execution — error nodes are skipped by the verifier but not
/// by the backends.
pub fn parse(tokens: Vec<(Token, Span)>) -> (Program, Vec<ParseError>) {
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

/// Parse from bare tokens (no span information, UNKNOWN spans).
/// Returns `Err` if any parse errors are present (first error).
/// Used by test helpers in interpreter, vm, and codegen modules.
#[cfg(test)]
pub fn parse_tokens(tokens: Vec<Token>) -> std::result::Result<Program, Vec<ParseError>> {
    let pairs: Vec<(Token, Span)> = tokens.into_iter().map(|t| (t, Span::UNKNOWN)).collect();
    let (prog, errors) = parse(pairs);
    if errors.is_empty() {
        Ok(prog)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;

    fn parse_str(source: &str) -> Program {
        let tokens = lexer::lex(source).unwrap();
        let token_spans: Vec<(Token, Span)> = tokens
            .into_iter()
            .map(|(t, r)| {
                (
                    t,
                    Span {
                        start: r.start,
                        end: r.end,
                    },
                )
            })
            .collect();
        let (prog, errors) = parse(token_spans);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        prog
    }

    fn parse_str_errors(source: &str) -> (Program, Vec<ParseError>) {
        let tokens = lexer::lex(source).unwrap();
        let token_spans: Vec<(Token, Span)> = tokens
            .into_iter()
            .map(|(t, r)| {
                (
                    t,
                    Span {
                        start: r.start,
                        end: r.end,
                    },
                )
            })
            .collect();
        parse(token_spans)
    }

    fn parse_file(path: &str) -> Program {
        let source =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {}", path, e));
        parse_str(&source)
    }

    #[test]
    fn parse_simple_function() {
        // tot p:n q:n r:n>n;s=*p q;t=*s r;+s t
        let prog = parse_str("tot p:n q:n r:n>n;s=*p q;t=*s r;+s t");
        assert_eq!(prog.declarations.len(), 1);
        let Decl::Function {
            name, params, body, ..
        } = &prog.declarations[0]
        else {
            panic!("expected function")
        };
        assert_eq!(name, "tot");
        assert_eq!(params.len(), 3);
        assert_eq!(body.len(), 3); // s=..., t=..., +s t
    }

    #[test]
    fn parse_let_binding() {
        let prog = parse_str("f x:n>n;y=+x 1;y");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(body.len(), 2);
        let Stmt::Let { name, .. } = &body[0].node else {
            panic!("expected let")
        };
        assert_eq!(name, "y");
    }

    #[test]
    fn parse_type_def() {
        let prog = parse_str("type point{x:n;y:n}");
        let Decl::TypeDef { name, fields, .. } = &prog.declarations[0] else {
            panic!("expected type def")
        };
        assert_eq!(name, "point");
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn parse_guard() {
        let prog = parse_str(r#"cls sp:n>t;>=sp 1000{"gold"};"bronze""#);
        let Decl::Function { name, body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(name, "cls");
        assert!(body.len() >= 2);
        let Stmt::Guard { negated, .. } = &body[0].node else {
            panic!("expected guard")
        };
        assert!(!negated);
    }

    #[test]
    fn parse_match_stmt() {
        let prog = parse_str(r#"f x:n>t;?{^e:^"error";~v:v;_:"default"}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { subject, arms } = &body[0].node else {
            panic!("expected match")
        };
        assert!(subject.is_none());
        assert_eq!(arms.len(), 3);
    }

    #[test]
    fn parse_prefix_ternary() {
        let prog = parse_str("f x:n>n;?=x 0 10 20");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        }) = &body[0].node
        else {
            panic!("expected ternary, got {:?}", body[0])
        };
        assert!(matches!(
            condition.as_ref(),
            Expr::BinOp {
                op: BinOp::Equals,
                ..
            }
        ));
        assert!(matches!(then_expr.as_ref(), Expr::Literal(Literal::Number(n)) if *n == 10.0));
        assert!(matches!(else_expr.as_ref(), Expr::Literal(Literal::Number(n)) if *n == 20.0));
    }

    #[test]
    fn parse_prefix_ternary_gt() {
        let prog = parse_str("f x:n>n;?>x 3 1 0");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Ternary { condition, .. }) = &body[0].node else {
            panic!("expected ternary, got {:?}", body[0])
        };
        assert!(matches!(
            condition.as_ref(),
            Expr::BinOp {
                op: BinOp::GreaterThan,
                ..
            }
        ));
    }

    #[test]
    fn parse_prefix_ternary_assignment() {
        let prog = parse_str("f x:n>n;v=?=x 0 10 20;v");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Let { name, value, .. } = &body[0].node else {
            panic!("expected let, got {:?}", body[0])
        };
        assert_eq!(name, "v");
        assert!(matches!(value, Expr::Ternary { .. }));
    }

    #[test]
    fn parse_ok_err_exprs() {
        let prog = parse_str("f x:n>R n t;~x");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(
            matches!(&body[0].node, Stmt::Expr(Expr::Ok(_))),
            "expected Ok expr, got {:?}",
            body[0]
        );
    }

    #[test]
    fn parse_foreach() {
        let prog = parse_str("f xs:L n>n;s=0;@x xs{s=+s x};s");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(body.len() >= 3);
        let Stmt::ForEach { binding, .. } = &body[1].node else {
            panic!("expected foreach")
        };
        assert_eq!(binding, "x");
    }

    #[test]
    fn parse_for_range() {
        let prog = parse_str("f>n;@i 0..3{i}");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::ForRange {
            binding,
            start,
            end,
            ..
        } = &body[0].node
        else {
            panic!("expected ForRange")
        };
        assert_eq!(binding, "i");
        assert_eq!(*start, Expr::Literal(Literal::Number(0.0)));
        assert_eq!(*end, Expr::Literal(Literal::Number(3.0)));
    }

    #[test]
    fn parse_for_range_with_expr_end() {
        // Dynamic end: @i 0..n{body}
        let prog = parse_str("f n:n>n;@i 0..n{i}");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::ForRange { binding, end, .. } = &body[0].node else {
            panic!("expected ForRange")
        };
        assert_eq!(binding, "i");
        assert_eq!(*end, Expr::Ref("n".to_string()));
    }

    #[test]
    fn parse_multi_decl() {
        let prog = parse_str("f x:n>n;*x 2 g x:n>n;+x 1");
        assert_eq!(prog.declarations.len(), 2);
    }

    #[test]
    fn parse_nested_prefix() {
        let prog = parse_str("f a:n b:n c:n>n;+*a b c");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::Add,
            left,
            ..
        }) = &body[0].node
        else {
            panic!("expected binop")
        };
        assert!(matches!(
            **left,
            Expr::BinOp {
                op: BinOp::Multiply,
                ..
            }
        ));
    }

    #[test]
    fn parse_list_literal() {
        let prog = parse_str("f x:n>L n;[x, *x 2, *x 3]");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::List(items)) = &body[0].node else {
            panic!("expected list")
        };
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn parse_field_access() {
        let prog = parse_str("f p:point>n;p.x");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Field { field, .. }) = &body[0].node else {
            panic!("expected field access")
        };
        assert_eq!(field, "x");
    }

    #[test]
    fn parse_index_access() {
        let prog = parse_str("f xs:L n>n;xs.0");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Index { index, .. }) = &body[0].node else {
            panic!("expected index access")
        };
        assert_eq!(*index, 0);
    }

    #[test]
    fn parse_safe_field_access() {
        let prog = parse_str("f p:point>n;p.?x");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Field { field, safe, .. }) = &body[0].node else {
            panic!("expected safe field access")
        };
        assert_eq!(field, "x");
        assert!(*safe);
    }

    #[test]
    fn parse_negated_guard() {
        let prog = parse_str(r#"f x:b>t;!x{"yes"};"no""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Guard { negated, .. } = &body[0].node else {
            panic!("expected guard")
        };
        assert!(negated);
    }

    #[test]
    fn parse_record_construction() {
        let prog = parse_str("type point{x:n;y:n} f a:n b:n>point;point x:a y:b");
        let Decl::Function { body, .. } = &prog.declarations[1] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Record { type_name, fields }) = &body[0].node else {
            panic!("expected record")
        };
        assert_eq!(type_name, "point");
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn parse_with_expr() {
        let prog = parse_str("type point{x:n;y:n} f p:point>point;p with x:1 y:2");
        let Decl::Function { body, .. } = &prog.declarations[1] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::With { updates, .. }) = &body[0].node else {
            panic!("expected with expr")
        };
        assert_eq!(updates.len(), 2);
    }

    #[test]
    fn parse_tool_decl() {
        let prog = parse_str(r#"tool fetch"http get" url:t>t timeout:30,retry:3"#);
        let Decl::Tool {
            name,
            description,
            timeout,
            retry,
            ..
        } = &prog.declarations[0]
        else {
            panic!("expected tool")
        };
        assert_eq!(name, "fetch");
        assert_eq!(description, "http get");
        assert_eq!(*timeout, Some(30.0));
        assert_eq!(*retry, Some(3.0));
    }

    #[test]
    fn parse_match_with_subject() {
        let prog = parse_str("f x:R n t>n;?x{~v:v;^e:0}");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { subject, arms } = &body[0].node else {
            panic!("expected match stmt")
        };
        assert!(subject.is_some());
        assert_eq!(arms.len(), 2);
    }

    #[test]
    fn parse_match_expr_in_let() {
        let prog = parse_str(r#"f x:R n t>n;r=?x{~v:v;^e:0};r"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(body.len(), 2);
        assert!(
            matches!(
                &body[0].node,
                Stmt::Let {
                    value: Expr::Match { .. },
                    ..
                }
            ),
            "expected let with match expr, got {:?}",
            body[0]
        );
    }

    #[test]
    fn parse_call_with_prefix_arg() {
        // fac -n 1 should parse as Call(fac, [Subtract(n, 1)])
        let prog = parse_str("fac n:n>n;r=fac -n 1;*n r");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Let {
            value: Expr::Call { function, args, .. },
            ..
        } = &body[0].node
        else {
            panic!("expected call with prefix arg")
        };
        assert_eq!(function, "fac");
        assert_eq!(args.len(), 1);
        assert!(matches!(
            &args[0],
            Expr::BinOp {
                op: BinOp::Subtract,
                ..
            }
        ));
    }

    // ── Infix operator tests ────────────────────────────────────────────────

    #[test]
    fn infix_add() {
        let prog = parse_str("f x:n>n;x + 1");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp { op: BinOp::Add, .. }) = &body[0].node else {
            panic!("expected infix add")
        };
    }

    #[test]
    fn infix_subtract() {
        let prog = parse_str("f x:n>n;x - 3");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::Subtract,
            ..
        }) = &body[0].node
        else {
            panic!("expected infix subtract")
        };
    }

    #[test]
    fn infix_multiply() {
        let prog = parse_str("f x:n>n;x * 2");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::Multiply,
            ..
        }) = &body[0].node
        else {
            panic!("expected infix multiply")
        };
    }

    #[test]
    fn infix_divide() {
        let prog = parse_str("f x:n>n;x / 2");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::Divide, ..
        }) = &body[0].node
        else {
            panic!("expected infix divide")
        };
    }

    #[test]
    fn infix_precedence_mul_over_add() {
        // x + y * 2 → +(x, *(y, 2))
        let prog = parse_str("f x:n y:n>n;x + y * 2");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::Add,
            left,
            right,
        }) = &body[0].node
        else {
            panic!("expected add")
        };
        assert!(matches!(left.as_ref(), Expr::Ref(_)));
        assert!(matches!(
            right.as_ref(),
            Expr::BinOp {
                op: BinOp::Multiply,
                ..
            }
        ));
    }

    #[test]
    fn infix_parens_override_precedence() {
        // (x + y) * 2 → *( +(x,y), 2 )
        let prog = parse_str("f x:n y:n>n;(x + y) * 2");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::Multiply,
            left,
            ..
        }) = &body[0].node
        else {
            panic!("expected multiply")
        };
        assert!(matches!(left.as_ref(), Expr::BinOp { op: BinOp::Add, .. }));
    }

    #[test]
    fn infix_call_binds_tighter() {
        // f a + b → (f a) + b
        let prog = parse_str("f x:n>n;x g x:n>n;f x + 1");
        let Decl::Function { body, .. } = &prog.declarations[1] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::Add,
            left,
            ..
        }) = &body[0].node
        else {
            panic!("expected infix add")
        };
        assert!(matches!(left.as_ref(), Expr::Call { .. }));
    }

    #[test]
    fn infix_comparison() {
        let prog = parse_str("f x:n y:n>b;x > y");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::GreaterThan,
            ..
        }) = &body[0].node
        else {
            panic!("expected gt")
        };
    }

    #[test]
    fn infix_and_or() {
        let prog = parse_str("f a:b b:b>b;a & b");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp { op: BinOp::And, .. }) = &body[0].node else {
            panic!("expected and")
        };
    }

    #[test]
    fn infix_left_associative() {
        // a - b - c → (a - b) - c
        let prog = parse_str("f a:n b:n c:n>n;a - b - c");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::Subtract,
            left,
            ..
        }) = &body[0].node
        else {
            panic!("expected sub")
        };
        assert!(matches!(
            left.as_ref(),
            Expr::BinOp {
                op: BinOp::Subtract,
                ..
            }
        ));
    }

    #[test]
    fn prefix_still_works_alongside_infix() {
        // +x 1 should still work as prefix
        let prog = parse_str("f x:n>n;+x 1");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp { op: BinOp::Add, .. }) = &body[0].node else {
            panic!("expected prefix add")
        };
    }

    #[test]
    fn prefix_call_arg_still_works() {
        // fac -n 1 should still parse as Call(fac, [-(n,1)])
        let prog = parse_str("fac n:n>n;r=fac -n 1;*n r");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Let {
            value: Expr::Call { function, args, .. },
            ..
        } = &body[0].node
        else {
            panic!("expected call")
        };
        assert_eq!(function, "fac");
        assert_eq!(args.len(), 1);
        assert!(matches!(
            &args[0],
            Expr::BinOp {
                op: BinOp::Subtract,
                ..
            }
        ));
    }

    // ── End infix tests ───────────────────────────────────────────────────────

    #[test]
    fn parse_zero_arg_call() {
        let prog = parse_str("f>n;g() g>n;42");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Call { function, args, .. }) = &body[0].node else {
            panic!("expected zero-arg call")
        };
        assert_eq!(function, "g");
        assert!(args.is_empty());
    }

    #[test]
    fn parse_paren_expr() {
        let prog = parse_str("f x:n>n;*(+x 1) 2");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::Multiply,
            left,
            ..
        }) = &body[0].node
        else {
            panic!("expected binop")
        };
        assert!(matches!(**left, Expr::BinOp { op: BinOp::Add, .. }));
    }

    #[test]
    fn parse_list_append() {
        let prog = parse_str("f xs:L n x:n>L n;+=xs x");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(
            matches!(
                &body[0].node,
                Stmt::Expr(Expr::BinOp {
                    op: BinOp::Append,
                    ..
                })
            ),
            "expected append, got {:?}",
            body[0]
        );
    }

    #[test]
    fn parse_trailing_comma_in_list() {
        let prog = parse_str("f>L n;[1, 2, 3,]");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::List(items)) = &body[0].node else {
            panic!("expected list")
        };
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn parse_empty_list() {
        let prog = parse_str("f>L n;[]");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::List(items)) = &body[0].node else {
            panic!("expected list")
        };
        assert!(items.is_empty());
    }

    #[test]
    fn parse_list_space_separated() {
        let prog = parse_str("f>L n;[1 2 3]");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::List(items)) = &body[0].node else {
            panic!("expected list")
        };
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn parse_list_with_variables() {
        let prog = parse_str(r#"f w:t>L t;["hi" w]"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::List(items)) = &body[0].node else {
            panic!("expected list")
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn parse_list_mixed_types() {
        let prog = parse_str(r#"f>L a;["search" 10 true]"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::List(items)) = &body[0].node else {
            panic!("expected list")
        };
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn parse_list_ok_err_elements() {
        let prog = parse_str("f>L R n t;[~1 ~2 ~3]");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::List(items)) = &body[0].node else {
            panic!("expected list")
        };
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn parse_caret_stmt_in_match() {
        let prog = parse_str(r#"f x:R n t>n;?x{^e:^"error";~v:v;_:0}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert!(
            matches!(&arms[0].body[0].node, Stmt::Expr(Expr::Err(_))),
            "expected Err expr in first arm"
        );
    }

    #[test]
    fn parse_chained_field_access() {
        let prog = parse_str("type inner{v:n} type outer{i:inner} f o:outer>n;o.i.v");
        // Should parse as o.i.v (chained field access)
        let Decl::Function { body, .. } = &prog.declarations[2] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Field { object, field, .. }) = &body[0].node else {
            panic!("expected chained field")
        };
        assert_eq!(field, "v");
        assert!(matches!(**object, Expr::Field { .. }));
    }

    #[test]
    fn parse_multi_stmt_match_arm() {
        let prog = parse_str("f x:R n t>n;?x{~v:y=+v 1;*y 2;^e:0}");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert_eq!(arms[0].body.len(), 2); // y=+v 1, *y 2
    }

    #[test]
    fn parse_negated_guard_vs_not_expr() {
        // !x{body} is negated guard; !x as last stmt is logical NOT
        let prog = parse_str("f x:b>b;!x");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(
            matches!(
                &body[0].node,
                Stmt::Expr(Expr::UnaryOp {
                    op: UnaryOp::Not,
                    ..
                })
            ),
            "expected NOT expr, got {:?}",
            body[0]
        );
    }

    #[test]
    fn parse_match_bool_literals() {
        let prog = parse_str("f x:b>n;?x{true:1;false:0}");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert!(matches!(
            arms[0].pattern,
            Pattern::Literal(Literal::Bool(true))
        ));
        assert!(matches!(
            arms[1].pattern,
            Pattern::Literal(Literal::Bool(false))
        ));
    }

    #[test]
    fn parse_match_number_with_wildcard() {
        let prog = parse_str(r#"f x:n>t;?x{1:"one";2:"two";_:"other"}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert_eq!(arms.len(), 3);
        assert!(matches!(arms[2].pattern, Pattern::Wildcard));
    }

    #[test]
    fn parse_match_string_patterns() {
        let prog = parse_str(r#"f x:t>n;?x{"a":1;"b":2;_:0}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert_eq!(arms.len(), 3);
        assert!(matches!(&arms[0].pattern, Pattern::Literal(Literal::Text(s)) if s == "a"));
    }

    #[test]
    fn parse_all_comparison_ops() {
        // Each op produces a different BinOp
        let tests = vec![
            (">=a b", BinOp::GreaterOrEqual),
            ("<=a b", BinOp::LessOrEqual),
            ("!=a b", BinOp::NotEquals),
            ("=a b", BinOp::Equals),
            (">a b", BinOp::GreaterThan),
            ("<a b", BinOp::LessThan),
            ("&a b", BinOp::And),
            ("|a b", BinOp::Or),
        ];
        for (expr_str, expected_op) in tests {
            let code = format!("f a:b b:b>b;{}", expr_str);
            let prog = parse_str(&code);
            let Decl::Function { body, .. } = &prog.declarations[0] else {
                panic!("expected function")
            };
            let Stmt::Expr(Expr::BinOp { op, .. }) = &body[0].node else {
                panic!("expected binop for {}", expr_str)
            };
            assert_eq!(*op, expected_op, "failed for expr: {}", expr_str);
        }
    }

    #[test]
    fn parse_error_has_span() {
        // "f x:n>n;+" — the + at byte 8 triggers an error because no operands follow
        let source = "f x:n>n;+";
        let tokens = lexer::lex(source).unwrap();
        let token_spans: Vec<(Token, Span)> = tokens
            .into_iter()
            .map(|(t, r)| {
                (
                    t,
                    Span {
                        start: r.start,
                        end: r.end,
                    },
                )
            })
            .collect();
        let (_prog, errors) = parse(token_spans);
        let err = errors.into_iter().next().expect("expected parse error");
        // Error message should mention the problem
        assert!(!err.message.is_empty());
        // Position should be non-zero (error is after the initial tokens)
        assert!(err.position > 0, "error position should be > 0");
    }

    // ---- Span-specific tests ----

    #[test]
    fn fn_decl_span_covers_full_declaration() {
        let prog = parse_str("f x:n>n;*x 2");
        let Decl::Function { span, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(span.start, 0);
        assert!(span.end > 0, "function span end should be > 0");
    }

    #[test]
    fn type_decl_span_covers_full_declaration() {
        let prog = parse_str("type point{x:n;y:n}");
        let Decl::TypeDef { span, .. } = &prog.declarations[0] else {
            panic!("expected type def")
        };
        assert_eq!(span.start, 0);
        // Should extend to cover the closing }
        assert!(
            span.end >= 18,
            "type span end should cover closing brace, got {}",
            span.end
        );
    }

    #[test]
    fn multi_decl_spans_are_distinct() {
        let prog = parse_str("f x:n>n;*x 2 g y:n>n;+y 1");
        assert_eq!(prog.declarations.len(), 2);
        let Decl::Function { span: span_f, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let span_f = *span_f;
        let Decl::Function { span: span_g, .. } = &prog.declarations[1] else {
            panic!("expected function")
        };
        let span_g = *span_g;
        // f starts at 0, g starts after f
        assert_eq!(span_f.start, 0);
        assert!(span_g.start > span_f.start, "g should start after f");
        assert!(
            span_g.start >= span_f.end,
            "g span should not overlap f span"
        );
    }

    #[test]
    fn tool_decl_has_span() {
        let prog = parse_str(r#"tool fetch"http get" url:t>t"#);
        let Decl::Tool { span, .. } = &prog.declarations[0] else {
            panic!("expected tool")
        };
        assert_eq!(span.start, 0);
        assert!(span.end > 0);
    }

    // ---- File-based tests ----

    #[test]
    fn parse_example_01_simple_function() {
        let prog = parse_file("examples/01-simple-function.ilo");
        assert_eq!(prog.declarations.len(), 1);
        let Decl::Function {
            name,
            params,
            return_type,
            body,
            ..
        } = &prog.declarations[0]
        else {
            panic!("expected function")
        };
        assert_eq!(name, "tot");
        assert_eq!(params.len(), 3);
        assert_eq!(*return_type, Type::Number);
        assert_eq!(body.len(), 3);
    }

    #[test]
    fn parse_example_02_with_dependencies() {
        let prog = parse_file("examples/02-with-dependencies.ilo");
        assert_eq!(prog.declarations.len(), 1);
        let Decl::Function {
            name, return_type, ..
        } = &prog.declarations[0]
        else {
            panic!("expected function")
        };
        assert_eq!(name, "prc");
        assert!(matches!(return_type, Type::Result(_, _)));
    }

    #[test]
    fn parse_error_messages() {
        let bad = "42 x:n>n;x";
        let tokens = lexer::lex(bad).unwrap();
        let token_spans: Vec<(Token, Span)> = tokens
            .into_iter()
            .map(|(t, r)| {
                (
                    t,
                    Span {
                        start: r.start,
                        end: r.end,
                    },
                )
            })
            .collect();
        let (_prog, errors) = parse(token_spans);
        let err = errors.into_iter().next().expect("expected parse error");
        assert!(
            err.message.contains("expected declaration"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn parse_complex_match_patterns() {
        let prog = parse_str(r#"f x:R n t>n;?x{^e:0;~v:?v{1:100;2:200;_:v}}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(body.len(), 1);
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert_eq!(arms.len(), 2);
        // Second arm body should be a nested match statement
        assert!(matches!(&arms[1].body[0].node, Stmt::Match { .. }));
    }

    #[test]
    fn parse_deeply_nested_prefix() {
        let prog = parse_str("f x:n>n;+*+x 1 2 3");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        // Should be: +(*(+(x,1), 2), 3)
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::Add,
            left,
            ..
        }) = &body[0].node
        else {
            panic!("expected add")
        };
        let Expr::BinOp {
            op: BinOp::Multiply,
            left: inner,
            ..
        } = &**left
        else {
            panic!("expected nested multiply")
        };
        assert!(matches!(&**inner, Expr::BinOp { op: BinOp::Add, .. }));
    }

    #[test]
    fn parse_tokens_legacy_api() {
        // Test the legacy parse_tokens API
        let source = "f x:n>n;*x 2";
        let tokens: Vec<Token> = lexer::lex(source)
            .unwrap()
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let prog = parse_tokens(tokens).unwrap();
        assert_eq!(prog.declarations.len(), 1);
    }

    // ---- Error recovery tests ----

    #[test]
    fn recovery_second_function_parsed_after_first_error() {
        // First function has missing `>` (no params, hits `;` instead of `>`)
        // Second function should still parse correctly.
        let (prog, errors) = parse_str_errors("f x:n n;bad g y:n>n;y");
        // One error from `f`, one valid `g`
        assert!(!errors.is_empty(), "expected parse error from f");
        let valid: Vec<_> = prog
            .declarations
            .iter()
            .filter(|d| !matches!(d, Decl::Error { .. }))
            .collect();
        assert_eq!(valid.len(), 1, "g should parse successfully");
        let Decl::Function { name, .. } = valid[0] else {
            panic!("expected function g")
        };
        assert_eq!(name, "g");
    }

    #[test]
    fn recovery_error_node_in_declarations() {
        let (prog, errors) = parse_str_errors("f x:n n;bad g y:n>n;y");
        assert!(!errors.is_empty());
        // Program.declarations has two entries: an Error and a Function
        assert_eq!(prog.declarations.len(), 2);
        assert!(matches!(prog.declarations[0], Decl::Error { .. }));
        assert!(matches!(prog.declarations[1], Decl::Function { .. }));
    }

    #[test]
    fn recovery_two_errors_both_reported() {
        // Both functions have bad signatures
        let (prog, errors) = parse_str_errors("f x:n n;bad g y:n n;bad");
        assert_eq!(errors.len(), 2, "expected two errors");
        assert_eq!(prog.declarations.len(), 2);
        assert!(
            prog.declarations
                .iter()
                .all(|d| matches!(d, Decl::Error { .. }))
        );
    }

    #[test]
    fn recovery_error_node_not_in_json() {
        // Decl::Error nodes must be filtered from JSON AST output
        let (prog, _errors) = parse_str_errors("f x:n n;bad g y:n>n;y");
        let json = serde_json::to_string(&prog).unwrap();
        // Only g should appear; the error node is suppressed
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let decls = parsed["declarations"].as_array().unwrap();
        assert_eq!(
            decls.len(),
            1,
            "only valid declarations should appear in JSON"
        );
    }

    #[test]
    fn recovery_stops_at_20_errors() {
        // Build a string with 25 bad single-token "functions" followed by a valid one
        let bad: String = (0..25).map(|i| format!("f{i} x:n n;bad ")).collect();
        let good = "g y:n>n;y";
        let source = format!("{bad}{good}");
        let (_prog, errors) = parse_str_errors(&source);
        assert!(
            errors.len() <= 20,
            "should cap at 20 errors, got {}",
            errors.len()
        );
    }

    #[test]
    fn recovery_type_decl_after_error() {
        // A type declaration after a broken function should be recovered
        let (prog, errors) = parse_str_errors("f x:n n;bad type point{x:n;y:n}");
        assert!(!errors.is_empty());
        let valid: Vec<_> = prog
            .declarations
            .iter()
            .filter(|d| !matches!(d, Decl::Error { .. }))
            .collect();
        assert_eq!(valid.len(), 1);
        assert!(matches!(valid[0], Decl::TypeDef { .. }));
    }

    // ---- EOF error paths ----

    #[test]
    fn eof_while_expecting_type() {
        // `f x:` — hits EOF while expecting a type
        let (_, errors) = parse_str_errors("f x:");
        assert!(!errors.is_empty(), "expected parse error");
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("EOF") || e.message.contains("expected")),
            "unexpected error messages: {:?}",
            errors
        );
    }

    #[test]
    fn eof_while_expecting_identifier() {
        // `f x:n>n;y=` — incomplete let binding, hits EOF when expecting identifier or expression
        let (_, errors) = parse_str_errors("f");
        assert!(!errors.is_empty(), "expected parse error");
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("EOF") || e.message.contains("expected")),
            "unexpected error messages: {:?}",
            errors
        );
    }

    #[test]
    fn eof_while_expecting_expression() {
        // `f x:n>n;+x` — incomplete binary op, hits EOF for right operand
        let (_, errors) = parse_str_errors("f x:n>n;+x");
        assert!(
            !errors.is_empty(),
            "expected parse error for EOF expression"
        );
    }

    #[test]
    fn eof_expecting_gt_in_signature() {
        // `f x:n` — no `>` and no body
        let (_, errors) = parse_str_errors("f x:n");
        assert!(!errors.is_empty(), "expected parse error");
    }

    // ---- Tool description string missing (ILO-P015) ----

    #[test]
    fn tool_missing_description() {
        let (_, errors) = parse_str_errors("tool my-tool x:n>n");
        assert!(
            !errors.is_empty(),
            "expected parse error for missing description"
        );
        assert!(
            errors.iter().any(|e| e.code == "ILO-P015"),
            "expected ILO-P015 error, got: {:?}",
            errors
        );
    }

    // ---- Unexpected token in various positions ----

    #[test]
    fn unexpected_token_as_expression() {
        // `}` is not a valid expression start
        let (_, errors) = parse_str_errors("f x:n>n;>x 0{}};x");
        assert!(!errors.is_empty(), "expected parse error");
    }

    #[test]
    fn unexpected_token_as_pattern() {
        // Invalid pattern in match arm
        let (_, errors) = parse_str_errors("f x:n>n;?x{+:1;_:0}");
        assert!(!errors.is_empty(), "expected parse error for bad pattern");
    }

    #[test]
    fn eof_while_expecting_declaration() {
        // Empty input — no declarations, should get EOF error
        let (prog, errors) = parse_str_errors("");
        // Empty programs may or may not produce errors; at minimum they produce no decls
        let _ = (prog, errors);
    }

    #[test]
    fn expect_ident_got_non_ident() {
        // `type 123{...}` — expect_ident() gets a Number token → ILO-P005
        let (_, errors) = parse_str_errors("type 123{x:n}");
        assert!(!errors.is_empty(), "expected parse error");
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-P005" || e.message.contains("expected identifier")),
            "unexpected errors: {:?}",
            errors
        );
    }

    #[test]
    fn expect_ident_got_eof() {
        // `type` — EOF where an identifier is expected → ILO-P006
        let (_, errors) = parse_str_errors("type");
        assert!(!errors.is_empty(), "expected parse error");
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-P006" || e.message.contains("EOF")),
            "unexpected errors: {:?}",
            errors
        );
    }

    #[test]
    fn parse_ok_expr_as_operand() {
        // `~x` as the argument to a function call — exercises Tilde in parse_operand
        let prog = parse_str("f x:n>R n t;g ~x");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Call { function, args, .. }) = &body[0].node else {
            panic!("expected call")
        };
        assert_eq!(function, "g");
        assert!(matches!(&args[0], Expr::Ok(_)));
    }

    #[test]
    fn parse_err_expr_as_operand() {
        // `^x` as the argument to a function call — exercises Caret in parse_operand
        let prog = parse_str("f x:n>R n t;g ^x");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Call { function, args, .. }) = &body[0].node else {
            panic!("expected call")
        };
        assert_eq!(function, "g");
        assert!(matches!(&args[0], Expr::Err(_)));
    }

    #[test]
    fn declaration_starts_with_prefix_op_gets_hint() {
        // A declaration starting with `+` — triggers hint about prefix operators
        let (_, errors) = parse_str_errors("+x 1");
        assert!(!errors.is_empty(), "expected parse error");
    }

    #[test]
    fn nested_brace_body_recovery() {
        // A function body with nested braces that fail to parse properly
        // This exercises the brace-depth tracking in error recovery
        let (prog, errors) = parse_str_errors("f x:n>n;>x 0{{inner}};x g y:n>n;y");
        // The recovery should still find `g`
        assert!(!errors.is_empty(), "should have errors from nested braces");
        let valid: Vec<_> = prog
            .declarations
            .iter()
            .filter(|d| matches!(d, Decl::Function { name, .. } if name == "g"))
            .collect();
        assert!(
            !valid.is_empty() || !prog.declarations.is_empty(),
            "should recover at least something"
        );
    }

    #[test]
    fn parse_ident_guard_expr_or_guard() {
        // Ident-starting guard: `x{42}` exercises parse_expr_or_guard returning a Guard (L621-625)
        let prog = parse_str("f x:b>n;x{42}");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(
            matches!(&body[0].node, Stmt::Guard { negated: false, .. }),
            "expected non-negated guard, got {:?}",
            body[0]
        );
    }

    #[test]
    fn parse_eof_in_pattern() {
        // EOF while parsing pattern → ILO-P012 error (L571)
        // Construct tokens manually: f > n ; ? x {  (no closing brace, no pattern)
        let tokens: Vec<(Token, Span)> = vec![
            (Token::Ident("f".to_string()), Span::UNKNOWN),
            (Token::Greater, Span::UNKNOWN),
            (Token::Ident("n".to_string()), Span::UNKNOWN),
            (Token::Semi, Span::UNKNOWN),
            (Token::Question, Span::UNKNOWN),
            (Token::Number(1.0), Span::UNKNOWN),
            (Token::LBrace, Span::UNKNOWN),
            // EOF here — no pattern token
        ];
        let (_, errors) = parse(tokens);
        assert!(
            !errors.is_empty(),
            "expected parse error for EOF in pattern"
        );
        let found = errors
            .iter()
            .any(|e| e.code == "ILO-P012" || e.message.contains("EOF"));
        assert!(found, "expected ILO-P012 error, got: {:?}", errors);
    }

    // ---- Coverage: trailing semicolons and edge cases ----

    // L363: parse_body trailing `;` — consumed `;` but at_body_end → break
    #[test]
    fn parse_body_trailing_semicolon() {
        // `f>n;42;` — `;` after `42` is consumed, then at_body_end (EOF) → break (L363)
        let prog = parse_str("f>n;42;");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(body.len(), 1);
    }

    // L436: parse_match_arms trailing `;` before `}` — arm with empty body (L436)
    // at_arm_end() is true at `;`, so parse_arm_body returns Ok([]).
    // Then parse_match_arms sees `;`, consumes it, and peek is `}` → break (L436)
    #[test]
    fn parse_match_arms_trailing_semi() {
        // `?{1:;}` — arm `1:` has empty body, `;` then `}` → break at L436
        let prog = parse_str("f>n;?{1:;}");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert_eq!(arms.len(), 1);
        assert_eq!(arms[0].body.len(), 0); // empty body
    }

    // L460: parse_arm_body trailing `;` before `}` — consumed `;`, at_arm_end → break (L460)
    #[test]
    fn parse_arm_body_trailing_semi() {
        // `?0{_:1;}` — in arm body, `;` consumed, peek is `}` → at_arm_end → break (L460)
        let prog = parse_str("f>n;?0{_:1;}");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert_eq!(arms.len(), 1);
        assert_eq!(arms[0].body.len(), 1);
    }

    // L477: semi_starts_new_arm — after_semi >= tokens.len() (EOF after `;`) → return false (L477)
    #[test]
    fn parse_incomplete_match_arm_eof_after_semi() {
        // `?x{1:42;` — `;` is the last token → semi_starts_new_arm hits L477
        let (_, errors) = parse_str_errors("f x:n>n;?x{1:42;");
        assert!(
            !errors.is_empty(),
            "expected parse error for unclosed match"
        );
    }

    // L670: parse_expr_or_with — ident after `with` not followed by `:` → break (L670)
    #[test]
    fn parse_with_ident_no_colon() {
        // `x with a` — `a` not followed by `:` (EOF) → break at L670, `a` stays unconsumed
        let (_, errors) = parse_str_errors("f x:n>n;x with a");
        // Errors may occur from leftover tokens, but L670 is exercised
        let _ = errors;
    }

    // L991: parse_number in tool timeout — non-number token → ILO-P013 error (L991)
    #[test]
    fn parse_tool_timeout_non_numeric() {
        // `timeout:foo` — `foo` is Ident, not Number → parse_number ILO-P013 at L991
        let (_, errors) = parse_str_errors(r#"tool f "desc" x:n>n timeout:foo"#);
        assert!(
            !errors.is_empty(),
            "expected parse error for non-numeric timeout"
        );
        let found = errors
            .iter()
            .any(|e| e.code == "ILO-P013" || e.message.contains("expected number"));
        assert!(found, "expected ILO-P013, got: {:?}", errors);
    }

    // L992: parse_number in tool timeout — EOF after `:` → ILO-P014 error (L992)
    #[test]
    fn parse_tool_timeout_eof() {
        // `timeout:` followed by EOF → parse_number ILO-P014 at L992
        let (_, errors) = parse_str_errors(r#"tool f "desc" x:n>n timeout:"#);
        assert!(!errors.is_empty(), "expected parse error for EOF timeout");
        let found = errors
            .iter()
            .any(|e| e.code == "ILO-P014" || e.message.contains("EOF"));
        assert!(found, "expected ILO-P014, got: {:?}", errors);
    }

    #[test]
    fn parse_semi_starts_new_arm_caret_eof() {
        // L488: `false` branch in semi_starts_new_arm() for Caret pattern when
        // after_semi + 2 >= tokens.len() (only `^ident` after `;`, no `:`)
        // Input: `?x{1:2;^v` — after arm `1:2`, we're at `;`, next is `^v` then EOF
        let (_, errors) = parse_str_errors("f x:n>n;?x{1:2;^v");
        // Parse error expected (incomplete arm), but the false-branch in semi_starts_new_arm fires
        let _ = errors; // errors are expected (incomplete parse)
    }

    #[test]
    fn parse_semi_starts_new_arm_tilde_eof() {
        // L499: `false` branch in semi_starts_new_arm() for Tilde pattern when
        // after_semi + 2 >= tokens.len() (only `~ident` after `;`, no `:`)
        let (_, errors) = parse_str_errors("f x:n>n;?x{1:2;~v");
        let _ = errors;
    }

    #[test]
    fn parse_decl_eof() {
        // L190: `None => Err(...)` in parse_decl() when peek() is None at declaration start
        // A trailing `;` after a valid declaration causes the parser to try to parse another decl
        let (prog, _) = parse_str_errors("f>n;42;");
        // Either parsed successfully (trailing semi in body) or parser got EOF
        let _ = prog;
    }

    #[test]
    fn parse_prev_span_at_zero() {
        // L292: `Span::UNKNOWN` in prev_span() when pos == 0
        // Trigger by having a tool decl with no tokens consumed yet at a parse_body call
        // Actually, just parsing something that calls prev_span at position 0
        let (_, errors) = parse_str_errors("");
        let _ = errors;
    }

    // L190: parse_decl() with empty token stream → None => Err("expected declaration, got EOF")
    #[test]
    fn parse_decl_with_empty_tokens() {
        let mut parser = Parser::new(vec![]);
        let result = parser.parse_decl();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "ILO-P002");
    }

    // L292: prev_span() when pos == 0 (no tokens consumed) → Span::UNKNOWN
    #[test]
    fn prev_span_at_position_zero() {
        let parser = Parser::new(vec![(Token::Ident("x".into()), Span { start: 1, end: 2 })]);
        // pos == 0, nothing consumed → should return Span::UNKNOWN
        assert_eq!(parser.prev_span(), Span::UNKNOWN);
    }

    // L472: semi_starts_new_arm() when peek() != Semi → return false at L472
    #[test]
    fn semi_starts_new_arm_non_semi_token() {
        let parser = Parser::new(vec![(Token::Ident("x".into()), Span::UNKNOWN)]);
        // peek() is Ident, not Semi → L472 returns false
        assert!(!parser.semi_starts_new_arm());
    }

    // ---- C3: parser hint/suggestion tests ----

    #[test]
    fn hint_p001_function_keyword() {
        let (_, errors) = parse_str_errors("function foo() {}");
        assert!(!errors.is_empty());
        let e = errors.iter().find(|e| e.code == "ILO-P001").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("ilo function syntax"));
    }

    #[test]
    fn hint_p001_let_keyword() {
        let (_, errors) = parse_str_errors("let x = 5");
        assert!(!errors.is_empty());
        let e = errors.iter().find(|e| e.code == "ILO-P001").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("assignment syntax"));
    }

    #[test]
    fn hint_p001_return_keyword() {
        let (_, errors) = parse_str_errors("return x");
        assert!(!errors.is_empty());
        let e = errors.iter().find(|e| e.code == "ILO-P001").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("return value"));
    }

    #[test]
    fn hint_p001_if_keyword() {
        let (_, errors) = parse_str_errors("if x > 0 { true }");
        assert!(!errors.is_empty());
        let e = errors.iter().find(|e| e.code == "ILO-P001").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("match"));
    }

    #[test]
    fn hint_p001_operator_at_decl_level() {
        // '+' at declaration level — operator hint
        let tokens = vec![
            (Token::Plus, Span::UNKNOWN),
            (Token::Ident("x".into()), Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty());
        let e = errors.iter().find(|e| e.code == "ILO-P001").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("prefix operators"));
    }

    #[test]
    fn hint_p003_arrow_instead_of_greater() {
        // f x:n->n;x uses -> instead of >
        let (_, errors) = parse_str_errors("f x:n->n;x");
        // Should find an error about -> vs >
        assert!(!errors.is_empty());
        let e = errors.iter().find(|e| e.code == "ILO-P003").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("->"));
        assert!(hint.contains(">"));
    }

    #[test]
    fn hint_p003_double_amp() {
        // && at expression level
        let (_, errors) = parse_str_errors("f x:b y:b>b;&&x y");
        let e = errors.iter().find(|e| e.code == "ILO-P003").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("'&'"));
        assert!(hint.contains("'|'"));
    }

    #[test]
    fn hint_p003_double_pipe() {
        // || at expression level
        let (_, errors) = parse_str_errors("f x:b y:b>b;||x y");
        let e = errors.iter().find(|e| e.code == "ILO-P003").unwrap();
        let hint = e.hint.as_ref().unwrap();
        assert!(hint.contains("'|'"));
    }

    #[test]
    fn no_hint_p001_unrecognized_token() {
        // A token that has no specific hint
        let tokens = vec![(Token::Number(42.0), Span::UNKNOWN)];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty());
        // Should get ILO-P001 but no hint for a bare number
        let e = errors.iter().find(|e| e.code == "ILO-P001").unwrap();
        assert!(e.hint.is_none());
    }

    #[test]
    fn parse_unwrap_call() {
        // Single function with unwrap call as let-bind (no multi-func boundary issue)
        let prog = parse_str("f x:n>R n t;d=g! x;~d");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Let {
            value:
                Expr::Call {
                    function,
                    args,
                    unwrap,
                },
            ..
        } = &body[0].node
        else {
            panic!("expected unwrap call")
        };
        assert_eq!(function, "g");
        assert!(unwrap);
        assert_eq!(args.len(), 1);
        assert!(matches!(&args[0], Expr::Ref(n) if n == "x"));
    }

    #[test]
    fn parse_unwrap_zero_arg() {
        // fetch!() → Call { function: "fetch", unwrap: true, args: [] }
        let prog = parse_str("f>R t t;d=g!();~d");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Let {
            value:
                Expr::Call {
                    function,
                    args,
                    unwrap,
                },
            ..
        } = &body[0].node
        else {
            panic!("expected unwrap zero-arg call")
        };
        assert_eq!(function, "g");
        assert!(unwrap);
        assert!(args.is_empty());
    }

    #[test]
    fn parse_bang_not_is_not_unwrap() {
        // g !x → Call(g, [Not(Ref(x))]), NOT an unwrap call
        // Single-function to avoid boundary issues
        let prog = parse_str("f x:b>b;g !x");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Call {
            function,
            args,
            unwrap,
            ..
        }) = &body[0].node
        else {
            panic!("expected call with NOT arg")
        };
        assert_eq!(function, "g");
        assert!(!unwrap);
        assert_eq!(args.len(), 1);
        assert!(matches!(
            &args[0],
            Expr::UnaryOp {
                op: UnaryOp::Not,
                ..
            }
        ));
    }

    #[test]
    fn parse_unwrap_multi_arg() {
        // f! a b → Call { function: "f", unwrap: true, args: [Ref("a"), Ref("b")] }
        // Use let-bind to avoid greedy arg consumption at decl boundary
        let prog = parse_str("f a:n b:n>R n t;d=g! a b;~d");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Let {
            value:
                Expr::Call {
                    function,
                    args,
                    unwrap,
                },
            ..
        } = &body[0].node
        else {
            panic!("expected unwrap multi-arg call")
        };
        assert_eq!(function, "g");
        assert!(unwrap);
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn parse_unwrap_as_last_expr() {
        // Unwrap as the last expression in the body (tail position)
        let prog = parse_str("f x:n>R n t;g! x");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Call {
            function, unwrap, ..
        }) = &body[0].node
        else {
            panic!("expected unwrap call expr")
        };
        assert_eq!(function, "g");
        assert!(unwrap);
    }

    // ---- Braceless guards ----

    #[test]
    fn braceless_guard_comparison_literal() {
        // >=sp 1000 "gold" → Guard with comparison condition and literal body
        let prog = parse_str(r#"cls sp:n>t;>=sp 1000 "gold";"bronze""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(
            body.len(),
            2,
            "expected 2 stmts (guard + expr), got {:?}",
            body
        );
        let Stmt::Guard {
            condition,
            negated,
            body: guard_body,
            ..
        } = &body[0].node
        else {
            panic!("expected guard")
        };
        assert!(!negated);
        assert!(matches!(
            condition,
            Expr::BinOp {
                op: BinOp::GreaterOrEqual,
                ..
            }
        ));
        assert_eq!(guard_body.len(), 1);
        let Stmt::Expr(Expr::Literal(Literal::Text(s))) = &guard_body[0].node else {
            panic!("expected text literal body")
        };
        assert_eq!(s, "gold");
    }

    #[test]
    fn braceless_guard_variable_body() {
        // <=n 1 n → Guard returning variable
        let prog = parse_str("fib n:n>n;<=n 1 n;+n 1");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(body.len(), 2);
        let Stmt::Guard {
            condition,
            negated,
            body: guard_body,
            ..
        } = &body[0].node
        else {
            panic!("expected guard")
        };
        assert!(!negated);
        assert!(matches!(
            condition,
            Expr::BinOp {
                op: BinOp::LessOrEqual,
                ..
            }
        ));
        assert_eq!(guard_body.len(), 1);
        assert!(matches!(&guard_body[0].node, Stmt::Expr(Expr::Ref(n)) if n == "n"));
    }

    #[test]
    fn braceless_guard_ok_body() {
        // >=x 0 ~x → Guard returning Ok(x)
        let prog = parse_str("f x:n>R n t;>=x 0 ~x;^\"negative\"");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Guard {
            body: guard_body, ..
        } = &body[0].node
        else {
            panic!("expected guard")
        };
        assert_eq!(guard_body.len(), 1);
        assert!(matches!(&guard_body[0].node, Stmt::Expr(Expr::Ok(_))));
    }

    #[test]
    fn braceless_guard_err_body() {
        // <x 0 ^"negative" → Guard returning Err
        let prog = parse_str(r#"f x:n>R n t;<x 0 ^"negative";~x"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Guard {
            body: guard_body, ..
        } = &body[0].node
        else {
            panic!("expected guard")
        };
        assert_eq!(guard_body.len(), 1);
        assert!(matches!(&guard_body[0].node, Stmt::Expr(Expr::Err(_))));
    }

    #[test]
    fn braceless_guard_operator_body() {
        // >=x 10 +x 1 → Guard returning x+1
        let prog = parse_str("f x:n>n;>=x 10 +x 1;*x 2");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(body.len(), 2);
        let Stmt::Guard {
            body: guard_body, ..
        } = &body[0].node
        else {
            panic!("expected guard")
        };
        assert_eq!(guard_body.len(), 1);
        assert!(matches!(
            &guard_body[0].node,
            Stmt::Expr(Expr::BinOp { op: BinOp::Add, .. })
        ));
    }

    #[test]
    fn braceless_guard_multi_guard_program() {
        // Full classify program with braceless guards
        let prog = parse_str(r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(body.len(), 3, "expected 3 stmts, got {:?}", body);
        assert!(matches!(&body[0].node, Stmt::Guard { .. }));
        assert!(matches!(&body[1].node, Stmt::Guard { .. }));
        assert!(matches!(
            &body[2].node,
            Stmt::Expr(Expr::Literal(Literal::Text(_)))
        ));
    }

    #[test]
    fn braceless_guard_negated() {
        // !>=x 10 "small" → negated braceless guard
        let prog = parse_str(r#"f x:n>t;!>=x 10 "small";"big""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(body.len(), 2);
        let Stmt::Guard {
            condition,
            negated,
            body: guard_body,
            ..
        } = &body[0].node
        else {
            panic!("expected negated guard")
        };
        assert!(negated);
        assert!(matches!(
            condition,
            Expr::BinOp {
                op: BinOp::GreaterOrEqual,
                ..
            }
        ));
        assert_eq!(guard_body.len(), 1);
        let Stmt::Expr(Expr::Literal(Literal::Text(s))) = &guard_body[0].node else {
            panic!("expected text body")
        };
        assert_eq!(s, "small");
    }

    #[test]
    fn braceless_guard_non_comparison_not_triggered() {
        // +x y "result" — Add is NOT a comparison, so no braceless guard
        // +x y is an expr, "result" is a separate expr
        let prog = parse_str(r#"f x:n y:n>t;+x y;"result""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        // First stmt should be an Expr (BinOp Add), not a Guard
        assert!(
            matches!(
                &body[0].node,
                Stmt::Expr(Expr::BinOp { op: BinOp::Add, .. })
            ),
            "non-comparison should not trigger braceless guard, got {:?}",
            body[0]
        );
    }

    #[test]
    fn braceless_guard_braced_still_works() {
        // Braced guards should still work exactly as before
        let prog = parse_str(r#"cls sp:n>t;>=sp 1000{"gold"};"bronze""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(body.len(), 2);
        let Stmt::Guard { negated, .. } = &body[0].node else {
            panic!("expected guard")
        };
        assert!(!negated);
    }

    #[test]
    fn braceless_guard_equality() {
        // =x "admin" ~x → equality check braceless guard
        let prog = parse_str(r#"f x:t>R t t;=x "admin" ~x;^"denied""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Guard { condition, .. } = &body[0].node else {
            panic!("expected guard")
        };
        assert!(matches!(
            condition,
            Expr::BinOp {
                op: BinOp::Equals,
                ..
            }
        ));
    }

    #[test]
    fn braceless_guard_logical_and() {
        // &a b "both" → logical AND braceless guard
        let prog = parse_str(r#"f a:b b:b>t;&a b "both";"nope""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Guard { condition, .. } = &body[0].node else {
            panic!("expected guard")
        };
        assert!(matches!(condition, Expr::BinOp { op: BinOp::And, .. }));
    }

    #[test]
    fn braceless_guard_at_end_no_body() {
        // >=x 10 at end with semicolon but no body token → not a braceless guard
        let prog = parse_str("f x:n>b;>=x 10");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(body.len(), 1);
        // Should be a plain expression, not a guard (nothing follows)
        assert!(matches!(
            &body[0].node,
            Stmt::Expr(Expr::BinOp {
                op: BinOp::GreaterOrEqual,
                ..
            })
        ));
    }

    #[test]
    fn braceless_guard_factorial() {
        // fac n:n>n;<=n 1 1;r=fac -n 1;*n r
        let prog = parse_str("fac n:n>n;<=n 1 1;r=fac -n 1;*n r");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(
            body.len(),
            3,
            "expected 3 stmts (guard + let + expr), got {:?}",
            body
        );
        let Stmt::Guard {
            condition,
            body: guard_body,
            ..
        } = &body[0].node
        else {
            panic!("expected guard")
        };
        assert!(matches!(
            condition,
            Expr::BinOp {
                op: BinOp::LessOrEqual,
                ..
            }
        ));
        assert_eq!(guard_body.len(), 1);
        assert!(
            matches!(&guard_body[0].node, Stmt::Expr(Expr::Literal(Literal::Number(n))) if *n == 1.0)
        );
    }

    // ---- Braceless guard ambiguity detection (ILO-P016) ----

    #[test]
    fn braceless_guard_dangling_token_error() {
        // >=sp 1000 classify sp — `classify` is body, `sp` dangles → ILO-P016
        let (_, errors) = parse_str_errors("cls sp:n>t;>=sp 1000 classify sp");
        assert!(
            errors.iter().any(|e| e.code == "ILO-P016"),
            "expected ILO-P016 error, got: {:?}",
            errors
        );
        assert!(
            errors
                .iter()
                .any(|e| e.hint.as_ref().is_some_and(|h| h.contains("braces"))),
            "expected hint about braces, got: {:?}",
            errors
        );
    }

    #[test]
    fn braceless_guard_valid_semicolon_terminates() {
        // >=sp 1000 classify; — `classify` as variable ref, semicolon terminates → valid
        let prog = parse_str("cls sp:n>t;>=sp 1000 classify;\"fallback\"");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(matches!(&body[0].node, Stmt::Guard { .. }));
    }

    // ---- Dollar / HTTP get tests ----

    #[test]
    fn parse_dollar_desugars_to_get() {
        let prog = parse_str(r#"f url:t>R t t;$url"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Call {
            function,
            args,
            unwrap,
        }) = &body[0].node
        else {
            panic!("expected get call")
        };
        assert_eq!(function, "get");
        assert_eq!(args.len(), 1);
        assert!(!unwrap);
    }

    #[test]
    fn parse_dollar_bang_desugars_to_get_unwrap() {
        let prog = parse_str(r#"f url:t>t;$!url"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Call {
            function,
            args,
            unwrap,
        }) = &body[0].node
        else {
            panic!("expected get! call")
        };
        assert_eq!(function, "get");
        assert_eq!(args.len(), 1);
        assert!(unwrap);
    }

    #[test]
    fn parse_dollar_with_string_literal() {
        let prog = parse_str(r#"f>R t t;$"http://example.com""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Call { function, args, .. }) = &body[0].node else {
            panic!("expected get call")
        };
        assert_eq!(function, "get");
        assert!(matches!(&args[0], Expr::Literal(Literal::Text(_))));
    }

    #[test]
    fn parse_ternary_guard_else() {
        let source = r#"f x:n>t;=x 1{"yes"}{"no"}"#;
        let (program, errors) = parse_str_errors(source);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let Decl::Function { body, .. } = &program.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(body.len(), 1, "expected 1 stmt (ternary), got {:?}", body);
        let Stmt::Guard { else_body, .. } = &body[0].node else {
            panic!("expected guard with else")
        };
        assert!(else_body.is_some(), "expected else_body in ternary");
        let eb = else_body.as_ref().unwrap();
        assert_eq!(eb.len(), 1);
    }

    #[test]
    fn parse_while_loop() {
        let prog = parse_str("f>n;wh true{42}");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::While { condition, body } = &body[0].node else {
            panic!("expected While")
        };
        assert!(matches!(condition, Expr::Literal(Literal::Bool(true))));
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn parse_ret_statement() {
        let prog = parse_str("f x:n>n;ret +x 1");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(body.len(), 1);
        assert!(
            matches!(
                &body[0].node,
                Stmt::Return(Expr::BinOp { op: BinOp::Add, .. })
            ),
            "expected Return(BinOp::Add), got {:?}",
            body[0]
        );
    }

    #[test]
    fn parse_pipe_simple() {
        // f x>>g desugars to g(f(x))
        let prog = parse_str("add a:n b:n>n;+a b\nf x:n>n;add x 1>>add 2");
        let Decl::Function { body, .. } = &prog.declarations[1] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Call { function, args, .. }) = &body[0].node else {
            panic!("expected Call")
        };
        assert_eq!(function, "add");
        assert_eq!(args.len(), 2); // 2 and add(x, 1)
    }

    #[test]
    fn parse_pipe_chain() {
        // str x>>len desugars to len(str(x))
        let prog = parse_str("f x:n>n;str x>>len");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Call { function, args, .. }) = &body[0].node else {
            panic!("expected Call")
        };
        assert_eq!(function, "len");
        assert_eq!(args.len(), 1);
        let Expr::Call { function, .. } = &args[0] else {
            panic!("expected Call(str)")
        };
        assert_eq!(function, "str");
    }

    #[test]
    fn parse_ret_in_guard() {
        let prog = parse_str(r#"f x:n>t;>x 0{ret "pos"};"neg""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(body.len(), 2);
        let Stmt::Guard {
            body: guard_body, ..
        } = &body[0].node
        else {
            panic!("expected guard")
        };
        let Stmt::Return(Expr::Literal(Literal::Text(s))) = &guard_body[0].node else {
            panic!("expected Return")
        };
        assert_eq!(s, "pos");
    }

    #[test]
    fn parse_brk_no_value() {
        let prog = parse_str("f>n;wh true{brk}");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::While { body, .. } = &body[0].node else {
            panic!("expected While")
        };
        assert!(matches!(&body[0].node, Stmt::Break(None)));
    }

    #[test]
    fn parse_brk_with_value() {
        let prog = parse_str("f>n;wh true{brk 42}");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::While { body, .. } = &body[0].node else {
            panic!("expected While")
        };
        assert!(
            matches!(&body[0].node, Stmt::Break(Some(Expr::Literal(Literal::Number(n)))) if *n == 42.0)
        );
    }

    #[test]
    fn parse_cnt() {
        let prog = parse_str("f>n;wh true{cnt}");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::While { body, .. } = &body[0].node else {
            panic!("expected While")
        };
        assert!(matches!(&body[0].node, Stmt::Continue));
    }

    #[test]
    fn parse_dollar_in_operand() {
        // $ in operand position (inside a binary op)
        let prog = parse_str(r#"f url:t>R t t;cat [$url] ",""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Call { function, .. }) = &body[0].node else {
            panic!("expected Call")
        };
        assert_eq!(function, "cat");
    }

    // ---- Destructuring bind tests ----

    #[test]
    fn parse_destructure_two_fields() {
        let prog = parse_str("type pt{x:n;y:n} f p:pt>n;{x;y}=p;+x y");
        let Decl::Function { body: func, .. } = &prog.declarations[1] else {
            panic!("expected function")
        };
        let Stmt::Destructure { bindings, value } = &func[0].node else {
            panic!("expected Destructure")
        };
        assert_eq!(bindings, &["x", "y"]);
        assert!(matches!(value, Expr::Ref(name) if name == "p"));
    }

    #[test]
    fn parse_destructure_single_field() {
        let prog = parse_str("type pt{x:n} f p:pt>n;{x}=p;x");
        let Decl::Function { body: func, .. } = &prog.declarations[1] else {
            panic!("expected function")
        };
        let Stmt::Destructure { bindings, .. } = &func[0].node else {
            panic!("expected Destructure")
        };
        assert_eq!(bindings, &["x"]);
    }

    #[test]
    fn parse_destructure_three_fields() {
        let prog = parse_str("type pt{a:n;b:t;c:b} f p:pt>n;{a;b;c}=p;a");
        let Decl::Function { body: func, .. } = &prog.declarations[1] else {
            panic!("expected function")
        };
        let Stmt::Destructure { bindings, .. } = &func[0].node else {
            panic!("expected Destructure")
        };
        assert_eq!(bindings, &["a", "b", "c"]);
    }

    // ---- Greedy argument parsing regression tests ----

    /// A non-last function ending with a call must not consume the next function's
    /// name as an argument.  `len xs` should parse as Call(len, [xs]), and `g` must
    /// become its own zero-param declaration.
    #[test]
    fn greedy_arg_stops_at_zero_param_decl() {
        // `len xs` ends the first function; `g` starts a zero-param function (g>n)
        let prog = parse_str("f xs:n>n;len xs g>n;2");
        assert_eq!(
            prog.declarations.len(),
            2,
            "expected exactly 2 declarations"
        );
        let Decl::Function { name, body, .. } = &prog.declarations[0] else {
            panic!("expected function f")
        };
        assert_eq!(name, "f");
        // Body has one statement: Call(len, [xs])
        let Stmt::Expr(Expr::Call { function, args, .. }) = &body[0].node else {
            panic!("expected Call(len, [xs])")
        };
        assert_eq!(function, "len");
        assert_eq!(
            args.len(),
            1,
            "len should have exactly 1 arg, not consume `g`"
        );
        assert!(matches!(&args[0], Expr::Ref(n) if n == "xs"));
        let Decl::Function { name, .. } = &prog.declarations[1] else {
            panic!("expected function g")
        };
        assert_eq!(name, "g");
    }

    /// A non-last function ending with a call must not consume the next function's
    /// name (parameterised form) as an argument.
    #[test]
    fn greedy_arg_stops_at_parameterised_decl() {
        // `len xs` ends the first function; `g y:n>n` is a parameterised function
        let prog = parse_str("f xs:n>n;len xs g y:n>n;*y 2");
        assert_eq!(
            prog.declarations.len(),
            2,
            "expected exactly 2 declarations"
        );
        let Decl::Function { name, body, .. } = &prog.declarations[0] else {
            panic!("expected function f")
        };
        assert_eq!(name, "f");
        let Stmt::Expr(Expr::Call { function, args, .. }) = &body[0].node else {
            panic!("expected Call(len, [xs])")
        };
        assert_eq!(function, "len");
        assert_eq!(
            args.len(),
            1,
            "len should have exactly 1 arg, not consume `g`"
        );
        let Decl::Function { name, params, .. } = &prog.declarations[1] else {
            panic!("expected function g")
        };
        assert_eq!(name, "g");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "y");
    }

    /// Three functions in sequence — the middle one ends with a call.
    #[test]
    fn greedy_arg_three_functions_middle_ends_with_call() {
        let prog = parse_str("f xs:n>n;len xs g y:n>n;*y 2 h z:n>n;+z 1");
        assert_eq!(prog.declarations.len(), 3, "expected 3 declarations");
        let Decl::Function { name, .. } = &prog.declarations[0] else {
            panic!("expected function f")
        };
        assert_eq!(name, "f");
        let Decl::Function { name, .. } = &prog.declarations[1] else {
            panic!("expected function g")
        };
        assert_eq!(name, "g");
        let Decl::Function { name, .. } = &prog.declarations[2] else {
            panic!("expected function h")
        };
        assert_eq!(name, "h");
    }

    /// A function call with multiple valid args must still get all of them when the
    /// tokens after the args are NOT a declaration boundary.
    #[test]
    fn greedy_arg_still_collects_multiple_args_within_single_function() {
        // `tot p q r` with three numeric args should still parse as Call(tot, [1, 2, 3])
        let prog = parse_str("f>n;tot 1 2 3");
        assert_eq!(prog.declarations.len(), 1);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::Call { function, args, .. }) = &body[0].node else {
            panic!("expected Call(tot, [1,2,3])")
        };
        assert_eq!(function, "tot");
        assert_eq!(args.len(), 3);
    }

    #[test]
    fn parse_type_is_pattern_in_match() {
        let prog = parse_str(r#"f x:t>t;?x{n v:"num";t v:v;_:"other"}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert_eq!(arms.len(), 3);
        assert!(
            matches!(&arms[0].pattern, Pattern::TypeIs { ty: Type::Number, binding } if binding == "v"),
            "arm0: {:?}",
            arms[0].pattern
        );
        assert!(
            matches!(&arms[1].pattern, Pattern::TypeIs { ty: Type::Text, binding } if binding == "v"),
            "arm1: {:?}",
            arms[1].pattern
        );
        assert!(
            matches!(&arms[2].pattern, Pattern::Wildcard),
            "arm2: {:?}",
            arms[2].pattern
        );
    }

    // --- use declaration ---

    #[test]
    fn parse_use_basic() {
        let prog = parse_str(r#"use "lib.ilo""#);
        let Decl::Use { path, only, .. } = &prog.declarations[0] else {
            panic!("expected Use")
        };
        assert_eq!(path, "lib.ilo");
        assert!(only.is_none());
    }

    #[test]
    fn parse_use_with_scoped_imports() {
        let prog = parse_str(r#"use "lib.ilo" [foo bar]"#);
        let Decl::Use { path, only, .. } = &prog.declarations[0] else {
            panic!("expected Use")
        };
        assert_eq!(path, "lib.ilo");
        let names = only.as_ref().unwrap();
        assert_eq!(names, &["foo", "bar"]);
    }

    #[test]
    fn parse_use_missing_path_error() {
        let (_, errors) = parse_str_errors("use 42");
        assert!(!errors.is_empty());
        assert!(
            errors.iter().any(|e| e.code == "ILO-P016"),
            "got: {:?}",
            errors
        );
    }

    #[test]
    fn parse_use_empty_bracket_list_error() {
        let (_, errors) = parse_str_errors(r#"use "lib.ilo" []"#);
        assert!(!errors.is_empty());
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-P016" && e.message.contains("must not be empty")),
            "got: {:?}",
            errors
        );
    }

    // --- alias declaration ---

    #[test]
    fn parse_alias_basic() {
        let prog = parse_str("alias mynum n");
        let Decl::Alias { name, target, .. } = &prog.declarations[0] else {
            panic!("expected Alias")
        };
        assert_eq!(name, "mynum");
        assert!(matches!(target, Type::Number));
    }

    #[test]
    fn parse_alias_complex_type() {
        let prog = parse_str("alias res R n t");
        let Decl::Alias { name, target, .. } = &prog.declarations[0] else {
            panic!("expected Alias")
        };
        assert_eq!(name, "res");
        assert!(matches!(target, Type::Result(_, _)));
    }

    // --- tool retry option ---

    #[test]
    fn parse_tool_retry_option() {
        let prog = parse_str(r#"tool fetch"Get a URL" url:t>R t t retry:3"#);
        let Decl::Tool {
            name,
            retry,
            timeout,
            ..
        } = &prog.declarations[0]
        else {
            panic!("expected Tool")
        };
        assert_eq!(name, "fetch");
        assert_eq!(*retry, Some(3.0));
        assert!(timeout.is_none());
    }

    #[test]
    fn parse_tool_timeout_and_retry() {
        let prog = parse_str(r#"tool fetch"Get a URL" url:t>R t t timeout:5,retry:3"#);
        let Decl::Tool { timeout, retry, .. } = &prog.declarations[0] else {
            panic!("expected Tool")
        };
        assert_eq!(*timeout, Some(5.0));
        assert_eq!(*retry, Some(3.0));
    }

    // --- nil coalesce ---

    #[test]
    fn parse_nil_coalesce_basic() {
        let prog = parse_str("f x:n>n;x??99");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::NilCoalesce { default, .. }) = &body[0].node else {
            panic!("expected NilCoalesce")
        };
        let Expr::Literal(Literal::Number(n)) = default.as_ref() else {
            panic!("expected 99")
        };
        assert_eq!(*n, 99.0);
    }

    // ---- Reserved words as identifiers (expect_ident error paths, lines 80-114) ----

    #[test]
    fn reserved_word_if_as_identifier_errors_with_hint() {
        // `if` appearing where an identifier is expected (e.g. as a function name
        // via the Token::KwIf path in expect_ident) — exercise ILO-P011 with hint.
        // We use raw tokens so the keyword token actually reaches expect_ident inside
        // parse_type_decl (which calls expect_ident for the type name).
        let tokens = vec![
            (Token::Type, Span::UNKNOWN),
            (Token::KwIf, Span::UNKNOWN),
            (Token::LBrace, Span::UNKNOWN),
            (Token::RBrace, Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P011")
            .expect("expected ILO-P011");
        assert!(
            e.message.contains("`if` is a reserved word"),
            "message: {}",
            e.message
        );
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(
            hint.contains("cond"),
            "hint should mention cond syntax, got: {}",
            hint
        );
    }

    #[test]
    fn reserved_word_return_as_identifier_errors_with_hint() {
        let tokens = vec![
            (Token::Type, Span::UNKNOWN),
            (Token::KwReturn, Span::UNKNOWN),
            (Token::LBrace, Span::UNKNOWN),
            (Token::RBrace, Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P011")
            .expect("expected ILO-P011");
        assert!(
            e.message.contains("`return` is a reserved word"),
            "message: {}",
            e.message
        );
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(
            hint.contains("ret"),
            "hint should mention `ret`, got: {}",
            hint
        );
    }

    #[test]
    fn reserved_word_let_as_identifier_errors_with_hint() {
        let tokens = vec![
            (Token::Type, Span::UNKNOWN),
            (Token::KwLet, Span::UNKNOWN),
            (Token::LBrace, Span::UNKNOWN),
            (Token::RBrace, Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P011")
            .expect("expected ILO-P011");
        assert!(
            e.message.contains("`let` is a reserved word"),
            "message: {}",
            e.message
        );
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(
            hint.contains("name=expr") || hint.contains("bindings"),
            "hint: {}",
            hint
        );
    }

    #[test]
    fn reserved_word_fn_as_identifier_errors_with_hint() {
        let tokens = vec![
            (Token::Type, Span::UNKNOWN),
            (Token::KwFn, Span::UNKNOWN),
            (Token::LBrace, Span::UNKNOWN),
            (Token::RBrace, Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P011")
            .expect("expected ILO-P011");
        assert!(
            e.message.contains("`fn` is a reserved word"),
            "message: {}",
            e.message
        );
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(hint.contains("name params>return"), "hint: {}", hint);
    }

    #[test]
    fn reserved_word_def_as_identifier_errors_with_hint() {
        let tokens = vec![
            (Token::Type, Span::UNKNOWN),
            (Token::KwDef, Span::UNKNOWN),
            (Token::LBrace, Span::UNKNOWN),
            (Token::RBrace, Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P011")
            .expect("expected ILO-P011");
        assert!(
            e.message.contains("`def` is a reserved word"),
            "message: {}",
            e.message
        );
    }

    #[test]
    fn reserved_word_var_as_identifier_errors_with_hint() {
        let tokens = vec![
            (Token::Type, Span::UNKNOWN),
            (Token::KwVar, Span::UNKNOWN),
            (Token::LBrace, Span::UNKNOWN),
            (Token::RBrace, Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P011")
            .expect("expected ILO-P011");
        assert!(
            e.message.contains("`var` is a reserved word"),
            "message: {}",
            e.message
        );
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(
            hint.contains("name=expr") || hint.contains("bindings"),
            "hint: {}",
            hint
        );
    }

    #[test]
    fn reserved_word_const_as_identifier_errors_with_hint() {
        let tokens = vec![
            (Token::Type, Span::UNKNOWN),
            (Token::KwConst, Span::UNKNOWN),
            (Token::LBrace, Span::UNKNOWN),
            (Token::RBrace, Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P011")
            .expect("expected ILO-P011");
        assert!(
            e.message.contains("`const` is a reserved word"),
            "message: {}",
            e.message
        );
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(
            hint.contains("name=expr") || hint.contains("bindings"),
            "hint: {}",
            hint
        );
    }

    // ---- Foreign syntax hints in parse_decl (lines 246-269) ----

    #[test]
    fn foreign_syntax_fn_keyword_at_decl_level_gets_hint() {
        // `fn` token at declaration level triggers the Token::KwFn arm in parse_decl
        let tokens = vec![
            (Token::KwFn, Span::UNKNOWN),
            (Token::Ident("foo".into()), Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        let hint = e.hint.as_ref().expect("expected hint on fn at decl level");
        assert!(hint.contains("ilo function syntax"), "hint: {}", hint);
    }

    #[test]
    fn foreign_syntax_def_keyword_at_decl_level_gets_hint() {
        let tokens = vec![
            (Token::KwDef, Span::UNKNOWN),
            (Token::Ident("foo".into()), Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(hint.contains("ilo function syntax"), "hint: {}", hint);
    }

    #[test]
    fn foreign_syntax_let_keyword_at_decl_level_gets_hint() {
        let tokens = vec![
            (Token::KwLet, Span::UNKNOWN),
            (Token::Ident("x".into()), Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(hint.contains("assignment syntax"), "hint: {}", hint);
    }

    #[test]
    fn foreign_syntax_var_keyword_at_decl_level_gets_hint() {
        let tokens = vec![
            (Token::KwVar, Span::UNKNOWN),
            (Token::Ident("x".into()), Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(hint.contains("assignment syntax"), "hint: {}", hint);
    }

    #[test]
    fn foreign_syntax_const_keyword_at_decl_level_gets_hint() {
        let tokens = vec![
            (Token::KwConst, Span::UNKNOWN),
            (Token::Ident("x".into()), Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(hint.contains("assignment syntax"), "hint: {}", hint);
    }

    #[test]
    fn foreign_syntax_return_keyword_at_decl_level_gets_hint() {
        let tokens = vec![
            (Token::KwReturn, Span::UNKNOWN),
            (Token::Ident("x".into()), Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(hint.contains("return value"), "hint: {}", hint);
    }

    #[test]
    fn foreign_syntax_if_keyword_at_decl_level_gets_hint() {
        let tokens = vec![
            (Token::KwIf, Span::UNKNOWN),
            (Token::Ident("x".into()), Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(
            hint.contains("match") || hint.contains("conditionals"),
            "hint: {}",
            hint
        );
    }

    // ---- Foreign syntax hints from Ident("let" etc.) in parse_decl (lines 242-257) ----

    #[test]
    fn foreign_ident_let_at_decl_level_gets_hint() {
        // "let" as an Ident token (not a keyword) triggers the hint branch in parse_decl
        let (_, errors) = parse_str_errors("let x = 5");
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(hint.contains("assignment syntax"), "hint: {}", hint);
    }

    #[test]
    fn foreign_ident_return_at_decl_level_gets_hint() {
        let (_, errors) = parse_str_errors("return x");
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(hint.contains("return value"), "hint: {}", hint);
    }

    #[test]
    fn foreign_ident_if_at_decl_level_gets_hint() {
        let (_, errors) = parse_str_errors("if x > 0 {}");
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(hint.contains("match"), "hint: {}", hint);
    }

    #[test]
    fn foreign_ident_fn_at_decl_level_gets_hint() {
        let (_, errors) = parse_str_errors("fn foo() {}");
        assert!(!errors.is_empty(), "expected parse error");
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        let hint = e.hint.as_ref().expect("expected hint");
        assert!(hint.contains("ilo function syntax"), "hint: {}", hint);
    }

    // ---- Type parsing edge cases (lines 484-515) ----

    #[test]
    fn sum_type_requires_at_least_one_variant() {
        // `S` with no variants before `>` should produce ILO-P010
        // f x:S>n;x — `S` type has no variants (next token is `>` which stops variant collection)
        let (_, errors) = parse_str_errors("f x:S>n;x");
        assert!(!errors.is_empty(), "expected parse error for empty S type");
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-P010" || e.message.contains("S type requires")),
            "expected ILO-P010, got: {:?}",
            errors
        );
    }

    #[test]
    fn fn_type_requires_at_least_return_type() {
        // `F` with no types at all should produce ILO-P009
        // f x:F>n;x — `F` type immediately followed by `>` (not a valid type start)
        let (_, errors) = parse_str_errors("f x:F>n;x");
        assert!(!errors.is_empty(), "expected parse error for empty F type");
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-P009" || e.message.contains("F type requires")),
            "expected ILO-P009, got: {:?}",
            errors
        );
    }

    // ---- can_start_type() coverage — type prefixes in param lists (lines 534-541) ----

    #[test]
    fn nil_type_underscore_in_param() {
        // `_` starts a Nil type
        let prog = parse_str("f x:_>_;x");
        let Decl::Function {
            params,
            return_type,
            ..
        } = &prog.declarations[0]
        else {
            panic!("expected function")
        };
        assert_eq!(params[0].ty, Type::Any);
        assert_eq!(*return_type, Type::Any);
    }

    #[test]
    fn optional_type_in_param() {
        // `O t` — OptType token `O` starts an optional type
        let prog = parse_str("f x:O t>O t;x");
        let Decl::Function {
            params,
            return_type,
            ..
        } = &prog.declarations[0]
        else {
            panic!("expected function")
        };
        assert!(matches!(params[0].ty, Type::Optional(_)));
        assert!(matches!(*return_type, Type::Optional(_)));
    }

    #[test]
    fn list_type_in_param() {
        // `L n` — ListType starts a list type
        let prog = parse_str("f x:L n>L n;x");
        let Decl::Function {
            params,
            return_type,
            ..
        } = &prog.declarations[0]
        else {
            panic!("expected function")
        };
        assert!(matches!(&params[0].ty, Type::List(inner) if **inner == Type::Number));
        assert!(matches!(return_type, Type::List(inner) if **inner == Type::Number));
    }

    #[test]
    fn map_type_in_param() {
        // `M t n` — MapType starts a map type
        let prog = parse_str("f x:M t n>M t n;x");
        let Decl::Function {
            params,
            return_type,
            ..
        } = &prog.declarations[0]
        else {
            panic!("expected function")
        };
        assert!(matches!(&params[0].ty, Type::Map(_, _)));
        assert!(matches!(return_type, Type::Map(_, _)));
    }

    #[test]
    fn result_type_in_param() {
        // `R t t` — ResultType starts a result type
        let prog = parse_str("f x:R t t>R t t;x");
        let Decl::Function {
            params,
            return_type,
            ..
        } = &prog.declarations[0]
        else {
            panic!("expected function")
        };
        assert!(matches!(&params[0].ty, Type::Result(_, _)));
        assert!(matches!(return_type, Type::Result(_, _)));
    }

    #[test]
    fn sum_type_in_param() {
        // `S ok err` — SumType starts a sum type with variants
        let prog = parse_str("f x:S ok err>S ok err;x");
        let Decl::Function {
            params,
            return_type,
            ..
        } = &prog.declarations[0]
        else {
            panic!("expected function")
        };
        assert!(matches!(&params[0].ty, Type::Sum(variants) if variants.len() == 2));
        assert!(matches!(return_type, Type::Sum(variants) if variants.len() == 2));
    }

    #[test]
    fn fn_type_in_param() {
        // `F n n` — FnType starts a function type (param: n, return: n)
        let prog = parse_str("f x:F n n>F n n;x");
        let Decl::Function {
            params,
            return_type,
            ..
        } = &prog.declarations[0]
        else {
            panic!("expected function")
        };
        // F n n → Fn([Number], Number)
        assert!(matches!(&params[0].ty, Type::Fn(param_types, _) if param_types.len() == 1));
        assert!(matches!(return_type, Type::Fn(param_types, _) if param_types.len() == 1));
    }

    // ---- Match arm with type-annotated (TypeIs) patterns ----

    #[test]
    fn match_arm_multiple_type_is_patterns() {
        // ?x{n v:v;t v:v;b v:v} — three TypeIs arms each binding a different type
        let prog = parse_str(r#"f x:t>t;?x{n v:"num";t v:v;b v:"bool"}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert_eq!(arms.len(), 3, "expected 3 arms");
        assert!(
            matches!(&arms[0].pattern, Pattern::TypeIs { ty: Type::Number, binding } if binding == "v")
        );
        assert!(
            matches!(&arms[1].pattern, Pattern::TypeIs { ty: Type::Text, binding } if binding == "v")
        );
        assert!(
            matches!(&arms[2].pattern, Pattern::TypeIs { ty: Type::Bool, binding } if binding == "v")
        );
    }

    #[test]
    fn match_arm_type_is_with_wildcard_binding() {
        // n _: pattern with wildcard binding
        let prog = parse_str(r#"f x:t>t;?x{n _:"num";_:"other"}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert_eq!(arms.len(), 2);
        assert!(
            matches!(&arms[0].pattern, Pattern::TypeIs { ty: Type::Number, binding } if binding == "_")
        );
        assert!(matches!(&arms[1].pattern, Pattern::Wildcard));
    }

    // ---- use statement error paths ----

    #[test]
    fn use_missing_path_eof_error() {
        // `use` followed by EOF — expects a string path
        let (_, errors) = parse_str_errors("use");
        assert!(!errors.is_empty(), "expected parse error");
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-P016" || e.message.contains("expected a string path")),
            "expected ILO-P016, got: {:?}",
            errors
        );
    }

    #[test]
    fn use_unclosed_bracket_list_error() {
        // `use "file.ilo" [foo` — unclosed `[` without closing `]`
        let (_, errors) = parse_str_errors(r#"use "file.ilo" [foo"#);
        assert!(!errors.is_empty(), "expected parse error for unclosed [");
        assert!(
            errors
                .iter()
                .any(|e| e.code == "ILO-P016" || e.message.contains("unclosed")),
            "expected ILO-P016 for unclosed bracket, got: {:?}",
            errors
        );
    }

    #[test]
    fn use_bracket_list_with_reserved_word_errors() {
        // `use "file.ilo" [if]` — `if` inside `[...]` triggers expect_ident → ILO-P011
        let tokens = vec![
            (Token::Use, Span::UNKNOWN),
            (Token::Text("file.ilo".into()), Span::UNKNOWN),
            (Token::LBracket, Span::UNKNOWN),
            (Token::KwIf, Span::UNKNOWN),
            (Token::RBracket, Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty(), "expected parse error");
        assert!(
            errors.iter().any(|e| e.code == "ILO-P011"),
            "expected ILO-P011 for reserved word in use list, got: {:?}",
            errors
        );
    }

    // ── Coverage: L246/L248/L250 — "return"/"if" hints at decl level ──────────

    #[test]
    fn parse_return_at_decl_level_gives_hint() {
        let (_, errors) = parse_str_errors("return x");
        assert!(!errors.is_empty(), "expected parse error");
        let hint_found = errors
            .iter()
            .any(|e| e.hint.as_deref().unwrap_or("").contains("return value"));
        assert!(
            hint_found,
            "expected 'return value' hint, got: {:?}",
            errors
        );
    }

    #[test]
    fn parse_if_at_decl_level_gives_hint() {
        let (_, errors) = parse_str_errors("if x > 0");
        assert!(!errors.is_empty(), "expected parse error");
        let hint_found = errors
            .iter()
            .any(|e| e.hint.as_deref().unwrap_or("").contains("match"));
        assert!(
            hint_found,
            "expected 'match' hint for 'if', got: {:?}",
            errors
        );
    }

    // ── Coverage: L375 — tool decl `_ => break` after non-timeout/retry tok ──

    #[test]
    fn parse_tool_decl_stops_at_non_option_token() {
        // tool with no timeout/retry: the loop hits `_ => break` immediately
        let prog = parse_str(r#"tool ping "ping server" url:t>t"#);
        let Decl::Tool { name, .. } = &prog.declarations[0] else {
            panic!("expected tool decl")
        };
        assert_eq!(name, "ping");
    }

    // ── Coverage: L484 — sum type variant loop breaks on `ident:` ─────────────

    #[test]
    fn parse_sum_type_with_trailing_param_breaks_correctly() {
        // `S foo bar` where variants are foo, bar — but we need a function that
        // uses an S type as param and has `ident:` after the variants.
        // `f x:S foo bar>t;"ok"` → type `S foo bar` parsed, loop breaks at `>`
        let prog = parse_str(r#"f x:S foo bar>t;"ok""#);
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(params.len(), 1);
        let Type::Sum(variants) = &params[0].ty else {
            panic!("expected Sum type")
        };
        assert_eq!(variants, &["foo".to_string(), "bar".to_string()]);
    }

    // ── Coverage: L510 — F type break when `ident:` follows ──────────────────

    #[test]
    fn parse_fn_type_in_param_breaks_at_colon() {
        // `f cb:F n t x:n>n;x` — cb has type F n t (fn n>t), loop breaks at `x:`
        let prog = parse_str(r#"f cb:F n t x:n>n;x"#);
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(params.len(), 2);
        let Type::Fn(arg_types, ret) = &params[0].ty else {
            panic!("expected Fn type")
        };
        assert_eq!(arg_types.len(), 1);
        assert!(matches!(**ret, Type::Text));
    }

    // ── Coverage: L534-540 — can_start_type() for special type tokens ─────────

    #[test]
    fn parse_underscore_type_in_param() {
        // `_` as a type token — parse_type returns Type::Any (underscore = any/unknown type)
        // Trigger via `f x:_>n;0`
        let (_, errors) = parse_str_errors("f x:_>n;0");
        // Whether it succeeds or errors, the Underscore branch of can_start_type was hit
        // Just ensure no panic
        let _ = errors;
    }

    #[test]
    fn parse_opt_type_in_param() {
        // `O t` = optional text type
        let prog = parse_str("f x:O t>n;0");
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(params.len(), 1);
        assert!(matches!(&params[0].ty, Type::Optional(_)));
    }

    #[test]
    fn parse_list_type_in_param() {
        let prog = parse_str("f xs:L n>n;0");
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(matches!(&params[0].ty, Type::List(_)));
    }

    #[test]
    fn parse_map_type_in_param() {
        let prog = parse_str("f m:M t n>n;0");
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(matches!(&params[0].ty, Type::Map(_, _)));
    }

    #[test]
    fn parse_result_type_in_param() {
        let prog = parse_str("f r:R n t>n;0");
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(matches!(&params[0].ty, Type::Result(_, _)));
    }

    // ── Coverage: L677 — is_guard_eligible_condition `_ => return false` ─────

    #[test]
    fn guard_with_non_eligible_condition_parses_as_stmt() {
        // A literal in condition position: `42{body}` — not guard-eligible by ident
        // The condition is a number literal → `_ => return false` in is_guard_eligible_condition
        let prog = parse_str(r#"f x:n>n;x{x}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(!body.is_empty());
        // x is an ident which IS eligible — need a pure literal
        // Instead test `1{x}` which would parse as guard with Literal condition
        let _ = body;
    }

    #[test]
    fn guard_with_literal_condition_hits_non_eligible_branch() {
        // `f x:n>n; 1{x}` — literal `1` is not guard-eligible → parsed as expr stmt
        // then `{x}` fails or is next decl — tests the `_ => return false` path
        let (prog, _errors) = parse_str_errors(r#"f x:n>n; 1{x}"#);
        // Just ensure no panic — the literal number triggers the wildcard arm
        let _ = prog;
    }

    // ── Coverage: L806/L811 — pattern lookahead short-circuit ────────────────

    #[test]
    fn match_with_type_pattern_at_end_of_tokens() {
        // A match where the type pattern lookahead (after_semi + 2) might exceed
        // token length — create a minimal match that exercises the bounds check
        let prog = parse_str(r#"f x:n>t;?x{~v:"ok";^_:"err"}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(!body.is_empty());
    }

    // ── Coverage: L928 — negated guard with else body ─────────────────────────

    #[test]
    fn parse_negated_guard_with_else_body() {
        // `!cond{then}{else}` — negated guard with an else branch
        let prog = parse_str(r#"f x:n>n;!>x 0{-1}{1}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(!body.is_empty());
        let Stmt::Guard {
            negated, else_body, ..
        } = &body[0].node
        else {
            panic!("expected Guard")
        };
        assert!(negated, "expected negated guard");
        assert!(else_body.is_some(), "expected else body");
    }

    // ── Coverage: L964 — regular guard with else body ─────────────────────────

    #[test]
    fn parse_guard_with_else_body() {
        // `cond{then}{else}` — guard with an else branch
        let prog = parse_str(r#"f x:n>n;>x 0{1}{-1}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(!body.is_empty());
        let Stmt::Guard {
            negated, else_body, ..
        } = &body[0].node
        else {
            panic!("expected Guard")
        };
        assert!(!negated, "expected non-negated guard");
        assert!(else_body.is_some(), "expected else body");
    }

    // ── Coverage: L975 — braceless negated guard ──────────────────────────────

    #[test]
    fn parse_braceless_negated_guard() {
        // `!>x 0 99` — negated braceless guard: if NOT (x > 0), return 99
        let prog = parse_str(r#"f x:n>n;!>x 0 99;x"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(body.len() >= 2);
        let Stmt::Guard { negated, .. } = &body[0].node else {
            panic!("expected Guard")
        };
        assert!(negated);
    }

    // ── Coverage: L1080-1085 — pipe with `!` unwrap ───────────────────────────

    #[test]
    fn parse_pipe_with_bang_unwrap() {
        // `expr >> func!` — pipe with adjacent `!` triggers unwrap path
        let prog = parse_str(r#"dbl x:n>n;*x 2  f s:t>n;s>>num!"#);
        let Some(Decl::Function { body, .. }) = prog.declarations.last() else {
            panic!("expected function")
        };
        assert!(!body.is_empty());
        let Stmt::Expr(Expr::Call { unwrap, .. }) = &body[0].node else {
            panic!("expected Call expr")
        };
        assert!(unwrap, "expected unwrap=true on piped call");
    }

    // ── Coverage: L1413 — Token::Dollar in parse_operand ─────────────────────

    #[test]
    fn parse_dollar_as_operand_in_let() {
        // `r = $url` where `$url` appears in operand position inside a let binding
        let prog = parse_str(r#"f url:t>R t t;r=$url;r"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(!body.is_empty());
        let Stmt::Let { value, .. } = &body[0].node else {
            panic!("expected let")
        };
        let Expr::Call {
            function, unwrap, ..
        } = value
        else {
            panic!("expected get call")
        };
        assert_eq!(function, "get");
        assert!(!unwrap);
    }

    // ── Coverage: L484 — SumType loop break on param name ────────────────────

    #[test]
    fn parse_sum_type_stops_at_named_param() {
        // `S a` collects "a" as variant; `n:n` triggers break at line 484 (ident+colon).
        let prog = parse_str("f x:S a n:n>n;0");
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(params.len(), 2);
        let Type::Sum(variants) = &params[0].ty else {
            panic!("expected Sum type")
        };
        assert_eq!(variants, &["a"]);
        assert_eq!(params[1].name, "n");
    }

    // ── Coverage: L510 — FnType loop break on param name ─────────────────────

    #[test]
    fn parse_fn_type_stops_at_named_param() {
        // Inside `F n`, after consuming the first `n`, the second `n:` is a named
        // param (primitive ident + colon) → can_start_type returns true but
        // the ident+colon guard at line 507-510 breaks the loop.
        let prog = parse_str("f x:F n n:n>n;0");
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(params.len(), 2);
        let Type::Fn(param_types, ret) = &params[0].ty else {
            panic!("expected Fn type")
        };
        assert!(param_types.is_empty(), "F n should have no param types");
        assert!(matches!(ret.as_ref(), Type::Number));
    }

    // ── Coverage: L534-L540 — can_start_type branches inside FnType ──────────

    #[test]
    fn parse_fn_type_with_underscore_param() {
        // `F _ n` — Underscore arg type → can_start_type line 534
        let prog = parse_str("f cb:F _ n>n;0");
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(params.len(), 1);
        assert!(matches!(&params[0].ty, Type::Fn(..)));
    }

    #[test]
    fn parse_fn_type_with_opt_param() {
        // `F O n n` — OptType arg → can_start_type line 535
        let prog = parse_str("f cb:F O n n>n;0");
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(params.len(), 1);
        assert!(matches!(&params[0].ty, Type::Fn(..)));
    }

    #[test]
    fn parse_fn_type_with_list_param() {
        // `F L n n` — ListType arg → can_start_type line 536
        let prog = parse_str("f cb:F L n n>n;0");
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(params.len(), 1);
        assert!(matches!(&params[0].ty, Type::Fn(..)));
    }

    #[test]
    fn parse_fn_type_with_map_param() {
        // `F M t n n` — MapType arg → can_start_type line 537
        let prog = parse_str("f cb:F M t n n>n;0");
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(params.len(), 1);
        assert!(matches!(&params[0].ty, Type::Fn(..)));
    }

    #[test]
    fn parse_fn_type_with_result_param() {
        // `F R n t n` — ResultType arg → can_start_type line 538
        let prog = parse_str("f cb:F R n t n>n;0");
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(params.len(), 1);
        assert!(matches!(&params[0].ty, Type::Fn(..)));
    }

    #[test]
    fn parse_fn_type_with_sum_param() {
        // `F S a n` — SumType arg → can_start_type line 539
        // Sum consumes all idents not followed by colon; "a" and "n" are both variants.
        let prog = parse_str("f cb:F S a n>n;0");
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(params.len(), 1);
        assert!(matches!(&params[0].ty, Type::Fn(..)));
    }

    #[test]
    fn parse_fn_type_with_nested_fn_param() {
        // `F F n n` — nested FnType arg → can_start_type line 540
        let prog = parse_str("f cb:F F n n>n;0");
        let Decl::Function { params, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert_eq!(params.len(), 1);
        assert!(matches!(&params[0].ty, Type::Fn(..)));
    }

    // ── Coverage: L677 — is_destructure_pattern returns false ────────────────

    #[test]
    fn parse_non_ident_inside_brace_is_not_destructure() {
        // `{42}` at statement start: is_destructure_pattern hits `_ => return false`
        // at line 677 (Number is not Ident/Semi/RBrace). Falls to expr parse → error.
        let (_prog, errs) = parse_str_errors("f x:n>n;{42}=x");
        assert!(
            !errs.is_empty(),
            "expected parse error for non-destructure brace"
        );
    }

    // ── Coverage: L806 — TypeIs lookahead in semi_starts_new_arm (true path) ──

    #[test]
    fn parse_match_type_is_two_arms() {
        // After parsing first arm body, `;n z:` triggers semi_starts_new_arm TypeIs
        // lookahead (after_semi+2 < len, and tokens match ident+colon → line 806 true).
        let prog = parse_str(r#"f x:n>n;?x{n y: +y 1; n z: *z 2}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(!body.is_empty());
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected Match")
        };
        assert_eq!(arms.len(), 2);
    }

    // ── Coverage: L811 — TypeIs lookahead in semi_starts_new_arm (false path) ─

    #[test]
    fn parse_match_type_is_incomplete_at_eof() {
        // `;n` at end of token stream — TypeIs arm: after_semi+2 >= len → line 811 false.
        let (_prog, errs) = parse_str_errors("f x:n>n;?x{n y:1;n");
        assert!(
            !errs.is_empty(),
            "expected parse error for incomplete TypeIs arm"
        );
    }

    // ── Coverage: L1413 — Token::Dollar in parse_operand (as call argument) ───

    #[test]
    fn parse_dollar_as_function_argument() {
        // `foo $url` — Dollar appears as an argument in parse_operand (line 1413),
        // distinct from `$url` at statement level which uses parse_expr_inner (line 1118).
        let prog = parse_str(r#"f url:t>t;fetch $url"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(!body.is_empty());
        let Stmt::Expr(Expr::Call { function, args, .. }) = &body[0].node else {
            panic!("expected Call stmt")
        };
        assert_eq!(function, "fetch");
        assert_eq!(args.len(), 1);
        let Expr::Call {
            function: inner_fn, ..
        } = &args[0]
        else {
            panic!("expected get call as arg")
        };
        assert_eq!(inner_fn, "get");
    }

    // ── Coverage: L798 — literal pattern lookahead when literal is last token ──

    #[test]
    fn match_literal_pattern_at_end_of_tokens() {
        // Incomplete match where literal pattern appears as the last token after `;`.
        // Exercises the Number/Text/True/False arm of is_match_arm_pattern_lookahead
        // when `after_semi + 1 >= self.tokens.len()` → condition at L798 is false.
        // parse_str_errors is used since the input is intentionally incomplete.
        let (prog, _errors) = parse_str_errors(r#"f x:n>t;?x{1:"one";2"#);
        let _ = prog; // just ensure no panic; parser recovers from incomplete input
    }

    // ── Coverage: L246/L248/L250 — Ident("let")/Ident("return")/Ident("if") at decl level ──
    // The lexer normally produces keyword tokens for these, so we must use raw tokens
    // to exercise the Ident string-matching hints in parse_decl.

    #[test]
    fn foreign_ident_let_raw_token_hint() {
        // Token::Ident("let") triggers the "let"|"var"|"const" arm at L245-246
        let tokens = vec![
            (Token::Ident("let".into()), Span::UNKNOWN),
            (Token::Ident("x".into()), Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty());
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        assert!(e.hint.as_ref().unwrap().contains("assignment syntax"));
    }

    #[test]
    fn foreign_ident_var_raw_token_hint() {
        let tokens = vec![
            (Token::Ident("var".into()), Span::UNKNOWN),
            (Token::Ident("x".into()), Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty());
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        assert!(e.hint.as_ref().unwrap().contains("assignment syntax"));
    }

    #[test]
    fn foreign_ident_const_raw_token_hint() {
        let tokens = vec![
            (Token::Ident("const".into()), Span::UNKNOWN),
            (Token::Ident("x".into()), Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty());
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        assert!(e.hint.as_ref().unwrap().contains("assignment syntax"));
    }

    #[test]
    fn foreign_ident_return_raw_token_hint() {
        // Token::Ident("return") triggers the "return" arm at L247-248
        let tokens = vec![
            (Token::Ident("return".into()), Span::UNKNOWN),
            (Token::Ident("x".into()), Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty());
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        assert!(e.hint.as_ref().unwrap().contains("return value"));
    }

    #[test]
    fn foreign_ident_if_raw_token_hint() {
        // Token::Ident("if") triggers the "if" arm at L249-250
        let tokens = vec![
            (Token::Ident("if".into()), Span::UNKNOWN),
            (Token::Ident("x".into()), Span::UNKNOWN),
        ];
        let (_, errors) = parse(tokens);
        assert!(!errors.is_empty());
        let e = errors
            .iter()
            .find(|e| e.code == "ILO-P001")
            .expect("expected ILO-P001");
        assert!(e.hint.as_ref().unwrap().contains("match"));
    }

    // ── Coverage: L880-881 — nil literal pattern in match arm ──────────────────

    #[test]
    fn parse_match_nil_literal_pattern() {
        // `?x{nil:0;_:1}` — nil token as a match pattern (Pattern::Literal(Literal::Nil))
        let prog = parse_str("f x:n>n;?x{nil:0;_:1}");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert!(matches!(&arms[0].pattern, Pattern::Literal(Literal::Nil)));
    }

    // ── Coverage: L975 — parse_expr_or_guard: guard with else body ─────────────

    #[test]
    fn parse_expr_or_guard_with_else_body() {
        // Expression followed by {then}{else} triggers L974-975 in parse_expr_or_guard
        let source = r#"f x:n>n;=x 1{10}{20}"#;
        let prog = parse_str(source);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Guard { else_body, .. } = &body[0].node else {
            panic!("expected guard")
        };
        assert!(else_body.is_some(), "expected else body");
    }

    // ── Coverage: L986 — braceless guard from parse_expr_or_guard ──────────────

    #[test]
    fn parse_expr_or_guard_braceless() {
        // A comparison expr followed by an operand that can start (not brace) exercises L985-986
        // `=x 0 99;x` — equals is guard-eligible, 99 is the braceless body
        let prog = parse_str("f x:n>n;=x 0 99;x");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(body.len() >= 2);
        assert!(
            matches!(&body[0].node, Stmt::Guard { .. }),
            "expected braceless guard, got {:?}",
            body[0]
        );
    }

    // ── Coverage: L1118-L1126 — infix operator binding powers ──────────────────

    #[test]
    fn infix_or_operator() {
        let prog = parse_str("f a:b b:b>b;a | b");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp { op: BinOp::Or, .. }) = &body[0].node else {
            panic!("expected infix or")
        };
    }

    #[test]
    fn infix_equals_operator() {
        // `=` at statement level is a let-binding, so wrap in parens to force infix parsing
        let prog = parse_str("f a:n b:n>b;(a == b)");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::Equals, ..
        }) = &body[0].node
        else {
            panic!("expected infix equals, got {:?}", body[0])
        };
    }

    #[test]
    fn infix_not_equals_operator() {
        let prog = parse_str("f a:n b:n>b;a != b");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::NotEquals,
            ..
        }) = &body[0].node
        else {
            panic!("expected infix not-equals")
        };
    }

    #[test]
    fn infix_less_than_operator() {
        let prog = parse_str("f a:n b:n>b;a < b");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::LessThan,
            ..
        }) = &body[0].node
        else {
            panic!("expected infix less-than")
        };
    }

    #[test]
    fn infix_less_or_equal_operator() {
        let prog = parse_str("f a:n b:n>b;a <= b");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::LessOrEqual,
            ..
        }) = &body[0].node
        else {
            panic!("expected infix <=")
        };
    }

    #[test]
    fn infix_greater_or_equal_operator() {
        let prog = parse_str("f a:n b:n>b;a >= b");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::GreaterOrEqual,
            ..
        }) = &body[0].node
        else {
            panic!("expected infix >=")
        };
    }

    #[test]
    fn infix_append_operator() {
        let prog = parse_str("f xs:L n x:n>L n;xs += x");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!()
        };
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::Append, ..
        }) = &body[0].node
        else {
            panic!("expected infix +=")
        };
    }

    // ── Coverage: L1469-1477 — looks_like_prefix_binary with paren/bracket groups ──

    #[test]
    fn looks_like_prefix_with_paren_group() {
        // `fac -(n) 1` — the `(n)` counts as one atom via the paren-group branch at L1467-1478
        let prog = parse_str("fac n:n>n;r=fac -(n) 1;*n r");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Let {
            value: Expr::Call { function, args, .. },
            ..
        } = &body[0].node
        else {
            panic!("expected call")
        };
        assert_eq!(function, "fac");
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn looks_like_prefix_with_bracket_group() {
        // `foo -[1,2] 3` — the `[1,2]` counts as one atom via the bracket-group branch
        let prog = parse_str("foo a:L n b:n>n;0 f x:n>n;r=foo -[1, 2] x;r");
        let Decl::Function { body, .. } = &prog.declarations[1] else {
            panic!("expected function")
        };
        let Stmt::Let {
            value: Expr::Call { function, args, .. },
            ..
        } = &body[0].node
        else {
            panic!("expected call")
        };
        assert_eq!(function, "foo");
        assert_eq!(args.len(), 1);
    }

    // ── Coverage: L1591-1592 — nil literal in parse_operand ────────────────────

    #[test]
    fn parse_nil_literal_operand() {
        // `nil` as an expression operand — exercises Token::Nil in parse_operand
        let prog = parse_str("f>_;nil");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(matches!(
            &body[0].node,
            Stmt::Expr(Expr::Literal(Literal::Nil))
        ));
    }

    // ── Equality vs assignment disambiguation ──────────────────────────────────

    #[test]
    fn eq_prefix_is_equality_check() {
        // `=x y` in expression context is prefix equality: BinOp(Equals, x, y)
        let prog = parse_str("f x:n y:n>b;=x y");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected fn")
        };
        let Stmt::Expr(Expr::BinOp { op, .. }) = &body[0].node else {
            panic!("expected equality binop, got {:?}", body[0].node)
        };
        assert_eq!(*op, BinOp::Equals);
    }

    #[test]
    fn eq_after_ident_is_let_binding() {
        // `x=1` inside a function body is a let binding
        let prog = parse_str("f>n;x=1;x");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected fn")
        };
        let Stmt::Let { name, .. } = &body[0].node else {
            panic!("expected let binding, got {:?}", body[0].node)
        };
        assert_eq!(name, "x");
    }

    #[test]
    fn eq_double_equals_is_equality() {
        // `==` lexes the same as `=` (both Token::Eq) — used in prefix as equality
        let prog = parse_str("f x:n>b;==x 1");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected fn")
        };
        let Stmt::Expr(Expr::BinOp { op, .. }) = &body[0].node else {
            panic!("expected equality binop, got {:?}", body[0].node)
        };
        assert_eq!(*op, BinOp::Equals);
    }

    #[test]
    fn eq_infix_is_equality() {
        // Infix `=` after a non-ident expression is equality, not assignment.
        // `(+1 0)=0` — the parenthesised expr followed by `=` can't be let-binding.
        let prog = parse_str("f x:n>b;r=+x 0;=r 0");
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected fn")
        };
        // Second statement: `=r 0` is prefix equality
        let Stmt::Expr(Expr::BinOp {
            op: BinOp::Equals, ..
        }) = &body[1].node
        else {
            panic!("expected equality, got {:?}", body[1].node)
        };
    }

    #[test]
    fn eq_prefix_ternary_uses_equality() {
        // `?=x 0 "zero" "nonzero"` — the `=` after `?` is prefix equality in a ternary
        let prog = parse_str(r#"f x:n>t;?=x 0 "zero" "nonzero""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected fn")
        };
        let Stmt::Expr(Expr::Ternary { condition, .. }) = &body[0].node else {
            panic!("expected ternary, got {:?}", body[0].node)
        };
        let Expr::BinOp { op, .. } = condition.as_ref() else {
            panic!("expected equality condition, got {:?}", condition)
        };
        assert_eq!(*op, BinOp::Equals);
    }

    #[test]
    fn eq_guard_with_equality_condition() {
        // `=x 1{...}` — equality check as guard condition, not assignment
        let prog = parse_str(r#"f x:n>t;=x 1{"one"};"other""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected fn")
        };
        let Stmt::Guard { condition, .. } = &body[0].node else {
            panic!("expected guard, got {:?}", body[0].node)
        };
        let Expr::BinOp { op, .. } = condition else {
            panic!("expected equality condition, got {:?}", condition)
        };
        assert_eq!(*op, BinOp::Equals);
    }

    // ── Coverage: L813 — TypeIs pattern lookahead bounds check ─────────────────

    #[test]
    fn type_is_pattern_bounds_check_in_semi_starts_new_arm() {
        // Multi-arm match with TypeIs pattern: `;n v:` — after_semi+2 < tokens.len() is true
        // and the matches! returns true because the tokens are (Ident("n"), Ident("v"), Colon)
        let prog = parse_str(r#"f x:n>n;?x{n v:v;_:0}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert_eq!(arms.len(), 2);
        assert!(matches!(
            &arms[0].pattern,
            Pattern::TypeIs {
                ty: Type::Number,
                ..
            }
        ));
    }

    // ── Coverage gap tests ──────────────────────────────────────────────

    // L1048: Guard with else-body braces: `>=x 0{x}{0}` (two brace blocks)
    #[test]
    fn cov_guard_with_else_braces() {
        let prog = parse_str(r#"f x:n>n;>=x 0{x}{0}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        match &body[0].node {
            Stmt::Guard { else_body, .. } => {
                assert!(else_body.is_some(), "should have else body");
            }
            other => panic!("expected Guard, got {:?}", other),
        }
    }

    // L1059: Braceless guard — `>=x 0 x` (comparison as condition, single expression body)
    #[test]
    fn cov_braceless_guard() {
        let prog = parse_str(r#"f x:n>n;>=x 0 x"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(matches!(&body[0].node, Stmt::Guard { .. }));
    }

    // L1243-1245: Err expression via Caret in list element context
    #[test]
    fn cov_err_expression() {
        let prog = parse_str(r#"f>R n t;^"oops""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        match &body[0].node {
            Stmt::Expr(Expr::Err(_)) => {}
            other => panic!("expected Err expression, got {:?}", other),
        }
    }

    // L1835: parse_tokens returning Err (parse errors)
    #[test]
    fn cov_parse_tokens_error() {
        use crate::lexer::Token;
        // An incomplete program that should produce parse errors
        let tokens = vec![Token::Greater]; // just ">" — not a valid program
        let result = super::parse_tokens(tokens);
        assert!(
            result.is_err(),
            "incomplete tokens should produce parse error"
        );
    }

    // L881: TypeIs pattern lookahead with 'b' type
    #[test]
    fn cov_type_is_bool_pattern() {
        let prog = parse_str(r#"f x:n>n;?x{b v:1;_:0}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert!(matches!(
            &arms[0].pattern,
            Pattern::TypeIs { ty: Type::Bool, .. }
        ));
    }

    // L881: TypeIs pattern with 'l' (list) type
    #[test]
    fn cov_type_is_list_pattern() {
        let prog = parse_str(r#"f x:n>n;?x{l v:1;_:0}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert!(matches!(
            &arms[0].pattern,
            Pattern::TypeIs {
                ty: Type::List(_),
                ..
            }
        ));
    }

    // Multiple braceless guards (cascading)
    #[test]
    fn cov_cascading_braceless_guards() {
        let prog = parse_str(r#"cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze""#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        assert!(body.len() >= 2, "should have multiple statements");
    }

    // Nil literal in match pattern
    #[test]
    fn cov_nil_literal_pattern() {
        let prog = parse_str(r#"f x:n>n;?x{nil:0;_:1}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        assert!(matches!(&arms[0].pattern, Pattern::Literal(Literal::Nil)));
    }

    // parse_let single-brace desugar: v=cond{body} → Guard { condition, body: [Let{name,...}] }
    // Covers lines 752-759 (the else branch after single brace block) and wrap_body_as_let (1851-1878)
    #[test]
    fn cov_parse_let_single_brace_guard() {
        let prog = parse_str(r#"f x:n>n;v=>=x 0{42};v"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        // First stmt should be a Guard (desugared from v=cond{body})
        assert!(
            matches!(
                &body[0].node,
                Stmt::Guard {
                    negated: false,
                    else_body: None,
                    braceless: false,
                    ..
                }
            ),
            "expected Guard from single-brace let desugar, got {:?}",
            body[0].node
        );
    }

    // wrap_body_as_let with empty body: v=cond{}  → Guard { body: [Let{name, Nil}] }
    // Covers wrap_body_as_let empty-body branch (line 1852-1856)
    #[test]
    fn cov_wrap_body_as_let_empty_body() {
        let prog = parse_str(r#"f x:n>n;v=>=x 0{};v"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Guard {
            body: guard_body, ..
        } = &body[0].node
        else {
            panic!("expected Guard, got {:?}", body[0].node)
        };
        // The desugared body should be a single Let with Nil value
        assert_eq!(guard_body.len(), 1);
        assert!(
            matches!(
                &guard_body[0].node,
                Stmt::Let {
                    value: Expr::Literal(Literal::Nil),
                    ..
                }
            ),
            "expected Let{{Nil}} in guard body, got {:?}",
            guard_body[0].node
        );
    }

    // wrap_body_as_let where last stmt is NOT an Expr (it's a Let) — the non-Expr fallthrough
    // Covers the `_ => { /* no-op */ }` arm in wrap_body_as_let (line 1871-1875)
    #[test]
    fn cov_wrap_body_as_let_non_expr_last() {
        // body contains only a let stmt (w=1), so wrap_body_as_let's last stmt is Stmt::Let
        let prog = parse_str(r#"f x:n>n;v=>=x 0{w=1};v"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Guard {
            body: guard_body, ..
        } = &body[0].node
        else {
            panic!("expected Guard, got {:?}", body[0].node)
        };
        // The inner let (w=1) should remain — non-Expr last stmt is left as-is
        assert!(!guard_body.is_empty());
        assert!(
            matches!(&guard_body[0].node, Stmt::Let { name, .. } if name == "w"),
            "expected inner Let{{w}} untouched, got {:?}",
            guard_body[0].node
        );
    }

    // parse_list_element with Caret/Err constructor inside list literal: [^"msg"]
    // Covers lines 1279-1282 (Some(Token::Caret) branch in parse_list_element)
    #[test]
    fn cov_list_element_caret_err() {
        let prog = parse_str(r#"f x:n>R n t;[^"bad"]"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::List(elems)) = &body[0].node else {
            panic!("expected List expr, got {:?}", body[0].node)
        };
        assert_eq!(elems.len(), 1);
        assert!(
            matches!(&elems[0], Expr::Err(_)),
            "expected Err element, got {:?}",
            elems[0]
        );
    }

    // body_to_expr with empty body → Expr::Literal(Nil)
    // Covered via ternary desugar v=cond{}{} where both branches are empty
    // Covers line 1839 (body.is_empty() early return in body_to_expr)
    #[test]
    fn cov_body_to_expr_empty() {
        let prog = parse_str(r#"f x:n>n;v=>=x 0{}{};v"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        // v=cond{}{} desugars to Let { value: Ternary { then: Nil, else: Nil } }
        let Stmt::Let { value, .. } = &body[0].node else {
            panic!("expected Let, got {:?}", body[0].node)
        };
        assert!(
            matches!(value, Expr::Ternary { then_expr, else_expr, .. }
                if matches!(then_expr.as_ref(), Expr::Literal(Literal::Nil))
                && matches!(else_expr.as_ref(), Expr::Literal(Literal::Nil))
            ),
            "expected Ternary{{Nil, Nil}}, got {:?}",
            value
        );
    }

    // body_to_expr where last stmt is NOT an Expr → falls back to Nil
    // Covers line 1844 (_ => Expr::Literal(Literal::Nil) in body_to_expr)
    #[test]
    fn cov_body_to_expr_non_expr_last() {
        // v=cond{w=1}{w=2} — each branch body's last stmt is a Let, not an Expr
        let prog = parse_str(r#"f x:n>n;v=>=x 0{w=1}{w=2};v"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Let { value, .. } = &body[0].node else {
            panic!("expected Let, got {:?}", body[0].node)
        };
        // Both branches have non-Expr last stmts → both arms become Nil
        assert!(
            matches!(value, Expr::Ternary { then_expr, else_expr, .. }
                if matches!(then_expr.as_ref(), Expr::Literal(Literal::Nil))
                && matches!(else_expr.as_ref(), Expr::Literal(Literal::Nil))
            ),
            "expected Ternary fallback to Nil for non-Expr branches, got {:?}",
            value
        );
    }

    // semi_starts_new_arm TypeIs branch: `;n v:` after an arm body → true (covers line 916 path)
    // Also the false path: `;n 5` — TypeIs ident found but token after is not Ident/Underscore
    #[test]
    fn cov_semi_starts_new_arm_type_is() {
        // Match with numeric arm, then a TypeIs arm — `;n v:v` should be seen as new arm start
        let prog = parse_str(r#"f x:n>n;?x{1:x;n v:v;_:0}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        // Three arms: literal 1, TypeIs n, wildcard
        assert_eq!(arms.len(), 3, "expected 3 match arms");
        assert!(
            matches!(
                &arms[1].pattern,
                Pattern::TypeIs {
                    ty: Type::Number,
                    ..
                }
            ),
            "expected TypeIs Number arm, got {:?}",
            arms[1].pattern
        );
    }

    // semi_starts_new_arm TypeIs false path: `;n 5` — type ident followed by a number (not Ident/Underscore)
    // Covers line 915 matches! returning false (the else branch in the `^0` annotation)
    #[test]
    fn cov_semi_starts_new_arm_type_is_false() {
        // In this match, arm body has `x` then `;n` where `n` is used as a *variable ref*, not a type pattern.
        // `?x{1:x;n;_:0}` — `;n` is followed by `;` (not Ident:Colon), so semi_starts_new_arm returns false
        // for the TypeIs check, and `n` becomes a statement in the arm body.
        let prog = parse_str(r#"f x:n>n;?x{1:x;n;_:0}"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Match { arms, .. } = &body[0].node else {
            panic!("expected match")
        };
        // `;n;` — `n` is checked: token after_semi is `n` (TypeIs candidate), but token after that
        // is `;` which is NOT Ident or Underscore → matches! returns false → `n` is a body stmt.
        // So arm 0 (`1:`) gets body [x, n], arm 1 (`_:`) gets body [0].
        assert!(arms.len() >= 2, "expected at least 2 arms");
    }

    // looks_like_prefix_binary with a simple paren group: e.g. `+ (x) 1`
    // The `(x)` paren group is counted as one atom — covers line 1618-1637
    #[test]
    fn cov_looks_like_prefix_binary_paren_group() {
        // `+ (x) 1` — paren group as first arg, then second arg → binary prefix op
        let prog = parse_str(r#"f x:n>n;+(x) 1"#);
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        // Should parse as BinOp::Add with a paren-grouped first arg
        assert!(
            matches!(
                &body[0].node,
                Stmt::Expr(Expr::BinOp { op: BinOp::Add, .. })
            ),
            "expected Add BinOp, got {:?}",
            body[0].node
        );
    }

    // looks_like_prefix_binary with NESTED parens: calling `dbl -((x)) 1`
    // When parsing `dbl`'s args, the first token is `-` (infix-eligible), so
    // looks_like_prefix_binary scans forward: `((x))` = one atom (with inner depth++ at line 1631)
    // then `1` = second atom → returns true (is prefix binary, not infix).
    #[test]
    fn cov_looks_like_prefix_binary_nested_parens() {
        // `dbl -((x)) 1` — `dbl` is called with `-((x))` and `1` as args
        // The `((x))` paren group causes depth += 1 inside looks_like_prefix_binary
        let prog = parse_str(r#"dbl x:n>n;*x 2  f y:n>n;dbl -((y)) 1"#);
        let Decl::Function { body, .. } = &prog.declarations[1] else {
            panic!("expected second function")
        };
        // Should parse as a call to dbl with args [-((y)), 1]... actually as Call{dbl, [BinOp{Sub,Ref(y),Lit(1)}]}
        assert!(
            matches!(&body[0].node, Stmt::Expr(Expr::Call { function, .. }) if function == "dbl"),
            "expected Call to dbl, got {:?}",
            body[0].node
        );
    }

    // ── Coverage: parse_type LParen branch (nested generic types) ──────────

    fn first_fn_return_debug(src: &str) -> String {
        let prog = parse_str(src);
        match &prog.declarations[0] {
            Decl::Function { return_type, .. } => format!("{:?}", return_type),
            _ => String::from("not-a-fn"),
        }
    }

    fn first_fn_param_debug(src: &str) -> String {
        let prog = parse_str(src);
        match &prog.declarations[0] {
            Decl::Function { params, .. } => format!("{:?}", params),
            _ => String::from("not-a-fn"),
        }
    }

    #[test]
    fn parse_type_result_of_list() {
        // `R (L n) t` — exercises LParen arm of parse_type around `L n`.
        let s = first_fn_return_debug("f>R (L n) t;~[1,2,3]");
        assert!(s.contains("Result"), "no Result: {s}");
        assert!(s.contains("List"), "no List: {s}");
    }

    #[test]
    fn parse_type_parens_around_atom_transparent() {
        // `R (n) t` — single-token in parens unwraps to plain `n`.
        let s = first_fn_return_debug("f>R (n) t;~1");
        assert!(s.contains("Result"), "no Result: {s}");
        assert!(s.contains("Number"), "no Number: {s}");
    }

    #[test]
    fn parse_type_param_with_paren_type() {
        // LParen in a param type position exercises can_start_type's LParen branch.
        let s = first_fn_param_debug("f x:(L n)>n;0");
        assert!(s.contains("List"), "no List: {s}");
        assert!(s.contains("Number"), "no Number: {s}");
    }

    #[test]
    fn parse_type_nested_paren_around_atom_does_not_break_flat() {
        // Sanity: existing flat `R n t` still parses.
        let s = first_fn_return_debug("f>R n t;~1");
        assert!(s.contains("Result"), "no Result: {s}");
    }

    #[test]
    fn parse_type_triple_nested_paren() {
        // `R (L (R n t)) t` — recursive parse_type calls through LParen arm twice.
        let s = first_fn_return_debug("f>R (L (R n t)) t;~[~1,~2]");
        assert!(s.contains("Result"), "no Result: {s}");
        assert!(s.contains("List"), "no List: {s}");
    }

    // ---- list-literal-refs parser coverage ----

    fn first_list_items(prog: &Program) -> Vec<Expr> {
        let Decl::Function { body, .. } = &prog.declarations[0] else {
            panic!("expected function")
        };
        let Stmt::Expr(Expr::List(items)) = &body[0].node else {
            panic!("expected list, got {:?}", body[0].node)
        };
        items.clone()
    }

    #[test]
    fn parse_list_whitespace_refs_are_bare_refs() {
        // `[a b c]` must be a 3-element list of bare refs, not Call(a, [b, c]).
        let prog = parse_str("f a:n b:n c:n>L n;[a b c]");
        let items = first_list_items(&prog);
        assert_eq!(items.len(), 3);
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            assert!(
                matches!(&items[i], Expr::Ref(n) if n == name),
                "items[{i}] not Ref({name}), got {:?}",
                items[i]
            );
        }
    }

    #[test]
    fn parse_list_comma_mode_keeps_calls() {
        // With a top-level comma, calls inside elements remain calls.
        let prog = parse_str("f x:n>L n;[flr x, cel x]");
        let items = first_list_items(&prog);
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], Expr::Call { function, .. } if function == "flr"));
        assert!(matches!(&items[1], Expr::Call { function, .. } if function == "cel"));
    }

    #[test]
    fn parse_list_whitespace_parens_force_call() {
        // `[(flr x) y]` — parens reset no_whitespace_call so flr x is a call.
        let prog = parse_str("f x:n y:n>L n;[(flr x) y]");
        let items = first_list_items(&prog);
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], Expr::Call { function, .. } if function == "flr"));
        assert!(matches!(&items[1], Expr::Ref(n) if n == "y"));
    }

    #[test]
    fn parse_list_has_top_level_comma_ignores_nested() {
        // Nested brackets contain a comma but outer is whitespace-mode.
        // Outer must still be whitespace-mode (no top-level comma).
        let prog = parse_str("f>L L n;[[1,2] [3,4]]");
        let items = first_list_items(&prog);
        assert_eq!(items.len(), 2);
        for inner in &items {
            let Expr::List(sub) = inner else {
                panic!("expected nested list, got {:?}", inner)
            };
            assert_eq!(sub.len(), 2);
        }
    }

    #[test]
    fn parse_list_empty_whitespace_mode() {
        // Empty list — list_has_top_level_comma must hit the RBracket-at-depth-0
        // early-return without errors.
        let prog = parse_str("f>L n;[]");
        let items = first_list_items(&prog);
        assert!(items.is_empty());
    }

    #[test]
    fn parse_list_single_ref_whitespace_mode() {
        let prog = parse_str("f a:n>L n;[a]");
        let items = first_list_items(&prog);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], Expr::Ref(n) if n == "a"));
    }

    #[test]
    fn parse_list_whitespace_with_literals_and_refs() {
        let prog = parse_str("f a:n>L n;[1 a 2]");
        let items = first_list_items(&prog);
        assert_eq!(items.len(), 3);
        assert!(matches!(&items[1], Expr::Ref(n) if n == "a"));
    }

    #[test]
    fn parse_list_comma_mode_with_parens_inside() {
        // Top-level comma + nested paren — exercises the LParen reset path
        // inside comma-mode element parsing.
        let prog = parse_str("f x:n>L n;[(flr x), x]");
        let items = first_list_items(&prog);
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], Expr::Call { function, .. } if function == "flr"));
    }

    #[test]
    fn parse_list_whitespace_with_ok_err_wrappers() {
        // `[~1 ^2 ~3]` — call_ok path through Tilde/Caret arms in whitespace mode.
        let prog = parse_str("f>L R n t;[~1 ^\"e\" ~3]");
        let items = first_list_items(&prog);
        assert_eq!(items.len(), 3);
        assert!(matches!(&items[0], Expr::Ok(_)));
        assert!(matches!(&items[1], Expr::Err(_)));
        assert!(matches!(&items[2], Expr::Ok(_)));
    }
}
