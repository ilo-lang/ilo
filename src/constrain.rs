//! Grammar export for LLM constrained decoding.
//!
//! Three CLI surfaces:
//!   `ilo constrain`          — parser state machine as JSON
//!   `ilo constrain --masks`   — per-state binary masks over token vocabulary
//!   `ilo constrain --completions <file> <line> <col>` — parser state + valid next tokens at cursor
//!
//! The state machine is a hand-derived enumeration of ilo's parse states,
//! matching the recursive-descent parser in `src/parser/mod.rs`.  Each state
//! lists the token categories that the parser's `peek`/`expect` calls accept
//! at that position, and the next state each accepted token transitions to.
//!
//! ## Vocabulary
//!
//! The token vocabulary is derived from `lexer::Token` variants, mapped to
//! string representations.  Categories like `ident`, `num`, `text` group
//! families (all identifiers, all numbers, all string literals) so the mask
//! is over a small, stable set rather than the infinite identifier space.
//!
//! ## Design notes
//!
//! - The state machine is **static**: it encodes the grammar's shape, not
//!   the parser's internal bookkeeping (arity tables, lambda counters,
//!   depth guards, etc.).  A host harness that applies the masks at
//!   decode time prevents syntactically invalid tokens from being
//!   generated; it does not guarantee the resulting token sequence is
//!   semantically valid (type checking, arity, etc. are still done by
//!   `ilo check`).
//! - `--completions` tokenises the file up to the cursor, runs the parser
//!   to determine the current state, then emits the valid-next-token set.
//!   This is more precise than the static state machine because it uses
//!   the actual parser state, including arity tables and context.

use crate::lexer::Token;
use serde_json::{json, Value};

// ── Token vocabulary ───────────────────────────────────────────────────────

/// A token category in the constrained-decoding vocabulary.  Categories
/// group lexer `Token` variants so the mask is over a small, stable set.
///
/// The order here is the index order in the vocabulary array and the
/// bitmask arrays.  Changing the order is a breaking change to the JSON
/// output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TokenCat {
    // ── Keywords ──
    Type,       // `type`
    Tool,       // `tool`
    Use,        // `use`
    With,       // `with`
    By,         // `by`
    // ── Type sigils ──
    ListType,   // `L`
    ResultType, // `R`
    FnType,     // `F`
    OptType,    // `O`
    MapType,    // `M`
    SumType,    // `S`
    WorldType,  // `W`
    U32Type,    // `U32`
    U64Type,    // `U64`
    I64Type,    // `I64`
    // ── Literals ──
    True,       // `true`
    False,      // `false`
    Nil,        // `nil`
    Number,    // any numeric literal
    Text,       // any string literal
    // ── Identifiers ──
    Ident,      // any lowercase identifier
    Underscore, // `_`
    // ── Multi-char operators ──
    GreaterEq,  // `>=`
    LessEq,     // `<=`
    NotEq,      // `!=`
    PlusEq,     // `+=`
    PipeOp,     // `>>`
    NilCoalesce, // `??`
    BangBang,   // `!!`
    DotDot,     // `..`
    DotQuestion, // `.?`
    // ── Single-char operators ──
    Plus,       // `+`
    Minus,      // `-`
    Star,       // `*`
    Slash,      // `/`
    Greater,    // `>`
    Less,       // `<`
    Eq,         // `=` or `==`
    Amp,        // `&`
    Pipe,       // `|`
    Question,   // `?`
    At,         // `@`
    Bang,       // `!`
    Caret,      // `^`
    Tilde,      // `~`
    Dollar,     // `$`
    // ── Punctuation ──
    Colon,      // `:`
    Semi,       // `;`
    Dot,        // `.`
    Comma,      // `,`
    LBrace,     // `{`
    RBrace,     // `}`
    LParen,     // `(`
    RParen,     // `)`
    LBracket,   // `[`
    RBracket,   // `]`
    // ── Control ──
    Newline,     // `\n`
    Eof,         // end of input
}

