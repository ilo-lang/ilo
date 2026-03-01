# AI Coding Agent Mechanics: Tools, Protocols, and Sandboxing

A comparative analysis of the six major AI coding agents, focused on their
tool sets, file editing mechanisms, sandboxing models, OS interaction
patterns, and MCP integration. Written for the ilo-lang project to
understand what operations agents actually perform on codebases.

Research date: March 2026.

---

## Table of Contents

1. Claude Code (Anthropic)
2. Codex CLI (OpenAI)
3. Cursor
4. Kilo Code
5. VS Code Copilot (GitHub)
6. OpenCode
7. Cross-Agent Comparison Tables
8. Implications for ilo-lang

---

## 1. Claude Code (Anthropic)

Claude Code is a terminal-native CLI agent. It runs as an interactive
tool in the user's shell, communicating with Claude models via the
Anthropic API. Written in TypeScript/Node.js.

### 1.1 Complete Tool List (18 tools)

```
 #  Tool              Category         Purpose
 1  Bash              execution        Run shell commands
 2  BashOutput        execution        Get output from running bash processes
 3  KillShell         execution        Kill running shell processes
 4  Read              file-read        Read file contents (with line range support)
 5  Edit              file-write       Search-and-replace in files
 6  Write             file-write       Create new files or full rewrites
 7  Glob              search           File pattern matching (e.g., **/*.ts)
 8  Grep              search           Regex content search (ripgrep-based)
 9  NotebookEdit      file-write       Edit Jupyter notebook cells
10  WebFetch          web              Fetch URL + process with AI model
11  WebSearch         web              Search the web, return links
12  TodoWrite         planning         Create/manage structured task lists
13  Task              agent            Launch sub-agents for complex tasks
14  AskUserQuestion   interaction      Ask the user for clarification
15  Skill             extensibility    Execute learned skills / slash commands
16  SlashCommand      extensibility    Execute slash commands
17  EnterPlanMode     planning         Switch to planning mode
18  ExitPlanMode      planning         Switch back from planning mode
```

Sub-agents (launched via Task) get a subset: Bash, Glob, Grep, Read,
Edit, MultiEdit, Write, NotebookRead, NotebookEdit, WebFetch, TodoRead,
TodoWrite, WebSearch.

### 1.2 File Editing Model

Claude Code uses **exact string search-and-replace** for its Edit tool.
The model provides an `old_string` (text to find in the file) and a
`new_string` (replacement text). The edit fails if `old_string` is not
found or is not unique in the file. A `replace_all` flag can change
every occurrence.

The Write tool does **full file creation or overwrite**. It is intended
for new files; the system prompt enforces reading a file before writing
to it, preferring Edit for modifications.

Key design choices:
- Edit requires a prior Read of the file (enforced by the tool).
- `old_string` must be unique unless `replace_all` is true.
- The model must match exact indentation (tabs/spaces).
- No line-number-based editing; matching is purely by string content.

This is a **search-replace** model, not a diff/patch model. The model
never generates unified diffs or line numbers for edits.

### 1.3 Bash Execution Model

Bash commands run in a child process. Key properties:
- Working directory persists between commands.
- Shell state (variables, aliases) does NOT persist between calls.
- The shell environment initializes from the user's profile.
- Commands have a configurable timeout (default 120s, max 600s).
- Background execution is supported via `run_in_background`.
- No interactive mode support (no -i flags, no REPLs).

The system prompt instructs the model to prefer specialized tools over
shell equivalents: use Glob instead of `find`, Grep instead of `grep`,
Read instead of `cat`, Edit instead of `sed`.

### 1.4 Sandboxing Model

Claude Code uses **OS-level sandboxing** applied to the Bash tool:

**macOS (Seatbelt):**
- Uses Apple's `sandbox-exec` (Seatbelt framework).
- Enabled by default since v1.0.20.
- Seatbelt profiles define allowed filesystem and network access.

**Linux (Bubblewrap):**
- Uses bubblewrap (bwrap), the same tool used by Flatpak.
- Requires WSL2 for Windows (WSL1 not supported).
- Pre-generated seccomp BPF filters for x86-64 and ARM.

**Two isolation boundaries:**
1. **Filesystem:** Read follows deny-only (allowed everywhere, deny
   specific paths like ~/.ssh). Write follows allow-only (denied
   everywhere, explicitly allow paths like `.` and `/tmp`).
2. **Network:** Linux removes the network namespace entirely; all
   traffic must go through Unix domain socket proxies (via socat).
   macOS Seatbelt allows only specific localhost ports.

