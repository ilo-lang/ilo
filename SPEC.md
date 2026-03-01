# ilo Language Spec

ilo is a token-minimal language for AI agents. Every design choice is evaluated against total token cost: generation + retries + context loading.

---

## Functions

```
<name> <param>:<type> ...><return-type>;<body>
```

- No parens around params — `>` separates params from return type
- `;` separates statements — no newlines required
- Last expression is the return value (no `return` keyword)
- Zero-arg call: `make-id()`

```
tot p:n q:n r:n>n;s=*p q;t=*s r;+s t
```

---

## Types

| Syntax | Meaning |
|--------|---------|
| `n` | number (f64) |
| `t` | text (string) |
| `b` | bool |
| `_` | nil |
| `L n` | list of number |
| `R n t` | result: ok=number, err=text |
| `order` | named type |

---

## Naming

Short names everywhere. 1–3 chars.

| Long | Short | Rule |
|------|-------|------|
| `order` | `ord` | truncate |
| `customers` | `cs` | consonants |
| `data` | `d` | single letter |
| `level` | `lv` | drop vowels |
| `discount` | `dc` | initials |
| `final` | `fin` | first 3 |
| `items` | `its` | first 3 |

Function names follow the same rules. Field names in constructors and external tool names keep their full form — they define the public interface.

---

## Operators

Prefix notation.

### Binary

| Op | Meaning | Types |
|----|---------|-------|
| `+a b` | add / concat / list concat | `n`, `t`, `L` |
| `+=a v` | append to list | `L` |
| `-a b` | subtract | `n` |
| `*a b` | multiply | `n` |
| `/a b` | divide | `n` |
| `=a b` | equal | any |
| `!=a b` | not equal | any |
| `>a b` | greater than | `n`, `t` |
| `<a b` | less than | `n`, `t` |
| `>=a b` | greater or equal | `n`, `t` |
| `<=a b` | less or equal | `n`, `t` |
| `&a b` | logical AND (short-circuit) | any (truthy) |
| `\|a b` | logical OR (short-circuit) | any (truthy) |

### Unary

| Op | Meaning | Types |
|----|---------|-------|
| `-x` | negate | `n` |
| `!x` | logical NOT | any (truthy) |

### Infix

| Op | Meaning | Types |
|----|---------|-------|
| `a??b` | nil-coalesce (if a is nil, return b) | any |
| `a>>f` | pipe (desugar to `f(a)`) | any |

Nesting is unambiguous — no parentheses needed:

```
+*a b c     -- (a * b) + c
*a +b c     -- a * (b + c)
>=+x y 100  -- (x + y) >= 100
-*a b *c d  -- (a * b) - (c * d)
```

Each nested operator saves 2 tokens (no `(` `)` needed). Flat expressions like `+a b` save 1 char vs `a + b`. Across 25 expression patterns, prefix notation saves **22% tokens** and **42% characters** vs infix. See [research/explorations/prefix-vs-infix/](research/explorations/prefix-vs-infix/) for the full benchmark.

Disambiguation: `-` followed by one atom is unary negate, followed by two atoms is binary subtract.

### Operands

Operator operands are **atoms** (literals, refs, field access) or **nested prefix operators**. Function calls are NOT operands — bind call results to a variable first:

```
-- DON'T: *n fac p  →  parses as Multiply(n, fac) with p dangling
-- DO:    r=fac p;*n r
```

---

## Builtins

Called like functions, compiled to dedicated opcodes.

| Call | Meaning | Returns |
|------|---------|---------|
| `len x` | length of string (bytes) or list (elements) | `n` |
| `str n` | number to text (integers format without `.0`) | `t` |
| `num t` | text to number (Err if unparseable) | `R n t` |
| `abs n` | absolute value | `n` |
| `min a b` | minimum of two numbers | `n` |
| `max a b` | maximum of two numbers | `n` |
| `flr n` | floor (round toward negative infinity) | `n` |
| `cel n` | ceiling (round toward positive infinity) | `n` |
| `get url` | HTTP GET | `R t t` |
| `spl t sep` | split text by separator | `L t` |
| `cat xs sep` | join list of text with separator | `t` |
| `has xs v` | membership test (list: element, text: substring) | `b` |
| `hd xs` | head (first element/char) of list or text | element / `t` |
| `tl xs` | tail (all but first) of list or text | `L` / `t` |
| `rev xs` | reverse list or text | same type |
| `srt xs` | sort list (all-number or all-text) or text chars | same type |
| `slc xs a b` | slice list or text from index a to b | same type |
| `rnd` | random float in [0, 1) | `n` |
| `rnd a b` | random integer in [a, b] inclusive | `n` |
| `now` | current Unix timestamp (seconds) | `n` |

