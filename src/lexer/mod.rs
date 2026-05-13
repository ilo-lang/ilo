use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(skip r"[ \t]+")]
#[logos(skip(r"--[^\n]*", allow_greedy = true))]
pub enum Token {
    // Keywords
    #[token("type")]
    Type,
    #[token("tool")]
    Tool,
    #[token("use")]
    Use,
    #[token("with")]
    With,
    #[token("timeout")]
    Timeout,
    #[token("retry")]
    Retry,

    // Type constructors (uppercase)
    #[token("L")]
    ListType,
    #[token("R")]
    ResultType,
    #[token("F")]
    FnType,
    #[token("O")]
    OptType,
    #[token("M")]
    MapType,
    #[token("S")]
    SumType,

    // Reserved keywords from other languages — not valid in ilo, emit friendly errors
    #[token("if")]
    KwIf,
    #[token("return")]
    KwReturn,
    #[token("let")]
    KwLet,
    #[token("fn")]
    KwFn,
    #[token("def")]
    KwDef,
    #[token("var")]
    KwVar,
    #[token("const")]
    KwConst,

    // Boolean literals
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("nil")]
    Nil,

    // Multi-char operators (greedy — must come before single-char)
    #[token(">=")]
    GreaterEq,
    #[token("<=")]
    LessEq,
    #[token("!=")]
    NotEq,
    #[token("+=")]
    PlusEq,
    #[token(">>")]
    PipeOp,
    #[token("??")]
    NilCoalesce,

    // Single-char operators
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token(">")]
    Greater,
    #[token("<")]
    Less,
    #[token("=")]
    #[token("==")]
    Eq,
    #[token("&")]
    Amp,
    #[token("|")]
    Pipe,

    // Special
    #[token("?")]
    Question,
    #[token("@")]
    At,
    #[token("!")]
    Bang,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
    #[token("$")]
    Dollar,

    // Punctuation
    #[token(":")]
    Colon,
    #[token(";")]
    Semi,
    #[token("..")]
    DotDot,
    #[token(".?")]
    DotQuestion,
    #[token(".")]
    Dot,
    #[token(",")]
    Comma,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("_")]
    Underscore,

    // Literals
    #[regex(r"-?[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?", |lex| lex.slice().parse::<f64>().ok())]
    Number(f64),

    #[regex(r#""[^"\\]*(?:\\.[^"\\]*)*""#, |lex| {
        let s = lex.slice();
        let inner = &s[1..s.len()-1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some(other) => { out.push('\\'); out.push(other); }
                    None => {}
                }
            } else {
                out.push(c);
            }
        }
        Some(out)
    })]
    Text(String),

    // Identifiers: lowercase with hyphens
    #[regex(r"[a-z][a-z0-9]*(-[a-z0-9]+)*", |lex| lex.slice().to_string(), priority = 1)]
    Ident(String),

    // Newlines (kept for line tracking, parser skips them)
    #[token("\n")]
    Newline,
}

