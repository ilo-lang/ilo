// =============================================================================
// Stable error-code namespace
// =============================================================================
//
// Every diagnostic ilo emits has the shape `ILO-<letter><digits>`. The letter
// is the **namespace** — it tells an agent or tool which compiler phase raised
// the error without reading the message. Numeric ranges are reserved per
// namespace, with generous gaps so future codes slot in cleanly:
//
// | Range          | Letter | Area                          | Status            |
// |----------------|--------|-------------------------------|-------------------|
// | ILO-L000-099   | L      | Lexer / tokenisation          | active            |
// | ILO-P100-199   | P      | Parser / syntax               | active            |
// | ILO-N200-299   | N      | Names / resolution            | reserved          |
// | ILO-I300-399   | I      | Imports                       | reserved          |
// | ILO-T400-499   | T      | Types                         | active            |
// | ILO-V500-599   | V      | Verifier (post-type checks)   | reserved          |
// | ILO-R600-699   | R      | Runtime                       | active            |
// | ILO-D700-799   | D      | Deprecation warnings          | reserved          |
// | ILO-E800-899   | E      | Engine-specific limitations   | reserved          |
// | ILO-S900-999   | S      | Skill / spec system           | reserved          |
//
// ## Stability contract
//
// 1. **Existing codes never change.** ilo shipped with a flat numbering scheme
//    (`ILO-L001`, `ILO-P001`, `ILO-T001`, `ILO-R001`, `ILO-W001`) where each
//    namespace started at 001. Those codes remain valid forever — agents and
//    pinned tool configs depend on them. The new range scheme is **additive**:
//    new codes allocated from now on land in the documented hundreds-block of
//    their namespace; historical codes stay where they are.
//
// 2. **New codes go in the right namespace, not the next free slot.** A new
//    parser error gets `ILO-P101` (or the next free P1xx), not `ILO-P022`.
//    A new name-resolution error gets `ILO-N201`, not `ILO-T037`. This is
//    enforced by the cross-engine regression test in `tests/error_codes.rs`,
//    which asserts every code emitted by the compiler lives in its
//    documented range.
//
// 3. **Each namespace has reserved space.** 100 codes per namespace is far
//    more than ilo will ever need at the current pace of growth; the
//    spacing is intentional so adding new categories never forces a
//    renumber.
//
// 4. **Deprecation warnings are issued via `ILO-D###` codes.** They emit at
//    compile time, are surfaced as warnings (not errors), and do not fail
//    the build. The range is reserved but currently empty — the first
//    `ILO-D###` ships with the first feature deprecation.
//
// ## Namespace definitions
//
// - **L (Lexer)** — character-class issues, malformed literals, identifier-
//   shape violations. Raised before a syntax tree exists.
// - **P (Parser)** — token-stream errors: missing tokens, unexpected tokens,
//   incomplete declarations. Raised after the lexer succeeds.
// - **N (Names)** — name resolution failures: unknown identifier, duplicate
//   definitions, shadowing of reserved names. Currently emitted under the
//   T-prefix for historical reasons; the N-range is reserved for the eventual
//   split.
// - **I (Imports)** — `use` resolution: missing file, IO error, cycle. P017
//   (use-import failed) is the historical home; new import diagnostics get
//   `ILO-I3xx`.
// - **T (Types)** — type-checking errors: mismatched types, arity, generic
//   instantiation, record/variant shape.
// - **V (Verifier)** — post-type checks that aren't strictly type errors:
//   exhaustiveness, reachability, register caps, mutation-shape warnings.
//   T029 (unreachable), T032/T033 (discarded mutation), T035/T036 (register
//   caps) are historical T-residents; new verifier diagnostics get
//   `ILO-V5xx`.
// - **R (Runtime)** — failures observed at execution time: division by zero,
//   index out of bounds, panic-unwrap, builtin failures.
// - **D (Deprecation)** — compile-time warnings for features scheduled for
//   removal. Forward-looking; no codes allocated yet.
// - **E (Engine-specific)** — limitations of a particular execution engine
//   (VM register caps surfaced ahead of time, JIT-unsupported opcode, AOT
//   constraint). Forward-looking; no codes allocated yet.
// - **S (Skill / spec)** — skill-bundle loader errors, manifest issues,
//   spec-link breaks. Forward-looking; no codes allocated yet.

/// The compiler phase that emits a diagnostic.
///
/// This is the provenance field: it tells tooling (and humans) which stage of
/// compilation raised the error without having to parse the error code letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Lexer / tokenisation (ILO-L###)
    Lex,
    /// Parser / syntax analysis (ILO-P###)
    Parse,
    /// Type-checker / verifier (ILO-T###, ILO-V###)
    Verify,
    /// Runtime interpreter, VM, or JIT (ILO-R###)
    Runtime,
    /// Engine-specific compilation limitation (ILO-E###)
    Engine,
}

impl Phase {
    /// Canonical lowercase string, matches the `phase` key used in JSON output.
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Lex => "lex",
            Phase::Parse => "parse",
            Phase::Verify => "verify",
            Phase::Runtime => "runtime",
            Phase::Engine => "engine",
        }
    }
}

/// An entry in the error code registry.
#[allow(dead_code)] // fields are used by tooling / --explain / golden tests
pub struct ErrorEntry {
    pub code: &'static str,
    pub phase: Phase,        // which compiler phase emits this code
    pub short: &'static str, // brief description for tooling / --list-errors
    pub long: &'static str,  // full explanation for --explain
}

/// Documented namespace ranges. Used by the cross-engine regression test in
/// `tests/error_codes.rs` to assert every emitted code lands in its range.
///
/// Each entry is `(letter, lo, hi)`. `lo` and `hi` are inclusive. Codes outside
/// any documented range fail the test — there is no fallback bucket on purpose.
pub static NAMESPACE_RANGES: &[(char, u16, u16)] = &[
    // Active namespaces — historical block (1..99) plus canonical range.
    ('L', 1, 99),    // Lexer
    ('P', 1, 99),    // Parser, historical (P001-P021)
    ('P', 100, 199), // Parser, canonical
    ('T', 1, 99),    // Types, historical (T001-T036)
    ('T', 400, 499), // Types, canonical
    ('R', 1, 99),    // Runtime, historical (R001-R026)
    ('R', 600, 699), // Runtime, canonical
    ('W', 1, 99),    // Warnings (W001 retired but documented)
    // Reserved namespaces — new codes land here from now on.
    ('N', 200, 299), // Names / resolution
    ('I', 300, 399), // Imports
    ('V', 500, 599), // Verifier (post-type)
    ('D', 700, 799), // Deprecation warnings
    ('E', 800, 899), // Engine-specific
    ('S', 900, 999), // Skill / spec
];

/// Parse an `ILO-<letter><digits>` code into `(letter, number)`. Returns
/// `None` if the code is malformed. Used by the namespace regression test.
pub fn parse_code(code: &str) -> Option<(char, u16)> {
    let rest = code.strip_prefix("ILO-")?;
    let mut chars = rest.chars();
    let letter = chars.next()?;
    if !letter.is_ascii_uppercase() {
        return None;
    }
    let digits: String = chars.collect();
    let n: u16 = digits.parse().ok()?;
    Some((letter, n))
}

/// True if `code` lives in a documented namespace range.
pub fn in_documented_range(code: &str) -> bool {
    let Some((letter, n)) = parse_code(code) else {
        return false;
    };
    NAMESPACE_RANGES
        .iter()
        .any(|&(l, lo, hi)| l == letter && n >= lo && n <= hi)
}

