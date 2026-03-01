# Shell Languages & Execution Research for ilo-lang

Research into shell languages, execution models, and patterns relevant to ilo — a
token-minimal programming language for AI agents. Evaluated against the manifesto:
**total tokens from intent to working code**.

Key thesis: Shell execution (`sh cmd` or `run cmd`) was identified as more important
than HTTP for local agents. Claude Code, Cursor, Codex, and other agentic CLIs all
use bash/shell as their primary tool. ilo currently has `get url` for HTTP but no
shell execution primitive. This document analyzes what ilo needs.

---

## 1. Bash/Zsh — The Baseline

### Core primitives

Bash is the lingua franca of shell scripting. Every AI coding agent uses it as
its primary execution tool. The primitives:

| Primitive | Syntax | What it does |
|-----------|--------|-------------|
| Command execution | `cmd arg1 arg2` | Fork + exec, wait for exit |
| Pipe | `cmd1 \| cmd2` | stdout of cmd1 → stdin of cmd2 |
| Redirect stdout | `cmd > file` | Write stdout to file |
| Redirect stderr | `cmd 2> file` | Write stderr to file |
| Redirect both | `cmd &> file` or `cmd 2>&1` | Merge stderr into stdout |
| Subshell | `$(cmd)` or `` `cmd` `` | Capture stdout as string |
| Process substitution | `<(cmd)` / `>(cmd)` | File descriptor as argument |
| Background | `cmd &` | Non-blocking execution |
| Chaining | `cmd1 && cmd2` | Run cmd2 only if cmd1 succeeds |
| Chaining | `cmd1 \|\| cmd2` | Run cmd2 only if cmd1 fails |
| Exit code | `$?` | Integer 0-255, 0 = success |
| Env vars | `VAR=val cmd` | Set for one command |
| Here-string | `cmd <<< "text"` | Feed string as stdin |

### How shell execution works internally

1. The shell **forks** a child process via `fork()` (or `posix_spawn`)
2. The child calls `exec()` to replace itself with the target binary
3. The parent **waits** (for foreground) or **continues** (for background)
4. File descriptors for stdin/stdout/stderr are set up before exec
5. Exit code is captured from `waitpid()` — 0 = success, non-zero = failure
6. Pipes connect fd1 of process N to fd0 of process N+1 via `pipe()`

### Environment variable handling

- Inherited from parent process at fork time
- `export VAR=val` makes it available to all children
- `VAR=val cmd` sets only for that one command (no export)
- `$VAR` expands at shell parse time, before command execution
- `env` command lists all current environment variables
- `.env` files are a convention, not a shell feature (need `source` or `dotenv`)

### What agents actually use from bash

Analysis of Claude Code, Codex CLI, and Cursor tool patterns reveals a clear
hierarchy of shell usage:

**High frequency (every session):**
- `cat`, `head`, `tail` — read file contents
- `grep`, `rg` — search file contents
- `find`, `ls` — discover files
- `git` subcommands — status, diff, log, add, commit, push
- `cd` — change working directory
- `echo` — write output / test commands

**Medium frequency (most sessions):**
- `npm`/`pip`/`cargo` — package management
- `python`/`node`/`bun` — run scripts
- `curl`/`wget` — HTTP requests
- `mkdir`, `rm`, `mv`, `cp` — filesystem operations
- `sed`, `awk` — text transformation
- `wc` — counting (lines, words)

**Low frequency (complex tasks):**
- `docker` — container operations
- `ssh` — remote execution
- `tar`, `zip` — archive operations
- `chmod`, `chown` — permissions
- `kill`, `ps` — process management

**Patterns observed:**
1. Commands are overwhelmingly **single-shot**: run, capture output, analyze
2. Pipes are common but typically 2-3 stages max: `cmd | grep | wc`
3. Exit codes are checked but not deeply — agents care about success/failure
4. Stderr is often more useful than stdout for error diagnosis
5. Agents rarely use background processes or job control
6. Environment variables are read (not set) in most tool invocations

### What bash gets wrong for agents

From ilo's OPEN.md, which already identified these:

- **No types** — everything is text. `jq` output looks like an error message
- **Silent failures** — `curl` can fail and the pipeline continues with empty input
- **Text parsing tax** — agents generate `grep`, `awk`, `sed` to extract structured data
- **Quoting hell** — escaping rules cause retry loops (token cost)
- **No verification** — `rm -rf /` is syntactically valid, semantically catastrophic

Resource profile from AgentCgroup research: test execution commands spike to
~518MB memory, file exploration uses only ~4.5MB, git operations ~13.5MB. The
burst-silence pattern means 98.5% of memory bursts occur during tool calls.

---

## 2. Nushell — The Closest Analog to ilo's Future

Nushell is the most relevant comparison for ilo's shell ambitions. It replaces
bash's text pipelines with structured data pipelines — exactly the model
ilo's OPEN.md describes as "typed shell for agents."

### Structured data pipelines

