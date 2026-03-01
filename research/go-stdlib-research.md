# Go Standard Library Research for ilo

Go's standard library and design philosophy analyzed through ilo's lens: **total tokens from intent to working code**. Go is relevant because it is the closest mainstream language to ilo's philosophy — simplicity over expressiveness, one way to do things, explicit error handling, fast compilation, small spec.

---

## Why Go matters for ilo

Go and ilo share unusual convictions:

1. **Simplicity is a feature.** Go has ~25 keywords. ilo has ~6 abbreviated keywords (sigils replace most). Both reject the "more features = more power" assumption.
2. **One way to do things.** Go has one loop (`for`), one string type, one way to handle errors. ilo has one loop sigil (`@`), one conditional form (guard), one error pattern (`R`+`?`+`!`).
3. **Explicit over implicit.** Go requires explicit error checking (`if err != nil`). ilo requires explicit `?` matching or `!` auto-unwrap. Neither has exceptions.
4. **Verification before execution.** Go refuses to compile with unused imports or variables. ilo verifies all calls resolve, all types align, all dependencies exist.
5. **Small spec, big stdlib.** Go ships batteries included — HTTP server, JSON, crypto, testing — all in the standard library. ilo's "tool" model replaces stdlib with tool declarations, but the same principle applies: the runtime should handle common needs without external dependencies.

Where they diverge: Go prioritizes human readability (meaningful names, gofmt, documentation). ilo explicitly sacrifices human readability for token efficiency. Go is general-purpose; ilo is agent-purpose.

---

## 1. File I/O — `os`, `io`, `bufio`, `filepath`

### What Go provides

```go
// Read file
data, err := os.ReadFile("config.json")

// Write file
err := os.WriteFile("output.txt", []byte(content), 0644)

// Buffered I/O
scanner := bufio.NewScanner(file)
for scanner.Scan() {
    line := scanner.Text()
}

// Path manipulation
dir := filepath.Dir(path)
ext := filepath.Ext(path)
abs, _ := filepath.Abs(rel)
joined := filepath.Join("dir", "sub", "file.txt")
```

**Design choices worth noting:**
- `os.ReadFile` / `os.WriteFile` are one-liners for the 90% case. `bufio` for the 10% that need streaming.
- `filepath` handles OS-specific separators transparently. The agent never thinks about `/` vs `\`.
- Every operation returns `(result, error)` — no exceptions, no silent failures.

### Relevance to ilo

ilo's planned file I/O (G8) mirrors Go's layered approach:

| Go | ilo (planned) | Token comparison |
|----|---------------|------------------|
| `os.ReadFile("f.txt")` | `fread "f.txt"` | 3 vs 2 tokens |
| `os.WriteFile("f.txt", data, 0644)` | `fwrite "f.txt" data` | 5 vs 3 tokens |
| `bufio.Scanner` + loop | `@line stream{...}` (G6) | ~15 vs ~5 tokens |
| `filepath.Join("a","b","c")` | Not planned | — |

**Recommendation for ilo:**
- `fread` and `fwrite` as builtins (already planned in G8) — matches Go's one-liner philosophy.
- **Skip `filepath`-style path manipulation.** Go needs it because Go programs run across OS. ilo programs run through an agent that knows the OS. Path manipulation is a tool concern or handled by the agent.
- Go's `bufio.Scanner` pattern (line-by-line iteration) maps beautifully to ilo's `@line stream{...}` — if G6 streams land.
- Go's file permissions model (`0644`) should be hidden in ilo. Agents should not think about unix permissions. Use sensible defaults.

---

## 2. Networking — `net/http`, `net`, WebSocket

### What Go provides

```go
// HTTP GET — 2 lines for the common case
resp, err := http.Get("https://api.example.com/data")
body, err := io.ReadAll(resp.Body)

// HTTP POST with JSON
resp, err := http.Post(url, "application/json", bytes.NewBuffer(jsonData))

// HTTP server — famous for simplicity
http.HandleFunc("/", handler)
http.ListenAndServe(":8080", nil)

// Raw TCP
conn, err := net.Dial("tcp", "example.com:80")
conn.Write([]byte("GET / HTTP/1.1\r\n..."))

