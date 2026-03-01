# Model Context Protocol (MCP) -- Technical Research

## Purpose

This document provides a mechanical reference for the Model Context Protocol (MCP), Anthropic's open standard for connecting AI agents to external tools, data sources, and services. The focus is on protocol internals: wire format, message flows, transport layers, primitives, and lifecycle. This research informs what ilo-lang needs to support natively for agent-tool interaction.

The authoritative specification lives at https://modelcontextprotocol.io/specification/. The current stable versions are 2025-06-18 and 2025-11-25 (which adds async tasks and extensions). This document covers the protocol as of 2025-11-25.

---

## 1. Architecture: Host, Client, Server

MCP uses a three-tier architecture:

```
  Host (AI application / IDE / agent runtime)
    |
    +-- Client (protocol handler, 1:1 with a server)
    |     |
    |     +-- Server (exposes tools, resources, prompts)
    |
    +-- Client
          |
          +-- Server
```

- **Host**: The AI-enabled application (chatbot, IDE, agent runtime). A host may create multiple clients.
- **Client**: Manages one MCP connection. Handles JSON-RPC framing, capability negotiation, and method routing. Lives inside the host process.
- **Server**: An independent process or service that exposes capabilities. Communicates with exactly one client per session.

A single host can connect to many servers simultaneously (one client per server). The host decides which server to route a tool call to based on the tool name returned during discovery.

**Implication for ilo**: An ilo runtime acting as an agent would be the Host. It would need a client component per MCP server it connects to. Tool calls in ilo programs would route through these clients.

---

## 2. Wire Format: JSON-RPC 2.0

All MCP communication uses JSON-RPC 2.0 over whatever transport is active. Three message types exist:

### 2.1 Requests

A request expects a response. It carries an `id` that the responder echoes back.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/list",
  "params": {}
}
```

Fields:
- `jsonrpc`: Always `"2.0"`.
- `id`: Unique per-session. String or integer. Used to match responses.
- `method`: The operation name (e.g., `"initialize"`, `"tools/call"`, `"sampling/createMessage"`).
- `params`: Optional object or array. Method-specific parameters.

### 2.2 Responses

A response matches a prior request by `id`. Contains exactly one of `result` or `error`.

Success:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "tools": [ ... ]
  }
}
```

Error:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32601,
    "message": "Method not found"
  }
}
```

Standard JSON-RPC error codes:
- `-32700` -- Parse error (invalid JSON)
- `-32600` -- Invalid request
- `-32601` -- Method not found
- `-32602` -- Invalid params
- `-32603` -- Internal error
- `-32042` -- URL elicitation required (MCP-specific)

### 2.3 Notifications

Fire-and-forget messages. No `id`, no response expected.

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/initialized"
}
```

Used for: lifecycle events (`notifications/initialized`), progress updates (`notifications/progress`), cancellations (`notifications/cancelled`), list-change signals (`notifications/tools/list_changed`), and logging.

### 2.4 Key Design Property

JSON-RPC 2.0 was chosen deliberately. It is the same foundation used by the Language Server Protocol (LSP). It is simple, human-readable, universally parseable, and avoids reinventing RPC primitives. The protocol is stateful -- each session maintains connection state from initialization through shutdown.

---

## 3. Transport Layers

MCP defines three transports. All carry JSON-RPC 2.0 messages; they differ only in how bytes move between client and server.

### 3.1 stdio (Standard Input/Output)

The client spawns the server as a subprocess. Communication happens over stdin/stdout.

```
Client ---stdin--->  Server process
Client <--stdout---  Server process
             stderr -> logging (optional, not protocol messages)
```

Rules:
- Messages are newline-delimited. MUST NOT contain embedded newlines.
- Server MUST NOT write non-MCP data to stdout.
- Client MUST NOT write non-MCP data to the server's stdin.
- Server MAY write UTF-8 logging to stderr. Client MAY capture or ignore it.

This is the standard transport for local tools: filesystem access, database queries, CLI wrappers. It is the simplest to implement -- no HTTP, no TLS, no session tokens.

**Implication for ilo**: An ilo runtime could spawn MCP servers as child processes and read/write JSON-RPC over pipes. This maps cleanly to a `spawn + read-line + write-line` pattern.

### 3.2 Streamable HTTP (Current Standard for Remote)

Replaces the old SSE transport as of protocol version 2025-03-26.

The server exposes a single HTTP endpoint (e.g., `https://example.com/mcp`). Communication:

- **Client to Server**: HTTP POST with JSON-RPC body.
- **Server to Client**: The response can be either:
  - A single JSON-RPC response (simple case), or
  - An SSE stream (for streaming results, progress, server-initiated requests).
- **Server to Client (unsolicited)**: Client opens an HTTP GET to the same endpoint. Server sends SSE events.

