# TODO

## What's next (uncompleted work, priority order)

1. **Tool provider infrastructure** (D1d) — `ToolProvider` trait, HTTP provider, tool config
2. **Value ↔ JSON** (D1e) — serialise/deserialise ilo values at tool boundary
3. **JSON parsing** (I1) — `jp` builtin, agents live in JSON
4. **Shell execution** (I2) — `run` builtin + backtick syntax
5. ~~**Env vars**~~ ✅ (I3) — `env` builtin
6. **Logging** (I5) — `log`/`dbg` to stderr
7. **HTTP methods** (G1) — `post`, `put`, `patch`, `del`
8. **Cranelift JIT gaps** — nil coalesce, safe nav, while, break/continue, range, early return
9. ~~**Verifier gaps**~~ ✅ — unreachable code warning (`ILO-T029`) and `brk`/`cnt` outside loop (`ILO-T028`) both implemented
10. **Optional type** (E2) — typed nullability with `O n`
11. ~~**Destructuring bind**~~ ✅ (F8) — `{a;b}=expr`

See detailed specs for each below.

---

## Sigil changes (do first — unblocks other work)

- [x] Decide Err-wrap sigil to replace `!` → chose `^` (caret)
- [x] Reassign `!x` → logical NOT (`UnaryOp::Not`, `OP_NOT` already in AST/VM)
- [x] Update SPEC.md, example `.ilo` files, README with new sigils

## Basics — complete what's already there

### Parser gaps (AST/VM support exists, no parser production)

- [x] List literals `[a, b, c]` — parser production added, connects to existing `Expr::List` and `OP_LISTNEW`
- [x] Unary negation `-x` — `UnaryOp::Negate` in AST, parser now disambiguates: `-x` = negate, `-x y` = subtract
- [x] Logical NOT `!x` — parser production added, connects to existing `UnaryOp::Not` and `OP_NOT`

### Missing fundamental operators

- [x] Logical AND `&a b` — short-circuit jump sequence (JMPF), no new opcode needed
- [x] Logical OR `|a b` — short-circuit jump sequence (JMPT), no new opcode needed
- [x] String comparison `<` `>` `<=` `>=` — lexicographic comparison on text values in VM + interpreter

### Builtins (new opcodes — keep dispatch O(1), JIT-eligible where numeric)

Note: all builtin names are single tokens (no hyphens — manifesto: "every hyphen doubles token cost").

- [x] `len x` — length of string (bytes) or list
- [x] `+=x v` — append single value to list, return new list
- [x] `+a b` — extend to lists: concatenate two lists (already handles `n` add and `t` concat)
- [x] Index access `x.0`, `x.1` — by integer literal (dot notation, consistent with field access)
- [x] `str n` — number to text
- [x] `num t` — text to number (returns `R n t`, Err if unparseable)
- [x] `abs n` — absolute value
- [x] `min a b` — minimum of two numbers
- [x] `max a b` — maximum of two numbers
- [x] `flr n` — floor
- [x] `cel n` — ceil

## Verification

Manifesto principle: "Verification before execution. All calls resolve, all types align, all dependencies exist."

- [x] Type verifier — check all call sites resolve to known functions with correct arity
- [x] Match exhaustiveness — warn when match has no wildcard arm and not all cases covered (see OPEN.md)
- [x] Arity check at call sites — covered by type verifier (static check at all call sites)

## Tooling

- [x] Pretty-printer / formatter — dense wire format for LLM I/O, expanded form for human review (see OPEN.md: "Hybrid approach")
- [x] Rename formatter flags: `--dense` / `-d` and `--expanded` / `-e`. `--fmt` / `--fmt-expanded` kept as aliases
- [ ] `--expanded` wraps long comments at ~80 chars, adding `--` prefix on continuation lines
- [ ] `--dense` strips unnecessary newlines within comments — long comments stay on one line

## Error messages — Phase B (infrastructure + rendering)

Gives spans, structured diagnostics, and dual-mode output (human + machine).

### B1. Span infrastructure ✓
- [x] Add `Span { start: usize, end: usize }` type to AST module
- [x] Lexer: attach `Span` to every token (already has byte `position`, extend to start/end)
- [x] Parser: attach `Span` to every `Expr`, `Stmt`, `Decl`, `Pattern`, `MatchArm` node
- [x] Source map helper: byte offset → line:col conversion (store original source or line start offsets)

### B2. Diagnostic data model ✓
- [x] `Diagnostic` struct: severity, code, message, primary span, secondary spans (with labels), suggestion (optional), notes
- [x] `Severity` enum: Error, Warning, Hint
- [x] `Suggestion` struct: message, replacement text, span, confidence (MachineApplicable / MaybeIncorrect)
- [x] Collect diagnostics into a `Vec<Diagnostic>` instead of returning early on first error

