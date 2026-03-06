# Data Manipulation in ilo

How ilo stacks up against pandas + Python stdlib for data work. The goal isn't
to replicate pandas — it's to identify what primitives would make ilo viable for
the kind of data scripting AI agents actually do: reading CSV, filtering rows,
aggregating stats, reshaping, and emitting results.

---

## What Python gives you

### pandas core operations (by frequency of actual use)

| Operation | pandas | Python stdlib alternative |
|---|---|---|
| Read CSV | `pd.read_csv(path)` | `csv.DictReader(open(path))` |
| Read JSON | `pd.read_json(path)` | `json.load(open(path))` |
| Filter rows | `df[df.col > 0]` | `filter(lambda r: r['col'] > 0, rows)` |
| Select columns | `df[['a','b']]` | `[{k: r[k] for k in ['a','b']} for r in rows]` |
| Map values | `df.col.apply(fn)` | `[fn(r['col']) for r in rows]` |
| Group + aggregate | `df.groupby('k').agg({'v': 'sum'})` | `itertools.groupby` + manual |
| Sort | `df.sort_values('col')` | `sorted(rows, key=lambda r: r['col'])` |
| Join | `df.merge(other, on='id')` | manual dict lookup |
| Describe/stats | `df.col.mean()`, `.std()`, `.describe()` | `statistics.mean`, `statistics.stdev` |
| Unique values | `df.col.unique()` | `set(r['col'] for r in rows)` |
| String ops | `df.col.str.upper()`, `.str.contains()` | `str.upper()`, `in` |
| Datetime parse | `pd.to_datetime(df.col)` | `datetime.strptime` |
| Pivot/reshape | `df.pivot_table(...)` | nested dict manipulation |
| Write CSV | `df.to_csv(path)` | `csv.DictWriter` |
| Write JSON | `df.to_json(path)` | `json.dump` |

### The real workflow

A typical data script does this:

```python
import pandas as pd

df = pd.read_csv("report.csv")                      # read
df = df[df["amount"] > 0]                           # filter
df["amount_usd"] = df["amount"] / 100               # transform
summary = df.groupby("category")["amount_usd"].sum() # aggregate
print(summary.to_json())                             # output
```

That's 5 lines. The tokens are low because pandas does all the column-indexing,
type-coercion, and iteration implicitly.

---

## What ilo has today

### List primitives (apply to `L` = list of values)

| ilo | semantics |
|---|---|
| `map fn xs` | apply fn to each element |
| `flt fn xs` | filter by predicate |
| `fld fn xs init` | left fold / reduce |
| `len xs` | length |
| `hd xs` | first element |
| `tl xs` | rest of list |
| `rev xs` | reverse |
| `srt xs` | sort (natural order) |
| `srt fn xs` | sort by key function |
| `slc xs i j` | slice from i to j |
| `cat xs ys` | concatenate two lists |
| `zip xs ys` | pair elements → `L L any` |

### Record primitives (apply to `M` = map/dict)

| ilo | semantics |
|---|---|
| `mmap fn m` | apply fn to each value in record |
| `mget m key` | get value by key |
| `mset m key val` | set value, return new record |
| `mkeys m` | list of keys |
| `mvals m` | list of values |
| `mhas m key` | bool — key exists |
| `mdel m key` | remove key |
| `mmerge m1 m2` | merge two records |

### JSON (convert between `t` and structured values)

| ilo | semantics |
|---|---|
| `jpar s` | parse JSON string → value |
| `jdmp v` | dump value → JSON string |

### String primitives

| ilo | semantics |
|---|---|
| `spl sep s` | split string on separator → `L t` |
| `has sub s` | bool — substring present |
| `cat a b` | concatenate strings |
| `str n` | number → string |
| `num s` | string → number |
| `len s` | string length |

### Math

| ilo | semantics |
|---|---|
| `abs`, `min`, `max` | standard |
| `flr`, `cel`, `rnd` | floor, ceil, round |
| `+`, `-`, `*`, `/`, `%` | arithmetic |
| `pow`, `sqrt` | power, square root |

### I/O

| ilo | semantics |
|---|---|
| `get url` / `$url` | HTTP GET → `R t t` |
| `env key` | read env var → `R t t` |
| `rd path` | read file as string → `R t t` |
| `rdl path` | read file as list of lines → `R (L t) t` |
| `wr path s` | write string to file → `R t t` |
| `wrl path xs` | write list of lines to file → `R t t` |
| `prnt v` | print + passthrough |
| `jpar`/`jdmp` | JSON in/out |

---

## The gaps

### 1. File I/O — ✅ implemented

`rd`, `rdl`, `wr`, `wrl` are now builtins. All return `R _ t` for error handling.

```
rd path         → R t t          -- read whole file as string
rdl path        → R (L t) t      -- read file as list of lines
wr path s       → R t t          -- write string to file (overwrite)
wrl path xs     → R t t          -- write list of lines (joins with \n)
```

#### Subsection reading

Bash idioms like `tail -n 20 file`, `head -n 5 file`, `sed -n '10,20p' file`
compose from `rdl` + `slc` — no extra builtins needed:

```
-- last 20 lines (tail):
slc (rdl! path) -20 -1
-- first 5 lines (head):
slc (rdl! path) 0 5
-- lines 10-20 (sed -n '10,20p'):
slc (rdl! path) 10 20
```

### 2. String escapes — ✅ implemented

`\n`, `\t`, `\r`, `\"`, `\\` now work in string literals.

```
spl "\n" content    -- split file into lines
cat row "\n"        -- add newline to a row
"col1\tcol2"        -- tab-separated fields
```

### 3. `fmt` / string interpolation

Python f-strings are unavoidable in data work: `f"total: {n}"`. ilo has no
string formatting today.