// WebSocket (gorilla/websocket — not stdlib but de facto standard)
conn, _, err := websocket.DefaultDialer.Dial(url, nil)
conn.WriteMessage(websocket.TextMessage, []byte(msg))
_, message, err := conn.ReadMessage()
```

**Design choices worth noting:**
- `http.Get` is a package-level convenience function — one line for the 90% case.
- The full `http.Client` with timeouts, headers, redirects is available but not required.
- Go's HTTP server is so simple that it's commonly used as a production server without frameworks.
- WebSocket is NOT in stdlib — Go's committee was conservative about adding it. This is telling: Go's stdlib boundary is drawn at "stable, universal protocols."

### Relevance to ilo

ilo already has `get`/`$` (D1b). The roadmap covers HTTP methods (G1), WebSocket (G2), and raw TCP (G5).

| Go | ilo (current/planned) | Tokens |
|----|----------------------|--------|
| `http.Get(url)` + `io.ReadAll` | `get url` or `$url` | ~8 vs 2 |
| `http.Post(url, ct, body)` | `post url body` (G1) | ~6 vs 3 |
| `websocket.Dial(url)` | `ws url` (G2) | ~4 vs 2 |
| `conn.WriteMessage(msg)` | `ws-send c msg` (G2) | ~4 vs 3 |
| `conn.ReadMessage()` | `ws-recv c` (G2) | ~3 vs 2 |
| `http.ListenAndServe` | Not planned | — |

**Lessons from Go for ilo:**

1. **Convenience wrappers win.** Go's `http.Get` is a one-liner that hides `http.Client` complexity. ilo's `$url` is the same philosophy pushed further — one sigil.

2. **No HTTP server needed.** ilo programs are agent tools, not web services. Go's `http.ListenAndServe` is irrelevant. If an agent needs to serve HTTP, that is a tool server concern (like the Playwright server).

3. **WebSocket as a separate concern.** Go correctly keeps WebSocket out of stdlib. ilo should follow suit — `ws` builtins (G2) are important for CDP/real-time, but they are in a feature flag, not core.

4. **The `Content-Type` tax.** Go forces you to specify `"application/json"` on POST. ilo should default to `application/json` — it is what agents send 95% of the time. Token savings: 1 eliminated string per POST.

5. **Response handling.** Go separates the response status, headers, and body. ilo's `R t t` collapses this to ok-body or err-message. For agents, this is the right tradeoff — agents rarely inspect headers or status codes directly. If they need to, a richer response type (record with `status:n`, `body:t`, `headers:M t t`) can be opt-in.

---

## 3. Process Execution — `os/exec`

### What Go provides

```go
// Simple command — capture output
out, err := exec.Command("ls", "-la").Output()

// With stdin/stdout/stderr
cmd := exec.Command("grep", "error")
cmd.Stdin = strings.NewReader(input)
var stdout bytes.Buffer
cmd.Stdout = &stdout
err := cmd.Run()

// Long-running process
cmd := exec.Command("node", "server.js")
cmd.Start()
// ... later
cmd.Wait()

// Environment
cmd.Env = append(os.Environ(), "KEY=VALUE")
cmd.Dir = "/path/to/working/dir"
```

**Design choices worth noting:**
- `exec.Command("name", "arg1", "arg2")` separates command name from args — no shell injection by design.
- `.Output()` is the convenience one-liner (captures stdout). `.Run()` for more control.
- `.Start()` + `.Wait()` for async processes — same pattern as ilo's planned `spawn` + `proc-wait`.
- Environment is explicit, not inherited by default (actually it is inherited — Go inherits parent env, but you can override).

### Relevance to ilo

ilo plans two levels (I2 + G3): `run "cmd"` for one-shot execution, `spawn cmd args` for long-running processes.

| Go | ilo (planned) | Tokens |
|----|---------------|--------|
| `exec.Command("ls","-la").Output()` | `run "ls -la"` or `` `ls -la` `` | ~6 vs 2-3 |
| `cmd.Start()` | `spawn "node" "server.js"` (G3) | ~4 vs 3 |
| `cmd.Wait()` | `proc-wait h` (G3) | ~3 vs 2 |

**Lessons from Go for ilo:**

1. **Shell injection prevention.** Go's `exec.Command` takes separate args, preventing injection. ilo's `run "ls -la"` passes a single string to `/bin/sh -c` — this is deliberately less safe but more token-efficient. The tradeoff is correct for ilo: agents generate commands, not untrusted user input. If safety is needed, `spawn cmd args` with separate args (G3) is the safe alternative.

2. **Two levels of API.** Go has `.Output()` (simple) and `.Start()`/`.Wait()` (complex). ilo plans the same: `run` (simple one-shot) and `spawn`/`proc-wait`/`proc-kill` (complex lifecycle). This layered design is good — copy it.

3. **Environment as explicit argument.** Go's `cmd.Env` is explicit. ilo's `env key` reads env vars, and `spawn cmd args env` (G3 with maps) passes them. This is right — agents should not silently inherit environment.

---

## 4. Data Formats — `encoding/json`, `encoding/xml`, `encoding/csv`

### What Go provides

```go
// JSON — struct tags for mapping
type User struct {
    Name  string `json:"name"`
    Email string `json:"email"`
}
var user User
json.Unmarshal(data, &user)  // JSON → struct
out, _ := json.Marshal(user) // struct → JSON

// Dynamic JSON (when shape is unknown)
var result map[string]interface{}
json.Unmarshal(data, &result)

// CSV
reader := csv.NewReader(file)
records, _ := reader.ReadAll()

