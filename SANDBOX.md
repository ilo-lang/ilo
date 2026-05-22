# ilo Capability Sandbox: Operator Guide

> CLI capability flags for multi-tenant and sandboxed deployments (ILO-59).

## Overview

ilo programs can read files, write files, make network requests, and spawn
subprocesses. In single-user / trusted contexts this is fine — the same
footprint as any scripting language. In multi-tenant deployments (agents
running untrusted ilo code on a shared server) this leaves SSRF,
arbitrary-filesystem-read, and arbitrary-command-execution open.

Capability flags give operators a per-process sandbox. Any `--allow-*` flag
switches the runtime from **permissive** (legacy default, no restrictions) to
**restricted** (only explicitly listed targets are permitted). Capabilities not
mentioned default to unrestricted when in restricted mode, so you can add a
single flag without breaking other IO.

## Flags

| Flag | Value syntax | Effect |
|------|-------------|--------|
| `--allow-net[=HOSTS]` | comma-separated hosts, `*`, or empty | Gate outbound HTTP/HTTPS |
| `--allow-read[=PATHS]` | comma-separated path prefixes, `*`, or empty | Gate file reads |
| `--allow-write[=PATHS]` | comma-separated path prefixes, `*`, or empty | Gate file writes |
| `--allow-run[=CMDS]` | comma-separated command names, `*`, or empty | Gate subprocess spawning |

**Value semantics:**

- Omitted flag → that capability is unrestricted (permissive).
- `--allow-net=*` or `--allow-net=all` → net unrestricted (explicit All).
- `--allow-net=api.example.com,cdn.example.com` → only those two hosts.
- `--allow-net=` (empty value) → all network blocked.

Once any `--allow-*` flag is present the mode is restricted; all four
dimensions are individually governed by their flag (or `Policy::All` if that
flag was omitted).

## Matching rules

**Network (`--allow-net`):** host extracted from URL (scheme and path stripped).
Exact match or leading `*.`-wildcard: `*.example.com` matches
`api.example.com` and `example.com`.

**Read / write (`--allow-read`, `--allow-write`):** path-prefix matching with
separator boundary. `/tmp` permits `/tmp/foo` but not `/tmpfoo`. Trailing
slash on the prefix is normalised.

**Run (`--allow-run`):** exact command name or basename match. `/usr/bin/ls`
is matched by `ls` in the allowlist.

## Error code

A denied capability emits `ILO-CAP-001` as the error value returned from the
builtin:

```
ILO-CAP-001 blocked by --allow-net policy: host=evil.example is not in the allowlist
```

The error is a normal ilo `R` (Result) `Err` value — programs can pattern-match
it with `?{res|er: ...}` and react programmatically. It is not a fatal abort.

## Capability matrix

| Builtin | Capability checked |
|---------|-------------------|
| `get`, `post`, `put`, `patch`, `del`, `http-get`, `http-post`, `fetch` | `--allow-net` |
| `rd`, `rd-lines`, `ls`, `lsr` | `--allow-read` |
| `wr`, `wr-lines`, `wr-app` | `--allow-write` |
| `run`, `run2` | `--allow-run` |

## Recipes

### Block all IO

```sh
ilo run --allow-net= --allow-read= --allow-write= --allow-run= untrusted.ilo
```

### Allow only outbound calls to one API

```sh
ilo run --allow-net=api.example.com trusted.ilo
```

### Read-only scratch space

```sh
ilo run --allow-read=/data --allow-write=/tmp agent.ilo
```

### Wildcard subdomain

```sh
ilo run --allow-net="*.internal.corp" service.ilo
```

## Backwards compatibility

`Caps::Permissive` is the default. Any script that does not pass `--allow-*`
runs without restriction — identical behaviour to pre-0.13.

## See also

- `examples/capability-sandbox.ilo` — runnable demo.
- `SPEC.md` — capability flags section.
- ILO-59 (Linear) — implementation ticket.
- ILO-47 (Linear) — `World` capability parameter (the language-level long-term move).
