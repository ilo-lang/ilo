# JavaScript/TypeScript Ecosystem Research for ilo

Catalog of JS/TS primitives relevant to an AI agent programming language. Evaluated against ilo's design metric: **total tokens from intent to working code**.

---

## 1. File I/O

Three runtimes, three APIs. All async-first.

### Node.js (`node:fs`)

```js
import { readFile, writeFile, mkdir, readdir, stat, rm } from 'node:fs/promises';
const text = await readFile('file.txt', 'utf-8');
await writeFile('out.txt', 'hello');
await mkdir('dir', { recursive: true });
const entries = await readdir('dir');
const info = await stat('file.txt');
await rm('dir', { recursive: true });
```

Sync variants exist (`readFileSync`, etc.) — block the event loop. Stream API for large files: `createReadStream`, `createWriteStream`.

### Deno (`Deno.*`)

```js
const text = await Deno.readTextFile('file.txt');
await Deno.writeTextFile('out.txt', 'hello');
await Deno.mkdir('dir', { recursive: true });
for await (const entry of Deno.readDir('dir')) { ... }
const info = await Deno.stat('file.txt');
await Deno.remove('dir', { recursive: true });
```

Requires `--allow-read` / `--allow-write` permissions. Also supports `node:fs` via compat layer.

### Bun (`Bun.file` / `Bun.write`)

```js
const text = await Bun.file('file.txt').text();
const json = await Bun.file('data.json').json();
const bytes = await Bun.file('img.png').arrayBuffer();
await Bun.write('out.txt', 'hello');
await Bun.write('out.bin', new Uint8Array([1, 2, 3]));
```

`Bun.file()` returns a lazy `BunFile` conforming to the `Blob` interface — `.text()`, `.json()`, `.stream()`, `.arrayBuffer()`, `.bytes()`. For `mkdir`, `readdir`, `stat`, `rm` — use `node:fs` compat (nearly complete).

### What an agent needs

| Operation | Frequency | ilo mapping |
|-----------|-----------|-------------|
| Read file as text | Very high | Could be a builtin: `read path` > `R t t` |
| Write text to file | Very high | Could be a builtin: `write path data` > `R _ t` |
| List directory | Medium | Tool |
| File metadata (exists, size) | Medium | Tool |
| Create/delete dirs | Low | Tool |
| Stream large files | Low | Tool (agents work on small data) |

**ilo implication:** File I/O maps to 2 builtins (`read`, `write`) returning `R` types. Directory operations stay as tools. Bun's `Bun.file(path).text()` is the cleanest API shape — lazy reference, then read. But for ilo, a flat `read path` is more token-efficient.

---

## 2. Networking

### `fetch` (universal)

Available in all three runtimes. Web standard. The primary HTTP client.

```js
const res = await fetch(url);
const body = await res.text();           // or .json(), .arrayBuffer()
const ok = res.ok;                       // true if 200-299
const status = res.status;               // numeric status code

// POST with headers
const res = await fetch(url, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token}` },
  body: JSON.stringify(data),
});
```

### Node.js `http` / `https` modules

Lower-level. Rarely needed when `fetch` exists. Used for custom servers:

```js
import http from 'node:http';
http.createServer((req, res) => { res.end('hello'); }).listen(3000);
```

### Deno / Bun HTTP servers

```js
// Deno
Deno.serve({ port: 3000 }, (req) => new Response('hello'));

// Bun
Bun.serve({ port: 3000, fetch: (req) => new Response('hello') });
```

Both use web-standard `Request`/`Response` objects. Bun: ~52k req/sec, Deno: ~29k, Node: ~14k in benchmarks.

### WebSocket

```js
// Client (all runtimes)
const ws = new WebSocket('ws://host/path');
ws.onmessage = (e) => console.log(e.data);
ws.send('hello');

// Server: runtime-specific or use `ws` npm package
```

### Server-Sent Events (SSE)

Critical for AI agents — this is how LLM APIs stream responses.

```js
// Client via fetch + ReadableStream (works everywhere)
const res = await fetch(url);
const reader = res.body.getReader();
const decoder = new TextDecoder();
while (true) {
  const { done, value } = await reader.read();
  if (done) break;
  const chunk = decoder.decode(value);
  // parse SSE format: "data: ...\n\n"
}