Session management:
- The server MAY assign a session ID via the `Mcp-Session-Id` header during initialization.
- The client MUST include this header on subsequent requests.
- Supports connection resumability: client sends `Last-Event-ID` header on reconnect; server MAY replay missed events.

This is the transport for remote/cloud MCP servers. A single endpoint handles everything -- no dual-endpoint complexity.

### 3.3 SSE (Server-Sent Events) -- Deprecated

The original remote transport (protocol version 2024-11-05). Required two endpoints:
- `/sse` -- Client opens GET, receives SSE stream from server.
- `/messages` -- Client POSTs JSON-RPC requests.

Deprecated because: dual endpoints are complex, connection drops lose in-flight responses, and there is no resumability. Streamable HTTP fixes all of these.

Still available for backward compatibility. Clients can auto-detect: POST an `initialize` request; if 4xx, fall back to GET for SSE.

### 3.4 Transport Selection Summary

| Transport       | Use Case          | Endpoints    | Status     |
|----------------|-------------------|-------------|------------|
| stdio          | Local processes    | stdin/stdout | Active     |
| Streamable HTTP| Remote/cloud       | Single URL   | Active     |
| SSE            | Legacy remote      | Dual URLs    | Deprecated |

---

## 4. Lifecycle: Initialization, Operation, Shutdown

MCP is stateful. Every session goes through three phases.

### 4.1 Phase 1: Initialization

The client sends an `initialize` request. This MUST be the first message and MUST NOT be batched with other requests.

**Client -> Server: initialize request**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2025-06-18",
    "capabilities": {
      "roots": { "listChanged": true },
      "sampling": {},
      "elicitation": {}
    },
    "clientInfo": {
      "name": "ilo-runtime",
      "version": "0.1.0"
    }
  }
}
```

**Server -> Client: initialize response**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2025-06-18",
    "capabilities": {
      "tools": { "listChanged": true },
      "resources": { "subscribe": true, "listChanged": true },
      "prompts": { "listChanged": true },
      "logging": {}
    },
    "serverInfo": {
      "name": "weather-server",
      "version": "2.0.0"
    },
    "instructions": "This server provides weather data for any city worldwide."
  }
}
```

**Client -> Server: initialized notification**
```json
{
  "jsonrpc": "2.0",
  "method": "notifications/initialized"
}
```

Protocol version negotiation:
- Client sends the latest version it supports.
- If the server supports that version, it echoes it back.
- If not, the server responds with a version it does support.
- If the client cannot work with the server's version, it disconnects.

Capability negotiation:
- Client declares what it can handle (sampling, roots, elicitation).
- Server declares what it offers (tools, resources, prompts, logging).
- Neither party may use features the other did not advertise.
- This creates a binding contract for the session.

**Client capabilities** (what the client can provide to the server):
- `roots` -- Client can expose filesystem roots. `listChanged`: will notify on changes.
- `sampling` -- Client can fulfill `sampling/createMessage` requests.
- `elicitation` -- Client can show forms/dialogs to the user on the server's behalf.

**Server capabilities** (what the server offers to the client):
- `tools` -- Server exposes callable tools. `listChanged`: will notify when tool list changes.
- `resources` -- Server exposes readable resources. `subscribe`: supports subscriptions. `listChanged`: will notify on changes.
- `prompts` -- Server exposes prompt templates. `listChanged`: will notify on changes.
- `logging` -- Server can send log messages.
- `completions` -- Server supports argument auto-completion.

Rules before initialization completes:
- Client SHOULD NOT send requests other than pings.
- Server SHOULD NOT send requests other than pings and logging.

### 4.2 Phase 2: Operation

Normal bidirectional communication. Both sides send requests, responses, and notifications according to the negotiated capabilities.

Key constraint: neither party may invoke features not agreed upon in initialization.

### 4.3 Phase 3: Shutdown

No explicit shutdown message. Shutdown is transport-level:
- **stdio**: Client closes stdin to the server. Waits for exit. Escalates to SIGTERM/SIGKILL if needed.
- **HTTP**: Client stops sending requests. Server may time out the session.

### 4.4 Utility Messages (Available in All Phases)

**Ping** (either direction):
```json
{
  "jsonrpc": "2.0",
  "id": 99,
  "method": "ping"
}
```
Response: `{ "jsonrpc": "2.0", "id": 99, "result": {} }`

**Progress notification** (either direction):
```json
{
  "jsonrpc": "2.0",
  "method": "notifications/progress",
  "params": {
    "progressToken": "abc-123",
    "progress": 50,
    "total": 100,
    "message": "Processing records..."
  }
}
```

**Cancellation notification** (either direction):
```json
{
  "jsonrpc": "2.0",
  "method": "notifications/cancelled",
  "params": {
    "requestId": 5,
    "reason": "User cancelled"
  }
}
```

