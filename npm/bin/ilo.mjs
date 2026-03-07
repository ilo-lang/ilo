#!/usr/bin/env -S node --no-warnings

import { readFile } from "node:fs/promises";
import { WASI } from "node:wasi";
import { argv, exit } from "node:process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(__dirname, "..", "ilo.wasm");

const wasi = new WASI({
  version: "preview1",
  args: ["ilo", ...argv.slice(2)],
  env: process.env,
  preopens: {
    "/": "/",
  },
});

const wasmBuffer = await readFile(wasmPath);
const { instance } = await WebAssembly.instantiate(
  wasmBuffer,
  wasi.getImportObject()
);

try {
  wasi.start(instance);
} catch (err) {
  if (err.code === "ERR_WASI_NOT_STARTED") {
    // Normal exit
  } else if (typeof err.exitCode === "number") {
    exit(err.exitCode);
  } else {
    throw err;
  }
}
