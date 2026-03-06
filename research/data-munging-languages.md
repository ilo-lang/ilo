# Data Munging Languages: What Makes Them Concise

> An analysis of awk, sed, jq, Perl, Ruby, R/dplyr, and Miller for language designers.
> Focus: which design decisions produce conciseness, and which transfer to a new language.

## Executive Summary

The tools covered here share a common architectural insight: **the program is a
transformation pipeline, not a sequence of instructions**. The most token-efficient
tools achieve conciseness through four mechanisms:

1. **Implicit iteration** — the runtime loops over records; the programmer writes only
   the per-record logic
2. **Implicit field binding** — structured fields are addressable without parsing
   boilerplate
3. **Pipeline composition** — transformations chain without intermediate variable names
4. **Type-aware defaults** — sensible defaults for delimiters, output formats, numeric
   coercion

Token cost vs Python for "sum a column by group":

| Tool | Relative tokens | Best domain |
|------|----------------|-------------|
| awk | 0.10–0.15x | Line-oriented text, TSV/CSV |
| sed | 0.05–0.10x | Stream substitution, line selection |
| jq | 0.15–0.25x | JSON transformation |
| Perl | 0.12–0.20x | Regex-heavy, mixed text/binary |
| Ruby | 0.20–0.35x | OO data pipelines, CSV |
| R/dplyr | 0.20–0.30x | Tabular/statistical data |
| Miller | 0.10–0.18x | Named-field CSV/TSV/JSON |
| pandas | 1.0x | Baseline |

---

## 1. awk

### Design: Pattern/Action over Implicit Records

awk's implicit loop iterates over *records* (lines by default). Each rule is
`pattern { action }`. If there is no pattern, the action runs on every record. If
there is no action, the matching record is printed. The programmer writes nothing
about opening files, splitting fields, or looping.

**Key built-in variables:**

| Variable | Meaning |
|----------|---------|
| `NR` | Records read so far |
| `NF` | Fields in current record |
| `FS` | Input field separator (default: whitespace) |
| `$0` | Entire current record |
| `$1..$NF` | Individual fields |

**Implicit coercion:** `"42" + 0` is `42`. Eliminates explicit casting.

**Auto-initialized accumulators:** `count[$1]++` just works — no declaration, no
default value. This is awk's single biggest feature for aggregation.

### The 5 Idioms

```awk
# 1. Read (print all)
awk '1' file.txt

# 2. Filter
awk '/error/' log.txt               # regex pattern — action defaults to print
awk '$3 > 100' data.tsv             # numeric condition
awk 'NR>1' data.csv                 # skip header
awk 'NR>=10 && NR<=20' file.txt     # line range

# 3. Transform
awk 'BEGIN{OFS=","} {print $2,$1,$3}' data.tsv
awk '{print $0, $2*$3}' prices.tsv  # add computed column

# 4. Aggregate
awk '{sum[$1]+=$2} END{for(k in sum) print k,sum[k]}' data.tsv  # group sum
awk '{s+=$2} END{print s}' data.tsv                               # grand total
awk '{count[$1]++} END{for(k in count) print count[k],k}' data.tsv

# 5. Write
awk '{print > $1".txt"}' data.tsv   # split output by field
awk 'BEGIN{FS="\t";OFS=","} {$1=$1;print}' data.tsv  # TSV→CSV
```

### Why It's Concise

| Mechanism | Token savings vs Python |
|-----------|------------------------|
| Implicit `for record in records` | Saves ~3 lines per program |
| `$n` field access | `row.split('\t')[n-1]` → `$n` (saves 5+ tokens) |
| Uninitialized vars are 0/empty | No `setdefault`, no `defaultdict` |
| Pattern-as-guard | No explicit `if re.match(...)` |
| `BEGIN`/`END` | Clean setup/teardown without boilerplate |

---

## 2. sed

### Design: Stream Substitution

sed applies editing commands to each line. Its conciseness comes from extreme
specialization — it does one thing (substitution + line selection) with minimal syntax.

