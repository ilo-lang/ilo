---
name: ilo-errors
description: Use this when reading ILO-XXXX error codes or fixing failures. Lists the common codes with one-line cause + fix; run `ilo --explain ILO-XXXX` for the long form.
---

# ilo error codes

`ILO-L###` lex, `ILO-P###` parse, `ILO-T###` type, `ILO-R###` runtime. Run `ilo --explain ILO-XXXX` for the long form.

ilo has no borrow checker, no lifetimes, no ownership rules, no `&`/`&mut`. The four classes above are the registry; ownership errors do not exist.

## Lex

- **L001 unknown token** - `AND`/`OR`/`\x{}` shorthand. Use `&` `|` `(x:t>r;body)`.

## Parse

- **P003 unexpected token** - missing brace/semi, or `=<a b`. Use `<=a b`. Collapse multi-line bodies to `;`-separated.
- **P009 unparenthesised lambda** - wrap `(p:t>r;body)`.
- **P020 incomplete function header** - header missing `>type;body`; finish it.
- **P021 double-minus prefix-binop trap** - `- -*a b *c d` ambiguous. Use `- 0 +*a b *c d` or bind first.

## Type

- **T001 unknown identifier** - declare, import, or check spelling.
- **T004 type mismatch** - return/param doesn't match. Change expression or declaration.
- **T005 wrong operand type** - convert with `num s` / `str n`, or use the right op.
- **T006 arity mismatch** - add/remove args to match signature.
- **T007 not a function** - calling a value. Rename or rebind.
- **T010 non-result error-propagate** - `!` in a non-`R` fn. Declare `>R t t` or use `??` / match.

## Runtime

- **R001 div-by-zero** - guard with `=b 0 ^"..."`.
- **R004 wrong main arity** - CLI args don't match signature.
<<<<<<< HEAD
- **R012 no functions defined** - typo'd flag swallowed as positional. (Phase 2 captures run natively; ILO-E802 covers the >255 overflow only.)
=======
- **R012 capture not supported** - typo'd flag, or capturing lambda on non-tree (auto-fallback usually handles).
>>>>>>> e8d1ecc437f0632f933bf5bfba17c9bfa06d0067
- **R020 file not found** - check path or `env "HOME"`.
- **R030 http error** - non-2xx or network. Match `^e`.
- **R040 json parse error** - bad `jpar` input. Match `^e`.

## Patterns

`^"divide by zero"`: guard denominator. Mystery arity after `--engine tree`: not a flag, use `--run-tree`/`--run-vm`/`--jit`. `NaN` in output: `asin`/`acos`/`sqrt`/`log` out-of-domain upstream; clamp at boundary.

## JSON shape

Diagnostics emit one JSON object per line: `{"code","message","span":{"file","line","col","len"},"hint"}`. Route on `code`; edit at `span`. For the wire shape and repair loop load `ilo-edit-loop`.
