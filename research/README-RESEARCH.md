# README Research — ilo-lang competitive analysis

_What makes great READMEs for programming languages and AI agent tooling?_

---

## Part 1 — Programming language / package READMEs

### NumPy
**Hook:** Credibility-first. Badges (PyPI, Nature publication, security scores) establish institutional legitimacy immediately.
**Code:** None in the README itself — relies entirely on links to docs.
**Install:** No install instructions — relies on links.
**Standouts:**
- Inverted-pyramid structure — essentials before prose
- "Diverse contributor roles" section (reviewing, docs, translation) — humanises the project
- Institutional tone; feels like an org, not a project

**For ilo:** Add visual proof of quality (GitHub stars, release badge). The broad contributor roles framing could work for ilo too.

---

### Rust
**Hook:** Three pillars (Performance, Reliability, Productivity) with concrete applications ("critical services, embedded devices"), not abstract claims.
**Code:** Minimal/none in the README.
**Install:** Points to *The Book* rather than bloating the README.
**Standouts:**
- No badge clutter — logo variants signal polish instead
- "Building from source exists but isn't recommended" — honest, sets expectations
- Benefits-focused, not feature-focused

**For ilo:** Reframe "0.33x the tokens" into **benefit language**: "33% less context window per program, 22% fewer characters to generate, fewer retries." Connect to developer pain, not raw metrics.

---

### Roc
**Hook:** "Work in progress!" — immediate trust through honesty.
**Code:** No code examples.
**Install:** Action-first — links to installation, tutorial, docs, community chat.
**Standouts:**
- Naming sponsors (corporate + individuals) makes support tangible
- "Don't hesitate to ask" lowers barrier explicitly
- Concise — respects reader time

**For ilo:** Add a sponsors/contributors section naming anyone backing the project. Tone is welcoming but Roc shows how brevity + links beats long prose.

---

### Bun
**Hook:** Problem-first. `bun run index.tsx` *demonstrates* TypeScript/JSX support solving a known friction point.
**Code:** Four commands early — shows breadth immediately.
**Install:** Multi-path — Bash, npm, Docker, Homebrew; matches user preferences and platform requirements.
**Standouts:**
- "Problem-solving framing" — each example solves a named pain point
- Groups features by mental model: Runtime / Package Manager / Bundler
- Speed claims backed by benchmarks ("5x faster than Node")

**For ilo:** Best model for ilo's install section. Also: reframe the opening — "AI agents generate tokens. Every token costs. ilo cuts context window by 33%."

---

### Gleam
**Hook:** Mascot (Lucy) creates emotional connection before tech.
**Code:** None in README — links out to website.
**Install:** None in README.
**Standouts:**
- "Not owned by a corporation" + "💖" appeals to developers valuing independence
- Strategic badge placement — release + Discord = active development + live community
- Very brief — landing page rather than onboarding

**For ilo:** Emotional/brand approach is smart; ilo is more utilitarian. Instead emphasise agent autonomy and cost awareness.

---

### Cross-language patterns that work

| Pattern | Rating | Notes |
|---------|--------|-------|
| Value prop as outcome (Rust: "reliable software") | ★★★★★ | Connects to pain, not features |
| Code examples early (Bun) | ★★★★★ | Visual proof beats prose |
| Multiple install paths | ★★★★☆ | Match user's environment |
| Status transparency (Roc: "WIP!") | ★★★★☆ | Builds trust |
| Sponsor/contributor visibility | ★★★☆☆ | Human investment signal |
| Badge shields | ★★★☆☆ | Social proof; clutter if overdone |

---

## Part 2 — AI agent tooling READMEs

### Model Context Protocol (MCP) — Anthropic
**Hook:** "Specification foundation for agent interoperability." Index/gateway style — deliberately minimal, points to external Mintlify docs.
**Code:** None in the README.
**Structure:** Specification-first, minimal README. The spec is the product.
**Key pattern:** README as signpost, not documentation.

---

### Anthropic Python SDK
**Hook:** "Access Claude API from Python." Simple, functional.
**Code examples show:** token budgets, model selection, async usage, streaming.
**Standouts:**
- Emphasises async-first patterns
- API key management shown early
- No fluff — just "here's how to call the API"

---

### OpenAI Python SDK
**Hook:** "Convenient programmatic access."
**Code examples show:** request tracking IDs, webhook verification, retries, streaming.
**Standouts:**
- **Operational concerns up front** — cost, latency, safety before features
- Progressive disclosure: basic → advanced → operational
- Request ID tracking for debugging is unique to AI cost environments

---

### LangChain
**Hook:** "Agent engineering platform." Six benefits before any technical detail.
**Code:** Minimal in README — points to cookbook.
**Standouts:**
- Ecosystem narrative — names partners (LangGraph, LangSmith, 100+ integrations)
- Positions as orchestration layer, not monolithic solution
- Cross-links are core to the value proposition

---

### DSPy (Stanford)
**Hook:** "Programming, not prompting."
**Code:** Shows the philosophical difference — programs vs. hand-crafted prompts.
**Standouts:**
- **Academic citations** (9 papers) — unique to AI agent tooling
- Researcher attribution builds credibility
- Research-credibility is a selling point because practitioners want theoretical soundness

---

### Pydantic AI
**Hook:** "FastAPI feeling for GenAI."
**Code examples show:** type-safe agents, structured output validation, dependency injection.
**Standouts:**
- **"Built by the Pydantic team, trusted by OpenAI/Anthropic/Google"** — borrowed authority
- Full working "bank agent" example — guided scenario not just snippets
- Observability via Logfire mentioned early (operational concern)

