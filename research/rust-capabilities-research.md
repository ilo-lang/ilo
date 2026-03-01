# Rust Standard Library & Language Features: Research for ilo

Rust's ownership model, type system, and standard library analyzed through ilo's lens: **total tokens from intent to working code**. Rust is the most technically relevant comparison language because ilo is implemented in Rust, shares Rust's explicit error handling philosophy (Result/Option, no exceptions), and targets a similar safety-before-execution ethos. This document catalogs Rust's capabilities, identifies gaps in ilo, and proposes minimal additions where the gap matters for AI agent workloads.

---

## Why Rust matters for ilo

Rust and ilo share deep structural commitments:

1. **Explicit error handling.** Rust has `Result<T, E>` and `Option<T>`. ilo has `R ok err` and the planned `O n`. Neither uses exceptions. Both force the programmer to handle failure structurally.
2. **The `?` operator.** Rust's `?` auto-propagates errors up the call stack. ilo's `!` does the same thing in one character less (`get! url` vs `get(url)?`). Same semantics, terser syntax.
3. **Exhaustive pattern matching.** Rust's `match` requires covering all variants. ilo's `?` matching checks exhaustiveness via verifier (ILO-T024). Both catch missed cases before runtime.
4. **Verification before execution.** Rust refuses to compile unsound code. ilo verifies all calls resolve, all types align, all dependencies exist before executing anything.
5. **No null.** Rust encodes absence as `Option<T>`. ilo has `_` (nil) at runtime but is adding `O n` (E2) to enforce nil-checking at the type level.

Where they diverge: Rust prioritizes memory safety (ownership, lifetimes, borrowing). ilo has garbage collection (Rc/clone in the interpreter, value types in the VM) and does not need ownership semantics. Rust is general-purpose; ilo is agent-purpose. Rust optimizes for zero-cost abstractions; ilo optimizes for zero-cost token generation.

---

## 1. Error Handling

### What Rust provides

```rust
// Result<T, E> — success or failure with typed error
fn get_user(id: u64) -> Result<User, DbError> { ... }

// Option<T> — present or absent
fn find_item(name: &str) -> Option<Item> { ... }

// ? operator — propagate error to caller
fn process(id: u64) -> Result<String, AppError> {
    let user = get_user(id)?;           // propagate DbError
    let profile = get_profile(&user)?;  // propagate ProfileError
    Ok(format!("{}: {}", user.name, profile.bio))
}

// From/Into conversions — automatic error type widening
impl From<DbError> for AppError {
    fn from(e: DbError) -> Self { AppError::Db(e) }
}

// map, and_then, unwrap_or, unwrap_or_else on Result/Option
let name = get_user(id)
    .map(|u| u.name)
    .unwrap_or("unknown".to_string());

// ok_or / ok_or_else — Option to Result conversion
let item = find_item("key").ok_or(AppError::NotFound)?;
```

### What ilo has

| Rust | ilo | Status |
|------|-----|--------|
| `Result<T, E>` | `R ok err` | Implemented |
| `Option<T>` | `O n` | Planned (E2) |
| `?` operator | `!` auto-unwrap | Implemented |
| `From<E>` conversions | None | Gap |
| `.map()` on Result | None | Gap |
| `.unwrap_or()` | `??` (nil-coalesce) | Implemented (for nil) |
| `.and_then()` | `!` chaining | Partial |
| `ok_or()` (Option->Result) | None | Gap |

### Gap analysis

**1a. Error type conversion (From/Into)**

Rust's `?` operator calls `.into()` on the error type, allowing automatic widening: a function returning `Result<T, AppError>` can use `?` on a `Result<T, DbError>` if `From<DbError> for AppError` exists. This means different error types compose seamlessly.

ilo's `!` only works when the caller and callee have the same error type. If `get-user` returns `R profile t` and `charge` returns `R receipt t`, chaining works because both error types are `t`. But if error types differ, `!` cannot auto-convert.

**Does this matter for agents?** Moderately. In practice, most ilo tools return `R ok t` where the error type is always text. Rust's rich error type hierarchies exist because Rust programs are long-lived and need to distinguish error kinds for recovery. Agent programs are short-lived and typically either retry or propagate. Text errors are sufficient for 90% of agent use cases.

**Minimal fix if needed:** No language change required. Convention is sufficient: all tool errors are `t` (text). If typed errors become necessary, sum types (E3) provide the mechanism, and a future `From`-like trait (E6) could enable auto-conversion. But this is very low priority.

**1b. Combinators on Result (map, and_then, unwrap_or)**

Rust's Result combinators enable functional-style error handling without `match`:

```rust
let name = get_user(id)
    .map(|u| u.name)                    // transform Ok value
    .unwrap_or("unknown".to_string());  // default on Err
```

ilo's equivalent today:

```
r=get-user id;?r{~u:u.name;^_:"unknown"}
```

This is 11 tokens vs Rust's ~12 tokens (after tokenization). The gap is small. With pipes (already implemented), the pattern becomes slightly more composable, but Result combinators require lambda syntax (E5) to be truly useful.

**Does this matter for agents?** Low. The match-based pattern in ilo is already terse. Combinators save tokens only for simple transforms, and ilo's `!` handles the most common case (propagation) in 1 token.

**1c. Option-to-Result conversion (ok_or)**

Rust's `option.ok_or(error)` converts `None` to `Err(error)`. This bridges optional values into the error-handling pipeline.

ilo's equivalent would be:

```
?val{_:^"not found";~v:~v}
```

7 tokens. A dedicated conversion is slightly more terse but infrequent enough to defer.