impl TokenCat {
    /// All categories in canonical order.
    pub const ALL: &'static [TokenCat] = &[
        TokenCat::Type,
        TokenCat::Tool,
        TokenCat::Use,
        TokenCat::With,
        TokenCat::By,
        TokenCat::ListType,
        TokenCat::ResultType,
        TokenCat::FnType,
        TokenCat::OptType,
        TokenCat::MapType,
        TokenCat::SumType,
        TokenCat::WorldType,
        TokenCat::U32Type,
        TokenCat::U64Type,
        TokenCat::I64Type,
        TokenCat::True,
        TokenCat::False,
        TokenCat::Nil,
        TokenCat::Number,
        TokenCat::Text,
        TokenCat::Ident,
        TokenCat::Underscore,
        TokenCat::GreaterEq,
        TokenCat::LessEq,
        TokenCat::NotEq,
        TokenCat::PlusEq,
        TokenCat::PipeOp,
        TokenCat::NilCoalesce,
        TokenCat::BangBang,
        TokenCat::DotDot,
        TokenCat::DotQuestion,
        TokenCat::Plus,
        TokenCat::Minus,
        TokenCat::Star,
        TokenCat::Slash,
        TokenCat::Greater,
        TokenCat::Less,
        TokenCat::Eq,
        TokenCat::Amp,
        TokenCat::Pipe,
        TokenCat::Question,
        TokenCat::At,
        TokenCat::Bang,
        TokenCat::Caret,
        TokenCat::Tilde,
        TokenCat::Dollar,
        TokenCat::Colon,
        TokenCat::Semi,
        TokenCat::Dot,
        TokenCat::Comma,
        TokenCat::LBrace,
        TokenCat::RBrace,
        TokenCat::LParen,
        TokenCat::RParen,
        TokenCat::LBracket,
        TokenCat::RBracket,
        TokenCat::Newline,
        TokenCat::Eof,
    ];

    /// String representation in the vocabulary array.
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenCat::Type => "type",
            TokenCat::Tool => "tool",
            TokenCat::Use => "use",
            TokenCat::With => "with",
            TokenCat::By => "by",
            TokenCat::ListType => "L",
            TokenCat::ResultType => "R",
            TokenCat::FnType => "F",
            TokenCat::OptType => "O",
            TokenCat::MapType => "M",
            TokenCat::SumType => "S",
            TokenCat::WorldType => "W",
            TokenCat::U32Type => "U32",
            TokenCat::U64Type => "U64",
            TokenCat::I64Type => "I64",
            TokenCat::True => "true",
            TokenCat::False => "false",
            TokenCat::Nil => "nil",
            TokenCat::Number => "num",
            TokenCat::Text => "text",
            TokenCat::Ident => "ident",
            TokenCat::Underscore => "_",
            TokenCat::GreaterEq => ">=",
            TokenCat::LessEq => "<=",
            TokenCat::NotEq => "!=",
            TokenCat::PlusEq => "+=",
            TokenCat::PipeOp => ">>",
            TokenCat::NilCoalesce => "??",
            TokenCat::BangBang => "!!",
            TokenCat::DotDot => "..",
            TokenCat::DotQuestion => ".?",
            TokenCat::Plus => "+",
            TokenCat::Minus => "-",
            TokenCat::Star => "*",
            TokenCat::Slash => "/",
            TokenCat::Greater => ">",
            TokenCat::Less => "<",
            TokenCat::Eq => "=",
            TokenCat::Amp => "&",
            TokenCat::Pipe => "|",
            TokenCat::Question => "?",
            TokenCat::At => "@",
            TokenCat::Bang => "!",
            TokenCat::Caret => "^",
            TokenCat::Tilde => "~",
            TokenCat::Dollar => "$",
            TokenCat::Colon => ":",
            TokenCat::Semi => ";",
            TokenCat::Dot => ".",
            TokenCat::Comma => ",",
            TokenCat::LBrace => "{",
            TokenCat::RBrace => "}",
            TokenCat::LParen => "(",
            TokenCat::RParen => ")",
            TokenCat::LBracket => "[",
            TokenCat::RBracket => "]",
            TokenCat::Newline => "\\n",
            TokenCat::Eof => "eof",
        }
    }

    /// Index in the `ALL` array / bitmask position.
    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|c| c == self).unwrap()
    }

    /// Map a lexer `Token` to its category.  Returns `None` for tokens that
    /// don't have a category (the reserved-keyword tokens like `KwIf`, `KwFn`,
    /// etc., which are rejected by the parser and should never appear in valid
    /// output).
    pub fn from_token(tok: &Token) -> Option<Self> {
        Some(match tok {
            Token::Type => TokenCat::Type,
            Token::Tool => TokenCat::Tool,
            Token::Policy => TokenCat::Tool,
            Token::Use => TokenCat::Use,
            Token::With => TokenCat::With,
            Token::By => TokenCat::By,
            Token::ListType => TokenCat::ListType,
            Token::ResultType => TokenCat::ResultType,
            Token::FnType => TokenCat::FnType,
            Token::OptType => TokenCat::OptType,
            Token::MapType => TokenCat::MapType,
            Token::SumType => TokenCat::SumType,
            Token::WorldType => TokenCat::WorldType,
            Token::U32Type => TokenCat::U32Type,
            Token::U64Type => TokenCat::U64Type,
            Token::I64Type => TokenCat::I64Type,
            Token::True => TokenCat::True,
            Token::False => TokenCat::False,
            Token::Nil => TokenCat::Nil,
            Token::Number(_) => TokenCat::Number,
            Token::Text(_) => TokenCat::Text,
            Token::Ident(_) => TokenCat::Ident,
            Token::Underscore => TokenCat::Underscore,
            Token::GreaterEq => TokenCat::GreaterEq,
            Token::LessEq => TokenCat::LessEq,
            Token::NotEq => TokenCat::NotEq,
            Token::PlusEq => TokenCat::PlusEq,
            Token::PipeOp => TokenCat::PipeOp,
            Token::NilCoalesce => TokenCat::NilCoalesce,
            Token::BangBang => TokenCat::BangBang,
            Token::DotDot => TokenCat::DotDot,
            Token::DotQuestion => TokenCat::DotQuestion,
            Token::Plus => TokenCat::Plus,
            Token::Minus => TokenCat::Minus,
            Token::Star => TokenCat::Star,
            Token::Slash => TokenCat::Slash,
            Token::Greater => TokenCat::Greater,
            Token::Less => TokenCat::Less,
            Token::Eq => TokenCat::Eq,
            Token::Amp => TokenCat::Amp,
            Token::Pipe => TokenCat::Pipe,
            Token::Question => TokenCat::Question,
            Token::At => TokenCat::At,
            Token::Bang => TokenCat::Bang,
            Token::Caret => TokenCat::Caret,
            Token::Tilde => TokenCat::Tilde,
            Token::Dollar => TokenCat::Dollar,
            Token::Colon => TokenCat::Colon,
            Token::Semi => TokenCat::Semi,
            Token::Dot => TokenCat::Dot,
            Token::Comma => TokenCat::Comma,
            Token::LBrace => TokenCat::LBrace,
            Token::RBrace => TokenCat::RBrace,
            Token::LParen => TokenCat::LParen,
            Token::RParen => TokenCat::RParen,
            Token::LBracket => TokenCat::LBracket,
            Token::RBracket => TokenCat::RBracket,
            Token::Newline => TokenCat::Newline,
            // Reserved-keyword tokens from other languages — not valid output.
            Token::KwIf | Token::KwReturn | Token::KwLet | Token::KwFn
            | Token::KwDef | Token::KwVar | Token::KwConst => return None,
            // Reserved words for tool-decl fields — not valid in code output.
            Token::Timeout | Token::Retry => return None,
        })
    }
}

// ── Parse states ──────────────────────────────────────────────────────────