/// Convert indented newlines to semicolons so multi-line file format works.
///
/// Rules:
/// - `\n` followed by whitespace (indented continuation) → `;`
/// - `\n` at column 0 (new declaration) → kept as `\n`
/// - `;` immediately after `{` or before `}` → removed
/// - Inside `(...)` or `[...]` (list literal, paren-group, fn-call arg list),
///   `\n` is treated as whitespace: no `;` is emitted, so multi-line list and
///   paren expressions parse correctly. String literals are walked through so
///   `(`/`[` inside text don't affect depth.
/// - Continuation lines starting with `>>` (pipe operator) suppress the `;`
///   so `xs\n  >>map{...}` chains correctly. `>>` is never a valid statement
///   start, so this is unambiguous. Other operators (`+`, `-`, `*`, ...) are
///   valid prefix-call statement heads and are NOT special-cased.
pub fn normalize_newlines(source: &str) -> String {
    if !source.contains('\n') {
        return source.to_string();
    }

    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    // Track the last non-whitespace char pushed to `out` to avoid O(n) trim_end scans.
    let mut last_significant: Option<char> = None;
    // Depth of open `(` and `[` we're currently inside. `{` is tracked
    // separately by `last_significant` (existing precedent).
    let mut bracket_depth: u32 = 0;

    while let Some(c) = chars.next() {
        if c == '"' {
            // Pass through string literal content verbatim so `--` inside a
            // string isn't mistaken for a comment, `\n` (if ever present
            // inside a string) isn't rewritten to `;`, and `(`/`[` inside
            // text don't bump bracket depth. Mirrors logos's string regex:
            // closing quote terminates unless escaped.
            out.push(c);
            last_significant = Some(c);
            while let Some(sc) = chars.next() {
                out.push(sc);
                if sc == '\\' {
                    if let Some(esc) = chars.next() {
                        out.push(esc);
                    }
                } else if sc == '"' {
                    last_significant = Some(sc);
                    break;
                }
            }
        } else if c == '-' && chars.peek() == Some(&'-') {
            // `--` starts a line comment. Drop the comment content (including
            // both dashes) up to but not including the next `\n`, so the
            // following `\n` is handled normally by the loop. This matches the
            // logos `--[^\n]*` skip rule but runs BEFORE newline normalization,
            // so an indented comment line doesn't bleed `;` separators into
            // the comment body where the logos regex would then swallow them.
            chars.next(); // consume second '-'
            while let Some(&nc) = chars.peek() {
                if nc == '\n' {
                    break;
                }
                chars.next();
            }
            // Do not push anything; do not update last_significant. The
            // surrounding `\n` handling on the next loop iteration emits the
            // appropriate `;` or newline based on the line that follows.
        } else if c == '\n' {
            // Inside `(...)` or `[...]`, treat newlines as whitespace —
            // don't emit `;` or `\n`, but emit a single space so tokens on
            // adjacent lines don't get glued together (e.g. `(+x\n  1)`
            // must not become `(+x1)`). Then skip indent on the next line.
            if bracket_depth > 0 {
                out.push(' ');
                while matches!(chars.peek(), Some(' ') | Some('\t')) {
                    chars.next();
                }
                continue;
            }
            // Check if next line is indented (starts with space or tab)
            if matches!(chars.peek(), Some(' ') | Some('\t')) {
                // Peek past indent at the first real char on the next line
                // so we can decide whether to emit a `;` before it.
                let mut lookahead = chars.clone();
                while matches!(lookahead.peek(), Some(' ') | Some('\t')) {
                    lookahead.next();
                }
                // `>>` (pipe operator) at the start of a continuation line is
                // never a statement start — it must be chaining the previous
                // line's expression. Suppress the `;` so the chain parses.
                // Other operators (`+`/`-`/`*`) are valid prefix-call
                // statement starts and must NOT trigger this.
                let next_is_pipe = {
                    let mut probe = lookahead.clone();
                    probe.next() == Some('>') && probe.next() == Some('>')
                };
                // Indented continuation → emit `;` and skip the whitespace
                // But first check if the last non-whitespace char was `{` — if so, skip the `;`
                // Also skip if `out` already ends in `;` (e.g. previous line
                // was a comment that produced no significant output), or if
                // the continuation begins with `>>` (pipe chain).
                if last_significant == Some('{') || out.ends_with(';') || next_is_pipe {
                    // Don't emit `;`
                } else {
                    out.push(';');
                }
                // Skip leading whitespace on the continuation line
                while matches!(chars.peek(), Some(' ') | Some('\t')) {
                    chars.next();
                }
                // If the continuation line starts with `}`, don't add `;` before it
                if chars.peek() == Some(&'}') && last_significant != Some('{') && out.ends_with(';')
                {
                    out.pop(); // remove the `;` we just pushed
                }
            } else if chars.peek() == Some(&'}') {
                // Non-indented `}` closes a block — don't emit newline
            } else {
                // Not indented → keep newline (declaration boundary)
                out.push('\n');
            }
        } else {
            out.push(c);
            if !c.is_ascii_whitespace() {
                last_significant = Some(c);
            }
            match c {
                '(' | '[' => bracket_depth += 1,
                ')' | ']' => {
                    bracket_depth = bracket_depth.saturating_sub(1);
                }
                _ => {}
            }
        }
    }

    out
}