**Does this matter for agents?** Low. The pattern appears occasionally (checking if a key exists before using it), but it is well-served by the existing match syntax. If Optional (E2) lands, `val??^"not found"` could work as an idiomatic pattern with nil-coalesce producing an Err, but this needs design thought.

### Verdict

ilo's error handling is strong. The `R`/`!`/`?` trio covers the same ground as Rust's `Result`/`?`/`match`. The main gap is Optional (`O`), which is tracked as E2 and is the next type system priority. Error type conversion and Result combinators are low priority -- the text-error convention and match syntax are sufficient for agents.

---

## 2. Traits and Generics

### What Rust provides

```rust
// Generics — write once, use with any type
fn first<T>(items: &[T]) -> Option<&T> {
    items.first()
}

// Trait — shared behavior across types
trait Summary {
    fn summarize(&self) -> String;
}

impl Summary for Article {
    fn summarize(&self) -> String {
        format!("{}: {}", self.title, self.author)
    }
}

// Trait bounds — constrain generic types
fn notify<T: Summary>(item: &T) {
    println!("Breaking: {}", item.summarize());
}

// Iterator trait — the foundation of collection processing
trait Iterator {
    type Item;
    fn next(&mut self) -> Option<Self::Item>;
}
```

### What ilo has

ilo is fully monomorphic. No type variables, no polymorphism, no generics. Every function has a fixed, concrete type signature.

### Gap analysis

**2a. Generics for builtins (map, filter, fold)**

The biggest pain point: ilo cannot type-check generic list operations. `map f xs` needs the signature `fn(fn(a>b), L a) > L b`, which requires type variables. Without generics, map/filter/fold are either:

- Untyped builtins (verifier skips them) -- loses the "verification before execution" guarantee
- Per-type variants (`map-n`, `map-t`) -- combinatorial explosion
- Inline loops (`@x xs{...}`) -- verbose but fully typed

The inline loop is ilo's current answer:

```
-- Rust: items.iter().map(|x| x * 2).collect::<Vec<_>>()
-- ilo:  @x items{*x 2}
```

The ilo version is 5 tokens. Rust's is ~13 tokens. ilo wins on token count already. The issue is not terseness -- it is composability. You cannot chain `@` loops without intermediate variables:

```
-- Rust: items.iter().filter(|x| x > 5).map(|x| x * 2).sum()
-- ilo:  a=@x items{>x 5{x}};b=@x a{*x 2};s=0;@x b{s=+s x};s
```

15 tokens vs Rust's ~15 tokens. But with generic builtins + pipe:

```
-- ilo (future): flt {>_ 5} items>>map {*_ 2}>>fld + 0
```

~12 tokens. Marginal savings, but much more readable for complex chains.

**Does this matter for agents?** Medium. Agents generate list processing frequently (filtering API results, transforming data, aggregating). The inline `@` loop works but requires more structural tokens. Generic builtins would reduce generation complexity.

**2b. Traits / interfaces**

Rust traits enable:
- Polymorphic dispatch (call `.summarize()` on any type that implements Summary)
- Operator overloading (via `Add`, `Display`, etc.)
- Collection abstractions (Iterator, IntoIterator)

**Does this matter for agents?** No. As noted in ilo's TYPE-SYSTEM.md: "Agents generate concrete code for specific tasks. They don't write frameworks or abstract interfaces." Traits solve the human problem of code reuse across a codebase maintained over time. Agent programs are disposable -- generated fresh per task. There is no codebase to maintain, no library to abstract.

**2c. Associated types and type-level computation**

Rust's associated types (`type Item` in Iterator), const generics (`[T; N]`), and GATs (generic associated types) are powerful but complex. They solve problems that arise in large-scale library design.

**Does this matter for agents?** No. These features add complexity to type annotations without reducing agent error rates. The type annotation tax (already 17-22% of ilo program tokens) would increase significantly.

### Verdict

Generics (E5) are the one feature from this category worth pursuing, specifically to enable typed `map`/`flt`/`fld` builtins. Traits (E6) and associated types are out of scope. The implementation order is correct: E5 is fifth priority, gated on lambda syntax, and E6 is deferred indefinitely.

---

## 3. Iterators and Collection Processing

### What Rust provides

Rust's iterator system is its crown jewel for data processing:

```rust
// Lazy iterator chain — nothing allocates until .collect()
let result: Vec<i32> = items.iter()
    .filter(|x| **x > 5)
    .map(|x| x * 2)
    .take(10)
    .collect();

// fold/reduce
let sum: i32 = items.iter().fold(0, |acc, x| acc + x);
let sum: i32 = items.iter().sum(); // sugar for fold with Add

// zip — pair two iterators
let pairs: Vec<_> = names.iter().zip(scores.iter()).collect();

// enumerate — index + value
for (i, item) in items.iter().enumerate() { ... }

// chain — concatenate iterators
let all: Vec<_> = first.iter().chain(second.iter()).collect();

// find, any, all, position, count
let found = items.iter().find(|x| **x > 100);
let has_big = items.iter().any(|x| *x > 100);

// flat_map — map + flatten
let words: Vec<&str> = lines.iter().flat_map(|l| l.split(' ')).collect();

// partition — split by predicate
let (big, small): (Vec<_>, Vec<_>) = items.iter().partition(|x| **x > 50);

// windows, chunks — sliding/fixed-size views
for window in items.windows(3) { ... }
for chunk in items.chunks(5) { ... }
```

### What ilo has

