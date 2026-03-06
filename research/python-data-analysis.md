# Python Data Analysis: Primitives for Language Designers

> What "good" looks like in pandas, polars, and Python stdlib — an opinionated
> analysis for designing a token-minimal data language.

## Executive Summary

After surveying Python's data ecosystem (pandas, polars, stdlib), command-line tools
(jq, awk), and real-world data scripts, 12–15 operations account for ~80% of all data
manipulation code. The core insight: **most data work is filter → transform →
aggregate**, and the complexity of any given tool is almost entirely in how it handles
these three primitives plus their composition.

---

## Part 1: The 80% Operations

These appear in virtually every real data script, in roughly this order of frequency:

1. **Load** — read data from file/string (csv, json, tsv)
2. **Filter** — keep rows matching a predicate
3. **Select/Project** — keep only certain columns/fields
4. **Map/Transform** — compute new column from existing ones
5. **Sort** — order rows by one or more keys
6. **Group + Aggregate** — group by key, compute sum/count/mean
7. **Count** — how many rows / how many per group
8. **Rename** — change field names
9. **Drop** — remove columns
10. **Join** — combine two tables on a key
11. **Deduplicate** — unique rows
12. **Pivot/Reshape** — wide↔long transformations
13. **Head/Tail/Sample** — inspect subsets
14. **Write** — output to file/string

Operations 1–9 appear in nearly every script. Operations 10–14 appear in roughly
40–60% of scripts. Joins and pivots are conceptually complex but syntactically
expensive in every tool.

---

## Part 2: Token Cost Analysis

### Load

```python
# pandas — 5 tokens
df = pd.read_csv("data.csv")
df = pd.read_json("data.json")
df = pd.read_csv("data.tsv", sep="\t")

# polars — identical surface
df = pl.read_csv("data.csv")
df = pl.scan_csv("data.csv")          # lazy variant

# stdlib — 15–25 tokens
with open("data.csv") as f:
    rows = list(csv.DictReader(f))    # list of dicts

with open("data.json") as f:
    data = json.load(f)
```

**Key:** pandas/polars ~5 tokens. stdlib ~15–25 tokens. Stdlib forces managing file
handle, reader object, and materialization separately.

### Filter

```python
# pandas boolean indexing — 7 tokens
result = df[df["age"] > 30]

# pandas query string — 6 tokens
result = df.query("age > 30")

# multiple conditions — bracket hell
result = df[(df["age"] > 30) & (df["city"] == "NYC")]   # 14 tokens
result = df.query("age > 30 and city == 'NYC'")          # 8 tokens

# polars
result = df.filter(pl.col("age") > 30)                   # 8 tokens
result = df.filter((pl.col("age") > 30) & (pl.col("city") == "NYC"))  # 14

# stdlib
result = [r for r in rows if r["age"] > 30]              # 10 tokens
result = [r for r in rows if r["age"] > 30 and r["city"] == "NYC"]   # 15
```

**Key observation:** Filter predicates are always `column OP value`. Column reference
syntax is the only thing that varies across tools.

### Sort

```python
# pandas
result = df.sort_values("age")
result = df.sort_values(["age", "name"], ascending=[True, False])

# polars
result = df.sort("age")
result = df.sort(["age", "name"], descending=[False, True])

# stdlib
result = sorted(rows, key=lambda r: r["age"])
result = sorted(rows, key=lambda r: (r["age"], r["name"]))
```

All tools land at 5–8 tokens for single-key sort.

### Group + Aggregate — the complexity peak

```python
# pandas
result = df.groupby("city")["sales"].sum()           # returns Series
result = df.groupby("city")["sales"].sum().reset_index()  # returns DataFrame

result = df.groupby("city").agg({
    "sales": "sum",
    "quantity": "mean",
    "customer": "count"
})

# polars — consistent, no reset_index needed
result = df.group_by("city").agg(
    pl.col("sales").sum(),
    pl.col("quantity").mean(),
    pl.col("customer").count()
)

# stdlib — requires sort before groupby, footgun
rows_sorted = sorted(rows, key=lambda r: r["city"])
for city, group in itertools.groupby(rows_sorted, key=lambda r: r["city"]):
    total = sum(r["sales"] for r in group)

# OR: defaultdict
from collections import defaultdict
totals = defaultdict(float)
for r in rows:
    totals[r["city"]] += r["sales"]
```

