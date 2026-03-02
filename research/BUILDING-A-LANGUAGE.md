# Building a Programming Language: Research

How people start programming languages from scratch, and what this means for ilo.

## The Pipeline

Every language follows roughly the same path:

```
Source → Lexer → Tokens → Parser → AST → [Interpreter OR Compiler] → Output
```

Most people start with the grammar and a handful of example programs (we have both).

## Parser Approaches

Most production languages use **hand-written recursive descent parsers** (Go, Rust, Python, Swift, Lua). Reasons: better error messages, more performance, more flexibility for context-sensitive quirks.

For expression-heavy languages, a **Pratt parser** handles operator precedence naturally. Given ilo's sigil-based syntax (`?`, `!`, `~`, `@`, `>`), this is the right fit.

| Tool | Type | Best For |
|------|------|----------|
| Hand-written recursive descent | Manual | Production languages, best errors |
| Pratt parser | Manual technique | Operator precedence, expression-heavy |
| ANTLR | Parser generator (LL) | JVM ecosystems, complex grammars |
| PEG parsers (pest, nom) | Combinators | Rust/JS ecosystems, unified lexing+parsing |
| tree-sitter | Incremental generator | Editor integration only, not the language parser |

## The Spectrum: Simple → Complex

### Level 1: Embedded DSL (days)
Language as functions/macros in a host language. No parser needed.

### Level 2: Transpiler (days to weeks)
Parse ilo, output Python/JS, run with host runtime. **Fastest path to execution.** BAML took this approach — transpiles to Python/TypeScript/Ruby, compiler written in Rust.

### Level 3: Tree-Walking Interpreter (weeks)
Parse to AST, walk the tree to execute. Simple, slow, complete. This is Part I of Crafting Interpreters.

### Level 4: Bytecode VM (weeks to months)
Compile AST to bytecode, execute on custom VM. Much faster. Lua, Python, Ruby all use this. Part II of Crafting Interpreters.

### Level 5: Native Compiler via LLVM (months)
Compile to LLVM IR, get native code + optimisations for free. Rust, Swift, Zig use this. Covered by [createlang.rs](https://createlang.rs/).

### Level 6: Custom Backend (months to years)
Only Go does this. Almost never necessary.

## How Famous Languages Started

**Python** — Guido's Christmas vacation project (1989). Tree-walking interpreter in C. Released 0.9.0 in 1991.

**Lua** — Evolved from data-description languages at PUC-Rio. Bytecode VM in C from the start. Entire implementation is ~30,000 lines of C.

**Rust** — Graydon Hoare's personal project (2006). First compiler in OCaml (~38,000 lines). No type checker, terrible codegen. Rewrote in Rust (self-hosted) years later. The OCaml prototype was crucial for exploring the design space cheaply.

**Go** — Designed at Google. First compiler in C, later rewritten in Go. Custom backend (not LLVM) because Ken Thompson had decades of codegen experience.

**Zig** — Started in C++, later self-hosted. Uses LLVM backend.

**Common pattern**: prototype in an existing language → get something running → iterate on design → optionally self-host later.

## Modern Developments (2024-2026)

### AI-Assisted Language Development

**Steve Klabnik's Rue language** is the headline story. A 13-year Rust veteran built a new systems language from scratch using Claude AI:
- ~100,000 lines of Rust in 11 days
- ~700+ commits
- Working compiler in roughly two weeks
- Klabnik directed and reviewed; Claude wrote most of the code

This is directly relevant to ilo — a solo developer with a clear design vision using AI to accelerate implementation.

### Rust as Implementation Language

Rust is the dominant choice for new language tooling: BAML, Rue, SWC, Ruff, Oxc all use it. [createlang.rs](https://createlang.rs/) (completed Dec 2025) provides a full tutorial from lexer through LLVM JIT.

### Other Modern Expectations

- **LSP support early** — IDE integration is expected from the start now
- **tree-sitter grammar** — gives syntax highlighting in most editors
- **WASM as a target** — browser/edge portability via LLVM

## Interpreted vs Compiled: What's Right for ilo?

This is the key question. ilo has a unique position: it's a language for AI agents, with verification-before-execution as the core value prop.

### The Answer: Both (in phases)

**Phase 1: Verifier** — *completed*
- The verifier is ilo's differentiator — built first
- Originally planned to transpile to Python, but skipped directly to the bytecode VM

**Phase 2: Bytecode VM (compiled to bytecode)** — *completed*
- Register-based bytecode VM with custom opcodes
- Full control over execution model, tool orchestration, typed-shell semantics
- See `research/jit-backends.md` for benchmark results (66ns/call interpreted)

**Phase 3: JIT compilation** — *in progress*
- Cranelift JIT backend: 2ns/call for numeric functions
- Custom ARM64 JIT backend: 2ns/call
- Within 2x of LuaJIT (1ns) for pure-numeric workloads
- See `research/jit-backends.md` for full benchmark comparison

**Phase 4: Optional native compilation** — *future*
- LLVM backend if performance matters beyond JIT
- WASM target for portability
- Only if the use case demands it

### Why Not Just One?

- Pure interpreter: too slow for compute-heavy work, but ilo programs are mostly I/O (tool calls), so speed may not matter
- Pure compiler: too much upfront work, delays getting real feedback
- Transpiler first: fast feedback loop, throwaway code, replace later

### The Type Checking Question

Type checking happens **at verification time, before execution** — regardless of whether execution is interpreted or compiled. The verifier walks the AST, resolves calls, checks types, validates the function graph. This is independent of how you run the code afterward.

In ilo's model:
1. Agent generates ilo source
2. **Verifier** parses + type-checks (catches errors cheaply)
3. **Runtime** executes (interpreter, transpiler, or compiler — doesn't matter to the agent)

The verifier is the product. The runtime is an implementation detail.

## Path Taken

### Step 1: Lexer + Parser — *done*
Hand-written recursive descent + Pratt parser in Rust. ilo's grammar is small and sigil-based. [createlang.rs](https://createlang.rs/) was a useful reference for the Rust path.

### Step 2: Verifier — *done*
Walks the AST. Checks: all calls resolve to known functions, all types align, all dependencies exist. Returns structured errors. This is ilo's core innovation.

### Step 3: Register VM — *done*
Bytecode VM with register-based architecture. Skipped the transpiler-to-Python phase entirely.

### Step 4: JIT backends — *in progress*
Cranelift and custom ARM64 backends. See `research/jit-backends.md` for benchmarks.

### Step 5: Ship and Learn
Get agents writing and running ilo programs. Learn from real usage.

## Key Resources

### Books
- **[Crafting Interpreters](https://craftinginterpreters.com/)** — the single best starting point, free online
- **[Writing an Interpreter in Go](https://interpreterbook.com/)** — practical, project-based
- **[Create Your Own Programming Language with Rust](https://createlang.rs/)** — full pipeline including LLVM JIT, completed Dec 2025
- **Types and Programming Languages** by Pierce — for formalising ilo's type system later

### Blogs & Articles
- [Steve Klabnik: Thirteen Years of Rust and the Birth of Rue](https://steveklabnik.com/writing/thirteen-years-of-rust-and-the-birth-of-rue/) — AI-assisted language creation
- [BAML: Building a New Programming Language in 2024](https://boundaryml.com/blog/building-a-new-programming-language) — transpiler-based approach
- [Drew DeVault: How to Design a New Programming Language](https://drewdevault.com/2020/12/25/How-to-design-a-new-programming-language.html) — practical advice from Hare creator

### Communities
- r/ProgrammingLanguages on Reddit
- Programming Languages Discord
