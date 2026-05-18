# Language: Zero

Zero (Vercel) is a typed, Rust-like language for AI-generated code. Relevant skill modules loaded above.

Critical reminders:
- Functions: `pub fun name(arg: type) -> ret { ... }`
- Numeric type: `f64`
- Strings: `text`
- Results: `Result[T, text]` with `Ok(v)` and `Err(msg)`
- Mutable bindings: `let mut x = ...`
- Top-level functions should be `pub fun`.
- The file is compiled via `zero check` — must parse and type-check.
