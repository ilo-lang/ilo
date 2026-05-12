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
pub fn normalize_newlines(source: &str) -> String {
    if !source.contains('\n') {
        return source.to_string();
    }

    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    // Track the last non-whitespace char pushed to `out` to avoid O(n) trim_end scans.
    let mut last_significant: Option<char> = None;

    while let Some(c) = chars.next() {
        if c == '\n' {
            // Check if next line is indented (starts with space or tab)
            if matches!(chars.peek(), Some(' ') | Some('\t')) {
                // Indented continuation → emit `;` and skip the whitespace
                // But first check if the last non-whitespace char was `{` — if so, skip the `;`
                if last_significant == Some('{') {
                    // Don't emit `;` after `{`, just skip whitespace
                } else {
                    out.push(';');
                }
                // Skip leading whitespace on the continuation line
                while matches!(chars.peek(), Some(' ') | Some('\t')) {
                    chars.next();
                }
                // If the continuation line starts with `}`, don't add `;` before it
                if chars.peek() == Some(&'}') && last_significant != Some('{') {
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
                    if let Some((Token::Ident(prev), prev_span)) = tokens.last() {
                        if prev_span.end == span.start {
                            let sigil_char = normalized[span.clone()].chars().next().unwrap();
                            return Err(uppercase_mid_ident_error(
                                prev,
                                sigil_char,
                                &normalized[span.end..],
                                prev_span.start,
                            ));
                        }
                    }
                }
                tokens.push((token, span));
            }
            Err(()) => {
                let span = lexer.span();
                let bad = &normalized[span.clone()];
                // Single uppercase ASCII letter directly after an ident is a
                // mid-identifier capital (e.g. `isAgg` → `is` + bad `A`).
                if bad.len() == 1
                    && bad.chars().next().unwrap().is_ascii_uppercase()
                    && let Some((Token::Ident(prev), prev_span)) = tokens.last()
                    && prev_span.end == span.start
                {
                    let c = bad.chars().next().unwrap();
                    return Err(uppercase_mid_ident_error(
                        prev,
                        c,
                        &normalized[span.end..],
                        prev_span.start,
                    ));
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
        assert_eq!(tokens[2].0, Token::Number(-7.0));
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
