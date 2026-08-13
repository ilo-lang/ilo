---
name: ilo-language
description: Use this when writing or reviewing .@ source (canonical extension; .ilo also accepted with deprecation warning). Prefix notation, type sigils, guards, match, pipes, records, Result.
---

# ilo language

Prefix-notation, strongly-typed, verified pre-run. Bodies `;`-separated or newline-indented. RC-managed; type checker enforces shape only.

## fn

`tot p:n q:n r:n>n;s=*p q;t=*s r;+s t`. No param parens. `>` returns, `;` separates, last expr returns. Zero-arg: `make-id()`.

Single-line `f x:n>n;+x 1`; brace-block `f x:n>n { ... }`; multi-line = header then indented body (newline = `;`, trailing header `;` optional). Bind intermediates then tail expr. Early return: braceless guard `>=x 0 val` or `ret val`. Result unwrap mid-body: `v=call!;use v`.

## types

`n` num, `t` text, `b` bool, `_` nil/any. `L n` list, `M t n` map, `R n t` result, `O n` optional, `S a b c` sum (closed, runtime `t`), `F n t` fn-type. Named: `order`. Type vars: any letter except `n t b`. `?? x d` nil-coalesce; unwraps `O T` only. For `R T E` use `default-on-err r d`.

## spacing

Adjacent tokens parse (ILO-537/544): `90"A"` ok, `f(x)2` extends the call (`f(x)2` ≡ `f(x) 2` ≡ `f x 2`). Prefer spaces for readability.

## operators

Binary `+ - * / % < > <= >= = !=`, bool `& | !`, append `+=`. Nest `+*a b c`=`(a*b)+c`; outer binds inner LEFT. Atoms/nested-ops not calls; bind first: `r=fac -n 1;*n r`. No compound `<=a b`. Glued `-n` = neg literal; bare `0 -1` errs ILO-P001. **`??` precedence**: `+a ??d b`=`a + (d ?? b)`, NOT `(a??d)+b`. For `(a??d)+b` bind first (`x=a??d;+x b`) or wrap (`+(a??d) b`).

### `*/` `/*` `+-` `-+` — adjacent-prefix-pair trap

NOT 3-arg compounds — two separate prefix ops, outer binds inner LEFT:

```
*/a b c = (a/b)*c   /*a b c = (a*b)/c   +-a b c = (a-b)+c   -+a b c = (a+b)-c
```

So `*/ sz 0.3 0` is `(sz/0.3)*0` — div-by-zero comes from the `/`, not the trailing 0 (runtime hints the parse order). For multiply-then-divide `(a*b)/c`: `/*a b c`, or bind first `r=*a b;/r c`.

## idents

`[a-z][a-z0-9]*(-[a-z0-9]+)*`, short (1-3 chars). No capitals/underscores except after `.` / `.?` for JSON keys (`r.URL`). Comments `-- to EOL`; `--x` is a comment (use `- -x 1`).

## guards & conditionals

`cond expr` early-returns (`>=sp 1000 "gold"`); `cond{body}` no early return; `cond{a}{b}` value form. Ternary `?h cond a b`, spaces between ALL operands; nests: `?h >=x 90 "A" ?h >=x 80 "B" "C"`; `?h cond{...}` illegal; 3+ branches read better as match `?x{90:"A";_:"F"}`. `!` negates. Bare comparison IS a guard; bind for a bool: `r=>a b;r`.

## match

`?r{~v:v;^e:^+"failed: "e;_:"unknown"}`. Arms: `"lit":body`, `42:body`, `~v:body` ok-bind, `^e:body` err-bind, `_:body` else. Multi-token subj wraps: `?(e){…}`. Bare-call scrutinee also fine: `?safe-div a b{~v:str v;^e:e}` — known-arity fn followed by exactly its args then `{` parses as `?(safe-div a b){…}`, no rebind needed.

## results

`div a:n b:n>R n t;=b 0 ^"divide by zero";~/a b`. `!` auto-unwraps in `R`-fns. `!!` panics on `^e`/`nil`. `default-on-err r d` unwraps `R T E` to `T` with `d` on Err (Result `??`).

