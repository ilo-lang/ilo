---
name: ilo-language
description: Use this when writing or reviewing .ilo source. Prefix notation, type sigils, guards, match, pipes, records, Result.
---

# ilo language

Prefix-notation, strongly-typed, verified pre-run. Bodies single-line, `;`-separated. No borrow checker, no lifetimes. RC-managed; type checker enforces shape only.

## Fn syntax

`tot p:n q:n r:n>n;s=*p q;t=*s r;+s t`. No param parens. `>` returns. `;` separates. Last expr returns. Zero-arg: `make-id()`.

## Types

`n` num, `t` text, `b` bool, `_` nil/any. `L n` list, `M t n` map, `R n t` result, `O n` optional, `S a b c` sum (closed, runtime `t`), `F n t` fn-type. Named: `order`. Type vars: any letter except `n t b`. `?? x d` nil-coalesce; unwraps `O T`.

## Operators (prefix)

Binary `+ - * / % < > <= >= = !=`, bool `& | !`, append `+=`. Nest: `+*a b c` = `(a*b)+c`; outer binds inner LEFT. Take atoms/nested-ops, NOT calls; bind first: `r=fac -n 1;*n r`. No compound: `<=a b`, not `=<a b`.

## Idents + comments

`[a-z][a-z0-9]*(-[a-z0-9]+)*`, short (1-3 chars). No capitals/underscores except after `.` / `.?` for JSON keys (`r.URL`). Comments `-- to EOL`; `--x` is a comment (use `- -x 1`).

## Guards

Flat early returns at statement: `cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze"`. Braceless `cond expr` cheaper than `cond{expr}`. Bare comparison at statement IS a guard; bind to return otherwise: `r=>a b;r`.

## Match

`?r{~v:v;^e:^+"failed: "e;_:"unknown"}`. Arms: `"lit":body`, `42:body`, `~v:body` ok-bind, `^e:body` err-bind, `_:body` else.

## Results

`div a:n b:n>R n t;=b 0 ^"divide by zero";~/a b`. `!` auto-unwraps in `R`-fns (`d=get! url`). `!!` panic-unwraps on `^e`/`nil`.

## Loops

`@x xs{body}` foreach, `@i 0..5{body}` range half-open, `wh <i 10{...}` while. `brk`, `cnt`, `ret v`.

## Pipes

`xs >> flt pos >> map sq` desugars left-to-right. Wrap `()` for non-last fns.

## Records

`type point{x:n;y:n}`, `p=point x:10 y:20`. `p.x`, `{x;y}=p`, `p with x:30`, `p.?missing` safe-nav.

## Lambdas

Parenthesised: `map (x:n>n;+x 1) xs`. Captures tree-only; VM/JIT/AOT auto-fall-back.

## Multi-fn files

Non-last fns end with safe expr (op, index, match, literal, parens). Last fn: anything.

## Strings

`"text"` with `\n \t \" \\`. Multi-line `"""..."""`. Interp `"{x}"`.

## Reserved names

Binding/user-fn under a builtin fires `ILO-P011`. Unreserved 2-char permanently safe; 4+ always safe; unreserved 3-char usually safe (no hard promise). 4+ builtins (`take drop mget mset flat range`) also reserved.

- 2: `at hd tl rd wr ct`
- 3: `abs avg cap cat cel chr cos det dot env exp fft fld flr flt fmt frq get grp has inv len log lsd lst lwr map max min mod now num ord pow pst rdb rdl rev rgx rng rnd rou run sin slc spl srt str sum tan trm unq upr wrl zip`

## Not here

No `&`, `&mut`, refs, lifetimes, ownership errors. Lifetime reasoning = wrong model.