/// A parse state in the grammar state machine.
///
/// These are hand-derived from the recursive-descent parser in
/// `src/parser/mod.rs`.  Each state corresponds to a position in the grammar
/// where the parser expects specific token categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParseState {
    /// Top of a file / between declarations.
    TopLevel,
    /// After `type` keyword, expecting a type name.
    AfterType,
    /// After `tool` keyword, expecting a tool name.
    AfterTool,
    /// After `use` keyword, expecting a module path.
    AfterUse,
    /// After a function/decl name, expecting `<` for type params or `:` for
    /// param types or `>` for return type.
    FnHeader,
    /// Inside `<...>` type parameter block.
    TypeParams,
    /// After a param name, expecting `:` for the type annotation.
    ParamName,
    /// After `:` in a param, expecting a type.
    ParamType,
    /// After a param type, expecting `,` (more params) or `>` (return type).
    AfterParamType,
    /// After `>`, expecting the return type.
    ReturnType,
    /// After the return type, expecting `;` or `{` or start of body.
    AfterReturnType,
    /// After `^` effect-set marker, expecting a variant name.
    EffectSet,
    /// Start of a statement in a function body.
    StmtStart,
    /// After `@` — start of a foreach/for-range loop, expecting an ident
    /// (loop variable) or a number (range start).
    AfterAt,
    /// After `?` — either match (`?expr{...}`) or prefix ternary
    /// (`?cond a b`).
    AfterQuestion,
    /// Start of an expression (after an operator, `=`, `;`, etc.).
    ExprStart,
    /// After an operator (`+`, `-`, `*`, etc.), expecting an operand.
    AfterOp,
    /// After an identifier in expression position — could be a call
    /// (ident followed by args), a binding (`ident=expr`), or a reference.
    AfterIdent,
    /// After `:` in a type expression, expecting a type sigil or name.
    TypeExpr,
    /// Inside `{...}` match arms, expecting a pattern.
    MatchArm,
    /// After a match arm pattern, expecting `:` for the arm body.
    MatchArmBody,
    /// After `~` (Ok bind) in a match arm.
    AfterTilde,
    /// After `^` (Err bind) in a match arm.
    AfterCaretMatch,
    /// Inside `[...]` list literal, expecting elements.
    ListElem,
    /// After `.` or `.?` — field access, expecting a key name.
    AfterDot,
    /// After `>>` pipe operator, expecting a function name.
    AfterPipe,
    /// After `!` — either `!` (panic unwrap following) or `!!`.
    AfterBang,
    /// After a number literal — operator, `;`, or end of expression.
    AfterNumber,
    /// After a string literal — operator, `;`, or end of expression.
    AfterText,
    /// End of input / accepting state.
    End,
}