// XML — struct tags
var feed Feed
xml.Unmarshal(data, &feed)
```

**Design choices worth noting:**
- Go's JSON uses struct tags for field mapping — the struct definition IS the schema. This is exactly what ilo records do: `type profile{id:t;name:t;email:t}` IS the schema.
- `map[string]interface{}` is Go's escape hatch for unknown JSON — like ilo's `>t` (raw text).
- Go has built-in JSON, XML, CSV. Most languages require third-party libraries.
- `encoding/json` handles both serialization and deserialization in one package.

### Relevance to ilo

ilo's position (from OPEN.md): "format parsing is a tool concern." But JSON is the exception (I1).

| Go | ilo (planned) | Tokens |
|----|---------------|--------|
| `json.Unmarshal(data, &user)` | `jparse text "profile"` (I1) | ~5 vs 3 |
| `json.Marshal(user)` | `jdump value` (I1) | ~3 vs 2 |
| `result["name"]` (dynamic) | `jp body "name"` (I1) | ~3 vs 3 |
| `csv.NewReader(f).ReadAll()` | Not planned (tool) | — |
| `xml.Unmarshal(data, &f)` | Not planned (tool) | — |

**Lessons from Go for ilo:**

1. **Struct tags = record declarations.** Go's `json:"name"` annotation maps JSON fields to struct fields. ilo's record declarations already serve this role: `type profile{id:t;name:t}` tells the runtime how to map JSON. D1e (Value <-> JSON) should exploit this — ilo records ARE the schema, just like Go struct tags.

2. **Dynamic JSON needs an escape hatch.** Go's `map[string]interface{}` is ugly but necessary. ilo's `>t` (raw text) plus `jp` (JSON path lookup) is a cleaner equivalent for agents. An agent rarely needs the full parsed tree — it needs specific fields. `jp body "user.name"` is more efficient than parsing the entire response into a dynamic structure.

3. **JSON is special.** Go puts JSON in stdlib. ilo should treat JSON as a builtin concern, not just a tool concern. The `jp` builtin (I1) is the right call — it is the agent equivalent of Go's `json.Unmarshal`.

4. **XML/CSV are NOT special.** Go has them in stdlib because Go is general-purpose. ilo should not. XML/CSV parsing is a tool concern — an agent calling an XML API would use a tool that returns parsed records. Token comparison: teaching ilo to parse XML would add spec complexity (tokens for agents to learn) while saving very few generation tokens.

---

## 5. String Manipulation — `regexp`, `strings`, `fmt`, `encoding`

### What Go provides

```go
// strings package — comprehensive
strings.Contains(s, "error")    // substring check
strings.Split(s, ",")           // split
strings.Join(parts, ", ")       // join
strings.Replace(s, "old", "new", -1)
strings.ToUpper(s) / strings.ToLower(s)
strings.TrimSpace(s)
strings.HasPrefix(s, "http")
strings.HasSuffix(s, ".json")

// regexp
re := regexp.MustCompile(`\d+`)
matches := re.FindAllString(text, -1)
result := re.ReplaceAllString(text, "***")

// fmt — formatted output
s := fmt.Sprintf("Hello %s, you have %d items", name, count)

// encoding
base64.StdEncoding.EncodeToString(data)
hex.EncodeToString(data)
url.QueryEscape(s)
```

**Design choices worth noting:**
- Go's `strings` package has 40+ functions. Most are one-liners. No method chaining — `strings.ToUpper(strings.TrimSpace(s))`.
- `regexp.MustCompile` panics on bad regex — "fail fast." Regular `Compile` returns an error.
- `fmt.Sprintf` is the universal string formatting tool — `%s`, `%d`, `%f`, `%v`.
- Encoding is split into subpackages: `encoding/base64`, `encoding/hex`, `net/url`.

### Relevance to ilo

ilo already has several string builtins. Here is the mapping:

| Go | ilo (current) | Status |
|----|--------------|--------|
| `strings.Contains(s, sub)` | `has s sub` | Done |
| `strings.Split(s, sep)` | `spl s sep` | Done |
| `strings.Join(parts, sep)` | `cat parts sep` | Done |
| `len(s)` | `len s` | Done |
| `s[a:b]` | `slc s a b` | Done |
| `strings.Replace` | Not planned | — |
| `strings.ToUpper` / `ToLower` | Not planned | — |
| `strings.TrimSpace` | Not planned | — |
| `strings.HasPrefix` / `HasSuffix` | Not planned (`has` does contains) | — |
| `regexp.FindAllString` | `matchall text pat` (I8) | Planned |
| `regexp.ReplaceAllString` | `sub text pat repl` (I8) | Planned |
| `fmt.Sprintf` | `fmt "pattern" args...` (I4) | Planned |
| `base64.Encode/Decode` | `b64enc`/`b64dec` (I7) | Planned |
| `url.QueryEscape` | `urlencode` (I7) | Planned |

**Gap analysis — what Go has that ilo might need:**

1. **`replace` / `sub` (non-regex).** Simple string replacement is extremely common. `sub text "old" "new"` could work alongside regex `sub` — or use a single `sub` that treats the pattern as literal when it has no regex metacharacters. Go separates `strings.Replace` (literal) from `regexp.ReplaceAllString` (regex). ilo should unify: one `sub` builtin, regex patterns when needed, literal otherwise.

2. **`upper` / `lower`.** Case conversion comes up in URL normalization, header construction, comparisons. Low token cost to add: `upper t` / `lower t`. But frequency is low for agents — they usually work with APIs that are case-insensitive or case-normalized.

3. **`trim`.** Whitespace trimming is common when processing tool output. `trim t` removes leading/trailing whitespace. High frequency, low cost.

4. **`prefix` / `suffix` checks.** `has` does substring containment. Prefix/suffix checks require `slc` + comparison today (3+ tokens). `pfx s p` / `sfx s p` would save tokens, but the frequency is low.

**Recommendation:** Add `trim`, `sub` (literal), `upper`/`lower` as builtins if token analysis shows agents need them frequently. `replace` is the most likely candidate — building URLs, modifying tool output, and constructing messages all require string replacement.

---

## 6. Concurrency — goroutines, channels, `sync`, `select`

### What Go provides

```go
// Goroutines — lightweight concurrent execution
go fetchURL(url)

// Channels — typed communication
ch := make(chan string)
go func() { ch <- "result" }()
msg := <-ch

