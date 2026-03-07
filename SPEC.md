# ilo Language Spec

ilo is a token-optimised programming language for AI agents. Every design choice is evaluated against total token cost: generation + retries + context loading.

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
| `O n` | optional number (nil or n) |
| `M t n` | map from text keys to numbers |
| `S red green blue` | sum type — one of named text variants |
| `F n t` | function type: takes n, returns t (used in HOF params) |
| `order` | named type |
| `a` | type variable — any single lowercase letter except n, t, b |

### Optional (`O T`)

`O T` accepts either `nil` or a value of type `T`.

```
f x:O n>n;??x 0     -- unwrap optional or default to 0
g>O n;nil           -- returns nil (valid O n)
h>O n;42            -- returns 42 (valid O n)
```

`??x default` — nil-coalesce: returns `x` if non-nil, else `default`. Unwraps `O T` to `T`.

### Sum types (`S a b c`)

Closed set of named text variants. Verifier-enforced; runtime value is always `t`.

```
color x:S red green blue > t
  ?x{red:"ff0000";green:"00ff00";blue:"0000ff"}
```

Sum types are compatible with `t` — a sum value can be passed to any `t` parameter.

### Map type (`M k v`)

Dynamic key-value collection. Keys are always text at runtime.

```
mmap                      -- empty map
mset m k v               -- return new map with key k set to v
mget m k                 -- value at key k, or nil
mhas m k                 -- b: true if key exists
mkeys m                  -- L t: sorted list of keys
mvals m                  -- L v: values sorted by key
mdel m k                 -- return new map with key k removed
len m                     -- number of entries
```

Example:

```
scores>M t n
  m=mmap
  m=mset m "alice" 99
  m=mset m "bob" 87
  mget m "alice"        -- 99
```

### Type variables

A single lowercase letter (other than `n`, `t`, `b`) in type position is a type variable, treated as `unknown` during verification. Used for higher-order function signatures:

```
identity x:a>a;x
apply f:F a a x:a>a;f x
```

Type variables provide weak generics — the verifier accepts any type for `a` without consistency checking across call sites.

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

### Reserved words

The following identifiers are reserved and cannot be used as names: `if`, `return`, `let`, `fn`, `def`, `var`, `const`. Using them produces a friendly error with the ilo equivalent:

```
-- ERROR: `if` is a reserved word. Use: ?cond{true:... false:...}
-- ERROR: `return` is a reserved word. Last expression is the return value.
-- ERROR: `let` is a reserved word. Use: name = expr
-- ERROR: `fn`/`def` is a reserved word. Use: name param:type > rettype; body
```

---

## Comments

```
-- full line comment
+a b -- end of line comment
-- no multi-line comments; use consecutive -- lines
-- like this
```

Single-line only. `--` to end of line. No multi-line comment syntax — newlines are a human display concern, not a language concern. An entire ilo program can be one line. Use consecutive `--` lines when humans need multi-line comments. Stripped at the lexer level before parsing — comments produce no AST nodes and cost zero runtime tokens. Generating `--` costs 1 LLM token, so comments are essentially free.

**Gotcha:** `--x 1` is a comment, not "negate (x minus 1)". The lexer matches `--` greedily as a comment and eats the rest of the line. To negate a subtraction, use a space or bind first:

```
-- DON'T: --x 1        (comment, not negate-subtract)
-- DO:    - -x 1       (space separates the two minus operators)
-- DO:    r=-x 1;-r    (bind first)
```

---

## Operators

Both prefix and infix notation are supported. **Prefix is preferred** — it is the token-optimal form that eliminates parentheses and produces denser code. Infix is available for readability when needed.

### Binary

| Prefix | Infix | Meaning | Types |
|--------|-------|---------|-------|
| `+a b` | `a + b` | add / concat / list concat | `n`, `t`, `L` |
| `+=a v` | | append to list | `L` |
| `-a b` | `a - b` | subtract | `n` |
| `*a b` | `a * b` | multiply | `n` |
| `/a b` | `a / b` | divide | `n` |
| `=a b` | `a == b` | equal (prefix `=` is preferred; `==a b` also accepted) | any |
| `!=a b` | `a != b` | not equal | any |
| `>a b` | `a > b` | greater than | `n`, `t` |
| `<a b` | `a < b` | less than | `n`, `t` |
| `>=a b` | `a >= b` | greater or equal | `n`, `t` |
| `<=a b` | `a <= b` | less or equal | `n`, `t` |
| `&a b` | `a & b` | logical AND (short-circuit) | any (truthy) |
| `\|a b` | `a \| b` | logical OR (short-circuit) | any (truthy) |

