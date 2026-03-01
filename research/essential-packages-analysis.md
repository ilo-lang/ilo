# Essential Packages: What Language Designers Missed

Cross-language analysis of packages so universally installed they are effectively part of
the language. This reveals the gap between what designers thought was core vs what developers
actually need -- and what it means for ilo, where agents cannot install packages.

---

## The Phenomenon

Every major programming language has a set of third-party packages that appear in >50% of
projects. These are not niche libraries -- they are capabilities that the language's standard
library lacks but that real-world programs universally require. The existence of these
"essential packages" is a design signal: it tells us what the language got wrong about scope.

For ilo, this signal is critical. ilo programs are self-contained: an agent cannot
`pip install requests` or `npm install axios`. Whatever an ilo program needs must be either
a builtin or a declared tool. The essential packages of other languages are a map of exactly
what builtins ilo needs.

---

## Language-by-Language Catalog

### Python

Python's standard library is famously "batteries included" -- yet the ecosystem has
converged on a set of third-party packages that nearly every project installs.

| Package | Installs/month | % of projects | Capability gap |
|---------|---------------|---------------|----------------|
| **requests** | 300M+ | ~70% | Usable HTTP client (urllib is painful) |
| **pydantic** | 200M+ | ~55% | Data validation and settings management |
| **pytest** | 150M+ | ~65% (of tested projects) | Modern testing framework |
| **python-dotenv** | 120M+ | ~50% | Load .env files into environment |
| **click / typer** | 100M+ | ~45% | CLI argument parsing (argparse is verbose) |
| **black** | 80M+ | ~50% (of formatted projects) | Code formatting |
| **mypy / pyright** | 70M+ | ~45% | Static type checking |
| **flask / fastapi** | 60M+ | ~40% (of web projects) | Web framework |
| **pandas** | 60M+ | ~35% | Tabular data manipulation |
| **boto3** | 55M+ | ~30% | AWS SDK |
| **httpx** | 50M+ | ~25% | Async HTTP client |

**What the stdlib got wrong:**

1. **HTTP client.** `urllib` and `urllib2` exist but are so painful to use that `requests`
   became the de facto standard. The API difference is stark:
   ```python
   # urllib (stdlib): 5 lines, manual encoding, manual error handling
   req = urllib.request.Request(url, headers={'Authorization': f'Bearer {token}'})
   try:
       response = urllib.request.urlopen(req, timeout=10)
       data = json.loads(response.read().decode('utf-8'))
   except urllib.error.HTTPError as e:
       handle_error(e)

   # requests (third-party): 1 line, obvious API
   data = requests.get(url, headers={'Authorization': f'Bearer {token}'}, timeout=10).json()
   ```
   The lesson: a bad stdlib API is worse than no stdlib API. Developers will replace it.

2. **Data validation.** Python has no runtime type enforcement. `pydantic` fills this gap
   by turning type annotations into runtime validators. This is a signal that developers
   need structured data validation at the boundary between trusted and untrusted data --
   exactly what ilo's type verifier and tool boundaries provide.

3. **Environment configuration.** `os.getenv()` exists, but `python-dotenv` is installed in
   half of all projects because developers need to load configuration from `.env` files in
   development. The pattern: config comes from environment variables, but those variables
   need to be loaded from somewhere.

4. **Code formatting.** `black` exists because Python has no official formatter. Go's
   `gofmt` solved this at the language level. ilo's dense format solves it at the
   specification level -- there is exactly one canonical form.

5. **Type checking.** Python added type hints in 3.5 (2015) but no type checker. `mypy`
   fills this gap. The lesson: types without enforcement are suggestions. ilo enforces
   types before execution -- verification is built into the language, not bolted on.

---

### JavaScript / TypeScript

The JS ecosystem is famously fragmented, but a core set of packages has emerged as
near-universal.

| Package | Weekly downloads | % of projects | Capability gap |
|---------|----------------|---------------|----------------|
| **typescript** | 50M+ | ~70% (of all JS) | Type system (JS has none) |
| **eslint** | 40M+ | ~65% | Linting (no built-in linter) |
| **prettier** | 35M+ | ~55% | Code formatting (no built-in) |
| **axios / node-fetch** | 30M+ | ~50% | HTTP client (fetch was not always in Node) |
| **dotenv** | 30M+ | ~55% | Load .env files into process.env |
| **jest / vitest** | 25M+ | ~60% (of tested) | Testing framework |
| **zod** | 20M+ | ~35% | Runtime type validation |
| **lodash** | 45M+ | ~40% (declining) | Utility functions (array, object, string) |
| **chalk** | 25M+ | ~35% | Terminal colors |
| **commander / yargs** | 20M+ | ~30% | CLI argument parsing |
| **express / fastify** | 15M+ | ~40% (of server projects) | Web framework |
| **fs-extra** | 15M+ | ~25% | File operations that fs lacks |
| **debug** | 40M+ | ~40% (as transitive) | Debug logging with namespaces |
| **inquirer / prompts** | 10M+ | ~20% | Interactive CLI prompts |

**What the ecosystem reveals:**

1. **TypeScript IS JavaScript.** When 70% of JavaScript projects install TypeScript, the
   language has effectively admitted that dynamic typing is insufficient for production code.
   TypeScript is the single most popular npm package because the language's core design
   choice -- no types -- was wrong. ilo's position (types verified before execution) is
   validated by the entire JS ecosystem voting with their `package.json`.