// Select — multiplex channels
select {
case msg := <-ch1:
    handle(msg)
case msg := <-ch2:
    handle(msg)
case <-time.After(5 * time.Second):
    timeout()
}

// WaitGroup — wait for multiple goroutines
var wg sync.WaitGroup
for _, url := range urls {
    wg.Add(1)
    go func(u string) {
        defer wg.Done()
        fetch(u)
    }(url)
}
wg.Wait()

// Mutex — shared state protection
var mu sync.Mutex
mu.Lock()
counter++
mu.Unlock()
```

**Design choices worth noting:**
- Goroutines are the killer feature. `go f()` — 2 tokens to launch concurrent work.
- Channels are the ONLY way to communicate between goroutines (by convention). No shared memory.
- `select` is a language primitive for multiplexing — not a library function.
- Go does NOT have async/await. Goroutines + channels replace it entirely.
- The concurrency model is built into the language, not bolted on.

### Relevance to ilo — THIS IS THE BIG QUESTION

ilo is currently fully synchronous. The roadmap has G4 (async runtime) as a future phase with a key open question: should ilo expose concurrency to the language, or hide it in the runtime?

**Go's concurrency model is relevant for three ilo use cases:**

1. **Parallel tool calls.** Agent calls 3 APIs. They are independent. Calling them sequentially wastes time. Go: `go fetch(url1); go fetch(url2); go fetch(url3)`. ilo planned: `par{get url1;get url2;get url3}` (G4).

2. **WebSocket + polling.** CDP communication requires sending a message and waiting for an async response while potentially receiving other events. Go: goroutine reads messages, sends to channel, main goroutine selects. ilo: blocking `ws-recv` works for request-response but fails for multiplexed communication.

3. **Streaming responses.** SSE from LLM APIs delivers tokens incrementally. Go: goroutine reads SSE, sends each event to channel. ilo: G1d is planned but the iteration model is unclear.

**Design analysis:**

| Approach | Go equivalent | Token cost | Agent complexity |
|----------|--------------|------------|-----------------|
| Sequential (current) | No goroutines | 0 extra | None — agents know sequential |
| Runtime-hidden parallel | Go's `http.Client` internal goroutines | 1 token (`par{...}`) | Low — agent sees `par` block |
| Language-level goroutines | `go func()` + channels | 5-10 tokens per concurrent op | High — agents must reason about concurrency |
| Channel-based | `ch := make(chan); go func()` | 10+ tokens | Very high — deadlock risk, ordering |

**Recommendation for ilo:**

1. **Do NOT expose goroutines or channels.** Go's concurrency model is elegant but requires reasoning about synchronization, deadlocks, and ordering. Agents generating concurrent code will produce deadlocks — a failure mode that is nearly impossible to diagnose from error messages alone. The retry cost of a deadlock is catastrophic: the program hangs, produces no error, and the agent has no signal to fix.

2. **DO use Go's `par` pattern — runtime-managed parallelism.** The planned `par{call1;call2;call3}` (G4) is the right abstraction. The runtime launches all calls concurrently and collects results. The agent writes sequential-looking code. Internally, this is goroutines + WaitGroup, but the agent never sees it.

3. **`select`-like multiplexing as `poll`.** For WebSocket/SSE (G9), a `poll conns timeout` builtin that waits on multiple connections is cleaner than exposing channels. This is Go's `select` with the channel complexity hidden.

4. **Go's mistake to avoid:** Go makes it easy to launch goroutines but hard to know when they finish. `WaitGroup` is ceremony. ilo's `par{...}` avoids this by scoping concurrency — all concurrent work finishes before the block returns.

**Token comparison (parallel API calls):**
```
# Go: ~25 tokens
var wg sync.WaitGroup
for _, url := range urls {
    wg.Add(1)
    go func(u string) {
        defer wg.Done()
        fetch(u)
    }(url)
}
wg.Wait()

# ilo (planned): ~5 tokens
par{get url1;get url2;get url3}

# ilo (current, sequential): ~6 tokens
r1=get url1;r2=get url2;r3=get url3
```

---

## 7. Error Handling — `error` interface, `errors`, `fmt.Errorf`

### What Go provides

```go
// Every function returns error
result, err := doSomething()
if err != nil {
    return fmt.Errorf("failed to do thing: %w", err)
}

// Error wrapping (Go 1.13+)
err = fmt.Errorf("context: %w", originalErr)
errors.Is(err, ErrNotFound)
errors.As(err, &target)

// Custom errors
type NotFoundError struct {
    ID string
}
func (e *NotFoundError) Error() string {
    return "not found: " + e.ID
}

