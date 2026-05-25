# HTTP streaming in ilo

> **Status: shipping.** Server side via `ilo httpd` + chunked transfer
> encoding landed under ILO-46 / ILO-379. Client-side consumer builtins
> (`get-stream`, `pst-stream`, etc.) ship under
> [ILO-448](https://linear.app/ilo-lang/issue/ILO-448), the symmetric
> counterpart so ilo programs can consume long-lived chunked / SSE
> responses without buffering.

## Client-side: lazy line iterators (ILO-448)

Four builtins return a lazy `L t` iterator that drains the response body
one chunk-line at a time as bytes arrive. The body is never fully
buffered:

| Builtin | Description |
|---|---|
| `get-stream url > L t` | GET, response streamed as lines |
| `get-stream-h url headers > L t` | GET with custom headers, response streamed |
| `pst-stream url body > L t` | POST, response streamed as lines |
| `pst-stream-h url body headers > L t` | POST with custom headers, response streamed |

Consume via `@line stream {...}` foreach:

```ilo
@line (get-stream "http://localhost:7777/events/stream") {
  prnt line
}
```

Each iteration binds the next chunk-line with the trailing newline
stripped (and `\r` for `\r\n` chunked encoding). Mid-stream I/O errors
(socket close, malformed chunk framing) surface as `ILO-R009` with a
`http-stream read error: ...` message from the foreach. The initial
connection failure (DNS, refused) surfaces as an `Err` from the builtin
itself — but because the foreach expects a `LazyHttpLines`, that case
ends as a clean runtime error rather than a silent empty iteration.

Cap-checked via `--allow-net` before opening the connection (no socket
opens if the cap is denied). Tree + VM engines only in this release;
Cranelift JIT is a follow-up.

Reference: `examples/sse-client.ilo`.

## Server-side: `ilo httpd`

Available since ILO-379. `ilo httpd <handler.ilo> --port N` runs a handler
function `fn handle (req: Request) > Response` over chunked
transfer-encoding.

Since ILO-481 the handler file's `use` imports are resolved at startup,
relative to the handler's own directory, exactly like `ilo run` and
`ilo check`. A handler can therefore split logic across sibling modules
(`use "store.ilo"`) instead of inlining everything in one file. A missing
import surfaces a real diagnostic and the server refuses to start.

### Response body shapes

The response `body` field takes one of three shapes:

| Body value | Wire format | Buffering |
|---|---|---|
| `t` (string) | `Content-Length` | n/a (already a string) |
| `L t` (list) | `Transfer-Encoding: chunked` | list materialised before first byte |
| lazy iterator | `Transfer-Encoding: chunked` | **none** — streamed line by line |

A lazy iterator body (ILO-482) is the SSE / long-poll / file-tail path:
each line the iterator yields is written and flushed as its own chunk as
soon as the handler produces it, so the connection can be held open
indefinitely and the body is never fully buffered. Any value that drains
lazily works as the body:

```ilo
type rsp{status:n;body:_}

-- Proxy an upstream SSE / chunked source straight through, streaming.
handler req:_>rsp
  rsp status:200 body:(get-stream "http://localhost:7778/events/stream")
```

`for-line stdin` works the same way (tail stdin line by line). If the
client disconnects mid-stream the connection thread drops the iterator
(closing any upstream connection / open file) and exits cleanly with no
panic. A zero-arg `body` function is called first and may itself return
any of the three shapes.

> **Note (`get-stream` granularity).** `get-stream`'s underlying reader
> fills a 16 KiB backing buffer before yielding a line, so when proxying
> an upstream whose total body is smaller than that, lines can arrive in
> one batch rather than one at a time. The httpd plumbing itself streams
> per line; the batching is a `get-stream` buffer-size artifact tracked
> as a follow-up. A `tail-file` source (a lazy `tail -f` over a growing
> file, which crew's `/events/stream` needs) is the other follow-up.

Reference: `examples/httpd-stream.ilo`.

## Buffered HTTP (unchanged)

For request/response patterns where the entire body is small and easy to
buffer, the existing surface stays the cheaper option:

| Builtin | Description |
|---|---|
| `get url > R t t` | GET, body buffered |
| `pst url body > R t t` | POST, body buffered |
| `get-to url ms > R t t` | GET with timeout, still buffered |
| `pst-to url body ms > R t t` | POST with timeout, still buffered |
| `getx url > R (M t _) t` | GET with rich response map, buffered |
| `pstx url body > R (M t _) t` | POST with rich response map, buffered |

For stdin streaming, see `for-line stdin` (ILO-70).

## Out of scope

* WebSocket (separate track).
* HTTP/3 / QUIC.
* WASM streaming (`get-stream` returns `Err` on WASM; buffered `get`/`pst`
  remain the WASM-supported path via the `ilo_http` fetch host import).

## References

* ILO-46 — HTTP streaming, streaming stdin, and ilo httpd subcommand
* ILO-379 — chunked transfer-encoding for `ilo httpd`
* ILO-448 — client-side HTTP streaming builtins (`get-stream`, `pst-stream`, …)
* ILO-481 — `ilo httpd` resolves `use` imports in handler files
* ILO-482 — `ilo httpd` lazy streaming response body (handler-driven SSE)
