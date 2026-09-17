#!/usr/bin/env python3
"""ilo-mcp-server.py — G5.1 prototype: expose ilo functions as MCP tools.

PLAN.md G5.1. The selling point being measured: the tool schema is
*generated from the ilo AST* (`ilo --ast`), so the resident schema cost is
the typed signature, not hand-written JSON Schema boilerplate.

Transports:
  stdio (default)           JSON-RPC 2.0 over stdin/stdout
  --http --port 8391        MCP streamable-HTTP: POST /mcp, JSON responses,
                            Mcp-Session-Id header on initialize

Protocol: initialize / tools/list / tools/call / ping; notifications
(including `initialized`) are accepted and produce no response.

Discovery: every `*.ilo` file in --dir; every public Function declaration
becomes one tool (named `demo:tri` style: file stem + function name).

Result contract (v0.3): every tool carries an `outputSchema` derived from
the ilo return type; successful calls wrap parsed stdout as
`structuredContent: {"result": ...}`; failing calls surface ilo's stable
diagnostic code (`ILO-*`) as `structuredContent: {"error": {"code",
"message"}}`.

Type mapping (ilo → JSON Schema):
  Number → number, Text → string, Bool → boolean
  {List: T} → {"type":"array","items":schema(T)}
  {Map: [K,V]} → {"type":"object","additionalProperties":schema(V)}
  {Optional: T} → schema(T) marked nullable
  {Result: OK, _} → schema(OK)  (runtime errors surface as isError)
  {Fn: _, _} → tool is SKIPPED (function params can't cross JSON)
  anything else → {} (any)

Usage:
  python3 scripts/ilo-mcp-server.py --dir mcp-tools --ilo ./target/release/ilo
  python3 scripts/ilo-mcp-server.py --dir mcp-tools --http --port 8391
  python3 scripts/ilo-mcp-server.py --dir mcp-tools --stats  # schema cost
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

PROTOCOL_VERSION = "2025-06-18"
SERVER_INFO = {"name": "ilo-mcp-server", "version": "0.1.0"}

try:
    import tiktoken
    _enc = tiktoken.get_encoding("cl100k_base")
except ImportError:
    _enc = None


def tok(text: str) -> int:
    return len(_enc.encode(text)) if _enc else len(text) // 4


def norm(name: str) -> str:
    return name.replace("-", "_").lower()


def schema(ty, depth: int = 0):
    """Map an ilo AST type node to a JSON Schema fragment."""
    if depth > 8:
        return {}
    if ty == "Number":
        return {"type": "number"}
    if ty == "Text":
        return {"type": "string"}
    if ty == "Bool":
        return {"type": "boolean"}
    if isinstance(ty, dict):
        if "List" in ty:
            return {"type": "array", "items": schema(ty["List"], depth + 1)}
        if "Map" in ty:
            kv = ty["Map"]
            return {"type": "object",
                    "additionalProperties": schema(kv[1] if len(kv) > 1 else {}, depth + 1)}
        if "Optional" in ty:
            return schema(ty["Optional"], depth + 1) | {"nullable": True}
        if "Result" in ty:
            ok = ty["Result"]
            return schema(ok[0] if isinstance(ok, list) and ok else ok, depth + 1)
    return {}  # named types, generics, F types → any


class Tool:
    def __init__(self, file: Path, name: str, description: str,
                 params: list[dict], input_schema: dict, schema_bytes: int,
                 return_type=None):
        self.tool_name = f"{file.stem}:{name}"
        self.description = description
        self.params = params
        self.input_schema = input_schema
        self.schema_bytes = schema_bytes
        self.file = file
        self.fn = name
        # MCP structured output envelope: the tool's value lives under
        # "result"; outputSchema describes that envelope.
        self.output_schema = {"type": "object",
                              "properties": {"result": schema(return_type)},
                              "required": ["result"]}


def discover(dir_path: Path, ilo_bin: str) -> list[Tool]:
    tools: list[Tool] = []
    for file in sorted(dir_path.glob("*.ilo")):
        source = file.read_text()
        proc = subprocess.run([ilo_bin, "--ast", str(file)],
                              capture_output=True, text=True, timeout=30)
        if proc.returncode != 0:
            print(f"ilo-mcp: skipping {file} (ast error)", file=sys.stderr)
            continue
        try:
            ast = json.loads(proc.stdout)
        except json.JSONDecodeError:
            print(f"ilo-mcp: skipping {file} (ast not json)", file=sys.stderr)
            continue

        description = ""
        for line in source.splitlines():
            if line.startswith("--"):
                text = line[2:].strip()
                if text and not text.startswith("MCP"):
                    description = text
                    break
            elif line.strip():
                break

        for decl in ast.get("declarations", []):
            fn = decl.get("Function")
            if not fn or fn.get("name") == "main":
                continue
            if fn.get("name", "").startswith("_"):
                continue  # _-prefixed = private by convention
            props: dict = {}
            required: list[str] = []
            skip = False
            for p in fn.get("params", []):
                ty = p["ty"]
                if isinstance(ty, dict) and "Fn" in ty:
                    skip = True
                    break
                fragment = schema(ty)
                if not (isinstance(ty, dict) and "Optional" in ty):
                    required.append(norm(p["name"]))
                props[norm(p["name"])] = fragment | {
                    "description": f"ilo param `{p['name']}`"}
            if skip:
                continue
            input_schema = {"type": "object", "properties": props,
                            "required": required,
                            "additionalProperties": False}
            tools.append(Tool(file, fn["name"], description or fn["name"],
                              fn.get("params", []), input_schema,
                              len(json.dumps(input_schema)),
                              return_type=fn.get("return_type")))
    return tools


def call_tool(tool: Tool, ilo_bin: str, arguments: dict) -> dict:
    argv = [ilo_bin, str(tool.file), tool.fn]
    for p in tool.params:
        key = norm(p["name"])
        if key not in arguments:
            continue
        v = arguments[key]
        if isinstance(v, bool):
            argv.append("true" if v else "false")
        elif isinstance(v, (int, float)):
            argv.append(format(v))
        elif isinstance(v, (list, dict)):
            argv.append(json.dumps(v))
        else:
            argv.append(str(v))
    proc = subprocess.run(argv, capture_output=True, text=True, timeout=30)
    if proc.returncode != 0:
        text = (proc.stderr or proc.stdout).strip()
        out = {"content": [{"type": "text", "text": text}], "isError": True}
        # ilo emits one JSON diagnostic per line with a stable `code`
        # (ILO-P*/ILO-T*/ILO-C*). Surface the first as the structured
        # error so clients can branch on code, not prose.
        for line in text.splitlines():
            try:
                diag = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(diag, dict) and "code" in diag and "message" in diag:
                out["structuredContent"] = {"error": {
                    "code": diag["code"], "message": diag["message"]}}
                break
        return out
    out = {"content": [{"type": "text", "text": proc.stdout.strip()}]}
    result_schema = tool.output_schema["properties"]["result"]
    try:
        out["structuredContent"] = {
            "result": coerce(proc.stdout.strip(), result_schema)}
    except (json.JSONDecodeError, ValueError):
        pass  # unparseable → text content remains the contract
    return out


def coerce(text: str, sch: dict):
    """Coerce ilo's CLI stdout to the declared type.

    JSON parse first (covers tools that print structured values); bare
    scalar output falls back to the declared schema. This tolerates
    tools whose declared Text return actually renders a JSON map."""
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass
    ty = sch.get("type")
    if ty == "number":
        return float(text)
    if ty == "boolean":
        return text == "true"
    return text  # string / any


def err(code: int, message: str) -> dict:
    return {"code": code, "message": message}


def dispatch(req: dict, tools: list[Tool], ilo_bin: str):
    """Handle one JSON-RPC request. Returns (result | None, error | None).
    (None, None) means the request was a notification: no response."""
    rid = req.get("id")
    method = req.get("method", "")
    if method == "initialize":
        return {"protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": SERVER_INFO}, None
    if method == "ping":
        return {}, None
    if method.startswith("notifications/"):
        return None, None
    if method == "tools/list":
        return {"tools": [
            {"name": t.tool_name, "description": t.description,
             "inputSchema": t.input_schema,
             "outputSchema": t.output_schema} for t in tools]}, None
    if method == "tools/call":
        params = req.get("params", {})
        match = next((t for t in tools
                      if t.tool_name == params.get("name")), None)
        if match is None:
            return None, err(-32602, f"unknown tool: {params.get('name')}")
        arguments = params.get("arguments", {}) or {}
        required = match.input_schema.get("required", [])
        missing = [k for k in required if k not in arguments]
        if missing:
            return None, err(
                -32602,
                f"invalid arguments for {match.tool_name}: "
                f"missing {', '.join(missing)}")
        try:
            return call_tool(match, ilo_bin, arguments), None
        except subprocess.TimeoutExpired:
            return ({"content": [{"type": "text", "text": "timeout"}],
                     "isError": True,
                     "structuredContent": {"error": {"code": "ILO-TIMEOUT",
                                                     "message": "tool call exceeded 30s"}}}), None
    if rid is not None:
        return None, err(-32601, f"method not found: {method}")
    return None, None


def serve_stdio(tools: list[Tool], ilo_bin: str) -> int:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue
        result, error = dispatch(req, tools, ilo_bin)
        if result is None and error is None:
            continue
        msg: dict = {"jsonrpc": "2.0", "id": req.get("id")}
        if error is not None:
            msg["error"] = error
        else:
            msg["result"] = result
        sys.stdout.write(json.dumps(msg) + "\n")
        sys.stdout.flush()
    return 0


def serve_http(tools: list[Tool], ilo_bin: str, port: int) -> int:
    from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

    class Handler(BaseHTTPRequestHandler):
        def _reply(self, code: int, payload: dict, session: str | None = None):
            body = json.dumps(payload).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            if session:
                self.send_header("Mcp-Session-Id", session)
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self):
            if self.path.rstrip("/") != "/mcp":
                self._reply(404, {"error": "not found"})
                return
            length = int(self.headers.get("Content-Length", 0))
            try:
                req = json.loads(self.rfile.read(length))
            except json.JSONDecodeError:
                self._reply(400, {"error": "invalid json"})
                return
            result, error = dispatch(req, tools, ilo_bin)
            if result is None and error is None:
                self._reply(202, {})
                return
            if error is not None:
                self._reply(200, {"jsonrpc": "2.0", "id": req.get("id"),
                                  "error": error})
                return
            session = self.headers.get("Mcp-Session-Id") or os.urandom(8).hex()
            self._reply(200, {"jsonrpc": "2.0", "id": req.get("id"),
                              "result": result}, session)

        def do_GET(self):
            self._reply(405, {"error": "GET unsupported (JSON mode); use POST"})

        def log_message(self, *_a):
            pass

    print(f"ilo-mcp: http://127.0.0.1:{port}/mcp "
          f"({len(tools)} tools, JSON mode)", file=sys.stderr)
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dir", default="mcp-tools")
    ap.add_argument("--ilo", default=os.environ.get("ILO", "ilo"))
    ap.add_argument("--http", action="store_true",
                    help="streamable-HTTP transport instead of stdio")
    ap.add_argument("--port", type=int, default=8391)
    ap.add_argument("--stats", action="store_true",
                    help="print resident schema cost and exit")
    args = ap.parse_args()

    tools = discover(Path(args.dir), args.ilo)

    if args.stats:
        total = sum(t.schema_bytes for t in tools)
        for t in tools:
            print(f"  {t.tool_name:<28} {t.schema_bytes:5d} B schema")
        print(f"  {'TOTAL':<28} {total:5d} B "
              f"(~{total // 4} tokens chars/4; resident cost of the whole "
              f"server's tools/list)")
        return 0

    if args.http:
        return serve_http(tools, args.ilo, args.port)
    return serve_stdio(tools, args.ilo)


if __name__ == "__main__":
    sys.exit(main())
