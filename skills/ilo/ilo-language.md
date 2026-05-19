---
name: ilo-language
description: Use this when writing or reviewing .ilo source. Covers prefix notation, type sigils, guards, match, pipes, records, and Result handling.
---

# ilo language

Prefix-notation, strongly-typed, verified pre-run. Bodies single-line, `;`-separated. No borrow checker, no lifetimes. RC-managed values; type checker enforces shape only.

## Function syntax

`tot p:n q:n r:n>n;s=*p q;t=*s r;+s t`. No param parens. `>` returns. `;` separates statements. Last expr returns. Zero-arg: `make-id()`.

## Types

`n` num, `t` text, `b` bool, `_` nil/any. `L n` list, `M t n` map, `R n t` result, `O n` optional, `S a b c` sum (closed, runtime `t`), `F n t` fn-type. Named: `order`. Type vars: any letter except `n t b`. `?? x d` nil-coalesce; unwraps `O T`.

## Operators (prefix)

Binary `+ - * / % < > <= >= = !=`, bool `& | !`, append `+=`. Nesting: `+*a b c` = `(a*b)+c`; outer binds inner LEFT. Operators take atoms/nested-ops, NOT calls; bind first: `r=fac -n 1;*n r`. Compound prefix doesn't compose: `<=a b`, not `=<a b`.

## Identifiers + comments

Idents `[a-z][a-z0-9]*(-[a-z0-9]+)*`, short (1-3 chars). No capitals/underscores except after `.` / `.?` for JSON keys (`r.URL`). Comments `-- to EOL`; `--x` is a comment, not double-negate (use `- -x 1`).

## Guards

Flat early returns at statement position: `cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze"`. Braceless `cond expr` is cheaper than `cond{expr}`. Bare comparison at statement IS a guard; bind to return otherwise: `r=>a b;r`.

## Match

`?r{~v:v;^e:^+"failed: "e;_:"unknown"}`. Arms: `"lit":body`, `42:body`, `~v:body` ok-bind, `^e:body` err-bind, `_:body` else.

## Results

`div a:n b:n>R n t;=b 0 ^"divide by zero";~/a b`. `!` auto-unwraps in `R`-fns (`d=get! url`). `!!` panic-unwraps anywhere on `^e`/`nil`.

## Loops

`@x xs{body}` foreach, `@i 0..5{body}` range half-open, `wh <i 10{...}` while. `brk` exits, `cnt` skips, `ret v` early-returns.

## Pipes

`xs >> flt pos >> map sq` desugars left-to-right to nested calls. Wrap in `()` for non-last fns in multi-fn files.

## Records

`type point{x:n;y:n}`, `p=point x:10 y:20`. `p.x` access, `{x;y}=p` destructure, `p with x:30` update, `p.?missing` safe-nav (nil).

## Lambdas

Parenthesised, passed directly: `map (x:n>n;+x 1) xs`. Capturing lambdas run only on tree engine; VM/JIT/AOT auto-fall-back.

## Multi-function files

Non-last fns end with a safe expression (op, index, match, literal, parens). Last fn: anything.

## Strings

`"text"` with `\n \t \" \\`. Multi-line `"""..."""`. Interp `"{x} items"`.

## What's not here

No `&`, `&mut`, references, lifetimes, or ownership errors. Lifetime reasoning means wrong mental model.