// Browser: EventSource API (GET only)
const es = new EventSource(url);
es.onmessage = (e) => console.log(e.data);
```

`EventSource` is browser-only, GET-only. For LLM APIs (POST + auth headers), use `fetch` + streaming. Libraries like `fetch-event-stream` wrap the SSE parsing.

### What an agent needs

| Operation | Frequency | ilo mapping |
|-----------|-----------|-------------|
| HTTP GET | Very high | `get url` (already builtin) |
| HTTP POST with JSON body | Very high | Needs new builtin or tool |
| Set headers (auth tokens) | High | Part of POST/GET config |
| Stream SSE responses | High (LLM calls) | Tool or specialized builtin |
| WebSocket | Medium | Tool |
| Serve HTTP | Low | Tool |

**ilo implication:** `get` already exists. A `post url body` builtin returning `R t t` covers the most common agent task (calling APIs). Headers could be a config concern. SSE streaming is important for LLM-in-the-loop patterns but may be better as a tool than a language primitive.

---

## 3. Process Execution

### Node.js (`child_process`)

```js
import { exec, execSync, spawn, spawnSync } from 'node:child_process';

// Simple: get output as string
const { stdout } = execSync('ls -la', { encoding: 'utf-8' });

// Async with streams
const child = spawn('git', ['status']);
child.stdout.on('data', (data) => console.log(data.toString()));
child.on('close', (code) => console.log(`exit: ${code}`));
```

`exec` runs in a shell, `spawn` runs directly. `fork` creates Node.js child processes with IPC. Node is notably slow at spawning processes.

### Deno (`Deno.Command`)

```js
const cmd = new Deno.Command('ls', { args: ['-la'] });
const { stdout, stderr, code } = await cmd.output();
const text = new TextDecoder().decode(stdout);

// Sync variant
const { stdout } = cmd.outputSync();
```

Requires `--allow-run` permission. Output is `Uint8Array` — must decode manually.

### Bun (`Bun.spawn` / `Bun.$`)

```js
// Direct spawn
const proc = Bun.spawn(['ls', '-la']);
const text = await new Response(proc.stdout).text();

// Bun Shell — the killer feature
import { $ } from 'bun';
const output = await $`ls -la`.text();
await $`echo "hello" > file.txt`;
await $`cat file.txt | grep hello`;
```

**Bun Shell `$` syntax** is a tagged template literal that:
- Escapes interpolations automatically (prevents injection)
- Supports pipes, redirects, globs natively
- Cross-platform (Windows/Linux/macOS)
- Built-in commands (`ls`, `cd`, `rm`) implemented natively
- 60% faster process spawning than Node.js

```js
// Variable interpolation (safe — auto-escaped)
const name = "world";
await $`echo ${name}`;

// Capture output
const result = await $`git status`.text();

// Error handling — non-zero exit throws
try {
  await $`false`;
} catch (e) {
  console.log(e.exitCode); // 1
}

// Quiet mode
await $`noisy-command`.quiet();
```

### What an agent needs

| Operation | Frequency | ilo mapping |
|-----------|-----------|-------------|
| Run shell command, get output | Very high | Builtin: `sh "cmd"` > `R t t` |
| Run with args (safe) | High | `exec cmd args` > `R t t` |
| Pipe commands | Medium | Tool or shell string |
| Background processes | Low | Tool |

**ilo implication:** Bun's `$` is brilliant for human developers but too complex for a language primitive. A simple `sh "git status"` > `R t t` builtin covers 90% of agent shell needs. The `exec` variant with separate args prevents injection without template literal machinery.

---

## 4. Data Formats

### JSON (native)

```js
const obj = JSON.parse('{"a": 1}');
const str = JSON.stringify(obj);
const pretty = JSON.stringify(obj, null, 2);
```

Universal across all runtimes. The only format with native support.

### XML

No built-in parser in Node.js/Bun/Deno. Options:
- Browser: `DOMParser` (native, DOM tree)
- Node.js: `fast-xml-parser` (3.6k dependents, pure JS, fast), `xml2js` (legacy)
- `txml`: 3-5x faster than `fast-xml-parser`, pure JS, works everywhere

### CSV

No built-in. Options:
- `csv-parse`: streaming, battle-tested
- `papaparse`: browser + Node, auto-detect delimiters
- Manual: `text.split('\n').map(line => line.split(','))` — works for simple cases

### What an agent needs

ilo's OPEN.md already states: **"Format parsing is a tool concern, not a language concern."** This is correct.

| Format | Agent frequency | ilo approach |
|--------|----------------|--------------|
| JSON parse/stringify | Very high | Builtin at tool boundary (JSON <-> Value mapping) |
| XML | Low | Tool |
| CSV | Medium | Tool, or `spl` builtin on text |
| YAML | Low | Tool |
| HTML | Medium | Tool (e.g., `fetch url "css-selector"`) |

**ilo implication:** JSON is the one essential format. ilo already handles it at the tool boundary — tool declarations define the schema, the runtime maps JSON to typed records. XML/CSV/HTML parsing stays in tools. This matches the Unix philosophy: `curl | jq` = `get url;?{...}`. The parsing complexity lives in the tool, not the composition language.

---

## 5. String Manipulation

### Regex

```js
const re = /^(\d{4})-(\d{2})-(\d{2})$/;
const match = re.exec('2025-01-15');      // ['2025-01-15', '2025', '01', '15']
const found = 'hello'.match(/l+/g);       // ['ll']
const replaced = 'foo bar'.replace(/foo/, 'baz');
const test = /\d+/.test('abc123');        // true
```

Full PCRE-style regex. Named groups: `/(?<year>\d{4})/`. LLMs are good at generating regex patterns.

### Template literals

```js
const msg = `Hello ${name}, you have ${count} items`;
const multiline = `line 1
line 2`;
// Tagged templates (Bun Shell uses this)
const html = html`<div>${content}</div>`;
```

### Encoding APIs

```js
// Base64
const encoded = btoa('hello');           // 'aGVsbG8='
const decoded = atob('aGVsbG8=');        // 'hello'
// Note: btoa/atob don't handle Unicode — use TextEncoder for that