| Rust iterator | ilo equivalent | Tokens | Gap? |
|--------------|----------------|--------|------|
| `.map(\|x\| x*2)` | `@x xs{*x 2}` | 5 | None -- ilo is terser |
| `.filter(\|x\| x>5)` | `@x xs{>x 5{x}}` | 7 | Slight gap -- guard-as-filter is verbose |
| `.fold(0, \|a,x\| a+x)` | `s=0;@x xs{s=+s x};s` | 8 | Gap -- `fld + 0 xs` would be 4 |
| `.sum()` | `s=0;@x xs{s=+s x};s` | 8 | Same gap as fold |
| `.collect()` | Implicit (@ always returns list) | 0 | ilo wins -- no allocation ceremony |
| `.zip(other)` | No equivalent | -- | Gap |
| `.enumerate()` | No equivalent | -- | Gap |
| `.chain(other)` | `+xs ys` (list concat) | 3 | None -- already concise |
| `.find(\|x\| x>100)` | `@x xs{>x 100{ret x}}` | 8 | Gap -- ret-from-loop workaround |
| `.any(\|x\| x>100)` | `@x xs{>x 100{ret true}};false` | 10 | Gap |
| `.flat_map(f)` | No equivalent | -- | Gap |
| `.partition(pred)` | No equivalent | -- | Gap |
| `.take(n)` | `slc xs 0 n` | 4 | None -- slice covers this |
| `.skip(n)` | `slc xs n (len xs)` | 5 | Slight gap -- needs len call |
| `.rev()` | `rev xs` | 2 | None |
| `.sort()` | `srt xs` | 2 | None |
| `.count()` | `len xs` | 2 | None |

### Gap analysis

**3a. Fold/reduce -- the biggest iterator gap**

Sum, product, max, min of a list -- these are among the most common list operations in agent code (aggregating API results, computing totals, finding extremes). ilo requires 8 tokens for a sum:

```
s=0;@x xs{s=+s x};s
```

Rust requires ~8 tokens too (`.iter().fold(0, |a,x| a+x)`), but has sugar: `.sum()` is 2 tokens.

ilo's planned `fld` builtin would match:
```
fld + 0 xs     -- 4 tokens
```

Gates on E5 (generics). However, a monomorphic special-case approach could work sooner: hardcode `fld` for `+`, `*`, `max`, `min` on `L n` without full generics. This is the approach recommended in TODO.md.

**Does this matter for agents?** Yes. Aggregation appears in nearly every data-processing task. Saving 4 tokens per aggregation adds up.

**Minimal fix:** Implement `fld` as a monomorphic builtin for numeric lists first. Signature: `fld op:t init:n xs:L n > n` where `op` is one of `"+"`, `"*"`, `"max"`, `"min"`. No generics needed. 4 tokens for any numeric aggregation.

**3b. Enumerate -- index + value iteration**

Rust's `.enumerate()` pairs each element with its index. Common when agents need position-aware processing (e.g., "process the third item differently").

ilo's workaround:
```
i=0;@x xs{process i x;i=+i 1}    -- 10 tokens
```

With range iteration (F7, planned):
```
@i 0..len xs{x=xs.i;process i x}  -- 8 tokens
```

**Does this matter for agents?** Medium. Index-aware iteration appears in ~20% of list processing tasks. Range iteration (F7) partially addresses this.

**Minimal fix:** Range iteration (F7) is the pragmatic answer. A dedicated `enum` builtin (`@ix xs{...}` where `i` and `x` are bound) would save 2-3 tokens but adds parsing complexity.

**3c. Zip -- parallel iteration**

Rust's `.zip()` pairs elements from two iterators. Used for correlating two lists (names + scores, keys + values).

ilo has no equivalent. Workaround:
```
@i 0..len xs{a=xs.i;b=ys.i;process a b}  -- 10 tokens
```

**Does this matter for agents?** Low-medium. Zip appears in data correlation tasks but is less common than map/filter/fold.

**Minimal fix:** A `zip` builtin returning `L L` (list of pairs) would be natural: `zs=zip xs ys;@p zs{p.0 p.1}`. But this requires tuples or nested lists as pairs, which adds complexity. Defer until range iteration (F7) provides the workaround.

**3d. Find -- first match**

Finding the first element matching a predicate. Common in agent lookups.

ilo's workaround with `ret`:
```
@x xs{>x 100{ret x}};_    -- 8 tokens
```

This works with early return (F5, implemented). The 8-token cost is acceptable.

**Does this matter for agents?** Low. The ret-from-loop pattern is clear and functional.

**3e. Flat_map -- map + flatten**

Transforms each element into a list and concatenates. Used for "split all lines into words" or "get all items from all orders."

ilo has no equivalent. Workaround:
```
r=[];@x xs{ys=get-items x;r=+r ys};r    -- 11 tokens
```

**Does this matter for agents?** Low. Nested list flattening appears occasionally. A `flat` builtin (flatten one level) plus `@` loop covers it.

### Verdict

ilo's `@` loop is surprisingly competitive with Rust's iterators for simple cases. The main gaps:

1. **Fold/reduce** -- high priority. Implement `fld` as monomorphic builtin (no generics needed).
2. **Enumerate/index loops** -- medium priority. Range iteration (F7) is the answer.
3. **Zip, flat_map, partition** -- low priority. Workarounds exist via range iteration + concat.

The lazy evaluation aspect of Rust iterators (no intermediate allocations) is irrelevant for ilo -- agent programs process small datasets. Allocation efficiency is a non-concern.

---

## 4. Concurrency

### What Rust provides