/// Lex source code into a stream of tokens with positions.
pub fn lex(source: &str) -> Result<Vec<(Token, std::ops::Range<usize>)>, LexError> {
    let normalized = normalize_newlines(source);
    let mut lexer = Token::lexer(&normalized);
    let mut tokens: Vec<(Token, std::ops::Range<usize>)> = Vec::new();

    while let Some(result) = lexer.next() {
        match result {
            Ok(token) => {
                let span = lexer.span();
                // Detect uppercase mid-identifier: a single uppercase type sigil
                // (L/R/F/O/M/S) sitting flush against a preceding ident.
                if is_type_sigil(&token) {
                    let prev_info = tokens.last().and_then(|(t, s)| match t {
                        Token::Ident(name) if s.end == span.start => {
                            Some((name.clone(), s.clone()))
                        }
                        _ => None,
                    });
                    if let Some((prev_name, prev_span)) = prev_info {
                        // At a post-dot field-access position, real-world
                        // JSON (NVD, AWS, Stripe, GitHub) is overwhelmingly
                        // camelCase. Absorb the rest of the camelCase run
                        // into a single Ident token rather than erroring.
                        // The strict lowercase rule still applies to
                        // bindings (no preceding Dot/DotQuestion).
                        if prev_ident_is_post_dot(&tokens) {
                            if let Some(_consumed) = absorb_camel_tail(
                                &normalized,
                                span.start,
                                span.end,
                                &mut lexer,
                                &mut tokens,
                            ) {
                                continue;
                            }
                        }
                        let sigil_char = normalized[span.clone()].chars().next().unwrap();
                        return Err(uppercase_mid_ident_error(
                            &prev_name,
                            sigil_char,
                            &normalized[span.end..],
                            prev_span.start,
                        ));
                    }
                }
                tokens.push((token, span));
            }
            Err(()) => {
                let span = lexer.span();
                let bad = &normalized[span.clone()];
                // Single uppercase ASCII letter directly after an ident is a
                // mid-identifier capital (e.g. `isAgg` → `is` + bad `A`).
                if bad.len() == 1 && bad.chars().next().unwrap().is_ascii_uppercase() {
                    let prev_info = tokens.last().and_then(|(t, s)| match t {
                        Token::Ident(name) if s.end == span.start => {
                            Some((name.clone(), s.clone()))
                        }
                        _ => None,
                    });
                    if let Some((prev_name, prev_span)) = prev_info {
                        // Post-dot field access: merge the camelCase tail into
                        // the preceding Ident (mirrors the snake_case post-pass
                        // below). Bindings still error normally.
                        if prev_ident_is_post_dot(&tokens) {
                            if let Some(_consumed) = absorb_camel_tail(
                                &normalized,
                                span.start,
                                span.end,
                                &mut lexer,
                                &mut tokens,
                            ) {
                                continue;
                            }
                        }
                        let c = bad.chars().next().unwrap();
                        return Err(uppercase_mid_ident_error(
                            &prev_name,
                            c,
                            &normalized[span.end..],
                            prev_span.start,
                        ));
                    }
                }
                let (code, suggestion) = lex_error_kind(bad);
                return Err(LexError {
                    code,
                    position: span.start,
                    snippet: bad.to_string(),
                    suggestion,
                });
            }
        }
    }

    // Post-lex: split `Dot Number(N.M)` into `Dot Number(N) Dot Number(M)` so
    // that chained literal-int dot-index access on nested lists parses correctly.
    // Source `xs.0.0` tokenises as `Ident Dot Number(0.0)` because the number
    // regex is greedy — without this pass the trailing `.0` is swallowed by the
    // float literal and the second index disappears. Only fires when the Number
    // immediately follows a Dot/DotQuestion (no whitespace) and its source slice
    // contains a `.` but no exponent, so genuine floats like `1e2` or `f 1.5` are
    // untouched.
    {
        let mut i = 0;
        while i < tokens.len() {
            if i == 0 {
                i += 1;
                continue;
            }
            let prev_is_dot = matches!(tokens[i - 1].0, Token::Dot | Token::DotQuestion)
                && tokens[i - 1].1.end == tokens[i].1.start;
            if !prev_is_dot {
                i += 1;
                continue;
            }
            let Token::Number(_) = tokens[i].0 else {
                i += 1;
                continue;
            };
            let span = tokens[i].1.clone();
            let slice = &normalized[span.clone()];
            if slice.contains('e') || slice.contains('E') || slice.starts_with('-') {
                i += 1;
                continue;
            }
            let Some(dot_at) = slice.find('.') else {
                i += 1;
                continue;
            };
            let head = &slice[..dot_at];
            let tail = &slice[dot_at + 1..];
            let (Ok(h), Ok(t)) = (head.parse::<f64>(), tail.parse::<f64>()) else {
                i += 1;
                continue;
            };
            let head_span = span.start..span.start + dot_at;
            let dot_span = span.start + dot_at..span.start + dot_at + 1;
            let tail_span = span.start + dot_at + 1..span.end;
            tokens.splice(
                i..i + 1,
                [
                    (Token::Number(h), head_span),
                    (Token::Dot, dot_span),
                    (Token::Number(t), tail_span),
                ],
            );
            // Advance past the new triple; the new tail Number could itself
            // be followed by another `.` outside the slice, but additional
            // chaining (xs.0.0.0) would already be split because the lexer
            // emitted distinct tokens for the next group.
            i += 3;
        }
    }

    // Post-lex: split a glued negative-literal `Number(-N)` back into
    // `Minus` + `Number(N)` when the preceding token is one that introduces
    // a fresh expression position. Six personas hit this in the assessment
    // log: writing `-0 v` (intending `0 - v`) silently produces wrong results
    // because Logos's `-?[0-9]+...` regex greedily consumes the leading `-`,
    // so the parser sees `Number(-0)` followed by a stray `Ref(v)`. Same trap
    // for `-1 cv`, `r1=-1 t2`, `v=p.1;-0 v`, etc. The canonical workaround is
    // adding a space (`- 0 v`) but it's an easy-to-forget tax on numerical
    // formulas.
    //
    // The split is gated on the *preceding* token rather than blanket-applied
    // so that legitimate negative-literal-as-call-arg cases are preserved:
    // `at xs -1`, `+a -3`, `into -3 0 10`, `<r -0.05`, `[1 -2 3]` all keep
    // their `Number(-N)` token because the preceding token is value-producing
    // (Ident/Number/etc). `LBracket` is *also* excluded so that
    // `[-2 1 3]` (a comma-free list literal whose first element is negative)
    // continues to lex as four tokens — splitting it would make the parser
    // greedy-subtract `-2 1` into `Subtract(2, 1)` and silently produce a
    // 2-element list.
    //
    // Contexts that *do* split:
    //   - start of input (no previous token)
    //   - `;` (statement boundary)
    //   - `\n` (declaration boundary, after normalize_newlines)
    //   - `=` (rhs of an assignment)
    //   - `{` (start of a block - function body, conditional arm)
    //   - `(` (start of a parenthesised expression)
    //
    // After splitting, the parser's existing `parse_minus` handles both
    // `Negate(N)` (no following operand) and `Subtract(N, M)` (operand
    // follows), so the unary-negation case at expression start (`a=-3`)
    // still produces the same `-3` value via `Negate(3)`.
    {
        let mut i = 0;
        while i < tokens.len() {
            let Token::Number(_) = tokens[i].0 else {
                i += 1;
                continue;
            };
            let span = tokens[i].1.clone();
            let slice = &normalized[span.clone()];
            if !slice.starts_with('-') {
                i += 1;
                continue;
            }
            let prev_splits = i == 0
                || matches!(
                    tokens[i - 1].0,
                    Token::Semi | Token::Newline | Token::Eq | Token::LBrace | Token::LParen
                );
            if !prev_splits {
                i += 1;
                continue;
            }
            // Re-parse the positive tail (skip the leading `-`) so the new
            // Number carries the correct value. The slice is guaranteed by
            // the lexer regex to be a valid f64 literal.
            let positive_slice = &slice[1..];
            let Ok(n) = positive_slice.parse::<f64>() else {
                i += 1;
                continue;
            };
            let minus_span = span.start..span.start + 1;
            let number_span = span.start + 1..span.end;
            tokens.splice(
                i..i + 1,
                [(Token::Minus, minus_span), (Token::Number(n), number_span)],
            );
            // Step past both new tokens - the new Number is not itself a
            // candidate for re-splitting (its slice doesn't start with `-`).
            i += 2;
        }
    }

    // Post-lex: after `.` or `.?` (field access), accept reserved keywords
    // (`type`, `if`, `let`, `fn`, `var`, `use`, `with`, type sigils `R`/`L`/`F`/`O`/`M`/`S`,
    // `true`, `false`, `nil`, ...) as plain field names by rewriting the keyword
    // token back into a `Token::Ident` using the original source slice. Real-world
    // JSON keys are frequently named after keywords (`type`, `if`, `use`), and
    // dot-access on those should "just work" — the workaround was the verbose
    // `jpth! resp "type"` per field. Only fires when the keyword token sits flush
    // against a preceding `Dot`/`DotQuestion` (no whitespace), so reserved words
    // in binding position still emit their friendly ILO-P011 error.
    //
    // This runs before the snake_case pass below so `record.type_id` correctly
    // stitches: after this pass the token sequence becomes
    // `Dot Ident("type") Underscore Ident("id")`, then the snake_case loop merges
    // it into `Dot Ident("type_id")`.
    {
        let mut i = 0;
        while i < tokens.len() {
            if i == 0 {
                i += 1;
                continue;
            }
            let prev_is_dot = matches!(tokens[i - 1].0, Token::Dot | Token::DotQuestion)
                && tokens[i - 1].1.end == tokens[i].1.start;
            if !prev_is_dot {
                i += 1;
                continue;
            }
            if matches!(tokens[i].0, Token::Ident(_) | Token::Number(_)) {
                i += 1;
                continue;
            }
            let span = tokens[i].1.clone();
            let slice = &normalized[span.clone()];
            // Only rewrite tokens whose source slice is a valid bare field name —
            // identifier-shaped (`[A-Za-z][A-Za-z0-9_]*`). This catches keyword
            // tokens (`type`, `if`, `use`, ...) and type sigils (`R`, `L`, `F`,
            // `O`, `M`, `S`), but skips punctuation like `..` or `.?` that the
            // lexer happens to emit as non-Ident tokens.
            let mut chars = slice.chars();
            let first_ok = chars
                .next()
                .map(|c| c.is_ascii_alphabetic())
                .unwrap_or(false);
            let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !first_ok || !rest_ok {
                i += 1;
                continue;
            }
            tokens[i] = (Token::Ident(slice.to_string()), span);
            i += 1;
        }
    }

    // Post-lex: after `.` or `.?` (field access), accept JSON-style snake_case
    // field names by merging contiguous `Ident (Underscore (Ident|Number))*`
    // runs back into a single `Ident` token. Real-world JSON (which agents
    // consume via `jpar!`) is overwhelmingly snake_case (`stargazers_count`,
    // `change_1d`, ...), and dot-access on those keys is the canonical path.
    // The strict identifier rule (lowercase + hyphens) still applies to
    // bindings, so `my_var=5` keeps emitting ILO-L002 below.
    let mut i = 0;
    while i + 2 < tokens.len() {
        let prev_is_dot = i > 0
            && matches!(tokens[i - 1].0, Token::Dot | Token::DotQuestion)
            && tokens[i - 1].1.end == tokens[i].1.start;
        if !prev_is_dot {
            i += 1;
            continue;
        }
        if !matches!(tokens[i].0, Token::Ident(_)) {
            i += 1;
            continue;
        }
        // Greedily collect contiguous `_ (Ident | integer Number Ident?)`
        // segments. Each `_Number` group may also absorb a trailing letter
        // glued to the number (e.g. `change_1d`, `x_2y_3z`), and the loop
        // continues afterward so alternating segments like
        // `ema_20d_change_5d` stitch fully.
        let mut j = i + 1;
        let mut has_underscore = false;
        while j + 1 < tokens.len()
            && tokens[j].0 == Token::Underscore
            && tokens[j - 1].1.end == tokens[j].1.start
            && tokens[j].1.end == tokens[j + 1].1.start
        {
            match &tokens[j + 1].0 {
                Token::Ident(_) => {
                    has_underscore = true;
                    j += 2;
                }
                Token::Number(n) if n.fract() == 0.0 && *n >= 0.0 => {
                    has_underscore = true;
                    j += 2;
                    // Absorb a trailing letter glued to the number
                    // (e.g. the `d` in `change_1d`).
                    if j < tokens.len()
                        && tokens[j - 1].1.end == tokens[j].1.start
                        && matches!(tokens[j].0, Token::Ident(_))
                    {
                        j += 1;
                    }
                }
                _ => break,
            }
        }
        if !has_underscore {
            i += 1;
            continue;
        }
        let start = tokens[i].1.start;
        let end = tokens[j - 1].1.end;
        let merged = normalized[start..end].to_string();
        let new_tok = (Token::Ident(merged), start..end);
        tokens.splice(i..j, std::iter::once(new_tok));
        i += 1;
    }

    // Post-lex: detect underscore-separated identifier fragments like
    // `rev_ps` → Ident("rev"), Underscore, Ident("ps") with no whitespace.
    for i in 0..tokens.len().saturating_sub(2) {
        let (a, sa) = (&tokens[i].0, &tokens[i].1);
        let (b, sb) = (&tokens[i + 1].0, &tokens[i + 1].1);
        let (c, sc) = (&tokens[i + 2].0, &tokens[i + 2].1);
        if matches!(a, Token::Ident(_))
            && *b == Token::Underscore
            && matches!(c, Token::Ident(_))
            && sa.end == sb.start
            && sb.end == sc.start
        {
            let Token::Ident(ap) = a else { unreachable!() };
            let Token::Ident(cp) = c else { unreachable!() };
            // Greedily collect any further `_ident` pairs in the same run.
            let mut combined = format!("{ap}_{cp}");
            let mut end = sc.end;
            let mut j = i + 3;
            while j + 1 < tokens.len()
                && tokens[j].0 == Token::Underscore
                && matches!(tokens[j + 1].0, Token::Ident(_))
                && tokens[j - 1].1.end == tokens[j].1.start
                && tokens[j].1.end == tokens[j + 1].1.start
            {
                if let Token::Ident(s) = &tokens[j + 1].0 {
                    combined.push('_');
                    combined.push_str(s);
                }
                end = tokens[j + 1].1.end;
                j += 2;
            }
            return Err(LexError {
                code: "ILO-L002",
                position: sa.start,
                snippet: normalized[sa.start..end].to_string(),
                suggestion: format!(
                    "underscores are not allowed in identifiers; use hyphens (e.g. `{}`)",
                    combined.replace('_', "-")
                ),
            });
        }
    }

    Ok(tokens)
}