| Bash | Nushell |
|------|---------|
| Text streams | Records, tables, lists |
| `grep` to filter | `where` clause |
| `awk` to extract | `.column` access |
| `sort` (text) | `sort-by column` (typed) |
| `jq` for JSON | Native JSON/YAML/TOML/CSV parsing |
| Pipe: bytes | Pipe: structured values |

Example — find large files:
```
# Bash: text parsing required
ls -la | awk '$5 > 1000000 {print $9, $5}'

# Nushell: structured query
ls | where size > 1mb | select name size
```

The pipeline operator `|` in Nushell passes structured values between commands.
Each command declares its input/output types. The runtime checks type
compatibility. Errors carry source spans.

### Type system

Nushell has a dual type system:
- **Compile-time types:** checked during parsing, catch mismatches early
- **Runtime types:** the `Value` enum carries actual data with type tags

Recent additions (2025) include runtime type checking for pipeline input
types. Structural typing allows flexible matching — `[{a: 123} {a: 456, b: 789}]`
is a subtype of `table<a: int>`.

Types include: `int`, `float`, `string`, `bool`, `list`, `record`, `table`,
`duration`, `filesize`, `date`, `binary`, `nothing`, `error`.

**ilo parallel:** ilo's type system already has `n`, `t`, `b`, `_`, `L`, `R`,
records. Nushell validates that structured pipelines with types work. The key
difference: Nushell discovers types at runtime, ilo verifies at compile time.

### par-each — parallel iteration

Nushell's `par-each` replaces `each` for parallel execution:

```
# Sequential
ls | where type == dir | each { |row| { name: $row.name, len: (ls $row.name | length) } }

# Parallel — same syntax, different command
ls | where type == dir | par-each { |row| { name: $row.name, len: (ls $row.name | length) } }
```

Benchmarks show 3-4x speedup for I/O-bound operations (21ms to 6ms in one test).
The API is identical to `each` — drop-in replacement.

**ilo gap:** ilo has `@` for iteration but no parallel variant. The proposed
`@!` or `par{...}` from research/python-stdlib-analysis.md maps directly to
Nushell's `par-each`. This validates the design direction.

### Environment variables

Nushell's env model is structured:
- `$env.VAR` to read (record access, not string expansion)
- `$env.VAR = "value"` to set (scoped to current block)
- `$env.VAR?` for safe access (returns `nothing` if unset, not error)
- `$env.VAR? | default "fallback"` for defaults
- `with-env { FOO: "bar" } { command }` for temporary env
- `ENV_CONVERSIONS` for automatic type coercion (PATH string → list)

The `$env` is a structured record, not a flat string map. This means you can
pipe it, filter it, query it: `$env | select PATH HOME`.

**ilo implication:** Environment access should return structured values, not
raw strings. `env "PATH"` returning `L t` (list of text, split on `:`) would
be more useful than returning the raw colon-separated string.

### External command execution

Nushell bridges structured pipelines with external processes:
- `run-external` or `^command` for explicit external invocation
- Automatic detection: if not a builtin/custom command, try external
- I/O is handled via thread-based readers to prevent deadlocks
- Exit codes captured as structured values
- Background jobs via `job spawn`, killed via `job kill`

Limitation: background jobs are threads, not processes — they die when the
shell exits. No `disown` equivalent (as of 2025). External process spawning
into the background is still an open issue.

### Error handling

- `try { cmd } catch { |e| handle $e }` — structured error handling
- Errors carry source spans for precise location reporting
- `ShellError` instances include expected type, actual type, span
- Streams can yield `Value::Error` for per-element errors

### NUON — Nushell Object Notation

Nushell has its own serialization format that preserves all Nushell types
(durations, filesizes, dates). Unlike JSON, it round-trips without data loss.

**ilo lesson:** A native serialization format for ilo values would enable
efficient persistence and IPC between ilo processes.

---

## 3. Fish — User-Friendly Shell Design

Fish (Friendly Interactive Shell) is relevant for its design philosophy
around discoverability and error prevention, though less relevant for ilo's
execution model.

### Key design choices

- **Autosuggestions out of the box** — no plugins needed. Grayed-out suggestions
  from command history appear as you type. Right arrow to accept.
- **Syntax highlighting** — green for valid commands, red for errors, real-time
  as you type. Catches typos before execution.
- **No POSIX compliance** — deliberately breaks from POSIX in favor of cleaner
  syntax. `set VAR value` instead of `VAR=value`. No `$()`, uses `()` directly.
- **Man page parsing** — auto-generates completions from installed man pages.
  Zero-config tab completion for any installed tool.
- **Web-based configuration** — `fish_config` opens a browser UI for themes,
  prompts, and settings.

### Syntax differences from bash

| Bash | Fish |
|------|------|
| `export VAR=val` | `set -gx VAR val` |
| `VAR=val` (local) | `set VAR val` |
| `$(cmd)` | `(cmd)` |
| `[ condition ]` | `test condition` |
| `if [ ]; then; fi` | `if condition; end` |
| `for x in list; do; done` | `for x in list; end` |
| `function f() { }` | `function f; end` |
| `alias x='cmd'` | `function x; cmd $argv; end` |