```bash
# 1. Read (pass through)
sed '' file.txt

# 2. Filter (select lines)
sed -n '/pattern/p' file.txt        # print matching
sed '/pattern/!d' file.txt          # delete non-matching
sed -n '10,20p' file.txt            # line range
sed '1d' data.csv                   # skip header
sed -n '/START/,/END/p' file.txt    # between patterns

# 3. Transform (substitution)
sed 's/foo/bar/g' file.txt          # global replace
sed -E 's/(first) (last)/\2, \1/' names.txt  # capture groups
sed 's/[[:space:]]*$//' file.txt    # trim trailing whitespace
sed -i '' 's/old/new/g' file.txt    # in-place edit (BSD)

# 4. Aggregate — sed is weak here; no arithmetic

# 5. Write
sed -n '/pattern/w output.txt' file.txt
sed -i.bak 's/foo/bar/g' file.txt   # in-place with backup
```

### Why It's Concise

- Address prefix: `10,20s/foo/bar/` vs `if 10 <= lineno <= 20: line.replace(...)`
- In-place edit: read → transform → write in one flag
- Pattern + implicit print: the most common operation (extract matching lines) is zero tokens beyond the pattern

### Limitation

No field awareness, no arithmetic. sed is not a data language — it is a stream
editor. Include it here only as the extreme of line-based conciseness.

---

## 3. jq

### Design: Filter Pipeline over JSON

Every jq expression is a **filter**: takes JSON input, produces JSON output. Filters
compose with `|`. The identity `.` passes through unchanged. This creates a deeply
consistent compositional model.

**Core primitives:**

| Syntax | Meaning |
|--------|---------|
| `.field` | Object field access |
| `.[n]` | Array index |
| `.[]` | Iterate array/object (implicit flatMap) |
| `\|` | Pipe (sequential composition) |
| `,` | Parallel output (fan-out) |
| `select(expr)` | Filter — keep if expr truthy |
| `map(expr)` | Apply filter to each element |
| `group_by(.f)` | Group array by field |
| `..` | Recursive descent |

### The 5 Idioms

```bash
# 1. Read
jq '.name' data.json                           # extract field
jq '{name: .name, age: .age}' data.json        # restructure
jq -r '.name' data.json                        # raw string (no JSON quotes)
jq '.users[].address.city' data.json           # nested access

# 2. Filter
jq '.[] | select(.age > 25)' users.json
jq '[.[] | select(.active == true)]' users.json
jq '.[] | select(.email != null)' users.json

# 3. Transform
jq '.[] | {id: .user_id, label: .name}' data.json
jq '.[] | . + {full_name: (.first + " " + .last)}' users.json
jq -r '.[] | "\(.name): \(.score)"' data.json  # string interpolation

# 4. Aggregate
jq '[.[].score] | add' data.json               # sum
jq '. | length' array.json                     # count
jq 'group_by(.dept) | map({dept: .[0].dept, total: map(.salary) | add})' data.json
jq '[.[].category] | group_by(.) | map({(.[0]): length}) | add' data.json  # freq count

# 5. Write
jq -r '.[] | [.name, .age] | @csv' data.json   # CSV output
jq -r '.[] | [.name, .age] | @tsv' data.json   # TSV output
jq -c '.[]' array.json                          # NDJSON (one object per line)
```

### Why It's Concise

| Mechanism | Savings |
|-----------|---------|
| `.field` access | `obj['field']` → `.field` (saves 2 tokens, no quotes) |
| `.[]` implicit iteration | No `for item in data:` (saves 1 line) |
| `select()` inline | No separate `filter()` or list comprehension |
| `map()` without lambda | `map(.score * 2)` vs `[x['score']*2 for x in data]` |
| `group_by` + `map` | Replaces 10+ lines of Python defaultdict |

### Weakness

Token cost rises sharply for joins between two files and for named accumulator state
across multiple passes. `reduce` syntax is verbose.

---

## 4. Perl

### Design: awk-Compatible One-Liners with Full Regex

Perl borrowed awk's implicit loop (`-n`/`-p`), sed's regex, and added hash data
structures and full programmability. Conciseness comes from `-lane` flags + `%hash`
aggregation + terse regex.

**Key flags:** `-n` (read loop, no print), `-p` (read loop + print), `-a` (auto-split
into `@F`), `-F/pat/` (field separator), `-l` (auto-chomp + newline), `-i[ext]`
(in-place edit). Built-in vars: `$.` = line number, `$_` = current line, `@F` =
split fields.

### The 5 Idioms