All child processes inherit sandbox restrictions. Running `npm install`
inside the sandbox means every postinstall script is also sandboxed.
This reduces permission prompts by 84% with <15ms latency overhead.

The sandbox is open-sourced as `@anthropic-ai/sandbox-runtime`.

### 1.5 MCP Integration

Claude Code supports MCP as a client. MCP servers extend the tool set
with external capabilities. Configuration happens via project-level
`.mcp.json` files or through the `/mcp` slash command. Claude Code
can also run AS an MCP server, exposing its tools to other clients.

### 1.6 Notable Patterns

- **Tool preference hierarchy:** Specialized tools over Bash. The
  system prompt explicitly says "use Grep, not grep."
- **Read-before-write enforcement:** Edit and Write tools fail if the
  file has not been Read first in the conversation.
- **Sub-agent isolation:** Task tool launches child agents with scoped
  tool access and independent context windows.
- **Web tools are split:** WebFetch (known URL -> content) vs.
  WebSearch (query -> links). WebSearch intentionally returns only
  titles and URLs, not page content.
- **TodoWrite as a planning primitive:** Used for structured task
  tracking with states (pending/in_progress/completed).

---

## 2. Codex CLI (OpenAI)

Codex CLI is a terminal-native coding agent from OpenAI. Written in
TypeScript, it uses the Responses API with GPT-5 family models. Its
execution model is a single-agent ReAct loop.

### 2.1 Complete Tool List

```
 #  Tool              Category         Purpose
 1  shell             execution        Run shell commands (default)
 2  apply_patch       file-write       Create/update/delete files via V4A diffs
 3  read_file         file-read        Read file contents
 4  update_plan       planning         Manage TODO/plan items
 5  web_search        web              Search the web (from OpenAI cache)
 6  exec_command      execution        Launch long-lived PTY sessions (experimental)
 7  write_stdin       execution        Feed input to exec_command sessions
 8  spawn_agent       agent            Multi-agent: launch child agent (experimental)
 9  send_input        agent            Multi-agent: send input to child agent
10  resume_agent      agent            Multi-agent: resume paused agent
11  wait              agent            Multi-agent: wait for agent completion
12  close_agent       agent            Multi-agent: terminate child agent
```

Plus MCP-provided tools from configured servers.

### 2.2 File Editing Model: apply_patch with V4A Diffs

Codex uses a **structured diff format called V4A** (Version 4A patches).
This is distinct from unified diffs or search-replace:

```
Operations:
- create_file: Create a new file with specified content
- update_file: Apply V4A diff to existing file
- delete_file: Remove a file at specified path
```

V4A diffs use **contextual anchors** to identify edit regions rather
than line numbers or exact string matches. The model has been heavily
trained on this format. OpenAI states: "We strongly recommend using our
exact apply_patch implementation as the model has been trained to excel
at this diff format."

Key properties:
- GPT-family models are trained specifically on V4A (not unified diff).
- The format supports file creation, updates, and deletion.
- Patches use context lines to anchor changes (similar to unified diff
  but with a custom format).
- Known edge case: parser does not correctly handle multiple
  `change_context` operations in a single patch (reported by Warp).

The system prompt instructs: "If a tool exists for an action, prefer
to use the tool instead of shell commands (e.g., read_file over cat)."

### 2.3 Bash Execution Model

Two shell tools:
- `shell`: Runs a command, returns output. On Windows, uses PowerShell.
  The prompt says "always fill in workdir; avoid using cd in the command
  string."
- `exec_command` (experimental): Launches a long-lived PTY for
  streaming output, REPLs, or interactive sessions. `write_stdin`
  sends additional input.

### 2.4 Sandboxing Model

Codex uses **OS-level sandboxing** with two mechanisms on Linux:

**Landlock (kernel 5.13+):**
- Capability-based filesystem restrictions.
- Configurable writable roots (workspace directory).
- Read-anywhere, write-only to whitelisted directories.

**seccomp-BPF:**
- System call filtering at the kernel level.
- Blocks network-related syscalls unless explicitly allowed.
- Granular control (e.g., allow `recvfrom` for local IPC, deny
  `connect`).

**macOS:** Uses Seatbelt (same framework as Claude Code).

**Alternative pipeline (opt-in):**
- `features.use_linux_sandbox_bwrap = true` enables Bubblewrap.
- Vendored bwrap compiled as part of the Linux build.
- In managed proxy mode, routes egress through proxy-only bridge.

