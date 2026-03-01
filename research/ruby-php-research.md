# Ruby & PHP Capabilities Research for ilo

Research into Ruby and PHP features relevant to ilo's design as a token-minimal AI agent programming language. Focus: what patterns from these languages are worth adopting, what should be avoided, and what ilo already handles better.

---

## 1. Ruby Capabilities Catalog

### 1.1 File I/O — File, IO, Dir, Pathname

**What Ruby provides:**

| Class | Role | Key Methods |
|-------|------|-------------|
| `File` | Read/write/stat/rename/delete files | `read`, `write`, `open`, `exist?`, `delete`, `rename`, `size` |
| `IO` | Low-level byte/line I/O, pipe creation | `read`, `write`, `gets`, `puts`, `pipe` |
| `Dir` | Directory listing, creation, deletion | `entries`, `glob`, `mkdir`, `chdir`, `exist?` |
| `Pathname` | Unified facade over File+Dir+FileTest | `join`, `basename`, `dirname`, `exist?`, `read`, `glob` |

**How Ruby does it (minimal examples):**

```ruby
# Read entire file
content = File.read("data.json")

# Write file
File.write("out.txt", "hello")

# Block-based (auto-closes)
File.open("log.txt", "a") { |f| f.puts "entry" }

# Pathname unification
require 'pathname'
p = Pathname.new("/tmp") / "data" / "file.json"
p.read          # reads file
p.exist?        # checks existence
p.dirname       # parent directory
```

**Key design insight:** `Pathname` is an immutable value object. `/` operator joins paths. Every method returns a new `Pathname`. Thread-safe by design. The block pattern (`File.open { |f| ... }`) guarantees resource cleanup without explicit close calls.

**ilo relevance:**
- File I/O is a **tool concern**, not a language concern (per OPEN.md: "format parsing is a tool concern"). An agent needing file access would call a `read-file` or `write-file` tool.
- However, the Pathname-as-value pattern is worth noting: paths as immutable typed values that compose with `/` maps to ilo's record + operator model.
- Ruby's block-based auto-cleanup maps to ilo's Result type — `R t t` handles the success/failure case; cleanup is the tool's job.

---

### 1.2 Networking — Net::HTTP, open-uri, Sockets

**What Ruby provides:**

| Library | Ceremony Level | Best For |
|---------|---------------|----------|
| `open-uri` | Minimal — `URI.open(url).read` | Simple GETs |
| `Net::HTTP` | Medium — connection management, headers, HTTPS | Full HTTP |
| `TCPSocket` | Low-level — raw TCP/UDP | Custom protocols |

**How Ruby does it:**

```ruby
# open-uri: 1 line for GET
require 'open-uri'
body = URI.open("https://api.example.com/data").read

# Net::HTTP: more control
require 'net/http'
uri = URI("https://api.example.com/data")
res = Net::HTTP.get_response(uri)
body = res.body if res.is_a?(Net::HTTPSuccess)

# Persistent connections
Net::HTTP.start(uri.host, uri.port, use_ssl: true) do |http|
  res1 = http.get("/users")
  res2 = http.get("/orders")
end
```

**Key design insight:** Ruby's `open-uri` wraps HTTP with a file-like interface — `open(url)` returns an IO-like object. This "URLs are just files" abstraction is powerful but leaky (no POST, no headers control). Net::HTTP's block form (`start { |http| ... }`) manages connection lifecycle.

**ilo relevance:**
- ilo already has `get url` / `$url` returning `R t t`. This is MORE concise than even `open-uri` (3 chars vs ~30).
- POST/PUT/DELETE: currently deferred. When added, should follow the same `verb url body` pattern — keep it terse. `post url data` / `put url data` / `del url`.
- Connection pooling is a runtime concern, not a language concern. The tool provider infrastructure (D1d) handles this.

---

### 1.3 Process Execution — Backticks, system, exec, Open3

**Ruby provides 5 distinct methods for running external commands:**

| Method | Returns | Captures stdout | Captures stderr | Replaces process |
|--------|---------|----------------|----------------|-----------------|
| `` `cmd` `` / `%x(cmd)` | stdout as String | Yes | No | No |
| `system(cmd)` | `true`/`false`/`nil` | No (prints to console) | No | No |
| `exec(cmd)` | Never returns | No | No | Yes |
| `Open3.capture3` | stdout, stderr, status | Yes | Yes | No |
| `Open3.popen3` | stdin, stdout, stderr, thread | Yes | Yes | No |

