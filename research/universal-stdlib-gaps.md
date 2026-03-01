# Universal Standard Library Gaps: Cross-Language Analysis for ilo

What every language's users install first reveals what every language's stdlib gets wrong. This document catalogs the "practically required" third-party packages across Python, JavaScript/TypeScript, Rust, Go, and Ruby -- packages so universal they feel like missing stdlib features. Each package represents a capability gap that real programs hit immediately.

The organizing question for ilo: **which of these universal gaps should ilo address as builtins, which as tools, and which should it ignore?**

---

## 1. Python: Top Packages and What They Reveal

PyPI serves over 300 billion downloads annually. The top packages by downloads are dominated by transitive dependencies (urllib3, certifi, idna, charset-normalizer are all pulled in by requests). Filtering to direct-use packages reveals the real demand signal.

### Top 15 Direct-Use Packages

| Rank | Package | Downloads/yr | What It Provides | Stdlib Gap |
|------|---------|-------------|-----------------|-----------|
| 1 | **requests** | ~15B | HTTP that actually works | urllib is unusable for real HTTP |
| 2 | **numpy** | ~10B | Numeric arrays and math | No array computing in stdlib |
| 3 | **boto3** | ~7B | AWS SDK | No cloud integration |
| 4 | **python-dateutil** | ~7B | Date parsing and arithmetic | datetime is painful |
| 5 | **pydantic** | ~4B | Data validation and schemas | No runtime type validation |
| 6 | **pytest** | ~3B | Testing framework | unittest is verbose and outdated |
| 7 | **click/typer** | ~3B | CLI framework | argparse is ceremony-heavy |
| 8 | **httpx** | ~2.5B | Async HTTP + HTTP/2 | urllib has no async, no HTTP/2 |
| 9 | **python-dotenv** | ~2B | .env file loading | No env-file support |
| 10 | **SQLAlchemy** | ~1.5B | Database ORM | sqlite3 is raw SQL only |
| 11 | **Pillow** | ~1.5B | Image manipulation | No image processing |
| 12 | **black/ruff** | ~1.2B | Formatting/linting | No built-in formatter |
| 13 | **beautifulsoup4** | ~1B | HTML parsing | html.parser is minimal |
| 14 | **FastAPI** | ~800M | Web framework | No web framework |
| 15 | **cryptography** | ~800M | Modern crypto | hashlib is limited |

### Gap Domains

- **HTTP**: requests, httpx, aiohttp -- stdlib urllib is universally rejected
- **Data Validation**: pydantic -- no runtime schema enforcement
- **Dates**: python-dateutil -- datetime needs external help for parsing
- **Testing**: pytest -- unittest is too verbose
- **CLI**: click, typer -- argparse is ceremony
- **Environment**: python-dotenv -- no .env loading
- **Database**: SQLAlchemy -- no ORM
- **Formatting**: black, ruff -- no canonical formatter (until recently)
- **Web**: FastAPI, Flask, Django -- no web framework
- **Crypto**: cryptography -- hashlib is too limited

---

## 2. JavaScript/TypeScript: Top Packages and What They Reveal

npm hosts over 2.2 million packages. The most depended-on packages reveal fundamental language gaps.

### Top 15 Direct-Use Packages

| Rank | Package | Dependents | What It Provides | Stdlib Gap |
|------|---------|-----------|-----------------|-----------|
| 1 | **lodash** | 69K+ | Utility functions | Missing array/object helpers |
| 2 | **chalk** | 40K+ | Terminal colors | No terminal styling |
| 3 | **commander/yargs** | 32K+ | CLI parsing | No CLI framework |
| 4 | **express** | 27K+ | HTTP server | http module is raw |
| 5 | **axios** | 16K+ | HTTP client | fetch was missing until 2022 |
| 6 | **dotenv** | 15K+ | .env file loading | No env-file support |
| 7 | **uuid** | 14K+ | UUID generation | crypto.randomUUID only recent |
| 8 | **zod** | ~12K | Runtime validation | TypeScript types vanish at runtime |
| 9 | **date-fns/dayjs** | ~10K | Date manipulation | Date object is broken |
| 10 | **winston/pino** | ~8K | Structured logging | console.log is unstructured |
| 11 | **jest/vitest** | ~8K | Testing framework | No built-in test runner (until Node 20) |
| 12 | **pg/mysql2** | ~7K | Database clients | No database support |
| 13 | **prettier/eslint** | ~7K | Formatting/linting | No canonical formatter |
| 14 | **socket.io** | ~5K | WebSocket abstraction | WebSocket API is low-level |
| 15 | **next.js** | ~5K | Meta-framework | No full-stack framework |

### Gap Domains

- **Utility Functions**: lodash -- Array.prototype is insufficient
- **HTTP Client**: axios -- fetch was missing for a decade
- **HTTP Server**: express, fastify, hono -- http.createServer is primitive
- **Validation**: zod, joi, yup -- TS types don't exist at runtime
- **Dates**: date-fns, dayjs, luxon, moment -- Date is broken (Temporal arriving)
- **Environment**: dotenv -- no .env loading
- **CLI**: commander, yargs -- no argument parser
- **Terminal**: chalk -- no color/styling API
- **Logging**: winston, pino -- console.log is not structured
- **Testing**: jest, vitest -- only recently addressed by Node test runner
- **ID Generation**: uuid -- only recently addressed by crypto.randomUUID

