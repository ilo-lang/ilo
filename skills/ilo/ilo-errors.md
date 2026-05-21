---
name: ilo-errors
description: Use this when reading ILO-XXXX error codes or fixing failures. Lists the common codes with one-line cause + fix; run `ilo --explain ILO-XXXX` for the long form.
---

# ilo error codes

`ILO-L###` lex, `ILO-P###` parse, `ILO-T###` type, `ILO-R###` runtime. `ilo --explain ILO-XXXX` for long form. No borrow/lifetime errors.

## Lex

- **L001 unknown token** - `AND`/`OR`/`\x{}` shorthand. Use `&` `|` `(x:t>r;body)`.

## Parse

- **P003 unexpected token** - missing brace/semi, or `=<a b`. Use `<=a b`. Multi-line indented bodies work natively; no need to collapse to `;`-separated.
- **P011 reserved name** - builtin or alias name used as binding LHS or function name (`head=...`, `length=...`, `flat=...`). The call-site rewrite silently mis-dispatches to the builtin. Rename: `myhd`, `hdr`, `flatv`, etc.
- **P009 unparenthesised lambda** - wrap `(p:t>r;body)`.
- **P020 incomplete function header** - header missing `>type;body`; finish it.
- **P021 double-minus prefix-binop trap** - `- -*a b *c d` ambiguous. Use `- 0 +*a b *c d` or bind first.
- **P103 AST nesting depth exceeded** - parser refused source nesting more than 256 levels deep (DoS guard for `ilo serv`). Flatten by binding intermediates, or raise the cap with `--max-ast-depth N`.

## Type

- **T001 unknown identifier** - declare, import, or check spelling.
- **T004 type mismatch** - return/param doesn't match. Change expression or declaration.
- **T005 undefined function / call-vs-binop trap** - calling something not callable. Often the shape `dx=xj 0-xi` (parses as `dx=(xj 0)-xi`, a call). Use prefix op: `-xj xi`, or pre-bind: `nxi=0-xi;+xj nxi`.
- **T006 arity mismatch** - add/remove args to match signature.
- **T007 not a function** - calling a value. Rename or rebind.
- **T010 non-result error-propagate** - `!` in a non-`R` fn. Declare `>R t t` or use `??` / match.
- **T013 builtin arg type** - `cat` is list-concat; text uses `fmt`/`+`.
- **T038 non-bool ternary cond** - `?h c a b` cond must be `b`. Bind first.
- **T039 0-arg fn as value** - `f` is a 0-arg fn used as a ref, not a call. Use `f()` in value position.

## Runtime

- **R001 div-by-zero** - guard with `=b 0 ^"..."`.
- **R004 wrong main arity** - CLI args don't match signature.
- **R012 no functions defined** - typo'd flag swallowed as positional.
- **R016 wall-clock runtime exceeded** - `ilo run` killed the program at the 60 s (default) budget. Almost always an infinite loop - check loop variables increment and recursion has a base case. Raise with `--max-runtime SECS` if legitimate.
- **R017 stdout output exceeded** - `ilo run` killed the program at the ~100 MB (default) stdout budget. Usually a `prnt` call inside an unbounded loop. Raise with `--max-output-bytes BYTES` if legitimate.
- **R020 file not found** - check path or `env "HOME"`.
- **R030 http error** - non-2xx or network. Match `^e`.
- **R040 json parse error** - bad `jpar` input. Match `^e`.

## Patterns

`^"divide by zero"`: guard denom. `NaN`: clamp `asin`/`acos`/`sqrt`/`log` inputs at boundary.

## JSON shape

One JSON object per line: `{"code","message","span":{"file","line","col","len"},"hint"}`. Route on `code`; edit at `span`. See `ilo-edit-loop`.
