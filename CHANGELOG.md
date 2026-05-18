# Changelog

## Unreleased

### Changed

- Versioning scheme: semver → CalVer. Releases are `YY.M` (e.g. `26.5`), patches `YY.M.P` (e.g. `26.5.1`). The version string carries recency so an agent loading `ilo spec --json ai` knows which spec applies without a changelog lookup. Last semver release is `0.12.1`; first CalVer release cuts on the next breaking change as `26.X`. Hard cut, no `0.13` bridge. Branching model splits: `main` carries stable + RC tags (`26.5`, `26.5.1`, `26.5.2-rc.1`), `next` carries dev tags only (`26.6-dev.N`). See `README.md#versioning` for the full release / patch flow.

### Added

- `.@` is the new canonical source file extension. `.@` tokenises as two tokens (`foo`, `.@`) on cl100k and o200k vs three for `foo.ilo` - one token saved per filename mention. All `examples/` and `tests/` source files in this repo have been renamed to `.@`. `.ilo` continues to be accepted but emits a deprecation hint on stderr at load time: `hint: .ilo extension is deprecated; rename to .@`. Rename your files with: `find . -name '*.ilo' -exec sh -c 'mv "$1" "${1%.ilo}.@"' _ {} \;`
- **Typed HIR module (`src/hir/`).** Phase 5 Stage 5a. A thin high-level
  intermediate representation that sits between the verified AST and concrete
  code emission. Every Phase 5 backend (Cranelift refactor, Python refactor,
  WASM Component Model, Zero transpile) will consume `hir::Program`. Includes:
  - `hir::lower(ast, verify_out)` — AST → HIR lowering pass.
  - Documented departures from the AST: function body tail-expression split,
    guard polarity folded into `UnaryOp(Not)`, `Ternary` → value-level `If`,
    `Alias`/`Use`/`Error` decls dropped.
  - `hir::walker::walk` — throwaway walker that raises HIR → AST and runs the
    existing tree interpreter. Used by the round-trip test only; deleted in
    Stage 5f when real backends supersede it.
  - `tests/hir_roundtrip.rs` — exercises every `examples/*.ilo` file with a
    no-arg `-- run:` annotation and asserts the AST-walk and HIR round-trip
    paths produce identical outcomes. 375 cases across 228 example files
    pass; zero round-trip failures.
  - `src/hir/DESIGN.md` documents the shape, the departures, the deferrals,
    and open questions for Stage 5b.
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

- `--run-vm` renamed to `--vm`, symmetric in shape with `--jit` and `--run-llvm` (where the flag names the engine, not the action). `--run-vm` is retained as a hidden alias for one release; every invocation emits a one-shot stderr hint `hint: --run-vm → --vm (canonical form). The --run-vm alias will be removed in 0.13.0.`. Carry-forward scripts and personas that hard-coded `--run-vm` keep working through 0.12.x and pick up the nudge to update. Hard removal lands in 0.13.0 with the tree-walker drop.

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
- `dirname path > t`, `basename path > t`, `pathjoin parts:L t > t` path-manipulation builtins. POSIX semantics with Unix forward-slash separator (Windows backslash handling deferred to 0.13.0). `dirname` returns `""` (not `"."`) for plain filenames so `pathjoin [dirname p basename p]` round-trips without injecting a phantom `./` prefix. `pathjoin` is list-form (not variadic) to avoid the ILO-P101 arity-inference trap. Pure text ops, no I/O, no Result wrapper, tree-bridge eligible so VM and Cranelift inherit cross-engine parity for free. Closes the four-builtin `cat (slc (spl p "/") 0 -1) "/"` dance every filesystem persona was paying.
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
