# Swift & Kotlin: Capabilities Relevant to an Agent Language

Research into Swift and Kotlin features evaluated against ilo's manifesto: **total tokens from intent to working code**. Both languages have strong Optional/Result/concurrency patterns. This document catalogs what they offer and extracts what matters for ilo.

---

## 1. File I/O

### Swift

`FileManager` + URL-based access. Two patterns:

```swift
// Modern (Data/String init with URL)
let data = try Data(contentsOf: URL(fileURLWithPath: "/tmp/data.json"))
let text = try String(contentsOfFile: "/tmp/file.txt", encoding: .utf8)
try text.write(toFile: "/tmp/out.txt", atomically: true, encoding: .utf8)

// FileManager for existence/attributes/directory operations
FileManager.default.fileExists(atPath: path)
FileManager.default.contentsOfDirectory(atPath: path)
```

Errors are thrown — integrates with `try/catch`. No async file I/O in stdlib (Foundation is synchronous for disk ops).

### Kotlin

`java.io.File` + `kotlin.io` extensions:

```kotlin
val text = File("/tmp/file.txt").readText()
File("/tmp/out.txt").writeText(content)
File("/tmp/data").readLines()       // List<String>
File("/tmp/data").bufferedReader().useLines { lines -> ... }
```

Extension functions on `File` (`readText()`, `writeText()`, `readLines()`) make file ops single-expression. `use` / `useLines` handle resource cleanup automatically (like `try-with-resources` but as a scope function).

### Relevance to ilo

File I/O is a **tool concern**, not a language concern (see OPEN.md: "Format parsing is a tool concern"). Agents compose tool results; tools handle filesystem access. If ilo adds a file builtin, the model is the same as `get`:

```
tool read"Read file" path:t>R t t timeout:5
tool write"Write file" path:t content:t>R _ t timeout:5
```

**Key takeaway:** Both Swift and Kotlin prove that file I/O benefits from Result-typed returns (Swift throws, Kotlin exceptions) rather than silent failure. ilo already has this via `R ok err` on tools. No language-level file API needed — it's a tool declaration.

---

## 2. Networking

### Swift

`URLSession` with async/await (Swift 5.5+):

```swift
let (data, response) = try await URLSession.shared.data(from: url)
let (data, response) = try await URLSession.shared.data(for: request)
```

Pre-async: completion handler callbacks (verbose, error-prone). The async version is dramatically more concise and natural.

### Kotlin

`kotlinx-coroutines` + `ktor-client` or `java.net.URL`:

```kotlin
// Simple (blocking)
val body = URL("https://api.example.com/data").readText()

// Ktor (async, structured)
val client = HttpClient(CIO)
val response: HttpResponse = client.get("https://api.example.com/data")
val body: String = response.body()
```

Kotlin coroutines make async networking look synchronous — `suspend` functions are called like regular functions.

### Relevance to ilo