## contracts (prototype)

Optional `req`/`ens` after return type: `div a:n b:n>R n t req b!=0;...`. Warning-only ILO-W030 at call sites lacking a matching guard; pattern-based, not SMT.

## optional vs result

`O T` = nil-or-value, unwrap `?? x d`. `R T E` = ok-or-err with payload, unwrap `~`/`^` arms, `!`, `!!`, or `default-on-err r d`. `??` on `R` = ILO-T041; `default-on-err` on `O` = ILO-T040.

`O t`: `name = ?? name-opt "default"` — nil-coalesce, `O t -> t`.
`R t t`: `name = default-on-err r "fallback"`, or `?r{~v:v;^_:"fallback"}` — Result unwrap, `R t e -> t`.

## loops

`@x xs{body}` foreach, `@i 0..5{body}` range, `wh <i 10{...}` while. `brk`, `cnt`, `ret v` (returns from enclosing fn even inside a loop body; no sentinel needed). Tail user-fn calls trampoline (no stack growth); deep iter: `cd n:n>n;=n 0 0;cd -n 1`. Direct name, no `!`/`!!`.

## tail-call optimisation

Tail calls don't consume stack — tail recursion runs to arbitrary depth (no `loop` keyword by design). Tail position = last body stmt, `ret` expr, tail-`?` arm, braceless-guard body. Direct user-fn calls only, no `!`/`!!`.

## effects

Optional sigils `/http /fs /io /net /ml /time /rand` after the return type: `fetch url:t>R t t /http`. No sigils = pure; side-effectful calls in pure fns fire ILO-W051 (warning; `--strict` fails). Declared must cover actual, transitively. Over-declaring safe. Tools count as `/http`.

## pipes

`xs >> flt pos >> map sq` desugars left-to-right. Wrap `()` for non-last fns.

Result-aware: when a stage returns `R`/`O`, `>>` auto-unwraps `~v` into the next stage and short-circuits `^e` out of the enclosing fn (which must return `R`/`O`): `get url>>jpar>>jpth "name"`.

## lambdas

Parens: `map (x:n>n;+x 1) xs`. Captures tree-only; VM/JIT auto-fallback.

## multi-fn files

Non-last fns end with safe expr (op, index, match, literal, parens); last fn: anything.

## script mode

Bare top-level statements auto-wrap into a synthetic `main>_;`. `prnt +2 2` alone prints 4. With decls, each statement on its OWN unindented line; glued to a decl's line it joins that body (self-call → ILO-V500). Explicit `main` + bare stmts = ILO-P104.

## strings

`"text"` with `\n \t \" \\`. Multi-line `"""..."""`. Interp `"hi {name}"` => `fmt "hi {}" name`. Single-ident slots only. `{{`/`}}` escape inside interpolated strings. Bare `{}` still positional; don't mix `{ident}` + `{}` in one string.

## shadow tests

`test fn-name { ok fn-name arg... expected; err fn-name arg... expected-err }`. `ok` asserts return equals expected; `err` asserts `^expected-err`. Literal args only. Runs at `ilo check` time. Failure = `ILO-T050`; missing under `--strict` = `ILO-W020`. Complements `-- run:` / `-- out:` annotation tests.

## reserved names (DO NOT use as bindings)

**All 1-3 char lowercase identifiers are likely reserved builtins.** If you need a local variable, use 4+ chars: `total` not `tl`, `avg-v` not `av`, `count` not `ct`. Reserved: `at hd pi tl rd wr ct` and `abs avg b64 cap cat cel chr cos del det dot env exp fft fld flr flt fmt frq get grp has hed inv len log lst lwr map max min mod now num opt ord pat pow pst put rdb rdl rep rev rgx rng rnd rou run sin slc spl srt str sum tan tau trm unq upr wra wrl zip`.

Shadowing a builtin/alias fires `ILO-P011`. 4+ chars safe except `take drop mget mset flat range`.

## cross-lang gotchas

No `&`, `&mut`, refs, lifetimes, ownership errors. Lifetime reasoning = wrong model.