**Network isolation:**
- Default `workspace-write` mode has NO network access.
- Must explicitly enable via config or flags.
- Cloud Codex uses two-phase runtime: setup phase has network
  (for `npm install` etc.), agent phase runs offline by default.

**Sandbox modes:**
- `read-only`: Browse files only, no changes.
- `workspace-write` (default): Read anywhere, write to workspace.
- `danger-full-access`: No restrictions (for isolated runners).

**ReadOnlyAccess policy (v0.100.0+):** Configurable policy for
granular read access control, restricting which directories Codex
can read from.

Debug tool: `codex debug landlock` shows applied rules and filters.

### 2.5 MCP Integration

Codex supports MCP via STDIO or streaming HTTP servers configured in
`~/.codex/config.toml`. Servers launch automatically at session start.
MCP tools appear alongside built-ins. Codex can also run AS an MCP
server. Managed via `codex mcp` CLI commands.

### 2.6 Notable Patterns

- **Minimal tool surface:** Only 2 core tools (shell + apply_patch)
  for the default configuration. The philosophy is that shell can do
  almost everything.
- **V4A is a training artifact:** The diff format exists because the
  model was trained on it, not because it is inherently superior. It
  is model-specific.
- **Parallel tool calling:** When enabled, the model batches multiple
  tool calls using `multi_tool_use.parallel`.
- **Git worktrees for isolation:** Multiple agents can work on the
  same repo in isolated worktrees.
- **ReAct loop bias:** The system prompt encodes "keep working until
  done" — read, edit, test, iterate.
- **Organization policy enforcement:** `requirements.toml` can lock
  down approval policies and sandbox modes across a team.

---

## 3. Cursor

Cursor is an IDE-integrated agent built as a VS Code fork. It uses a
proprietary two-model architecture with a fine-tuned Mixture-of-Experts
model (Composer) and a specialized apply model.

### 3.1 Tool List

```
 #  Tool               Category         Purpose
 1  codebase_search    search           Semantic search over indexed codebase
 2  grep_search        search           Exact keyword/pattern search
 3  read_file          file-read        Read file contents (250-750 line limit)
 4  list_dir           search           List directory structure
 5  edit_file          file-write       Suggest and apply edits
 6  delete_file        file-write       Delete files
 7  terminal_command   execution        Execute terminal commands
 8  web_search         web              Real-time web search
 9  recent_changes     context          Track recent file modifications
10  MCP tools          extensibility    External services via MCP
```

Agent mode is limited to 25 tool calls per session (extendable via
"Continue").

### 3.2 File Editing: Two-Model Architecture

Cursor's most distinctive feature is its **two-stage edit pipeline:**

**Stage 1: Planning (Frontier Model)**
A large model (Claude Sonnet, GPT-4o, or Composer) generates "lazy
diffs" — high-level descriptions of what should change. The model
may output search-and-replace blocks or partial file rewrites.

**Stage 2: Applying (Fast Apply Model)**
A fine-tuned 70B model applies the planned changes to the actual
file at ~1000 tokens/second using **speculative edits** (a variant
of speculative decoding). Rather than having the LLM generate diffs,
the apply model **rewrites the entire file** because:
- LLMs struggle with diff formats (rare in training data).
- Line number accuracy is poor across tokenizers.
- Full rewrite lets the model use more tokens for "thinking."
- Only Claude Opus could output accurate diffs consistently.

**Speculative Edits:**
Since most of the output will be identical to the existing code, a
deterministic algorithm speculates future tokens (unchanged code
lines), achieving up to 9x speedup over vanilla inference. This is
NOT standard draft-model speculative decoding — it exploits the
prior that edits are sparse within a file.

Cursor found that full file rewrites outperform aider-style diffs
for files under 400 lines.

### 3.3 Tool Protocol

Cursor uses **XML-based tool calling** in its system prompts. Tools
are invoked via XML tags like `<edit_file>`, `<target_file>`, and
`<code_edit>`. This is a deliberate choice:

- XML requires less "attention budget" from the model than JSON.
- JSON forces early commitment to field values, reducing flexibility.
- XML-based tool calls produce better coding results in Cursor's evals.

This contrasts with the industry trend toward native/JSON function
calling (which Roo Code adopted, reporting ~10% failure rates with XML).

### 3.4 Sandboxing Model

Cursor provides terminal command sandboxing via `sandbox.json` with
network and filesystem policies. Commands execute through the agent
with preserved history and native terminal integration. Cursor 2.0
supports up to 8 parallel agents, each in an isolated copy of the
codebase.