/// All stable error codes for the ilo language.
pub static REGISTRY: &[ErrorEntry] = &[
    // ── Lexer ────────────────────────────────────────────────────────────────
    ErrorEntry {
        code: "ILO-L001",
        phase: Phase::Lex,
        short: "unexpected character",
        long: r#"## ILO-L001: unexpected character

A character was encountered that is not part of the ilo language.

**Example:**

    f x:n>n; $x

The `$` character is not valid in ilo source. Remove it or replace it
with a valid operator or identifier.
"#,
    },
    ErrorEntry {
        code: "ILO-L002",
        phase: Phase::Lex,
        short: "underscore in identifier — use hyphens",
        long: r#"## ILO-L002: underscore in identifier

ilo uses hyphens as word separators in identifiers, not underscores.

**Example that triggers this:**

    my_func x:n>n;x

**Fix:**

    my-func x:n>n;x
"#,
    },
    ErrorEntry {
        code: "ILO-L003",
        phase: Phase::Lex,
        short: "uppercase identifier — use lowercase",
        long: r#"## ILO-L003: uppercase identifier

ilo identifiers must be lowercase. Single uppercase letters (`L`, `R`)
are reserved for the built-in `List` and `Result` type constructors.

**Example that triggers this:**

    MyFunc x:n>n;x

**Fix:**

    my-func x:n>n;x
"#,
    },
    // ── Parser ───────────────────────────────────────────────────────────────
    ErrorEntry {
        code: "ILO-P001",
        phase: Phase::Parse,
        short: "unexpected token at top level",
        long: r#"## ILO-P001: unexpected token at top level

A token was found where a new declaration was expected. Declarations
start with a function name followed by parameters, or with `type`/`tool`.

**Common causes:**
- A stray token left over from a previous edit
- A missing semicolon between statement and the return expression

**Example:**

    f x:n>n; = x   -- stray `=` before expression
"#,
    },
    ErrorEntry {
        code: "ILO-P002",
        phase: Phase::Parse,
        short: "unexpected end of file at top level",
        long: r#"## ILO-P002: unexpected end of file

The file ended while the parser was expecting another declaration.
An incomplete function definition is a common cause.

**Example:**

    f x:n>n;    -- body missing
"#,
    },
    ErrorEntry {
        code: "ILO-P003",
        phase: Phase::Parse,
        short: "unexpected token",
        long: r#"## ILO-P003: unexpected token

A token was found where a different token was expected. The error
message names the expected and actual tokens using their source
characters (e.g. ``expected `>`, got `|` ``), not the parser's
internal token-kind names.

**Example:**

    f x:n|n;x

    ERROR ILO-P003: expected `>`, got `|`

The fix is to use `>` between the parameter list and the return type:

    f x:n>n;x
"#,
    },
    ErrorEntry {
        code: "ILO-P004",
        phase: Phase::Parse,
        short: "unexpected end of file",
        long: r#"## ILO-P004: unexpected end of file

The file ended before a required token was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P005",
        phase: Phase::Parse,
        short: "expected identifier, got token",
        long: r#"## ILO-P005: expected identifier

An identifier (function name, variable name, parameter name) was
expected but a different token was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P006",
        phase: Phase::Parse,
        short: "expected identifier, got end of file",
        long: r#"## ILO-P006: expected identifier, got end of file

The file ended before a required identifier was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P007",
        phase: Phase::Parse,
        short: "expected type annotation, got token",
        long: r#"## ILO-P007: expected type annotation

A type annotation (`n`, `t`, `b`, `L n`, `R n t`, or a type name)
was expected but a different token was found.

**Example:**

    f x: >n;x   -- type missing after `:`
"#,
    },
    ErrorEntry {
        code: "ILO-P008",
        phase: Phase::Parse,
        short: "expected type annotation, got end of file",
        long: r#"## ILO-P008: expected type annotation, got end of file

The file ended before a required type annotation was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P009",
        phase: Phase::Parse,
        short: "expected expression, got token",
        long: r#"## ILO-P009: expected expression

An expression was expected (e.g., a function body) but a different
token was found.

**Example:**

    f x:n>n;   -- body is empty; a semicolon ends a statement but
               -- the function body expression is missing
"#,
    },
    ErrorEntry {
        code: "ILO-P010",
        phase: Phase::Parse,
        short: "expected expression, got end of file",
        long: r#"## ILO-P010: expected expression, got end of file

The file ended before a required expression was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P011",
        phase: Phase::Parse,
        short: "expected pattern, got token",
        long: r#"## ILO-P011: expected pattern

A match pattern was expected but a different token was found.
Patterns include literals, `_` wildcard, type constructors (`Ok x`,
`Err e`, `true`, `false`), and record patterns.
"#,
    },
    ErrorEntry {
        code: "ILO-P012",
        phase: Phase::Parse,
        short: "expected pattern, got end of file",
        long: r#"## ILO-P012: expected pattern, got end of file

The file ended inside a match expression before a pattern was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P013",
        phase: Phase::Parse,
        short: "expected number literal, got token",
        long: r#"## ILO-P013: expected number literal

A numeric literal was required (e.g., for a list index `x.0`) but
a different token was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P014",
        phase: Phase::Parse,
        short: "expected number literal, got end of file",
        long: r#"## ILO-P014: expected number literal, got end of file

The file ended before a required number literal was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P015",
        phase: Phase::Parse,
        short: "expected tool description string",
        long: r#"## ILO-P015: expected tool description string

A `tool` declaration requires a string literal as its description.

**Example:**

    tool my-tool with { ... }       -- missing description
    tool my-tool "does things" with { ... }  -- correct
"#,
    },
    ErrorEntry {
        code: "ILO-P016",
        phase: Phase::Parse,
        short: "unexpected token after braceless guard body",
        long: r#"## ILO-P016: unexpected token after braceless guard body

Braceless guards allow a single expression as the body without braces.
If you need a function call as the guard body, use braces.

**Wrong:**

    cls sp:n>t;>=sp 1000 classify sp

The parser reads `classify` as the guard body and `sp` is left dangling.

**Correct:**

    cls sp:n>t;>=sp 1000{classify sp}

Braces are required when the guard body is a function call, because the
parser cannot know the function's arity to determine where the body ends.

Single-expression bodies (literals, variables, operators, ok/err wraps)
do not need braces:

    cls sp:n>t;>=sp 1000 "gold";>=sp 500 "silver";"bronze"
"#,
    },
    ErrorEntry {
        code: "ILO-P017",
        phase: Phase::Parse,
        short: "use-import failed",
        long: r#"## ILO-P017: use-import failed

A `use "path.ilo"` declaration could not be resolved. Possible causes:

- The path is not reachable from a file context (inline code via
  `ilo '<src>'` has no base directory to resolve against)
- The file does not exist at the given relative path
- The file could not be read (permissions, IO error)

The diagnostic message identifies the specific failure mode.

**Note:** ILO-P017 used to be raised by inline lambdas with captures from
the enclosing scope. Closure capture now works on every engine (tree, VM,
Cranelift JIT/AOT) so that path no longer errors; the code was repurposed
for `use`-import resolution.
"#,
    },
    ErrorEntry {
        code: "ILO-P018",
        phase: Phase::Parse,
        short: "variadic builtin not in trailing position",
        long: r#"## ILO-P018: variadic builtin not in trailing position

`fmt` (and its `format` alias) is variadic — it takes a template plus any
number of trailing values. When used as a nested argument to another known
builtin, it MUST occupy the LAST argument slot of the outer call, because the
parser has no way to know where `fmt`'s args end and the outer's resume.

**Wrong:**

    f x:t y:t z:t>n;f x fmt "tmpl {}" 1 z

`fmt` here is at the middle slot of a 3-arg outer `f`; the parser can't tell
whether `fmt` consumes `"tmpl {}" 1` or `"tmpl {}" 1 z`.

**Fix A: move `fmt` to the trailing slot** if the outer's signature allows
(most common idiom — `prnt fmt "..."`, `wr path fmt "..."`, `prnt str fmt
"..."` all already satisfy this rule).

**Fix B: wrap the `fmt` call in parens** to group its args explicitly:

    f x (fmt "tmpl {}" 1) z

The parens make the `fmt` call self-contained, so the outer's arg counter
treats it as a single operand.
"#,
    },
    ErrorEntry {
        code: "ILO-P019",
        phase: Phase::Parse,
        short: "use-import name not found",
        long: r#"## ILO-P019: use-import name not found

A `use "path.ilo" { name }` declaration listed a name that does not
exist in the imported file. The other names in the list are still
imported; only the missing ones produce this diagnostic.

**Fix:** correct the spelling, or remove the missing name from the
import list.
"#,
    },
    ErrorEntry {
        code: "ILO-P020",
        phase: Phase::Parse,
        short: "incomplete function header",
        long: r#"## ILO-P020: incomplete function header

A function header (`name params>type;body`) ran off the end of its line
without supplying everything the parser needed. The most common shapes:

**Missing return type after `>`:**

    f1 a:n>n;+a 1
    f2 a:n>R
    main>n;0

`f2`'s `R` (Result) needs an ok-type AND an err-type; here the line ends
with just `R`.

**Missing `>` entirely:**

    f1 a:n>n;+a 1
    f2 a:n
    main>n;0

Without `>` the parser cannot tell where the parameter list ends and the
return type starts.

**Fix:** finish the header on the same line as the function name. If you
want to wrap a long header across multiple lines, indent the
continuation:

    f2 a:n
      >R n t;...

Indented lines are joined with `;` automatically; only an unindented
line break ends a declaration.

This diagnostic exists so the error span lands on the function whose
header is incomplete, not on the next function in the file.
"#,
    },
    ErrorEntry {
        code: "ILO-P101",
        phase: Phase::Parse,
        short: "list-literal element is a builtin call without parens",
        long: r#"## ILO-P101: list-literal element is a builtin call without parens

Inside a list literal `[...]`, each whitespace-separated token is treated as
its own element by default. `[a b c]` is a 3-element list, not a call.
When an element starts with a builtin name that takes operands, the parser
can't know how many of the following tokens are call arguments and how many
are sibling list elements.

For builtins with a fixed known arity (e.g. `str`, `at`, `map`), ilo
auto-expands the call up to that arity. But for variadic builtins like
`fmt` and `fmt2`, the arity isn't fixed, so the call can't be auto-expanded
and the bare name would silently fall through as an undefined reference.

**Wrong:**

    row=[k str c fmt2 rv 2]

`fmt2 rv 2` is a 2-arg call producing one formatted string, but the list
parser would treat `fmt2`, `rv`, and `2` as three separate elements.

**Fix A: wrap the call in parens.**

    row=[k str c (fmt2 rv 2)]

The parens group the call as one element. Works for any builtin.

**Fix B: bind the call first, then use the binding.**

    s=fmt2 rv 2
    row=[k str c s]

Use this when the same value is needed in more than one place, or when the
inline form gets unreadable.
"#,
    },
    ErrorEntry {
        code: "ILO-P102",
        phase: Phase::Parse,
        short: "top-level binding outside a function declaration",
        long: r#"## ILO-P102: top-level binding outside a function declaration

ilo programs are made of declarations: functions (`name>type;body`), type
declarations (`type T = ...`), tools (`tool name ...`), or `use` imports.
A bare `name=expr` is a **binding statement**, not a declaration - it has
to live inside a function body.

This diagnostic fires when a file starts with (or contains) a top-level
chain like:

    pts=gen-pts
    cs0=[[4.8 4.9][6.2 7.1]]
    cs1=iter cs0 pts
    cs2=iter cs1 pts
    prnt cs2

Without a function header to anchor those bindings, the parser either
fails on the bare `=` (ILO-P003) or - when a prior `name>type;body`
declaration sits above - slurps the whole chain into that function's
body, producing a wall of misleading ILO-T005 cascades that point at
the wrong line.

**Fix: wrap the chain in a `main>_;` entry point.**

    main>_;
    pts=gen-pts
    cs0=[[4.8 4.9][6.2 7.1]]
    cs1=iter cs0 pts
    cs2=iter cs1 pts
    prnt cs2

`main>_;` is the conventional entry-point header - the underscore means
"infer the return type from the body". The bindings inside the body are
now real let-bindings inside `main`'s scope.

**Why this matters:** ilo is a token-minimal language for agents. The
manifesto target is that a wrong program produces *one* actionable
diagnostic, not a cascade. ILO-P102 collapses what used to be 5-50
ILO-T005 lines (one per slurped binding) into a single pointer at the
shape fix.
"#,
    },
    ErrorEntry {
        code: "ILO-P021",
        phase: Phase::Parse,
        short: "ambiguous double-minus prefix-binop chain",
        long: r#"## ILO-P021: ambiguous double-minus prefix-binop chain

A statement of the shape `- -<op> a b <op> c d`, where each `<op>` is a
prefix binop in `{+, *, /}` and is followed by two atoms, is rejected
because it parses in a way that almost always disagrees with the
intuitive reading.

**Example that triggers this (damped pendulum, natural form):**

    f gl:n s:n b:n om:n>n;- -*gl s *b om

The author wants `-g*s - b*om`. The parser instead reads it as:

1. Inner `-` is a binary subtract: `(g*s) - (b*om)`.
2. Outer `-` has only one operand left, so it becomes unary negate.
3. Final expression: `-((g*s) - (b*om)) = -g*s + b*om`.

The sign of the second product is flipped. The verifier sees a valid
expression and the evaluator runs it, so the bug is silent — only
domain knowledge surfaces it.

**Fixes**

Negate the sum of both products (cheapest):

    - 0 +*gl s *b om            -- = 0 - (g*s + b*om) = -g*s - b*om

Or bind first, then operate:

    p=*gl s; q=*b om; - 0 +p q  -- = -(p + q)

If you really do want `-((a OP1 b) - (c OP2 d))`, bind the inner
subtract and negate it explicitly:

    p=*a b; q=*c d; r=- p q; - 0 r

This diagnostic exists to catch a specific silent-miscompile shape;
single-atom variants like `- -a b` (negate of subtract over atoms) are
unambiguous and remain accepted.
"#,
    },
    ErrorEntry {
        code: "ILO-P022",
        phase: Phase::Parse,
        short: "malformed generic type-parameter block",
        long: r#"## ILO-P022: malformed generic type-parameter block

The `<…>` generic type-parameter block after a function name is
malformed.

**Valid forms**

```
mn<a>              -- unbounded type variable
mn<a:comparable>   -- bounded type variable
mn<a:comparable b:numeric>  -- multiple type variables
```

**Rules**

* Each entry is a single lowercase letter, optionally followed by
  `:boundname`.
* Valid bound names: `any`, `comparable`, `numeric`, `text`.
* Multiple variables are space-separated inside the `<…>`.

**Common mistakes**

```
mn<A:comparable>   -- uppercase letter, not allowed (lexer rejects capitals)
mn<ab:comparable>  -- multi-letter name, not allowed
mn<a:Comparable>   -- capitalised bound name, not allowed (lexer rejects capitals)
```
"#,
    },
    ErrorEntry {
        code: "ILO-P103",
        phase: Phase::Parse,
        short: "AST nesting depth exceeded",
        long: r#"## ILO-P103: AST nesting depth exceeded

The parser refused a program whose expression or statement tree nests more
deeply than the configured cap (default 256). A deeply nested input is
almost always a denial-of-service payload aimed at `ilo serv` or any other
context that compiles untrusted source — `((((...((1 + 1))))...))` recurses
straight through the OS thread stack on a tree-walker parser, and pathological
verifier complexity follows from there.

The default cap of 256 is far above anything hand-written: the deepest
expression in the in-tree examples is under 20 levels. If a legitimate program
genuinely needs more, raise the cap with `--max-ast-depth N` on `ilo`,
`ilo run`, `ilo check`, `ilo build`, or `ilo serv`:

    ilo --max-ast-depth 1024 run prog.ilo
    ilo serv --max-ast-depth 1024

**Fix:** flatten the expression by binding intermediates, or override the cap
deliberately if the depth is real.
"#,
    },
    // ── Type / Verifier ──────────────────────────────────────────────────────
    ErrorEntry {
        code: "ILO-T001",
        phase: Phase::Verify,
        short: "duplicate type definition",
        long: r#"## ILO-T001: duplicate type definition

A `type` declaration uses a name that was already defined.

**Fix:** rename one of the types or remove the duplicate.
"#,
    },
    ErrorEntry {
        code: "ILO-T002",
        phase: Phase::Verify,
        short: "duplicate function/tool definition",
        long: r#"## ILO-T002: duplicate function or tool definition

A function or tool uses a name that was already defined in this file.

**Fix:** rename one of the functions or remove the duplicate.
"#,
    },
    ErrorEntry {
        code: "ILO-T003",
        phase: Phase::Verify,
        short: "undefined type",
        long: r#"## ILO-T003: undefined type

A type name used in a signature or record literal is not defined.

**Example:**

    f x:Point>n;x.val   -- 'Point' is not defined

**Fix:** add a `type Point { ... }` declaration, or correct the spelling.
"#,
    },
    ErrorEntry {
        code: "ILO-T004",
        phase: Phase::Verify,
        short: "undefined variable",
        long: r#"## ILO-T004: undefined variable

A variable name was used that has not been bound in the current scope.
Variables are bound by `let` statements or function parameters.

**Example:**

    f x:n>n;+x y   -- 'y' is not defined

**Fix:** bind the variable before use, or pass it as a parameter:

    f x:n y:n>n;+x y

### Common pitfall: `name.N` after `zip`

ilo has no tuple type. `zip xs ys` returns `L (L n)` — a list of
two-element lists, not a list of tuples. Agents reaching for
`tup.0` / `pair.0` tuple-access syntax will see ILO-T004 on the
unbound `tup` / `pair` name.

**Wrong:**

    g pair:L n>n;+pair.0 pair.1   -- pair.0 works only once pair is bound

If `pair` is unbound (e.g. you wrote `tup.0` without binding `tup`),
the fix is to bind it from the outer list and index with `at`:

    f>L n;
      xs=[1 2 3];ys=[10 20 30];zs=zip xs ys;
      map (pair:L n>n;+at pair 0 at pair 1) zs
"#,
    },
    ErrorEntry {
        code: "ILO-T005",
        phase: Phase::Verify,
        short: "undefined function",
        long: r#"## ILO-T005: undefined function

A function was called that is not defined in this file or as a builtin.

**Example:**

    f x:n>n;double x   -- 'double' is not defined

**Fix:** define the function, or correct the spelling.

### Gotcha: call vs binary-op in assignment-RHS

Whitespace-juxtaposition is the call syntax in ilo, so a bare name
followed by another token in an expression is parsed as a call, not as
"name then operator". The classic case:

    f xi:n xj:n>n;dx=xj 0-xi;dx

This parses as `dx = (xj 0) - xi` — a call to `xj` with argument `0`,
whose result is then subtracted from `xi`. Verification fails with
ILO-T005 because `xj` is a number, not a function.

The agent almost certainly meant one of:

- `dx=-xj xi`              -- subtract: prefix `-`
- `dx=+xj -0 xi`           -- xj + (0-xi)
- `nxi=0-xi;dx=+xj nxi`    -- pre-bind the operand

In ilo's prefix-operator world there is no ambiguity once the operator
leads the expression. The "looks like infix" shape `name expr` is
always a call.
"#,
    },
    ErrorEntry {
        code: "ILO-T006",
        phase: Phase::Verify,
        short: "arity mismatch",
        long: r#"## ILO-T006: arity mismatch

A function was called with the wrong number of arguments.

**Example:**

    add a:n b:n>n;+a b
    f x:n>n;add x   -- 'add' expects 2 args, got 1

**Fix:** pass the correct number of arguments.
"#,
    },
    ErrorEntry {
        code: "ILO-T007",
        phase: Phase::Verify,
        short: "type mismatch at call site",
        long: r#"## ILO-T007: type mismatch at call site

An argument passed to a function has the wrong type.

**Example:**

    double x:n>n;*x 2
    f s:t>n;double s   -- 's' is 't', but 'double' expects 'n'

**Fix:** pass a value of the correct type, or use a conversion builtin
such as `num` to convert text to a number.
"#,
    },
    ErrorEntry {
        code: "ILO-T008",
        phase: Phase::Verify,
        short: "return type mismatch",
        long: r#"## ILO-T008: return type mismatch

The type of the return expression does not match the declared return type.

**Example:**

    f x:n>t;x   -- 'x' is 'n', but 'f' declares return type 't'

**Fix:** change the return expression or correct the return type annotation.
"#,
    },
    ErrorEntry {
        code: "ILO-T009",
        phase: Phase::Verify,
        short: "arithmetic operator type error",
        long: r#"## ILO-T009: arithmetic operator type error

An arithmetic operator (`+`, `-`, `*`, `/`) was applied to operands
of mismatched or wrong types.

`+` works on `n + n`, `t + t`, or `L T + L T`.
`-`, `*`, `/` require `n` operands.
"#,
    },
    ErrorEntry {
        code: "ILO-T010",
        phase: Phase::Verify,
        short: "comparison operator type error",
        long: r#"## ILO-T010: comparison operator type error

A comparison operator (`<`, `>`, `<=`, `>=`, `=`, `!=`) was applied
to operands of mismatched or non-comparable types.

Comparisons require both operands to be the same type (`n` or `t`).
"#,
    },
    ErrorEntry {
        code: "ILO-T011",
        phase: Phase::Verify,
        short: "append (+=) type error",
        long: r#"## ILO-T011: append (+=) type error

The `+=` operator requires a list on the left side. The element being
appended must match the list's element type.

**Example:**

    f xs:n>L n;+=xs 1   -- 'xs' is 'n', not a list
"#,
    },
    ErrorEntry {
        code: "ILO-T012",
        phase: Phase::Verify,
        short: "negate type error",
        long: r#"## ILO-T012: negate type error

Unary negation (`-x`) requires a numeric argument.

**Example:**

    f s:t>n;-s   -- cannot negate a text value
"#,
    },
    ErrorEntry {
        code: "ILO-T013",
        phase: Phase::Verify,
        short: "builtin argument type error",
        long: r#"## ILO-T013: builtin argument type error

A builtin function was called with an argument of the wrong type.

Common builtins and their required types:
- `len` — `t` or `L T`
- `str` — `n`
- `num` — `t`
- `abs`, `flr`, `cel` — `n`
- `min`, `max` — `n`, `n`
"#,
    },
    ErrorEntry {
        code: "ILO-T014",
        phase: Phase::Verify,
        short: "foreach collection type error",
        long: r#"## ILO-T014: foreach collection type error

The `foreach` builtin requires a list as its first argument.

**Example:**

    f s:t>n;foreach s x;x   -- 's' is 't', not a list
"#,
    },
    ErrorEntry {
        code: "ILO-T015",
        phase: Phase::Verify,
        short: "record missing field",
        long: r#"## ILO-T015: record missing field

A record literal is missing one or more fields required by the type.

**Example:**

    type point{x:n;y:n}
    f>point;point{x=1}   -- missing 'y'

**Fix:** include all required fields.
"#,
    },
    ErrorEntry {
        code: "ILO-T016",
        phase: Phase::Verify,
        short: "record unknown field",
        long: r#"## ILO-T016: record unknown field

A record literal or `with` expression includes a field name that
does not exist on the type.

**Fix:** remove the extra field or correct the spelling.
"#,
    },
    ErrorEntry {
        code: "ILO-T017",
        phase: Phase::Verify,
        short: "record field type mismatch",
        long: r#"## ILO-T017: record field type mismatch

A field in a record literal was given a value of the wrong type.

**Fix:** ensure the value matches the field's declared type.
"#,
    },
    ErrorEntry {
        code: "ILO-T018",
        phase: Phase::Verify,
        short: "field access on non-record type",
        long: r#"## ILO-T018: field access on non-record type

A field access (`value.field` or `value.?field`) was attempted on a
value that is not a record type. Field access (including the safe
`.?` form) is only valid on named `type` instances.

**Fix:** if the receiver is a map (`M _ _`), use `mget m "key"`, which
returns an Option you can match on or default with `??`.
"#,
    },
    ErrorEntry {
        code: "ILO-T019",
        phase: Phase::Verify,
        short: "field not found on type",
        long: r#"## ILO-T019: field not found on type

A field name used in a field access expression (`value.name`) does
not exist on the record type.

**Fix:** correct the field name spelling.
"#,
    },
    ErrorEntry {
        code: "ILO-T020",
        phase: Phase::Verify,
        short: "'with' on non-record type",
        long: r#"## ILO-T020: 'with' on non-record type

The `with` expression for updating record fields requires a record value.
"#,
    },
    ErrorEntry {
        code: "ILO-T021",
        phase: Phase::Verify,
        short: "'with' field not found",
        long: r#"## ILO-T021: 'with' field not found

A field name used in a `with` expression does not exist on the record type.
"#,
    },
    ErrorEntry {
        code: "ILO-T022",
        phase: Phase::Verify,
        short: "'with' field type mismatch",
        long: r#"## ILO-T022: 'with' field type mismatch

A value provided in a `with` expression has the wrong type for the field.
"#,
    },
    ErrorEntry {
        code: "ILO-T023",
        phase: Phase::Verify,
        short: "index access on non-list type",
        long: r#"## ILO-T023: index access on non-list type

A list index access (`value.0`) was attempted on a non-list value.
"#,
    },
    ErrorEntry {
        code: "ILO-T024",
        phase: Phase::Verify,
        short: "non-exhaustive match",
        long: r#"## ILO-T024: non-exhaustive match

A match expression does not cover all possible cases. Add a wildcard
arm (`_ -> expr`) to handle any unmatched values, or add explicit
arms for each missing case.

**Example:**

    f r:R n t>n;match r{Ok v->v}   -- missing Err arm and wildcard
"#,
    },
    ErrorEntry {
        code: "ILO-T025",
        phase: Phase::Verify,
        short: "'!' used on non-Result call",
        long: r#"## ILO-T025: '!' used on non-Result call

The `!` auto-unwrap operator can only be used on function calls that
return a Result type (`R ok err`). The called function returns a
different type.

**Example:**

    f x:n>n;x
    g x:n>n;f! x   -- error: f returns n, not R

**Fix:** Remove `!` or change the called function to return `R`.
"#,
    },
    ErrorEntry {
        code: "ILO-T026",
        phase: Phase::Verify,
        short: "'!' used in non-Result function",
        long: r#"## ILO-T026: '!' used in non-Result function

The `!` auto-unwrap operator propagates errors to the enclosing
function, so the enclosing function must return a Result type
(`R ok err`).

**Example:**

    inner x:n>R n t;~x
    outer x:n>n;inner! x   -- error: outer returns n, not R

**Fix:** Change the enclosing function's return type to `R`.
"#,
    },
    ErrorEntry {
        code: "ILO-T027",
        phase: Phase::Verify,
        short: "braceless guard body looks like a function name",
        long: r#"## ILO-T027: braceless guard body looks like a function name

A braceless guard's body is a single identifier that matches a known
function name. This usually means you intended to call the function
but forgot to wrap it in braces.

**Wrong:**

    cls sp:n>t;>=sp 1000 classify

`classify` is treated as a variable reference, not a function call.

**Correct:**

    cls sp:n>t;>=sp 1000{classify sp}

Use braces when the guard body is a function call.
"#,
    },
    ErrorEntry {
        code: "ILO-T028",
        phase: Phase::Verify,
        short: "brk/cnt used outside a loop",
        long: r#"## ILO-T028: brk/cnt used outside a loop

`brk` (break) and `cnt` (continue) can only be used inside a loop
body (`@` foreach or `wh` while).

**Wrong:**

    f x:n>n;brk

**Correct:**

    f xs:L n>n;@ xs x{brk x}
"#,
    },
    ErrorEntry {
        code: "ILO-T029",
        phase: Phase::Verify,
        short: "unreachable code",
        long: r#"## ILO-T029: unreachable code

Code after a `ret` (early return) or `brk` (break) statement will
never be executed.

**Example:**

    f x:n>n;ret x;*x 2   -- '*x 2' is unreachable

**Fix:** remove the unreachable code or move it before the `ret`/`brk`.
"#,
    },
    ErrorEntry {
        code: "ILO-T030",
        phase: Phase::Verify,
        short: "circular type alias",
        long: r#"## ILO-T030: circular type alias

A `type` alias declaration references itself, either directly or
through a cycle of other aliases. Aliases must be acyclic so the
verifier can resolve them to concrete types.

**Example that triggers this:**

    type foo foo                -- direct self-reference
    type a b;type b a           -- 2-cycle

**Fix:** break the cycle by introducing a named record type, or
remove one side of the cycle.
"#,
    },
    ErrorEntry {
        code: "ILO-T031",
        phase: Phase::Verify,
        short: "type alias shadows builtin",
        long: r#"## ILO-T031: type alias shadows a builtin type

A `type` alias uses a name reserved for a builtin type (`n`, `t`,
`b`, `L`, `R`, `_`). These names are part of the language and cannot
be redefined.

**Fix:** choose a different name for the alias.
"#,
    },
    ErrorEntry {
        code: "ILO-T032",
        phase: Phase::Verify,
        short: "bare 'fmt' result is discarded",
        long: r#"## ILO-T032: bare 'fmt' result is discarded

`fmt` (and `fmt2`) are pure-functional formatters — they build a string
and return it. When called as a non-tail statement with no binding, the
returned string is silently discarded on every engine (tree, VM, Cranelift).
Nothing reaches stdout.

The common mistake is treating `fmt` like Rust's `println!` or Python's
`print` — but `fmt` does not perform any I/O.

**Example (bug):**

    report v:n>n;fmt "v={}" v;prnt "done";v
    -- 'fmt "v={}" v' is evaluated and thrown away

**Fix — print it:**

    report v:n>n;prnt fmt "v={}" v;prnt "done";v

**Fix — capture it:**

    report v:n>n;line=fmt "v={}" v;prnt line;v

`fmt` as the **tail** expression of a function is fine — that returns the
string to the caller, which is the documented idiom (`say-x>t;fmt "x={}" 42`).
This warning only fires when `fmt` is followed by another statement.
"#,
    },
    ErrorEntry {
        code: "ILO-T033",
        phase: Phase::Verify,
        short: "bare mutation-shaped builtin result is discarded",
        long: r#"## ILO-T033: bare mutation-shaped builtin result is discarded

`+=xs v`, `mset m k v`, and `mdel m k` look like in-place mutation but
ilo's functional semantics return a **new** value. The source binding
is only updated when the call is written as an assignment:

    xs = +=xs v
    m  = mset m k v
    m  = mdel m k

As a bare statement with no rebind, the result is silently discarded on
every engine (tree, VM, Cranelift) and the original binding is unchanged.
The fix is to assign the result back, or to use it in a larger expression.

**Example (bug):** bare `+=` inside a loop body

    f>L n;out=[];@i 0..3{+=out i};out
    -- returns [], not [0, 1, 2]; +=out i throws its result away each pass

**Fix — rebind:**

    f>L n;out=[];@i 0..3{out=+=out i};out
    -- returns [0, 1, 2]

**Example (bug):** bare `mset` before returning the map

    f>M t n;m=mmap;mset m "a" 1;m
    -- returns {}, not {a: 1}; mset's result is thrown away

**Fix — rebind:**

    f>M t n;m=mmap;m=mset m "a" 1;m
    -- returns {a: 1}

This warning fires when any of these calls appear at a position whose
value is discarded:
- a non-tail statement in a function/if/match body, OR
- anywhere inside a loop body (each iteration discards its tail).

Tail position in a function body, `if` arm, or `?{}` arm is fine — the
value flows out as the function/branch result.
"#,
    },
    ErrorEntry {
        code: "ILO-T043",
        phase: Phase::Verify,
        short: "recursive call discarded at non-tail position",
        long: r#"## ILO-T043: recursive call discarded at non-tail position

A recursive self-call appears before another statement in the function
body. Because ilo evaluates statements functionally and only the tail
expression becomes the function's return value, any earlier expression
statement has its result silently dropped. The recursion runs but its
return value never reaches the caller.

A call is in **tail position** when it IS the function's return value:
- the last statement of the body
- the expression of a `ret` statement
- an arm of a tail-position `?` match
- the body of a braceless guard

Calls anywhere else (followed by other statements, used as an operand,
inside `@`/`wh` loops, in a non-tail match arm) are NOT in tail position.

**Example (bug):** recursive linear search with discarded recursive return

    find-idx xs:L n target:n i:n>n
      =target at xs i i
      find-idx xs target +i 1
      -1

The recursive call is the second statement; the third statement (`-1`)
discards its return. Every call falls through to `-1`.

**Fix — put the recursive call in tail position:**

    find-idx xs:L n target:n i:n>n
      >=i len xs (-1)
      =target at xs i i
      find-idx xs target +i 1

The out-of-bounds branch is the early exit (braceless guard, also in tail
position). The recursive call is now the last statement, so its return
flows out as ours.

**Alternative fix — explicit `ret`:**

    find-idx xs:L n target:n i:n>n
      =target at xs i i
      ret find-idx xs target +i 1
      -- unreachable, kept for illustration
      -1

`ret <call>` puts the call in tail position via early return.

**Alternative fix — ternary:**

    find-idx xs:L n target:n i:n>n
      ?h =target at xs i i i (find-idx xs target +i 1)

`?h cond then else` consumes the branch values directly.

The warning fires only when caller and callee names match (a self-call).
Bare non-recursive user-fn calls at non-tail position do not warn — they
may be legitimately side-effecting (logging, file I/O).
"#,
    },
    ErrorEntry {
        code: "ILO-T044",
        phase: Phase::Verify,
        short: "net builtin called with a World that has net=false",
        long: r#"## ILO-T044: net builtin called with a World that has net=false

A network builtin (`get`, `pst`, `put`, `pat`, `del`, `hed`, `opt`,
`getx`, `pstx`, `get-many`, `get-to`, `pst-to`) is called in a scope
that contains a World value statically known to deny network access
(constructed via `world-no-net`).

This is a **static capability violation**: the code explicitly declared
that net access is denied, then attempted a net operation anyway.

**Example:**

    fetch w:W url:t>R t t
      wn = world-no-net
      get url              -- ERROR ILO-T044: wn has net=false

**Fix:** either remove the `world-no-net` binding, or restructure so
the net call happens in a context where net access is allowed.

    fetch url:t>R t t
      get url              -- ok: no net-denied World in scope

Note: this check only fires for syntactically known constructions
(`world-no-net`). Dynamic World values (from the `world` builtin or
function parameters) are not checked statically — those are enforced
at runtime via the `--allow-net` flag.
"#,
    },
    ErrorEntry {
        code: "ILO-T034",
        phase: Phase::Verify,
        short: "'!' / '!!' used on a non-callable value",
        long: r#"## ILO-T034: '!' / '!!' used on a non-callable value

`!` and `!!` are the **auto-unwrap operators**, and they only apply to
function calls. They take the Result or Optional returned by a call
and unwrap it: `~v` / non-nil flows through as the inner value, while
`^e` / nil either propagates (`!`) or aborts (`!!`).

When the operator is attached to a bare identifier that resolves to a
value (a local binding, a parameter, a destructured field), there is
no call for the operator to act on. In v0.11.4 and earlier this shape
silently returned `nil` on the default-engine inline path; the verifier
now catches it.

**Example (bug):**

    main>R n t;x=42;x!
    -- 'x' is a Number, not a function — '!' has nothing to unwrap

**Fix — match on a Result-valued binding:**

    ?x{~v:v;^e:^e}

**Fix — auto-unwrap a producer's return at the assignment:**

    scs = producer! ...
    -- now 'scs' holds the unwrapped value; reference it directly

This error fires only when the bang is adjacent to the ident (`x!`,
`y!!`). Bang inside a call argument (`f !x`) is the logical-NOT prefix
and is unaffected.
"#,
    },
    ErrorEntry {
        code: "ILO-T035",
        phase: Phase::Verify,
        short: "Result arm (`~v:` / `^e:`) on Option subject",
        long: r#"## ILO-T035: Result arm (`~v:` / `^e:`) on Option subject

`~v:` and `^e:` are **Result** arms - they discriminate on the Ok/Err
tag carried by a `R T E` value. Optional values (`O T`) are not tagged
this way: an Option is either the inner value or `_` (nil). At runtime
a `~v:` arm on an Option subject never matches and the match silently
falls through to the wildcard or to no arm at all - usually returning
the wrong answer with no error.

The verifier now flags this shape so the bug surfaces at check time
instead of being chased through wrong outputs.

**Example (bug):**

    main>n;m=mmap;m=mset m "k" 5;?(mget m "k"){~v:v;_:0}
    -- mget returns O n; ~v: never matches; result is 0, not 5

**Fix - unwrap with `??`:**

    main>n;m=mmap;m=mset m "k" 5;??(mget m "k") 0
    -- ??x default: inner value if non-nil, default otherwise

**Fix - match a literal value and `_:` for nil:**

    main>t;m=mmap;m=mset m "k" 5;?(mget m "k"){5:"hit";_:"miss"}

`~v:` / `^e:` arms remain correct on Result (`R T E`) subjects and are
unaffected.
"#,
    },
    ErrorEntry {
        code: "ILO-T036",
        phase: Phase::Verify,
        short: "call requires too many register slots (VM cap)",
        long: r#"## ILO-T036: call requires too many register slots

A call site needs more contiguous VM registers than the 256-slot
window allows. Splitting the call into intermediate bindings reduces
the slot pressure.

**Fix:** bind sub-expressions to locals so each call has fewer live
operands at the same time.

This is a VM-specific limit; the tree interpreter and Cranelift JIT
have higher caps. See also ILO-T035 (function exceeds the 256-register
VM cap).
"#,
    },
    // ── Engine-specific ──────────────────────────────────────────────────────
    ErrorEntry {
        code: "ILO-E801",
        phase: Phase::Engine,
        short: "AOT compile needs an entry function",
        long: r#"## ILO-E801: AOT compile needs an entry function

`ilo compile <file>` builds a standalone native binary, which means the
binary's `main()` calls a single entry function. AOT picks that entry
the same way the in-process engines do:

1. an explicit positional `func` argument wins
   (`ilo compile foo.ilo -o foo entry-fn`)
2. otherwise a file with a single user-defined function uses it
3. otherwise a function called `main` is used if defined

If none of those apply, AOT errors with this code instead of compiling
the first-declared function as the entry — which historically produced
a binary that SIGSEGV'd because the chosen function's shape did not
match the wrapper's expectation.

**Wrong:**

    helper>n;42
    run>n;helper

Two functions, no `main`, no explicit entry → ILO-E801.

**Fixes:**

- Rename one of the functions to `main`:

      helper>n;42
      main>n;helper

- Pass the entry function name on the command line:

      ilo compile prog.ilo -o prog run

The other engines (tree / VM / Cranelift JIT) raise the same kind of
error at the CLI dispatch layer; ILO-E801 is the AOT-side equivalent so
the failure mode is the same across every engine.
"#,
    },
    // ── Warnings ─────────────────────────────────────────────────────────────
    ErrorEntry {
        code: "ILO-W001",
        phase: Phase::Verify,
        short: "guard without else inside loop (retired)",
        long: r#"## ILO-W001: guard without else inside loop (retired)

This warning has been retired. Braced guards `cond{body}` are now
conditional execution (no early return), making them safe inside loops.
Use braceless guards `cond expr` for early return, or `ret` inside
braced guards for explicit early return from loops.
"#,
    },
    ErrorEntry {
        code: "ILO-W002",
        phase: Phase::Verify,
        short: "iterating jpar! result, use jpar-list! instead",
        long: r#"## ILO-W002: iterating jpar! result

You wrote something like `@x (jpar! body){...}`. `jpar` parses any JSON
value (object, array, scalar) and returns `R ? t`; after `!` the
inner value is polymorphic (`?` / Unknown), so the verifier cannot
prove it is a list. At runtime iteration only succeeds when the JSON
top-level happens to be an array, and even when it is, the
polymorphic return type forces you to thread `?` through any wrapping
function.

**Fix:** use `jpar-list!` instead:

```ilo
@x (jpar-list! body){prnt x}
```

`jpar-list` asserts the top-level value is an array, returns
`R (L _) t`, and after `!` you get `L _`, a list ready to iterate.
The intent ("this JSON is an array") is captured at parse time, and
the surrounding function's return type stays clean.

Use plain `jpar` when the JSON shape is unknown and you want to
inspect it with `?`; use `jpar-list` when you know (or expect) the
response to be an array.
"#,
    },
    // ── Runtime ──────────────────────────────────────────────────────────────
    ErrorEntry {
        code: "ILO-R001",
        phase: Phase::Runtime,
        short: "undefined variable at runtime",
        long: r#"## ILO-R001: undefined variable at runtime

A variable was referenced that does not exist in the current scope.
This should normally be caught by the verifier (ILO-T004). Seeing
this at runtime indicates the program was run without verification,
or a dynamic path was taken.
"#,
    },
    ErrorEntry {
        code: "ILO-R002",
        phase: Phase::Runtime,
        short: "undefined function at runtime",
        long: r#"## ILO-R002: undefined function at runtime

A function was called that is not defined. This should normally be
caught by the verifier (ILO-T005).
"#,
    },
    ErrorEntry {
        code: "ILO-R003",
        phase: Phase::Runtime,
        short: "division by zero",
        long: r#"## ILO-R003: division by zero

A division operation (`/`) was performed with a zero divisor.

**Fix:** check that the divisor is non-zero before dividing.
"#,
    },
    ErrorEntry {
        code: "ILO-R004",
        phase: Phase::Runtime,
        short: "runtime type error",
        long: r#"## ILO-R004: runtime type error

An operation was applied to a value of the wrong type at runtime.
This may indicate a verifier gap for a dynamic code path.
"#,
    },
    ErrorEntry {
        code: "ILO-R005",
        phase: Phase::Runtime,
        short: "field not found at runtime",
        long: r#"## ILO-R005: field not found at runtime

A field access was performed on a record that does not have the
requested field. Normally caught statically (ILO-T019).
"#,
    },
    ErrorEntry {
        code: "ILO-R006",
        phase: Phase::Runtime,
        short: "list index out of bounds",
        long: r#"## ILO-R006: list index out of bounds

A list index access used an index that is out of the list's range.
ilo lists are zero-indexed.

**Fix:** check `len` before indexing into the list.
"#,
    },
    ErrorEntry {
        code: "ILO-R007",
        phase: Phase::Runtime,
        short: "foreach requires a list",
        long: r#"## ILO-R007: foreach requires a list

The `foreach` builtin was given a non-list value at runtime.
Normally caught statically (ILO-T014).
"#,
    },
    ErrorEntry {
        code: "ILO-R008",
        phase: Phase::Runtime,
        short: "'with' requires a record",
        long: r#"## ILO-R008: 'with' requires a record

The `with` expression was applied to a non-record value at runtime.
Normally caught statically (ILO-T020).
"#,
    },
    ErrorEntry {
        code: "ILO-R009",
        phase: Phase::Runtime,
        short: "builtin argument error at runtime",
        long: r#"## ILO-R009: builtin argument error at runtime

A builtin function received the wrong type of argument at runtime.
Normally caught statically (ILO-T013).
"#,
    },
    ErrorEntry {
        code: "ILO-R010",
        phase: Phase::Runtime,
        short: "compile error: undefined variable",
        long: r#"## ILO-R010: compile error: undefined variable

The VM compiler encountered an undefined variable while compiling a function.
Normally caught statically (ILO-T004) before compilation.
"#,
    },
    ErrorEntry {
        code: "ILO-R011",
        phase: Phase::Runtime,
        short: "compile error: undefined function",
        long: r#"## ILO-R011: compile error: undefined function

The VM compiler encountered an undefined function reference.
Normally caught statically (ILO-T005) before compilation.
"#,
    },
    ErrorEntry {
        code: "ILO-R012",
        phase: Phase::Runtime,
        short: "no functions defined",
        long: r#"## ILO-R012: no functions defined

The program has no callable functions. At least one function
must be defined to run a program.
"#,
    },
    ErrorEntry {
        code: "ILO-R013",
        phase: Phase::Runtime,
        short: "internal VM error",
        long: r#"## ILO-R013: internal VM error

The virtual machine encountered an unexpected internal state,
such as an unrecognised opcode. This indicates a compiler bug,
not a user error.

If you see this, please file a bug report.
"#,
    },
    ErrorEntry {
        code: "ILO-R014",
        phase: Phase::Runtime,
        short: "auto-unwrap propagated Err / nil",
        long: r#"## ILO-R014: auto-unwrap propagated Err / nil

The `!` auto-unwrap operator observed an `^e` (Err) or `nil` return
from the called function and propagated it as the enclosing function's
return value. This is normal control flow, not a bug — the message
identifies the propagation point for diagnostic purposes.

**See also:**

- ILO-R026 — `!!` panic-unwrap aborts the program instead of propagating.
- ILO-T025 / ILO-T026 — static checks that `!` is applied correctly.
"#,
    },
    ErrorEntry {
        code: "ILO-R015",
        phase: Phase::Runtime,
        short: "AOT runtime fault",
        long: r#"## ILO-R015: AOT runtime fault

An AOT-compiled ilo binary received a fatal signal (SIGSEGV, SIGBUS,
SIGFPE, SIGILL, or SIGABRT) and aborted. Unlike the tree-walker, VM,
and JIT backends — which surface runtime errors as structured
`ILO-R###` diagnostics through `JIT_RUNTIME_ERROR` — AOT binaries
execute as standalone native code, so any hard fault would otherwise
exit with a raw signal exit code (e.g. 139 for SIGSEGV) and no
diagnostic on stderr.

The AOT runtime installs an async-signal-safe handler in
`ilo_aot_init` that writes a single JSON line to stderr identifying
the signal before letting the default handler terminate the process
with the conventional exit code (128 + signo).

A hard fault from an AOT binary is always a bug in ilo itself —
either a codegen issue in the Cranelift AOT backend, or a missing
runtime check that the other engines apply. Please file an issue
with the source program and the JSON diagnostic.
"#,
    },
    ErrorEntry {
        code: "ILO-R016",
        phase: Phase::Runtime,
        short: "wall-clock runtime budget exceeded",
        long: r#"## ILO-R016: wall-clock runtime budget exceeded

`ilo run` aborted because the program ran for longer than the
configured wall-clock budget (default 60 s). The watchdog thread
fires this when `elapsed > --max-runtime SECS`, writes a structured
diagnostic to stderr, and exits with code 1.

By far the most common cause is an infinite loop: a `wh` body that
doesn't update its loop variable, or a recursion with no base case.
The mandelbrot persona run that surfaced this guard missed a
`col=col+1` increment and would have spun forever - the cap turns
that into a clear signal the agent can act on.

Override with `--max-runtime N` (seconds; 0 disables) when a
legitimate program needs longer. Long-running batch jobs and
training loops are the normal reason to bump or disable it.

```
ilo --max-runtime 300 main.ilo    -- allow 5 minutes
ilo --max-runtime 0   main.ilo    -- disable the cap
```
"#,
    },
    ErrorEntry {
        code: "ILO-R017",
        phase: Phase::Runtime,
        short: "stdout output budget exceeded",
        long: r#"## ILO-R017: stdout output budget exceeded

`ilo run` aborted because the program wrote more bytes to stdout
than the configured budget (default ~100 MB). Every `prnt` call in
every engine (tree, VM, Cranelift JIT) charges its output against
the budget; when the total exceeds `--max-output-bytes`, the next
write triggers a structured diagnostic to stderr and exits 1.

The most common cause is a loop calling `prnt` without termination
or without backing off: an unbounded `wh` body, a recursion with
no base case, or a missing increment on the loop variable. The
budget keeps a runaway from filling disk or the agent transcript
with megabytes of useless output before anyone notices.

Override with `--max-output-bytes N` (bytes; 0 disables) when a
legitimate program produces a lot of output - typically structured
data dumps, log replay, or a code-generation pipeline.

```
ilo --max-output-bytes 1073741824 main.ilo    -- raise to 1 GB
ilo --max-output-bytes 0 main.ilo              -- disable the cap
```
"#,
    },
    ErrorEntry {
        code: "ILO-R026",
        phase: Phase::Runtime,
        short: "panic-unwrap on Err / nil",
        long: r#"## ILO-R026: panic-unwrap on Err / nil

`func!! args` (panic-unwrap) aborts the program with exit code 1
when the called function returns `^e` (Err) or `nil` (Optional).
Unlike `func!`, which propagates the failure as the enclosing
function's return value, `!!` terminates immediately and writes
a `panic-unwrap: ...` diagnostic to stderr.

Use `!!` when the failure is unrecoverable in the current context:
short scripts, glue code, or main entry points where there is no
caller to react to the error. Use `!` instead when the caller
should observe and respond to the failure.

```
main>t;rdl!! "input.txt"    -- aborts with panic-unwrap if file missing
main>n;num!! "abc"          -- aborts with panic-unwrap: abc
```
"#,
    },
    ErrorEntry {
        code: "ILO-R099",
        phase: Phase::Runtime,
        short: "internal runtime error",
        long: r#"## ILO-R099: internal runtime error

A catch-all for unexpected runtime failures that don't map to a more
specific code. The diagnostic message carries the inner cause.

If you encounter this, it usually indicates a verifier gap or a bug
in a builtin — please file an issue with the source that triggers it.
"#,
    },
    // ── Engine-specific limitations ────────────────────────────────────────
    ErrorEntry {
        code: "ILO-E802",
        phase: Phase::Engine,
        short: "inline lambda exceeds 255-capture VM cap",
        long: r#"## ILO-E802: inline lambda exceeds 255-capture VM cap

The register VM encodes the capture count for `OP_MAKE_CLOSURE` in an
8-bit field, so an inline lambda whose body references more than 255
free variables from the enclosing scope cannot be compiled.

In practice this never trips on real code — a lambda capturing more
than 255 distinct outer names is almost certainly a programming
mistake. If you genuinely need that many, refactor the captured state
into a record and capture the single record value instead.

Phase 2 closure capture is otherwise fully supported across every
in-process engine (tree, VM, Cranelift JIT). This diagnostic only
fires on the pathological wide-capture case.
"#,
    },
    ErrorEntry {
        code: "ILO-T045",
        phase: Phase::Verify,
        short: "generic type variable used inconsistently or bound violated",
        long: r#"## ILO-T045: generic type variable inconsistency / bound violation
        code: "ILO-T044",
        short: "generic type variable used inconsistently or bound violated",
        long: r#"## ILO-T044: generic type variable inconsistency / bound violation

A function with explicit generic type parameters (`<a:comparable>`,
`<a:numeric>`, etc.) was called with arguments that either:

* used the **same type variable** with **different concrete types**, or
* violated the **bound** declared for that variable.

### Bounds

| Bound        | Permitted types |
|--------------|-----------------|
| `any`        | any type (default when no bound is written) |
| `comparable` | `n`, `t`, `b`   |
| `numeric`    | `n`             |
| `text`       | `t`             |

### Example — inconsistent usage

```
mn<a:comparable> x:a y:a>a
  r=x;>(x) y{r=y};r

mn 1 "two"   -- ILO-T044: 'a' bound to n then t
```

Fix: pass two values of the same type.

### Example — bound violation

```
add-one<a:numeric> x:a>a;+x 1

add-one "hello"   -- ILO-T044: text does not satisfy numeric
```

Fix: pass a numeric argument.
"#,
    },
];