### Unary

| Op | Meaning | Types |
|----|---------|-------|
| `-x` | negate | `n` |
| `!x` | logical NOT | any (truthy) |

### Special infix

| Op | Meaning | Types |
|----|---------|-------|
| `a??b` | nil-coalesce (if a is nil, return b) | any |
| `a>>f` | pipe (desugar to `f(a)`) | any |

### Prefix nesting (no parens needed)

```
+*a b c     -- (a * b) + c
*a +b c     -- a * (b + c)
>=+x y 100  -- (x + y) >= 100
-*a b *c d  -- (a * b) - (c * d)
```

### Infix precedence

Standard mathematical precedence (higher binds tighter):

| Level | Operators |
|-------|-----------|
| 6 | `*` `/` |
| 5 | `+` `-` `+=` |
| 4 | `>` `<` `>=` `<=` |
| 3 | `=` `!=` |
| 2 | `&` |
| 1 | `\|` |

Function application binds tighter than all infix operators:

```
f a + b     -- (f a) + b, NOT f(a + b)
x * y + 1   -- (x * y) + 1
(x + y) * 2 -- parens override precedence
```

Each nested prefix operator saves 2 tokens (no `(` `)` needed). Flat prefix like `+a b` saves 1 char vs `a + b`. Across 25 expression patterns, prefix notation saves **22% tokens** and **42% characters** vs infix. See [research/explorations/prefix-vs-infix/](research/explorations/prefix-vs-infix/) for the full benchmark.

Disambiguation: `-` followed by one atom is unary negate, followed by two atoms is binary subtract.

### Operands

Operator operands are **atoms** (literals, refs, field access) or **nested prefix operators**. Function calls are NOT operands — bind call results to a variable first:

```
-- DON'T: *n fac p  →  parses as Multiply(n, fac) with p dangling
-- DO:    r=fac p;*n r
```

**Negative literals vs binary minus**: the lexer greedily includes a leading `-` into number tokens. `-1`, `-7`, `-0` are all number literals. To subtract from zero, use a space: `- 0 v` (Minus token, then `0`, then `v`).

```
f v:n>n;-0 v   -- WRONG: -0 is Number(-0.0); v is a stray token
f v:n>n;- 0 v  -- OK: binary subtract: 0 - v = -v
```

---

## String Literals

Text values are written in double quotes. Escape sequences:

| Sequence | Meaning |
|----------|---------|
| `\n` | newline |
| `\t` | tab |
| `\r` | carriage return |
| `\"` | literal double quote |
| `\\` | literal backslash |

