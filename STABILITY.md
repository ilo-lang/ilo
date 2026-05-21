# Stability matrix

ilo is pre-1.0 and under active development. Not everything churns at the same rate. This file tells agents and integrators which surfaces are safe to pin to today and which to expect to move.

The README banner says "expect breaking changes" - that's true at the language-as-a-whole level. But individual surfaces have very different change policies. An agent that scaffolds against `ILO-XXXX` error codes and `schemaVersion: 1` JSON envelopes will carry forward across releases; one that pins to a specific builtin's signature or a CLI flag name will not.

Three tiers below. Each entry names the surface, what's locked in, what isn't, and the change policy.

## Stable

Surfaces with a stable contract. Additive changes are allowed; breaking changes require a major-version bump and a migration note in `CHANGELOG.md`.

### JSON envelope: `schemaVersion: 1`

Every `--json` output across the CLI (`ilo check --json`, `ilo run --json`, `ilo skill list --json`, `ilo serv` responses, etc.) carries a top-level `schemaVersion: 1` field. The shape under that version is locked: fields may be added, but existing field names, types, and semantics will not change without bumping `schemaVersion`.

- **Stable**: presence of `schemaVersion`, top-level fields (`error`, `skills`, etc.) at their current names and types.
- **Not stable**: ordering of object keys (treat as unordered), prose inside `message` strings.
- **Change policy**: additive only at `schemaVersion: 1`. Any breaking change goes to `schemaVersion: 2` and ships alongside `1` for a deprecation window.

### Error code namespace: `ILO-<letter><digits>`

Every diagnostic has a stable code. The letter classifies the phase (`L` lex, `P` parse, `T` type, `R` runtime, etc.); the digits are an ID that never gets renumbered or reused.

- **Stable**: existing codes (`ILO-P009`, `ILO-T004`, `ILO-R009`, …) keep their meaning and phase letter forever. New codes are added with new IDs.
- **Not stable**: the human-readable message that follows the code. Prose is tuned constantly as agents trip on edge cases; agents should route on the code, not the string.
- **Change policy**: additive only. Codes are never renumbered, never reused, never silently demoted.

### `serv` protocol phases

The `ilo serv` line protocol (`READY` -> request -> response, with `schemaVersion: 1` JSON on each frame) is the integration surface for hosting ilo in another process.

- **Stable**: phase names, framing, `schemaVersion: 1` envelope on every frame.
- **Not stable**: the set of accepted request `kind` values can grow; existing kinds keep their semantics.
- **Change policy**: additive only inside `schemaVersion: 1`.

### File version pragma

`-- ilo 0.x` at the top of a source file pins the language version for that file. The pragma format itself (line 1 comment, `ilo <version>` token) is stable.

- **Stable**: the pragma syntax.
- **Not stable**: which language features a given version accepts. That's the whole point - the pragma exists so future ilo versions can ship breaking grammar changes without retroactively breaking older files.

### Manifesto principles

The token-cost framing in `MANIFESTO.md` is the durable design contract. Specific builtins and grammar move; the five principles (token-conservative, constrained, self-contained, language-agnostic, graph-reducible) do not.

### Reserved-name policy (forward-compatibility rule)

`SPEC.md#reserved-namespaces` documents the rule that future builtins land at 4+ characters. 2-character names not on today's reserve list stay safe across releases; 3-character names are highly likely to stay safe.

- **Stable**: the rule itself (new builtins go 4+ chars).
- **Not stable**: the full reserve list grows over time (additive).
- **Change policy**: a 3-char addition is allowed but called out in `CHANGELOG.md`. 2-char additions don't happen.

## Provisional

Surfaces that are user-facing but expected to move pre-1.0. Pin at your own risk; expect renames, signature changes, or removal with a deprecation notice in `CHANGELOG.md`.

### Individual builtin signatures

