# ilo vs Python: The Token-Density Problem

Updated 2026-08-07. The 2026-08-03 version of this doc concluded "not
publishable". That verdict is superseded: the 2026-08-05 session found and
removed the structural tax that was drowning the density signal, and the
benchmark now flips to ilo. The old analysis is kept at the bottom because
its diagnosis was half right and the half it got wrong is instructive.

## The claim

The ilo manifesto (P1) claims the language is more token-conservative than
conventional languages. The closed-loop benchmark (ILO-364,
`scripts/closed-loop-bench.py`) measures the full agent loop against
Python — generation + diagnostics + repair retries, Sonnet, retry cap 5,
5 tasks, N=3 runs per config.

## The result (2026-08-05, clean A/B)

Same harness, same prompt, same model on both sides; only the ilo binary
changed (PR #785: script mode + ILO-V500, plus #786: ILO-R600).

|                        | before (main @ da2733fc) | after (script-mode binary) |
|------------------------|--------------------------|----------------------------|
| total gen tokens (N=3) | ilo 3587 vs py 2335 (**+53.6%**) | ilo 2193 vs py 4447 (**-50.7%**) |
| per-run deltas         | +35.5 / +26.3 / +132.6   | **-75.5 / -45.4 / -22.7**  |
| ilo success            | 4/15                     | **11/15** (py 15/15)       |
| entry-point diagnostics| P102 ×19, P011 ×6        | **0**                      |
| dominant failure       | entry-point ceremony     | ILO-T0* ×24 (real type friction) |

Runs: `closed-loop-2026-08-05-19{3105,3416,3923}.json` (after) vs
`-18{4131,4340,4509}.json` (before), in the `ilo-feat-ilo-bench` worktree's
`bench/`. Per-attempt traces (`attempt_trace`, ILO-518) are in every JSON —
each claim below is checkable against a specific generation.

Caveats, stated up front:

- Python's totals swing wildly between batches (its run-1 total was 1818
  in one batch, 245 in another), so the **-50.7% magnitude is noisy. The
  direction is the solid part**: ilo lower in 3/3 runs, success nearly
  tripled, and the diagnostic census moved from 25 entry-point errors to
  zero.
- N=3 on 5 tasks. Do not quote a headline percentage without the range.
- ilo still fails 4/15 to Python's 0/15. Density claim holds; a
  reliability claim does not (yet — see frontier below).

## What actually flipped it

The Aug 3 doc blamed parser polish (paren spacing, reserved-name hints).
Instrumented traces showed the real bottleneck was structural: **the
entry-point tax**. Models write the Python shape — define a function, then
call it at top level:

```
tri n:n>n;/(*n +n 1) 2
prnt tri 10
```

Pre-fix ilo rejected exactly that shape, and the diagnostics
(ILO-P011 "prnt is a builtin and cannot be used as a function name",
ILO-P102's `main>_;` hint) could not steer the model out: P102 fired 19
times in one N=3 batch while the model failed 11/15 tasks. Being told the
answer wasn't enough; the tax had to go.

Three changes, all merged 2026-08-05:

1. **Script mode (ILO-439, PR #785).** Bare top-level statements wrap into
   a synthetic `main>_;`. The decl-plus-trailing-call shape above just
   runs. Deliberate inversion of the original ticket's "reject mixed"
   rule, on trace evidence.
2. **ILO-V500 unconditional-recursion (PR #785).** A same-line trailing
   call joins the function's own body and self-recurses; tail-call
   trampolining turned that into a silent 20s timeout per attempt.
   Now a verify-time error naming both fixes. Fired 12× in the after-runs,
   each one converting a timeout into an instant retry signal.
3. **ILO-R600 CLI arg type guard (ILO-517, PR #786).** Text bound to a
   numeric param used to give NaN at rc=0 (the JIT echoed the raw string).
   The repair loop's whole error signal was the string "NaN" and the model
   rewrote arithmetic instead of the entry point. Now rc=1 with the param
   and value named.

Also load-bearing: harness instrumentation (ILO-518 — per-attempt code +
stderr in the results JSON, timestamped filenames). Without it the failures
were unattributable token counts; with it, every wrong hypothesis died in
one trace read. Two of this doc's own earlier claims were killed that way.

## Where the density comes from

Unchanged from Aug 3 and still true — when ilo succeeds, it is denser:
winning-attempt sizes like 38 tok vs Python's 36 on simple-function, and
2-3.4× advantages on tool-interaction / data-transform in the earlier
batches. No `def`/`return` boilerplate, prefix ops, compact builtins,
`;`-separated statements. The point of the Aug 5 work is that this
advantage was always there; it was being spent on retries.

## The remaining frontier (in leverage order)

1. **Fn-body slurp — ILO-533.** A multi-statement body's tail expression
   still eats the next unindented line as call args
   (`quad x:n>n;a=double x;double a` + newline + `prnt quad 7` →
   T006 "arity mismatch" pointing inside `quad`). ILO-T006 was 18 of 33
   type errors in the after-runs, including on visibly correct programs.
   The sibling bug in script-statement collection was fixed in #785
   (`script_stmt_boundary`); the fn-body variant needs the same stop
   applied to top-level body parsing.
2. **ILO-W051 on hello-world — ILO-531.** `main>_;prnt x` warns
   (undeclared `/io` effect) on stderr of a *successful* run; any loop
   that feeds stderr back as an error signal carries a distractor.
3. **Repair-loop memory — ILO-530.** The harness resends only the latest
   error; models re-emit byte-identical failures (observed twice).
4. **Variance.** N=5+ runs before quoting numbers anywhere public.

## Publishability

The honest claim today: *on this 5-task closed-loop benchmark, ilo
generates ~half the tokens Python does (direction stable across runs,
magnitude noisy) and fails 4/15 tasks to Python's 0.* With ILO-533 fixed
and N=5, the reliability gap should close enough for the blog-post
version. The Aug 3 estimate of "1-2 days of focused work, not a research
breakthrough" was right in spirit and wrong in target: it was one day, and
the work was removing ceremony, not polishing diagnostics.

---

## Appendix: the 2026-08-03 analysis (superseded)

Kept for the record. Its "density is real but narrow" finding held. Its
"what would flip the result" list missed the actual lever: it proposed
paren-spacing fixes, reserved-name repair hints, and prefix-binop hints —
all diagnostic polish — while the binding constraint was that the natural
program shape models emit was illegal in the language. The three tasks it
recorded as hard failures (pipeline-report, text-analysis,
grade-calculator) came from a 10-task N=1 config that later sessions
dropped to the 5-task N=3 set; its specific numbers are not comparable to
the tables above.

Original headline: ilo 5 wins / Python 2 / ilo 3 total failures at
800-1000 wasted tokens each; "on working tasks only, roughly tied";
verdict "not publishable". Root causes it identified that remain live:
reserved-name collisions (`avg`, `tl` — ILO-P011) and prefix-binop
misparse (`*r ss i`) — both now subsumed under the ILO-533 /
type-friction frontier rather than being the headline story.
