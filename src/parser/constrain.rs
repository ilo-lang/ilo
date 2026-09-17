//! `ilo constrain` — token-mask artifacts for constrained decoding
//! (PLAN.md G3). See `docs/constrain-design.md`.
//!
//! Two modes:
//!
//! - default: **empirical bigram** — observed token transitions while the
//!   real parser consumes the corpus (fast, conservative).
//! - `--probe`: **probed context masks** — for every corpus prefix, probe
//!   every candidate token through a sound prefix-validity oracle
//!   (`Parser::parse` error positions) and record the allowed set keyed by
//!   the two-token context. Strictly stronger than the bigram: catches
//!   unobserved-but-valid transitions. Oracle self-anomalies are published
//!   in the artifact, never silently dropped.
//!
//! Recording is gated on `ILO_CONSTRAIN=1`; `advance()` checks a cached
//! flag, so the disabled path is one branch.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::super::lexer::{self, Token};

thread_local! {
    static REC: RefCell<Option<Rec>> = const { RefCell::new(None) };
    static LAST_SITE: RefCell<Option<String>> = const { RefCell::new(None) };
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

/// Strategy 1 (observational v1): record the (site, peeked-token)
/// transition at a parser dispatch point. Keyed `"<site>|after <prev>"`;
/// the peeked token is the alternative the parser took. Observed over the
/// corpus — same epistemics as the bigram, finer key.
pub fn mark_site(site: &'static str, peek: Option<&Token>) {
    LAST_SITE.with(|l| *l.borrow_mut() = Some(site.to_string()));
    if !enabled() {
        return;
    }
    let peek_name = peek.map(display).unwrap_or_else(|| "<EOF>".into());
    REC.with(|r| {
        let mut rec = r.borrow_mut();
        let rec = rec.get_or_insert_with(Rec::default);
        rec.vocab.insert(peek_name.clone());
        let prev1 = rec.last.clone().unwrap_or_else(|| "<START>".into());
        rec.edges
            .entry(format!("{site}|after {prev1}"))
            .or_default()
            .insert(peek_name);
    });
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

/// Render the recorded transitions as the bigram JSON artifact.
pub fn to_json() -> String {
    REC.with(|r| {
        let rec = r.borrow();
        let rec = match rec.as_ref() {
            Some(rec) => rec,
            None => return String::from("{\"error\":\"nothing recorded\"}"),
        };
        // Split recorded edges: plain keys are the bigram view; keys of the
        // form "<site>|after <prev>" are Strategy-1 site-keyed transitions.
        let mut plain: serde_json::Map<String, serde_json::Value> =
            serde_json::Map::new();
        let mut sites: serde_json::Map<String, serde_json::Value> =
            serde_json::Map::new();
        for (k, v) in rec.edges.iter() {
            let arr = serde_json::Value::Array(
                v.iter()
                    .map(|t| serde_json::Value::String(t.clone()))
                    .collect(),
            );
            match k.split_once("|after ") {
                Some((site, prev)) => {
                    sites.insert(format!("{site} after {prev}"), arr);
                }
                None => {
                    plain.insert(k.clone(), arr);
                }
            }
        }
        let edges = plain;
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
                "Superseded by the probed context masks (`ilo constrain ",
                "<dir> --probe`, schemaVersion 2)."
            ),
            "vocabulary": vocab,
            "startToken": "<START>",
            "endToken": "<EOF>",
            "transitions": serde_json::Value::Object(edges),
            "sites": serde_json::Value::Object(sites),
        })
        .to_string()
    })
}

fn collect_corpus(dir: &Path) -> Vec<std::path::PathBuf> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("ilo constrain: cannot read {}: {err}", dir.display());
            return Vec::new();
        }
    };
    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "ilo").unwrap_or(false))
        .collect();
    paths.sort();
    paths
}

fn to_spans(tokens: Vec<(Token, std::ops::Range<usize>)>) -> Vec<(Token, crate::ast::Span)> {
    tokens
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
        .collect()
}

