#!/usr/bin/env python3
"""test-mcp-http.py — end-to-end test for the streamable-HTTP transport.

Starts scripts/ilo-mcp-server.py --http as a subprocess, polls until the
port accepts, then exercises the wire: initialize (captures Mcp-Session-Id),
tools/list, tools/call happy path and typed error. Exits non-zero on any
failure; exits 0 with a skip notice when the ilo binary is absent.

Usage: python3 scripts/test-mcp-http.py [--ilo ./target/release/ilo] [--port 8391]
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SERVER = ROOT / "scripts" / "ilo-mcp-server.py"
TOOLS_DIR = ROOT / "mcp-tools"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ilo", default=str(ROOT / "target/release/ilo"))
    ap.add_argument("--port", type=int, default=8391)
    args = ap.parse_args()

    if not Path(args.ilo).exists():
        print(f"skip: ilo binary not found at {args.ilo}", file=sys.stderr)
        return 0

    srv = subprocess.Popen(
        [sys.executable, str(SERVER), "--dir", str(TOOLS_DIR),
         "--ilo", args.ilo, "--http", "--port", str(args.port)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    url = f"http://127.0.0.1:{args.port}/mcp"
    try:
        deadline = time.time() + 10
        ready = False
        while time.time() < deadline:
            try:
                urllib.request.urlopen(url, timeout=1)
            except urllib.error.HTTPError as e:
                if e.code == 405:  # GET rejected = server up
                    ready = True
                    break
            except (urllib.error.URLError, ConnectionError, OSError):
                time.sleep(0.1)
        if not ready:
            raise AssertionError("server did not become ready in 10s")

        def post(payload, session=None):
            headers = {"Content-Type": "application/json",
                       "Accept": "application/json"}
            if session:
                headers["Mcp-Session-Id"] = session
            req = urllib.request.Request(url, data=json.dumps(payload).encode(),
                                         headers=headers, method="POST")
            with urllib.request.urlopen(req, timeout=10) as r:
                return r.headers.get("Mcp-Session-Id"), json.loads(r.read())

        session, init = post({"jsonrpc": "2.0", "id": 1,
                              "method": "initialize", "params": {}})
        assert init["result"]["serverInfo"]["name"] == "ilo-mcp-server"
        assert session, "initialize must return Mcp-Session-Id"

        _, tools = post({"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
                        session)
        names = [t["name"] for t in tools["result"]["tools"]]
        assert "demo:tri" in names and "gates:severity" in names, names

        _, call = post({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                        "params": {"name": "demo:tri",
                                   "arguments": {"n": 10}}}, session)
        assert call["result"]["content"][0]["text"] == "55", call

        _, gate = post({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                        "params": {"name": "gates:severity",
                                   "arguments": {"value": 5}}}, session)
        assert gate["result"]["content"][0]["text"] == "low", gate

        _, unknown = post({"jsonrpc": "2.0", "id": 5, "method": "tools/call",
                           "params": {"name": "nope", "arguments": {}}},
                          session)
        assert unknown["error"]["code"] == -32602, unknown

        print("mcp http e2e: OK (initialize + session, list, calls, errors)")
        return 0
    finally:
        srv.kill()


if __name__ == "__main__":
    sys.exit(main())