/// True when the last token is an `Ident` and the token before it is a
/// `Dot`/`DotQuestion` sitting flush against it — i.e. the Ident is in
/// post-dot field-access position (`record.<ident>` or `record.?<ident>`).
fn prev_ident_is_post_dot(tokens: &[(Token, std::ops::Range<usize>)]) -> bool {
    let n = tokens.len();
    if n < 2 {
        return false;
    }
    let (last_tok, last_span) = &tokens[n - 1];
    let (prev_tok, prev_span) = &tokens[n - 2];
    matches!(last_tok, Token::Ident(_))
        && matches!(prev_tok, Token::Dot | Token::DotQuestion)
        && prev_span.end == last_span.start
}

/// Absorb a camelCase JSON-key tail into the preceding `Ident` token.
///
/// Called from the main lex loop when an uppercase character appears flush
/// against a post-dot `Ident` (e.g. the `S` in `record.baseSeverity`). Scans
/// `normalized` from `from` consuming `[A-Za-z0-9]` characters, replaces the
/// last token with a merged `Ident` spanning `prev_span.start..end`, and
/// advances the logos lexer past the absorbed bytes. Returns `Some(end)` on
/// success, `None` if nothing was absorbed (defensive — caller falls through
/// to the existing error path).
///
/// Underscores are deliberately excluded here: snake_case stitching is handled
/// by the dedicated post-lex pass below so that mixed `gitURL_count` still
/// works (camelCase merges first, then the snake pass picks up the `_count`).
fn absorb_camel_tail(
    normalized: &str,
    span_start: usize,
    span_end: usize,
    lexer: &mut logos::Lexer<'_, Token>,
    tokens: &mut Vec<(Token, std::ops::Range<usize>)>,
) -> Option<usize> {
    let bytes = normalized.as_bytes();
    let mut end = span_start;
    while end < bytes.len() {
        let b = bytes[end];
        if b.is_ascii_alphanumeric() {
            end += 1;
        } else {
            break;
        }
    }
    if end == span_start {
        return None;
    }
    let (prev_tok, prev_span) = tokens.pop()?;
    let Token::Ident(_) = prev_tok else {
        // Defensive: caller already checked, but restore on mismatch.
        tokens.push((prev_tok, prev_span));
        return None;
    };
    let merged_span = prev_span.start..end;
    let merged = normalized[merged_span.clone()].to_string();
    tokens.push((Token::Ident(merged), merged_span));
    // Advance the logos lexer past the bytes we just absorbed. Logos has
    // already consumed up to `span_end` (the end of the offending token —
    // either the type sigil's 1 byte or the rejected uppercase byte), so
    // bump by the remaining extent.
    let bump = end.saturating_sub(span_end);
    if bump > 0 {
        lexer.bump(bump);
    }
    Some(end)
}