```
"hello\nworld"      -- two-line string
"col1\tcol2"        -- tab-separated
spl "\n" text       -- split file content into lines
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
| `mod a b` | remainder (modulo); errors on zero divisor | `n` |
| `flr n` | floor (round toward negative infinity) | `n` |
| `cel n` | ceiling (round toward positive infinity) | `n` |
| `rnd` | random float in [0, 1) | `n` |
| `rnd a b` | random integer in [a, b] (inclusive) | `n` |
| `now` | current Unix timestamp (seconds) | `n` |
| `get url` | HTTP GET | `R t t` |
| `get url headers` | HTTP GET with custom headers (`M t t` map) | `R t t` |
| `post url body` | HTTP POST with text body | `R t t` |
| `post url body headers` | HTTP POST with body and custom headers (`M t t` map) | `R t t` |
| `env key` | read environment variable | `R t t` |
| `rd path` | read file; format auto-detected from extension (`.csv`/`.tsv`→grid, `.json`→graph, else text) | `R ? t` |
| `rd path fmt` | read file with explicit format override (`"csv"`, `"tsv"`, `"json"`, `"raw"`) | `R ? t` |
| `rdl path` | read file as list of lines | `R (L t) t` |
| `rdb s fmt` | parse string/buffer in given format — for data from HTTP, env vars, etc. | `R ? t` |
| `wr path s` | write text to file (overwrite) | `R t t` |
| `wr path data "csv"` | write list-of-lists as CSV (with proper quoting) | `R t t` |
| `wr path data "tsv"` | write list-of-lists as TSV | `R t t` |
| `wr path data "json"` | write any value as pretty JSON | `R t t` |
| `wrl path xs` | write list of lines to file (joins with `\n`) | `R t t` |
| `trm s` | trim leading and trailing whitespace | `t` |
| `spl t sep` | split text by separator | `L t` |
| `fmt tmpl args…` | format string — `{}` placeholders filled left-to-right | `t` |
| `cat xs sep` | join list of text with separator | `t` |
| `has xs v` | membership test (list: element, text: substring) | `b` |
| `hd xs` | head (first element/char) of list or text | element / `t` |
| `tl xs` | tail (all but first) of list or text | `L` / `t` |
| `rev xs` | reverse list or text | same type |
| `srt xs` | sort list (all-number or all-text) or text chars | same type |
| `srt fn xs` | sort list by key function (returns number or text key) | `L` |
| `unq xs` | remove duplicates, preserve order (list or text chars) | same type |
| `slc xs a b` | slice list or text from index a to b | same type |
| `jpth json path` | JSON path lookup (dot-separated keys, array indices) | `R t t` |
| `jdmp value` | serialise ilo value to JSON text | `t` |
| `prnt value` | print value to stdout, return it unchanged (passthrough) | same type |
| `jpar text` | parse JSON text into ilo values | `R ? t` |
| `grp fn xs` | group list by key function | `M t (L a)` |
| `flat xs` | flatten one level of nesting | `L a` |
| `sum xs` | sum of numeric list (0 for empty) | `n` |
| `avg xs` | mean of numeric list (error if empty) | `n` |
| `rgx pat s` | regex: no groups→all matches; groups→first match captures | `L t` |
| `mmap` | create empty map | `M t _` |
| `mget m k` | value at key k (nil if missing) | element or nil |
| `mset m k v` | new map with key k set to v | `M k v` |
| `mhas m k` | true if key exists | `b` |
| `mkeys m` | sorted list of keys | `L t` |
| `mvals m` | values sorted by key | `L v` |
| `mdel m k` | new map with key k removed | `M k v` |

### Builtin aliases

All builtins accept long-form names that resolve to the canonical short form after parsing. Using a long form triggers a hint suggesting the short form. This lets newcomers write readable code while learning the canonical names.

| Long form | → | Short |
|-----------|---|-------|
| `floor` | → | `flr` |
| `ceil` | → | `cel` |
| `round`, `random` | → | `rnd` |
| `string` | → | `str` |
| `number` | → | `num` |
| `length` | → | `len` |
| `head` | → | `hd` |
| `tail` | → | `tl` |
| `reverse` | → | `rev` |
| `sort` | → | `srt` |
| `slice` | → | `slc` |
| `unique` | → | `unq` |
| `filter` | → | `flt` |
| `fold` | → | `fld` |
| `flatten` | → | `flat` |
| `concat` | → | `cat` |
| `contains` | → | `has` |
| `group` | → | `grp` |
| `average` | → | `avg` |
| `print` | → | `prnt` |
| `trim` | → | `trm` |
| `split` | → | `spl` |
| `format` | → | `fmt` |
| `regex` | → | `rgx` |
| `read` | → | `rd` |
| `readlines` | → | `rdl` |
| `readbuf` | → | `rdb` |
| `write` | → | `wr` |
| `writelines` | → | `wrl` |

```
length xs   -- works, but emits: hint: `length` → `len` (canonical short form)
len xs      -- canonical — no hint
```

`get` and `post` return `Ok(body)` on success, `Err(message)` on failure (connection error, timeout, DNS failure, etc). `$` is a terse alias for `get`:

```
get url          -- R t t: Ok=response body, Err=error message
$url             -- same as get url
get! url         -- auto-unwrap: Ok→body, Err→propagate to caller
$!url            -- same as get! url

post url body           -- R t t: HTTP POST with text body
post url body headers   -- R t t: HTTP POST with body and custom headers

