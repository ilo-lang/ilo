# Changelog

## 0.13.0

### Breaking

- **Tree-walker engine removed.** ilo now ships two execution engines (bytecode register VM, default; Cranelift JIT/AOT, opt-in via `--jit`), down from three. Six-PR ILO-45 series lifted every remaining HOF off the `OP_CALL_BUILTIN_TREE` bridge to native VM dispatch (map/flt/fld closure-bind ctx via OP_CALL_DYN argc=2/3 in PR A #619; srt/rsrt closure-bind ctx via new `OP_RSRT_BY_KEY=191` finalizer + ctx-threading `emit_hof_keyed_finalize_ctx` in PR B #699; ct/ct-ctx via inline counter loop in PR C #707), then deleted the now-unreachable HOF impls in PR D #712, then deleted the eval loop itself in PR E #721 (`eval_body`, `eval_stmt`, `eval_expr`, the trampoline, `BodyResult`, self-rebind peepholes, pattern + binop helpers, the public `runtime::run*` entry points, the user-fn dispatch tail of `call_function`). `call_function` is now a builtin-only dispatcher; `Env` is two fields (`functions`, `caps`). `ilo trace` and its `run_with_trace` library API are gone (fired from `eval_body`; VM/JIT trace path tracked at ILO-343). `ilo --bench` drops the "Rust interpreter" row; VM is the interpreter baseline. `ilo serv` and `ilo repl` migrated to `vm::compile` + `vm::run`. Per ILO-234 the ~30 non-HOF bridge entries (regex, fmt, fs metadata, crypto, calendar, sleep, run, env-all, math constants, jkeys, etc.) stay routed through the bridge code in `src/runtime/`. Side-effect fix: VM/Cranelift bridge previously bypassed CLI capability flags because `Env::new()` defaulted to permissive caps; fixed via new `ACTIVE_CAPS` thread-local installed by `call_builtin_for_bridge_with_caps` / `call_builtin_for_bridge_with_program_and_caps`. ~7900 lines deleted net. Closes ILO-45.

## Unreleased

The codegen layer. A typed HIR sits between the verified AST and code
emission, and four backends now live behind a single `Backend` trait:
Cranelift (native, default), Python source, WASM Component Model, and Zero
source / binary. The CLI is locked to exactly five forms.

```
ilo build file.ilo            # native binary (Cranelift; default)
ilo build file.ilo --wasm     # WebAssembly Component Model binary
ilo build file.ilo --0        # Zero source (.0)
ilo build file.ilo --0bin     # native binary via the Zero compiler
ilo build file.ilo --py       # Python source (.py)
```

### Added (from main)

- `ILO-P102` diagnostic for top-level `name=expr` bindings outside any function declaration. Catches the "forgot the `main>_;` wrapper" misparse that k-means and linear-regression personas hit when chaining imperative bindings at the top level. Without the wrapper the parser used to either die on the bare `=` (a bare `ILO-P003`) or, when a prior `name>type;body` decl was in scope, slurp the whole chain into that fn's body and emit a wall of misleading `ILO-T005` cascades anchored on the wrong line. `ILO-P102` collapses both shapes into a single diagnostic that names the offending binding and suggests the `main>_;` wrapper. Parser-only change; identical output across VM and JIT.

### Fixed (from main)

- New `ILO-W002` warning when the foreach collection is a direct `jpar!` or `jpar!!` call. Surfaces the hint pointing at `jpar-list!`, which asserts the top-level JSON is an array and returns `R (L _) t` so the unwrap composes cleanly into `@`. Catches the `mempool-fee-estimator` failure mode where the polymorphic `jpar` Ok type forced the wrapping function's return type to `R t t` and threaded `?` through downstream code. The `@x (jpar-list! body){...}` form continues to type-check silently; `xs = jpar! body; @x xs{...}` (the explicit-bind form) is unchanged. Diagnostic-only; no behaviour change in the runtime engines. Closes pending.md item #5f.
- Cascading `ILO-T005 undefined function 'X'` errors from a single parse failure now collapse to one diagnostic per parse-failed function with a cross-reference back to the originating parse error. Previously, ONE broken function body produced N undefined-function errors (one per call site), burying the root cause; the cron-explainer persona logged 286 ILO-T005, 107 ILO-P009, and 47 ILO-P001 from roughly 10 root causes in a single run. The parser now records function names whose return-type or body failed to parse on `Program.parse_failed_fns`, and the verifier (1) skips type-checking those functions' bodies (their AST is poison) and (2) emits one collapsed `ILO-T005` per parse-failed name with a hint pointing at the root parse error code. Real undefined-function errors (typos, missing imports) still surface normally with the usual suggestion text.

### Changed (from main)

- `find_libilo_a` (AOT linker helper in `src/vm/compile_cranelift.rs`) now honours `CARGO_TARGET_DIR` and `.cargo/config.toml`'s `build.target-dir` before falling back to `$CARGO_MANIFEST_DIR/target`. Fix worktrees that redirect cargo's target dir out of the tree (e.g. `[build] target-dir = "/tmp/ilo-targets/..."`) no longer need a `ln -sf .../release/libilo.a target/release/libilo.a` workaround for the AOT tests to find the staticlib. Test-infrastructure only; no user-visible change to `ilo compile`.
- Versioning scheme: semver → CalVer. Releases are `YY.M` (e.g. `26.5`), patches `YY.M.P` (e.g. `26.5.1`). The version string carries recency so an agent loading `ilo spec --json ai` knows which spec applies without a changelog lookup. Last semver release is `0.12.1`; first CalVer release cuts on the next breaking change as `26.X`. Hard cut, no `0.13` bridge. Branching model splits: `main` carries stable + RC tags (`26.5`, `26.5.1`, `26.5.2-rc.1`), `next` carries dev tags only (`26.6-dev.N`). See `README.md#versioning` for the full release / patch flow.

### Cross-backend conformance (`tests/conformance.rs`)

Runs every `examples/*.ilo` with `-- run:` + `-- out:` headers through every
available backend and reports honest per-backend numbers. 218 conformance
cases at the CalVer cut (26.X).

| backend | pass | unsupported | fail |
| --- | ---: | ---: | ---: |
| cranelift | 87 | 0 | 131 |
| python | 0 | 0 | 218 |
| wasm | 0 | 213 | 5 |
| zero | 0 | 209 | 9 |

Reading the numbers honestly:

- **Cranelift native**: the production backend. The 131 fails are a mix of
  pre-existing AOT bugs surfaced by the dispatch log baselines (duplicate
  `ilo_strconst_*`, unsupported opcode 176, `nil` from `zip`) and entry-point
  mismatches between `ilo run` (which picks `main` or the named function
  cleanly) and `ilo build` (which currently uses the auto-main-pick path).
  None of these are 26.X regressions; all carry over from 0.12.x and are
  follow-up work.
- **Python**: emits library code with no `if __name__ == "__main__"`
  dispatcher, so the subprocess runner can't pick the entry function. The
  emit itself is byte-identical to the pre-refactor Python output (covered
  by `tests/python_emit_byte_identical.rs`). Wrapping the emit with a CLI
  dispatcher is follow-up work.
- **WASM**: the narrow Stage 5d walker only lowers the hello-world subset.
  Anything richer surfaces `ILO-B201` and is counted as `unsupported`. The
  walker grows in subsequent releases.
- **Zero**: same shape as WASM. The narrow Stage 5e walker covers
  hello-world; everything else surfaces `ILO-B302`. Walker widens release
  by release.

The brief frames this stage as "honest reporting, not artificial
completeness". The numbers above are exactly that. Each follow-up is filed
against Phase 6.

### Added (since 0.12.x)

- **Typed HIR module (`src/hir/`).** Phase 5 Stage 5a. A thin high-level
  intermediate representation that sits between the verified AST and concrete
  code emission. Every Phase 5 backend (Cranelift refactor, Python refactor,
  WASM Component Model, Zero transpile) will consume `hir::Program`. Includes:
  - `hir::lower(ast, verify_out)` — AST → HIR lowering pass.
  - Documented departures from the AST: function body tail-expression split,
    guard polarity folded into `UnaryOp(Not)`, `Ternary` → value-level `If`,
    `Alias`/`Use`/`Error` decls dropped.
  - `src/hir/DESIGN.md` documents the shape, the departures, the deferrals,
    and the open questions Stage 5b picked up.
### Added

<<<<<<< HEAD
- `.@` is the new canonical source file extension. `.@` tokenises as two tokens (`foo`, `.@`) on cl100k and o200k vs three for `foo.ilo` - one token saved per filename mention. All `examples/` and `tests/` source files in this repo have been renamed to `.@`. `.ilo` continues to be accepted but emits a deprecation hint on stderr at load time: `hint: .ilo extension is deprecated; rename to .@`. Rename your files with: `find . -name '*.ilo' -exec sh -c 'mv "$1" "${1%.ilo}.@"' _ {} \;`
- **Typed HIR module (`src/hir/`).** Phase 5 Stage 5a. A thin high-level
  intermediate representation that sits between the verified AST and concrete
  code emission. Every Phase 5 backend (Cranelift refactor, Python refactor,
  WASM Component Model, Zero transpile) will consume `hir::Program`. Includes:
  - `hir::lower(ast, verify_out)` — AST → HIR lowering pass.
  - Documented departures from the AST: function body tail-expression split,
    guard polarity folded into `UnaryOp(Not)`, `Ternary` → value-level `If`,
    `Alias`/`Use`/`Error` decls dropped.
  - Note: the throwaway `hir::walker` + `hir::raise` scaffolding and the
    `tests/hir_roundtrip.rs` corpus check existed during Stage 5a-5e
    development to prove the lowering pass was information-preserving.
    Stage 5f deletes them; the cross-backend conformance suite supersedes.
- **`Backend` trait and Cranelift refactor (`src/backend/`).** Phase 5
  Stage 5b. Pluggable codegen surface. Future backends (Python, WASM
  Component Model, Zero) drop in as additional impls without touching
  the CLI dispatch.
  - `backend::Backend` — associated `NAME`, associated `Config`, single
    `emit(&hir, config) -> Result<Artefact, BackendError>` method.
  - `backend::Artefact { path, kind, metadata }` and
    `backend::ArtefactKind::{NativeBinary, Wasm, SourceFile { ext }}`.
  - `backend::BackendError::{Io, CodegenFailed, UnsupportedFeature}`
    with `to_json()` for `ilo build --json` (JSON shape documented on
    the method).
  - `backend::cranelift::CraneliftBackend` — first concrete impl. Wraps
    the existing `vm::compile_cranelift::compile_to_binary` so codegen
    is preserved exactly. `CraneliftConfig` carries the bytecode
    `CompiledProgram` as a documented side-channel until Cranelift is
    lowered to consume HIR directly (deferred).
  - `ilo build file.ilo` dispatches through the trait. No CLI change,
    no user-visible behaviour change.
  - `ILO_KEEP_OBJ=1` env var preserves the Cranelift `.o` file after
    linking, for object-level byte-identical regression testing.
  - `tests/aot_byte_identical.rs` — object-level byte-identical regression
    against 136 baseline `.o` sha256s captured at Stage 5a tip. The
    linked-binary level is not suitable because `libilo.a` content
    changes with every Rust code addition; the `.o` isolates Cranelift
    codegen output.
  - `tests/aot-baselines/` — `obj-baselines.tsv` + `MANIFEST.md`
    documenting capture point, determinism notes, and regeneration
    procedure.
- **Python backend refactor (`src/backend/python/`).** Phase 5 Stage 5c.
  The existing Python transpile (was `src/codegen/python.rs`) now lives
  behind the `Backend` trait. Validates the trait against a transpile-style
  backend, complementing Cranelift's direct codegen shape.
  - `backend::python::PythonBackend` — second concrete impl. Consumes the
    verified AST via `PythonConfig::program`; HIR is taken as input on the
    trait surface but currently ignored (HIR does not yet carry the full
    surface the Python emit needs; lowering it is a later concern).
  - `ilo build file.ilo --py [-o out.py]` is the canonical CLI form.
  - `tests/python_emit_byte_identical.rs` + `tests/python-baselines/` —
    10 baseline `.py` files captured pre-refactor; the test asserts the
    post-refactor `ilo build --py` output matches byte-for-byte.

- **WASM Component Model backend (`src/backend/wasm/`).** Phase 5
  Stage 5d. The first genuinely new backend: emits `.wasm` (and a
  sibling `.wit`) via the `wasm-encoder` crate. One backend, many
  edge runtimes (Wasmtime, Cloudflare Workers, Fastly Compute,
  Vercel Edge, Wasmer).
  - `ilo build file.ilo --wasm` — defaults to `--target wasm32-component`
    (Component Model wrapper via `wasm-tools component new` + the
    bundled WASI preview1 adapter).
  - `--target wasm32-wasip1` — plain WASI preview1 core module.
  - `--target wasm32-wasip2` — placeholder for preview2; same encoder
    output as wasip1 today.
  - `--target wasm32-unknown-unknown` (alias `wasm32-web`) — browser
    target with no host imports.
  - Stage 5d covers the hello-world subset of HIR: top-level `prnt`
    calls with string, number, or bool literal arguments, plus `Ok` /
    literal tail expressions. Richer HIR constructs surface as
    `BackendError::UnsupportedFeature` pointing at the native Cranelift
    backend; capacity to lower them lands in subsequent stages.
  - Capability mismatches surface at emit time as
    `BackendError::CodegenFailed { code: "ILO-B201", .. }` with a hint
    naming the supported targets. The full per-target builtin matrix
    lives in `docs/wasm-capabilities.md`.
  - WASI preview1 adapter bundled in-tree at
    `assets/wasi-adapter/wasi_snapshot_preview1.reactor.wasm` (~52KB,
    pinned to Wasmtime v25). Offline builds work; no fetch on first
    `--wasm` invocation.
  - New deps: `wasm-encoder = "0.249"` (runtime, MIT / Apache-2.0),
    `wasmparser = "0.249"` (dev-only validator). `wasm-tools` is invoked
    as a subprocess for the Component Model wrap, not a library dep.
  - `tests/wasm_emit.rs` — encoder round-trip + capability matrix +
    JSON error shape (6 tests).
  - `tests/wasm_runtime.rs` — Wasmtime-subprocess execution of a WASI
    hello-world and a 3-line print sequence (2 tests, skipped when
    `wasmtime` is not on PATH).
  - Error code namespace `ILO-B2##` reserved for the WASM backend.
    Cranelift uses `ILO-B1##`, Zero will use `ILO-B3##`, Python `ILO-B4##`.

- **Zero transpile backend (`src/backend/zero/`).** Phase 5 Stage 5e.
  Real ilo to Zero bridge: the two-layer-stack thesis now has a working
  source-level handoff plus a chained `--0bin` path for native binaries
  built by Zero's own toolchain.
  - `ilo build file.ilo --0 [-o out.0]` — emit idiomatic Zero source.
    The generated `main` matches Zero's canonical entry shape:
    `pub fun main(world: World) -> Void raises { check world.out.write("...\n") }`.
  - `ilo build file.ilo --0bin [-o bin]` — emit `.0` source then invoke
    the pinned `zero` compiler (0.1.2) to produce a native binary. Both
    paths produce identical source; `--0bin` adds the build step.
  - Stage 5e v1 covers the same hello-world subset as the WASM backend:
    top-level `prnt` calls with text/number/bool literal arguments, plus
    `Ok` / literal tail expressions. Richer constructs surface as
    `BackendError::CodegenFailed { code: "ILO-B3##", .. }` with hints
    pointing at the Cranelift native backend.
  - Pinned toolchain: `zero 0.1.2`. Recorded in `.zero-version` at the
    repo root and as `PINNED_ZERO_VERSION` in
    `src/backend/zero/mod.rs`. Subprocess invocation prefers
    `/Users/dan/.zero/bin/zero` and falls back to `zero` on PATH; a
    missing compiler surfaces `ILO-B303` with the install one-liner
    (`curl https://zerolang.ai/install.sh | sh`).
  - `zero build --json` flag passed by default; both stdout and stderr
    captured because Zero 0.1.2 prints diagnostics to stdout.
  - Error code namespace `ILO-B3##`: `ILO-B301` (zero rejected source),
    `ILO-B302` (HIR construct unsupported), `ILO-B303` (`zero` missing),
    `ILO-B304` (IO), `ILO-B305` (entry not found).
  - `tests/zero_emit.rs` — source emit + `zero check` validation
    (4 tests, subprocess tests skipped when `zero` is not on PATH).
  - `tests/zero_binary.rs` — `--0bin` round-trip: ilo source to Zero
    source to native binary to expected stdout (2 tests, skipped when
    `zero` is not on PATH).
  - `tests/zero_capability.rs` — asserts unsupported features surface
    with the documented `ILO-B3##` codes (5 tests).
  - `examples/zero-bridge/hello.ilo` + README demonstrate the chain.
  - Capability matrix at `docs/zero-transpile-capabilities.md` mirrors
    the prep doc; covers clean / shim / unsupported constructs, error
    codes, and the Zero upgrade procedure.
  - No new runtime deps. `zero` is subprocess-only for `--0bin`; not
    linked into `libilo.a`.

- **CLI surface lock + conformance + walker delete.** Phase 5 Stage 5f.
  - `ilo build --help` (new) prints the manifesto-strict surface: exactly
    five forms, one per backend, with one-line descriptions. Same five
    forms listed in `ilo --help`.
  - Throwaway HIR scaffolding (`src/hir/walker.rs`, `src/hir/raise.rs`,
    `tests/hir_roundtrip.rs`) deleted. The cross-backend conformance suite
    supersedes the round-trip check.
  - `tests/conformance.rs` walks every conformance-headered example in
    `examples/` and exercises Cranelift, Python, WASM, and Zero
    end-to-end. Marked `#[ignore]` because of cost (~70s, 218 cases × 4
    backends); run with `cargo test --release --features cranelift
    --test conformance -- --ignored --nocapture`. Reports per-backend
    pass / skip / unsupported / fail counts at the end. Honest numbers
    above.
  - Per-example skip markers: `-- conformance-skip-<backend>: <reason>`.

### Changed (breaking)

- **`--emit python` removed.** The legacy `ilo <file-or-code> --emit python`
  form no longer transpiles. Per the manifesto-strict CLI (one canonical
  form per backend), it now prints a migration hint and exits with code 2:
  `ilo build <file.ilo> --py`. Pre-1.0 we break this cleanly; the migration
  hint stays in 26.X and goes away in the next release.

### Not changed in 26.X

- The internal engine-selector flags (`--run-tree`, `--run-vm`, `--run-llvm`,
  `--jit`) remain on the `ilo run` / positional surface. The Phase 5 brief
  scoped CLI cleanup to `ilo build`'s output flags; sweeping the engine
  selectors touches 170+ test files and is a separate cleanup. Engine choice
  is internal in spirit (the `Backend` trait now owns codegen); making it
  fully internal in the CLI is Phase 6 work.

No public API changes (other than `--emit python` removal). No other CLI changes. No behaviour changes.

### Added (more from main, builtins)
=======
- `_=expr` explicit discard bind. Evaluates `expr` for side effects and drops the result without allocating a binding. The `_` sigil is not a real local — it cannot be read back after the statement. Primary uses: (a) silencing ILO-T033 when discarding the return value of `mset`/`+=`/`mdel` is genuinely intentional, (b) calling a side-effecting function at non-tail position when the return value is irrelevant. All three engines (tree-interpreter, VM, Cranelift JIT/AOT) produce the same behaviour: the RHS expression is fully evaluated, its result is discarded with no register/slot allocation. The verifier still checks the RHS for type errors and T005 undefined-function; it does not insert `_` into scope so a subsequent `_` reference still resolves to the wildcard/nil sentinel. (ILO-36)

- `idxof s sub > O n` builtin. Returns the first Unicode code-point index of `sub` in `s`, or nil when not found. Index is in code-point units (same convention as `at`), not raw byte offsets. Empty `sub` returns 0 (Python / JS semantics). Closes the verbose `flt`+`len` workaround scrapingbee-chain and tui-client personas reached for when locating substrings. Tree-bridge eligible: pure 2-arg text-in / option-n-out, no FnRef args, no I/O. VM and Cranelift inherit through the bridge without new opcodes. Part of the 0.13.0 text-utility batch (ILO-39).
- `\xNN` hex escape in string literals. Two hex digits after `\x` encode a single Unicode code point in U+0000..=U+00FF. Case-insensitive (`\x1b` and `\x1B` both produce ESC). Non-hex digits after `\x` are passed through literally (lexer stays infallible). Closes the ANSI-escape friction that tui-client personas hit when embedding colour codes (`"\x1b[31m"` is now legal). Part of the 0.13.0 text-utility batch (ILO-39).
>>>>>>> origin/main

- `matvec xm ys > L n` builtin. Native matrix-vector product as a flat vector. Replaces the `flatten matmul xm (map (y:n>L n;[y]) ys)` ceremony every linear-regression-style persona was paying (three lines / ~10 tokens per use). Errors as `ILO-R009` on dim mismatch, empty matrix, or ragged rows. Tree-bridge eligible -- VM and Cranelift inherit through the bridge without new opcodes. Closes pending.md #5an.
- `lstsq xm ys > L n` builtin. Ordinary least squares via the normal equations: returns the coefficient vector `b` minimising `||xm·b - ys||²`. Closed-form OLS as a thin wrapper around `solve (Xᵀ X) (Xᵀ y)` - collapses the 5-line recipe (`transpose` + `matmul` + `matmul` + `solve` + index-fiddling) into a single call, saving ~30 tokens per OLS use. Errors as ILO-R009 on rank-deficient design, underdetermined system (cols > rows), row/length mismatch, or empty input. Same precision tier as `solve`/`inv`/`det` (LU with partial pivoting); numerically inferior to QR/SVD for ill-conditioned designs. Tree-bridge eligible - VM and Cranelift inherit through the bridge with no new opcodes. Motivated by the linear-regression persona.
- `OP_TAILCALL` opcode and VM-compiler emission. When a static user-fn call sits in tail position (the function's last statement, or the last statement of any context that itself sits in tail position), the bytecode VM compiler now emits `OP_TAILCALL` instead of `OP_CALL` + `OP_RET`. At runtime the VM reuses the current `CallFrame` rather than pushing a new one, so a function that recurses only in tail position runs in O(1) frame memory: `count-down 5_000_000` now completes in tens of milliseconds on `--vm`, mirroring the tree-interpreter trampoline shipped in the previous PR. Cross-function tail chains (`f` tail-calls `g` tail-calls `h`) work too -- the chunk index is swapped in place. Auto-unwrap (`!` / `!!`) calls stay on the normal `OP_CALL` path because the post-call result probe wants to inspect the value before deciding whether to propagate. The Cranelift JIT/AOT path lowers `OP_TAILCALL` identically to `OP_CALL` for now (semantically correct, no host-stack TCO benefit); native `return_call` lowering ships in a follow-up PR.
- `jpar-list text` builtin: parse a JSON string and assert the top-level value is an array. Returns `R (L _) t`. `jpar-list! body` unwraps to `L _` directly, so `@x (jpar-list! body){...}` type-checks without a binding or type annotation. `jpar` is unchanged (returns `R _ t`); use `jpar-list` when the response is known to be a JSON array.
- `get-to url timeout-ms > R t t` and `pst-to url body timeout-ms > R t t` builtins. HTTP GET and POST with an explicit per-request timeout in milliseconds. The timeout rounds up to the nearest whole second (minreq granularity). Returns `Err` when the deadline is exceeded, identical to any other connection failure from the caller's view. Both are tree-bridge eligible so VM and Cranelift JIT/AOT inherit them without new opcodes. Closes pending.md item #29.
- `tz-offset tz:t epoch:n > R n t` builtin. Returns the UTC offset in seconds for a named IANA timezone at a given Unix epoch. Handles DST transitions correctly via chrono-tz: the offset reflects the actual local time rule at that instant. Positive values are east of UTC. Returns `Err` on unknown timezone name. Tree-bridge eligible (VM and Cranelift dispatch through the tree interpreter, no new opcodes). Covers London BST/GMT transitions, New York EST/EDT, Tokyo JST (no DST), and the full IANA tz database.
- `run2 cmd:t args:L t > R RunResult t` builtin. Like `run` but returns a typed `RunResult` record (`r.stdout:t`, `r.stderr:t`, `r.exit:n`) instead of a loose Map. The key difference: `exit` is a number, not text, so numeric comparisons work directly (`=0 r.exit`, `<0 r.exit`). Non-zero exit is NOT an error; `Err` only on spawn failure (cmd not found, permission denied, etc.). Signal-killed processes on Unix surface as `exit:-1`. Same no-shell-no-glob security model, same concurrency + 10 MiB output-cap policy as `run`. Tree-bridge eligible (VM and Cranelift dispatch through the interpreter arm). `run` is unchanged for compatibility.
- `rgxall-multi pats:L t s:t > L t` builtin. Apply multiple patterns to a single string and get one flat list of all hits in pattern order. Per-pattern semantics follow `rgxall1`: 0 capture groups returns whole matches; 1 capture group returns capture-1 strings; 2+ capture groups errors with a hint to use `rgxall`. Replaces the verbose `flat (map (p:t>L t;rgxall1 p line) pats)` workaround (~20 tokens per call site saved). Motivated by cron-explainer and historical-archeologist personas, which both needed multi-pattern scan on a single line. Tree-bridge eligible alongside `rgxall1`; no new opcodes.
- `fmod a b` builtin: floor-mod, always non-negative when `b > 0`. Equivalent to Python `a % b` and JS `Math.floor((a % b + b) % b)`. Implemented across VM, JIT, and AOT. Eliminates the `(raw + 7) % 7` workaround that every TZ/weekday persona needed with signed `mod`. `mod` is unchanged (C-style signed remainder).
- `dtparse-rel s now > R n t` builtin. Resolves a natural-language relative-date phrase to a Unix epoch anchored at `now`. Supported: `today`/`yesterday`/`tomorrow`, `N days/weeks/months ago`, `in N days/weeks/months` (singular + plural), `last/next/this <weekday>` (monday-sunday or mon-sun; `last`/`next` never return today), and ISO-8601 `YYYY-MM-DD` passthrough. Month arithmetic clamps to the last valid day (Jan 31 + 1 month = Feb 28/29). Tree-bridge eligible -- VM and Cranelift pick it up automatically. Eliminates ~40 LoC of date-arithmetic helpers per date persona (P1 #8 from the persona feedback log).
- `dur-parse s > R n t` and `dur-fmt n > t` duration builtins. `dur-parse` parses human-readable duration strings ("3h 30m", "1 week 2 days", "1.5 hours", "90s") into total seconds; lenient, accepts abbreviations `s`/`m`/`h`/`d`/`w`, full singular/plural names, decimal quantities, and mixed sequences with or without spaces between number and unit. A leading `-` is sticky and applies to every following token until an explicit `+` resets it, so `"-1m 30s"` parses to `-90` (which makes the `dur-fmt -> dur-parse` round-trip symmetric for negative durations). Months are deliberately unsupported because a month is not a fixed number of seconds; `"3mo"` / `"3 months"` error rather than silently misparse. `dur-fmt` formats seconds as human-readable text, dropping zero parts and using the largest applicable units ("2h 42m", "1 day", "30s"). Negative values emit a single leading minus. Fractional seconds are preserved with up to 3 decimal places (`90.5 -> "1m 30.5s"`, `0.5 -> "0.5s"`); trailing zeros are stripped. Both are tree-bridge eligible: VM and Cranelift dispatch through the interpreter arm with no dedicated opcodes. Closes the duration-parse/format gap that `schedule-arithmetic` and `event-chronology` personas were stubbing out manually.
- File version pragma (optional). Top-of-file sigil `^26.5` declares the minimum required runtime. Sigil-led (principle 4), ~3 tokens (principle 1), first-class syntax (not a magic comment). Must be on the first line with no leading whitespace when present. Verifier: absent pragma silently assumes the latest installed runtime (no diagnostic) so existing 0.x files and any unannotated file keep verifying as-is; a pragma older than the runtime with a known breaking change between fails with a migration pointer; a pragma newer than the runtime fails asking to upgrade. Tooling: `ilo --version-of <file>` reads the pragma and returns nothing when absent; the formatter canonicalises position when present and never inserts one. Ships with the CalVer cut.

## 0.12.1

### Security

- Added a gitleaks secret-scan gate to the release workflow. Every `v*` tag push now runs `gitleaks/gitleaks-action@v2` against the full repo before the build, build-wasm, release, and publish jobs fire. A leaked API key, token, or PEM blocks the tag. Whitelist lives in `.gitleaks.toml` at the repo root and covers the placeholder strings used by `examples/apps/*` (`SCRAPINGBEE_KEY_PLACEHOLDER_...`, `sk-PLACEHOLDER-...`, `YOUR_*_HERE`, `REPLACE_ME`). Default gitleaks ruleset otherwise. Run locally with `gitleaks detect --source . --no-git`.

### Breaking

- `ls dir` renamed to `lsd dir`. Six rerun10 personas tripped ILO-P011 on `ls=rdl! p` because `ls` was reserved; rename frees `ls` for user code. `walk`, `glob` unchanged.
- `--run-tree` and its `--run` alias removed from the public CLI. They now error with the unknown-flag guard. The tree-walker stays in-tree as the dispatch target for the HOF / regex / fmt-variadic / fmt2 / IO / sleep / ct / rsrt / closure-bind-ctx shapes the VM and Cranelift haven't lifted natively yet; the VM bails to it transparently for every op in `is_tree_bridge_eligible`. Use `--vm` (the default) for everything else.
- `ilo tools --json` is now an envelope `{"schemaVersion":1,"tools":[...]}` instead of a bare array. Indexing consumers should read `.tools[0]` instead of `[0]`. Brings the last hold-out into the uniform CLI `--json` contract — every other emitter (`run`, `graph`, `--ast`, `serv`, `spec --json`) gained `schemaVersion:1` additively in the same release.

### Renamed

- `--run-vm` renamed to `--vm`, symmetric in shape with `--jit` and `--run-llvm` (where the flag names the engine, not the action). `--run-vm` is retained as a hidden alias for one release; every invocation emits a one-shot stderr hint `hint: --run-vm → --vm (canonical form). The --run-vm alias will be removed in 26.X.`. Carry-forward scripts and personas that hard-coded `--run-vm` keep working through 0.12.x and pick up the nudge to update. Hard removal lands in 26.X with the tree-walker drop.

### Diagnostics

- ILO-P011 now catches every builtin alias (`head`, `length`, `filter`, `concat`, `tail`, `sort`, `reverse`, `flatten`, `contains`, `group`, `average`, `print`, `trim`, `split`, `format`, `regex`, `read`, `readlines`, `readbuf`, `write`, `writelines`, `lset`, `floor`, `ceil`, `round`, `rand`, `random`, `rng`, `string`, `number`, `slice`, `unique`, `fold`) as a reserved name when used as a binding LHS or user-function declaration, at all three parser sites (top-level, local, fn-decl). Previously only `rng` and `rand` had per-alias guards; every other long-form alias like `head=...` was accepted silently and the call-site rewrite to the canonical builtin (`hd`) bypassed the user binding entirely, returning empty output with no error. A single `resolve_alias` check now covers the full alias table so new aliases land protected automatically. Surfaced by rerun-prompt-generator and changelog-validator rerun12, which both bound `head=...` and got empty output.

- `schemaVersion: 1` uniform across every CLI `--json` envelope. Five legacy emitters (`ilo run`, `ilo graph`, `ilo --ast`, `ilo serv`, `ilo tools --json`) and the new `ilo spec --json` mode all now carry the field at the top level so a single routing branch in agent code handles every command. For four of those five the change is strictly additive (object envelopes get one extra field); `ilo tools --json` is the lone breaking-but-additive wrap noted above. `ilo serv` carries `schemaVersion:1` on every line, including the `ready` handshake and every error response (`request`, `lex`, `parse`, `verify`, `runtime`, `program` phases). Full audit in `JSON_OUTPUT.md`.
- `ilo spec --json [lang|ai]` new mode wraps the markdown / `ai.txt` prose as `{"schemaVersion":1,"format":"markdown"|"ai-txt","content":"..."}`. Plain-text mode is unchanged.

### Added

- `prod xs > n` and `cprod xs > L n` builtins. `prod` is the multiplicative mirror of `sum` (product of all elements; empty list returns 1). `cprod` is the running-product mirror of `cumsum` (each element i is the product of xs[0..=i]; empty list returns []). Both are fully compiled across tree, VM, JIT, and AOT. The `prod []` identity-1 semantic matches the mathematical convention and avoids the ILO-R009 empty-list error that `avg []` raises.
- `wra path s` write-append builtin. Appends text to the file at `path`, creating it if it does not exist. Signature `R t t` mirrors `wr`; Ok returns the path, Err returns the OS error message. Lowers through the tree-bridge so all three engines (VM, Cranelift JIT, AOT) pick it up without a new opcode. Python codegen emits `open(path, 'a')`. Covers the "incremental log / accumulate output across steps" pattern agents reach for next after `wr`.
- `examples/apps/` directory of real-world programs harvested from persona-rerun workspaces (batch-loop-orchestration, agent-repair-loop, error-budget, doc-discovery, config-shaper, ecommerce-analytics, text-mining). Each program ships with `-- run:` / `-- out:` markers and any required input lives in `examples/apps/fixtures/`, so the existing `tests/examples_engines.rs` harness exercises them on every CI run. The harness now recurses one level into subdirectories. Complements the flat `examples/` layout (per-feature pins) with end-to-end shapes that an agent actually writes.
- `mpairs m > L (L _)` builtin returns a sorted-by-key list of `[k, v]` 2-element lists. Invariant: `mpairs m == zip (mkeys m) (mvals m)`. Replaces the common `map (fn k > [k (mget m k)]) (mkeys m)` cascade, killing both a lambda and a per-iteration hashmap lookup. New OP_MPAIRS opcode; tree, VM and JIT all share the same sort-then-zip walk.
- `rand` alias for `rnd` (universal short-form for random; matches C/Python/Rust/Go/JS naming). Resolves to canonical `rnd` after parsing; rejected as binding or user-fn name via ILO-P011 to prevent silent shadow mis-dispatch. `random` continues to resolve to `rnd` as before.
- `ilo check --strict` flag. Treats every warning-severity diagnostic (ILO-T032 bare `fmt`, ILO-T033 bare `mset`/`+=`/`mdel`, future warning codes) as a hard exit-code failure so CI harnesses can fail-on-warning. The diagnostic stream itself is unchanged: warnings still emit with `severity: "warning"` in the JSON output, only the exit code is elevated. Surfaced by rerun11 ci-gating personas that ran `ilo check src/*.ilo` in CI and missed mset / fmt traps because the verifier exited 0 on warnings.
- `mget-or m k default > v` and `lget-or xs i default > a`. Defaulted lookups for Map and List that return the element type directly, no `O v` to coalesce, no OOB error for `lget-or`. The verifier enforces that the default matches the container's element/value type so the return shape is `v` / `a`, never `O v`. Both lower through the tree-bridge, so every engine inherits semantics without new opcodes. Closes the manifesto-friction `(mget m k) ?? d` and `i<len?at xs i:d` ceremony agents kept reaching for.
- `argmax xs > n`, `argmin xs > n`, `argsort xs > L n`. Index-returning aggregates with numpy naming. `argmax` returns the 0-based index of the maximum element (first occurrence wins on ties); `argmin` the same for minimum; `argsort` returns the stable sorted-index permutation ascending (smallest to largest, empty list returns `[]`). All three error on empty input except `argsort`. All lower through the tree-bridge, so VM and Cranelift inherit them without new opcodes. Closes the `srt fn (enumerate xs)` + extract-first pattern agents converged on for argmax/argmin-style queries.
- `dirname path > t`, `basename path > t`, `pathjoin parts:L t > t` path-manipulation builtins. POSIX semantics with Unix forward-slash separator (Windows backslash handling deferred to a future release). `dirname` returns `""` (not `"."`) for plain filenames so `pathjoin [dirname p basename p]` round-trips without injecting a phantom `./` prefix. `pathjoin` is list-form (not variadic) to avoid the ILO-P101 arity-inference trap. Pure text ops, no I/O, no Result wrapper, tree-bridge eligible so VM and Cranelift inherit cross-engine parity for free. Closes the four-builtin `cat (slc (spl p "/") 0 -1) "/"` dance every filesystem persona was paying.
- `rdin > R t t` and `rdinl > R (L t) t`. Stdin read primitives. `rdin` reads all of stdin as text; `rdinl` reads it line by line with newlines stripped. Both return Err on I/O failure and on WASM targets (where stdin is unavailable). Both are 0-arg and lower through the tree-bridge so VM and Cranelift inherit them without new opcodes. Unblocks the Unix-pipeline persona class: programs can now receive piped input directly instead of reading a file or embedding data in argv. Closes the gap surfaced in the rerun12 lang-surface proposal (#5 rdin/rdinl ADOPT).
- Math constants `pi` (3.141592653589793), `tau` (6.283185307179586), `e` (2.718281828459045). Zero-arg builtins returning the canonical IEEE-754 `f64` value. Tree-bridge-eligible, so VM and Cranelift JIT/AOT inherit with no new opcodes; Python codegen emits `math.pi` / `math.tau` / `math.e`. Stops agents hardcoding `3.14159...` or reconstructing pi via `* 2 (atan2 0 -1)` - both shapes surfaced in fft-peak rerun12. Note: because `e` is now a builtin name, any existing code using `e` as a local binding will get an ILO-P011 diagnostic on upgrade; rename to `ev`, `er`, or similar.
- `default-on-err r d > T` builtin. Unwraps `R T E` to `T`, returning `d` if the result is Err. The Result mirror of `??` (nil-coalesce for `O T`). Kills the common `?r{~v:v;^_:default}` pattern when the error payload is unused. Lowers through the tree-bridge (2-arg, pure), so VM and Cranelift JIT inherit semantics without a new opcode. Verifier emits ILO-T040 when the first arg is not `R T E` (hint steers at `??` only when the first arg is Optional, avoiding misleading steers for plain `n`/`t`/`b` first args); ILO-T042 when the default's type doesn't match the Ok type (split from T040 so the agent can target the right arg); ILO-T041 when `??` is used on a Result value (steering to `default-on-err`). T041 is intentionally suppressed when the lhs type is `Unknown` (e.g. type-variable params, `_`-typed values) to avoid false positives on generic code; regression-tested.

### Diagnostics

- ILO-T039: 0-arg user function used as a bare reference in value position. When an agent writes `v = my-fn` or `+my-fn 100` and `my-fn` takes no arguments, ilo now emits ILO-T039 with a hint pointing at `my-fn()` as the correct call form. Previously the agent received a silent type error or a generic type-mismatch diagnostic with no actionable suggestion, costing one or more retries.
- ILO-T006 on `lst` (and its `lset` alias) now carries a suggestion clarifying that `lst xs i v` is "list set at index" (3 args, returns a new list with index `i` replaced by `v`), not "last element". The 1-arg case (`lst xs`) points at the canonical `at xs -1` for last-element intent; other arities point at the 3-arg signature without misreading the call as a last-element attempt. Surfaced by git-workflow rerun11 - agents reached for `lst xs` and hit an empty-suggestion arity error.
- ILO-T013 on `cat "a" "b"` (the string-concat instinct from Python/JS) now suggests `fmt "{}{}" a b` or `+a b` as the canonical text-concat shapes. `cat` is list-concat; the verifier used to flag the type error but leave the user to guess the fix, costing one round-trip on every new-write. Surfaced by scaffold rerun11.

### Diagnostics

- `if` reserved-word hint now suggests `?expr{true:...;false:...}` (semicolon-separated arms) instead of the space-separated `?expr{true:... false:...}` shape, which the parser rejected with `ILO-P003 expected Semi, got False`. Agents following the hint hit a second error and burned tokens retrying. Surfaced by logs-forensics rerun11. Doc-discovery rerun11 re-confirmed. Same fix landed in SPEC.md and ai.txt. Added `examples/conditional-shapes.ilo` as the canonical in-context learning example covering all four bool-conditional shapes (`?h{true:a;false:b}`, `?h{a}{b}`, `?h a b`, `?h cond a b`).

### Fixed (docs)

- `rsrt fn xs` and `rsrt fn ctx xs` now documented in `SPEC.md`, `ai.txt`, `skills/ilo/ilo-builtins.md`, and the site builtins reference. The key-function and ctx-arg forms have been implemented since PR #316 (verifier, interpreter, parser, VM bridge eligibility, 9 regression tests, `examples/rsrt-by-key.ilo`) but were absent from the canonical docs, so agents couldn't discover them without reading source. Surfaced when grepping the four doc surfaces for `rsrt fn` returned zero hits.

### Fixed

- `?h cond a b` silent-truthy bug. A paren-grouped prefix-comparison in cond position (`?h (> p 0.5) 1 0`) was mis-parsed as a zero-param inline lambda, lifted into a synthetic decl, and silently always took the then-branch. ml-tabular rerun11's logistic-regression classifier scored 25.75% instead of 84.6%; streaming-tail and devops-sre rerun11 hit the same family. Two-layer fix: the parser now requires a `;` body separator at paren-depth 1 before treating `(> ...)` as an inline lambda, so `(> p 0.5)` parses as a grouped comparison; the verifier additionally rejects (ILO-T038) any ternary cond that doesn't type-check to `b`, catching the broader family (partial-applied fn-refs, `R b E` without unwrap, etc.). Hint steers to bind-first `c=<expr>;?h c a b` or the brace ternary `?cond{...}`.
- `walk dir` and `glob dir pat` now skip unreadable subdirectories (most commonly `chmod 000` or sandbox roots) instead of aborting the entire traversal. Previously the first permission-denied subdir would poison the whole walk and lose every readable path that had already been collected, breaking realistic uses like `walk /` or `walk ~/`. An unreadable root still returns Err so the agent can distinguish "starting point unreadable" from "descendant unreadable". Surfaced by filesystem-walk rerun11.
- `fmt` with a literal template now rejects slot/arg-count mismatches at verify (`ILO-T013`), with a targeted hint when a list literal is the sole value arg for a multi-slot template. Surfaced by pdf-analyst rerun11: `fmt "x={} y={}" [a, b]` used to silently produce `x=[a, b] y={}` because the list bound to the first `{}` and the second slot was left literal. `fmt` is variadic but does not splat lists - pass positional args, or use a single-slot template if you want the list rendered as one value.
- AOT-compiled binaries now bind list-typed `main` parameters (`main args:L t`, `main xs:L n`, etc.) the same way the tree-walker / VM / JIT do. Previously the AOT entry shim ran every argv slot through a scalar-only parser, so `main args:L t > t; cat args ","` silently produced `nil` (cat needed a `L t`, got a `t`) and `len args` returned the character count instead of the list length. The verifier passed in both cases, the divergence was runtime-only. `generate_main` now consults the entry function's declared param types and routes `L _` params through a new `ilo_aot_parse_arg_list` helper that mirrors the binary-side `parse_cli_args_typed` coercion (parses `[a,b,c]` literals, bare comma lists, and wraps every other shape as `[value]`). Surfaced by cli-builder rerun11.
- CSV/TSV reader now tracks quote state across record separators. A cell containing `\n` (which the writer correctly emits as a quoted multi-line field per RFC 4180) used to be re-parsed as two rows, so `rd path "csv"` silently disagreed with `wr path data "csv"`. The reader is now a single-pass scanner over the whole document and round-trips multi-line quoted fields, embedded quotes, and CRLF line endings byte-stably across tree and VM. Surfaced by csv-pipeline rerun10.

### Docs

- `[Records]` coverage in `ai.txt` expanded so the record-declaration syntax (`type name{field:type;...}`, space-separated constructor, strict vs `.?` access, nominal-typing rule, Map cross-reference, ILO-T019/T021/T022 verifier surface) is in the compact spec instead of buried in the long form. Surfaced by saas-platform / doc-discovery rerun11, where six agents had to grep the repo for `type ` because `ilo help ai` was thin on the topic.
- `skills/ilo/ilo-language.md` now lists the reserved short-builtin names (2-char and 3-char) plus the 4+ char forward-compatibility rule. Surfaced by rerun11: agents kept binding `lst`/`hd`/`rev`/`rd`/`split` etc., tripping ILO-P011, because the reserved list lived only in `SPEC.md` and never made it into the agent-loadable skill. Tight ~80-token addition, neighbouring sections trimmed to stay inside the 5000-token aggregate skill budget.
- `JSON_OUTPUT.md` updated to reflect that all six emitters brought into the convention in 0.12.1 (`ilo run`, `ilo graph`, `ilo --ast`, `ilo serv`, `ilo tools --json`, `ilo spec --json`) carry `schemaVersion: 1`. Audit table drops the "(legacy shape)" notes and adds an "Agent equivalent" column pointing `ilo repl` users at `ilo serv` and `ilo spec` users at `ilo spec ai`. Conventions section updated to state that every envelope carries `schemaVersion` rather than just new ones.

### Performance

- `mset` accumulator via helper fn no longer pays a ~1000x perf cliff. The canonical DRY refactor `addto m k v > mset m k v` followed by `m = addto m k v` in a loop now runs at the same speed as the inline form. New OP_MOVE_OWN / OP_CALL_OWN1 opcodes thread the first arg into the helper at the caller's RC, and a tail-position rewrite lets the helper's `mset m k v` fire the existing in-place fast path. 40k rows: 25.8s to 0.01s on VM; JIT and AOT linear at 1M rows.

### Diagnostics

- Parser diagnostics (ILO-P001/P003/P004/P005/P007/P009/P011/P013/P016) now render tokens using their source characters (`` `>` ``, `` `{` ``, `` `>>` ``, `` identifier `foo` ``, `` number `42` ``) instead of parser-internal `TokenKind` variant names (`Greater`, `LBrace`, `PipeOp`, `Ident("foo")`, `Number(42.0)`). The old wording `expected Greater, got PipeOp` told an agent nothing about what character it had typed; the new `` expected `>`, got `>>` `` shows the offending bytes directly. Surfaced by agent-repair-loop rerun11.

### Added

- `rand` alias for `rnd` (universal short-form for random; matches C / Python / Rust / Go / JS naming). Closes the round-vs-random muscle-memory trap where agents reach for `rnd` expecting "round" (drop-vowels of `round`) and silently get random floats. Canonical for random stays `rnd`; canonical for rounding stays `rou` (alias `round`).

## 0.12.0 - 2026-05-19

### Breaking

- VM is the default engine. No need to pass `--run-vm`.
- `$` is now the `run` sigil (was the HTTP `get` alias). Use `get url` for HTTP.
- `post` is now `pst`. Drop the vowel like every other I/O verb (`rd`, `wr`, `srt`, `flt`).

### Added

- `run cmd args > R (M t t) t` argv-list process spawn. Returns `{stdout, stderr, code}` on success. Result Err only on spawn failure; non-zero exits land in the `code` field. No shell, no interpolation, so no command-injection vector.
- `ls dir`, `walk dir`, `glob dir pat` filesystem traversal. Each returns `R (L t) t` so missing-dir and permission-denied are typed at the boundary.
- `env-all > R M t t` returns the full process environment as a Map, pairs with `env key`.
- `ilo run`, `ilo check`, `ilo build` verbs. Bare-arg form still works.
- `--json` on every subcommand. Schemas documented in `JSON_OUTPUT.md`.
- `--bench` JSON output includes an `engine` field so you can tell which engine produced the timing numbers.
- VS Code extension at `extensions/vscode/`. Syntax highlighting, snippets, `--` comment handling, Cursor install script.
- Tree-sitter grammar at `github.com/ilo-lang/tree-sitter-ilo`, covers 98% of the example corpus. Wires into Neovim, Helix, Zed.
- Modular skill: six pages (`ilo-language`, `ilo-builtins`, `ilo-errors`, `ilo-tools`, `ilo-engines`, `ilo-agent`) under 5k tokens each.
- `ILO-R015` AOT runtime fault diagnostic. Hard faults emit JSON to stderr before the OS reports the exit code.
- Engine audit corpus at `tests/engine-matrix/` covering every engine on every feature shape.
- Closed-loop benchmark harness at `research/closed-loop-bench/`.
- Memory-model guide at `site/docs/guide/memory-model.md`.

### Changed

- Closure-capturing HOFs (`srt`, `grp`, `uniqby` with inline lambdas) run natively on VM and Cranelift JIT. No more tree-bridge fallback.
- RC fast paths for sole-owned values on `rev` / `srt` / `flt`. In-place mutation when no other reference exists.
- Error-code namespaces allocated and stable; ranges documented in SPEC.

### Fixed

- AOT sum types compile (cranelift string-constant interning no longer collides across functions).
- AOT default entry resolves to `main` instead of "first declared function", so `ilo compile file.ilo -o out && ./out` works.
- AOT hard faults emit `ILO-R015` JSON instead of raw SIGSEGV.
- SPEC drift on closure-capture: it was claimed tree-only, but VM and JIT handled it from Phase 2 onward. AOT was the actual lag and is now documented honestly.

## 0.11.6 - 2026-05-17

- HeapObj::ListView foundation and OP_WINDOW reshape to emit ListView, dropping window-construction RC traffic from O(n·k) to O(n). Bio microbenches went from 4-6s to 0.18-0.49s.
- Inline lambdas Phase 1: parenthesised function literals lift to synthetic top-level decls; closure-capture lands later in 0.12.0.
- `rgxall1` flat-capture form, `ct` count-by-predicate builtin.
- Bare-bool prefix ternary `?h a b`.
- Source spans thread through Cranelift JIT runtime-error helpers.
- ILO-P021 rejects the `--N` prefix-binop trap.
- EOF parse errors anchor on the dangling token instead of line 1 col 1.
- CLI hyphenated subcommands and non-ident positionals route to `main`.

## 0.11.5 - 2026-05-16

- Cranelift JIT catches panics and falls back to non-JIT engines (handles the AArch64 near-call relocation assertion seen on `rustc 1.85`).
- HOF tree-bridge error parity on Cranelift.
- `?bool{a}{b}` sugar for prefix ternary, closing a five-release papercut.
- Brace-block function bodies accepted; multi-line hint shows both shapes.
- `x!` bare-ident bang fix (v0.11.4 P0 silently returning nil).
- `flt`/`map` fused over window into a stride-1 in-place loop.

## 0.11.4 - 2026-05-16

- Fixed Cranelift JIT `srt`-after-`map` TLS desync silent miscompile.
- Restored documented auto-run for `main` and inline programs.
- `OP_LISTAPPEND` rebind shape routes through the in-place helper on Cranelift JIT.

## 0.11.3 - 2026-05-16

- New builtin: `mapr` for short-circuit Result propagation across `map`.
- `padl` / `padr` accept an optional pad-char arg for zero-pad and dot-leader patterns.
- `fmt` rejects printf-style format specs (`{:...}`) at parse time instead of silently returning the literal.
- `sum` and `avg` now work on VM and Cranelift, not just tree.
- Shadow-rebind register aliasing fix on VM and Cranelift.
- `xs.i` desugars to `at xs i` when `i` is a bound variable.
- Parser rejects builtin-named binding LHS with ILO-P011, with rename suggestion.
- `at xs i` auto-floors fractional indices.
- Lexer decodes `\f \b \v \a \0 \/` escape sequences.

## 0.11.2 - 2026-05-15

- Inline lambdas Phase 1: parenthesised function literals lift to synthetic decls (closure-capture in Phase 2).
- Wire `rgx`, `rgxall`, `fmt`, `rd`, `rdb` through VM and Cranelift via tree bridge.
- `lst xs i v` plus `lset` alias for list-update.
- `chars s` builtin: explode string into single-char strings.
- `sleep ms` for pure-ilo polling tails.
- `frq` drops the type-prefix from output keys, matching `grp` convention.
- O(n²) → O(n) `mset` accumulator via RC=1 in-place HashMap mutation.
- `.?` returns nil on missing field, not just on nil object.
- Multi-line bodies inside brackets, parens, and `>>` pipe chains.
- `hd`/`tl`/`at` out-of-range errors harmonised across tree, VM, Cranelift.
- Entry function returning `Value::Err` exits 1.

## 0.11.1 - 2026-05-13

- CLI runs single-fn files automatically, lists multi-fn files. `--ast` gates AST dump.
- `ord` and `chr` for per-char codepoint round-trip.
- `rgxall` multi-match capture-group extraction for HTML scraping.
- `match` arms accept brace-block bodies.
- Reserved keywords accepted as field names at dot-access.
- camelCase accepted at post-dot field access.
- `at s i` on text no longer allocates a `Vec` per call.

## 0.11.0 - 2026-05-13

- Removed the custom ARM64 JIT backend. Cranelift JIT is the optimising path.
- New math builtins: `pow`, `sqrt`, `log`, `exp`, `sin`, `cos`, `tan`, `log10`, `log2`, `atan2`.
- `at xs i` for nth-element list access, with Python-style negative indexing.
- Builtins as HOF args (verifier + interpreter).
- `!` on Optional types across verifier, interpreter, VM, Cranelift.
- Nested generic types like `R (L n) t`.
- Snake_case field names in dot-access position.
- Prefix-binop expressions accepted as call arguments.
- Scientific-notation float literals.
- `??` accepted as a prefix operator.

## 0.10.3 - 2026-05-11

- Friendly errors for identifier-confusion cases (similar-name typos suggest the right binding).
- Skill documents the three ways to run ilo from an agent.

## 0.10.2 - 2026-05-11

- Fixed `slc` and `mset` silent miscompilation in loops.
- Release workflow publishes `pi-ilo-lang` to npm.
- Skills-ref validate in lint job.

## 0.10.1 - 2026-05-02

- AOT compilation via `ilo compile`. Full AOT opcode parity with the JIT.
- `OP_RECFLD_NAME` implemented in JIT and AOT.
- O(n²) → O(n) list-append; 5x speedup on `foreach` accumulator workloads.
- SKILL.md converted to the Agent Skills spec-conformant format.
- Renamed `--run-interp` to `--run-tree`.
- `ai.txt` tracked as source, drift-checked in CI.

## 0.10.0 - 2026-03-08

- Space-separated list literals and heterogeneous lists.
- `_` type changes from nil to any/unknown.
- Coverage rounds for VM, verifier, interpreter.

## 0.9.1 - 2026-03-08

- Rust safety review pass: removed problematic unwraps, scoped RAII for `ACTIVE_REGISTRY` pointer, debug assertions for `as_heap_ref`.
- Enum-based builtin dispatch.
- 221 new VM tests for interpreter parity.

## 0.9.0 - 2026-03-07

- Long-form aliases for builtins (e.g. `length` for `len`).
- Removed unwraps and unnecessary clones in cleanup pass.

## 0.8.2 - 2026-03-07

- Interactive REPL with nvim-style commands.

## 0.8.1 - 2026-03-07

- Full infix operator support.
- npm WASM package for universal installation.
- `mod` builtin for modulo / remainder.
- Guard-in-loop warning (ILO-W001).
- Idiomatic hints system.
- `==` accepted as sugar for `=` (equality).
- And/Or short-circuit fix on left-operand register clobbering.

## 0.8.0 - 2026-03-07

- P2 data builtins: `grp`, `flat`, `sum`/`avg`, `rgx`.

## 0.7.0 - 2026-03-06

- Internal refactor release (see [GitHub release](https://github.com/ilo-lang/ilo/releases/tag/v0.7.0) for diff).

## 0.6.0 - 2026-03-06

- JIT-arm64 handles mprotect failure.
- MCP stdin error handling no longer panics on partial reads.
- Tools JSON output no longer panics on unwrap.

## 0.5.0 - 2026-03-04

- First Cranelift JIT compiler pass.

## 0.4.0 - 2026-03-03

- Bytecode VM lands as a second engine alongside the tree-walker interpreter.

## 0.3.0 - 2026-03-01

- Type system, verify pass, error codes (the `ILO-XXXX` namespace begins here).

## 0.2.0 - 2026-03-01

- Builtins expand: collections (`map`, `flt`, `fld`), text helpers, basic I/O.

## 0.1.2 - 2026-02-27

- Lexer and parser bug fixes; manifest expanded.

## 0.1.1 - 2026-02-27

- Initial CLI flag set: `--ai`, `--tools`, `--help`.
- README and SPEC drafts.

## 0.1.0 - 2026-02-27

Initial public release. Tree-walker interpreter, prefix-notation language, manifesto published. Token-conservative design target set at one-third Python's tokens on the canonical example.
