# Python Standard Library Analysis for ilo

What an AI agent actually needs from Python's capabilities, evaluated against ilo's manifesto: **total tokens from intent to working code**. For each capability domain, we catalog what Python offers, what agents actually use, what's human ceremony, and what ilo should provide.

The organizing principle: ilo is a **typed shell for composing tool calls**, not a general-purpose language. Format parsing is a tool concern. Heavy computation is a tool concern. ilo provides the verified composition layer. This analysis identifies which Python primitives are truly compositional glue (belong in ilo) vs which are domain-specific work (belong in tools).

---

## 1. File I/O

### What Python offers

```python
# Reading/writing
with open("file.txt", "r") as f: content = f.read()
with open("file.txt", "w") as f: f.write(data)
Path("file.txt").read_text()
Path("file.txt").write_text(data)

# Paths
from pathlib import Path
p = Path("/home") / "user" / "file.txt"
p.parent, p.stem, p.suffix, p.exists()
os.path.join, os.path.split, os.path.exists

# Directories
os.listdir, os.walk, os.makedirs
Path.iterdir(), Path.glob("*.py"), Path.mkdir(parents=True)
shutil.copy, shutil.rmtree, shutil.move

# Temp files
tempfile.NamedTemporaryFile, tempfile.mkdtemp
```

### What agents actually use

Agents interact with files constantly. The core operations:

1. **Read file** -- the single most common file operation. Read a config, read source code, read data.
2. **Write file** -- second most common. Write generated code, write configs, write data.
3. **List directory** -- discover what files exist before reading them.
4. **Check existence** -- guard before read/write.
5. **Path joining** -- construct paths from components.
6. **File metadata** -- size, modification time (rare but needed for caching decisions).
7. **Glob/find** -- find files matching a pattern.

### What's human ceremony

- `with` context managers -- agents don't leak file handles because programs are short-lived
- `pathlib` vs `os.path` dual API -- one way to do things
- `shutil` as a separate module -- copy/move are basic operations, not a separate namespace
- Encoding parameter on every `open()` call -- default to UTF-8, period
- File modes (`r`, `w`, `a`, `rb`, `wb`) -- agents need read-text, write-text, read-bytes, write-bytes. Four operations, not a mode string.

### What ilo needs

File I/O is a **tool concern** in ilo's model. The language doesn't need file builtins -- it needs tools:

```
tool read"Read file contents" path:t>R t t
tool write"Write text to file" path:t data:t>R _ t
tool ls"List directory" path:t>R L t t
tool exists"Check if path exists" path:t>b
```

**Rationale:** File operations are side effects with real failure modes (permissions, disk full, missing paths). They belong in the tool layer with timeout/retry semantics, not as builtins. The `get` builtin is network I/O exposed as a builtin because HTTP GET is *the* primitive agent operation; file I/O is less universal (many agents operate in sandboxed/API-only environments).

**If ilo ever adds file builtins**, the minimal set:

| Builtin | Python equivalent | Tokens |
|---------|-------------------|--------|
| `rd path` | `Path(path).read_text()` | 2 |
| `wr path data` | `Path(path).write_text(data)` | 3 |
| `ls path` | `os.listdir(path)` | 2 |
| `ex path` | `Path(path).exists()` | 2 |

All return `R` types. No modes, no encoding params, no context managers.

---

## 2. Networking

### What Python offers

```python
# HTTP — stdlib
urllib.request.urlopen(url).read()
urllib.request.Request(url, headers={...}, method="POST", data=b"...")

# HTTP — popular third-party
requests.get(url, headers={}, params={}, timeout=5)
requests.post(url, json={}, headers={})
httpx.AsyncClient().get(url)

# Sockets
socket.socket(AF_INET, SOCK_STREAM)
sock.connect((host, port))
sock.send(data); sock.recv(1024)

# WebSocket (third-party)
websockets.connect(uri)
ws.send(message); ws.recv()

# Low-level
http.client.HTTPConnection
http.server.HTTPServer
```

### What agents actually use

1. **HTTP GET** -- fetch data from APIs, download content. Already in ilo as `get`/`$`.
2. **HTTP POST** -- submit data to APIs. The second most important network operation.
3. **HTTP with headers** -- auth tokens (`Authorization: Bearer ...`), content types, API keys.
4. **HTTP with JSON body** -- POST/PUT/PATCH with structured payloads.
5. **Response status code** -- 200 vs 404 vs 500 matters for control flow.
6. **Response headers** -- rate limiting (`Retry-After`), pagination (`Link`), content type.

### What agents rarely need directly

- **Raw sockets** -- agents call APIs, not socket protocols
- **WebSocket** -- streaming is an infrastructure concern, not a composition concern
- **UDP** -- almost never
- **HTTP server** -- agents are clients, not servers
- **Connection pooling** -- runtime concern, not language concern
- **SSL/TLS configuration** -- should just work

### What's human ceremony

- `urllib` vs `requests` vs `httpx` -- three ways to do the same thing
- `requests.Session()` for connection reuse -- runtime optimization, not semantics
- Manual `response.json()` parsing -- should be automatic when return type is a record
- Exception handling for every HTTP call -- ilo's `R` type handles this structurally
- `response.raise_for_status()` -- Python's "opt-in error checking" pattern; ilo checks by default

### What ilo needs

ilo already has `get url` returning `R t t`. The next primitives:

