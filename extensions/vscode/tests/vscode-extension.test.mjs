import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it } from "node:test";

const here = dirname(fileURLToPath(import.meta.url));
const root = dirname(here);

describe("VS Code extension manifest", () => {
  it("contributes ilo language support for .ilo files", async () => {
    const manifest = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
    const language = manifest.contributes?.languages?.find((entry) => entry.id === "ilo");
    const grammar = manifest.contributes?.grammars?.find((entry) => entry.language === "ilo");
    const snippets = manifest.contributes?.snippets?.find((entry) => entry.language === "ilo");

    assert.ok(language, "language contribution missing");
    assert.deepEqual(language.extensions, [".ilo"]);
    assert.equal(language.configuration, "./language-configuration/ilo.json");

    assert.ok(grammar, "grammar contribution missing");
    assert.equal(grammar.scopeName, "source.ilo");
    assert.equal(grammar.path, "./syntaxes/ilo.tmLanguage.json");

    assert.ok(snippets, "snippets contribution missing");
    assert.equal(snippets.path, "./snippets/ilo.code-snippets");
  });

  it("is not marked private (we publish to the marketplace)", async () => {
    const manifest = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
    assert.notEqual(manifest.private, true);
    assert.equal(manifest.publisher, "ilo-lang");
    assert.equal(manifest.name, "ilo-lang");
  });

  it("ships snippets for the highest-friction patterns", async () => {
    const snippets = JSON.parse(await readFile(join(root, "snippets/ilo.code-snippets"), "utf8"));
    for (const key of ["function", "tool", "match", "let", "loop", "guard"]) {
      assert.ok(snippets[key], `missing snippet: ${key}`);
      assert.ok(snippets[key].prefix, `snippet ${key} missing prefix`);
      assert.ok(snippets[key].body, `snippet ${key} missing body`);
    }
  });

  it("language config uses ilo's -- line comment and kebab-case word pattern", async () => {
    const config = JSON.parse(await readFile(join(root, "language-configuration/ilo.json"), "utf8"));
    assert.equal(config.comments.lineComment, "--");
    assert.ok(!config.comments.blockComment, "ilo has no block comments");
    // Word pattern must include hyphens so kebab-case identifiers select cleanly.
    assert.match(config.wordPattern, /A-Za-z0-9-/);
  });

  it("grammar parses as JSON and declares the source.ilo scope", async () => {
    const grammar = JSON.parse(await readFile(join(root, "syntaxes/ilo.tmLanguage.json"), "utf8"));
    assert.equal(grammar.scopeName, "source.ilo");
    assert.ok(Array.isArray(grammar.patterns));
    assert.ok(grammar.repository);
  });

  it("highlights ilo comments, strings, numbers, and builtins", async () => {
    const grammar = JSON.parse(await readFile(join(root, "syntaxes/ilo.tmLanguage.json"), "utf8"));
    assert.equal(grammar.repository.comment.name, "comment.line.double-dash.ilo");
    assert.equal(grammar.repository.string.name, "string.quoted.double.ilo");
    assert.equal(grammar.repository.number.name, "constant.numeric.ilo");
    // The builtin rule should at least mention some core HOFs.
    const builtinMatch = grammar.repository.builtin.match;
    for (const name of ["map", "flt", "fld", "len", "fmt"]) {
      assert.match(builtinMatch, new RegExp(`\\b${name}\\b`));
    }
  });
});
