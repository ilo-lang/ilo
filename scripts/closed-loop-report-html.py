#!/usr/bin/env python3
"""Render closed-loop bench run JSON into one self-contained HTML report.

Reads the `closed-loop-<date>-<config>[-<leg>].json` files written by
`closed-loop-bench.py` and emits a single HTML file with all styling and
scripting inlined — no CDN, no build step, no network. Any run already on disk
can be re-rendered without re-spending tokens, which is the point: the JSON is
the measurement, the HTML is a view of it.

Two things this renderer does that the raw JSON does not:

* **Merges a sweep's per-leg files.** The comparator matrix runs the harness
  once per language, so a 5-leg sweep writes 5 JSON files that each contain the
  ilo arm plus one comparator. Merging them (and averaging repeated arms) is
  what makes "ilo vs five languages" one table instead of five.
* **Reports characters and per-row means.** Token counts alone cannot answer
  "what did this cost" or "how long did it take per program".

Usage:
    scripts/closed-loop-report-html.py [-o bench/report.html] [run.json ...]

With no paths, every `bench/closed-loop-*.json` is rendered.
"""

from __future__ import annotations

import argparse
import html
import json
import re
import sys
from datetime import datetime
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
BENCH_DIR = REPO_ROOT / "bench"

# ilo first, then comparators in the order the matrix runs them; anything else
# sorts alphabetically after these.
LEG_ORDER = ["ilo", "python", "bash", "zero", "ailang", "nanolang", "moonbit"]
OUTCOMES = ["working", "partial", "failed"]

# Numeric row fields that are averaged when an arm appears in several leg files
# (the ilo arm is measured once per leg, so it usually has the most repeats).
AVERAGED = [
    "generation_tokens", "input_tokens", "input_cache_hit_tokens",
    "input_cache_miss_tokens", "effective_input_tokens", "cache_savings_usd",
    "cost_usd", "generated_chars", "final_code_chars", "wall_time_s",
    "attempts_total", "success_rate",
]


def load_task_meta() -> dict[str, dict]:
    """Task definitions from tasks.json, keyed by id (annotation only).

    The run JSON records measurements, not what was asked; the description and
    expected output live in tasks.json, which the harness reads.
    """
    try:
        data = json.loads((BENCH_DIR / "closed-loop" / "tasks.json").read_text())
    except (OSError, json.JSONDecodeError):
        return {}
    return {t["id"]: t for t in data.get("tasks", []) if t.get("id")}


TASK_META = load_task_meta()


# ---------------------------------------------------------------------------
# Formatting helpers
# ---------------------------------------------------------------------------

def E(s) -> str:
    return html.escape("" if s is None else str(s))


def fmt_int(v, dash: str = "—") -> str:
    return dash if v is None else f"{round(v):,}"


def fmt_money(v, dash: str = "—") -> str:
    return dash if v is None else f"${v:,.4f}"


def fmt_s(v, dash: str = "—") -> str:
    return dash if v is None else f"{v:,.1f}s"


def fmt_ratio(v) -> str:
    """Ratio cell: bar colouring is the point — under 1× is cheaper than ilo."""
    if v is None:
        return '<td class="num">—</td>'
    cls = "ok" if v < 1 else ("bad" if v > 1 else "")
    return f'<td class="num ratio {cls}">{v:.2f}×</td>'


def mean(values) -> float | None:
    vals = [v for v in values if isinstance(v, (int, float))]
    return sum(vals) / len(vals) if vals else None


# ---------------------------------------------------------------------------
# Loading
# ---------------------------------------------------------------------------

def mean_rows(rows: list[dict]) -> dict:
    """Collapse repeated measurements of one (task, language, model) into means.

    The matrix measures the ilo arm once per leg, so without this the same ilo
    task would appear five times and every total would count it five times.
    """
    out = dict(rows[0])
    n = len(rows)
    for field in AVERAGED:
        vals = [r[field] for r in rows if isinstance(r.get(field), (int, float))]
        if vals:
            out[field] = sum(vals) / len(vals)
    solved = [r["attempts_to_success"] for r in rows
              if isinstance(r.get("attempts_to_success"), int)]
    out["attempts_to_success"] = mean(solved) if solved else None
    worked = sum(1 for r in rows if r.get("final_outcome") == "working")
    out["final_outcome"] = ("working" if worked == n else
                            "failed" if worked == 0 else "partial")
    out["repeats"] = n
    return out


def run_config(run: dict) -> dict:
    """The axes a run varies, read from its rows (filenames can lie)."""
    results = run.get("results") or []
    row = (results or [{}])[0]
    models = list(dict.fromkeys(r.get("model") for r in results if r.get("model")))
    langs = sorted(
        {r.get("language") for r in results if r.get("language")},
        key=lang_key,
    )
    return {
        "cache": row.get("cache_mode") or run.get("cache_mode") or "?",
        "align": bool(row.get("align")),
        "context": row.get("context"),
        "models": models,
        "langs": langs,
    }


def run_title(run: dict) -> str:
    """Plain-language tab label for a run's config."""
    cfg = run_config(run)
    parts = [f"{cfg['cache']} cache",
             "aligned" if cfg["align"] else "rewritten prompt"]
    if cfg["context"]:
        parts.append(f"{cfg['context']} spec")
    if cfg["models"]:
        parts.append(" + ".join(cfg["models"]))
    comparators = [l for l in cfg["langs"] if l != "ilo"]
    if comparators:
        parts.append("vs " + ", ".join(comparators))
    # Two sweeps can share a config (same cache/align/context/model); the date
    # is what tells them apart, so it leads the label.
    when = (run.get("generated") or "")[:10]
    return " · ".join(([when] if when else []) + parts)