```rust
// async/await (with tokio)
async fn fetch_all(urls: Vec<String>) -> Vec<Result<String, Error>> {
    let futures: Vec<_> = urls.into_iter()
        .map(|url| reqwest::get(url))
        .collect();
    futures::future::join_all(futures).await
}

// Channels (mpsc)
let (tx, rx) = tokio::sync::mpsc::channel(100);
tokio::spawn(async move {
    tx.send("hello").await.unwrap();
});
let msg = rx.recv().await.unwrap();

// Mutex, RwLock, Arc for shared state
let counter = Arc::new(Mutex::new(0));

// tokio::spawn — lightweight tasks
tokio::spawn(async { long_running_task().await });

// select! — wait for first of multiple futures
tokio::select! {
    msg = rx.recv() => handle_message(msg),
    _ = tokio::time::sleep(Duration::from_secs(5)) => handle_timeout(),
}

// Streams — async iterators
let mut stream = tokio_stream::wrappers::ReceiverStream::new(rx);
while let Some(item) = stream.next().await {
    process(item);
}
```

### What ilo has

ilo is fully synchronous. `get url` blocks until the response arrives. There is no async, no spawn, no channels, no parallel execution.

### Gap analysis

**4a. Parallel tool calls**

The single most important concurrency pattern for agents: call multiple independent tools simultaneously.

```rust
// Rust: parallel HTTP requests
let (user, orders, profile) = tokio::join!(
    get_user(id),
    get_orders(id),
    get_profile(id),
);
```

ilo today (sequential):
```
u=get-user! id;o=get-orders! id;p=get-profile! id
```

Three sequential HTTP calls. If each takes 200ms, total is 600ms. Parallel would be ~200ms.

**Does this matter for agents?** Yes. Agent workloads are I/O-bound. An agent orchestrating API calls spends most time waiting for responses. Parallel calls directly reduce wall-clock time.

**Minimal fix:** The planned `par{...}` syntax (G4) is correct:
```
par{u=get-user id;o=get-orders id;p=get-profile id}
```

Internally async (tokio), externally sequential from the ilo program's perspective. The agent does not need to think about concurrency -- the runtime parallelizes independent calls. This is the right design: no new concepts (promises, futures, await), just a block that says "run these in parallel."

Token cost: 1 token (`par`) + braces. Net saving: none in tokens, significant in wall-clock time.

**4b. Async/await as a language feature**

Should ilo expose async/await to the programmer?

Rust requires explicit async/await because it has no runtime -- the programmer must choose an executor (tokio, async-std) and manage future lifetimes. This explicitness is the Rust way.

For ilo, the answer is clearly **no**. Reasons:

1. **Manifesto: "one way to do things."** Adding async/await creates two worlds: sync functions and async functions. This doubles the vocabulary.
2. **Agents do not manage concurrency.** An agent generates a sequence of tool calls. The runtime manages parallelism. This is the same philosophy as SQL: you describe what you want, the engine decides how to execute it.
3. **Token cost.** `async`, `await`, promise types, error handling across async boundaries -- all add tokens with no reduction in agent error rate.

The correct approach is runtime-managed parallelism behind `par{}`, with the ilo program remaining sequential.

**4c. Channels and message passing**

Rust's channels (mpsc, broadcast, watch) enable inter-task communication. Relevant for:
- WebSocket event handling (G2)
- Process output streaming (G3)
- Long-running background tasks

ilo's planned design uses blocking calls (`ws-recv conn`) rather than channels. This is simpler and sufficient for CDP-style request-response patterns.

**Does this matter for agents?** Low for now. If ilo needs event-driven patterns (multiple WebSocket connections, SSE streams), channels or a `poll`/`select` mechanism becomes necessary. But this is deferred to G9 (event/callback model).

**4d. Shared state (Mutex, Arc)**

Rust's ownership model makes shared mutable state explicit via `Arc<Mutex<T>>`. This prevents data races at compile time.

**Does this matter for agents?** No. ilo programs are single-threaded from the language's perspective. If `par{}` runs calls in parallel, each call is independent -- no shared mutable state. The runtime manages internal synchronization. The agent never needs Mutex/Arc.

### Verdict

Concurrency is a runtime concern, not a language concern for ilo. The right additions:

1. **`par{...}`** -- parallel tool calls (G4). High priority for agent performance.
2. **Runtime-internal async** -- tokio behind the scenes for non-blocking I/O. Already planned (D1d, G4).
3. **No language-level async/await.** No channels, no mutexes, no futures in the language.

This matches Go's original philosophy of hiding concurrency behind goroutines + channels at the runtime level, except ilo goes further by hiding channels too. The agent writes sequential code; the runtime optimizes.

---

## 5. File I/O

### What Rust provides

```rust
// Read file — one-liner
let content = std::fs::read_to_string("config.json")?;

// Write file — one-liner
std::fs::write("output.txt", content)?;

// Path manipulation
let path = Path::new("/home").join("user").join("file.txt");
let ext = path.extension();
let parent = path.parent();
let exists = path.exists();

// Directory listing
for entry in std::fs::read_dir("./")? {
    let entry = entry?;
    println!("{}", entry.path().display());
}

// Metadata
let meta = std::fs::metadata("file.txt")?;
println!("size: {}, modified: {:?}", meta.len(), meta.modified());

// Buffered reading (large files)
let file = File::open("big.txt")?;
let reader = BufReader::new(file);
for line in reader.lines() {
    let line = line?;
    process(line);
}

// Create directories recursively
std::fs::create_dir_all("path/to/nested/dir")?;

// Temp files
let tmp = tempfile::NamedTempFile::new()?;
```

### What ilo has

Nothing. No file system access. Already planned as G8.

### Gap analysis

**5a. Read/write files**

The most fundamental agent operation after HTTP. Agents read configs, source files, data files. They write generated code, logs, results.

Rust's `fs::read_to_string` / `fs::write` are one-liners. ilo's planned equivalents:

| Rust | ilo (planned G8) | Tokens |
|------|-------------------|--------|
| `fs::read_to_string("f")?` | `fread! "f"` | 2 |
| `fs::write("f", data)?` | `fwrite! "f" data` | 3 |
| `path.exists()` | `fexists "f"` | 2 |

