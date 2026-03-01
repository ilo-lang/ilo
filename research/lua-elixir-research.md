# Lua & Elixir Research for ilo

Comparative analysis of Lua and Elixir language features, evaluated against ilo's manifesto: **total tokens from intent to working code**. Both languages offer deep philosophical parallels to ilo -- Lua through radical minimalism and embeddability, Elixir through pipe-oriented composition and explicit error handling. This document catalogs what they offer, what ilo already handles, and what minimal additions (if any) are justified.

---

## Part 1: Lua

Lua originated in 1993 at PUC-Rio as a language for extending software applications. Its designers (Ierusalimschy, Figueiredo, Celes) made a bet that still holds: a tiny language with powerful mechanisms beats a large language with many features. Thirty years later, Lua powers game engines (Roblox, World of Warcraft), databases (Redis), HTTP servers (Nginx/OpenResty), and embedded firmware -- all because it is small enough to fit anywhere.

### 1.1 Minimalism: 22 Keywords vs 0

Lua 5.4 has exactly 22 reserved words:

```
and  break  do  else  elseif  end  false  for  function  goto  if
in  local  nil  not  or  repeat  return  then  true  until  while
```

That is remarkably few for a language with control flow, iteration, local scoping, boolean logic, and function definitions. Every keyword is a single English word. The reference manual is ~100 pages -- smaller than most language tutorials.

**ilo has ~4 abbreviated English keywords** (`type`, `tool`, `wh`, `ret`, plus `brk`/`cnt` for loop control). The core syntax uses sigils (`?`, `!`, `~`, `^`, `@`, `>`, `>>`, `.?`, `??`) and structural tokens (`=`, `;`, `{`, `}`, `:`) instead of full English words. Builtins (`len`, `str`, `num`, `get`, `spl`, `cat`, etc.) are abbreviated but not reserved words.

**Comparison:**

| Dimension | Lua | ilo |
|-----------|-----|-----|
| Reserved words | 22 | ~6 abbreviated (`type`, `tool`, `wh`, `ret`, `brk`, `cnt`) |
| Control flow | `if/elseif/else/end`, `for/do/end`, `while/do/end`, `repeat/until` | `cond{body}`, `?{arms}`, `@v xs{body}`, `wh cond{body}` |
| Function def | `function name(params) ... end` | `name params>ret;body` |
| Error handling | `pcall`/`xpcall` (exception-based) | `R ok err` + `?` + `!` (value-based) |
| Scope | `local x = expr` | `x=expr` |
| Boolean operators | `and`, `or`, `not` | `&`, `\|`, `!` |

**Why this matters for AI agents:** Every keyword an LLM must generate is a token. Lua's `function total(p, q, r) local s = p * q; local t = s * r; return s + t end` is 25 tokens. ilo's `tot p:n q:n r:n>n;s=*p q;t=*s r;+s t` is 8 tokens. Lua reduced keywords to 22; ilo reduced them to zero by replacing every keyword with a structural sigil.

**What ilo can learn:** Lua's 22-keyword design has survived 30 years without expansion pressure. The lesson is that a small, stable vocabulary is more valuable than an expressive one. ilo's sigil-based approach is the logical endpoint of this trajectory -- but ilo should monitor whether agents struggle with sigil density. Lua's keywords are self-documenting (`if`, `for`, `return`); ilo's sigils require spec loading. The tradeoff: ilo saves ~17 tokens per function definition but costs ~16 lines of spec context. If the spec is cached in the LLM's prompt, the savings compound across every function. If it is not, each generation starts with a spec-loading tax.

**Assessment:**
- Relevant to AI agents? **Yes.** Keyword minimalism directly reduces token cost.
- Does ilo handle this? **Yes, and goes further.** Zero keywords vs 22.
- Minimal addition needed: **None.** The sigil approach is correct. Monitor agent generation accuracy as sigil count grows.

---

### 1.2 Tables as Universal Data Structure

Lua has exactly one compound data structure: the table. Tables serve as arrays, dictionaries, objects, modules, namespaces, and environments. There is no separate array type, no map type, no struct type, no class type.

```lua
-- Array
local items = {1, 2, 3}

-- Dictionary
local config = {host = "localhost", port = 8080}

-- Object (table + metatable)
local Dog = {}
Dog.__index = Dog
function Dog.new(name) return setmetatable({name = name}, Dog) end
function Dog:bark() return self.name .. " barks!" end

-- Module
local M = {}
function M.greet(name) return "Hello, " .. name end
return M
```

One construct, many uses. The mechanism is general; the programmer applies the policy.

**ilo has four compound constructs:** lists (`L n`), records (`type point{x:n;y:n}`), results (`R ok err`), and planned maps (`M t n`). Each is distinct, typed, and verified.

**Comparison:**

| Dimension | Lua tables | ilo types |
|-----------|-----------|-----------|
| Array | `{1, 2, 3}` | `[1, 2, 3]` (typed: `L n`) |
| Dictionary | `{a=1, b=2}` | `M{"a":1;"b":2}` (planned E4) |
| Record/Struct | `{name="x", age=30}` | `type user{name:t;age:n}` |
| Object | table + metatable | Not supported (no OOP) |
| Module | table of functions | Multi-function programs |
| Namespace | `_ENV` tables | Declaration graph (flat) |

**Why ilo chose typed diversity over universal tables:** Lua tables are flexible but unverified. An agent generating `user.naem` (typo) in Lua discovers the error at runtime -- `nil` silently propagates. In ilo, the verifier catches it at compile time with ILO-T017 ("unknown field 'naem' on type user") and suggests the correct spelling. Each type the verifier knows about is a class of errors it prevents.

The token cost of Lua's flexibility: zero type annotations (saves tokens in generation) but more retries when runtime errors surface. The token cost of ilo's types: ~17% of program tokens are type annotations, but each annotation prevents ~100-200 tokens of retry cost per caught error.

**What ilo can learn:** Lua's single-table approach works because the host application provides the constraints (game engines define what fields an entity must have). ilo's tool declarations serve the same role -- they constrain the agent's world. The parallel: Lua tables + host constraints = ilo values + tool schemas.

Lua's table-as-module pattern is worth noting. In Lua, a module is just a table returned by a file. In ilo, a multi-function program serves the same purpose. Both avoid heavyweight module systems.

**Assessment:**
- Relevant to AI agents? **Partially.** Universal data structures reduce conceptual overhead but increase runtime errors.
- Does ilo handle this? **Yes, differently.** Typed constructs trade generation cost for retry prevention.
- Minimal addition needed: **Maps (E4)** are the one gap. Lua tables handle dynamic key-value data natively; ilo currently requires `t` (raw text) for this case. Once `M t n` lands, ilo covers all common data shapes.

---

### 1.3 Embeddability: Language Inside a Host

Lua was designed from day one as an embedded language -- a library with a C API, not a standalone program. The standalone interpreter is a tiny application built on top of the library. Embeddability shaped every language design decision:

1. **Small size.** The entire Lua implementation is ~30,000 lines of C. The binary is ~200KB. It must fit inside the host without bloating it.
2. **Clean API boundary.** The C API uses a virtual stack for all data exchange. C pushes values, calls Lua functions, reads results. No global state leaks between host and guest.
3. **Mechanisms over syntax.** Lua favors mechanisms representable through the C API. Syntax is not accessible through an API; functions are. This is why Lua has metatables (callable through the API) instead of operator overloading syntax (which would require parser extensions).
4. **Host controls policy.** Lua provides the mechanism (tables, functions, coroutines). The host decides the policy (what functions exist, what the sandbox allows, what libraries are loaded).