**How backticks work in detail:**

```ruby
# Basic: returns stdout as string
output = `ls -la`

# With interpolation
dir = "/tmp"
output = `ls #{dir}`

# With %x alternative syntax (any delimiter)
output = %x(date)
output = %x{whoami}
output = %x-uname -a-

# Exit status via $? global
output = `cat /etc/hosts`
if $?.success?
  puts output
else
  puts "Failed with exit code #{$?.exitstatus}"
end

# Newline handling — .chomp is standard
hostname = `hostname`.chomp
```

**Under the hood:** Both backticks and `system` use `fork()` + `exec()`. Backticks capture the forked process's stdout into a string. `$?` is set to a `Process::Status` object after execution.

**Security concern:** Backticks with interpolation are vulnerable to command injection. `user_input = "hello; rm -rf *"` + `` `echo #{user_input}` `` executes both commands. Ruby's `system` with array form is safe: `system("echo", user_input)`.

**ilo relevance:**
- **Backtick-as-shell-exec is relevant for agents.** An agent that can run shell commands has immense capability with minimal vocabulary. Ruby's backtick is 2 tokens (the backtick pair) for arbitrary shell access.
- ilo currently has no shell execution. The `tool` mechanism is the intended external interface. But a shell execution builtin (like `sh "cmd"` returning `R t t`) would be enormously powerful — stdout as Ok, stderr/failure as Err. This is essentially what ilo's `get` does for HTTP.
- The `$?` global for exit status is clunky. ilo's `R` type is better — the Result carries success/failure inline.
- Security: for agent use, injection is less of a concern (the agent IS the programmer), but sandboxing matters. This is a runtime/policy concern, not a language concern.

---

### 1.4 Data Formats — JSON, YAML, XML, CSV

**All four are built into Ruby's standard library:**

| Format | Library | Parse | Generate |
|--------|---------|-------|----------|
| JSON | `json` | `JSON.parse(str)` → Hash/Array | `hash.to_json` |
| YAML | `yaml` | `YAML.load(str)` → Ruby objects | `obj.to_yaml` |
| CSV | `csv` | `CSV.parse(str)` → Array of Arrays | `CSV.generate { \|csv\| ... }` |
| XML | `rexml` | `REXML::Document.new(str)` | Built-in, verbose |

```ruby
require 'json'
data = JSON.parse('{"name":"alice","age":30}')
data["name"]  # => "alice"

require 'yaml'
config = YAML.load(File.read("config.yml"))

require 'csv'
CSV.foreach("data.csv", headers: true) { |row| puts row["name"] }
```

**Key design insight:** JSON in Ruby maps directly to native types (Hash, Array, String, Integer, Float, true/false, nil). No wrapper objects. `JSON.parse` returns a plain Hash.

**ilo relevance:**
- Per OPEN.md: "format parsing is a tool concern, not a language concern." ilo composes typed tool results. Tools return `R record t` or `R t t`.
- JSON-to-Value mapping is already planned (D1e: `Value::from_json(type_hint)`). This is the RIGHT boundary — the tool parses, ilo consumes typed values.
- YAML/CSV/XML parsing would be tools, not builtins. An agent calls `parse-csv data` and gets back a list of records.
- **One exception worth considering:** JSON is so fundamental to API communication that a `json` builtin (parse text to Value) might earn its place alongside `get`. `j=json txt` → typed Value. This saves a tool roundtrip for the most common data format.

---

### 1.5 String Manipulation — Regex, Interpolation, Encoding

**What Ruby provides:**

| Feature | Syntax | Example |
|---------|--------|---------|
| Regex literal | `/pattern/flags` | `/\d+/` |
| Match test | `=~` | `"abc" =~ /\d/` → nil |
| Match data | `.match` | `"age:30".match(/(\d+)/)[1]` → "30" |
| Interpolation | `"#{expr}"` | `"Hello #{name}"` |
| Substitution | `.gsub` | `"hello".gsub(/l/, "r")` → "herro" |
| Split | `.split` | `"a,b,c".split(",")` → ["a","b","c"] |
| Encoding | `.encode` | `str.encode("UTF-8")` |

