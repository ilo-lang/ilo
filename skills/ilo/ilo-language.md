---
name: ilo-language
description: Use this when writing or reviewing .@ source (canonical extension; .ilo also accepted with deprecation warning). Prefix notation, type sigils, guards, match, pipes, records, Result.
---

# ilo language

Prefix-notation, strongly-typed, verified pre-run. Bodies `;`-separated or newline-indented. RC-managed; type checker enforces shape only.

## fn

`tot p:n q:n r:n>n;s=*p q;t=*s r;+s t`. No param parens. `>` returns, `;` separates, last expr returns. Zero-arg: `make-id()`.

Single-line: `f x:n>n;+x 1`. Brace-block: `f x:n>n { s=+x 1; *s s }` (same semantics, braces wrap whole body). Multi-line: `f x:n>n` then indented body, newline = `;` (PR #501 also normalises CRLF). Trailing `;` on header (`f x:n>n;\n  a=+x 1\n  *a 2`) is optional; both forms parse. Multi-step: bind intermediates then tail expr: `add-and-double x:n y:n>n;s=+x y;*s 2`. Early return: braceless guard `>=x 0 val` or `ret val`. Result unwrap mid-body: `v=call!;use v`.

## types

`n` num, `t` text, `b` bool, `_` nil/any. `L n` list, `M t n` map, `R n t` result, `O n` optional, `S a b c` sum (closed, runtime `t`), `F n t` fn-type. Named: `order`. Type vars: any letter except `n t b`. `?? x d` nil-coalesce; unwraps `O T` only. For `R T E` use `default-on-err r d`.

## spacing (CRITICAL)

**Every token needs whitespace separation.** ilo has no implicit concatenation or adjacency. A number immediately followed by a string (`90"A"`) is a parse error; use `90 "A"`. A closing paren followed by a number (`)2`) is a parse error; use `) 2`. This is the single most common model mistake.

## operators

Binary `+ - * / % < > <= >= = !=`, bool `& | !`, append `+=`. Nest `+*a b c`=`(a*b)+c`; outer binds inner LEFT. Atoms/nested-ops not calls; bind first: `r=fac -n 1;*n r`. No compound `<=a b`. Glued `-n` = neg literal; bare `0 -1` errs ILO-P001. **`??` precedence**: `+a ??d b`=`a + (d ?? b)`, NOT `(a??d)+b`. For `(a??d)+b` bind first (`x=a??d;+x b`) or wrap (`+(a??d) b`).

### `*/` `/*` `+-` `-+` — adjacent-prefix-pair trap (READ THIS)

`*/` is **NOT** a 3-arg compound multiply-then-divide. It is two **separate** prefix ops `*` then `/`, parsed by the standard "outer binds inner LEFT" rule. So:

```
*/a b c   -- parses as  (a/b)*c   ← b is the DIVISOR (2nd arg), not the 3rd
/*a b c   -- parses as  (a*b)/c   ← c is the divisor
+-a b c   -- parses as  (a-b)+c
-+a b c   -- parses as  (a+b)-c
```

This is the most-asked-about gotcha in agent feedback: `*/ sz 0.3 0` looks like "scale `sz` by 0.3, then divide by 0" but actually evaluates `(sz / 0.3) * 0` — and if the second arg is `0` you get a runtime divide-by-zero from the `/`, not from the trailing `0`. The runtime fires a `hint:` diagnostic naming the parse order for all four pairs (`*/`, `/*`, `+-`, `-+`) at prefix position.

To get **multiply-then-divide** `(a*b)/c` (the common percentage-scaling shape), pick one:

```
/*a b c        -- swap the prefix-pair order (terse)
r=*a b;/r c    -- bind the product, then divide (explicit)
```

Worked example — scale `sz` by 30% with explicit divisor:

```
-- DON'T:  */ sz 0.3 100   parses as (sz / 0.3) * 100 = sz * 333.33...
-- DO:     /*sz 0.3 100    parses as (sz * 0.3) / 100 = sz * 0.003
-- DO:     r=*sz 0.3;/r 100
```

## idents

`[a-z][a-z0-9]*(-[a-z0-9]+)*`, short (1-3 chars). No capitals/underscores except after `.` / `.?` for JSON keys (`r.URL`). Comments `-- to EOL`; `--x` is a comment (use `- -x 1`).

## guards & conditionals

Three distinct shapes. `cond expr` early return (`>=sp 1000 "gold"`); `cond{body}` runs body NO early return; `cond{a}{b}` value no early return. Ternary: `?h cond a b` (3-arg: bool cond, true-value, false-value — requires spaces between ALL operands: `?h >=x 90 "A" "B"`). `?h cond{...}` illegal. `!` negates all. **Nested ternaries supported**: `?h >=x 90 "A" ?h >=x 80 "B" "C"`. For 3+ branches, match is clearer: `?x{90:"A";80:"B";70:"C";_:"F"}` or guard chain. Bare comparison IS a guard; bind to return a bool: `r=>a b;r`.

## match

`?r{~v:v;^e:^+"failed: "e;_:"unknown"}`. Arms: `"lit":body`, `42:body`, `~v:body` ok-bind, `^e:body` err-bind, `_:body` else. Multi-token subj wraps: `?(e){…}`. Bare-call scrutinee also fine: `?safe-div a b{~v:str v;^e:e}` — known-arity fn followed by exactly its args then `{` parses as `?(safe-div a b){…}`, no rebind needed.

## results

`div a:n b:n>R n t;=b 0 ^"divide by zero";~/a b`. `!` auto-unwraps in `R`-fns. `!!` panics on `^e`/`nil`. `default-on-err r d` unwraps `R T E` to `T` with `d` on Err (Result `??`).

## contracts (prototype)

Optional `req` (precondition) and `ens` (postcondition) after return type, before `;`/body. `div a:n b:n>R n t req b!=0;=b 0 ^"divide by zero";~/a b`. `ens result>=0` stored, shown by `ilo explain`. Warning-only: ILO-W030 fires at call sites where the verifier can't find a matching preceding guard. Pattern-based (guards like `=b 0 ^"..."` satisfy `req b!=0`), not SMT.

## optional vs result

Two distinct types, two distinct unwraps. `O T` = maybe-value (`nil` or `T`), no error payload; unwrap with `?? x d`. `R T E` = ok-or-err with payload; unwrap with `~`/`^` match arms, `!`, `!!`, or `default-on-err r d`. Using `??` on `R T E` is ILO-T041; using `default-on-err` on `O T` is ILO-T040.

`O t`: `name = ?? name-opt "default"` — nil-coalesce, `O t -> t`.
`R t t`: `name = default-on-err r "fallback"`, or `?r{~v:v;^_:"fallback"}` — Result unwrap, `R t e -> t`.

## loops

`@x xs{body}` foreach, `@i 0..5{body}` range, `wh <i 10{...}` while. `brk`, `cnt`, `ret v` (returns from enclosing fn even inside a loop body; no sentinel needed). Tail user-fn calls trampoline (no stack growth); deep iter: `cd n:n>n;=n 0 0;cd -n 1`. Direct name, no `!`/`!!`.

## tail-call optimisation

Tail calls do not consume host-stack frames. A function that recurses in tail position runs to arbitrary depth — use tail-recursive accumulators for iteration beyond what `@` covers. No `loop` keyword by design. Tail position = last stmt of body, `ret` expr, an arm of a tail-position `?` match, body of a braceless guard. Peephole fires on direct user-fn name calls with no `!`/`!!`. Tree + VM trampoline today; JIT/AOT pending. Example: `count-down n:n>n;=n 0 0;count-down -n 1`.

## effects

Optional sigils after return type track side effects at verify time. No runtime cost.

`/http /fs /io /net /ml /time /rand` after the return type (and after `^effect_set` if present):

```
fetch url:t>R t t /http
save path:t data:t>R _ t /fs /http
pure-sum xs:L n>n;sum xs
```

No sigils = pure. Verifier rejects side-effectful calls in pure fns (ILO-W051, warning; `--strict` to fail). Declared must cover actual (transitive: calling a `/http` fn makes caller `/http`). Over-declaring safe. Tools (external calls) count as `/http`.

## pipes

`xs >> flt pos >> map sq` desugars left-to-right. Wrap `()` for non-last fns.

**Result-aware short-circuit (ILO-510).** When the left side returns `R T E` or `O T`, `>>` auto-unwraps: `~v` passes inner value to next stage; `^e` short-circuits and propagates out of enclosing fn. Non-Result values pass through unchanged. No explicit `!` needed per stage: `get url>>jpar>>jpth "name"` — if any stage returns `^e`, remaining stages skip and `^e` propagates. Enclosing fn must return `R`/`O`.

## lambdas

Parens: `map (x:n>n;+x 1) xs`. Captures tree-only; VM/JIT auto-fallback.

## multi-fn files

Non-last fns end with safe expr (op, index, match, literal, parens); last fn: anything.

## strings

`"text"` with `\n \t \" \\`. Multi-line `"""..."""`. Interp `"hi {name}"` => `fmt "hi {}" name`. Single-ident slots only. `{{`/`}}` escape inside interpolated strings. Bare `{}` still positional; don't mix `{ident}` + `{}` in one string.

## shadow tests

`test fn-name { ok fn-name arg... expected; err fn-name arg... expected-err }`. `ok` asserts return equals expected; `err` asserts `^expected-err`. Literal args only. Runs at `ilo check` time. Failure = `ILO-T050`; missing under `--strict` = `ILO-W020`. Complements `-- run:` / `-- out:` annotation tests.

## reserved names (DO NOT use as bindings)

**All 1-3 char lowercase identifiers are likely reserved builtins.** If you need a local variable, use 4+ chars: `total` not `tl`, `avg-v` not `av`, `count` not `ct`. Reserved: `at hd pi tl rd wr ct` (2-char) and `abs avg b64 cap cat cel chr cos del det dot env exp fft fld flr flt fmt frq get grp has hed inv len log lst lwr map max min mod now num opt ord pat pow pst put rdb rdl rep rev rgx rng rnd rou run sin slc spl srt str sum tan tau trm unq upr wra wrl zip` (3-char). Also avoid `avg` `tl` `len` `sum` `map` `cat` `str` — the model reaches for these most.

Fn/binding shadowing builtin/alias fires `ILO-P011`. 2-char safe; 4+ safe except `take drop mget mset flat range`; 3-char safe.

`e` `at hd pi tl rd wr ct` `abs avg cap cat cel chr cos det dot env exp fft fld flr flt fmt frq get grp has inv len log lsd lst lwr map max min mod now num ord pow pst rdb rdl rev rgx rng rnd rou run sin slc spl srt str sum tan tau trm unq upr wrl zip`

## cross-lang gotchas

No `&`, `&mut`, refs, lifetimes, ownership errors. Lifetime reasoning = wrong model.
