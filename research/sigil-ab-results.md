# ILO-415: Sigil A/B Experiment — `>>` vs `->` as Pipe Operator

**Date:** 2026-05-22  
**Branch:** `research/sigil-ab-experiment`  
**Status:** Harness + initial results (5 tasks)

## Overview

This experiment tests whether replacing `>>` with `->` as ilo's pipe operator
would be ergonomic and semantically clear. Variant A (`>>`) is the current
implementation. Variant B (`->`) is the proposed alternative.

**Key question:** Does `->` read as a natural left-to-right flow (like Elixir's
`|>` or Haskell's `&`)? And does its absence from the current tokenizer tell us
anything about the implementation cost?

## Variant Definitions

| Variant | Pipe sigil | Result-bind | Status |
|---------|-----------|-------------|--------|
| A | `>>` | `<-` (not used in ilo; `~`/`^` match arms or `!`) | **Current implementation** |
| B | `->` | unchanged | **Not yet implemented** |

## Task Corpus

5 representative tasks were selected to cover:

| Task | Description | Pipe pattern |
|------|-------------|-------------|
| T01 | Basic linear chain (dbl→inc→sq) | `x>>dbl>>inc>>sq` |
| T02 | List pipeline with HOFs | `xs>>flt pos>>map sq` |
| T03 | String builtins chain | `s>>upr>>len` |
| T04 | Pipe inside larger expression (paren-wrap) | `+(a>>dbl) (b>>inc)` |
| T05 | Multi-line pipe continuation | `x\n  >>str\n  >>len` |

## Results

### A/B Check + Run Summary

| Task | Tokens | A check | A run | B check | B run |
|------|-------:|---------|-------|---------|-------|
| t01-basic-pipe | 21 | PASS | PASS | FAIL | SKIP |
| t02-pipe-with-list | 19 | PASS | PASS | FAIL | SKIP |
| t03-pipe-str-transform | 8 | PASS | PASS | FAIL | SKIP |
| t04-pipe-paren-wrap | 20 | PASS | PASS | FAIL | SKIP |
| t05-pipe-multiline | 11 | PASS | PASS | FAIL | SKIP |
| **Total** | — | **5/5** | **5/5** | **0/5** | — |

### Variant B Parse Error Categories

| Task | Error code | Message |
|------|-----------|---------|
| t01-basic-pipe | ILO-P010 | expected expression, got EOF |
| t02-pipe-with-list | ILO-P010 | expected expression, got EOF |
| t03-pipe-str-transform | ILO-P010 | expected expression, got EOF |
| t04-pipe-paren-wrap | ILO-P009 | expected expression, got `)` |
| t05-pipe-multiline | ILO-P009 | expected expression, got `;` |

Two error classes emerge from the parse failures:
- **ILO-P010** (3 tasks): The `-` in `->` is parsed as a unary minus, consuming
  the `>` as a comparison, leaving no expression after it → EOF
- **ILO-P009** (2 tasks): Inside parens or at line continuation, the partial
  parse reaches `)` or `;` without completing the expression

### Token Count Comparison

Token counts are **identical** between variants (both `>>` and `->` are 2 chars).
The `tokcount` builtin (bytes/3.4 approximation) produces the same value for
both sigils. If `->` were adopted, no token budget impact is expected.

## Cross-Language Warning Implication

`->` is currently detected by `warn_cross_language_syntax()` in `src/main.rs`
as a cross-language mistake indicator, with hint text:

```
'->' — ilo uses '>' for return type separator
```

Adopting `->` as pipe would require:
1. Removing this warning for `->` in pipe position
2. Adding a new lexer token (`TokArrow` or reuse existing minus+gt scanning)
3. Parser changes in `parse_pipe_chain()` (or equivalent)

This is a non-trivial but contained change — the parser currently looks for
`PipeOp` (`>>`) specifically in `parse_expr`.

## Initial Findings

1. **`>>` advantage:** Visually distinct, already tokenized, zero ambiguity with
   unary minus. The double-char prevents `-x>>f` misparses.

2. **`->` advantage:** Matches mental model ("flows into"), consistent with
   languages developers already know (Elixir `|>`, F# `|>`, Haskell `&`).
   Slightly more readable in English: `x -> double -> increment`.

3. **`->` collision risk:** Currently a cross-language warning sigil. The `-`
   prefix means the lexer parses `x->f` as `x - (>f)` (comparison), not a pipe.
   All 5 B-variant tasks hit parse errors of this form.

4. **Asymmetry with Result-bind:** ilo uses `~v` / `^e` / `!` for Result
   unwrapping — NOT `<-`. The ticket description mentions leaving `<-` for
   Result-bind unchanged, but `<-` is also **not currently a token** in ilo.
   Adopting `->` for pipe while adding `<-` for Result-bind would be a larger
   two-sigil change.

5. **No token budget difference:** Both sigils produce identical token counts.
   The choice is purely ergonomic/readability.

## Recommendation

The `>>` sigil has zero ambiguity and no implementation risk. `->` is visually
appealing but requires lexer/parser surgery and conflicts with the existing
cross-language warning system. A middle path to explore: `|>` (like Elixir)
avoids the minus-collision entirely.

## Next Steps to Complete

- [ ] Expand task corpus to 10+ tasks (add tasks with `<-` bind if added)
- [ ] Run harness on a patched branch where `->` is actually tokenized
  (`src/lexer.rs` + `src/parser/mod.rs`) to get true B-pass rates
- [ ] Add `|>` as variant C for three-way comparison
- [ ] Test interaction with negative literals: `x->f -1` vs `x>>f -1` — does
  parser ambiguity extend to multi-arg partial-apply pipes?
- [ ] Survey ilo corpus (all `*.ilo` in `examples/` + `tests/`) for `>>` usage
  frequency to understand how many files would need updating if sigil changed
- [ ] Run persona (agent) generation tasks to measure whether `->` or `>>`
  reduces first-shot hallucination in AI-generated ilo code

## Harness

Runnable at: `./research/sigil-ab/run.sh`

Requirements: `ilo` binary (auto-builds from `$CARGO_TARGET_DIR` if not found),
bash, python3.

```bash
# From repo root:
CARGO_TARGET_DIR=/tmp/target-415 ./research/sigil-ab/run.sh
# Or specify binary directly:
./research/sigil-ab/run.sh --ilo=/path/to/ilo
```

Machine-readable results: `research/sigil-ab/results.json`