def load_runs(paths: list[Path]) -> list[dict]:
    """Read run files, merging leg files that belong to one sweep."""
    groups: dict[tuple[str, str], list[dict]] = {}
    for p in paths:
        try:
            payload = json.loads(p.read_text())
        except (OSError, json.JSONDecodeError) as exc:
            print(f"[report] skipping {p}: {exc}", file=sys.stderr)
            continue
        if not payload.get("results"):
            print(f"[report] skipping {p}: no results", file=sys.stderr)
            continue
        payload["_source"] = (p.name if p.parent == BENCH_DIR
                              else str(p.relative_to(REPO_ROOT)) if REPO_ROOT in p.parents
                              else str(p))

        m = re.match(r"^closed-loop-(\d{4}-\d{2}-\d{2})-(.+)$", p.stem)
        date_part, label = (m.group(1), m.group(2)) if m else ("", p.stem)
        leg = payload.get("leg")
        if leg and label.endswith(f"-{leg}"):
            label = label[: -(len(leg) + 1)]
        # Older runs omit cache_mode; the filename is then the only source.
        if not payload.get("cache_mode"):
            payload["cache_mode"] = label.split("-", 4)[-1] if label.count("-") >= 4 else label
        groups.setdefault((date_part, label), []).append(payload)

    runs: list[dict] = []
    for (date_part, label), members in groups.items():
        if len(members) == 1:
            run = members[0]
            run["_legs"] = [run["leg"]] if run.get("leg") else []
        else:
            # One sweep, several leg files: fold them into one view, averaging
            # arms measured more than once (notably ilo).
            by_key: dict[tuple, list[dict]] = {}
            for m in members:
                for r in m["results"]:
                    by_key.setdefault(
                        (r["task"], r["language"], r["model"]), []).append(r)
            run = dict(members[0])
            run["results"] = [mean_rows(v) for v in by_key.values()]
            run["_legs"] = sorted({m["leg"] for m in members if m.get("leg")})
            run["_sources"] = sorted(m["_source"] for m in members)
            run["cache_mode"] = label
        runs.append(run)

    # Oldest → newest so the tab strip reads chronologically; newest is shown.
    runs.sort(key=lambda r: (r.get("generated") or "", r["_source"]))
    return runs


# ---------------------------------------------------------------------------
# Aggregation
# ---------------------------------------------------------------------------

def lang_key(lang: str) -> tuple:
    return (LEG_ORDER.index(lang) if lang in LEG_ORDER else len(LEG_ORDER), lang)


def lang_totals(results: list[dict]) -> dict[str, dict]:
    agg: dict[str, dict] = {}
    for r in results:
        a = agg.setdefault(r["language"], {
            "rows": 0, "gen": 0, "chars": 0, "char_rows": 0, "inp": 0,
            "cost": 0.0, "wall": 0.0, "attempts": 0, "repeats": 0,
            "working": 0, "success": 0.0, "outcomes": {},
        })
        a["rows"] += 1
        a["gen"] += r.get("generation_tokens") or 0
        if isinstance(r.get("generated_chars"), (int, float)):
            a["chars"] += r["generated_chars"]
            a["char_rows"] += 1
        a["inp"] += r.get("input_tokens") or 0
        a["cost"] += r.get("cost_usd") or 0.0
        a["wall"] += r.get("wall_time_s") or 0.0
        a["attempts"] += r.get("attempts_total") or 0
        a["repeats"] += r.get("repeats") or 1
        a["success"] += r.get("success_rate") or 0.0
        oc = r.get("final_outcome") or "failed"
        a["outcomes"][oc] = a["outcomes"].get(oc, 0) + 1
        if oc == "working":
            a["working"] += 1

    for lang, a in agg.items():
        n, cn = a["rows"], a["char_rows"]
        a["mean_gen"] = a["gen"] / n if n else None
        a["mean_chars"] = a["chars"] / cn if cn else None
        a["mean_inp"] = a["inp"] / n if n else None
        a["mean_cost"] = a["cost"] / n if n else None
        a["mean_wall"] = a["wall"] / n if n else None
        a["mean_attempts"] = a["attempts"] / n if n else None
        a["mean_success"] = 100.0 * a["success"] / n if n else None
        a["chars_per_gen"] = (a["chars"] / a["gen"]) if a["gen"] and cn else None
    return agg


def run_stats(run: dict) -> dict:
    results = run["results"]
    agg = lang_totals(results)
    char_rows = [r for r in results if isinstance(r.get("generated_chars"), (int, float))]
    gen = sum(r.get("generation_tokens") or 0 for r in results)
    chars = sum(r.get("generated_chars") or 0 for r in char_rows)
    inp = sum(r.get("input_tokens") or 0 for r in results)
    cost = sum(r.get("cost_usd") or 0.0 for r in results)
    wall = sum(r.get("wall_time_s") or 0.0 for r in results)
    rows = len(results)
    return {
        "results": results,
        "langs": sorted(agg, key=lang_key),
        "agg": agg,
        "tasks": list(dict.fromkeys(r["task"] for r in results)),
        "rows": rows,
        "gen": gen,
        "chars": chars,
        "char_rows": len(char_rows),
        "inp": inp,
        "cost": cost,
        "wall": wall,
        "mean_gen": gen / rows if rows else None,
        "mean_chars": chars / len(char_rows) if char_rows else None,
        "mean_inp": inp / rows if rows else None,
        "mean_cost": cost / rows if rows else None,
        "mean_wall": wall / rows if rows else None,
        "chars_per_gen": chars / gen if gen and char_rows else None,
        "working": sum(1 for r in results if r.get("final_outcome") == "working"),
        "max_gen": max((r.get("generation_tokens") or 0) for r in results) or 1,
    }


