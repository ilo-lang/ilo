# Deploying ilo — cross-platform build guide

ilo ships pre-built binaries for five native targets and one WASM/WASI edge
target. This document covers how to build for each target and how to deploy to
edge runtimes.

## Supported targets

| Triple | Platform | Notes |
|---|---|---|
| `aarch64-apple-darwin` | macOS Apple Silicon | Native runner |
| `x86_64-apple-darwin` | macOS Intel | Native runner |
| `x86_64-unknown-linux-musl` | Linux x86-64 | Statically linked, no libc dep |
| `aarch64-unknown-linux-musl` | Linux arm64 | Statically linked, no libc dep |
| `x86_64-pc-windows-msvc` | Windows x86-64 | Requires MSVC toolchain |
| `wasm32-wasip1` | WASM/WASI edge | Build-only; HTTP not yet supported |

## Prerequisites

```sh
rustup target add <triple>
```

For `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` when
cross-compiling from a non-Linux host, use
[`cross`](https://github.com/cross-rs/cross) (requires Docker):

```sh
cargo install cross --git https://github.com/cross-rs/cross
```

## Building for a specific target

### Native build (host matches target)

```sh
cargo build --release --target <triple>
```

### Cross-compilation with `cross`

```sh
cross build --release --target x86_64-unknown-linux-musl
cross build --release --target aarch64-unknown-linux-musl
```

The output binary is at `target/<triple>/release/ilo` (or `ilo.exe` on
Windows).

### WASM/WASI

```sh
cargo build --release --target wasm32-wasip1 --no-default-features
```

The `--no-default-features` flag disables `cranelift` and `http`, which do not
compile to WASM. The output is at `target/wasm32-wasip1/release/ilo.wasm`.

## Using `ilo compile --target`

The `compile` (and `build`) subcommand accepts a `--target` flag:

```sh
ilo compile --target x86_64-unknown-linux-musl hello.ilo -o hello-linux
ilo compile --target wasm32-wasip1 hello.ilo -o hello.wasm
```

This validates the triple against the supported list and passes it through to
the underlying `cargo build` invocation. The target must already be installed
via `rustup target add`.

## macOS link warnings

ilo CI sets `-mmacosx-version-min=15.0` via `.cargo/config.toml` to silence
the SDK-version mismatch warnings emitted by the macOS linker on modern Xcode
runners. If you build locally on macOS and see link warnings, add:

```sh
export RUSTFLAGS="-C link-arg=-mmacosx-version-min=15.0"
```

or use the `.cargo/config.toml` entry which applies it automatically for all
macOS builds in this workspace.

## Edge runtime deployment

### Cloudflare Workers / Fastly Compute / Deno Deploy

Build the WASM target as above, then upload `ilo.wasm` to your edge platform.
Most WASI Preview 1 runtimes support the binary without modification.

### Deno

```sh
deno run --allow-read ilo.wasm -- run hello.ilo
```

### wasmtime (local smoke test)

```sh
wasmtime ilo.wasm -- run hello.ilo
```

## HTTP on WASM — known gap

`ilo`'s HTTP builtins (`http-get`, `http-post`, …) are **not available** on
the WASM target. WASI Preview 1 has no socket or HTTP capability. This is
tracked in issue #48 (WASM HTTP via fetch shim) and Architectural item C
(HTTP streaming). Until those land, ilo programs that use HTTP must run on a
native target.
