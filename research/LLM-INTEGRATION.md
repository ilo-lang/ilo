# LLM Integration Research

## Problem

AI agents built in ilo constantly need to call LLMs — to summarise, classify, generate, translate, reason. Currently the only way to do this is via a declared `tool` or an HTTP call to an API endpoint. Neither is ergonomic for the most common operation an AI agent does.

## Options

### Option A: Builtin `llm`

A first-class language builtin, same pattern as `get`/`env`:

```
llm model:t prompt:t > R t t
```

```
r=llm! "ant:haiku" "summarise this"
r=llm! "oai:gpt-4o" fmt "translate to %s: %s" lang text
```

**Pros:**
- Zero setup — works immediately, like `get url`
- Consistent with existing builtin style
- Verifier can type-check call sites

**Cons:**
- Hard-codes LLM as a concept in the runtime
- API keys must come from env vars (`env! "ANTHROPIC_API_KEY"`) — implicit
- Model strings are unverified (`"ant:typo"` fails at runtime, not verify time)
- Two runtimes to maintain (interpreter + VM path)

---

### Option B: Standard library `.ilo` file

A `llm.ilo` file (or `providers/anthropic.ilo` etc.) that users import:

```
use "llm/anthropic.ilo"   -- declares: haiku, sonnet, opus

r=haiku! "summarise this"
r=sonnet! fmt "translate to %s: %s" lang text
```

The library file contains `tool` declarations wired to HTTP endpoints:

```
-- anthropic.ilo
tool haiku"claude-haiku-4-5" prompt:t>R t t
tool sonnet"claude-sonnet-4-5" prompt:t>R t t
tool opus"claude-opus-4-5" prompt:t>R t t
```

Backed by an `HttpProvider` config or MCP server that handles auth.

**Pros:**
- No new language features — `use` + `tool` already exist
- Swappable: `use "llm/openai.ilo"` gives the same function names, different backend
- Verifier sees real function signatures
- Composable with the tool graph (`ilo tools --graph`)
- Provider-agnostic: same ilo code, different provider file

**Cons:**
- Requires setup (provider file + API key config)
- Slightly more boilerplate for the simplest case

---

### Option C: MCP server for LLMs

Wrap LLM providers as MCP servers. Use `--mcp llm.json` to auto-inject tool declarations.

```json
{
  "mcpServers": {
    "llm": {
      "command": "npx",
      "args": ["-y", "@ilo/llm-mcp-server"]
    }
  }
}
```

Then in ilo:
```
-- tools injected automatically via MCP discovery
r=haiku! "summarise this"
```

**Pros:**
- Reuses all existing MCP infrastructure
- Works with any MCP-compatible LLM wrapper (many already exist)
- No new language features
- Can evolve independently of ilo

**Cons:**
- Requires Node.js / external process
- More moving parts for simple use cases

---

### Option D: Hybrid — builtin for common case, lib for control

Simple builtin for 80% case, library for fine-grained control:

```
-- builtin: default model from ILO_LLM env var or config
llm! "summarise this"

-- builtin with model:
llm! "ant:haiku" "summarise this"

-- library for structured output, system prompts, temperature etc:
use "llm/anthropic.ilo"
r=haiku-json! schema prompt
```

---

## Model string conventions (if builtin)

| String | Provider | Model |
|---|---|---|
| `"ant:haiku"` | Anthropic | claude-haiku-4-5 |
| `"ant:sonnet"` | Anthropic | claude-sonnet-4-5 |
| `"ant:opus"` | Anthropic | claude-opus-4-5 |
| `"oai:gpt-4o"` | OpenAI | gpt-4o |
| `"oai:gpt-4o-mini"` | OpenAI | gpt-4o-mini |
| `"ggl:gemini-flash"` | Google | gemini-2.0-flash |

Provider prefix before `:`, model shortname after. Resolved at runtime.

---

## Advanced features (all options)

These come after the basic call works:

- **System prompt:** `llm model system prompt` — 3-arg form
- **Structured output:** `llm-json model schema prompt` — returns parsed record matching schema
- **Streaming:** `llm-stream model prompt` — returns stream handle (gates on G6)
- **Embeddings:** `embed model text` → `L n` (vector)
- **Multi-turn:** conversation state as a list of messages → `L t` or a record type

---

## Recommendation

**Start with Option B (standard library).** Reasons:

1. `use` + `tool` already work — this is buildable today with zero new language features
2. Forces good design: provider is explicit, swappable, version-controlled
3. The library can ship as part of ilo's standard distribution (`~/.ilo/lib/` or bundled)
4. If a builtin later proves necessary, it can be added without breaking library users

**Ship order:**
1. Write `lib/llm/anthropic.ilo` with `tool` declarations + HTTP provider config
2. Write `lib/llm/openai.ilo` same pattern
3. Document the pattern so users can add their own providers
4. Revisit builtin if the library approach proves too verbose for the common case

## Open questions

- Where does the library live? Bundled in the binary? Installed to `~/.ilo/lib/`? Git submodule?
- How does auth work? `env! "ANTHROPIC_API_KEY"` passed to HttpProvider config? Or a separate auth layer?
- Structured output: can ilo's type system express JSON schema well enough to verify LLM outputs at verify time?
- Should `use "llm/anthropic.ilo"` work out of the box, or does it require a separate install step?