# ---------------------------------------------------------------------------
# Rendering
# ---------------------------------------------------------------------------

def cards(st: dict) -> str:
    """Totals and per-row means for the three costs: time, tokens, characters."""
    def card(title, value, sub, hint=""):
        t = f' title="{E(hint)}"' if hint else ""
        return (f'<div class="card"{t}><div class="k">{title}</div>'
                f'<div class="v">{value}</div><div class="s">{sub}</div></div>')

    legs = len(st["langs"])
    chars_known = bool(st["char_rows"])
    return (
        '<div class="cards">'
        + card("generation tokens", fmt_int(st["gen"]),
               f"mean {fmt_int(st['mean_gen'])} per row · includes reasoning",
               "Every token the model emitted, reasoning included; the total "
               "sums every task-row in this tab")
        + card("code characters",
               fmt_int(st["chars"]) if chars_known else "—",
               (f"mean {fmt_int(st['mean_chars'])} per row · emitted code only"
                if chars_known else "not captured in this run (older JSON)"),
               "Characters of emitted code, excluding reasoning — the density "
               "metric, since reasoning tokens dominate token counts")
        + card("execution time", fmt_s(st["wall"], "—"),
               f"mean {fmt_s(st['mean_wall'])} per row",
               "Harness wall time across all attempts (generation + execution)")
        + card("characters per token",
               "—" if st["chars_per_gen"] is None else f"{st['chars_per_gen']:.2f}",
               "higher is denser code")
        + card("input tokens", fmt_int(st["inp"]),
               f"mean {fmt_int(st['mean_inp'])} per row · provider-side, pre-cache")
        + card("spend", fmt_money(st["cost"]),
               f"mean {fmt_money(st['mean_cost'])} per row")
        + card("working", f"{st['working']}/{st['rows']}",
               "task-rows solved within the retry cap")
        + card("legs", str(legs),
               f"{st['rows']} rows · {len(st['tasks'])} tasks")
        + "</div>"
    )


def lang_table(st: dict) -> str:
    rows = []
    # Same contract as the per-task tables: the whole field is listed, and a
    # language with no measurement in this tab says so instead of going absent.
    universe = list(LEG_ORDER) + [l for l in st["langs"] if l not in LEG_ORDER]
    for lang in universe:
        a = st["agg"].get(lang)
        if a is None:
            rows.append(
                f'<tr class="unmeasured"><td class="lang">{E(lang)}</td>'
                + '<td class="num">—</td>' * 14
                + f'<td>{unmeasured_cell()}</td></tr>'
            )
            continue
        att = "—" if a["mean_attempts"] is None else f"{a['mean_attempts']:.2f}"
        oc = "".join(
            f'<span class="chip {o}">{a["outcomes"].get(o, 0)}</span>'
            for o in OUTCOMES if a["outcomes"].get(o)
        )
        cpg = "—" if a["chars_per_gen"] is None else f"{a['chars_per_gen']:.2f}"
        rows.append(
            f'<tr><td class="lang">{E(lang)}</td>'
            f'<td class="num">{a["repeats"]}</td>'
            f'<td class="num">{a["rows"]}</td>'
            f'<td class="num">{fmt_int(a["gen"])}</td>'
            f'<td class="num">{fmt_int(a["mean_gen"])}</td>'
            f'<td class="num">{fmt_int(a["chars"])}</td>'
            f'<td class="num">{fmt_int(a["mean_chars"])}</td>'
            f'<td class="num">{cpg}</td>'
            f'<td class="num">{fmt_int(a["inp"])}</td>'
            f'<td class="num">{fmt_money(a["cost"])}</td>'
            f'<td class="num">{fmt_money(a["mean_cost"])}</td>'
            f'<td class="num">{fmt_s(a["wall"])}</td>'
            f'<td class="num">{fmt_s(a["mean_wall"])}</td>'
            f'<td class="num">{att}</td>'
            f'<td class="num">{a["working"]}/{a["rows"]}</td>'
            f'<td>{oc}</td></tr>'
        )
    return (
        '<div class="scroll"><table class="grid sortable"><thead><tr>'
        '<th data-sort="text">leg</th>'
        '<th data-sort="num" title="measurements averaged into each task-row">reps</th>'
        '<th data-sort="num" title="distinct task-rows behind the totals">tasks</th>'
        '<th data-sort="num">gen tok</th>'
        '<th data-sort="num" title="mean generation tokens per task-row">tok/row</th>'
        '<th data-sort="num">code chars</th>'
        '<th data-sort="num" title="mean emitted-code characters per task-row">chars/row</th>'
        '<th data-sort="num" title="characters per generation token: higher is denser">chars/tok</th>'
        '<th data-sort="num">input tok</th>'
        '<th data-sort="num">cost</th>'
        '<th data-sort="num" title="mean spend per task-row">$/row</th>'
        '<th data-sort="num">wall</th>'
        '<th data-sort="num" title="mean seconds per task-row">s/row</th>'
        '<th data-sort="num" title="mean attempts per task-row">att/row</th>'
        '<th data-sort="num">working</th><th data-sort="text">outcomes</th>'
        "</tr></thead><tbody>" + "".join(rows) + "</tbody></table></div>"
    )