// TextEncoder / TextDecoder (UTF-8)
const encoder = new TextEncoder();
const bytes = encoder.encode('hello');    // Uint8Array
const decoder = new TextDecoder();
const str = decoder.decode(bytes);        // 'hello'

// Node.js Buffer
const buf = Buffer.from('hello', 'utf-8');
const b64 = buf.toString('base64');
const hex = buf.toString('hex');

// New: Uint8Array.prototype.toBase64() (2025+)
const b64 = new Uint8Array([1,2,3]).toBase64();
```

### String methods

```js
str.includes(sub)        str.startsWith(pre)       str.endsWith(suf)
str.indexOf(sub)         str.slice(start, end)     str.trim()
str.trimStart()          str.trimEnd()             str.padStart(len, char)
str.padEnd(len, char)    str.repeat(n)             str.split(sep)
str.toUpperCase()        str.toLowerCase()         str.replaceAll(old, new)
str.at(index)            str.normalize()
```

### What an agent needs

| Operation | Frequency | ilo mapping |
|-----------|-----------|-------------|
| Split / join | Very high | `spl` / `cat` (already builtins) |
| Contains / includes | High | `has` (already builtin) |
| Slice | High | `slc` (already builtin) |
| Replace / regex | Medium | Could be builtin: `rep text pattern replacement` |
| Case conversion | Medium | Could be builtin: `up text` / `low text` |
| Trim | Medium | Could be builtin: `trm text` |
| Base64 encode/decode | Low-Medium | Tool or builtin |
| Template interpolation | N/A | ilo uses `+` concat for text |

**ilo implication:** ilo already has `spl`, `cat`, `has`, `slc`, `hd`, `tl`, `rev`. The main gaps for agents: regex matching (`mtc text pattern`) and text replacement (`rep text pattern replacement`). Template literals are unnecessary — `+` concat is more token-efficient in prefix notation. Base64 could be a tool.

---

## 6. Concurrency

### Promises & async/await

```js
// Promise creation
const p = new Promise((resolve, reject) => {
  setTimeout(() => resolve(42), 1000);
});

// async/await (sugar over Promises)
async function fetchData() {
  const res = await fetch(url);
  return await res.json();
}

// Combinators
const [a, b] = await Promise.all([fetchA(), fetchB()]);     // parallel, all must succeed
const first = await Promise.race([fetchA(), fetchB()]);      // first to settle
const results = await Promise.allSettled([a(), b(), c()]);   // all complete, no throw
const first = await Promise.any([a(), b()]);                 // first success
```

### Worker Threads (Node.js) / Web Workers

```js
// Node.js
import { Worker, isMainThread, parentPort, workerData } from 'node:worker_threads';

