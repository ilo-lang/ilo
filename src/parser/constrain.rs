//! `ilo constrain` — empirical token-transition masks (PLAN.md G3, v0).
//!
//! Records observed token transitions while the real parser consumes the
//! example corpus, then emits a JSON artifact a constrained-decoding host
//! can load to mask tokens that never continue a valid program from the
//! observed context.
//!
//! **This is the empirical v0, not the grammar artifact of record.** It is
//! a bigram over token classes from the `examples/` corpus: unobserved-but-
//! valid transitions are masked off, so it is conservative by construction.
//! The instrumented parser-state machine (Strategy 1 in
//! `docs/constrain-design.md`) supersedes it.
//!
//! Enable recording with `ILO_CONSTRAIN=1` in the environment; `advance()`
//! checks a cached flag, so the disabled path is one branch.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::super::lexer::{self, Token};

thread_local! {
    static REC: RefCell<Option<Rec>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Rec {
    prev: Option<String>,
    last: Option<String>,
    vocab: BTreeSet<String>,
    edges: BTreeMap<String, BTreeSet<String>>,
}

/// Raw vocabulary name for a token. Class tokens (`ident`, `string`,
/// `number`) stand in for infinite literal families; punctuation and
/// keywords map to their source characters. Falls back to the Debug variant
/// name so the vocabulary can never silently miss a kind.
pub fn display(token: &Token) -> String {
    match token {
        Token::Ident(_) => "ident".into(),
        Token::Text(_) => "string".into(),
        Token::Number(_) => "number".into(),
        Token::Newline => "newline".into(),
        Token::Underscore => "_".into(),
        Token::Colon => ":".into(),
        Token::Comma => ",".into(),
        Token::Semi => ";".into(),
        Token::Dot => ".".into(),
        Token::DotDot => "..".into(),
        Token::DotQuestion => ".?".into(),
        Token::LBrace => "{".into(),
        Token::RBrace => "}".into(),
        Token::LBracket => "[".into(),
        Token::RBracket => "]".into(),
        Token::LParen => "(".into(),
        Token::RParen => ")".into(),
        Token::Dollar => "$".into(),
        Token::PipeOp => ">>".into(),
        Token::NilCoalesce => "??".into(),
        Token::BangBang => "!!".into(),
        Token::PlusEq => "+=".into(),
        Token::NotEq => "!=".into(),
        Token::GreaterEq => ">=".into(),
        Token::LessEq => "<=".into(),
        Token::Amp => "&".into(),
        Token::At => "@".into(),
        Token::Bang => "!".into(),
        Token::Caret => "^".into(),
        Token::Tilde => "~".into(),
        Token::Minus => "-".into(),
        Token::Star => "*".into(),
        Token::Slash => "/".into(),
        Token::Plus => "+".into(),
        Token::Greater => ">".into(),
        Token::Less => "<".into(),
        Token::Eq => "=".into(),
        Token::Question => "?".into(),
        Token::Pipe => "|".into(),
        other => format!("{other:?}"),
    }
}

/// True when ILO_CONSTRAIN is set in the environment. Cached after first
/// read; the environment is not re-scanned per token.
pub fn enabled() -> bool {
    thread_local! {
        static ON: bool = std::env::var("ILO_CONSTRAIN").is_ok();
    }
    ON.with(|v| *v)
}

/// Begin a fresh program (resets the previous-token cursor).
pub fn begin_program() {
    if !enabled() {
        return;
    }
    REC.with(|r| {
        if let Some(rec) = &mut *r.borrow_mut() {
            rec.prev = None;
        }
    });
}

/// Record one consumed token (called from `Parser::advance`).
pub fn record(token: &Token) {
    if !enabled() {
        return;
    }
    let name = display(token);
    REC.with(|r| {
        let mut rec = r.borrow_mut();
        let rec = rec.get_or_insert_with(Rec::default);
        rec.vocab.insert(name.clone());
        if let Some(prev) = &rec.prev {
            rec.edges
                .entry(prev.clone())
                .or_default()
                .insert(name.clone());
        } else {
            rec.edges
                .entry("<START>".into())
                .or_default()
                .insert(name.clone());
        }
        rec.prev = Some(name.clone());
        rec.last = Some(name);
    });
}

/// Close a program: link the final token to EOF.
pub fn end_program() {
    if !enabled() {
        return;
    }
    REC.with(|r| {
        if let Some(rec) = &mut *r.borrow_mut() {
            if let Some(last) = &rec.last {
                rec.edges
                    .entry(last.clone())
                    .or_default()
                    .insert("<EOF>".into());
            }
        }
    });
}

/// Render the recorded transitions as the constrain JSON artifact.
pub fn to_json() -> String {
    REC.with(|r| {
        let rec = r.borrow();
        let rec = match rec.as_ref() {
            Some(rec) => rec,
            None => return String::from("{\"error\":\"nothing recorded\"}"),
        };
        let edges: serde_json::Map<String, serde_json::Value> = rec
            .edges
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    serde_json::Value::Array(
                        v.iter()
                            .map(|t| serde_json::Value::String(t.clone()))
                            .collect(),
                    ),
                )
            })
            .collect();
        let vocab = serde_json::Value::Array(
            rec.vocab
                .iter()
                .map(|t| serde_json::Value::String(t.clone()))
                .collect(),
        );
        serde_json::json!({
            "schemaVersion": 1,
            "kind": "empirical-transitions",
            "note": concat!(
                "Observed bigram transitions over the examples/ corpus. ",
                "Conservative: unobserved-but-valid transitions are absent. ",
                "Superseded by the instrumented parser state machine ",
                "(docs/constrain-design.md, Strategy 1)."
            ),
            "vocabulary": vocab,
            "startToken": "<START>",
            "endToken": "<EOF>",
            "transitions": serde_json::Value::Object(edges),
        })
        .to_string()
    })
}

/// `ilo constrain [corpus-dir]` — replay the corpus and print the artifact.
pub fn constrain_cmd(corpus_dir: &str) -> i32 {
    let dir = Path::new(corpus_dir);
    if !dir.is_dir() {
        eprintln!("ilo constrain: corpus directory not found: {corpus_dir}");
        return 2;
    }

    REC.with(|r| *r.borrow_mut() = Some(Rec::default()));

    let mut files = 0usize;
    let mut parse_errors = 0usize;
    let mut lex_errors = 0usize;
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("ilo constrain: cannot read {corpus_dir}: {err}");
            return 2;
        }
    };
    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "ilo").unwrap_or(false))
        .collect();
    paths.sort();

    for path in paths {
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let tokens = match lexer::lex(&source) {
            Ok(t) => t,
            Err(_) => {
                lex_errors += 1;
                continue;
            }
        };
        begin_program();
        let token_spans: Vec<(Token, crate::ast::Span)> = tokens
            .into_iter()
            .map(|(t, r)| {
                (
                    t,
                    crate::ast::Span {
                        start: r.start,
                        end: r.end,
                    },
                )
            })
            .collect();
        let (_, errs) = super::parse(token_spans);
        end_program();
        files += 1;
        if !errs.is_empty() {
            parse_errors += 1;
        }
    }

    if files == 0 {
        eprintln!("ilo constrain: no .ilo files found in {corpus_dir}");
        return 2;
    }

    println!("{}", to_json());
    eprintln!(
        "ilo constrain: {files} files replayed ({lex_errors} lex errors, \
         {parse_errors} with parse errors — transitions recorded up to the \
         error point)"
    );
    0
}
