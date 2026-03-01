# Agent Framework Tool-Calling Mechanics: A Comparative Analysis

Research for ilo-lang — understanding universal patterns across all major agent
frameworks to identify what a token-minimal agent language must support.

---

## Table of Contents

1. [Anthropic Claude (Messages API + Agent SDK)](#1-anthropic-claude)
2. [OpenAI (Responses API + Agents SDK)](#2-openai)
3. [MCP (Model Context Protocol)](#3-mcp-model-context-protocol)
4. [LangChain / LangGraph](#4-langchain--langgraph)
5. [CrewAI](#5-crewai)
6. [AutoGen (Microsoft)](#6-autogen-microsoft)
7. [Vercel AI SDK](#7-vercel-ai-sdk)
8. [Google A2A Protocol](#8-google-a2a-protocol)
9. [Universal Patterns](#9-universal-patterns)
10. [Implications for ilo-lang](#10-implications-for-ilo-lang)

---

## 1. Anthropic Claude

### Tool Definition (Messages API)

Tools are defined per-request via a `tools` array. Each tool uses JSON Schema for
its `input_schema`:

```json
{
  "model": "claude-opus-4-6",
  "max_tokens": 1024,
  "tools": [
    {
      "name": "get_weather",
      "description": "Get current weather in a given location",
      "input_schema": {
        "type": "object",
        "properties": {
          "location": {
            "type": "string",
            "description": "City and state, e.g. San Francisco, CA"
          }
        },
        "required": ["location"]
      }
    }
  ],
  "messages": [
    { "role": "user", "content": "What is the weather in SF?" }
  ]
}
```

Key field: `input_schema` (not `parameters` — Anthropic's naming diverges from
OpenAI). Strict mode adds `"strict": true` at the tool level and requires
`"additionalProperties": false` in the schema.

### Tool Call (Model Output)

Claude emits a `tool_use` content block inside the assistant message:

```json
{
  "role": "assistant",
  "content": [
    {
      "type": "tool_use",
      "id": "toolu_01D7FLrfh4GYq7yT1ULFeyMV",
      "name": "get_weather",
      "input": { "location": "San Francisco, CA" }
    }
  ]
}
```

Fields: `type` (always `"tool_use"`), `id` (Anthropic-generated, prefix `toolu_`),
`name`, `input` (parsed JSON object, not a string).

### Tool Result (Return to Model)

Results are sent as a user message with `tool_result` content blocks:

```json
{
  "role": "user",
  "content": [
    {
      "type": "tool_result",
      "tool_use_id": "toolu_01D7FLrfh4GYq7yT1ULFeyMV",
      "content": "72°F, sunny"
    }
  ]
}
```

The `content` field accepts either a plain string or an array of typed content
blocks (`text`, `image`). Error results set `"is_error": true`:

```json
{
  "type": "tool_result",
  "tool_use_id": "toolu_01D7FLrfh4GYq7yT1ULFeyMV",
  "is_error": true,
  "content": "Error: Location 'Atlantis' not found."
}
```

### Parallel Tool Use

Claude can emit multiple `tool_use` blocks in a single assistant message. All
corresponding `tool_result` blocks must appear in the subsequent user message.
The matching is by `tool_use_id`, not by order.

### Agent SDK Sandboxing

The Claude Agent SDK (the engine behind Claude Code) uses OS-level sandboxing:

- **Filesystem**: bubblewrap (Linux) / sandbox-exec (macOS). Default: read
  entire filesystem, write only to working directory.
- **Network**: removed network namespace (Linux) / Seatbelt profiles (macOS),
  traffic routed through built-in proxy.
- **Permission modes**: `default`, `accept_edits`, `plan`, `bypass_permissions`.
- **Evaluation order**: PreToolUse Hook -> Deny Rules -> Allow Rules -> Ask Rules
  -> Permission Mode -> canUseTool Callback -> PostToolUse Hook.
- **Static analysis**: bash commands are analyzed pre-execution; risky operations
  (system files, sensitive directories) require explicit approval.

The Agent SDK reduced permission prompts by 84% through sandboxing.

### Key Schema Properties

| Property | Value |
|---|---|
| Schema location | `input_schema` (top-level tool field) |
| Schema format | JSON Schema draft-compatible |
| Arguments encoding | Parsed JSON object (not stringified) |
| Call ID format | `toolu_` prefix + alphanumeric |
| Result channel | `tool_result` in user message |
| Error signaling | `is_error: true` flag |
| Parallel calls | Multiple `tool_use` blocks in one message |

---

## 2. OpenAI

### Tool Definition (Responses API)

Tools are defined with `type: "function"` and use `parameters` (JSON Schema):

```json
{
  "tools": [
    {
      "type": "function",
      "name": "get_weather",
      "description": "Get current temperature for a given location.",
      "parameters": {
        "type": "object",
        "properties": {
          "location": {
            "type": "string",
            "description": "City and country"
          }
        },
        "required": ["location"],
        "additionalProperties": false
      },
      "strict": true
    }
  ]
}
```

Key differences from Anthropic: field is `parameters` (not `input_schema`),
tools have an explicit `type: "function"` wrapper, and `strict: true` is
recommended by default.

### Tool Call (Model Output — Responses API)

The Responses API returns `function_call` output items:

```json
{
  "type": "function_call",
  "id": "fc_12345xyz",
  "call_id": "call_12345xyz",
  "name": "get_weather",
  "arguments": "{\"location\": \"Paris, France\"}",
  "status": "completed"
}
```

Critical difference: `arguments` is a **JSON string** (not a parsed object).
The caller must `JSON.parse()` / `json.loads()` it. This is a legacy design
from the Chat Completions API that persists in the Responses API.

### Tool Call (Model Output — Chat Completions API, legacy)

```json
{
  "role": "assistant",
  "tool_calls": [
    {
      "id": "call_abc123",
      "type": "function",
      "function": {
        "name": "get_weather",
        "arguments": "{\"location\": \"Paris\"}"
      }
    }
  ]
}
```

Note the nested `function` object — another layer of wrapping.

### Tool Result (Return to Model — Responses API)

```json
{
  "type": "function_call_output",
  "call_id": "call_12345xyz",
  "output": "72°F, sunny"
}
```

### Tool Result (Return to Model — Chat Completions API, legacy)

```json
{
  "role": "tool",
  "tool_call_id": "call_abc123",
  "content": "72°F, sunny"
}
```

### Parallel Tool Calls

Enabled by default. Disable with `"parallel_tool_calls": false` for sequential
execution (zero or one tool per turn).

### Built-in Tools (Responses API)

The Responses API has first-party tools that execute server-side:
- `web_search` — web search
- `file_search` — RAG over uploaded files
- `code_interpreter` — sandboxed Python execution
- `image_generation` — DALL-E
- `computer_use` — screen interaction
- Remote MCP servers (via `mcp` tool type)

### Agents SDK Tools

The OpenAI Agents SDK (Python) wraps function tools with type inference:

```python
from agents import Agent, function_tool

@function_tool
def get_weather(city: str) -> str:
    """Get weather for a city."""
    return f"72°F in {city}"

agent = Agent(
    name="Weather bot",
    instructions="You help with weather.",
    tools=[get_weather],
)
```

Schema is auto-generated from type hints. The SDK also supports:
- `codex_tool` — wraps Codex CLI for workspace-scoped tasks
- MCP server integration
- `is_enabled` parameter for runtime tool filtering

### Codex CLI Sandboxing

- **Sandbox modes**: workspace-write (default), read-only, full-access, YOLO
- **Default**: no network access, write limited to working directory
- **OS enforcement**: Linux namespace isolation, Windows sandbox
- **Config**: `~/.codex/config.toml` and `.codex/config.toml` (project-scoped)
- **Writable expansion**: `--add-dir` flag for multi-project coordination

### Key Schema Properties

| Property | Value |
|---|---|
| Schema location | `parameters` (under tool definition) |
| Schema format | JSON Schema |
| Arguments encoding | **JSON string** (must be parsed) |
| Call ID format | `call_` prefix (Chat) / `fc_` prefix (Responses) |
| Result channel | `function_call_output` (Responses) / `role: "tool"` (Chat) |
| Error signaling | Return error string in output (no dedicated flag) |
| Parallel calls | Default on, `parallel_tool_calls: false` to disable |

---

## 3. MCP (Model Context Protocol)

MCP is the vertical protocol (agent-to-tools), complementing A2A's horizontal
protocol (agent-to-agent). Both Anthropic and OpenAI support MCP natively.

### Tool Definition

Tools are defined on MCP servers and discovered via `tools/list`:

```json
{
  "name": "read_file",
  "description": "Read contents of a file at the given path.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "path": {
        "type": "string",
        "description": "Absolute path to the file"
      }
    },
    "required": ["path"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "content": { "type": "string" },
      "size": { "type": "integer" }
    }
  }
}
```

Key naming: `inputSchema` (camelCase — differs from both Anthropic's
`input_schema` and OpenAI's `parameters`). Optional `outputSchema` for
structured result validation.

### Tool Invocation (`tools/call`)

MCP uses JSON-RPC 2.0 for all communication:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "read_file",
    "arguments": {
      "path": "/home/user/data.txt"
    }
  }
}
```

### Tool Result

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "File contents here..."
      }
    ],
    "structuredContent": {
      "content": "File contents here...",
      "size": 1234
    },
    "isError": false
  }
}
```

Dual-track output: `content` (unstructured, for display/LLM consumption) +
`structuredContent` (typed, validated against `outputSchema`). For backward
compatibility, structured results SHOULD also include serialized JSON in a
`TextContent` block within `content`.

### Tool Naming Rules

- 1-128 characters, case-sensitive
- Allowed: `[A-Za-z0-9_\-.]`
- No spaces, commas, or special characters
- Must be unique within a server

### Transport

JSON-RPC 2.0 over stdio (local) or HTTP+SSE (remote). Schema is defined in
TypeScript first, published as JSON Schema for interop.

### Key Schema Properties

| Property | Value |
|---|---|
| Schema location | `inputSchema` (camelCase) |
| Output schema | `outputSchema` (optional) |
| Schema format | JSON Schema |
| Arguments encoding | Parsed JSON object |
| Transport | JSON-RPC 2.0 (stdio or HTTP+SSE) |
| Result format | Dual: `content` (display) + `structuredContent` (typed) |
| Error signaling | `isError: true` in result |
| Discovery | `tools/list` method |

---

## 4. LangChain / LangGraph

### Tool Definition — `@tool` Decorator

```python
from langchain_core.tools import tool

@tool
def search_database(query: str, limit: int = 10) -> str:
    """Search the customer database for records matching the query.

    Args:
        query: Search terms to look for
        limit: Maximum number of results to return
    """
    return f"Found {limit} results for '{query}'"
```

Schema is auto-generated from type hints + docstring. The decorator produces a
`StructuredTool` with a JSON Schema `args_schema`.

### Tool Definition — Pydantic Schema

```python
from langchain_core.tools import tool
from pydantic import BaseModel, Field

class SearchInput(BaseModel):
    query: str = Field(description="Search terms")
    limit: int = Field(default=10, ge=1, le=100, description="Max results")

@tool(args_schema=SearchInput)
def search_database(query: str, limit: int = 10) -> str:
    """Search the customer database."""
    return f"Found {limit} results for '{query}'"
```

### Tool Definition — BaseTool Subclass

```python
from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field
from typing import Type

class SearchInput(BaseModel):
    query: str = Field(description="Search terms")

class SearchTool(BaseTool):
    name: str = "search_database"
    description: str = "Search the customer database"
    args_schema: Type[BaseModel] = SearchInput

    def _run(self, query: str) -> str:
        return f"Results for '{query}'"
```

### Binding Tools to Models

```python
from langchain_openai import ChatOpenAI

llm = ChatOpenAI(model="gpt-4o")
llm_with_tools = llm.bind_tools([search_database, other_tool])
```

`bind_tools` converts tool schemas to the provider's native format automatically
(OpenAI format, Anthropic format, etc.). This is the key abstraction —
LangChain is a **schema translator** layer.

### Tool Call Message Format

```python
result = llm_with_tools.invoke([("user", "Find users named Alice")])
result.tool_calls
# [
#   {
#     "name": "search_database",
#     "args": {"query": "Alice"},
#     "id": "toolu_01Bfrz1Uhu84ggZd96Ae9De8"
#   }
# ]
```

The `.tool_calls` attribute is a normalized list of dicts with `name`, `args`
(parsed dict, not JSON string), and `id`. This normalization is consistent
regardless of which LLM provider is used.

### Model-Specific Direct Binding

```python
model_with_tools = model.bind(
    tools=[{
        "type": "function",
        "function": {
            "name": "multiply",
            "description": "Multiply two integers.",
            "parameters": {
                "type": "object",
                "properties": {
                    "a": {"type": "number"},
                    "b": {"type": "number"}
                },
                "required": ["a", "b"]
            }
        }
    }]
)
```

This bypasses LangChain's schema translation and uses OpenAI's native format
directly.

### LangGraph Agent Loop

```python
from langgraph.graph import StateGraph, END
from typing import TypedDict, Annotated, List
import operator

class AgentState(TypedDict):
    messages: Annotated[List, operator.add]

def call_model(state: AgentState):
    response = llm_with_tools.invoke(state["messages"])
    return {"messages": [response]}

def should_continue(state: AgentState):
    last = state["messages"][-1]
    if last.tool_calls:
        return "tools"
    return "end"

workflow = StateGraph(AgentState)
workflow.add_node("agent", call_model)
workflow.add_node("tools", tool_node)
workflow.set_entry_point("agent")
workflow.add_conditional_edges("agent", should_continue,
                               {"tools": "tools", "end": END})
workflow.add_edge("tools", "agent")
app = workflow.compile()
```

The graph is cyclic: `agent -> tools -> agent -> ...` until no tool calls remain.
This is the universal agent loop pattern.

### Key Schema Properties

| Property | Value |
|---|---|
| Schema source | Type hints + docstring / Pydantic BaseModel |
| Internal format | JSON Schema (via `args_schema`) |
| Wire format | Translated per-provider by `bind_tools` |
| Arguments encoding | Parsed dict (normalized from provider format) |
| Call ID | Passed through from provider |
| Agent loop | LangGraph StateGraph with cyclic edges |
| Multi-agent | Supervisor, peer-to-peer, sequential patterns |

---

## 5. CrewAI

### Tool Definition — `@tool` Decorator

```python
from crewai.tools import tool

@tool("web_search")
def web_search(query: str) -> str:
    """Search the web for information on the given query."""
    return f"Results for: {query}"
```

Function name becomes tool name (or override via decorator arg). Docstring
becomes description. Type hints define schema.

### Tool Definition — BaseTool Subclass

```python
from crewai.tools import BaseTool
from pydantic import BaseModel, Field
from typing import Type

class SearchInput(BaseModel):
    """Input schema for WebSearchTool."""
    query: str = Field(..., description="Search query string")
    max_results: int = Field(5, ge=1, le=50, description="Max results")

class WebSearchTool(BaseTool):
    name: str = "web_search"
    description: str = "Search the web for current information."
    args_schema: Type[BaseModel] = SearchInput

    def _run(self, query: str, max_results: int = 5) -> str:
        return f"Top {max_results} results for: {query}"
```

CrewAI's `BaseTool` mirrors LangChain's pattern (name, description,
args_schema, _run), but is independent — CrewAI is 100% decoupled from
LangChain.

### Agent + Task + Crew Assembly

```python
from crewai import Agent, Task, Crew

researcher = Agent(
    role="Researcher",
    goal="Find accurate information",
    backstory="An expert research analyst...",
    tools=[WebSearchTool()],
    llm="gpt-4o"
)

writer = Agent(
    role="Writer",
    goal="Write compelling content",
    backstory="A skilled technical writer..."
)

research_task = Task(
    description="Research the topic: {topic}",
    expected_output="A detailed summary of findings",
    agent=researcher
)

writing_task = Task(
    description="Write an article based on the research",
    expected_output="A polished article",
    agent=writer
)

crew = Crew(
    agents=[researcher, writer],
    tasks=[research_task, writing_task],
    process="sequential"  # or "hierarchical"
)

result = crew.kickoff(inputs={"topic": "AI agents"})
```

### Orchestration Modes

- **Sequential**: agents execute in order, each receiving prior agent's output
- **Hierarchical**: a manager agent delegates tasks dynamically
- **Flows**: event-driven pipelines with conditional branching, looping,
  parallelism — the backbone for production orchestration

### Tool Delegation

In hierarchical mode, the manager agent can delegate tool access. Agents only
have access to tools explicitly assigned to them. Tool results are always
strings (serialized by the tool).

### Key Schema Properties

| Property | Value |
|---|---|
| Schema source | Pydantic BaseModel / type hints |
| Schema format | JSON Schema (via Pydantic) |
| Tool results | Always strings |
| Tool assignment | Per-agent (explicit list) |
| Orchestration | Sequential, hierarchical, flows |
| Caching | Default on (key = tool name + input params) |
| Memory | Short-term, long-term, entity, contextual |

---

## 6. AutoGen (Microsoft)

### Tool Definition — FunctionTool (AutoGen 0.4+)

```python
from autogen_core.tools import FunctionTool
from typing_extensions import Annotated

async def get_stock_price(
    ticker: str,
    date: Annotated[str, "Date in YYYY/MM/DD"]
) -> float:
    import random
    return random.uniform(10, 200)

stock_tool = FunctionTool(
    get_stock_price,
    description="Fetch the stock price for a given ticker."
)
```

Schema is auto-generated from type hints. `Annotated` types provide field-level
descriptions (similar to Pydantic `Field(description=...)`).

The `strict` parameter enforces structured output mode (no default values
allowed, explicit args only).

### Tool Execution

```python
import json
from autogen_core import CancellationToken

# Direct execution
result = await stock_tool.run_json(
    {"ticker": "AAPL", "date": "2021/01/01"},
    CancellationToken()
)
print(stock_tool.return_value_as_string(result))
```

`run_json` takes a dict (parsed arguments) and returns the result. The
`return_value_as_string` method serializes results for LLM consumption.

### Tool Use with Model Client

```python
from autogen_core.models import UserMessage
from autogen_ext.models.openai import OpenAIChatCompletionClient

model_client = OpenAIChatCompletionClient(model="gpt-4o-mini")
user_msg = UserMessage(content="What is AAPL stock price?", source="user")

create_result = await model_client.create(
    messages=[user_msg],
    tools=[stock_tool],
    cancellation_token=CancellationToken()
)

# Parse the tool call
arguments = json.loads(create_result.content[0].arguments)
tool_result = await stock_tool.run_json(arguments, CancellationToken())
```

Note: `create_result.content[0].arguments` is a **JSON string** — the OpenAI
format leaks through.

### Legacy Tool Registration (AutoGen 0.2)

```python
import autogen

autogen.register_function(
    get_stock_price,
    caller=assistant,      # agent that suggests calls
    executor=user_proxy,   # agent that executes calls
    name="get_stock_price",
    description="Fetch stock price for a ticker."
)
```

The 0.2 API separated caller (LLM agent) from executor (runtime agent) — a
two-agent pattern where the assistant decides what to call and the user proxy
actually runs it.

### Agent-as-Tool (AgentTool)

```python
from autogen_agentchat.agents import AssistantAgent
from autogen_agentchat.tools import AgentTool

math_expert = AssistantAgent(
    "math_expert",
    model_client=model_client,
    system_message="You are a math expert."
)

math_tool = AgentTool(
    agent=math_expert,
    return_value_as_last_message=True
)

general_agent = AssistantAgent(
    "assistant",
    model_client=model_client,
    tools=[math_tool]
)
```

Agents can be wrapped as tools for other agents — the multi-agent equivalent
of function composition.

### Migration: AutoGen -> Microsoft Agent Framework

Microsoft Agent Framework is the successor. It merges AutoGen and Semantic
Kernel into a unified multi-language SDK. Same patterns, new namespace.

### Key Schema Properties

| Property | Value |
|---|---|
| Schema source | Type hints + `Annotated` descriptions |
| Schema format | JSON Schema (auto-generated) |
| Arguments encoding | JSON string (from model), parsed via `json.loads` |
| Execution | `run_json(dict, CancellationToken)` |
| Result serialization | `return_value_as_string(result)` |
| Agent-as-tool | `AgentTool` wrapper |
| Multi-agent | RoundRobinGroupChat, Swarm, custom topologies |

---

## 7. Vercel AI SDK

### Tool Definition (AI SDK 5+)

```typescript
import { z } from "zod";
import { tool, generateText } from "ai";

const weatherTool = tool({
  description: "Get weather for a location",
  inputSchema: z.object({
    location: z.string().describe("City name"),
  }),
  outputSchema: z.object({
    temperature: z.number(),
    condition: z.string(),
  }),
  execute: async ({ location }) => {
    return { temperature: 72, condition: "sunny" };
  },
});
```

AI SDK 5 renamed `parameters` to `inputSchema` and added `outputSchema` to
align with MCP naming conventions. The `execute` function is co-located with
the schema definition.

### Tool Definition (AI SDK 4, legacy)

```typescript
const weatherTool = tool({
  description: "Get weather for a location",
  parameters: z.object({
    location: z.string().describe("City name"),
  }),
  execute: async ({ location }) => {
    return { temperature: 72, condition: "sunny" };
  },
});
```

### Using Tools with generateText

```typescript
const result = await generateText({
  model: openai("gpt-4o"),
  tools: { weather: weatherTool },
  prompt: "What is the weather in London?",
});

// result.toolCalls — array of tool invocations
// result.toolResults — array of results
```

Tools are passed as a **named object** (not an array) — the key becomes the
tool name.

### Multi-Step Agent Loop

```typescript
const result = await generateText({
  model: openai("gpt-4o"),
  tools: { weather: weatherTool },
  prompt: "What is the weather in London?",
  stopWhen: stepCountIs(5),   // max 5 tool-call rounds
});
```

The SDK orchestrates the loop automatically: call model -> extract tool calls ->
execute tools -> append results -> call model again -> repeat until text
response or step limit.

### Human-in-the-Loop (AI SDK 6)

```typescript
const weatherTool = tool({
  description: "Get weather",
  inputSchema: z.object({ location: z.string() }),
  needsApproval: true,  // requires human approval before execution
  execute: async ({ location }) => { /* ... */ },
});
```

### Strict Mode (AI SDK 6)

Opt-in per tool — compatible tools use strict schema validation, others use
regular mode. Both can coexist in the same call.

### Key Schema Properties

| Property | Value |
|---|---|
| Schema source | Zod objects |
| Schema location | `inputSchema` (v5+) / `parameters` (v4) |
| Output schema | `outputSchema` (optional, v5+) |
| Schema format | Zod -> JSON Schema (auto-converted) |
| Tool namespace | Object keys (not array) |
| Execution | Co-located `execute` function |
| Agent loop | Built-in via `stopWhen` / step counting |
| Approval | `needsApproval: true` per tool |

---

## 8. Google A2A Protocol

A2A is the **horizontal** protocol (agent-to-agent), complementing MCP's
**vertical** protocol (agent-to-tools). It enables opaque agents from different
vendors to communicate without sharing internal state.

### Agent Card (Discovery)

Published at `/.well-known/agent.json`:

```json
{
  "name": "Recipe Agent",
  "description": "Finds and suggests recipes based on ingredients.",
  "url": "https://recipe-agent.example.com/a2a",
  "version": "1.0.0",
  "capabilities": {
    "streaming": true,
    "pushNotifications": false,
    "stateTransitionHistory": false
  },
  "skills": [
    {
      "id": "find-recipe",
      "name": "Find Recipe",
      "description": "Finds recipes matching given ingredients",
      "inputModes": ["text"],
      "outputModes": ["text"]
    }
  ],
  "defaultInputModes": ["text"],
  "defaultOutputModes": ["text"],
  "authentication": {
    "schemes": ["Bearer"]
  },
  "provider": {
    "organization": "Example Corp",
    "url": "https://example.com"
  }
}
```

Required fields: `name`, `url`, `version`, `capabilities`, `skills`.
All JSON field names use camelCase (protocol requirement).

### Request — `message/send`

A2A uses JSON-RPC 2.0 over HTTP(S):

```json
{
  "jsonrpc": "2.0",
  "id": "req-001",
  "method": "message/send",
  "params": {
    "message": {
      "role": "user",
      "parts": [
        {
          "kind": "text",
          "text": "Find me a pasta recipe with mushrooms"
        }
      ],
      "messageId": "msg-001"
    }
  }
}
```

First message omits `contextId` (server generates it). Subsequent messages
include both `contextId` and `taskId` for continuity.

### Response — Completed Task

```json
{
  "jsonrpc": "2.0",
  "id": "req-001",
  "result": {
    "kind": "task",
    "id": "task-uuid-001",
    "contextId": "ctx-uuid-001",
    "status": {
      "state": "completed"
    },
    "artifacts": [
      {
        "artifactId": "artifact-001",
        "name": "Recipe",
        "parts": [
          {
            "kind": "text",
            "text": "Mushroom Pasta: Sauté mushrooms..."
          }
        ]
      }
    ]
  }
}
```

### Response — Submitted (Async)

```json
{
  "jsonrpc": "2.0",
  "id": "req-002",
  "result": {
    "id": "task-uuid-002",
    "contextId": "ctx-uuid-002",
    "status": {
      "state": "submitted",
      "timestamp": "2025-03-15T11:00:00Z"
    }
  }
}
```

Task states: `submitted`, `working`, `input-required`, `completed`, `canceled`,
`rejected`, `failed`. Terminal states cannot be restarted.

### Streaming — `message/stream`

Requires `capabilities.streaming: true`. Response uses SSE
(`Content-Type: text/event-stream`). Each event is a JSON-RPC response fragment.

### Error Response

```json
{
  "jsonrpc": "2.0",
  "id": "req-001",
  "error": {
    "code": -32052,
    "message": "Validation error - Invalid request data"
  }
}
```

Standard JSON-RPC error codes plus A2A-specific codes.

### Key Architectural Differences from MCP

| Aspect | MCP (Vertical) | A2A (Horizontal) |
|---|---|---|
| Purpose | Agent-to-tool | Agent-to-agent |
| Participants | Client + server | Client agent + remote agent |
| State | Stateless tool calls | Stateful task lifecycle |
| Discovery | `tools/list` | `/.well-known/agent.json` |
| Output | Tool results | Artifacts (multi-part) |
| Transport | JSON-RPC (stdio/HTTP) | JSON-RPC (HTTP), gRPC (v0.3+) |
| Content | Text, structured data | Multi-modal parts (text, data, file) |

### Three-Layer Architecture

1. **Canonical Data Model** (`a2a.proto`): Core types — messages, tasks, agents
2. **Abstract Operations**: Capabilities independent of transport
3. **Protocol Bindings**: JSON-RPC, gRPC, HTTP/REST mappings

The proto file is the normative source. All SDKs and JSON schemas are generated
from it.

---

## 9. Universal Patterns

### Pattern 1: JSON Schema as the Universal Tool Schema

Every framework uses JSON Schema (or generates it) for tool parameter
definitions:

| Framework | Schema Field Name | Source |
|---|---|---|
| Anthropic | `input_schema` | Hand-written or Pydantic |
| OpenAI | `parameters` | Hand-written or Pydantic/Zod |
| MCP | `inputSchema` | Hand-written |
| LangChain | `args_schema` | Pydantic BaseModel |
| CrewAI | `args_schema` | Pydantic BaseModel |
| AutoGen | (auto-generated) | Type hints + Annotated |
| Vercel AI SDK | `inputSchema` | Zod objects |
| A2A | (skills, not tools) | Agent Card JSON |

**Convergence**: JSON Schema `type: "object"` with `properties`, `required`,
and optionally `additionalProperties: false`. The naming varies but the
structure is identical.

### Pattern 2: The Tool Call Lifecycle

Every framework follows the same 4-step cycle:

```
1. DEFINE  — Register tool schemas with the model
2. CALL    — Model emits tool call (name + arguments + ID)
3. EXECUTE — Runtime executes the tool with parsed arguments
4. RETURN  — Send result back to model with matching ID
```

This cycle repeats until the model produces a text response (no more tool
calls). The loop is the core agent pattern.

### Pattern 3: Call ID Matching

Every framework generates a unique ID per tool call and requires the result to
reference that ID:

| Framework | Call ID | Result Reference |
|---|---|---|
| Anthropic | `id` (in `tool_use`) | `tool_use_id` (in `tool_result`) |
| OpenAI | `call_id` / `id` | `call_id` (in `function_call_output`) |
| MCP | JSON-RPC `id` | JSON-RPC `id` in response |
| LangChain | `id` (in `tool_calls`) | Passed through from provider |
| A2A | `taskId` | `taskId` in subsequent messages |

### Pattern 4: Parallel Tool Execution

Most frameworks support multiple tool calls per turn:

- **Anthropic**: Multiple `tool_use` blocks in one assistant message
- **OpenAI**: Multiple `function_call` items; `parallel_tool_calls` toggle
- **LangChain**: Multiple entries in `tool_calls` list
- **Vercel**: Multiple tools resolved per step
- **MCP**: One `tools/call` per request (parallelism is client-side)
- **A2A**: One task per `message/send` (parallelism is client-side)

### Pattern 5: Error Signaling

| Framework | Error Mechanism |
|---|---|
| Anthropic | `is_error: true` flag on `tool_result` |
| OpenAI | Error string in `output` (no dedicated flag) |
| MCP | `isError: true` in result |
| LangChain | Exception handling in tool execution |
| CrewAI | Exception handling, tool returns error string |
| AutoGen | Exception in `run_json`, or error in return value |
| Vercel | Exception in `execute` function |
| A2A | JSON-RPC error object / task state `"failed"` |

### Pattern 6: Schema Generation from Code

Every Python/TS framework auto-generates JSON Schema from typed code:

| Source | Generator |
|---|---|
| Python type hints | `FunctionTool` (AutoGen), `@tool` (LangChain/CrewAI) |
| Pydantic BaseModel | LangChain `args_schema`, CrewAI `args_schema` |
| Python `Annotated` | AutoGen field descriptions |
| Zod objects | Vercel AI SDK `inputSchema` |
| TypeScript types | Vercel AI SDK (via Zod) |

### Pattern 7: Sandboxing Convergence

Both Anthropic and OpenAI converge on the same sandboxing model:

| Feature | Claude Agent SDK | Codex CLI |
|---|---|---|
| Default writes | Working directory only | Working directory only |
| Default reads | Entire filesystem | Entire filesystem |
| Network | Blocked by default | Blocked by default |
| OS mechanism | bubblewrap/sandbox-exec | Namespace isolation/Windows sandbox |
| Config levels | Rules + hooks + modes | TOML config + flags |
| Expansion | Permission rules | `--add-dir` flag |

### Pattern 8: Output Schema (Emerging)

Newer frameworks add output validation alongside input validation:

- **MCP**: `outputSchema` + dual `content`/`structuredContent` return
- **Vercel AI SDK 5+**: `outputSchema` (Zod)
- **OpenAI**: Structured Outputs (`strict: true`)
- **Anthropic**: Structured Outputs (`strict: true` on tools)

This trend toward bidirectional schema validation (input AND output) matches
ilo's `tool name"desc" params>return` pattern.

### Pattern 9: The Two Protocol Axes

```
                    Agent-to-Tool (Vertical)
                           MCP
                            |
                            |
Agent-to-Agent (Horizontal) +---- A2A Protocol
                            |
                            |
                    Both use JSON-RPC 2.0
                    Both use JSON Schema
                    Both support streaming
```

MCP and A2A are complementary. An agent uses MCP to access tools and A2A to
communicate with other agents. Both share JSON-RPC 2.0 transport and JSON
Schema for structure.

---

## 10. Implications for ilo-lang

### What ilo Must Support

Based on the universal patterns above, ilo needs to handle these mechanical
realities:

**1. JSON Schema is the universal tool interface.**

Every framework defines tools via JSON Schema. ilo's `tool` declarations must
map bidirectionally to JSON Schema:

```
tool weather"Get weather" location:s > WeatherResult
```

This must generate:
```json
{
  "name": "weather",
  "description": "Get weather",
  "inputSchema": {
    "type": "object",
    "properties": {
      "location": { "type": "string" }
    },
    "required": ["location"]
  }
}
```

And ilo must consume tool schemas discovered via MCP `tools/list` and convert
them back into ilo type signatures for verification.

**2. The call-ID lifecycle is universal.**

Every framework generates a call ID and expects results to reference it. ilo's
runtime must track call IDs transparently. The agent never needs to see or
generate IDs — the runtime manages them.

**3. Arguments are always a flat JSON object.**

Despite framework differences (parsed object vs JSON string), the arguments are
always a JSON object with string keys mapping to typed values. ilo's positional
args must serialize to `{"param1": value1, "param2": value2}` at the boundary.
This is a naming/ordering convention, not a type problem.

**4. Results are string-first, structured-second.**

Most frameworks serialize tool results to strings for LLM consumption. MCP's
dual-track (`content` + `structuredContent`) is the most sophisticated. ilo's
`R ok err` result type covers this: `~v` for the structured value,
`return_value_as_string()` equivalent for the text representation.

**5. Error signaling is a boolean + message.**

Whether it's `is_error: true` (Anthropic), a JSON-RPC error object (MCP/A2A),
or an exception (Python frameworks), errors are always: did it fail? + what
went wrong? ilo's `R ok err` with `^e` error path handles this natively.

**6. Parallel tool calls require batch semantics.**

When the model requests multiple tools simultaneously, the runtime must execute
all and return all results in one message. ilo's runtime should support this as
concurrent execution of independent tool calls.

**7. Sandbox defaults converge on: write cwd, read all, no network.**

Both major AI companies enforce the same default sandbox. ilo programs running
as agent tools should assume these constraints and declare required permissions
explicitly.

**8. Output schemas are becoming standard.**

The trend toward `outputSchema` / `structuredContent` means ilo's tool return
types are not just documentation — they're protocol-level contracts. The
`tool name"desc" params>return` syntax already captures this.

**9. Discovery is a schema exchange.**

MCP `tools/list` and A2A `/.well-known/agent.json` are both schema discovery
mechanisms. ilo's verifier can consume discovered schemas the same way it
consumes hand-written declarations — they're all JSON Schema at the wire level.

### What ilo Can Ignore

- **Framework-specific wrapper layers**: LangChain's `bind_tools`, CrewAI's
  `Crew`/`Flow`, AutoGen's `AgentTool` — these are orchestration concerns
  above the language level.
- **Multi-agent orchestration patterns**: Supervisor, hierarchical, round-robin
  — these are runtime topology decisions, not language features.
- **Provider-specific field naming**: `input_schema` vs `parameters` vs
  `inputSchema` — the runtime adapter handles naming translation.
- **Agent personas/roles**: CrewAI's `role`/`goal`/`backstory`, AutoGen's
  `system_message` — these are prompt-engineering concerns.

### The Minimum Viable Protocol Surface

For ilo to work with all frameworks, it needs exactly:

1. **Tool declaration** -> JSON Schema (bidirectional)
2. **Result type** -> success value + error (R ok err)
3. **Type mapping** -> ilo types <-> JSON Schema types
4. **Call semantics** -> positional args <-> named JSON object
5. **Discovery ingestion** -> MCP tools/list -> ilo function signatures

Everything else is runtime adapter code, not language design.

---

## See Also

- [coding-agents-research.md](coding-agents-research.md) — comparative analysis of agent tools, sandboxing, and OS interaction patterns
- [mcp-protocol-research.md](mcp-protocol-research.md) — MCP protocol specifics referenced in the discovery ingestion surface