Implementations SHOULD establish timeouts on all sent requests. When a timeout fires, send a cancellation notification and stop waiting.

---

## 5. The Six Primitives

MCP defines six primitives divided into two categories:

**Server-side** (server exposes, client consumes):
1. Tools -- callable actions with side effects
2. Resources -- read-only data
3. Prompts -- reusable message templates

**Client-side** (client exposes, server consumes):
4. Sampling -- server asks client's LLM for completions
5. Roots -- client tells server its filesystem boundaries
6. Elicitation -- server asks client to collect user input

### 5.1 Tools

Tools are the core agent interaction primitive. A tool represents an operation the server is willing to perform: query a database, call an API, write a file, run a calculation. Tools MAY have side effects (unlike resources).

Tools are **model-controlled**: the LLM discovers available tools and decides which to invoke based on user intent. However, a human-in-the-loop SHOULD be able to approve or deny any invocation.

#### Discovery: tools/list

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/list",
  "params": {}
}
```

Supports pagination via optional `cursor` param.

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "tools": [
      {
        "name": "get_weather",
        "title": "Get Weather",
        "description": "Get current weather for a city. Returns temperature in Celsius and conditions.",
        "inputSchema": {
          "type": "object",
          "properties": {
            "city": {
              "type": "string",
              "description": "City name, e.g. 'Paris' or 'Tokyo'"
            },
            "units": {
              "type": "string",
              "enum": ["celsius", "fahrenheit"],
              "description": "Temperature unit. Defaults to celsius."
            }
          },
          "required": ["city"]
        },
        "outputSchema": {
          "type": "object",
          "properties": {
            "temperature": { "type": "number" },
            "conditions": { "type": "string" },
            "humidity": { "type": "number" }
          },
          "required": ["temperature", "conditions"]
        },
        "annotations": {
          "readOnlyHint": true,
          "openWorldHint": true
        }
      },
      {
        "name": "delete_file",
        "title": "Delete File",
        "description": "Permanently delete a file at the given path.",
        "inputSchema": {
          "type": "object",
          "properties": {
            "path": {
              "type": "string",
              "description": "Absolute file path to delete"
            }
          },
          "required": ["path"]
        },
        "annotations": {
          "readOnlyHint": false,
          "destructiveHint": true,
          "idempotentHint": true,
          "openWorldHint": false
        }
      }
    ]
  }
}
```

Tool definition fields:
- `name`: Unique identifier. 1-128 chars. Allowed chars: `A-Z`, `a-z`, `0-9`, `_`, `-`, `.`. Case-sensitive.
- `title`: Optional human-readable display name.
- `description`: Natural language description. This is what the LLM reads to decide whether to use the tool.
- `inputSchema`: JSON Schema (draft 2020-12) defining expected arguments. Top-level type is always `object`.
- `outputSchema`: Optional JSON Schema for structured output validation.
- `annotations`: Behavioral hints (see below).

Tool annotations (all boolean, all advisory):
- `readOnlyHint` (default: false) -- Tool does not modify its environment.
- `destructiveHint` (default: true) -- Tool may destroy data. Only meaningful when readOnlyHint is false.
- `idempotentHint` (default: false) -- Repeated calls with same args have no additional effect. Only meaningful when readOnlyHint is false.
- `openWorldHint` (default: true) -- Tool interacts with external/unbounded entities (e.g., the web).

Annotations are hints only. They are not enforced. Clients MAY use them for UI treatment (e.g., requiring confirmation for destructive tools).

#### Invocation: tools/call

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "get_weather",
    "arguments": {
      "city": "Paris",
      "units": "celsius"
    }
  }
}
```

**Successful response:**
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "Current weather in Paris: 18C, partly cloudy, 65% humidity"
      }
    ],
    "structuredContent": {
      "temperature": 18,
      "conditions": "partly cloudy",
      "humidity": 65
    },
    "isError": false
  }
}
```