`get` returns `Ok(body)` on success, `Err(message)` on failure (connection error, timeout, DNS failure, etc). `$` is a terse alias:

```
get url          -- R t t: Ok=response body, Err=error message
$url             -- same as get url
get! url         -- auto-unwrap: Ok→body, Err→propagate to caller
$!url            -- same as get! url
```

Behind the `http` feature flag (on by default). Without the feature, `get` returns `Err("http feature not enabled")`.

---

## Lists

```
xs=[1, 2, 3]
empty=[]
```

Comma-separated expressions in brackets. Trailing comma allowed. Use with `@` to iterate:

```
@x xs{+x 1}
```

Index by integer literal (dot notation):
```
xs.0     # first element
xs.2     # third element
```

**CLI list arguments:** Pass lists from the command line with commas (brackets also accepted):
```
ilo 'f xs:L n>n;len xs' 1,2,3       → 3
ilo 'f xs:L t>t;xs.0' 'a,b,c'       → a
```

---

## Statements

| Form | Meaning |
|------|---------|
| `x=expr` | bind |
| `cond{body}` | guard: return body if cond true |
| `cond expr` | braceless guard (single-expression body) |
| `cond{then}{else}` | ternary: evaluate then or else (no early return) |
| `!cond{body}` | guard: return body if cond false |
| `!cond expr` | braceless negated guard |
| `!cond{then}{else}` | negated ternary |
| `?x{arms}` | match named value |
| `?{arms}` | match last result |
| `@v list{body}` | iterate list |
| `ret expr` | early return from function |
| `~expr` | return ok |
| `^expr` | return err |
| `func! args` | call + auto-unwrap Result |
| `wh cond{body}` | while loop |
| `brk` / `brk expr` | exit enclosing loop (optional value) |
| `cnt` | skip to next iteration of enclosing loop |
| `expr>>func` | pipe: pass result as last arg to func |

---

## Match Arms

| Pattern | Meaning |
|---------|---------|
| `"gold":body` | literal text |
| `42:body` | literal number |
| `~v:body` | ok — bind inner value to `v` |
| `^e:body` | err — bind inner value to `e` |
| `_:body` | wildcard |

Arms separated by `;`. First match wins.

```
cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze"
```

### Braceless Guards

When the guard condition is a comparison or logical operator (`>=`, `<=`, `>`, `<`, `=`, `!=`, `&`, `|`) and the body is a single expression, braces are optional:

```
cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze"
```

Equivalent to `>=sp 1000{"gold"}` — saves 2 tokens per guard. Both forms produce identical AST.

Negated braceless guards also work: `!<=n 0 ^"must be positive"`.

### Ternary (Guard-Else)

A guard followed by a second brace block becomes a ternary — it produces a value without early return:

```
f x:n>t;=x 1{"yes"}{"no"}
```

Unlike guards, ternary does **not** return from the function. Code after the ternary continues executing:

```
f x:n>n;=x 0{10}{20};+x 1   -- always returns x+1, ternary value is discarded
```

Negated ternary: `!=x 1{"not one"}{"one"}`.

### Early Return

`ret expr` explicitly returns from the current function:

```
f x:n>n;>x 0{ret x};0         -- return x early if positive, else 0
f xs:L n>n;@x xs{>=x 10{ret x}};0  -- return first element >= 10
```

Guards already provide early return for simple cases. Use `ret` when you need early return inside a loop or deeply nested block.

Code after `ret` or `brk` in the same block is unreachable and triggers a warning (`ILO-T029`).

### While Loop

`wh cond{body}` loops while condition is truthy:

```
f>n;i=0;s=0;wh <i 5{i=+i 1;s=+s i};s    -- sum 1..5 = 15
f>n;i=0;wh true{i=+i 1;>=i 3{ret i}};0   -- early return from loop
```

Variable rebinding (`i=+i 1`) inside while loops updates the existing variable rather than creating a new binding.