def ratio_table(st: dict) -> str:
    """Per-row means divided by ilo's: <1× means cheaper/smaller than ilo."""
    if "ilo" not in st["agg"] or len(st["langs"]) < 2:
        return ""
    base = st["agg"]["ilo"]

    def ratio(a, field):
        b, i = a.get(field), base.get(field)
        return (b / i) if b is not None and i else None

    rows = []
    for lang in st["langs"]:
        if lang == "ilo":
            continue
        a = st["agg"][lang]
        rows.append(
            f'<tr><td class="lang">{E(lang)}</td>'
            + fmt_ratio(ratio(a, "mean_gen"))
            + fmt_ratio(ratio(a, "mean_chars"))
            + fmt_ratio(ratio(a, "mean_cost"))
            + fmt_ratio(ratio(a, "mean_wall"))
            + f'<td class="num">{a["working"]}/{a["rows"]}</td>'
            + f'<td class="num">{base["working"]}/{base["rows"]}</td></tr>'
        )
    return (
        '<div class="scroll"><table class="grid sortable"><thead><tr>'
        '<th data-sort="text">leg</th>'
        '<th data-sort="num" title="mean generation tokens per row, leg ÷ ilo">tokens</th>'
        '<th data-sort="num" title="mean emitted-code characters per row, leg ÷ ilo">chars</th>'
        '<th data-sort="num" title="mean spend per row, leg ÷ ilo">cost</th>'
        '<th data-sort="num" title="mean wall time per row, leg ÷ ilo">time</th>'
        '<th data-sort="num">working</th><th data-sort="num">ilo working</th>'
        "</tr></thead><tbody>" + "".join(rows) + "</tbody></table></div>"
    )


def task_blocks(st: dict) -> str:
    """One table per task: every language, measured or not, side by side.

    The flat alternative (one row per task×language) hides the thing the bench
    exists to show — how the languages compare *on the same program*. Grouping by
    task puts them adjacent, and a leg the sweep has not reached keeps its row
    with a "not measured" mark, so a partial sweep cannot read as a full one.
    """
    by_task: dict[str, list[dict]] = {}
    for r in st["results"]:
        by_task.setdefault(r["task"], []).append(r)

    # tasks.json order first (the canonical list), then anything it does not name.
    ordered = [t for t in TASK_META if t in by_task]
    ordered += [t for t in by_task if t not in TASK_META]

    blocks = []
    for task in ordered:
        rows = sorted(by_task[task], key=lambda r: lang_key(r["language"]))
        solved = [r for r in rows
                  if r.get("final_outcome") == "working" and r.get("generation_tokens")]
        best_gen = min((r["generation_tokens"] for r in solved), default=None)

        meta = TASK_META.get(task) or {}
        tags = []
        if meta.get("difficulty"):
            tags.append(f'<span class="tag">{E(meta["difficulty"])}</span>')
        if meta.get("category"):
            tags.append(f'<span class="tag">{E(meta["category"])}</span>')
        if meta.get("expected_output"):
            tags.append(f'<span class="tag">expects {E(meta["expected_output"])}</span>')

        body = []
        rows_by_lang = {r["language"]: r for r in rows}
        # Every language the field knows is a row, always. A leg the sweep has
        # not reached yet reads "not measured" instead of vanishing, so no tab
        # can look like "ilo vs bash" while five other legs are still pending.
        universe = list(LEG_ORDER) + [l for l in rows_by_lang if l not in LEG_ORDER]
        for lang in universe:
            r = rows_by_lang.get(lang)
            if r is None:
                body.append(
                    f'<tr class="unmeasured"><td class="lang">{E(lang)}</td>'
                    + '<td class="num">—</td>' * 8
                    + f'<td>{unmeasured_cell()}</td></tr>'
                )
                continue
            gen = r.get("generation_tokens")
            chars = r.get("generated_chars")
            dens = (chars / gen) if isinstance(chars, (int, float)) and gen else None
            rep = (f' <span class="rep">×{r["repeats"]}</span>'
                   if (r.get("repeats") or 1) > 1 else "")
            mark = (' class="num best" title="fewest generation tokens among the '
                    'legs that solved this task"'
                    if best_gen is not None and gen == best_gen else ' class="num"')
            dens_cell = "—" if dens is None else f"{dens:.2f}"
            # Truncation is a *cause* of failure, not a failure: the attempt hit
            # the model's output cap, emitted no code, and reads as ordinary.
            # Absent on rows written before the field existed — dash, not zero.
            trunc = r.get("truncated_attempts")
            trunc_cell = ("—" if trunc is None else
                          f'<span class="badmark" title="attempts that hit '
                          f'max_tokens and emitted no code">{trunc}</span>'
                          if trunc else "0")
            body.append(
                f'<tr><td class="lang">{E(r["language"])}{rep}</td>'
                f'<td{mark}>{fmt_int(gen)}</td>'
                f'<td class="num">{fmt_int(chars)}</td>'
                f'<td class="num">{dens_cell}</td>'
                f'<td class="num">{fmt_int(r.get("input_tokens"))}</td>'
                f'<td class="num">{fmt_money(r.get("cost_usd"))}</td>'
                f'<td class="num">{r.get("attempts_total") or "—"}</td>'
                f'<td class="num">{trunc_cell}</td>'
                f'<td class="num">{fmt_s(r.get("wall_time_s"))}</td>'
                f'<td>{outcome_cell(r.get("final_outcome"))}</td></tr>'
            )

        # The unmeasured rows above name the missing legs per task, so the
        # gap needs no separate note.

        blocks.append(
            f'<div class="taskblock">'
            f'<div class="taskhead"><span class="tid">{E(task)}</span>'
            f'{"".join(tags)}</div>'
            + (f'<div class="note">{E(meta["description"])}</div>'
               if meta.get("description") else "")
            + '<div class="scroll"><table class="grid"><thead><tr>'
            '<th>leg</th><th>gen tokens</th>'
            '<th title="characters of emitted code, excluding reasoning">code chars</th>'
            '<th title="emitted characters per generation token; higher is denser">chars/tok</th>'
            '<th>input</th><th>cost</th><th>attempts</th>'
            '<th title="attempts that hit the model output cap and emitted no code">trunc</th>'
            '<th>wall</th>'
            '<th>outcome</th></tr></thead><tbody>'
            + "".join(body) + "</tbody></table></div></div>"
        )
    return "".join(blocks)


