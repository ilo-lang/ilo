# ilo Parser Recon: `src/parser/mod.rs`

## Summary

**File**: `src/parser/mod.rs`
**Lines**: 13,110
**Branch**: `main` (commit `da2733fc`)

The parser is a hand-written recursive-descent parser (no parser combinator library). It lives entirely in one module with the `Parser` struct holding all state including token stream, position, depth tracking, function arity metadata, and newline-boundary markers for error recovery.

---

## 1. How the pipe operator `>>` is parsed

**Function**: `Parser::maybe_pipe` at **line 3600**

The pipe operator is parsed as a **postfix desugaring pass**, not as an infix operator. It runs at the tail end of `parse_expr_body` (line 3540), after `maybe_with` (record update) and `maybe_nil_coalesce` (`??`):

```
parse_expr_body → parse_expr_inner → maybe_with → maybe_nil_coalesce → maybe_pipe
```

### `maybe_pipe` mechanics (lines 3600-3643)

```rust
fn maybe_pipe(&mut self, mut expr: Expr) -> Result<Expr> {
    while matches!(self.peek(), Some(Token::PipeOp)) {
        self.advance(); // consume >>
        let func_name = self.expect_ident()?;
        let unwrap = self.maybe_postfix_unwrap();     // supports `>> func!` 
        // Parse additional explicit args...
        let mut args = Vec::new();
        let pipe_outer_arity = self.fn_arity.get(&func_name).copied();
        while self.can_start_operand() {
            let arg_idx = args.len();
            let in_fn_pos = self.is_fn_ref_position(&func_name, arg_idx);
            let outer_ctx = pipe_outer_arity
                .filter(|&k| k > 0)
                .map(|k| (func_name.as_str(), k - 1, arg_idx));
            args.push(self.parse_call_arg(in_fn_pos, outer_ctx)?);
        }
        // Piped value becomes last arg
        args.push(expr);
        expr = Expr::Call { function: func_name, args, unwrap };
    }
    Ok(expr)
}
```

**Key design points:**
- `Token::PipeOp` is the `>>` token (lexer line 86)
- The piped value becomes the **last argument** of the target function call
- `xs >> map sq` desugars to `map(sq, xs)` — not `map(xs)` with `sq` as closure
- `xs >> func!` supports `!` unwrap propagation on piped calls (test at line 11293)
- Pipe chains work: `expr >> f >> g` desugars to `g(expr)` then `f(g(expr))` via the `while` loop
- The pipe uses `fn_arity` to determine how many explicit args to expect before appending the piped value

### AST representation

Pipes are **fully desugared** at parse time. There is no `Expr::Pipe` variant. The pipe `xs >> map sq` becomes `Expr::Call { function: "map", args: [Expr::Ref("sq"), Expr::Ref("xs")], unwrap: UnwrapMode::None }`. This means downstream stages (verifier, VM, JIT, AOT) never see pipes — they only see calls.

### Implication for pipeline error short-circuit feature

Since pipes desugar to plain `Expr::Call`, adding Result-aware short-circuit to `>>` would require either:
- A new `Expr::Pipe` variant that the VM/interpreter checks at runtime, OR
- A desugaring that wraps each stage in a match/unwrap check (complicating the AST)
- The `UnwrapMode` field on `Expr::Call` already supports `Propagate` — if pipe stages used `unwrap: Propagate` instead of `None`, `>>` would get `!`-like error propagation for free

---

## 2. Top-level construct dispatch

**Entry**: `parse_program` (line 490) → `parse_decl` (line 676) → `parse_decl_body` (line 684)

### `parse_decl_body` dispatch (line 684)

The parser first runs a series of **guard checks** for common mis-binds (reserved keywords, builtins, aliases, top-level `name=expr`), then dispatches on `match self.peek()`:

