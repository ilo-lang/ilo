#!/usr/bin/env python3
"""test-mcp-server.py — end-to-end wire test for scripts/ilo-mcp-server.py.

Exercises the real stdio JSON-RPC loop: initialize → tools/list → tools/call
(happy path on all three demo tools, a typed runtime error, an unknown-tool
error). Exits non-zero on any failure. Skips (exit 0) when the ilo binary
isn't present, so it can run in jobs that don't build the compiler.

Usage: python3 scripts/test-mcp-server.py [--ilo ./target/release/ilo]
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SERVER = ROOT / "scripts" / "ilo-mcp-server.py"
TOOLS_DIR = ROOT / "mcp-tools"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ilo", default=str(ROOT / "target/release/ilo"))
    args = ap.parse_args()

    if not Path(args.ilo).exists():
        print(f"skip: ilo binary not found at {args.ilo}", file=sys.stderr)
        return 0

    srv = subprocess.Popen(
        [sys.executable, str(SERVER), "--dir", str(TOOLS_DIR),
         "--ilo", args.ilo],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
        cwd=str(ROOT))

    def rpc(rid, method, params=None):
        msg: dict = {"jsonrpc": "2.0", "id": rid, "method": method}
        if params is not None:
            msg["params"] = params
        srv.stdin.write(json.dumps(msg) + "\n")
        srv.stdin.flush()
        while True:
            line = srv.stdout.readline()
            if not line:
                raise AssertionError("server closed before replying")
            resp = json.loads(line)
            if resp.get("id") == rid:
                return resp

    try:
        init = rpc(1, "initialize", {"protocolVersion": "2025-06-18"})
        assert init["result"]["serverInfo"]["name"] == "ilo-mcp-server"
        srv.stdin.write(json.dumps(
            {"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
        srv.stdin.flush()

        tools = rpc(2, "tools/list")["result"]["tools"]
        names = [t["name"] for t in tools]
        assert "demo:tri" in names and "demo:slug" in names, names
        # private (_-prefixed) helpers must not leak into the tool list
        assert not any(n.startswith("demo:__") for n in names), names
        # v0.3 result contract: every tool carries an outputSchema derived
        # from the ilo return type
        by_name = {t["name"]: t for t in tools}
        assert by_name["demo:tri"]["outputSchema"]["properties"]["result"] == {
            "type": "number"}, by_name["demo:tri"]
        assert by_name["gates:within-budget"]["outputSchema"]["properties"][
            "result"] == {"type": "boolean"}

        tri = rpc(3, "tools/call",
                  {"name": "demo:tri", "arguments": {"n": 10}})
        assert tri["result"]["content"][0]["text"] == "55", tri
        assert tri["result"]["structuredContent"] == {"result": 55}, tri

        slug = rpc(4, "tools/call", {
            "name": "demo:slug", "arguments": {"text": "Hello   MCP World"}})
        assert slug["result"]["content"][0]["text"] == "hello-mcp-world", slug
        assert slug["result"]["structuredContent"] == {
            "result": "hello-mcp-world"}, slug

        stats = rpc(5, "tools/call", {
            "name": "demo:stats-summary",
            "arguments": {"numbers_json": [3, 1, 4, 1, 5]}})
        structured = stats["result"].get("structuredContent", {})
        assert structured.get("result", {}).get("count") == 5, stats

        bad_arg = rpc(6, "tools/call",
                      {"name": "demo:tri", "arguments": {"n": "oops"}})
        assert bad_arg["result"].get("isError") is True, bad_arg
        # v0.3 error contract: ilo's stable diagnostic code surfaces
        # structurally so clients can branch on code, not prose
        berr = bad_arg["result"].get("structuredContent", {}).get("error", {})
        assert berr.get("code") == "ILO-R600", bad_arg
        assert "expects" in berr.get("message", ""), bad_arg

        unknown = rpc(7, "tools/call", {"name": "nope", "arguments": {}})
        assert unknown["error"]["code"] == -32602, unknown

        missing = rpc(11, "tools/call", {"name": "demo:tri",
                                         "arguments": {}})
        assert missing["error"]["code"] == -32602, missing
        assert "missing n" in missing["error"]["message"], missing

        gates_asap = rpc(8, "tools/call", {
            "name": "gates:route-priority",
            "arguments": {"subject": "ASAP: refund request"}})
        assert gates_asap["result"]["content"][0]["text"] == "priority-normal", gates_asap
        gates_urgent = rpc(9, "tools/call", {
            "name": "gates:route-priority",
            "arguments": {"subject": "urgent: refund request"}})
        assert gates_urgent["result"]["content"][0]["text"] == "priority-high", gates_urgent
        gate_no = rpc(10, "tools/call", {
            "name": "gates:within-budget",
            "arguments": {"requested": 120, "limit": 100}})
        assert gate_no["result"]["content"][0]["text"] == "false", gate_no

        # families 5/6 (quality, lists): scalar coercion into the
        # structured envelope + schema-derived outputSchema presence
        clamp = rpc(12, "tools/call", {
            "name": "quality:clampinto",
            "arguments": {"x": 15, "lo": 0, "hi": 10}})
        assert clamp["result"]["structuredContent"] == {"result": 10}, clamp
        assert by_name["quality:clampinto"]["outputSchema"]["properties"][
            "result"] == {"type": "number"}
        total = rpc(13, "tools/call", {
            "name": "lists:sum-list", "arguments": {"xs": [1, 2, 3, 4]}})
        assert total["result"]["structuredContent"] == {"result": 10}, total
        blank = rpc(14, "tools/call", {
            "name": "quality:not-blank", "arguments": {"s": "   "}})
        assert blank["result"]["structuredContent"] == {"result": False}, blank

        print("mcp e2e: OK "
              "(initialize, list+outputSchema, tri/slug/stats, gate calls, "
              "quality/lists envelopes, typed error, unknown tool)")
        return 0
    finally:
        srv.kill()


if __name__ == "__main__":
    sys.exit(main())