**ilo as an embedded agent language:** ilo is designed to be embedded in agent runtimes, not host applications. But the parallel is striking:

| Dimension | Lua embedding | ilo embedding |
|-----------|--------------|---------------|
| Host | C/C++ application (game, database, server) | Agent runtime (Claude, GPT, custom) |
| Guest | Lua scripts | ilo programs |
| API boundary | C API virtual stack | Tool declarations + Value<->JSON |
| Extension mechanism | Register C functions in Lua state | `tool` declarations pointing to external services |
| Sandboxing | Remove `os`/`io` libraries from state | Closed-world verification (only declared tools callable) |
| Policy source | Host application code | Tool provider configuration |

Lua's embedding insight translates directly: ilo programs should never assume what tools exist. The agent runtime (the "host") provides the tool graph. ilo (the "guest") verifies and composes within that graph.

**What Lua's embeddability teaches ilo:**

1. **Size budget matters.** Lua's 200KB binary fits in firmware. ilo's `help ai` (compact spec for LLM consumption) should fit in ~16 lines. Both are constrained by context -- Lua by memory, ilo by token windows. The spec IS ilo's "binary size."

2. **API over syntax.** Lua avoided syntactic features that could not be represented in the C API. ilo should avoid features that cannot be represented in tool declarations. If a feature requires the agent to understand ilo-specific syntax beyond the spec, it adds to the "spec loading" cost. Mechanisms accessible through the universal tool-call interface (like `get`, `fread`, `hash`) are cheaper to teach than new syntax.

3. **Host removes capabilities, not adds them.** In Lua, a restricted sandbox removes `os.execute` from the state. In ilo, a restricted agent environment removes tool declarations. Both achieve safety by subtraction. This is more robust than allowlisting -- the default is "nothing available," and the host explicitly adds what is needed.

**Assessment:**
- Relevant to AI agents? **Highly relevant.** The embedding model maps directly to agent runtime integration.
- Does ilo handle this? **Partially.** Tool declarations and closed-world verification implement the "host provides capabilities" model. The `ToolProvider` trait (D1d) makes this explicit.
- Minimal addition needed: **Tool discovery from host** (D2: MCP integration). Today, tool signatures are agent-authored. They should come from the host runtime (MCP server discovery), just as Lua's available functions come from the host application. This closes the "constrained principle gap" noted in D-AGENT-INTEGRATION.md.

---

### 1.4 Metatables and Metamethods

Metatables are Lua's mechanism for extending behavior without new syntax. Every table can have a metatable that defines how the table responds to operations:

```lua
local Vector = {}
Vector.__index = Vector

function Vector.new(x, y)
  return setmetatable({x = x, y = y}, Vector)
end

-- Operator overloading via metamethod
function Vector.__add(a, b)
  return Vector.new(a.x + b.x, a.y + b.y)
end

-- Indexing via metamethod
function Vector.__tostring(v)
  return "(" .. v.x .. ", " .. v.y .. ")"
end

local v1 = Vector.new(1, 2)
local v2 = Vector.new(3, 4)
local v3 = v1 + v2  -- calls Vector.__add
```

Key metamethods: `__add`, `__sub`, `__mul`, `__div` (arithmetic), `__index`, `__newindex` (table access), `__call` (function call), `__tostring` (string conversion), `__len` (length), `__eq`, `__lt`, `__le` (comparison).

This is Lua's "mechanisms over policies" at its purest. The language does not have classes, operator overloading, or property access syntax. It has metatables, and you build all of those on top.

**ilo's approach: no user-extensible behavior.** ilo operators (`+`, `-`, `*`, `/`, `=`, etc.) have fixed semantics defined by type. `+` on numbers is addition, on text is concatenation, on lists is concatenation. There is no mechanism for users to change what `+` means for a custom type.

**Why this is correct for ilo:**

1. **Constrained generation.** When an agent generates `+a b`, the verifier knows exactly what this means based on the types of `a` and `b`. If `+` were user-overloadable, the verifier would need to resolve the metamethod, adding complexity and potential for error.

2. **No abstraction tax.** Metatables enable abstraction (OOP, DSLs, custom types). Agents generate concrete code for specific tasks -- they do not build abstractions. The manifesto: "Agents generate concrete code, not abstract interfaces."

3. **Token cost of metamethods.** Defining a metatable in Lua costs ~20-30 tokens. Using it saves tokens at call sites. But agents generate single-use programs, so the definition cost is rarely amortized across enough uses to pay off.

**What ilo could learn (future):** If ilo ever adds traits/interfaces (Phase E6), Lua's metamethod model offers a lesson: define behavior via a lookup table (metatable/trait implementation), not via syntax. This keeps the language grammar fixed while allowing extensibility. But E6 is the lowest priority feature and may never be needed.

**Assessment:**
- Relevant to AI agents? **No.** Agents do not need extensible operator semantics.
- Does ilo handle this? **Yes, by deliberate omission.** Fixed operator semantics with type-based dispatch is the right choice.
- Minimal addition needed: **None.**

---

### 1.5 Coroutines: Cooperative Multitasking

Lua provides full asymmetric coroutines -- cooperative threads that yield explicitly. Only one coroutine runs at a time. There is no preemption.

```lua
function producer()
  while true do
    local value = generate_value()
    coroutine.yield(value)
  end
end

function consumer(co)
  while true do
    local ok, value = coroutine.resume(co)
    if not ok then break end
    process(value)
  end
end

local co = coroutine.create(producer)
consumer(co)
```

Applications: cooperative multitasking in games (each NPC is a coroutine), lazy generators (produce values on demand), event-loop integration (coroutine yields on I/O, resumes when data arrives), state machines (each state is a yield point).

**Relevance to agent task switching:**

Agent workflows naturally have yield points -- waiting for tool responses, waiting for user input, waiting for external events. Lua's coroutine model maps to this:

| Lua coroutine | Agent workflow |
|---------------|----------------|
| `coroutine.create(fn)` | Start a tool-calling workflow |
| `coroutine.yield(value)` | Wait for tool response |
| `coroutine.resume(co, result)` | Supply tool result, continue workflow |
| Status: suspended/running/dead | Workflow: waiting/executing/complete |

**ilo's current model:** Fully synchronous. Tool calls block. No explicit concurrency mechanism. The runtime handles blocking; the language stays sequential.

**Why ilo should NOT add coroutines:**

1. **Agents do not manage concurrency.** The runtime does. Coroutines require the programmer (the agent) to decide when to yield and resume. This is concurrency reasoning, which agents do poorly -- the retry cost of a missed yield is undiagnosable.

2. **Runtime-level parallelism is cheaper.** ilo's planned `par{...}` block (G4) and dependency-graph-based auto-parallelism achieve the same result with zero agent complexity. The runtime detects independent tool calls and parallelizes them. No yield, no resume, no coroutine management.

3. **Token cost.** Lua coroutine code: `coroutine.create`, `coroutine.yield`, `coroutine.resume` = 3 tokens per operation. ilo's sequential `r=tool! args` = 1 token. Even if coroutines were added, the sequential model is more token-efficient for the common case.

**Where coroutines WOULD help:** Streaming responses (SSE from LLM APIs, WebSocket event loops). A coroutine could process events as they arrive without buffering the entire response. But this is a runtime concern (G6: streams), not a language concern.

**Assessment:**
- Relevant to AI agents? **The concept is relevant (task suspension/resumption), but the mechanism is wrong.** Agents should not manage concurrency explicitly.
- Does ilo handle this? **The runtime should, not the language.** Planned `par{...}` and dependency-graph parallelism address the underlying need.
- Minimal addition needed: **None at the language level.** Runtime-level streaming (G6) would use coroutine-like mechanics internally, invisible to the agent.