def outcome_cell(outcome) -> str:
    o = outcome or "failed"
    return f'<span class="pill {E(o)}">{E(o)}</span>'


def unmeasured_cell() -> str:
    return '<span class="pill unmeasured">not measured</span>'


def run_section(run: dict, idx: int, active: bool) -> str:
    st = run_stats(run)
    cfg = run_config(run)
    legs = run.get("_legs") or []
    sources = run.get("_sources") or [run["_source"]]
    meta = [
        f"{st['rows']} rows",
        f"{len(st['tasks'])} tasks",
        f"{len(st['langs'])} legs" + (f" ({', '.join(E(l) for l in legs)})" if legs else ""),
    ]
    if cfg["models"]:
        meta.append("model " + ", ".join(E(m) for m in cfg["models"]))
    if cfg["context"]:
        meta.append(f"{E(cfg['context'])} spec set")
    if len(sources) > 1:
        meta.append(f"merged from {len(sources)} leg files")
    # The leg universe is known (LEG_ORDER); what a tab measured is known; the
    # complement is the honest part of the header. A sweep that ran 2 of 6 legs
    # must say so where a reader looks first, not only inside each task table.
    missing = [l for l in LEG_ORDER if l not in st["langs"]]
    partial = ""
    if missing and len(st["langs"]) < len(LEG_ORDER):
        partial = (
            f'<p class="warn"><strong>Partial sweep.</strong> '
            f'Measured: {", ".join(E(l) for l in st["langs"])}. '
            f'Not measured here: {", ".join(E(l) for l in missing)} — no run on '
            f'this task set in this tab, so no comparison against '
            f'{E("them" if len(missing) > 1 else "it")} is available yet.</p>'
        )
    ratio = ratio_table(st)
    trunc_rows = [r for r in st["results"] if (r.get("truncated_attempts") or 0)]
    trunc_total = sum(r["truncated_attempts"] for r in trunc_rows)
    banner = ""
    if trunc_rows:
        affected = ", ".join(
            f"{E(r['language'])}/{E(r['task'])}" for r in trunc_rows[:6]
        )
        more = f" (+{len(trunc_rows) - 6} more)" if len(trunc_rows) > 6 else ""
        banner = (
            f'<p class="warn"><strong>Cap-limited rows.</strong> '
            f'{trunc_total} attempt(s) across {len(trunc_rows)} row(s) stopped at '
            f'the model output cap and emitted no code — {affected}{more}. Those '
            f'rows measure the cap, not the language; a re-run with a higher cap '
            f'is required before they can be read as losses.</p>'
        )
    return f"""
<section class="run{' active' if active else ''}" id="run-{idx}">
  <h2>{E(run_title(run))}</h2>
  <div class="meta">{' · '.join(meta)}</div>
  <div class="sub">Sources: {' '.join(f'<code>{E(s)}</code>' for s in sources)}</div>
  {cards(st)}
  {banner}
  {partial}
  <h3>Per task, by language</h3>
  <p class="note">One table per task — every language measured on that task, on the same
  program. <em>gen tokens</em> is everything the model emitted (reasoning included);
  <em>code chars</em> is the emitted program only, so <em>chars/tok</em> is the density
  metric. A language the sweep did not run stays listed as “not measured”, never
  silently dropped.</p>
  {task_blocks(st)}
  <h3>Cost per language</h3>
  <p class="note">Totals cover every task-row in this tab; <em>per row</em> means per single
  task attempt-set (one language, one task), so it is comparable across tabs with
  different task counts. <em>reps</em> is how many measurements were averaged into a
  row — the ilo arm is measured once per comparator leg. <em>code chars</em> count emitted code only — reasoning
  tokens are inside <em>gen tok</em> and are the bulk of them.</p>
  {lang_table(st)}
  {f'<h3>Head-to-head vs ilo</h3><p class="note">Mean per task-row, leg ÷ ilo. Below 1× is smaller/cheaper than ilo; green is better for the leg.</p>{ratio}' if ratio else ''}
</section>"""


