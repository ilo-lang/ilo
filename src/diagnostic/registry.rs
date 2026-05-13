/// An entry in the error code registry.
#[allow(dead_code)] // `short` is used by tooling; `long` is used by --explain
pub struct ErrorEntry {
    pub code: &'static str,
    pub short: &'static str, // brief description for tooling / --list-errors
    pub long: &'static str,  // full explanation for --explain
}

/// All stable error codes for the ilo language.
pub static REGISTRY: &[ErrorEntry] = &[
    // ── Lexer ────────────────────────────────────────────────────────────────
    ErrorEntry {
        code: "ILO-L001",
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
        short: "unexpected token",
        long: r#"## ILO-P003: unexpected token

A token was found where a different token was expected. The error
message names the expected and actual tokens.
"#,
    },
    ErrorEntry {
        code: "ILO-P004",
        short: "unexpected end of file",
        long: r#"## ILO-P004: unexpected end of file

The file ended before a required token was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P005",
        short: "expected identifier, got token",
        long: r#"## ILO-P005: expected identifier

An identifier (function name, variable name, parameter name) was
expected but a different token was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P006",
        short: "expected identifier, got end of file",
        long: r#"## ILO-P006: expected identifier, got end of file

The file ended before a required identifier was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P007",
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
        short: "expected type annotation, got end of file",
        long: r#"## ILO-P008: expected type annotation, got end of file

The file ended before a required type annotation was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P009",
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
        short: "expected expression, got end of file",
        long: r#"## ILO-P010: expected expression, got end of file

The file ended before a required expression was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P011",
        short: "expected pattern, got token",
        long: r#"## ILO-P011: expected pattern

A match pattern was expected but a different token was found.
Patterns include literals, `_` wildcard, type constructors (`Ok x`,
`Err e`, `true`, `false`), and record patterns.
"#,
    },
    ErrorEntry {
        code: "ILO-P012",
        short: "expected pattern, got end of file",
        long: r#"## ILO-P012: expected pattern, got end of file

The file ended inside a match expression before a pattern was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P013",
        short: "expected number literal, got token",
        long: r#"## ILO-P013: expected number literal

A numeric literal was required (e.g., for a list index `x.0`) but
a different token was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P014",
        short: "expected number literal, got end of file",
        long: r#"## ILO-P014: expected number literal, got end of file

The file ended before a required number literal was found.
"#,
    },
    ErrorEntry {
        code: "ILO-P015",
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
        short: "inline lambda captures outer scope",
        long: r#"## ILO-P017: inline lambda captures outer scope

Phase 1 inline lambdas — `(p:t>r;body)` passed to a HOF like `srt`, `map`,
`flt`, `fld`, `grp` — cannot close over variables from the enclosing function.
Every name referenced in the body must be a parameter of the lambda, a name
bound locally inside the lambda body, or a known top-level function/builtin.

**Wrong:**

    rank xs:L n threshold:n>L n
      srt (x:n>n;-x threshold) xs

The lambda references `threshold` from the enclosing scope.

**Fix A: use the HOF's ctx-arg form.** Every closure-aware HOF accepts an
optional context value that is threaded through every call:

    rank xs:L n threshold:n>L n
      srt (x:n c:n>n;-x c) threshold xs

**Fix B: define a top-level helper** that takes the value as a param and use
`srt fn ctx xs`:

    diff x:n c:n>n;-x c
    rank xs:L n threshold:n>L n;srt diff threshold xs

Closure capture is tracked as a Phase 2 follow-up; once it lands, free
variables will be captured by value automatically.
"#,
    },
    ErrorEntry {
        code: "ILO-P018",
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
    // ── Type / Verifier ──────────────────────────────────────────────────────
    ErrorEntry {
        code: "ILO-T001",
        short: "duplicate type definition",
        long: r#"## ILO-T001: duplicate type definition

A `type` declaration uses a name that was already defined.

**Fix:** rename one of the types or remove the duplicate.
"#,
    },
    ErrorEntry {
        code: "ILO-T002",
        short: "duplicate function/tool definition",
        long: r#"## ILO-T002: duplicate function or tool definition

A function or tool uses a name that was already defined in this file.

**Fix:** rename one of the functions or remove the duplicate.
"#,
    },
    ErrorEntry {
        code: "ILO-T003",
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
        short: "undefined variable",
        long: r#"## ILO-T004: undefined variable

A variable name was used that has not been bound in the current scope.
Variables are bound by `let` statements or function parameters.

**Example:**

    f x:n>n;+x y   -- 'y' is not defined

**Fix:** bind the variable before use, or pass it as a parameter:

    f x:n y:n>n;+x y
"#,
    },
    ErrorEntry {
        code: "ILO-T005",
        short: "undefined function",
        long: r#"## ILO-T005: undefined function

A function was called that is not defined in this file or as a builtin.

**Example:**

    f x:n>n;double x   -- 'double' is not defined

**Fix:** define the function, or correct the spelling.
"#,
    },
    ErrorEntry {
        code: "ILO-T006",
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
        short: "comparison operator type error",
        long: r#"## ILO-T010: comparison operator type error

A comparison operator (`<`, `>`, `<=`, `>=`, `=`, `!=`) was applied
to operands of mismatched or non-comparable types.

Comparisons require both operands to be the same type (`n` or `t`).
"#,
    },
    ErrorEntry {
        code: "ILO-T011",
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
        short: "negate type error",
        long: r#"## ILO-T012: negate type error

Unary negation (`-x`) requires a numeric argument.

**Example:**

    f s:t>n;-s   -- cannot negate a text value
"#,
    },
    ErrorEntry {
        code: "ILO-T013",
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
        short: "foreach collection type error",
        long: r#"## ILO-T014: foreach collection type error

The `foreach` builtin requires a list as its first argument.

**Example:**

    f s:t>n;foreach s x;x   -- 's' is 't', not a list
"#,
    },
    ErrorEntry {
        code: "ILO-T015",
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
        short: "record unknown field",
        long: r#"## ILO-T016: record unknown field

A record literal or `with` expression includes a field name that
does not exist on the type.

**Fix:** remove the extra field or correct the spelling.
"#,
    },
    ErrorEntry {
        code: "ILO-T017",
        short: "record field type mismatch",
        long: r#"## ILO-T017: record field type mismatch

A field in a record literal was given a value of the wrong type.

**Fix:** ensure the value matches the field's declared type.
"#,
    },
    ErrorEntry {
        code: "ILO-T018",
        short: "field access on non-record type",
        long: r#"## ILO-T018: field access on non-record type

A field access (`value.field`) was attempted on a value that is not
a record type. Field access is only valid on named `type` instances.
"#,
    },
    ErrorEntry {
        code: "ILO-T019",
        short: "field not found on type",
        long: r#"## ILO-T019: field not found on type

A field name used in a field access expression (`value.name`) does
not exist on the record type.

**Fix:** correct the field name spelling.
"#,
    },
    ErrorEntry {
        code: "ILO-T020",
        short: "'with' on non-record type",
        long: r#"## ILO-T020: 'with' on non-record type

The `with` expression for updating record fields requires a record value.
"#,
    },
    ErrorEntry {
        code: "ILO-T021",
        short: "'with' field not found",
        long: r#"## ILO-T021: 'with' field not found

A field name used in a `with` expression does not exist on the record type.
"#,
    },
    ErrorEntry {
        code: "ILO-T022",
        short: "'with' field type mismatch",
        long: r#"## ILO-T022: 'with' field type mismatch

A value provided in a `with` expression has the wrong type for the field.
"#,
    },
    ErrorEntry {
        code: "ILO-T023",
        short: "index access on non-list type",
        long: r#"## ILO-T023: index access on non-list type

A list index access (`value.0`) was attempted on a non-list value.
"#,
    },
    ErrorEntry {
        code: "ILO-T024",
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
        short: "unreachable code",
        long: r#"## ILO-T029: unreachable code

Code after a `ret` (early return) or `brk` (break) statement will
never be executed.

**Example:**

    f x:n>n;ret x;*x 2   -- '*x 2' is unreachable

**Fix:** remove the unreachable code or move it before the `ret`/`brk`.
"#,
    },
    // ── Warnings ─────────────────────────────────────────────────────────────
    ErrorEntry {
        code: "ILO-W001",
        short: "guard without else inside loop (retired)",
        long: r#"## ILO-W001: guard without else inside loop (retired)

This warning has been retired. Braced guards `cond{body}` are now
conditional execution (no early return), making them safe inside loops.
Use braceless guards `cond expr` for early return, or `ret` inside
braced guards for explicit early return from loops.
"#,
    },
    // ── Runtime ──────────────────────────────────────────────────────────────
    ErrorEntry {
        code: "ILO-R001",
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
        short: "undefined function at runtime",
        long: r#"## ILO-R002: undefined function at runtime

A function was called that is not defined. This should normally be
caught by the verifier (ILO-T005).
"#,
    },
    ErrorEntry {
        code: "ILO-R003",
        short: "division by zero",
        long: r#"## ILO-R003: division by zero

A division operation (`/`) was performed with a zero divisor.

**Fix:** check that the divisor is non-zero before dividing.
"#,
    },
    ErrorEntry {
        code: "ILO-R004",
        short: "runtime type error",
        long: r#"## ILO-R004: runtime type error

An operation was applied to a value of the wrong type at runtime.
This may indicate a verifier gap for a dynamic code path.
"#,
    },
    ErrorEntry {
        code: "ILO-R005",
        short: "field not found at runtime",
        long: r#"## ILO-R005: field not found at runtime

A field access was performed on a record that does not have the
requested field. Normally caught statically (ILO-T019).
"#,
    },
    ErrorEntry {
        code: "ILO-R006",
        short: "list index out of bounds",
        long: r#"## ILO-R006: list index out of bounds

A list index access used an index that is out of the list's range.
ilo lists are zero-indexed.

**Fix:** check `len` before indexing into the list.
"#,
    },
    ErrorEntry {
        code: "ILO-R007",
        short: "foreach requires a list",
        long: r#"## ILO-R007: foreach requires a list

The `foreach` builtin was given a non-list value at runtime.
Normally caught statically (ILO-T014).
"#,
    },
    ErrorEntry {
        code: "ILO-R008",
        short: "'with' requires a record",
        long: r#"## ILO-R008: 'with' requires a record

The `with` expression was applied to a non-record value at runtime.
Normally caught statically (ILO-T020).
"#,
    },
    ErrorEntry {
        code: "ILO-R009",
        short: "builtin argument error at runtime",
        long: r#"## ILO-R009: builtin argument error at runtime

A builtin function received the wrong type of argument at runtime.
Normally caught statically (ILO-T013).
"#,
    },
    ErrorEntry {
        code: "ILO-R010",
        short: "compile error: undefined variable",
        long: r#"## ILO-R010: compile error: undefined variable

The VM compiler encountered an undefined variable while compiling a function.
Normally caught statically (ILO-T004) before compilation.
"#,
    },
    ErrorEntry {
        code: "ILO-R011",
        short: "compile error: undefined function",
        long: r#"## ILO-R011: compile error: undefined function

The VM compiler encountered an undefined function reference.
Normally caught statically (ILO-T005) before compilation.
"#,
    },
    ErrorEntry {
        code: "ILO-R012",
        short: "no functions defined",
        long: r#"## ILO-R012: no functions defined

The program has no callable functions. At least one function
must be defined to run a program.
"#,
    },
    ErrorEntry {
        code: "ILO-R013",
        short: "internal VM error",
        long: r#"## ILO-R013: internal VM error

The virtual machine encountered an unexpected internal state,
such as an unrecognised opcode. This indicates a compiler bug,
not a user error.

If you see this, please file a bug report.
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
}
