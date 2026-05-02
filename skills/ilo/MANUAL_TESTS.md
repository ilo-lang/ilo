# Manual cross-host tests for the ilo skill

CI runs the spec validator (`skills-ref validate`) plus the Rust integration
tests in `tests/skill_md.rs`. Those catch regressions in shape and
portability, but they cannot verify that real agent UIs actually load and
activate the skill. Run this checklist before each release.

One line per check; flip to a tick once verified.

- [ ] **Claude Code CLI**: install via `claude plugins install ilo-lang/ilo` (or the marketplace), then ask Claude to "write an ilo factorial" and confirm the skill activates (you'll see `ilo help ai` or `scripts/ensure-ilo.sh` invoked).
- [ ] **Claude Desktop**: enable the skill in plugins, ask in chat "write me an ilo program that adds two numbers", confirm the skill auto-loads and code is correct.
- [ ] **Codex CLI**: copy or symlink `skills/ilo/` into Codex's skills directory, ask Codex to "write ilo code that filters a list", confirm skill activates and runs `scripts/ensure-ilo.sh`.
- [ ] **Cold install path**: on a machine without `ilo` installed, run `skills/ilo/scripts/ensure-ilo.sh` directly, confirm it installs from GitHub release (or falls back to npm) and that `ilo --version` works afterward.
- [ ] **Update path**: with an older `ilo` already installed, run `scripts/ensure-ilo.sh` and confirm it upgrades to the latest tag.
- [ ] **Offline path**: disconnect network and run `scripts/ensure-ilo.sh`; confirm it exits non-zero with a clear error rather than hanging.