---

## 3. Rust: Top Crates and What They Reveal

crates.io hosts over 200,000 crates. Serde alone has 145M+ downloads, making it the most depended-on crate by a large margin.

### Top 15 Direct-Use Crates

| Rank | Crate | Downloads | What It Provides | Stdlib Gap |
|------|-------|----------|-----------------|-----------|
| 1 | **serde** | 145M+ | Serialization framework | No derive-based serialization |
| 2 | **serde_json** | 120M+ | JSON support for serde | No JSON in stdlib |
| 3 | **tokio** | 100M+ | Async runtime | No async runtime in stdlib |
| 4 | **rand** | 37M+ | Random number generation | No random in stdlib |
| 5 | **anyhow** | 52M+ | Application error handling | Error handling is verbose |
| 6 | **thiserror** | 50M+ | Library error derive | Custom errors need boilerplate |
| 7 | **clap** | 45M+ | CLI argument parsing | No CLI parser |
| 8 | **tracing** | 40M+ | Structured logging | log crate is basic |
| 9 | **reqwest** | 35M+ | HTTP client | No HTTP client in stdlib |
| 10 | **chrono** | 30M+ | Date/time handling | std::time is minimal |
| 11 | **regex** | 28M+ | Regular expressions | No regex in stdlib |
| 12 | **uuid** | 25M+ | UUID generation | No UUID in stdlib |
| 13 | **hyper** | 22M+ | HTTP implementation | No HTTP in stdlib |
| 14 | **axum** | 42M+ | Web framework | No web framework |
| 15 | **sqlx** | 15M+ | Async SQL | No database support |

### Gap Domains

- **Serialization**: serde, serde_json -- THE fundamental gap; every program needs it
- **Async Runtime**: tokio -- Rust has async syntax but no runtime
- **Error Handling**: anyhow, thiserror -- std::error::Error is too bare
- **HTTP**: reqwest, hyper, axum -- no networking at all
- **Random**: rand -- no random number generation
- **CLI**: clap -- no argument parser
- **Dates**: chrono -- std::time has no calendar
- **Regex**: regex -- no pattern matching engine
- **Logging**: tracing -- no structured logging
- **UUID**: uuid -- no ID generation
- **Database**: sqlx, diesel -- no database support

Rust's stdlib is deliberately minimal ("batteries not included"), making the gap analysis especially revealing. The Rust community has effectively standardized around specific crates for each gap -- serde, tokio, clap, anyhow are de facto stdlib.

---

## 4. Go: Top Packages and What They Reveal

Go has a large stdlib ("batteries included"), so the third-party packages reveal gaps where even Go's generous stdlib falls short.

### Top 15 Direct-Use Packages

| Rank | Package | Usage % | What It Provides | Stdlib Gap |
|------|---------|--------|-----------------|-----------|
| 1 | **testify** | ~60% | Test assertions and mocking | testing is assertion-free |
| 2 | **gorilla/mux or chi** | ~30% | HTTP routing with params | net/http router is basic |
| 3 | **cobra** | ~25% | CLI framework | flag is primitive |
| 4 | **viper** | ~20% | Configuration management | No config library |
| 5 | **zap/zerolog** | ~20% | Structured logging | log is unstructured |
| 6 | **gorm** | ~18% | ORM | database/sql is raw |
| 7 | **google/uuid** | ~15% | UUID generation | No UUID in stdlib |
| 8 | **golang-jwt/jwt** | ~12% | JWT tokens | No JWT support |
| 9 | **go-redis** | ~10% | Redis client | No Redis client |
| 10 | **aws-sdk-go** | ~10% | AWS SDK | No cloud SDKs |
| 11 | **gin/echo/fiber** | ~48%+ | Web framework | net/http is raw |
| 12 | **grpc-go** | ~8% | gRPC client/server | No gRPC in stdlib |
| 13 | **golangci-lint** | ~40% | Linter aggregator | go vet is limited |
| 14 | **samber/lo** | growing | Lodash-like utilities | No generic collection helpers (pre-1.18) |
| 15 | **godotenv** | ~5% | .env file loading | No env-file support |

### Gap Domains

- **Testing**: testify -- stdlib testing has no assertions
- **HTTP Routing**: chi, gorilla/mux -- stdlib router lacks path parameters
- **CLI**: cobra -- flag package is primitive
- **Configuration**: viper -- no config file parsing
- **Logging**: zap, zerolog -- log is unstructured
- **Database/ORM**: gorm, sqlx -- database/sql is raw
- **UUID**: google/uuid -- no ID generation
- **Auth**: golang-jwt -- no JWT support
- **Environment**: godotenv -- no .env loading
- **Web Framework**: gin, echo, fiber -- net/http is too low-level for most

Go's big stdlib (HTTP server, JSON, crypto, testing) means its gaps are in higher-level concerns: frameworks, assertions, configuration, and structured logging. The gap between "stdlib has it" and "developers install something else anyway" is instructive.

---

## 5. Ruby: Top Gems and What They Reveal