### 3.5 MCP Integration

Cursor supports MCP servers for external tool integration. MCP tools
are accessed via the Chat interface and can interact with databases,
APIs, and custom services.

### 3.6 Notable Patterns

- **Two-model architecture is unique:** No other agent separates
  planning and applying into distinct models.
- **Full-rewrite over diffs:** A data-driven decision that most LLMs
  cannot reliably generate diffs.
- **KV cache optimization:** Extensive caching, cache warming, and
  speculative caching (predicting what users will accept).
- **MoE for the agent model:** Composer uses Mixture-of-Experts,
  routing each token to specialized MLPs.
- **Tab completion model:** A separate, smaller model for real-time
  code completion (distinct from agent mode).
- **Agent harness per model:** Cursor tunes instructions and tools
  for each frontier model it supports.

---

## 4. Kilo Code

Kilo Code is an open-source VS Code extension (also supports JetBrains
and CLI) descended from the Cline/Roo Code lineage. It uses a mode-based
architecture with configurable tool access.

### 4.1 Complete Tool List

```
 #  Tool                   Category         Purpose
 1  read_file              file-read        Read file contents (with line ranges, PDF/DOCX support)
 2  write_to_file          file-write       Create new files or full overwrite
 3  apply_diff             file-write       Apply structured diffs to files
 4  replace_in_file        file-write       Search-and-replace in files
 5  execute_command         execution        Run terminal commands
 6  search_files           search           Regex search in files
 7  list_files             search           List directory contents
 8  codebase_search        search           Semantic search over codebase
 9  browser_action         browser          Browser automation (test web apps)
10  ask_followup_question  interaction      Ask user for clarification
11  attempt_completion     control-flow     Signal task completion
12  new_task               control-flow     Start a new task
13  switch_mode            control-flow     Switch between modes
14  run_slash_command       extensibility    Run slash commands
15  generate_image         generation       Generate images
16  MCP tools              extensibility    External services via MCP
```

### 4.2 File Editing Model

Kilo Code provides **three file editing mechanisms:**

1. **write_to_file:** Full file creation or complete overwrite. All
   changes require user approval via a diff view interface.
2. **apply_diff:** Structured diffs applied to existing files. Used in
   the common tool chain: `read_file -> apply_diff -> attempt_completion`.
3. **replace_in_file:** Search-and-replace for targeted edits (inherited
   from the Cline lineage).

This is the most flexible editing model of any agent — offering full
rewrite, structured diff, AND search-replace.

### 4.3 Mode-Based Tool Filtering

Kilo Code's defining feature is **modes that restrict tool access:**

- **Ask Mode:** Read-only tools and information gathering only.
- **Architect Mode:** Design-focused tools, documentation, limited
  execution rights.
- **Code Mode:** Full tool access for implementation.
- **Debug Mode:** Focused on issue identification and fixing.
- **Orchestrator Mode:** Decomposes tasks into subtasks, assigns
  specialized mode agents, coordinates execution.
- **Custom Modes:** User-defined tool subsets for specialized workflows.

### 4.4 Sandboxing

Kilo Code does not provide OS-level sandboxing. Security is
permission-based: every tool use requires explicit user approval.
The UI shows Save/Reject buttons and optional auto-approve toggles.
This is a **UX-level consent model**, not a security boundary.

### 4.5 MCP Integration

Kilo Code has deep MCP integration including a **MCP Server Marketplace**
— a built-in way to browse and install MCP servers for extending
capabilities. MCP tools integrate seamlessly with built-in tools in the
execution pipeline. The extension segments MCP contexts by operational
mode to minimize token consumption.

### 4.6 Notable Patterns

- **Cline/Roo lineage:** Inherits the XML tool calling protocol from
  Cline. (Roo Code has since migrated to native function calling.)
- **Browser automation as a first-class tool:** `browser_action`
  enables testing web applications directly.
- **attempt_completion as explicit control flow:** The agent must
  explicitly signal when it considers a task done.
- **Three editing strategies:** Offers the model a choice between
  full rewrite, diff, and search-replace — more options than any
  other agent.
- **Orchestrator for multi-agent:** A meta-mode that plans and
  delegates rather than executing directly.

---

## 5. VS Code Copilot (GitHub)

GitHub Copilot's agent mode runs inside VS Code, using the editor's
infrastructure for file access, terminal, and problem detection.

### 5.1 Built-in Tool List