// Sentinel errors
var ErrNotFound = errors.New("not found")
```

**Design choices worth noting:**
- No exceptions. Period. Every error is a return value.
- `if err != nil` is the most written line in Go — it is verbose but explicit.
- Error wrapping (`%w`) adds context without losing the original error.
- Custom error types allow callers to inspect error details.
- Go community has debated error handling verbosity for 15 years. The explicit approach won.

### Relevance to ilo — DIRECTLY RELEVANT

ilo's error handling is Go's error handling made terse:

| Go | ilo | Token savings |
|----|-----|---------------|
| `result, err := f()` + `if err != nil { return err }` | `f! args` (auto-unwrap) | ~12 tokens vs 1 |
| `if err != nil { return fmt.Errorf("context: %w", err) }` | `?{^e:^+"context: "e}` | ~15 tokens vs ~8 |
| `result, err := f()` + `if err != nil { cleanup(); return err }` | `f args;?{^e:cleanup;^e}` | ~15 tokens vs ~8 |

**ilo has already solved Go's biggest criticism.** The `if err != nil` boilerplate — which accounts for roughly 30% of Go code by some estimates — is eliminated by `!` (auto-unwrap). This is ilo's version of Rust's `?` operator, which Go famously rejected.

**Lessons from Go for ilo:**

1. **Error wrapping is valuable.** Go's `fmt.Errorf("context: %w", err)` adds context to propagated errors. ilo's `^+"context: "e` achieves the same thing with string concatenation. This pattern should be documented as a best practice — agents need to know that wrapping errors with context helps debugging.

2. **Sentinel errors are unnecessary for ilo.** Go's `var ErrNotFound = errors.New(...)` pattern exists because Go errors are interfaces that can be inspected. ilo errors are text. Pattern matching on error text (`?{^e:has e "not found"{...}}`) is sufficient for agents. Adding typed error variants would cost more tokens than it saves.

3. **Go's `errors.Is`/`errors.As` for error chain inspection is overkill for ilo.** Agents don't build complex error hierarchies. They call a tool, it succeeds or fails, they retry or report. ilo's flat `R ok err` with text errors is the right level of granularity.

4. **The "no exceptions" principle is critical.** Go proved that explicit error handling works at scale. ilo doubles down on this — `R` return types make errors visible in function signatures, `?` forces handling, `!` is opt-in propagation. An agent can never accidentally ignore an error (the verifier catches unhandled `R` values through match exhaustiveness).

---

## 8. Environment — `os.Getenv`, `os.Args`, `flag`

### What Go provides

```go
// Environment variables
key := os.Getenv("API_KEY")
value, exists := os.LookupEnv("OPTIONAL_VAR")

// Command-line args
args := os.Args[1:]

// Flag parsing
port := flag.Int("port", 8080, "server port")
verbose := flag.Bool("verbose", false, "verbose output")
flag.Parse()
```

**Design choices worth noting:**
- `os.Getenv` returns empty string if not set — no error. `LookupEnv` returns `(string, bool)` for explicit existence check.
- `os.Args` is raw — no parsing. `flag` package does structured parsing.
- `flag` uses a registration pattern — define flags, then parse all at once.

### Relevance to ilo

ilo's planned `env key` (I3) maps directly to Go's `os.LookupEnv`:

| Go | ilo (planned) | Tokens |
|----|---------------|--------|
| `os.Getenv("API_KEY")` | `env "API_KEY"` | ~4 vs 2 |
| `os.LookupEnv("KEY")` | `env "KEY"` returns `R t t` | ~5 vs 2 |
| `os.Args[1:]` | CLI args passed to function params | 0 (implicit) |
| `flag.Int("port", 8080, "...")` | Not needed | — |

**Lessons from Go for ilo:**

1. **`env` should return `R t t`, not empty string.** Go's `Getenv` returning empty string on missing vars is a mistake — you can't distinguish "not set" from "set to empty." ilo's `R t t` approach (I3) is better: `Err("not set")` vs `Ok("")` vs `Ok("value")`.

2. **CLI args are already handled.** ilo passes CLI args to function parameters positionally. This is more type-safe than Go's `os.Args` (raw strings) and more token-efficient than Go's `flag` (declaration ceremony). No new features needed.

3. **No `flag` package needed.** ilo programs are generated by agents for specific tasks. They don't need self-documenting CLI interfaces with help text and default values. Function parameters ARE the "flags."

---

## 9. Cryptography — `crypto/sha256`, `crypto/hmac`, `crypto/tls`

### What Go provides

```go
// SHA-256
hash := sha256.Sum256([]byte("data"))
hexHash := hex.EncodeToString(hash[:])

// HMAC
mac := hmac.New(sha256.New, []byte(key))
mac.Write([]byte(message))
signature := hex.EncodeToString(mac.Sum(nil))

// TLS — transparent via http.Client
// Go's http.Client uses TLS by default for HTTPS URLs
resp, err := http.Get("https://secure.example.com")
```

**Design choices worth noting:**
- Go puts crypto in stdlib — no third-party dependency for basic hashing and signing.
- TLS is transparent — `http.Get("https://...")` just works. No configuration needed for the common case.
- HMAC is two lines — create, write, sum. Simple API for a common need.

### Relevance to ilo

ilo's planned hashing (I9) is minimal and correct:

| Go | ilo (planned) | Tokens |
|----|---------------|--------|
| `sha256.Sum256(data)` + hex encode | `hash text` | ~6 vs 2 |
| `hmac.New` + `Write` + `Sum` | `hmac key msg` | ~8 vs 3 |
| TLS via HTTP client | Transparent (http-tls feature) | 0 |

**Lessons from Go for ilo:**

1. **Hash should return hex string directly.** Go requires `hex.EncodeToString` as a separate step. ilo's `hash text` should return a hex string — agents always want the hex form, never raw bytes.

2. **HMAC is essential.** AWS, Stripe, GitHub webhooks — the most common APIs agents interact with require HMAC signatures. Go's `crypto/hmac` is simple. ilo's `hmac key msg` (I9) is simpler. Two tokens for API authentication.

3. **TLS should be transparent.** Go handles TLS automatically for HTTPS URLs. ilo should do the same — the `http-tls` feature flag adds rustls, and `get "https://..."` just works. Agents should never think about TLS configuration.

4. **No encryption builtins needed.** Go has `crypto/aes`, `crypto/rsa`, etc. ilo should NOT. Encryption is a tool concern — it requires key management, IV generation, padding modes, and other details that are dangerous to get wrong. Agents should call encryption tools, not implement crypto.

---

## 10. Time/Date — `time` package

### What Go provides

```go
// Current time
now := time.Now()
unix := now.Unix()