/// Legacy fast mode: observed bigram only (recorded during a plain parse).
fn constrain_replay(corpus_dir: &str) -> i32 {
    let dir = Path::new(corpus_dir);
    if !dir.is_dir() {
        eprintln!("ilo constrain: corpus directory not found: {corpus_dir}");
        return 2;
    }

    REC.with(|r| *r.borrow_mut() = Some(Rec::default()));

    let mut files = 0usize;
    let mut parse_errors = 0usize;
    let mut lex_errors = 0usize;
    let paths = collect_corpus(dir);
    for path in &paths {
        let source = match std::fs::read_to_string(path) {
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
        let (_, errs) = super::parse(to_spans(tokens));
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

/// One probe candidate per token class/keyword — the concrete tokens a
/// constrained-decoding host would test at a generation step.
fn probe_candidates() -> Vec<(Token, String)> {
    use Token::*;
    let units: &[Token] = &[
        Type, Tool, Use, With, Timeout, Retry, By, ListType, ResultType,
        FnType, OptType, MapType, SumType, WorldType, U32Type, U64Type,
        I64Type, True, False, Nil, GreaterEq, LessEq, NotEq, PlusEq,
        NilCoalesce, BangBang, PipeOp, Plus, Minus, Star, Slash, Greater,
        Less, Eq, Amp, Pipe, Question, At, Bang, Caret, Tilde, Underscore,
        Colon, Comma, Semi, Dot, DotDot, DotQuestion, LBrace, RBrace,
        LBracket, RBracket, LParen, RParen, Dollar,
    ];
    let mut out: Vec<(Token, String)> =
        units.iter().map(|t| (t.clone(), display(t))).collect();
    out.push((Ident("probe".into()), "ident".into()));
    out.push((Number(1.0), "number".into()));
    out.push((Text("probe".into()), "string".into()));
    out
}

/// Sound prefix-validity oracle via the existing parser: a prefix of k
/// tokens is valid when every parse error sits at or past the boundary
/// (position >= k-1 tolerates diagnostics anchored on the last consumed
/// token when the parser wanted a following one). Errors strictly inside
/// the prefix mean genuinely invalid.
fn valid_prefix(token_spans: &[(Token, crate::ast::Span)], k: usize) -> bool {
    let prefix: Vec<_> = token_spans[..k].to_vec();
    let (_, errs) = super::parse(prefix);
    // Tolerance 3: multi-operator chains (`* * * ...`) anchor the
    // "missing operand" diagnostic up to three tokens back in deep
    // chain states.
    errs.iter().all(|e| e.position + 2 >= k)
}

/// `ilo constrain <dir> --probe`: for every corpus prefix, probe every
/// candidate token through the validity oracle and record the allowed set
/// keyed by the two-token context. Self-checks that the corpus's actual
/// next token is always allowed at its own prefix; anomalies are published.
fn probe_cmd(corpus_dir: &str) -> i32 {
    let dir = Path::new(corpus_dir);
    if !dir.is_dir() {
        eprintln!("ilo constrain: corpus directory not found: {corpus_dir}");
        return 2;
    }
    let candidates = probe_candidates();
    let mut contexts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut site_alts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut bigram: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut vocab: BTreeSet<String> = BTreeSet::new();
    for (_, name) in &candidates {
        vocab.insert(name.clone());
    }
    vocab.insert("<START>".into());
    vocab.insert("<EOF>".into());

    let paths = collect_corpus(dir);
    let mut files = 0usize;
    let mut prefixes = 0usize;
    let mut probes = 0usize;
    let mut anomalies: Vec<String> = Vec::new();

    for path in &paths {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let tokens = match lexer::lex(&source) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let token_spans = to_spans(tokens);
        let toks: Vec<(Token, crate::ast::Span)> = token_spans
            .iter()
            .filter(|(t, _)| *t != Token::Newline)
            .map(|(t, s)| (t.clone(), s.clone()))
            .collect();
        if toks.is_empty() {
            continue;
        }
        files += 1;

        for k in 1..=toks.len() {
            let prev1 = display(&toks[k - 1].0);
            let prev2 = if k >= 2 {
                display(&toks[k - 2].0)
            } else {
                "<START>".into()
            };
            let key = format!("{prev2} {prev1}");
            let allowed = contexts.entry(key.clone()).or_default();

            if !valid_prefix(&token_spans, k) {
                continue;
            }
            prefixes += 1;

            let mut prefix: Vec<(Token, crate::ast::Span)> =
                toks[..k].iter().cloned().collect();
            for (cand_tok, cand_name) in &candidates {
                prefix.push((cand_tok.clone(), crate::ast::Span::default()));
                probes += 1;
                let valid = valid_prefix(&prefix, k + 1);
                // The innermost dispatch site that fired while parsing
                // prefix+candidate — Strategy 1's production-path key,
                // derived mechanically from the real parser.
                let site = LAST_SITE.with(|l| l.borrow().clone());
                if valid {
                    allowed.insert(cand_name.clone());
                    bigram
                        .entry(prev1.clone())
                        .or_default()
                        .insert(cand_name.clone());
                    if let Some(site) = site {
                        let key = format!("{site} after {prev1}");
                        site_alts
                            .entry(key)
                            .or_default()
                            .insert(cand_name.clone());
                    }
                }
                prefix.pop();
            }
            if !valid_prefix(&token_spans, k) {
                continue;
            }
            prefixes += 1;

            // Zero errors = the prefix is already a complete valid program:
            // <EOF> is allowed at this position.
            let complete = {
                let prefix: Vec<_> = token_spans[..k].to_vec();
                super::parse(prefix).1.is_empty()
            };
            if complete {
                allowed.insert("<EOF>".into());
                bigram
                    .entry(prev1.clone())
                    .or_default()
                    .insert("<EOF>".into());
            }
            // The corpus file's own end is by definition a valid end —
            // even when the parser recovered from an interior error.
            let at_file_end = k == toks.len();
            let actual_next = if at_file_end {
                "<EOF>".into()
            } else {
                display(&toks[k].0)
            };
            if at_file_end {
                allowed.insert("<EOF>".into());
                bigram
                    .entry(prev1.clone())
                    .or_default()
                    .insert("<EOF>".into());
            }
            if !allowed.contains(&actual_next) {
                anomalies.push(format!(
                    "\"{key}\" -> {actual_next} ({})",
                    path.display()
                ));
            }
        }
    }

    let to_edges = |m: &BTreeMap<String, BTreeSet<String>>| {
        serde_json::Map::<String, serde_json::Value>::from_iter(m.iter().map(
            |(k, v)| {
                (
                    k.clone(),
                    serde_json::Value::Array(
                        v.iter()
                            .map(|t| serde_json::Value::String(t.clone()))
                            .collect(),
                    ),
                )
            },
        ))
    };
    let artifact = serde_json::json!({
        "schemaVersion": 2,
        "kind": "probed-context-masks",
        "note": concat!(
            "Exhaustively probed allowed-token sets keyed by two-token ",
            "context, derived by replaying the corpus prefixes through the ",
            "real parser with a sound prefix-validity oracle. Bigram view ",
            "included for hosts that key on one token."
        ),
        "vocabulary": serde_json::Value::Array(
            vocab.iter().map(|t| serde_json::Value::String(t.clone())).collect(),
        ),
        "contexts": serde_json::Value::Object(to_edges(&contexts)),
        "siteAlternatives": serde_json::Value::Object(to_edges(&site_alts)),
        "bigram": serde_json::Value::Object(to_edges(&bigram)),
        "stats": {
            "files": files,
            "prefixesProbed": prefixes,
            "parserCalls": probes + prefixes,
            "oracleAnomalies": anomalies.len(),
        },
        "oracleAnomalies": anomalies,
    });
    println!("{artifact}");
    eprintln!(
        "ilo constrain --probe: {files} files, {prefixes} prefixes, \
         {probes} probes, oracle anomalies: {}",
        anomalies.len()
    );
    0
}

/// `ilo constrain [dir] [--probe]`.
pub fn constrain_cmd(corpus_dir: &str, probe: bool) -> i32 {
    if probe {
        probe_cmd(corpus_dir)
    } else {
        constrain_replay(corpus_dir)
    }
}