---

### 1.6 "Mechanisms Over Policies"

This is Lua's deepest design principle. Rather than providing specific solutions (policies), Lua provides general mechanisms that can be combined to implement those solutions.

**Examples of mechanisms over policies in Lua:**

| Need | Policy (what other languages do) | Mechanism (what Lua does) |
|------|------|------|
| Object-oriented programming | `class` keyword, `extends`, `implements` | Tables + metatables + `__index` chain |
| Modules/packages | `import`/`module` keywords | Tables returned from `require()` |
| Iterators | `iterator` protocol, `yield` | Functions that return functions (closures) |
| Namespaces | `namespace` keyword | Nested tables + environments |
| Exceptions | `try`/`catch`/`throw` | `pcall(fn)` returning `(ok, result_or_error)` |
| Sandboxing | Permission system | Remove functions from `_ENV` |

**How this maps to ilo:**

ilo follows a similar philosophy, but goes further by removing the need for policies entirely in most cases:

| Need | Lua mechanism + user policy | ilo approach |
|------|------|------|
| Error handling | `pcall` → user decides pattern | `R` type + `?` + `!` -- one way, enforced by verifier |
| Data structures | Tables → user decides array vs map | `L`, `R`, records, `M` -- each typed, each verified |
| Iteration | Closure-based iterators | `@v xs{body}` -- one syntax, verified |
| Modules | Tables returned from files | Multi-function programs in one file |
| Tool integration | Host registers C functions | `tool` declarations -- verified before execution |

ilo's twist: **ilo provides mechanisms, but the verifier enforces policies.** Lua provides mechanisms and trusts the programmer. ilo provides mechanisms and trusts the verifier. This is the correct adaptation for agents -- agents generate code quickly but make mistakes that a verifier catches cheaply.

**The deepest parallel:** Lua's `pcall(fn)` returning `(ok, result_or_error)` is structurally identical to ilo's `R ok err`. Both represent fallible operations as values, not control flow. Lua leaves the handling to the programmer; ilo's verifier ensures exhaustive handling via match exhaustiveness (ILO-T024).

**Assessment:**
- Relevant to AI agents? **Yes.** Mechanisms over policies reduces vocabulary size, which reduces token cost.
- Does ilo handle this? **Yes, and improves on it.** Mechanisms + verifier-enforced policies = fewer retries than mechanisms + programmer trust.
- Minimal addition needed: **None.** The philosophy is already embedded in ilo's design.

---

### 1.7 LuaJIT: Performance Through Minimal Design

LuaJIT is widely considered the fastest dynamic language implementation. Key facts:

- **2ns per call** for simple numeric functions on Apple M4 (from ilo's own benchmarks in jit-backends.md).
- The JIT compiler adds only ~32KB of code to the Lua core.
- Trace-based compilation: monitors hot execution paths, compiles them to native code.
- LuaJIT 2.0's hand-written assembler interpreter is faster than most languages' compiled output.

**Why LuaJIT is so fast:** Lua's minimal design directly enables JIT optimization. Fewer language constructs means fewer cases for the trace compiler to handle. Tables have a dual array/hash representation that LuaJIT exploits for fast access. Coroutines enable trace stitching across yield/resume boundaries.

Cloudflare's WAF exemplifies this: Lua code representing firewall rules is JIT-compiled as attacks trigger specific paths. The JIT adapts dynamically to attack patterns -- something a static compiler cannot do.

**ilo's JIT position (from jit-backends.md):**

| Backend | Performance |
|---------|------------|
| ilo Cranelift JIT | 2ns/call |
| LuaJIT | 1ns/call |
| ilo Custom ARM64 JIT | 2ns/call |
| V8 (Node.js) | 18ns/call |
| CPython | 80ns/call |

ilo's JIT is within 2x of LuaJIT for numeric functions (2ns vs 1ns). Both achieve fast codegen through the same mechanism: minimal language design enables efficient code generation.

**What LuaJIT teaches ilo:**

1. **Trace compilation rewards simplicity.** LuaJIT's trace compiler works because Lua has few constructs to trace through. ilo's prefix notation is even simpler -- fixed-arity operators mean the trace compiler never needs to handle operator precedence ambiguity.

2. **Interpreter speed matters.** LuaJIT's hand-written assembler interpreter is faster than most compiled languages. ilo's register VM (66ns) is respectable but 60x slower than the JIT. For short-lived agent programs, the interpreter matters more than the JIT because compilation cost dominates for single-run programs.

3. **JIT eligibility should expand.** ilo's JIT currently handles only pure-numeric functions. LuaJIT traces through tables, strings, and control flow. Expanding ilo's JIT to handle strings, records, and conditionals would bring the interpreter's overhead classes of programs into JIT territory.

**Assessment:**
- Relevant to AI agents? **Moderately.** Agent programs are I/O-bound (waiting for tools), so JIT speed matters less than for compute-bound workloads. But fast execution reduces the "execution" term in the total cost equation.
- Does ilo handle this? **Yes, for numeric code.** Cranelift JIT matches LuaJIT.
- Minimal addition needed: **Expand JIT eligibility** to strings and control flow. This is already tracked in jit-backends.md.

---

### 1.8 Standard Library: What Lua Includes vs Excludes

Lua's standard library is deliberately minimal, organized into eight modules:

| Module | Contents | Size rationale |
|--------|----------|----------------|
| `basic` | `print`, `type`, `tonumber`, `tostring`, `pcall`, `error`, `assert` | Essential primitives |
| `string` | Pattern matching, `format`, `sub`, `rep`, `find`, `gsub` | No full regex (saves ~4000 lines of code) |
| `table` | `insert`, `remove`, `sort`, `concat`, `move` | Minimal table manipulation |
| `math` | `abs`, `ceil`, `floor`, `max`, `min`, `random`, `sin`, `cos`, etc. | ISO C math |
| `io` | `open`, `read`, `write`, `close`, `lines` | ISO C I/O |
| `os` | `clock`, `date`, `time`, `execute`, `getenv`, `remove`, `rename` | ISO C OS interface |
| `coroutine` | `create`, `resume`, `yield`, `status`, `wrap` | Core concurrency |
| `debug` | `getinfo`, `sethook`, `traceback` | Introspection (optional) |

**What is NOT included:** networking, GUI, full regex, JSON, XML, database access, HTTP, cryptography, image processing. All of these come from external libraries (LuaRocks).

**Design principle:** Portability restricts the stdlib to what is available in ISO C. Everything platform-specific is excluded. Lua's string patterns (simpler than PCRE regex) were chosen because full POSIX regex would add 4000+ lines -- bigger than all Lua standard libraries combined.

**Comparison with ilo's builtins:**

| Lua stdlib | ilo builtin | Status |
|-----------|------------|--------|
| `string.len` | `len` | Done |
| `string.sub` | `slc` | Done |
| `string.find` | `has` (contains) | Done |
| `string.format` | `fmt` (I4) | Planned |
| `table.insert` | `+=` (append) | Done |
| `table.sort` | `srt` | Done |
| `table.concat` | `cat` | Done |
| `math.abs` | `abs` | Done |
| `math.ceil` | `cel` | Done |
| `math.floor` | `flr` | Done |
| `math.max` / `math.min` | `max` / `min` | Done |
| `tonumber` / `tostring` | `num` / `str` | Done |
| `pcall` (error handling) | `!` auto-unwrap | Done |
| `io.open` / `io.read` | `fread` / `fwrite` (G8) | Planned |
| `os.time` | `now()` (I6) | Planned |
| `os.getenv` | `env` (I3) | Planned |
| `os.execute` | `run` (I2) | Planned |
| Sockets | `get` / `$` (HTTP) | Done (HTTP only) |
| JSON | `jp` / `jdump` (I1) | Planned |

**What ilo takes from Lua's stdlib philosophy:**

1. **Size budget.** Lua rejects full regex because it is too big relative to the language. ilo should apply the same test: every builtin added is a line in the spec (context cost) and a concept agents must learn. The spec IS ilo's "binary size."

2. **ISO C as boundary.** Lua's boundary is "what ISO C provides." ilo's boundary should be "what an agent needs for tool orchestration." This is more specific: HTTP, JSON, env vars, hashing, time -- the primitives of API-calling workflows.

3. **Pattern matching over regex.** Lua's simplified patterns cover 80% of use cases at 12% of the code size. ilo should consider whether regex (I8) is worth the spec complexity. A simpler pattern syntax (like Lua's `%d`, `%a`, `%w` classes) might be more agent-friendly.

