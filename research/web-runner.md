# Web Runner for ilo — Design Exploration

A browser-based playground where you paste ilo code, click run, and see output. No install, no server — the ilo interpreter runs client-side as WebAssembly.

## Why this fits ilo unusually well

Most language playgrounds need file trees, package management, and multi-file support. ilo doesn't. Programs are single-line expressions or small function blocks. The entire interaction model is: input box, output box, run button. That's it.

ilo's design constraints make this even better:
- **Closed vocabulary** — no imports, no packages, no filesystem. Nothing to stub out.
- **Verification before execution** — errors show before the program runs. The playground can display type errors, arity mismatches, and scope issues live as you type.
- **Dense syntax** — a meaningful program fits in a single line. No scrolling, no editor chrome needed.
- **Self-contained units** — each function declares everything it needs. No hidden global state.

Compare: the Rust Playground needs a server farm to compile code. The Go Playground runs on Google's servers. ilo can run entirely on the client because the interpreter is small and programs are tiny.

## Architecture: Rust interpreter compiled to WASM

```
User's browser
┌──────────────────────────────────────────────┐
│  HTML page                                   │
│  ┌──────────────────────┐  ┌──────────────┐  │
│  │  Input (textarea or  │  │  Output      │  │
│  │  CodeMirror)         │  │  (<pre>)     │  │
│  └──────────┬───────────┘  └──────▲───────┘  │
│             │ source string       │ result   │
│             ▼                     │          │
│  ┌──────────────────────────────────────┐    │
│  │  ilo.wasm                            │    │
│  │  lex → parse → verify → interpret    │    │
│  │  (Rust compiled to wasm32)           │    │
│  └──────────────────────────────────────┘    │
└──────────────────────────────────────────────┘
```

No server. No backend. The `.wasm` file is a static asset served from GitHub Pages or any CDN.

### What compiles to WASM

The pipeline that needs to ship: **lexer** (313 LOC) + **parser** (3,084 LOC) + **AST** (~400 LOC) + **verifier** (2,507 LOC) + **interpreter** (2,281 LOC). Total: ~8,500 lines of Rust.

Not included: JIT backends (Cranelift, ARM64, LLVM), CLI argument parsing, benchmarking, file I/O, the Python transpiler. These are excluded via feature flags or `#[cfg]` gates.

### What about the VM?

The register VM (~4,600 LOC) could also compile to WASM. It's faster than the interpreter (10.7x for numeric code) and has no platform-specific dependencies — no `mmap`, no `libc`, no signal handlers. The main cost is binary size.

**Recommendation:** start with the interpreter (simpler, smaller WASM binary), add the VM as an option later if performance matters. For the tiny programs a playground runs, the interpreter is fast enough.

### The HTTP builtin problem

`get`/`$` uses `minreq` for synchronous HTTP. That won't work in WASM — browsers don't allow synchronous network requests from the main thread.

Options:
1. **Disable it** — compile with `--no-default-features` (the `http` feature flag already exists). `get` returns `Err "http not available in playground"`. Clean, honest, zero complexity.
2. **Replace with browser `fetch`** — use `wasm-bindgen-futures` + `web-sys` to call `fetch()`. Requires making the interpreter async, which is a significant change.
3. **Web Worker + sync XMLHttpRequest** — run the WASM in a Worker where synchronous XHR is technically available. Hacky but avoids async changes to the interpreter.

**Recommendation:** option 1 for v1. The playground is for learning syntax and testing logic, not for making HTTP calls. Add a note: "HTTP builtins require the CLI."

## Build tooling

### wasm-pack + wasm-bindgen

The standard Rust→WASM pipeline. `wasm-pack build` compiles the crate, runs `wasm-bindgen` to generate JS glue code, and produces a `pkg/` directory ready to import.

```toml
# Cargo.toml — add a library target for WASM
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"
```

```rust
// src/lib.rs — WASM entry point
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn run(source: &str, args: &str) -> String {
    // args is comma-separated: "5,10,hello"
    let arg_list = parse_args(args);

    let tokens = match lexer::lex(source) {
        Ok(t) => t,
        Err(e) => return format_error(&e, source),
    };

    let token_spans = tokens.into_iter()
        .map(|(t, r)| (t, Span { start: r.start, end: r.end }))
        .collect();

    let (program, parse_errors) = parser::parse(token_spans);
    if !parse_errors.is_empty() {
        return format_errors(&parse_errors, source);
    }

    if let Err(verify_errors) = verify::verify(&program) {
        return format_errors(&verify_errors, source);
    }

    match interpreter::run(&program, None, arg_list) {
        Ok(val) => val.to_string(),
        Err(e) => format_runtime_error(&e, source),
    }
}

#[wasm_bindgen]
pub fn check(source: &str) -> String {
    // Returns verification errors as JSON (for live error display)
    // ... lex → parse → verify, return errors as JSON array
}

#[wasm_bindgen]
pub fn format_code(source: &str) -> String {
    // Returns formatted code (dense or expanded)
    // ... lex → parse → format
}
```