```
 #  Tool               Category         Purpose
 1  editFiles          file-write       Apply code edits
 2  codebase           search           Search workspace (semantic + keyword + file name)
 3  search             search           Workspace text search
 4  problems           context          Read compiler/lint errors from editor
 5  changes            context          Read source control changes
 6  usages             context          Find symbol usages/references
 7  runInTerminal      execution        Run commands in integrated terminal
 8  terminalLastCommand context         Get last terminal command output
 9  fetch              web              Fetch web content
10  githubRepo         context          Access GitHub repository information
11  MCP tools          extensibility    External services via MCP
```

Tool sets group related tools: a "reader" set includes `changes`,
`codebase`, `problems`, and `usages`.

### 5.2 File Editing Model

Copilot agent mode generates **proposed edits** that are applied through
VS Code's native editor APIs. The model generates changes, VS Code
presents them as diffs, and the user can accept or reject. The system
detects compile and lint errors after edits and auto-corrects in a loop.

VS Code supports multiple edit formats depending on the model:
- OpenAI models (GPT-4.1, o4-mini): **apply_patch** format (V4A diffs).
- Anthropic models (Claude Sonnet): **replace_string** tool.

This multi-format support is a consequence of VS Code being
model-agnostic — it adapts its edit protocol to match the model's
trained format.

### 5.3 Sandboxing

VS Code Copilot does not provide OS-level sandboxing. Security is
through the VS Code permission model:
- Terminal commands require user approval.
- File edits are presented in a diff view for review.
- Rich undo capabilities for reverting changes.
- `autoFix` setting controls automatic error correction.

The GitHub Copilot Coding Agent (cloud-based) runs in isolated
containers on GitHub's infrastructure, providing stronger isolation
for asynchronous tasks.

### 5.4 MCP Integration

VS Code has comprehensive MCP support (GA as of mid-2025):
- Configuration via `.mcp.json` files in the project tree.
- Admin governance via enterprise policy and access controls.
- OAuth 2.0 authentication for remote servers.
- Fully qualified tool names (`search/codebase`) to avoid conflicts.
- Max 128 tools enabled per chat request.
- **Limitation:** Only MCP tools are exposed to agents (not resources
  or prompts from the MCP spec).

### 5.5 Notable Patterns

- **Editor-native tools:** Unlike terminal agents, Copilot's tools
  are wired directly into VS Code APIs (problems list, symbol
  references, source control). This gives it information that
  terminal agents must reconstruct via shell commands.
- **Model-adaptive edit format:** Supports both V4A (OpenAI) and
  replace_string (Anthropic) depending on the model.
- **Multi-source codebase search:** Combines semantic search, keyword
  search, filename search, git-modified files, and workspace symbols.
- **Cross-agent compatibility:** Agent files in `.claude/agents`
  work in both VS Code and Claude Code, with tool name mapping.
- **Background agents:** CLI-based agents using git worktrees for
  isolation from the main workspace.
- **Custom agents via .agent.md files:** Declarative agent definitions
  with YAML frontmatter specifying tool access and behavior.

---

## 6. OpenCode

OpenCode is a Go-based terminal agent built by the SST (Serverless
Stack) team. Uses Bubble Tea for TUI. MIT-licensed, 100K+ GitHub stars.
Supports 75+ model providers.

### 6.1 Complete Tool List

```
 #  Tool              Category         Purpose
 1  read              file-read        Read file contents (one or more files)
 2  write             file-write       Create files or apply patches
 3  edit              file-write       Search-and-replace (old_string/new_string)
 4  patch             file-write       Apply patch files/diffs
 5  multiedit         file-write       Multiple edits in a single call
 6  grep              search           Regex content search (ripgrep-based)
 7  glob              search           File pattern matching
 8  list (ls)         search           List files/directories with metadata
 9  bash              execution        Execute shell commands
10  lsp               code-intel       LSP operations (experimental)
11  subagent/task     agent            Delegate to specialized subagent
12  skill             extensibility    Load skill files (SKILL.md)
13  MCP tools         extensibility    External services via MCP
```

### 6.2 File Editing Model

OpenCode provides **four editing mechanisms:**

1. **edit:** Exact search-and-replace with `old_string` / `new_string`.
   Identical to Claude Code's Edit tool. Known issues with formatting
   conflicts — when OpenCode auto-formats after edit, the model's
   subsequent edits fail because it expects unformatted content.
2. **write:** Full file creation or overwrite, can also apply patches.
3. **patch:** Apply external patch files to the codebase.
4. **multiedit:** Batch multiple edits in a single tool call.

