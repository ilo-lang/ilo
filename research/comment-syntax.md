# Comment Syntax Exploration

Current syntax: `--` to end of line. Should we change it?

## Current: `--` (double dash)

```
+a b -- add a and b
```

**Pros:**
- SQL-style, familiar to many
- 1 LLM token to generate
- Stripped at lexer level (logos skip rule), zero runtime cost

**Cons:**
- Ambiguous with nested subtraction. `--x 1` is a comment, not "negate (x minus 1)". Must write `- -x 1` or `r=-x 1;-r`
- This is a silent footgun — the lexer eats the rest of the line with no error

## Option A: `#` (hash)

```
+a b # add a and b
```

**Pros:**
- 1 LLM token, 1 character (saves 1 char vs `--`)
- No operator ambiguity — `#` isn't used as an operator
- Familiar from Python, Ruby, shell, YAML, TOML

**Cons:**
- Heavily overloaded symbol: markdown headers, CSS selectors, hex colors, C preprocessor
- ilo code inside markdown would need escaping or fencing
- Could conflict if `#` is ever wanted for another purpose (e.g., map literals, tagged values)

## Option B: `//` (double slash)

```
+a b // add a and b
```

**Pros:**
- C/Java/JS/Rust-style, very widely known
- No operator ambiguity — `/` is division, but `//` can be lexed greedily as comment
- 1 LLM token

**Cons:**
- Same greedy-lexer trick as `--`: `//x 1` would be a comment, not "divide divide x 1". But `//x 1` is unlikely to be intentional code anyway.
- Could conflict if `//` is ever wanted for integer division (Python uses `//`)
- 2 characters like `--`

## Option C: `/* */` (C-style block comments)

```
+a b /* add a and b */
/* multi-line
   comment */
```

**Pros:**
- Enables multi-line comments
- Very widely known
- No line-scoped ambiguity

**Cons:**
- Missing `*/` silently eats the rest of the file — much worse than `--` eating one line
- `/*` looks like `/` then `*` (divide then multiply) — real parsing ambiguity in prefix notation
- More tokens to generate (at least 2: `/*` and `*/`)
- Nesting `/* /* */ */` is a classic footgun (does inner `*/` close outer?)

## Option D: Keep `--` but add `#` as single-line alternative

Support both. `--` for backward compat, `#` as the shorter form.

**Cons:**
- Two ways to do the same thing — violates "one way to do things" principle
- More lexer rules

## The `--` ambiguity in practice

How likely is `--x 1` in real ilo code? In prefix notation, nested operators are common:

```
+*a b c     -- (a * b) + c       ✓ fine
-*a b *c d  -- (a * b) - (c * d) ✓ fine
--a b       -- COMMENT, not -(a - b)  ✗ gotcha
```

The pattern `-<op>` is fine for any operator except `-` itself. So the ambiguity only hits one specific case: negating a subtraction. Workarounds exist (`- -a b` or bind-first), but it's still surprising.

## Recommendation

`--` is good enough. The ambiguity is narrow (only `-` after `-`), the workarounds are simple, and changing comment syntax at this point would churn every example in the SPEC, README, and research docs.

If the ambiguity proves painful in practice, `#` is the cleanest replacement — no operator conflicts, saves a character, widely understood.

Multi-line comments (`/* */`) are not recommended due to the parsing ambiguity with `/*` in prefix notation and the unclosed-comment footgun.