def build_html(runs: list[dict]) -> str:
    # Newest run is the default view and the leftmost tab; ids stay
    # chronological so deep links are stable across regenerations.
    order = [(len(runs) - 1 - i, r) for i, r in enumerate(reversed(runs))]
    tabs = "".join(
        f'<button data-run="run-{i}" aria-selected="{str(i == order[0][0]).lower()}" '
        f'title="{E(r["_source"])} — {E(", ".join(r.get("_legs") or []))}">'
        f'{E(run_title(r))}</button>'
        for i, r in order
    )
    latest = runs[-1]
    gen = (latest.get("generated") or "")[:19].replace("T", " ")
    sections = "".join(
        run_section(r, i, i == order[0][0]) for i, r in order
    )
    sources = ", ".join(f"<code>{E(s)}</code>" for r in runs for s in (r.get("_sources") or [r["_source"]]))
    return f"""<!doctype html>
<html lang="en" data-theme="dark"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Closed-loop benchmark report</title>
<link rel="icon" href="data:image/svg+xml,{MARK_SVG_URI}">
<style>{CSS}</style></head><body>
<header class="top">
  <div class="brand">
    <span class="mark" aria-hidden="true">{MARK_INLINE}</span>
    <h1><span class="word">ilo</span><span class="sep">/</span><span class="what">closed-loop benchmark</span></h1>
  </div>
  <div class="sub">{len(runs)} run{'s' if len(runs) != 1 else ''} · newest {E(gen)}</div>
  <div class="sub wide">A model is asked to write each task in each language; measured per attempt-set are tokens, emitted characters, dollars and seconds. Program runtime is a separate bench (<code>bench/run.sh</code>).</div>
  <div class="spacer"></div>
  <button id="theme" title="Toggle light/dark">light</button>
</header>
<main>
  <div class="tabs">{tabs}</div>
  <details class="legend">
    <summary>What the run names mean</summary>
    <dl>
      <dt>warm cache</dt><dd>The provider's prefix cache is reused between repair turns, so repeated prompt prefixes bill at the cache rate. <strong>cold</strong> busts the cache on every attempt — an upper bound on input cost.</dd>
      <dt>aligned</dt><dd>The spec sits in the system message and repairs append turns, so the cached prefix survives the retry. <strong>rewritten prompt</strong> is the older shape: each repair rewrites the prompt, which breaks the prefix on purpose.</dd>
      <dt>curated / core / full spec</dt><dd>How much of ilo's own skill documentation the model was given in context. Comparators get their language's native agent-facing docs bundled instead.</dd>
    </dl>
  </details>
  <details class="legend">
    <summary>What the numbers mean</summary>
    <dl>
      <dt>generation tokens</dt><dd>Everything the model emitted, reasoning included. Total sums every task-row; mean/row divides by task-rows.</dd>
      <dt>code characters</dt><dd>Characters of the emitted program only — reasoning excluded. chars ÷ gen tokens is the density metric; higher is denser.</dd>
      <dt>execution time</dt><dd>Wall time of the whole attempt-set — every generation call plus every run of the program. It is not the program's own runtime (that is the wall-clock microbench in <code>bench/run.sh</code>).</dd>
      <dt>input tokens</dt><dd>Prompt tokens resent to the provider (spec + task + repairs), before cache discounts.</dd>
      <dt>reps</dt><dd>Measurements averaged into a task-row. The comparator matrix measures the ilo arm once per comparator leg, so ilo rows show ×N.</dd>
      <dt>working</dt><dd>Task-rows whose program ran and printed the expected output within the retry cap.</dd>
      <dt>best</dt><dd>In a per-task table, the fewest generation tokens among the languages that solved that task. It says who got there with the least emitted text, not who is fastest overall.</dd>
      <dt>not measured</dt><dd>This language has no row on this task in this tab — the sweep has not run it yet, or it is absent from this run's legs. It is a gap in coverage, never a result.</dd>
      <dt>trunc</dt><dd>Attempts that stopped at the model's output-token cap (<code>finish_reason=length</code>) and emitted no code at all. A truncated attempt is billed like any other and reads as an ordinary failure, so a non-zero count here means the row is measuring the cap, not the language. A dash means the run predates this field.</dd>
    </dl>
  </details>
  {sections}
</main>
<footer>
  Rendered from {sources} — the JSON is the measurement, this page is a view.
  Regenerate with <code>scripts/closed-loop-report-html.py</code>.
</footer>
<script>{JS}</script>
</body></html>
"""


MARK_INLINE = """<svg viewBox="0 0 32 32" width="26" height="26" role="img" aria-label="ilo">
  <rect width="32" height="32" rx="8" fill="#0a0a12"/>
  <circle cx="16" cy="16" r="10" fill="#f59e0b" opacity="0.12"/>
  <text x="16" y="22" text-anchor="middle" font-family="system-ui,sans-serif"
        font-weight="800" font-size="16" fill="#f59e0b">ilo</text>
</svg>"""

MARK_SVG_URI = (
    "%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32'"
    " height='32' rx='8' fill='%230a0a12'/%3E%3Ctext x='16' y='22' text-anchor='middle'"
    " font-family='system-ui,sans-serif' font-weight='800' font-size='16'"
    " fill='%23f59e0b'%3Eilo%3C/text%3E%3C/svg%3E"
)