### What's relevant for ilo

**Positive lessons:**
1. **Zero-config discoverability** — Fish proves that intelligent defaults
   beat configuration. ilo's "constrained" principle aligns: fewer choices,
   better defaults.
2. **Real-time validation** — syntax highlighting catches errors before
   execution. ilo's verifier does this at the language level.
3. **Helpful error messages** — Fish's errors explain what went wrong and
   suggest fixes. ilo already has this with "did you mean?" suggestions.

**Anti-patterns for ilo:**
1. **POSIX incompatibility** — Fish's choice to break POSIX means scripts are
   not portable. ilo doesn't have this concern (it's a new language), but it
   validates the approach: if you're going to break convention, break it
   completely and provide better alternatives.
2. **Interactive-first** — Fish is optimized for human interaction (web config,
   autosuggestions). ilo is agent-first. Different audience, different priorities.

Fish was rewritten from C++ to Rust in version 4.0 (February 2025), reaching
4.5.0 by February 2026. This validates Rust as a shell implementation language.

---

## 4. PowerShell — Object Pipeline Model

PowerShell pioneered the object pipeline — passing structured objects between
commands instead of text. This is directly relevant to ilo's vision.

### Object pipeline vs text pipeline

| Aspect | Bash | PowerShell |
|--------|------|------------|
| Pipeline content | Text bytes | .NET objects |
| Filtering | `grep pattern` | `Where-Object { $_.Prop -gt 5 }` |
| Extraction | `awk '{print $3}'` | `Select-Object Name, Size` |
| Sorting | `sort` (text) | `Sort-Object -Property Name` |
| Error handling | Exit codes + stderr text | Exception objects + error stream |
| Type safety | None | Rich type system (.NET) |
| Discovery | `man cmd` | `Get-Member`, `Get-Help` |

### How cmdlets work

Every cmdlet declares:
- **Input types** — what objects it accepts via pipeline
- **Output types** — what objects it produces
- **Parameters** — named, typed, with validation attributes
- **Parameter binding** — ByValue (match type) and ByPropertyName (match name)

This is remarkably similar to ilo's tool declarations:
```
# PowerShell: declared types, named params, pipeline input
function Get-UserProfile {
    [CmdletBinding()]
    param([Parameter(ValueFromPipeline)]$UserId)
    ...
}

# ilo: declared types, positional params, result type
tool get-profile"Get user profile" uid:t>R profile t
```

### Multiple output streams

PowerShell has 6+ streams: Output, Error, Warning, Verbose, Debug, Information.
Each carries structured objects. Compare to bash's 2 (stdout, stderr) carrying
text.

**ilo implication:** ilo's `R ok err` captures the most important distinction
(success/failure). Additional streams (debug, verbose) could be tool-provider
concerns, not language concerns.

### What PowerShell gets right for agents

1. **Object pipeline eliminates parsing** — no `grep`/`awk`/`sed` tax
2. **Type-safe parameter binding** — wrong types caught before execution
3. **Rich error objects** — errors carry context, not just text
4. **Discoverability** — `Get-Member` shows what an object can do

### What PowerShell gets wrong for agents

1. **Verbosity** — `Get-ChildItem | Where-Object { $_.Length -gt 1MB }` is 11
   tokens. Nushell's equivalent is 7, ilo's would be fewer.
2. **.NET dependency** — massive runtime, not embeddable
3. **Windows-first** — cross-platform support improved but not native
4. **Cmdlet naming convention** — `Verb-Noun` is discoverable but verbose

---

## 5. Bun Shell — JS-Native Shell Execution

Bun Shell is directly relevant because Anthropic acquired Bun in December 2025
to power Claude Code infrastructure. Bun Shell represents the direction
Anthropic is investing in for agent-native shell execution.

### The $ template literal

```js
import { $ } from "bun";

// Basic execution
await $`echo "Hello World"`;

// Capture output
const text = await $`ls *.js`.text();
const json = await $`cat data.json`.json();
const lines = await $`ls`.lines();

// Pipes
const result = await $`echo "Hello World!" | wc -w`.text();

// Destructure stdout, stderr, exit code
const { stdout, stderr, exitCode } = await $`cmd`.nothrow().quiet();
```

### Key design decisions

1. **Not a system shell** — Bun Shell does not invoke `/bin/sh`. It re-implements
   bash parsing in-process. This is faster and more secure.
2. **Automatic escaping** — all interpolated values are treated as single literal
   strings. `${userInput}` cannot inject commands.
3. **Promise-based** — every command returns a Promise. Natural for async JS.
4. **Multiple output formats** — `.text()`, `.json()`, `.lines()`, `.blob()`,
   `.arrayBuffer()`. The caller chooses the parse format.
5. **Exit code handling** — throws on non-zero by default. `.nothrow()` to
   capture instead. `.throws(true/false)` for global config.
6. **Built-in commands** — `ls`, `cd`, `rm`, `echo` etc. reimplemented natively.
   20x faster than zx for built-in operations.