```ruby
# Regex is first-class
email = "user@example.com"
if email =~ /\A[\w+\-.]+@[a-z\d\-]+(\.[a-z]+)*\.\w+\z/i
  puts "Valid"
end

# Capture groups
"2025-01-15".match(/(\d{4})-(\d{2})-(\d{2})/) do |m|
  year, month, day = m[1], m[2], m[3]
end

# Interpolation inside regex
domain = "example\\.com"
/#{domain}$/

# gsub with block
"Hello World".gsub(/\w+/) { |word| word.upcase }
# => "HELLO WORLD"
```

**Key design insight:** Ruby's regex is first-class (literal syntax, `=~` operator, `$~` match data). Regex interpolation means patterns are composable. String interpolation with `#{}` is universal — works in strings, regex, heredocs.

**ilo relevance:**
- ilo has `spl` (split), `has` (substring test), `+` (concat). No regex.
- **Regex is a significant capability gap for text extraction.** Agents frequently need to extract patterns from text (parse URLs, extract numbers, match formats). Without regex, this requires chaining `spl`/`has`/`slc` — verbose and error-prone.
- A regex match builtin could be: `rx text pattern` → `R L t t` (Ok = list of captures, Err = no match or bad pattern). Or `rx text pattern` → `R t t` (Ok = first match, Err = no match).
- String interpolation: ilo uses `+` for concat (`+"Hello " name`). This is more token-efficient than `"Hello #{name}"` (which costs the `#{}` delimiters). ilo's approach is already optimal for agents.
- Encoding: not relevant for agent use. UTF-8 everywhere.

---

### 1.6 Concurrency — Threads, Fibers, Ractor

**Ruby's three concurrency primitives:**

| Primitive | Type | Parallelism | Best For |
|-----------|------|-------------|----------|
| Threads | Preemptive, GVL-limited | No (I/O only) | Waiting on network/disk |
| Fibers | Cooperative, manual yield | No | Non-blocking I/O, generators |
| Ractors | Isolated interpreters | Yes (each has own GVL) | CPU-bound, experimental |

```ruby
# Threads — concurrent I/O
threads = urls.map { |url| Thread.new { Net::HTTP.get(URI(url)) } }
results = threads.map(&:value)

# Fibers — cooperative
fib = Fiber.new do
  Fiber.yield 1
  Fiber.yield 2
  3
end
fib.resume  # => 1
fib.resume  # => 2
fib.resume  # => 3

# Ractors — true parallelism (Ruby 3.0+)
ractors = 4.times.map do
  Ractor.new { heavy_computation }
end
results = ractors.map(&:take)
```

**Key design insight:** Ruby's GVL means threads are only useful for I/O concurrency. Fibers are lightweight but require explicit `yield`. Ractors achieve true parallelism by forbidding shared mutable state — each Ractor is an isolated world that communicates via message passing.

**ilo relevance:**
- ilo programs are currently sequential. Concurrency is a future concern.
- **Ractor's isolation model aligns with ilo's "self-contained" principle.** Each function declares its deps, no global state. If ilo adds concurrency, isolated execution units with message passing (like Ractors) would be natural.
- For agent use, the most relevant concurrency pattern is **parallel tool calls** — fire off multiple `get` calls simultaneously. This is a runtime optimization, not a language feature. The tool provider (D1d) could parallelize independent tool calls in a DAG automatically.
- Fibers could inform a future "generator" pattern for streaming data, but this is speculative.

---

### 1.7 Error Handling — begin/rescue/ensure

**Ruby's exception model:**

```ruby
begin
  risky_operation
rescue ArgumentError, TypeError => e
  # Handle specific errors
rescue StandardError => e
  # Handle general errors
  retry if retryable?     # restart begin block
else
  # Runs only if no exception
ensure
  # ALWAYS runs (cleanup)
end

# Method-level rescue (no begin needed)
def fetch(url)
  Net::HTTP.get(URI(url))
rescue SocketError => e
  "Connection failed: #{e.message}"
end

# Custom exceptions
class PaymentError < StandardError; end
class InsufficientFunds < PaymentError; end

raise InsufficientFunds, "Balance too low"
```

**Key design insight:** Ruby's `ensure` guarantees cleanup. `retry` re-executes the begin block (useful for transient failures). Method-level rescue eliminates `begin/end` boilerplate. Custom exception hierarchies enable `rescue ParentError` to catch all subtypes.