RubyGems hosts the gems ecosystem. Rails dominance means many top gems are Rails-related.

### Top 15 Direct-Use Gems

| Rank | Gem | Downloads | What It Provides | Stdlib Gap |
|------|-----|----------|-----------------|-----------|
| 1 | **rails** | 500M+ | Full-stack web framework | No web framework |
| 2 | **bundler** | 400M+ | Dependency management | No dependency manager (was separate) |
| 3 | **rack** | 350M+ | HTTP server interface | No standard HTTP interface |
| 4 | **nokogiri** | 300M+ | HTML/XML parsing | REXML is slow and limited |
| 5 | **rspec** | 250M+ | Testing framework | Test::Unit is verbose |
| 6 | **activesupport** | 250M+ | Core extensions | Missing string/date/hash helpers |
| 7 | **devise** | 100M+ | Authentication | No auth framework |
| 8 | **puma** | 100M+ | Web server | WEBrick is too slow |
| 9 | **sidekiq** | 80M+ | Background jobs | No job queue |
| 10 | **faraday/httparty** | 70M+ | HTTP client | Net::HTTP is verbose |
| 11 | **rubocop** | 60M+ | Linter/formatter | No canonical formatter |
| 12 | **dotenv** | 50M+ | .env file loading | No env-file support |
| 13 | **pg** | 50M+ | PostgreSQL client | No database drivers |
| 14 | **redis** | 40M+ | Redis client | No Redis support |
| 15 | **faker** | 30M+ | Test data generation | No fake data generator |

### Gap Domains

- **Web Framework**: rails, sinatra -- no web framework
- **HTTP Client**: faraday, httparty -- Net::HTTP is verbose
- **HTML Parsing**: nokogiri -- REXML is limited
- **Testing**: rspec -- Test::Unit is verbose
- **Auth**: devise -- no auth framework
- **Background Jobs**: sidekiq -- no job queue
- **Environment**: dotenv -- no .env loading
- **Database**: pg, mysql2 -- no database drivers beyond sqlite
- **Formatting**: rubocop -- no canonical formatter

---

## 6. Cross-Language Gap Synthesis

### Capability Domains That Appear in EVERY Language

These are the universal gaps -- capabilities that no major language's stdlib adequately provides, forcing every ecosystem to reinvent the wheel.

#### 6.1 HTTP Client (Usable, Not Raw)

| Language | Stdlib | What People Actually Use | The Gap |
|----------|--------|------------------------|---------|
| Python | urllib | requests, httpx | urllib's API is hostile |
| JS/TS | fetch (recent) | axios, got | fetch was missing for a decade |
| Rust | (none) | reqwest | No HTTP at all |
| Go | net/http | net/http (good enough) | Go is the exception |
| Ruby | Net::HTTP | faraday, httparty | Net::HTTP is verbose |

**Universal truth**: Every language needs a one-line HTTP GET and a three-line HTTP POST with JSON. Go is the only language where the stdlib is good enough, and even there people use frameworks on top.

**ilo status**: `get url` / `$url` already provides this. `post url body` is planned. ilo is ahead of every language here -- 2 tokens for HTTP GET.

#### 6.2 Environment Variable / .env Loading

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | os.environ | python-dotenv |
| JS/TS | process.env | dotenv |
| Rust | std::env | dotenvy |
| Go | os.Getenv | godotenv |
| Ruby | ENV | dotenv |

**Universal truth**: Every language can read env vars, but none loads .env files by default. The dotenv pattern is the single most cross-language package -- same concept, same name, five ecosystems.

**ilo status**: `env key` is planned (I3). The .env loading question is a runtime concern, not a language concern. The runtime could load .env files before execution.

#### 6.3 Data Validation / Runtime Schema Enforcement

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | (none) | pydantic, attrs, dataclasses |
| JS/TS | (TypeScript types vanish) | zod, joi, yup |
| Rust | (serde + custom) | serde, validator |
| Go | (struct tags) | go-playground/validator |
| Ruby | (none) | ActiveModel validations, dry-validation |

**Universal truth**: Static types are not enough. Programs need runtime validation of data from external sources (APIs, user input, config files). TypeScript makes this painfully obvious -- types disappear at runtime, so zod exists to bring them back.

**ilo status**: ilo's tool declarations ARE runtime schemas. `tool get-user"..." uid:t>R profile t` declares the expected shape. The runtime validates tool responses against declared types. This is pydantic/zod built into the language, not bolted on. ilo is ahead of every language here.

#### 6.4 CLI Argument Parsing

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | argparse | click, typer |
| JS/TS | process.argv | commander, yargs |
| Rust | (none) | clap |
| Go | flag | cobra |
| Ruby | OptionParser | thor |

**Universal truth**: Every language's built-in argument parser is either missing or insufficient for real CLI applications. The gap is between "parse flags" and "build a CLI application with subcommands, help text, and validation."

**ilo status**: ilo passes CLI args directly to function parameters. No parser needed -- the function signature IS the interface. This is the right choice for agent programs. CLI parsing is a human concern (help text, tab completion, subcommands) that agents never need.