/// Look up an error entry by code (e.g. `"ILO-T005"`).
pub fn lookup(code: &str) -> Option<&'static ErrorEntry> {
    REGISTRY.iter().find(|e| e.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_known_code() {
        let e = lookup("ILO-T005").expect("ILO-T005 should be in registry");
        assert_eq!(e.code, "ILO-T005");
        assert!(!e.short.is_empty());
        assert!(e.long.contains("ILO-T005"));
    }

    #[test]
    fn lookup_unknown_returns_none() {
        assert!(lookup("ILO-XXXX").is_none());
        assert!(lookup("").is_none());
    }

    #[test]
    fn all_codes_unique() {
        let mut codes: Vec<&str> = REGISTRY.iter().map(|e| e.code).collect();
        codes.sort_unstable();
        let len_before = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), len_before, "duplicate codes in registry");
    }

    #[test]
    fn all_codes_have_content() {
        for entry in REGISTRY {
            assert!(
                !entry.short.is_empty(),
                "{} missing short description",
                entry.code
            );
            assert!(
                !entry.long.is_empty(),
                "{} missing long description",
                entry.code
            );
        }
    }

    #[test]
    fn parse_code_roundtrip() {
        assert_eq!(parse_code("ILO-T004"), Some(('T', 4)));
        assert_eq!(parse_code("ILO-P101"), Some(('P', 101)));
        assert_eq!(parse_code("ILO-R026"), Some(('R', 26)));
        assert_eq!(parse_code("ILO-XXXX"), None);
        assert_eq!(parse_code("T004"), None);
        assert_eq!(parse_code(""), None);
    }

    #[test]
    fn registry_codes_in_documented_ranges() {
        for entry in REGISTRY {
            assert!(
                in_documented_range(entry.code),
                "{} not in any documented namespace range",
                entry.code
            );
        }
    }

    #[test]
    fn reserved_namespaces_have_room() {
        // Forward-looking namespaces should each span at least 100 codes so
        // future categories never have to renumber. This is a regression
        // guard against accidentally tightening a range.
        for ns in ['N', 'I', 'V', 'D', 'E', 'S'] {
            let total: u16 = NAMESPACE_RANGES
                .iter()
                .filter(|(l, _, _)| *l == ns)
                .map(|(_, lo, hi)| hi - lo + 1)
                .sum();
            assert!(
                total >= 100,
                "namespace {ns} has only {total} reserved codes; expected >= 100"
            );
        }
    }
}