**ilo relevance:**
- ilo uses `R ok err` (Result type) instead of exceptions. This is **strictly better for agents**:
  - Exceptions are invisible in the type signature — an agent can't know what might throw without reading the body.
  - Results are explicit in the signature: `>R profile t` — the agent sees the error possibility at the call site.
  - `!` auto-unwrap is ilo's equivalent of letting errors propagate: `get! url` propagates the Err to the caller.
- `ensure` (guaranteed cleanup) maps to ilo's inline compensation pattern: `charge pid amt;?{^e:release rid;^+"Payment failed: "e;~cid:continue}`. The rollback is explicit in the control flow.
- `retry` is interesting but dangerous (infinite loops). ilo's `tool` declarations have `retry:n` which is bounded and handled by the runtime.
- **No adoption needed.** ilo's Result type is superior for agent use. Exception hierarchies are unnecessary when errors are typed values.

---

### 1.8 Environment — ENV, ARGV

**Ruby's environment access:**

```ruby
# ENV — hash-like object wrapping process environment
ENV["HOME"]            # => "/home/user"
ENV["API_KEY"]         # => "sk-..."
ENV.fetch("PORT", "3000")  # with default

# ARGV — array of command-line arguments
# ruby script.rb --verbose data.json
ARGV[0]  # => "--verbose"
ARGV[1]  # => "data.json"
# Note: ARGV[0] is first arg, NOT program name ($0 is program name)

# Destructuring
first, *rest = ARGV
```

**ilo relevance:**
- ilo already handles CLI arguments via function parameters: `ilo 'f x:n y:n>n;+x y' 3 4` maps positional args to params.
- `ENV` access is not currently in ilo. For agent use, environment variables are important for:
  - API keys (`ENV["OPENAI_KEY"]`)
  - Configuration (`ENV["DATABASE_URL"]`)
  - Feature flags (`ENV["DEBUG"]`)
- A builtin `env "KEY"` returning `R t t` (Ok=value, Err=not set) would be minimal. Or `env "KEY"` returning the value or nil, combined with `??`: `k=env "API_KEY"??"default"`.
- This is a 3-char builtin (matches `get`, `len`, `str`, `num`, `abs`, `min`, `max`, `flr`, `cel`, `spl`, `cat`, `has`, `srt`, `slc`, `rev`).

---

### 1.9 Backtick Execution — Deep Dive

**Exact mechanics:**

1. Ruby encounters `` `cmd` `` or `%x(cmd)`
2. Calls `Kernel#`` ` `` method (yes, backtick is a method name)
3. Forks the current process via `fork()`
4. In the child process, calls `exec(cmd)` via `/bin/sh -c cmd`
5. Parent process waits (blocking) for child to complete
6. Captures child's stdout into a String
7. Sets `$?` to `Process::Status` object
8. Returns the captured stdout String

**What gets captured:** Only stdout. Stderr goes to the parent's stderr (visible in terminal but not captured). To capture stderr: `` `cmd 2>&1` `` (redirect in shell) or use `Open3.capture3`.

**Return value:** Always a String (stdout content). Empty string on failure (non-zero exit). The *exit status* is in `$?`, not the return value. This is a design flaw — success/failure is a side channel.

**String interpolation:** Backticks support `#{}` interpolation:
```ruby
file = "data.json"
content = `cat #{file}`
```

**The `%x` alternative:** Identical to backticks but with configurable delimiters. Useful when the command itself contains backticks.

**Overridability:** Since `` ` `` is a Kernel method, it can be overridden:
```ruby
def `(cmd)
  puts "Would run: #{cmd}"
  "mocked output"
end
`dangerous-command`  # => prints warning, returns mock
```

**ilo relevance — the case for a shell builtin:**

Ruby's backtick is essentially: `run_shell(cmd: string) -> stdout: string` with exit status on the side. ilo could do this better:

```
-- Hypothetical shell execution builtin
sh "ls -la"              -- R t t: Ok=stdout, Err=stderr+exitcode
sh! "cat data.json"      -- auto-unwrap: stdout or propagate error
d=sh! "curl -s api.com"  -- capture API response
```

This maps perfectly to ilo's existing patterns:
- Returns `R t t` like `get`
- Works with `!` auto-unwrap
- Works with `?` match for error handling
- 2 chars / 1 token for the builtin name

**Why this matters for agents:** Shell execution is the universal escape hatch. An agent with `sh` can do anything the host system can do — install packages, run scripts, process files, call APIs, interact with git. It's the most powerful single capability an agent can have, and Ruby proved it works with a 1-character syntax.

---

### 1.10 Blocks and Iterators — Composition Model

**How Ruby composes operations:**

```ruby
# Block: anonymous code passed to a method
[1,2,3].map { |x| x * 2 }         # => [2,4,6]
[1,2,3].select { |x| x > 1 }      # => [2,3]
[1,2,3].reduce(0) { |sum, x| sum + x }  # => 6

