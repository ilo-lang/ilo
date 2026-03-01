# Contributing to ilo

Thanks for your interest in contributing to ilo!

## Getting Started

```bash
git clone https://github.com/danieljohnmorris/ilo-lang
cd ilo-lang
cargo test
```

## Development

- **Run tests:** `cargo test`
- **Run clippy:** `cargo clippy -- -W clippy::all`
- **Run a program:** `cargo run -- 'f x:n>n;*x 2' 5`

## Pull Requests

1. Fork the repo and create a feature branch from `main`
2. Make your changes
3. Ensure `cargo test` and `cargo clippy` pass
4. Submit a PR with a clear description of the change

## Architecture

The pipeline flows: **Lexer** -> **Parser** -> **AST** -> **Verifier** -> **Interpreter/VM/Cranelift JIT**

Key source files:
- `src/lexer/mod.rs` — tokenizer
- `src/parser/mod.rs` — parser producing AST
- `src/ast/mod.rs` — AST types
- `src/verify.rs` — static type checker
- `src/interpreter/mod.rs` — tree-walking interpreter
- `src/vm/mod.rs` — register-based VM
- `src/codegen/python.rs` — Python transpiler
- `src/diagnostic/` — error codes and reporting

## Community

Join [r/ilolang](https://www.reddit.com/r/ilolang/) for discussion, feedback, and updates.

## Language Spec

See [SPEC.md](SPEC.md) for the full language specification. Changes to language syntax or semantics should update the spec.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