```perl
# 1. Read
perl -ane 'print $F[0], "\n"' file.txt      # first field
perl -F: -ane 'print $F[2], "\n"' /etc/passwd

# 2. Filter
perl -ne 'print if /pattern/' file.txt
perl -ne 'print unless /pattern/' file.txt
perl -lane 'print if $F[2] > 100' data.tsv
perl -ne 'print if 10..20' file.txt         # flip-flop line range

# 3. Transform
perl -pe 's/foo/bar/g' file.txt
perl -lane 'print join("\t", $F[1], $F[0], @F[2..$#F])' data.tsv
perl -pe 's/(?<last>\w+), (?<first>\w+)/$+{first} $+{last}/' names.txt

# 4. Aggregate
perl -lane '$s += $F[1]; END{print $s}' data.tsv
perl -lane '$h{$F[0]} += $F[1]; END{print "$_ $h{$_}" for keys %h}' data.tsv
perl -ne 'print unless $seen{$_}++' file.txt   # deduplicate

# 5. Write
perl -i.bak -pe 's/old/new/g' file.txt
```

### vs awk

Perl's `-lane` matches awk almost exactly. Perl adds: full regex engine, real data
structures (HoA, HoH), CPAN modules. Cost: `$F[0]` vs `$1` (neutral), slightly more
verbose for pure numeric work.

---

## 5. Ruby

### Design: Enumerable + Method Chaining

Ruby's `Enumerable` mixin provides `map`, `select`, `reject`, `reduce`, `group_by`,
`each_with_object`, `flat_map`, `tally`, `min_by`, `max_by`, `sort_by`, `zip`. These
are available on `Array`, `Hash`, and any class including `Enumerable`. Ruby's method
chaining enables readable multi-step pipelines without intermediate variables.

### The 5 Idioms

```ruby
# 1. Read
ruby -ane 'puts $F[0]' file.txt
rows = CSV.read('data.csv', headers: true)

# 2. Filter
ruby -ne 'print if /pattern/' file.txt
rows.select { |r| r['age'].to_i > 18 }

# 3. Transform
ruby -pe 'gsub(/foo/, "bar")' file.txt
rows.map { |r| {name: r['name'], score: r['score'].to_f * 1.1} }
nested.flat_map { |r| r[:tags] }   # flatten one level

# 4. Aggregate
ruby -ane '$s = $s.to_f + $F[1].to_f; END{puts $s}' data.tsv
rows.group_by { |r| r['dept'] }
    .transform_values { |v| v.sum { |r| r['salary'].to_f } }
words.tally                                    # frequency count — Ruby 2.7+
rows.each_with_object(Hash.new(0)) { |r, h| h[r['dept']] += r['salary'].to_f }

# 5. Write
ruby -i.bak -pe 'gsub(/old/, "new")' file.txt
CSV.open('output.csv', 'w') { |csv| rows.each { |r| csv << r.values } }
```

### Key Feature: `tally`

```ruby
["a", "b", "a", "c", "b", "a"].tally   # {"a"=>3, "b"=>2, "c"=>1}
```

One word = group-by-identity-count. No accumulator, no sorting first.

### vs Perl

Ruby is ~1.2–1.5x more tokens than Perl but reads significantly more clearly.
`Enumerable` methods are more discoverable than Perl idioms.

---

## 6. R and dplyr

### Design: 6-Verb Vocabulary + Pipe + NSE

dplyr's key insight: data frame transformation is a small, composable vocabulary:

| Verb | Meaning |
|------|---------|
| `filter()` | Select rows |
| `select()` | Select/rename/reorder columns |
| `mutate()` | Add or transform columns |
| `arrange()` | Sort rows |
| `summarise()` | Collapse to aggregates |
| `group_by()` | Set grouping for subsequent verbs |

Compose via `|>` (base R 4.1+). **Column names are unquoted** — `filter(df, age > 18)`
not `filter(df, df["age"] > 18)`. Non-standard evaluation (NSE) saves 2–3 tokens per
column reference.

### The 5 Idioms