The search-and-replace model is the primary editing mechanism, but the
availability of `patch` and `write` gives the model fallback options.

### 6.3 Sandboxing: None (by default)

**OpenCode does NOT sandbox the agent.** Its permission system is purely
a UX feature — it prompts for confirmation before executing commands
or writing files, but this is not enforced at the OS level. A
compromised or malicious prompt can bypass it.

Known security issues:
- No OS-level filesystem restrictions.
- No network isolation.
- A vulnerability was identified where malicious websites could
  execute commands via XSS in the web UI.
- The team acknowledged they have "done a poor job handling security
  reports."

**Third-party mitigations:**
- `opencode-sandbox` plugin: Uses `@anthropic-ai/sandbox-runtime`
  (the same library Claude Code uses) to wrap bash commands with
  Seatbelt/Bubblewrap restrictions.
- Docker sandboxes: Docker provides guides for running OpenCode in
  isolated containers.

### 6.4 Search Tools

OpenCode's search tools use **ripgrep under the hood** for grep, glob,
and list operations. Ripgrep respects `.gitignore` patterns by default.
A `.ignore` file can explicitly include paths that would normally be
ignored.

### 6.5 LSP Integration (Experimental)

The LSP tool is unique among terminal agents. When enabled
(`OPENCODE_EXPERIMENTAL_LSP_TOOL=true`), it provides:
- goToDefinition, findReferences, hover
- documentSymbol, workspaceSymbol
- goToImplementation
- prepareCallHierarchy, incomingCalls, outgoingCalls

This gives the agent access to **type-aware code intelligence** that
other terminal agents (Claude Code, Codex CLI) must approximate
through grep/glob searches or shell commands.

### 6.6 MCP Integration

OpenCode supports MCP with both local (STDIO) and remote (HTTP)
servers. Configuration in `opencode.json`. Supports OAuth 2.0 for
remote servers with authorization code flow + PKCE and dynamic client
registration. MCP tools appear alongside built-ins. Custom tools can
override built-in tools by using the same name.

### 6.7 Notable Patterns

- **Model freedom as core value:** 75+ model providers, switch
  mid-session without losing context.
- **LSP tool is a differentiator:** No other terminal agent provides
  direct LSP operations as a tool.
- **SQLite for persistence:** Session data stored locally in SQLite.
- **Plugin architecture:** "Actions" and "skills" teach the agent
  domain-specific tasks.
- **No OS sandboxing is a real gap:** The team is aware but has not
  shipped a solution; third-party plugins fill the void.
- **Client-server architecture:** TUI frontend is separated from
  backend (LLM communication, tool execution, session management).

---

## 7. Cross-Agent Comparison Tables

### 7.1 Tool Category Matrix

```
Category          Claude  Codex   Cursor  Kilo    Copilot  OpenCode
                  Code    CLI                     (VSCode)
─────────────────────────────────────────────────────────────────────
File Read         Read    read    read    read    codebase read
                          _file   _file   _file   /search
File Write        Edit    apply   edit    write   editFiles edit
  (primary)       +Write  _patch  _file   _to_f            +write
  (mechanism)     S&R     V4A     2-model S&R/    V4A or   S&R
                          diff    rewrite diff/   S&R
                                          S&R
Shell exec        Bash    shell   term    exec    runIn    bash
                                  _cmd    _cmd    Terminal
Glob/pattern      Glob    (shell) (no)    list    (no)     glob
                                          _files
Grep/search       Grep    (shell) grep    search  search   grep
                                  _search _files  /cbase
Semantic search   (no)    (no)    code    code    codebase (no)
                                  base_s  base_s
Web fetch         Web     web     web     (MCP)   fetch    (MCP)
                  Fetch   _search _search
Web search        Web     web     web     (MCP)   (no)     (MCP)
                  Search  _search _search
Browser           (no)    (no)    (no)    browser (no)     (no)
                                          _action
LSP               (no)    (no)    (no)    (no)    usages/  lsp
                                                  problems (exp.)
Planning          Todo    update  (no)    new     (no)     (no)
                  Write   _plan           _task
Sub-agents        Task    spawn   (8 par  orches  back     subagent
                          _agent  allel)  trator  ground
Notebooks         Note    (no)    (no)    (no)    (no)     (no)
                  bookE
User interaction  AskUser (no)    (no)    ask     (no)     (no)
                  Quest           followup
MCP support       Yes     Yes     Yes     Yes+    Yes      Yes
                                          Mktplc
```

### 7.2 File Editing Mechanisms