| Token | Dispatches to | What it parses |
|-------|--------------|----------------|
| `Token::Type` | `parse_type_decl` (line 1192) | `type name{field:type;...}` |
| `Token::Tool` | `parse_tool_decl` (line 1363) | `tool name"desc"(params)>ret timeout:n,retry:n` |
| `Token::Use` | `parse_use_decl` (line 915) | `use "path"` imports |
| `Token::Underscore` + `Ident` | `parse_fn_decl` (line 1431) | `_name` private functions |
| `Token::Ident` | (guards first, then) `parse_fn_decl` (line 1431) | `name params>ret;body` |
| `Token::Caret` (pos 0) + `Number` | `parse_version_pragma` (line 3421) | `^26.5` version directive |
| Anything else | Error `ILO-P001` | `expected declaration` |

### Guard checks before dispatch (lines 686-794)

Before the `match`, the parser checks for:
1. **Reserved keyword binding** (`var=5`, `let=5`, `if=5`) → `ILO-P011`
2. **Loop-control as binding** (`cnt=5`, `brk=5`) → `ILO-P011`
3. **Builtin `fld` as binding** (`fld=5`) → `ILO-P011`
4. **Builtin alias as binding** (`head=5`, `length=5`) → `ILO-P011`
5. **Any builtin as binding** (`map=5`, `flat=5`) → `ILO-P011`
6. **Top-level `name=expr` outside function** → `ILO-P102`

These produce actionable hints pointing to the correct ilo syntax before falling through to the main dispatch.

---

## 3. File length

**13,110 lines** in `src/parser/mod.rs`.

This includes:
- Parser struct definition + construction: ~200 lines (lines 1-490)
- `parse_program` + error recovery: ~80 lines (lines 490-570)
- Declaration parsers (`fn`, `type`, `tool`, `use`, `alias`): ~800 lines (lines 676-1600)
- Type parsing: ~400 lines (lines 1859-2200)
- Statement parsing: ~1200 lines (lines 2194-3400)
- Expression parsing (including pipe, infix, prefix, call, atom): ~3000 lines (lines 3532-5500)
- Utility methods (peek, advance, sync, expect): ~1500 lines (lines 5500-7160)
- Public API functions: ~30 lines (lines 7162-7200)
- **Test module**: ~5900 lines (lines 7200-13110) — nearly half the file

---

## 4. Main `pub fn` entry points

| Function | Line | Purpose |
|----------|------|---------|
| `set_max_ast_depth_override(cap: usize)` | 31 | Sets global AST depth limit override |
| `Parser::new(tokens: Vec<(Token, Span)>)` | 146 | Construct parser from token+span pairs |
| `Parser::new_with_max_depth(tokens, max_depth)` | 152 | Construct with custom depth cap |
| `Parser::parse_program(&mut self)` | 490 | Main entry: parse all declarations, returns `(Program, Vec<ParseError>)` |
| `parse(tokens: Vec<(Token, Span)>)` | 7162 | Module-level shortcut: constructs Parser, calls `parse_program` |
| `parse_with_max_depth(tokens, max_depth)` | 7170 | Same as `parse` but with custom depth cap (used by CLI) |
| `parse_tokens(tokens: Vec<Token>)` | 7182 | Test-only: parse from bare tokens (no spans), returns `Result<Program, Vec<ParseError>>` |

---

## Key AST types used by the parser

From `src/ast/mod.rs`:

- **`Decl`** enum: `Function`, `TypeDef`, `Tool`, `Alias`, `Use`, `Error`, `VersionPragma`
- **`Expr`** enum (line 442): `Literal`, `Ref`, `Field`, `Call`, `BinOp`, `Match`, `Ok`, `Err`, `NilCoalesce`, `With`, and more. **No `Pipe` variant** — pipes desugar to `Call`.
- **`Stmt`** enum: `Let`, `Destructure`, `Expr`, `Guard`, `Return`, `Match`, `Foreach`, `While`, `Bang`, `Caret`
- **`UnwrapMode`** enum (line 412): `None` (default), `Propagate` (`!`), `Panic` (`!!`)
- **`Param`**: name + type pair
- **`Type`** enum (line 104): `Number`, `Text`, `Bool`, `Any`, `Optional`, `List`, `Map`, `Result`, `Sum`, `Fn`, `Named`, plus width-specific types (`U32`, `U64`, `I64`)

