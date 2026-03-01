# TODO

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

##### F7. Range iteration — `@i 0..n{body}` (medium priority)

Index-based loops without constructing a list. Avoids list allocation for numeric ranges.

- [ ] Syntax: `@i 0..n{body}` — bind `i` to each integer in `[0, n)`
- [ ] Parser: recognise `..` between two numeric expressions in `@` collection position
- [ ] AST: add `Expr::Range { start, end }` or extend `ForEach` with range variant
- [ ] Interpreter: iterate from start (inclusive) to end (exclusive), bind each integer to loop variable
- [ ] VM: compile like existing foreach but with integer counter instead of list indexing — no `OP_LISTGET`, just `OP_ADD` + `OP_LT`
- [ ] Verifier: start and end must be `n`; loop variable is `n`
- [ ] Dynamic end: `@i 0..len xs{xs.i}` — end expression evaluated once before loop starts
- [ ] Step variant (deferred): `@i 0..10..2{body}` for step=2 — lower priority
- [ ] Cranelift JIT: standard counted loop — optimal for JIT
- [ ] Python codegen: emit as `for i in range(start, end):`
- [ ] Tests: basic range, range with expressions, range variable in body, empty range (start >= end), range + break
- [ ] SPEC.md: document range syntax

##### F8. Destructuring bind — `{a;b}=expr` (medium priority)

Extract multiple record fields into local variables in one statement.

- [ ] Syntax: `{a;b;c}=expr` — bind `a` to `expr.a`, `b` to `expr.b`, `c` to `expr.c`
- [ ] Field names match variable names (ilo convention: short field names)
- [ ] Parser: recognise `{` at statement start followed by identifiers + `}=`
- [ ] AST: add `Stmt::Destructure { bindings: Vec<String>, value: Expr }`
- [ ] Interpreter: evaluate expression, extract each named field, bind to scope
- [ ] VM: compile expression → register, emit `OP_GETFIELD` for each binding
- [ ] Verifier: expression must be a record type with all named fields present. Bind each variable to its field's type
- [ ] Renaming syntax (deferred): `{name:n;email:e}=expr` — bind `n` to `expr.name`. Lower priority
- [ ] Cranelift JIT: sequence of field loads from record
- [ ] Python codegen: emit as `a, b, c = expr.a, expr.b, expr.c`
- [ ] Tests: basic destructure, missing field error, type inference from fields, destructure in loop body
- [ ] SPEC.md: document destructuring syntax

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

##### E1. Type aliases (pure sugar — no runtime changes)

Lets users name complex types without creating records. No new AST nodes at runtime, just resolution at parse/verify time.

- [ ] Syntax: `alias name type` as a new `Decl` variant — e.g. `alias res R n t`, `alias ids L n`
- [ ] Parser: recognise `alias` keyword at declaration position, parse name + type
- [ ] AST: add `Decl::Alias { name: String, target: Type, span: Span }`
- [ ] Verifier: resolve aliases during declaration collection — expand `Named("res")` → `Result(Number, Text)` before body verification
- [ ] Cycle detection — `alias a b` + `alias b a` must error (ILO-T0xx: circular type alias)
- [ ] Error messages: show alias name in user-facing messages, expanded form in notes
- [ ] Formatter: emit `alias` declarations in both dense and expanded formats
- [ ] Python codegen: emit type alias as comment or `TypeAlias` (3.12+)
- [ ] Tests: alias in function signatures, nested aliases (`alias rlist L res`), cycles, shadowing a builtin type name
- [ ] SPEC.md: document alias syntax

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
- ~~`now`~~ ✅

#### Tooling
- LSP / language server — completions, diagnostics, hover info for editor integration
- REPL — interactive evaluation for exploration and debugging
- Debugger — step through execution, inspect bindings at each statement
- Playground — web-based editor with live evaluation (WASM target)

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