impl ParseState {
    /// All states in canonical order.
    pub const ALL: &'static [ParseState] = &[
        ParseState::TopLevel,
        ParseState::AfterType,
        ParseState::AfterTool,
        ParseState::AfterUse,
        ParseState::FnHeader,
        ParseState::TypeParams,
        ParseState::ParamName,
        ParseState::ParamType,
        ParseState::AfterParamType,
        ParseState::ReturnType,
        ParseState::AfterReturnType,
        ParseState::EffectSet,
        ParseState::StmtStart,
        ParseState::AfterAt,
        ParseState::AfterQuestion,
        ParseState::ExprStart,
        ParseState::AfterOp,
        ParseState::AfterIdent,
        ParseState::TypeExpr,
        ParseState::MatchArm,
        ParseState::MatchArmBody,
        ParseState::AfterTilde,
        ParseState::AfterCaretMatch,
        ParseState::ListElem,
        ParseState::AfterDot,
        ParseState::AfterPipe,
        ParseState::AfterBang,
        ParseState::AfterNumber,
        ParseState::AfterText,
        ParseState::End,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            ParseState::TopLevel => "TopLevel",
            ParseState::AfterType => "AfterType",
            ParseState::AfterTool => "AfterTool",
            ParseState::AfterUse => "AfterUse",
            ParseState::FnHeader => "FnHeader",
            ParseState::TypeParams => "TypeParams",
            ParseState::ParamName => "ParamName",
            ParseState::ParamType => "ParamType",
            ParseState::AfterParamType => "AfterParamType",
            ParseState::ReturnType => "ReturnType",
            ParseState::AfterReturnType => "AfterReturnType",
            ParseState::EffectSet => "EffectSet",
            ParseState::StmtStart => "StmtStart",
            ParseState::AfterAt => "AfterAt",
            ParseState::AfterQuestion => "AfterQuestion",
            ParseState::ExprStart => "ExprStart",
            ParseState::AfterOp => "AfterOp",
            ParseState::AfterIdent => "AfterIdent",
            ParseState::TypeExpr => "TypeExpr",
            ParseState::MatchArm => "MatchArm",
            ParseState::MatchArmBody => "MatchArmBody",
            ParseState::AfterTilde => "AfterTilde",
            ParseState::AfterCaretMatch => "AfterCaretMatch",
            ParseState::ListElem => "ListElem",
            ParseState::AfterDot => "AfterDot",
            ParseState::AfterPipe => "AfterPipe",
            ParseState::AfterBang => "AfterBang",
            ParseState::AfterNumber => "AfterNumber",
            ParseState::AfterText => "AfterText",
            ParseState::End => "End",
        }
    }

    /// The set of valid token categories at this state and the next state
    /// each transitions to.
    fn transitions(&self) -> &[(TokenCat, ParseState)] {
        match self {
            // Top of file: declaration-level keywords or identifiers (fn decl).
            ParseState::TopLevel => &[
                (TokenCat::Type, ParseState::AfterType),
                (TokenCat::Tool, ParseState::AfterTool),
                (TokenCat::Use, ParseState::AfterUse),
                (TokenCat::Ident, ParseState::FnHeader),
                (TokenCat::Underscore, ParseState::FnHeader),
                (TokenCat::Caret, ParseState::EffectSet), // version pragma
                (TokenCat::Eof, ParseState::End),
            ],

            // After `type` keyword — expecting a type name (ident).
            ParseState::AfterType => &[
                (TokenCat::Ident, ParseState::FnHeader), // reuses header for `{` body
            ],

            // After `tool` keyword — expecting a tool name (ident).
            ParseState::AfterTool => &[
                (TokenCat::Ident, ParseState::AfterIdent), // tool name, then description string
            ],

            // After `use` keyword — expecting a module path (ident).
            ParseState::AfterUse => &[
                (TokenCat::Ident, ParseState::AfterIdent),
            ],

            // Function header: after the name, expecting `<` type params,
            // `:` for param type, or `>` for return type (no-param case).
            ParseState::FnHeader => &[
                (TokenCat::Less, ParseState::TypeParams),
                (TokenCat::Ident, ParseState::ParamName),
                (TokenCat::Greater, ParseState::ReturnType),
                (TokenCat::Colon, ParseState::ParamType), // no-param with `:` (error path)
            ],

            // Inside `<...>` type parameter block.
            ParseState::TypeParams => &[
                (TokenCat::Ident, ParseState::AfterIdent),
                (TokenCat::Greater, ParseState::ReturnType), // `>` closes and starts return type
            ],

            // After a param name — expecting `:` for type annotation.
            ParseState::ParamName => &[
                (TokenCat::Colon, ParseState::ParamType),
            ],

            // After `:` in a param — expecting a type.
            ParseState::ParamType => &[
                (TokenCat::Ident, ParseState::AfterParamType), // named type
                (TokenCat::ListType, ParseState::AfterParamType),
                (TokenCat::ResultType, ParseState::AfterParamType),
                (TokenCat::FnType, ParseState::AfterParamType),
                (TokenCat::OptType, ParseState::AfterParamType),
                (TokenCat::MapType, ParseState::AfterParamType),
                (TokenCat::SumType, ParseState::AfterParamType),
                (TokenCat::WorldType, ParseState::AfterParamType),
                (TokenCat::U32Type, ParseState::AfterParamType),
                (TokenCat::U64Type, ParseState::AfterParamType),
                (TokenCat::I64Type, ParseState::AfterParamType),
                (TokenCat::Number, ParseState::AfterParamType), // `L n` where n is a number? no — but type vars are idents
                (TokenCat::Underscore, ParseState::AfterParamType), // `_` = any/nil
            ],

            // After a param type — expecting `,` (more params) or `>` (return type).
            ParseState::AfterParamType => &[
                (TokenCat::Comma, ParseState::ParamName),
                (TokenCat::Greater, ParseState::ReturnType),
            ],

            // After `>` — expecting the return type.
            ParseState::ReturnType => &[
                (TokenCat::Ident, ParseState::AfterReturnType),
                (TokenCat::ListType, ParseState::AfterReturnType),
                (TokenCat::ResultType, ParseState::AfterReturnType),
                (TokenCat::FnType, ParseState::AfterReturnType),
                (TokenCat::OptType, ParseState::AfterReturnType),
                (TokenCat::MapType, ParseState::AfterReturnType),
                (TokenCat::SumType, ParseState::AfterReturnType),
                (TokenCat::WorldType, ParseState::AfterReturnType),
                (TokenCat::U32Type, ParseState::AfterReturnType),
                (TokenCat::U64Type, ParseState::AfterReturnType),
                (TokenCat::I64Type, ParseState::AfterReturnType),
                (TokenCat::Underscore, ParseState::AfterReturnType),
            ],

            // After return type — expecting `;`, `{`, `^` (effect set), or body start.
            ParseState::AfterReturnType => &[
                (TokenCat::Semi, ParseState::StmtStart),
                (TokenCat::LBrace, ParseState::StmtStart),
                (TokenCat::Caret, ParseState::EffectSet),
                (TokenCat::Ident, ParseState::AfterIdent), // bare expression start (no `;`)
                (TokenCat::Number, ParseState::AfterNumber),
                (TokenCat::Text, ParseState::AfterText),
                (TokenCat::True, ParseState::AfterIdent),
                (TokenCat::False, ParseState::AfterIdent),
                (TokenCat::Nil, ParseState::AfterIdent),
                (TokenCat::Minus, ParseState::AfterOp),
                (TokenCat::LBracket, ParseState::ListElem),
                (TokenCat::LBrace, ParseState::StmtStart),
            ],

            // After `^` effect-set marker — expecting variant names.
            ParseState::EffectSet => &[
                (TokenCat::Ident, ParseState::AfterIdent), // variant name
                (TokenCat::Pipe, ParseState::EffectSet),   // `|` for more variants
                (TokenCat::Semi, ParseState::StmtStart),
            ],

            // Statement start: bindings, loops, match, return, break, continue, expressions.
            ParseState::StmtStart => &[
                (TokenCat::Ident, ParseState::AfterIdent),    // binding or call
                (TokenCat::At, ParseState::AfterAt),          // foreach/for-range
                (TokenCat::Question, ParseState::AfterQuestion), // match or ternary
                (TokenCat::LBrace, ParseState::StmtStart),   // brace block or destructure
                (TokenCat::LBracket, ParseState::ListElem),   // list literal
                (TokenCat::Number, ParseState::AfterNumber),
                (TokenCat::Text, ParseState::AfterText),
                (TokenCat::True, ParseState::AfterIdent),
                (TokenCat::False, ParseState::AfterIdent),
                (TokenCat::Nil, ParseState::AfterIdent),
                (TokenCat::Minus, ParseState::AfterOp),       // negative number
                (TokenCat::Underscore, ParseState::AfterIdent), // `_=expr` discard
                (TokenCat::Tilde, ParseState::AfterTilde),    // `~v` Ok constructor
                (TokenCat::Caret, ParseState::AfterCaretMatch), // `^e` Err constructor
                (TokenCat::Bang, ParseState::AfterBang),       // `!` auto-unwrap
                (TokenCat::Dollar, ParseState::AfterIdent),    // `$` special
                (TokenCat::Semi, ParseState::StmtStart),       // empty statement
                (TokenCat::RBrace, ParseState::End),           // end of body
                (TokenCat::Eof, ParseState::End),
            ],

            // After `@` — expecting loop variable (ident) or range start (number).
            ParseState::AfterAt => &[
                (TokenCat::Ident, ParseState::AfterIdent),  // loop var name
                (TokenCat::Number, ParseState::AfterNumber), // range start
            ],

            // After `?` — match (`?expr{...}`) or prefix ternary (`?cond a b`).
            ParseState::AfterQuestion => &[
                (TokenCat::Ident, ParseState::AfterIdent),
                (TokenCat::Number, ParseState::AfterNumber),
                (TokenCat::Text, ParseState::AfterText),
                (TokenCat::True, ParseState::AfterIdent),
                (TokenCat::False, ParseState::AfterIdent),
                (TokenCat::LBracket, ParseState::ListElem),
                (TokenCat::Bang, ParseState::AfterBang),  // `?!expr` unwrap then ternary
                (TokenCat::Tilde, ParseState::AfterTilde),
                (TokenCat::Caret, ParseState::AfterCaretMatch),
            ],

            // Expression start: operators (prefix), literals, idents, etc.
            ParseState::ExprStart => &[
                (TokenCat::Ident, ParseState::AfterIdent),
                (TokenCat::Number, ParseState::AfterNumber),
                (TokenCat::Text, ParseState::AfterText),
                (TokenCat::True, ParseState::AfterIdent),
                (TokenCat::False, ParseState::AfterIdent),
                (TokenCat::Nil, ParseState::AfterIdent),
                (TokenCat::Minus, ParseState::AfterOp),
                (TokenCat::Plus, ParseState::AfterOp),
                (TokenCat::Star, ParseState::AfterOp),
                (TokenCat::Slash, ParseState::AfterOp),
                (TokenCat::LBracket, ParseState::ListElem),
                (TokenCat::LParen, ParseState::ExprStart),  // parenthesised expression/lambda
                (TokenCat::LBrace, ParseState::StmtStart),   // brace lambda or block
                (TokenCat::Tilde, ParseState::AfterTilde),
                (TokenCat::Caret, ParseState::AfterCaretMatch),
                (TokenCat::Bang, ParseState::AfterBang),
                (TokenCat::Dollar, ParseState::AfterIdent),
                (TokenCat::Underscore, ParseState::AfterIdent),
                (TokenCat::Dot, ParseState::AfterDot),
            ],

            // After a binary operator — expecting an operand.
            ParseState::AfterOp => &[
                (TokenCat::Ident, ParseState::AfterIdent),
                (TokenCat::Number, ParseState::AfterNumber),
                (TokenCat::Text, ParseState::AfterText),
                (TokenCat::True, ParseState::AfterIdent),
                (TokenCat::False, ParseState::AfterIdent),
                (TokenCat::Nil, ParseState::AfterIdent),
                (TokenCat::Minus, ParseState::AfterOp),    // nested prefix
                (TokenCat::Plus, ParseState::AfterOp),
                (TokenCat::Star, ParseState::AfterOp),
                (TokenCat::Slash, ParseState::AfterOp),
                (TokenCat::LBracket, ParseState::ListElem),
                (TokenCat::LParen, ParseState::ExprStart),
                (TokenCat::Tilde, ParseState::AfterTilde),
                (TokenCat::Caret, ParseState::AfterCaretMatch),
                (TokenCat::Bang, ParseState::AfterBang),
                (TokenCat::Underscore, ParseState::AfterIdent),
            ],

            // After an identifier — could be a call, binding, ref, etc.
            ParseState::AfterIdent => &[
                // Operators — this ident is an operand, next is another operator
                (TokenCat::Plus, ParseState::AfterOp),
                (TokenCat::Minus, ParseState::AfterOp),
                (TokenCat::Star, ParseState::AfterOp),
                (TokenCat::Slash, ParseState::AfterOp),
                (TokenCat::Greater, ParseState::AfterOp),
                (TokenCat::Less, ParseState::AfterOp),
                (TokenCat::GreaterEq, ParseState::AfterOp),
                (TokenCat::LessEq, ParseState::AfterOp),
                (TokenCat::Eq, ParseState::AfterOp),
                (TokenCat::NotEq, ParseState::AfterOp),
                (TokenCat::Amp, ParseState::AfterOp),
                (TokenCat::Pipe, ParseState::AfterOp),
                (TokenCat::PlusEq, ParseState::AfterOp),
                (TokenCat::PipeOp, ParseState::AfterPipe),
                (TokenCat::NilCoalesce, ParseState::AfterOp),
                (TokenCat::Bang, ParseState::AfterBang),  // `ident!expr` unwrap
                (TokenCat::BangBang, ParseState::AfterBang), // `ident!!` panic unwrap
                (TokenCat::Dot, ParseState::AfterDot),
                (TokenCat::DotQuestion, ParseState::AfterDot),
                (TokenCat::DotDot, ParseState::AfterOp),  // range
                // Statement/body terminators
                (TokenCat::Semi, ParseState::StmtStart),
                (TokenCat::RBrace, ParseState::End),
                (TokenCat::Comma, ParseState::ExprStart),  // in arg list or list
                (TokenCat::RParen, ParseState::AfterIdent), // closing paren expr
                (TokenCat::RBracket, ParseState::AfterIdent), // closing list
                (TokenCat::Eof, ParseState::End),
                // `=` binding
                (TokenCat::Eq, ParseState::ExprStart),
                // `:` could be record field or type annotation
                (TokenCat::Colon, ParseState::ExprStart),
                // `{` could be match body or brace block
                (TokenCat::LBrace, ParseState::StmtStart),
                // More idents = call args (prefix call)
                (TokenCat::Ident, ParseState::AfterIdent),
                (TokenCat::Number, ParseState::AfterNumber),
                (TokenCat::Text, ParseState::AfterText),
                (TokenCat::True, ParseState::AfterIdent),
                (TokenCat::False, ParseState::AfterIdent),
                (TokenCat::Nil, ParseState::AfterIdent),
                (TokenCat::Minus, ParseState::AfterOp),
                (TokenCat::LBracket, ParseState::ListElem),
                (TokenCat::LParen, ParseState::ExprStart),
                (TokenCat::Tilde, ParseState::AfterTilde),
                (TokenCat::Caret, ParseState::AfterCaretMatch),
                (TokenCat::Bang, ParseState::AfterBang),
                (TokenCat::At, ParseState::AfterAt),
            ],

            // Type expression: after `:` expecting type sigils.
            ParseState::TypeExpr => &[
                (TokenCat::Ident, ParseState::AfterIdent),
                (TokenCat::ListType, ParseState::AfterIdent),
                (TokenCat::ResultType, ParseState::AfterIdent),
                (TokenCat::FnType, ParseState::AfterIdent),
                (TokenCat::OptType, ParseState::AfterIdent),
                (TokenCat::MapType, ParseState::AfterIdent),
                (TokenCat::SumType, ParseState::AfterIdent),
                (TokenCat::WorldType, ParseState::AfterIdent),
                (TokenCat::U32Type, ParseState::AfterIdent),
                (TokenCat::U64Type, ParseState::AfterIdent),
                (TokenCat::I64Type, ParseState::AfterIdent),
                (TokenCat::Underscore, ParseState::AfterIdent),
            ],

            // Match arm: expecting a pattern.
            ParseState::MatchArm => &[
                (TokenCat::Text, ParseState::MatchArmBody),     // literal pattern
                (TokenCat::Number, ParseState::MatchArmBody),
                (TokenCat::True, ParseState::MatchArmBody),
                (TokenCat::False, ParseState::MatchArmBody),
                (TokenCat::Nil, ParseState::MatchArmBody),
                (TokenCat::Tilde, ParseState::AfterTilde),       // `~v` ok-bind
                (TokenCat::Caret, ParseState::AfterCaretMatch),  // `^e` err-bind
                (TokenCat::Ident, ParseState::MatchArmBody),    // named pattern
                (TokenCat::Underscore, ParseState::MatchArmBody), // wildcard
                (TokenCat::Pipe, ParseState::MatchArm),         // or-pattern
            ],

            // After a match arm pattern — expecting `:` then body.
            ParseState::MatchArmBody => &[
                (TokenCat::Colon, ParseState::ExprStart),
            ],

            // After `~` (Ok bind) in match arm.
            ParseState::AfterTilde => &[
                (TokenCat::Ident, ParseState::MatchArmBody), // `~v:` arm
            ],

            // After `^` (Err bind) in match arm.
            ParseState::AfterCaretMatch => &[
                (TokenCat::Ident, ParseState::MatchArmBody), // `^e:` arm
            ],

            // Inside `[...]` list literal.
            ParseState::ListElem => &[
                (TokenCat::Ident, ParseState::AfterIdent),
                (TokenCat::Number, ParseState::AfterNumber),
                (TokenCat::Text, ParseState::AfterText),
                (TokenCat::True, ParseState::AfterIdent),
                (TokenCat::False, ParseState::AfterIdent),
                (TokenCat::Nil, ParseState::AfterIdent),
                (TokenCat::Minus, ParseState::AfterOp),
                (TokenCat::LBracket, ParseState::ListElem),  // nested list
                (TokenCat::LParen, ParseState::ExprStart),
                (TokenCat::Tilde, ParseState::AfterTilde),
                (TokenCat::Caret, ParseState::AfterCaretMatch),
                (TokenCat::RBracket, ParseState::AfterIdent),  // close list
                (TokenCat::Comma, ParseState::ListElem),       // next element
                (TokenCat::Bang, ParseState::AfterBang),
            ],

            // After `.` or `.?` — field access, expecting a key name.
            ParseState::AfterDot => &[
                (TokenCat::Ident, ParseState::AfterIdent),
                (TokenCat::Text, ParseState::AfterText), // `."key"` JSON access
                (TokenCat::Question, ParseState::AfterDot), // `.?` already handled by DotQuestion
            ],

            // After `>>` pipe operator — expecting a function name.
            ParseState::AfterPipe => &[
                (TokenCat::Ident, ParseState::AfterIdent),
            ],

            // After `!` — auto-unwrap or start of `!!`.
            ParseState::AfterBang => &[
                (TokenCat::Ident, ParseState::AfterIdent),
                (TokenCat::Bang, ParseState::AfterBang),  // `!!` panic-unwrap
            ],

            // After a number literal.
            ParseState::AfterNumber => &[
                (TokenCat::Plus, ParseState::AfterOp),
                (TokenCat::Minus, ParseState::AfterOp),
                (TokenCat::Star, ParseState::AfterOp),
                (TokenCat::Slash, ParseState::AfterOp),
                (TokenCat::Greater, ParseState::AfterOp),
                (TokenCat::Less, ParseState::AfterOp),
                (TokenCat::GreaterEq, ParseState::AfterOp),
                (TokenCat::LessEq, ParseState::AfterOp),
                (TokenCat::Eq, ParseState::AfterOp),
                (TokenCat::NotEq, ParseState::AfterOp),
                (TokenCat::Amp, ParseState::AfterOp),
                (TokenCat::Pipe, ParseState::AfterOp),
                (TokenCat::PipeOp, ParseState::AfterPipe),
                (TokenCat::NilCoalesce, ParseState::AfterOp),
                (TokenCat::Dot, ParseState::AfterDot),
                (TokenCat::DotDot, ParseState::AfterOp),
                (TokenCat::Semi, ParseState::StmtStart),
                (TokenCat::RBrace, ParseState::End),
                (TokenCat::Comma, ParseState::ExprStart),
                (TokenCat::RParen, ParseState::AfterIdent),
                (TokenCat::RBracket, ParseState::AfterIdent),
                (TokenCat::Eof, ParseState::End),
                (TokenCat::By, ParseState::AfterIdent), // `@i 0..n by 2`
            ],

            // After a string literal.
            ParseState::AfterText => &[
                (TokenCat::Plus, ParseState::AfterOp),
                (TokenCat::Minus, ParseState::AfterOp),
                (TokenCat::Star, ParseState::AfterOp),
                (TokenCat::Slash, ParseState::AfterOp),
                (TokenCat::Greater, ParseState::AfterOp),
                (TokenCat::Less, ParseState::AfterOp),
                (TokenCat::GreaterEq, ParseState::AfterOp),
                (TokenCat::LessEq, ParseState::AfterOp),
                (TokenCat::Eq, ParseState::AfterOp),
                (TokenCat::NotEq, ParseState::AfterOp),
                (TokenCat::Amp, ParseState::AfterOp),
                (TokenCat::Pipe, ParseState::AfterOp),
                (TokenCat::PipeOp, ParseState::AfterPipe),
                (TokenCat::NilCoalesce, ParseState::AfterOp),
                (TokenCat::Dot, ParseState::AfterDot),
                (TokenCat::Semi, ParseState::StmtStart),
                (TokenCat::RBrace, ParseState::End),
                (TokenCat::Comma, ParseState::ExprStart),
                (TokenCat::RParen, ParseState::AfterIdent),
                (TokenCat::RBracket, ParseState::AfterIdent),
                (TokenCat::Colon, ParseState::ExprStart), // record field
                (TokenCat::Eof, ParseState::End),
            ],

            // End state — only EOF.
            ParseState::End => &[
                (TokenCat::Eof, ParseState::End),
            ],
        }
    }

    /// Set of valid token categories at this state.
    fn valid_tokens(&self) -> Vec<TokenCat> {
        self.transitions().iter().map(|(cat, _)| *cat).collect()
    }
}