---

## Parser state fields (struct Parser, line 61)

| Field | Purpose |
|-------|---------|
| `tokens: Vec<(Token, Span)>` | Token stream |
| `pos: usize` | Current position |
| `depth: usize` | Recursion depth (capped by `max_depth`) |
| `max_depth: usize` | AST nesting limit (CLI `--max-ast-depth`) |
| `decl_boundary: Vec<Option<Span>>` | Newline positions for error recovery |
| `fn_arity: HashMap<String, usize>` | Known function arities (builtins + user fns) |
| `fn_param_is_fn: HashMap<String, Vec<bool>>` | Which params are HOF positions |
| `fn_param_names: HashMap<String, Vec<String>>` | Param names for labelled-arg resolution |
| `ctx: ParseContext` | Transient parsing-mode flags |
| `lifted_decls: Vec<Decl>` | Synthetic top-level decls from lambda lifting |
| `lambda_counter: usize` | Monotonic synthetic name generator |
| `lambda_depth: usize` | Enclosing lambda body depth |
| `parse_failed_fns: HashMap<String, ParseFailRef>` | Fns whose body parse errored (for cascade suppression) |
| `glued_eq_binding_sites: HashSet<String>` | Diagnostic-only: `name==expr` misparse tracking |
| `h_keyword_simple_ref_sites: Vec<(Span, String)>` | Diagnostic-only: `?h <ref> <a> <b>` advisory |

---

## Notes for roadmap features

### Pipeline error short-circuit
- `maybe_pipe` (line 3600) is where to add Result-awareness
- Currently `unwrap` is set to `None` unless `func!` is written
- Simplest approach: change pipe desugaring to emit `unwrap: Propagate` by default when the piped expression type is `R`, or add a new `Expr::PipeChain` variant that the verifier and VM handle
- The `fn_arity` map is already consulted to determine arg structure

### Constrained decoding
- `parse_program` (line 490) + `parse_decl_body` (line 684) are the entry points to enumerate parse states
- The `match self.peek()` dispatch in `parse_decl_body` is the top-level state machine
- `Token` enum in `src/lexer/mod.rs` defines the full token vocabulary

### Shadow test blocks
- New `Token::Test` would need adding to the lexer
- `parse_decl_body` dispatch would need a `Some(Token::Test) => self.parse_test_block()` arm
- `Decl` enum would need a new `Test` variant or test blocks attach to the preceding `Decl::Function`

### Effect sigils
- `parse_fn_decl` (line 1431) parses function headers — effect sigils would go after the return type
- `Decl::Function` already has `effect_set: Option<Vec<String>>` (line 195) — this is for declared error variants (`^Variant|Variant`), which could be extended or a new `effects: Vec<String>` field added
- `parse_tool_decl` (line 1363) would need a similar extension for tool effect annotations

### Tool policies
- `parse_tool_decl` (line 1363) already parses `timeout:n,retry:n` in a `while` loop (lines 1383-1400)
- Adding `policy{...}` would extend this loop or add a new clause
- `Decl::Tool` (line 218 in ast) has `timeout: Option<f64>` and `retry: Option<f64>` — a `policy: Option<ToolPolicy>` field would extend this

### Optional contracts
- `parse_fn_decl` (line 1431) parses function headers — `req`/`ens` clauses would go after the return type
- `Decl::Function` would need `requires: Vec<Expr>` and `ensures: Vec<Expr>` fields
- The verifier (`src/verify.rs`, 13K lines) would need a new SMT pass