-- Custom headers: build an M t t map with mmap/mset
h=mmap
h=mset h "x-api-key" "secret"
r=get url h      -- GET with x-api-key header
r=post url body h -- POST with x-api-key header
```

Behind the `http` feature flag (on by default). Without the feature, `get`/`post` return `Err("http feature not enabled")`.

`env` reads an environment variable by name, returning `Ok(value)` or `Err("env var 'KEY' not set")`:

```
env key          -- R t t: Ok=value, Err=not set message
env! key         -- auto-unwrap: Ok→value, Err→propagate to caller
```

### JSON builtins

`jpth` extracts a value from a JSON string by dot-separated path. Array elements are accessed by numeric index:

```
jpth json "name"            -- R t t: Ok=extracted value as text, Err=error
jpth json "user.name"       -- nested path lookup
jpth json "items.0.name"    -- array index access
jpth! json "name"           -- auto-unwrap
```

`jdmp` serialises any ilo value to a JSON string:

```
jdmp 42                     -- "42"
jdmp "hello"                -- "\"hello\""
jdmp [1, 2, 3]             -- "[1,2,3]"
jdmp (pt x:1 y:2)          -- "{\"x\":1,\"y\":2}"
```

`jpar` parses a JSON string into ilo values. JSON objects become records with type name `json`, arrays become lists, strings/numbers/bools/null map directly:

```
jpar text                   -- R ? t: Ok=parsed value, Err=parse error
r=jpar! "{\"x\":1}"        -- r is a json record, access with r.x
```


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

Guards replace `if`/`else if`/`else`. They are flat statements — no nesting, no closing braces to match. Each guard returns early if its condition is true; otherwise execution falls through to the next statement. Multiple guards chain vertically, keeping indentation depth constant regardless of how many conditions there are.

Match replaces `switch`. There is no fall-through — each arm is independent. The `_` arm is the default catch-all.

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
| `@i a..b{body}` | range iteration: i from a (inclusive) to b (exclusive) |
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
| `n v:body` | number — branch if value is a number, bind to `v` |
| `t v:body` | text — branch if value is text, bind to `v` |
| `b v:body` | bool — branch if value is a bool, bind to `v` |
| `l v:body` | list — branch if value is a list, bind to `v` |
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

**Comparison operators always start a guard at statement position.** You cannot use `=`, `<`, `>`, `<=`, `>=` etc. as a standalone return expression — the parser treats them as a guard condition and expects a following return value. To return a comparison result, bind it first:

```
-- WRONG: r=has xs v;=r true   -- =r true is parsed as a guard, not a return expression
-- OK:    r=has xs v;r          -- return the bool directly (only safe as the last statement)
-- OK:    has xs v              -- bare call is safe as last statement in last function
```

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

### Range Iteration

`@i a..b{body}` iterates `i` from `a` (inclusive) to `b` (exclusive). The index variable is a fresh binding per iteration; other variables in the body update the enclosing scope:

```
f>n;s=0;@i 0..5{s=+s i};s      -- sum 0+1+2+3+4 = 10
f>n;xs=[];@i 0..3{xs=+=xs i};xs -- [0, 1, 2]
```

### While Loop

`wh cond{body}` loops while condition is truthy:

```
f>n;i=0;s=0;wh <i 5{i=+i 1;s=+s i};s    -- sum 1..5 = 15
f>n;i=0;wh true{i=+i 1;>=i 3{ret i}};0   -- early return from loop
```

Variable rebinding inside loops updates the existing variable rather than creating a new binding.

### Break and Continue

`brk` exits the enclosing `wh` or `@` loop. `cnt` skips to the next iteration:

```
f>n;i=0;wh true{i=+i 1;>=i 3{brk}};i    -- i = 3
f>n;i=0;s=0;wh <i 5{i=+i 1;>=i 3{cnt};s=+s i};s   -- s = 3 (skips i>=3)
```

`brk expr` provides an optional value (currently discarded — the loop result is the last body value before the break).

Both `brk` and `cnt` work inside guards within loops. Using them outside a loop is a compile-time error (no-op in current implementation).

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

Destructure:
```
{x;y}=p
```
Binds `x` to `p.x` and `y` to `p.y`. All named fields must exist on the record.

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

Tool declarations are verified statically like functions — call sites are type-checked and arity-checked. At runtime, tool calls dispatch through a provider configured via `--tools <config.json>`:

```json
{
  "tools": {
    "get-user": {
      "url": "https://api.example.com/get-user",
      "method": "POST",
      "timeout_secs": 5,
      "retries": 2,
      "headers": { "Authorization": "Bearer token" }
    }
  }
}
```

ilo serialises call arguments as `{"args": [...]}` (JSON array), sends them to the endpoint, and deserialises the response body back to an ilo value. HTTP 2xx → `Ok(response)`, non-2xx → `Err("HTTP <status>: ...")`. Without `--tools`, tool calls return `Ok(_)` (stub behaviour).

**Value ↔ JSON mapping:**

| ilo type | JSON |
|----------|------|
| `n` | number |
| `t` | string |
| `b` | boolean |
| `_` | null |
| `L n` | array |
| `R ok err` | `{"ok": ...}` or `{"err": ...}` |
| record | object |

Tool return type `>t` is the escape hatch — any JSON response is coerced to a text string without parsing.

---

## Imports

Split programs across files with `use`:

```
use "path/to/file.ilo"         -- import all declarations
use "path/to/file.ilo" [name1 name2]  -- import only named declarations
```

All imported declarations merge into a flat shared namespace — no qualification, no `mod::fn` syntax. The verifier catches name collisions.

```
-- math.ilo
dbl n:n>n; *n 2
half n:n>n; /n 2