**Error response (tool execution failure):**
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "Error: City 'Atlantis' not found in weather database"
      }
    ],
    "isError": true
  }
}
```

Critical distinction: **protocol errors** vs **tool execution errors**:
- Protocol errors (invalid method, bad params) use the JSON-RPC `error` field. The LLM never sees these.
- Tool execution errors (API timeout, invalid input, business logic failure) use `isError: true` inside `result`. The LLM DOES see these and can reason about them, retry, or change strategy.

Content types in the `content` array:
- `TextContent`: `{ "type": "text", "text": "..." }`
- `ImageContent`: `{ "type": "image", "data": "<base64>", "mimeType": "image/png" }`
- `AudioContent`: `{ "type": "audio", "data": "<base64>", "mimeType": "audio/wav" }`
- `EmbeddedResource`: `{ "type": "resource", "resource": { "uri": "...", "text": "...", "mimeType": "..." } }`

The `structuredContent` field (introduced 2025-06-18) contains a JSON object matching the tool's `outputSchema`. When both `content` and `structuredContent` are present, they MUST be semantically equivalent. For backward compatibility, servers SHOULD include a `TextContent` block with serialized JSON alongside `structuredContent`.

#### Change notification

When available tools change at runtime:
```json
{
  "jsonrpc": "2.0",
  "method": "notifications/tools/list_changed"
}
```

Client should re-fetch with `tools/list`.

**Implication for ilo**: Tool definitions map directly to function signatures. `inputSchema` is a JSON Schema object -- ilo records (`type name{field:type;...}`) map 1:1 to this. `outputSchema` maps to the return type. The `name` field becomes the callable identifier. Tool annotations could inform ilo's verifier (e.g., flagging destructive calls for confirmation).

### 5.2 Resources

Resources represent read-only data that provides context to the LLM. Files, database schemas, API documentation, configuration. Unlike tools, resources have NO side effects.

Resources are **application-controlled** (the host/client decides which resources to fetch), not model-controlled like tools.

#### Discovery: resources/list

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "resources/list",
  "params": {}
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "result": {
    "resources": [
      {
        "uri": "file:///project/src/main.rs",
        "name": "Main source file",
        "description": "The application entry point",
        "mimeType": "text/x-rust"
      },
      {
        "uri": "db://production/schema",
        "name": "Database schema",
        "mimeType": "application/json"
      }
    ]
  }
}
```

Each resource has:
- `uri`: Unique identifier. Any valid URI (file://, http://, custom://).
- `name`: Human-readable label.
- `description`: Optional.
- `mimeType`: Optional content type hint.

#### URI Templates: resources/templates/list

For parameterized resources (RFC 6570 URI templates):

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "result": {
    "resourceTemplates": [
      {
        "uriTemplate": "logs://app/{date}/errors",
        "name": "Error logs by date",
        "description": "Application error logs for a specific date",
        "mimeType": "text/plain"
      }
    ]
  }
}
```

#### Reading: resources/read

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "method": "resources/read",
  "params": {
    "uri": "file:///project/src/main.rs"
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "result": {
    "contents": [
      {
        "uri": "file:///project/src/main.rs",
        "mimeType": "text/x-rust",
        "text": "fn main() {\n    println!(\"Hello\");\n}"
      }
    ]
  }
}
```

Content is either `text` (UTF-8) or `blob` (base64 binary). A single read MAY return multiple items (e.g., reading a directory returns its files).

Resources support subscriptions: client subscribes to a URI, server sends `notifications/resources/updated` when it changes.

### 5.3 Prompts

Prompts are reusable message templates exposed by the server. They are **user-controlled**: the user explicitly selects which prompt to use (e.g., from a menu or slash command). They are NOT invoked by the model autonomously.

#### Discovery: prompts/list

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 8,
  "result": {
    "prompts": [
      {
        "name": "code_review",
        "title": "Code Review",
        "description": "Review code for bugs, style issues, and improvements",
        "arguments": [
          {
            "name": "language",
            "description": "Programming language of the code",
            "required": true
          },
          {
            "name": "focus",
            "description": "What to focus on: bugs, style, performance, or all",
            "required": false
          }
        ]
      }
    ]
  }
}
```

#### Retrieval: prompts/get

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 9,
  "method": "prompts/get",
  "params": {
    "name": "code_review",
    "arguments": {
      "language": "rust",
      "focus": "bugs"
    }
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 9,
  "result": {
    "description": "Code review for Rust, focusing on bugs",
    "messages": [
      {
        "role": "user",
        "content": {
          "type": "text",
          "text": "Review the following Rust code for bugs. Focus on memory safety, error handling, and logic errors."
        }
      }
    ]
  }
}
```

The server fills in the `{{language}}` and `{{focus}}` placeholders and returns fully-formed messages ready to feed to the LLM. Messages can include embedded resources, images, or any content type.

### 5.4 Sampling (Client-side)

Sampling inverts the typical flow: the **server** asks the **client** to run an LLM completion. The client has access to a model; the server does not. This enables agentic patterns where a tool needs AI reasoning as part of its execution.

The client MUST have declared `sampling` capability during initialization.

#### Request: sampling/createMessage