4. **No networking in core.** Lua puts sockets in external libraries. ilo already breaks from this (HTTP `get` is a builtin) because HTTP is fundamental to agent workloads. This is the right deviation -- ilo is not general-purpose.

**Assessment:**
- Relevant to AI agents? **Yes.** The stdlib boundary question directly affects spec size and agent learning cost.
- Does ilo handle this? **Yes, with deliberate choices.** Builtins are selected for agent workloads, not general computing.
- Minimal addition needed: **Stick to the planned roadmap.** The builtins in I1-I9 cover agent needs. Avoid the temptation to add general-purpose stdlib features that agents rarely use.

---

## Part 2: Elixir

Elixir (2012, Jose Valim) runs on the BEAM VM (Erlang's virtual machine), inheriting Erlang's battle-tested concurrency, distribution, and fault tolerance. Elixir adds modern syntax, a powerful macro system, and tooling (Mix, Hex, ExUnit) that make the BEAM accessible to a wider audience. Where Lua teaches ilo about minimalism and embedding, Elixir teaches ilo about composition, error handling, and concurrent orchestration.

### 2.1 `{:ok, val}/{:error, reason}` Pattern

Elixir's dominant error handling pattern uses tagged tuples:

```elixir
case File.read("data.json") do
  {:ok, content} -> process(content)
  {:error, reason} -> handle_error(reason)
end
```

Every fallible function returns `{:ok, value}` or `{:error, reason}`. This is a convention, not a type -- Elixir is dynamically typed. The convention is so strong that it shapes the entire ecosystem:

```elixir
# Pattern matching in function heads
def handle_result({:ok, user}), do: send_welcome(user)
def handle_result({:error, :not_found}), do: {:error, "User not found"}
def handle_result({:error, reason}), do: {:error, "Failed: #{reason}"}
```

**ilo's `R ok err`:**

| Dimension | Elixir `{:ok, val}/{:error, reason}` | ilo `R ok err` |
|-----------|------|------|
| Nature | Convention (tuple + atoms) | Type (verified by compiler) |
| Enforcement | None (runtime crash if pattern mismatch) | Verifier checks exhaustive matching (ILO-T024) |
| Propagation | Manual: `with` or explicit matching | `!` auto-unwrap: 1 token |
| Nesting | `{:ok, {:ok, inner}}` possible (messy) | `R R n t t` possible but discouraged |
| Construction | `{:ok, value}` | `~value` (Ok), `^reason` (Err) |
| Matching | `{:ok, v} ->` | `~v:` |

**ilo validates Elixir's core insight and hardens it:**

Elixir proved that tagged-tuple error handling is more composable than exceptions. ilo took this insight and made it typed: `R n t` in a function signature tells the verifier (and the agent) that this function can fail. The verifier enforces handling. Elixir's convention is "you should handle errors"; ilo's type is "you must handle errors."

The token comparison is stark:

```elixir
# Elixir: 15+ tokens for error propagation
case get_user(id) do
  {:ok, user} -> {:ok, user.name}
  {:error, reason} -> {:error, reason}
end
```

```
-- ilo: 3 tokens for the same thing
u=get-user! id;~u.name
```

**Assessment:**
- Relevant to AI agents? **Foundational.** Every tool call is fallible. Error handling is the most common pattern in agent code.
- Does ilo handle this? **Yes, better than Elixir.** `R` type + `!` auto-unwrap + verifier enforcement = typed, terse, verified.
- Minimal addition needed: **Error-to-Optional bridge** (from swift-kotlin-research.md). `f? args` converting `Err` to `nil`, combined with `??`, would give a 4-token "call with fallback" pattern. Gates on the `O` type (Phase E2).

---

### 2.2 Pipe Operator `|>`

Elixir's pipe operator passes the result of the left expression as the first argument of the right expression:

```elixir
"hello world"
|> String.split(" ")
|> Enum.map(&String.upcase/1)
|> Enum.join(", ")
# => "HELLO, WORLD"
```

Without pipes, this would be nested calls:

```elixir
Enum.join(Enum.map(String.split("hello world", " "), &String.upcase/1), ", ")
```

The pipe makes data flow visible and left-to-right. It is arguably Elixir's most iconic feature.

**ilo's `>>` pipe:**

```
-- ilo pipe (implemented)
spl "hello world" " ">>map upper>>cat ", "

-- Without pipe
a=spl "hello world" " ";b=map upper a;cat b ", "
```

| Dimension | Elixir `\|>` | ilo `>>` |
|-----------|------|------|
| Syntax | `expr \|> func(args)` | `expr>>func args` |
| Argument position | First argument | Last argument |
| Token cost | 1 token per pipe | 1 token per pipe |
| Desugar | Parse-time rewrite | Parse-time rewrite (no new AST node) |
| Works with `!` | No native integration | `f x>>g!>>h` (auto-unwrap in pipe) |

**Key difference: first vs last argument.** Elixir pipes into the first argument; ilo pipes into the last. This is because ilo's builtins take the "subject" as the last argument for some operations, matching the Unix convention where the data flows last. The choice of argument position determines how naturally functions compose.

**Tension with prefix notation:** Pipes are inherently infix/postfix -- they reverse the prefix reading order. Elixir code reads left-to-right because it is infix. ilo code normally reads inside-out because it is prefix. Pipes give ilo a left-to-right alternative for linear chains.

This is a design tension ilo should embrace, not resolve. Prefix is better for nested expressions (`+*a b c`). Pipes are better for linear chains (`f x>>g>>h`). Both exist. The agent should use whichever produces fewer tokens for the specific case.

**What Elixir's pipe culture teaches ilo:**

1. **Pipes encourage small, composable functions.** Elixir functions tend to take one "subject" argument and return a transformed version. This style maps perfectly to ilo's builtins (`len`, `str`, `spl`, `cat`, `rev`, `srt`).

2. **Pipe chains replace intermediate variables.** Elixir developers avoid naming intermediate results. ilo's pipe does the same: `f x>>g>>h` eliminates `a=f x;b=g a;h b` (saves 2 bindings = ~4 tokens per chain).

3. **Error handling in pipes is hard.** Elixir's pipes break when a step returns `{:error, reason}` instead of the expected value. The `with` construct exists partly to handle this. ilo's `!` in pipes (`f x>>g!>>h`) solves this more elegantly -- auto-unwrap at any pipe stage.

**Assessment:**
- Relevant to AI agents? **Yes.** Linear tool-call chains are the most common agent pattern. Pipes save ~2 tokens per step.
- Does ilo handle this? **Yes.** `>>` is implemented and works with `!` for error propagation.
- Minimal addition needed: **None.** The current design is sound. Encourage pipe usage in spec examples.

---

### 2.3 Pattern Matching Everywhere

Elixir integrates pattern matching into every control structure:

```elixir
# In function heads
def process(%User{role: :admin, name: name}), do: "Admin: #{name}"
def process(%User{role: :user, name: name}), do: "User: #{name}"

# In case
case fetch_user(id) do
  {:ok, %{name: name, verified: true}} -> welcome(name)
  {:ok, %{verified: false}} -> {:error, "Not verified"}
  {:error, reason} -> {:error, reason}
end

# In with
with {:ok, user} <- get_user(id),
     {:ok, profile} <- get_profile(user),
     {:ok, _} <- validate(profile) do
  {:ok, profile}
end

# In function guards
def classify(score) when score >= 90, do: "A"
def classify(score) when score >= 80, do: "B"
def classify(_score), do: "C"
```

**ilo's pattern matching:**

```
-- Match arms (like Elixir case)
?r{~v:use v;^e:handle e}

-- Guards (like Elixir function guards)
>=sp 1000 "gold";>=sp 500 "silver";"bronze"

-- Auto-unwrap (like Elixir with)
u=get-user! id;p=get-profile! u;validate! p;~p
```

**Comparison:**

| Pattern type | Elixir | ilo | Token comparison |
|-------------|--------|-----|-----------------|
| Result matching | `case r do {:ok, v} -> ... {:error, e} -> ... end` | `?r{~v:...;^e:...}` | ~15 vs ~7 |
| Guard chain | Three `def classify` clauses | `>=sp 1000 "gold";>=sp 500 "silver";"bronze"` | ~20 vs ~8 |
| Happy-path chain | `with {:ok, a} <- f(), {:ok, b} <- g(a) do` | `a=f!;b=g! a;~b` | ~25 vs ~5 |
| Literal match | `case tier do "gold" -> 100; "silver" -> 50; _ -> 10 end` | `?tier{"gold":100;"silver":50;_:10}` | ~15 vs ~8 |

ilo's pattern matching is consistently 2-3x more token-efficient than Elixir's. The savings come from:
- No `case`/`do`/`end` delimiters (replaced by `?` and `{}`/`;`)
- No `<-` pattern operator (replaced by `:` in match arms)
- No `{:ok, v}` tuple syntax (replaced by `~v`)
- No `when` keyword (guards are bare expressions)

**What Elixir's pattern matching teaches ilo:**

1. **Multi-clause functions are powerful.** Elixir's ability to define multiple function clauses with different patterns is expressive. ilo does not support this -- one function, one body. Instead, ilo uses guards at the top of the body, which is more token-efficient for the common case (2-4 branches) but less elegant for many branches.

2. **Deep structural matching.** Elixir can match nested structures: `%{user: %{address: %{city: city}}}`. ilo can only match on Result (`~v`/`^e`) and literal values. Matching on record fields would require destructuring (F7 in CONTROL-FLOW.md).

3. **Guards on parameters.** Elixir's `when score >= 90` on function parameters catches invalid input at the function boundary. ilo's guards do the same but inside the body, which is fine for agent code (single-entry functions).

**Assessment:**
- Relevant to AI agents? **Yes.** Pattern matching is the core of agent error handling and branching.
- Does ilo handle this? **Yes, more concisely.** Guards + `?` match + `!` auto-unwrap cover the common patterns.
- Minimal addition needed: **Destructuring bind (F7)** for extracting multiple record fields in one statement. This is already planned.

---

### 2.4 `with` Statement: Multi-Step Error Handling

Elixir's `with` chains pattern-matched steps, short-circuiting on the first failure:

```elixir
with {:ok, user} <- get_user(id),
     {:ok, profile} <- get_profile(user.id),
     {:ok, _} <- authorize(profile),
     {:ok, _} <- send_notification(profile) do
  {:ok, "Done"}
else
  {:error, :not_found} -> {:error, "User not found"}
  {:error, :unauthorized} -> {:error, "Access denied"}
  {:error, reason} -> {:error, "Failed: #{reason}"}
end
```

This is ~40 tokens for a 4-step workflow with 3 error handlers.

**ilo equivalent with `!`:**

```
u=get-user! id;p=get-profile! u.id;authorize! p;send-notification! p;~"Done"
```

This is ~10 tokens. The `!` on each call does what `with`'s `<-` does: unwrap Ok, propagate Err.

The entire `else` block in Elixir's `with` is implicit in ilo -- `!` propagates the exact error from whichever step fails. If the agent needs custom error messages:

```
u=get-user id;?u{^_:^"User not found";~u:p=get-profile u.id;?p{^_:^"Access denied";~p:send-notification! p;~"Done"}}
```

This is more verbose (~20 tokens) but still half the tokens of Elixir's `with` and includes custom error messages.

**The key insight:** ilo's `!` is more concise than Elixir's `with` for the happy path (the 90% case). Elixir's `with` + `else` is more expressive for custom error handling (the 10% case). For agents, optimizing the 90% case saves more total tokens.

**Assessment:**
- Relevant to AI agents? **The underlying pattern (multi-step error propagation) is the core agent pattern.**
- Does ilo handle this? **Yes, better than Elixir for the common case.** `!` is 1 token vs `with`'s ~5-token overhead per step.
- Minimal addition needed: **None.** The current `!` + `?` combination covers both simple propagation and custom error handling.

---

### 2.5 OTP/GenServer: Actor Model and Supervision Trees

OTP (Open Telecom Platform) is Elixir's framework for building concurrent, fault-tolerant systems. The core abstractions:

1. **GenServer:** A generic server process. Holds state, handles synchronous calls (`handle_call`) and asynchronous messages (`handle_cast`). Standardized callbacks for init, terminate, and code change.

2. **Supervisor:** Watches child processes. When a child crashes, the supervisor restarts it according to a strategy (`:one_for_one`, `:one_for_all`, `:rest_for_one`).

3. **Supervision trees:** Supervisors supervise other supervisors, forming a tree. Failure containment: a crash in a leaf node only affects its subtree, not the whole system.

4. **"Let it crash" philosophy:** Instead of defensive programming (catch every possible error), design processes to crash cleanly and let the supervisor restart them. This simplifies individual process logic dramatically.

**Relevance to agent orchestration:**

| OTP concept | Agent equivalent | ilo mapping |
|-------------|-----------------|-------------|
| GenServer (stateful process) | Tool with session state (WebSocket, database connection) | `tool` declaration with timeout/retry |
| Supervisor (restart on failure) | Agent retry logic | `retry:n` in tool declaration |
| Supervision tree | Multi-agent workflow hierarchy | Program dependency graph |
| "Let it crash" | "Let it fail, propagate error" | `!` auto-unwrap (propagate `^e`) |
| Message passing | Tool call/response | `R ok err` return values |

**"Let it crash" maps to ilo's error propagation.** In both Elixir and ilo, individual operations are allowed to fail. The system-level response to failure is what matters:

```elixir
# Elixir: GenServer crashes, supervisor restarts it
# The GenServer code is simple -- no try/catch needed
def handle_call(:fetch, _from, state) do
  data = dangerous_operation!()  # may crash -- that's OK
  {:reply, data, state}
end
```

```
-- ilo: tool call fails, error propagates, agent handles at top level
-- The function code is simple -- no defensive checks needed
f uid:t>R t t;u=get-user! uid;p=get-profile! u.id;~p.name
```

Both achieve simplicity through the same insight: separate the "what to do" (function/GenServer logic) from the "what to do when it fails" (supervisor/caller error handling).

**What ilo should NOT take from OTP:**

1. **Explicit process management.** `GenServer.start_link`, `Supervisor.init`, `DynamicSupervisor.start_child` -- this is infrastructure ceremony that agents should never write.

2. **Message passing syntax.** `send`, `receive`, mailboxes -- agents should not reason about message ordering and selective receive.

3. **Hot code reloading.** OTP supports replacing code in running processes. Agent programs are short-lived; regeneration replaces reloading.

**What ilo SHOULD take from OTP:**

1. **Declarative retry/restart.** ilo's `tool` declaration already has `retry:n`. This is the right level of abstraction -- the agent declares intent ("retry twice"), the runtime implements policy.

2. **Supervision as runtime concern.** If ilo programs run in an agent loop, the loop IS the supervisor. A failed program generates an error; the agent loop restarts with error context. This maps to OTP's supervisor without language-level constructs.

3. **Failure isolation.** OTP's per-process isolation (one crash does not affect others) maps to ilo's per-function isolation. A tool call failure in one function does not corrupt another function's state (there is no shared mutable state).

**Assessment:**
- Relevant to AI agents? **The philosophy ("let it crash", declarative retry, failure isolation) is highly relevant. The mechanisms (GenServer, Supervisor, process management) are not.**
- Does ilo handle this? **Partially.** `tool retry:n` handles declarative retry. `!` handles error propagation. The agent loop acts as a supervisor.
- Minimal addition needed: **Runtime-level supervision** for the agent loop (D4). The language itself does not need OTP constructs.

---

### 2.6 Streams: Lazy Enumeration

Elixir distinguishes eager `Enum` from lazy `Stream`:

```elixir
# Eager: processes entire list at each step
1..1000
|> Enum.filter(&(&1 > 500))
|> Enum.map(&(&1 * 2))
|> Enum.take(5)
# Creates intermediate lists at each step

# Lazy: processes one element at a time
1..1000
|> Stream.filter(&(&1 > 500))
|> Stream.map(&(&1 * 2))
|> Enum.take(5)
# No intermediate lists -- elements flow through the pipeline
```

Streams are valuable when:
- The data is large (millions of elements)
- The data is infinite (sensor readings, log streams)
- You only need a subset (early termination saves work)
- Memory is constrained (one element at a time vs entire list)

**Relevance to AI agent workloads:**

| Use case | Eager (ilo `@`) | Lazy (stream) |
|----------|------|------|
| Process 10 tool results | Fine | Unnecessary overhead |
| Filter 100 records | Fine | Unnecessary |
| Process 1M log lines | Memory explosion | Process-one-at-a-time |
| Real-time event stream | Cannot hold all events | Process as they arrive |
| LLM token stream | Must buffer entire response | Process token-by-token |

Most agent workloads deal with small collections (10-100 items). Streams add complexity without benefit for these cases. But two agent scenarios need streams:

1. **Log processing.** Agent analyzing application logs cannot load millions of lines.
2. **LLM streaming.** SSE responses from LLM APIs arrive token-by-token.

**ilo's planned streaming (G6):** `@line stream{...}` for line-by-line processing. This is the right abstraction -- lazy iteration with familiar syntax. The `@` sigil unifies eager list iteration and lazy stream iteration.

**Assessment:**
- Relevant to AI agents? **For most workloads, no.** For log processing and LLM streaming, yes.
- Does ilo handle this? **Not yet.** `@` is eager. G6 (streams) is planned.
- Minimal addition needed: **G6 stream iteration.** The `@line stream{body}` pattern handles the agent use cases without exposing lazy stream combinators. Keep it simple -- agents do not need `Stream.unfold` or `Stream.resource`.

---

### 2.7 Phoenix LiveView: Real-Time Web Patterns

Phoenix LiveView enables real-time web UIs with server-rendered HTML over WebSockets. Key patterns:

1. **Declarative state management.** The server holds state; the client renders diffs. No client-side state management.
2. **Efficient diffs.** LiveView tracks which assigns changed and sends only the diff. 5-10x faster than full HTML replacement.
3. **PubSub for broadcast.** Changes propagate to all connected clients via PubSub.
4. **Async data loading.** `assign_async` loads data in the background, updates the UI when ready.

**Relevance to ilo:** Mostly not relevant. ilo programs are tool orchestration scripts, not web applications. However, two patterns are worth noting:

1. **Server-driven UI.** LiveView's model (server holds truth, client renders) maps to "agent holds program, runtime executes." The agent does not need to understand the execution environment; it generates the program and the runtime handles execution.

2. **Diff-based updates.** If ilo programs are edited incrementally (agent modifies one function), the runtime could re-verify only the changed function and its dependents, not the entire program. This is "diff-based verification" -- analogous to LiveView's diff-based rendering.

**Assessment:**
- Relevant to AI agents? **No, for the web patterns. The diff/incremental update concept is tangentially useful.**
- Does ilo handle this? **N/A.**
- Minimal addition needed: **None.** If incremental verification becomes important, it is a runtime optimization, not a language feature.

---

### 2.8 Mix: Build Tool and Dependency Management

Mix is Elixir's build tool, providing:

```elixir
# Create project
mix new my_app

# Dependencies (in mix.exs)
defp deps do
  [{:jason, "~> 1.4"}, {:plug, "~> 1.14"}]
end

# Fetch dependencies
mix deps.get

# Run tests
mix test

# Compile
mix compile

# Custom tasks
mix my_custom_task
```

**Relevance to ilo:** ilo programs are single files (or inline code). There is no build step, no dependency resolution, no project structure. This is deliberate -- agent programs are generated fresh for each task.

However, Mix's `mix.exs` as a declarative manifest has a parallel: ilo's `tool` declarations at the top of a program ARE the dependency manifest. They declare what external capabilities the program needs:

```
tool get-user"Retrieve user by ID" uid:t>R profile t timeout:5,retry:2
tool send-email"Send an email" to:t subject:t body:t>R _ t timeout:10,retry:1
```

This IS `mix.exs` for a single-use program. No separate manifest file needed. The program declares its own dependencies inline.

**Assessment:**
- Relevant to AI agents? **The concept (declarative dependency manifest) is relevant. The build tool machinery is not.**
- Does ilo handle this? **Yes.** Tool declarations serve as inline dependency manifests.
- Minimal addition needed: **None.**

---

## Part 3: Cross-Cutting Themes

### 3.1 Pipe-Oriented Thinking (Elixir `|>` vs ilo prefix + `>>`)

Elixir's pipe operator fundamentally shaped how Elixir developers think about programs: data flows left-to-right through a series of transformations. This "pipeline thinking" maps to how agents naturally compose tool calls -- get data, transform it, pass it to the next step.

**Three composition models compared:**

```elixir
# Elixir: pipe-oriented (left to right)
id
|> get_user()
|> get_profile()
|> extract_email()
|> send_notification()
```

```lua
-- Lua: nested calls (inside out)
send_notification(extract_email(get_profile(get_user(id))))
```

```
-- ilo: bind-first (top to bottom, sequential)
u=get-user! id;p=get-profile! u;e=extract-email p;send-notification! e

-- ilo: pipe (left to right)
get-user! id>>get-profile!>>extract-email>>send-notification!
```

**Why both prefix and pipe coexist in ilo:**

Prefix notation excels at nested expressions where operators combine values:
```
-- Prefix: natural for math/logic (3 tokens for nested ops)
+*a b c        -- (a * b) + c

-- Pipe: awkward for math (*a b>>+ c feels wrong)
```

Pipes excel at linear chains where each step feeds the next:
```
-- Pipe: natural for sequential processing
spl t " ">>rev>>cat ", "

-- Prefix: requires intermediate binds
a=spl t " ";b=rev a;cat b ", "
```

The guideline for agents: **use prefix for expressions, pipes for chains.** This is two ways to do one thing, which normally violates the manifesto. But the two forms are complementary, not redundant -- they optimize different patterns. Prefix saves tokens in nested expressions (22% savings vs infix). Pipes save tokens in linear chains (~4 tokens per 3-step chain).

---

### 3.2 Embeddability (Lua as Language-in-Runtime vs ilo as Agent-Language)

Both Lua and ilo are designed to be embedded, but in different hosts:

| Dimension | Lua | ilo |
|-----------|-----|-----|
| Host | C/C++ application | AI agent runtime |
| Host provides | C functions registered in Lua state | Tool declarations from MCP/config |
| Guest provides | Scripts extending host behavior | Programs orchestrating tools |
| Communication | C API virtual stack | Value <-> JSON at tool boundary |
| Size constraint | Memory (200KB binary) | Tokens (~16-line spec) |
| Sandbox | Remove libraries from state | Closed-world verification |
| Lifecycle | Long-running (game loop, server) | Short-lived (generate, verify, execute, discard) |

**The key insight from Lua's embedding model:**

Lua's embeddability shapes its language design -- features that cannot be exposed through the C API are avoided. Similarly, ilo's "embeddability" in agent runtimes should shape its design: features that cannot be expressed in the ~16-line compact spec (`ilo help ai`) are too complex. If an agent cannot learn a feature from the spec, the feature increases total token cost through generation errors and retries.

**Lua's API-first design principle applied to ilo:** Every ilo feature should be "spec-first" -- describable in a few lines of the compact spec. The `!` operator passes this test ("call + auto-unwrap Result -- propagates error"). A hypothetical coroutine system would fail it (requires explaining yield, resume, scheduling, and error handling across yield boundaries).

---

### 3.3 Minimalism Spectrum (Lua's 22 vs ilo's 0)

The minimalism spectrum for relevant languages:

```
More keywords                                     Fewer keywords
Java (~50)  Python (~35)  Go (~25)  Lua (~22)  Forth (~0)  ilo (~6)
```

Lua sits near the minimal end of mainstream languages. ilo sits at the extreme -- ~6 abbreviated keywords (`type`, `tool`, `wh`, `ret`, `brk`, `cnt`) plus single-character sigils for all control flow.

**What each level of minimalism costs and gains:**

| Level | Keywords | Generation cost | Learning cost | Error clarity |
|-------|----------|----------------|---------------|---------------|
| Java ~50 | 50 reserved words | High (many valid next-tokens) | High (large spec) | High (verbose errors) |
| Python ~35 | 35 reserved words | Medium | Medium | Medium |
| Go ~25 | 25 reserved words | Low-medium | Low | High |
| Lua ~22 | 22 reserved words | Low | Low | Medium (runtime) |
| ilo ~6 | ~6 abbreviated keywords, ~15 sigils | Very low | Very low (with spec) | High (verified + suggestions) |

**Lua and ilo both prove that fewer keywords work.** Lua has run production systems for 30 years with 22 keywords. ilo achieves 10/10 LLM generation accuracy with ~6 abbreviated keywords and sigil-based syntax (tested across 4 task types with claude-haiku-4-5). The evidence suggests that the lower bound for keywords is zero, as long as:

1. The spec is available in context
2. Sigils are unambiguous (no overloading)
3. Examples demonstrate every construct
4. Error messages reference the spec

**The risk ilo should monitor:** As ilo adds features, the sigil count grows. Currently: `?`, `!`, `~`, `^`, `@`, `>`, `>>`, `.?`, `??`, `=`, `;`, `{`, `}`, `:`, `--`. That is 15 structural tokens. If this grows to 25+, the cognitive load approaches Lua's 22 keywords, but with less self-documentation. Each new sigil should pass the test: "Does this save more tokens than the spec line needed to explain it?"

---

## Part 4: Feature-by-Feature Assessment Matrix

### Lua Features

| Feature | Agent relevance | ilo coverage | Addition needed |
|---------|----------------|-------------|-----------------|
| 22 keywords (minimalism) | High | Already surpassed (~6 abbreviated keywords) | None |
| Tables (universal data) | Medium | Typed alternatives (`L`, `R`, records) | Maps (E4) |
| Embeddability / C API | High | Tool declarations + ToolProvider | MCP integration (D2) |
| Metatables / metamethods | None | Deliberately omitted | None |
| Coroutines | Low (concept useful, mechanism wrong) | Runtime parallelism (G4) | None at language level |
| "Mechanisms over policies" | High (philosophical) | Mechanisms + verifier-enforced policies | None |
| LuaJIT performance | Medium | Cranelift JIT matches LuaJIT for numerics | Expand JIT eligibility |
| Minimal stdlib | High (spec size budget) | Agent-focused builtins | Stick to roadmap (I1-I9) |
| String patterns (not regex) | Medium | `has`, `spl`, `cat` + planned I8 | Consider simpler patterns |
| `pcall` as error handling | High | `R` + `?` + `!` (superior) | None |
| Closures / first-class functions | Medium | Not yet (E5: generics + lambdas) | Phase E5 |
| `nil` as value | Low | `_` (nil type) + planned `O` (optional) | Phase E2 |

### Elixir Features

| Feature | Agent relevance | ilo coverage | Addition needed |
|---------|----------------|-------------|-----------------|
| `{:ok, val}/{:error, reason}` | Foundational | `R ok err` + `~`/`^` (typed, verified) | Error-to-Optional bridge |
| Pipe `\|>` | High | `>>` (implemented, works with `!`) | None |
| Pattern matching everywhere | High | `?` match + guards (more concise) | Destructuring (F7) |
| `with` statement | High | `!` auto-unwrap (more concise) | None |
| OTP / GenServer | Philosophy relevant, mechanisms not | `tool retry:n` + `!` propagation | Runtime supervision (D4) |
| Streams (lazy) | Low (most agent data is small) | `@` is eager | G6 for streams |
| Phoenix LiveView | Not relevant | N/A | None |
| Mix build tool | Concept relevant | Tool declarations as inline manifest | None |
| Macro system | Not relevant | No metaprogramming | None |
| `Enum.map/filter/reduce` | High | `@` loop + bind-first pattern | `map`/`flt`/`fld` builtins (needs E5) |
| Protocols (ad-hoc polymorphism) | Not relevant | Deliberately omitted | None |
| Behaviours (callback contracts) | Low | N/A | None |
| Sigils (`~r`, `~w`, etc.) | Low | Sigils used differently (structural) | None |
| `Task.async` + `Task.await` | Medium | Planned `par{...}` (G4) | Runtime parallelism |

---

## Part 5: Synthesis and Recommendations

### Already validated by Lua and Elixir (no action needed)

1. **Zero-keyword minimalism.** Lua's 22-keyword success validates reducing keywords. ilo's 0-keyword sigil approach is the logical extreme and works (10/10 generation accuracy).

2. **Value-based error handling.** Both Lua's `pcall` returning `(ok, result)` and Elixir's `{:ok, val}/{:error, reason}` validate ilo's `R ok err`. ilo adds type verification and `!` auto-unwrap, making it strictly superior for agent workloads.

3. **Pipe operator.** Elixir proved pipes transform how developers think about composition. ilo's `>>` captures this benefit at 1 token per pipe stage.

4. **Mechanisms over policies.** Lua's philosophy of providing general mechanisms instead of specific features is validated by 30 years of use. ilo follows this with its small set of constructs (`?`, `@`, `!`, `~`, `^`) that combine to handle any control flow pattern.

5. **Embeddability as design constraint.** Lua's embeddability shaped its minimalism (nothing that cannot be expressed through the C API). ilo's embeddability in agent runtimes should follow the same principle (nothing that cannot be explained in the compact spec).

### Validated as planned (proceed with roadmap)

1. **Maps (E4)** -- Lua tables handle dynamic key-value natively. ilo needs `M t n` for tool responses with variable keys.

2. **Optional type (E2)** -- Both Lua's nil propagation issues and Elixir's `nil` handling validate the need for typed optionals. The `O` type + verifier enforcement would catch nil crashes pre-execution.

3. **Error-to-Optional bridge** -- Elixir's pattern of converting errors to nil (via `case`/default) and Lua's `pcall` returning false on error both point to needing `f? args` + `??` for the "call with fallback" pattern.

4. **Runtime parallelism (G4)** -- Elixir's `Task.async`/`Task.await` and Lua's coroutines both address concurrent execution. ilo's `par{...}` block is the right abstraction: the runtime parallelizes, the language stays sequential.

5. **Streams (G6)** -- Elixir's `Stream` module proves lazy enumeration is needed for large data. ilo's `@line stream{body}` is the right minimal approach.

6. **Destructuring bind (F7)** -- Elixir's pattern matching in bindings (`%{name: n} = user`) saves tokens for record field extraction. ilo's planned `{n;e}=expr` matches this.

### New insights from this research

1. **Spec size as binary size.** Lua's 200KB binary constraint shaped its design. ilo's ~16-line compact spec constraint should shape its design with equal discipline. Every feature added is a spec line; every spec line is a context token. Apply Lua's ruthless size budgeting to ilo's spec.

2. **String patterns, not full regex.** Lua rejected POSIX regex because the implementation (4000+ lines) was bigger than all Lua standard libraries combined. ilo should consider whether the planned regex support (I8) could use a simplified pattern syntax (like Lua's `%d`, `%a`, `%w`) instead of full PCRE. Agent workloads rarely need lookahead, backreferences, or non-greedy matching. Simpler patterns = smaller spec section = fewer agent learning tokens.

3. **Lua's stdlib boundary = ilo's builtin boundary.** Lua includes only what ISO C provides. ilo should include only what tool orchestration requires: HTTP, JSON, env, hashing, time, file I/O, string basics. Resist the temptation to add general-purpose builtins (matrix math, compression, image processing) that agents rarely need.

4. **OTP's "let it crash" = ilo's `!` propagation.** The philosophical alignment is exact. Both say: "individual operations should be simple; error handling belongs at a higher level." In OTP, the supervisor handles crashes. In ilo, the caller (or the agent loop) handles `^e`. This should be emphasized in ilo's documentation as a design principle, not just a feature.

5. **Coroutines are runtime, not language.** Both Lua's coroutines and Elixir's processes are concurrency mechanisms exposed to the programmer. For agents, concurrency should be invisible -- the runtime detects parallelism from the dependency graph. This is ilo's "graph-native" principle applied to execution, validated by comparing Lua/Elixir's explicit models with the complexity they impose on code generators.

### Features to NOT add (validated by Lua/Elixir analysis)

| Feature | Why not |
|---------|---------|
| Metatables / operator overloading | Agents generate concrete code, not abstractions |
| Coroutines / explicit concurrency | Agents should not manage yield/resume; runtime handles parallelism |
| OTP-style process management | Too much ceremony; agent programs are short-lived |
| Macro system (Elixir) | Metaprogramming adds spec complexity without reducing agent token cost |
| Protocol / behavior system | Agents do not write reusable libraries |
| Full regex engine | Consider Lua's simplified patterns instead |
| Module / namespace system | Programs are small single files; tool declarations are the namespace |
| Hot code reloading | Programs are regenerated, not patched |

---

## Appendix A: Token Comparison — Lua vs Elixir vs ilo

### Task: Fetch API, extract field, handle error

```lua
-- Lua (with http library): ~30 tokens
local http = require("socket.http")
local json = require("cjson")
local body, code = http.request(url)
if code ~= 200 then
  return nil, "HTTP error: " .. code
end
local data = json.decode(body)
return data.name
```

```elixir
# Elixir (with HTTPoison): ~25 tokens
case HTTPoison.get(url) do
  {:ok, %{status_code: 200, body: body}} ->
    {:ok, Jason.decode!(body)["name"]}
  {:ok, %{status_code: code}} ->
    {:error, "HTTP error: #{code}"}
  {:error, reason} ->
    {:error, "Request failed: #{reason.reason}"}
end
```

```
-- ilo: ~5 tokens
n=jp! ($!url) "name";~n
```

### Task: Process list, filter, transform

```lua
-- Lua: ~20 tokens
local result = {}
for _, item in ipairs(items) do
  if item.score >= 100 then
    table.insert(result, item.name)
  end
end
return result
```

```elixir
# Elixir: ~12 tokens
items
|> Enum.filter(&(&1.score >= 100))
|> Enum.map(& &1.name)
```

```
-- ilo: ~8 tokens
@i its{>=i.score 100{i.name}}
```

### Task: Multi-step workflow with error handling

```lua
-- Lua: ~40 tokens
local ok, user = pcall(get_user, id)
if not ok then return nil, "User lookup failed: " .. user end
local ok2, profile = pcall(get_profile, user.id)
if not ok2 then return nil, "Profile lookup failed: " .. profile end
local ok3, _ = pcall(send_email, profile.email, "Hello", msg)
if not ok3 then return nil, "Email failed" end
return true
```

```elixir
# Elixir: ~30 tokens
with {:ok, user} <- get_user(id),
     {:ok, profile} <- get_profile(user.id),
     {:ok, _} <- send_email(profile.email, "Hello", msg) do
  {:ok, true}
else
  {:error, reason} -> {:error, reason}
end
```

```
-- ilo: ~8 tokens
u=get-user! id;p=get-profile! u.id;send-email! p.email "Hello" msg;~true
```

**Pattern:** ilo achieves 3-5x token reduction vs Lua and 2-4x vs Elixir for typical agent tasks, while maintaining equivalent or stronger safety guarantees through static verification.

---

## Appendix B: Language Philosophy Comparison

| Principle | Lua | Elixir | ilo |
|-----------|-----|--------|-----|
| Core insight | One mechanism per concern | Data flows through pipes | Minimize total tokens |
| Data structure | One (table) | Many (tuple, list, map, struct) | Typed set (L, R, record, M) |
| Error model | pcall/xpcall (exception-like) | {:ok}/{:error} convention | R type + verifier enforcement |
| Concurrency | Coroutines (cooperative) | Processes (preemptive, BEAM) | Runtime-managed (planned) |
| Extension | Metatables (user extensible) | Protocols + macros | Tools (host extensible) |
| Typing | Dynamic, untyped | Dynamic, convention-typed | Static, verified |
| Composition | Nested calls | Pipes (`\|>`) | Prefix nesting + pipes (`>>`) |
| Community motto | "Mechanisms over policies" | "Let it crash" | "Total tokens from intent to code" |
| Stdlib philosophy | ISO C boundary | Batteries included (OTP) | Agent workload boundary |
| Target user | C/C++ host application | Web/distributed systems developer | AI agent runtime |
| Spec size | ~100 pages | ~2000+ pages (with OTP) | ~16 lines (compact) / ~5 pages (full) |