# Chaining blocks (method chaining)
users
  .select { |u| u.active? }
  .map { |u| u.name }
  .sort
  .first(10)

# yield — inject caller's code into a method
def with_retry(n)
  n.times do |attempt|
    result = yield(attempt)
    return result if result
  rescue => e
    raise if attempt == n - 1
  end
end

with_retry(3) { |i| http_get(url) }

# Proc/Lambda — blocks as objects
doubler = ->(x) { x * 2 }
[1,2,3].map(&doubler)  # => [2,4,6]

# to_proc shorthand
["hello", "world"].map(&:upcase)  # => ["HELLO", "WORLD"]
```

**The composition model:**

Ruby blocks are the key to its expressiveness. They enable:
1. **Internal iteration** — the collection controls traversal, the block provides the logic
2. **Resource management** — `File.open { |f| ... }` guarantees close
3. **Custom control flow** — `with_retry(3) { action }` is a user-defined control structure
4. **Lazy evaluation** — `(1..Float::INFINITY).lazy.select(&:odd?).first(10)`

**Method chaining + blocks** creates a pipeline:
```ruby
data
  .map { |row| parse(row) }        # transform
  .select { |r| r.valid? }         # filter
  .group_by { |r| r.category }     # aggregate
  .transform_values { |v| v.count } # summarize
