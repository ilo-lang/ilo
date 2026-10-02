"""Fork 5 / exp-11 — constrained-decode backend for the closed-loop harness.

Wired from ``closed-loop-bench.py`` after fence-strip and before header,
meta, or soft-edge recovery, so a reject sees the raw model emit.

DeepSeek hosted Chat Completions cannot attach logit or grammar masks.
``RejectRetryBackend`` is the feasible path: post-emit ``ilo check``
(JSON when the CLI accepts it; NDJSON diagnostics on stderr otherwise)
then a structured repair signal. ``LocalMaskBackend`` is a placeholder
for a future guided local sampler. It is not a DeepSeek mask.

No network. Unit-test friendly. British English comments.
"""

from __future__ import annotations

import json
import os
import subprocess
import tempfile
from dataclasses import dataclass, field
from typing import Any, Protocol


# Primary illegal families from exp-07 failure modes (artefact reds).
TARGET_ILLEGAL_CODES = ("ILO-P023", "ILO-P009", "ILO-P007", "ILO-P011")


@dataclass
class ConstrainResult:
    """Outcome of one post-emit (or request-attach) constrain pass."""

    ok: bool
    mode: str
    codes: list[str] = field(default_factory=list)
    repair_signal: str | None = None  # structured text for the next user turn
    raw: dict[str, Any] | None = None  # optional check --json body


class ConstrainBackend(Protocol):
    """Attach site for Fork 5.

    - attach_request: mutate / annotate the provider JSON body (noop on DeepSeek).
    - validate_emit: post-fence-strip programme text → accept or reject+signal.
    """

    name: str

    def attach_request(self, body: dict[str, Any]) -> dict[str, Any]:
        ...

    def validate_emit(self, code: str) -> ConstrainResult:
        ...


class NoopBackend:
    """Control arm — constrain off."""

    name = "none"

    def attach_request(self, body: dict[str, Any]) -> dict[str, Any]:
        return body

    def validate_emit(self, code: str) -> ConstrainResult:
        return ConstrainResult(ok=True, mode=self.name)


class RejectRetryBackend:
    """Post-emit grammar reject via ``ilo check`` subprocess.

    Feasible on the live DeepSeek OpenAI-compatible path. Does not mask
    logits; it refuses an illegal full programme before run/judge and
    returns a structured repair signal for the closed loop.
    """

    name = "reject-retry"

    def __init__(self, ilo_bin: str = "ilo", timeout_s: float = 10.0) -> None:
        self.ilo_bin = ilo_bin
        self.timeout_s = timeout_s

    def attach_request(self, body: dict[str, Any]) -> dict[str, Any]:
        # DeepSeek: nothing to attach. Keep the hook so LocalMask / a future
        # provider grammar field can land without rewriting call_deepseek.
        return body

    def validate_emit(self, code: str) -> ConstrainResult:
        text = (code or "").strip()
        if not text:
            return ConstrainResult(
                ok=False,
                mode=self.name,
                codes=["EMPTY"],
                repair_signal=(
                    "ILO-CONSTRAIN: empty programme. Emit a complete ilo "
                    "programme with a typed main header (e.g. main>t)."
                ),
            )

        raw, stderr, rc = self._run_check(text)
        if rc == 0:
            return ConstrainResult(ok=True, mode=self.name, raw=raw)

        codes = _codes_from_check(raw, stderr)
        signal = _repair_signal(codes, stderr, raw)
        return ConstrainResult(
            ok=False,
            mode=self.name,
            codes=codes,
            repair_signal=signal,
            raw=raw,
        )

    def _run_check(self, code: str) -> tuple[dict[str, Any] | None, str, int]:
        """Prefer ``ilo check <file> --json``; fall back to plain check.

        Diagnostics are NDJSON on stderr when stderr is not a TTY (the
        subprocess case), including without an explicit ``--json`` once
        auto-detect chooses JSON. ``--json`` is requested anyway so a TTY
        capture still asks for machine-readable codes.
        """
        with tempfile.NamedTemporaryFile(
            suffix=".ilo", mode="w", delete=False, encoding="utf-8"
        ) as f:
            f.write(code)
            path = f.name
        try:
            for args in (
                [self.ilo_bin, "check", path, "--json"],
                [self.ilo_bin, "check", "--json", path],
                [self.ilo_bin, "check", path],
            ):
                try:
                    r = subprocess.run(
                        args,
                        capture_output=True,
                        text=True,
                        timeout=self.timeout_s,
                    )
                except FileNotFoundError:
                    return None, f"ilo binary not found: {self.ilo_bin}", 127
                except subprocess.TimeoutExpired:
                    return None, "ilo check timeout", 124

                raw = _payload_from_streams(r.stdout or "", r.stderr or "")
                # Unsupported --json: usage / clap noise, no diagnostic.
                # A real failure carries ILO- codes or parsed JSON; keep it.
                if "--json" in args and _json_flag_unsupported(r.stderr or "", r.returncode, raw):
                    continue
                detail = (r.stderr or r.stdout or "")
                return raw, detail, r.returncode
            return None, "ilo check unavailable", 127
        finally:
            try:
                os.unlink(path)
            except OSError:
                pass