### Build command

```bash
wasm-pack build --target web --no-default-features
# Produces: pkg/ilo_bg.wasm, pkg/ilo.js, pkg/ilo.d.ts
```

`--no-default-features` excludes `cranelift` and `http`. The binary should be small — logos (lexer), serde/serde_json (AST serialization), and thiserror are the only remaining dependencies.

### Expected binary size

Rough estimate based on similar projects (Rust interpreters compiled to WASM):
- Interpreter + lexer + parser + verifier: **200–500KB** gzipped
- With `wasm-opt -Oz`: potentially under **300KB** gzipped
- Without serde (if we skip JSON AST output in WASM mode): **150–300KB** gzipped

For comparison: CodeMirror is ~150KB gzipped. The total page weight would be under 1MB.

## Frontend: minimal HTML

ilo's playground doesn't need a framework. The entire frontend fits in a single HTML file.

```
playground/
  index.html      — the page (textarea, output, run button, examples)
  pkg/
    ilo_bg.wasm    — compiled interpreter
    ilo.js         — wasm-bindgen glue
```

### Editor choice

| Option | Size | Syntax highlighting | Mobile | Complexity |
|--------|------|---------------------|--------|------------|
| `<textarea>` | 0KB | No | Yes | None |
| CodeMirror 6 | ~150KB gz | Custom grammar | Yes | Moderate |
| Monaco | ~2MB gz | Custom grammar | Poor | High |

**Recommendation:** start with a `<textarea>` and monospace font. ilo programs are short — syntax highlighting is nice-to-have, not essential. Add CodeMirror later if demand justifies it.

CodeMirror 6 is the right upgrade path: modular (import only what you need), good mobile support, and well-suited to small custom languages. Monaco is overkill — it's VS Code's editor and pulls in 2MB of JavaScript.

### Minimal HTML sketch

```html
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>ilo playground</title>
  <style>
    body { font-family: system-ui; max-width: 800px; margin: 2rem auto; }
    textarea, pre { font-family: 'SF Mono', 'Fira Code', monospace; font-size: 14px; width: 100%; }
    textarea { height: 80px; resize: vertical; }
    pre { background: #f5f5f5; padding: 1rem; min-height: 2rem; white-space: pre-wrap; }
    .error { color: #c00; }
    button { margin: 0.5rem 0; }
    .args { width: 200px; font-family: monospace; }
  </style>
</head>
<body>
  <h1>ilo playground</h1>
  <textarea id="code" spellcheck="false">f x:n>n;*x 2</textarea>
  <div>
    <label>args: <input class="args" id="args" value="21"></label>
    <button id="run">Run</button>
    <select id="examples">
      <option value="">Examples...</option>
      <option value="f x:n>n;*x 2|5">double</option>
      <option value="tot p:n q:n r:n>n;s=*p q;t=*s r;+s t|10,20,0.1">total+tax</option>
      <option value="f xs:L n>n;@x xs{>x 10{ret x}};0|3,7,15,2">first >= 10</option>
    </select>
  </div>
  <pre id="output"></pre>
  <script type="module">
    import init, { run, check } from './pkg/ilo.js';
    await init();

    const code = document.getElementById('code');
    const args = document.getElementById('args');
    const output = document.getElementById('output');

    document.getElementById('run').onclick = () => {
      output.className = '';
      const result = run(code.value, args.value);
      if (result.startsWith('error:')) {
        output.className = 'error';
      }
      output.textContent = result;
    };

    // Live error checking as you type
    let timer;
    code.oninput = () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        const errors = check(code.value);
        // Show inline error hints...
      }, 300);
    };

    // Example selector
    document.getElementById('examples').onchange = (e) => {
      if (!e.target.value) return;
      const [src, a] = e.target.value.split('|');
      code.value = src;
      args.value = a || '';
      e.target.selectedIndex = 0;
    };
  </script>
</body>
</html>
```

Total frontend: ~80 lines of HTML/CSS/JS. No build step, no bundler, no dependencies.

## Precedents: how other languages do it

### Fully client-side (WASM interpreter/compiler in browser)

