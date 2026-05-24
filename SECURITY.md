# Release security

This page documents the security gates that run before an ilo release tag is
cut. The goal is simple: a leaked credential, API key, private key, or other
secret should never make it onto a published artifact, a published crate, a
published npm package, or a GitHub release.

## Secret-scan posture

Pre-release secret scanning is **off the release workflow** as of v26.5.
GitHub's native push-protection catches the common credential shapes at
push time, and the release path is owned end-to-end (only `v*` tags from
maintainers can publish). Previous releases ran
[`gitleaks/gitleaks-action@v2`](https://github.com/gitleaks/gitleaks-action)
as a `secret-scan` job, but the action started requiring a paid licence for
organisation repos and blocked every release without a license key.

If you want a local pre-tag scan, the gitleaks CLI is still free:

```sh
gitleaks detect --source . --no-git --redact --verbose
gitleaks detect --source . --redact --verbose   # includes git history
```

Run it manually before pushing a `v*` tag if the changeset touches anything
sensitive. A maintainer should triage and rotate any finding before
re-cutting the tag.

## Install-script integrity verification

The release workflow's `release` job runs `sha256sum ilo-* > checksums-sha256.txt`
and uploads the resulting file alongside every published binary. The
`curl ... | sh` installers shipped from `https://ilo-lang.ai/install.sh` and
`/install.ps1` (canonical source in [`scripts/install/`](./scripts/install/))
fetch that checksum file together with the binary and refuse to install if
the SHA-256 doesn't match. This closes the standard supply-chain attack
window on the curl-pipe install path: a tampered binary on GitHub's CDN, a
TLS-intercepted download, or a mirrored asset all fail the check before the
binary is made executable. An offline regression test
(`scripts/install/test-install-sh.sh`) runs on every CI push and exercises
the happy, tamper, and missing-asset code paths.

## If a secret leaks

1. Treat the finding as a real incident: rotate the credential immediately,
   regardless of where the leak appears (working tree, history, comment, or
   doc).
2. Once rotated, scrub the secret from history (`git filter-repo` or
   BFG), force-push the cleaned history, and re-cut the tag.
