# Contributing to ilo

Thanks for your interest in contributing to ilo!

For the release secret-scan gate (gitleaks, allowlist, incident procedure) see [docs/release-secret-scan.md](docs/release-secret-scan.md).

## Getting Started

```bash
git clone https://github.com/ilo-lang/ilo
cd ilo
cargo test
```

## Shared build cache (recommended)

ilo development typically involves many worktrees (one per fix / feature
branch). By default each worktree gets its own `target/` directory and these
add up fast, historically into the tens of GB on a single developer machine.
Set a shared cache once in your shell config:

```bash
# ~/.zshrc or ~/.bashrc
export CARGO_TARGET_DIR="$HOME/.cargo-shared"
mkdir -p "$CARGO_TARGET_DIR"
```

All worktrees then share one build cache. Cargo namespaces artefacts per
package + feature combination internally, so this is safe across branches.

## Empirical discipline

Claims about ilo's performance, token count, or runtime behaviour need
benchmarks or tests, not assertions. "Measured, not asserted" is the rule.
If you're tightening a hot path, add a criterion bench or a deterministic
timing harness. If you're claiming a token win in docs, the number comes
from the benchmark, not from rounding what feels right.

The binary-size tripwire (`tests/binary_size.rs`) is one expression of this:
AOT output for a trivial program is checked against a ceiling on every CI
run, so dep upgrades that inflate the runtime fail loudly instead of
drifting silently.

## Strategic context

Longer-form plans for upcoming phases live in `zero-gap-specs/` (separate
repo). Briefs there set the why; PRs here implement the how.

## Development

- **Run tests:** `cargo test`
- **Run clippy:** `cargo clippy -- -W clippy::all`
- **Run a program:** `cargo run -- 'f x:n>n;*x 2' 5`

## Pull Requests

1. Fork the repo and create a feature branch from `next`
2. Make your changes
3. Ensure `cargo test` and `cargo clippy` pass
4. Submit a PR with a clear description of the change

Patches against the current release line (a fix for `26.5` that needs to ship as `26.5.1`) branch from `main` instead. See [README.md#versioning](README.md#versioning) for the full branch / tag model.

## Architecture

The pipeline flows: **Lexer** -> **Parser** -> **AST** -> **Verifier** -> **Interpreter/VM/Cranelift JIT**

Key source files:
- `src/lexer/mod.rs` - tokenizer
- `src/parser/mod.rs` - parser producing AST
- `src/ast/mod.rs` - AST types
- `src/verify.rs` - static type checker
- `src/interpreter/mod.rs` - tree-walking interpreter
- `src/vm/mod.rs` - register-based VM
- `src/codegen/python.rs` - Python transpiler
- `src/diagnostic/` - error codes and reporting

## Community

- [ilo-lang.ai](https://ilo-lang.ai) - docs, playground, and examples
- [hello@ilo-lang.ai](mailto:hello@ilo-lang.ai) - get in touch

## Language Spec

See [SPEC.md](SPEC.md) for the full language specification. Changes to language syntax or semantics should update the spec.

## Golden-file diagnostics

ilo pins the JSON shape of its diagnostic output with golden snapshot files
stored under `conformance/diagnostics/`. When you change a diagnostic message,
add a `phase` field, or add a new error code, CI will fail on a diff unless
you also update the matching snapshot.

### Running the golden tests

```
cargo test --features golden
```

### Updating (blessing) snapshots

If you intentionally changed a diagnostic's JSON shape, re-bless the affected
golden files:

```
cargo test --features golden -- --bless
```

This rewrites every `conformance/diagnostics/<CODE>.expected.json` whose
content has changed. Review the diff with `git diff conformance/` before
committing — the snapshot is part of the stability contract.

You can also bless a single code by running the specific test:

```
ILO_GOLDEN_BLESS=1 cargo test --features golden golden_ilo_t004
```

### Adding a new error code

1. Add the entry to `src/diagnostic/registry.rs` with a `phase` field set to
   the correct `Phase::*` variant.
2. Run `cargo test --features golden -- --bless` to generate the initial
   golden file.
3. Commit both the registry change and the new `.expected.json` file.

### Provenance surface

`conformance/provenance-surface.json` maps every language feature to the
compiler function that owns it and the fixture that exercises it. When you add
a new feature or move code to a different function, update this file to keep
the map accurate. The `provenance_surface_is_valid` golden test will catch
structural problems.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