| Language | Architecture | Notes |
|----------|-------------|-------|
| **Gleam** | Rust compiler → WASM, emits JS, runs JS in browser | Compiler compiled to WASM, virtual in-memory filesystem |
| **Loxcraft** | Rust Lox interpreter → WASM via `wasm-pack` | Closest analogue to what ilo would build. Hosted on GitHub Pages. |
| **Adventlang** | Go interpreter → WASM, runs in Web Worker pool | Documents the Worker timeout pattern for infinite loops |
| **Roc** | REPL on roc-lang.org runs via WASM | Early/alpha but functional |
| **Lua** | C interpreter compiled to WASM | ~150KB, near-instant startup |
| **SQLite** | C library compiled to WASM | Proves large C codebases work in WASM |
| **Ruby (ruby.wasm)** | CRuby compiled to WASM via Emscripten | ~30MB — much larger than ilo would be |

### Server-side execution (ilo should NOT do this)

| Language | Architecture | Why server-side |
|----------|-------------|-----------------|
| **Rust** | play.rust-lang.org, Docker containers on AWS | Compiler is enormous (>100MB), needs native code gen |
| **Go** | go.dev/play, Google servers | Compiler + runtime are large |
| **Python** | Various, often server-side | CPython has filesystem/OS deps |

ilo's interpreter is small enough (~8.5K LOC, no heavy dependencies) that client-side is the obvious choice. No server to maintain, no security sandbox to build, no scaling concerns.

### Loxcraft is the closest analogue

Loxcraft (a Lox interpreter in Rust) is compiled to WASM via `wasm-pack` and hosted on GitHub Pages. It demonstrates the exact pattern ilo would follow: Rust interpreter → WASM → browser. The difference is that ilo's interpreter is larger (2,281 vs ~1,000 LOC) but the architecture is identical.

### The Gleam model for richer features

Gleam compiles its Rust-based compiler to WASM and runs it entirely in the browser. Key lessons:
- They inject a virtual in-memory filesystem instead of real FS access
- They expose a JS API from the WASM module (not a CLI interface)
- The WASM module is loaded once, then called repeatedly
- No server infrastructure needed

ilo is simpler than Gleam because ilo doesn't need a virtual filesystem at all — programs are strings, not files.

### The Adventlang model for infinite loop handling

Adventlang (a Go interpreter compiled to WASM) runs in a Web Worker pool. When a user clicks "Run", an idle Worker is consumed. If execution exceeds a timeout, `worker.terminate()` kills it. Key insight from Adventlang's design: **a running WASM module cannot be paused or stopped mid-execution** — the only option is terminating the Worker. This is why the Web Worker approach is essential, not optional.

## Features beyond "run"

Once the core runner works, several features come naturally:

### Live verification (as you type)
The verifier runs on every keystroke (debounced 200–300ms). Type errors, undefined variables, and arity mismatches appear instantly — before clicking Run. This is the killer feature. No other playground gives you verified error codes in real-time for a language this small.

### Format toggle
A button that reformats between dense wire format (for LLMs) and expanded human-readable format. Uses the existing `codegen::fmt` module.

### Share via URL
Encode the program in the URL hash: `playground.ilo-lang.org/#f%20x:n>n;*x%202`. ilo programs are short enough that URL encoding works without a URL shortener. No server needed.

### Spec sidebar
Display the compact spec (`ilo -ai` output) in a collapsible sidebar. Agents and humans can reference it while writing code.

### AST inspector
Toggle to show the parsed AST as JSON (the existing `serde_json::to_string_pretty(&program)` output). Useful for language developers and curious users.

### Python transpile
Show the Python equivalent alongside the output. Uses the existing `codegen::python::emit` module. Good for teaching — "here's what this ilo code means in Python."

## Security and sandboxing

WASM in the browser is already sandboxed:
- **Memory isolation** — WASM linear memory is separate from the page's memory. The interpreter can't touch the DOM or read cookies.
- **No filesystem access** — `wasm32-unknown-unknown` has no WASI, no file system. The interpreter physically cannot read files.
- **No network access** — with the `http` feature disabled, there are no network calls. Even if enabled, CORS policies apply.
- **CPU limits** — a `setTimeout` watchdog in JS can kill the WASM execution if it runs too long (catches infinite loops). The interpreter's main loop can also check a fuel counter.

### Infinite loop protection

The interpreter doesn't currently have a fuel/step counter. Two options:

1. **JS-side timeout** — run the interpreter in a Web Worker with a `setTimeout` kill switch (e.g., 5 seconds). If it doesn't respond, terminate the Worker and show "execution timed out."

2. **Rust-side fuel counter** — add a step limit to the interpreter's `eval` loop. Every expression evaluation decrements a counter; at zero, return an error. This is more precise but requires modifying the interpreter.

**Recommendation:** option 1 for v1 (zero interpreter changes). Option 2 later if needed.

## WASM-specific concerns

### Dependencies that won't compile to WASM