fn is_type_sigil(t: &Token) -> bool {
    matches!(
        t,
        Token::ListType
            | Token::ResultType
            | Token::FnType
            | Token::OptType
            | Token::MapType
            | Token::SumType
    )
}

fn uppercase_mid_ident_error(
    prev: &str,
    cap: char,
    rest_after_cap: &str,
    start: usize,
) -> LexError {
    // Reconstruct the offending identifier by reading trailing [A-Za-z0-9-] chars
    // so hyphenated tails like `isHello-world` are echoed in full.
    let trailing: String = rest_after_cap
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    let offset = prev.len();
    let full = format!("{prev}{cap}{trailing}");
    let lower = full.to_lowercase();
    let hyphenated = {
        let mut s = String::with_capacity(full.len() + 2);
        for (i, c) in full.chars().enumerate() {
            if i > 0 && c.is_ascii_uppercase() && !s.ends_with('-') {
                s.push('-');
            }
            s.push(c.to_ascii_lowercase());
        }
        s
    };
    LexError {
        code: "ILO-L003",
        position: start,
        snippet: full.clone(),
        suggestion: format!(
            "identifiers must be lowercase ASCII; got '{full}' (capital '{cap}' at offset {offset}). Use lowercase, e.g. `{hyphenated}` or `{lower}`"
        ),
    }
}