#### 6.5 Date/Time Beyond Timestamps

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | datetime | python-dateutil, arrow, pendulum |
| JS/TS | Date (broken) | date-fns, dayjs, luxon, moment; Temporal arriving |
| Rust | std::time (minimal) | chrono, time |
| Go | time (good) | Go's time is adequate |
| Ruby | Time/Date (OK) | ActiveSupport time extensions |

**Universal truth**: Getting the current timestamp is easy. Parsing dates, formatting them, doing arithmetic, handling timezones -- every language struggles. JavaScript's Date is the worst offender, but even Python's datetime needs dateutil for basic parsing.

**ilo status**: `now()` (timestamp) and `slp n` (sleep) are planned (I6). ISO 8601 formatting (`iso n`) and parsing (`ts t`) cover 90% of agent needs. Complex date manipulation is a tool concern.

#### 6.6 UUID / Unique ID Generation

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | uuid (good) | uuid (stdlib is fine!) |
| JS/TS | crypto.randomUUID (recent) | uuid package |
| Rust | (none) | uuid crate |
| Go | (none) | google/uuid |
| Ruby | SecureRandom.uuid | SecureRandom (stdlib is fine) |

**Universal truth**: Programs need unique identifiers. Only Python and Ruby have adequate stdlib support. The rest need third-party packages. This is a trivially small feature with enormous reach.

**ilo status**: `uid()` is planned. One token, one opcode. High value, low cost.

#### 6.7 Structured Logging

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | logging | structlog, loguru |
| JS/TS | console.log | winston, pino, bunyan |
| Rust | (none) | tracing, log + env_logger |
| Go | log | zap, zerolog, slog (1.21) |
| Ruby | Logger | Rails logger, semantic_logger |

