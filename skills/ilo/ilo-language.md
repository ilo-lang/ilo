---
name: ilo-language
description: Use this when writing or reviewing .@ source (canonical extension; .ilo also accepted with deprecation warning). Infix arithmetic, if/else, for/while, types, guards, match, pipes, records, Result.
---

# ilo language

Strongly-typed, verified pre-run. Bodies `;`-separated or newline-indented. RC-managed; type checker enforces shape only.

This branch (`compat/agent-natural`) is an A/B against `main` measuring whether **leading with the surface agents already reach for** reduces total token cost vs `main`'s prefix-canonical surface. Prefix forms keep parsing. Skill docs lead with the natural form.

## fn

`tot p:n q:n r:n>n;s=p*q;t=s*r;s+t`. No param parens. `>` returns, `;` separates, last expr returns. Zero-arg: `make-id()`.

Single-line: `f x:n>n;x+1`. Multi-line: `f x:n>n` then indented body, newline = `;`.

## types

`n` num, `t` text, `b` bool, `_` nil/any. `L n` list, `M t n` map, `R n t` result, `O n` optional, `S a b c` sum (closed, runtime `t`), `F n t` fn-type. Named: `order`. Type vars: any letter except `n t b`. `?? x d` nil-coalesce; unwraps `O T` only. For `R T E` use `default-on-err r d`.

## operators

Infix is canonical here; prefix still parses everywhere.

```
a + b * c           -- arithmetic, standard precedence
x >= 0 & x <= 100   -- comparison + boolean
n != 0              -- inequality
xs + [v]            -- list concat (also: xs += v rebinds w/ append)
```

Available: `+ - * / %`, comparison `= != > < >= <=`, boolean `& | !`, append `+=` (rebind shape: `xs = xs += v`). Infix follows standard precedence (`* /` > `+ -` > comparison > `&` > `|` > `??`). Function application binds tighter than every infix op: `f a + b` is `(f a) + b`.

**Nil-coalesce `??` is infix-only.** Never start a statement with `??`. Pattern: `name = name-opt ?? "default"`. Right-binds looser than every arithmetic/boolean op.

**Negative literals.** `-1` is a number literal. To subtract, use spaces: `a - b`. `a -b` glues `-b` as a negative literal (intended for `at xs -1` and `[-2, 1, 3]`).

Prefix-Polish forms (`+a b`, `*a b`, `>=a b`) keep parsing for the same reason `.ilo` keeps parsing: ecosystem code that was generated in prefix style runs unchanged. Reach for them when you want to nest without parens (`+*a b c` = `(a*b)+c`).

## idents

`[a-z][a-z0-9]*(-[a-z0-9]+)*`, short (1-3 chars). No capitals/underscores except after `.` / `.?` for JSON keys (`r.URL`). Comments `-- to EOL`; `--x` is a comment (use `- -x 1`).

## conditionals

`if cond { a } else { b }` is the canonical conditional. Value-producing when used as an expression (both branches required). At statement position, `else` is optional and absent `else` yields `nil` (handy as a guard).

```
v = if x >= 0 { x } else { -x }                    -- expression form, both branches required
if found { log "hit" }                              -- statement form, no else needed
if x > 0 { ret x }                                  -- early return inside a fn body
status = if score >= 1000 { "gold" } else { if score >= 500 { "silver" } else { "bronze" } }
```

No `else if` chaining sugar — nest `if` inside the `else` block as shown. The braceless prefix-guard form (`>=sp 1000 "gold";>=sp 500 "silver";"bronze"`) still parses but the skill docs no longer lead with it.

## match

`?r{~v: v; ^e: ^"failed: " + e; _: "unknown"}`. Arms: `"lit": body`, `42: body`, `~v: body` ok-bind, `^e: body` err-bind, `_: body` else. Multi-token subject wraps: `?(e){…}`. Bare-call scrutinee parses: `?safe-div a b{~v: str v; ^e: e}`.

**Block arm bodies** accept `pat: { stmt; stmt; expr }` — last statement's expression is the arm value. Single-expression arms still work unchanged.

```
?r {
  ~rows: { n = len rows; total = sum rows; total / n }
  ^e:    log e
}
```

## results

`div a:n b:n>R n t; if b = 0 { ^"divide by zero" } else { ~a/b }`. `!` auto-unwraps in `R`-fns. `!!` panics on `^e`/`nil`. `default-on-err r d` unwraps `R T E` to `T` with `d` on Err (Result `??`).

## optional vs result

Two types, two unwraps. `O T` (`nil` or `T`): no error payload, unwrap with `name ?? "default"`. `R T E` (ok-or-err with payload): `~`/`^` match arms, `!`, `!!`, or `default-on-err r d`. Using `??` on `R T E` is ILO-T041; using `default-on-err` on `O T` is ILO-T040.

## loops

```
for x in xs { ... }      -- foreach
for i in 0..n { ... }    -- range
while cond { ... }       -- while
```

`brk`, `cnt`, `ret v` (returns from enclosing fn even inside loop body). Tail user-fn calls trampoline (deep iteration without stack growth): `cd n:n>n; if n = 0 { 0 } else { cd n - 1 }`.

The shorthand forms still parse: `@x xs{...}` aliases `for x in xs{...}`, `@i 0..n{...}` aliases `for i in 0..n{...}`, `wh cond{...}` aliases `while cond{...}`. Use whichever reads clearer; cost difference is ~2 tokens per loop header.

## tail-call optimisation

Tail calls do not consume host-stack frames. A function that recurses in tail position runs to arbitrary depth. Tail position = last stmt of body, `ret` expr, an arm of a tail-position `?` match, body of a braceless guard or `if` branch. Peephole fires on direct user-fn name calls with no `!`/`!!`. Tree + VM trampoline today; JIT/AOT pending.

## pipes

`xs >> flt pos >> map sq` desugars left-to-right. Wrap `()` for non-last fns.

## lambdas

Parens: `map (x:n>n; x+1) xs`. Captures tree-only; VM/JIT auto-fallback.

## multi-fn files

Non-last fns end with safe expr (op, index, match, literal, parens); last fn: anything.

## strings

`"text"` with `\n \t \" \\`. Multi-line `"""..."""`. Interp `"hi {name}"` => `fmt "hi {}" name`. Single-ident slots only. `{{`/`}}` escape inside interpolated strings. Bare `{}` still positional; don't mix `{ident}` + `{}` in one string.

## reserved names

Fn/binding shadowing a builtin/alias fires `ILO-P011`. Control-flow keywords (`if`, `else`, `for`, `while`, `fn`, `def`, `let`, `var`, `const`, `return`, `true`, `false`, `nil`, `type`, `tool`, `use`) are also rejected at binding/function-name positions. 2-char names safe; 4+ safe except `take drop mget mset flat range`; most 3-char safe.

Reserved short builtins: `e` `at hd pi tl rd wr ct` `abs avg cap cat cel chr cos det dot env exp fft fld flr flt fmt frq get grp has inv len log lsd lst lwr map max min mod now num ord pow pst rdb rdl rev rgx rng rnd rou run sin slc spl srt str sum tan tau trm unq upr wrl zip`

## cross-lang gotchas

No `&`, `&mut`, refs, lifetimes, ownership errors. Lifetime reasoning = wrong model. `tup.0` doesn't work (no tuple type — use `at pair 0`).
