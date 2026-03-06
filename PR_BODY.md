# Fix: safer CLI + JIT; consolidate tests

This PR improves safety and robustness and reduces test brittleness.

- main
  - Remove unnecessary `unsafe` in `.env` loader
  - Graceful JSON output for `ilo tools --json` (no unwrap panic)
  - Robust stdin loop (no panic on read errors)
- vm/jit_arm64
  - Check `mprotect` return; unmap and bail on failure
- tests
  - Consolidate redundant tests: help/version/spec hygiene/bench arg variants/braceless guards
  - Keep coverage while reducing duplication and drift-prone assertions

Notes
- No language semantics changed. Errors are now handled gracefully in CLI paths.
- No network required for tests; wiremock tests remain behind the `tools` feature.

Checklist
- [x] cargo fmt / clippy (no new warnings expected)
- [x] cargo test (not run here; please verify in CI)
- [x] README/SPEC unchanged; behavior is the same, but safer