**Server -> Client:**
```json
{
  "jsonrpc": "2.0",
  "id": 10,
  "method": "sampling/createMessage",
  "params": {
    "messages": [
      {
        "role": "user",
        "content": {
          "type": "text",
          "text": "Summarize these error logs and identify the root cause:\n\nERROR 2025-01-15 DB connection timeout\nERROR 2025-01-15 DB connection timeout\nWARN 2025-01-15 Pool exhausted, 0 available connections"
        }
      }
    ],
    "systemPrompt": "You are a production incident analyst. Be concise.",
    "maxTokens": 200,
    "includeContext": "thisServer",
    "modelPreferences": {
      "hints": [
        { "name": "claude-sonnet-4-20250514" }
      ],
      "costPriority": 0.8,
      "speedPriority": 0.5,
      "intelligencePriority": 0.3
    }
  }
}
```

Fields:
- `messages`: Conversation history to send to the LLM.
- `systemPrompt`: Optional. The client MAY modify or ignore it.
- `maxTokens`: Required. Maximum tokens for the completion.
- `includeContext`: `"none"`, `"thisServer"`, or `"allServers"`. Hints to the client about what additional context to include.
- `modelPreferences`: Optional hints. `hints` suggests model names; `costPriority`, `speedPriority`, `intelligencePriority` are 0-1 floats for tradeoff guidance.

**Client -> Server:**
```json
{
  "jsonrpc": "2.0",
  "id": 10,
  "result": {
    "role": "assistant",
    "content": {
      "type": "text",
      "text": "Root cause: database connection pool exhaustion. All connections timed out, likely due to a slow query or connection leak."
    },
    "model": "claude-sonnet-4-20250514",
    "stopReason": "endTurn"
  }
}
```

Human-in-the-loop: The client SHOULD show the user what the server wants to ask, let the user edit/approve/reject the prompt, then show the completion for approval before returning it to the server.

The client decides which model to use. The server's `modelPreferences` are advisory. API costs are borne by the client, not the server.

### 5.5 Roots (Client-side)

Roots define the boundaries of what the server should operate on. They are URIs (typically `file://` paths) representing the client's workspace scope.

The client MUST have declared `roots` capability during initialization.

#### Server requests roots: roots/list

**Server -> Client:**
```json
{
  "jsonrpc": "2.0",
  "id": 11,
  "method": "roots/list"
}
```

**Client -> Server:**
```json
{
  "jsonrpc": "2.0",
  "id": 11,
  "result": {
    "roots": [
      {
        "uri": "file:///home/user/projects/my-app",
        "name": "My Application"
      },
      {
        "uri": "file:///home/user/projects/shared-lib",
        "name": "Shared Library"
      }
    ]
  }
}
```

When roots change (user switches projects), the client sends:
```json
{
  "jsonrpc": "2.0",
  "method": "notifications/roots/list_changed"
}
```

The server then re-fetches roots with `roots/list`.

**Critical caveat**: Roots are informational/advisory. The MCP specification does NOT enforce that servers respect root boundaries. A server is not technically prevented from accessing files outside declared roots. It is a trust mechanism, not a security mechanism.

### 5.6 Elicitation (Client-side)

Elicitation allows the server to pause execution and ask the user a question via a structured form. Introduced in 2025-06-18.

The client MUST have declared `elicitation` capability during initialization.

#### Request: elicitation/create

**Server -> Client:**
```json
{
  "jsonrpc": "2.0",
  "id": 12,
  "method": "elicitation/create",
  "params": {
    "message": "Which database should we deploy the migration to?",
    "requestedSchema": {
      "type": "object",
      "properties": {
        "database": {
          "type": "string",
          "enum": ["staging", "production"],
          "description": "Target database environment"
        },
        "confirm_destructive": {
          "type": "boolean",
          "description": "Confirm that this migration includes destructive changes"
        }
      },
      "required": ["database", "confirm_destructive"]
    }
  }
}
```

The `requestedSchema` uses a restricted subset of JSON Schema -- flat objects only (no nesting). Supported property types:
- `string` -- with optional `minLength`, `maxLength`, `format` (`"email"`, `"uri"`, `"date"`, `"date-time"`), and `enum`.
- `number` / `integer` -- with optional `minimum`, `maximum`.
- `boolean`.

Enum variants can have titles via JSON Schema `oneOf` with `const`/`title` pairs.

**Client -> Server (user accepted):**
```json
{
  "jsonrpc": "2.0",
  "id": 12,
  "result": {
    "action": "accept",
    "content": {
      "database": "staging",
      "confirm_destructive": true
    }
  }
}
```

Three possible actions:
- `"accept"` -- User submitted the form. `content` contains the values.
- `"decline"` -- User explicitly rejected the request.
- `"cancel"` -- User dismissed without choosing (closed dialog, pressed Escape).

The 2025-11-25 spec adds **URL-mode elicitation**: instead of an in-client form, the server can send a URL (e.g., an OAuth page) for the user to complete in a browser. Error code `-32042` signals that URL elicitations must be completed before the original request can be retried.

---

## 6. Tool Discovery: The inputSchema in Detail

The `inputSchema` on every tool is the critical interface between the LLM and the external world. It tells the model exactly what arguments to provide.

