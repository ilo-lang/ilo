---
name: ilo-language
description: Use this when writing or reviewing .ilo source. Covers prefix notation, type sigils, guards, match, pipes, records, and Result handling.
---

# ilo language

Prefix-notation, strongly-typed, verified pre-run. Bodies single-line, `;`-separated.

## Function syntax

```
name p:type q:type > return-type ; body
tot p:n q:n r:n>n;s=*p q;t=*s r;+s t
```

No param parens. `>` returns. `;` separates statements. Last expr returns. Zero-arg: `make-id()`.

## Types

`n` number, `t` text, `b` bool, `_` nil/any. `L n` list, `M t n` map, `R n t` result, `O n` optional, `S a b c` sum (closed, runtime `t`), `F n t` fn-type. Named: `order`. Type vars: any letter except `n t b`.

`?? x d` nil-coalesce; unwraps `O T` to `T`.

## Operators (prefix)

Binary `+ - * / % < > <= >= = !=`, bool `& | !`, append `+=`.

Nesting: `+*a b c` = `(a*b)+c`. Outer binds inner as LEFT: `*/a b c` = `(a/b)*c`.

Operators take atoms/nested-ops, NOT calls. Bind first:

```
-- WRONG: *n fac -n 1
-- RIGHT: r=fac -n 1;*n r
```

Compound prefix doesn't compose: `<=a b`, not `=<a b`.

## Identifiers

`[a-z][a-z0-9]*(-[a-z0-9]+)*`. No capitals/underscores except after `.` / `.?` for JSON keys (`r.URL`). Short names (1-3 chars).

## Comments

`-- to EOL`. `--x` is a comment, not double-negate; use `- -x 1`.

## Guards

Flat early returns at statement position:

```
cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze"
```

Braceless `cond expr` is cheaper than `cond{expr}`. A bare comparison at statement IS a guard; bind to return otherwise: `r=>a b;r`.

## Match

```
?r{~v:v;^e:^+"failed: "e;_:"unknown"}
```

Arms: `"lit":body`, `42:body`, `~v:body` ok-bind, `^e:body` err-bind, `_:body` else.

## Results

```
div a:n b:n>R n t;=b 0 ^"divide by zero";~/a b
```

`!` auto-unwraps in `R`-returning fns: `d=get! url`. `!!` panic-unwraps anywhere on `^e`/`nil`.

## Loops

```
@x xs{body}        -- foreach
@i 0..5{body}      -- range, half-open
wh <i 10{i=+i 1}   -- while
```

`brk` exits, `cnt` skips, `ret v` early-returns.

## Pipes

`xs >> flt pos >> map sq`, left-to-right, desugars to nested calls. Wrap in `()` for non-last fns in multi-fn files.

## Records

```
type point{x:n;y:n}
p=point x:10 y:20
p.x                -- access
{x;y}=p            -- destructure
p with x:30        -- update
p.?missing         -- safe nav -> nil
```

## Lambdas

Parenthesised, passed directly to HOFs:

```
map (x:n>n;+x 1) xs
flt (s:t>b;>(len s) 0) ws
```

Capturing lambdas run only on the tree engine; VM/JIT/AOT auto-fall-back.

## Multi-function files

Non-last fns must end with a safe expression (binary/unary op, index, match, literal, parens). Last fn: anything.

```
dbl x:n>n;+*x 2 0      -- safe
main x:n>n;dbl x        -- last
```

## Strings

`"text"` with `\n \t \" \\`. Multi-line `"""..."""`. Interp `"{x} items"`.
