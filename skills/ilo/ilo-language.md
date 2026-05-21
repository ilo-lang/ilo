---
name: ilo-language
description: Use this when writing or reviewing .ilo source. Prefix notation, type sigils, guards, match, pipes, Results, loops, lambdas.
---

# ilo language

Prefix-notation, strongly-typed, verified pre-run. Bodies `;`-separated or newline-indented. RC-managed; type checker enforces shape only.

## fn

`tot p:n q:n r:n>n;s=*p q;t=*s r;+s t`. No param parens. `>` returns, `;` separates, last expr returns. Zero-arg: `make-id()`.

## types

`n` num, `t` text, `b` bool, `_` nil/any. `L n` list, `M t n` map, `R n t` result, `O n` optional, `S a b c` sum (closed, runtime `t`), `F n t` fn-type. Named: `order`. Type vars: any letter except `n t b`. `?? x d` nil-coalesce; unwraps `O T` only. For `R T E` use `default-on-err r d`.

## operators

Binary `+ - * / % < > <= >= = !=`, bool `& | !`, append `+=`. Nest `+*a b c`=`(a*b)+c`; outer binds inner LEFT. Atoms/nested-ops not calls; bind first: `r=fac -n 1;*n r`. No compound `<=a b`. Glued `-n` = neg literal; bare `0 -1` errs ILO-P001.

## idents

`[a-z][a-z0-9]*(-[a-z0-9]+)*`, short (1-3 chars). No capitals/underscores except after `.` / `.?` for JSON keys (`r.URL`). Comments `-- to EOL`; `--x` is a comment (use `- -x 1`).

## guards

Flat early returns: `cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze"`. Braceless `cond expr` cheaper than `cond{expr}`. Bare comparison IS a guard; bind to return: `r=>a b;r`.

## match

`?r{~v:v;^e:^+"failed: "e;_:"unknown"}`. Arms: `"lit":body`, `42:body`, `~v:body` ok-bind, `^e:body` err-bind, `_:body` else. Multi-token subj wraps: `?(e){…}`.

## results

`div a:n b:n>R n t;=b 0 ^"divide by zero";~/a b`. `!` auto-unwraps in `R`-fns. `!!` panics on `^e`/`nil`. `default-on-err r d` unwraps `R T E` to `T` with `d` on Err (Result `??`).

## optional vs result

Two distinct types, two distinct unwraps. `O T` = maybe-value (`nil` or `T`), no error payload; unwrap with `?? x d`. `R T E` = ok-or-err with payload; unwrap with `~`/`^` match arms, `!`, `!!`, or `default-on-err r d`. Using `??` on `R T E` is ILO-T041; using `default-on-err` on `O T` is ILO-T040.

`O t`: `name = ?? name-opt "default"` — nil-coalesce, `O t -> t`.
`R t t`: `name = default-on-err r "fallback"`, or `?r{~v:v;^_:"fallback"}` — Result unwrap, `R t e -> t`.

## loops

`@x xs{body}` foreach, `@i 0..5{body}` range, `wh <i 10{...}` while. `brk`, `cnt`, `ret v`. Tail user-fn calls trampoline (no stack growth); deep iter: `cd n:n>n;=n 0 0;cd -n 1`. Direct name, no `!`/`!!`.

## pipes

`xs >> flt pos >> map sq` desugars left-to-right. Wrap `()` for non-last fns.

## lambdas

Parens: `map (x:n>n;+x 1) xs`. Captures tree-only; VM/JIT auto-fallback.

## multi-fn files

Non-last fns end with safe expr (op, index, match, literal, parens); last fn: anything.

## strings

`"text"` with `\n \t \" \\`. Multi-line `"""..."""`. Interp `"{x}"`.

## reserved names

Fn/binding shadowing builtin/alias fires `ILO-P011`. 2-char safe; 4+ safe except `take drop mget mset flat range`; 3-char safe.

`e` `at hd pi tl rd wr ct` `abs avg cap cat cel chr cos det dot env exp fft fld flr flt fmt frq get grp has inv len log lsd lst lwr map max min mod now num ord pow pst rdb rdl rev rgx rng rnd rou run sin slc spl srt str sum tan tau trm unq upr wrl zip`

## cross-lang gotchas

No `&`, `&mut`, refs, lifetimes, ownership errors. Lifetime reasoning = wrong model.