if (isMainThread) {
  const worker = new Worker('./worker.js', { workerData: { input: 42 } });
  worker.on('message', (result) => console.log(result));
} else {
  const result = heavyComputation(workerData.input);
  parentPort.postMessage(result);
}

// SharedArrayBuffer for zero-copy sharing
const shared = new SharedArrayBuffer(1024);
const arr = new Int32Array(shared);
Atomics.store(arr, 0, 42);
Atomics.load(arr, 0);  // 42
Atomics.wait(arr, 0, 42);   // block until value changes
Atomics.notify(arr, 0, 1);  // wake one waiter
```

### What an agent needs

| Pattern | Frequency | ilo mapping |
|---------|-----------|-------------|
| Sequential calls (await) | Very high | Default — ilo is sequential |
| Parallel independent calls | High | `all [call1, call2]` or automatic from DAG |
| First-success racing | Low | Tool |
| Background workers | Low | Not needed — agents are task-oriented |
| Shared memory / Atomics | Very low | Not needed |

**ilo implication:** ilo is synchronous by design — tool calls block. The one valuable concurrency primitive is `Promise.all` — running independent tool calls in parallel. This maps to ilo's graph-native principle: if two calls have no data dependency, execute them in parallel. Could be expressed as `all [call1 arg, call2 arg]` or inferred from the dependency graph.

---

## 7. Error Handling

### try/catch/finally

```js
try {
  const data = JSON.parse(input);
  return process(data);
} catch (e) {
  if (e instanceof SyntaxError) { /* JSON parse error */ }
  if (e instanceof TypeError) { /* type mismatch */ }
  console.error(e.message, e.stack);
  throw new Error('processing failed', { cause: e });
} finally {
  cleanup();
}
```

### Error types

Built-in: `Error`, `TypeError`, `RangeError`, `ReferenceError`, `SyntaxError`, `URIError`, `EvalError`, `AggregateError`.

Custom errors:
```js
class ApiError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}
```

### Error chaining (ES2022)

```js
throw new Error('high-level msg', { cause: originalError });
```

### What an agent needs

ilo's `R ok err` + `?` match + `!` auto-unwrap already covers this comprehensively:

```
get-user uid;?{^e:^+"Lookup failed: "e;~d:use d}
```

This is equivalent to try/catch but:
- Typed (verifier checks exhaustiveness)
- Composable (error propagation via `^`)
- Token-efficient (`!` auto-unwrap = try/catch in 1 token)
- No exception unwinding (errors are values, not control flow)

**ilo implication:** No changes needed. ilo's error handling is already superior to try/catch for agent use. The `cause` chaining pattern maps to `^+"context: "e` — prepending context to error messages.

---

## 8. Environment

### Node.js

```js
process.env.API_KEY                 // environment variable
process.env.HOME                    // home directory
process.argv                        // command-line args (array of strings)
process.cwd()                       // current working directory
process.exit(1)                     // exit with code
process.platform                    // 'linux', 'darwin', 'win32'
process.version                     // 'v22.0.0'
```

### Deno

```js
Deno.env.get('API_KEY')             // get env var
Deno.env.set('KEY', 'value')        // set env var
Deno.args                           // command-line args (after --)
Deno.cwd()                          // current working directory
Deno.exit(1)                        // exit with code
```

Requires `--allow-env` permission for env access.

### Bun

```js
Bun.env.API_KEY                     // or process.env.API_KEY
Bun.argv                            // command-line args
process.cwd()                       // CWD (Node compat)
```

### What an agent needs

| Operation | Frequency | ilo mapping |
|-----------|-----------|-------------|
| Read env var | Very high | Builtin: `env "API_KEY"` > `O t` |
| CLI arguments | High | Already supported: `process.argv` equivalent via CLI args |
| Current directory | Medium | Builtin or tool |
| Exit with code | Low | Implicit from program result |

**ilo implication:** `env "KEY"` returning `O t` (optional text — key might not exist) is the main gap. CLI args are already handled by ilo's CLI. Agent tasks frequently need API keys and config from environment.

---

## 9. Cryptography

### Web Crypto API (universal)

```js
// Hashing
const hash = await crypto.subtle.digest('SHA-256', new TextEncoder().encode('hello'));
const hex = [...new Uint8Array(hash)].map(b => b.toString(16).padStart(2, '0')).join('');

// HMAC
const key = await crypto.subtle.generateKey({ name: 'HMAC', hash: 'SHA-256' }, true, ['sign']);
const sig = await crypto.subtle.sign('HMAC', key, data);

