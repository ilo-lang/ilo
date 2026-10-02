---
name: ilo-excerpt-compact
description: Aggressively small ilo excerpt for artefact closed-loop (exp-08). Entire language surface in ~4k chars — prefer over curated skills modules.
---

INTRO: ilo is a prefix-notation language for AI agents. Every token is counted: generation + retries + context loading. This spec is the entire language in under 3K tokens.

SYNTAX: Function: f x:n y:t>type;body. Params: name:type. Return: >type. Body: semicolon-separated, last expr returns. No return keyword. Script mode: bare statements auto-wrap in main. Comment: --.

TYPES: n=number(f64) t=text(string) b=bool _=any. L n=list R n t=result O n=optional M t n=map. Type after : in params, after > in return.

OPERATORS: Prefix preferred: +a b *a b -a b /a b >a b =a b !b. Nest outer-first: +*a b c=(a*b)+c. Infix available: a + b. Append: +=. Pipe: >>. Nil-coalesce: ?? x default (O only). Result unwrap: default-on-err r d (R only). Negate: !.

SPACING: Every token needs whitespace. 90"A" fails, write 90 "A". )2 fails, write ) 2. }rs fails, write } rs. This is the most common error.

PRINT: prnt x. fmt "{}" args. fmt2 value decimals (fmt2 3.14159 2 = 3.14). fmt "{:.1f}" x for 1 decimal.

LISTS: [1 2 3]. map {x> body} xs. flt {x> body} xs. fld {a x> body} xs init. sum xs. len xs. at xs i (0-indexed, negative from end). range start end. rev xs. srt xs. take n xs. slc xs start end. zip xs ys. flat xs.

HOF LAMBDAS: Brace form {x> body} — bare param, types inferred. Paren form (x:n>n;body) — typed. Use brace form for map/flt/fld. Space after > in braces: {x> >x 0} not {x>>x 0}.

MAPS: mmap (empty). mset m k v. mget m k (nil on miss). mget-or m k default. mhas m k. mkeys m. mvals m. mpairs m.

STRINGS: spl s sep. cat xs sep (join list). + a b (concat text). len s. has s sub. trm s. upr s. lwr s. num s (text to R n t). str x (to text).

MATH: abs min max mod fmod flr cel rou. rou x digits rounds to N dp. clamp x lo hi. pow b e. sqrt. Constants: pi tau e.

CONTROL FLOW: Ternary: ?h cond then else. Match: ?x{"lit":body;42:body;~v:ok-bind;^e:err-bind;_:else}. Loop: @x xs{body}. Early return: guard cond val (exits function). ret val (exits from any depth). Break: brk. Continue: cnt. Cleanup: defer expr (runs when the enclosing block exits — normal, ret, brk, cnt or error), errdefer expr (error exit only). Both are block-scoped and fire once per block entry, LIFO within a block; reserved, cannot be binding names.

RESULT: R ok-type err-type. Ok: ~v in match. Err: ^e in match. Unwrap mid-body: v=call!. Default: default-on-err (num s) 0. nil-coalesce: ?? opt-val default (O type only, not R).

ENV: env key > R t t. env-or key default > t. env-all > R (M t t) t.

FILES: rd path > R t t. rdl path > R (L t) t. wr path content. rdb path. wrl path lines.

NAMING: Lowercase only, 1-3 chars preferred. Identifiers: [a-z][a-z0-9]*(-[a-z0-9]+)*. Hyphen joins segments: env-or. No capitals, no underscores in bindings. All 1-3 char lowercase names are likely reserved builtins. Use 4+ chars for locals: total, avg-v, count, name, result.

KEY BUILTINS (3-char): abs at avg b64 cat cel cos det dot env exp fld flt fmt fmod frq get grp has hdr inv len log lst lwr map max min mod now num ord pi pow pst put rdb rdl rev rgx rng rnd rou run sin slc spl srt str sum tan tau tl trm unq upr wra wrl wro zip. Also reserved: len sum map flt fld srt rev at hd tl pi rd wr ct. Use 4+ chars to avoid collisions.

ERRORS: Diagnostics are JSON with code (ILO-XXXX), message, suggestion. Fix plans carry before/after edits. The repair loop sees the error and the fix plan. Common: ILO-T004 undefined variable, ILO-T006 arity mismatch, ILO-P003 parse error, ILO-P011 reserved name (now allowed as binding — shadows builtin in value position).

ENGINE: Tree interpreter (default). Register VM + Cranelift JIT for hot paths. ilo build produces native binary. Functions with closures (map/flt/fld) run on VM (JIT bails). All engines pass the same test suite.

EXAMPLES:
Triangular: main n:n>n;s=*n +n 1;/s 2;s
Pipeline: sum flt {x> >x 10} map {x> *x x} (range 1 6)
Safe divide: div a:n b:n>n;=b 0 0;/a b
Read env: prnt env-or "HOME" "/tmp"
Format: prnt fmt "x={:.2f}" 3.14159
Match: ?x{"a":1;"b":2;_:0}
Loop: @i range 0 10{prnt i}
Grade: ?h >=x 90 "A" ?h >=x 80 "B" "C"