// Duration
time.Sleep(5 * time.Second)
elapsed := time.Since(start)

// Formatting — Go's unique approach
formatted := now.Format("2006-01-02 15:04:05")  // reference time!
parsed, _ := time.Parse("2006-01-02", "2024-03-15")

// Comparison
if deadline.Before(time.Now()) { ... }
```

**Design choices worth noting:**
- Go's time formatting uses a **reference time** (`Mon Jan 2 15:04:05 MST 2006`) instead of `%Y-%m-%d` format codes. This is famously controversial but arguably more readable.
- `time.Sleep` takes a `Duration`, not a number — type safety.
- `time.Since(start)` is a convenience for `time.Now().Sub(start)`.
- Go refuses to have a `Date` type separate from `Time` — one type for everything temporal.

### Relevance to ilo

ilo's planned time support (I6) is minimal:

| Go | ilo (planned) | Tokens |
|----|---------------|--------|
| `time.Now().Unix()` | `now()` | ~4 vs 1 |
| `time.Sleep(5 * time.Second)` | `sleep 5` | ~5 vs 2 |
| `time.Format(...)` | `fmt-time n pattern` (deferred) | ~5 vs 3 |

**Lessons from Go for ilo:**

1. **`now()` returns Unix timestamp as float.** Go's `time.Now()` returns a complex `Time` struct. ilo should return a plain number (seconds since epoch, float for sub-second). Agents can do arithmetic on numbers directly: `elapsed = -now() start` (subtract).

2. **`sleep` is essential despite being a side effect.** Go has `time.Sleep` in stdlib. ilo needs `sleep n` for rate limiting and polling. The manifesto concern about "pure execution" should yield to practical necessity — agents call APIs with rate limits.

3. **Skip time formatting.** Go's `time.Format` is complex (the reference time approach). ilo should defer `fmt-time` — if an agent needs formatted timestamps, a tool can handle it. `now()` + `sleep` cover 90% of agent time needs (rate limiting, timeouts, elapsed time measurement).

4. **Duration as number, not type.** Go has a dedicated `Duration` type. ilo should keep time as plain `n` (seconds). `sleep 0.5` sleeps 500ms. No new type needed. This is the token-minimal choice.

---

## 11. Collections — slices, maps, channels

### What Go provides

```go
// Slices
s := []int{1, 2, 3}
s = append(s, 4)
sub := s[1:3]
sort.Ints(s)
slices.Contains(s, 2)  // Go 1.21+

// Maps
m := map[string]int{"a": 1, "b": 2}
v, ok := m["key"]  // comma-ok idiom
delete(m, "key")
for k, v := range m { ... }

// Channels (as collections)
ch := make(chan int, 10)  // buffered
ch <- 42
v := <-ch
close(ch)
for v := range ch { ... }  // iterate until closed
```

**Design choices worth noting:**
- Go slices are dynamic arrays with `append` — similar to ilo's lists.
- Go maps use the "comma-ok" idiom (`v, ok := m["key"]`) — existence check + value retrieval in one operation.
- `for range` works on slices, maps, strings, AND channels — unified iteration.
- Go 1.21+ added `slices` and `maps` packages with generic helpers (Contains, Sort, etc.).

### Relevance to ilo

ilo has lists, plans maps (E4), and should NOT have channels.

| Go | ilo (current/planned) | Status |
|----|----------------------|--------|
| `[]int{1,2,3}` | `[1,2,3]` | Done |
| `append(s, v)` | `+=xs v` | Done |
| `s[1:3]` | `slc xs 1 3` | Done |
| `sort.Ints(s)` | `srt xs` | Done |
| `slices.Contains(s, v)` | `has xs v` | Done |
| `len(s)` | `len xs` | Done |
| `map[string]int{"a":1}` | `M{...}` (E4) | Planned |
| `m["key"]` | `at k m` or `get k m` (E4) | Planned |
| `for range` | `@v xs{...}` | Done |
| Channels | NOT planned | Deliberate |

**Lessons from Go for ilo:**

1. **Comma-ok pattern maps to `R`/`O`.** Go's `v, ok := m["key"]` is a two-value return that forces existence checking. ilo's map access should return `O v` (optional) — forces the agent to handle the missing-key case via `??` or `?`. This is already planned in E4.

2. **Unified iteration is good.** Go's `for range` works on slices, maps, strings, and channels. ilo's `@` should work on lists (done), maps (E4: `@k m{...}` for keys, `@kv m{...}` for key-value pairs), and potentially streams (G6: `@line stream{...}`). The `@` sigil becomes the universal iterator.

3. **No `make`.** Go requires `make(chan, n)`, `make(map)`, `make([]int, len, cap)` for initialization. ilo uses literals (`[1,2,3]`, `M{...}`) — no factory function needed. Token savings from avoiding `make`.

4. **`delete` as a builtin.** Go has `delete(m, key)` for map entry removal. ilo should have `del k m` if maps (E4) land — agents modifying tool configuration or building requests need to remove fields.

---

## 12. Type System — interfaces, structs, generics

### What Go provides

```go
// Structs — named product types
type User struct {
    Name  string
    Email string
}
u := User{Name: "Alice", Email: "a@b.com"}