2. **fetch took 14 years.** Node.js launched in 2009. `node-fetch` polyfilled the browser's
   `fetch` API. Native `fetch` did not land in Node.js until v18 (2022). For 13 years,
   every Node.js project that made HTTP requests needed a third-party dependency for the
   most basic networking operation. The lesson: HTTP clients should ship day one.

3. **dotenv is a confession.** Every language's ecosystem independently invented a `.env`
   loader. This is not a coincidence -- it is a universal need that zero languages
   addressed in their stdlib. The need: load configuration from the environment, with a
   local override file for development.

4. **zod and pydantic are twins.** JavaScript's `zod` and Python's `pydantic` solve the
   same problem: validate data at the boundary between trusted code and untrusted input
   (API responses, user data, config files). Both languages lack runtime type validation.
   ilo's type system + tool boundaries handle this natively.

5. **lodash is dying because the stdlib caught up.** Lodash provided `map`, `filter`,
   `reduce`, `cloneDeep`, `debounce`, `throttle`, and dozens of other utilities that JS
   lacked. As ES6+ added `Array.prototype.map/filter/reduce`, `Object.entries/values/keys`,
   `structuredClone`, and optional chaining, lodash usage declined. The lesson: when the
   stdlib provides the capability, the third-party package dies. This confirms that
   building capabilities into the language is the right approach.

---

### Ruby

Ruby's ecosystem is smaller but highly concentrated around a few dominant packages.

| Package | Downloads | % of projects | Capability gap |
|---------|----------|---------------|----------------|
| **bundler** | Universal | ~95% | Dependency management (was third-party until Ruby 2.6) |
| **rails** | Dominant | ~60% (of web) | Full web framework |
| **rspec** | High | ~65% (of tested) | Testing (Minitest exists but rspec preferred) |
| **rubocop** | High | ~55% | Linting and formatting |
| **nokogiri** | High | ~40% | HTML/XML parsing (REXML is too slow) |
| **pry** | High | ~50% | Better REPL and debugger |
| **httparty / faraday** | Medium | ~35% | HTTP client (Net::HTTP is verbose) |
| **dotenv** | Medium | ~40% | Load .env files |
| **sidekiq** | Medium | ~30% | Background job processing |
| **devise** | Medium | ~25% (of web) | Authentication |

**What Ruby reveals:**

1. **Net::HTTP is the new urllib.** Ruby has a built-in HTTP client, but it is so verbose
   that `httparty` and `faraday` exist to wrap it. The same pattern as Python's
   `urllib` -> `requests`. A bad built-in HTTP client is worse than none.

2. **Bundler was absorbed.** Bundler started as a third-party gem and was eventually merged
   into Ruby's stdlib (Ruby 2.6). This is the clearest possible signal: when a package is
   installed in 95% of projects, it belongs in the language. The same happened with `pip`
   being bundled with Python.

3. **Nokogiri vs REXML.** Ruby has a built-in XML parser (REXML), but it is too slow for
   real-world use. Nokogiri (a C-binding wrapper) replaced it entirely. The lesson for ilo:
   if a builtin cannot handle production workloads, it is worse than not having one at all.
   ilo's approach -- delegating parsing to tools -- avoids this trap entirely.

---

### Go

Go is the most relevant comparison for ilo because of its shared design philosophy:
simplicity, one way to do things, explicit error handling.

| Package | Stars/Usage | % of projects | Capability gap |
|---------|------------|---------------|----------------|
| **gorilla/mux** or **chi** | 20k+ / 17k+ | ~50% (pre-1.22) | HTTP routing (stdlib ServeMux was too basic) |
| **testify** | 22k+ | ~55% | Testing assertions (stdlib has none) |
| **cobra** | 36k+ | ~40% | CLI framework |
| **viper** | 26k+ | ~35% | Configuration management |
| **zap / zerolog** | 21k+ / 10k+ | ~45% | Structured logging (stdlib log is basic) |
| **sqlx** | 15k+ | ~30% | Enhanced database driver |
| **godotenv** | 7k+ | ~25% | Load .env files |
| **uuid** | 5k+ | ~30% | UUID generation |
| **go-playground/validator** | 16k+ | ~30% | Struct validation |

**What Go reveals -- and what Go 1.22 fixed:**

1. **ServeMux was the biggest gap.** Go shipped with an HTTP server and router, but
   `http.ServeMux` was so basic (no path parameters, no method matching) that gorilla/mux
   and chi became universal. **Go 1.22 (Feb 2024) finally fixed this** by adding method
   and path parameter support to `ServeMux`. The 12-year gap is the strongest possible
   evidence that stdlib gaps persist for years even in languages with strong stdlib
   philosophies.

2. **No testing assertions.** Go's `testing` package provides the test runner but no
   assertion helpers. `if got != want { t.Errorf(...) }` is the pattern, repeated hundreds
   of times per project. `testify` provides `assert.Equal(t, got, want)` -- saving 2-3
   lines per assertion. Go's designers argued that assertions hide information. The
   community disagreed by a margin of 55%.

