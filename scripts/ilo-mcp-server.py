#!/usr/bin/env python3
"""ilo-mcp-server.py — G5.1 prototype: expose ilo functions as MCP tools.

PLAN.md G5.1. The selling point being measured: the tool schema is
*generated from the ilo AST* (`ilo --ast`), so the resident schema cost is
the typed signature, not hand-written JSON Schema boilerplate.

Protocol: MCP stdio (JSON-RPC 2.0) — initialize / tools/list / tools/call.
Discovery: every `*.ilo` file in --dir; every public Function declaration
becomes one tool (named `demo:tri` style: file stem + function name).

Type mapping (ilo → JSON Schema):
  Number → number, Text → string, Bool → boolean
  {List: T} → {"type":"array","items":schema(T)}
  {Map: [K,V]} → {"type":"object","additionalProperties":schema(V)}
  {Optional: T} → schema(T) (JSON Schema 2020-12 prefixItems-style union
                  is avoided; tools receive absent → skipped)
  {Result: OK, _} → schema(OK)  (tool calls return runtime errors as
                  isError content, not as typed Err payloads)
  {Fn: _, _} → tool is SKIPPED (function params can't cross JSON)
  anything else → {} (any)

Argument names are matched hyphen/score-insensitively (MCP convention is
snake_case; ilo identifiers are hyphenated).

Usage:
  python3 scripts/ilo-mcp-server.py --dir mcp-tools --ilo ./target/release/ilo
  python3 scripts/ilo-mcp-server.py --dir mcp-tools --stats   # schema cost only
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
            inner = schema(ty["Optional"], depth + 1)
            return inner | {"nullable": True}
        if "Result" in ty:
            ok = ty["Result"]
            return schema(ok[0] if isinstance(ok, list) and ok else ok, depth + 1)
    return {}  # named types, generics, F types → any


class Tool:
    def __init__(self, file: Path, name: str, description: str,
                 params: list[dict], input_schema: dict, schema_bytes: int):
        self.tool_name = f"{file.stem}:{name}"
        self.description = description
        self.params = params
        self.input_schema = input_schema
        self.schema_bytes = schema_bytes
        self.file = file
        self.fn = name


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

        # Description: the leading `--` comment block's first meaningful line.
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
                if isinstance(ty, dict) and "Optional" in ty:
                    pass  # nullable already marked; not required
                else:
                    required.append(norm(p["name"]))
                props[norm(p["name"])] = fragment | {
                    "description": f"ilo param `{p['name']}`"}
            if skip:
                continue
            input_schema = {"type": "object", "properties": props,
                            "required": required,
                            "additionalProperties": False}
            schema_bytes = len(json.dumps(input_schema))
            tools.append(Tool(file, fn["name"], description or fn["name"],
                              fn.get("params", []), input_schema, schema_bytes))
    return tools


def call_tool(tool: Tool, ilo_bin: str, arguments: dict) -> dict:
    argv = [ilo_bin, str(tool.file), tool.fn]
    matched = {norm(p["name"]): p for p in tool.params}
    for p in tool.params:
        key = norm(p["name"])
        if key in arguments:
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
        return {"content": [{"type": "text",
                             "text": (proc.stderr or proc.stdout).strip()}],
                "isError": True}
    out = {"content": [{"type": "text", "text": proc.stdout.strip()}]}
    try:
        out["structuredContent"] = json.loads(proc.stdout)
    except json.JSONDecodeError:
        pass
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dir", default="mcp-tools")
    ap.add_argument("--ilo", default=os.environ.get("ILO", "ilo"))
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

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue
        rid = req.get("id")
        method = req.get("method", "")
        if method == "initialize":
            resp = {"protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {"tools": {}},
                    "serverInfo": SERVER_INFO}
        elif method == "ping":
            resp = {}
        elif method.startswith("notifications/"):
            continue
        elif method == "tools/list":
            resp = {"tools": [
                {"name": t.tool_name, "description": t.description,
                 "inputSchema": t.input_schema} for t in tools]}
        elif method == "tools/call":
            params = req.get("params", {})
            match = next((t for t in tools
                          if t.tool_name == params.get("name")), None)
            if match is None:
                send(rid, error=err(-32602, f"unknown tool: {params.get('name')}"))
                continue
            try:
                result = call_tool(match, args.ilo,
                                   params.get("arguments", {}))
            except subprocess.TimeoutExpired:
                result = {"content": [{"type": "text", "text": "timeout"}],
                          "isError": True}
            send(rid, result=result)
        else:
            if rid is not None:
                send(rid, error=err(-32601, f"method not found: {method}"))
            continue

        if rid is not None:
            send(rid, result=resp)
    return 0


def err(code: int, message: str) -> dict:
    return {"code": code, "message": message}


def send(rid, result=None, error=None) -> None:
    msg: dict = {"jsonrpc": "2.0", "id": rid}
    if error is not None:
        msg["error"] = error
    else:
        msg["result"] = result
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


if __name__ == "__main__":
    sys.exit(main())