ilo already has `get url` / `$url` returning `R t t`. The language-level API is a single builtin, not a library. The insight from Swift/Kotlin: **async/await makes networking code look synchronous**. ilo sidesteps this entirely — the runtime handles blocking; the language stays sequential. If ilo adds async tool calls, the model would be transparent (tools block from the program's perspective, runtime parallelizes underneath).

**Key takeaway:** Swift's async/await and Kotlin's coroutines solve the problem of making async code readable. ilo doesn't need this at the language level because tool calls are inherently opaque — the runtime decides execution strategy. The `get` builtin and `tool` declarations are sufficient.

---

## 3. Process Execution

### Swift

`Process` class (Foundation):

```swift
let process = Process()
process.executableURL = URL(fileURLWithPath: "/usr/bin/ls")
process.arguments = ["-la"]
let pipe = Pipe()
process.standardOutput = pipe
try process.run()
process.waitUntilExit()
let data = pipe.fileHandleForReading.readDataToEndOfFile()
```

Verbose. Requires setting up pipes, waiting, reading. Process termination status via `process.terminationStatus`.

### Kotlin

`ProcessBuilder` (JDK):

```kotlin
val process = ProcessBuilder("ls", "-la")
    .redirectErrorStream(true)
    .start()
val output = process.inputStream.bufferedReader().readText()
val exitCode = process.waitFor()
```

Slightly less verbose than Swift. Kotlin extension functions make it cleaner, but it's still the JDK `ProcessBuilder` underneath.

### Relevance to ilo

Process execution is another tool concern. An ilo tool declaration:

```
tool exec"Run command" cmd:t args:L t>R t t timeout:30
```

Neither Swift nor Kotlin has a one-liner for "run command, get output or error." Both require ~5-10 lines of setup. ilo's tool model is strictly better for this use case — the complexity lives in the tool provider, the program just calls `exec! "ls" ["-la"]`.

**Key takeaway:** No language primitives needed. Tool declarations handle process execution with typed results and timeout/retry semantics that neither Swift nor Kotlin provides natively.

---

## 4. Data Formats

### Swift

`Codable` protocol + `JSONDecoder`/`JSONEncoder`:

```swift
struct User: Codable {
    let name: String
    let age: Int
}
let user = try JSONDecoder().decode(User.self, from: jsonData)
let json = try JSONEncoder().encode(user)
```

Codable is compile-time. The struct definition IS the schema. `PropertyListDecoder`/`Encoder` handles plist. Custom `CodingKeys` for field name mapping.

### Kotlin

`kotlinx.serialization` (compile-time) or Gson/Jackson (reflection):

```kotlin
@Serializable
data class User(val name: String, val age: Int)
val user = Json.decodeFromString<User>(jsonString)
val json = Json.encodeToString(user)
```

`data class` + `@Serializable` is the idiomatic pattern. The data class definition IS the schema.

### Relevance to ilo

This is the strongest parallel to ilo's existing design. Both Swift's `Codable` and Kotlin's `@Serializable` demonstrate the principle that **type definitions ARE schemas**. ilo already does this:

```
type user{name:t;age:n}
```

This record definition serves as both the in-language type and the JSON schema for tool boundaries. From OPEN.md: "Tool declarations ARE schemas — `tool name"desc" params>return` is a schema declaration." MCP's `inputSchema`/`outputSchema` map directly to ilo records.

The key difference: Swift and Kotlin require explicit encode/decode calls. ilo's runtime handles the JSON-to-Value mapping transparently at tool boundaries. The agent never writes serialization code.

**Key takeaway:** ilo's `type` declarations already serve the role of `Codable`/`@Serializable`. No serialization API needed in the language — the runtime maps JSON to typed records automatically. This saves significant tokens vs Swift/Kotlin where agents would need to write decode/encode calls.

---

## 5. String Manipulation

### Swift

String interpolation + Regex (Swift 5.7+):

```swift
let msg = "Hello, \(name)! You have \(count) items."
let regex = /\d{3}-\d{4}/
if let match = input.firstMatch(of: regex) { ... }
// String is a Collection — subscriptable, iterable
name.prefix(3)
name.split(separator: " ")
name.contains("@")
```

Swift's `Regex` builder (5.7+) is compile-time checked. String interpolation is the primary string-building mechanism.

### Kotlin

String templates + Regex:

```kotlin
val msg = "Hello, $name! You have $count items."
val regex = """\d{3}-\d{4}""".toRegex()
regex.find(input)?.value
name.take(3)
name.split(" ")
name.contains("@")
```

Triple-quoted raw strings avoid escaping. Extension functions (`take`, `split`, `contains`) make string ops chainable.

### Relevance to ilo

ilo handles string operations via builtins and operators:

```
+name " has " +str count " items"    -- string concat (prefix)
spl t " "                             -- split
has t "@"                             -- contains
slc t 0 3                             -- substring
```

**String interpolation** is the notable absence. Both Swift (`\(expr)`) and Kotlin (`$expr`) save tokens vs manual concatenation. In ilo:

```
-- Current: 7 tokens for a 3-part string
+"Hello, " +name "!"

-- Hypothetical interpolation: fewer tokens?
-- Not clear — interpolation syntax adds { } or \ delimiters
-- And prefix + is already a single token per concat
```

Analysis: With prefix notation, string concatenation is already terse (`+a b` = 3 tokens per concat). Interpolation would save tokens only for 3+ segments where the `+` nesting gets deep. But interpolation requires a new syntax form (string with embedded expressions), adding to the grammar surface. The manifesto says "one way to do things" — having both concat and interpolation violates this.

**Regex:** Not needed as a language feature. ilo delegates format parsing to tools (see OPEN.md). A regex tool (`tool match"Regex match" t:t pat:t>R L t t`) is sufficient.

**Key takeaway:** String concat via `+` is already token-competitive with interpolation for short strings. Regex is a tool concern. Neither feature justifies expanding ilo's grammar.

---

## 6. Concurrency

### Swift

Structured concurrency (Swift 5.5+):

```swift
// async/await
let result = try await fetchUser(id)

// TaskGroup — parallel execution
try await withThrowingTaskGroup(of: User.self) { group in
    for id in ids {
        group.addTask { try await fetchUser(id) }
    }
    for try await user in group { ... }
}

// Actors — thread-safe state
actor UserCache {
    var cache: [String: User] = [:]
    func get(_ id: String) async -> User? { cache[id] }
}
```

Three layers: `async/await` (sequential async), `TaskGroup` (structured parallelism), `actor` (safe shared state). All integrated with the type system — the compiler enforces data isolation.

### Kotlin

Coroutines + Flow:

```kotlin
// suspend functions (async/await equivalent)
suspend fun fetchUser(id: String): User = ...

// Structured concurrency
coroutineScope {
    val user = async { fetchUser(id) }
    val profile = async { fetchProfile(id) }
    combine(user.await(), profile.await())
}

// Flow (reactive streams)
flow { emit(fetchUser(id)) }
    .map { it.name }
    .collect { println(it) }
```

`coroutineScope` enforces structured concurrency — child coroutines must complete before the scope exits. `Flow` is cold (lazy), like Kotlin's answer to reactive streams.

### Relevance to ilo

Concurrency is **the single most important design decision** for ilo's future, and both Swift and Kotlin validate the same approach: **structured concurrency with transparent syntax**.

Today ilo is fully sequential. The runtime blocks on tool calls. For agent workflows, this is a real limitation — fetching 3 independent API responses sequentially wastes time.

The Swift/Kotlin model suggests ilo could add parallel tool execution **without changing the language syntax**:

```
-- These tools are independent — runtime could execute in parallel
u=get-user! uid
p=get-profile! uid
o=get-orders! uid
-- This depends on all three — blocks until all complete
notify u p o
```

The runtime (not the language) would analyze the dependency graph and parallelize independent tool calls. This aligns with ilo's "graph-native" principle — the call graph reveals parallelism opportunities.

If explicit parallelism is needed, a minimal syntax:

```
-- Hypothetical: all[] executes tools in parallel, returns list
rs=all[get-user uid, get-profile uid, get-orders uid]
```

**Actors and shared state:** Not relevant. ilo programs are short-lived, tool-orchestration scripts — not long-running services. There is no shared mutable state to protect.

**Key takeaway:** Both Swift and Kotlin prove that structured concurrency (scoped parallelism with automatic cancellation) is the right model. ilo should get this for free from the runtime's dependency graph analysis, not from language syntax. The zero-syntax-cost approach: runtime detects independent tool calls and parallelizes them. This saves agents from ever writing concurrency code.

---

## 7. Error Handling

### Swift

`throws` / `try` / `catch` + `Result` + `Optional`:

```swift
// throws/try/catch
func fetchUser(_ id: String) throws -> User { ... }
do {
    let user = try fetchUser(id)
} catch {
    print("Failed: \(error)")
}

// try? — convert to Optional
let user = try? fetchUser(id)    // User?

// try! — force unwrap (crash on error)
let user = try! fetchUser(id)    // User (crashes if error)

// Result type
func fetch(_ url: URL) -> Result<Data, Error> { ... }
switch result {
case .success(let data): ...
case .failure(let error): ...
}
```

Three error-handling styles: exceptions (`try/catch`), optionals (`try?`), and explicit `Result`. The `try?` bridge between exceptions and optionals is particularly elegant — it says "I don't care about the error details, just give me nil on failure."

### Kotlin

Exceptions + `Result` + `runCatching`:

```kotlin
// Traditional try/catch
try {
    val user = fetchUser(id)
} catch (e: Exception) {
    println("Failed: ${e.message}")
}

// runCatching — wraps in Result
val result = runCatching { fetchUser(id) }
result.getOrNull()           // User?
result.getOrElse { default } // User
result.map { it.name }       // Result<String>
result.fold(
    onSuccess = { use(it) },
    onFailure = { handle(it) }
)
```

`runCatching` bridges exceptions into `Result`. The `fold`/`map`/`getOrElse` combinators on `Result` allow functional error handling without `try/catch`.

### Relevance to ilo

ilo's error handling is already more concise than both Swift and Kotlin:

| Pattern | Swift | Kotlin | ilo |
|---------|-------|--------|-----|
| Call + handle error | `do { try f() } catch { ... }` (5+ tokens) | `try { f() } catch (e) { ... }` (5+ tokens) | `f x;?{^e:handle;~v:use}` (7 tokens) |
| Propagate error | `let x = try f()` | `val x = f()` (throws) | `x=f! x` (3 tokens) |
| Error to nil | `try? f()` | `runCatching { f() }.getOrNull()` | would need `O` type |
| Default on error | `(try? f()) ?? default` | `runCatching { f() }.getOrElse { default }` | `f x;?{^_:default;~v:v}` |

ilo's `!` auto-unwrap is equivalent to Swift's `try` and Rust's `?` — propagate the error, give me the value. At 1 token (`!` on the function name), it's the most concise of all three.

**Swift's `try?` pattern** is notable: convert a Result/throw to Optional. This would map to ilo as:

```
-- Hypothetical: f? converts Err to nil (drops error detail)
x=f? uid         -- R ok err → O ok
x??default       -- O ok → value with fallback
```

This two-step pattern (`f? args` + `??`) would be 4 tokens total for "call, ignore error, use default." Currently it takes ~8 tokens with full match syntax. But this requires the `O` (Optional) type from Phase E2.

**Key takeaway:** ilo's `!` auto-unwrap is already best-in-class for error propagation. The gap is in the "I don't care about the error, just give me a default" pattern. Swift's `try?` + `??` and Kotlin's `getOrElse` both solve this in 2-3 tokens. ilo could match this with the `O` type + `??` nil-coalescing (both already planned in TYPE-SYSTEM.md and CONTROL-FLOW.md).

---

## 8. Optional Chaining / Null Safety

### Swift

Optional chaining + nil coalescing + unwrap patterns:

```swift
// Optional chaining — short-circuits on nil
let city = user?.address?.city          // String?

// Nil coalescing
let name = user?.name ?? "Unknown"      // String

// guard let — unwrap or exit scope
guard let user = fetchUser(id) else { return nil }

// if let — unwrap in scope
if let user = fetchUser(id) {
    use(user)
}

// Optional map/flatMap
let upper = name.map { $0.uppercased() }    // String?
```

Swift makes nil-handling **pervasive and cheap**. The `?.` chain + `??` default pattern handles 90% of optional cases in 1-2 tokens per step.

### Kotlin

Null safety is baked into the type system:

```kotlin
// ?. safe call — short-circuits on null
val city = user?.address?.city          // String?

// ?: Elvis operator (nil coalescing)
val name = user?.name ?: "Unknown"      // String

// !! force unwrap (throws on null)
val name = user!!.name                  // String (throws NPE if null)

// let scope function — unwrap + operate
user?.let { sendEmail(it) }

// Smart cast — null check promotes type
if (user != null) {
    user.name    // compiler knows user is non-null here
}
```

Kotlin's approach is nearly identical to Swift's but with `?:` instead of `??` and `!!` for force-unwrap.

### Relevance to ilo

ilo already has the key pieces:

| Feature | Swift | Kotlin | ilo (current) | ilo (planned) |
|---------|-------|--------|---------------|---------------|
| Safe navigation | `?.` | `?.` | `.?` (implemented) | -- |
| Nil coalescing | `??` | `?:` | `??` (implemented) | -- |
| Force unwrap | `!` | `!!` | `!` (auto-unwrap on R) | extend to `O` |
| Optional type | `T?` / `Optional<T>` | `T?` | -- | `O n` (Phase E2) |

ilo chose `.?` over `?.` to avoid ambiguity with the `?` match operator. And `??` over `?:` because `?:` conflicts with match arm syntax (`pattern:body`). These are the right choices for ilo's sigil-heavy grammar.

**What ilo is missing vs Swift/Kotlin:**

1. **Typed optionals.** Swift and Kotlin enforce null safety at compile time. ilo currently allows nil at runtime in any position. The `O` type (Phase E2) would close this gap — the verifier would force `.?` or `??` when accessing optional fields.

2. **Optional map/flatMap.** Swift's `optional.map { transform }` and Kotlin's `?.let { transform }` apply a function to the inner value if present. ilo's equivalent would be a match: `?v{~x:transform x;_:_}` (7 tokens). With the pipe operator: `v>>transform` wouldn't nil-check. A dedicated `v.?>>transform` pattern would need design work.

3. **Smart casts / guard let.** Swift's `guard let x = expr else { return }` and Kotlin's smart casts narrow the type after a nil check. ilo's match arms (`?v{~x:use x}`) serve the same purpose but don't narrow the type in subsequent statements — once you exit the match, the binding is gone. This is fine for ilo's small-function style, where match arms contain the entire remaining logic.

**Key takeaway:** ilo's `.?` and `??` already provide the core chaining and defaulting patterns from Swift/Kotlin. The main gap is compile-time optional typing (`O` type) — Phase E2 in TYPE-SYSTEM.md. Once `O` is added, ilo's null safety will be on par with Swift/Kotlin in capability but significantly more concise in syntax.

---

## 9. Pattern Matching

### Swift

`switch` with exhaustiveness checking:

```swift
switch status {
case .pending: handlePending()
case .active: handleActive()
case .closed: handleClosed()
// compiler error if a case is missing
}

// Value binding in patterns
switch result {
case .success(let data): use(data)
case .failure(let error): handle(error)
}

// Where clauses
switch score {
case let x where x >= 90: "A"
case let x where x >= 80: "B"
default: "C"
}

// Tuple patterns, enum associated values, etc.
```

Swift requires exhaustive matching on enums — the compiler catches missing cases.

### Kotlin

`when` expression:

```kotlin
when (status) {
    Status.PENDING -> handlePending()
    Status.ACTIVE -> handleActive()
    Status.CLOSED -> handleClosed()
    // exhaustive when sealed class/enum used
}

// Value binding
when (val result = fetchUser()) {
    is Success -> use(result.data)
    is Failure -> handle(result.error)
}

// Guard-like conditions
when {
    score >= 90 -> "A"
    score >= 80 -> "B"
    else -> "C"
}
```

Kotlin's `when` is more flexible than `switch` — it can match on type (`is`), conditions (no subject), and ranges (`in 1..10`). Exhaustive only for sealed hierarchies.

### Relevance to ilo

ilo's `?` match with exhaustiveness checking (ILO-T024) already covers the core patterns:

```
-- Result matching (like Swift .success/.failure)
?r{~v:use v;^e:handle e}

-- Value matching (like Swift case "gold":)
?tier{"gold":100;"silver":50;_:10}

-- Guard chains (like Kotlin's when{})
>=sp 1000 "gold";>=sp 500 "silver";"bronze"
```

**What Swift/Kotlin have that ilo doesn't (yet):**

1. **Enum/sum type matching.** Swift and Kotlin enforce exhaustiveness on enums. ilo only matches on text literals, numbers, and Result variants. Phase E3 (sum types) in TYPE-SYSTEM.md would add:
   ```
   enum status{pending;active;closed}
   ?s{pending:...;active:...;closed:...}   -- exhaustiveness checked
   ```

2. **Where clauses / range patterns.** Swift's `case let x where x >= 90` and Kotlin's `in 1..10` combine binding with conditions. ilo's guard chains (`>=sp 1000 "gold"`) serve the same purpose without a special pattern syntax — and are more concise.

3. **Type patterns.** Kotlin's `is` pattern matches on runtime type. ilo's F12 (Type pattern matching) in CONTROL-FLOW.md addresses this for the `t` escape-hatch case where tools return untyped data.

**Key takeaway:** ilo's guard chains are already more concise than Swift/Kotlin pattern matching for conditional logic. The main gap is enum exhaustiveness (Phase E3). Guard-style matching is the right design for an agent language — it avoids the syntactic overhead of `switch`/`when` blocks while providing the same safety guarantees via the verifier.

---

## 10. Type System

### Swift

Protocols, generics, associated types, opaque types:

```swift
protocol Describable {
    var description: String { get }
}

func process<T: Describable>(_ item: T) -> String { item.description }

// Associated types
protocol Container {
    associatedtype Item
    func get() -> Item
}

// Opaque types (some)
func makeView() -> some View { Text("Hello") }
```

Swift's type system is rich — protocol-oriented programming is the dominant paradigm. Generics with constraints, existentials (`any Protocol`), and opaque return types (`some Protocol`) provide multiple abstraction layers.

### Kotlin

Generics, sealed classes, extension functions, data classes:

```kotlin
// Generics
fun <T> firstOrNull(list: List<T>): T? = list.firstOrNull()

// Sealed class (sum type)
sealed class Result<out T> {
    data class Success<T>(val data: T) : Result<T>()
    data class Failure(val error: String) : Result<Nothing>()
}

// Extension functions
fun String.isPalindrome(): Boolean = this == this.reversed()

// Data class (record/struct with auto-generated equals, hashCode, copy)
data class User(val name: String, val age: Int)
```

Kotlin's type system is pragmatic — `data class` eliminates boilerplate, `sealed class` enables exhaustive matching, extension functions add capabilities without inheritance.

### Relevance to ilo

**What ilo has today:**

From TYPE-SYSTEM.md: 7 type constructors (`n`, `t`, `b`, `_`, `L`, `R`, named records). Everything is monomorphic. No generics, no protocols/interfaces, no type variables.

**Swift/Kotlin features evaluated for ilo:**

| Feature | Swift | Kotlin | ilo assessment |
|---------|-------|--------|---------------|
| Generics | Protocols + constraints | Bounded type params | Phase E5 — high value but high cost. Needed for `map`/`filter`/`fold` builtins. Without generics, list processing requires manual loops. |
| Protocols/Interfaces | Protocol-oriented programming | Interfaces + default methods | Phase E6 — lowest priority. Agents generate concrete code, not abstract interfaces. Deferred until real use cases emerge. |
| Sum types | enum with associated values | sealed class | Phase E3 — medium priority. Needed for exhaustive matching beyond Result. `enum status{pending;active;closed}` |
| Data classes / records | struct (value type) | data class (auto equals/copy) | Already have `type name{fields}` + `with` for update. ilo's records are equivalent to Kotlin's `data class`. |
| Extension functions | Protocol extensions | `fun Type.method()` | Not applicable. ilo has no method syntax — all functions are free-standing. Extension functions solve a problem (adding methods to types you don't own) that doesn't exist in ilo's model. |
| Opaque types | `some Protocol` | N/A (reified generics) | Not applicable. Opaque types hide implementation details — an abstraction concern. Agents generate concrete types. |
| Associated types | `associatedtype` | N/A | Not applicable. Gates on protocols, which are deferred. |

**Kotlin's scope functions (let, run, apply, also, with):**

```kotlin
user?.let { sendEmail(it) }                    // operate on non-null
User("Alice", 30).apply { validate() }         // configure object
fetchUser(id).also { log(it) }                 // side-effect + pass through
with(config) { host = "localhost"; port = 8080 } // multiple operations on same object
```

Scope functions are a convenience for reducing repetition of a receiver expression. In ilo:
- `let` equivalent: match arm `?v{~x:use x}` or future `v.?>>func`
- `apply`/`also` equivalent: bind + operate + return original — `x=expr;sideeffect x;x`
- `with` equivalent: record `with` syntax — `obj with field:val`

ilo's `with` expression already covers the `apply`/`with` use case. The others are handled by binding.

**Key takeaway:** ilo's type system is deliberately minimal — the manifesto says type annotations cost tokens, and the sweet spot is "catch common errors with cheap annotations." The priority ordering from TYPE-SYSTEM.md (aliases > optionals > sum types > maps > generics > traits) is validated by this Swift/Kotlin analysis. Agents don't need protocols, generics, or extension functions for tool orchestration. They need Optional (`O`) to catch nil crashes, sum types for exhaustive tool-status matching, and eventually generics for list-processing builtins.

---

## 11. Kotlin Coroutines & Structured Concurrency (detailed)

```kotlin
// Sequential
suspend fun fetchBoth(id: String): Pair<User, Profile> {
    val user = fetchUser(id)       // suspends, waits
    val profile = fetchProfile(id) // suspends, waits
    return Pair(user, profile)
}

// Parallel (structured)
suspend fun fetchBoth(id: String): Pair<User, Profile> = coroutineScope {
    val user = async { fetchUser(id) }
    val profile = async { fetchProfile(id) }
    Pair(user.await(), profile.await())
}

// Structured concurrency — cancellation propagates
coroutineScope {
    launch { longRunning() }    // cancelled if scope fails
    launch { anotherTask() }   // cancelled if scope fails
}

// Flow (cold reactive stream)
fun userUpdates(): Flow<User> = flow {
    while (true) {
        emit(fetchLatestUser())
        delay(1000)
    }
}
```

**Key property of structured concurrency:** child tasks cannot outlive their parent scope. If the parent is cancelled, all children are cancelled. This prevents leaked goroutines/threads.

### Relevance to ilo

Structured concurrency maps perfectly to ilo's tool-orchestration model. Consider a multi-tool workflow:

```
-- ilo today (sequential)
u=get-user! uid
p=get-profile! uid
o=get-orders! uid
r=build-report u p o
send-email! u.email r
```

The runtime's dependency graph reveals that `get-user`, `get-profile`, and `get-orders` are independent (no data dependencies). `build-report` depends on all three. `send-email` depends on `build-report`.

A structured-concurrency runtime would:
1. Execute `get-user`, `get-profile`, `get-orders` in parallel
2. Wait for all three to complete
3. Execute `build-report`
4. Execute `send-email`

**No syntax changes required.** The dependency graph IS the concurrency model. This is ilo's "graph-native" principle applied to execution.

If a tool times out or fails, structured cancellation means: cancel sibling tasks, propagate `^e` to the caller. The `!` auto-unwrap handles this naturally.

**Flow/Streams:** Not relevant for ilo's target use case (short-lived tool orchestration). Agents don't build reactive pipelines. If streaming is ever needed, it would be a tool concern (a tool that emits chunks, not a language construct).

---

## 12. Kotlin Extension Functions (detailed)

```kotlin
fun String.wordCount(): Int = this.split(" ").size
fun List<Int>.median(): Double = sorted().let { it[it.size / 2].toDouble() }
fun <T> T.also(block: (T) -> Unit): T { block(this); return this }
```

Extension functions let you add methods to types you don't own. They compile to static functions with the receiver as the first argument — they're syntactic sugar.

### Relevance to ilo

ilo's functions are already free-standing with the "subject" as the first argument:

```
len xs        -- Kotlin equivalent: xs.len()
spl t " "     -- Kotlin equivalent: t.split(" ")
has t "@"     -- Kotlin equivalent: t.contains("@")
```

In Kotlin, extension functions enable chaining: `text.split(" ").filter { it.isNotEmpty() }.joinToString(",")`. In ilo, the pipe operator (`>>`) serves the same purpose:

```
spl t " ">>flt empty>>cat ","
```

Extension functions solve the "discoverability" problem — `text.` in an IDE shows all available operations. ilo doesn't have IDEs (yet), and the closed-world principle means all available operations are in the spec. An agent doesn't need auto-complete; it needs the spec.

**Key takeaway:** Extension functions are IDE-oriented syntactic sugar. ilo's free-standing functions + pipe operator provide the same chaining capability. The "one way to do things" principle argues against adding method syntax.

---

## 13. Kotlin Data Classes (detailed)

```kotlin
data class User(val name: String, val age: Int)
// Auto-generates: equals(), hashCode(), toString(), copy(), componentN()

val user = User("Alice", 30)
val updated = user.copy(age = 31)        // structural update
val (name, age) = user                    // destructuring
```

### Relevance to ilo

ilo records already provide the equivalent:

```
type user{name:t;age:n}
u=user name:"Alice" age:30
u2=u with age:31                -- structural update (like copy())
```

The missing piece is **destructuring** (F7 in CONTROL-FLOW.md):

```
-- Proposed: {n;a}=u  (bind name→u.name, age→u.age by field name)
-- Current:  n=u.name;a=u.age  (2 statements)
```

Destructuring saves 1 token per field. For a 3-field record, that's 3 tokens saved minus 1 for the `{}=` syntax = 2 net tokens. Modest savings but high frequency.

**Key takeaway:** ilo records already match Kotlin data classes in capability. Destructuring bind is the one gap worth closing.

---

## Synthesis: What Matters for an Agent Language

### Already in ilo (no action needed)

| Capability | Swift/Kotlin pattern | ilo equivalent |
|-----------|---------------------|----------------|
| Error propagation | `try`/`throws` / exceptions | `!` auto-unwrap |
| Result type | `Result<T,E>` | `R ok err` |
| Safe navigation | `?.` | `.?` |
| Nil coalescing | `??` / `?:` | `??` |
| Pattern matching | `switch`/`when` exhaustive | `?` match + ILO-T024 |
| Data classes/records | `data class` / `struct` | `type name{fields}` + `with` |
| Record update | `.copy()` | `with` expression |
| JSON schema mapping | `Codable` / `@Serializable` | `type` declarations = schemas |
| Free-standing functions | Extension functions (compiled) | All functions are free-standing |
| Chaining | Method chaining / extension chains | `>>` pipe operator |

### Planned features validated by Swift/Kotlin (proceed as designed)

| Feature | ilo Phase | Swift/Kotlin validation |
|---------|-----------|------------------------|
| Optional type (`O n`) | E2 | Both languages center their null-safety on compile-time optionals. The verifier catching nil crashes pre-execution would save ~150 tokens per prevented retry. |
| Sum types / enums | E3 | Swift enums with associated values and Kotlin sealed classes both demonstrate that exhaustive matching on user-defined variants is essential for reliable code. |
| Generics | E5 | Both languages need generics for `map`/`filter`/`fold`. ilo can defer this longer because `@` loops + bind-first handle most cases. |
| Destructuring bind | F7 | Kotlin destructuring (`val (a, b) = pair`) and Swift tuple patterns both reduce tokens. Modest savings per use but high frequency. |

### New insights from this research

1. **Runtime-level structured concurrency (zero syntax cost).** Both Swift (TaskGroup) and Kotlin (coroutineScope) prove that structured concurrency is the right model. ilo's graph-native principle enables this at the runtime level — the dependency graph reveals parallelism. No `async`/`await` keywords needed. This is the highest-leverage insight: agents get parallel tool execution without writing concurrency code.

2. **Error-to-Optional bridge.** Swift's `try?` pattern (convert error to nil, then nil-coalesce) is a 2-token pattern for "call with fallback." ilo could support this as `f? args` (like `!` but converts Err to nil instead of propagating). Combined with `??`, this gives: `f? uid ?? default` — 4 tokens for call-with-fallback vs 8+ tokens with full match. This gates on the `O` type (Phase E2).

3. **Scope functions are unnecessary.** Kotlin's `let`/`run`/`apply`/`also`/`with` are 5 different ways to operate on a value in a scope. ilo's `with` expression + bind + match cover all these cases. Adding scope functions would violate "one way to do things."

4. **Protocol/interface abstraction is unnecessary for agents.** Both Swift (protocol-oriented) and Kotlin (interface + default methods) emphasize abstraction. Agents generate concrete code for specific tasks — they don't build reusable libraries. Phase E6 (traits) is correctly deferred.

5. **Extension functions are IDE sugar.** They solve discoverability (`text.` shows available ops in an IDE). Agents don't use IDEs — they read the spec. ilo's free-standing functions are the right model.

### Priority ordering (revised)

Based on this analysis, the token-savings priority for features inspired by Swift/Kotlin:

```
Highest impact:
1. Runtime parallel tool execution (zero syntax, graph-based)       — new insight
2. O type + verifier nil safety (Phase E2)                          — validated
3. Error-to-Optional bridge: f? args (Swift try? equivalent)        — new insight
4. Sum types for exhaustive matching (Phase E3)                     — validated

Medium impact:
5. Destructuring bind (Phase F7)                                    — validated
6. Generics for map/filter/fold (Phase E5)                          — validated

Low impact / not needed:
7. String interpolation                                             — prefix + is sufficient
8. Protocols/interfaces                                             — agents don't abstract
9. Extension functions / scope functions                            — free functions + pipe suffice
10. Regex as language feature                                       — tool concern
11. Explicit async/await keywords                                   — runtime handles this
12. Method syntax                                                   — violates one-way principle
```