3. **Structured logging took 15 years.** Go launched in 2009 with `log.Printf`. The
   `slog` package (structured logging) was not added to the stdlib until Go 1.21 (Aug
   2023). For 14 years, every Go project that needed JSON logs -- which is every production
   Go service -- used zap, zerolog, or logrus. The lesson: logging format matters for
   machine consumption, and text-only logging is insufficient.

4. **godotenv exists in Go too.** Even in a language with a strong "just use the stdlib"
   culture, .env loading is a universal need.

---

### Rust

Rust has the most extreme version of this phenomenon: the language is deliberately minimal,
and the crate ecosystem fills enormous gaps.

| Crate | Downloads | % of projects | Capability gap |
|-------|----------|---------------|----------------|
| **serde / serde_json** | 350M+ | ~75% | Serialization (no built-in) |
| **tokio** | 250M+ | ~60% | Async runtime (language has async/await but no runtime) |
| **clap** | 150M+ | ~45% | CLI argument parsing |
| **reqwest** | 130M+ | ~40% | HTTP client (no built-in) |
| **anyhow / thiserror** | 120M+ | ~50% | Ergonomic error handling |
| **tracing** | 100M+ | ~40% | Structured logging/tracing |
| **rand** | 100M+ | ~40% | Random number generation |
| **regex** | 80M+ | ~35% | Regular expressions |
| **chrono / time** | 70M+ | ~35% | Date/time handling |
| **uuid** | 50M+ | ~25% | UUID generation |
| **dotenv / dotenvy** | 30M+ | ~20% | Load .env files |

**What Rust reveals:**

1. **serde is the language.** When 75% of Rust projects install the same crate, it is no
   longer optional -- it is the language's serialization model. serde's derive macro
   (`#[derive(Serialize, Deserialize)]`) is so universal that Rust programs are practically
   unwriteable without it. The lesson: serialization at data boundaries is not optional.
   ilo handles this at the tool boundary via `Value <-> JSON` mapping (D1e).

2. **Rust shipped async without a runtime.** Rust added `async`/`await` to the language in
   1.39 (2019) but deliberately excluded the async runtime. tokio fills this gap for 60%
   of Rust projects. The decision was intentional (no runtime in the language) but the
   consequence is that "Rust async" effectively means "Rust + tokio." ilo avoids this by
   keeping execution fully synchronous at the language level, with parallelism handled by
   the runtime.

3. **anyhow + thiserror: error ergonomics matter.** Rust's `Result<T, E>` is the model ilo
   followed, but even Rust developers found raw `Result` too verbose. `anyhow` provides
   `anyhow::Result` (any error type) and `context("message")` for error wrapping.
   `thiserror` provides derive macros for custom error types. ilo's `!` auto-unwrap and
   `^+"context: "e` pattern already provide these ergonomics natively.

4. **No built-in random numbers.** Rust has no `rand` in the stdlib. This is a deliberate
   choice (cryptographic randomness vs pseudo-randomness is a footgun), but it means every
   project that needs a random number must pull in a crate. For ilo, random number
   generation is an agent need (generating IDs, sampling) that should be a builtin.

5. **No built-in regex.** Rust delegates regex to a crate. The `regex` crate is high-quality
   (no backtracking, guaranteed linear time), but every text-processing project needs to
   add it. For ilo, regex is a question of builtin vs tool -- the Python and Ruby research
   suggest builtin behind a feature flag.

---

### PHP

PHP is instructive because it started as a "do everything" language with 8,000+ built-in
functions, yet still has essential packages.

| Package | Installs | % of projects | Capability gap |
|---------|---------|---------------|----------------|
| **laravel / symfony** | Dominant | ~70% (of web) | Web framework (raw PHP is tedious) |
| **guzzle** | Very high | ~55% | HTTP client (file_get_contents lacks features) |
| **phpunit** | Very high | ~65% (of tested) | Testing framework |
| **monolog** | Very high | ~50% | Structured logging (no built-in) |
| **carbon** | High | ~40% | Date/time manipulation (DateTime is limited) |
| **vlucas/phpdotenv** | High | ~40% | Load .env files |
| **composer** | Universal | ~95% | Dependency management (was third-party) |

**What PHP reveals:**

1. **8,000 built-in functions were not enough.** PHP has `file_get_contents()`,
   `json_decode()`, `preg_match()`, `date()`, and thousands more -- yet guzzle, monolog,
   carbon, and dotenv are still essential. The lesson: quantity of builtins does not equal
   coverage. What matters is whether the builtins solve the actual workflow patterns that
   developers encounter. PHP's `file_get_contents()` can fetch URLs, but it cannot set
   headers, handle redirects properly, or parse response codes -- so guzzle exists.

2. **Composer, like Bundler, was absorbed.** PHP's dependency manager started as a
   third-party tool and became the de facto standard. Both Ruby and PHP's ecosystems
   independently converged on the same pattern: the package manager is too important to be
   a package.

---

## Universal Patterns Across All Languages

The catalog above reveals patterns that repeat across every language, regardless of design
philosophy, age, or domain. These are not language-specific gaps -- they are capabilities
that all programmers need and that no language designer anticipated (or chose to include).

### Pattern 1: HTTP Client

**Every language needs a third-party HTTP client.**