| Dependency | Used for | WASM-compatible? | Action |
|-----------|----------|-------------------|--------|
| `logos` | Lexer | Yes (pure Rust, no I/O) | Keep |
| `serde` + `serde_json` | AST serialization | Yes | Keep |
| `thiserror` | Error types | Yes | Keep |
| `libc` | Listed in Cargo.toml | Not on `wasm32-unknown-unknown` | Gate behind `#[cfg(not(target_arch = "wasm32"))]` |
| `minreq` | HTTP GET | No (needs TCP) | Excluded by `--no-default-features` |
| `cranelift-*` | JIT compilation | No (needs native codegen) | Excluded by `--no-default-features` |
| `inkwell` (LLVM) | JIT compilation | No | Excluded by feature flag |

The only issue is `libc`. It's listed as a non-optional dependency in `Cargo.toml`. Need to check where it's used — it may only be needed for the ARM64 JIT (`jit_arm64.rs` uses `mmap`/`mprotect`). If so, make it optional or gate the import.

### println! / eprintln!

The interpreter uses `eprintln!` for tool call debug output (line 419). In WASM, `eprintln!` goes to the browser console via `wasm-bindgen`'s default panic hook. For the playground, we should capture output to a string buffer instead of printing to stderr.

The `run()` function in `interpreter/mod.rs` returns a `Value`, not printed text. This is already correct for the playground — we call `run()` and display `val.to_string()`. The `eprintln!` in tool call logging is debug-only and can be left as-is (goes to console) or gated.

### build.rs (compact spec generation)

`build.rs` reads `SPEC.md` at compile time. This works fine with `wasm-pack` — the build script runs on the host, not in WASM. The generated `spec_ai.txt` is embedded via `include_str!`. No issue.

## Implementation plan

### Phase 1: Minimal runner (1–2 days)

1. Add `[lib]` target to `Cargo.toml` with `crate-type = ["cdylib", "rlib"]`
2. Create `src/lib.rs` with `#[wasm_bindgen]` entry points: `run(source, args)`, `check(source)`, `format_code(source)`
3. Gate `libc` dependency behind `#[cfg(not(target_arch = "wasm32"))]`
4. Build with `wasm-pack build --target web --no-default-features`
5. Create `playground/index.html` with textarea, args input, run button, output pane
6. Test locally with a static file server

### Phase 2: Polish (1 day)

7. Add example programs dropdown
8. Add share-via-URL-hash
9. Add format toggle (dense ↔ expanded)
10. Add live verification on keystroke
11. Web Worker for infinite loop protection
12. Deploy to GitHub Pages

### Phase 3: Rich features (optional, later)

13. CodeMirror 6 with custom ilo grammar (syntax highlighting, bracket matching)
14. AST inspector panel
15. Python transpile view
16. Spec reference sidebar
17. VM backend as a toggle (faster execution)

## Open questions

### Should the playground support multi-function programs?

Currently the CLI auto-detects the first function or lets you name one. The playground could:
- **Always run the first function** — simplest, matches how most examples work
- **Show a function picker** — dropdown populated from the AST after parsing
- **Run all top-level expressions** — REPL-like behavior

**Leaning toward:** function picker, populated automatically. It's a `<select>` element populated from `program.declarations`, zero-effort UX.

### Should `println`-style output be supported?

ilo currently has no `print` builtin — functions return values. The playground shows the return value. If `print` is added later, the playground would need stdout capture (a string buffer passed to the interpreter). This is straightforward but not needed today.

### Hosting

Options:
- **GitHub Pages** — free, automatic from a branch or `/docs` folder, custom domain support
- **Cloudflare Pages** — free, global CDN, faster for international users
- **Netlify** — free tier, similar to Cloudflare Pages

All work since the playground is entirely static files. No server, no database, no API. GitHub Pages is simplest since the code is already on GitHub.

## Relationship to other work

- **TODO.md line 595** lists "Playground — web-based editor with live evaluation (WASM target)" as a future item under Tooling.
- The `http` feature flag (`Cargo.toml:8`) already separates networking from the core — the playground build excludes it cleanly.
- The `codegen::fmt` module provides both dense and expanded formatting — the playground can offer both views.
- The `codegen::python` transpiler can show Python equivalents of ilo code.
- The diagnostic system (ANSI/JSON/text modes) can emit JSON errors for the playground to render.

## Summary

| Aspect | Decision |
|--------|----------|
| Execution | Client-side WASM (interpreter backend) |
| Build tool | `wasm-pack` + `wasm-bindgen` |
| Editor | `<textarea>` for v1, CodeMirror 6 later |
| HTTP builtin | Disabled (returns error string) |
| Infinite loop protection | Web Worker timeout for v1 |
| Frontend framework | None — single HTML file |
| Hosting | GitHub Pages |
| Binary size (est.) | 200–500KB gzipped |
| Effort | 2–3 days for a working playground |
