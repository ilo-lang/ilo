# Releasing ilo

## Tag conventions

ilo uses CalVer (`YY.M`) for the public-facing release name; tags follow `vYY.M(.P)?`. Legacy semver `vX.Y.Z` is still accepted by the validator for any pre-CalVer hotfix lines.

| Tag format | Meaning | Published to |
|------------|---------|-------------|
| `vYY.M` | First release of month YY.M (e.g. `v26.5`) | GitHub Releases, crates.io, npm |
| `vYY.M.P` | Patch P of month YY.M (e.g. `v26.5.1`) | GitHub Releases, crates.io, npm |
| `vYY.M-dev.N` / `vYY.M.P-dev.N` | Dev iteration (testable build) | GitHub Releases (git tag only) |
| `vX.Y.Z`, `vX.Y.Z-dev.N` | Legacy semver line | Same as above |

- Cargo and npm both require strict semver in package metadata. The release workflow normalises any `YY.M` tag to `YY.M.0` before running `npm version` / `cargo publish`, so `Cargo.toml` and `*/package.json` must always be the **normalised** form (`26.5.0`, not `26.5`).
- `N` in `-dev.N` is a monotonically increasing integer that **resets to 1 on each new minor version**.
- Dev tags are pushed to the repo and get a GitHub release with binaries, but are **never published to crates.io or npm**.
- Only clean tags (no `-dev.` suffix) trigger crates.io / npm publishing.

## Soaked release definition

A release is considered "soaked" and ready for a clean `vX.Y.Z` tag when all of the following are true:

1. The full test suite passes (`cargo test --all-features`).
2. Cross-engine tests pass (`tests/examples_engines` or equivalent).
3. Examples engines test suite passes.
4. At least one full persona run has been completed against Opus without regressions.

Only soaked builds should receive a clean release tag.

## Release-prep checklist

1. Bump the version in `Cargo.toml`, `Cargo.lock` (the `[[package]] name = "ilo"` entry), `npm/package.json`, and `pi/package.json` — all to the **normalised semver form** (`YY.M.0` for a first-of-month release, `YY.M.P` for patches).
2. Update `CHANGELOG.md` — move entries from `Unreleased` to the new version heading.
3. Run the full test suite locally:
   ```
   cargo test --all-features
   ```
4. Confirm the build is soaked (see definition above).
5. Create and push the tag in CalVer form (no `.0` needed in the tag itself):
   ```
   git tag v26.5            # or v26.5.1 for a patch
   git push origin v26.5
   ```
6. CI normalises `v26.5` → `26.5.0`, builds, creates the GitHub release, and publishes to crates.io and npm.

## Dev iteration workflow

When you want a testable build without cutting a real release:

```
git tag vX.Y.Z-dev.N
git push origin vX.Y.Z-dev.N
```

CI will build binaries and create a GitHub pre-release, but will **skip** crates.io and npm publishing. The GitHub release is marked as a pre-release automatically.

## CI tag validation

The release workflow validates the pushed tag before doing anything else. The rules are:

- Allowed: `vYY.M` (e.g. `v26.5`)
- Allowed: `vYY.M.P` (e.g. `v26.5.1`)
- Allowed: `vX.Y.Z` (legacy semver, e.g. `v0.12.0`)
- Allowed: any of the above with a `-dev.N` suffix (e.g. `v26.5-dev.1`, `v0.12.0-dev.3`)
- Any other tag format matching `v*` will cause the workflow to fail immediately.

This prevents accidental publishes from experimental or malformed tags.