ilo is terser. The design is correct: simple builtins, `R` return type, `!` auto-unwrap.

**Does this matter for agents?** Yes. File I/O is essential. This is a blocking gap.

**Minimal fix:** Implement G8 (`fread`, `fwrite`, `fexists`, `fappend`) as builtins behind the `fs` feature flag. No path manipulation needed -- agents know the full path.

**5b. Directory listing**

Agents need to discover what files exist (find source files, list configs, scan directories).

Rust: `fs::read_dir` returns an iterator of `DirEntry`. ilo planned: `fls path > R L t t` (returns list of file names or error).

**Does this matter for agents?** Medium. Listing directories is needed for discovery tasks but less frequent than read/write.

**5c. Path manipulation**

Rust's `Path`/`PathBuf` handle OS-specific separators, extension extraction, parent directories, etc.

**Does this matter for agents?** No. As noted in the Go research: "ilo programs run through an agent that knows the OS. Path manipulation is a tool concern." Agents construct full paths as text strings. They do not need `Path::join` or `extension()`.

**5d. Buffered reading / streaming**

For large files, Rust provides `BufReader` for line-by-line iteration. ilo's planned stream model (G6) addresses this: `@line stream{...}`.

**Does this matter for agents?** Low. Agent programs process small-to-medium files. Reading entire files into memory (`fread`) covers 95% of cases. Streaming is needed only for log tailing or very large datasets.

### Verdict

File I/O is a real gap. G8 (`fread`/`fwrite`/`fexists`/`fappend`) should be prioritized. Path manipulation and streaming are unnecessary for the common case. The design is already correct in the roadmap.

---

## 6. String Processing

### What Rust provides

```rust
// Basic operations
let upper = s.to_uppercase();
let lower = s.to_lowercase();
let trimmed = s.trim();
let replaced = s.replace("old", "new");
let contains = s.contains("needle");
let starts = s.starts_with("prefix");
let ends = s.ends_with("suffix");

// Split and join
let parts: Vec<&str> = s.split(',').collect();
let joined = parts.join(", ");

// Formatting
let msg = format!("Hello, {}! You have {} items.", name, count);

// Regex (regex crate — not stdlib but de facto standard)
let re = Regex::new(r"\d+")?;
let matches: Vec<&str> = re.find_iter(text).map(|m| m.as_str()).collect();
let replaced = re.replace_all(text, "***");

// Parsing
let n: i32 = "42".parse()?;
let f: f64 = "3.14".parse()?;
```

### What ilo has

| Rust | ilo | Status |
|------|-----|--------|
| `split` | `spl t sep` | Implemented |
| `join` | `cat xs sep` | Implemented |
| `contains` | `has t sub` | Implemented |
| `len` | `len t` | Implemented |
| `starts_with` / `ends_with` | None | Gap |
| `trim` | None | Gap |
| `replace` | None | Gap |
| `to_uppercase` / `to_lowercase` | None | Gap |
| `format!` (interpolation) | None | Gap |
| `parse` (to number) | `num t` | Implemented |
| `to_string` (from number) | `str n` | Implemented |
| Regex | None | Gap |
| `slice` | `slc t a b` | Implemented |
| `reverse` | `rev t` | Implemented |
| `sort` (chars) | `srt t` | Implemented |

### Gap analysis

**6a. String formatting / interpolation**

The biggest string processing gap. Every language has some form of string interpolation:

```rust
format!("Hello, {}! Total: ${:.2}", name, total)
```

ilo has only string concatenation:
```
m=+"Hello, " name;m=+m "! Total: $";m=+m (str total)
```

12 tokens for what format does in ~8. The gap widens with more interpolation points.

**Does this matter for agents?** Yes. Agents construct messages, prompts, URLs, and payloads constantly. String interpolation is high-frequency.

**Minimal fix:** A `fmt` builtin or template syntax. Options:

Option A -- positional `fmt`:
```
fmt "Hello, {}! Total: ${}" name (str total)    -- 7 tokens
```

Option B -- embedded references (like shell):
```
"Hello, {name}! Total: ${total}"    -- 3 tokens but changes string semantics
```

Option B is far more token-efficient but requires parser changes to string literals. Option A is safer -- a new builtin with `{}` placeholders, no language changes.

Recommendation: `fmt` builtin. `fmt template args...` where `{}` is replaced by successive arguments. Returns `t`. Token cost: 1 extra token for `fmt`. Token savings: 3-8 per interpolation.

**6b. Trim, replace, starts_with, ends_with**

Common string operations missing from ilo's builtins:

| Operation | Proposed | Tokens |
|-----------|----------|--------|
| `s.trim()` | `trm t` | 2 |
| `s.replace(a, b)` | `rep t old new` | 4 |
| `s.starts_with(p)` | `pfx t p` | 3 |
| `s.ends_with(s)` | `sfx t s` | 3 |
| `s.to_uppercase()` | `upc t` | 2 |
| `s.to_lowercase()` | `lwc t` | 2 |

**Does this matter for agents?** Medium. `replace` and `trim` appear frequently in data cleaning. `starts_with`/`ends_with` appear in routing and classification. These are workaround-able with `spl`/`slc`/`has` but cost more tokens.

**Minimal fix:** Add `trm` and `rep` as builtins (highest frequency). `pfx`/`sfx` can wait. Case conversion is low priority -- agents rarely need it.

**6c. Regex**

Rust's `regex` crate is external but ubiquitous. Regex is the universal tool for pattern matching in text.

