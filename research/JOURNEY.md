# The ilo Design Journey

How ilo's syntax evolved from idea to implementation — the explorations, benchmarks, and decisions that shaped the language.

## Syntax Variants

We explored 9 syntax variants before settling on the current design (idea9). Each idea explores a different syntax. Every folder has a SPEC and 5 example programs.

| Idea | Tokens | vs Py | Chars | vs Py | Score |
|------|--------|-------|-------|-------|-------|
| python-baseline | 871 | 1.00x | 3635 | 1.00x | — |
| [idea1-basic](explorations/idea1-basic/) | 921 | 1.06x | 3108 | 0.86x | 10.0 |
| [idea1-compact](explorations/idea1-compact/) | 677 | 0.78x | 2564 | 0.71x | 10.0 |
| [idea2-tool-calling](explorations/idea2-tool-calling/) | 983 | 1.13x | 3203 | 0.88x | 10.0 |
| [idea3-constrained-decoding](explorations/idea3-constrained-decoding/) | 598 | 0.69x | 2187 | 0.60x | 10.0 |
| [idea4-ast-bytecode](explorations/idea4-ast-bytecode/) | 584 | 0.67x | 1190 | 0.33x | 9.8 |
| [idea5-workflow-dag](explorations/idea5-workflow-dag/) | 710 | 0.82x | 2603 | 0.72x | 10.0 |
| [idea6-mcp-composition](explorations/idea6-mcp-composition/) | 956 | 1.10x | 2978 | 0.82x | 9.5 |
| [idea7-dense-wire](explorations/idea7-dense-wire/) | 351 | 0.40x | 1292 | 0.36x | 10.0 |
| [idea8-ultra-dense](explorations/idea8-ultra-dense/) | 285 | 0.33x | 901 | 0.25x | 10.0 |
| [idea9-ultra-dense-short](explorations/idea9-ultra-dense-short/) | 287 | 0.33x | 787 | 0.22x | 10.0 |

**Tokens** = total tokens across 5 examples (cl100k_base, comments stripped). **Chars** = total characters. **Score** = LLM generation accuracy /10 (claude-haiku-4-5, spec + all examples as context). See [test-summary.txt](explorations/test-summary.txt) for per-task breakdown.

### Key findings

- **Token count and accuracy are independent.** idea8 and idea9 achieve 0.33x the tokens of Python with perfect 10/10 accuracy. Terseness does not hurt generation quality when the spec is clear.
- **Prefix notation is a significant win.** Across 25 expression patterns: 22% fewer tokens, 42% fewer characters vs infix. See the [prefix-vs-infix benchmark](explorations/prefix-vs-infix/).
- **Positional args work.** We initially worried positional args would cause parameter-swap errors. Across all variants and task types, accuracy remained at 10/10. The concern was unfounded.
- **idea9 (ultra-dense-short)** was selected as the final syntax — lowest character count (0.22x Python) while maintaining perfect generation accuracy and the best token efficiency tied with idea8.

## Research Documents

| Document | Topic |
|----------|-------|
| [BUILDING-A-LANGUAGE.md](BUILDING-A-LANGUAGE.md) | How to build a language from scratch — parser approaches, implementation strategy |
| [TYPE-SYSTEM.md](TYPE-SYSTEM.md) | Type system design and decisions |
| [CONTROL-FLOW.md](CONTROL-FLOW.md) | Guards, match, loops — why no if/else |
| [DATA-MANIPULATION.md](DATA-MANIPULATION.md) | Data processing builtins and pipeline design |
| [LLM-INTEGRATION.md](LLM-INTEGRATION.md) | How ilo integrates with LLMs and AI agents |
| [D-AGENT-INTEGRATION.md](D-AGENT-INTEGRATION.md) | Agent framework integration design |
| [OPEN.md](OPEN.md) | Open design questions |
| [TODO.md](TODO.md) | Planned work |

## Other Research

| Document | Topic |
|----------|-------|
| [coding-agents-research.md](coding-agents-research.md) | Survey of coding agent architectures |
| [data-munging-languages.md](data-munging-languages.md) | How other languages handle data munging |
| [error-messages-research.md](error-messages-research.md) | Error message design across languages |
| [jit-backends.md](jit-backends.md) | JIT compilation backend options |
| [mcp-protocol-research.md](mcp-protocol-research.md) | MCP protocol for tool integration |
| [python-stdlib-analysis.md](python-stdlib-analysis.md) | Python stdlib coverage analysis |
| [shell-languages-research.md](shell-languages-research.md) | Shell language comparison |
