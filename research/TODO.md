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
- [x] `rnd` / `rnd a b` — random float [0,1) or random int in [a,b]
- [x] `now` — Unix timestamp (seconds)
- [x] `@i a..b{body}` — range iteration (interpreter scope bug fixed 2026-03-05)

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

## Agent integration — Phase D (make ilo programs do things)

Manifesto: "a minimal, verified action space." The language verifies and executes locally — Phase D connects it to the outside world.

### D1. Tool Execution (foundation)

Plumbing first — make tool calls actually do things. HTTP-native (tools are APIs, not scripts).

#### D1a. Fix existing bugs ✅
- [x] Fix VM empty-chunk bug — tool `Decl` compiles stub chunk (LOADK Nil → WRAPOK → RET), returns `Ok(Nil)` matching interpreter
- [x] Fix VM test helper `parse_program` discarding token spans (broke auto-unwrap `!` adjacency check)

#### D1b. HTTP builtin: `get` / `$` ✅
- [x] `get url` / `$url` — built-in HTTP GET, returns `R t t` (ok=response body, err=error message)
- [x] `$` is the terse alias for `get` (same AST node, like `help`/`-h`)
- [x] Respects the curl model: one thing in, one thing out, composes with everything
- [x] 3 chars / 1 token. `d=get url;d.name` beats `curl -s url | jq '.name'`

#### D1c. Auto-unwrap operator `!` ✅
- [x] `get! url` — unwrap ok value, propagate err to caller (like Rust's `?`)
- [x] Works on any `R` return type, not just `get`
- [x] `d=get! url;d.name` — 18 chars, verified, error-handled
- [x] Without `!`, use explicit match: `get url;?{~d:d.name;^e:^e}`

#### D1d. Tool provider infrastructure ✅
- [x] Add `tokio` + `reqwest` as deps behind a `tools` feature flag
- [x] `ToolProvider` trait — async executor interface: `async fn call(&self, name: &str, args: Vec<Value>) -> Result<Value, ToolError>`
- [x] `HttpProvider` — tool name maps to an HTTP endpoint. JSON request/response. Respects `timeout` and `retry` from `tool` decl
- [x] `StubProvider` — current behaviour (`Ok(Nil)`), used in tests and when no provider configured
- [x] Wire into interpreter + VM — both backends receive an `Option<&dyn ToolProvider>`, tool calls dispatch through it
- [x] Tool config — `ilo program.ilo --tools tools.json` where `tools.json` maps tool names to URLs/endpoints

#### D1e. Value ↔ JSON at tool boundary ✅
- [x] `Value::to_json()` — serialise ilo values to JSON for tool call args
- [x] `Value::from_json(type_hint)` — deserialise JSON to ilo values, guided by the tool's declared return type
- [x] JSON objects → records (if matching type declared), JSON arrays → lists, primitives → n/t/b, null → nil
- [x] Unknown/complex shapes: tool declares `>t`, gets raw JSON string — passable to another tool without parsing
- [x] Format parsing is a tool concern, not a language concern (see OPEN.md)

#### D1f. Tests ✅
- [x] Mock HTTP server for integration tests
- [x] `StubProvider` for unit tests
- [x] Test `get`/`$` with real HTTP (httpbin or similar)

### D2. MCP Integration

- [x] MCP client: connect to MCP servers, discover tools, call them (builds on D1 async infra)
- [x] `ilo run program.ilo --mcp server.json` — load tool signatures from MCP server config
- [x] Auto-populate tool declarations from MCP server discovery (graph loading option 3: query on demand)

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
- Reserve keywords at lexer level — `if`, `return`, `let`, `fn`, `def`, `var`, `const` are currently valid identifiers (only caught as hints at declaration position). Low urgency while user base is small.

#### Control structures
- Pattern matching on type — `?x{n v:...; t v:...}` to branch on runtime type (useful when tools return `t` escape hatch)

#### Type system (Phase E)
- Optional type — `O n` or `n?` for nullable values
- Sum types / tagged unions — `S a b c` for closed sets of variants beyond ok/err
- Map type — `M t n` for key-value collections (records are fixed-shape; maps are dynamic)
- Generic functions — `map f:fn(a>b) xs:L a > L b` for higher-order typed transforms


#### Tooling
- LSP / language server — completions, diagnostics, hover info for editor integration
- REPL — interactive evaluation for exploration and debugging
- Playground — web-based editor with live evaluation (WASM target)

#### Codegen targets
- JavaScript / TypeScript emit — like Python codegen but for JS ecosystem
- WASM emit — compile to WebAssembly for browser/edge execution

#### Program structure
- Multi-file programs / module system (programs are small by design — may never need this)
- Imports — `use "other.ilo"` to compose programs from multiple files