| Language | Stdlib HTTP | Essential package | Gap |
|----------|-----------|-------------------|-----|
| Python | urllib (painful) | requests, httpx | Usable API |
| JavaScript | fetch (added late) | axios, node-fetch | Was missing for 13 years |
| Ruby | Net::HTTP (verbose) | httparty, faraday | Ergonomic API |
| Go | net/http (good!) | -- | Go is the exception |
| Rust | None | reqwest | Entire capability |
| PHP | file_get_contents (limited) | guzzle | Headers, redirects, errors |

**Why designers missed it:** Languages were designed before the API economy. HTTP was a
protocol for browsers, not a universal application integration layer. By the time every
program needed to call APIs, the stdlib was frozen.

**ilo's answer:** `get url` / `$url` is already a builtin (D1b). `post url body` is
planned. HTTP is not an afterthought in ilo -- it is a first-class primitive. This is the
correct design for a language born in the API era.

### Pattern 2: Environment Configuration

**Every language needs a dotenv package.**

| Language | Stdlib env access | Essential package |
|----------|-----------------|-------------------|
| Python | os.getenv() | python-dotenv |
| JavaScript | process.env | dotenv |
| Ruby | ENV[] | dotenv |
| Go | os.Getenv() | godotenv |
| Rust | std::env::var() | dotenv/dotenvy |
| PHP | $_ENV | vlucas/phpdotenv |

**Why designers missed it:** Environment variable access was always in the stdlib. What
was missing was the *workflow*: developers need local `.env` files for development secrets,
but environment variables in production. The dotenv pattern bridges this gap. No language
anticipated this workflow because it emerged from 12-factor app methodology (2011).

**ilo's answer:** `env key` is planned (I3) as a builtin returning `R t t`. For agents,
the dotenv workflow is less relevant (agents receive configuration from their orchestrator,
not from `.env` files), but env var access is critical for API keys and runtime
configuration.

### Pattern 3: Data Validation at Boundaries

**Every language needs a runtime validation library.**

| Language | Stdlib validation | Essential package |
|----------|-----------------|-------------------|
| Python | None | pydantic |
| JavaScript | None | zod, joi, yup |
| Ruby | None (ActiveModel in Rails) | dry-validation |
| Go | None | go-playground/validator |
| Rust | None (serde handles shape) | validator |
| PHP | None | respect/validation |

**Why designers missed it:** Type systems check types at compile time (or not at all in
dynamic languages). But the boundary between your code and external data (API responses,
user input, config files) is where types break down. Data arrives as untyped JSON/text and
must be validated against expected shapes. No language provides this at the stdlib level.

**ilo's answer:** ilo's tool declarations ARE the validation schema. When a tool declares
`>R profile t`, the runtime validates the JSON response against the `profile` record
definition. The type system extends to the boundary. This is what pydantic and zod do, but
built into the language.

### Pattern 4: Code Formatting

**Every language eventually needs an official formatter.**

| Language | Built-in formatter | Essential package | Year of resolution |
|----------|-------------------|-------------------|-------------------|
| Python | None | black, autopep8, yapf | black (2018) -- still third-party |
| JavaScript | None | prettier | 2017 -- still third-party |
| Ruby | None | rubocop | Still third-party |
| Go | **gofmt** (built-in) | -- | 2009 (day one) |
| Rust | **rustfmt** (official) | -- | 2015 (early ecosystem) |
| PHP | None (PSR standards) | php-cs-fixer | Still third-party |

**Why designers missed it:** Early language designers viewed formatting as a matter of
taste. Go proved them wrong: `gofmt` ships with Go, is non-configurable, and eliminates
all formatting debates. The result: every Go file looks the same. Rust followed with
`rustfmt`. Python and JavaScript still have competing formatters.

**ilo's answer:** Dense format is canonical. There is exactly one representation.
`dense(parse(dense(parse(src)))) == dense(parse(src))`. Formatting debates are impossible
because there are no formatting choices. ilo solved this at the specification level, which
is even more fundamental than Go's `gofmt`.

### Pattern 5: Testing Framework

**Every language's built-in testing is insufficient.**

| Language | Stdlib testing | Essential package |
|----------|--------------|-------------------|
| Python | unittest | pytest |
| JavaScript | None (node:test added 2022) | jest, vitest, mocha |
| Ruby | Minitest | rspec |
| Go | testing (no assertions) | testify |
| Rust | #[test] (basic) | -- (Rust's is unusually good) |
| PHP | None | phpunit |

**Why designers missed it:** Language designers build testing as a way to run code and
check results. Developers want testing as a way to express intent, with rich assertions,
fixtures, mocking, and parallel execution. The gap is between "mechanism" and "workflow."

**ilo's relevance:** Testing is not applicable to ilo in the traditional sense. ilo programs
are verified before execution -- the verifier IS the test framework. Type checking,
exhaustiveness checking, call resolution, and arity checking catch the errors that testing
frameworks catch in other languages. The remaining testing need (does the program produce
correct output?) is an orchestration concern, not a language concern.

### Pattern 6: Structured Logging

**Every language needs structured logging.**

| Language | Stdlib logging | Essential package |
|----------|--------------|-------------------|
| Python | logging (old-style) | structlog, loguru |
| JavaScript | console.log (unstructured) | pino, winston |
| Ruby | Logger (basic) | rails logger, semantic_logger |
| Go | log (text only, until 1.21) | zap, zerolog, logrus |
| Rust | log (facade only) | tracing, env_logger |
| PHP | None | monolog |