---

### LlamaIndex
**Hook:** "Data framework for LLM apps."
**Install:** Dual strategy — `llama-index` (batteries-included) vs. `llama-index-core` + modules for production.
**Standouts:**
- Three-tier users: quick prototyper / modular builder / enterprise (LlamaParse)
- Positions as data layer, not agent layer

---

### AutoGen (Microsoft)
**Hook:** "Multi-agent framework."
**Structure:** Three explicit API tiers — Core → AgentChat → Extensions.
**Standouts:**
- **Layered abstraction named explicitly** — different tiers for different skill levels
- Max iteration limits, streaming configuration shown early (operational)
- No-code GUI alternative shown alongside SDK

---

## Part 3 — AI tooling vs. traditional README differences

### What AI agent tooling READMEs do that traditional software doesn't

| Pattern | Why it appears |
|---------|---------------|
| Token budget examples in getting-started code | Cost is a first-class concern in LLM apps |
| Model selection as central first parameter | LLM choice is like choosing a database |
| Request tracking/IDs shown early | Debugging + cost attribution |
| Streaming structured outputs as core feature | Latency UX is key for AI products |
| Dependency injection for agent context | Agents have persistent state |
| Academic paper citations | Theoretical soundness sells in ML |
| Multi-agent conversation flows as narrative examples | Agents compose; humans script |
| Specification-as-foundation positioning (MCP) | AI needs interop standards, not just libraries |
| "Trusted by OpenAI/Anthropic/Google" | Borrowed institutional authority |
| Three-tier API architecture explicitly named | Different user sophistication levels |

### Common TOC order across AI tooling

1. Logo / branding
2. One-sentence value statement
3. Badges (downloads, version, social, license)
4. Installation (one command)
5. Quick example (paste and run in 30 seconds)
6. Why / benefits
7. Feature showcase or guided scenario
8. Ecosystem / integrations
9. Links (docs, Discord, GitHub issues)
10. License

---

## Part 4 — How ilo compares

### What ilo does well

- **Python vs ilo comparison is the best opening code block** — concrete and immediate
- **Multiple install paths** — npm, curl, cargo, Claude Code plugin, Cowork. Matches Bun's approach.
- **Comprehensive** — covers every feature without relying on external docs
- **Honest about the design trade-off** — prefix notation prioritised for token minimality
- **Prefix vs infix benchmark** — rare for a README; quantified claim backed by research

### What ilo is missing

#### 1. Outcome-first opening
Current: _"ilo — Toki Pona for "tool". A programming language for AI agents."_
Better: Lead with the problem and the number: **"33% less context window. 22% fewer characters. Built for the token cost of AI generation."**

#### 2. Badges / social proof
No badges at all. Minimum recommended:
- `![version](https://img.shields.io/crates/v/ilo)` — latest release
- `![license](https://img.shields.io/badge/license-MIT-blue)` — trust signal
- `![tests](...)` — 2294 tests passing is a strong signal
- GitHub stars (once visible)

#### 3. Agent-aware framing missing from top of file
The README is human-centric — explains syntax, shows examples. It doesn't explicitly call out the MCP integration, the `ilo help ai` compact spec, or the tool protocol as being first-class. That's a big gap for AI agent tooling READMEs.

#### 4. "Why ilo over X" section
Every strong AI tooling README has a comparison or positioning statement. ilo should have:
- vs. Python snippets in system prompts (verbose, unverified, fragile)
- vs. JSON function schemas (no control flow, no composition)
- vs. raw LLM code generation (unverified output)

#### 5. Guided scenario (real-world example)
The pydantic AI "bank agent" example, or LangChain's cookbook pattern. ilo should show a short but complete real program — not just one-liners. Something like: read a CSV, filter, aggregate, write result — 3-5 ilo lines.

#### 6. Operational concerns absent
No mention of:
- Error output cost (compact error codes mean less context used for debugging)
- The verification step saving retries (a token cost argument)
- `.env` loading in the context of secret management

#### 7. Three-tier user split not explicit
ilo has three audiences but doesn't name them:
1. **Agent developers** — `ilo help ai`, system prompt loading
2. **ilo programmers** — syntax reference, examples, SPEC.md
3. **Integrators** — MCP, `--tools`, `ilo serv`, JSON output

---

## Part 5 — Structural recommendation

```
# ilo

[logo or ASCII art]

**33% less context window. AI-first language — verified before execution.**

[badges: version | license | tests | community]

## Quick start
[2-3 one-liners that work immediately]

## Why ilo
- Token cost: 0.33x Python in tokens, 0.22x in characters
- Verified: type errors caught before execution, not at runtime
- Compact errors: ILO-T004 not a stack trace — agents get precise feedback
- MCP-native: ilo programs compose as MCP tools

## What it looks like
[existing Python→ilo comparison — keep this, it's great]

## Install
[keep existing multi-path section]

## Running
[trim slightly — move advanced backends to bottom or SPEC.md]

## For AI agents
[`ilo help ai`, plugin install, context loading — currently buried mid-page]

## Documentation
[keep existing table]

## Community
[keep Reddit link + add Discord if/when created]
```

---

_Research conducted March 2026. Projects reviewed: NumPy, Rust, Roc, Bun, Gleam (languages/packages); MCP, Anthropic SDK, OpenAI SDK, LangChain, DSPy, Pydantic AI, LlamaIndex, AutoGen (AI agent tooling)._