**Key observation:** Group-aggregate is the most "deserving" of a language primitive.
Conceptually simple but every tool requires 2–3 method calls with subtle
shape-of-return issues (`reset_index()` in pandas).

### Deduplicate

```python
# pandas — 4–8 tokens
result = df.drop_duplicates()
result = df.drop_duplicates(subset=["user_id"])
result = df.drop_duplicates(subset=["user_id"], keep="last")

# polars
result = df.unique()
result = df.unique(subset=["user_id"])

# stdlib — 6 lines
seen = set()
result = []
for r in rows:
    key = r["user_id"]
    if key not in seen:
        seen.add(key)
        result.append(r)
```

### Count

```python
len(df)                          # total rows
df["city"].value_counts()        # frequency per value — 3 tokens
df.groupby("city").size()        # same as groupby
df["city"].nunique()             # distinct count
Counter(r["city"] for r in rows) # stdlib equivalent — very clean
```

`value_counts()` and `Counter` are genuinely magical — collapse
group-by-key-count-occurrences into one word.

---

## Part 3: What Pandas Does That Feels Magical

### Boolean Series as Index

```python
df[df["age"] > 30]
```

Overloads `[]` to accept a boolean array. Concise but requires understanding that
`df["age"] > 30` produces a boolean Series, not a Python bool.

### `value_counts()`

```python
df["country"].value_counts()
```

One word collapses: sort descending by frequency + group by unique value + count each
group.

### `.str` accessor — vectorized string ops

```python
df["name"].str.lower()
df["name"].str.contains("Smith")
df["email"].str.split("@").str[1]    # get domain from email
```

Apply this string op to every element, without a loop. Clean, readable.

### `groupby().agg()` with aggregation specs

```python
df.groupby("city").agg({"sales": ["sum", "mean"], "qty": "count"})
```

---

## Part 4: Pandas Boilerplate (token waste)

### `reset_index()` After Groupby

```python
result = df.groupby("city")["sales"].sum().reset_index()
```

After `.groupby().sum()`, the group key becomes the index. Pure bookkeeping noise
that polars eliminates.

### Column Name Cleanup After Merge

```python
result = df1.merge(df2, on="user_id", suffixes=("_left", "_right"))
result = result.rename(columns={"value_left": "value1", "value_right": "value2"})
```

### `.copy()` to Avoid SettingWithCopyWarning

```python
subset = df[df["age"] > 30].copy()
subset["new_col"] = "value"     # without .copy(), may warn or silently fail
```

Leaky abstraction from pandas' internal view/copy system.

---

## Part 5: Polars vs Pandas

| Feature | Pandas | Polars |
|---------|--------|--------|
| Column access | `df["col"]` or `df.col` | `pl.col("col")` (verbose but consistent) |
| Index | Yes (causes bugs) | No index concept |
| Lazy evaluation | No (dask is separate) | `scan_csv()` → `LazyFrame` |
| Mutation | `df["col"] = expr` | `with_columns()` + `.alias()` |
| `reset_index()` | Required after groupby | Never needed |
| Multi-agg | Dict syntax OR named-agg (two spellings) | Single `agg()` with expressions |
| Performance >1M rows | Baseline | 5–20x faster |
| Type coercion | Permissive | Strict |

**Polars gets right:** immutability, no index, uniform expression API, lazy eval.

---

## Part 6: Python Stdlib

### `csv` Module

```python
# Read
rows = list(csv.DictReader(open("data.csv")))      # list of dicts

# Write
writer = csv.DictWriter(f, fieldnames=["name", "age"])
writer.writeheader()
writer.writerows(rows)
```