```

**ilo relevance — what maps to ilo:**

| Ruby Pattern | ilo Equivalent | Token Cost |
|-------------|---------------|------------|
| `arr.map { \|x\| x * 2 }` | `@x xs{*x 2}` | ilo: 6 tokens, Ruby: ~10 |
| `arr.select { \|x\| x > 1 }` | No direct equivalent (need filter builtin or tool) | -- |
| `arr.reduce(0) { \|s,x\| s+x }` | `s=0;@x xs{s=+s x};s` (with while) | ilo: ~12, Ruby: ~12 |
| Method chaining | `>>` pipe operator | Similar |
| `File.open { }` (cleanup) | Tool handles cleanup | N/A |

**Gaps revealed:**
- **`filter`/`select`**: ilo's `@` always produces a list by collecting results. There's no built-in filter. A `flt xs fn` builtin (filter list by predicate) would fill this gap. Or: `@` with a guard that skips elements.
- **`reduce`/`fold`**: No builtin. Expressible with `wh` + accumulator but verbose. A `fld xs init fn` builtin would save significant tokens for aggregation.
- **Block-as-argument**: Ruby passes blocks to methods. ilo has no closures/lambdas/first-class functions. This is intentional (constrained vocabulary), but means patterns like `with_retry(3) { action }` must be expressed differently — via `tool` declarations with `retry:n`.

---

## 2. PHP Capabilities Catalog

### 2.1 What Made PHP Successful for Web

**The low-ceremony design:**

| Feature | PHP | Traditional Languages |
|---------|-----|----------------------|
| Handle HTTP GET | `$_GET["name"]` | Parse request object, extract params |
| Handle HTTP POST | `$_POST["email"]` | Parse request body, decode form data |
| Read environment | `$_ENV["DB_HOST"]` | Import os module, call getenv |
| Read file | `file_get_contents("f.txt")` | Open, read, close (or context manager) |
| HTTP GET request | `file_get_contents("https://...")` | Import HTTP library, create client, call |
| Parse JSON | `json_decode($str)` | Import JSON module, call parse |
| Execute shell | `shell_exec("cmd")` | Import subprocess, configure, call |

**PHP's 4 key success patterns:**

1. **Superglobals are ambient.** `$_GET`, `$_POST`, `$_ENV`, `$_SERVER` are always available. No imports, no initialization, no dependency injection. The HTTP request is *just there*.

2. **URLs are files.** `file_get_contents("https://api.com/data")` uses the same function for local files and HTTP URLs. PHP's stream wrappers make the protocol transparent.

3. **Everything returns immediately usable types.** `json_decode()` returns arrays/objects. `file_get_contents()` returns a string. No wrapper classes, no result objects (until recently).

4. **The program IS the request handler.** In PHP's original model, each `.php` file handles one URL. No routing framework, no application object, no main function. The file executes and the output is the response.

### 2.2 PHP Process Execution

| Function | Returns | stdout | stderr | Exit status |
|----------|---------|--------|--------|-------------|
| `shell_exec($cmd)` | stdout as string (or null) | Captured | No | No |
| `exec($cmd, &$out, &$ret)` | Last line of stdout | Via reference array | No | Via reference |
| `system($cmd, &$ret)` | Last line (also prints) | Prints to output | No | Via reference |
| `passthru($cmd, &$ret)` | void (raw to browser) | Direct passthrough | Direct | Via reference |
| `proc_open(...)` | Resource handle | Via pipes | Via pipes | Via `proc_close()` |

**Notable:** `shell_exec()` is literally aliased to the backtick operator in PHP: `` $output = `ls -la`; `` is identical to `$output = shell_exec("ls -la");`. PHP borrowed Ruby's backtick syntax.

### 2.3 PHP Superglobals — Agent Relevance

| Superglobal | Purpose | Agent Equivalent |
|-------------|---------|-----------------|
| `$_GET` | URL query parameters | Tool call arguments (already in ilo) |
| `$_POST` | Form/request body data | Tool call arguments |
| `$_ENV` | Environment variables | `env` builtin (proposed) |
| `$_SERVER` | Server/request metadata | Runtime context (not needed in ilo) |
| `$_COOKIE` | HTTP cookies | Not relevant for agents |
| `$_SESSION` | Session state | State between tool calls (deferred) |
| `$_FILES` | Uploaded files | Tool concern |
| `$GLOBALS` | All global variables | Anti-pattern for ilo (no globals) |

**Analysis:** The superglobal that matters most for agents is `$_ENV`. Agents need configuration (API keys, endpoints, feature flags) and environment variables are the standard way to provide it. `$_GET`/`$_POST` map to ilo's existing tool parameter mechanism — when a tool is called, arguments are the "request."

### 2.4 PHP Data Formats

| Format | Built-in? | Parse | Generate |
|--------|-----------|-------|----------|
| JSON | Yes | `json_decode($s)` → array/object | `json_encode($v)` |
| XML | Yes (SimpleXML, DOM) | `simplexml_load_string($s)` | Built-in |
| CSV | Yes | `str_getcsv($s)` / `fgetcsv($f)` | `fputcsv($f, $row)` |
| YAML | No (ext: yaml) | `yaml_parse($s)` | `yaml_emit($v)` |
| INI | Yes | `parse_ini_file($f)` | No built-in |

### 2.5 PHP String Manipulation & Regex

PHP uses PCRE (Perl-Compatible Regular Expressions) via `preg_*` functions:

```php
// Match
preg_match('/(\d+)/', 'age: 30', $matches);
$matches[1]  // => "30"

// Replace
preg_replace('/\d+/', 'X', 'age: 30');  // => "age: X"

// Split
preg_split('/\s*,\s*/', 'a, b, c');  // => ["a", "b", "c"]
```

**Notable:** PHP's regex returns `1`/`0`/`false` — three distinct values that are easy to confuse due to loose typing. This is a classic PHP footgun.

### 2.6 PHP Error Handling

PHP evolved from error codes to exceptions:

```php
// Modern try/catch (PHP 5+)
try {
    $data = json_decode(file_get_contents($url), true, 512, JSON_THROW_ON_ERROR);
} catch (JsonException $e) {
    echo "Parse failed: " . $e->getMessage();
}

