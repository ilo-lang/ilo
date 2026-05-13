#!/usr/bin/env bash
# Bump the ilo version across every file that ships it.
# Run from the repo root: scripts/bump-version.sh 0.11.0
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: scripts/bump-version.sh X.Y.Z" >&2
  exit 1
fi

v="$1"
if [[ ! "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: version must be X.Y.Z, got '$v'" >&2
  exit 1
fi

if [[ ! -f Cargo.toml ]]; then
  echo "error: run from repo root (no Cargo.toml here)" >&2
  exit 1
fi

# Cargo.toml: the top-level [package] version (first `version = "..."` line).
awk -v v="$v" '
  !done && /^version = "[0-9]+\.[0-9]+\.[0-9]+"$/ { print "version = \"" v "\""; done=1; next }
  { print }
' Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml

# AGENTS.md: the "Current version: **X.Y.Z**" line.
awk -v v="$v" '
  /^- Current version: \*\*[0-9]+\.[0-9]+\.[0-9]+\*\*/ {
    sub(/\*\*[0-9]+\.[0-9]+\.[0-9]+\*\*/, "**" v "**")
  }
  { print }
' AGENTS.md > AGENTS.md.tmp && mv AGENTS.md.tmp AGENTS.md

# .claude-plugin/{plugin,marketplace}.json: every `"version": "X.Y.Z"` line.
# Line-based replacement so we don't reflow the JSON or escape unicode.
for f in .claude-plugin/plugin.json .claude-plugin/marketplace.json; do
  awk -v v="$v" '
    { line = $0
      if (match(line, /"version":[[:space:]]*"[0-9]+\.[0-9]+\.[0-9]+"/)) {
        sub(/"[0-9]+\.[0-9]+\.[0-9]+"/, "\"" v "\"", line)
      }
      print line
    }
  ' "$f" > "$f.tmp" && mv "$f.tmp" "$f"
done

# Refresh Cargo.lock so the workspace stays consistent.
if ! cargo update -p ilo --precise "$v" >/dev/null 2>&1; then
  echo "warning: cargo update failed; falling back to cargo check to refresh Cargo.lock" >&2
  cargo check --quiet
fi

echo "bumped to $v"
echo "review with: git diff"