```
Agent         Primary Method       Format              Speed
────────────────────────────────────────────────────────────────
Claude Code   Search & Replace     old_str/new_str     Model speed
Codex CLI     V4A Patch            Context-anchored    Model speed
                                   diff format
Cursor        Two-model rewrite    Full file rewrite   ~1000 tok/s
                                   via apply model     (speculative)
Kilo Code     S&R + Diff + Write   Multiple formats    Model speed
Copilot       Model-adaptive       V4A (OpenAI) or     Model speed
                                   S&R (Anthropic)
OpenCode      Search & Replace     old_str/new_str     Model speed
              + Patch fallback     + unified diff
```

### 7.3 Sandboxing Comparison

```
Agent         OS Sandbox    Filesystem         Network        Default
────────────────────────────────────────────────────────────────────
Claude Code   Seatbelt/     Deny-read list,    Proxy-only     ON
              Bubblewrap    Allow-write list    (no namespace)
Codex CLI     Landlock/     Read-anywhere,     Blocked by     ON
              seccomp/      Write-workspace    seccomp unless
              (opt: bwrap)                     enabled
Cursor        sandbox.json  Configurable       Configurable   Limited
Kilo Code     None          Approval UX only   No restriction OFF
Copilot       None (local)  Editor permissions No restriction OFF
              Containers    Full isolation     Full isolation ON
              (cloud)                                         (cloud)
OpenCode      None          Approval UX only   No restriction OFF
              (3rd-party    (plugin: seatbelt/ (plugin adds
              plugin avail) bwrap)             restrictions)
```

### 7.4 MCP Integration Depth

```
Agent         Client  Server  Marketplace  Auth     Governance
────────────────────────────────────────────────────────────────
Claude Code   Yes     Yes     No           Basic    Project-level
Codex CLI     Yes     Yes     No           Config   Org-level
Cursor        Yes     No      No           Config   Per-project
Kilo Code     Yes     No      YES          Config   Mode-based
Copilot       Yes     No      No           OAuth    Enterprise
OpenCode      Yes     No*     No           OAuth    Permission-based

* OpenCode as MCP server is proposed but not yet implemented.
```

---

## 8. Implications for ilo-lang

### 8.1 The Universal Tool Primitives

Every agent, regardless of architecture, implements these operations:

```
1. READ     - Read file contents
2. WRITE    - Create/overwrite file
3. EDIT     - Modify existing file (search-replace or diff)
4. SEARCH   - Find files by name/pattern (glob)
5. GREP     - Find content within files (regex)
6. EXEC     - Run a shell command
7. FETCH    - Get content from a URL
```

These seven operations are the **irreducible core** that every coding
agent needs. They map directly to what an ilo program needs to be able
to express when orchestrating tool use.

### 8.2 The Search-Replace Edit Model Dominates

Four of six agents (Claude Code, Kilo Code, OpenCode, and Copilot
with Anthropic models) use **exact string search-and-replace** as
their primary editing mechanism. This is the simplest model:
- No line numbers to track.
- No diff format to learn.
- Matches are unambiguous (must be unique in file).
- The model only generates the changed content.

Codex's V4A patch format is model-specific (GPT-family only).
Cursor's full-rewrite approach requires a second specialized model.

For ilo's tool declarations, **search-replace is the format to
optimize for** — it is the most common, simplest, and most portable
across models.

### 8.3 Shell Execution is Universal but Overloaded

Every agent has a "run shell command" tool, but agents differ in
whether they rely on it as a general-purpose fallback:

- **Codex CLI:** Shell is a primary tool. The agent uses it for
  everything the other tools do not cover.
- **Claude Code:** Shell exists but the system prompt actively
  discourages it in favor of specialized tools ("use Grep, not grep").
- **OpenCode:** Similar to Claude Code — specialized tools preferred.

The implication: ilo should model shell execution as a distinct tool
type, but provide enough built-in operations (file I/O, search, HTTP)
that agents do not need to fall back to shell for common tasks.

### 8.4 Sandboxing Patterns

Agents that sandbox do it at the OS level with the same two primitives:
1. **Filesystem allow/deny lists** (path-based)
2. **Network namespace/proxy isolation** (block all, allow specific)

This suggests ilo's tool execution model should be **sandboxable by
default** — tools should declare what filesystem and network access
they require, enabling a runtime to enforce restrictions.

### 8.5 The Semantic Search Gap

Terminal agents (Claude Code, Codex CLI, OpenCode) lack semantic
search — they rely on grep and glob. IDE agents (Cursor, Kilo Code,
Copilot) have it because they embed the codebase in a vector index.

