# Releasing ilo

## Tag conventions

| Tag format | Meaning | Published to |
|------------|---------|-------------|
| `vX.Y.Z` | Clean release | GitHub Releases, crates.io, npm |
| `vX.Y.Z-dev.N` | Dev iteration (testable build) | GitHub Releases (git tag only) |

- `N` in `-dev.N` is a monotonically increasing integer that **resets to 1 on each new minor version** (i.e. the first dev build for `0.14.0` is `0.14.0-dev.1`).
- Dev tags are pushed to the repo and get a GitHub release with binaries, but are **never published to crates.io or npm**.
- Only clean `vX.Y.Z` tags trigger crates.io / npm publishing (enforced by CI tag validation — see below).

## Soaked release definition

A release is considered "soaked" and ready for a clean `vX.Y.Z` tag when all of the following are true:

1. The full test suite passes (`cargo test --all-features`).
2. Cross-engine tests pass (`tests/examples_engines` or equivalent).
3. Examples engines test suite passes.
4. At least one full persona run has been completed against Opus without regressions.

Only soaked builds should receive a clean release tag.

## Release-prep checklist

1. Bump the version in `Cargo.toml` (and `npm/package.json` / `pi/package.json` if they track it manually).
2. Update `CHANGELOG.md` — move entries from `Unreleased` to the new version heading.
3. Run the full test suite locally:
   ```
   cargo test --all-features
   ```
4. Confirm the build is soaked (see definition above).
5. Create and push the tag:
   ```
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```
6. CI will build, create the GitHub release, and publish to crates.io and npm.

## Dev iteration workflow

When you want a testable build without cutting a real release:

```
git tag vX.Y.Z-dev.N
git push origin vX.Y.Z-dev.N
```

CI will build binaries and create a GitHub pre-release, but will **skip** crates.io and npm publishing. The GitHub release is marked as a pre-release automatically.

## CI tag validation

The release workflow validates the pushed tag before doing anything else. The rules are:

- Allowed: `vX.Y.Z` (e.g. `v0.13.0`)
- Allowed: `vX.Y.Z-dev.N` (e.g. `v0.13.0-dev.3`)
- Any other tag format matching `v*` will cause the workflow to fail immediately.

This prevents accidental publishes from experimental or malformed tags.