**Why designers missed it:** Early languages logged to console/files for humans. Modern
systems need machine-readable logs (JSON) for aggregation, search, and alerting. The
"structured logging" pattern (key-value pairs instead of format strings) did not emerge
until the DevOps era.

**ilo's relevance:** ilo programs are short-lived tool orchestration scripts, not
long-running services. Traditional logging is not a concern. If logging is needed, it is a
tool concern (`tool log"Write log entry" msg:t level:t>R _ t`), not a language concern.

### Pattern 7: CLI Argument Parsing

**Every language needs a better CLI parser.**

| Language | Stdlib CLI | Essential package |
|----------|----------|-------------------|
| Python | argparse (verbose) | click, typer |
| JavaScript | process.argv (raw) | commander, yargs |
| Ruby | OptionParser (adequate) | thor, optimist |
| Go | flag (basic) | cobra, pflag |
| Rust | std::env::args (raw) | clap, structopt |
| PHP | $argv (raw) | symfony/console |

**Why designers missed it:** Early languages treated CLI arguments as a simple array.
Modern CLI tools need subcommands, flags with types, help text generation, auto-completion,
and validation. The gap between `argv` and a real CLI experience is enormous.

**ilo's answer:** This pattern is irrelevant for ilo. ilo programs are generated by agents
for specific tasks. They do not need self-documenting CLI interfaces. Function parameters
ARE the CLI interface: `ilo 'f x:n y:n>n;+x y' 3 4` maps positional args to typed params.
No parsing library needed.

---

## The Capability Matrix

Combining all languages, here is the universal set of capabilities that "essential packages"
provide, organized by how ilo should handle them:

### Already in ilo (builtins)

| Capability | Languages that need a package | ilo builtin |
|-----------|------------------------------|-------------|
| HTTP GET | Python, JS, Ruby, Rust, PHP | `get url` / `$url` |
| String split/join | (Most have stdlib) | `spl` / `cat` |
| String contains | (Most have stdlib) | `has` |
| List operations | (Most have stdlib) | `@`, `+=`, `+`, `len`, `hd`, `tl`, `rev`, `srt`, `slc` |
| Type conversions | (Most have stdlib) | `str`, `num` |
| Math basics | (Most have stdlib) | `+`, `-`, `*`, `/`, `abs`, `min`, `max`, `flr`, `cel` |
| Error handling | Python (try/except), JS (try/catch), Rust (anyhow) | `R`, `?`, `!`, `^`, `~` |
| Code formatting | Python (black), JS (prettier), Ruby (rubocop) | Dense format (canonical) |
| Data validation | Python (pydantic), JS (zod), Go (validator) | Type verifier + tool boundaries |

### Planned for ilo (roadmap items)

| Capability | Languages that need a package | ilo plan | Phase |
|-----------|------------------------------|----------|-------|
| HTTP POST | Python (requests), JS (axios), PHP (guzzle) | `post url body` | G1 |
| Env vars | ALL languages (dotenv) | `env key` | I3 |
| JSON parse | Python (json), Rust (serde_json) | `jp`/`jsn` | I1 |
| Shell execution | Ruby (backticks), PHP (shell_exec) | `sh cmd` or `run cmd` | I2 |
| String replace | All languages need it | `sub`/`rpl` | I8 |
| Trim whitespace | All languages need it | `trm` | I8 |
| Regex | Ruby, Python, PHP (built-in); Rust, Go (crates/packages) | `rgx`/`rga`/`rgs` | I8 |
| Base64 encode/decode | JS (btoa/atob), Python, Rust | `b64e`/`b64d` | I7 |
| URL encode/decode | All languages need it | `urle`/`urld` | I7 |
| SHA-256 hash | Python (hashlib), Rust (no stdlib) | `sha` / `hash` | I9 |
| HMAC | Python (hmac), Go (crypto/hmac) | `hmac` | I9 |
| UUID generation | Rust (uuid), Go (uuid), Python (uuid) | `uid()` | I9 |
| Timestamp | All languages | `now()` | I6 |
| Sleep/delay | All languages | `slp n` | I6 |
| ISO date format | All languages (chrono/carbon/moment) | `iso n` | I6 |
| Maps/dicts | (Most have stdlib but ilo lacks) | `M t v` | E4 |
| Random numbers | Rust (rand), Python (random) | `rnd()` | I9 |
| File read/write | All languages | `rd`/`wr` or `fread`/`fwrite` | G8 |
| Parallel execution | Python (asyncio), JS (Promise.all), Rust (tokio) | `par{...}` | G4 |

### Tool concerns (not builtins)

| Capability | Why it is a tool, not a builtin |
|-----------|-------------------------------|
| Web frameworks (express, flask, rails) | Agents consume APIs, they do not serve them |
| Database drivers (sqlx, prisma) | Domain-specific, requires connection management |
| HTML/XML parsing (nokogiri, cheerio) | Format-specific parsing is a tool concern |
| CSV parsing | Format-specific |
| YAML/TOML parsing | Format-specific |
| Email sending | External service with complex config |
| Background jobs (sidekiq, celery) | Long-running service infrastructure |
| Authentication (devise, passport) | Domain-specific, not composable |
| Image processing | Domain-specific computation |
| Machine learning | Heavy computation, tool territory |