// Traditional: functions return false on failure
$content = file_get_contents("missing.txt");  // returns false
if ($content === false) {
    // handle error
}
```

**Agent relevance:** PHP's mixed error model (some functions return false, some throw, some set error codes) is exactly what ilo avoids. ilo's uniform `R ok err` + `!` auto-unwrap is cleaner.

### 2.7 PHP Concurrency

PHP is traditionally single-threaded per request. Modern options:
- **Fibers** (PHP 8.1+) — cooperative multitasking
- **parallel** extension — true multi-threading
- **pcntl_fork()** — process forking
- **ReactPHP / Amp** — async event loops

Not particularly relevant for ilo's current design.

---

## 3. Patterns Worth Adopting for ilo

### 3.1 Shell Execution Builtin (from Ruby backticks) -- HIGH VALUE

**The pattern:** Ruby's backtick syntax gives agents shell access in 2 tokens. PHP's `shell_exec()` / backtick alias does the same.

**Proposed ilo equivalent:**

```
-- sh: execute shell command, returns R t t (Ok=stdout, Err=stderr+code)
sh "ls -la"                    -- R t t
d=sh! "cat data.json"         -- auto-unwrap stdout
sh! "git add -A"              -- fire-and-forget (discard stdout)
sh "curl -s https://api.com"  -- but prefer get for HTTP
```

**Why:**
- `sh` is 2 chars / 1 token, matches the builtin naming convention
- Returns `R t t` — consistent with `get`
- Works with `!` auto-unwrap and `?` match
- Gives agents universal system access — file ops, git, package management, anything
- The tool mechanism is for structured API calls; `sh` is for unstructured system commands
- Combined with `get` for HTTP and `sh` for shell, an agent can interact with almost anything

**Token comparison:**
```
-- Read a file:
d=sh! "cat data.json"      -- 6 tokens in ilo
`cat data.json`             -- 3 tokens in Ruby (but no error handling)

-- With error handling:
sh "cat data.json";?{~d:use d;^e:^e}   -- 12 tokens in ilo (typed, explicit)
```

**Tradeoff:** `sh` is inherently untyped — it returns text. The agent must parse the output. This is the bash problem ilo was designed to avoid. Mitigation: `sh` is the escape hatch, not the primary interface. Typed tools are preferred; `sh` is for when no tool exists.

### 3.2 Environment Variable Access (from Ruby ENV / PHP $_ENV) -- HIGH VALUE

**The pattern:** Both Ruby (`ENV["KEY"]`) and PHP (`$_ENV["KEY"]`) provide direct access to environment variables.

**Proposed ilo equivalent:**

```
-- env: read environment variable
k=env "API_KEY"              -- returns value or nil
k=env "API_KEY"??"default"   -- with nil-coalesce for defaults
k=env "PORT";p=num k??3000   -- parse to number with default
```

**Why:**
- Agents need configuration: API keys, endpoints, database URLs
- `env` is 3 chars / 1 token, matches existing builtins
- Returns `t` or nil (not `R t t` — missing env vars aren't "errors", they're absent values)
- Composes with `??` for defaults
- No import or setup required (like PHP superglobals — just there)

### 3.3 Regex Match Builtin (from Ruby regex / PHP preg_match) -- MEDIUM VALUE

**The pattern:** Both Ruby (`=~`, `.match`, `.gsub`) and PHP (`preg_match`, `preg_replace`) provide built-in regex.

**Proposed ilo equivalent:**

```
-- rx: regex match, returns R L t t (Ok=list of captures, Err=no match)
rx "age: 30" "(\d+)"        -- R L t t: Ok=["30"]
m=rx! text pattern           -- auto-unwrap captures

-- rxr: regex replace
rxr "age: 30" "\d+" "X"     -- "age: X"
```

**Why:**
- Agents frequently extract structured data from text (parse URLs, dates, numbers, log lines)
- Without regex, text extraction requires chaining `spl`/`has`/`slc` — error-prone and verbose
- `rx` is 2 chars / 1 token
- Returns `R L t t` — captures as list of text, consistent with ilo types

**Tradeoff:** Regex patterns are notoriously token-expensive. `/(\d{4})-(\d{2})-(\d{2})/` is many tokens. But the alternative (manual parsing) is even more tokens. Regex is net-positive for token cost.

### 3.4 Filter/Reduce Builtins (from Ruby blocks + Enumerable) -- MEDIUM VALUE

**The pattern:** Ruby's `select`/`reject`/`reduce` with blocks compose operations on collections.

**The gap:** ilo's `@` iterates and collects. There's no filter or fold.

**Proposed:**

```
-- Today: filter requires manual accumulation
r=[];@x xs{>x 5{+=r x}};r

