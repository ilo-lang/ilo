# ilo Language Extension

Syntax highlighting, snippets, and editor configuration for [ilo](https://ilo-lang.ai) `.ilo` files in VS Code and Cursor.

ilo is a token-optimised programming language for AI agents. This extension covers:

- Syntax highlighting (comments, strings, numbers, type sigils, builtins, operators)
- `--` line comments with auto-toggle
- Bracket pairs, auto-close, and surrounding for `{} [] () "" ''`
- Word pattern that includes hyphens (ilo identifiers are kebab-case)
- Snippets for the highest-friction patterns: `fn`, `tool`, `match`, `let`, `loop`, `guard`, `type`, `tern`

## Install

**VS Code marketplace** (once published):

```sh
code --install-extension ilo-lang.ilo-lang
```

**Cursor** (until Open VSX is live):

```sh
cd extensions/vscode
npm run install:cursor
```

Reload the Cursor window after install.

## Develop

```sh
npm test                 # runs the manifest + grammar smoke tests
npm run install:cursor   # install into ~/.cursor/extensions for local testing
```

To package a `.vsix` for the marketplace, install `vsce` then run `vsce package` in this directory.

## Links

- Language site: <https://ilo-lang.ai>
- Source: <https://github.com/ilo-lang/ilo>
- Spec: <https://github.com/ilo-lang/ilo/blob/main/SPEC.md>