// Encrypt/Decrypt (AES-GCM)
const key = await crypto.subtle.generateKey({ name: 'AES-GCM', length: 256 }, true, ['encrypt', 'decrypt']);
const iv = crypto.getRandomValues(new Uint8Array(12));
const ciphertext = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, plaintext);

// Random
crypto.randomUUID();                        // 'f47ac10b-58cc-4372-...'
crypto.getRandomValues(new Uint8Array(16)); // random bytes
```

Available in all three runtimes. Node also has its legacy `crypto` module with stream-based APIs.

### What an agent needs

| Operation | Frequency | ilo mapping |
|-----------|-----------|-------------|
| Generate UUID | High | Builtin: `uuid()` > `t` |
| Hash (SHA-256) | Medium | Builtin: `hash text` > `t` |
| Random number | Medium | Builtin: `rnd min max` > `n` |
| HMAC signing | Low | Tool |
| Encrypt/decrypt | Low | Tool |

**ilo implication:** UUID generation and hashing are common enough for builtins. Full cryptographic operations are tool territory — too complex and error-prone for language primitives.

---

## 10. Time / Date

### `Date` (legacy)

```js
const now = Date.now();                    // milliseconds since epoch
const d = new Date();
d.toISOString();                           // '2025-01-15T10:30:00.000Z'
d.getFullYear(); d.getMonth();             // month is 0-indexed (!)
```

Notorious problems: mutable, 0-indexed months, no timezone support, bad parsing.

### `Temporal` API (ES2026 — shipping)

Firefox 139 (May 2025) shipped first. Chrome 144 (Jan 2026) followed. Now in production browsers.

```js
const now = Temporal.Now.instant();                           // exact moment
const date = Temporal.PlainDate.from('2025-01-15');          // date only
const time = Temporal.PlainTime.from('10:30:00');            // time only
const dt = Temporal.PlainDateTime.from('2025-01-15T10:30'); // date + time
const zdt = Temporal.ZonedDateTime.from('2025-01-15T10:30[America/New_York]');
const dur = Temporal.Duration.from({ hours: 2, minutes: 30 });

// Arithmetic
const tomorrow = date.add({ days: 1 });
const diff = date1.until(date2);          // Duration

// Comparison
Temporal.PlainDate.compare(a, b);         // -1, 0, 1
```

Key properties: immutable, explicit types for different use cases, 1-indexed months, IANA timezone support, nanosecond precision.

### What an agent needs

| Operation | Frequency | ilo mapping |
|-----------|-----------|-------------|
| Current timestamp (epoch ms) | High | Builtin: `now()` > `n` |
| ISO format current time | Medium | Builtin: `time()` > `t` |
| Parse date string | Low-Medium | Tool |
| Date arithmetic | Low | Tool |
| Timezone conversion | Low | Tool |

**ilo implication:** `now()` (epoch milliseconds as number) and `time()` (ISO string) cover most agent needs. Complex date manipulation is tool territory.

---

## 11. Collections

### Array

```js
// Creation
const arr = [1, 2, 3];
const range = Array.from({ length: 10 }, (_, i) => i);  // [0..9]

// Functional
arr.map(x => x * 2)              arr.filter(x => x > 1)
arr.reduce((acc, x) => acc + x)  arr.find(x => x > 1)
arr.some(x => x > 5)             arr.every(x => x > 0)
arr.flatMap(x => [x, x])         arr.flat(depth)

// Mutation
arr.push(4)    arr.pop()    arr.shift()    arr.unshift(0)
arr.splice(1, 1)            arr.sort((a, b) => a - b)
arr.reverse()

// Access
arr[0]         arr.at(-1)   arr.includes(2)
arr.indexOf(2) arr.slice(1, 3)
```

### Map / Set

```js
const m = new Map();
m.set('key', 'value');  m.get('key');  m.has('key');  m.delete('key');
m.size;  m.keys();  m.values();  m.entries();