7. **Cross-platform** — same API on Windows, Linux, macOS.

### Environment variables

```js
await $`echo $FOO`.env({ FOO: "bar" });  // scoped env
await $`echo $HOME`.cwd("/tmp");          // scoped cwd
```

### Redirect operators

```js
await $`cmd 1>&2`;        // stdout to stderr
await $`cmd 2>&1`;        // stderr to stdout
await $`cmd > file.txt`;  // stdout to file
await $`cmd 2> err.txt`;  // stderr to file
```

### What Bun Shell means for ilo

Bun Shell validates several ilo design directions:

1. **Shell-in-process is viable** — you don't need to fork `/bin/sh`. A shell
   re-implementation within the runtime is faster and more controllable.
2. **Structured output from shell** — `.json()`, `.lines()`, `.text()` prove
   that shell output can be structured at the boundary. ilo's `R t t` is the
   starting point; returning structured types (records, lists) would be the
   Bun Shell equivalent.
3. **Automatic escaping is essential** — for agent-generated commands, injection
   prevention must be built in. ilo's tool system already handles this (tools
   are typed functions, not string-interpolated commands), but a `sh` builtin
   would need the same protection.
4. **Exit code → Result mapping** — Bun Shell's "throw on non-zero" maps
   directly to ilo's `R ok err`. Exit code 0 → `~stdout`, non-zero → `^stderr`.

### Bun.spawn for lower-level control

```js
const proc = Bun.spawn(["cmd", "arg1", "arg2"], {
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
    env: { FOO: "bar" },
    cwd: "/tmp"
});
const output = await new Response(proc.stdout).text();
const exitCode = await proc.exited;
```

This lower-level API exposes the full process lifecycle: spawn, pipe I/O,
wait for exit. ilo's equivalent would be the `sh`/`run` builtin internally.

---

## 6. Deno — Permission Model (Validates ilo's Verifier)

Deno's permission model is the strongest validation of ilo's "closed world"
verifier approach. Deno proves that a runtime can be secure by default.

### Secure by default

A Deno program has **zero** OS access unless explicitly granted:
- No file reads (`--allow-read`)
- No file writes (`--allow-write`)
- No network (`--allow-net`)
- No environment variables (`--allow-env`)
- No subprocess spawning (`--allow-run`)
- No high-resolution time (`--allow-hrtime`)
- No FFI (`--allow-ffi`)

### Granular permissions

Permissions can be scoped to specific paths or hosts:
```
deno --allow-read=/path/to/dir --allow-net=api.example.com script.ts
```

### Permission auditing (Deno 2.5+)

```
DENO_AUDIT_PERMISSIONS=/var/log/deno-audit.jsonl deno run script.ts
DENO_TRACE_PERMISSIONS=1 deno run script.ts  # includes stack traces
```

Every permission check is logged with the permission type and value. Stack
traces show exactly where in code the access was attempted.

### Permission broker

For enterprise use, Deno can delegate all permission checks to an external
broker process:
```
DENO_PERMISSION_BROKER_PATH=/path/to/broker deno run script.ts
```

When using a broker, all `--allow-*` and `--deny-*` flags are ignored.
Every permission check goes to the broker, which decides per-request.

### Security advisories and escalation risks

