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

    // Step keyword for range loops: `@i 0..n by 2{...}`
    #[token("by")]
    By,

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
    // `!!` panic-unwrap. Must precede single-char `!` so logos picks the
    // longer match. Symmetric with `!` over R / O, but on Err / nil aborts
    // with diagnostic + exit 1 instead of propagating to the enclosing fn.
    #[token("!!")]
    BangBang,

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
                    Some('f') => out.push('\u{000C}'),
                    Some('b') => out.push('\u{0008}'),
                    Some('v') => out.push('\u{000B}'),
                    Some('a') => out.push('\u{0007}'),
                    Some('0') => out.push('\u{0000}'),
                    Some('/') => out.push('/'),
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

impl Token {
    /// Return a user-facing rendering of this token suitable for inclusion
    /// in diagnostics. Operators and punctuation render as their source
    /// character(s) wrapped in backticks (e.g. `` `>` `` for `Token::Greater`),
    /// content-carrying tokens render with their literal payload
    /// (`` identifier `foo` ``, `` number `42` ``, `` text `"hi"` ``), and
    /// keyword/type tokens render as the keyword in backticks (`` `if` ``,
    /// `` `L` ``). The intent is that an agent reading a diagnostic sees the
    /// same characters they would have typed, not the parser's internal
    /// `TokenKind` variant name (`Greater`, `PipeOp`, `LBrace` ...).
    pub fn user_facing_name(&self) -> String {
        match self {
            // Step keyword
            Token::By => "`by`".into(),

            // Keywords
            Token::Type => "`type`".into(),
            Token::Tool => "`tool`".into(),
            Token::Use => "`use`".into(),
            Token::With => "`with`".into(),
            Token::Timeout => "`timeout`".into(),
            Token::Retry => "`retry`".into(),

            // Type constructors
            Token::ListType => "`L`".into(),
            Token::ResultType => "`R`".into(),
            Token::FnType => "`F`".into(),
            Token::OptType => "`O`".into(),
            Token::MapType => "`M`".into(),
            Token::SumType => "`S`".into(),

            // Reserved cross-language keywords
            Token::KwIf => "`if`".into(),
            Token::KwReturn => "`return`".into(),
            Token::KwLet => "`let`".into(),
            Token::KwFn => "`fn`".into(),
            Token::KwDef => "`def`".into(),
            Token::KwVar => "`var`".into(),
            Token::KwConst => "`const`".into(),

            // Boolean / nil
            Token::True => "`true`".into(),
            Token::False => "`false`".into(),
            Token::Nil => "`nil`".into(),

            // Multi-char operators
            Token::GreaterEq => "`>=`".into(),
            Token::LessEq => "`<=`".into(),
            Token::NotEq => "`!=`".into(),
            Token::PlusEq => "`+=`".into(),
            Token::PipeOp => "`>>`".into(),
            Token::NilCoalesce => "`??`".into(),
            Token::BangBang => "`!!`".into(),

            // Single-char operators
            Token::Plus => "`+`".into(),
            Token::Minus => "`-`".into(),
            Token::Star => "`*`".into(),
            Token::Slash => "`/`".into(),
            Token::Greater => "`>`".into(),
            Token::Less => "`<`".into(),
            Token::Eq => "`=`".into(),
            Token::Amp => "`&`".into(),
            Token::Pipe => "`|`".into(),

            // Special
            Token::Question => "`?`".into(),
            Token::At => "`@`".into(),
            Token::Bang => "`!`".into(),
            Token::Caret => "`^`".into(),
            Token::Tilde => "`~`".into(),
            Token::Dollar => "`$`".into(),

            // Punctuation
            Token::Colon => "`:`".into(),
            Token::Semi => "`;`".into(),
            Token::DotDot => "`..`".into(),
            Token::DotQuestion => "`.?`".into(),
            Token::Dot => "`.`".into(),
            Token::Comma => "`,`".into(),
            Token::LBrace => "`{`".into(),
            Token::RBrace => "`}`".into(),
            Token::LParen => "`(`".into(),
            Token::RParen => "`)`".into(),
            Token::LBracket => "`[`".into(),
            Token::RBracket => "`]`".into(),
            Token::Underscore => "`_`".into(),

            // Literals — include the payload so the diagnostic mentions
            // the actual offending value (`number 42`, `text "hi"`).
            Token::Number(n) => {
                if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e16 {
                    format!("number `{}`", *n as i64)
                } else {
                    format!("number `{n}`")
                }
            }
            Token::Text(s) => format!("text `\"{s}\"`"),
            Token::Ident(name) => format!("identifier `{name}`"),

            // Newline isn't usually surfaced (the normaliser eats them),
            // but render conservatively if one slips through.
            Token::Newline => "newline".into(),
        }
    }
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
    normalize_newlines_with_map(source).0
}