### 6.1 Schema Format

Always JSON Schema (draft 2020-12 as of 2025-11-25). Top-level type is always `"object"`. Properties define individual parameters.

```json
{
  "type": "object",
  "properties": {
    "query": {
      "type": "string",
      "description": "SQL query to execute. Must be a SELECT statement.",
      "maxLength": 10000
    },
    "database": {
      "type": "string",
      "enum": ["users", "orders", "analytics"],
      "description": "Which database to query"
    },
    "limit": {
      "type": "integer",
      "description": "Maximum rows to return",
      "minimum": 1,
      "maximum": 1000
    },
    "include_headers": {
      "type": "boolean",
      "description": "Include column names in results"
    }
  },
  "required": ["query", "database"]
}
```

### 6.2 Description as Micro-Prompt Engineering

Each property description tells the LLM what to extract from conversation context to fill that parameter. These descriptions are effectively micro-prompts:

- Good: `"description": "City name, e.g. 'Paris' or 'New York'"`
- Bad: `"description": "city"`

The LLM reads the tool description + property descriptions to decide whether the tool matches the user's intent and how to populate the arguments.

### 6.3 Schema Best Practices from the Spec

- Keep schemas flat. Avoid deep nesting, `oneOf`, `allOf` -- they increase token count and parsing difficulty for the model.
- If a tool needs complex input, split it into multiple simpler tools.
- Property descriptions are critical for LLM accuracy.
- Name tools clearly. The `name` + `description` combo is what the LLM uses for tool selection.
- Naming constraints: 1-128 chars, `[A-Za-z0-9_\-.]` only, case-sensitive, unique per server.

### 6.4 Output Schema

Optional. When present, `structuredContent` in the response MUST conform to it. Clients SHOULD validate against it.

```json
"outputSchema": {
  "type": "object",
  "properties": {
    "rows": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": { "type": "string" }
      }
    },
    "row_count": { "type": "integer" }
  },
  "required": ["rows", "row_count"]
}
```

**Implication for ilo**: ilo's type system (`n`, `t`, `b`, records, lists) maps onto JSON Schema types: `number` -> `n`, `string` -> `t`, `boolean` -> `b`, `object` -> ilo record, `array` -> `L`. The MCP `inputSchema` can be mechanically translated to ilo tool declarations. A tool discovered via MCP becomes a callable function signature in ilo's closed world.

---

## 7. End-to-End Message Flow: Agent Invokes a Tool

Here is the complete sequence from agent intent to tool result, showing every MCP message exchanged.

### Phase 0: Connection Setup

```
Agent runtime spawns MCP server (stdio) or connects (HTTP)
  |
  +-> Client sends: initialize request
  +<- Server sends: initialize response (capabilities: tools, resources)
  +-> Client sends: notifications/initialized
```

### Phase 1: Tool Discovery

```
Agent needs to know what tools are available.
  |
  +-> Client sends:
      { "jsonrpc":"2.0", "id":2, "method":"tools/list", "params":{} }
  |
  +<- Server responds:
      { "jsonrpc":"2.0", "id":2, "result": { "tools": [
        { "name":"get_weather", "description":"...", "inputSchema":{...} },
        { "name":"send_email",  "description":"...", "inputSchema":{...} }
      ]}}
```

The host registers these tool signatures. In ilo terms, this populates the function table with external tool declarations.

### Phase 2: LLM Decides to Use a Tool

The host presents the user's message + available tool schemas to the LLM. The LLM's response includes a tool-use decision:

```
LLM output (internal to host, NOT an MCP message):
  "I need to call get_weather with city='Tokyo'"
```

This is model-specific. The LLM's tool-use output format varies by provider (Anthropic tool_use blocks, OpenAI function_call, etc.). MCP does NOT define how the LLM decides -- only how the call is executed.

### Phase 3: Tool Execution

```
  +-> Client sends:
      { "jsonrpc":"2.0", "id":3, "method":"tools/call",
        "params": { "name":"get_weather", "arguments":{"city":"Tokyo"} } }
  |
  +<- Server sends (optional, during execution):
      { "jsonrpc":"2.0", "method":"notifications/progress",
        "params": { "progressToken":"req-3", "progress":50, "total":100 } }
  |
  +<- Server responds:
      { "jsonrpc":"2.0", "id":3, "result": {
        "content": [{ "type":"text", "text":"Tokyo: 22C, clear sky" }],
        "structuredContent": { "temperature":22, "conditions":"clear sky" },
        "isError": false
      }}
```

### Phase 4: Result Fed Back to LLM

The host takes the tool result and feeds it back into the LLM's conversation:

```
LLM receives (internal, not MCP):
  [tool result: "Tokyo: 22C, clear sky"]

LLM generates final response:
  "The current weather in Tokyo is 22 degrees Celsius with clear skies."
```