---

## The Critical Insight for ilo

### Why this analysis matters more for ilo than for any other language

In Python, a developer who needs HTTP can `pip install requests` in 2 seconds. In
JavaScript, `npm install axios` takes 1 second. The "essential package" problem is an
inconvenience in traditional languages -- you install the package and move on.

**In ilo, the essential package problem is a hard wall.**

An ilo program is self-contained. An agent generating an ilo program cannot install
packages. It cannot reach out to npm or PyPI. Whatever capability the program needs must
come from one of two sources:

1. **Builtins** -- compiled into the ilo runtime, always available.
2. **Declared tools** -- external services that the program's environment provides.

There is no third option. No package manager, no import system, no dynamic loading.

This means every gap in the "essential packages" list above that is not covered by a
builtin or a commonly-available tool is a capability that ilo programs **cannot access at
all**. Not "inconvenient to access" -- literally impossible.

### The builtin vs tool boundary

The analysis above reveals a natural boundary:

**Builtins** should cover capabilities that are:
- Universal (needed by >50% of programs regardless of domain)
- Stateless or nearly stateless (no connection management, no sessions)
- Fast (completes in milliseconds to seconds)
- Token-efficient as builtins (1-3 tokens vs 5-10 tokens as tool declarations + calls)

**Tools** should cover capabilities that are:
- Domain-specific (databases, specific APIs, specific file formats)
- Stateful (connection pools, sessions, transactions)
- Slow (minutes-long operations, human-in-the-loop)
- Configurable (endpoints, credentials, retry policies)

Applying this boundary to the essential packages:

| Essential capability | Builtin or tool? | Rationale |
|---------------------|-----------------|-----------|
| HTTP GET/POST | **Builtin** | Universal, stateless, 2-3 tokens |
| Env vars | **Builtin** | Universal, stateless, 2 tokens |
| JSON parse/serialize | **Builtin** | Universal at tool boundary, 2 tokens |
| Shell execution | **Builtin (with feature flag)** | Universal for local agents, stateless |
| String manipulation (replace, trim, case) | **Builtin** | Stateless, 2-3 tokens |
| Regex | **Builtin (with feature flag)** | Stateless, 3 tokens |
| Base64/URL encoding | **Builtin** | Stateless, 2 tokens |
| Hash/HMAC | **Builtin** | Stateless, 2-3 tokens |
| UUID/random | **Builtin** | Stateless, 1-2 tokens |
| Timestamp/sleep | **Builtin** | Stateless, 1-2 tokens |
| File read/write | **Builtin (with feature flag)** | Nearly universal, but has side effects |
| Parallel execution | **Builtin (structural)** | `par{...}` block, not a library |
| Web framework | **Tool** | Agents are clients, not servers |
| Database | **Tool** | Stateful, domain-specific |
| Format parsing (XML, CSV, YAML) | **Tool** | Domain-specific |

### The numbers: tokens saved by builtins vs tools

Consider fetching a user from an API and extracting their email:

**As builtins (current + planned):**
```
k=env! "API_KEY";u=$!(+"https://api.com/users/123?key=" k);jp! u "email"
```
~20 tokens. Everything is built in. The agent generates one line.

**As tools (if builtins did not exist):**
```
tool env"Read env var" key:t>R t t
tool http-get"HTTP GET" url:t>R t t
tool json-path"Extract JSON field" data:t path:t>R t t
k=env! "API_KEY";body=http-get! +"https://api.com/users/123?key=" k;jp=json-path! body "email";jp
```
~55 tokens. Three tool declarations + three calls. The agent must generate significantly
more code, and every additional token is a chance for error.

**The token tax of tools:** Every tool declaration costs ~10-15 tokens (name, description,
params, return type, options). A program that needs 5 common capabilities (HTTP, env, JSON,
string replace, hash) would spend 50-75 tokens just declaring tools before writing any
logic. With builtins, those tokens are zero.

This is why the essential packages analysis matters for ilo: **builtins eliminate the token
tax that tools impose on universal capabilities.**

---

## Gap Analysis: What ilo Needs Today

Based on the universal patterns identified above, here is the prioritized list of
capabilities that ilo should provide as builtins, ordered by frequency of need across the
essential package data:

### Tier 1: Critical (blocks real-world programs without them)

| Builtin | Signature | Justification | Status |
|---------|-----------|---------------|--------|
| `env key` | `>R t t` | 6/6 languages need dotenv; API keys are universal | Planned (I3) |
| `post url body` | `>R t t` | 6/6 languages need HTTP POST; every API write | Planned (G1) |
| `jp text path` | `>R t t` | JSON is the API interchange format; serde=75% of Rust | Planned (I1) |
| `sh cmd` | `>R t t` | Ruby/PHP prove shell exec is universal for agents | Planned (I2) |

These four builtins, combined with the existing `get`/`$`, would enable an ilo program to:
- Read configuration from the environment
- Call any HTTP API (GET and POST)
- Parse JSON responses
- Execute shell commands for anything else

This is the minimum viable set for an agent that can do real work.

### Tier 2: High value (saves significant tokens across many programs)

