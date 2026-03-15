# ilo-lang — Claude context

**ilo** is a token-optimised programming language for AI agents.

- GitHub: https://github.com/ilo-lang/ilo
- Language spec: [SPEC.md](SPEC.md)
- Current version: **0.10.0** (installed at `~/.cargo/bin/ilo` via `cargo install`)

## What ilo is

Prefix-notation, strongly-typed, AI-agent-first language. Programs are small, verified before execution, and designed to minimise total token cost across generation + retries + context loading.

## Key files

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI entry point, import resolution, .env loading |
| `src/lexer/mod.rs` | Tokeniser (logos) |
| `src/parser/mod.rs` | Recursive-descent parser |
| `src/ast/mod.rs` | AST types (`Decl`, `Stmt`, `Expr`, `Type`) |
| `src/verify.rs` | Type verifier |
| `src/interpreter/mod.rs` | Tree-walking interpreter |
| `src/vm/mod.rs` | Register VM + bytecode compiler |
| `src/vm/compile_cranelift.rs` | AOT compiler (Cranelift ObjectModule → native binary) |
| `src/codegen/` | Python, explain, fmt, dense-wire emitters |
| `src/tools/` | MCP client, HTTP tool provider |
| `examples/` | Runnable example programs (also `cargo test` regression suite) |
| `skills/ilo/` | Agent Skill (Claude Code plugin + cross-platform installer) |
| `npm/` | npm package (WASM build + Node.js WASI shim for `npx ilo-lang`) |
| `SPEC.md` | Full language specification |
| `research/TODO.md` | Planned work |

## Running

```bash
ilo 'dbl x:n>n;*x 2' 5          # inline code
ilo program.ilo funcname args   # from file
cargo test                       # full test suite
```

## Release process

1. Bump `version` in `Cargo.toml`
2. Commit + tag `vX.Y.Z`
3. `git push origin main --tags` → triggers `.github/workflows/release.yml` → builds 5 native targets + WASM, publishes GitHub Release + npm package

## WASM / npm

`cargo build --target wasm32-wasip1 --release --no-default-features` produces `ilo.wasm` (2.1MB). The `npm/` directory wraps it in a Node.js WASI shim published as `ilo-lang` on npm. Requires `NPM_TOKEN` secret in GitHub repo settings.

## Agent teams

When parallelisable work arises (coverage gaps, multi-file refactors, independent bug fixes), use **agent teams**:

1. Create a team with `TeamCreate` (name + description)
2. Create tasks with `TaskCreate` (one per agent)
3. Spawn teammates with `Agent` using `team_name`, `name`, `isolation: "worktree"`, `model: "sonnet"`, `run_in_background: true`
4. Assign tasks via `TaskUpdate` with `owner`
5. When agents complete, review worktree branches and cherry-pick/merge into main
6. Shut down teammates with `SendMessage` type `shutdown_request`, then `TeamDelete`

Rules:
- Use `model: "sonnet"` for implementation, `model: "haiku"` for comprehension/validation
- Always `isolation: "worktree"` — no merge conflicts
- One focused task per agent (e.g. "cover vm/mod.rs" not "improve all coverage")

## Benchmarks

The canonical benchmark suite is `research/bench/run.sh`. It compares ilo's 4 execution engines (interpreter, VM, JIT, AOT) against 12+ languages across 11 micro-benchmark categories. Results auto-generate the website benchmarks page.