**Universal truth**: Every language has print-to-stdout. No language (until Go 1.21's slog) has structured logging in stdlib. The gap is between "print a string" and "emit a structured log event with severity, timestamp, and fields."

**ilo status**: Not planned. Agent programs are short-lived scripts, not long-running services. stdout IS the logging mechanism. The agent framework that invokes ilo handles logging. This is a correct omission for ilo's use case.

#### 6.8 Testing Framework

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | unittest | pytest |
| JS/TS | (none, until Node 20) | jest, vitest, mocha |
| Rust | #[test] (good) | Built-in is good enough |
| Go | testing (bare) | testify |
| Ruby | Test::Unit | rspec |

**Universal truth**: Every language's built-in test runner is either missing or insufficient. The gap is between "run a function and check if it panics" and "assertions, fixtures, parameterized tests, mocking."

**ilo status**: Not planned. Agent programs are verified before execution. Testing is a human development concern. ilo's verifier catches type errors, missing functions, and exhaustiveness gaps -- the class of errors that testing catches in other languages. This is a correct omission.

#### 6.9 Database Access / ORM

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | sqlite3 | SQLAlchemy, Django ORM |
| JS/TS | (none) | prisma, pg, mysql2, drizzle |
| Rust | (none) | sqlx, diesel, sea-orm |
| Go | database/sql (raw) | gorm, sqlx |
| Ruby | (none) | ActiveRecord, Sequel |

**Universal truth**: Programs need databases. No language provides a usable ORM or query builder in stdlib. Even Go's database/sql, which provides the interface, requires third-party drivers and most developers use an ORM on top.

**ilo status**: Database access is a tool concern. `tool query"Run SQL" sql:t params:L t>R L record t` -- tools handle the driver, connection, and query execution. ilo composes the typed results. This is correct.

#### 6.10 Code Formatting / Linting

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | (none, until recently) | black, ruff, flake8 |
| JS/TS | (none) | prettier, eslint |
| Rust | rustfmt (built-in!) | rustfmt + clippy |
| Go | gofmt (built-in!) | gofmt + golangci-lint |
| Ruby | (none) | rubocop |

**Universal truth**: Code needs a canonical format. Only Rust and Go ship with an official formatter. Python, JS, and Ruby all needed third-party solutions. The languages that ship formatters (Go, Rust) have notably less style debate.

**ilo status**: Dense format is canonical. `dense(parse(dense(parse(src)))) == dense(parse(src))`. No formatting choices, no style debates. ilo is ahead of every language except Go and Rust here.

#### 6.11 Hashing / Cryptographic Primitives

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | hashlib, hmac | cryptography (for advanced) |
| JS/TS | crypto.subtle | crypto (mostly adequate) |
| Rust | (none) | sha2, hmac, ring |
| Go | crypto/* (good) | crypto/* (stdlib is fine) |
| Ruby | Digest, OpenSSL | OpenSSL (stdlib is fine) |

**Universal truth**: SHA-256 and HMAC are needed for webhook verification, API authentication, and data integrity. Most languages have adequate stdlib support, but Rust has none.

**ilo status**: `sha t` and `hmac key t` planned (I9). Correct scope -- webhook verification and API auth, not full encryption.

#### 6.12 Random Number Generation

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | random, secrets | random (stdlib is fine) |
| JS/TS | Math.random() | crypto.getRandomValues, uuid |
| Rust | (none) | rand crate |
| Go | math/rand, crypto/rand | Both adequate |
| Ruby | Random, SecureRandom | SecureRandom (stdlib is fine) |

**Universal truth**: Programs need random numbers. Most languages provide this in stdlib, but Rust notably does not. The gap between "random float" and "cryptographically secure random" matters for security-sensitive applications.

**ilo status**: `rnd()` / `rnd a b` planned. Should default to cryptographically secure.

#### 6.13 Regex Engine

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | re | re (stdlib is fine) |
| JS/TS | RegExp (native) | Built-in is fine |
| Rust | (none) | regex crate |
| Go | regexp (good) | regexp (stdlib is fine) |
| Ruby | Regexp (native) | Built-in is fine |

**Universal truth**: Regex is in every language's stdlib except Rust. It is a fundamental text processing capability. LLMs are excellent at generating regex patterns, making this especially agent-relevant.

**ilo status**: Planned (I8) as builtins behind a feature flag. `rgx pat text`, `rga pat text`, `rgs pat repl text`.

#### 6.14 HTML/XML Parsing

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | html.parser (minimal) | beautifulsoup4, lxml |
| JS/TS | DOMParser (browser only) | fast-xml-parser, cheerio |
| Rust | (none) | scraper, html5ever |
| Go | html, xml (basic) | goquery |
| Ruby | REXML (slow) | nokogiri |

**Universal truth**: Web scraping and HTML processing need a real parser. No language provides one that developers are happy with.

**ilo status**: Tool concern. HTML parsing is domain-specific. A tool like `tool scrape"Extract from HTML" html:t sel:t>R t t` handles it.

#### 6.15 Web Framework / HTTP Server

| Language | Stdlib | What People Actually Use |
|----------|--------|------------------------|
| Python | http.server (toy) | FastAPI, Flask, Django |
| JS/TS | http.createServer | express, fastify, hono, next.js |
| Rust | (none) | axum, actix-web, rocket |
| Go | net/http (usable) | gin, echo, fiber |
| Ruby | WEBrick (toy) | rails, sinatra |

**Universal truth**: Every language needs a web framework. This is the most popular package category across all ecosystems.

**ilo status**: Not planned. Agents consume APIs, they don't serve them. Correct omission.

---

## 7. Cross-Language Gap Analysis Matrix

The master table. Each row is a capability that at least 3 of the 5 languages need third-party packages to address.

| # | Capability | Python | JS/TS | Rust | Go | Ruby | Universal? | Agent-Relevant? |
|---|-----------|--------|-------|------|-----|------|-----------|----------------|
| 1 | **HTTP client (usable)** | requests | axios/fetch | reqwest | net/http (OK) | faraday | 4/5 | **YES** -- agents call APIs |
| 2 | **Env var / .env loading** | dotenv | dotenv | dotenvy | godotenv | dotenv | **5/5** | **YES** -- agents need secrets |
| 3 | **Data validation / schemas** | pydantic | zod | serde+validator | validator | dry-valid | **5/5** | **YES** -- agents handle external data |
| 4 | **CLI parsing** | click | commander | clap | cobra | thor | **5/5** | No -- human concern |
| 5 | **Date/time beyond timestamps** | dateutil | date-fns | chrono | time (OK) | activesupport | 4/5 | Partial -- timestamps yes, calendars no |
| 6 | **UUID generation** | uuid (OK) | uuid | uuid | google/uuid | SecureRandom (OK) | 3/5 | **YES** -- agents create records |
| 7 | **Structured logging** | structlog | pino | tracing | zap/slog | semantic_log | **5/5** | No -- runtime concern |
| 8 | **Testing framework** | pytest | jest | built-in (OK) | testify | rspec | 4/5 | No -- human concern |
| 9 | **Database / ORM** | SQLAlchemy | prisma | sqlx | gorm | ActiveRecord | **5/5** | Partial -- tool concern |
| 10 | **Code formatting** | black | prettier | rustfmt (OK) | gofmt (OK) | rubocop | 3/5 | No -- human concern |
| 11 | **Hashing (SHA/HMAC)** | hashlib (OK) | crypto (OK) | sha2/hmac | crypto (OK) | Digest (OK) | 1/5 | **YES** -- webhook verification |
| 12 | **Random numbers** | random (OK) | Math.random | rand | math/rand (OK) | Random (OK) | 1/5 | **YES** -- ID gen, sampling |
| 13 | **Regex** | re (OK) | RegExp (OK) | regex crate | regexp (OK) | Regexp (OK) | 1/5 | **YES** -- text extraction |
| 14 | **HTML/XML parsing** | bs4/lxml | cheerio | scraper | goquery | nokogiri | **5/5** | Partial -- tool concern |
| 15 | **Web framework** | FastAPI | express | axum | gin | rails | **5/5** | No -- agents don't serve |
| 16 | **JSON parse/serialize** | json (OK) | JSON (OK) | serde_json | json (OK) | json (OK) | 1/5 | **YES** -- data interchange |
| 17 | **Async runtime** | asyncio (OK-ish) | built-in | tokio | goroutines (OK) | (limited) | 2/5 | Partial -- runtime concern |
| 18 | **Error handling ergonomics** | (exceptions) | (exceptions) | anyhow/thiserror | (if err!=nil) | (exceptions) | 2/5 | **YES** -- every call can fail |
| 19 | **String replace/trim** | built-in (OK) | built-in (OK) | built-in (OK) | strings (OK) | built-in (OK) | 0/5 | **YES** -- text processing |
| 20 | **Template/interpolation** | f-strings (OK) | template lit (OK) | format! (OK) | fmt (OK) | interpolation (OK) | 0/5 | **YES** -- building messages |

---

## 8. Human Concerns vs Agent Concerns

Not all gaps matter equally for ilo. Classifying by who actually needs the capability:

### Pure Human Concerns (ilo should NOT address)

| Capability | Why Human-Only |
|-----------|---------------|
| CLI parsing (click, cobra, clap) | Help text, tab completion, subcommands -- agents pass positional args |
| Testing framework (pytest, jest) | Agents don't write tests; the verifier catches errors |
| Code formatting (black, prettier) | ilo has a canonical dense format |
| Structured logging (pino, zap) | Agent programs are short-lived; stdout suffices |
| Web framework (express, FastAPI) | Agents consume APIs, not serve them |
| Background jobs (celery, sidekiq) | Infrastructure concern, not composition concern |
| Auth framework (devise, passport) | Application concern, not language concern |

### Pure Agent Concerns (ilo SHOULD address)

| Capability | Why Agent-Critical |
|-----------|-------------------|
| HTTP client (usable GET/POST) | Agents call APIs constantly |
| Environment variables | Agents need API keys and config from env |
| Data validation / schemas | Agents handle untrusted external data |
| JSON parse/serialize | Universal data interchange format |
| UUID generation | Creating records, request IDs, correlation |
| Hashing (SHA-256, HMAC) | Webhook verification, API auth signatures |
| Error handling ergonomics | Every tool call can fail |
| String manipulation | Building URLs, messages, payloads |
| Shell execution | Running system commands (git, build tools) |
| Regex | Extracting structured data from text |

### Shared Concerns (address partially)

| Capability | What Agents Need | What Humans Need |
|-----------|-----------------|-----------------|
| Date/time | Timestamps, ISO 8601, sleep | Calendars, timezones, formatting |
| Database | Call a query tool | Connection pooling, migrations, ORM |
| HTML parsing | Extract text from a page | DOM traversal, CSS selectors |
| Random numbers | Generate IDs, nonces | Statistical distributions, seeding |
| Async/parallel | Parallel tool calls | Event loops, streams, channels |

---

## 9. What This Means for ilo: Builtin vs Tool Boundary

The cross-language analysis reveals a clear partition:

### Tier 1: Must Be Builtins (every agent needs these, token cost of tool declaration is prohibitive)

These capabilities appear in the "agent-critical" column AND are needed so frequently that declaring them as tools on every program wastes tokens.

| Builtin | Capability | Cross-Language Evidence | ilo Status |
|---------|-----------|------------------------|-----------|
| `get url` | HTTP GET | 4/5 languages need third-party | Done |
| `post url body` | HTTP POST | Same packages provide both GET and POST | Planned (G1) |
| `env key` | Environment variables | dotenv in 5/5 languages | Planned (I3) |
| `sha t` | SHA-256 hash | Used for webhook verification in every ecosystem | Planned (I9) |
| `hmac key t` | HMAC signing | Required by Stripe, GitHub, AWS, Slack APIs | Planned (I9) |
| `uid()` | UUID generation | 3/5 languages need third-party | Planned |
| `now()` | Current timestamp | Every language has this but ilo doesn't yet | Planned (I6) |
| `slp n` | Sleep/delay | Rate limiting, polling | Planned (I6) |
| `rpl t old new` | String replace | Built-in everywhere but ilo | Planned |
| `trm t` | Trim whitespace | Built-in everywhere but ilo | Planned |
| `sh cmd` | Shell execution | Ruby backticks, subprocess, child_process | Planned |

### Tier 2: Builtins Behind Feature Flags (important but not universal)

These are valuable for many agent programs but add binary size or security surface.

| Builtin | Capability | Feature Flag | Cross-Language Evidence |
|---------|-----------|-------------|------------------------|
| `rgx pat t` | Regex match | `regex` | 1/5 need third-party but every agent uses regex |
| `rga pat t` | Regex find all | `regex` | Same |
| `rgs pat rep t` | Regex replace | `regex` | Same |
| `b64e t` | Base64 encode | `encoding` | Auth headers, binary data |
| `b64d t` | Base64 decode | `encoding` | Same |
| `urle t` | URL encode | `encoding` | Query string construction |
| `urld t` | URL decode | `encoding` | Same |
| `rd path` | Read file | `fs` | faraday/fs in every language |
| `wr path data` | Write file | `fs` | Same |
| `rnd()` | Random float | `crypto` | ID generation, sampling |
| `iso n` | Timestamp to ISO 8601 | `time` | API date formatting |
| `ts t` | ISO 8601 to timestamp | `time` | API date parsing |

### Tier 3: Tool Concern (declare as tools, not builtins)

These capabilities appear in every language's ecosystem but are too domain-specific, too complex, or too security-sensitive for language builtins.

| Capability | Why Tool, Not Builtin | Equivalent Tool Declaration |
|-----------|----------------------|---------------------------|
| Database access | Drivers, connection pooling, query optimization | `tool query"Run SQL" sql:t>R L rec t` |
| HTML/XML parsing | Complex parsers, CSS selectors, DOM | `tool scrape"Extract" html:t sel:t>R t t` |
| Web framework | Agents don't serve HTTP | N/A |
| Background jobs | Infrastructure concern | N/A |
| Auth/JWT | Security-critical, complex | `tool verify-jwt"..." tok:t>R claims t` |
| Email sending | External service | `tool send-email"..." to:t subj:t body:t>R _ t` |
| Image processing | Domain-specific | `tool resize-img"..." path:t w:n h:n>R t t` |
| CSV/YAML/TOML parsing | Format-specific | `tool parse-csv"..." data:t>R L L t t` |
| Encryption | Dangerous to get wrong | `tool encrypt"..." data:t key:t>R t t` |

### Tier 4: Not Needed (ilo's design eliminates the need)

These capabilities are major package categories in other languages but ilo's design makes them irrelevant.

| Capability | Why ilo Doesn't Need It |
|-----------|------------------------|
| CLI parsing | Function parameters ARE the interface |
| Testing framework | Verifier catches type/call/exhaustiveness errors |
| Code formatter | Dense format is canonical and non-configurable |
| Structured logging | Short-lived scripts; stdout is the log |
| ORM/query builder | Tools return typed records; composition is ilo's job |
| Data validation library | Tool declarations ARE schemas; runtime validates |
| Template engine | `+` concat in prefix notation is sufficient |
| Package manager | Self-contained programs; no imports, no dependencies |
| Async/await syntax | Runtime parallelizes independent calls from DAG |

---

## 10. The "ilo Advantage" -- Gaps Already Closed by Design

The most interesting finding is not what ilo needs to add, but what ilo's design already eliminates:

### 10.1 Data Validation (pydantic/zod equivalent)

In Python, you install pydantic to validate API responses. In TypeScript, you install zod because types vanish at runtime. In ilo:

```
tool get-user"Retrieve user" uid:t>R profile t
type profile{id:t;name:t;email:t;verified:b}
```

The tool declaration IS the schema. The runtime validates responses against it. The agent never writes validation code. This eliminates an entire category of packages.

**Token savings vs Python/pydantic**: ~50 tokens per validated API call.

### 10.2 Error Handling Ergonomics (anyhow/thiserror equivalent)

Rust developers install anyhow to avoid writing `impl Display for MyError`. Go developers write `if err != nil` on every third line. In ilo:

```
d=get-user! uid          -- auto-unwrap: propagate error or give me the value
get-user uid;?{^e:^+"Failed: "e;~d:use d}  -- explicit handling with context
```

The `!` operator is Rust's `?` operator, Go's `if err != nil { return err }`, and Python's `try/except` -- all in one token.

### 10.3 Code Formatting (black/prettier equivalent)

ilo has exactly one format: dense. No configuration, no debates, no packages to install.

### 10.4 Package Management (npm/pip/cargo equivalent)

ilo programs are self-contained. No imports, no dependencies, no lock files, no node_modules. Tools are declared in the program. The complexity of package management -- which spawned entire ecosystems (npm, pip, cargo, bundler) -- does not exist.

### 10.5 CLI Parsing (click/cobra/clap equivalent)

```bash
ilo 'f x:n y:n>n;+x y' 3 4
```

Function parameters are the CLI interface. No argparse, no commander, no clap. The function signature provides types, arity, and names. The runtime handles conversion.

---

## 11. Priority Ranking for ilo Implementation

Based on the cross-language evidence, ranked by (frequency of need) x (token savings) x (agent relevance):

### Critical (blocks real agent workflows)

1. **`env key`** -- every agent needs API keys. dotenv is in 5/5 ecosystems. 2 tokens.
2. **`post url body`** -- every API interaction beyond GET. 3 tokens.
3. **`sh cmd`** -- universal escape hatch. Ruby/PHP proved this works. 2 tokens.
4. **JSON builtins** (`jsn t`, `ser v`) -- universal interchange format. 2 tokens each.

### High (common agent tasks)

5. **`uid()`** -- record creation, request correlation. 1 token.
6. **`now()`** -- timestamps, timing, cache keys. 1 token.
7. **`sha t` / `hmac key t`** -- webhook verification, API auth. 2-3 tokens.
8. **`rpl t old new`** -- string replacement. Used in every program. 4 tokens.
9. **`trm t`** -- whitespace cleanup from API responses. 2 tokens.
10. **`slp n`** -- rate limiting, polling intervals. 2 tokens.

### Medium (important but deferrable)

11. **Regex** (`rgx`, `rga`, `rgs`) -- text extraction. Feature flag. 3 tokens each.
12. **Encoding** (`b64e`, `b64d`, `urle`, `urld`) -- auth headers, URL construction. 2 tokens each.
13. **File I/O** (`rd`, `wr`) -- local file access. Feature flag. 2-3 tokens.
14. **Time formatting** (`iso n`, `ts t`) -- API date handling. 2 tokens each.
15. **`upr t` / `lwr t`** -- case conversion. 2 tokens each.

### Low (nice to have, can wait)

16. **Random** (`rnd()`, `rnd a b`) -- ID generation (uid covers most cases). 1-3 tokens.
17. **Modulo** (`%a b`) -- cycling, divisibility. 3 tokens.
18. **Power** (`pow a b`) -- exponential backoff. 3 tokens.
19. **`pfx t p` / `sfx t s`** -- prefix/suffix check (workaround with `slc` + `=`). 3 tokens.
20. **Parallel execution** (`par{...}`) -- needs runtime design. Variable tokens.

---

## 12. The Universal Lesson

The cross-language analysis reveals a hierarchy of software needs:

```
Layer 0: Compute (math, logic, control flow)         -- every language has this
Layer 1: Text processing (strings, regex, encoding)  -- most languages have this
Layer 2: I/O (HTTP, files, processes, env vars)       -- stdlib quality varies wildly
Layer 3: Data interchange (JSON, validation, schemas) -- almost always third-party
Layer 4: Frameworks (web, testing, CLI, ORM)          -- always third-party
Layer 5: Domain (images, ML, crypto, cloud)           -- always third-party
```

ilo should own Layers 0-2 as builtins, handle Layer 3 at the tool boundary (schemas are built-in, format parsing is tools), and leave Layers 4-5 entirely to tools.

The packages that every language installs first -- requests, dotenv, pydantic/zod, uuid -- map to Layers 2-3. These are the gaps ilo must fill. The packages that build ecosystems -- express, pytest, SQLAlchemy -- map to Layers 4-5. These are tool concerns.

The deepest insight: **ilo's tool declaration system eliminates Layer 3 entirely.** No other language does this. Tool declarations are schemas, schemas are validation, validation is the tool boundary. What pydantic is to Python and zod is to TypeScript, `tool name"desc" params>return` is to ilo -- but built into the language, not installed after.

---

## 13. Comparison with ilo's Existing Research

This analysis aligns with and extends the existing research files:

- **go-stdlib-research.md**: Confirmed Go's stdlib strengths (HTTP, JSON, crypto). Go's gaps (testing assertions, routing, structured logging) are human concerns that ilo correctly omits.
- **python-stdlib-analysis.md**: Confirmed the priority ranking (HTTP > env > shell > validation). Python's biggest lessons: `R` type is better than exceptions, tool declarations beat pydantic.
- **js-ts-ecosystem.md**: Confirmed the validation gap (zod) is ilo's biggest advantage. TypeScript types vanishing at runtime is THE problem ilo's tool schemas solve.
- **ruby-php-research.md**: Confirmed shell execution (`sh`) as high priority. Ruby's backtick pattern validates the design.
- **swift-kotlin-research.md**: Confirmed structured concurrency should be runtime-level, not syntax-level. Optional type (`O`) is validated by both Swift and Kotlin.

### New findings not in existing research

1. **The dotenv pattern** is the single most universal package across all 5 ecosystems -- same name, same concept, five languages. This validates `env` as critical priority.
2. **UUID generation** appears as a gap in 3/5 languages, stronger signal than expected.
3. **The Layer 0-5 hierarchy** provides a principled framework for the builtin/tool boundary.
4. **ilo's tool declarations already solve Layer 3** -- this was implicit in the existing research but not explicitly called out as ilo's biggest advantage over all five ecosystems.

---

## Appendix A: Data Sources

Package download statistics and dependency counts sourced from:

- Python: [PyPI Stats](https://pypistats.org/), [Top PyPI Packages](https://hugovk.github.io/top-pypi-packages/)
- JavaScript: [npm trends](https://npmtrends.com/), [npm rank](https://gist.github.com/anvaka/8e8fa57c7ee1350e3491), [npm-high-impact](https://github.com/wooorm/npm-high-impact)
- Rust: [crates.io](https://crates.io/crates?sort=recent-downloads), [lib.rs/stats](https://lib.rs/stats), [State of the Crates 2025](https://ohadravid.github.io/posts/2024-12-state-of-the-crates/)
- Go: [JetBrains Go Ecosystem 2025](https://blog.jetbrains.com/go/2025/11/10/go-language-trends-ecosystem-2025/), [Go packages by import count](https://medium.com/skyline-ai/most-imported-golang-packages-some-insights-fb12915a07)
- Ruby: [RubyGems Stats](https://rubygems.org/stats), [BestGems](https://bestgems.org/), [Ruby Toolbox](https://www.ruby-toolbox.com/)

## Appendix B: Methodology

"Practically required" is defined as:
1. Package appears in the top 15 by downloads or dependents in its ecosystem
2. Package addresses a capability gap (not just a framework preference)
3. The capability it provides is used by >50% of non-trivial programs in that language
4. No adequate stdlib alternative exists (or the stdlib alternative is universally rejected)

Packages that are primarily transitive dependencies (urllib3, certifi, idna in Python; tslib, supports-color in JS) are excluded from the direct-use rankings.

"Agent-relevant" is defined as: an AI agent composing tool calls would need this capability, as opposed to a human developer building an application.

---

## See Also

- [essential-packages-analysis.md](essential-packages-analysis.md) — complementary analysis: same phenomenon from the "what developers install" angle
- [OPEN.md](OPEN.md) — builtin naming decisions consolidating proposals from both documents