fn lex_error_kind(bad_token: &str) -> (&'static str, String) {
    if bad_token.contains('_') && bad_token.len() > 1 {
        (
            "ILO-L002",
            format!(
                "Use hyphens instead of underscores: '{}'",
                bad_token.replace('_', "-")
            ),
        )
    } else if bad_token.chars().next().is_some_and(|c| c.is_uppercase()) && bad_token.len() > 1 {
        (
            "ILO-L003",
            format!("Use lowercase: '{}'", bad_token.to_lowercase()),
        )
    } else {
        (
            "ILO-L001",
            format!("Unexpected character(s): '{bad_token}'"),
        )
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Lex error at position {position}: '{snippet}'. {suggestion}")]
pub struct LexError {
    pub code: &'static str,
    pub position: usize,
    pub snippet: String,
    pub suggestion: String,
}

#[cfg(test)]
#[allow(clippy::approx_constant)]
mod tests {
    use super::*;

    #[test]
    fn lex_simple_function() {
        let source = "tot p:n q:n r:n>n;s=*p q;t=*s r;+s t";
        let tokens = lex(source).unwrap();
        assert!(!tokens.is_empty());
        // First token should be identifier "tot"
        assert_eq!(tokens[0].0, Token::Ident("tot".to_string()));
    }

    #[test]
    fn lex_operators() {
        let source = ">=<=!=><+-*/";
        let tokens = lex(source).unwrap();
        let types: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(
            types,
            vec![
                Token::GreaterEq,
                Token::LessEq,
                Token::NotEq,
                Token::Greater,
                Token::Less,
                Token::Plus,
                Token::Minus,
                Token::Star,
                Token::Slash,
            ]
        );
    }

    #[test]
    fn lex_special_tokens() {
        let source = "?@!^~$";
        let tokens = lex(source).unwrap();
        let types: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(
            types,
            vec![
                Token::Question,
                Token::At,
                Token::Bang,
                Token::Caret,
                Token::Tilde,
                Token::Dollar
            ]
        );
    }

    #[test]
    fn lex_type_constructors() {
        let source = "L R";
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].0, Token::ListType);
        assert_eq!(tokens[1].0, Token::ResultType);
    }

    #[test]
    fn lex_keywords_vs_idents() {
        let source = "type tool with timeout retry";
        let tokens = lex(source).unwrap();
        let types: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(
            types,
            vec![
                Token::Type,
                Token::Tool,
                Token::With,
                Token::Timeout,
                Token::Retry,
            ]
        );
    }

    #[test]
    fn lex_string_literal() {
        let source = r#""hello world""#;
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].0, Token::Text("hello world".to_string()));
    }

    #[test]
    fn lex_comment_ignored() {
        let source = "-- this is a comment\ntot";
        let tokens = lex(source).unwrap();
        assert!(
            tokens
                .iter()
                .any(|(t, _)| *t == Token::Ident("tot".to_string()))
        );
    }

    #[test]
    fn lex_punctuation() {
        let source = ":;.,{}()_";
        let tokens = lex(source).unwrap();
        let types: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(
            types,
            vec![
                Token::Colon,
                Token::Semi,
                Token::Dot,
                Token::Comma,
                Token::LBrace,
                Token::RBrace,
                Token::LParen,
                Token::RParen,
                Token::Underscore,
            ]
        );
    }

    #[test]
    fn lex_number_literals() {
        let source = "42 3.14 -7";
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].0, Token::Number(42.0));
        assert_eq!(tokens[1].0, Token::Number(3.14));
        // After a value-producing token (Number), `-7` stays a negative
        // literal so call-arg patterns like `f 1 -7` keep their meaning.
        assert_eq!(tokens[2].0, Token::Number(-7.0));
    }

    /// Negative-literal-vs-subtract: `-0 v` at fresh-expression position
    /// must lex as three tokens (Minus, 0, v) so the parser sees prefix
    /// subtract. Documented papercut hit by six+ personas in the
    /// assessment log; previously `Number(-0)` + stray `Ident(v)` silently
    /// produced wrong results.
    #[test]
    fn lex_neg_zero_at_start_splits_into_minus_number() {
        let source = "-0 v";
        let tokens: Vec<_> = lex(source).unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(
            tokens,
            vec![
                Token::Minus,
                Token::Number(0.0),
                Token::Ident("v".to_string()),
            ]
        );
    }

    /// Same split fires after `;` (statement boundary).
    #[test]
    fn lex_neg_literal_after_semi_splits() {
        let source = "v=p;-0 v";
        let tokens: Vec<_> = lex(source).unwrap().into_iter().map(|(t, _)| t).collect();
        // ... ; - 0 v
        assert_eq!(tokens[3], Token::Semi);
        assert_eq!(tokens[4], Token::Minus);
        assert_eq!(tokens[5], Token::Number(0.0));
        assert_eq!(tokens[6], Token::Ident("v".to_string()));
    }

    /// Split fires after `=` (rhs of assignment): `r1=-1 t2`.
    #[test]
    fn lex_neg_literal_after_eq_splits() {
        let source = "r1=-1 t2";
        let tokens: Vec<_> = lex(source).unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(tokens[0], Token::Ident("r1".to_string()));
        assert_eq!(tokens[1], Token::Eq);
        assert_eq!(tokens[2], Token::Minus);
        assert_eq!(tokens[3], Token::Number(1.0));
        assert_eq!(tokens[4], Token::Ident("t2".to_string()));
    }

    /// Split fires after `{` (block start): `{-0 v}`.
    #[test]
    fn lex_neg_literal_after_lbrace_splits() {
        let source = "{-0 v}";
        let tokens: Vec<_> = lex(source).unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(tokens[0], Token::LBrace);
        assert_eq!(tokens[1], Token::Minus);
        assert_eq!(tokens[2], Token::Number(0.0));
        assert_eq!(tokens[3], Token::Ident("v".to_string()));
        assert_eq!(tokens[4], Token::RBrace);
    }

    /// Split fires after `(` so `(-0 v)` is `Subtract(0, v)`.
    #[test]
    fn lex_neg_literal_after_lparen_splits() {
        let source = "(-0 v)";
        let tokens: Vec<_> = lex(source).unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(tokens[0], Token::LParen);
        assert_eq!(tokens[1], Token::Minus);
        assert_eq!(tokens[2], Token::Number(0.0));
        assert_eq!(tokens[3], Token::Ident("v".to_string()));
        assert_eq!(tokens[4], Token::RParen);
    }

    /// Negative literal as call arg after an ident must NOT split:
    /// `at xs -1` calls `at` with three args, `-1` stays a literal.
    #[test]
    fn lex_neg_literal_after_ident_stays_literal() {
        let source = "at xs -1";
        let tokens: Vec<_> = lex(source).unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(
            tokens,
            vec![
                Token::Ident("at".to_string()),
                Token::Ident("xs".to_string()),
                Token::Number(-1.0),
            ]
        );
    }

    /// Negative literal mid-list (after a Number) stays literal:
    /// `[1 -2 3]` is a 3-element list `[1, -2, 3]`.
    #[test]
    fn lex_neg_literal_mid_list_stays_literal() {
        let source = "[1 -2 3]";
        let tokens: Vec<_> = lex(source).unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(
            tokens,
            vec![
                Token::LBracket,
                Token::Number(1.0),
                Token::Number(-2.0),
                Token::Number(3.0),
                Token::RBracket,
            ]
        );
    }

    /// Negative literal at the *start* of a comma-free list must also
    /// stay a literal — otherwise `[-2 1 3]` would split into
    /// `[ - 2 1 3 ]` and parse-greedy `Subtract(2, 1)` into a 2-element
    /// list. `LBracket` is deliberately excluded from the split contexts.
    #[test]
    fn lex_neg_literal_after_lbracket_stays_literal() {
        let source = "[-2 1 3]";
        let tokens: Vec<_> = lex(source).unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(
            tokens,
            vec![
                Token::LBracket,
                Token::Number(-2.0),
                Token::Number(1.0),
                Token::Number(3.0),
                Token::RBracket,
            ]
        );
    }

    /// Float negative literal at fresh-expression position also splits.
    #[test]
    fn lex_neg_float_at_start_splits() {
        let source = "-0.05 r";
        let tokens: Vec<_> = lex(source).unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(tokens[0], Token::Minus);
        assert_eq!(tokens[1], Token::Number(0.05));
        assert_eq!(tokens[2], Token::Ident("r".to_string()));
    }

    /// Prefix subtract via `+a -3` (negate-3 as second operand to `+`):
    /// the `-3` after an ident must STAY a literal so `+a -3` means
    /// `a + (-3)`. Pinned by PR #172.
    #[test]
    fn lex_neg_literal_after_prefix_binop_operand_stays() {
        let source = "+a -3";
        let tokens: Vec<_> = lex(source).unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(
            tokens,
            vec![
                Token::Plus,
                Token::Ident("a".to_string()),
                Token::Number(-3.0),
            ]
        );
    }

    #[test]
    fn lex_booleans() {
        let source = "true false";
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].0, Token::True);
        assert_eq!(tokens[1].0, Token::False);
    }

    #[test]
    fn lex_idea9_example01() {
        let source = "tot p:n q:n r:n>n;s=*p q;t=*s r;+s t";
        let tokens = lex(source).unwrap();
        // Should lex without errors
        assert!(tokens.len() > 10);
    }

    #[test]
    fn lex_idea9_example03() {
        let source = r#"cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze""#;
        let tokens = lex(source).unwrap();
        assert!(tokens.len() > 5);
    }

    #[test]
    fn lex_dollar_token() {
        let tokens = lex("$").unwrap();
        assert_eq!(tokens[0].0, Token::Dollar);
    }

    #[test]
    fn lex_double_equals_is_eq() {
        // == is sugar for = — both lex as Token::Eq
        let single = lex("=a b").unwrap();
        let double = lex("==a b").unwrap();
        assert_eq!(single[0].0, Token::Eq);
        assert_eq!(double[0].0, Token::Eq);
        // Both followed by the same Ident
        assert_eq!(single[1].0, double[1].0);
    }

    #[test]
    fn lex_assign_then_equality_with_double_eq() {
        // e==c n should lex as: Ident("e"), Eq, Ident("c"), Ident("n")
        // (assignment e = then equality == c n won't work because == is one token)
        // Actually: e==c → Ident("e"), Eq(==), Ident("c"), Ident("n")
        let tokens = lex("e==c n").unwrap();
        let types: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(
            types,
            vec![
                Token::Ident("e".to_string()),
                Token::Eq,
                Token::Ident("c".to_string()),
                Token::Ident("n".to_string()),
            ]
        );
    }

    #[test]
    fn lex_dotdot_token() {
        let tokens = lex("0..3").unwrap();
        let types: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(
            types,
            vec![Token::Number(0.0), Token::DotDot, Token::Number(3.0)]
        );
    }

    #[test]
    fn lex_dot_vs_dotdot() {
        // Make sure single dot still works
        let tokens = lex("x.y").unwrap();
        let types: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(
            types,
            vec![
                Token::Ident("x".to_string()),
                Token::Dot,
                Token::Ident("y".to_string())
            ]
        );
    }

    #[test]
    fn lex_suggest_fix_underscore() {
        let (code, suggestion) = super::lex_error_kind("my_func");
        assert_eq!(code, "ILO-L002");
        assert!(suggestion.contains("my-func"), "got: {}", suggestion);
    }

    #[test]
    fn lex_suggest_fix_uppercase() {
        let (code, suggestion) = super::lex_error_kind("MyFunc");
        assert_eq!(code, "ILO-L003");
        assert!(suggestion.contains("myfunc"), "got: {}", suggestion);
    }

    #[test]
    fn lex_suggest_fix_generic() {
        let (code, suggestion) = super::lex_error_kind("#");
        assert_eq!(code, "ILO-L001");
        assert!(
            suggestion.contains("Unexpected character"),
            "got: {}",
            suggestion
        );
    }

    // normalize_newlines tests

    #[test]
    fn normalize_inline_unchanged() {
        assert_eq!(normalize_newlines("dbl x:n>n;*x 2"), "dbl x:n>n;*x 2");
    }

    #[test]
    fn normalize_indented_body() {
        assert_eq!(
            normalize_newlines("greet name:t>t\n  +\"hello \" name"),
            "greet name:t>t;+\"hello \" name"
        );
    }

    #[test]
    fn normalize_multi_statement() {
        assert_eq!(
            normalize_newlines("calc a:n b:n>n\n  s=+a b\n  p=*a b\n  +s p"),
            "calc a:n b:n>n;s=+a b;p=*a b;+s p"
        );
    }

    #[test]
    fn normalize_separate_functions_preserved() {
        let src = "dbl x:n>n;*x 2\ninc x:n>n;+x 1";
        let result = normalize_newlines(src);
        assert!(
            result.contains('\n'),
            "newline between functions should be preserved: {result}"
        );
    }

    #[test]
    fn normalize_type_def_braces() {
        assert_eq!(
            normalize_newlines("type point{\n  x:n\n  y:n\n}"),
            "type point{x:n;y:n}"
        );
    }

    #[test]
    fn normalize_nested_braces() {
        assert_eq!(
            normalize_newlines("cls sp:n>t\n  >=sp 1000{\n    \"gold\"\n  }\n  \"bronze\""),
            "cls sp:n>t;>=sp 1000{\"gold\"};\"bronze\""
        );
    }
}
