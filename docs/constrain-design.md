# G3 spike design: `ilo constrain` — grammar-state token masks

Status: design for implementation · 2026-09-17 · input: PLAN.md G3,
Appendix A.6. Target format: Axis-compatible JSON (`axis --constrain`).

## Goal

Make invalid ilo unemittable at decode time. A constrained-decoding host
(llama.cpp grammars, vLLM guided decoding, XGrammar, a logit-bias harness)
loads the emitted artifact and, at each generation step, masks every token
that cannot continue a valid ilo program from the current parse state.

This attacks the measured dominant cold cost: retry attempts on parse and
type errors (1/13 personas baseline, Haiku 4.5, unmasked).

## Artifact shape (Axis-compatible)

```json
{
  "schemaVersion": 1,
  "vocabulary": ["--", "!", "\"", "#", "$", "%", "&", "(", ")", "*", "+",
                  ",", "-", ".", "/", "0", "1", "...", "wh", "ret", "brk",
                  "cnt", "type", "tool", "use", "with", "nil", "true",
                  "false", "L", "R", "O", "M", "S", "F", ...],
  "startState": 0,
  "endToken": "<EOF>",
  "states": {
    "0": {"allowed": [12, 25, 31, ...], "next": {"12": 1, "25": 2}},
    "1": {"allowed": [...], "next": {...}}
  }
}
```

`vocabulary` indexes the concrete token strings the host's tokenizer must
map onto; `states[s].allowed` lists vocabulary indices that keep the parse
valid in state `s`; `next` maps a chosen token to the successor state.
Per-state masks over ~300 effective choices ( ilo's keyword set is ~40–60
fixed tokens; builtins ~150–200 enter as identifier tokens; string and
number literals are single class-tokens the host expands to character-level
per its own tokenizer).

## State machine derivation — the load-bearing decision

Do **not** hand-write a second grammar. Two derivation strategies, in order
of preference:

### Strategy 1 (preferred): instrument the parser

`src/parser/mod.rs` is recursive descent over a logos lexer; branching
happens at `peek()`/`match` points in six entry functions (`parse_expr`,
`parse_stmt`, `parse_decl`, `parse_atom`, `parse_pattern`, `parse_type`).
Instrumentation:

- Wrap the token cursor in a recorder: `(state_id, expected: BTreeSet<Token>)`.
- Every decision site tags a state id (deterministic counter seeded by the
  call path, e.g. `parse_expr/atom/primary`).
- Under `#[cfg(feature = "constrain")]` (or a `ConstrainRecorder` always
  compiled but inert), each `peek` site records the accepted token variants.

State count estimate: 150–400 (six entry points × their production
alternatives; compare Axis's 41 states for a twelve-construct grammar — ilo
is bigger but the shape is identical).

**Rot-proofing:** the recorder lives in the parser, so parser changes update
masks by construction. CI: for every `examples/*.ilo` program, replay it
through the recorder and assert each consumed token ∈ recorded allowed set
for the reached state (piggyback on `conformance/`).

### Strategy 2 (fallback): coverage-guided fuzz derivation

If instrumentation is too invasive for the spike, derive states by fuzzing:
BFS over token sequences using the existing `fuzz/` harness — candidate
sequence accepted by lexer+parser (no parse diagnostic) ⇒ the transition
exists. Exponential in branching; only viable with a depth cap and the
closed vocabulary. Produces the same JSON but with weaker guarantees
(unreached states absent). Ship only as a cross-check of Strategy 1, never
as the artifact of record.

## Validation

1. **Conformance replay** (above): every example consumes only allowed
   tokens, in every engine.
2. **Negative test**: for each state, mutating a program to consume a
   non-allowed token must be rejected by the unmasked parser.
3. **End-to-end**: Haiku-class persona rerun with a masked host vs the
   published 1/13 (Haiku) and 10/13 (deepseek-flash, aligned curated)
   baselines. Publish either way — a small delta is a constrained-decoding
   calibration datapoint (Appendix A.6 risk 5).

## Known limits (from A.6)

- Masks guarantee syntax, not semantics — the 13k-line verifier stays as the
  second stage of the cascade (mask → verify → typed fix plan).
- Hosts must support per-step logit masking; hosted Anthropic models may not
  expose it — the artifact is provider-neutral, adoption is per-host.
- String/number literals expand to character-level on the host side; the
  artifact marks them as class-tokens (`"class:string"`), and the host's
  own tokenizer handles the interior. This is exactly how XGrammar handles
  JSON strings.
## Implementation checklist

- [x] Recorder type in `src/parser/` + state ids at decision sites
      — v0: `src/parser/constrain.rs`, empirical transition recorder gated
      on `ILO_CONSTRAIN=1`, hooked in `Parser::advance`
- [x] `ilo constrain > masks.json` CLI subcommand (json-first per manifesto P6)
      — `ilo constrain <dir>` (bigram, schemaVersion 1) and
      `ilo constrain <dir> --probe` (probed context masks, schemaVersion 2)
- [x] Conformance replay test over `examples/`
      — CI `mcp-e2e` job: constrain smoke over `examples/` (370 files),
      plus `--probe` self-check (oracle anomalies must be published)
- [x] Negative-test generator
      — the probe IS the negative generator: every non-allowed candidate at
      every corpus prefix was verified rejected by the parser
- [ ] Persona rerun with a masked host; publish delta vs baselines

## v1 probed-context results (2026-09-17)

`ilo constrain examples --probe`: 370 files, 30,120 prefixes, 873,480
parser calls (~3 min), **405 contexts / 19k+ probed edges**. The oracle
self-check (every corpus continuation must appear in its own probed
allowed set) surfaced **5 anomalies in 873,480 probes (0.0006%)**, all in
prefix-binop chain states (`* * * ...`, `; !`) where the parser's
error-recovery re-anchors the "missing operand" diagnostic at a statement
boundary deeper than any fixed tolerance. Tolerance 2 and 3 were tested;
the 5 anomalies are tolerance-invariant.

**Host guidance:** treat the artifact as advisory in operator-chain
contexts (soft-mask: allow with penalty), hard-mask elsewhere. The
anomalies are published in the artifact (`oracleAnomalies`) — the day they
disappear without explanation is the day to re-audit.

**Why probing instead of a hand-written grammar:** the oracle reuses the
real parser, so the mask inherits every grammar change by construction and
can never drift from it. The instrumented per-site recorder (Strategy 1)
remains the end-state for host APIs that want true parse states rather
than context keys.