| Builtin/Tool | Purpose | Signature |
|-------------|---------|-----------|
| `get url` | HTTP GET (exists) | `>R t t` |
| `post url body` | HTTP POST with text/JSON body | `>R t t` |
| `req method url headers body` | Full HTTP request | `>R t t` |

**Design question:** Should `post` be a builtin (like `get`) or a tool?

Arguments for builtin:
- POST is the second most common agent HTTP operation
- `post url body` is 3 tokens -- extremely terse
- Agents calling APIs need POST for every write operation

Arguments for tool:
- Headers, auth, content-type add complexity
- POST semantics vary (JSON body vs form body vs raw)
- The `tool` system already handles parameterized external calls

**Recommendation:** Add `post url body` as a builtin (returns `R t t`, body auto-serialized as JSON if it's a record, text otherwise). For full HTTP control, provide a `req` builtin or tool with method/headers/body. This covers 95% of agent HTTP needs in 2-3 tokens.

Headers could use a record:

```
h=headers auth:"Bearer tok123" ct:"application/json"
r=req "POST" url h body
```

Or a simpler approach -- headers as a list of key:value text pairs:

```
r=req "POST" url ["Authorization:Bearer tok123"] body
```

---

## 3. Process Execution

### What Python offers

```python
# subprocess (modern)
subprocess.run(["ls", "-la"], capture_output=True, text=True)
subprocess.run("ls -la", shell=True, capture_output=True)
subprocess.Popen(cmd, stdout=PIPE, stderr=PIPE)
result.stdout, result.stderr, result.returncode

# os (legacy)
os.system("ls -la")
os.popen("ls -la").read()

# shlex for safe argument splitting
shlex.split("ls -la 'file name'")
```

### What agents actually use

This is a **critical** capability. AI agents frequently need to:

1. **Run shell commands** -- `git status`, `npm install`, `cargo build`, `ls`, `cat`
2. **Capture stdout** -- get command output for processing
3. **Check exit code** -- did the command succeed?
4. **Capture stderr** -- error messages for debugging
5. **Pipe commands** -- `cat file | grep pattern | wc -l`
6. **Run with timeout** -- prevent hanging on interactive commands

### What's human ceremony

- `subprocess.run` vs `os.system` vs `Popen` -- three interfaces for the same thing
- `capture_output=True, text=True` boilerplate on every call
- `shell=True` security warnings -- agents in sandboxed environments don't care
- `shlex.split` for argument safety -- the shell itself handles this
- `Popen` for streaming -- rare in agent contexts; agents want the full output

### What ilo needs

Shell execution is **essential** for agents. This is arguably more important than HTTP for local-agent workflows (Claude Code, Cursor, Devin all execute shell commands as their primary tool).

| Builtin | Purpose | Signature |
|---------|---------|-----------|
| `sh cmd` | Run shell command, return stdout | `>R t t` |
| `sh! cmd` | Run + auto-unwrap (propagate non-zero exit) | stdout text |

The `R` result maps perfectly: `~stdout` on exit 0, `^stderr` on non-zero exit.

```
-- List files, check for errors
r=sh "ls -la";?r{~out:process out;^err:^+"ls failed: "err}

-- Or with auto-unwrap
files=sh! "ls -la"
```

**Token comparison:**
```python
# Python: 8 tokens minimum
result = subprocess.run(["ls", "-la"], capture_output=True, text=True)
if result.returncode != 0: raise Exception(result.stderr)
output = result.stdout

# ilo: 2-3 tokens
out=sh! "ls -la"
```

**Design decision:** Should `sh` be a builtin or tool?

Builtin:
- Shell execution is as fundamental as HTTP for agents
- Every agent framework provides it
- `sh cmd` at 2 tokens is maximally terse
- Timeout can be a runtime default (e.g. 30s) with override

Tool:
- Shell execution is platform-dependent
- Security implications in multi-tenant environments
- Some agent environments intentionally sandbox shell access

**Recommendation:** Builtin behind a feature flag (like `get` is behind `http`). `sh` feature on by default, disabled with `--no-default-features` for sandboxed environments.

---

## 4. Data Formats

### What Python offers

```python
# JSON
json.loads(text)         # parse
json.dumps(obj)          # serialize
json.dumps(obj, indent=2)  # pretty-print

# YAML (third-party)
yaml.safe_load(text)
yaml.dump(obj)

# XML
xml.etree.ElementTree.parse(file)
ET.fromstring(text)

# CSV
csv.reader(file)
csv.DictReader(file)
csv.writer(file)

# TOML (3.11+)
tomllib.loads(text)

# INI
configparser.ConfigParser()
```

### What agents actually use

1. **JSON parse** -- API responses, config files, tool outputs. **By far** the most common.
2. **JSON serialize** -- constructing API request bodies, writing configs.
3. **CSV read** -- data processing, spreadsheet data.
4. **YAML read** -- config files (Docker, K8s, CI/CD).
5. **TOML read** -- Rust/Python project configs.
6. **XML** -- increasingly rare; legacy APIs, HTML parsing.

### What's human ceremony

- `json.loads` / `json.dumps` naming -- not discoverable
- `indent=2` for pretty-printing -- formatting is a display concern
- `yaml.safe_load` vs `yaml.load` security distinction -- just be safe by default
- `csv.DictReader` vs `csv.reader` -- agents always want the dict form (keyed access)
- XML's verbose DOM API -- agents want to extract values, not traverse nodes

### What ilo needs

From OPEN.md: **"Format parsing is a tool concern, not a language concern."** This is the right principle. However, JSON is special because it's the tool boundary format -- JSON<->Value mapping is built into ilo's tool infrastructure (D1e in TODO).

| Capability | Mechanism | Notes |
|-----------|-----------|-------|
| JSON parse | Tool boundary (automatic) | Tool returns typed record/list, JSON mapping implicit |
| JSON parse (raw) | `jsn text` builtin | When tool returns `t` and agent needs to extract fields |
| JSON serialize | Tool boundary (automatic) | Record/list auto-serialized when passed to tool |
| CSV/YAML/TOML/XML | Tools | `tool csv-read"Parse CSV" data:t>L L t`, etc. |

**The key insight:** Most data format handling in Python is boilerplate that bridges untyped text to typed data. ilo's tool declarations *are* the schema. When a tool declares `>R profile t`, the runtime maps JSON to a typed record automatically. The agent never calls `json.loads`.

**Only JSON needs language-level support** because it's the universal API interchange format. Everything else is domain-specific and belongs in tools.

Possible JSON builtins:

```
jsn text     -- parse JSON text to ilo value (R val t)
ser value    -- serialize ilo value to JSON text
```

But even these might be unnecessary if the tool boundary handles all JSON automatically. An agent composing tool calls never touches raw JSON -- it flows through typed interfaces.

---

## 5. String Manipulation

### What Python offers

```python
# Basic operations
s.upper(), s.lower(), s.strip(), s.replace(old, new)
s.startswith(prefix), s.endswith(suffix)
s.split(sep), sep.join(list), s.find(sub)
s[start:end], len(s)

# Regex
import re
re.search(pattern, text)
re.findall(pattern, text)
re.sub(pattern, replacement, text)
re.match(pattern, text)
re.compile(pattern)

# Formatting
f"Hello {name}, you have {count} items"
"Hello {}".format(name)
"%s has %d items" % (name, count)
Template("$name has $count items").substitute(...)

# Encoding
base64.b64encode(data), base64.b64decode(data)
urllib.parse.quote(text), urllib.parse.unquote(text)
text.encode('utf-8'), bytes.decode('utf-8')
binascii.hexlify(data)
```

### What agents actually use

1. **String concatenation** -- building prompts, messages, URLs. Already in ilo as `+a b`.
2. **Split/join** -- parsing delimited data. Already in ilo as `spl`/`cat`.
3. **Substring search** -- checking if text contains something. Already in ilo as `has`.
4. **String interpolation/formatting** -- constructing URLs, messages, payloads.
5. **Replace** -- text transformation.
6. **Trim/strip** -- cleaning whitespace from API responses.
7. **Regex match/extract** -- parsing semi-structured text.
8. **Case conversion** -- API key normalization, comparison.
9. **Base64 encode/decode** -- auth tokens, binary data in JSON.
10. **URL encoding** -- constructing query strings.

### What's human ceremony

- Three string formatting syntaxes (f-strings, `.format()`, `%`) -- one way to do things
- `re.compile` for performance -- premature optimization; agents run once
- Separate `re` module import -- should be inline
- `str.encode('utf-8')` for byte conversion -- UTF-8 should be implicit
- `urllib.parse.quote` in a separate module -- URL encoding is a builtin-level operation for API work

### What ilo needs

ilo already has: `+` (concat), `spl` (split), `cat` (join), `has` (contains), `len`, `hd`, `tl`, `slc` (slice), `rev` (reverse).

Missing primitives that agents need:

| Builtin | Purpose | Python equivalent | Frequency |
|---------|---------|-------------------|-----------|
| `rpl t old new` | Replace substring | `s.replace(old, new)` | Very high |
| `trm t` | Trim whitespace | `s.strip()` | High |
| `upr t` | Uppercase | `s.upper()` | Medium |
| `lwr t` | Lowercase | `s.lower()` | Medium |
| `idx t sub` | Find index of substring (-1 if missing) | `s.find(sub)` | Medium |
| `pfx t pre` | Starts with prefix | `s.startswith(pre)` | Medium |
| `sfx t suf` | Ends with suffix | `s.endswith(suf)` | Medium |
| `b64e t` | Base64 encode | `base64.b64encode()` | Medium (auth) |
| `b64d t` | Base64 decode | `base64.b64decode()` | Medium (auth) |
| `urle t` | URL encode | `urllib.parse.quote()` | Medium (API) |
| `urld t` | URL decode | `urllib.parse.unquote()` | Low |

**Regex:** Should regex be a builtin or tool?

Arguments for builtin:
- Agents use regex constantly for text extraction
- `rgx pattern text` at 3 tokens is very terse
- LLMs are excellent at generating regex patterns

Arguments for tool:
- Regex engines are complex (backtracking, unicode)
- Adding a regex engine inflates the binary
- Most regex use cases are extracting structured data -- which tools handle better

**Recommendation:** Add regex as a builtin behind a feature flag. Core operations:

```
rgx pattern text       -- find first match, return R t t (Ok=match, Err="no match")
rga pattern text       -- find all matches, return L t
rgs pattern repl text  -- replace all matches, return t
```

3 builtins, 3 tokens each. Covers 95% of agent regex use.

**String interpolation:** ilo currently builds strings with `+` concatenation:

```
m=+"Hello "+name+", you have "+(str count)+" items"
```

This is verbose. A template builtin would help:

```
fmt "Hello {}, you have {} items" name (str count)
```

But this conflicts with the manifesto's "one way to do things" -- `+` already concatenates. And `fmt` needs variadic args (not in ilo). **Defer until real pain is measured.**

---

## 6. Concurrency

### What Python offers

```python
# async/await
async def fetch(url):
    async with aiohttp.ClientSession() as session:
        response = await session.get(url)
        return await response.text()
asyncio.run(fetch(url))

# Threading
thread = threading.Thread(target=func, args=(...))
thread.start(); thread.join()
from concurrent.futures import ThreadPoolExecutor
with ThreadPoolExecutor(max_workers=5) as executor:
    results = executor.map(func, urls)

# Multiprocessing
from multiprocessing import Pool
with Pool(4) as p: results = p.map(func, data)
```

### What agents actually use

1. **Parallel HTTP requests** -- fetch multiple APIs concurrently.
2. **Parallel tool calls** -- call independent tools in parallel.
3. **Timeout on operations** -- don't block forever.
4. **Sequential pipelines** -- most agent work is sequential (call A, use result in B).

### What agents don't need

- Thread synchronization (locks, semaphores, conditions)
- Shared mutable state
- Process pools for CPU parallelism
- Event loops they manage themselves
- Cancellation tokens / cooperative cancellation
- async/await syntax

### What's human ceremony

- `async`/`await` keyword ceremony -- agents want parallel execution, not coroutine management
- `asyncio.run()` boilerplate -- agents shouldn't manage event loops
- Thread management (start/join) -- agents want "run these in parallel, give me all results"
- `concurrent.futures` abstraction layers -- too much ceremony for "do N things at once"

### What ilo needs

Agent concurrency is embarrassingly parallel: "call these 3 tools, wait for all, continue." No shared state, no synchronization, no futures.

The right primitive is **parallel map** or **parallel call**:

```
-- Sequential (current):
a=get! url1;b=get! url2;c=get! url3

-- Parallel (proposed):
[a, b, c]=par{get url1;get url2;get url3}

-- Or parallel map over a list:
rs=@! x urls{get x}    -- @! = parallel foreach
```

**Design options:**

1. **`par{...}` block** -- execute all statements in parallel, return list of results. Simple, explicit.
2. **`@!` parallel foreach** -- like `@` but iterations run concurrently. Natural extension.
3. **Runtime decides** -- the runtime analyzes the dependency graph and parallelizes independent calls automatically. No syntax needed. Most aligned with ilo's philosophy -- the agent shouldn't need to think about parallelism.

**Recommendation:** Option 3 (runtime auto-parallelization) is most aligned with the manifesto, but hard to implement. Start with option 1 (`par{...}`) as an explicit construct, which makes the intent clear and is simple to verify.

Timeout is already in tool declarations (`timeout:5`). No additional language support needed.

---

## 7. Error Handling

### What Python offers

```python
try:
    result = risky_operation()
except ValueError as e:
    handle_value_error(e)
except (IOError, OSError) as e:
    handle_io_error(e)
except Exception as e:
    handle_any(e)
finally:
    cleanup()
raise ValueError("bad input")
# Custom exceptions
class AppError(Exception): pass
```

### What agents actually use

1. **Try/catch around tool calls** -- API might fail, file might not exist.
2. **Error propagation** -- if step 2 fails, the whole workflow fails.
3. **Error messages** -- need to know *what* failed to fix it.
4. **Retry on transient errors** -- API timeout -> try again.
5. **Compensation on failure** -- if charge fails, release the reservation.

### What agents don't need

- Exception class hierarchies -- agents care about "did it work?" not "was it a ValueError or TypeError?"
- `finally` blocks -- short-lived programs don't need cleanup
- Re-raising with modified messages -- `raise ... from ...`
- Custom exception types -- `R ok err` with text errors is sufficient
- Stack traces -- agents need the error message, not the call stack

### What's human ceremony

- `try`/`except`/`finally`/`else` -- four keywords for "might fail"
- Exception class hierarchy (`ValueError` vs `TypeError` vs `RuntimeError`) -- agents just need the message
- `raise` + exception construction -- verbose
- Multiple `except` clauses -- agents rarely branch on error type
- `traceback` module for formatting -- display concern

### What ilo already has (and it's good)

ilo's error handling is already well-designed for agents:

```
-- R ok err type: structural, verified, zero ceremony
get-user uid;?{^e:^+"Lookup failed: "e;~d:use d}

-- ! auto-unwrap: propagate errors in 1 token
d=get-user! uid

-- Compensation:
rid=reserve items;charge pid amt;?{^e:release rid;^+"Payment failed: "e;~cid:continue}
```

**ilo's R type + ! operator is strictly better than Python's try/except for agents.** It's structural (verifier checks exhaustiveness), terse (1 token for propagation), and compositional (results flow through pipes).

**What's missing:**

| Feature | Purpose | Proposal |
|---------|---------|----------|
| Retry | Re-attempt on transient failure | Already in tool declarations: `retry:3` |
| Error type matching | Branch on error kind | `?r{^"timeout":retry;^e:^e;~v:v}` (text prefix matching) |
| Multiple error types | Different error variants | Defer to sum types (E3) |

**Verdict:** ilo's error handling is already excellent. No additions needed for Phase D. Sum types (Phase E3) will add typed error variants when needed.

---

## 8. Environment

### What Python offers

```python
# Environment variables
os.environ["API_KEY"]
os.environ.get("API_KEY", "default")
os.getenv("API_KEY", "default")

# Command-line arguments
sys.argv[1:]
import argparse
parser = argparse.ArgumentParser()
parser.add_argument("--output", default="result.txt")

# stdin/stdout
input()                     # read line from stdin
sys.stdin.read()            # read all stdin
print(result)               # write to stdout
sys.stderr.write("error")  # write to stderr
```

### What agents actually use

1. **Env vars** -- API keys, config values, secrets. **Critical** for agents -- secrets should never be in code.
2. **Command-line args** -- ilo already handles this (positional args to functions).
3. **Stdout** -- returning results. ilo already does this (last expression printed).
4. **Stdin** -- piping data in. Useful for `cat data.json | ilo 'process-program'`.

### What's human ceremony

- `argparse` -- agents don't need help text, subcommands, or argument parsing libraries
- `sys.argv` raw array access -- ilo's typed function params are already better
- `print()` with formatting options -- output is the return value

### What ilo needs

| Builtin | Purpose | Signature |
|---------|---------|-----------|
| `env key` | Read environment variable | `>R t t` (Err if not set) |
| `env key default` | Read env var with default | `>t` |

That's it. Two forms of one operation. ilo's function parameters already handle CLI args. stdout is already the return value.

**stdin:** Could be a builtin `inp()` that reads all of stdin as text. Enables piping:

```bash
cat data.json | ilo 'f d:t>t;process d'  # d = stdin content
```

But ilo already accepts args from the command line. Stdin support is a **runtime concern** (the CLI reads stdin and passes it as an argument), not a language concern.

**Recommendation:** Add `env` as a builtin. Stdin support in the CLI runner (not the language).

---

## 9. Cryptography

### What Python offers

```python
# Hashing
import hashlib
hashlib.sha256(data.encode()).hexdigest()
hashlib.md5(data.encode()).hexdigest()

# HMAC
import hmac
hmac.new(key, message, hashlib.sha256).hexdigest()

# Secrets
import secrets
secrets.token_hex(32)
secrets.token_urlsafe(32)

# Encryption (third-party)
from cryptography.fernet import Fernet
key = Fernet.generate_key()
cipher = Fernet(key)
encrypted = cipher.encrypt(data)
```

### What agents actually use

1. **SHA256 hash** -- verifying data integrity, generating cache keys, API signature verification.
2. **HMAC** -- webhook signature verification (GitHub, Stripe, Slack all use HMAC-SHA256).
3. **Random tokens** -- generating unique IDs, nonces, session tokens.
4. **UUID generation** -- unique identifiers for records, requests.

### What agents rarely need

- Symmetric/asymmetric encryption -- agents don't encrypt data; they call APIs that handle encryption
- Certificate management -- transport-layer concern
- Key derivation (PBKDF2, scrypt) -- auth systems handle this
- MD5 -- deprecated, but still used for legacy cache keys

### What's human ceremony

- `hashlib.sha256(data.encode()).hexdigest()` -- 6 method calls for one hash
- Separate `hmac` module -- HMAC is just hash + key
- `secrets` vs `random` distinction -- agents should always get cryptographic randomness

### What ilo needs

| Builtin | Purpose | Signature |
|---------|---------|-----------|
| `sha text` | SHA-256 hash (hex string) | `>t` |
| `hmac key text` | HMAC-SHA256 (hex string) | `>t` |
| `uid()` | Generate UUID v4 | `>t` |
| `rnd()` | Random float 0.0-1.0 | `>n` |
| `rnd a b` | Random integer in range | `>n` |

5 builtins. Covers webhook verification, cache keys, unique IDs, and random values. No encryption (tool concern), no key management (infra concern), no certificate handling (transport concern).

**Feature flag:** `crypto` -- adds sha/hmac. `uid` and `rnd` could be default (no deps).

---

## 10. Time/Date

### What Python offers

```python
# Current time
import time
time.time()                    # Unix timestamp float
time.monotonic()               # Monotonic clock

# datetime
from datetime import datetime, timedelta
now = datetime.now()
now = datetime.utcnow()
formatted = now.strftime("%Y-%m-%d %H:%M:%S")
parsed = datetime.strptime(text, "%Y-%m-%d")
delta = timedelta(hours=2, minutes=30)
future = now + delta

# ISO format
now.isoformat()
datetime.fromisoformat(text)

# timezone
from datetime import timezone
datetime.now(timezone.utc)
```

### What agents actually use

1. **Current timestamp** -- logging, cache expiry, scheduling.
2. **ISO 8601 format** -- API dates are almost always ISO 8601.
3. **Timestamp arithmetic** -- "2 hours from now", "is this expired?"
4. **Parse date string** -- reading API response dates.
5. **Sleep/delay** -- rate limiting, polling intervals.

### What agents don't need

- Timezone conversion libraries -- agents work in UTC
- `strftime` format strings -- ISO 8601 covers 95% of cases
- Calendar arithmetic (business days, holidays) -- domain-specific tool
- Locale-aware date formatting -- display concern

### What's human ceremony

- `datetime.now()` vs `time.time()` vs `datetime.utcnow()` -- three ways to get current time
- `strftime` format codes (`%Y-%m-%d`) -- just use ISO 8601
- `timedelta` construction verbosity -- `timedelta(hours=2, minutes=30)` is 6+ tokens
- Timezone-naive vs timezone-aware datetime objects -- just always use UTC

### What ilo needs

| Builtin | Purpose | Signature |
|---------|---------|-----------|
| `now()` | Unix timestamp (seconds, f64) | `>n` |
| `iso n` | Timestamp to ISO 8601 string (UTC) | `>t` |
| `ts t` | Parse ISO 8601 string to timestamp | `>R n t` |
| `slp n` | Sleep for n seconds | `>_` |

4 builtins. Timestamps are numbers (f64) -- arithmetic works with standard `+`/`-`/`*`. "Two hours from now" is `+now() 7200`. Duration is just a number of seconds.

No timezone type, no date type, no timedelta type. Just numbers (unix timestamps) and text (ISO 8601 strings). This is sufficient for every agent use case and costs zero type system complexity.

---

## 11. Math

### What Python offers

```python
import math
math.floor(x), math.ceil(x), math.sqrt(x)
math.pow(x, y), math.log(x), math.log10(x)
math.sin(x), math.cos(x), math.tan(x)
math.pi, math.e, math.inf
abs(x), min(a, b), max(a, b), round(x, n)
sum(iterable), sorted(iterable)
int(x), float(x)

# Random
import random
random.random(), random.randint(a, b)
random.choice(list), random.shuffle(list)
random.sample(list, k)
```

### What agents actually use

1. **Arithmetic** -- already in ilo (`+`, `-`, `*`, `/`)
2. **Floor/ceil** -- already in ilo (`flr`, `cel`)
3. **Abs/min/max** -- already in ilo (`abs`, `min`, `max`)
4. **Rounding** -- for currency, display values
5. **Power** -- occasionally, for exponential backoff, geometric calculations
6. **Modulo** -- checking divisibility, cycling through lists
7. **Square root** -- rare in agent code
8. **Random** -- ID generation, sampling (covered in crypto section)

### What agents almost never need

- Trigonometric functions -- agents don't do graphics or physics
- Logarithms -- unless doing ML, which is a tool concern
- Mathematical constants (pi, e) -- rare
- Complex numbers -- never

### What's human ceremony

- Separate `math` module import -- basic math should be inline
- `math.pow(x, y)` instead of `x ** y` -- operator is better
- `sum()` as a function vs `+` reduce -- agents need fold/reduce, not a special `sum`

### What ilo already has and what's missing

Already has: `+`, `-`, `*`, `/`, `abs`, `min`, `max`, `flr`, `cel`, `str` (n->t), `num` (t->n).

| Builtin | Purpose | Priority |
|---------|---------|----------|
| `%a b` or `mod a b` | Modulo | High -- needed for cycling, divisibility checks |
| `pow a b` | Power | Medium -- exponential backoff, simple math |
| `rnd a b` | Round to b decimal places | Medium -- currency, display |
| `sqr n` | Square root | Low |

**Recommendation:** Add `%` (modulo) as a prefix operator. It's used frequently enough to warrant single-character syntax. `pow` as a builtin. `rnd` for rounding (note: conflicts with random -- use `rnd` for random, `rnd` overloaded by arity? Or `rndf` for round-float? Better: `rnd n d` = round n to d decimals, `rnd()` = random. Arity disambiguates.)

---

## 12. Collections

### What Python offers

```python
# Lists
xs = [1, 2, 3]
xs.append(x), xs.extend(ys), xs.pop(), xs.insert(i, x)
xs[i], xs[start:end], xs.index(v), xs.count(v)
sorted(xs), reversed(xs), len(xs)
list(map(f, xs)), list(filter(f, xs))
[f(x) for x in xs if cond(x)]  # list comprehension

# Dicts
d = {"key": "value"}
d[key], d.get(key, default), d.keys(), d.values(), d.items()
d.update(other), {**d1, **d2}  # merge
{k: v for k, v in items if cond}  # dict comprehension

# Sets
s = {1, 2, 3}
s.add(x), s.remove(x), s.union(other), s.intersection(other)
x in s  # O(1) membership

# Tuples
t = (1, "hello", True)  # immutable, heterogeneous

# Named tuples / dataclasses
from dataclasses import dataclass
@dataclass
class Point:
    x: float
    y: float
```

### What agents actually use

1. **Lists** -- already in ilo. The primary collection.
2. **Dicts/Maps** -- key-value lookup, API response data, config. **NOT yet in ilo** (Phase E4).
3. **List iteration** -- already in ilo (`@`).
4. **List filtering** -- selecting items that match a condition.
5. **List transformation** -- applying a function to each element.
6. **Key access** -- getting a value by key from a dict/record. Records already do this.
7. **Membership test** -- already in ilo (`has`).
8. **Sorting** -- already in ilo (`srt`).

### What agents rarely need

- Sets -- agents don't do set theory; use lists with `has` for membership
- Tuples -- ilo records serve the same purpose with named fields
- List comprehensions (as syntax) -- agents need map/filter, not a special syntax
- Dict comprehensions -- rare
- `collections.Counter`, `defaultdict`, `OrderedDict` -- specialized tools

### What's human ceremony

- List comprehensions vs `map()`/`filter()` -- two ways to do the same thing
- `dict.get(key, default)` vs `dict[key]` with `KeyError` -- ilo's `??` (nil-coalesce) is better
- Mutable vs immutable collections -- agents don't care about mutability semantics
- `dataclass` decorator ceremony -- ilo's `type point{x:n;y:n}` is already minimal

### What ilo already has and what's missing

Already has: `[1, 2, 3]` (list literals), `@` (iterate), `+=` (append), `+` (concat), `spl`/`cat` (split/join), `has` (contains), `hd`/`tl` (head/tail), `rev` (reverse), `srt` (sort), `slc` (slice), `len`, index access (`xs.0`).

Missing high-value operations:

| Capability | Purpose | Proposal | Priority |
|-----------|---------|----------|----------|
| Map (transform) | Apply function to each element | `map f xs` or `@x xs{expr}` already works | Exists (via `@`) |
| Filter | Select matching elements | `flt cond xs` or guard in `@` loop | High |
| Reduce/fold | Accumulate list to single value | `fld op init xs` | High (needs E5) |
| Map type | Dynamic key-value store | `M t n` (Phase E4) | High |
| Map literal | Construct map inline | `M{"k1":v1;"k2":v2}` | High (with E4) |
| Unique/dedup | Remove duplicates | `unq xs` | Low |
| Flatten | Flatten nested lists | `flt xs` (name conflict with filter!) | Low |
| Zip | Pair elements from two lists | `zip xs ys` | Low |
| Enumerate | Index + value pairs | `@i x xs{...}` (two-var `@`) | Medium |

**The `@` loop already covers map and filter patterns**, just with more tokens:

```
-- Map: double each element
@x xs{*x 2}

-- Filter: keep elements > 5 (currently returns list with nil gaps -- needs filter semantics)
@x xs{>x 5{x}}
```

The gap is that `@` with a guard returns a list with nil entries for non-matching elements. A true `flt` builtin would return only matching elements.

**Map type (E4) is the biggest gap.** API responses are full of key-value structures that don't fit static records. Without maps, agents must use `t` (raw text) and lose type checking.

---

## 13. Type System

### What Python offers

```python
# Type hints (PEP 484+)
def greet(name: str) -> str: ...
x: int = 5
xs: list[int] = [1, 2, 3]
d: dict[str, int] = {"a": 1}

# Optional / Union
from typing import Optional, Union
def find(id: str) -> Optional[User]: ...
def parse(x: Union[str, int]) -> float: ...
# 3.10+ syntax
def parse(x: str | int) -> float: ...

# Generics
from typing import TypeVar, Generic
T = TypeVar('T')
def first(xs: list[T]) -> T: ...

# Protocols (structural typing)
from typing import Protocol
class Readable(Protocol):
    def read(self) -> str: ...

# TypedDict
from typing import TypedDict
class Movie(TypedDict):
    name: str
    year: int

# Literal types
from typing import Literal
def set_mode(mode: Literal["read", "write"]): ...

# Runtime checking: none (unless using pydantic, beartype, etc.)
```

### What agents actually use

1. **Basic types** -- know that a function takes a string and returns a number. Already in ilo.
2. **Optional/nullable** -- API fields that might be null. Planned for ilo (E2).
3. **Result types** -- success/failure. Already in ilo as `R ok err`.
4. **Record types** -- structured data from APIs. Already in ilo.
5. **List types** -- collections. Already in ilo as `L elem`.
6. **Generic types** -- `map` that works on any list. Planned for ilo (E5).

### What agents don't need

- Union types beyond Optional/Result -- `str | int` is a human convenience
- Protocol/structural typing -- agents write concrete code for specific tasks
- `TypeVar` ceremony -- too verbose, too abstract
- `TypedDict` vs `dataclass` vs `NamedTuple` -- one record type is enough
- Literal types -- string matching works fine
- Type narrowing / type guards -- verifier handles this
- Runtime type checking libraries (pydantic) -- ilo verifies before execution

### What's human ceremony

- `from typing import ...` on every file -- ilo types are built in
- `Optional[X]` instead of `X?` or `X | None` -- evolved through 3 syntax versions
- `TypeVar` declaration before use -- too much boilerplate for agents
- Generic class definitions -- agents don't write generic libraries
- `Protocol` ceremony -- structural typing is good but the syntax is heavy

### What ilo already has and roadmap

ilo's type system is already well-suited for agents:

| Feature | Python | ilo | Status |
|---------|--------|-----|--------|
| Primitives | `int`, `float`, `str`, `bool` | `n`, `t`, `b` | Done |
| Nil/None | `None` | `_` | Done |
| Lists | `list[int]` | `L n` | Done |
| Result | No stdlib equivalent | `R ok err` | Done |
| Records | `@dataclass` | `type name{fields}` | Done |
| Optional | `Optional[X]` | `O n` | Planned (E2) |
| Maps | `dict[str, int]` | `M t n` | Planned (E4) |
| Enums | `enum.Enum` | `enum name{variants}` | Planned (E3) |
| Generics | `TypeVar` + `Generic` | `a`, `b` type vars | Planned (E5) |
| Type aliases | `type Alias = ...` | `alias name type` | Planned (E1) |

**The type system roadmap (E1-E6) is well-aligned with agent needs.** The key insight from Python: agents need *cheap, precise* types that prevent errors, not *expressive* types that enable abstraction. ilo's 1-char type syntax (`n`, `t`, `b`, `L`, `R`) is already optimal for token cost.

---

## Synthesis: Priority Capabilities for ilo

### Tier 1 -- Essential agent primitives (highest impact)

These are capabilities that every agent needs and currently cannot do in ilo:

| Capability | Python equivalent | Proposed ilo | Tokens | Status |
|-----------|-------------------|-------------|--------|--------|
| Shell execution | `subprocess.run()` | `sh cmd` / `sh! cmd` | 2 | New |
| HTTP POST | `requests.post()` | `post url body` | 3 | New |
| Env vars | `os.getenv()` | `env key` / `env key default` | 2-3 | New |
| Replace string | `s.replace()` | `rpl t old new` | 4 | New |
| Trim whitespace | `s.strip()` | `trm t` | 2 | New |
| Modulo | `a % b` | `%a b` | 3 | New |
| Current time | `time.time()` | `now()` | 1 | New |

### Tier 2 -- High-value for API-heavy agents

| Capability | Python equivalent | Proposed ilo | Tokens | Status |
|-----------|-------------------|-------------|--------|--------|
| Full HTTP request | `requests.request()` | `req method url headers body` | 5 | New |
| JSON parse (raw) | `json.loads()` | `jsn text` | 2 | New |
| JSON serialize | `json.dumps()` | `ser value` | 2 | New |
| Base64 encode | `base64.b64encode()` | `b64e t` | 2 | New |
| Base64 decode | `base64.b64decode()` | `b64d t` | 2 | New |
| URL encode | `urllib.parse.quote()` | `urle t` | 2 | New |
| SHA-256 hash | `hashlib.sha256()` | `sha t` | 2 | New |
| HMAC | `hmac.new()` | `hmac key t` | 3 | New |
| UUID | `uuid.uuid4()` | `uid()` | 1 | New |
| Timestamp to ISO | `datetime.isoformat()` | `iso n` | 2 | New |
| ISO to timestamp | `datetime.fromisoformat()` | `ts t` | 2 | New |
| Sleep | `time.sleep()` | `slp n` | 2 | New |

### Tier 3 -- Nice to have, deferrable

| Capability | Python equivalent | Proposed ilo | Notes |
|-----------|-------------------|-------------|-------|
| Regex match | `re.search()` | `rgx pat text` | Feature flag |
| Regex replace | `re.sub()` | `rgs pat repl text` | Feature flag |
| Uppercase | `s.upper()` | `upr t` | Low frequency |
| Lowercase | `s.lower()` | `lwr t` | Low frequency |
| Starts/ends with | `s.startswith()` | `pfx t pre` / `sfx t suf` | `has` partially covers |
| Find index | `s.find()` | `idx t sub` | Low frequency |
| Power | `math.pow()` | `pow a b` | Low frequency |
| Random | `random.random()` | `rnd()` / `rnd a b` | With crypto flag |
| File read | `open().read()` | `rd path` | Tool or feature flag |
| File write | `open().write()` | `wr path data` | Tool or feature flag |
| Parallel exec | `ThreadPoolExecutor` | `par{...}` | Needs design |
| Map/filter builtins | `map()`, `filter()` | `map f xs`, `flt f xs` | Needs E5 (generics) |

### What ilo should NOT add from Python

| Python feature | Why not in ilo |
|---------------|----------------|
| Classes / OOP | Agents don't write class hierarchies |
| Decorators | Framework pattern, not agent pattern |
| Generators / yield | Complex state machines; agents compose tools |
| Context managers | Short-lived programs don't need resource cleanup |
| Import system | Programs are self-contained by manifesto |
| Global variables | Manifesto: "no globals, no ambient state" |
| Multiple inheritance | Never |
| Metaclasses | Never |
| Property descriptors | Never |
| Operator overloading | One meaning per operator |
| Comprehensions (as special syntax) | `@` loop covers this |
| String formatting mini-language | `+` concatenation is sufficient |
| Docstrings | Comments (`--`) are enough |
| `assert` statements | Verifier catches errors before runtime |
| Dynamic attribute access (`getattr`) | Records have fixed fields |

---

## Key Principle: Builtins vs Tools

The hardest design decision is the **builtin vs tool boundary**. The manifesto says "format parsing is a tool concern." Extending this principle:

**Builtins** (compile to opcodes, always available):
- Operations that are *composition glue*: string manipulation, math, data access
- Operations every agent needs regardless of domain: HTTP GET, env vars
- Operations that are too small to be tools: `len`, `+`, `has`

**Tools** (declared, external, fallible):
- Operations with *side effects on external systems*: file I/O, database, API calls
- Operations that are *domain-specific*: CSV parsing, image processing, ML inference
- Operations that need *timeout/retry semantics*: slow API calls, network requests

**The gray area:** `sh` (shell execution), `post` (HTTP POST), file I/O. These have side effects but are so fundamental to agents that the token cost of declaring them as tools on every program is prohibitive.

**Recommendation:** Make them builtins behind feature flags. The runtime provides them; the agent doesn't need to declare them. The feature flag system lets sandboxed environments disable them.

```
default features: [cranelift, http, shell]
http features: get, post, req
shell features: sh
crypto features: sha, hmac
time features: now, iso, ts, slp
```

This gives agents full capability by default while allowing locked-down environments to strip features.