/// Normalize newlines and also produce a byte-level mapping from each
/// normalized-offset back to the corresponding original-source offset.
///
/// `map[i]` is the original-source byte index that produced the byte at
/// position `i` of the returned `String`. The mapping has length
/// `normalized.len() + 1`, with `map[normalized.len()]` equal to
/// `source.len()` so that a token span's `end` index can be remapped the
/// same way as its `start`.
///
/// Used by `lex` so that token spans emitted by logos against the
/// rewritten source are translated back to the user's actual source
/// before any diagnostic surfaces them. Without this, parse-error spans
/// inside multi-line function bodies drift onto downstream statements
/// because `;` characters that replace `\n` (and indentation that gets
/// stripped) shift every following byte.
pub fn normalize_newlines_with_map(source: &str) -> (String, Vec<u32>) {
    // Fast path: nothing the normaliser cares about. Triple-quoted
    // strings (`"""..."""`) need rewriting even on single-line input
    // because logos's string regex can't span them, so we can't take
    // the identity shortcut when the source contains `"""`.
    if !source.contains('\n') && !source.contains("\"\"\"") {
        // Identity case: the lexer sees exactly the source bytes, so the
        // map is the identity over `0..=source.len()`.
        let len = source.len();
        let mut map = Vec::with_capacity(len + 1);
        for i in 0..=len {
            map.push(i as u32);
        }
        return (source.to_string(), map);
    }

    let mut out = String::with_capacity(source.len());
    // `map[i]` = original byte offset producing normalized byte `i`.
    let mut map: Vec<u32> = Vec::with_capacity(source.len() + 1);
    // Track the last non-whitespace char pushed to `out` to avoid O(n) trim_end scans.
    let mut last_significant: Option<char> = None;
    // Depth of open `(` and `[` we're currently inside. `{` is tracked
    // separately by `last_significant` (existing precedent).
    let mut bracket_depth: u32 = 0;

    // Char-indexed iterator yielding `(byte_offset, char)`. We need
    // explicit byte offsets so the map can record where each emitted
    // byte came from in the original source.
    let mut iter = source.char_indices().peekable();

    /// Push every byte of `s` to `out` and record `orig` as the source
    /// offset for each.
    fn push_str_with(out: &mut String, map: &mut Vec<u32>, s: &str, orig: usize) {
        for _ in 0..s.len() {
            map.push(orig as u32);
        }
        out.push_str(s);
    }

    /// Push a single char to `out` recording `orig` as the source offset
    /// for each of the char's UTF-8 bytes.
    fn push_char_with(out: &mut String, map: &mut Vec<u32>, c: char, orig: usize) {
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        push_str_with(out, map, s, orig);
    }

    while let Some((i, c)) = iter.next() {
        // Windows CRLF: consume the `\r` silently when it is immediately
        // followed by `\n`. The `\n` is then handled on the next iteration
        // as a normal newline, preserving correct line/column accounting.
        // Standalone `\r` (old Mac line endings) is treated as whitespace
        // and passed through to the logos error path unchanged.
        if c == '\r' && iter.peek().map(|(_, ch)| *ch) == Some('\n') {
            // Do not push `\r` to output; do not update last_significant.
            // The `\n` on the next iteration does all the work.
            continue;
        }
        if c == '"' {
            // Triple-quoted multi-line string (`"""..."""`). Detect by
            // checking whether the next two source bytes are also `"`. If
            // so, scan to the closing `"""`, apply indent stripping when
            // the closing delimiter sits on its own line, and emit the
            // result as a synthesised single-line `"..."` literal so
            // logos's existing string regex consumes it. Each emitted
            // byte is mapped back to the source byte that produced it
            // (or the nearest source byte, for escape chars synthesised
            // to encode raw newlines / quotes).
            let src_bytes = source.as_bytes();
            if src_bytes.get(i + 1) == Some(&b'"') && src_bytes.get(i + 2) == Some(&b'"') {
                // Consume the two extra `"` from the iterator (`c` is the
                // first one, already taken). The opening sits at byte `i`.
                iter.next();
                iter.next();
                // Find the closing `"""`. Inside a triple-quoted string,
                // `\"` is *not* an escape; we don't peel escapes here.
                // logos decodes the synthesised single-quoted form. So we
                // scan raw bytes for the next `"""` sequence.
                let content_start = i + 3;
                let mut j = content_start;
                while j + 2 < src_bytes.len()
                    && !(src_bytes[j] == b'"'
                        && src_bytes[j + 1] == b'"'
                        && src_bytes[j + 2] == b'"')
                {
                    j += 1;
                }
                let (content_end, close_end) = if j + 2 < src_bytes.len() {
                    (j, j + 3)
                } else {
                    // Unterminated: scan to EOF, leave it to logos to
                    // surface as a lex error against the synthesised form.
                    (src_bytes.len(), src_bytes.len())
                };
                // Advance the iterator past the entire triple-quoted span
                // (content + closing `"""`, if present).
                while let Some(&(pi, _)) = iter.peek() {
                    if pi >= close_end {
                        break;
                    }
                    iter.next();
                }
                let raw = &source[content_start..content_end];
                let stripped = strip_triple_indent(raw);
                // Emit synthesised single-quoted string. Each emitted byte
                // points back at the source byte that produced it; injected
                // escape backslashes are mapped to the offending raw byte.
                push_char_with(&mut out, &mut map, '"', i);
                let stripped_chars: Vec<(usize, char)> = stripped.bytes.char_indices().collect();
                let mut k = 0usize;
                while k < stripped_chars.len() {
                    let (b_off, ch) = stripped_chars[k];
                    let orig = stripped.src_offsets[b_off] + content_start;
                    match ch {
                        '\n' => {
                            // Encode raw newline as `\n` escape so the
                            // synthesised single-quoted form lexes cleanly.
                            push_str_with(&mut out, &mut map, "\\n", orig);
                        }
                        '\r' => {
                            push_str_with(&mut out, &mut map, "\\r", orig);
                        }
                        '"' => {
                            // Bare `"` inside triple-quoted content must
                            // be escaped in the synthesised single-quoted
                            // form. (Real `"""` triplets are the closing
                            // delimiter; they never appear in content.)
                            push_str_with(&mut out, &mut map, "\\\"", orig);
                        }
                        '\\' => {
                            // Pass `\` + following char through verbatim
                            // so existing escape sequences (`\n`, `\t`,
                            // `\"`, ...) still decode the same way they
                            // do in `"..."`. A `\` immediately before a
                            // raw newline becomes `\\n` so the escape
                            // remains well-formed.
                            push_char_with(&mut out, &mut map, '\\', orig);
                            if k + 1 < stripped_chars.len() {
                                let (nb_off, nch) = stripped_chars[k + 1];
                                let norig = stripped.src_offsets[nb_off] + content_start;
                                match nch {
                                    '\n' => push_char_with(&mut out, &mut map, 'n', norig),
                                    '\r' => push_char_with(&mut out, &mut map, 'r', norig),
                                    _ => push_char_with(&mut out, &mut map, nch, norig),
                                }
                                k += 2;
                                continue;
                            }
                        }
                        _ => {
                            push_char_with(&mut out, &mut map, ch, orig);
                        }
                    }
                    k += 1;
                }
                // Closing `"`. Map to the first byte of the closing
                // `"""` if present, else EOF.
                let close_pos = if close_end > content_end {
                    content_end
                } else {
                    source.len().saturating_sub(1)
                };
                push_char_with(&mut out, &mut map, '"', close_pos);
                last_significant = Some('"');
                continue;
            }
            // Pass through string literal content verbatim so `--` inside a
            // string isn't mistaken for a comment, `\n` (if ever present
            // inside a string) isn't rewritten to `;`, and `(`/`[` inside
            // text don't bump bracket depth. Mirrors logos's string regex:
            // closing quote terminates unless escaped.
            push_char_with(&mut out, &mut map, c, i);
            last_significant = Some(c);
            while let Some((si, sc)) = iter.next() {
                push_char_with(&mut out, &mut map, sc, si);
                if sc == '\\' {
                    if let Some((ei, esc)) = iter.next() {
                        push_char_with(&mut out, &mut map, esc, ei);
                    }
                } else if sc == '"' {
                    last_significant = Some(sc);
                    break;
                }
            }
        } else if c == '-' && iter.peek().map(|(_, ch)| *ch) == Some('-') {
            // `--` starts a line comment. Drop the comment content (including
            // both dashes) up to but not including the next `\n`, so the
            // following `\n` is handled normally by the loop. This matches the
            // logos `--[^\n]*` skip rule but runs BEFORE newline normalization,
            // so an indented comment line doesn't bleed `;` separators into
            // the comment body where the logos regex would then swallow them.
            iter.next(); // consume second '-'
            while let Some(&(_, nc)) = iter.peek() {
                if nc == '\n' {
                    break;
                }
                iter.next();
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
                push_char_with(&mut out, &mut map, ' ', i);
                while matches!(iter.peek().map(|(_, ch)| *ch), Some(' ') | Some('\t')) {
                    iter.next();
                }
                continue;
            }
            // Skip blank lines (lines that are empty or whitespace-only)
            // before deciding declaration-boundary vs continuation. Without
            // this, a blank line between two indented statements inside a
            // function body emits a literal `\n` that the parser interprets
            // as a top-level declaration boundary, breaking parsing with a
            // misleading ILO-P009 cascade. Scan ahead through any run of
            // whitespace-only lines; the *next* non-blank line's indentation
            // is what determines whether this `\n` boundary continues the
            // current block or starts a new declaration.
            loop {
                let mut probe = iter.clone();
                // Skip whitespace on the current candidate "next" line.
                while matches!(probe.peek().map(|(_, ch)| *ch), Some(' ') | Some('\t')) {
                    probe.next();
                }
                // If the line is empty (immediate `\n`), consume the
                // whitespace + the trailing `\n` from the real iterator
                // and continue scanning. Otherwise stop.
                if probe.peek().map(|(_, ch)| *ch) == Some('\n') {
                    while matches!(iter.peek().map(|(_, ch)| *ch), Some(' ') | Some('\t')) {
                        iter.next();
                    }
                    iter.next(); // consume the trailing `\n`
                } else {
                    break;
                }
            }
            // Check if next line is indented (starts with space or tab)
            if matches!(iter.peek().map(|(_, ch)| *ch), Some(' ') | Some('\t')) {
                // Peek past indent at the first real char on the next line
                // so we can decide whether to emit a `;` before it.
                let mut lookahead = iter.clone();
                while matches!(lookahead.peek().map(|(_, ch)| *ch), Some(' ') | Some('\t')) {
                    lookahead.next();
                }
                // `>>` (pipe operator) at the start of a continuation line is
                // never a statement start — it must be chaining the previous
                // line's expression. Suppress the `;` so the chain parses.
                // Other operators (`+`/`-`/`*`) are valid prefix-call
                // statement starts and must NOT trigger this.
                let next_is_pipe = {
                    let mut probe = lookahead.clone();
                    probe.next().map(|(_, c)| c) == Some('>')
                        && probe.next().map(|(_, c)| c) == Some('>')
                };
                // Indented continuation → emit `;` and skip the whitespace
                // But first check if the last non-whitespace char was `{` — if so, skip the `;`
                // Also skip if `out` already ends in `;` (e.g. previous line
                // was a comment that produced no significant output), or if
                // the continuation begins with `>>` (pipe chain).
                if last_significant == Some('{') || out.ends_with(';') || next_is_pipe {
                    // Don't emit `;`
                } else {
                    push_char_with(&mut out, &mut map, ';', i);
                }
                // Skip leading whitespace on the continuation line
                while matches!(iter.peek().map(|(_, ch)| *ch), Some(' ') | Some('\t')) {
                    iter.next();
                }
                // If the continuation line starts with `}`, don't add `;` before it
                if iter.peek().map(|(_, ch)| *ch) == Some('}')
                    && last_significant != Some('{')
                    && out.ends_with(';')
                {
                    out.pop(); // remove the `;` we just pushed
                    map.pop();
                }
            } else if iter.peek().map(|(_, ch)| *ch) == Some('}') {
                // Non-indented `}` closes a block — don't emit newline
            } else {
                // Not indented → keep newline (declaration boundary)
                push_char_with(&mut out, &mut map, '\n', i);
            }
        } else {
            push_char_with(&mut out, &mut map, c, i);
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

    // Sentinel for one-past-end so that token spans can remap `end`.
    map.push(source.len() as u32);
    debug_assert_eq!(map.len(), out.len() + 1);

    (out, map)
}

/// Result of indent-stripping a triple-quoted string body.
///
/// `bytes` is the resulting content; `src_offsets[i]` is the byte offset
/// into the *original* triple-quoted content (i.e. `&source[content_start..
/// content_end]`) that produced byte `i` of `bytes`. The vector has one
/// entry per byte of `bytes`, and is used by `normalize_newlines_with_map`
/// to keep span attribution accurate for tokens emitted from the
/// synthesised single-quoted form.
struct StrippedTriple {
    bytes: String,
    src_offsets: Vec<usize>,
}

/// Apply indent stripping to the body of a triple-quoted string.
///
/// Rules:
/// - If the body contains no newline, return it unchanged. (Single-line
///   form `"""x"""` behaves exactly like `"x"`.)
/// - Otherwise, treat the body as a sequence of lines split on `\n`.
/// - If the body starts with a newline (i.e. the opening `"""` is the
///   last thing on its line), drop that leading newline.
/// - If the body ends with whitespace-then-EOF immediately before the
///   closing `"""` (i.e. the closing `"""` sits on its own line), strip
///   the trailing whitespace-only line and compute the common leading
///   whitespace prefix across the remaining non-blank lines (matching
///   Rust's `indoc!` macro / Python `textwrap.dedent`). The terminating
///   `\n` of the last content line is preserved.
/// - Otherwise keep the body verbatim (closing `"""` is inline).
fn strip_triple_indent(raw: &str) -> StrippedTriple {
    let bytes = raw.as_bytes();
    if !bytes.contains(&b'\n') {
        // Single-line case: pass through unchanged.
        let src_offsets: Vec<usize> = (0..bytes.len()).collect();
        return StrippedTriple {
            bytes: raw.to_string(),
            src_offsets,
        };
    }

    // Identify a leading newline (opening `"""` ends the line). Optional
    // `\r` before `\n` is handled by treating CRLF as a single newline
    // boundary at the start.
    let mut start = 0usize;
    if bytes.first() == Some(&b'\n') {
        start = 1;
    } else if bytes.len() >= 2 && bytes[0] == b'\r' && bytes[1] == b'\n' {
        start = 2;
    }

    // Detect "closing `"""` on its own line": the body, after the leading
    // newline drop, ends with `\n` followed only by spaces/tabs. If so,
    // we'll strip that trailing whitespace-only line AND compute the
    // common indent across the remaining content lines.
    let (end, dedent_active) = {
        let mut last_nl: Option<usize> = None;
        for (k, &b) in bytes[start..].iter().enumerate() {
            if b == b'\n' {
                last_nl = Some(start + k);
            }
        }
        match last_nl {
            Some(nl) => {
                let after_nl = &bytes[nl + 1..];
                if after_nl.iter().all(|&b| b == b' ' || b == b'\t') {
                    // Keep the terminating `\n` of the last content line
                    // in the output (matches `indoc!`); drop everything
                    // after it (the closing-`"""`-line indent).
                    (nl + 1, true)
                } else {
                    (bytes.len(), false)
                }
            }
            None => (bytes.len(), false),
        }
    };

    let inner = &raw[start..end];
    if !dedent_active {
        let src_offsets: Vec<usize> = (start..end).collect();
        return StrippedTriple {
            bytes: inner.to_string(),
            src_offsets,
        };
    }

    // `inner` ends in `\n` (we kept the terminating newline). Splitting
    // on `\n` produces an empty trailing element which we deliberately
    // don't emit; the loop appends `\n` only between adjacent elements.
    let lines: Vec<&str> = inner.split('\n').collect();
    // The trailing indent line (whitespace between final content `\n`
    // and the closing `"""`) is also a dedent constraint, so that the
    // closing delimiter's indentation can define the baseline (PEP 257
    // intuition).
    let closing_indent = &raw[end..];
    let mut common: Option<&str> = None;
    for line in lines.iter() {
        if line.chars().all(|c| c == ' ' || c == '\t') {
            // Blank/whitespace-only line: not a constraint.
            continue;
        }
        let lead: &str = {
            let mut byte_end = 0;
            for (off, ch) in line.char_indices() {
                if ch == ' ' || ch == '\t' {
                    byte_end = off + ch.len_utf8();
                } else {
                    break;
                }
            }
            &line[..byte_end]
        };
        common = Some(match common {
            None => lead,
            Some(prev) => common_prefix(prev, lead),
        });
    }
    let common: &str = match (common, closing_indent.is_empty()) {
        (Some(c), false) => common_prefix(c, closing_indent),
        (Some(c), true) => c,
        (None, _) => closing_indent,
    };
    let strip_len = common.len();

    let mut out_bytes = String::with_capacity(inner.len());
    let mut src_offsets: Vec<usize> = Vec::with_capacity(inner.len());
    let mut line_offset_in_inner = 0usize;
    for (li, line) in lines.iter().enumerate() {
        let line_bytes = line.as_bytes();
        let drop = if line_bytes.starts_with(common.as_bytes()) {
            strip_len
        } else if line.chars().all(|c| c == ' ' || c == '\t') {
            line_bytes.len()
        } else {
            0
        };
        let line_abs_start = start + line_offset_in_inner;
        for (off, ch) in line[drop..].char_indices() {
            let abs = line_abs_start + drop + off;
            out_bytes.push(ch);
            for _ in 0..ch.len_utf8() {
                src_offsets.push(abs);
            }
        }
        if li + 1 < lines.len() {
            out_bytes.push('\n');
            src_offsets.push(line_abs_start + line.len());
        }
        line_offset_in_inner += line.len() + 1; // +1 for the `\n` separator
    }
    debug_assert_eq!(out_bytes.len(), src_offsets.len());
    StrippedTriple {
        bytes: out_bytes,
        src_offsets,
    }
}

/// Common byte prefix of two whitespace-only strings.
fn common_prefix<'a>(a: &'a str, b: &str) -> &'a str {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let mut i = 0;
    while i < ab.len() && i < bb.len() && ab[i] == bb[i] {
        i += 1;
    }
    &a[..i]
}