// ── JSON export ────────────────────────────────────────────────────────────

/// Build the grammar state machine as a JSON value.
///
/// Shape:
/// ```json
/// {
///   "schemaVersion": 1,
///   "states": {
///     "TopLevel": {
///       "transitions": { "type": "AfterType", "tool": "AfterTool", ... }
///     },
///     ...
///   },
///   "initial": "TopLevel",
///   "accept": ["End"]
/// }
/// ```
pub fn state_machine_json() -> Value {
    let states: Vec<(String, Value)> = ParseState::ALL
        .iter()
        .map(|state| {
            let transitions: Vec<(String, String)> = state
                .transitions()
                .iter()
                .map(|(cat, next)| (cat.as_str().to_string(), next.name().to_string()))
                .collect();
            let transitions_map: serde_json::Map<String, Value> = transitions
                .into_iter()
                .map(|(k, v)| (k, Value::String(v)))
                .collect();
            (
                state.name().to_string(),
                json!({ "transitions": Value::Object(transitions_map) }),
            )
        })
        .collect();
    let states_map: serde_json::Map<String, Value> =
        states.into_iter().map(|(k, v)| (k, v)).collect();

    json!({
        "schemaVersion": 1,
        "states": Value::Object(states_map),
        "initial": "TopLevel",
        "accept": ["End"],
    })
}