**Does this matter for agents?** Low-medium. Regex is powerful but error-prone for LLM generation. LLMs frequently generate incorrect regex patterns, leading to retry loops. This directly conflicts with ilo's manifesto: regex increases retry cost.

**Minimal fix:** If needed, a `re` builtin: `re pattern text > R L t t` (returns matches or error). But defer until a compelling agent use case emerges. Most text extraction is better handled by tools (JSON parsing via `jp`, HTML parsing via tools) than by regex in the language.

### Verdict

String formatting (`fmt`) is the highest-priority gap. `trm` and `rep` builtins are medium priority. Regex is low priority and potentially counterproductive for LLM generation accuracy.

---

## 7. Serialization (Serde / JSON)

### What Rust provides

```rust
// Derive-based serialization
#[derive(Serialize, Deserialize)]
struct User {
    name: String,
    age: u32,
    email: Option<String>,
}

// Serialize to JSON
let json = serde_json::to_string(&user)?;

// Deserialize from JSON
let user: User = serde_json::from_str(&json)?;

// Dynamic JSON (serde_json::Value)
let v: serde_json::Value = serde_json::from_str(&json)?;
let name = v["name"].as_str();
let items = v["items"].as_array();

// JSON path access
let deep = v["users"][0]["address"]["city"].as_str();

// Flexible formats — same derive works for TOML, YAML, MessagePack, etc.
let toml: Config = toml::from_str(&toml_string)?;
```

### What ilo has

ilo internally uses serde_json for AST serialization and JSON error output. At the language level:

| Capability | ilo status |
|-----------|-----------|
| JSON parse (text -> value) | Planned (I1: `jp`) |
| JSON path access | Planned (I1: `jp text path`) |
| JSON serialize (value -> text) | Not planned |
| Record <-> JSON mapping | Planned (D1e) |
| Dynamic JSON (untyped) | `t` escape hatch |

### Gap analysis

**7a. JSON parsing and path access**

The most common agent operation after HTTP: parse a JSON response and extract a field.

Rust:
```rust
let v: Value = serde_json::from_str(&body)?;
let name = v["data"]["user"]["name"].as_str().unwrap_or("unknown");
```

ilo planned (I1):
```
n=jp! body "data.user.name"
```

3 tokens. Dramatically terser. The `jp` (JSON path) builtin is the right design -- it combines parsing and path extraction in one call.

**Does this matter for agents?** Yes. JSON parsing is the #1 data extraction operation for API-consuming agents.

**Minimal fix:** Implement I1 (`jp` builtin). High priority.

**7b. JSON serialization (value -> text)**

Converting ilo values to JSON text for API request bodies.

ilo planned: D1e (Value <-> JSON mapping) handles this at the tool boundary. Records auto-serialize to JSON when passed to tools. But there is no explicit "serialize this to JSON text" builtin.

**Does this matter for agents?** Medium. Agents constructing API payloads need to produce JSON. Currently they would concatenate strings manually:

```
b=+"{\"name\":\"" name;b=+b "\",\"age\":";b=+b (str age);b=+b "}"
```

Terrible. 15+ tokens for a simple object.

**Minimal fix:** A `js` (JSON stringify) builtin: `js value > t`. Converts any ilo value (record, list, number, text) to JSON text. Combined with record construction:

```
p=user name:"alice" age:30;b=js p    -- 8 tokens, clean
```

**7c. Multiple serialization formats (TOML, YAML, MessagePack)**

Rust's serde ecosystem supports dozens of formats through the same derive macros.

**Does this matter for agents?** No. JSON is the universal agent interchange format. TOML/YAML are config file formats -- if an agent needs to read a YAML config, that is a tool concern (`tool read-yaml"Parse YAML" path:t>R t t`). ilo should not build a multi-format serialization framework.

### Verdict

`jp` (JSON path parse) and `js` (JSON stringify) are the two essential builtins. `jp` is already planned (I1) and high priority. `js` should be added to the roadmap. Everything else is a tool concern.

---

## 8. Pattern Matching

### What Rust provides

```rust
// Match with destructuring
match user {
    User { name, age, .. } if age >= 18 => format!("{} is adult", name),
    User { name, .. } => format!("{} is minor", name),
}

// Match on enums with data
match shape {
    Shape::Circle(r) => std::f64::consts::PI * r * r,
    Shape::Rect(w, h) => w * h,
    Shape::Triangle { base, height } => 0.5 * base * height,
}

// Nested pattern matching
match response {
    Ok(Response { status: 200, body }) => process(body),
    Ok(Response { status, .. }) => handle_error(status),
    Err(e) => retry(e),
}

// Or patterns
match status {
    200 | 201 | 204 => "success",
    400..=499 => "client error",
    500..=599 => "server error",
    _ => "unknown",
}

// if let / while let
if let Some(user) = find_user(id) {
    send_email(user);
}
while let Some(msg) = receiver.recv().await {
    process(msg);
}

// let-else (Rust 1.65+)
let Some(user) = find_user(id) else {
    return Err("not found");
};

// @ bindings
match age {
    n @ 0..=12 => println!("child: {}", n),
    n @ 13..=19 => println!("teen: {}", n),
    n => println!("adult: {}", n),
}
```

### What ilo has

| Rust pattern | ilo equivalent | Status |
|-------------|---------------|--------|
| Literal match | `"gold":body` / `42:body` | Implemented |
| Ok/Some match | `~v:body` | Implemented |
| Err/None match | `^e:body` / `_:body` | Implemented |
| Wildcard | `_:body` | Implemented |
| Exhaustiveness | ILO-T024 | Implemented |
| Guard clauses | `cond{body}` / `cond body` | Implemented |
| Enum destructuring | None | Gap (needs E3) |
| Record destructuring in match | None | Gap |
| Nested patterns | None | Gap |
| Or patterns | None | Gap |
| Range patterns | None | Gap |
| `if let` | `?v{~x:use x}` | Covered (match with one arm) |
| `let-else` | `?v{_:^"err"};use v` | Covered (guard) |
| Match guards | None | Gap |