/// Lex source code into a stream of tokens with positions.
///
/// All token spans (and lex-error positions) returned by this function
/// are byte offsets into the *original* `source`, not the normalized
/// rewrite that logos actually scans. The remap is built by
/// `normalize_newlines_with_map` and applied as the final step before
/// return, so callers can slice the original source by these spans and
/// `SourceMap::lookup` resolves the right line/col.
pub fn lex(source: &str) -> Result<Vec<(Token, std::ops::Range<usize>)>, LexError> {
    let (normalized, span_map) = normalize_newlines_with_map(source);
    // Map a normalized byte offset back to an original-source byte
    // offset. Out-of-range offsets clamp to `source.len()` defensively
    // (should never happen given the `+1` sentinel in the map).
    let remap = |off: usize| -> usize {
        span_map
            .get(off)
            .copied()
            .map(|x| x as usize)
            .unwrap_or(source.len())
    };
    match lex_normalized(&normalized) {
        Ok(mut tokens) => {
            for (_, sp) in tokens.iter_mut() {
                *sp = remap(sp.start)..remap(sp.end);
            }
            Ok(tokens)
        }
        Err(mut e) => {
            e.position = remap(e.position);
            Err(e)
        }
    }
}

/// Inner lex that operates on already-normalized source. All token
/// spans and `LexError.position` are byte offsets into `normalized`.
/// `lex` calls this and remaps spans back to the original source.
fn lex_normalized(normalized: &str) -> Result<Vec<(Token, std::ops::Range<usize>)>, LexError> {
    let mut lexer = Token::lexer(normalized);
    let mut tokens: Vec<(Token, std::ops::Range<usize>)> = Vec::new();

    while let Some(result) = lexer.next() {
        match result {
            Ok(token) => {
                let span = lexer.span();
                // Detect uppercase mid-identifier: a single uppercase type sigil
                // (L/R/F/O/M/S) sitting flush against a preceding ident.
                if is_type_sigil(&token) {
                    // Friendly hint for `OR`/`AND`/`NOT` from other languages
                    // when the first letter happens to lex as a type sigil
                    // (`O` → OptType, `S` → SumType, etc.). Without this the
                    // parser surfaces a downstream `expected expression, got
                    // OptType` that doesn't mention the actual mistake.
                    // Detect by scanning forward in the source for an
                    // uppercase-letter run starting at this sigil; if the
                    // resulting word is a known logical keyword, error.
                    // Only fires when the preceding token doesn't form an
                    // ident-merge candidate (handled below) — i.e. fresh
                    // expression position, not `xxxOR`.
                    let prev_is_ident_flush = matches!(
                        tokens.last(),
                        Some((Token::Ident(_), s)) if s.end == span.start
                    );
                    // Type-context guard: in `:OR n n` the sigil run is a
                    // compact form of `O R n n` (Optional of Result), which
                    // is valid type syntax. The hint should only fire when
                    // we're at expression/binding position, not in a type.
                    // Type position is unambiguous via the previous token:
                    // `:`, `>`, a type sigil itself, or a sum-type variant
                    // (which follows another `S`-typed ident name).
                    let prev_is_type_position = matches!(
                        tokens.last().map(|(t, _)| t),
                        Some(Token::Colon)
                            | Some(Token::Greater)
                            | Some(Token::ListType)
                            | Some(Token::ResultType)
                            | Some(Token::FnType)
                            | Some(Token::OptType)
                            | Some(Token::MapType)
                            | Some(Token::SumType)
                    );
                    if !prev_is_ident_flush
                        && !prev_token_is_dot_flush(&tokens, span.start)
                        && !prev_is_type_position
                        && let Some((word, end)) = scan_uppercase_run(normalized, span.start)
                        && word.len() >= 2
                        && let Some((canonical, hint)) = logical_keyword_message(&word)
                    {
                        return Err(LexError {
                            code: "ILO-L001",
                            position: span.start,
                            snippet: normalized[span.start..end].to_string(),
                            suggestion: format!(
                                "`{word}` is not an ilo keyword. ilo uses `{canonical}` ({hint})"
                            ),
                        });
                    }
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
                                normalized,
                                span.start,
                                span.end,
                                &mut lexer,
                                &mut tokens,
                            ) {
                                continue;
                            }
                        }
                        let sigil_char = normalized[span.clone()].chars().next().unwrap();
                        return Err(uppercase_mid_ident_error_with_source(
                            &prev_name,
                            sigil_char,
                            &normalized[span.end..],
                            prev_span.start,
                            Some(normalized),
                        ));
                    }
                    // Leading-uppercase JSON key flush after a Dot/DotQuestion
                    // (e.g. `r.URL` where `U` is consumed as a sigil-less
                    // capital, or `r.MetaData` where `M` is the MapType
                    // sigil). No preceding Ident to merge into; emit a fresh
                    // Ident covering the whole identifier-shaped run.
                    if prev_token_is_dot_flush(&tokens, span.start) {
                        if let Some(_consumed) = emit_ident_at_dot(
                            normalized,
                            span.start,
                            span.end,
                            &mut lexer,
                            &mut tokens,
                        ) {
                            continue;
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
                                normalized,
                                span.start,
                                span.end,
                                &mut lexer,
                                &mut tokens,
                            ) {
                                continue;
                            }
                        }
                        let c = bad.chars().next().unwrap();
                        return Err(uppercase_mid_ident_error_with_source(
                            &prev_name,
                            c,
                            &normalized[span.end..],
                            prev_span.start,
                            Some(normalized),
                        ));
                    }
                    // Leading-uppercase JSON key flush after `.` or `.?`
                    // (`r.URL`, `r.ID`, `r.AccessKey`). Logos rejects the
                    // capital because it isn't a sigil; there is no preceding
                    // Ident to merge into, so emit a fresh Ident spanning the
                    // whole identifier-shaped run.
                    if prev_token_is_dot_flush(&tokens, span.start) {
                        if let Some(_consumed) = emit_ident_at_dot(
                            normalized,
                            span.start,
                            span.end,
                            &mut lexer,
                            &mut tokens,
                        ) {
                            continue;
                        }
                    }
                }
                // Detect uppercase logical-keyword attempts: `AND`, `OR`, `NOT`
                // from other languages. Logos rejects the first capital letter
                // as ILO-L001; without a friendlier hint the persona sees
                // `unexpected token 'A'` and has to guess the actual mistake.
                // We scan forward from the rejected single uppercase letter
                // for an all-caps identifier-shaped run, then check whether it
                // spells one of the known logical keywords. (qa-tester and
                // scientific-researcher rerun3 both hit this.)
                if bad.len() == 1
                    && bad.chars().next().unwrap().is_ascii_uppercase()
                    && let Some((word, end)) = scan_uppercase_run(normalized, span.start)
                    && let Some((canonical, hint)) = logical_keyword_message(&word)
                {
                    return Err(LexError {
                        code: "ILO-L001",
                        position: span.start,
                        snippet: normalized[span.start..end].to_string(),
                        suggestion: format!(
                            "`{word}` is not an ilo keyword. ilo uses `{canonical}` ({hint})"
                        ),
                    });
                }
                // Haskell/Rust-style lambda shorthand: `\x{body}` or
                // `\x -> body`. Reached for by personas coming from Haskell
                // (`\x -> ...`) and Rust closure mental models. Logos rejects
                // the leading `\` and the persona sees a bare ILO-L001
                // "unexpected token '\\'" with no path forward. Point at the
                // canonical parenthesised-lambda form so the first retry is
                // the right one. (quant-trader rerun7.)
                if bad == "\\"
                    && let Some(hint) = backslash_lambda_hint(normalized, span.end)
                {
                    return Err(LexError {
                        code: "ILO-L001",
                        position: span.start,
                        snippet: bad.to_string(),
                        suggestion: hint,
                    });
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
    //   - `-` (operand slot of an outer prefix-minus: `- -0 a bo`)
    //
    // The `Minus` context fixes a repeat trap that hit multiple personas
    // (scientific-researcher rerun9, plus gis-analyst / ml-engineer /
    // content-mod from earlier reruns): `- -0 a bo` lexed as
    // `Minus, Number(-0.0), Ident(a), Ident(bo)`, where the outer `-`
    // consumed `-0` and `a` as a binary subtract and left `bo` orphaned,
    // surfacing as a misleading ILO-P020 "incomplete function header for
    // `bo`". After this split, the tokens become
    // `Minus, Minus, Number(0), Ident(a), Ident(bo)`, and `parse_minus`
    // reads it as `Subtract(Subtract(0, a), bo)` = `-a - bo`. Same shape
    // covers the wider `*-0 k s` / `t=-0 /t 6` family from earlier sessions.
    // Collateral: `- -N M` (N != 0) changes from `Subtract(-N, M)` to
    // `Negate(Subtract(N, M))` - the new reading matches a natural
    // left-associative parse. Zero hits in the repo before this fix; pinned
    // with explicit regression tests below and in
    // `regression_minus_zero_decl_parse.rs`.
    //
    // We deliberately do NOT add the other prefix binops (`+`, `*`, `/`)
    // to this set: `+ -3 5` currently reads as `Add(-3, 5) = 2`, and
    // splitting it would leave `+` with only one operand and produce a
    // parse error. The `Minus` carve-out is safe because `parse_minus`
    // already disambiguates unary-negate vs binary-subtract via
    // `can_start_operand`.
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
                    Token::Semi
                        | Token::Newline
                        | Token::Eq
                        | Token::LBrace
                        | Token::LParen
                        | Token::Minus
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

/// True when the last token in the stream is `Dot`/`DotQuestion` and sits
/// flush against `span_start` (no whitespace). Used to detect leading-
/// uppercase JSON keys at post-dot field-access position (e.g. the `U` in
/// `r.URL`, where there is no preceding Ident to merge into).
fn prev_token_is_dot_flush(tokens: &[(Token, std::ops::Range<usize>)], span_start: usize) -> bool {
    let Some((tok, sp)) = tokens.last() else {
        return false;
    };
    matches!(tok, Token::Dot | Token::DotQuestion) && sp.end == span_start
}

/// Emit a fresh `Ident` token covering a leading-uppercase JSON key that
/// appears flush after `.` or `.?` (e.g. `r.URL`, `r.AccessKey`). Mirrors
/// `absorb_camel_tail` but pushes a new token rather than merging into a
/// preceding `Ident`. Scans `normalized` from `span_start` consuming
/// `[A-Za-z0-9]` characters, pushes `Token::Ident(slice)` with the
/// resulting span, and bumps the logos lexer past the absorbed bytes.
/// Returns `Some(end)` on success, `None` if nothing was absorbed.
///
/// Underscores are deliberately excluded — the snake_case post-pass picks
/// up `_count` etc., so mixed `URL_count` still stitches correctly.
fn emit_ident_at_dot(
    normalized: &str,
    span_start: usize,
    span_end: usize,
    lexer: &mut logos::Lexer<'_, Token>,
    tokens: &mut Vec<(Token, std::ops::Range<usize>)>,
) -> Option<usize> {
    let bytes = normalized.as_bytes();
    let mut end = span_start;
    while end < bytes.len() {
        if bytes[end].is_ascii_alphanumeric() {
            end += 1;
        } else {
            break;
        }
    }
    if end == span_start {
        return None;
    }
    let new_span = span_start..end;
    let new_ident = normalized[new_span.clone()].to_string();
    tokens.push((Token::Ident(new_ident), new_span));
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

fn uppercase_mid_ident_error_with_source(
    prev: &str,
    cap: char,
    rest_after_cap: &str,
    start: usize,
    source: Option<&str>,
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
    let hyphenated = hyphenate_camel(&full);

    let mut suggestion = format!(
        "identifiers must be lowercase ASCII; got '{full}' (capital '{cap}' at offset {offset}). Use lowercase, e.g. `{hyphenated}` or `{lower}`"
    );

    // Scan the rest of the source for additional camelCase offenders so the
    // user can fix them all in one pass instead of one ILO-L003 per run. Skip
    // the current offender (same start position) and any identifier sitting
    // in a post-dot field-access position (those are absorbed, not rejected).
    if let Some(src) = source {
        let mut extras: Vec<String> = Vec::new();
        for offender in scan_camel_offenders(src) {
            if offender.start == start {
                continue;
            }
            // Exclude the current offender name and any duplicates so the
            // list shows only distinct additional identifiers to rename.
            if offender.full == full {
                continue;
            }
            if !extras.iter().any(|e| e == &offender.full) {
                extras.push(offender.full);
            }
        }
        if !extras.is_empty() {
            let preview: Vec<String> = extras.iter().take(5).cloned().collect();
            let more = if extras.len() > preview.len() {
                format!(" (+{} more)", extras.len() - preview.len())
            } else {
                String::new()
            };
            suggestion.push_str(&format!(
                ". Also found in this file: {}{}",
                preview.join(", "),
                more
            ));
        }
    }

    LexError {
        code: "ILO-L003",
        position: start,
        snippet: full.clone(),
        suggestion,
    }
}

fn hyphenate_camel(full: &str) -> String {
    let mut s = String::with_capacity(full.len() + 2);
    for (i, c) in full.chars().enumerate() {
        if i > 0 && c.is_ascii_uppercase() && !s.ends_with('-') {
            s.push('-');
        }
        s.push(c.to_ascii_lowercase());
    }
    s
}

#[derive(Debug)]
struct CamelOffender {
    start: usize,
    full: String,
}

/// Scan source for camelCase identifiers (lowercase-start with an
/// uppercase ASCII letter mid-identifier) that would trigger ILO-L003.
/// Skips identifiers immediately preceded by `.` or `.?` because those
/// are post-dot field accesses, which the lexer absorbs rather than rejects.
fn scan_camel_offenders(src: &str) -> Vec<CamelOffender> {
    let bytes = src.as_bytes();
    let mut out: Vec<CamelOffender> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // Skip string literal content so format strings like
        // `"%Y-%m-%dT%H:%M"` don't surface `dT` as a fake camelCase
        // offender. Mirrors the pre-pass at the top of this file: a `"`
        // opens a string that runs to the next unescaped `"`.
        if b == b'"' {
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                if c == b'\\' {
                    // Skip the escape byte too (handles `\"`, `\\`, etc.).
                    i += 2;
                    continue;
                }
                if c == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        // Skip `--` line comments so identifiers explaining the bug in
        // a comment don't double-report.
        if b == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Find start of a lowercase-led identifier.
        let prev = if i == 0 { 0 } else { bytes[i - 1] };
        let prev_prev = if i >= 2 { bytes[i - 2] } else { 0 };
        let is_post_dot = prev == b'.' || (prev == b'?' && prev_prev == b'.');
        if b.is_ascii_lowercase() && !(prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-')
        {
            // Walk the identifier-shaped run and look for a mid-capital.
            let start = i;
            let mut j = i;
            let mut found_cap = false;
            while j < bytes.len() {
                let c = bytes[j];
                if c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' {
                    j += 1;
                } else if c.is_ascii_uppercase() && j > start {
                    found_cap = true;
                    j += 1;
                } else {
                    break;
                }
            }
            if found_cap && !is_post_dot {
                // Extend through any trailing alphanumeric/hyphen tail so
                // `helloWorld-x` reports the full ident.
                while j < bytes.len() {
                    let c = bytes[j];
                    if c.is_ascii_alphanumeric() || c == b'-' {
                        j += 1;
                    } else {
                        break;
                    }
                }
                if let Ok(full) = std::str::from_utf8(&bytes[start..j]) {
                    out.push(CamelOffender {
                        start,
                        full: full.to_string(),
                    });
                }
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

/// Detect the Haskell/Rust backslash-lambda shorthand `\x{body}` or
/// `\x -> body`. Returns a hint pointing at the canonical parenthesised
/// lambda form when the source after a rejected `\` looks like one of those
/// shapes; `None` otherwise (so we fall back to the generic ILO-L001 hint).
///
/// The shape we match is `\` + one identifier-shaped run + (`{` or `->` or
/// space-then-ident-then-`->`). Single-param case covers the common reach
/// (`map \x{+x 1} xs`, `flt \x -> >x 0 xs`); multi-param Haskell is rare
/// from agents, so we keep the hint focused.
fn backslash_lambda_hint(source: &str, after_backslash: usize) -> Option<String> {
    let bytes = source.as_bytes();
    let mut i = after_backslash;
    // Skip a single optional space between `\` and the param name.
    if i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    // Param must be an identifier-shaped run (`[a-z][a-z0-9-]*`). If the
    // first char isn't a lowercase ASCII letter, this isn't a lambda
    // attempt and we leave it to the generic handler.
    if i >= bytes.len() || !bytes[i].is_ascii_lowercase() {
        return None;
    }
    let param_start = i;
    while i < bytes.len()
        && (bytes[i].is_ascii_lowercase() || bytes[i].is_ascii_digit() || bytes[i] == b'-')
    {
        i += 1;
    }
    let param = &source[param_start..i];
    if param.is_empty() {
        return None;
    }
    // Skip optional spaces between param and the following delimiter.
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    // Accept `{body}` (the user-targeted shape) or `-> body` (Haskell).
    let looks_like_lambda =
        bytes[i] == b'{' || (i + 1 < bytes.len() && bytes[i] == b'-' && bytes[i + 1] == b'>');
    if !looks_like_lambda {
        return None;
    }
    Some(format!(
        "`\\{param}{{body}}` is a Haskell/Rust lambda shorthand. ilo lambdas are parenthesised: `({param}:t>r;body)` (substitute the param type and return type). Example: `map (x:n>n;+x 1) xs`."
    ))
}

/// Scan an all-uppercase identifier-shaped run starting at `start`.
/// Returns `(word, end_offset)` if at least one uppercase letter is consumed.
/// Used by the L001 path to detect logical-keyword attempts like `AND`/`OR`/`NOT`.
fn scan_uppercase_run(source: &str, start: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_uppercase() {
        end += 1;
    }
    if end == start {
        return None;
    }
    Some((source[start..end].to_string(), end))
}

/// Map an all-uppercase word to its ilo-canonical replacement, when it is a
/// known logical-keyword attempt from another language. Returns
/// `(canonical_form, descriptive_hint)`.
fn logical_keyword_message(word: &str) -> Option<(&'static str, &'static str)> {
    match word {
        "AND" => Some(("&", "single `&` for logical and")),
        "OR" => Some(("|", "single `|` for logical or")),
        "NOT" => Some(("!", "prefix `!` for logical not")),
        _ => None,
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

    /// Standard C-style escape sequences must decode to their control
    /// characters inside `"..."` literals, not pass through as literal
    /// backslash-letter pairs. pdf-analyst rerun3 hit this on `\f` (the
    /// form-feed character `pdftotext` writes between PDF pages): `spl raw
    /// "\f"` returned 1 because the lexer was emitting two chars `\` `f`
    /// instead of `0x0C`. Cover the full escape table here so future
    /// regressions trip the unit test, not a persona running real data.
    #[test]
    fn lex_string_escapes_full_set() {
        let cases = [
            (r#""\n""#, "\n"),
            (r#""\t""#, "\t"),
            (r#""\r""#, "\r"),
            (r#""\"""#, "\""),
            (r#""\\""#, "\\"),
            (r#""\f""#, "\u{000C}"),
            (r#""\b""#, "\u{0008}"),
            (r#""\v""#, "\u{000B}"),
            (r#""\a""#, "\u{0007}"),
            (r#""\0""#, "\u{0000}"),
            (r#""\/""#, "/"),
            // Mixed: pdftotext-style page separator (`page1\fpage2`).
            (r#""page1\fpage2""#, "page1\u{000C}page2"),
        ];
        for (src, expected) in cases {
            let tokens = lex(src).unwrap();
            assert_eq!(
                tokens[0].0,
                Token::Text(expected.to_string()),
                "escape decode mismatch for {src}"
            );
        }
    }

    /// Unknown escapes preserve the backslash + char verbatim so existing
    /// programs that abuse `\` in strings (e.g. Windows paths in test data)
    /// don't suddenly change meaning. This is the long-standing fallback
    /// behaviour, locked in by test so future escape additions don't drop
    /// it.
    #[test]
    fn lex_string_unknown_escape_passes_through() {
        let tokens = lex(r#""\z""#).unwrap();
        assert_eq!(tokens[0].0, Token::Text("\\z".to_string()));
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

    /// Negative literal after a `-` token splits: `- -0 a bo` must become
    /// `Minus, Minus, Number(0), Ident(a), Ident(bo)` so the outer minus can
    /// recurse into the inner subtract and consume `bo` as its second
    /// operand, instead of orphaning `bo` and tripping ILO-P020.
    /// Originating bug: scientific-researcher rerun9 `-0` literal hijack.
    #[test]
    fn lex_neg_zero_after_minus_splits() {
        let source = "- -0 a bo";
        let tokens: Vec<_> = lex(source).unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(
            tokens,
            vec![
                Token::Minus,
                Token::Minus,
                Token::Number(0.0),
                Token::Ident("a".to_string()),
                Token::Ident("bo".to_string()),
            ]
        );
    }

    /// The same split fires for non-zero negative literals after a `-` token.
    /// `- -3 5` lexes as `-, -, 3, 5`, which parses as
    /// `Negate(Subtract(3, 5))` = `Negate(-2)` = `2`. The pre-fix reading was
    /// `Subtract(-3, 5)` = `-8`; zero in-repo usages, intentionally changed
    /// to match the natural left-associative parse and pinned here.
    #[test]
    fn lex_neg_int_after_minus_splits() {
        let source = "- -3 5";
        let tokens: Vec<_> = lex(source).unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(
            tokens,
            vec![
                Token::Minus,
                Token::Minus,
                Token::Number(3.0),
                Token::Number(5.0),
            ]
        );
    }

    /// `+ -3 5` must NOT trigger the split. The `Minus` carve-out is
    /// deliberately narrow; adding `Plus` to the split set would leave `+`
    /// with only one operand and break `Add(-3, 5) = 2`. This pins the
    /// negative control so the carve-out stays narrow.
    #[test]
    fn lex_neg_literal_after_plus_stays() {
        let source = "+ -3 5";
        let tokens: Vec<_> = lex(source).unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(
            tokens,
            vec![Token::Plus, Token::Number(-3.0), Token::Number(5.0),]
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

    // ── persona-diagnostic batch 2: logical-keyword friendly hints ────────

    #[test]
    fn lex_uppercase_and_emits_friendly_hint() {
        // `AND` from other languages — the leading `A` lex-fails as L001.
        let err = lex("main>b;AND a b").unwrap_err();
        assert_eq!(err.code, "ILO-L001");
        assert!(
            err.suggestion.contains("AND"),
            "suggestion: {}",
            err.suggestion
        );
        assert!(
            err.suggestion.contains("ilo uses `&`"),
            "suggestion: {}",
            err.suggestion
        );
        // Snippet should cover the full `AND` run, not just `A`.
        assert_eq!(err.snippet, "AND");
    }

    #[test]
    fn lex_uppercase_or_emits_friendly_hint() {
        // `OR` — `O` is the OptType sigil so logos succeeds at lex; the
        // friendly hint is gated on the sigil-emit path.
        let err = lex("main>b;OR a b").unwrap_err();
        assert_eq!(err.code, "ILO-L001");
        assert!(
            err.suggestion.contains("OR"),
            "suggestion: {}",
            err.suggestion
        );
        assert!(
            err.suggestion.contains("ilo uses `|`"),
            "suggestion: {}",
            err.suggestion
        );
        assert_eq!(err.snippet, "OR");
    }

    #[test]
    fn lex_uppercase_not_emits_friendly_hint() {
        let err = lex("main>b;NOT a").unwrap_err();
        assert_eq!(err.code, "ILO-L001");
        assert!(
            err.suggestion.contains("NOT"),
            "suggestion: {}",
            err.suggestion
        );
        assert!(
            err.suggestion.contains("ilo uses `!`"),
            "suggestion: {}",
            err.suggestion
        );
    }

    #[test]
    fn lex_post_dot_uppercase_keyword_not_treated_as_logical() {
        // `r.OR` is a JSON post-dot field access (the field happens to be
        // named `OR`); the absorb-camel-at-dot path should still handle it.
        // The new logical-keyword check must not pre-empt that case.
        let tokens = lex("f r:R n t>n;r.OR").unwrap();
        // The trailing `OR` should be absorbed as a post-dot Ident,
        // not surface as a lex error.
        assert!(
            tokens
                .iter()
                .any(|(t, _)| matches!(t, Token::Ident(s) if s == "OR")),
            "expected post-dot `OR` Ident; tokens: {:?}",
            tokens
        );
    }

    #[test]
    fn lex_mid_ident_uppercase_keyword_still_l003() {
        // `fooAND` should remain a mid-ident camelCase L003 (suggesting
        // `foo-and`), not the new logical-keyword L001 — the new path is
        // gated on fresh-position only.
        let err = lex("main>b;fooAND a b").unwrap_err();
        assert_eq!(err.code, "ILO-L003");
    }

    #[test]
    fn lex_type_position_or_is_optional_result_not_logical() {
        // `f x:OR n n>n` is the compact form of `O R n n` (Optional of
        // Result). The logical-keyword hint must NOT fire in type position
        // because that would reject valid type compositions.
        let tokens = lex("f x:OR n n>n;??x 0").unwrap();
        // Should lex as: Ident(f), Ident(x), Colon, OptType, ResultType,
        // Ident(n), Ident(n), Greater, ...
        assert!(
            tokens.iter().any(|(t, _)| matches!(t, Token::OptType)),
            "expected OptType in tokens: {:?}",
            tokens
        );
        assert!(
            tokens.iter().any(|(t, _)| matches!(t, Token::ResultType)),
            "expected ResultType in tokens: {:?}",
            tokens
        );
    }

    #[test]
    fn lex_backslash_lambda_brace_emits_hint() {
        // Haskell/Rust-style `\x{body}` shorthand. quant-trader rerun7
        // reached for it inside `map`. Without the targeted hint the agent
        // sees only `unexpected token '\'`.
        let err = lex("f xs:L n>L n;map \\x{+x 1} xs").unwrap_err();
        assert_eq!(err.code, "ILO-L001");
        assert_eq!(err.snippet, "\\");
        assert!(
            err.suggestion.contains("Haskell/Rust"),
            "suggestion should call out the source language: {}",
            err.suggestion
        );
        assert!(
            err.suggestion.contains("(x:t>r;body)"),
            "suggestion should point at the canonical parenthesised lambda form: {}",
            err.suggestion
        );
    }

    #[test]
    fn lex_backslash_lambda_arrow_emits_hint() {
        // Haskell's `\x -> body` form should also trigger the hint — same
        // mental model, just different delimiter after the param.
        let err = lex("f xs:L n>L n;map \\x -> +x 1 xs").unwrap_err();
        assert_eq!(err.code, "ILO-L001");
        assert!(
            err.suggestion.contains("Haskell/Rust"),
            "suggestion: {}",
            err.suggestion
        );
    }

    #[test]
    fn lex_backslash_lambda_space_param_emits_hint() {
        // Tolerate a single space between `\` and the param name
        // (Haskell-style `\ x -> ...`).
        let err = lex("f xs:L n>L n;map \\ x{+x 1} xs").unwrap_err();
        assert_eq!(err.code, "ILO-L001");
        assert!(
            err.suggestion.contains("(x:t>r;body)"),
            "suggestion: {}",
            err.suggestion
        );
    }

    #[test]
    fn lex_lone_backslash_no_lambda_hint() {
        // A bare `\` with no following ident+brace/arrow should fall back to
        // the generic ILO-L001 hint — we only want the lambda hint when the
        // shape is unambiguously a lambda attempt.
        let err = lex("f x:n>n;\\").unwrap_err();
        assert_eq!(err.code, "ILO-L001");
        assert!(
            !err.suggestion.contains("Haskell/Rust"),
            "lone `\\` should not surface the lambda hint, got: {}",
            err.suggestion
        );
    }

    #[test]
    fn lex_backslash_followed_by_ident_no_brace_no_hint() {
        // `\foo` with no `{` or `->` after the param isn't a lambda shape —
        // could be anything (escape attempt, typo). Fall back to generic.
        let err = lex("f x:n>n;\\foo bar").unwrap_err();
        assert_eq!(err.code, "ILO-L001");
        assert!(
            !err.suggestion.contains("Haskell/Rust"),
            "`\\foo bar` (no brace/arrow) should not surface the lambda hint, got: {}",
            err.suggestion
        );
    }

    #[test]
    fn lex_type_position_after_greater_or_is_optional_result() {
        // `f>OR n n;...` — return-type position after `>` also splits OR.
        let tokens = lex("f>OR n n;~42").unwrap();
        assert!(
            tokens.iter().any(|(t, _)| matches!(t, Token::OptType)),
            "expected OptType in tokens: {:?}",
            tokens
        );
    }

    // --- blank-line continuation tests (nlp-engineer rerun7 P0) ---
    //
    // A blank line between indented statements inside a function body must
    // be treated as a continuation, not a declaration boundary. Without the
    // fix, a literal `\n` is emitted mid-body and the parser interprets the
    // next indented statement as a new top-level declaration, producing a
    // misleading ILO-P009 / T002 / T004 cascade.

    #[test]
    fn normalize_blank_line_between_indented_statements_is_continuation() {
        let src = "main>n;\n  x=42;\n\n  prnt x;\n  0\n";
        let got = normalize_newlines(src);
        // Must NOT contain a `\n` mid-body — that would be a declaration
        // boundary. All statements collapse to a single `main>n;...` line.
        assert!(
            !got.trim_end_matches('\n').contains('\n'),
            "blank line should not produce mid-body newline: {:?}",
            got
        );
        assert_eq!(got, "main>n;x=42;prnt x;0\n");
    }

    #[test]
    fn normalize_multiple_consecutive_blank_lines_collapse() {
        let src = "main>n;\n  x=42;\n\n\n\n  prnt x;\n  0\n";
        let got = normalize_newlines(src);
        assert_eq!(got, "main>n;x=42;prnt x;0\n");
    }

    #[test]
    fn normalize_blank_line_with_trailing_whitespace_is_continuation() {
        // Blank lines that contain only spaces/tabs must also be treated
        // as continuation, not declaration boundary.
        let src = "main>n;\n  x=42;\n   \n\t\n  prnt x;\n  0\n";
        let got = normalize_newlines(src);
        assert_eq!(got, "main>n;x=42;prnt x;0\n");
    }

    #[test]
    fn normalize_blank_line_before_non_indented_keeps_decl_boundary() {
        // Blank line followed by a NON-indented line is still a declaration
        // boundary (the blank lines just separate top-level declarations).
        let src = "f>n;1\n\ng>n;2\n";
        let got = normalize_newlines(src);
        assert!(
            got.contains('\n'),
            "blank line before top-level decl must keep newline: {:?}",
            got
        );
        // The two declarations must end up separated by a single `\n`.
        assert_eq!(got, "f>n;1\ng>n;2\n");
    }

    #[test]
    fn normalize_blank_lines_at_start_of_function_body() {
        // Blank line *immediately* after the function header, before the
        // first indented statement, must be tolerated.
        let src = "main>n;\n\n  x=42;\n  prnt x;\n  0\n";
        let got = normalize_newlines(src);
        assert_eq!(got, "main>n;x=42;prnt x;0\n");
    }

    #[test]
    fn normalize_trailing_blank_lines_at_eof() {
        // Trailing blank lines must not crash and must not corrupt the
        // final declaration boundary.
        let src = "main>n;\n  x=42;\n  0\n\n\n";
        let got = normalize_newlines(src);
        assert!(got.starts_with("main>n;x=42;0"));
    }

    #[test]
    fn normalize_prnt_fmt_then_blank_line_parses() {
        // The exact nlp-engineer rerun7 repro shape: `prnt fmt "tmpl" arg;`
        // followed by a blank line and another statement. The variadic
        // arg-list must NOT swallow tokens across the blank line.
        let src = "main>n;\n  x=42;\n  prnt fmt \"x={}\" x;\n\n  y=99;\n  prnt y;\n  0\n";
        let got = normalize_newlines(src);
        assert!(
            !got.trim_end_matches('\n').contains('\n'),
            "prnt fmt + blank line + next stmt must collapse: {:?}",
            got
        );
        // End-to-end parse + lex sanity check on the same shape.
        let tokens = lex(src).unwrap();
        assert!(!tokens.is_empty(), "lex must produce tokens");
    }

    #[test]
    fn normalize_blank_line_offset_map_preserves_span_fidelity() {
        // Span integrity: an error on a line that comes *after* a blank
        // line must remap to its original-source byte offset, not to
        // wherever the normalized rewrite landed. Without a faithful map,
        // ILO-P009 (and any other diagnostic emitted on post-blank lines)
        // would anchor at the wrong column.
        let src = "main>n;\n  x=42;\n\n  y=99;\n  prnt y;\n  0\n";
        let (normalized, map) = normalize_newlines_with_map(src);
        // The sentinel +1 byte at the end of map covers one-past-end.
        assert_eq!(map.len(), normalized.len() + 1);
        // Every map entry must point inside the original source.
        let src_len = src.len() as u32;
        for &off in &map {
            assert!(off <= src_len, "map offset {} > src.len() {}", off, src_len);
        }
        // The first `y` in the normalized output must remap to the `y` on
        // line 4 of the original source (after the blank line on line 3).
        let y_pos_normalized = normalized.find("y=99").expect("y=99 in normalized");
        let y_pos_original = map[y_pos_normalized] as usize;
        assert_eq!(
            &src[y_pos_original..y_pos_original + 4],
            "y=99",
            "span remap landed at wrong original byte"
        );
    }
}
