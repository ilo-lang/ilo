# ilo-lang — Agent context

**ilo** is a token-optimised programming language for AI agents.

- GitHub: https://github.com/ilo-lang/ilo
- Language spec: [SPEC.md](SPEC.md)
- Current version: **0.11.6** (installed at `~/.cargo/bin/ilo` via `cargo install`)

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

## Adding builtins

Every new builtin reserves its name from the user namespace at parse time (`ILO-P011`). To keep that reservation forecastable across releases, follow these rules when adding a builtin:

1. **Land the long name first.** New builtins ship under a name of **4 characters or longer** (`countif`, `flatmap`, `cumsum`, `partition`, `dtparse`, …). The 2-character namespace is closed to new entries — agents rely on "any 2-char name not in the published reserve list is mine to use, forever." Adding a new 2-char builtin breaks that promise and breaks every carry-forward script that happened to bind the name.
2. **3-char names are discouraged but not banned.** The 3-char surface is already dense and most short forms are taken. If a 3-char name reads obviously natural for the operation (and no plausible domain binding would reach for it), it's acceptable, but the long form must land first and the 3-char form is added as an alias afterwards. Call out the new reservation in the changelog.
3. **Short aliases come later.** Once the long-name builtin has shipped, a short alias may be added through the `builtin_aliases` mechanism (see `src/builtins.rs` and the `### Builtin aliases` section of SPEC.md) — but only if:
   - The long form is unambiguous (no other builtin or near-name conflicts).
   - The short form does not shadow a plausible user binding (e.g. `ct` for "count" was unsafe because analytics agents reach for it as "category text"; `nwhere` or `countif` would have been the right long form).
   - The new reservation is added to the `### Reserved namespaces` enumeration in `SPEC.md` in the same commit.
4. **Drift guard.** `tests/regression_reserved_names_doc.rs` asserts the SPEC enumeration matches `Builtin::ALL` at test time. Adding a builtin without updating the doc fails the suite — that's the contract that keeps the published forecast accurate.
5. **Existing short builtins are grandfathered.** Anything already on the reserve list (`avg`, `ct`, `sin`, `flat`, `frq`, …) stays. The 3+ char-first rule applies to new entries only.

The manifesto framing: agents budget around the reserve list once, not once per release. The cost of a release that breaks carry-forward scripts on a name collision is paid by every running agent; the cost of a 4-char long-name plus a deferred short alias is paid once by the language.

## Release process

1. Bump version everywhere: `scripts/bump-version.sh X.Y.Z` (rewrites `Cargo.toml`, `AGENTS.md`, `.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`; `npm/` and `pi/` are bumped from the tag by CI)
2. Commit + tag `vX.Y.Z`
3. `git push origin main --tags` → triggers `.github/workflows/release.yml` → builds 5 native targets + WASM, publishes GitHub Release + npm package

## WASM / npm

`cargo build --target wasm32-wasip1 --release --no-default-features` produces `ilo.wasm` (2.1MB). The `npm/` directory wraps it in a Node.js WASI shim published as `ilo-lang` on npm. Requires `NPM_TOKEN` secret in GitHub repo settings.