OpenCode's experimental LSP tool is an interesting middle ground:
instead of semantic search, it provides **type-aware navigation**
(go-to-definition, find-references, call-hierarchy).

For ilo, the graph-native principle already addresses this: if program
structure is explicit (declared edges, queryable dependencies), agents
do not need semantic search or LSP — the graph IS the index.

### 8.6 Planning as a Tool

Three agents have explicit planning tools:
- Claude Code: TodoWrite (structured task lists with states)
- Codex CLI: update_plan (TODO management)
- Kilo Code: Orchestrator mode (meta-agent that plans and delegates)

Planning is not a file operation — it is a **coordination primitive**.
For ilo programs that orchestrate multi-step workflows, this suggests
a need for structured state tracking as a built-in concept, not just
a tool.

### 8.7 Sub-Agent Delegation

Four of six agents support sub-agent delegation:
- Claude Code: Task tool
- Codex CLI: spawn_agent / send_input / resume_agent / wait / close_agent
- Kilo Code: Orchestrator mode + new_task
- OpenCode: subagent/task

Sub-agents get scoped tool access and isolated context windows. This
is the primary mechanism for handling complex tasks that exceed a
single context window. The pattern is: decompose into subtasks, run
each in isolation, aggregate results.

For ilo, this maps to the **graph-native composition** principle:
programs are subgraphs that can be executed independently. The
language already supports this structurally — the runtime just needs
to support parallel/isolated execution of subgraphs.

### 8.8 What ilo Needs as Built-in Operations

Based on universal tool patterns across all six agents:

```
Category        ilo operation     Maps to agent tool
──────────────────────────────────────────────────────
File I/O        read              Read / read_file
                write             Write / write_to_file
                edit (S&R)        Edit / replace_in_file
Search          find (glob)       Glob / list_files
                grep (regex)      Grep / search_files
Execution       exec              Bash / shell / execute_command
HTTP            get               WebFetch / fetch
                (post, etc.)      (tool declarations)
Graph query     deps / calls      (ilo-native, no agent equivalent)
                impacts
Error handling  R ok err          (already in ilo)
Planning state  (track/status)    TodoWrite / update_plan
```

The ilo-native graph operations (deps, calls, impacts) have NO
equivalent in any agent's built-in tools. These are what makes ilo
structurally distinct — agents currently reconstruct this information
through repeated grep/search operations.

### 8.9 Token Costs of Tool Calls

Every tool call costs tokens for:
1. The tool name and parameters (prompt tokens).
2. The tool output (response tokens added to context).
3. The model's reasoning about what tool to use next.

ilo's token-minimal design should extend to tool declarations. The
`tool` syntax (`tool get-user"desc" uid:t>R profile t`) is already
more compact than any agent's tool definition format. The key insight
is that agents spend significant tokens on tool selection and output
parsing — ilo's constrained vocabulary and typed returns eliminate
much of this overhead.

---

## Sources

### Claude Code
- Anthropic Engineering: Claude Code Sandboxing
- Claude Code System Prompts (Piebald-AI)
- Claude Code Built-in Tools Reference
- sandbox-runtime (GitHub)

### Codex CLI
- Codex CLI Documentation (developers.openai.com)
- Codex Sandboxing Documentation
- Apply Patch / V4A format (OpenAI API)
- Codex Prompting Guide
- Codex CLI (GitHub)

### Cursor
- How Cursor Built Fast Apply (Fireworks AI)
- How Cursor Shipped its Coding Agent (ByteByteGo)
- Cursor Agent Tools (Community Forum)
- Cursor Instant Apply (Bind AI)

### Kilo Code
- Kilo Code (GitHub)
- Kilo Code Tool Use Overview
- DeepWiki: Kilo Code Architecture

### VS Code Copilot
- GitHub Copilot Agent Mode Announcement
- Agent Mode in VS Code Documentation
- Tools with Agents (VS Code)
- Custom Agents via .agent.md files

### OpenCode
- OpenCode Documentation
- OpenCode Tools
- OpenCode MCP Servers
- DeepWiki: OpenCode File System Tools
- Docker Sandboxes for OpenCode

---

## See Also

- [agent-framework-tool-mechanics.md](agent-framework-tool-mechanics.md) — deep dive into tool-calling protocols and JSON Schema patterns across agent frameworks
- [mcp-protocol-research.md](mcp-protocol-research.md) — MCP protocol mechanics that agents use for tool discovery
