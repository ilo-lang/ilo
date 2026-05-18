"""Closed-loop benchmark driver.

Drives an LLM (or a mock) through generation -> compile -> repair -> retry
until the generated source passes a test harness, measuring tokens, time, and
success rate per (variant, task, session-length, model, seed) tuple.

Usage:
    # Mock mode (no API key required) — exercises the loop end-to-end
    python3 driver.py --mock --variant python --task 01-simple-function --n 1

    # Sample matrix run with mock — produces results/raw/*.json
    python3 driver.py --mock --sample

    # Full matrix with real Anthropic API (requires ANTHROPIC_API_KEY)
    python3 driver.py --full

Mock mode injects synthetic LLM behaviour:
  - first attempt: a slightly broken reference impl
  - second attempt: the correct reference impl
This is enough to exercise the compile -> repair -> retry path without API spend.
The numbers from mock mode are NOT publishable — they exist to validate plumbing.

Real mode caches the system prompt via Anthropic's cache_control mechanism.
Cache hit rates are logged. Budget caps and seed counts come from CLI flags.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import sys
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any

from variants import get_variant


BENCH_DIR = Path(__file__).parent
TASKS_DIR = BENCH_DIR / "tasks"
PROMPTS_DIR = BENCH_DIR / "prompts"
RESULTS_DIR = BENCH_DIR / "results"
RAW_DIR = RESULTS_DIR / "raw"
DATA_DIR = BENCH_DIR / "data"


# Variants under test. The base adapter is python/ilo/zero; the "ilo-pre/post"
# distinction is in the system prompt loaded.
VARIANT_LIST = [
    "python-baseline",
    "ilo-pre-phase-1",
    "ilo-post-phase-1",
    "zero",
]

SESSION_LENGTHS = [1, 5, 20, 50, 100]

MODELS = [
    "claude-haiku-4-5-20251001",
    "claude-sonnet-4-5",
]

RETRY_CAP = 5


# --- Spec sizes (in characters, approximate) -----------------------------

# These match the spec_sizes table from the design doc. Exact ai.txt token
# counts can be computed at run time by the real driver; the mock uses these
# rough sizes to simulate cache amortisation.
SPEC_CHARS = {
    "python-baseline": 0,
    "ilo-pre-phase-1": 16000,
    "ilo-post-phase-1": 4800,
    "zero": 4300,
}


@dataclass
class TaskResult:
    variant: str
    task: str
    session_length: int
    model: str
    seed: int
    total_tokens: int
    spec_load_tokens: int
    first_try_generation_tokens: int
    retry_count: int
    retry_tokens: int
    time_seconds: float
    success: bool
    usd_cost: float
    cache_hit_rate: float
    notes: list[str] = field(default_factory=list)


# --- Pricing -------------------------------------------------------------

# Per-million-token rates (USD). Update when Anthropic prices change.
PRICING = {
    "claude-haiku-4-5-20251001": {"input": 0.80, "output": 4.00, "cache_read": 0.08, "cache_write": 1.00},
    "claude-sonnet-4-5":         {"input": 3.00, "output": 15.00, "cache_read": 0.30, "cache_write": 3.75},
}


def estimate_cost(model: str, input_tokens: int, output_tokens: int,
                  cache_read_tokens: int = 0, cache_write_tokens: int = 0) -> float:
    p = PRICING.get(model)
    if p is None:
        return 0.0
    return (
        input_tokens / 1_000_000 * p["input"]
        + output_tokens / 1_000_000 * p["output"]
        + cache_read_tokens / 1_000_000 * p["cache_read"]
        + cache_write_tokens / 1_000_000 * p["cache_write"]
    )


# --- LLM client abstraction ----------------------------------------------

class MockLLM:
    """Deterministic mock that returns broken-then-fixed source.

    Token counts are approximated from len(output)//4 — mirrors the cl100k
    ratio for English/code closely enough for plumbing validation.
    """

    def __init__(self, task: dict, variant: str, seed: int):
        self.task = task
        self.variant = variant
        self.seed = seed
        self.attempts = 0
        rng = random.Random(seed)
        # Roughly 30% chance the mock succeeds first try, else needs 1 retry.
        self._first_try_ok = rng.random() < 0.7 if variant != "ilo-pre-phase-1" else rng.random() < 0.5

    def _reference(self) -> str:
        impl_key = self.variant.split("-")[0]
        return self.task["reference_impls"].get(impl_key, "")

    def _broken(self) -> str:
        # Deliberately break syntax in a way the compiler will reject.
        ref = self._reference()
        if self.variant.startswith("python"):
            return ref.replace("def ", "def_BROKEN_")
        if self.variant.startswith("ilo"):
            return ref.replace(";", "$$$", 1) if ";" in ref else ref + "$$$"
        if self.variant.startswith("zero"):
            return ref.replace("pub fun", "pub_fun_BROKEN", 1)
        return ref + "\n???"

    def generate(self, _prompt: str) -> tuple[str, int, int]:
        """Return (source, input_tokens, output_tokens)."""
        self.attempts += 1
        if self._first_try_ok or self.attempts >= 2:
            src = self._reference()
        else:
            src = self._broken()
        # Approx tokens.
        out_tokens = max(1, len(src) // 4)
        in_tokens = 100 + len(_prompt) // 4
        return src, in_tokens, out_tokens


class AnthropicLLM:
    """Real Anthropic client with prompt caching for the system spec."""

    def __init__(self, model: str, system_prompt: str):
        try:
            import anthropic  # type: ignore
        except ImportError as exc:
            raise RuntimeError(
                "anthropic SDK not installed. Run: pip install anthropic"
            ) from exc
        self.client = anthropic.Anthropic()
        self.model = model
        # Use cache_control on the system block so subsequent tasks in the
        # session pay the 10% cached-read rate, not full input rate.
        self.system_blocks = [
            {"type": "text", "text": system_prompt,
             "cache_control": {"type": "ephemeral"}}
        ]
        self.history: list[dict] = []
        self.cache_reads = 0
        self.cache_writes = 0
        self.input_tokens = 0
        self.output_tokens = 0

    def generate(self, prompt: str) -> tuple[str, int, int]:
        self.history.append({"role": "user", "content": prompt})
        resp = self.client.messages.create(
            model=self.model,
            max_tokens=2048,
            system=self.system_blocks,
            messages=self.history,
        )
        text = "".join(b.text for b in resp.content if getattr(b, "type", None) == "text")
        # Strip code fences if the model added them despite instructions.
        text = _strip_fences(text)
        self.history.append({"role": "assistant", "content": text})

        usage = getattr(resp, "usage", None)
        in_tok = getattr(usage, "input_tokens", 0) or 0
        out_tok = getattr(usage, "output_tokens", 0) or 0
        cache_read = getattr(usage, "cache_read_input_tokens", 0) or 0
        cache_write = getattr(usage, "cache_creation_input_tokens", 0) or 0
        self.input_tokens += in_tok
        self.output_tokens += out_tok
        self.cache_reads += cache_read
        self.cache_writes += cache_write
        return text, in_tok + cache_read + cache_write, out_tok


def _strip_fences(text: str) -> str:
    t = text.strip()
    if t.startswith("```"):
        first_nl = t.find("\n")
        if first_nl != -1:
            t = t[first_nl + 1:]
        if t.endswith("```"):
            t = t[: -3]
    return t.strip() + "\n"


# --- Spec loading --------------------------------------------------------

def load_spec_for_variant(variant: str) -> str:
    """Return the system prompt for a given variant.

    For "ilo-post-phase-1" this loads only the relevant skill modules
    (simulated as a smaller slice of ai.txt when modular skills aren't
    yet shipped).
    """
    base = (PROMPTS_DIR / "system-base.md").read_text()

    if variant == "python-baseline":
        return base + "\n\n" + (PROMPTS_DIR / "system-python.md").read_text()

    if variant.startswith("ilo"):
        lang = (PROMPTS_DIR / "system-ilo.md").read_text()
        # Phase-1 simulation: load only a slice of ai.txt for post-phase-1.
        ai_txt = Path.home() / "code/ilo-lang/ilo/ai.txt"
        if ai_txt.is_file():
            full = ai_txt.read_text()
            if variant == "ilo-post-phase-1":
                # Take ~30% of the spec (proxy for "2 relevant skill modules").
                slice_len = max(1, len(full) // 3)
                spec_body = full[:slice_len]
            else:
                spec_body = full
        else:
            spec_body = ""
        return base + "\n\n" + lang + "\n\n## Language spec\n\n" + spec_body

    if variant == "zero":
        return base + "\n\n" + (PROMPTS_DIR / "system-zero.md").read_text()

    raise ValueError(f"unknown variant: {variant}")


# --- The repair loop -----------------------------------------------------

def run_single_task(variant: str, task: dict, model: str, seed: int,
                    is_first_in_session: bool, mock: bool,
                    llm: Any) -> TaskResult:
    """Execute one task: generation, compile, repair, retry, test.

    Returns a TaskResult with token + time + success metrics.
    """
    adapter = get_variant(variant)
    workdir = Path("/tmp") / f"bench-{variant}-{task['id']}-{seed}"
    workdir.mkdir(parents=True, exist_ok=True)

    prompt_intro = (
        "Task:\n" + task["prompt"]
        + "\nReturn ONLY the source code. No markdown fences, no commentary."
    )

    t0 = time.time()
    spec_tokens_charged = SPEC_CHARS.get(variant, 0) // 4 if is_first_in_session else int(SPEC_CHARS.get(variant, 0) / 4 * 0.10)

    source, in_tok, out_tok = llm.generate(prompt_intro)
    first_try_gen = out_tok
    total_in = in_tok
    total_out = out_tok
    retries = 0
    retry_tokens = 0
    success = False
    notes: list[str] = []

    for attempt in range(RETRY_CAP + 1):
        check = adapter.compile_check(source, workdir)
        if not check.ok:
            if attempt == RETRY_CAP:
                notes.append(f"compile failed after {RETRY_CAP} retries: {check.error[:200]}")
                break
            retries += 1
            fix_prompt = (
                f"Your previous output failed to compile:\n\n{check.error}\n\n"
                f"Previous source:\n{source}\n\n"
                "Return a corrected full source file. Only the code, no commentary."
            )
            source, in_tok, out_tok = llm.generate(fix_prompt)
            total_in += in_tok
            total_out += out_tok
            retry_tokens += out_tok
            continue

        test = adapter.run_tests(source, task, workdir)
        if test.ok:
            success = True
            break
        if attempt == RETRY_CAP:
            notes.append(f"tests failed after {RETRY_CAP} retries: {test.error[:200]}")
            break
        retries += 1
        fix_prompt = (
            f"Your code compiles, but the tests failed:\n\n{test.error}\n\n"
            f"Source:\n{source}\n\n"
            "Return a corrected full source file."
        )
        source, in_tok, out_tok = llm.generate(fix_prompt)
        total_in += in_tok
        total_out += out_tok
        retry_tokens += out_tok

    elapsed = time.time() - t0
    cache_hit_rate = 0.0
    if isinstance(llm, AnthropicLLM):
        denom = llm.cache_reads + llm.cache_writes + llm.input_tokens
        cache_hit_rate = llm.cache_reads / denom if denom else 0.0
        usd = estimate_cost(model, llm.input_tokens, llm.output_tokens,
                            llm.cache_reads, llm.cache_writes)
    else:
        # Mock pricing — assume mostly cached after first task.
        usd = estimate_cost(model, total_in if is_first_in_session else 0,
                            total_out,
                            0 if is_first_in_session else total_in,
                            spec_tokens_charged if is_first_in_session else 0)
        cache_hit_rate = 0.0 if is_first_in_session else 0.85

    return TaskResult(
        variant=variant,
        task=task["id"],
        session_length=0,  # filled in by caller
        model=model,
        seed=seed,
        total_tokens=total_in + total_out + spec_tokens_charged,
        spec_load_tokens=spec_tokens_charged,
        first_try_generation_tokens=first_try_gen,
        retry_count=retries,
        retry_tokens=retry_tokens,
        time_seconds=round(elapsed, 3),
        success=success,
        usd_cost=round(usd, 5),
        cache_hit_rate=round(cache_hit_rate, 3),
        notes=notes,
    )


def run_session(variant: str, task: dict, session_length: int, model: str,
                seed: int, mock: bool) -> list[TaskResult]:
    """Run `session_length` repeats of `task` under one cached system prompt."""
    system_prompt = load_spec_for_variant(variant)
    if mock:
        llm = MockLLM(task, variant, seed)
    else:
        llm = AnthropicLLM(model, system_prompt)

    results: list[TaskResult] = []
    for i in range(session_length):
        # Reset mock attempt counter between tasks so each task gets a fresh
        # try at the broken-then-fixed sequence.
        if mock:
            llm.attempts = 0
        r = run_single_task(variant, task, model, seed,
                            is_first_in_session=(i == 0), mock=mock, llm=llm)
        r.session_length = session_length
        results.append(r)
    return results


# --- Matrix orchestration ------------------------------------------------

def load_tasks() -> list[dict]:
    return sorted(
        (json.loads(p.read_text()) for p in TASKS_DIR.glob("*.json")),
        key=lambda t: t["id"],
    )


def run_matrix(variants: list[str], tasks: list[dict], lengths: list[int],
               models: list[str], seeds: int, mock: bool,
               budget_cap_usd: float) -> dict:
    RAW_DIR.mkdir(parents=True, exist_ok=True)
    all_results: list[TaskResult] = []
    spent = 0.0
    cells_run = 0
    cells_total = len(variants) * len(tasks) * len(lengths) * len(models) * seeds

    for variant in variants:
        for task in tasks:
            for length in lengths:
                for model in models:
                    for seed in range(seeds):
                        cells_run += 1
                        print(
                            f"[{cells_run}/{cells_total}] variant={variant} "
                            f"task={task['id']} N={length} model={model} seed={seed}",
                            file=sys.stderr,
                        )
                        if spent >= budget_cap_usd and not mock:
                            print(f"  budget cap (${budget_cap_usd}) reached — stopping",
                                  file=sys.stderr)
                            return _aggregate(all_results)
                        try:
                            rs = run_session(variant, task, length, model, seed, mock)
                        except Exception as exc:
                            print(f"  ERROR: {exc}", file=sys.stderr)
                            continue
                        cell_cost = sum(r.usd_cost for r in rs)
                        spent += cell_cost
                        all_results.extend(rs)
                        cell_summary = {
                            "variant": variant, "task": task["id"], "n": length,
                            "model": model, "seed": seed,
                            "successes": sum(1 for r in rs if r.success),
                            "total_tokens": sum(r.total_tokens for r in rs),
                            "usd_cost": round(cell_cost, 5),
                            "cache_hit_rate": rs[-1].cache_hit_rate if rs else 0.0,
                        }
                        raw_path = (RAW_DIR /
                                    f"{variant}__{task['id']}__N{length}__{model}__seed{seed}.json")
                        raw_path.write_text(json.dumps(
                            {"summary": cell_summary,
                             "per_task": [asdict(r) for r in rs]}, indent=2))

    return _aggregate(all_results)


def _aggregate(results: list[TaskResult]) -> dict:
    """Median + variance per (variant, task, N, model) across seeds."""
    from statistics import median, pstdev

    grouped: dict[tuple, list[TaskResult]] = {}
    for r in results:
        key = (r.variant, r.task, r.session_length, r.model)
        grouped.setdefault(key, []).append(r)

    out_rows = []
    for (variant, task, n, model), rs in sorted(grouped.items()):
        totals = [r.total_tokens for r in rs]
        times = [r.time_seconds for r in rs]
        usds = [r.usd_cost for r in rs]
        successes = [1.0 if r.success else 0.0 for r in rs]
        retries = [r.retry_count for r in rs]
        out_rows.append({
            "variant": variant,
            "task": task,
            "session_length": n,
            "model": model,
            "samples": len(rs),
            "median": {
                "total_tokens": median(totals),
                "time_seconds": median(times),
                "usd_cost": round(median(usds), 5),
                "success_rate": sum(successes) / len(successes),
                "retry_count_avg": sum(retries) / len(retries),
            },
            "variance": {
                "total_tokens_stdev": round(pstdev(totals), 1) if len(totals) > 1 else 0.0,
                "time_seconds_stdev": round(pstdev(times), 3) if len(times) > 1 else 0.0,
            },
        })

    return {
        "metadata": {
            "ran_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "spec_sizes_chars": SPEC_CHARS,
            "retry_cap": RETRY_CAP,
        },
        "results": out_rows,
    }


# --- Entry point ---------------------------------------------------------

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mock", action="store_true",
                    help="Use mock LLM (no API key required)")
    ap.add_argument("--full", action="store_true",
                    help="Run the full 5x5x5x2 matrix (real API only)")
    ap.add_argument("--sample", action="store_true",
                    help="Run a small validation sample matrix")
    ap.add_argument("--variant", default=None, help="Single variant")
    ap.add_argument("--task", default=None, help="Single task id")
    ap.add_argument("--n", type=int, default=None, help="Session length")
    ap.add_argument("--model", default=None, help="Single model")
    ap.add_argument("--seeds", type=int, default=3)
    ap.add_argument("--budget", type=float, default=300.0,
                    help="Anthropic API spend cap (USD)")
    ap.add_argument("--out", default=None, help="Aggregated output path")
    args = ap.parse_args()

    tasks = load_tasks()
    if args.task:
        tasks = [t for t in tasks if t["id"] == args.task or t["name"] == args.task]
        if not tasks:
            print(f"task not found: {args.task}", file=sys.stderr); sys.exit(2)

    if args.full:
        variants, lengths, models, seeds = VARIANT_LIST, SESSION_LENGTHS, MODELS, args.seeds
    elif args.sample:
        variants = ["python-baseline", "ilo-post-phase-1", "zero"]
        lengths = [1, 5, 20]
        models = [MODELS[0]]
        seeds = max(1, args.seeds)
    else:
        variants = [args.variant] if args.variant else ["python-baseline"]
        lengths = [args.n] if args.n else [1]
        models = [args.model] if args.model else [MODELS[0]]
        seeds = args.seeds

    if not args.mock and not os.environ.get("ANTHROPIC_API_KEY"):
        print("ANTHROPIC_API_KEY not set — pass --mock for a dry run, or export the key.",
              file=sys.stderr)
        sys.exit(2)

    print(f"Running matrix: variants={variants} tasks={[t['id'] for t in tasks]} "
          f"lengths={lengths} models={models} seeds={seeds} mock={args.mock}",
          file=sys.stderr)

    agg = run_matrix(variants, tasks, lengths, models, seeds,
                     mock=args.mock, budget_cap_usd=args.budget)

    out_path = Path(args.out) if args.out else (RESULTS_DIR / "aggregated.json")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(agg, indent=2))
    print(f"\nWrote {out_path}  ({len(agg['results'])} rows)", file=sys.stderr)

    # Also drop a CSV copy under data/ for easy chart import.
    csv_path = DATA_DIR / "aggregated.csv"
    csv_path.parent.mkdir(parents=True, exist_ok=True)
    write_csv(agg, csv_path)
    print(f"Wrote {csv_path}", file=sys.stderr)


def write_csv(agg: dict, path: Path) -> None:
    import csv
    rows = agg["results"]
    with path.open("w", newline="") as f:
        w = csv.writer(f)
        w.writerow([
            "variant", "task", "session_length", "model", "samples",
            "median_total_tokens", "median_time_seconds", "median_usd_cost",
            "success_rate", "retry_count_avg",
            "stdev_total_tokens", "stdev_time_seconds",
        ])
        for r in rows:
            w.writerow([
                r["variant"], r["task"], r["session_length"], r["model"], r["samples"],
                r["median"]["total_tokens"], r["median"]["time_seconds"],
                r["median"]["usd_cost"], r["median"]["success_rate"],
                r["median"]["retry_count_avg"],
                r["variance"]["total_tokens_stdev"], r["variance"]["time_seconds_stdev"],
            ])


if __name__ == "__main__":
    main()