### Gap analysis

**8a. Record destructuring in match arms**

Rust:
```rust
match user {
    User { name, verified: true, .. } => send(name),
    User { name, .. } => reject(name),
}
```

ilo has no destructuring in match arms. You match Ok/Err/literal, then access fields manually:
```
?r{~u:u.verified{send u.name};reject u.name;^e:^e}
```

**Does this matter for agents?** Low-medium. Record destructuring in match saves a few field accesses but adds pattern complexity. ilo's approach (match on Result/Option, then access fields) is more predictable for LLM generation.

**Minimal fix:** Defer until sum types (E3) land. Sum types make match-with-destructuring natural (`?shape{circle r:do-circle r;rect w h:do-rect w h}`). Without sum types, destructuring in match arms has limited value.

**8b. Or patterns**

Rust: `200 | 201 | 204 => "success"`. Matching multiple values in one arm.

ilo has no or-patterns. Workaround:
```
?s{200:~"ok";201:~"ok";204:~"ok";_:^"fail"}
```

Repetitive. An or-pattern would be:
```
?s{200|201|204:~"ok";_:^"fail"}
```

**Does this matter for agents?** Low. Or-patterns save tokens when matching multiple literals, but this pattern is uncommon in agent code. Agents typically branch on status categories (success/failure), not exhaustive status lists.

**8c. Match guards**

Rust: `x if x > 100 => ...`. Additional predicate on a match arm.

ilo has no match guards. Workaround is guard inside the arm body:
```
?r{~v:>v 100{process v};reject v;^e:^e}
```

**Does this matter for agents?** Low. Guards inside arm bodies are equivalent and ilo already supports them.

**8d. Nested patterns**

Rust:
```rust
match result {
    Ok(Some(value)) => use_value(value),
    Ok(None) => default(),
    Err(e) => handle(e),
}
```

ilo:
```
?r{~v:?v{~x:use x;_:default()};^e:handle e}
```

Nested matches work but are verbose. Nested patterns would flatten:
```
?r{~~x:use x;~_:default();^e:handle e}
```

**Does this matter for agents?** Low. Nested Result/Option is uncommon in agent code. Tools return flat `R ok err`, not nested Results.

### Verdict

ilo's pattern matching covers the essential cases well. The `~`/`^`/`_`/literal pattern set handles Result, Option, and value matching. Gaps exist in destructuring (needs E3), or-patterns, and match guards, but these are low priority for agents. The biggest improvement would come from sum types (E3), which unlock enum pattern matching -- the pattern that Rust uses most heavily.

---

## 9. Ownership, Lifetimes, and Borrowing

### What Rust provides

Rust's defining feature: compile-time memory safety without garbage collection.

```rust
// Ownership — each value has exactly one owner
let s1 = String::from("hello");
let s2 = s1;  // s1 is moved, no longer valid

// Borrowing — references without ownership transfer
fn len(s: &str) -> usize { s.len() }

// Lifetimes — compiler tracks reference validity
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}

// Clone — explicit copy when needed
let s2 = s1.clone();
```

### Relevance to ilo

**None.** Ownership, borrowing, and lifetimes solve the problem of memory management without garbage collection. ilo's runtime uses `Value` enum types that are `Clone`-derived, reference-counted where needed, and garbage-collected by Rust's normal drop semantics. Agent programs are short-lived -- there is no memory leak concern over minutes or hours. Ownership complexity would massively increase the token cost of type annotations with zero benefit for agents.

ilo correctly ignores this entire category. No action needed.

---

## 10. Modules and Visibility

### What Rust provides

```rust
mod network {
    pub fn get(url: &str) -> Result<String, Error> { ... }
    fn internal_helper() { ... }  // private
}

use network::get;
use std::collections::HashMap;
```

### Relevance to ilo

ilo programs are flat -- all declarations exist at the top level. There are no modules, no imports, no visibility modifiers. This is deliberate:

1. **Self-contained units.** Each function declares its dependencies (what it calls, what types it uses). No import ceremony.
2. **Closed world.** All functions are known at verification time. No external modules to resolve.
3. **Small programs.** Agent programs are typically 5-20 functions. Namespace collisions are not a real problem at this scale.

**Does this matter for agents?** No. Modules solve the problem of organizing large codebases maintained by teams over years. Agent programs are disposable single-purpose scripts. If programs grow large enough to need namespacing, tool declarations provide the boundary (each tool is an external namespace).

No action needed.

---

## 11. Smart Pointers and Interior Mutability

### What Rust provides

`Box<T>`, `Rc<T>`, `Arc<T>`, `Cell<T>`, `RefCell<T>`, `Cow<T>`.

### Relevance to ilo

None. These are implementation-level concerns for the Rust runtime, not language-level concepts. ilo's VM already uses the equivalent internally (boxed values, reference counting for records). The agent never sees or needs these concepts.

No action needed.

---

## 12. Summary of Gaps and Recommendations

### Tier 1: High priority, matters now

| Gap | Rust feature | ilo fix | Priority |
|-----|-------------|---------|----------|
| Optional type | `Option<T>` | E2: `O n` | Next type system item |
| JSON parsing | `serde_json` | I1: `jp` builtin | High |
| File I/O | `std::fs` | G8: `fread`/`fwrite` | High |
| Fold/reduce | `.fold()` / `.sum()` | Monomorphic `fld` builtin | High |
| String formatting | `format!` | `fmt` builtin | High |
| Parallel calls | `tokio::join!` | G4: `par{...}` | High |