class LocalMaskBackend:
    """Placeholder for true logit / grammar masks on a local guided server.

    attach_request would inject provider-specific guided fields (GBNF, JSON
    schema of tokens, etc.). validate_emit may still double-check with ilo.
    Not usable on the DeepSeek hosted API — the factory returns this object
    so the harness can refuse the arm before any request is sent.
    """

    name = "local-mask"

    def attach_request(self, body: dict[str, Any]) -> dict[str, Any]:
        raise NotImplementedError(
            "local-mask requires a guided local OpenAI-compat endpoint; "
            "DeepSeek hosted API has no grammar/logit mask field"
        )

    def validate_emit(self, code: str) -> ConstrainResult:
        raise NotImplementedError("local-mask not implemented")


def make_constrain_backend(
    mode: str,
    *,
    ilo_bin: str = "ilo",
) -> ConstrainBackend:
    """Factory for --constrain {none,reject-retry,local-mask}."""
    key = (mode or "none").strip().lower()
    if key in ("none", "off", "0", ""):
        return NoopBackend()
    if key in ("reject-retry", "reject", "check"):
        return RejectRetryBackend(ilo_bin=ilo_bin)
    if key in ("local-mask", "mask", "guided"):
        return LocalMaskBackend()
    raise ValueError(
        f"unknown constrain mode {mode!r}; expected none|reject-retry|local-mask"
    )


def _payload_from_streams(stdout: str, stderr: str) -> dict[str, Any] | None:
    """One JSON object, or NDJSON diagnostic lines from ilo check."""
    text = (stdout or "").strip()
    if text.startswith("{") and "\n" not in text:
        try:
            obj = json.loads(text)
        except json.JSONDecodeError:
            obj = None
        if isinstance(obj, dict):
            return obj
    diags: list[dict[str, Any]] = []
    for blob in (stdout, stderr):
        for line in (blob or "").splitlines():
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(obj, dict):
                diags.append(obj)
    if not diags:
        return None
    if len(diags) == 1:
        return diags[0]
    return {"diagnostics": diags}


def _json_flag_unsupported(
    stderr: str,
    returncode: int,
    raw: dict[str, Any] | None,
) -> bool:
    """True when ``--json`` was refused, so the next argv should be tried."""
    if returncode == 0 or raw is not None:
        return False
    low = (stderr or "").lower()
    if "ilo-" in low:
        return False
    return (
        "usage" in low
        or "unexpected" in low
        or "unrecognized" in low
        or "unknown" in low
        or returncode == 2
    )


def _codes_from_check(raw: dict[str, Any] | None, stderr: str) -> list[str]:
    codes: list[str] = []
    if isinstance(raw, dict):
        err = raw.get("error") or raw.get("diagnostics") or raw
        # Best-effort: walk common ilo JSON shapes for code fields.
        stack = [err]
        while stack:
            cur = stack.pop()
            if isinstance(cur, dict):
                c = cur.get("code") or cur.get("id")
                if isinstance(c, str) and c.startswith("ILO-"):
                    codes.append(c)
                stack.extend(cur.values())
            elif isinstance(cur, list):
                stack.extend(cur)
    if not codes:
        for token in TARGET_ILLEGAL_CODES:
            if token in (stderr or ""):
                codes.append(token)
        # bare P023 etc.
        for bare in ("P023", "P009", "P007", "P011"):
            full = f"ILO-{bare}"
            if bare in (stderr or "") and full not in codes:
                codes.append(full)
    # de-dupe, stable order
    seen: set[str] = set()
    out: list[str] = []
    for c in codes:
        if c not in seen:
            seen.add(c)
            out.append(c)
    return out or ["ILO-UNKNOWN"]


def _repair_signal(
    codes: list[str],
    stderr: str,
    raw: dict[str, Any] | None,
) -> str:
    head = "ILO-CONSTRAIN reject — programme failed ilo check before run."
    code_line = "codes: " + ", ".join(codes)
    hints = []
    if any(c.endswith("P023") for c in codes):
        hints.append(
            "P023: inside lambdas/HOF use a boolean *expression* (>b), "
            "never a braceless guard; prefer ?cond then else."
        )
    if any(c.endswith("P009") for c in codes):
        hints.append(
            "P009: one conditional shape only — prefix ternary ?cond{then}{else}; "
            "do not mix ?cond a {stmts}."
        )
    if any(c.endswith("P007") for c in codes):
        hints.append("P007: list types need an element (L n / L t / L _); bare L> is illegal.")
    if any(c.endswith("P011") for c in codes):
        hints.append(
            "P011: avoid reserved bindings (e = Euler); match patterns need proper shape."
        )
    detail = ""
    if raw is not None:
        detail = "check_json: " + json.dumps(raw)[:800]
    else:
        detail = "stderr: " + (stderr or "")[:800]
    parts = [head, code_line, *hints, detail]
    return "\n".join(parts)