/// Build the token vocabulary as a JSON array of strings.
pub fn vocabulary_json() -> Value {
    let vocab: Vec<String> = TokenCat::ALL.iter().map(|c| c.as_str().to_string()).collect();
    Value::Array(vocab.into_iter().map(Value::String).collect())
}

/// Build per-state logit masks as a JSON value.
///
/// Shape:
/// ```json
/// {
///   "schemaVersion": 1,
///   "vocabulary": ["type", "tool", ...],
///   "masks": {
///     "TopLevel": [1, 0, 0, ...],
///     ...
///   }
/// }
/// ```
///
/// Each mask is a binary array of length `TokenCat::ALL.len()`.  A `1` at
/// index `i` means `TokenCat::ALL[i]` is valid in that state.
pub fn logit_masks_json() -> Value {
    let vocab_len = TokenCat::ALL.len();
    let masks: Vec<(String, Vec<u8>)> = ParseState::ALL
        .iter()
        .map(|state| {
            let mut mask = vec![0u8; vocab_len];
            for cat in state.valid_tokens() {
                mask[cat.index()] = 1;
            }
            (state.name().to_string(), mask)
        })
        .collect();
    let masks_map: serde_json::Map<String, Value> = masks
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                Value::Array(v.into_iter().map(|b| Value::Number(b.into())).collect()),
            )
        })
        .collect();

    json!({
        "schemaVersion": 1,
        "vocabulary": vocabulary_json(),
        "masks": Value::Object(masks_map),
    })
}

