#!/usr/bin/env node
// Install the ilo VS Code extension into Cursor's local extensions directory.
// Cursor uses VS Code's extension model, so a plain file copy works until
// the extension is published to Open VSX.
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const extensionRoot = dirname(here);

const manifest = JSON.parse(await readFile(join(extensionRoot, "package.json"), "utf8"));
const extensionId = `${manifest.publisher}.${manifest.name}-${manifest.version}`;
const extensionDir = join(homedir(), ".cursor", "extensions", extensionId);

await rm(extensionDir, { force: true, recursive: true });
await mkdir(join(extensionDir, "language-configuration"), { recursive: true });
await mkdir(join(extensionDir, "syntaxes"), { recursive: true });
await mkdir(join(extensionDir, "snippets"), { recursive: true });

await cp(join(extensionRoot, "package.json"), join(extensionDir, "package.json"));
await cp(join(extensionRoot, "README.md"), join(extensionDir, "README.md"));
await cp(
  join(extensionRoot, "language-configuration/ilo.json"),
  join(extensionDir, "language-configuration", "ilo.json"),
);
await cp(
  join(extensionRoot, "syntaxes/ilo.tmLanguage.json"),
  join(extensionDir, "syntaxes", "ilo.tmLanguage.json"),
);
await cp(
  join(extensionRoot, "snippets/ilo.code-snippets"),
  join(extensionDir, "snippets", "ilo.code-snippets"),
);
await writeFile(join(extensionDir, ".installed-by-ilo"), new Date().toISOString());

console.log(`Installed ${manifest.publisher}.${manifest.name} to ${extensionDir}`);
console.log("Reload the Cursor window for syntax highlighting to activate.");
