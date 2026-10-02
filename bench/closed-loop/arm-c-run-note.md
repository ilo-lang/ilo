# Gold-pack Arm C run note (template)

British English. No live spend in the change that added this file.
Fill the blank tip when the live is scheduled. Pass bars are the NX-01v2
text; do not tighten them in the live note.

## Pins

| Item | Value |
|------|-------|
| Base tip (task-modules harness) | `7b15b28dac876f977d5280387e6ae09c5bb56b8f` |
| Pack tip | `git rev-parse HEAD` on this branch at schedule time |
| ilo CalVer | 26.5.0 (`ilo --version` on that tip) |
| Card | `bench/closed-loop/context/one-shape-conditional.md` |
| Card sha256 | `e3495fad4dd014e19e3ca34b2a27b1320fc570ec1859f2cbe46aedd63f12f98a` |
| Canonical conditional | `?h cond then else` |
| Nested else | `(?h cond then else)` |
| HOF predicates | `>b` comparison expressions |
| Task set | `artefact-exp07` (four tasks; bank not widened) |
| Rails | shape-hint + header-recovery + fold-meta-stdout (frozen; no new header rail) |

The card is not a loaded module. Each `ilo-excerpt-*-oneshape.md` copies
the two shape lines from the card byte-for-byte.

## Invoke

Offline (no key):

```bash
python3 scripts/closed-loop-bench.py --dry-run \
  --task-set artefact-exp07 --context task-oneshape
```

Arm B stays `--context task-modules` (multi-shape excerpts). Arm C is
`--context task-oneshape`, which reads `oneshape_modules` on the same
four tasks.

Live, only after this offline note is green, and only if a parent
authorises the key. Do not run it from the pack PR:

```bash
python3 scripts/closed-loop-bench.py \
  --provider deepseek --model deepseek-chat \
  --task-set artefact-exp07 \
  --context task-oneshape \
  --retry-cap 2 \
  --repair-shape-hint --header-recovery --fold-meta-stdout \
  --python --lang2-docs bench/closed-loop/context/python-control-docs.md \
  --output-dir bench/goldpack-v1-live-arm-c-YYYY-MM-DD/
```

## Offline context chars (harness `len`, this pack)

| task | Arm B `task-modules` | Arm C `task-oneshape` |
|------|---------------------:|----------------------:|
| schedule-window | 659 | 837 |
| typed-rollup | 719 | 845 |
| record-normalize | 699 | 965 |
| csv-sales-summary | 777 | 985 |

Same band as Arm B (under 1k chars). The extra characters are the
canonical `?h` lines, the non-canonical mark, and the `@` loop sentence
on the two tasks that red-failed ILO-P009.

## Pass bar (NX-01v2, Arm C only)

All of:

1. Treatment landed: Arm C mean inp ≤ half of Arm A mean inp on the same
   tip, and the loaded docs teach exactly one conditional shape.
2. Success: Arm C success ≥ 3/4.
3. Sum clause: Arm C mean intent→green sum ≤ Arm P mean intent→green sum
   on the same set, or (if the five-term sum is not instrumented) Arm C
   mean inp on working cells ≤ Arm P mean inp and success ≥ Arm P
   success. Label which surrogate was used. Do not call a surrogate Pass
   a Gate G Pass.

Fail if the treatment landed and success stays ≤ 2/4, or success ≥ 3/4
but the sum (or labelled surrogate) still loses to Python.
Inconclusive if inp does not halve versus Arm A, or the loaded docs
still teach more than one conditional.

## Soft edge (proposed, not a compiler change)

Arm B live 2026-10-02: ILO-P009 on `@` is the only code shared by two
reds (record-normalize attempt 2, typed-rollup attempt 2). The one-shape
excerpts for those tasks name the loop form. ILO-P007 was typed only.
ILO-P023 did not appear. No parser or verifier change in this pack.

## Goldens

`bench/closed-loop/goldens-exp07/` is unchanged. `ilo check` and `ilo run`
on 26.5.0:

| golden | conditional it uses | run |
|--------|---------------------|-----|
| schedule-window | `?h` (canonical) | green |
| csv-sales-summary | match `?v{~x:x;^_:0}` | green |
| record-normalize | brace `exists{cnt}` | green |
| typed-rollup | braceless guards | green |

A canonical `?h` rewrite of the three non-canonical goldens also matches
the recorded stdout (checked offline, not committed). Rewrite them in a
separate PR if the pin should become one-shape. Do not edit them inside
a live night.