const s = new Set([1, 2, 3]);
s.add(4);  s.has(2);  s.delete(1);  s.size;
// Set operations (ES2025)
s.union(other);  s.intersection(other);  s.difference(other);
```

### WeakMap / WeakRef

For garbage-collection-friendly caches. Not relevant for agent programs.

### Typed Arrays

```js
const buf = new ArrayBuffer(16);
const i32 = new Int32Array(buf);
const f64 = new Float64Array(buf);
const u8 = new Uint8Array(buf);
```

For binary data manipulation. Used with WebSocket, crypto, file I/O for binary formats.

### What an agent needs

| Collection | Agent frequency | ilo mapping |
|------------|----------------|-------------|
| List (Array) | Very high | `L n`, `L t` — already exists |
| Map (key-value) | High | Phase E: `M t n` — planned |
| Set | Low | Can be simulated with list + dedup |
| Typed arrays | Very low | Not needed |

**ilo implication:** Lists exist. Maps are the major gap (planned as E4). The functional operations (`map`, `filter`, `reduce`) gate on generics + lambdas (E5). Until then, `@` iteration + guards cover most cases:

```
-- "filter to items > 10"
r=[];@x xs{>x 10{+=r x}};r

-- "sum"
s=0;@x xs{s=+s x};s
```

---

## 12. TypeScript Type System

### Core types

```ts
// Primitives
let n: number;          let s: string;           let b: boolean;
let u: undefined;       let v: void;             let a: any;
let nv: never;          let uk: unknown;

// Literal types
let dir: 'north' | 'south' | 'east' | 'west';
let code: 200 | 404 | 500;

// Object types
interface User { id: string; name: string; email?: string; }
type Point = { x: number; y: number };

// Function types
type Handler = (req: Request) => Response;
type AsyncFn = (id: string) => Promise<User>;
```

### Generics

```ts
function first<T>(arr: T[]): T | undefined { return arr[0]; }
function map<T, U>(arr: T[], fn: (item: T) => U): U[] { ... }

// Constrained generics
function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] { ... }

// Generic interfaces
interface Result<T, E = Error> { ok: boolean; value?: T; error?: E; }
```

### Utility types

```ts
Partial<T>            // all fields optional
Required<T>           // all fields required
Pick<T, K>            // subset of fields
Omit<T, K>            // exclude fields
Record<K, V>          // key-value object type
Readonly<T>           // all fields readonly
ReturnType<F>         // infer return type of function
Parameters<F>         // infer parameter types as tuple
Awaited<T>            // unwrap Promise type
Extract<T, U>         // types in T assignable to U
Exclude<T, U>         // types in T not assignable to U
NonNullable<T>        // remove null/undefined
```

### Conditional types

```ts
type IsString<T> = T extends string ? true : false;
type ElementType<T> = T extends Array<infer E> ? E : never;
type ApiResponse<T> = T extends 'user' ? User : T extends 'post' ? Post : never;
```

### Mapped types

```ts
type Nullable<T> = { [K in keyof T]: T[K] | null };
type ReadonlyRecord<T> = { readonly [K in keyof T]: T[K] };
```

### What an agent needs

TypeScript's type system is extremely expressive — far more than ilo needs. The relevant subset:

| TS feature | Agent frequency | ilo equivalent |
|------------|----------------|----------------|
| Basic types (number, string, bool) | Very high | `n`, `t`, `b` |
| Null/undefined handling | Very high | `_` (nil), `O n` (optional, planned) |
| Union types (Result pattern) | Very high | `R ok err` |
| Interfaces / object types | High | `type name{fields}` (records) |
| Arrays with element types | High | `L n`, `L t` |
| Generics | Medium | Planned (E5) |
| Utility types | Low | Not needed — agents don't build abstractions |
| Conditional/mapped types | Very low | Not needed |
| Literal types / discriminated unions | Medium | Planned as sum types (E3) |

**ilo implication:** ilo's monomorphic type system is appropriate. TypeScript's generics exist because humans write reusable libraries. Agents generate fresh programs per task — they rarely need `Partial<Pick<User, 'name' | 'email'>>`. ilo's approach (concrete types, `R` for errors, records for structure) covers the agent use case without the token cost of TS's type-level computation.

---

## What Bun and Deno Add That Node Doesn't

### Bun additions

| Feature | Impact for agents |
|---------|-------------------|
| **`Bun.$` shell syntax** | Tagged template for cross-platform shell commands with safe interpolation. Beautiful API but complex for a language primitive. |
| **`Bun.file()` / `Bun.write()`** | Lazy file references with Blob interface. Clean shape. |
| **`Bun.serve()`** | One-line HTTP server with web-standard Request/Response. |
| **Built-in TypeScript** | No compilation step — `bun run file.ts` just works. |
| **Built-in bundler** | `bun build` — no webpack/esbuild needed. |
| **Built-in test runner** | `bun test` with Jest-compatible API. |
| **Built-in SQLite** | `bun:sqlite` — embedded database with zero deps. |
| **60% faster process spawning** | Uses `posix_spawn(3)` — matters for shell-heavy agents. |
| **JavaScriptCore engine** | Faster startup than V8 (~5ms vs ~25ms). |
| **Acquired by Anthropic (Dec 2025)** | Powers Claude Code and Claude Agent SDK. Direct relevance. |

### Deno additions

| Feature | Impact for agents |
|---------|-------------------|
| **Permission model** | `--allow-read`, `--allow-net`, `--allow-env`, `--allow-run`. Sandboxes untrusted code — critical for running agent-generated programs. |
| **Permission sets in config** (2.5+) | Define permissions in `deno.json` — declarative security. |
| **Permission broker** | External process can dynamically grant/deny permissions at runtime. |
| **Permission audit logging** | Track what permissions were requested — observability for agent execution. |
| **Built-in TypeScript** | Native TS execution, no compilation step. |
| **URL-based imports** | No `node_modules` — import from URLs directly. |
| **Built-in formatter + linter** | `deno fmt`, `deno lint` — zero config. |
| **Web-standard APIs** | `fetch`, `Request`, `Response`, `WebSocket` — same API as browsers. |
| **Deno Deploy** | Edge deployment platform. |

### Key insight for ilo

**Deno's permission model is the most relevant innovation for agent-generated code.** When an agent writes a program, that program should not have ambient access to the filesystem, network, and environment. Deno's approach — deny by default, grant explicitly — is exactly right for sandboxing agent output.

ilo's tool declarations already embed this thinking: `tool get-user"..." uid:t>R profile t timeout:5,retry:2`. The tool declaration IS a permission grant — "this program can call get-user with these parameters." The closed-world verifier ensures no undeclared tools are called.

**Bun's `$` shell syntax is the most relevant API innovation for developer experience,** but it's a human-facing feature. For agents, a simpler `sh "cmd"` > `R t t` achieves the same result with fewer tokens.

---

## Bun's `$` Shell Syntax — Deep Dive

The Bun Shell is an embedded cross-platform shell interpreter:

```js
import { $ } from 'bun';