### B3. Renderers ✓
- [x] Human renderer (ANSI): header line, `-->` location, gutter + source lines, labeled underlines (`^^^`), colored by severity
- [x] JSON renderer: structured output matching the Diagnostic model, one JSON object per diagnostic
- [x] Auto-detect: TTY → ANSI, piped → JSON. Override with `--json`/`-j`, `--text`/`-t`, `--ansi`/`-a` (mutually exclusive, error if multiple)
- [x] Respect `NO_COLOR` env var
- [x] Show full function source in errors (leverage ilo's density — whole function fits in one line)

### B4. Wire up existing errors ✓
- [x] Lexer errors → Diagnostic with span
- [x] Parser errors → Diagnostic with span (C1 error recovery — multi-error)
- [x] Verifier errors → Diagnostic with span
- [x] Interpreter runtime errors → Diagnostic (no span — deferred to C4)
- [x] VM runtime errors → Diagnostic (no span — deferred to C4)

## Error messages — Phase C (polish, do after grammar stabilises)

After language features settle. 

### C1. Error recovery ✓
- [x] Parser: continue after errors using panic-mode recovery (sync on `;`, `}`, `>`, next decl keyword)
- [x] Poison AST nodes: mark failed parses as error nodes, suppress cascading errors in verifier
- [x] Report multiple errors per file (cap at ~20 to avoid noise)
- [x] Verifier: analyse all functions even if earlier ones have errors

### C2. Error codes ✓
- [x] Assign stable codes: `ILO-L___` (lexer), `ILO-P___` (parser), `ILO-T___` (type/verifier), `ILO-R___` (runtime)
- [x] Error code registry: catalogue of all codes with short description
- [x] `--explain ILO-T001` flag: print expanded explanation with examples
- [x] Include code in both human and JSON output

### C3. Suggestions and Fix-Its ✓
- [x] "Did you mean?" for undefined variables/functions — Damerau-Levenshtein, threshold `max(1, len/3)`, scope-aware
- [x] Type mismatch suggestions — e.g. "use `num` to convert text to number"
- [x] Missing pattern arm suggestions — list the uncovered cases
- [x] Arity mismatch — show expected vs actual signature
- [x] Cross-language syntax detection — detect `===`, `&&`, `||`, `function`, `def`, `fn` and suggest ilo equivalents

### C4. Runtime source mapping ✓
- [x] Compiler: emit instruction-to-span table alongside bytecode
- [x] VM: on error, look up current instruction pointer in span table
- [x] Interpreter: thread current Stmt/Expr span through evaluation for error context
- [x] Stack trace with source locations for nested function calls

## Python codegen

- [x] Fix lossy match arm codegen — let bindings in match arms are silently dropped when emitted as ternaries

---

## Agent integration — Phase D (make ilo programs do things)

Manifesto: "a minimal, verified action space." The language verifies and executes locally — Phase D connects it to the outside world.

### D1. Tool Execution (foundation)

Plumbing first — make tool calls actually do things. HTTP-native (tools are APIs, not scripts).

#### D1a. Fix existing bugs ✅
- [x] Fix VM empty-chunk bug — tool `Decl` compiles stub chunk (LOADK Nil → WRAPOK → RET), returns `Ok(Nil)` matching interpreter
- [x] Fix VM test helper `parse_program` discarding token spans (broke auto-unwrap `!` adjacency check)

#### D1b. HTTP builtin: `get` / `$`
- [x] `get url` / `$url` — built-in HTTP GET, returns `R t t` (ok=response body, err=error message)
- [x] `$` is the terse alias for `get` (same AST node, like `help`/`-h`)
- [x] Respects the curl model: one thing in, one thing out, composes with everything
- [x] 3 chars / 1 token. `d=get url;d.name` beats `curl -s url | jq '.name'`

#### D1c. Auto-unwrap operator `!` ✅
- [x] `get! url` — unwrap ok value, propagate err to caller (like Rust's `?`)
- [x] Works on any `R` return type, not just `get`
- [x] `d=get! url;d.name` — 18 chars, verified, error-handled
- [x] Without `!`, use explicit match: `get url;?{~d:d.name;^e:^e}`

#### D1d. Tool provider infrastructure
- [ ] Add `tokio` + `reqwest` as deps behind a `tools` feature flag
- [ ] `ToolProvider` trait — async executor interface: `async fn call(&self, name: &str, args: Vec<Value>) -> Result<Value, ToolError>`
- [ ] `HttpProvider` — tool name maps to an HTTP endpoint. JSON request/response. Respects `timeout` and `retry` from `tool` decl
- [ ] `StubProvider` — current behaviour (`Ok(Nil)`), used in tests and when no provider configured
- [ ] Wire into interpreter + VM — both backends receive an `Option<&dyn ToolProvider>`, tool calls dispatch through it
- [ ] Tool config — `ilo program.ilo --tools tools.json` where `tools.json` maps tool names to URLs/endpoints

#### D1e. Value ↔ JSON at tool boundary
- [ ] `Value::to_json()` — serialise ilo values to JSON for tool call args
- [ ] `Value::from_json(type_hint)` — deserialise JSON to ilo values, guided by the tool's declared return type
- [ ] JSON objects → records (if matching type declared), JSON arrays → lists, primitives → n/t/b, null → nil
- [ ] Unknown/complex shapes: tool declares `>t`, gets raw JSON string — passable to another tool without parsing
- [ ] Format parsing is a tool concern, not a language concern (see OPEN.md)

#### D1f. Tests
- [ ] Mock HTTP server for integration tests
- [ ] `StubProvider` for unit tests
- [ ] Test `get`/`$` with real HTTP (httpbin or similar)

### D2. MCP Integration

- [ ] MCP client: connect to MCP servers, discover tools, call them (builds on D1 async infra)
- [ ] `ilo run program.ilo --mcp server.json` — load tool signatures from MCP server config
- [ ] Auto-populate tool declarations from MCP server discovery (graph loading option 3: query on demand)

### D3. Tool Discovery & Progressive Disclosure

- [ ] `ilo tools` — list available tools from configured sources
- [ ] `ilo tools --mcp server.json` — discover and display tool signatures
- [ ] Progressive disclosure: tool names first (cheap), full signatures on demand
- [ ] Tool graph: which tools depend on which types, what produces what

### D4. Agent Loop

- [ ] `ilo serve` — stdio-based agent loop (read task → generate program → verify → execute → return result)
- [ ] JSON protocol for agent integration (task in, result out, errors structured)
- [ ] The "typed shell" mode: interactive tool composition with verification

### Not yet (deferred)

#### Language hardening
- Reserve keywords at lexer level — `if`, `return`, `let`, `fn`, `def`, `var`, `const` are currently valid identifiers (only caught as hints at declaration position). Reserving them protects future design space and prevents confusing programs (e.g. a function named `return`). Low urgency while user base is small.
- Parser body boundary — newlines are filtered and `at_body_end()` only checks `None | RBrace`, so a bare `Ref` at the end of a function body greedily consumes the next declaration's name. Workaround: wrap in parens `~(func! x)`. A proper fix would use newlines as declaration separators in file mode.

#### Control structures — Phase F (expand control flow)

Ranked by token savings × frequency. See [research/CONTROL-FLOW.md](research/CONTROL-FLOW.md) for full research (Perl, Ruby, Bash, APL/K, Haskell, Elixir, Rust, Awk, Forth).

##### F0. Braceless single-expression guards (highest frequency — every guard benefits)

ilo operators have fixed arity, so the parser always knows when a condition expression is complete. This means single-expression guard bodies don't need `{}` — the parser can tell where the condition ends and the body begins. Inspired by Ruby/Perl postfix conditionals, but exploiting prefix notation's self-delimiting property.

- [x] Syntax: `>=sp 1000 "gold"` — braces optional when guard body is a single expression. Multi-statement bodies still require braces
- [x] Parser: after parsing a complete condition expression, if next token is NOT `{` and NOT `;`, parse one expression as the guard body
- [x] Scope: braceless bodies are single expressions — atoms, prefix operators, ok/err wraps (`~x`, `^"err"`)
- [x] Conservative limit: function calls in braceless guards may be ambiguous (unknown arity at parse time) — require braces for call bodies: `>=sp 1000{classify sp}`
- [x] Negated guards: `!verified "not ok"` — `!verified` is complete unary, `"not ok"` is body
- [x] No AST changes — parsed into the same `Stmt::Guard` node, body is a single-expression `Vec<Spanned<Stmt>>`
- [x] Interpreter: no changes — guard body evaluation is the same regardless of brace syntax
- [x] VM: no changes — same compilation path
- [x] Formatter: `--fmt` emits braceless form for single-expression guards; `--fmt-expanded` emits braced form
- [x] **Ambiguity detection & hints (critical — prevents retries):**
  - [x] Detect dangling tokens after braceless guard body: if parser consumes a single identifier as guard body but the next token is NOT `;`, `}`, or EOF, emit a hint
  - [x] Hint text: `"function calls in braceless guards need braces: >=sp 1000{classify sp}"` — uses `error_hint()` infrastructure (already exists for `&&`→`&`, `->`→`>`, etc.)
  - [x] Verifier cross-check: if braceless guard body is a single identifier that matches a known function name, emit warning `"did you mean to call '<name>'? Use braces: cond{<name> args}"` — reuses existing Levenshtein/scope-aware suggestion system
  - [x] JSON error output: hint appears in `suggestion` field so agent tooling can auto-fix
  - [x] `--explain` entry for the error code: show braceless vs braced examples, explain when braces are required
  - [x] Test: `>=sp 1000 classify sp` → error with hint mentioning braces. `>=sp 1000 classify;` → valid (classify is a variable ref, not a call). Agent gets actionable fix on first error, no retry needed
- [x] Tests: braceless guard with literal, with operator expression, with ok/err wrap, with variable ref; multi-statement still requires braces; negated braceless guard; mixed braceless and braced in same function
- [x] SPEC.md: document optional braces for single-expression guards, with explicit note on when braces are required

**Token comparison:**
```
# Current:                            26 chars, braces on each guard
cls sp:n>t;>=sp 1000{"gold"};>=sp 500{"silver"};"bronze"

# Proposed:                           22 chars — saves 4 chars, 4 tokens
cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze"
```

**Why it works:** `>=sp 1000` takes exactly 2 operands — the parser knows it's complete. The next token (`"gold"`) must be the guard body. This disambiguation is free in prefix notation but impossible in infix.

See [research/CONTROL-FLOW.md](research/CONTROL-FLOW.md) § F0 for full analysis.

##### F1. Ternary / guard-else expression (highest priority — no new opcodes)

Guards return from the function. There's no expression-level conditional that stays local. Match-as-expression works but costs 5+ tokens for a simple if/else.

- [x] Syntax: `cond{then}{else}` — two adjacent brace bodies after a condition expression
- [x] Semantics: evaluate condition; if truthy, evaluate first body; if falsy, evaluate second body. Does NOT return from function (unlike guard)
- [x] Parser: detect second `{` after guard body close `}`. If present, parse as ternary expression; if absent, parse as guard (existing behaviour)
- [x] AST: reuse `Stmt::Guard` with `else_body: Option<Vec<Spanned<Stmt>>>`
- [x] Interpreter: evaluate condition, push scope for chosen branch, evaluate body, pop scope
- [x] VM: compile as `JMPF cond → else_label; <then body>; JMP end; else_label: <else body>; end:`
- [x] Verifier: both branches verified
- [x] Python codegen: emit as `if/else` blocks
- [x] Tests: basic ternary, no-early-return, negated ternary, guard unchanged
- [x] SPEC.md: document guard-else syntax, contrast with guard (returns) vs ternary (local)

**Token comparison:**
```
# Current (match-as-expression):  8 tokens
r=?{=x 1:a;_:b}

# Proposed (guard-else):          5 tokens — saves 3
r==x 1{a}{b}
```

##### F2. Nil-coalescing operator — `??` (high priority — expression-level)

Handles the most common Optional/nil pattern: "use this value, or fall back to a default."

- [x] Syntax: `a??b` — evaluate `a`; if nil, evaluate `b`; otherwise return `a`
- [x] Parser: recognise `??` as infix operator (parsed between `maybe_with` and `maybe_pipe`)
- [x] AST: `Expr::NilCoalesce { value, default }`
- [x] Interpreter: evaluate left; if `Value::Nil`, evaluate right; otherwise return left
- [x] VM: `OP_JMPNN` (jump if not nil) — compile left → JMPNN skip → compile right → MOVE → skip:
- [x] Verifier: if left is Nil, result is right type; otherwise left type
- [ ] Works with Optional (E2): `O n ?? 0` → unwrap to `n` with default `0`
- [x] Works without Optional: `val ?? "fallback"` — runtime nil check even without typed Optional
- [ ] Cranelift JIT: nil comparison + conditional move
- [x] Python codegen: emit as `(a if a is not None else b)`
- [x] Tests: nil coalesce, non-nil passthrough, nested coalesce `a ?? b ?? c`
- [x] SPEC.md: document `??` operator

**Token comparison:**
```
# Current:                        7 tokens
?v{_:"default";~x:x}

# Proposed:                       2 tokens — saves 5
v??"default"
```

**Inspiration:** Perl `//`, C# `??`, Kotlin `?:`, Swift `??`, JS `??`.

##### F3. Safe field navigation — `.?` (high priority — prevents nil crashes)

Chained field access on possibly-nil values without nested matches. Short-circuits at first nil.

- [x] Syntax: `x.?field` — if `x` is nil, return nil; otherwise return `x.field`
- [x] Parser: `DotQuestion` token, handled alongside `Dot` in field access chain
- [x] AST: `safe: bool` flag on `Expr::Field` and `Expr::Index`
- [x] Interpreter: check if receiver is `Value::Nil`; if so, return `Value::Nil`
- [x] VM: `OP_JMPNN` + `OP_JMP` to skip field access on nil, in-place result register
- [x] Verifier: if object is nil type, result is nil
- [x] Chaining: `u.?addr.?city` — each `.?` propagates nil via sequential nil checks
- [ ] Cranelift JIT: nil comparison + conditional load
- [x] Python codegen: emit as `(x["field"] if x is not None else None)`
- [x] Tests: safe access on nil, safe access on value, chained safe access
- [x] SPEC.md: document `.?` operator

**Token comparison:**
```
# Current (nested match):         ~15 tokens
?u{_:_;~u:?u.addr{_:_;~a:a.city}}

# Proposed (safe nav):            3 tokens — saves 12
u.?addr.?city
```

**Inspiration:** Ruby `&.`, Kotlin `?.`, TypeScript `?.`, C# `?.`.

##### F4. Pipe operator — `>>` ✅

Linear chains of calls without naming intermediates. Desugars at parse time.

- [x] Syntax: `f x>>g>>h` — result of left side becomes last argument of right side
- [x] Lexer: `>>` token (`Token::PipeOp`)
- [x] Parser: `maybe_pipe()` desugars `expr >> func args` to `func(args..., expr)`
- [x] AST: no new node — desugars to `Expr::Call`
- [x] Interaction with `!`: `f x>>g!>>h` — each step can auto-unwrap independently
- [x] Tests: parser, interpreter, VM (simple, chain, extra args)
- [x] SPEC.md: documented `>>` operator

##### F5. Early return — `ret expr` ✅

Explicit return from anywhere in function body.

- [x] Syntax: `ret expr` — immediately return `expr` from the enclosing function
- [x] Parser: recognise `ret` keyword at statement position, parse following expression
- [x] AST: `Stmt::Return(Expr)`
- [x] Interpreter: return `BodyResult::Return(value)` immediately
- [x] VM: compile expression → register, emit `OP_RET register`
- [x] Verifier: return type checked via `infer_expr`
- [x] Verifier: warn on unreachable code after `ret` (`ILO-T029`)
- [ ] Cranelift JIT: straightforward — emit return instruction
- [x] Python codegen: emit as `return expr`
- [x] Formatter: emit as `ret expr`
- [x] Tests: parser, interpreter, VM, verifier, codegen
- [x] SPEC.md: documented `ret` syntax

##### F6. While loop — `wh cond{body}` ✅

`@` only iterates lists. While enables polling, convergence, and stateful loops.

- [x] Syntax: `wh cond{body}` — evaluate condition; if truthy, execute body; repeat
- [x] Parser: recognise `wh` keyword, parse condition expression + brace body
- [x] AST: `Stmt::While { condition: Expr, body: Vec<Spanned<Stmt>> }`
- [x] Interpreter: loop while `is_truthy(eval_expr(condition))`, execute body each iteration
- [x] VM: compile as `loop_top: eval cond → reg; JMPF reg → exit; <body>; JMP loop_top; exit:`
- [x] VM fix: `Stmt::Let` re-binding writes to existing register (needed for loop counters)
- [x] Verifier: condition + body type checked
- [ ] Cranelift JIT: standard loop with conditional back-edge
- [x] Interaction with break/continue (F9): `brk` jumps to exit, `cnt` jumps to loop_top
- [x] Python codegen: emit as `while <cond>: <body>`
- [x] Formatter: emit as `wh cond{body}`
- [x] Tests: parser, interpreter (basic, zero-iter, ret), VM (basic, zero-iter, ret)
- [x] SPEC.md: documented `wh` syntax

##### F7. Range iteration — `@i 0..n{body}` ✅

Index-based loops without constructing a list. Avoids list allocation for numeric ranges.

- [x] Syntax: `@i 0..n{body}` — bind `i` to each integer in `[0, n)`
- [x] Parser: recognise `..` between two numeric expressions in `@` collection position
- [x] AST: `Stmt::ForRange { binding, start, end, body }`
- [x] Interpreter: iterate from start (inclusive) to end (exclusive), bind each integer to loop variable
- [x] VM: integer counter with `OP_ADD` + `OP_LT`, no list allocation
- [x] Verifier: start and end must be `n`; loop variable is `n`
- [x] Dynamic end: `@i 0..n{body}` — end expression evaluated once before loop starts
- [ ] Step variant (deferred): `@i 0..10..2{body}` for step=2 — lower priority
- [ ] Cranelift JIT: standard counted loop — optimal for JIT
- [x] Python codegen: emit as `for i in range(start, end):`
- [x] Formatter: emit as `@i 0..n{body}` in dense and expanded modes
- [x] Tests: basic range, range with expressions, range variable in body, empty range (start >= end), range + break/continue
- [x] SPEC.md: documented range syntax

##### F8. Destructuring bind — `{a;b}=expr` ✅

Extract multiple record fields into local variables in one statement.

- [x] Syntax: `{a;b;c}=expr` — bind `a` to `expr.a`, `b` to `expr.b`, `c` to `expr.c`
- [x] Field names match variable names (ilo convention: short field names)
- [x] Parser: recognise `{` at statement start followed by identifiers + `}=`
- [x] AST: add `Stmt::Destructure { bindings: Vec<String>, value: Expr }`
- [x] Interpreter: evaluate expression, extract each named field, bind to scope
- [x] VM: compile expression → register, emit `OP_RECFLD` for each binding
- [x] Verifier: expression must be a record type with all named fields present. Bind each variable to its field's type
- [ ] Renaming syntax (deferred): `{name:n;email:e}=expr` — bind `n` to `expr.name`. Lower priority
- [ ] Cranelift JIT: sequence of field loads from record
- [x] Python codegen: emit as `a = p["a"]; b = p["b"]`
- [x] Formatter: `{a;b}=expr` (dense) / `{a;b} = expr` (expanded)
- [x] Tests: basic destructure, missing field error, type inference from fields, destructure in loop body
- [x] SPEC.md: document destructuring syntax

**Token comparison:**
```
# Current:                        9 tokens
r=get-user! id;n=r.name;e=r.email

# Proposed:                       5 tokens — saves 4
{name;email}=get-user! id
```

##### F9. Break/continue — `brk` / `cnt` ✅

Exit a loop early or skip to the next iteration.

- [x] Syntax: `brk` — exit enclosing `@` or `wh` loop immediately; `cnt` — skip to next iteration
- [x] `brk expr` — exit loop with optional value
- [x] Parser: recognise `brk` and `cnt` as statement keywords inside loop bodies
- [x] AST: `Stmt::Break(Option<Expr>)` and `Stmt::Continue`
- [x] Interpreter: `BodyResult::Break(Value)` / `BodyResult::Continue` propagation through guard, match, foreach, while
- [x] VM: `LoopContext` with `loop_top`, `continue_patches`, `break_patches`. `brk` → JMP to exit; `cnt` → JMP to loop_top (while) or idx increment (foreach)
- [x] Verifier: type inference for `brk expr`
- [x] Verifier: `brk`/`cnt` outside a loop → error (`ILO-T028`)
- [ ] Cranelift JIT: jump to loop exit / loop header
- [x] Python codegen: emit as `break` / `continue`
- [x] Formatter: emit as `brk` / `brk expr` / `cnt`
- [x] Tests: break from @, break from wh, continue in @/wh, break with value
- [x] SPEC.md: documented `brk`/`cnt` syntax

##### F10. Guard else — `cond{then}{else}` as statement ✅

Unified with F1 — ternary `cond{a}{b}` works at both expression and statement level.

##### F11. Type pattern matching — `?x{n v:...; t v:...}` (lower priority)

Branch on runtime type of a value. Needed when tools return `t` as escape hatch for unknown shapes.

- [ ] Syntax: `?x{n v:body; t v:body; b v:body; _:body}` — match on runtime type, bind to typed variable
- [ ] Parser: recognise type names (`n`, `t`, `b`, `L`, `R`) in pattern position, followed by binding name
- [ ] AST: add `Pattern::Type { type_tag: Type, binding: String }`
- [ ] Interpreter: check runtime type of value, bind to variable with narrowed type
- [ ] VM: emit type-tag comparison opcodes (check Value discriminant)
- [ ] Verifier: variable `v` in `n v:body` has type `n` within the arm body. Exhaustiveness check across type tags
- [ ] Interaction with sum types (E3): type patterns on enum variants are a generalization
- [ ] Cranelift JIT: discriminant comparison + conditional jump
- [ ] Python codegen: emit as `isinstance(x, int)` / `isinstance(x, str)` checks
- [ ] Tests: match on all scalar types, match on list/result, wildcard arm, exhaustiveness
- [ ] SPEC.md: document type patterns

##### F12. Reduce operator (deferred — gates on E5 generics)

Fold a list with an operator. The single most token-saving list operation from APL/K.

- [ ] Syntax: `fld op init list` — fold list with binary operator, starting from init value
- [ ] Examples: `fld + 0 xs` (sum), `fld * 1 xs` (product), `fld max 0 xs` (maximum)
- [ ] Requires E5 (generics) to type-check: `fld` has signature `fn(fn(a a>a), a, L a) > a`
- [ ] Alternative (non-generic): hardcode `fld` for common operators (`+`, `*`, `max`, `min`) — monomorphic special cases
- [ ] See Builtins section for full `map`/`flt`/`fld` signatures
- [ ] **Gates on E5** unless implemented as monomorphic special cases

**Token comparison (APL-inspired):**
```
# Current:                        8 tokens
s=0;@x xs{s=+s x};s

# Proposed:                       4 tokens — saves 4
fld + 0 xs
```

#### Type system — Phase E (expand the type language)

Manifesto: "constrained — small vocabulary, closed world, one way to do things." Each addition must justify its token cost.

##### E1. Type aliases ✅

Lets users name complex types without creating records. No new AST nodes at runtime, just resolution at parse/verify time.

- [x] Syntax: `alias name type` as a new `Decl` variant — e.g. `alias res R n t`, `alias ids L n`
- [x] Parser: recognise `alias` keyword at declaration position, parse name + type
- [x] AST: `Decl::Alias { name: String, target: Type, span: Span }`
- [x] Verifier: resolve aliases during declaration collection — expand `Named("res")` → `Result(Number, Text)` before body verification
- [x] Cycle detection — `alias a b` + `alias b a` errors
- [x] Error messages: show alias name in user-facing messages, expanded form in notes
- [x] Formatter: emit `alias` declarations in both dense and expanded formats
- [x] Python codegen: emit type alias as comment
- [x] Tests: alias in function signatures, nested aliases, cycles, shadowing
- [x] SPEC.md: documented alias syntax

##### E2. Optional type (typed nullability)

Currently `_` (nil) can appear anywhere at runtime with no type-level protection. `O n` means "number or nil" — verifier forces handling before use.

- [ ] Syntax decision: `O n` (prefix, consistent with `L`/`R`) vs `n?` (postfix, familiar). Recommend `O n` for consistency
- [ ] AST: add `Type::Optional(Box<Type>)` — parsed as `O <type>`
- [ ] Parser: recognise `O` as type constructor (like `L`, `R`), parse inner type
- [ ] Verifier `Ty` enum: add `Ty::Optional(Box<Ty>)` with conversion from AST
- [ ] Verifier: `O n` is not compatible with `n` — must unwrap first
- [ ] Unwrap via match: `?opt{~v:use v;_:default}` — reuse existing `~`/`_` match arms for Some/None
- [ ] Unwrap via `!`: `f! x` on `O n` return → propagate nil to caller (requires caller returns `O` too)
- [ ] Auto-wrap: assigning `n` to `O n` variable is allowed (implicit wrap, like `~x` for Result)
- [ ] Nil literal `_` has type `O T` for any T (compatible with any optional)
- [ ] Verifier: field access on `O record` must error — "unwrap optional before accessing fields"
- [ ] Interpreter: `Value::Nil` already exists — Optional is purely a type-level distinction, no new runtime value
- [ ] VM: no new opcodes needed — nil is already a value, Optional is compile-time only
- [ ] Cranelift JIT: no changes needed (type erasure at JIT level)
- [ ] Error codes: `ILO-T0xx` for "cannot use Optional as T without unwrapping", "field access on optional type"
- [ ] Match exhaustiveness: `O n` requires both `~v` and `_` arms (like `R` requires `~`/`^`)
- [ ] Python codegen: emit `Optional[int]` / `x if x is not None else ...`
- [ ] Tests: optional params, optional returns, nested `O O n` (should warn or flatten?), match exhaustiveness, auto-wrap, `!` propagation
- [ ] SPEC.md: document Optional type, unwrap patterns

##### E3. Sum types / tagged unions (user-defined variants)

Generalise `R ok err` — users define their own closed set of variants. `R` becomes sugar for a two-variant sum.

- [ ] Syntax decision: `enum name{a:type;b:type;c}` or `type name S a:type b:type c` — needs to declare variant names and optional payload types
- [ ] AST: add `Decl::Enum { name, variants: Vec<Variant>, span }` where `Variant { name, payload: Option<Type> }`
- [ ] Parser: recognise enum declaration syntax, parse variant list
- [ ] AST `Type`: enum types referenced by name (already `Type::Named` — enums are named types)
- [ ] Verifier: register enum in type environment during declaration collection
- [ ] Verifier: variant construction — `name.variant payload` or `variant payload` (scoping decision needed)
- [ ] Verifier: match on enum — require exhaustive coverage of all variants
- [ ] Runtime `Value`: add `Value::Variant { type_name, variant_name, payload: Option<Box<Value>> }` — or reuse tagged approach
- [ ] Interpreter: construct variants, match on variant name
- [ ] VM: opcodes for variant construction (`OP_VARIANT`), variant tag check in match dispatch
- [ ] Cranelift JIT: variant representation (tagged pointer or tag + payload pair)
- [ ] Subsumption: `R ok err` could desugar to `enum R{ok:ok_type;err:err_type}` internally — or keep `R` as special syntax that compiles to enum
- [ ] Error codes: `ILO-T0xx` for non-exhaustive enum match, unknown variant name, wrong payload type
- [ ] Formatter: emit enum declarations in dense/expanded format
- [ ] Python codegen: emit as `@dataclass` variants or `Enum` subclasses
- [ ] Tests: define enum, construct variants, match all arms, exhaustiveness errors, nested enums, enum in lists/results
- [ ] SPEC.md: document enum syntax, construction, matching

##### E4. Map type (dynamic key-value collections)

Records are fixed-shape (schema known at compile time). Maps are dynamic — keys determined at runtime. Needed for tool responses with variable keys.

- [ ] Syntax: `M key_type val_type` — e.g. `M t n` for string-to-number map
- [ ] AST: add `Type::Map(Box<Type>, Box<Type>)`
- [ ] Parser: recognise `M` as type constructor, parse key and value types
- [ ] Verifier `Ty` enum: add `Ty::Map(Box<Ty>, Box<Ty>)`
- [ ] Runtime `Value`: add `Value::Map(BTreeMap<String, Value>)` or `HashMap` — key type constrained to `t` initially? Or allow `n` keys?
- [ ] Interpreter: construct maps, access by key, iterate
- [ ] VM: opcodes — `OP_MAPNEW`, `OP_MAPGET`, `OP_MAPSET`, `OP_MAPHAS`
- [ ] Builtins: `get k m` → value lookup (returns `O v` — key might not exist), `has k m` → bool, `keys m` → `L t`, `vals m` → `L v`
- [ ] Map literal syntax decision: `{k:v;k:v}` conflicts with record construction — maybe `[k=v;k=v]` or `M{k:v;k:v}`
- [ ] Verifier: type-check key/value types at construction and access
- [ ] Foreach: `@k m{...}` iterates keys, `@kv m{...}` iterates key-value pairs (syntax TBD)
- [ ] JSON mapping: JSON objects with variable keys → `M t value_type`
- [ ] Cranelift JIT: map operations likely interpreter-only (too complex for JIT)
- [ ] Python codegen: emit as `dict[str, int]` etc.
- [ ] Tests: construct maps, access keys, missing key returns, iteration, type checking, JSON round-trip
- [ ] SPEC.md: document Map type, access patterns, builtins

##### E5. Generic functions (parametric polymorphism)

Unlocks `map`, `filter`, `fold` as typed builtins. Without generics, these are either untyped or need per-type variants.

- [ ] Syntax decision: type variables as lowercase single letters — `a`, `b` in type position. `map f:fn(a>b) xs:L a>L b`
- [ ] Function type syntax: `fn(param_types>return_type)` for higher-order function params
- [ ] AST `Type`: add `Type::Var(String)` for type variables, `Type::Fn(Vec<Type>, Box<Type>)` for function types
- [ ] Parser: recognise type variables (lowercase single-char in type position), parse `fn(...)` type
- [ ] Verifier: generic function verification — collect type variables from signature, unify at call sites
- [ ] Type unification: when calling `map f xs`, unify `a` with element type of `xs`, `b` with return type of `f`
- [ ] Monomorphisation vs erasure: decide strategy — monomorphise at verify time (simpler) or erase at runtime (all values already boxed, so erasure is natural)
- [ ] Verifier: reject ambiguous type variables that can't be inferred from arguments
- [ ] Lambda / anonymous function syntax (needed for `map`, `filter`): `\x>*x 2` or `{x>*x 2}` — syntax TBD
- [ ] Interpreter: pass function values (closures) as arguments — `Value::Closure` or `Value::FnRef`
- [ ] VM: `OP_CALL_INDIRECT` or similar for calling function-typed values
- [ ] Builtin generic functions: `map`, `flt`, `fld` (see Builtins section) — defined with generic signatures
- [ ] Cranelift JIT: function pointers / indirect calls for higher-order functions
- [ ] Error codes: `ILO-T0xx` for "cannot infer type variable", "function type mismatch"
- [ ] Python codegen: emit `TypeVar`, generic function signatures
- [ ] Tests: generic identity function, map over list, filter, fold, nested generics, inference failures
- [ ] SPEC.md: document type variables, function types, generic builtins

##### E6. Traits / interfaces (shared behaviour — lowest priority)

Define behaviour shared across record types. Lowest priority — agents generate concrete code, not abstract frameworks.

- [ ] Syntax decision: `trait name{method:fn(self>return_type)}` or similar
- [ ] AST: `Decl::Trait { name, methods }`, `Decl::Impl { trait_name, type_name, methods }`
- [ ] Verifier: check trait implementations satisfy all required methods with correct signatures
- [ ] Verifier: trait bounds on generic type variables — `f x:a>t where a:Printable`
- [ ] Runtime dispatch: static (monomorphised) vs dynamic (vtable) — static preferred for token efficiency
- [ ] Error codes: missing method, wrong signature, duplicate impl
- [ ] Tests: define trait, implement for multiple types, use in generic context
- [ ] SPEC.md: document trait syntax
- [ ] **Gate on E5** — traits require generics to be useful

#### Builtins
- `map f xs` — apply function to each list element (currently requires `@` loop + accumulator)
- `flt f xs` / `filter` — filter list by predicate
- `fld f init xs` / `reduce` — fold/reduce list to single value
- ~~`rev xs`~~ ✅
- ~~`srt xs`~~ ✅
- ~~`hd xs` / `tl xs`~~ ✅
- ~~`slc xs a b`~~ ✅
- ~~`has xs v`~~ ✅
- ~~`cat xs sep`~~ ✅
- ~~`spl t sep`~~ ✅
- `get k m` — get value from map by key (if maps are added)
- ~~`rnd` / `rnd a b`~~ ✅
- ~~`now()`~~ ✅

#### Tooling
- LSP / language server — completions, diagnostics, hover info for editor integration
- REPL — interactive evaluation for exploration and debugging
- Debugger — step through execution, inspect bindings at each statement
- Playground — web-based editor with live evaluation (WASM target)

---

## Future research (designs captured, not yet prioritised)

See `research/` for detailed exploration docs.

#### Networking — Phase G (expand I/O beyond HTTP GET)

Current state: `get`/`$` does synchronous HTTP GET via `minreq`. That's all. The following items expand networking to support richer agent interactions (browser automation, bidirectional communication, full HTTP methods).

##### G1. HTTP methods beyond GET

`get` only does GET. Agents calling APIs need POST/PUT/PATCH/DELETE with bodies and headers.

- [ ] `post url body` — HTTP POST, returns `R t t`. Body is text (JSON serialised by caller or tool)
- [ ] `put url body` — HTTP PUT, returns `R t t`
- [ ] `patch url body` — HTTP PATCH, returns `R t t`. Partial updates — the most common mutation method in REST APIs
- [ ] `del url` — HTTP DELETE, returns `R t t`
- [ ] Header support: `post url body hdrs` where `hdrs` is a record or `M t t` map (gates on E4 maps)
- [ ] Consider a unified `req` builtin: `req "POST" url body hdrs` — more tokens but one builtin instead of five
- [ ] Feature flag: extend `http` feature, still uses `minreq` (supports all methods)
- [ ] Content-Type defaults to `application/json` for POST/PUT/PATCH
- [ ] Status code access: currently `get` only returns body text. Consider `R resp t` where `resp` is a record `{status:n;body:t;headers:M t t}` — or keep simple and add `get-status` variant later

##### G1b. GraphQL (the other API protocol agents hit constantly)

Most modern APIs (GitHub, Shopify, Hasura, Contentful) are GraphQL. It's just POST with a JSON body, but the pattern is so common it deserves first-class support or at least a documented pattern.

- [ ] **Minimal approach:** GraphQL is HTTP POST to a single endpoint with `{"query": "...", "variables": {...}}` body. With G1 `post` + I1 `jp`, it already works:
  ```
  q="{\"query\":\"{user(id:1){name email}}\"}";r=post! "https://api.example.com/graphql" q;n=jp! r "data.user.name"
  ```
- [ ] **Convenience builtin:** `gql url query vars` — wraps the POST + JSON construction. Returns `R t t` (response data or error)
  ```
  r=gql! "https://api.example.com/graphql" "{user(id:1){name email}}" "{}";n=jp! r "user.name"
  ```
- [ ] **Design question:** is `gql` worth a dedicated builtin? Or is `post` + `jp` sufficient once those land? GraphQL is common enough that a one-liner matters for token efficiency
- [ ] Variables as ilo records: `gql url query vars` where `vars` is a record → auto-serialised to JSON. Gates on D1e (Value ↔ JSON)
- [ ] Error handling: GraphQL returns 200 even on errors. `gql` should check `response.errors` and return `Err` if present
- [ ] Feature flag: same as `http` — it's just a POST wrapper

##### G1c. gRPC (protobuf-based RPC)

gRPC uses HTTP/2 + Protocol Buffers. Common in microservice architectures (Kubernetes, Google Cloud APIs, many internal systems). More complex than REST/GraphQL.

- [ ] **Tool approach (recommended):** gRPC is complex (HTTP/2 framing, protobuf serialisation, streaming). Best handled as a tool server, not a language builtin
  ```
  tool grpc-call"Call gRPC method" endpoint:t method:t payload:t>R t t timeout:10
  ```
- [ ] **Why not builtin:** gRPC requires `.proto` schema files, code generation, and HTTP/2. This is fundamentally different from HTTP/1.1 text protocols. A gRPC tool server (in Go, Rust, or Python) translates between JSON and protobuf
- [ ] **grpcurl pattern:** like `curl` for gRPC — the tool server wraps `grpcurl` or equivalent
- [ ] **Reflection support:** `tool grpc-list"List gRPC services" endpoint:t>R t t` — discover available methods via gRPC reflection
- [ ] Feature flag: none in ilo — this lives in a tool server
- [ ] **If native eventually:** would need protobuf parsing (binary format), HTTP/2 support, and streaming. Massive scope. Gates on G5 (TCP), G6 (streams), G7 (binary data)

##### G1d. Server-Sent Events / SSE (streaming responses)

LLM APIs (OpenAI, Anthropic, etc.) use SSE for streaming responses. Agents calling other LLMs need this.

- [ ] SSE is HTTP GET/POST with `text/event-stream` content type — server sends `data: ...\n\n` frames over a long-lived connection
- [ ] `sse url` — open SSE connection, returns stream handle (like G6)
- [ ] `sse-recv h` — receive next event, returns `R t t` (event data or connection error)
- [ ] `sse-close h` — close SSE connection
- [ ] **Or:** SSE as a special case of G6 streams — `get` with streaming flag returns a stream handle instead of the full body
- [ ] **Use case:** agent calls OpenAI streaming API, processes tokens as they arrive, takes action before full response is complete
- [ ] Feature flag: extend `http` feature — SSE is just HTTP with chunked transfer encoding
- [ ] Gates on: G6 (streams) for the iteration model

##### G2. WebSocket client (bidirectional communication)

Required for browser automation (CDP), real-time APIs, and agent-to-agent communication. This is the big one.

- [ ] `ws url` — open WebSocket connection, returns `R ws t` (connection handle or error)
- [ ] `ws-send conn msg` — send text message, returns `R _ t`
- [ ] `ws-recv conn` — receive next message (blocking), returns `R t t`
- [ ] `ws-close conn` — close connection, returns `R _ t`
- [ ] Connection handle: new `Value::Handle(u64)` — opaque reference to runtime-managed resource
- [ ] Runtime resource table: `HashMap<u64, Box<dyn Resource>>` in interpreter/VM, handles get indices
- [ ] Feature flag: `ws` feature, uses `tungstenite` (sync WebSocket, no tokio needed)
- [ ] Timeout: `ws-recv conn 5` — optional timeout in seconds, returns `Err("timeout")` if exceeded
- [ ] Binary frames: `ws-recv` returns text frames as `t`, binary frames as base64-encoded `t` (or a new `bytes` type — deferred)
- [ ] **Design question:** blocking `ws-recv` means the program stalls waiting for messages. For CDP this is fine (request-response pattern). For event streams, may need callback/event model (deferred to G5)

##### G3. Process spawning

Required for launching browsers, running external tools, shell integration.

- [ ] `spawn cmd args` — launch process, returns `R proc t` (process handle or error)
- [ ] `proc-wait h` — wait for process to exit, returns `R n t` (exit code or error)
- [ ] `proc-kill h` — kill process, returns `R _ t`
- [ ] `proc-out h` — read stdout as text, returns `R t t`
- [ ] Process handle: reuses `Value::Handle(u64)` from G2
- [ ] Feature flag: `process` feature (uses `std::process`)
- [ ] **Security:** sandboxing concern — process spawning is powerful. Consider allowlist of executables, or make it tool-provider-only (not a language builtin)
- [ ] Environment variables: `spawn cmd args env` where `env` is `M t t` (gates on E4)

##### G4. Async runtime (foundation for concurrent I/O)

Current model is fully synchronous. Async unlocks parallel tool calls, non-blocking WebSocket, and multiplexed CDP communication.

- [ ] Introduce `tokio` behind `async` feature flag
- [ ] `ToolProvider` trait becomes async (already planned in D1d)
- [ ] Async builtins: `get`, `post`, `ws-send`, `ws-recv` become non-blocking internally
- [ ] **Language-level async:** deferred — the runtime handles async internally, ilo programs remain sequential from the agent's perspective
- [ ] **Parallel tool calls:** `par{call1;call2;call3}` — execute tool calls concurrently, collect results. Sugar for runtime-managed parallelism
- [ ] **Design question:** should ilo expose async to the language (promises, await) or keep it hidden in the runtime? Hidden is simpler and more constrained (manifesto: "one way to do things")

##### G5. TCP/UDP sockets (raw network I/O)

WebSocket and HTTP are application-level protocols. For lower-level networking — talking to databases, custom protocols, inter-process communication — ilo needs raw sockets.

- [ ] `tcp-conn host port` — open TCP connection, returns `R handle t`
- [ ] `tcp-send h data` — send text over TCP, returns `R n t` (bytes sent or error)
- [ ] `tcp-recv h max` — receive up to `max` bytes, returns `R t t` (data or error)
- [ ] `tcp-close h` — close connection, returns `R _ t`
- [ ] `tcp-listen port` — bind and listen on port, returns `R handle t` (server socket)
- [ ] `tcp-accept h` — accept incoming connection, returns `R handle t` (blocking)
- [ ] `udp-bind port` — create UDP socket bound to port, returns `R handle t`
- [ ] `udp-send h host port data` — send datagram, returns `R n t`
- [ ] `udp-recv h max` — receive datagram, returns `R t t`
- [ ] Connection handles: reuse `Value::Handle(u64)` from G2
- [ ] Feature flag: `net` feature (uses `std::net`)
- [ ] **Design question:** should raw sockets be language builtins or tool-provider-only? Raw sockets are powerful but dangerous. Builtins keep ilo self-contained; tool-provider-only keeps the sandbox tighter
- [ ] **Use cases:** database drivers (Postgres wire protocol, Redis RESP), custom agent-to-agent communication, SMTP, DNS lookups

##### G6. Streams and buffered I/O

Current I/O model is request-response (send, then receive complete response). Streams handle continuous/chunked data — log tailing, large file transfer, SSE, streaming LLM responses.

- [ ] `Value::Stream(u64)` — handle to a readable/writable stream (backed by runtime resource table)
- [ ] `read s max` — read up to `max` bytes/chars from stream, returns `R t t` (data or error). Returns empty string `""` at EOF
- [ ] `readln s` — read one line from stream (up to `\n`), returns `R t t`
- [ ] `write s data` — write text to stream, returns `R n t` (bytes written or error)
- [ ] `flush s` — flush buffered writes, returns `R _ t`
- [ ] `close s` — close stream, returns `R _ t`
- [ ] `eof s` — check if stream is at end, returns `b`
- [ ] Streams wrap: TCP sockets, process stdin/stdout, file handles, WebSocket connections
- [ ] Buffering: runtime manages read buffers internally (like `BufReader`), ilo programs don't manage buffer sizes
- [ ] **Line-based iteration:** `@line stream{process line}` — iterate lines from a stream. Natural fit with `@` loop syntax
- [ ] **Design question:** should streams be a distinct type or just handles that support read/write? Distinct type gives better verifier errors; handles are simpler

##### G7. Buffers and binary data

ilo currently has no binary data type. Everything is `n` (f64) or `t` (string). Binary data matters for: screenshots (PNG), file uploads, protocol framing, cryptographic operations.

- [ ] New type: `B` (bytes / buffer) — raw byte sequence
- [ ] `B` literals: deferred (no good syntax for binary literals in a token-minimal language)
- [ ] `b64enc data` — bytes to base64 text, returns `t`
- [ ] `b64dec text` — base64 text to bytes, returns `R B t`
- [ ] `buf-new n` — allocate empty buffer of capacity `n`, returns `B`
- [ ] `buf-len b` — length in bytes, returns `n`
- [ ] `buf-slc b start end` — slice bytes, returns `B`
- [ ] `buf-cat a b` — concatenate buffers, returns `B`
- [ ] `to-buf t` — encode text as UTF-8 bytes, returns `B`
- [ ] `to-txt b` — decode UTF-8 bytes as text, returns `R t t` (fails if invalid UTF-8)
- [ ] Integration with streams: `read`/`write` work on `B` as well as `t`
- [ ] Integration with WebSocket: binary frames return `B` instead of `t`
- [ ] Integration with `shot` (screenshot): CDP returns base64 PNG — `b64dec` → `B` → file write
- [ ] Feature flag: no feature flag needed — core type like `L` and `R`
- [ ] **Design question:** is `B` worth the complexity? Alternative: everything stays as base64 `t`, tools handle encoding. Simpler but wastes memory (base64 is 33% larger)

##### G8. File I/O

ilo has no file system access. Agents need to read configs, write outputs, save screenshots.

- [ ] `fread path` — read entire file as text, returns `R t t`
- [ ] `fwrite path data` — write text to file (overwrite), returns `R _ t`
- [ ] `fappend path data` — append text to file, returns `R _ t`
- [ ] `fexists path` — check if file exists, returns `b`
- [ ] `freadbin path` — read file as bytes, returns `R B t` (gates on G7 buffers)
- [ ] `fwritebin path data` — write bytes to file, returns `R _ t` (gates on G7)
- [ ] Feature flag: `fs` feature (uses `std::fs`)
- [ ] **Security:** file system access is a sandbox escape. Options:
  - Allowlist of directories (e.g. only `/tmp` and working directory)
  - Read-only by default, write requires `--allow-write` flag
  - Tool-provider-only (no builtins, file I/O via declared tools)
- [ ] **Design question:** builtins vs tools? File I/O as builtins keeps ilo self-contained for scripting. As tools, it's more constrained but requires tool provider setup for basic file operations

##### G9. Event/callback model (deferred — needs design)

For WebSocket event streams, browser events, server-sent events. Currently out of scope but noting the design space.

- [ ] Event loop concept: `on conn "event-name" {handler body}`
- [ ] Or pull-based: `poll conns timeout` — wait on multiple connections, return first message
- [ ] Or channel-based: `ch=chan();spawn-listener conn ch;msg=recv ch`
- [ ] **Recommendation:** defer until a concrete use case forces the design. CDP's request-response pattern works with blocking `ws-recv`

##### G10. Resource handles — unified design (foundation for G2-G8)

G2-G8 all introduce "handles" — opaque references to runtime-managed resources (sockets, processes, files, streams). This needs a unified design.

- [ ] `Value::Handle(u64)` — single opaque type for all external resources
- [ ] Runtime resource table: `HashMap<u64, Box<dyn Resource>>` — indexed by handle ID
- [ ] `Resource` trait: `close()`, `type_name()` — common interface for cleanup
- [ ] Auto-cleanup: resources closed when handle goes out of scope (or program exits)
- [ ] Verifier: handle types are opaque — can't do arithmetic on them, can only pass to handle-accepting builtins
- [ ] **Typed handles vs untyped:** `ws` vs `tcp` vs `file` — should the verifier distinguish handle types? Typed catches "passing a file handle to ws-send" at verify time. Untyped is simpler
- [ ] **Type syntax if typed:** `H ws`, `H tcp`, `H file` — or just `ws`, `tcp`, `file` as named types

#### Browser automation — Phase H (Playwright-for-ilo)

See [research/playwright-for-ilo.md](research/playwright-for-ilo.md) for full design exploration.

Two approaches: **tool-server wrapper** (practical, near-term) vs **native CDP client** (ambitious, needs G2+G3+G4).

##### H1. Playwright tool server (recommended first step)

Wrap Playwright (Node.js) as an external tool server. ilo calls it via HTTP or stdio. Gets browser automation without any new language primitives.

- [ ] Tool server: Node.js process running Playwright, exposing actions as HTTP endpoints or stdio JSON-RPC
- [ ] Tool declarations in ilo:
  ```
  tool nav"Navigate to URL" url:t>R _ t timeout:30
  tool click"Click element" sel:t>R _ t timeout:10
  tool fill"Fill input" sel:t val:t>R _ t timeout:10
  tool txt"Get text content" sel:t>R t t timeout:5
  tool shot"Screenshot" path:t>R t t timeout:10
  tool eval"Evaluate JS" code:t>R t t timeout:10
  ```
- [ ] Session management: tool server maintains browser state between calls
- [ ] Gates on D1d (ToolProvider infrastructure)

##### H2. Native CDP client (ambitious — needs G2, G3, G4)

Direct Chrome DevTools Protocol communication from ilo. No Node.js dependency.

- [ ] Launch Chromium with `--remote-debugging-port` via G3 process spawning
- [ ] Connect via WebSocket (G2) to CDP endpoint
- [ ] CDP message protocol: JSON request/response over WebSocket
- [ ] Subset of CDP domains: Page, Runtime, DOM, Network, Input
- [ ] **Massive scope** — CDP has dozens of domains with hundreds of methods. Start with ~10 essential operations
- [ ] Gates on: G2 (WebSocket), G3 (process spawn), G4 (async for multiplexed CDP), E4 (maps for JSON)

#### Agent essentials — Phase I (what agents actually need daily)

Gap analysis: what do real AI agents do that ilo can't express today? Ordered by how often agents hit the wall without it.

##### I1. JSON parsing (critical — agents live in JSON)

Agents call APIs. APIs return JSON. ilo can fetch JSON (`get url`) but can't do anything with the response except pass it as raw text. This is the #1 gap.

- [ ] `jp text key` — JSON path lookup, returns `R t t`. `jp body "name"` extracts `$.name` as text
- [ ] `jp text key` on nested paths: `jp body "address.city"` or `jp body "items.0.name"`
- [ ] `jparse text` — parse JSON text into ilo values (records, lists, numbers, text, bool, nil), returns `R <value> t`
- [ ] `jdump value` — serialise ilo value to JSON text, returns `t`
- [ ] **Minimal approach:** `jp` alone covers 80% of cases. Agent gets JSON string from API, picks out fields with `jp`, done. No full parse needed
- [ ] **Design tension:** manifesto says "format parsing is a tool concern." But JSON is so fundamental to agent work that not having it is like a shell without `grep`. Consider making `jp` a builtin exception, or accept that every agent needs a `json-extract` tool
- [ ] Feature flag: `json` feature (uses `serde_json` — already a dependency)
- [ ] **Integration with records:** `jparse text "profile"` could map JSON to a declared record type, verified at parse time — combines D1e (Value ↔ JSON) with a language builtin
- [ ] **Token comparison:**
  ```
  # Python: ~12 tokens
  data = json.loads(response.text)
  name = data["user"]["name"]

  # ilo with jp: ~4 tokens
  n=jp! body "user.name"

  # ilo without (current): impossible without tool
  ```

##### I2. Shell/command execution (critical — agents shell out constantly)

G3 covers low-level process spawning with handles. But agents need a simple "run this command, give me the output" — like backticks in Perl/Ruby/shell or `subprocess.run` in Python.

**Two forms:** `run` (builtin, verbose) and `` ` `` (backtick syntax, terse). Same relationship as `get url` and `$url`.

- [ ] `run cmd` — run system command, wait for completion, returns `R t t` (stdout or error). Stderr captured in error
- [ ] `run!` — auto-unwrap variant: `o=run! "ls -la"`
- [ ] `` `cmd` `` — terse alias for `run "cmd"`. Backtick-delimited string is executed as a shell command, returns `R t t`
- [ ] `` `!cmd` `` — terse auto-unwrap: `` o=`!ls -la` `` (or `o=run! "ls -la"`)
- [ ] Exit code: `run` returns `Err` if non-zero exit. Ok body is stdout text
- [ ] **vs G3 spawn:** `run` is the simple one-shot version. `spawn` is for long-running processes you interact with. `run "ls"` vs `h=spawn "node" "server.js"`
- [ ] **Cross-platform shell selection:**
  - macOS/Linux: `/bin/sh -c "..."`
  - Windows: `cmd.exe /C "..."` (or `powershell -Command "..."` — configurable)
  - Agent writes `run "git status"` — works everywhere. Platform-specific commands (`ls` vs `dir`) are the agent's problem, not the language's
- [ ] Feature flag: `shell` feature (uses `std::process::Command`)
- [ ] **Security:** same concerns as G3. `--allow-shell` flag? Or sandbox to specific commands?
- [ ] **Lexer changes for backticks:**
  - New token: `Token::Backtick(String)` — contents between `` ` `` delimiters
  - Escape: `` \` `` inside backticks for literal backtick (rare)
  - Parser: desugar `` `cmd` `` to `Call("run", [StringLit("cmd")])` — same AST as `run "cmd"`
  - `!` position: `` `!cmd` `` — `!` immediately after opening backtick, consistent with `$!url` pattern
- [ ] **Precedent in ilo:** `$url` is terse alias for `get url`. `` `cmd` `` is terse alias for `run "cmd"`. Both are sigil-based shortcuts for common operations. Pattern: frequent operations get single-character syntax
- [ ] **Token comparison:**
  ```
  # Python: ~8 tokens
  result = subprocess.run(["ls", "-la"], capture_output=True, text=True)
  output = result.stdout

  # ilo with run: ~3 tokens
  o=run! "ls -la"

  # ilo with backticks: ~2 tokens
  o=`ls -la`

  # terse auto-unwrap: ~2 tokens
  o=`!ls -la`
  ```

##### I3. Environment variables (critical — every agent needs config)

API keys, base URLs, secrets, feature flags. Every agent program needs to read env vars. Currently impossible in ilo.

- [x] `env key` — read environment variable, returns `R t t` (value or "not set")
- [x] `env! key` — auto-unwrap: `k=env! "API_KEY"`
- [x] **No env-set:** writing env vars is rarely needed and creates side effects. Read-only
- [x] Feature flag: none needed — `std::env::var` is stdlib
- [x] **Token comparison:**
  ```
  # Python: ~4 tokens
  key = os.environ["API_KEY"]

  # ilo: ~2 tokens
  k=env! "API_KEY"
  ```

##### I4. String interpolation / templating

Building URLs, prompts, messages. Currently requires chains of `+` which is verbose and error-prone:
`+++++"Hello " name ", your order " oid " is " status` — 11 tokens for a simple template.

- [ ] **Option A:** Template syntax in strings: `fmt "Hello {name}, order {oid} is {status}"` — clear but needs lexer changes for `{}`-in-strings
- [ ] **Option B:** Printf-style: `fmt "Hello %s, order %s is %s" name oid status` — no lexer changes, variadic
- [ ] **Option C:** Stay with `+` — it works, agents generate it fine, and it's already 10/10 accuracy. Token cost is the tradeoff
- [ ] **Recommendation:** `fmt` builtin with positional `%s` placeholders. No lexer changes, composes with existing types, `str` handles number→text conversion
- [ ] `fmt pattern args...` — returns `t`. `%s` substitutes args in order. Type-aware: numbers auto-convert via `str`
- [ ] **Token comparison:**
  ```
  # Current ilo: 11 tokens
  +++++"Hello " name ", order " oid " is " status

  # With fmt: 5 tokens
  fmt "Hello %s, order %s is %s" name oid status
  ```

##### I5. Logging / debug output

Agents need observability — what did it do, what's the current state, where did it fail. Currently ilo has no way to print debug output without it being the return value.

- [ ] `log msg` — write to stderr, returns `_` (nil). Does NOT affect return value or program flow
- [ ] `log` accepts any type — auto-converts via `str` for numbers, `jdump` for records/lists
- [ ] Log levels: `log msg` (info), `logw msg` (warn), `loge msg` (error), `logd msg` (debug)
- [ ] Or simpler: just `log msg` and `dbg expr` (debug-print with expression name + value, like Rust's `dbg!`)
- [ ] `dbg x` — prints `x = <value>` to stderr, returns the value unchanged (transparent — can insert anywhere in a pipeline)
- [ ] Feature flag: none — stderr is always available
- [ ] **Design question:** does logging violate the "pure function" feel? No — it's a side effect on stderr, doesn't affect computation. Same as Haskell's `trace`

##### I6. Time and timestamps

Rate limiting, timeouts, cache expiry, audit logs, scheduling. Agents work in time.

- [ ] `now()` — current Unix timestamp as `n` (seconds since epoch, float for sub-second precision)
- [ ] `sleep n` — pause execution for `n` seconds, returns `_`. For rate limiting, polling loops
- [ ] `fmt-time n pattern` — format timestamp as text. Deferred — complex, may be a tool concern
- [ ] **Design question:** `sleep` is a side effect that breaks pure execution. But agents need rate limiting. Alternative: runtime-level rate limiting on tool calls (automatic backoff in ToolProvider)

##### I7. Encoding/decoding (URL, HTML, base64)

Agents build URLs with parameters, handle HTML content, pass binary data.

- [ ] `urlencode t` — percent-encode text for URL parameters, returns `t`
- [ ] `urldecode t` — decode percent-encoded text, returns `R t t`
- [ ] `b64enc t` — encode text as base64, returns `t` (overlaps with G7 but works on text directly)
- [ ] `b64dec t` — decode base64 to text, returns `R t t`
- [ ] `htmlesc t` — escape HTML entities (`<>&"'`), returns `t`
- [ ] `htmlunesc t` — unescape HTML entities, returns `R t t`
- [ ] **Minimal set:** `urlencode` + `b64enc` + `b64dec` cover 90% of agent encoding needs
- [ ] Feature flag: none — pure string transformations, no deps

##### I8. Regex / pattern matching on text

Extracting structured data from unstructured text — log parsing, HTML scraping, validation.

- [ ] `match text pattern` — first regex match, returns `R t t` (matched text or no-match error)
- [ ] `matchall text pattern` — all matches, returns `L t`
- [ ] `sub text pattern replacement` — regex substitution, returns `t`
- [ ] `suball text pattern replacement` — global substitution, returns `t`
- [ ] Capture groups: `match text "(\d+)-(\w+)"` → access groups via `.0`, `.1` on result? Or return list of captures?
- [ ] Feature flag: `regex` feature (uses `regex` crate)
- [ ] **Design question:** regex is powerful but complex. Alternative: keep text processing as tool concern (a `regex` tool). But like JSON, it's so fundamental that agents hit the wall without it
- [ ] **Simpler alternative:** just `has` (already exists) + `spl` (exists) + `slc` (exists) cover basic cases. Regex for the hard stuff

##### I9. Hashing and checksums

Content deduplication, cache keys, API signature verification, integrity checks.

- [ ] `hash t` — SHA-256 hash of text, returns `t` (hex string). Single default algorithm
- [ ] `hmac key msg` — HMAC-SHA256, returns `t`. For API authentication (AWS, Stripe, etc.)
- [ ] Feature flag: `crypto` feature (uses `sha2` + `hmac` crates, or ring)
- [ ] **Scope limit:** hashing only, not encryption. Encryption is a tool concern
- [ ] **Use case:** many APIs require HMAC signatures: `sig=hmac secret (+method +path +timestamp)`

##### I10. Standard output and program I/O

Currently the only output is the return value of the main function. Agents need to write structured output, stream progress, and read input.

- [ ] `print val` — write value to stdout followed by newline, returns `_`
- [ ] `eprint val` — write to stderr (alias for `log`)
- [ ] `input prompt` — read line from stdin, returns `R t t`. For interactive mode
- [ ] **vs return value:** `print` is for streaming output during execution. Return value is the final result. Both matter for agent integration
- [ ] **Design question:** does `print` conflict with ilo's "return value is the output" model? In agent mode (D4 `ilo serve`), stdout is the protocol channel. `print` would need to go to stderr or a separate channel

##### I11. Sleep / delay / retry helpers

Agents need to wait between API calls (rate limiting), retry on failure, and implement backoff.

- [ ] `sleep n` — pause `n` seconds (also mentioned in I6)
- [ ] `retry n f args` — call function `f` up to `n` times with exponential backoff, returns first `~` result or last `^` error
- [ ] **Or:** retry as a pattern, not a builtin. `wh` loop + `sleep` + counter already works:
  ```
  poll url:t n:n>R t t;<=n 0{^"timeout"};r=get url;?r{~v:~v;^e:sleep 1;poll url -n 1}
  ```
- [ ] **Recommendation:** `sleep` as builtin, retry as a pattern. Keeps builtins minimal

#### Codegen targets
- JavaScript / TypeScript emit — like Python codegen but for JS ecosystem
- WASM emit — compile to WebAssembly for browser/edge execution
- Shell emit — transpile simple programs to bash (for environments where only shell is available)

#### Program structure
- Multi-file programs / module system (programs are small by design — may never need this)
- Imports — `use "other.ilo"` to compose programs from multiple files
- Namespacing — prevent name collisions when merging declaration graphs from multiple sources
- Compensation as a first-class concept (keep inline error handling for now)
- Graph query language (build the graph first, query it later)
