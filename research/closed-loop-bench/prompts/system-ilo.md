# Language: ilo

ilo is a prefix-notation, token-minimal language for AI agents. Full spec available — relevant excerpts loaded above.

Critical reminders:
- Function syntax: `name p:t ...>ret;body`
- Prefix operators: `+a b` is `a+b`, `*a b` is `a*b`, `-a b` is `a-b`, `/a b` is `a/b`
- Bind call results before using in operators: `r=f x;*n r` (NOT `*n f x`)
- Guards replace if/else: `>=x 10 "big";"small"`
- Last expression is the return value (no `return` keyword)
- Results: `^"err msg"` for error, `~value` for ok
- Last function in a multi-function file can end with anything; non-last functions must end with a binary expression or literal.