-- main.ilo
use "math.ilo"
run n:n>n; dbl! half n
```

### Rules

- Path is relative to the importing file's directory
- Transitive: if `a.ilo` uses `b.ilo`, `b.ilo`'s declarations are visible to `main.ilo` when it uses `a.ilo`
- Circular imports are an error (`ILO-P018`)
- Scoped import with unknown name: `ILO-P019`
- `use` in inline code (no file context): `ILO-P017`

### Error codes

| Code | Condition |
|------|-----------|
| `ILO-P017` | File not found or `use` in inline mode |
| `ILO-P018` | Circular import detected |
| `ILO-P019` | Name in `[...]` list not declared in the imported file |

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

### Multi-function files

Functions in a file are separated by **newlines**. The parser strips all newlines, so the token stream is flat. After parsing each function body, the parser uses the next newline-delimited boundary to start the next declaration.

A non-last function body's **final expression must not be a bare variable reference (`Ref`) or a function call**, because the parser greedily reads following tokens as additional call arguments. Safe endings prevent this:

| Ending | Example | Safe? | Why |
|--------|---------|-------|-----|
| Binary operator | `+n 0`, `*x 1` | ✓ | fixed arity — no greedy loop |
| Index access | `xs.0`, `rec.field` | ✓ | returns `Expr::Index`, not `Ref` |
| Match block | `?v{…}` | ✓ | ends with `}` |
| ForEach block | `@x xs{…}` | ✓ | ends with `}` |
| Parenthesised expr | `(x>>f>>g)` | ✓ | ends with `)` |
| Text/number literal | `"ok"`, `42` | ✓ | literal, not `Ref` |
| Bare variable (`Ref`) | `n`, `result` | ✗ | greedy loop fires |
| Bare function call | `len xs`, `f a` | ✗ | greedy loop fires |

The **last function in a file** can end with anything — greedy parsing stops at EOF.

```
-- Non-last functions: end with a binary expression
digs n:n>n;t=str n;l=len t;+l 0    -- +l 0 = l (binary, safe)
clamp n:n lo:n hi:n>n;<n lo lo;>n hi hi;+n 0  -- +n 0 = n (binary, safe)

-- Last function: bare call is fine
sz xs:L n>n;len xs                  -- EOF — greedy loop stops naturally
```

To use a pipe chain in a non-last function, wrap it in parentheses:
```
dbl-inc x:n>n;(x>>dbl>>inc)   -- parens prevent >> from consuming next function's name
inc-sq x:n>n;x>>inc>>sq       -- last function — no parens needed
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
--no-hints / -nh  Suppress idiomatic hints
NO_COLOR=1      Disable colour (same as --text)
```

JSON error output follows a structured schema with `severity`, `code`, `message`, `labels` (with spans), `notes`, and `suggestion` fields.

### Idiomatic hints

After successful execution, ilo scans the source for non-canonical forms and emits hints to stderr:

```
hint: `==` → `=` saves 1 char (both mean equality in ilo)
hint: `length` → `len` (canonical short form)
```

Builtin alias hints appear at most once per program (the first long-form name found). In JSON mode, hints appear as `{"hints":["..."]}` on stderr. Suppress with `--no-hints` / `-nh`.

---

## Formatter

Dense output is the default — newlines are for humans, not agents. No flag needed for dense format:

```
ilo 'code'                    Dense wire format (default)
ilo 'code' --dense / -d       Same, explicit
ilo 'code' --expanded / -e    Expanded human format (for code review)
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