// Basic execution
await $`echo hello`;

// Safe interpolation (auto-escaped)
const file = 'my file.txt';
await $`cat ${file}`;             // properly quoted

// Output capture
const text = await $`ls -la`.text();
const lines = await $`ls`.lines();
const json = await $`cat data.json`.json();

// Environment variables
await $`echo $HOME`;                         // reads env
await $`cmd`.env({ API_KEY: 'secret' });     // per-command env

// Pipes and redirects
await $`cat file.txt | grep pattern | wc -l`;
await $`echo hello > output.txt`;

// Globs
await $`ls **/*.ts`;

// Error handling
const result = await $`may-fail`.nothrow();  // don't throw on non-zero
console.log(result.exitCode);

// Built-in commands (no external process needed)
// cd, ls, rm, mv, cp, cat, echo, pwd, which, mkdir, touch
```

Key properties:
- Cross-platform (Windows MSYS included)
- Template literal tag prevents shell injection
- Built-in commands are Zig-native (faster than spawning processes)
- Glob support built-in
- `.text()`, `.lines()`, `.json()` for output parsing
- `.env()` for per-command environment

---

## Deno's Permission Model — Deep Dive

```bash
# All permissions
deno run -A script.ts

# Specific permissions
deno run --allow-read=/data --allow-net=api.example.com script.ts

# Permission flags
--allow-read[=path,...]      # filesystem read
--allow-write[=path,...]     # filesystem write
--allow-net[=host:port,...]  # network access
--allow-env[=VAR,...]        # environment variables
--allow-run[=cmd,...]        # subprocess execution
--allow-ffi                  # dynamic libraries (dangerous)
--allow-sys                  # system info (hostname, OS, etc.)
```

**Config file (Deno 2.5+):**
```json
{
  "permissions": {
    "allow-read": ["/data", "/config"],
    "allow-net": ["api.example.com"],
    "allow-env": ["API_KEY", "DATABASE_URL"]
  }
}
```

**Permission broker (advanced):**
An external process can intercept permission requests and grant/deny them dynamically. Enables:
- Audit logging of all permission requests
- Policy-based access control
- Interactive approval workflows

**Runtime permission API:**
```js
const status = await Deno.permissions.query({ name: 'read', path: '/etc' });
// status.state: 'granted' | 'denied' | 'prompt'