| Builtin | Signature | Justification | Status |
|---------|-----------|---------------|--------|
| `rpl t old new` | `>t` | String replace is universal; building URLs, messages | Planned (I8) |
| `trm t` | `>t` | Trim whitespace; cleaning API responses | Planned (I8) |
| `now()` | `>n` | Timestamps; every language needs time.time() | Planned (I6) |
| `slp n` | `>_` | Rate limiting, polling; universal in API work | Planned (I6) |
| `uid()` | `>t` | UUID generation; 4/6 languages need a package | Planned (I9) |
| `b64e t` / `b64d t` | `>t` / `>R t t` | Auth headers, data encoding | Planned (I7) |
| `urle t` / `urld t` | `>t` / `>t` | Building query strings; every API call | Planned (I7) |

### Tier 3: Medium value (needed frequently but workarounds exist)

| Builtin | Signature | Justification | Status |
|---------|-----------|---------------|--------|
| `sha t` | `>t` | Hash for cache keys, integrity checks, webhook verification | Planned (I9) |
| `hmac key msg` | `>t` | Webhook signatures (GitHub, Stripe, AWS) | Planned (I9) |
| `rnd()` / `rnd a b` | `>n` | Random numbers; Rust's most-installed capability gap | Planned (I9) |
| `rgx pat text` | `>R t t` | Regex match; text extraction from unstructured data | Planned (I8) |
| `rgs pat repl text` | `>t` | Regex replace | Planned (I8) |
| `upr t` / `lwr t` | `>t` | Case conversion; API normalization | Considered |
| `iso n` | `>t` | Timestamp to ISO 8601; API date format | Planned (I6) |
| `ts text` | `>R n t` | Parse ISO 8601 to timestamp | Planned (I6) |
| `rd path` | `>R t t` | Read file; universal for local agents | Planned (G8) |
| `wr path data` | `>R _ t` | Write file | Planned (G8) |

### Tier 4: Low value for agents (tool concern or rare)

| Capability | Why not a builtin |
|-----------|-------------------|
| XML/HTML parsing | Tool concern; domain-specific |
| CSV parsing | Tool concern; `spl` handles simple cases |
| YAML/TOML parsing | Tool concern; config is an infra concern |
| Date arithmetic | Tool concern; complex, locale-dependent |
| Timezone conversion | Tool concern; agents work in UTC |
| Encryption (AES, RSA) | Tool concern; dangerous to expose, requires key management |
| Compression (gzip, zlib) | Tool concern; rare in agent workflows |
| Database access | Tool concern; stateful, domain-specific |
| WebSocket | Planned as feature flag (G2); not universal |
| HTTP server | Agents are clients, not servers |
| Image processing | Tool concern; heavy computation |

---

## Comparison with Existing ilo Research

This analysis aligns with and reinforces the conclusions from ilo's existing research:

### go-stdlib-research.md alignment

The Go research recommended `fread`/`fwrite` (G8), `env` (I3), `hash`/`hmac` (I9),
`now()`/`sleep` (I6), and `par{...}` (G4) -- all of which appear in the essential packages
analysis above. The Go research also recommended against file path manipulation,
encryption, HTTP server, and XML parsing -- all confirmed as tool concerns by the
cross-language analysis.

The Go research's strongest insight -- that `if err != nil` boilerplate is eliminated by
`!` auto-unwrap -- is validated by Rust's anyhow/thiserror being in the essential packages
list. Two languages independently proved that error handling ergonomics matter enough to
justify a package/crate. ilo solved this at the language level.

### python-stdlib-analysis.md alignment

The Python research recommended `sh cmd` (I2), `post url body` (G1), `env key` (I3),
`rpl` (replace), `trm` (trim), and `%` (modulo) as Tier 1 additions. The essential
packages analysis confirms all of these: requests/httpx (HTTP), python-dotenv (env), and
pydantic (validation) are the top Python packages, matching the recommended builtins.

The Python research's classification of builtins vs tools (file I/O as tool, HTTP as
builtin, JSON as builtin, formatting as builtin, encryption as tool) is consistent with the
cross-language evidence.

### js-ts-ecosystem.md alignment

The JS/TS research recommended `post`, `read`/`write` (file I/O), `sh` (shell), and `env`
as Tier 1 gaps -- identical to the essential packages analysis. The JS research also
identified Deno's permission model as relevant to ilo's tool-declaration-as-permission
approach, and Bun's `$` shell syntax as inspiration for `sh`.

### ruby-php-research.md alignment

The Ruby/PHP research's strongest recommendation was `sh` (shell execution) inspired by
Ruby's backtick syntax, and `env` (environment access). Both are confirmed as universal
needs by the cross-language analysis. The Ruby research's insight that blocks/closures are
not needed (ilo's `@` + guards suffice) is validated by the observation that no essential
package in any language provides "better lambdas" -- the need is for capabilities, not
abstractions.

### swift-kotlin-research.md alignment

The Swift/Kotlin research's main recommendation -- runtime-level structured concurrency with
zero syntax cost -- is supported by the observation that tokio (60% of Rust projects) and
asyncio-related packages are among the most-installed. The research's recommendation to
defer generics, protocols, and extension functions is validated by the absence of "better
type system" packages in any language's essential list. Developers need capabilities (HTTP,
JSON, env), not abstractions.

---

## What Languages Got Right (and ilo should copy)

### Go: batteries-included stdlib

