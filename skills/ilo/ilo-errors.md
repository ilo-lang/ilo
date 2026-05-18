---
name: ilo-errors
description: Use this when reading ILO-XXXX error codes or fixing failures. Lists the common codes with one-line cause + fix; run `ilo --explain ILO-XXXX` for the long form.
---

# ilo error codes

`ILO-L###` lex, `ILO-P###` parse, `ILO-T###` type, `ILO-R###` runtime. Run `ilo --explain ILO-XXXX` for the long form.

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

- **R001 division by zero** - guard with `=b 0 ^"..."`.
- **R004 wrong main arity** - CLI args don't match signature.
- **R012 no functions / capture not supported** - typo'd flag swallowed as positional, or capturing lambda on a non-tree engine (auto-fallback should handle this).
- **R020 file not found** - check path or `env "HOME"`.
- **R030 http error** - non-2xx or network. Match `^e`.
- **R040 json parse error** - bad `jpar` input. Match `^e`.

## Common patterns

- `^"divide by zero"`: guard the denominator.
- Mystery arity after `--engine tree`: not a real flag. Use `--run-tree`, `--run-vm`, `--jit`.
- `NaN` in output: `asin`/`acos`/`sqrt`/`log` ran out-of-domain upstream; clamp at the boundary.

## JSON shape

```json
{"code":"ILO-T004","message":"expected n, got t",
 "span":{"file":"x.ilo","line":3,"col":12,"len":5},"hint":"..."}
```

Route on `code`; edit at `span`; pass `code` to `ilo --explain` for the long form.
