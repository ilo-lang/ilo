# Error Messages & Developer Diagnostics: Research Document

A comprehensive survey of programming languages and tools that are widely regarded as having the best error messages and developer diagnostics. This document is intended to guide the design and implementation of error reporting in ilo-lang.

---

## Table of Contents

1. [Language-by-Language Analysis](#language-by-language-analysis)
   - [Rust (rustc)](#1-rust-rustc)
   - [Elm](#2-elm)
   - [ReasonML / OCaml / ReScript](#3-reasonml--ocaml--rescript)
   - [Swift](#4-swift)
   - [Clang (C/C++)](#5-clang-cc)
   - [TypeScript](#6-typescript)
   - [Zig](#7-zig)
   - [Gleam](#8-gleam)
   - [Roc](#9-roc)
2. [Cross-Cutting Topics](#cross-cutting-topics)
   - [Error Anatomy](#error-anatomy)
   - [CLI Formatting](#cli-formatting)
   - [Error Recovery](#error-recovery)
   - ["Did You Mean?" Fuzzy Matching](#did-you-mean-fuzzy-matching)
   - [Runtime Errors & Stack Traces](#runtime-errors--stack-traces)
   - [LSP / IDE Integration](#lsp--ide-integration)
   - [Error Message Writing Guidelines](#error-message-writing-guidelines)
3. [Key Takeaways for ilo-lang](#key-takeaways-for-ilo-lang)
4. [Sources](#sources)

---

## Language-by-Language Analysis

### 1. Rust (rustc)

Rust is widely considered the gold standard for compiler error messages. Its diagnostics have evolved over a decade of deliberate, continuous effort across hundreds of contributors.

#### What Makes Rust Errors Good

- **Source-annotated labels**: Errors point directly at the relevant source code with colored underlines, showing both *what* is wrong and *why*.
- **Primary and secondary labels**: The primary label (red `^^^` underline) identifies the error itself; secondary labels (blue `---` underline) provide context (e.g., "first mutable borrow occurs here").
- **Error codes**: Each error has a stable code like `E0499` that can be explored with `rustc --explain E0499` for a full tutorial-style explanation.
- **Machine-applicable suggestions**: The compiler can suggest exact code fixes with varying confidence levels (`MachineApplicable`, `MaybeIncorrect`, `HasPlaceholders`, `Unspecified`).
- **Continuous refinement**: Even Rust 1.0.0 had solid error reporting, but every release has brought improvements. Color was added around 1.26.0. Error spans have been refined continuously through 1.87.0 and beyond.

#### Error Format (RFC 1644)

The current error format was designed in RFC 1644 and follows this structure:

```
error[E0499]: cannot borrow `foo.bar1` as mutable more than once at a time
  --> src/test/compile-fail/borrowck/borrowck-borrow-from-owned-ptr.rs:29:22
   |
28 |      let bar1 = &mut foo.bar1;
   |                      -------- first mutable borrow occurs here
29 |      let _bar2 = &mut foo.bar1;
   |                       ^^^^^^^^ second mutable borrow occurs here
30 |      *bar1;
31 |  }
   |  - first borrow ends here
```

Anatomy of this format:
- **Header line**: `error[E0499]: <message>` -- severity, error code, human-readable description.
- **Location pointer**: `-->` with file:line:col.
- **Line number gutter**: Numbers separated by a pipe `|` "wall" that visually separates line numbers from source code.
- **Primary label**: `^^^^^^^^` in red, answers "what is the error?"
- **Secondary labels**: `--------` in blue, answer "why is it an error?"
- **Elision**: Unannotated lines are elided (shown as `...`) after one blank line.

#### Expanded Error Format (`--explain`)

Rust also supports an expanded format that interleaves educational prose with the user's actual code:

```
error: cannot move out of borrowed content
   --> borrowck-move-out-of-vec-tail.rs:30:17

I'm trying to track the ownership of the contents of `tail`, which is
borrowed, through this match statement:

29  |              match tail {

In this match, you use an expression of the form [...]. When you do
this, it's like you are opening up the `tail` value and taking out its
contents. Because `tail` is borrowed, you can't safely move the contents.

30  |                  [Foo { string: aa },
    |                                 ^^ cannot move out of borrowed content

You can avoid moving the contents out by working with each part using a
reference rather than a move:

30  |                  [Foo { string: ref aa },
```

#### Key Design Principles

From the [Rust Compiler Development Guide](https://rustc-dev-guide.rust-lang.org/diagnostics.html):

- Write in plain, simple English understandable even when the reader is distracted.
- Messages should be "matter of fact" -- avoid capitalization and periods unless multiple sentences are needed.
- Surround code identifiers with backticks.
- Reduce the span to the smallest amount possible that still signifies the issue.
- Avoid multiple error messages for the same error.
- Never use "illegal" -- prefer "invalid" instead.
- Do not phrase suggestions as questions. Instead of "did you mean `Foo`?", say "there is a struct with a similar name: `Foo`".
- Remember that Rust's learning curve is steep, and compiler messages are an important learning tool.

#### How It Handles Different Error Types

| Error Type | Approach |
|---|---|
| **Syntax errors** | Points to exact location, suggests fixes (e.g., missing semicolons) |
| **Type errors** | Shows expected vs. found types with source annotations |
| **Borrow checker** | Shows lifetimes with multiple labeled spans across the source |
| **Suggestions** | `help:` lines with machine-applicable code patches |
| **Warnings** | Same format as errors but with yellow coloring |

---

### 2. Elm

Elm is famous for having the friendliest, most educational compiler error messages in any programming language. Elm's approach has directly inspired improvements in Rust, Gleam, Roc, and others.

#### What Makes Elm Errors Good

- **Conversational tone**: The compiler speaks in first person ("I ran into something unexpected") as if it is a collaborator, not an authority.
- **No jargon**: Terms like "token" and "identifier" are replaced with words like "name" or "value".
- **Educational by design**: Error messages are treated as opportunities to teach the language. The compiler explains not just what went wrong but why the language works the way it does.
- **Actionable suggestions**: Every error includes concrete steps to fix the issue.
- **Plain English**: Messages read like a helpful colleague explaining the problem.

#### Error Format

Elm errors follow a distinctive structure:

```
-- TYPE MISMATCH ------------------------------------------ src/Main.elm

The 1st argument to `viewUser` is not what I expect:

34|   viewUser "Alice"
              ^^^^^^^
This argument is a string of type:

    String

But `viewUser` needs the 1st argument to be:

    User

Hint: I see that `User` has a field called `name` that is a `String`.
Maybe you want to create a User record?
```

Key structural elements:
- **Error title**: ALL CAPS, descriptive category (e.g., `TYPE MISMATCH`, `NAMING ERROR`, `MISSING PATTERNS`).
- **File location**: Right-aligned on the header line.
- **Narrative explanation**: Written in first person, explaining the problem conversationally.
- **Source snippet**: Minimal, focused on the exact location.
- **Expected vs. found**: Clearly shows what types were expected and what was received.
- **Hints**: Separate section with additional guidance, often educational.

#### Syntax Error Reporting

Elm's syntax error system is built around structured error types that map to specific, helpful reports:

| Error Type | Report Title | Example Suggestion |
|---|---|---|
| `ModuleNameUnspecified` | MODULE NAME MISSING | "Add `module X exposing (..)` as the first line" |
| `ModuleNameMismatch` | MODULE NAME MISMATCH | "Change `module X` to match actual module name" |
| `UnexpectedPort` | UNEXPECTED PORTS | "Change to `port module` instead" |
| `FreshLine` | TOO MUCH INDENTATION | "Delete spaces before `module`" |

#### Key Design Principles

From Elm's error message philosophy and the [Caleb Mmer style guide](https://calebmer.com/2019/07/01/writing-good-compiler-error-messages.html):

- **The 80/20 rule**: 80% of the time developers know the error and need brevity. 20% of the time they are confused and need detail. Error messages must serve both.
- **Use present tense**: "I see" not "I found". The compiler reflects the current program state, not a discrete event.
- **Use first-person plural or singular**: "I ran into a problem" or "we see an error" -- personifies the compiler as a collaborator.
- **Write at a low reading level**: Use the Hemingway Editor or similar tools to keep language simple.
- **Use developer language**: Replace compiler-internal terms with words developers use when talking to each other.
- **Keep messages short**: 1-2 sentences is ideal, especially for IDE contexts.
- **Pinpoint the smallest possible location**: Red squiggles should guide developers to the exact point of failure.

---

### 3. ReasonML / OCaml / ReScript

The OCaml family of languages has improved its error messages significantly over time, particularly through the Reason and ReScript frontends.

#### What Makes Their Errors Notable

- **BetterErrors**: The Reason community created a project called BetterErrors that reformats OCaml's traditional error output into a more modern, readable format with colors, source snippets, and clearer wording.
- **Formatting over content**: The Reason frontend primarily changed the *formatting* of error messages rather than the content itself -- making the same information clearer through visual presentation.
- **Type error specialization**: OCaml's type inference engine produces detailed type mismatch errors, and the Reason/ReScript layer reformats these to be more digestible.

#### Traditional OCaml Errors vs. Improved Format

**Traditional OCaml:**
```
File "test.ml", line 5, characters 10-15:
Error: This expression has type string but an expression of type int was expected
```

**ReScript/Reason improved format:**
```
  We've found a bug for you!

  3 | let add = (a: int, b: int) => a + b;
  4 |
  5 | add("hello", 2)
          ^^^^^^^

  This has type: string
  But somewhere wanted: int
```

#### Design Tradeoffs

- **Clarity vs. precision**: Some participants in OCaml community discussions noted that "some clarity is lost by avoiding precise terminology." There is tension between making errors accessible and maintaining technical accuracy.
- **Tool compatibility**: Improved visual formatting can break GNU error conventions that tools rely on for parsing. The counterargument is that LSP makes CLI parsing less important.

---

### 4. Swift

Swift's compiler diagnostics are notable for their Fix-It system, which provides machine-applicable code corrections directly in Xcode.

#### What Makes Swift Errors Good

- **Fix-It suggestions**: The compiler not only identifies errors but proposes specific code changes. These can be applied with a single click in Xcode.
- **Contextual correctness**: Fix-It suggestions are syntactically and contextually correct -- not just string replacements but type-aware patches.
- **IDE-first design**: Swift's diagnostics are designed primarily for the Xcode IDE experience, with inline error markers, Fix-It buttons, and quick actions.
- **Compiler directives**: `#warning("message")` and `#error("message")` allow developers to create custom compile-time diagnostics.

#### Error Format

```
main.swift:5:15: error: binary operator '+' cannot be applied to operands of type 'String' and 'Int'
    let x = "hello" + 42
            ~~~~~~~ ^ ~~
main.swift:5:15: note: overloads for '+' exist with these partially matching parameter lists:
    (Int, Int), (String, String)
```

Fix-It example:
```
main.swift:3:12: error: expected ';' after expression
    let x = 5
               ^
               ;
```

#### Key Design Principles

- Fix-Its must be correct -- they should compile if applied.
- Errors show both the problematic location and related context (notes).
- Source ranges highlight the full extent of involved expressions.
- The diagnostic system is designed to integrate tightly with the IDE.

---

### 5. Clang (C/C++)

Clang fundamentally changed expectations for C/C++ compiler diagnostics. Before Clang, GCC's error messages were the industry standard -- and they were often cryptic. Clang's diagnostics were so good that GCC was forced to improve significantly (starting around GCC 5.0, maturing by GCC 8.0).

#### What Makes Clang Errors Good

- **Accurate column numbers**: Clang points at the exact problematic token, not just the general area where the parser noticed something wrong.
- **Source ranges**: Clang highlights the full extent of related expressions using `~~~~~` under the source line, showing which sub-expressions are involved.
- **Fix-It hints**: Concrete code transformations to fix the problem.
- **Typedef/alias preservation**: Clang shows user-defined type names (not expanded internal types) with an "aka" annotation when the underlying type matters.
- **Template type diffing**: For C++ template errors, Clang can show a tree diff of where two template types diverge, eliding common parts with `[...]`.
- **Color by default**: Clang was one of the first major compilers to use ANSI colors in its output.

#### Clang vs. GCC -- Real Examples

**Column accuracy (format strings):**
```
GCC:
  format-strings.c:91:5: warning: too few arguments for format
      printf("%.*d");
      ^

Clang:
  format-strings.c:91:13: warning: '.*' specified field precision
  is missing a matching 'int' argument
      printf("%.*d");
                ^
```

**Source range highlighting:**
```
GCC 4.9:
  t.c:7:39: error: invalid operands to binary + (have 'int' and 'struct A')
    return y + func(y ? ((SomeA.X + 40) + SomeA) / 42 + SomeA.X : SomeA.X);
                                        ^

Clang:
  t.c:7:39: error: invalid operands to binary expression ('int' and 'struct A')
    return y + func(y ? ((SomeA.X + 40) + SomeA) / 42 + SomeA.X : SomeA.X);
                         ~~~~~~~~~~~~~~ ^ ~~~~~
```

**Error recovery (missing semicolon):**
```
GCC 4.9:
  t.cc:4:8: error: invalid declarator before 'c'
   a<int> c;
          ^

Clang:
  t.cc:3:12: error: expected ';' after struct
  struct b {}
             ^
             ;
```

**Missing typename (cascading errors):**
GCC produces multiple cascading errors. Clang produces one error with a Fix-It:
```
  t.cc:1:26: error: missing 'typename' prior to dependent type name 'T::type'
  template<class T> void f(T::type) { }
                           ^~~~~~~
  typename
```

**Template type diffing:**
```
  Default:  vector<map<[...], float>> vs vector<map<[...], double>>

  Tree format:
    vector<
      map<
        [...],
        [float != double]>>
```

#### Key Design Principles

From the [Clang Diagnostics documentation](https://clang.llvm.org/diagnostics.html):
1. Pinpoint exactly what is wrong in the program.
2. Highlight related information so it is easy to understand at a glance.
3. Make the wording as clear as possible.
4. Provide Fix-It hints for small, localized problems.
5. Use colors to distinguish between different parts of the diagnostic.

---

### 6. TypeScript

TypeScript faces a unique challenge: its structural type system produces extremely complex type errors, especially with deeply nested generic types.

#### What Makes TypeScript Errors Notable

- **Detailed type comparison**: TypeScript tries to explain exactly where two types diverge in a structural comparison.
- **Error chains**: For nested type mismatches, TypeScript shows a chain of context ("Type X is not assignable to Type Y" -> "Property 'a' is missing" -> etc.).
- **Type alias preservation**: Since TypeScript 4.2, the compiler preserves type alias names in error messages rather than expanding them into their structural definitions. This makes errors dramatically more readable.

#### The Problem with TypeScript Errors

TypeScript errors are famously hard to read for complex types:

```
Type '{ name: string; age: number; email: string; }' is not assignable to type
'{ name: string; age: string; address: { street: string; city: string; }; }'.
  Types of property 'age' are incompatible.
    Type 'number' is not assignable to type 'string'.
  Property 'address' is missing in type '{ name: string; age: number; email: string; }'
    but required in type '{ name: string; age: string; address: { street: string; city: string; }; }'.
```

The full structural type is printed inline, which becomes unreadable for real-world types with many properties.

#### Community Solutions

- **pretty-ts-errors** (VS Code extension): Reformats TypeScript errors with syntax highlighting, clickable type declarations, and human-readable layout.
- **Custom error messages**: Library authors use conditional types and template literal types to produce custom error strings (e.g., `Type '"invalid"' is not assignable to type '"Expected a valid email address"'`).

#### How TypeScript Handles Different Error Types

| Error Type | Approach |
|---|---|
| **Syntax errors** | Points at exact location with clear message |
| **Type errors** | Shows full structural comparison with nested context |
| **Missing properties** | Lists missing properties with "did you mean?" for typos |
| **Generic constraints** | Shows constraint violation with the constraint type |

---

### 7. Zig

Zig's error diagnostics are notable for their compile-time evaluation traces and error return traces.

#### What Makes Zig Errors Good

- **Comptime stack traces**: When compile-time evaluation fails, Zig shows the full call chain through `comptime` evaluation with "note: called from here" annotations.
- **Error return traces**: Zig provides Error Return Traces that show the path an error took through the call stack. These are *not* stack traces -- they don't require unwinding. Instead, Zig maintains a hidden parameter that threads through failable function calls.
- **Precise location**: Column-accurate error pointing with `^` caret.
- **Custom `@compileError`**: Library authors can emit custom compile-time errors with domain-specific messages.

#### Error Format

```
src/main.zig:8:17: error: `uppr` must be all uppercase
    @compileError("`uppr` must be all uppercase");
    ^~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
src/main.zig:24:30: note: called from here
    const x = insensitive_eql("Hello", "hElLo");
              ~~~~~~~~~~~~~~~^~~~~~~~~~~~~~~~~~~
```

Error return trace example:
```
error: FileNotFound
/lib/std/os.zig:1378:13: 0x2082c6 in open (std)
/lib/std/fs.zig:754:34:  0x21a20a in openFile (std)
/src/main.zig:16:34:     0x22519a in main
```

#### Key Design Principles

- Blur the line between compile-time and runtime errors -- both get rich diagnostics.
- Error return traces are zero-cost at the call site (the cost is paid only by failable functions that maintain the trace node).
- Custom `@compileError` allows library authors to provide domain-specific error messages during compile-time evaluation.
- Reference traces can be controlled with `-freference-trace=N` to limit verbosity.

---

### 8. Gleam

Gleam is a newer language (for the Erlang VM and JavaScript) that has invested heavily in error message quality from the start.

#### What Makes Gleam Errors Good

- **Fault-tolerant compilation** (v1.2.0+): Instead of stopping at the first error, the compiler analyzes the entire module and returns all errors together. This is particularly important for LSP responsiveness during large refactors.
- **Cross-language syntax detection**: Gleam detects when users write syntax from other languages (e.g., `===` from JavaScript) and suggests the Gleam equivalent (`==`).
- **Contextual "Did you mean?"**: For unknown record fields, the compiler checks available fields in the type and suggests close matches.
- **Import suggestions**: When code references an unimported module, the compiler suggests which module to import, checking both the module name and whether it contains the value being accessed.
- **Pattern detection**: The compiler detects common mistakes like using `todo(message)` instead of `todo as "message"` and provides specific guidance.

#### Error Format

```
error: Unknown record field
  --> src/main.gleam:12:8
   |
12 |   user.nme
   |        ^^^ This field does not exist

Did you mean `name`?

The `User` type has these fields:
  - name: String
  - age: Int
  - email: String
```

```
error: Syntax error
  --> src/main.gleam:5:3
   |
 5 |   if condition {
   |   ^^ Gleam doesn't have if expressions

Hint: Use a case expression instead:
  case condition {
    True -> ...
    False -> ...
  }
```

#### Key Design Principles

- Errors should help people learning the language, not just experienced users.
- Detect and guide users coming from other languages.
- Fault-tolerant compilation allows returning multiple errors without cascading noise.
- Suggestions should be specific and actionable, not generic.

---

### 9. Roc

Roc is a very new language created by Richard Feldman (who previously worked extensively with Elm). Roc explicitly cites Elm as "the gold standard of the nicest compiler error messages, the friendliest compiler."

#### What Makes Roc Errors Good

- **Elm-inspired philosophy**: Friendly, educational, conversational tone.
- **Exhaustiveness checking**: The compiler tells you exactly which cases you missed in pattern matching.
- **Compile-time guarantees**: Type errors are caught before runtime with helpful context.
- **Error handling as values**: Roc uses tagged unions for errors, and the compiler enforces exhaustive handling.

#### Error Format

Roc follows a format similar to Elm's:

```
-- TYPE MISMATCH ---------------------------------------------- src/Main.roc

Something is off with the body of the `greet` definition:

5 |  greet = \name ->
6 |      "Hello, " + name
                    ^

This `+` operator works on numbers like Int and F64, but the values
here are:

    Str

Hint: To combine strings, use `Str.concat` instead.
```

#### Key Design Principles

- Friendly compiler messages are a core value, not an afterthought.
- The compiler should act as a teacher, especially for newcomers.
- Error messages should show the user's code, not abstract type representations.
- Suggestions should be concrete and directly applicable.

---

## Cross-Cutting Topics

### Error Anatomy

Every well-designed error message contains some subset of these elements:

#### Essential Elements

| Element | Description | Example |
|---|---|---|
| **Severity level** | Is this an error, warning, info, or hint? | `error:`, `warning:` |
| **Error code** | Stable identifier for lookup/documentation | `E0499`, `TS2322` |
| **Human-readable message** | Clear description of the problem | "cannot borrow as mutable more than once" |
| **Source location** | File, line, and column | `src/main.rs:29:22` |
| **Source snippet** | The actual code that triggered the error | The relevant lines of source |
| **Primary span** | Underline/highlight of the exact problematic code | `^^^^^^^^` |
| **Secondary spans** | Context that explains *why* | `--------` with labels |
| **Suggestion/Fix-It** | Concrete code change to fix the problem | `help: consider borrowing here: &foo` |
| **Notes** | Additional context or explanation | `note: first borrow occurs here` |
| **Hints** | Educational content or alternative approaches | `Hint: use Str.concat instead` |

#### The Hierarchy of Importance

1. **Location** -- Where is the problem? (most critical for experienced developers)
2. **What** -- What is the problem? (the primary label/message)
3. **Why** -- Why is this a problem? (secondary labels, context)
4. **How to fix** -- What should the developer do? (suggestions, Fix-Its)
5. **Learn more** -- Where can the developer learn about this? (error codes, `--explain`)

---

### CLI Formatting

How errors are visually presented in a terminal is just as important as the content.

#### Color Conventions

Most compilers have converged on these ANSI color conventions:

| Color | Usage |
|---|---|
| **Red** (bold) | Error severity, primary error highlights |
| **Yellow** (bold) | Warning severity, warning highlights |
| **Blue/Cyan** | Secondary labels, notes, informational context |
| **Green** | Suggestions, Fix-Its, "help" lines |
| **White/Bold** | Source code, file paths |
| **Dim/Gray** | Line numbers, gutters, decorative characters |

#### Structural Elements

```
error[E0499]: cannot borrow `foo.bar1` as mutable more than once at a time
^^^^^^         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
|              Human-readable message
Severity + Error code

  --> src/test/compile-fail/borrowck/borrowck-borrow-from-owned-ptr.rs:29:22
  ^^^                                                                  ^^^^^
  |                                                                    Line:Col
  Location arrow

   |
28 |      let bar1 = &mut foo.bar1;
   |                      -------- first mutable borrow occurs here
29 |      let _bar2 = &mut foo.bar1;
   |                       ^^^^^^^^ second mutable borrow occurs here
^  ^                       ^^^^^^^^ ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
|  |                       |        Label text
|  |                       Underline (primary ^^^ or secondary ---)
|  Gutter wall
Line number
```

#### Unicode vs. ASCII

Some compilers offer both ASCII and Unicode rendering modes:

- **ASCII mode**: Uses `|`, `-`, `^`, `+` for box drawing.
- **Unicode mode**: Uses `│`, `─`, `╭`, `╰`, `┬`, `└` for cleaner visuals.

GCC offers a `-fdiagnostics-text-art-charset=unicode` option for this. Most modern tools default to Unicode when the terminal supports it.

#### Key Formatting Principles

1. **Alignment matters**: Line numbers should be right-aligned in a fixed-width gutter.
2. **Visual separation**: Use a "wall" (pipe character) between the gutter and source code.
3. **Elision**: Skip unannotated lines to keep the error focused. Show `...` or a blank gutter line.
4. **Color fallback**: Always support `NO_COLOR` and `--color=never` for CI/piping/accessibility.
5. **Width awareness**: Wrap or truncate long lines sensibly. Consider terminal width.
6. **Consistent spacing**: Blank lines between separate errors. No blank lines within a single error.

---

### Error Recovery

Error recovery is the ability of a compiler to continue analyzing code after encountering an error, reporting multiple errors in a single pass without cascading noise.

#### Strategies

**1. Panic Mode Recovery**
The most common strategy. When the parser encounters an error, it discards tokens until it finds a synchronization point (semicolon, closing brace, keyword like `fn`, `let`, `struct`). Simple to implement, occasionally skips too much.

**2. Phrase-Level Recovery**
The parser performs local corrections to the remaining input -- replacing a prefix of the remaining input with something that allows parsing to continue. More precise than panic mode but harder to implement.

**3. Error Productions**
The grammar is augmented with "error productions" that match common mistakes (e.g., `=` instead of `==` in conditions). This allows the parser to recognize the mistake, report it, and continue with the corrected parse tree.

**4. Token Insertion/Deletion**
The parser inserts missing tokens (e.g., a semicolon) or deletes unexpected tokens to get back on track. Used by Clang for its missing-semicolon Fix-Its.

#### Cascading Error Prevention

A key challenge is preventing *cascading errors* -- where one real error causes the compiler to report dozens of false errors downstream. Strategies include:

- **Error dampening**: After reporting an error, suppress further errors in the immediate vicinity (e.g., Bison suppresses error messages until three consecutive tokens have been successfully shifted).
- **Poisoning**: Mark AST nodes as "error" nodes. When later analysis encounters an error node, it silently succeeds rather than reporting a new error.
- **Gleam's approach**: Analyze each definition independently so an error in one function does not cascade into errors in other functions.

#### Implementation Considerations

- Report at most N errors per file (common default: 20-50).
- Never report the same error twice.
- Track whether any errors have been reported; if so, do not proceed to code generation.
- Consider offering a `--error-limit=N` flag.

---

### "Did You Mean?" Fuzzy Matching

Suggestion algorithms are one of the most user-visible quality-of-life features in a compiler.

#### Algorithms

**Levenshtein Distance (Edit Distance)**
The most common algorithm. Counts the minimum number of single-character edits (insertions, deletions, substitutions) to transform one string into another.

- "nme" -> "name" = distance 1 (insertion)
- "tpye" -> "type" = distance 2 (transpositions as two edits)

**Damerau-Levenshtein Distance**
Extends Levenshtein to also count adjacent transpositions as a single edit.

- "tpye" -> "type" = distance 1 (transposition)

This is generally preferred for typo detection since transpositions are the most common typing mistake.

**Jaro-Winkler Similarity**
Gives higher scores to strings that match from the beginning. Useful when you want to prefer matches that start the same way (e.g., `get_user` vs. `get_users`).

#### Implementation Guidelines

- **Maximum distance threshold**: Typically allow edit distance of 1-2 for short identifiers, up to 3 for longer ones. A common heuristic: `max_distance = max(1, len(name) / 3)`.
- **Scope awareness**: Only suggest names that are in scope at the point of the error.
- **Type awareness**: When possible, only suggest names that would be type-correct in context (Gleam does this for module imports).
- **Ranking**: When multiple suggestions are within threshold, rank by edit distance first, then by scope proximity (local > module > imported).
- **BK-Trees**: For large symbol tables, a Burkhard-Keller tree allows O(log n) lookup of all strings within a given edit distance, rather than O(n) linear scan.
- **Avoid suggesting compiler internals**: Filter out internal/private symbols from suggestions.

#### Presentation

Different compilers present suggestions differently:

- **Rust**: "there is a struct with a similar name: `Foo`" (declarative, not a question)
- **Elm**: Lists close matches: "error1, floor, xor" (multiple options)
- **Gleam**: "Did you mean `name`?" (question form, single best match)
- **Racket**: Does not provide suggestions at all, reasoning that "students will follow well-meaning-but-wrong advice uncritically"

The Rust approach (declarative, not a question) is generally considered best practice because it avoids implying that the compiler knows the developer's intent.

---

### Runtime Errors & Stack Traces

While compile-time errors get the most attention, runtime error presentation is equally important.

#### Stack Trace Best Practices

1. **Show the user's code first**: The most relevant frame is usually in user code, not library/runtime code. Highlight or prioritize user frames.
2. **Source mapping**: For compiled/transpiled languages, map back to original source locations (JavaScript source maps, Python `.pyc` to `.py`).
3. **Collapse framework frames**: Allow collapsing/dimming frames from third-party libraries or the runtime itself.
4. **Show local variables**: When possible, display the values of local variables in each frame (Python tracebacks, Zig's safety checks).
5. **Reverse order**: Some languages show the most recent call first (Python), others show it last (Java). Most modern tools prefer most-recent-first.

#### Zig's Error Return Traces

Zig's approach is unique: instead of unwinding the stack when an error occurs, every failable function maintains a hidden `StackTraceNode` pointer that threads through the call chain. When an error propagates, Zig can show the complete path the error took through the program:

```
error: FileNotFound
/lib/std/os.zig:1378:13:  0x2082c6 in open (std)
/lib/std/fs.zig:754:34:   0x21a20a in openFile (std)
/src/main.zig:16:34:      0x22519a in main
```

This is an *error return trace*, not a stack trace. The call stack may have been completely unwound by the time the trace is displayed. The trace was built up incrementally as the error propagated.

---

### LSP / IDE Integration

Modern language tooling increasingly presents errors through the Language Server Protocol (LSP) rather than CLI output.

#### LSP Diagnostic Structure

The LSP `Diagnostic` object contains:

```json
{
  "range": { "start": {"line": 4, "character": 10}, "end": {"line": 4, "character": 15} },
  "severity": 1,
  "code": "E0499",
  "codeDescription": { "href": "https://doc.rust-lang.org/error_codes/E0499.html" },
  "source": "rustc",
  "message": "cannot borrow `foo.bar1` as mutable more than once at a time",
  "relatedInformation": [
    {
      "location": { "uri": "file:///src/main.rs", "range": {...} },
      "message": "first mutable borrow occurs here"
    }
  ],
  "tags": [],
  "data": { ... }
}
```

Key fields:
- **range**: Precise character-level span (not just line number).
- **severity**: 1=Error, 2=Warning, 3=Information, 4=Hint.
- **code**: Error code that can link to documentation.
- **relatedInformation**: Secondary spans with their own messages -- maps to Rust's secondary labels.
- **tags**: `Unnecessary` (for unused code dimming) or `Deprecated` (for strikethrough).
- **data**: Arbitrary data for code actions (Fix-Its).

#### Design Implications for Error Messages

When errors flow through LSP into an IDE:

1. **Messages must work standalone**: No ANSI colors, no source snippets, no line numbers. The IDE provides all visual context. The message text must make sense on its own.
2. **Short messages preferred**: IDE hover popups and problem panels have limited space. One to two sentences is ideal.
3. **Code actions replace Fix-Its**: Instead of textual suggestions, provide structured `CodeAction` objects that the IDE can apply automatically.
4. **Related information for context**: Use `relatedInformation` for secondary spans rather than embedding them in the message text.
5. **Error codes link to docs**: The `codeDescription.href` field lets IDEs show "learn more" links.

#### Dual Design Challenge

The fundamental tension: error messages must work well in *both* CLI and IDE contexts. Strategies:

- **Separate formatters**: Rust's `annotate-snippets` crate formats the CLI output, while the LSP layer sends structured data. The underlying diagnostic data model is the same.
- **IDE-first, CLI-enriched**: Design the core message for IDE contexts (short, standalone), then add source snippets and colors for the CLI rendering.
- **Progressive disclosure**: Short message in the IDE hover, full explanation available via error code link or `--explain` flag.

---

### Error Message Writing Guidelines

A synthesis of guidelines from Rust, Elm, Clang, Flow, Flix, and Racket.

#### Universal Rules

1. **Use plain, simple English.** Write for a developer who is distracted and scanning quickly.
2. **Be concise.** One to two sentences for the main message. Details go in notes/hints.
3. **Be specific.** Name the exact types, variables, and constructs involved.
4. **Be accurate.** Never blame the developer. Never guess wrong. If unsure, say so.
5. **Use backticks for code.** Surround identifiers, types, and code snippets with backticks.
6. **Avoid jargon.** Replace "token" with "character" or "symbol". Replace "identifier" with "name". Replace "expression" with "value".
7. **Never say "illegal".** Use "invalid", "unexpected", or "not allowed" instead.
8. **Use present tense.** "This value is a String" not "This value was found to be a String".
9. **Do not phrase suggestions as questions.** Instead of "Did you mean X?", say "There is a similar name: X" or "Hint: you can use X instead".
10. **State the constraint first, then what was found.** "Expected an Int, but found a String" is better than "Found a String where an Int was expected" (Racket's guideline).

#### Tone Guidelines

- **Friendly, not patronizing.** Treat the developer as a competent peer, not a student.
- **Neutral, not blaming.** "This function expects 2 arguments, but received 3" not "You passed too many arguments".
- **Matter-of-fact.** Avoid exclamation marks, emotional language, or humor in error messages.
- **Consistent voice.** Pick a voice (first person, second person, or impersonal) and stick with it throughout.

#### Formatting Guidelines

- **No capitalization** at the start of the message (Rust convention) unless it begins with a proper noun or code identifier.
- **No trailing period** for single-sentence messages. Use periods only when there are multiple sentences.
- **Use Markdown** in message text when the target supports it (backticks, bold for emphasis).
- **Curly quotes** ("like this") rather than straight quotes ('like this') for English text (Elm convention, optional).

#### The 80/20 Framework

Design every error message for two audiences simultaneously:

- **The 80% case**: An experienced developer who has seen this error before. They need: location, brief description, and maybe a suggestion. They will scan and fix in seconds.
- **The 20% case**: A developer encountering this error for the first time, possibly new to the language. They need: clear explanation, educational context, and concrete next steps.

The solution: short primary message (for the 80%), with optional notes/hints/`--explain` (for the 20%).

---

## Errors for Non-Human Consumers (LLMs, Agents, Tools)

ilo is designed as a language written and consumed by LLMs. This fundamentally changes what "good error output" means. Traditional compiler errors optimize for a human scanning terminal output. ilo errors must also optimize for an LLM reading stderr and deciding what to fix.

### Why This Matters

When an LLM writes ilo code and gets an error back, the error message is the **only feedback loop**. The LLM can't hover over squiggly lines, can't click "learn more", can't visually scan colored output. It reads plain text. The error must contain everything needed to make the correct fix in one shot.

### What LLMs Need from Errors

#### 1. Machine-Parseable Structure

LLMs benefit from consistent, predictable structure even more than humans do. A structured format like JSON or a rigid text format lets the LLM reliably extract location, error type, and suggestion:

```json
{
  "errors": [
    {
      "code": "ILO-T001",
      "severity": "error",
      "message": "type mismatch: expected `n`, found `t`",
      "file": "inline",
      "span": { "start": 14, "end": 17 },
      "line": 1,
      "col": 15,
      "source_line": "f x:n>n;+x \"hello\"",
      "label": "this is a `t` (text) value",
      "expected": "n",
      "found": "t",
      "suggestion": {
        "message": "convert text to number with `num`",
        "replacement": "num \"hello\""
      }
    }
  ]
}
```

A `--output=json` flag (or auto-detection when stdout is not a TTY) makes errors trivially parseable.

#### 2. The Full Source Context

Unlike a human who has the file open in an editor, an LLM may not have the source readily available. Errors should include:
- The **full source line** (not a truncated snippet)
- The **exact span** (byte offsets), not just line:col
- For multi-line issues, **all relevant source lines**

This lets the LLM reconstruct what it wrote and pinpoint where to edit.

#### 3. Actionable Fix Information

LLMs excel when given explicit instructions. The most useful error format for an LLM is:

```
error[ILO-T001]: type mismatch: expected `n`, found `t`
  at: col 15-17
  source: f x:n>n;+x "hello"
                  ^^^^^^^
  fix: convert text to number: replace `"hello"` with `num "hello"`
```

The `fix:` line is gold for LLMs. It's a direct instruction, not a hint. Where possible, provide:
- **What to replace** (the exact text)
- **What to replace it with** (the corrected text)
- **Why** (one phrase)

#### 4. Error Codes as Anchors

Stable error codes (`ILO-T001`, `ILO-P003`) let an LLM build up knowledge about error patterns across sessions. An LLM that has seen `ILO-T001` before can skip reading the full explanation and go straight to fixing.

#### 5. All Errors at Once

LLMs work best with batch feedback. If ilo stops at the first error, the LLM fixes it, re-runs, hits the next error, fixes it, re-runs — that's slow and expensive. Reporting **all errors in one pass** (with error recovery) lets the LLM fix everything at once.

#### 6. No Noise

Every line of output an LLM has to parse is context window it can't use for reasoning. Errors should be:
- **No ANSI escape codes** when not in a TTY (or with `--output=json`)
- **No decorative elements** (box drawing, color codes) in machine output
- **No repeated boilerplate** ("For more information, try...")
- **Deduped** — don't report cascading errors that stem from one root cause

### Dual-Mode Output Design

ilo should detect its audience and format accordingly:

| Signal | Audience | Format |
|--------|----------|--------|
| stdout is a TTY | Human | ANSI colors, source snippets, underlines, hints |
| stdout is piped / `--output=json` | Machine/LLM | Structured JSON, no decoration |
| `--output=text` | Human in CI | Plain text, no colors, but still formatted |

### The ilo Advantage

Because ilo source is ultra-dense (one line per function), errors can include the **entire function** in the error output without bloating:

```
error[ILO-T001]: type mismatch in `prc`: expected `n`, found `t`
  source: prc amt:n tax:n disc:t>n;s=*amt +1 tax;-s disc
                                                    ^^^^
  note: `disc` is declared as `t` (text) but `-` requires `n` (number)
  fix: change parameter type to `n`: `disc:n`
```

The entire function definition fits in the error. No scrolling, no "see line 47". This is a unique property of ilo's dense format — leverage it.

### Structured Diagnostic for Programmatic Consumption

Beyond LLMs, structured error output enables:
- **CI/CD integration**: Parse errors, count by severity, fail builds on specific codes
- **Editor plugins**: Feed JSON into any editor, not just LSP-compatible ones
- **Error aggregation**: Track which errors occur most frequently across a codebase
- **Auto-fix pipelines**: A tool reads errors with suggestions and applies fixes automatically

---

## ilo-Specific Design Principles

Beyond the general research above, ilo has unique characteristics that should shape its error design.

### 1. Dense Source = Show the Whole Function

ilo functions are typically one line. Error output can (and should) show the entire function definition, not just a snippet. This eliminates the "context problem" that plagues traditional compiler errors.

### 2. LLM-First, Human-Readable

The primary author of ilo code is an LLM. The primary consumer of ilo errors is also an LLM (in an agent loop). Human readability is important but secondary. This inverts the traditional priority: **machine parseability first, human aesthetics second**.

### 3. Prefix Notation Makes Spans Tricky

ilo uses prefix operators (`+x y`, `*a b`), which means spans for binary operations don't map neatly to the visual structure. The error renderer needs to handle this — pointing at `+` when the types of `x` and `y` don't match.

### 4. Verification is the Main Error Surface

ilo has a static verifier that catches type mismatches, undefined variables, arity errors, and exhaustiveness issues before execution. This is where most errors will occur. The verifier currently lacks span information entirely — adding spans here has the highest impact.

### 5. Wire Format Errors

When ilo source is transmitted as a "wire format" (dense form for LLM I/O), errors need to reference positions within that dense form. Since there's no file path, use `inline:1:15` or just `col 15`.

### 6. Error Budget

Given ilo's density, a single function might have 3-5 errors. Reporting all of them is fine. But for a file with 20 functions, each with errors, consider grouping by function and limiting to the most important errors first.

---

## Key Takeaways for ilo-lang

### Priorities (ordered by impact)

1. **Invest in source spans.** Track precise byte ranges for every AST node and thread them through compilation. This is the foundation for everything else — underlines, LSP ranges, Fix-Its, and JSON output.

2. **Dual-mode output from day one.** Structured JSON for LLM/agent/CI consumers (`--output=json` or auto-detect non-TTY). Rich ANSI-formatted text for humans in terminals. Same diagnostic data model underneath.

3. **Report all errors at once.** Implement error recovery so the compiler/verifier reports multiple errors per pass. LLMs fix faster with batch feedback. Humans waste less time on edit-compile cycles.

4. **Actionable fix suggestions.** Every error should include a concrete fix when possible — not as a hint, but as a "replace X with Y" instruction. Tag with confidence. This is the single highest-value feature for LLM consumers.

5. **Stable error codes.** Every error type gets a code (e.g., `ILO-T001`). Enables `--explain`, web lookup, LLM pattern recognition across sessions, and CI filtering.

6. **Show the whole function.** ilo's dense format means the entire function fits in the error. Use this advantage — no "see line 47", no scrolling, full context always visible.

7. **Get the CLI format right.** Adopt Rust-style annotated source (header + location arrow + gutter + labeled underlines) for human-facing output. This is the proven standard.

8. **Write messages for both audiences.** Plain English, specific about types and names, no jargon. But also structured enough that an LLM can extract the key info without parsing prose.

9. **Add "Did you mean?" suggestions.** Damerau-Levenshtein distance with threshold `max(1, len/3)`. Scope-aware. Present as statements, not questions.

10. **No noise.** Dedup cascading errors. No decorative boilerplate in machine output. Every line of error output should carry information.

### Architecture Recommendations

- **Diagnostic data structure**: Build a central `Diagnostic` struct with severity, code, message, primary span, secondary spans (with labels), suggestions (with applicability), and notes.
- **Rendering layer**: Create a formatter that takes a `Diagnostic` and renders it to either terminal (ANSI) or LSP JSON. Consider using or studying the [annotate-snippets](https://github.com/rust-lang/annotate-snippets-rs) crate for inspiration.
- **Source map**: Maintain a mapping from byte offsets to line/column positions. Store the original source text (or at least the relevant lines) for snippet rendering.
- **Error index**: Maintain a registry of all error codes with their documentation, similar to Rust's error index.

---

## Sources

### Language-Specific
- [Rust Compiler Development Guide - Diagnostics](https://rustc-dev-guide.rust-lang.org/diagnostics.html)
- [Rust RFC 1644 - Default and Expanded Errors](https://rust-lang.github.io/rfcs/1644-default-and-expanded-rustc-errors.html)
- [Evolution of Rust Compiler Errors (Kobzol)](https://kobzol.github.io/rust/rustc/2025/05/16/evolution-of-rustc-errors.html)
- [Helping with the Rust Errors (Sophia Turner)](https://www.sophiajt.com/helping-out-with-rust-errors/)
- [The Anatomy of Error Messages in Rust (RustFest)](https://rustfest.global/session/5-the-anatomy-of-error-messages-in-rust/)
- [Elm Syntax Error Reporting (DeepWiki)](https://deepwiki.com/elm/compiler/4.1-syntax-error-reporting)
- [Clang - Expressive Diagnostics](https://clang.llvm.org/diagnostics.html)
- [GCC vs Clang Diagnostics Comparison](https://easyaspi314.github.io/gcc-vs-clang.html)
- [Swift Compiler Diagnostics (Ole Begemann)](https://oleb.net/blog/2015/08/swift-compiler-diagnostics/)
- [TypeScript - Understanding Errors](https://www.typescriptlang.org/docs/handbook/2/understanding-errors.html)
- [Better Error Messages in TypeScript 4.2](https://dev.to/omril321/better-error-messages-in-typescript-4-2-smarter-type-alias-preservation-3j7)
- [pretty-ts-errors VS Code Extension](https://github.com/yoavbls/pretty-ts-errors)
- [Zig Language Overview](https://ziglang.org/learn/overview/)
- [Fault-Tolerant Gleam](https://gleam.run/news/fault-tolerant-gleam/)
- [Roc Language FAQ](https://www.roc-lang.org/faq)
- [Roc with Richard Feldman (Rust in Production Podcast)](https://corrode.dev/podcast/s05e04-roc/)

### Cross-Cutting
- [Writing Good Compiler Error Messages (Caleb Mmer)](https://calebmer.com/2019/07/01/writing-good-compiler-error-messages.html)
- [Error Message Style Guides of Various Languages (PyPy Blog)](https://pypy.org/posts/2021/12/error-message-style-guides.html)
- [Comparing Compiler Errors Across 8 Languages](https://www.amazingcto.com/developer-productivity-compiler-errors/)
- [Elm Error Messages - "Amazing, Informative, Paternalistic"](https://jamalambda.com/posts/2021-06-13-elm-errors.html)
- [annotate-snippets-rs (Rust Library)](https://github.com/rust-lang/annotate-snippets-rs)
- [LSP Specification 3.17](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
- [Error Detection and Recovery in Compilers (GeeksforGeeks)](https://www.geeksforgeeks.org/error-detection-recovery-compiler/)
- [ANSI Escape Codes (Wikipedia)](https://en.wikipedia.org/wiki/ANSI_escape_code)
- [GCC Diagnostic Message Formatting Options](https://gcc.gnu.org/onlinedocs/gcc/Diagnostic-Message-Formatting-Options.html)

---

## See Also

- [rust-capabilities-research.md](rust-capabilities-research.md) — discusses ILO-T024 exhaustiveness checking and ilo's error code system
- [CONTROL-FLOW.md](CONTROL-FLOW.md) — braceless guard section includes error hint design for ambiguous parses