### Break and Continue

`brk` exits the enclosing `wh` or `@` loop. `cnt` skips to the next iteration:

```
f>n;i=0;wh true{i=+i 1;>=i 3{brk}};i    -- i = 3
f>n;i=0;s=0;wh <i 5{i=+i 1;>=i 3{cnt};s=+s i};s   -- s = 3 (skips i>=3)
```

`brk expr` provides an optional value (currently discarded — the loop result is the last body value before the break).

Both `brk` and `cnt` work inside guards within loops. Using them outside a loop is a compile-time error (`ILO-T028`).

### Pipe Operator

`>>` chains calls by passing the left side as the last argument to the right side:

```
str x>>len           -- desugars to: len (str x)
add x 1>>add 2      -- desugars to: add 2 (add x 1)
f x>>g>>h            -- desugars to: h (g (f x))
```

Pipes desugar at parse time — no new AST node. Works with `!` for auto-unwrap: `f x>>g!>>h`.

### Safe Field Navigation

`.?` accesses a field only if the object is not nil; returns nil if it is:

```
user.?name         -- nil if user is nil, else user.name
user.?addr.?city   -- chained: nil propagates through chain
x.?name??"unknown" -- combine with ?? for defaults
```

Compiled via `OP_JMPNN` + `OP_JMP` to skip field access on nil values.

### Nil-Coalesce Operator

`??` evaluates the left side; if nil, evaluates and returns the right side:

```
x??42              -- if x is nil, returns 42
a??b??99           -- chained: first non-nil wins, else 99
mk 0??"default"   -- works with function results
```

Compiled via `OP_JMPNN` (jump if not nil) — right side is only evaluated when left is nil.

Use braces when the body has multiple statements:

```
>=sp 1000{a=classify sp;a}
```

```
?r{^e:^+"failed: "e;~v:v}
```

---

## Calls

Positional args, space-separated, no parens:

```
get-user uid
send-email d.email "Notification" msg
charge pid amt
```

### Call Arguments

Call arguments can be atoms or prefix expressions:

```
fac -n 1       -- Call(fac, [Subtract(n, 1)])
fac +a b       -- Call(fac, [Add(a, b)])
g +a b c       -- Call(g, [Add(a,b), c])  — 2 args
fac p           -- Call(fac, [Ref(p)])
```

Use parentheses when you need a full expression (including another call) as an argument:

```
f (g x)        -- Call(f, [Call(g, [x])])
```

---

## Records

Define:
```
type point{x:n;y:n}
```

Construct (type name as constructor):
```
p=point x:10 y:20
```

Access:
```
p.x
ord.addr.country
```

Update:
```
ord with total:fin cost:sh
```

---

## Tools (external calls)

```
tool <name>"<description>" <params>><return-type> timeout:<n>,retry:<n>
```

```
tool get-user"Retrieve user by ID" uid:t>R profile t timeout:5,retry:2
```

---

## Error Handling

`R ok err` return type. Call then match:

```
get-user uid;?{^e:^+"Lookup failed: "e;~d:use d}
```

Compensate/rollback inline:

```
charge pid amt;?{^e:release rid;^+"Payment failed: "e;~cid:continue}
```

### Auto-Unwrap `!`

`func! args` calls `func` and auto-unwraps the Result: if `~v` (Ok), returns `v`; if `^e` (Err), immediately returns `^e` from the enclosing function.

```
inner x:n>R n t;~x
outer x:n>R n t;d=inner! x;~d
```

Equivalent to `r=inner x;?r{~v:v;^e:^e}` but in 1 token instead of 12.

Rules:
- The called function must return `R` (else verifier error ILO-T025)
- The enclosing function must return `R` (else verifier error ILO-T026)
- `!` goes after the function name, before args: `get! url` not `get url!`
- Zero-arg: `fetch!()`

---

## Patterns (for LLM generators)

### Bind-first pattern

Always bind complex expressions to variables before using them in operators. Operators only accept atoms and nested operators as operands — not function calls.

```
-- DON'T: *n fac -n 1     (fac is an operand of *, not a call)
-- DO:    r=fac -n 1;*n r  (bind call result, then use in operator)
```

### Recursion template

```
<name> <params>><return>;<guard>;...;<recursive-calls>;combine
```