const result = await Deno.permissions.request({ name: 'net', host: 'api.example.com' });
```

---

## What Would an Agent Actually Use? — Priority Summary

Ranked by frequency in real-world agent tasks:

### Tier 1: Use constantly (builtin candidates)

| Capability | JS/TS API | ilo status | Agent use case |
|------------|-----------|------------|----------------|
| HTTP GET | `fetch` / `get` | Done (`get`/`$`) | API calls, data retrieval |
| HTTP POST | `fetch` | **Gap** | API calls with payloads, LLM calls |
| JSON parse/stringify | `JSON.parse/stringify` | Done (tool boundary) | Data interchange |
| File read | `fs.readFile` / `Bun.file` | **Gap** | Config, data files, code |
| File write | `fs.writeFile` / `Bun.write` | **Gap** | Output, logs, generated files |
| Shell command | `child_process.exec` / `Bun.$` | **Gap** | git, build tools, system commands |
| Env vars | `process.env` | **Gap** | API keys, config |
| String split/join | `str.split/join` | Done (`spl`/`cat`) | Text processing |
| Error handling | `try/catch` | Done (`R`/`?`/`!`) | Every fallible operation |

### Tier 2: Use often (builtin or tool)

| Capability | JS/TS API | ilo status | Agent use case |
|------------|-----------|------------|----------------|
| Regex match/replace | `RegExp` | **Gap** | Text extraction, validation |
| UUID generation | `crypto.randomUUID()` | **Gap** | Creating identifiers |
| Timestamp | `Date.now()` | **Gap** | Logging, timing |
| Hash (SHA-256) | `crypto.subtle.digest` | **Gap** | Checksums, dedup |
| Map/dict | `Map` / `Record` | Planned (E4) | Key-value data |
| Parallel calls | `Promise.all` | **Gap** | Independent API calls |
| Base64 | `btoa`/`atob` | **Gap** | Auth headers, data encoding |

### Tier 3: Use occasionally (tools only)

| Capability | JS/TS API | ilo status |
|------------|-----------|------------|
| WebSocket | `WebSocket` | Tool |
| SSE streaming | `fetch` + `ReadableStream` | Tool |
| HTTP server | `http.createServer` / `Bun.serve` | Tool |
| XML/HTML parsing | `DOMParser` / `fast-xml-parser` | Tool |
| CSV parsing | `csv-parse` / `papaparse` | Tool |
| Date arithmetic | `Temporal` / `Date` | Tool |
| Workers/threads | `Worker` / `worker_threads` | Not needed |
| Encrypt/decrypt | `crypto.subtle` | Tool |
| Process management | `spawn` with streams | Tool |

### Tier 4: Not needed for agents

| Capability | Why not |
|------------|---------|
| Shared memory / Atomics | Agents are task-oriented, not compute-parallel |
| WeakMap / WeakRef | GC optimization — not a concern |
| Typed arrays | Binary protocol handling — agents deal in text/JSON |
| Conditional types / mapped types | Human abstraction tooling |
| Module system (import/export) | ilo uses declarations, not modules |
| Class syntax | OOP machinery — agents compose functions |
| Proxy / Reflect | Metaprogramming — too complex, too error-prone |
| Generators / iterators | Async iteration — `@` loop is sufficient |

---

## Implications for ilo's Builtin Set

Based on this analysis, the highest-impact additions to ilo's builtins:

```
-- Tier 1 gaps (highest priority)
post url body          > R t t      -- HTTP POST (complement to get)
read path              > R t t      -- read file as text
write path data        > R _ t      -- write text to file
sh cmd                 > R t t      -- execute shell command
env key                > O t        -- read environment variable

-- Tier 2 gaps (high priority)
now()                  > n          -- epoch milliseconds
uuid()                 > t          -- random UUID
rnd min max            > n          -- random number in range

-- Tier 2 gaps (medium priority)
mtc text pattern       > L t        -- regex match
rep text pattern repl  > t          -- regex replace
hash text              > t          -- SHA-256 hex digest
b64 text               > t          -- base64 encode
d64 text               > R t t      -- base64 decode
trm text               > t          -- trim whitespace
up text                > t          -- uppercase
low text               > t          -- lowercase
```

This set, combined with ilo's existing builtins and the tool system, covers the primitives an agent needs for real-world tasks: calling APIs, processing files, running commands, handling errors, and transforming data.