```r
library(dplyr)

# 1. Read
df <- readr::read_csv("data.csv")   # tidyverse, faster, better types
glimpse(df)                          # compact overview

# 2. Filter
df |> filter(age > 18)
df |> filter(age > 18, status == "active")          # AND (multiple args)
df |> filter(age < 18 | age > 65)                   # OR
df |> filter(grepl("^alice", name, ignore.case=TRUE))

# 3. Transform (mutate)
df |> mutate(total = price * quantity)
df |> mutate(
  total = price * quantity,
  discounted = total * 0.9,
  label = paste(first_name, last_name)
)
df |> mutate(tier = case_when(
  score >= 90 ~ "A",
  score >= 80 ~ "B",
  TRUE        ~ "C"
))

# 4. Aggregate
df |>
  group_by(dept) |>
  summarise(avg_salary = mean(salary), headcount = n())

df |>
  group_by(region, product) |>
  summarise(total = sum(revenue), .groups = "drop")

# 5. Write
readr::write_csv(df, "output.csv")
```

### Full pipeline

```r
read_csv("sales.csv") |>
  filter(year == 2025, !is.na(revenue)) |>
  mutate(
    revenue_k = revenue / 1000,
    quarter   = paste0("Q", ceiling(month / 3))
  ) |>
  group_by(region, quarter) |>
  summarise(total_k = sum(revenue_k), deals = n(), .groups = "drop") |>
  arrange(region, quarter) |>
  write_csv("summary.csv")
```

~15 tokens of semantic content per line vs ~25–30 for equivalent pandas. Savings:
- No `df["col"]` notation — bare names
- `group_by` + `summarise` in 2 lines vs pandas `groupby().agg()` + dict + `reset_index()`

### Weakness

Not a one-liner tool. Library loading overhead. NSE makes metaprogramming subtle.
Less suited to streaming/line-oriented text.

---

## 7. Miller (mlr)

> The most overlooked tool. awk for named fields.

Miller understands CSV, TSV, JSON, and NDJSON natively. Every operation refers to
fields **by name**. Verb chains use `then` instead of `|`.

```bash
# Sum revenue by region, sort descending
mlr --csv stats1 -a sum -f revenue -g region then sort -nr revenue_sum data.csv

# Add computed field
mlr --csv mutate '$margin = $revenue - $cost' data.csv

# Filter + select + rename
mlr --csv filter '$status == "active"' \
    then cut -f name,score \
    then rename score,points data.csv

# Join two files on id
mlr --csv join -f lookup.csv -j id then unsparsify data.csv

# Convert CSV → JSON
mlr --csv --ojson cat data.csv

# Group + count
mlr --csv count-vals -g region data.csv
```

**Token cost vs Python:** 0.10–0.18x. Often 3–5x more concise than pandas for
"reshape + filter + aggregate + reformat" pipelines.

**Key design decisions:**
- Named field access (`$revenue` not `$3`)
- Format auto-detection: `--icsv`, `--ojson`, `--itsv`, etc.
- `then` chains verbs without intermediate files or pipes
- Streaming by default — handles files larger than RAM

---

## 8. DuckDB SQL

```sql
-- In-process, no server, reads CSV/Parquet/JSON natively
SELECT region, SUM(revenue) AS total
FROM read_csv_auto('data.csv')
WHERE year = 2025
GROUP BY region
ORDER BY total DESC;
```

DuckDB is increasingly the right answer for data munging at scale. SQL semantics,
reads files directly, no import step. If the user knows SQL, this is the
minimum-token path.

---

## Concrete Token Count Comparison

### Task: Sum column 2, grouped by column 1, from TSV, sorted descending

```awk
# awk — ~12 tokens
awk '{s[$1]+=$2} END{for(k in s) print s[k],k}' f | sort -rn
```

```bash
# jq (NDJSON input) — ~18 tokens
jq -s 'group_by(.dept) | map({dept:.[0].dept,total:(map(.salary)|add)}) | sort_by(-.total)' f
```

```r
# dplyr — ~30 tokens
read_tsv("f") |> group_by(V1) |> summarise(s=sum(V2)) |> arrange(desc(s))
```

```bash
# Miller — ~14 tokens
mlr --tsv stats1 -a sum -f V2 -g V1 then sort -nr V2_sum f
```

```perl
# Perl — ~15 tokens
perl -lane '$h{$F[0]}+=$F[1]; END{printf "%g %s\n",$h{$_},$_ for sort{$h{$b}<=>$h{$a}}keys%h}' f
```