CSS = """
:root{--bg:#0f1115;--panel:#161a21;--panel2:#1c2129;--fg:#e6e8ea;--dim:#98a0aa;
--line:#272c34;--accent:#6ea8fe;--ok:#3fb950;--warn:#d29922;--bad:#f85149;--bar:#2d4a7c;
--brand:#f59e0b}
@media (prefers-color-scheme: light){:root{--bg:#f6f7f9;--panel:#fff;--panel2:#f0f2f5;
--fg:#1b1f24;--dim:#5b6472;--line:#dfe3e8;--accent:#0b5cd5;--ok:#1a7f37;--warn:#9a6700;
--bad:#cf222e;--bar:#9cc0f5}}
:root[data-theme=light]{--bg:#f6f7f9;--panel:#fff;--panel2:#f0f2f5;--fg:#1b1f24;
--dim:#5b6472;--line:#dfe3e8;--accent:#0b5cd5;--ok:#1a7f37;--warn:#9a6700;--bad:#cf222e;
--bar:#9cc0f5}
:root[data-theme=dark]{--bg:#0f1115;--panel:#161a21;--panel2:#1c2129;--fg:#e6e8ea;
--dim:#98a0aa;--line:#272c34;--accent:#6ea8fe;--ok:#3fb950;--warn:#d29922;--bad:#f85149;
--bar:#2d4a7c}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:14px/1.5 ui-sans-serif,system-ui,
-apple-system,"Segoe UI",Roboto,sans-serif}
header.top{position:sticky;top:0;z-index:5;background:var(--panel);
border-bottom:1px solid var(--line);padding:12px 22px;display:flex;gap:16px;
align-items:center;flex-wrap:wrap}
header.top .brand{display:flex;align-items:center;gap:9px;min-width:0}
header.top .brand .mark{display:flex;flex:none;border-radius:8px;overflow:hidden}
header.top h1{margin:0;font-size:15px;font-weight:600;letter-spacing:-.01em;
white-space:nowrap;display:flex;align-items:baseline;gap:7px}
header.top h1 .word{color:var(--brand);font-weight:800;letter-spacing:-.02em}
header.top h1 .sep{color:var(--dim);font-weight:400}
header.top .sub{color:var(--dim);font-size:12px}
header.top .sub.wide{flex:1 0 100%;order:9;margin-top:-6px}
.spacer{flex:1}
main{max-width:1560px;margin:0 auto;padding:20px 22px 0}
button{font:inherit;color:var(--fg);background:var(--panel2);border:1px solid var(--line);
border-radius:8px;padding:5px 11px;cursor:pointer}
button:hover{border-color:var(--accent)}
.tabs{display:flex;gap:6px;flex-wrap:wrap;margin-bottom:12px}
.tabs button[aria-selected=true]{border-color:var(--accent);color:var(--accent);
background:color-mix(in srgb,var(--accent) 12%,transparent)}
.tabs button[aria-selected=false]{color:var(--dim)}
details.legend{background:var(--panel);border:1px solid var(--line);border-radius:10px;
padding:10px 14px;margin:0 0 18px}
details.legend summary{cursor:pointer;color:var(--dim);font-size:13px}
details.legend dl{margin:10px 0 2px;display:grid;grid-template-columns:max-content 1fr;
gap:6px 16px;font-size:13px}
details.legend dt{color:var(--fg);font-weight:600;white-space:nowrap}
details.legend dd{margin:0;color:var(--dim)}
section.run{display:none}
section.run.active{display:block}
section.run h2{margin:14px 0 2px;font-size:17px;letter-spacing:-.01em}
section.run h3{margin:26px 0 6px;font-size:13px;text-transform:uppercase;
letter-spacing:.06em;color:var(--dim);font-weight:600}
.meta{color:var(--fg);font-size:12.5px;opacity:.85}
.sub{color:var(--dim);font-size:12px;margin-top:2px}
.sub code{color:var(--fg);opacity:.8}
.note{color:var(--dim);font-size:12.5px;margin:0 0 10px;max-width:105ch}
.warn{color:var(--fg);font-size:13px;margin:0 0 14px;max-width:110ch;padding:10px 13px;
border:1px solid var(--bad);border-left-width:4px;border-radius:8px;background:var(--panel)}
.warn strong{color:var(--bad)}
.taskblock{margin:0 0 16px}
.taskhead{display:flex;align-items:baseline;gap:8px;flex-wrap:wrap;margin:0 0 4px}
.taskhead .tid{font-weight:700;font-size:13.5px;letter-spacing:-.01em}
.tag{font-size:11px;color:var(--dim);background:var(--panel2);
border:1px solid var(--line);border-radius:6px;padding:1px 7px}
.taskblock .note{margin:0 0 8px;max-width:100ch}
.pill.unmeasured{color:var(--dim);border-color:var(--line);background:transparent}
tr.unmeasured td{opacity:.5}
td.best{color:var(--ok);font-weight:700}
.badmark{color:var(--bad);font-weight:700}
.scroll{overflow-x:auto;border:1px solid var(--line);border-radius:10px;background:var(--panel)}
table.grid{border-collapse:separate;border-spacing:0;width:100%;font-size:13px}
table.grid th{position:sticky;top:0;background:var(--panel2);text-align:right;
padding:9px 12px;font-weight:600;color:var(--dim);font-size:11px;text-transform:uppercase;
letter-spacing:.04em;white-space:nowrap;border-bottom:1px solid var(--line);cursor:pointer;
user-select:none}
table.grid th[data-sort=text],table.grid td.lang,table.grid td:first-child{text-align:left}
table.grid th:hover{color:var(--accent)}
table.grid td{padding:8px 12px;border-bottom:1px solid var(--line);white-space:nowrap;
text-align:right}
table.grid tbody tr:last-child td{border-bottom:0}
table.grid tbody tr:hover{background:var(--panel2)}
td.num{font-variant-numeric:tabular-nums}
td.lang{font-weight:600}
td.ratio{font-variant-numeric:tabular-nums;font-weight:600}
td.ratio.ok{color:var(--ok)}
td.ratio.bad{color:var(--bad)}
.rep{color:var(--dim);font-weight:400;font-size:11px}
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(190px,1fr));gap:10px;
margin:14px 0 4px}
.card{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:12px 14px}
.card .k{color:var(--dim);font-size:11px;text-transform:uppercase;letter-spacing:.05em}
.card .v{font-size:22px;font-weight:700;letter-spacing:-.02em;margin:3px 0 1px;
font-variant-numeric:tabular-nums}
.card .s{color:var(--dim);font-size:11.5px}
th.barhead,td.bar{width:130px;padding:8px 12px}
td.bar span{display:block;height:7px;background:var(--bar);border-radius:4px;min-width:2px}
.pill{padding:2px 9px;border-radius:999px;font-size:11px;border:1px solid}
.pill.working{color:var(--ok);border-color:var(--ok)}
.pill.partial{color:var(--warn);border-color:var(--warn)}
.pill.failed,.pill.unknown{color:var(--bad);border-color:var(--bad)}
.chip{display:inline-block;min-width:22px;text-align:center;margin-right:4px;padding:1px 6px;
border-radius:6px;font-size:11px;background:var(--panel2);border:1px solid var(--line)}
.chip.working{color:var(--ok)}.chip.partial{color:var(--warn)}.chip.failed{color:var(--bad)}
footer{color:var(--dim);font-size:12px;padding:26px 22px;border-top:1px solid var(--line);
margin-top:30px}
footer code{color:var(--fg)}
"""