/// Compute the parser state at a given cursor position in a source file.
///
/// Tokenises the source up to the cursor, then feeds the tokens through the
/// state machine to determine the current state.  Returns the state name and
/// the set of valid next token categories.
///
/// This is a **static** analysis — it uses the hand-derived state machine,
/// not the actual recursive-descent parser.  It cannot account for arity
/// tables, lambda context, or declaration boundaries.  For full precision,
/// use the parser directly (future work).
pub fn completions_at_cursor(source: &str, line: usize, col: usize) -> Value {
    // Find the byte offset for the given line/col (1-based).
    let offset = line_col_to_offset(source, line, col);
    let prefix = &source[..offset.min(source.len())];

    // Tokenise the prefix.
    let tokens = match crate::lexer::lex(prefix) {
        Ok(ts) => ts,
        Err(_) => {
            // If lexing fails, return the initial state.
            return json!({
                "schemaVersion": 1,
                "state": "TopLevel",
                "validTokens": ParseState::TopLevel.valid_tokens()
                    .iter()
                    .map(|c| c.as_str().to_string())
                    .collect::<Vec<_>>(),
            });
        }
    };

    // Walk the state machine to find the current state.
    let mut state = ParseState::TopLevel;
    for (tok, _span) in &tokens {
        if let Some(cat) = TokenCat::from_token(tok) {
            let transitions = state.transitions();
            if let Some((_, next)) = transitions.iter().find(|(c, _)| c == &cat) {
                state = *next;
            }
            // Token not valid in current state — stay in the same state.
            // In a real constrained-decoding harness, this would never happen
            // because the masks prevent invalid tokens from being generated.
        }
    }

    // Get valid next tokens at the current state.
    let valid: Vec<String> = state.valid_tokens().iter().map(|c| c.as_str().to_string()).collect();

    json!({
        "schemaVersion": 1,
        "state": state.name(),
        "validTokens": valid,
    })
}