1. **Guard**: base case returns early — `<=n 1 1` (or `<=n 1{1}`)
2. **Bind**: bind recursive call results — `r=fac -n 1`
3. **Combine**: use bound results in final expression — `*n r`

### Factorial

```
fac n:n>n;<=n 1 1;r=fac -n 1;*n r
```

- `<=n 1 1` — braceless guard: if n <= 1, return 1
- `r=fac -n 1` — recursive call with prefix subtract as argument
- `*n r` — multiply n by result

### Fibonacci

```
fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b
```

- `<=n 1 n` — braceless guard: return n for 0 and 1
- `a=fib -n 1;b=fib -n 2` — two recursive calls, each with prefix arg
- `+a b` — add results

### Multi-statement bodies

Semicolons separate statements. Last expression is the return value.

```
f x:n>n;a=*x 2;b=+a 1;*b b    -- (x*2 + 1)^2
```

### DO / DON'T

```
-- DON'T: fac n:n>n;<=n 1 1;*n fac -n 1
--   ↑ *n sees fac as an atom operand, not a call

-- DO:    fac n:n>n;<=n 1 1;r=fac -n 1;*n r
--   ↑ bind-first: call result goes into r, then *n r works

-- DON'T: +fac -n 1 fac -n 2
--   ↑ + takes two operands; fac is just an atom ref

-- DO:    a=fac -n 1;b=fac -n 2;+a b
--   ↑ bind both calls, then combine
```

---

## Error Diagnostics

ilo verifies programs before execution and reports errors with stable codes, source context, and suggestions.

### Error codes

Every error has a stable code:

| Prefix | Phase |
|--------|-------|
| `ILO-L___` | lexer (tokenisation) |
| `ILO-P___` | parser (syntax) |
| `ILO-T___` | type verifier (static analysis) |
| `ILO-R___` | runtime (execution) |

Use `--explain` to see a detailed explanation:
```
ilo --explain ILO-T004
```

### Source context

Errors point at the relevant source location with a caret:
```
error[ILO-T005]: undefined function 'foo' (called with 1 args)
  --> 1:9
  |
1 | f x:n>n;foo x
  |         ^^^^^
  |
  = note: in function 'f'
  = suggestion: did you mean 'f'?
```

Parser, verifier, and runtime errors all show source spans. The verifier uses the enclosing statement span as the best available location for expression-level errors.

### Suggestions

The verifier provides context-aware hints:
- **Did you mean?** — Levenshtein-based suggestions for undefined variables, functions, fields, and types
- **Type conversion** — suggests `str` for n→t, `num` for t→n
- **Missing arms** — lists uncovered match patterns with types
- **Arity** — shows expected parameter signature

### Output formats

```
--ansi / -a     ANSI colour (default for TTY)
--text / -t     Plain text (no colour)
--json / -j     JSON (default for piped output)
NO_COLOR=1      Disable colour (same as --text)
```

JSON error output follows a structured schema with `severity`, `code`, `message`, `labels` (with spans), `notes`, and `suggestion` fields.

---

## Formatter

Two output modes for reformatting programs:

```
ilo 'code' --fmt              Dense wire format (canonical, for LLM I/O)
ilo 'code' --fmt-expanded     Expanded human format (for code review)
```

### Dense format

Single line per declaration, minimal whitespace. Operators glue to first operand:

```
cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze"
```

### Expanded format

Multi-line with 2-space indentation. Operators spaced from operands:

```
cls sp:n > t
  >= sp 1000 {
    "gold"
  }
  >= sp 500 {
    "silver"
  }
  "bronze"
```

Dense format is canonical — `dense(parse(dense(parse(src)))) == dense(parse(src))`.

---

## Complete Example

```
tool get-user"Retrieve user by ID" uid:t>R profile t timeout:5,retry:2
tool send-email"Send an email" to:t subject:t body:t>R _ t timeout:10,retry:1
type profile{id:t;name:t;email:t;verified:b}
ntf uid:t msg:t>R _ t;get-user uid;?{^e:^+"Lookup failed: "e;~d:!d.verified{^"Email not verified"};send-email d.email "Notification" msg;?{^e:^+"Send failed: "e;~_:~_}}
```

### Recursive Example

Factorial and Fibonacci as standalone functions:

```
fac n:n>n;<=n 1 1;r=fac -n 1;*n r
```

```
fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b
```
