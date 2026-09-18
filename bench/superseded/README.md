# Superseded runs

Runs kept as evidence of a measurement defect, not as measurements. Nothing here
is read by `closed-loop-report-html.py` (it globs `bench/closed-loop-*.json`,
non-recursively).

| File | Why it is not a result |
|---|---|
| `closed-loop-2026-09-18-warm-align-curated-bash.cap16384.{json,md}` | Measured at `max_tokens=16384`, which the dsflash reasoning trace exceeds. Its ilo `run-length-encode` row has `code_chars_by_turn=[329,288,0,0,274]` — two of five attempts emitted **zero characters** at `finish_reason=length`, billed as ordinary failures. Superseded by the same leg re-run at 65536. |
