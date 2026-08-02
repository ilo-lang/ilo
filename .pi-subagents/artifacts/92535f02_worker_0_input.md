# Task for worker

You are a delegated subagent running from a fork of the parent session. Treat the inherited conversation as reference-only context, not a live thread to continue. Do not continue or answer prior messages as if they are waiting for a reply. Your sole job is to execute the task below and return a focused result for that task using your tools.

Task:
# Feature: Pipeline Error Short-Circuit for `>>`

Make the pipe operator `>>` Result-aware. When the left side produces `^e` (Err), skip the right side and propagate `^e` through the pipe. Non-Result values pass through unchanged.

## Recon (already gathered from this repo)
- `maybe_pipe` at line 3600 in `src/parser/mod.rs` handles `>>`
- Pipes fully desugar to `Expr::Call` at parse time — no `Expr::Pipe` variant exists
- `Expr::Call` already has an `unwrap: UnwrapMode` field (line 412 of `src/ast/mod.rs`): `None`, `Propagate` (`!`), `Panic` (`!!`)
- Currently pipe-desugared calls use `unwrap: None` unless the agent writes `>> func!`
- `UnwrapMode::Propagate` already auto-unwraps and propagates `^e` — exactly the semantics we want

## Implementation
The cleanest path: in `maybe_pipe`, set the pipe-desugared call's `unwrap` to `Propagate` instead of `None`. This makes `xs >> f` behave like `xs >> f!` — Result-aware short-circuit. If this breaks existing non-Result pipe tests (where piping a non-Result through a non-Result function), then narrow it: only set `Propagate` when appropriate. But since `Propagate` on a non-Result value is a no-op (there's no `^e` to propagate), it should be safe.

Check: does `UnwrapMode::Propagate` cause a type error if the value isn't a Result? Read `src/verify.rs` to see how `Propagate` is type-checked. If `!` requires a `R` type, then naively setting `Propagate` on all pipes would break non-Result pipes. In that case, add a new `UnwrapMode::PipePropagate` that the verifier treats as conditional (propagate only if Result, pass-through if not).

## Steps
1. Read `src/parser/mod.rs` lines 3540-3650 (`maybe_pipe` + surrounding)
2. Read `src/ast/mod.rs` around line 412 (`UnwrapMode`) and line 442+ (`Expr::Call`)
3. Read `src/verify.rs` — grep for `Propagate` and `PipeOp` to see how unwraps are type-checked
4. Read `src/interpreter/mod.rs` and `src/vm/mod.rs` — grep for `Propagate` to see runtime behavior
5. Implement the change. Verify the approach with the type checker's rules for `Propagate` first — that determines whether you need a new UnwrapMode variant or can reuse `Propagate`.
6. Add tests in `tests/` for: basic short-circuit (`url >> get >> jpar` where get fails returns ^e), multi-stage pipe with error in middle, non-Result pipe (unchanged behavior), mixing Result and non-Result
7. Run `cargo test` — full suite must pass
8. Commit: `feat(pipes): >> short-circuits on ^e (Result-aware pipe operator)`

---
**Output:**
Write your findings to exactly this path: /tmp/ilo-feat1-pipe.md
This path is authoritative for this run.
Ignore any other output filename or output path mentioned elsewhere, including output destinations in the base agent prompt, system prompt, or task instructions.

## Acceptance Contract
Acceptance level: checked
Completion is not accepted from prose alone. End with a structured acceptance report.

Criteria:
- criterion-1: Implement the requested change without widening scope

Required evidence: changed-files, tests-added, commands-run, residual-risks, no-staged-files

Finish with a fenced JSON block tagged `acceptance-report` in this shape:
Use empty arrays when no items apply; array fields contain strings unless object entries are shown.
`criteriaSatisfied[].status` must be exactly one of: satisfied, not-satisfied, not-applicable.
`commandsRun[].result` must be exactly one of: passed, failed, not-run.
`manualNotes` and `notes` are optional strings; an empty string means no note and does not satisfy `manual-notes` evidence.
```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "specific proof"
    }
  ],
  "changedFiles": [
    "src/file.ts"
  ],
  "testsAddedOrUpdated": [
    "test/file.test.ts"
  ],
  "commandsRun": [
    {
      "command": "command",
      "result": "passed",
      "summary": "short result"
    }
  ],
  "validationOutput": [
    "validation output or concise summary"
  ],
  "residualRisks": [
    "none"
  ],
  "noStagedFiles": true,
  "diffSummary": "short description of the diff",
  "reviewFindings": [
    "blocker: file.ts:12 - issue found, or no blockers"
  ],
  "manualNotes": "anything else the parent should know"
}
```