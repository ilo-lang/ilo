# HTTP streaming in ilo — roadmap and current state

> **Status: roadmap placeholder.** The streaming primitives described here
> ship via [ILO-46](https://linear.app/ilo-lang/issue/ILO-46) (Architectural C).
> This document tracks the gap and will be updated when ILO-46 merges.

## Current state (pre-ILO-46)

ilo's HTTP surface is request/response only.  The existing builtins buffer the
entire response body before returning:

| Builtin | Description |
|---|---|
| `get url > R t t` | GET, body buffered |
| `pst url body > R t t` | POST, body buffered |
| `get-to url ms > R t t` | GET with timeout, still buffered |
| `pst-to url body ms > R t t` | POST with timeout, still buffered |
| `getx url > R (M t _) t` | GET with rich response map, buffered |
| `pstx url body > R (M t _) t` | POST with rich response map, buffered |

There is no chunked-read primitive, no SSE iterator, no streaming-body POST,
and no `ilo httpd` server subcommand.  `rdinl` on the input side is also
eager (reads to EOF before yielding lines).

## What ILO-46 ships (Architectural C)

Four new streaming builtins, all returning a lazy `L t` iterator consumable
via `@chunk stream {...}`:

```
get-stream url > L t
get-stream-h url hdrs > L t
pst-stream url body > L t
pst-stream-h url body hdrs > L t
```

A lazy stdin iterator:

```
for-line stdin > L t
```

And a new CLI subcommand:

```
ilo httpd <handler.ilo> --port N
```

`ilo httpd` is distinct from `ilo serv` (the agent-protocol RPC).  The handler
is a function `fn handle (req: Request) > Response`.

Reference examples that ship with ILO-46:

* `examples/sse-client.ilo` — consume an SSE endpoint via `get-stream`
* `examples/streaming-stdin.ilo` — process lines as they arrive
* `examples/httpd-hello.ilo` — serve HTTP via `ilo httpd`

## Out of scope

* WebSocket (separate track).
* HTTP/3 / QUIC.
* WASM `fetch` (ILO-48).
* Replacing `ilo serv`.

## Closing this placeholder

ILO-57 closes when ILO-46 merges.  At that point update this document to remove
the "roadmap placeholder" notice and link to the merged PR.

## References

* ILO-46 — [HTTP streaming, streaming stdin, and ilo httpd subcommand](https://linear.app/ilo-lang/issue/ILO-46)
* ILO-57 — [No HTTP streaming (roadmap placeholder, ships via Architectural C)](https://linear.app/ilo-lang/issue/ILO-57)
* `pending.md` entry C (P0 Architectural) and entry #30 (P3 net-new gaps)