Options:
```
fmt "total: {} rows: {}" n r    -- positional fmt (like Python str.format)
```

Or even simpler — just concatenation since ilo programs are short:
```
cat "total: " (str n)
```

The concatenation form adds tokens but stays within ilo's "one way to do things"
principle. `fmt` would be a meaningful save for multi-placeholder strings.

### 4. CSV parsing — `csv` builtin

The most common data format. Options:

**Option A: `csv` builtin** — reads CSV string → `L (L t)` (list of rows, each a list of fields):
```
parse-csv s:t>L L t; csv s
```

**Option B: composition** — with `\n` escape + `spl`:
```
-- parse CSV manually (works for simple cases, no quoting)
parse-row row:t>L t; spl "," row
parse-csv s:t>L L t; map parse-row (spl "\n" s)
```

Option B works for simple CSVs and requires no new builtin. A proper `csv`
builtin would handle quoted fields. For agent use, proper quoting matters.

### 5. Statistical aggregation

For data summaries, agents commonly need:

| operation | pandas | ilo gap |
|---|---|---|
| sum | `fld add xs 0` ✓ | — |
| count | `len xs` ✓ | — |
| mean | `sum/len` (2 lines) | no `mean` builtin |
| min/max | `fld min xs inf` or `min` on list? | `min`/`max` only take 2 args |
| sort by key | `sorted(xs, key=fn)` | ✅ `srt fn xs` |
| group by | manual fold | no `grp` builtin |

`min`/`max` with a list arg would eliminate the `fld` pattern for simple cases.

### 6. Sort by key — ✅ implemented

```python
sorted(rows, key=lambda r: r["amount"])
```

ilo: `srt fn xs` — sort by key function. Same name as `srt xs`, 2-arg form:

```
-- sort words by length
ln s:t>n;len s
srt ln words

-- sort records by numeric field
get-val r:L t>n;num r.1
srt get-val rows
```

Unlocks: "sort users by age", "sort transactions by amount" — core data ops.

### 7. String methods

| need | python | ilo gap |
|---|---|---|
| uppercase | `s.upper()` | no `upr` |
| lowercase | `s.lower()` | no `lwr` |
| trim whitespace | `s.strip()` | no `trm` |
| replace | `s.replace(a, b)` | no `rep` |
| starts with | `s.startswith(p)` | no `stw` |
| ends with | `s.endswith(p)` | no `enw` |
| pad/align | `s.ljust(n)` | no padding |

Most of these are low priority for agents (tools handle formatting) but `trm`
(trim) is needed whenever reading user input or file data.

### 8. `uniq` / dedup

```
uniq xs         -- remove duplicates, preserve order
uniqby fn xs    -- dedup by key
```

Common in data: deduplicate records by ID, find distinct categories.

---

## Priority ranking for ilo

Based on what agents actually need, in priority order:

| Priority | Feature | Effort | Impact |
|---|---|---|---|
| ✅ done | `\n`, `\t` string escapes | tiny — lexer fix | unlocks all file + CSV work |
| ✅ done | `rd path → R t t` | small | single most needed builtin |
| ✅ done | `rdl path → R (L t) t` | small | lines = data rows |
| ✅ done | `wr path s → R t t` | small | complete the I/O loop |
| ✅ done | `wrl path xs → R t t` | small | complete the I/O loop |
| ✅ done | `srt fn xs` — sort by key | medium | sort-by-key is essential |
| 🟠 P1 | `trm s` | tiny | needed for parsing real data |
| 🟡 P2 | `csv s` | medium | proper CSV with quoting |
| 🟡 P2 | `fmt template args…` | medium | removes str+cat boilerplate |
| 🟡 P2 | `mean`, `med`, `std` | small | data stats |
| 🟡 P2 | `uniq xs` | small | dedup |
| 🟢 P3 | `upr`, `lwr` | tiny | string normalization |
| 🟢 P3 | `rep old new s` | small | string substitution |
| 🟢 P3 | `grp fn xs` | medium | group-by without fold |

---

## What a viable data script looks like after P0+P1

```
-- count non-empty lines in a CSV
wc path:t>R n t
  lines=rdl! path
  flt (fn l:t>b;>len trm l 0) lines >> len

-- sum a column (CSV, col index 2)
sum-col path:t col:n>R n t
  lines=rdl! path
  rows=map (fn l:t>L t;spl "," l) lines
  vals=map (fn r:L t>n;num mget r col) rows
  ~fld (fn a:n b:n>n;+a b) vals 0

-- top-N by column
top path:t n:n>R L L t t
  lines=rdl! path
  rows=map (fn l:t>L t;spl "," l) lines
  sorted=srt (fn r:L t>n;num r.1) rows
  ~slc sorted 0 n
```

That's real data work — filtering, splitting, mapping, folding, sorting — in
compact ilo syntax. The only thing missing from today's ilo is `trm`.

---

## Assessment

**ilo for data scripting now (P0 done): 6/10** — file I/O and string escapes are
in. The core read → filter → transform → write loop works. Blockers removed.

**ilo for data scripting after P1: 7/10** — covers 80% of what agents do with
CSV/JSON data. Won't replace pandas for analytical work, but for agent pipelines
(read → filter → transform → emit), it's competitive with Python stdlib.

**Comparison to Python stdlib (no pandas):** Python without pandas is 5-6 lines
for the same work, but 2x the tokens. ilo with `rdl`+`srt fn xs`+`trm`+`\n` matches
Python's token density for data glue work.

**Not the goal:** replacing pandas for analytical/ML workloads. ilo targets agent
tool composition, not data science. The right ceiling is "can an agent read a file,
filter rows, and emit a result without reaching for Python?" — and P0+P1 gets
there.