JS = """
const q=new URLSearchParams(location.search);
function theme(t){document.documentElement.dataset.theme=t;
  localStorage.setItem('benchTheme',t);
  document.getElementById('theme').textContent=(t==='dark')?'light':'dark';}
document.getElementById('theme').onclick=()=>theme(
  document.documentElement.dataset.theme==='dark'?'light':'dark');
theme(localStorage.getItem('benchTheme')
  || (matchMedia('(prefers-color-scheme: light)').matches?'light':'dark'));

const tabs=[...document.querySelectorAll('.tabs button')];
function show(id){
  tabs.forEach(b=>b.setAttribute('aria-selected',String(b.dataset.run===id)));
  document.querySelectorAll('section.run').forEach(
    s=>s.classList.toggle('active',s.id===id));
  history.replaceState(null,'','?run='+id);
}
tabs.forEach(b=>b.onclick=()=>show(b.dataset.run));
const want=q.get('run');
show(tabs.some(b=>b.dataset.run===want)?want:(tabs[0]&&tabs[0].dataset.run));

function cellValue(tr,i,kind){
  const td=tr.children[i];
  if(!td)return '';
  const raw=td.dataset.v!==undefined?td.dataset.v:td.textContent.trim();
  return raw;
}
document.querySelectorAll('table.sortable').forEach(table=>{
  const heads=[...table.querySelectorAll('th')];
  heads.forEach((th,i)=>th.addEventListener('click',()=>{
    const kind=th.dataset.sort||'text';
    const dir=th.dataset.dir==='asc'?-1:1;
    heads.forEach(h=>{if(h!==th)delete h.dataset.dir;});
    th.dataset.dir=(dir===1)?'desc':'asc';
    const tbody=table.querySelector('tbody');
    const rows=[...tbody.querySelectorAll('tr')];
    rows.sort((a,b)=>{
      const va=cellValue(a,i,kind),vb=cellValue(b,i,kind);
      if(kind==='num'){
        const na=parseFloat(String(va).replace(/[^0-9.\\-]/g,''));
        const nb=parseFloat(String(vb).replace(/[^0-9.\\-]/g,''));
        const fa=isNaN(na),fb=isNaN(nb);
        if(fa&&fb)return 0;
        if(fa)return 1;            // missing values always sink
        if(fb)return -1;
        return (na-nb)*dir;
      }
      return String(va).localeCompare(String(vb))*dir;
    });
    rows.forEach(r=>tbody.appendChild(r));
  }));
});
"""


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("paths", nargs="*", help="run JSON files (default: all bench runs)")
    ap.add_argument("-o", "--out", default=str(BENCH_DIR / "report.html"),
                    help="output HTML path")
    args = ap.parse_args()

    paths = ([Path(p) for p in args.paths] if args.paths
             else sorted(BENCH_DIR.glob("closed-loop-*.json")))
    paths = [p for p in paths if p.is_file()]
    if not paths:
        print("[report] no run JSON found", file=sys.stderr)
        return 1

    runs = load_runs(paths)
    if not runs:
        print("[report] nothing to render", file=sys.stderr)
        return 1

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(build_html(runs))
    print(f"[report] {len(runs)} run(s), {sum(len(r['results']) for r in runs)} rows "
          f"→ {out} ({out.stat().st_size:,} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