### Phase 5 (Optional): Tool List Changes at Runtime

```
  +<- Server sends:
      { "jsonrpc":"2.0", "method":"notifications/tools/list_changed" }
  |
  +-> Client sends:
      { "jsonrpc":"2.0", "id":4, "method":"tools/list", "params":{} }
  |
  +<- Server responds with updated tool list
```

The agent's available function table updates dynamically.

### Summary Flow Diagram

```
User: "What's the weather in Tokyo?"
  |
  v
Host: present message + tool schemas to LLM
  |
  v
LLM: "I'll use get_weather(city='Tokyo')"
  |
  v
Client ----tools/call----> Server
  |                           |
  |                      (executes tool)
  |                           |
Client <---result---------- Server
  |
  v
Host: feed result back to LLM
  |
  v
LLM: "The weather in Tokyo is 22C and clear."
  |
  v
User sees response
```

---

## 8. Async Tasks (2025-11-25, Experimental)

The 2025-11-25 spec introduces Tasks, allowing any request to become asynchronous.

When a server cannot complete a request immediately, it returns a task handle instead of a result. The client can then poll for status or receive updates via notifications.

This enables:
- Long-running operations (deployment, data processing).
- Parallel task execution by the agent.
- Disconnect/reconnect without losing work.
- Status polling and progress tracking.

Tasks are still experimental and their design may change. But the pattern is significant: an agent kicks off work, continues reasoning, and collects results later. This maps to a `future` or `promise` pattern at the language level.

**Implication for ilo**: If ilo supports async tool calls, it would need a way to represent a "pending result" -- perhaps a value that the runtime can block on or poll. The `R ok err` result type could wrap a task handle, with a special `await` or `!` operator that blocks until the task completes.

---

## 9. Security and Trust Model

MCP's security model is worth understanding because it constrains what an agent language can safely do.

### 9.1 Human-in-the-Loop

The spec repeatedly emphasizes: there SHOULD always be a human in the loop with the ability to deny tool invocations. This applies to:
- `tools/call` -- Human can approve/deny each invocation.
- `sampling/createMessage` -- Human can edit/approve/deny the prompt and the completion.
- `elicitation/create` -- Human interacts directly.

### 9.2 Tool Descriptions Are Untrusted

Tool names, descriptions, and annotations come from the server. They SHOULD be treated as untrusted unless the server is known-trusted. A malicious server could describe a destructive tool as "read-only" via annotations.

### 9.3 Roots Are Advisory

Servers are not prevented from accessing files outside declared roots. The boundary is communicated, not enforced.

### 9.4 Input Validation

Tool execution errors (bad arguments, business logic failures) are returned inside `result` with `isError: true`. This lets the LLM see the error and self-correct. Protocol errors (malformed JSON, unknown method) are returned in the JSON-RPC `error` field and do NOT reach the LLM.

The 2025-11-25 spec clarifies: input validation failures should be tool execution errors (not protocol errors), specifically to enable model self-correction.

---

## 10. Implications for ilo-lang

### 10.1 Tool Declarations as First-Class Syntax

MCP tools are discovered via `tools/list` and have:
- A name (the callable identifier)
- An inputSchema (parameter types)
- An optional outputSchema (return type)
- Annotations (behavioral hints)

In ilo, this maps to:

```
-- MCP tool: get_weather(city:string, units?:string) -> {temperature:number, conditions:string}
-- ilo equivalent (conceptual):
tool get_weather "Get current weather" city:t units:t > weather
```

ilo's existing `tool name"desc" params>return` syntax (from D-AGENT-INTEGRATION.md) aligns directly.

### 10.2 Schema <-> Type Mapping

| JSON Schema   | ilo Type |
|--------------|----------|
| `number`     | `n`      |
| `string`     | `t`      |
| `boolean`    | `b`      |
| `null`       | `_`      |
| `object`     | record   |
| `array`      | `L x`    |

MCP's `inputSchema` properties map to ilo record fields. Required/optional maps to ilo's eventual optional type support.

### 10.3 Closed-World Verification from Discovery

ilo's "constrained" principle says every callable function is known ahead of time. MCP `tools/list` provides exactly this: a complete enumeration of available tools with their signatures. The ilo verifier can:
1. Fetch tool schemas from MCP servers at program load time.
2. Register them as external function declarations.
3. Verify all tool calls in the program against real schemas.
4. Reject programs that call non-existent tools or pass wrong argument types.

This makes `ToolProvider` a **signature source**, as noted in ilo's D-AGENT-INTEGRATION.md.

### 10.4 Error Handling

MCP returns tool errors in `result` with `isError: true`. This maps to ilo's `R ok err` type. A tool call that fails returns `R _ t` (nil ok, text error), which the agent handles with `?` conditional or `!` auto-unwrap.

