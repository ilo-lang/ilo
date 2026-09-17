# Skill-shelf eviction log

Every module growth past its recorded baseline (`bench/skill-token-baseline.json`)
must land a line here naming the module and the persona-transcript (or
measured-failure) artifact that justifies it. Criteria: the builtin/pattern
appears as a hand-rolled workaround in ≥ 3 independent persona transcripts,
each workaround costs > 40 tokens, and no composition of existing builtins
gets under that bar (see PLAN.md G2 / MANIFESTO principle 2).

Format (one per line):

`- <module> <+N tokens> — <reason> — transcript: <path>`