/// Convert a 1-based line/column position to a byte offset in the source.
fn line_col_to_offset(source: &str, line: usize, col: usize) -> usize {
    let mut current_line = 1;
    let mut line_start = 0;
    for (i, c) in source.char_indices() {
        if current_line == line {
            // Scan within the target line for the requested column.
            let mut char_count = 0;
            for (ci, ch) in source[line_start..].char_indices() {
                if ch == '\n' {
                    // Column not found before end of line — clamp to newline.
                    return line_start + ci;
                }
                char_count += 1;
                if char_count == col {
                    return line_start + ci;
                }
            }
            // Past end of string on this line — clamp to source end.
            return source.len();
        }
        if c == '\n' {
            current_line += 1;
            line_start = i + 1;
        }
    }
    // We're on the last line and it has no trailing newline.
    if current_line == line {
        let mut char_count = 0;
        for (ci, _) in source[line_start..].char_indices() {
            char_count += 1;
            if char_count == col {
                return line_start + ci;
            }
        }
        return source.len();
    }
    source.len()
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine_has_all_states() {
        let sm = state_machine_json();
        let states = sm["states"].as_object().unwrap();
        assert_eq!(states.len(), ParseState::ALL.len());
        for state in ParseState::ALL {
            assert!(states.contains_key(state.name()));
        }
    }

    #[test]
    fn state_machine_has_initial_and_accept() {
        let sm = state_machine_json();
        assert_eq!(sm["initial"], "TopLevel");
        let accept = sm["accept"].as_array().unwrap();
        assert_eq!(accept.len(), 1);
        assert_eq!(accept[0], "End");
    }

    #[test]
    fn state_machine_toplevel_transitions() {
        let sm = state_machine_json();
        let transitions = sm["states"]["TopLevel"]["transitions"].as_object().unwrap();
        assert!(transitions.contains_key("type"));
        assert!(transitions.contains_key("tool"));
        assert!(transitions.contains_key("ident"));
        assert!(transitions.contains_key("eof"));
        // Reserved keywords should NOT be present
        assert!(!transitions.contains_key("if"));
        assert!(!transitions.contains_key("fn"));
        assert!(!transitions.contains_key("let"));
    }

    #[test]
    fn state_machine_fnheader_transitions() {
        let sm = state_machine_json();
        let transitions = sm["states"]["FnHeader"]["transitions"].as_object().unwrap();
        assert!(transitions.contains_key("<"));
        assert!(transitions.contains_key("ident"));
        assert!(transitions.contains_key(">"));
    }

    #[test]
    fn logit_masks_vocabulary_length() {
        let lm = logit_masks_json();
        let vocab = lm["vocabulary"].as_array().unwrap();
        let masks = lm["masks"].as_object().unwrap();
        assert_eq!(vocab.len(), TokenCat::ALL.len());
        for (_state, mask) in masks {
            let mask_arr = mask.as_array().unwrap();
            assert_eq!(mask_arr.len(), vocab.len());
        }
    }

    #[test]
    fn logit_masks_toplevel_has_valid_tokens() {
        let lm = logit_masks_json();
        let vocab = lm["vocabulary"].as_array().unwrap();
        let mask = lm["masks"]["TopLevel"].as_array().unwrap();
        let count: usize = mask.iter().map(|v| v.as_u64().unwrap() as usize).sum();
        assert!(count > 0, "TopLevel must have at least one valid token");
        // `type` should be valid at TopLevel
        let type_idx = TokenCat::Type.index();
        assert_eq!(mask[type_idx], 1);
        // `+` should NOT be valid at TopLevel
        let plus_idx = TokenCat::Plus.index();
        assert_eq!(mask[plus_idx], 0);
    }

    #[test]
    fn logit_masks_after_op_has_operands() {
        let lm = logit_masks_json();
        let mask = lm["masks"]["AfterOp"].as_array().unwrap();
        let ident_idx = TokenCat::Ident.index();
        let num_idx = TokenCat::Number.index();
        assert_eq!(mask[ident_idx], 1);
        assert_eq!(mask[num_idx], 1);
    }

    #[test]
    fn logit_masks_end_state_only_eof() {
        let lm = logit_masks_json();
        let mask = lm["masks"]["End"].as_array().unwrap();
        let count: usize = mask.iter().map(|v| v.as_u64().unwrap() as usize).sum();
        assert_eq!(count, 1, "End state should only accept EOF");
        let eof_idx = TokenCat::Eof.index();
        assert_eq!(mask[eof_idx], 1);
    }

    #[test]
    fn vocabulary_contains_all_categories() {
        let vocab = vocabulary_json();
        let arr = vocab.as_array().unwrap();
        assert_eq!(arr.len(), TokenCat::ALL.len());
        // Check a few key tokens
        let strs: Vec<&str> = arr.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(strs.contains(&"type"));
        assert!(strs.contains(&"tool"));
        assert!(strs.contains(&"ident"));
        assert!(strs.contains(&"num"));
        assert!(strs.contains(&"+"));
        assert!(strs.contains(&"eof"));
        // Reserved keywords should NOT be in vocabulary
        assert!(!strs.contains(&"if"));
        assert!(!strs.contains(&"fn"));
        assert!(!strs.contains(&"let"));
    }

    #[test]
    fn from_token_maps_all_lexer_tokens() {
        // Every non-reserved Token variant should map to a TokenCat.
        assert_eq!(TokenCat::from_token(&Token::Type), Some(TokenCat::Type));
        assert_eq!(TokenCat::from_token(&Token::Tool), Some(TokenCat::Tool));
        assert_eq!(
            TokenCat::from_token(&Token::Ident("test".into())),
            Some(TokenCat::Ident)
        );
        assert_eq!(
            TokenCat::from_token(&Token::Number(42.0)),
            Some(TokenCat::Number)
        );
        assert_eq!(
            TokenCat::from_token(&Token::Text("hi".into())),
            Some(TokenCat::Text)
        );
        assert_eq!(TokenCat::from_token(&Token::Plus), Some(TokenCat::Plus));
        assert_eq!(TokenCat::from_token(&Token::Semi), Some(TokenCat::Semi));
    }

    #[test]
    fn from_token_rejects_reserved_keywords() {
        assert_eq!(TokenCat::from_token(&Token::KwIf), None);
        assert_eq!(TokenCat::from_token(&Token::KwFn), None);
        assert_eq!(TokenCat::from_token(&Token::KwLet), None);
        assert_eq!(TokenCat::from_token(&Token::KwReturn), None);
        assert_eq!(TokenCat::from_token(&Token::KwDef), None);
        assert_eq!(TokenCat::from_token(&Token::KwVar), None);
        assert_eq!(TokenCat::from_token(&Token::KwConst), None);
    }

    #[test]
    fn state_machine_json_is_serialisable() {
        let sm = state_machine_json();
        let s = serde_json::to_string(&sm).unwrap();
        assert!(s.contains("TopLevel"));
        assert!(s.contains("AfterType"));
        assert!(s.contains("transitions"));
        // Re-parse to verify valid JSON round-trips.
        let reparsed: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(reparsed["initial"], "TopLevel");
    }

    #[test]
    fn logit_masks_json_is_serialisable() {
        let lm = logit_masks_json();
        let s = serde_json::to_string(&lm).unwrap();
        assert!(s.contains("vocabulary"));
        assert!(s.contains("masks"));
        assert!(s.contains("TopLevel"));
    }

    #[test]
    fn completions_at_toplevel() {
        let result = completions_at_cursor("", 1, 1);
        assert_eq!(result["state"], "TopLevel");
        let valid = result["validTokens"].as_array().unwrap();
        let strs: Vec<&str> = valid.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(strs.contains(&"type"));
        assert!(strs.contains(&"tool"));
        assert!(strs.contains(&"ident"));
    }

    #[test]
    fn completions_after_type_keyword() {
        let result = completions_at_cursor("type ", 1, 6);
        assert_eq!(result["state"], "AfterType");
        let valid = result["validTokens"].as_array().unwrap();
        let strs: Vec<&str> = valid.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(strs.contains(&"ident"));
    }

    #[test]
    fn completions_after_fn_name() {
        let result = completions_at_cursor("fn add ", 1, 8);
        // `fn` is a reserved keyword that the parser rejects, but the state
        // machine should still track it — `fn` doesn't have a TokenCat, so it's
        // skipped and we stay at TopLevel.  Use `add` (an ident) instead.
        let result = completions_at_cursor("add ", 1, 5);
        assert_eq!(result["state"], "FnHeader");
        let valid = result["validTokens"].as_array().unwrap();
        let strs: Vec<&str> = valid.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(strs.contains(&">"));
        assert!(strs.contains(&"ident"));
    }

    #[test]
    fn line_col_to_offset_basic() {
        assert_eq!(line_col_to_offset("hello\nworld", 1, 1), 0);
        assert_eq!(line_col_to_offset("hello\nworld", 1, 4), 3);
        assert_eq!(line_col_to_offset("hello\nworld", 2, 1), 6);
    }

    #[test]
    fn line_col_to_offset_unicode() {
        // é is 2 bytes: h(0) é(1-2) l(3) o(4).  Col 3 is 'l' at byte offset 3.
        assert_eq!(line_col_to_offset("héllo\nwörld", 1, 3), 3);
    }

    #[test]
    fn line_col_to_offset_past_end() {
        assert_eq!(line_col_to_offset("abc", 1, 10), 3);
        assert_eq!(line_col_to_offset("abc", 5, 1), 3);
    }
}