### Tier 2: Medium priority, has workarounds

| Gap | Rust feature | ilo fix | Priority |
|-----|-------------|---------|----------|
| Range iteration | `0..n` | F7: `@i 0..n{...}` | Medium |
| JSON stringify | `serde_json::to_string` | `js` builtin | Medium |
| String trim | `.trim()` | `trm` builtin | Medium |
| String replace | `.replace()` | `rep` builtin | Medium |
| Sum types | `enum` | E3 | Medium |
| Enumerate | `.enumerate()` | Range iteration covers this | Medium |

### Tier 3: Low priority, defer

| Gap | Rust feature | Status | Reason |
|-----|-------------|--------|--------|
| Generics | `<T>` | E5 (planned) | High cost, lambda syntax prerequisite |
| Traits | `trait` | E6 (deferred) | Agents do not write abstractions |
| Error type conversion | `From`/`Into` | Not needed | Text errors convention sufficient |
| Regex | `regex` crate | Defer | High retry risk for LLM generation |
| Async/await | `async`/`await` | Never | Hidden behind `par{}` |
| Ownership/borrowing | Borrow checker | Never | ilo is GC-managed |
| Modules | `mod`/`use` | Never | Programs are flat by design |
| Nested patterns | Match destructuring | Defer until E3 | Uncommon in agent code |
| Or-patterns | `\|` in match | Defer | Low frequency |

### Tier 4: Explicitly excluded

| Rust feature | Reason for exclusion |
|-------------|---------------------|
| Lifetimes | Memory management abstraction -- ilo uses GC |
| Smart pointers | Implementation detail, not language concept |
| Unsafe blocks | ilo has no unsafe escape hatch by design |
| Macros (proc/declarative) | Metaprogramming adds vocabulary, increases retry rate |
| dyn Trait / vtable dispatch | Agents generate concrete types |
| Pin/Unpin | Async implementation detail |
| const generics | Type-level computation is token-expensive |
| Closures with move semantics | No ownership to move |

---

## 13. What ilo Gets Right That Rust Does Not

Areas where ilo's design is superior to Rust for the agent use case:

**13a. Auto-unwrap is terser than `?`**

Rust: `let user = get_user(id)?;` -- 8 tokens (including the semicolon, let, type)
ilo: `u=get-user! id` -- 4 tokens

ilo's `!` saves 50% of tokens on every fallible call. Across a 10-call program, that is 40 saved tokens.

**13b. No type annotation ceremony**

Rust: `fn total(price: f64, quantity: i32, rate: f64) -> f64` -- 14 tokens
ilo: `tot p:n q:n r:n>n` -- 8 tokens (including the signature separator `>`)

ilo's single-character types save ~6 tokens per function signature.

**13c. No import/use ceremony**

Rust programs begin with `use std::...` declarations. A typical program has 5-15 import lines. ilo has zero imports -- everything is in scope.

**13d. Pattern matching is built into control flow**

Rust requires explicit `match` blocks. ilo's `?{...}` integrates matching into the statement flow, and `!` eliminates matching entirely for the propagation case.

**13e. Implicit last-result matching**

ilo's `?{...}` (no subject) matches the result of the previous expression. Rust always requires naming the match subject. This saves 1-2 tokens per match and eliminates throwaway variable names.

**13f. No semicolon-vs-expression ambiguity**

In Rust, forgetting a semicolon changes whether something is a statement or expression, causing confusing type errors. ilo's last-expression-is-return-value rule has no ambiguity.

---

## 14. Implementation Notes (ilo Is Written in Rust)

Since ilo's runtime is implemented in Rust, several Rust features directly inform implementation decisions:

**14a. serde for JSON interchange**

ilo already uses `serde` + `serde_json` for AST serialization and structured error output. The `jp` builtin should use `serde_json::Value` internally -- it is already a dependency.

**14b. Cranelift for JIT**

The Cranelift JIT backend (already implemented) is a Rust-native code generator. Future additions (range iteration, fold, parallel calls) need Cranelift codegen paths.

**14c. tokio for async runtime**

The planned async runtime (G4) should use tokio. It is already the de facto standard, integrates with the existing Rust ecosystem, and handles the platform abstractions. The `http` feature could migrate from `minreq` (sync) to `reqwest` (async, tokio-based) when async lands.

**14d. Error propagation in the runtime**

ilo's runtime error handling (`RuntimeError`, `VmError`, `VmRuntimeError`) already follows Rust patterns: `thiserror` for derive, `?` for propagation, span attachment for source mapping. The error infrastructure is solid.

---

## Conclusion

Rust and ilo share a philosophical commitment to explicit error handling, verification before execution, and exhaustive pattern matching. Where they diverge is in the audience: Rust serves human systems programmers building long-lived, memory-safe infrastructure; ilo serves AI agents generating short-lived, token-minimal tool orchestration.

The most valuable Rust ideas for ilo are already adopted: Result types, the `?`/`!` operator, exhaustive match, static verification. The remaining gaps cluster in three areas:

1. **Data processing** -- fold/reduce, string formatting, JSON parsing. These are high-frequency agent operations that need builtins.
2. **I/O expansion** -- file I/O and parallel tool calls. Essential for real-world agent tasks.
3. **Type system growth** -- Optional type and sum types. Incremental additions that prevent specific classes of runtime failures.

Everything else from Rust -- ownership, lifetimes, traits, modules, async/await, smart pointers, macros -- is either irrelevant to agents, handled by the runtime, or actively harmful to token efficiency. ilo's strength is knowing what to exclude.
