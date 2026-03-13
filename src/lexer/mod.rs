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
    #[regex(r"-?[0-9]+(\.[0-9]+)?", |lex| lex.slice().parse::<f64>().ok())]
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
    let mut tokens = Vec::new();

    while let Some(result) = lexer.next() {
        match result {
            Ok(token) => tokens.push((token, lexer.span())),
            Err(()) => {
                let span = lexer.span();
                let bad = &normalized[span.clone()];
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

    Ok(tokens)
}

fn lex_error_kind(bad_token: &str) -> (&'static str, String) {
    if bad_token.contains('_') && bad_token.len() > 1 {
        (
            "ILO-L002",
            format!("Use hyphens instead of underscores: '{}'", bad_token.replace('_', "-")),
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
        assert_eq!(types, vec![
            Token::GreaterEq, Token::LessEq, Token::NotEq,
            Token::Greater, Token::Less,
            Token::Plus, Token::Minus, Token::Star, Token::Slash,
        ]);
    }

    #[test]
    fn lex_special_tokens() {
        let source = "?@!^~$";
        let tokens = lex(source).unwrap();
        let types: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(types, vec![Token::Question, Token::At, Token::Bang, Token::Caret, Token::Tilde, Token::Dollar]);
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
        assert_eq!(types, vec![
            Token::Type, Token::Tool, Token::With,
            Token::Timeout, Token::Retry,
        ]);
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
        assert!(tokens.iter().any(|(t, _)| *t == Token::Ident("tot".to_string())));
    }

    #[test]
    fn lex_punctuation() {
        let source = ":;.,{}()_";
        let tokens = lex(source).unwrap();
        let types: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(types, vec![
            Token::Colon, Token::Semi, Token::Dot, Token::Comma,
            Token::LBrace, Token::RBrace, Token::LParen, Token::RParen,
            Token::Underscore,
        ]);
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
        assert_eq!(types, vec![
            Token::Ident("e".to_string()),
            Token::Eq,
            Token::Ident("c".to_string()),
            Token::Ident("n".to_string()),
        ]);
    }

    #[test]
    fn lex_dotdot_token() {
        let tokens = lex("0..3").unwrap();
        let types: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(types, vec![Token::Number(0.0), Token::DotDot, Token::Number(3.0)]);
    }

    #[test]
    fn lex_dot_vs_dotdot() {
        // Make sure single dot still works
        let tokens = lex("x.y").unwrap();
        let types: Vec<_> = tokens.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(types, vec![Token::Ident("x".to_string()), Token::Dot, Token::Ident("y".to_string())]);
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
        assert!(suggestion.contains("Unexpected character"), "got: {}", suggestion);
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
        assert!(result.contains('\n'), "newline between functions should be preserved: {result}");
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