-- Proposed: flt builtin with inline predicate
-- Option A: builtin with expression (needs lambda/expression-as-value)
-- Option B: keep using @ with conditional append (already works)
```

**Assessment:** This is harder to adopt without first-class functions. Ruby's blocks work because you can pass code to a method. ilo intentionally lacks closures. The current `@` + guard + `+=` pattern is verbose but works. A `flt` builtin would need a way to express the predicate — this likely requires a design decision about anonymous expressions or lambda syntax.

**Defer until ilo considers closures or expression parameters.**

### 3.5 PHP's "URLs Are Files" Pattern -- ALREADY ADOPTED

**The pattern:** PHP's `file_get_contents($url)` treats URLs like files.

**ilo status:** Already adopted. `get url` is the equivalent — one terse builtin for HTTP. ilo goes further by making it even more concise (`$url` alias) and adding typed error handling (`R t t`). This is one area where ilo is already ahead of both Ruby and PHP.

### 3.6 Ruby's Pipe/Chain Composition -- ALREADY ADOPTED

**The pattern:** Ruby's method chaining (`arr.map{}.select{}.first`) composes operations linearly.

**ilo status:** Already adopted via `>>` pipe operator: `f x>>g>>h` desugars to `h(g(f(x)))`. This is the right model for agents — linear composition without naming intermediates.

---

## 4. Patterns NOT Worth Adopting

### 4.1 Exception Handling (Ruby begin/rescue/ensure)

**Why not:** ilo's Result type (`R ok err`) is strictly superior for agents. Exceptions are invisible in signatures. Results are explicit. Auto-unwrap (`!`) gives the ergonomic benefits of exceptions without the hidden control flow.

### 4.2 Concurrency Primitives (Ruby threads/fibers/Ractors)

**Why not (yet):** Agent programs are typically short-lived scripts, not long-running services. Concurrency matters at the runtime level (parallel tool calls) not the language level. Defer until agent patterns demand it.

### 4.3 PHP Superglobals (beyond ENV)

**Why not:** `$_GET`, `$_POST`, `$_SERVER` model a web request-response cycle. ilo agents don't serve HTTP — they consume APIs. Tool parameters replace GET/POST. The only superglobal worth adopting is `$_ENV` (as the `env` builtin above).

### 4.4 Ruby's Custom Exception Hierarchies

**Why not:** Error hierarchies require a class system. ilo's errors are text values in the Err branch of Result. Pattern matching on error text (`?r{^e:has e "timeout"{retry};^e:^e}`) is sufficient. Custom error types add vocabulary without proportional value.

### 4.5 Block/Lambda/Closure (Ruby blocks, Proc, Lambda)

**Why not (yet):** First-class functions increase the vocabulary and the valid-next-token space. ilo's "constrained" principle says fewer choices at each step. The `@` iterator + `>>` pipe + `?` match cover the most common composition patterns without closures. Revisit only if filter/fold patterns become frequent enough that the `@` + guard workaround costs more total tokens than adding lambdas.

---

## 5. Summary: Recommended Additions

| Addition | Source | Chars | Tokens | Returns | Priority |
|----------|--------|-------|--------|---------|----------|
| `sh cmd` | Ruby backticks | 2 | 1 | `R t t` | High |
| `env key` | Ruby ENV / PHP $_ENV | 3 | 1 | `t` or nil | High |
| `rx text pat` | Ruby regex / PHP preg_match | 2 | 1 | `R L t t` | Medium |
| `rxr text pat rep` | Ruby gsub / PHP preg_replace | 3 | 1 | `t` | Medium |
| `post url body` | Ruby Net::HTTP / PHP streams | 4 | 1 | `R t t` | Medium |

**What ilo already does better than both Ruby and PHP:**
- HTTP GET: `get url` / `$url` — more concise than any Ruby or PHP equivalent
- Error handling: `R ok err` + `!` + `?` — typed, explicit, composable, 0 hidden control flow
- Tool composition: typed parameters, verified before execution, closed world
- Process execution model (when `sh` is added): `sh cmd` → `R t t` is cleaner than Ruby's backtick+`$?` split or PHP's `shell_exec()` returning null on error

**The big insight from this research:** Ruby and PHP both validate ilo's core design — concise builtins, minimal ceremony, treating external resources (HTTP, files, shell) as simple function calls returning usable values. Where they differ is in error handling and typing, and ilo's approach (typed Results) is superior for agent use. The main capability gaps are shell execution (`sh`) and environment access (`env`), both of which are trivial to add and would dramatically expand what an agent can do.
