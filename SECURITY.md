# Release security

This page documents the security gates that run before an ilo release tag is
cut. The goal is simple: a leaked credential, API key, private key, or other
secret should never make it onto a published artifact, a published crate, a
published npm package, or a GitHub release.

## Secret scan (gitleaks)

Every push of a `v*` tag triggers `.github/workflows/release.yml`. The first
job is `secret-scan`, which runs
[`gitleaks/gitleaks-action@v2`](https://github.com/gitleaks/gitleaks-action)
over the full repository history. All downstream jobs (`build`, `build-wasm`,
`release`, `publish-crates`, `publish-npm`, `publish-pi`) declare
`needs: secret-scan`, so any finding blocks the entire release.

### What gets scanned

- Working tree (every tracked file).
- Full git history (`fetch-depth: 0`).
- Default gitleaks rule pack: AWS, GCP, Azure, GitHub, OpenAI, Anthropic,
  Stripe, Slack, JWT, generic high-entropy strings, PEM blocks, and more.

### Whitelist

Placeholder credentials shipped in `examples/` (especially `examples/apps/*`
for LLM-client and ScrapingBee demos) are explicitly allowed in
[`.gitleaks.toml`](./.gitleaks.toml). The current allow regex set:

- `SCRAPINGBEE_KEY_PLACEHOLDER_set_via_env_in_real_use`
- `sk-PLACEHOLDER[-_A-Za-z0-9]*`
- `REPLACE_ME` / `YOUR_*_HERE` / `EXAMPLE_*_KEY`

If a new example needs a placeholder credential, add it to the allowlist in
the same PR.

### Running locally

Before pushing a tag, or any time you want to sanity-check the working tree:

```sh
gitleaks detect --source . --no-git --redact --verbose
gitleaks detect --source . --redact --verbose   # includes git history
```

A clean run prints `no leaks found`. Anything else is a real finding to
triage before the release goes out.

## Why release-only, not per-PR

Running gitleaks on every PR added meaningful queue time without much
incremental safety: secrets in feature branches are caught at merge time by
GitHub's native push-protection, and the release gate is the last guarantee
before anything becomes public. The release-only model keeps developer
feedback fast and still blocks the public artifact path.

## If the scan finds something

1. The release job will fail with `secret-scan` red. No artifacts are built.
2. Treat the finding as a real incident: rotate the credential immediately,
   regardless of where the leak appears (working tree, history, comment, or
   doc).
3. Once rotated, scrub the secret from history (`git filter-repo` or
   BFG), force-push the cleaned history, and re-cut the tag.
4. If the finding is a false positive on a new placeholder shape, extend the
   allowlist in `.gitleaks.toml` in a follow-up PR and re-cut the tag.
