# Privacy Policy

**ilo** — a token-optimised programming language for AI agents

*Last updated: 2026-03-07*

## Data Collection

ilo does not collect, store, transmit, or process any personal data. The ilo CLI runs entirely on your local machine.

## Network Access

ilo makes no network requests except when:

- **You explicitly use `get!`** — an HTTP GET builtin that fetches a URL you provide in your program. No data is sent beyond the standard HTTP request to the URL you specify.
- **You explicitly use `post!`** — an HTTP POST builtin that sends data you provide to a URL you specify.

ilo never phones home, has no telemetry, no analytics, and no update checks.

## Claude Code Plugin

The ilo Agent Skill (Claude Code plugin) teaches AI agents how to write and run ilo programs. The plugin itself:

- Does not collect or transmit any data
- Does not add telemetry or tracking
- May download the ilo binary from GitHub Releases during installation (a standard public download with no authentication or tracking beyond GitHub's default server logs)

## Third Parties

ilo shares no data with third parties. Pre-built binaries are hosted on GitHub Releases.

## Contact

For privacy questions, open an issue at [github.com/ilo-lang/ilo](https://github.com/ilo-lang/ilo).