// Interfaces — implicit satisfaction
type Reader interface {
    Read(p []byte) (n int, err error)
}
// Any type with a Read method satisfies Reader — no "implements"

// Generics (Go 1.18+)
func Map[T, U any](s []T, f func(T) U) []U { ... }

// Type constraints
type Number interface {
    ~int | ~float64
}
func Sum[T Number](s []T) T { ... }
```

**Design choices worth noting:**
- Go interfaces are **implicit** — no `implements` keyword. If a type has the right methods, it satisfies the interface. This is duck typing with compile-time verification.
- Go survived 10+ years without generics. When generics arrived (1.18), they were minimal — type parameters with constraints, no variance, no higher-kinded types.
- Go structs have no methods in the definition — methods are declared separately: `func (u User) String() string { ... }`.
- Go has no enums. Constants + iota is the workaround.

### Relevance to ilo

ilo's type system is intentionally simpler than Go's. The TYPE-SYSTEM.md research already notes: "ilo's position: closer to Elm than Go in error handling, closer to Go in expressiveness."

| Go | ilo (current/planned) | Status |
|----|----------------------|--------|
| Structs | Records (`type point{x:n;y:n}`) | Done |
| Interfaces | Traits (E6) — deferred, lowest priority | Planned (far future) |
| Generics | E5 — planned but high cost | Planned |
| Enums | Sum types (E3) — `enum status{...}` | Planned |
| `any` (empty interface) | `Ty::Unknown` (internal) | Done (internal only) |

**Lessons from Go for ilo:**

1. **Defer generics like Go did.** Go launched in 2009, added generics in 2022. 13 years of production use informed the design. ilo should follow the same path: ship without generics, learn from real agent programs what generic patterns are actually needed, then add the minimum viable generics. The TODO already sequences this correctly (E5 is fifth priority).

2. **Implicit interfaces are wrong for ilo.** Go's implicit interface satisfaction is elegant but requires the compiler to scan all methods of all types. In ilo's constrained world, everything should be explicit — an agent should never wonder whether a type "accidentally" satisfies an interface. If traits (E6) are ever added, they should require explicit implementation declarations.

3. **Go's lack of enums is a mistake ilo should not repeat.** Go uses `const + iota` for enums, which provides no exhaustiveness checking. ilo's planned sum types (E3) with exhaustive `?` matching are strictly better — they catch missing cases at verify time, preventing a class of errors that Go still struggles with.

4. **`any` type is dangerous.** Go's `any` (empty interface) can hold anything, defeating the type system. ilo's `Ty::Unknown` is used internally for error recovery but should never be exposed as a user-facing type. The escape hatch is `t` (raw text) — it is typed (it is text) but unstructured. This is better than `any` because the agent knows it is working with text and must parse it.

5. **Go's struct construction syntax is good for ilo.** Go's `User{Name: "Alice"}` uses named fields. ilo's `point x:10 y:20` is the same pattern with fewer tokens (no braces, no quotes around field names). Both approaches make field mapping explicit — the agent cannot get argument order wrong.

---

## Go Design Principles Applied to ilo

### Principle: "Clear is better than clever"

Go's proverbs include "Clear is better than clever." For ilo, this translates to: **correct is better than terse**. A 3-token solution that an agent generates correctly 10/10 times beats a 2-token solution that fails 2/10 times.

**Applied to ilo decisions:**
- Prefix notation is "clever" by human standards but "clear" by agent standards — fixed arity makes every expression unambiguous.
- `!` auto-unwrap is a calculated cleverness — it saves many tokens and has clear semantics (propagate error), so it is worth the learning cost.
- Short variable names (`s`, `t`, `r`) are clever but not clearer. The TYPE-SYSTEM.md research shows they save characters but not tokens. Go's convention of short-but-meaningful names (`err`, `ctx`, `req`) is better for generation accuracy.

### Principle: "Don't communicate by sharing memory; share memory by communicating"

Go's concurrency mantra. For ilo, the equivalent is: **functions communicate through return values and tool calls, not shared state.**

ilo already enforces this:
- No global variables
- No mutable shared state
- Functions declare explicit dependencies
- Tool calls return results, not side effects (except for the tool's external effects)

This is stronger than Go — Go allows shared state with mutexes. ilo forbids it entirely.

### Principle: "A little copying is better than a little dependency"

Go's stdlib philosophy: inline small amounts of code rather than importing packages. For ilo, this translates to: **builtins over tool dependencies for common operations.**

JSON parsing (`jp`), HTTP GET (`get`), env vars (`env`), and hashing (`hash`) should be builtins, not tools. The token cost of declaring a tool, calling it, and handling its result is higher than a builtin. Go stdlib's lesson: if every program needs it, put it in the language.

### Principle: "Errors are values"

Go's most important design choice. In ilo, `R ok err` IS "errors are values." But ilo goes further:
- The `!` operator eliminates Go's `if err != nil` boilerplate
- The `?` match operator forces explicit handling
- The verifier checks that `R` return types are matched exhaustively

This is Go's error philosophy perfected for agents.

### Principle: "`gofmt` — one true format"

Go has exactly one code format. `gofmt` is non-configurable. This eliminates all formatting debates and makes all Go code look the same.

ilo takes this further:
- Dense format is canonical — `dense(parse(dense(parse(src)))) == dense(parse(src))`
- No formatting choices — no indentation preferences, no brace placement options
- The agent never wastes tokens on formatting decisions

Go's `gofmt` eliminated human formatting debates. ilo's dense format eliminates formatting entirely.

### Principle: "Unused imports are errors"

Go refuses to compile with unused imports or variables. This is unusually strict for a mainstream language. ilo should adopt the same philosophy:
- The verifier already checks that all calls resolve and all types exist
- **Consider adding:** unused variable detection (warn or error when a bound variable is never referenced)
- **Consider adding:** unused function detection (warn when a function is defined but never called)

For agents, unused code is wasted tokens — both in generation and in context loading. Go's strict approach is correct.

---

## Summary: What ilo Should Take from Go

### Take now (aligns with current roadmap)

| Go feature | ilo equivalent | Priority |
|-----------|---------------|----------|
| `os.ReadFile` / `os.WriteFile` one-liners | `fread` / `fwrite` (G8) | High |
| `json.Unmarshal` struct tag mapping | Record-guided JSON parsing (D1e + I1) | Critical |
| `http.Get` convenience wrapper | Already done: `get` / `$` | Done |
| `if err != nil` → return err | Already done: `!` auto-unwrap | Done |
| `fmt.Sprintf` | `fmt` builtin (I4) | High |
| `os.Getenv` / `os.LookupEnv` | `env key` (I3) | Critical |
| `crypto/sha256` + `crypto/hmac` | `hash` / `hmac` (I9) | Medium |
| `time.Now().Unix()` + `time.Sleep` | `now()` / `sleep n` (I6) | Medium |
| `gofmt` single format | Already done: dense format | Done |
| `errors` explicit handling | Already done: `R` + `?` + `!` | Done |

### Take selectively (adapt to ilo's context)

| Go feature | ilo adaptation | Notes |
|-----------|---------------|-------|
| Goroutines + WaitGroup | `par{...}` block (G4) | Hide concurrency behind a block |
| `select` multiplexing | `poll conns timeout` (G9) | Hide channels behind a builtin |
| `strings` package (40+ funcs) | Add `trim`, `sub`, maybe `upper`/`lower` | Only the highest-frequency operations |
| Generics (1.18) | Defer to E5, ship without | Follow Go's "wait and see" |
| Maps | `M t v` type (E4) | With `??` for missing-key handling |

### Do NOT take (wrong for ilo)

| Go feature | Why not |
|-----------|---------|
| Goroutines as user-visible | Deadlocks are undiagnosable for agents |
| Channels | Too complex, wrong concurrency model for tools |
| Interfaces (implicit) | Too implicit for constrained world |
| `any` type | Defeats the type system |
| `flag` package | Functions params are the CLI interface |
| `http.ListenAndServe` | Agents consume services, not serve them |
| `filepath` package | Path manipulation is OS concern, tool concern |
| XML/CSV parsers | Tool concern, not language concern |
| `crypto/aes` / encryption | Dangerous to expose, tool concern |
| Time formatting (`time.Format`) | Tool concern, not common enough for a builtin |
| `make()` constructor | Literals (`[]`, `M{}`) are shorter |

---

## Appendix: Token Comparison — Go vs ilo for Common Agent Tasks

### Task 1: Fetch JSON API and extract field

```go
// Go: ~18 tokens
resp, err := http.Get(url)
if err != nil { return err }
body, _ := io.ReadAll(resp.Body)
var data map[string]interface{}
json.Unmarshal(body, &data)
name := data["name"].(string)
```

```
-- ilo (with planned jp): ~4 tokens
n=jp! ($!url) "name"
```

### Task 2: Read env var, call API, handle error

```go
// Go: ~20 tokens
key := os.Getenv("API_KEY")
if key == "" { return errors.New("API_KEY not set") }
resp, err := http.Get("https://api.example.com?key=" + key)
if err != nil { return fmt.Errorf("API call failed: %w", err) }
body, _ := io.ReadAll(resp.Body)
```

```
-- ilo (planned): ~8 tokens
k=env! "API_KEY";r=get! +"https://api.example.com?key=" k;r
```

### Task 3: Process list of items

```go
// Go: ~15 tokens
var results []string
for _, item := range items {
    if item.Score >= 100 {
        results = append(results, item.Name)
    }
}
```

```
-- ilo: ~8 tokens
@i its{>=i.score 100{ret i.name}}
```

### Task 4: Parallel API calls (with planned par)

```go
// Go: ~30 tokens
var wg sync.WaitGroup
results := make([]string, len(urls))
for i, url := range urls {
    wg.Add(1)
    go func(i int, u string) {
        defer wg.Done()
        resp, _ := http.Get(u)
        body, _ := io.ReadAll(resp.Body)
        results[i] = string(body)
    }(i, url)
}
wg.Wait()
```

```
-- ilo (planned): ~5 tokens
par{get u1;get u2;get u3}
```

The pattern is consistent: ilo achieves 3-5x token reduction vs Go for common agent tasks, while maintaining equivalent safety guarantees through `R` types, `!` auto-unwrap, and verification before execution. Go's design philosophy validates ilo's choices; ilo pushes Go's principles to their token-minimal conclusion.