```python
# Python pandas — ~40 tokens
import pandas as pd
df = pd.read_csv('f', sep='\t', header=None)
result = df.groupby(0)[1].sum().sort_values(ascending=False)
print(result.to_string())
```

```python
# Python stdlib — ~70 tokens
from collections import defaultdict; import sys
d = defaultdict(float)
for line in open(sys.argv[1]):
    k, v = line.split('\t'); d[k] += float(v)
for k in sorted(d, key=d.get, reverse=True): print(d[k], k)
```

**Ratio: awk (12 tokens) → Python stdlib (70 tokens) = 6:1**

Savings come entirely from: implicit loop, auto-split, auto-initialized accumulators,
implicit input handling.

---

## Design Analysis: What Makes These Tools Concise

### Mechanism 1: Implicit Iteration (biggest win)

| Tool | How |
|------|-----|
| awk | Pattern/action applies to each record automatically |
| sed | Every command applies to each line unless addressed |
| jq | `.[]` emits each element; `map()` applies to each |
| Perl `-n` | `while (<>)` loop injected by runtime |
| dplyr | All verbs vectorized — `mutate()` applies to all rows |
| Miller | All verbs stream record-by-record |

**For ilo:** implicit iteration is non-negotiable for conciseness. `map fn xs` is
already implicit iteration — the question is whether the language can make the "current
item" implicitly available (jq's `.` context).

### Mechanism 2: Field Access Syntax

| Tool | Syntax | vs Python |
|------|--------|-----------|
| awk | `$1`, `$NF` | `row.split('\t')[0]` → `$1` (-5 tokens) |
| jq | `.field`, `.[n]` | `obj['field']` → `.field` (-2 tokens, no quotes) |
| dplyr NSE | bare `column_name` | `df['column_name']` → `column_name` (-3 tokens) |
| Miller | `$field` | `row['field']` → `$field` (-2 tokens) |

**For ilo:** record field access `r.field` is already 2 tokens. The question is
whether `mget r "field"` can become `.field` in a pipeline context.

### Mechanism 3: Auto-Initialized Accumulators

awk: `sum[$1] += $2` starts `sum[$1]` at 0 implicitly. This is the single biggest
token saver for aggregation tasks — no `defaultdict`, no `setdefault`, no
initialization block.

**For ilo:** `fld` handles this but requires naming the initial value. A `grp`
builtin that handles the group+aggregate pattern natively would close this gap.

### Mechanism 4: Pipeline Composition

All modern data tools converge on the pipeline model:

```
source | filter | transform | aggregate | sink
```

| Tool | Pipeline syntax |
|------|----------------|
| jq | `\|` operator |
| dplyr | `\|>` or `%>%` |
| Miller | `then` |
| ilo | `>>` |

**For ilo:** `>>` already exists. The question is whether the pipeline is the *default*
computation model for data work.

### Mechanism 5: Pattern-as-Guard

awk: `$3 > 100 { print }` — the condition is a guard with no explicit `if`. For data
filtering, this saves the `if` token and the wrapping braces.

ilo already has guards. For data work, guards are the right model — they read as
"keep if condition" rather than "if condition, keep".

---

## What ilo Should Take From These Tools

| Mechanism | Source | ilo today | ilo gap |
|-----------|--------|-----------|---------|
| Implicit iteration | awk, dplyr | `map fn xs` | No "current row" context |
| Named field access | jq, Miller | `mget r "key"` | Verbose |
| Auto-init accumulators | awk | `fld fn xs init` | Must name init value |
| Group + aggregate | awk, dplyr, Miller | `flt` + `fld` | No native `grp` |
| Sort by key | Python `sorted` | `srt` (natural order only) | No `srtby` |
| Frequency count | `Counter`, `value_counts` | `fld` + manual | No `count-by` |
| Deduplicate | `drop_duplicates`, `unique` | `flt` + manual | No `uniq` |
| String trim | `.str.strip()`, `strip()` | none | No `trm` |

The two highest-impact additions for data work:
1. **`srtby fn xs`** — sort by key function (closes the "sort by column" gap)
2. **`grp key-fn agg-fn xs`** — group + aggregate in one primitive (closes the awk/dplyr gap)

These two, combined with existing `rd`/`rdl`/`wr`/`wrl` + `spl "\n"` + `map`/`flt`/`fld`,
cover ~80% of real data scripting tasks.