### 10.5 Structured Content

MCP's `structuredContent` field returns typed JSON matching the `outputSchema`. When ilo defines the return type of a tool as a record, the runtime can deserialize `structuredContent` directly into an ilo value. The `content` text field serves as the human-readable fallback.

### 10.6 What ilo Does NOT Need to Handle

- **Transport details**: The ilo runtime handles stdio/HTTP framing. The language does not need transport syntax.
- **Initialization handshake**: Runtime concern. The language just calls tools; the runtime negotiates capabilities.
- **Sampling/Elicitation/Roots**: These are client-side primitives. If ilo is the agent (host/client), these are runtime features. If ilo is the tool (server), it might need to emit `sampling/createMessage` requests, which could be a builtin.
- **JSON serialization**: Tool boundary concern handled by the runtime. ilo values map to JSON; JSON maps back to ilo values.
- **Progress/cancellation**: Runtime handles these transparently.

### 10.7 What ilo DOES Need

1. **Tool declaration syntax** that maps to MCP's `inputSchema`/`outputSchema`.
2. **Result type** (`R ok err`) for all tool calls.
3. **Schema-to-type compiler** in the runtime that converts MCP `tools/list` responses into ilo function signatures.
4. **Dynamic tool table** that can update when `notifications/tools/list_changed` fires.
5. **Async support** (eventually) for MCP Tasks -- a way to represent pending results.

---

## Sources

- [MCP Specification (2025-11-25)](https://modelcontextprotocol.io/specification/2025-11-25)
- [MCP Specification (2025-06-18)](https://modelcontextprotocol.io/specification/2025-06-18)
- [MCP Architecture Overview](https://modelcontextprotocol.io/docs/learn/architecture)
- [MCP Tools Specification (Draft)](https://modelcontextprotocol.io/specification/draft/server/tools)
- [MCP Resources Specification](https://modelcontextprotocol.io/specification/2025-06-18/server/resources)
- [MCP Prompts Specification](https://modelcontextprotocol.io/specification/2025-06-18/server/prompts)
- [MCP Sampling Specification](https://modelcontextprotocol.io/specification/2025-06-18/client/sampling)
- [MCP Roots Specification](https://modelcontextprotocol.io/specification/2025-06-18/client/roots)
- [MCP Elicitation Specification](https://modelcontextprotocol.io/specification/2025-06-18/client/elicitation)
- [MCP Transports Specification](https://modelcontextprotocol.io/specification/2025-03-26/basic/transports)
- [MCP Lifecycle Specification](https://modelcontextprotocol.io/specification/2025-03-26/basic/lifecycle)
- [MCP 2025-11-25 Changelog](https://modelcontextprotocol.io/specification/2025-11-25/changelog)
- [MCP Message Types: JSON-RPC Reference Guide (Portkey)](https://portkey.ai/blog/mcp-message-types-complete-json-rpc-reference-guide/)
- [Understanding MCP Features: Tools, Resources, Prompts, Sampling, Roots, Elicitation (WorkOS)](https://workos.com/blog/mcp-features-guide)
- [MCP Primitives: The Mental Model Behind the Protocol (Portkey)](https://portkey.ai/blog/mcp-primitives-the-mental-model-behind-the-protocol/)
- [MCP 2025-11-25: Async Tasks, OAuth, Extensions (WorkOS)](https://workos.com/blog/mcp-2025-11-25-spec-update)
- [Why MCP Deprecated SSE (fka.dev)](https://blog.fka.dev/blog/2025-06-06-why-mcp-deprecated-sse-and-go-with-streamable-http/)
- [MCP Transport Comparison (MCPcat)](https://mcpcat.io/guides/comparing-stdio-sse-streamablehttp/)
- [MCP Tool Schema (Merge)](https://www.merge.dev/blog/mcp-tool-schema)
- [MCP Tool Annotations (Marc Nuri)](https://blog.marcnuri.com/mcp-tool-annotations-introduction)
- [MCP Protocol Mechanics and Architecture (Pradeep Loganathan)](https://pradeepl.com/blog/model-context-protocol/mcp-protocol-mechanics-and-architecture/)
- [Capabilities Negotiation in MCP (APXML)](https://apxml.com/courses/getting-started-model-context-protocol/chapter-1-architecture-and-fundamentals/capabilities-negotiation)
- [Defining Tool Schemas in MCP (APXML)](https://apxml.com/courses/getting-started-model-context-protocol/chapter-3-implementing-tools-and-logic/tool-definition-schema)
- [MCP Specification (Stainless Portal)](https://www.stainless.com/mcp/mcp-specification)
- [MCP GitHub Repository](https://github.com/modelcontextprotocol/modelcontextprotocol)