Go's stdlib includes HTTP server/client, JSON, crypto, testing, and more. The result: Go
has fewer essential third-party packages than any other language in this analysis. The
lesson for ilo: more builtins = fewer gaps. Go's stdlib philosophy ("a little copying is
better than a little dependency") maps directly to ilo's constraint (agents cannot install
dependencies).

### Rust: serde's "derive the schema" pattern

serde's `#[derive(Serialize, Deserialize)]` turns struct definitions into serialization
schemas. ilo's `type` declarations serve the same role at tool boundaries. This pattern --
type definitions as data schemas -- is the right answer to the validation problem that
pydantic/zod/validator all solve differently.

### Ruby: shell execution as a language primitive

Ruby's backtick syntax (`\`ls -la\``) proves that shell execution can be a language-level
feature, not just a library function. For agents, this is perhaps the single most powerful
capability: the ability to run arbitrary system commands. ilo's planned `sh cmd` follows
this model.

### PHP: URLs are just resources

PHP's `file_get_contents("https://...")` treats URLs the same as file paths. ilo's `get url`
follows the same philosophy: fetching data from a URL is as natural as any other operation.

### Swift/Kotlin: structured concurrency

Both languages prove that parallelism can be structured (scoped, cancellable, no leaked
tasks) without exposing threads or channels. ilo's planned `par{...}` block and the runtime
dependency graph analysis follow this approach.

---

## What Languages Got Wrong (and ilo should avoid)

### Python: too many ways to do the same thing

Python has `urllib`, `urllib2`, `urllib3`, `http.client`, AND `requests`. Three formatting
syntaxes (`%`, `.format()`, f-strings). Two path libraries (`os.path`, `pathlib`). This
violates ilo's principle of "one way to do things."

### JavaScript: no batteries at all

Node.js launched with almost nothing. No HTTP client (until 2022), no testing (until 2022),
no formatting, no type checking. The result: an explosion of packages where projects spend
more tokens configuring dependencies than writing logic. ilo should be the opposite:
everything a program commonly needs is built in.

### Rust: too minimal at the stdlib level

Rust's deliberate minimalism (no serialization, no async runtime, no random, no regex)
means that 75% of projects install serde, 60% install tokio, and 40% install each of reqwest,
rand, and regex. The "keep the stdlib small" philosophy is principled but imposes real costs.
For ilo, where installing dependencies is impossible, minimalism at the builtin level would
be fatal.

### Go: ignoring the community for too long

Go's ServeMux was inadequate for 12 years. Go's `log` package produced only text for 14
years. Go's testing package still has no assertions. In each case, the community built
packages that Go eventually had to semi-adopt (ServeMux was fixed in 1.22, slog was added
in 1.21). The lesson: if the community universally reaches for a package, the language
should absorb the capability.

### PHP: including everything without quality control

PHP's 8,000+ built-in functions include inconsistent naming (`str_replace` vs `strpos` vs
`substr`), inconsistent argument order (`array_map(fn, arr)` vs `array_filter(arr, fn)`),
and inconsistent error handling (some return false, some throw, some set error codes). The
lesson: more builtins are better, but they must be consistent. ilo's builtin naming
convention (3-char names, `R` return types, consistent argument order) avoids this trap.

---

## Conclusion: The Essential Builtin Set for ilo

The essential packages phenomenon, analyzed across six major languages, reveals a universal
set of ~20 capabilities that developers need regardless of language or domain. For ilo, where
agents cannot install packages, these capabilities must be builtins.

The current ilo builtin set covers math, string basics, list operations, HTTP GET, and
error handling. The roadmap correctly identifies the remaining essential capabilities (env,
HTTP POST, JSON, shell execution, regex, encoding, crypto, time, file I/O).

The priority, derived from cross-language frequency data:

```
Critical (every agent needs):    env, post, jp/jsn, sh
High value (most agents need):   rpl, trm, now, slp, uid, b64e/b64d, urle/urld
Medium value (many agents need): sha, hmac, rnd, rgx, rgs, upr/lwr, iso, ts, rd/wr
```

ilo's advantage over every language analyzed: the verification layer. Where Python needs
pydantic, JavaScript needs zod, and Go needs go-playground/validator, ilo's type system +
tool boundaries provide validation without a single extra token. Where Python needs black,
JavaScript needs prettier, and Ruby needs rubocop, ilo's canonical dense format provides
formatting without any tooling at all. Where Rust needs anyhow and thiserror, ilo's `!`
auto-unwrap and `^` error wrapping provide error ergonomics natively.

The essential packages of other languages are not just a list of things to build. They are
evidence that ilo's core design -- verified types, canonical formatting, typed error
handling, builtins over packages -- is the right approach for a language where the
programmer cannot install dependencies.

---

## See Also

- [universal-stdlib-gaps.md](universal-stdlib-gaps.md) — complementary analysis: same phenomenon from the "what stdlibs get wrong" angle
- [OPEN.md](OPEN.md) — builtin naming decisions and "essential packages" principle
- Language-specific research files for per-ecosystem detail: [go-stdlib-research.md](go-stdlib-research.md), [python-stdlib-analysis.md](python-stdlib-analysis.md), [js-ts-ecosystem.md](js-ts-ecosystem.md), [ruby-php-research.md](ruby-php-research.md), [rust-capabilities-research.md](rust-capabilities-research.md)