Adequate but everything is strings — no type inference.

### `itertools` — The Underused Gem

```python
# groupby — MUST sort first (footgun: only groups consecutive identical keys)
for key, group in itertools.groupby(sorted_rows, key=lambda r: r["city"]):
    items = list(group)

# flatten list of lists
flat = list(itertools.chain.from_iterable(nested))

# take first N
first_10 = list(itertools.islice(rows, 10))
```

### `collections`

```python
# Counter — frequency counts
counts = Counter(r["city"] for r in rows)
counts.most_common(5)

# defaultdict — accumulate without KeyError
by_city = defaultdict(list)
for r in rows:
    by_city[r["city"]].append(r)
```

`Counter` and `defaultdict` are the stdlib equivalents of `value_counts()` and
`groupby()`.

### `statistics`

```python
statistics.mean([1, 2, 3, 4, 5])    # 3.0
statistics.median(data)
statistics.stdev(data)
statistics.quantiles(data, n=4)      # quartiles
```

No vectorized operations — requires extracting column to a list first.

---

## Part 7: The 12 Essential Primitives

Ranked by **frequency** and **primitive-worthiness** (deserves first-class syntax):

### Simple — should be primitives

| Operation | Token cost (pandas) | Irreducible minimum |
|-----------|--------------------|--------------------|
| Load | 5 | 2 (`load "f"`) |
| Filter | 7–14 | 4 (`filter .age > 30`) |
| Select | 5–8 | 3 (`select name age`) |
| Sort | 4–8 | 3 (`sort age`) |
| Count (total) | 1 (`len`) | 1 |
| Count (freq) | 3 (`value_counts`) | 3 |
| Deduplicate | 4 | 2 (`unique`) |
| Rename | 6 | 3 (`rename old new`) |

### Medium — common but multi-concept

| Operation | Token cost (pandas) | Notes |
|-----------|--------------------|-|
| Map/Transform | 6–10 | "add computed column" deserves own primitive |
| Group + Aggregate | 8–15 | THE most important composite primitive |
| Flatten/Unnest | 4 (`explode`) | Needed for JSON with nested arrays |

### Complex — inherently multi-concept

| Operation | Token cost (pandas) | Notes |
|-----------|--------------------|-|
| Join | 8–12 | Requires type + keys always |
| Pivot | 12–18 | Requires 3+ args always |
| Window functions | 15–25 | Rolling, cumulative |

---

## Part 8: The 3-Line Test

A good measure of a data language is: can common tasks fit in 3 lines?

**Pandas (38 tokens, includes noise):**
```python
df = pd.read_csv("sales.csv")
result = df[df["year"] == 2024].groupby("region")["revenue"].sum().reset_index()
result.to_csv("summary.csv", index=False)
```

**Polars (32 tokens, cleaner but `pl.col()` is verbose):**
```python
df = pl.read_csv("sales.csv")
result = df.filter(pl.col("year") == 2024).group_by("region").agg(pl.col("revenue").sum())
result.write_csv("summary.csv")
```

**Hypothetical minimal (18 tokens):**
```
data = load "sales.csv"
result = data | filter .year == 2024 | group region | sum revenue
write result "summary.csv"
```

The gap between pandas (38) and the minimum (18) is almost entirely: mandatory
library prefix (`pd.`, `pl.`), mandatory column quoting, and pandas-specific
bookkeeping (`reset_index`, `index=False`).

---

## Conclusions for ilo

**The single biggest ergonomic win**: making column/field reference cheap (1–2 tokens
vs 4–6 in pandas/polars). Combined with implicit iteration, this covers 80% of data
work in roughly half the tokens of pandas.

**Operations that justify primitives in ilo:** filter, sort-by-key (`srtby`),
group+aggregate (`grp`), count-by, deduplicate (`uniq`), string ops (`trm`, `upr`,
`lwr`), CSV parse (`csv`).

**Operations that do NOT need primitives:** join (complex, use records + `flt`), pivot
(too complex, delegate to tools), window functions (too complex, delegate).