The set of builtins is growing every release. Individual signatures may tighten (extra optional arg), loosen (added overload), or in rare cases get renamed when the name turns out to mislead more than it helps. Aliases (e.g. `length` -> `len`, `concat` -> `cat`) are reserved alongside the canonical name; the canonical name is the stable target.

- **Provisional**: any individual builtin's exact arity, parameter order, error semantics on edge inputs.
- **Stable-ish**: the *existence* of the canonical short name for any builtin shipped in 0.12+ (we don't remove builtins; we add new ones).
- **Change policy**: signature changes ship with a hint at the old call site for at least one minor release before the old form is removed.

### CLI flag names

`--json`, `--max-ast-depth`, `--explain`, etc. may be renamed, consolidated, or get new short forms. The CLI surface is tuned by dogfooding, and that surfaces ergonomic issues that justify renames.

- **Provisional**: flag spellings, short forms, default values.
- **Stable**: subcommand names (`run`, `check`, `build`, `serv`, `test`, `help`, `skill`) - these don't churn.
- **Change policy**: renamed flags keep the old name as an alias for at least one minor release with a deprecation hint.

### Error message prose

The string after `ILO-<code>:` and any `hint:` lines below it. Tuned constantly.

- **Provisional**: every word of every error message and hint.
- **Stable**: the code itself, and the presence of a `hint:` line where one exists today.
- **Change policy**: prose can change at any release. Agents must not regex on the message string.

### `examples/` corpus and skill docs structure

The set of files in `examples/` and the markdown structure of `skills/ilo/*.md` is tuned per release to cover new dogfood findings.

- **Provisional**: file names, section headings inside skill docs.
- **Stable**: the existence of `skills/ilo/SKILL.md` as the top-level agent entry point.

### `ilo test` discovery and assertion syntax

The `-- run: <fn>` / `-- out: <expected>` example-runner conventions are stable for 0.12.x. The broader `ilo test` command surface (output format, exit codes, filter flags) is provisional.

- **Provisional**: `ilo test` flags and human-readable output format.
- **Stable**: `-- run:` / `-- out:` assertion comments at the top of an example file.

## Experimental

Surfaces explicitly marked experimental, behind a feature flag, or part of in-flight 0.13.0 work. May disappear without notice.

### 0.13.0 in-flight features

Anything labelled "0.13.0" in `ilo_assessment_feedback.md`, PR titles, or `CHANGELOG.md` unreleased section. Includes proposed `run-bg` / `srv` / `req` family, anonymous record literals, `_=expr` discard, paginated state, agent-natural surface, etc. Don't write production code against these until they ship in a released minor.

### AOT-compiled artifact format (`ilo build`)

The on-disk format of `ilo build` output is internal. Cross-version compatibility is not guaranteed; recompile from source on every ilo upgrade.

### Cranelift JIT internals

The Cranelift code path ships default-on as a Cargo feature; the JIT is reachable via env flags. Cross-engine *semantics* are stable (tree, VM, and JIT must agree); the JIT's compilation strategy, intermediate representation, and perf characteristics are not. Disabling the feature (`--no-default-features`) drops JIT but is not a supported integration mode.

### `extensions/` directory

The tree-sitter grammar, VSCode extension, and any IDE integration scaffolding under `extensions/` are best-effort and lag behind the language. Treat as experimental.

### Anything behind a `--features` flag

Cargo feature flags (`cranelift`, future flags for `srv` / `http-server` / etc.) gate experimental work. Surfaces visible only when a flag is enabled are experimental until the flag becomes default-on.

## How to read this

- If you're an agent generating ilo code: rely on the stable tier and the canonical builtin names. Don't pattern-match on error message prose.
- If you're integrating ilo into a pipeline: pin to `schemaVersion: 1` JSON envelopes and `ILO-<code>` error codes. Treat CLI flag spellings as moving targets and re-check `ilo --help` on every upgrade.
- If you're writing a tool against `ilo serv`: route on `kind` and code, ignore prose, expect new request kinds to appear.

When in doubt, `CHANGELOG.md` is the source of truth for what moved in any given release.
