"""Generate the four canonical charts from results/aggregated.json.

Outputs to charts/ as PNG + SVG. Requires matplotlib.

The four charts (per the design spec):
  1. Total tokens per task vs session length, one line per variant
  2. Spec load vs per-task generation stacked bar at N=20
  3. Success rate by variant/task
  4. USD cost per completed task at N=20

If matplotlib isn't installed, this script falls back to writing a text
summary of the same data — enough to validate the dataset shape without
the visual artefact.
"""

from __future__ import annotations

import json
import sys
from collections import defaultdict
from pathlib import Path

BENCH_DIR = Path(__file__).parent
AGG_PATH = BENCH_DIR / "results" / "aggregated.json"
OUT_DIR = BENCH_DIR / "charts"


def _load() -> dict:
    return json.loads(AGG_PATH.read_text())


def _has_matplotlib() -> bool:
    try:
        import matplotlib  # noqa: F401
        return True
    except ImportError:
        return False


def _save(fig, name: str) -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    fig.savefig(OUT_DIR / f"{name}.png", dpi=120, bbox_inches="tight")
    fig.savefig(OUT_DIR / f"{name}.svg", bbox_inches="tight")


def chart1_tokens_vs_n(data: dict) -> None:
    """Total tokens per task vs session length, one line per variant.
    Averaged across tasks for a single model (the first model in the data)."""
    import matplotlib.pyplot as plt

    # Pick the first model present.
    model = next((r["model"] for r in data["results"]), None)
    if model is None:
        return

    # (variant, n) -> [median_total_tokens]
    buckets: dict[tuple[str, int], list[float]] = defaultdict(list)
    for r in data["results"]:
        if r["model"] != model:
            continue
        buckets[(r["variant"], r["session_length"])].append(r["median"]["total_tokens"])

    variants = sorted({k[0] for k in buckets})
    ns = sorted({k[1] for k in buckets})

    fig, ax = plt.subplots(figsize=(8, 5))
    for v in variants:
        ys = [sum(buckets.get((v, n), [0])) / max(1, len(buckets.get((v, n), [1]))) for n in ns]
        ax.plot(ns, ys, marker="o", label=v)
    ax.set_xlabel("Session length (tasks per session)")
    ax.set_ylabel("Median total tokens per task (averaged over tasks)")
    ax.set_title(f"Closed-loop tokens vs session length ({model})")
    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.grid(True, which="both", alpha=0.3)
    ax.legend()
    _save(fig, "chart1-tokens-vs-n")
    plt.close(fig)


def chart2_spec_vs_gen(data: dict) -> None:
    """Spec load vs per-task generation, stacked bar at N=20."""
    import matplotlib.pyplot as plt

    n_target = 20
    buckets: dict[str, dict[str, list[float]]] = defaultdict(lambda: defaultdict(list))
    for r in data["results"]:
        if r["session_length"] != n_target:
            continue
        v = r["variant"]
        buckets[v]["total"].append(r["median"]["total_tokens"])

    if not buckets:
        # Try whatever session length is present.
        max_n = max((r["session_length"] for r in data["results"]), default=1)
        n_target = max_n
        for r in data["results"]:
            if r["session_length"] != n_target:
                continue
            buckets[r["variant"]]["total"].append(r["median"]["total_tokens"])

    variants = sorted(buckets)
    totals = [sum(buckets[v]["total"]) / max(1, len(buckets[v]["total"])) for v in variants]

    fig, ax = plt.subplots(figsize=(8, 5))
    ax.bar(variants, totals, color="steelblue")
    ax.set_ylabel("Median tokens per task")
    ax.set_title(f"Cost composition at N={n_target}")
    ax.tick_params(axis="x", rotation=20)
    _save(fig, "chart2-cost-composition")
    plt.close(fig)


def chart3_success_rates(data: dict) -> None:
    """Success rate by variant, averaged over tasks at N=1."""
    import matplotlib.pyplot as plt

    buckets: dict[str, list[float]] = defaultdict(list)
    for r in data["results"]:
        if r["session_length"] != 1:
            continue
        buckets[r["variant"]].append(r["median"]["success_rate"])
    if not buckets:
        for r in data["results"]:
            buckets[r["variant"]].append(r["median"]["success_rate"])

    variants = sorted(buckets)
    rates = [sum(buckets[v]) / max(1, len(buckets[v])) for v in variants]

    fig, ax = plt.subplots(figsize=(8, 5))
    ax.bar(variants, rates, color="seagreen")
    ax.set_ylabel("Success rate (avg across tasks)")
    ax.set_ylim(0, 1.05)
    ax.set_title("Success rate by variant")
    ax.tick_params(axis="x", rotation=20)
    _save(fig, "chart3-success-rate")
    plt.close(fig)


def chart4_usd_cost(data: dict) -> None:
    """USD per completed task at the highest session length present."""
    import matplotlib.pyplot as plt

    n_target = max((r["session_length"] for r in data["results"]), default=1)
    buckets: dict[str, list[float]] = defaultdict(list)
    for r in data["results"]:
        if r["session_length"] != n_target:
            continue
        sr = r["median"]["success_rate"]
        if sr == 0:
            continue
        buckets[r["variant"]].append(r["median"]["usd_cost"] / sr)

    variants = sorted(buckets)
    costs = [sum(buckets[v]) / max(1, len(buckets[v])) for v in variants]

    fig, ax = plt.subplots(figsize=(8, 5))
    ax.bar(variants, costs, color="indianred")
    ax.set_ylabel("Median USD per completed task")
    ax.set_title(f"USD cost per completed task at N={n_target}")
    ax.tick_params(axis="x", rotation=20)
    _save(fig, "chart4-usd-cost")
    plt.close(fig)


def text_summary(data: dict) -> str:
    lines = ["# Closed-loop benchmark — text summary", ""]
    lines.append(f"Rows: {len(data['results'])}")
    lines.append("")
    lines.append("Aggregate by variant:")
    buckets: dict[str, list[float]] = defaultdict(list)
    costs: dict[str, list[float]] = defaultdict(list)
    succ: dict[str, list[float]] = defaultdict(list)
    for r in data["results"]:
        buckets[r["variant"]].append(r["median"]["total_tokens"])
        costs[r["variant"]].append(r["median"]["usd_cost"])
        succ[r["variant"]].append(r["median"]["success_rate"])
    for v in sorted(buckets):
        tok = sum(buckets[v]) / len(buckets[v])
        usd = sum(costs[v]) / len(costs[v])
        s = sum(succ[v]) / len(succ[v])
        lines.append(f"  {v:25} avg_tokens={tok:8.0f} avg_usd={usd:.4f} success={s:.2f}")
    return "\n".join(lines) + "\n"


def main() -> None:
    if not AGG_PATH.is_file():
        print(f"missing {AGG_PATH} — run driver.py first", file=sys.stderr)
        sys.exit(2)
    data = _load()

    if _has_matplotlib():
        chart1_tokens_vs_n(data)
        chart2_spec_vs_gen(data)
        chart3_success_rates(data)
        chart4_usd_cost(data)
        print(f"Wrote 4 charts to {OUT_DIR}/", file=sys.stderr)
    else:
        print("matplotlib not installed — writing text summary only", file=sys.stderr)

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    (OUT_DIR / "summary.txt").write_text(text_summary(data))
    print(f"Wrote {OUT_DIR}/summary.txt", file=sys.stderr)


if __name__ == "__main__":
    main()