Deno's experience reveals important edge cases:
- `--allow-read` on `/proc/self/environ` provides equivalent of `--allow-env`
- `--allow-write` on `/proc/self/mem` provides equivalent of `--allow-all`
- `--allow-run` essentially invalidates the sandbox (spawned processes aren't sandboxed)

Deno 1.43+ blocks access to `/etc`, `/dev`, `/proc`, `/sys` on Linux and
`\\` paths on Windows without explicit `--allow-all`.

### How this validates ilo's approach

ilo's "closed world" verifier catches issues at **verification time**:
- All function calls must resolve to known functions
- All types must align
- All dependencies must be declared

Deno does this at **runtime** with permission checks. ilo goes further — the
program cannot even *reference* an unknown capability. This is strictly better
for agents:

| Aspect | Deno | ilo |
|--------|------|-----|
| When checked | Runtime | Verification time (before execution) |
| Unknown function | Runtime error | Compile error (ILO-T005) |
| Type mismatch | Runtime or never caught | Compile error (ILO-T009) |
| Undeclared dependency | Permission denied at runtime | Cannot exist in program |
| Cost of violation | Wasted execution + retry | Zero execution cost |

**Key insight:** Deno's `--allow-run` effectively breaks the sandbox. For ilo,
a `sh` builtin would be similar — it grants arbitrary OS access. The solution
is NOT to prevent shell execution (agents need it), but to:
1. Make shell commands explicit in the program (not hidden in tool implementations)
2. Verify shell command arguments are well-formed where possible
3. Run with appropriate OS-level sandboxing (containers, seccomp, etc.)
4. Audit all shell invocations (Deno's audit log pattern)

---

## 7. What AI Coding Agents Actually Do

### The tool hierarchy

Based on analysis of Claude Code, Codex CLI, Cursor, and other agentic tools:

**Tools by frequency of use:**

| Rank | Tool | What it does |
|------|------|-------------|
| 1 | File read | Read file contents (cat, Read) |
| 2 | File search | Find files by name/content (grep, rg, find) |
| 3 | File write/edit | Modify file contents |
| 4 | Shell execution | Run arbitrary commands (build, test, lint) |
| 5 | Git operations | Status, diff, commit, push |
| 6 | Package management | npm install, pip install, cargo build |
| 7 | HTTP requests | Fetch URLs, API calls |
| 8 | Web search | Research / documentation lookup |

Shell execution is the **4th most common** operation but the **most powerful** —
it subsumes file operations, git, package management, and HTTP. An agent with
only a shell tool can do everything; an agent with only file tools cannot run
tests.

### The execution pattern

The core loop for every AI coding agent:

```
1. Reason about the task
2. Choose a tool (usually file read or shell)
3. Execute the tool
4. Observe the output
5. Reason about the result
6. Repeat until done
```

This is the ReAct (Reason + Act) pattern. The key observation: **tool calls are
single-shot with captured output.** Agents don't maintain persistent shell
sessions or background processes. Each command is:
- Spawn process
- Wait for completion
- Capture stdout + stderr
- Check exit code
- Return result to agent

### Claude Code's shell integration

Claude Code's Bash tool:
- Spawns `/bin/bash -lc "command"` (login shell, single command)
- Captures stdout and stderr separately
- Returns exit code
- Has approval modes: suggest (ask first), auto (read/write in workspace),
  full-auto (all operations allowed)
- Inherits the user's bash environment (PATH, aliases, etc.)

Codex CLI's shell tool:
- Similar pattern: `["/bin/bash", "-lc", "command"]` on Unix
- `["powershell.exe", "-Command", "command"]` on Windows
- Sandbox policies: workspace-read, workspace-write, danger-full-access
- Prefers dedicated tools over shell where available (e.g., `read_file` over `cat`)

### What agents need from shell

Distilled from observed patterns:

1. **Run command, get output** — the fundamental operation
2. **Check success/failure** — exit code → boolean
3. **Capture stdout** — the command's output (usually text)
4. **Capture stderr** — error messages for diagnosis
5. **Set working directory** — run command in a specific path
6. **Set environment variables** — configure tool behavior
7. **Timeout** — don't block forever on hung processes
8. **Pipe output** — chain 2-3 commands (rare but needed)

What agents do NOT need:
- Background processes / job control
- Interactive input (stdin)
- Signal handling
- Terminal emulation
- Shell scripting features (loops, conditionals in bash)
- Process groups or sessions

---

## 8. Synthesis: What ilo Needs

### The sh builtin

The highest-priority addition. Proposed:

```
sh "command"       -- R t t: Ok=stdout, Err=stderr (on non-zero exit)
sh! "command"      -- auto-unwrap: stdout on success, propagate err
```

Design decisions:

**Return type: `R t t`**
- Ok: stdout as text (trimmed trailing newline, like Ruby's `.chomp`)
- Err: stderr as text + exit code in message
- Matches `get url` pattern — same return type, same error handling
- Auto-unwrap `!` works identically: `out=sh! "ls -la"`

**Command as text, not as tokens**
- `sh "git status"` not `sh git status` — the command is opaque to ilo
- This avoids ilo needing to understand shell syntax
- Interpolation via string concatenation: `sh +"git log -n " (str n)`
- Or via a template if ilo adds `fmt`: `sh (fmt "git log -n {}" n)`

**Working directory**
- Option A: `sh "cd /tmp && ls"` — bash handles it (simplest)
- Option B: `sh "ls" cwd:"/tmp"` — named option on the builtin
- Option A is sufficient for agents and costs zero design tokens

**Environment variables**
- Option A: `sh "FOO=bar cmd"` — bash handles it (simplest)
- Option B: `sh "cmd" env:{"FOO":"bar"}` — structured option
- Option A is sufficient initially

**Timeout**
- Default timeout (e.g., 30s) to prevent hung processes
- Override: `sh "cmd" timeout:60` — or just use tool declaration pattern
- Essential for agent reliability — a hung process blocks the entire loop

**Token cost analysis:**
```
# Current workaround: no shell execution, use tool
tool run-cmd"Run shell command" cmd:t>R t t

# With sh builtin:
sh "git status"        -- 3 tokens
sh! "cargo test"       -- 3 tokens (auto-unwrap)
r=sh "ls";?r{~o:o;^e:^e}  -- explicit error handling
```

### The env builtin

Read environment variables:

```
env "HOME"         -- R t t: Ok=value, Err="not set"
env! "HOME"        -- auto-unwrap: value or propagate err
env "FOO"??"bar"   -- with default (if env returns nil for unset)
```

Alternative design (nil for unset, no Result):
```
env "HOME"         -- t or nil
env "HOME"??"bar"  -- with nil-coalesce default
```

The nil-returning version is simpler and more composable with `??`.
Nushell's `$env.VAR?` pattern validates this approach.

**Token cost:** `env "PATH"` is 2 tokens. Cheaper than `sh "echo $PATH"` (4 tokens)
and semantically cleaner.

### Structured output from shell

Most shell output is text, but agents often need structured data:

```
# Raw text (default)
out=sh! "ls -la"              -- text

# JSON parsing (common pattern)
d=sh! "cat package.json"      -- text
-- need json parsing tool to make it structured

# Line splitting (very common)
ls=spl (sh! "ls") "\n"        -- L t: list of filenames
```

ilo should NOT add JSON parsing as a language feature (manifesto: "format
parsing is a tool concern"). But `spl` (split) on newlines for line-based
output is already available and covers the most common case.

For structured shell output, the Nushell/PowerShell model suggests a future
direction: commands that return typed values instead of text. But this requires
a fundamentally different execution model — ilo functions as shell commands,
not external processes. This is a longer-term vision, not an immediate need.

### par-each / parallel iteration

Nushell's `par-each` validates the need. ilo's proposed designs:

**Option 1: `@!` parallel foreach (Nushell-aligned)**
```
rs=@! x urls{get x}    -- parallel map over list
```

**Option 2: `par{...}` block (Go/Swift-aligned)**
```
[a, b, c]=par{get url1;get url2;get url3}
```

**Option 3: Runtime auto-parallelization (graph-native)**
No syntax — the runtime detects independent calls and parallelizes them.

Previous research (python-stdlib-analysis.md, swift-kotlin-research.md,
go-stdlib-research.md) converged on: start with Option 2 (`par{...}`) for
explicit parallelism. Option 3 is the long-term goal but requires dependency
graph analysis.

**For shell execution specifically:**
```
-- Sequential: 3 API calls, blocking
a=sh! "curl -s api1";b=sh! "curl -s api2";c=sh! "curl -s api3"

-- Parallel: 3 API calls, concurrent
[a,b,c]=par{sh "curl -s api1";sh "curl -s api2";sh "curl -s api3"}

-- Parallel foreach: same command, different args
rs=@! u urls{sh +"curl -s " u}
```

### Process control

Agents need minimal process control:

| Operation | Syntax | Notes |
|-----------|--------|-------|
| Run + wait | `sh "cmd"` | Default, synchronous |
| Run + timeout | `sh "cmd" timeout:30` | Kill after N seconds |
| Run in background | Future: `bg "cmd"` | Returns handle |
| Kill process | Future: `kill handle` | By handle from bg |
| Check if running | Future: `alive handle` | Returns bool |

Background processes are low priority. The overwhelming pattern is
synchronous execution. If needed, `par{...}` covers the common case
(multiple concurrent operations with a join point).

### Permission model

ilo's verifier is the permission model. Deno's experience informs the design:

1. **sh is declared in the program** — the verifier knows shell is used
2. **sh cannot be hidden** — no way to sneak shell execution into a tool
3. **Runtime sandboxing is orthogonal** — OS-level containers, seccomp, etc.
4. **Audit logging** — every `sh` call logged with command + result
5. **Allowlist mode** — future: `sh "cmd" allow:["git","cargo"]` restricts
   which executables can be invoked

The Deno lesson: `--allow-run` breaks the sandbox. For ilo, `sh` is always
a powerful escape hatch. The mitigation is not to restrict it (agents need it)
but to make it visible and auditable.

---

## 9. Comparison Matrix

| Feature | Bash | Nushell | Fish | PowerShell | Bun Shell | Deno | ilo (proposed) |
|---------|------|---------|------|------------|-----------|------|----------------|
| Pipeline type | Text | Structured | Text | Objects | Text + parse | N/A | Typed values |
| Error model | Exit code + text | ShellError + span | Exit code | Exception objects | ShellError + exitCode | Error objects | `R ok err` |
| Type safety | None | Structural | None | .NET types | None (JS) | TypeScript | Verified at compile |
| Env vars | `$VAR` | `$env.VAR` | `$VAR` | `$env:VAR` | `.env({})` | `Deno.env` | `env "VAR"` |
| Parallel | `cmd &` + wait | `par-each` | `cmd &` | Jobs / Runspaces | Promise.all | Async | `par{...}` / `@!` |
| Permission model | None | None | None | Execution Policy | Safe escaping | `--allow-*` | Verifier + sandbox |
| Agent-friendly | Baseline | Better | Worse (interactive) | Verbose | Good (JS interop) | Good | Best (designed for) |
| Shell-in-process | No (fork) | Yes | No (fork) | Yes (.NET) | Yes (native) | No (Deno.run) | Yes (proposed) |

---

## 10. Implementation Priority for ilo

Ranked by impact on agent workflows:

### Tier 1: Essential (unblocks agent usage)

**1. `sh` builtin — shell command execution**
- `sh "cmd"` → `R t t` (stdout/stderr)
- `sh! "cmd"` → auto-unwrap
- Default timeout (30s)
- Feature-flagged like `get` (behind `shell` feature)
- Implementation: `std::process::Command` in Rust, synchronous
- Token cost: 3 tokens for a verified, error-handled shell call

**2. `env` builtin — environment variable access**
- `env "VAR"` → `t` or nil
- Compose with `??`: `env "HOME"??"/tmp"`
- Implementation: `std::env::var()` in Rust
- Token cost: 2 tokens

### Tier 2: High value (parallel execution)

**3. `par{...}` block — parallel execution**
- Execute all statements concurrently, collect results
- `[a,b,c]=par{sh "cmd1";sh "cmd2";sh "cmd3"}`
- Implementation: thread pool or tokio tasks
- Requires async runtime (already planned in G4)

**4. `@!` parallel foreach**
- `rs=@! x xs{sh +"curl " x}`
- Drop-in parallel replacement for `@`
- Nushell's `par-each` validates this exact pattern

### Tier 3: Nice to have (polish)

**5. Process timeout on sh**
- `sh "cmd" timeout:60`
- Kill process after N seconds
- Essential for reliability but can default initially

**6. Working directory on sh**
- `sh "cmd" cwd:"/path"`
- Alternative: `sh "cd /path && cmd"` (bash handles it)

**7. Structured shell output helpers**
- `spl (sh! "ls") "\n"` already works for line splitting
- JSON parsing is a tool concern, not a language concern

---

## 11. Design Tensions

### Opaque commands vs verified commands

`sh "git status"` is opaque — ilo cannot verify the command is valid.
This breaks the "closed world" principle. Options:

1. **Accept the break** — `sh` is the escape hatch. Tools are verified,
   shell commands are not. This is pragmatic and matches Deno's `--allow-run`.

2. **Typed command wrappers** — `git.status()` instead of `sh "git status"`.
   Maximum verification but enormous surface area. Not practical.

3. **Command allowlists** — `sh "cmd" allow:["git"]` restricts to known
   executables. Partial verification. Good for production, unnecessary for dev.

**Recommendation:** Option 1 for now. `sh` is explicitly unverified. The
program declares it uses `sh`. The verifier knows. The runtime audits.

### Token cost of safety

```
# Minimal (3 tokens, no error handling)
sh! "cargo test"

# With error handling (8 tokens)
r=sh "cargo test";?r{^e:^+"test failed: "e;~o:o}

# With timeout (5 tokens)
sh "cargo test" timeout:120
```

The `!` auto-unwrap makes the common case cheap. Explicit error handling
is available when needed. This matches ilo's existing pattern for `get`/`$`.

### Shell-in-process vs fork

Bun Shell proves shell-in-process works. For ilo:

- **Phase 1:** Fork `/bin/sh` or use `std::process::Command`. Simplest,
  most compatible, works everywhere.
- **Phase 2 (future):** Embed a shell parser for built-in commands (ls, echo,
  cat). Faster, no fork overhead, cross-platform.
- **Phase 3 (future):** Full pipeline execution in-process, like Nushell.
  Structured output from every stage.

Phase 1 is sufficient for agent use cases. Agents execute single commands
and capture output — they don't need in-process pipelines.

---

## 12. Token Cost Comparison

How many tokens to run a shell command and handle the result:

| Pattern | Python | Bash (raw) | ilo (proposed) |
|---------|--------|------------|----------------|
| Run command, get output | `subprocess.run(cmd, capture_output=True)` (6 tokens) | `output=$(cmd)` (3 tokens) | `sh! "cmd"` (3 tokens) |
| Check success | `if result.returncode == 0:` (5 tokens) | `if [ $? -eq 0 ]` (7 tokens) | `r=sh "cmd";?r{~o:use o;^e:handle e}` (10 tokens) or `sh! "cmd"` (3 tokens) |
| Run + parse JSON | `json.loads(subprocess.check_output(cmd))` (4 tokens) | `cmd \| jq '.field'` (4 tokens) | `d=sh! "cmd";parse-json d` (5 tokens, with tool) |
| Run + get lines | `result.stdout.splitlines()` (3 tokens) | `cmd \| while read line` (5 tokens) | `spl (sh! "cmd") "\n"` (5 tokens) |
| Parallel 3 commands | `asyncio.gather(...)` (~15 tokens) | `cmd1 & cmd2 & cmd3 & wait` (7 tokens) | `par{sh "a";sh "b";sh "c"}` (8 tokens) |
| Env var with default | `os.environ.get("K", "d")` (6 tokens) | `${K:-d}` (1 token) | `env "K"??"d"` (4 tokens) |

ilo is competitive with bash on token count and far ahead on safety/verification.
The `!` operator is the key — it makes error-handled execution as cheap as
fire-and-forget.

---

## 13. Open Questions

1. **Should `sh` be a keyword or a builtin?** Builtin is consistent with `get`,
   `len`, `str`, etc. Keyword would allow special parsing (e.g., `sh git status`
   without quotes). Recommendation: builtin with string arg.

2. **Should `sh` capture stderr separately?** Current proposal: Ok=stdout,
   Err=stderr. Alternative: Ok=record{out:t;err:t;code:n} for full access.
   The record approach is more powerful but costs more tokens to destructure.

3. **Should there be a `sh` variant that returns exit code as number?**
   e.g., `shc "cmd"` → `n` (just the code). Useful for `test -f file` patterns.
   Low priority — `sh "test -f file"` with Ok/Err is sufficient.

4. **How should `sh` handle binary output?** stdout can be binary (images,
   archives). Current `t` type loses data. Future: `B` (bytes) type.
   Low priority — agents rarely produce binary output from shell commands.

5. **Should ilo support stdin piping to `sh`?** `sh "grep foo" stdin:data`.
   Useful for text processing. Medium priority.

6. **What is the right default timeout?** 30s covers most commands. Test suites
   may need 300s+. No timeout risks hanging. Recommendation: 30s default,
   overridable.

---

## Sources

- [Nushell Parallelism](https://www.nushell.sh/book/parallelism.html)
- [Nushell Pipelines](https://www.nushell.sh/book/pipelines.html)
- [Nushell Environment](https://www.nushell.sh/book/environment.html)
- [Nushell External Command Execution (DeepWiki)](https://deepwiki.com/nushell/nushell/4.3-external-command-execution)
- [Bun Shell Documentation](https://bun.com/docs/runtime/shell)
- [Bun.$ API Reference](https://bun.com/reference/bun/$)
- [The Bun Shell Blog Post](https://bun.sh/blog/the-bun-shell)
- [Bun Joins Anthropic](https://bun.com/blog/bun-joins-anthropic)
- [Anthropic Acquires Bun](https://www.anthropic.com/news/anthropic-acquires-bun-as-claude-code-reaches-usd1b-milestone)
- [Deno Security and Permissions](https://docs.deno.com/runtime/fundamentals/security/)
- [Deno Permissions Manual](https://deno.land/manual/getting_started/permissions)
- [Deno 2.5 Permissions in Config](https://deno.com/blog/v2.5)
- [Fish Shell vs Bash vs Zsh Comparison 2026](https://www.bitdoze.com/fish-shell-vs-bash-vs-zsh/)
- [Bash vs Zsh vs Fish Linux Shell Comparison 2025](https://an4t.com/bash-vs-zsh-vs-fish-linux-shell-comparison/)
- [PowerShell Objects and Data Piping](https://www.varonis.com/blog/how-to-use-powershell-objects-and-data-piping)
- [Mastering the PowerShell Pipeline](https://shahin.page/article/mastering-the-powershell-pipeline-objects-parameter-binding-and-troubleshooting)
- [Claude Code Overview](https://code.claude.com/docs/en/overview)
- [Anthropic Claude Code System Prompts](https://github.com/Piebald-AI/claude-code-system-prompts)
- [OpenAI Codex CLI](https://github.com/openai/codex)
- [Codex CLI Features](https://developers.openai.com/codex/cli/features/)
- [Codex Shell Execution (DeepWiki)](https://deepwiki.com/openai/codex/5.2-model-provider-configuration)
- [AgentCgroup: OS Resources of AI Agents](https://arxiv.org/html/2602.09345v2)
- [Code Execution with MCP (Anthropic)](https://www.anthropic.com/engineering/code-execution-with-mcp)
- [AI Coding Tools: The Agentic CLI Era](https://thenewstack.io/ai-coding-tools-in-2025-welcome-to-the-agentic-cli-era/)
- [Nushell for DevOps (Medium)](https://medium.com/@denismarshalltumakov/nushell-for-devops-a-practical-guide-f8142434e778)
- [Nushell Structured Data (JDriven)](https://jdriven.com/blog/2025/10/nushell)
```

---

The document covers all six shell technologies, focuses on what AI coding agents actually use (grounded in Claude Code and Codex CLI patterns), and synthesizes concrete recommendations for ilo. Key findings:

1. **`sh` builtin is the highest-priority addition** -- 3 tokens for a verified, error-handled shell call. Returns `R t t` matching the existing `get` pattern. Auto-unwrap with `!` makes the common case cheap.

2. **`env` builtin for environment variables** -- 2 tokens, composable with `??` for defaults. Nushell's `$env.VAR?` pattern validates the nil-returning design.

3. **`par{...}` for parallel execution** -- Nushell's `par-each` validates this exact need. Start with explicit `par{}` blocks, evolve toward runtime auto-parallelization from the dependency graph.

4. **Deno validates ilo's closed-world verifier** -- but Deno's `--allow-run` lesson is important: `sh` is always an escape hatch that breaks static verification. The mitigation is visibility and auditability, not restriction.

5. **Bun Shell's acquisition by Anthropic** signals investment in JS-native shell execution. Its key insights (shell-in-process, automatic escaping, structured output formats) inform ilo's implementation direction.

6. **Agents use shell commands in single-shot, capture-output patterns** -- no background processes, no interactive input, no job control. ilo's `sh` can be simple: spawn, wait, capture stdout/stderr, return Result.