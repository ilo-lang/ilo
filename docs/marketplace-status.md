# Plugin marketplace status (as of 2026-05-02)

This is a research-only snapshot of where the `ilo` Claude Code plugin (currently at
`v0.10.1`, see `.claude-plugin/marketplace.json`) is published and what, if anything,
needs to be done to surface that release on the various Claude Code / Agent Skills
marketplaces.

## TL;DR

`v0.10.1` is **fully live for any user who runs `/plugin marketplace add ilo-lang/ilo`**
(self-hosted GitHub marketplace path).

`v0.10.1` is **not live** in Anthropic's official community marketplace
(`anthropics/claude-plugins-community`). That marketplace pins us to commit
`4704712` (= `v0.8.0`, March 15) under a stale repo URL
(`danieljohnmorris/ilo-lang.git`). It does **not** auto-poll source repos for new
commits; the entry only refreshes when something on Anthropic's side re-runs the
internal review/vendor pipeline. To get `v0.10.1` listed there, the plugin has to
be re-submitted via [clau.de/plugin-directory-submission](https://clau.de/plugin-directory-submission)
(which redirects to `https://code.claude.com/docs/en/plugins#submit-your-plugin-to-the-official-marketplace`,
listing `claude.ai/settings/plugins/submit` and `platform.claude.com/plugins/submit`
as the in-app forms).

## How Claude Code marketplaces actually work

Anthropic's plugin system has two layers, both documented at
[code.claude.com/docs/en/plugin-marketplaces](https://code.claude.com/docs/en/plugin-marketplaces)
and [code.claude.com/docs/en/discover-plugins](https://code.claude.com/docs/en/discover-plugins):

1. **A marketplace** is just a git repo (or URL) hosting a
   `.claude-plugin/marketplace.json` catalog. Users register one with
   `/plugin marketplace add <owner>/<repo>` and install plugins from it with
   `/plugin install <name>@<marketplace>`. Marketplaces are decentralised - anyone
   can publish one.
2. **A plugin** is the thing the marketplace points at via a `source` entry. The
   source can be a GitHub repo (`{ "source": "github", "repo": "owner/repo" }`),
   a relative path (`"./"`), a git URL, or an npm package.

Version semantics (per [plugins-reference#version-management](https://code.claude.com/docs/en/plugins-reference)):

- If `plugin.json` has a `version` field, that string is the cache key. Users only
  receive a new copy when the field is bumped. `ilo` sets `"version": "0.10.1"`
  in both `plugin.json` and the marketplace entry, so this is our story.
- Otherwise the git commit SHA is the cache key.
- Users pull updates manually with `/plugin marketplace update <name>` followed
  by `/plugin update <name>@<marketplace>`, or automatically if the marketplace
  has auto-update enabled.

A marketplace `source` entry can additionally pin a specific commit `sha`. When it
does, a `/plugin install` of that plugin checks out exactly that commit on the
user's machine, regardless of what's now on the source repo's main branch. That's
the mechanism keeping us at v0.8.0 in the official community marketplace.

## Centralised Anthropic marketplace

There are three Anthropic-managed marketplaces:

| Repo | What it is | Auto-loaded? |
|------|------------|--------------|
| [`anthropics/claude-plugins-official`](https://github.com/anthropics/claude-plugins-official) | First-party + Anthropic-Verified third-party plugins (`github`, `linear`, `commit-commands`, etc.) | Yes - `claude-plugins-official` is loaded automatically at Claude Code startup |
| [`anthropics/claude-plugins-community`](https://github.com/anthropics/claude-plugins-community) | Read-only mirror of the community plugin marketplace. 1920 plugins as of 2026-05-01 sync. | No - users opt in with `/plugin marketplace add anthropics/claude-plugins-community` |
| [`anthropics/skills`](https://github.com/anthropics/skills) | Anthropic's first-party Agent Skills examples / reference. Marketplace name `anthropic-agent-skills`, only 3 plugins (`document-skills`, `example-skills`, `claude-api`). Not a community submission target. | No |

`ilo` is **not** in `claude-plugins-official`. That repo is curated by Anthropic
and only Anthropic-verified partners appear there. There's no public PR-based
path; PRs to `claude-plugins-community` are auto-closed (per the repo README).

`ilo` **is** in `claude-plugins-community`, but stale. Confirmed by inspecting
`https://raw.githubusercontent.com/anthropics/claude-plugins-community/main/.claude-plugin/marketplace.json`:

```json
{
  "name": "ilo",
  "description": "Write, run, debug, and explain programs in ilo - a token-optimised programming language for AI agents",
  "source": {
    "source": "url",
    "url": "https://github.com/danieljohnmorris/ilo-lang.git",
    "sha": "4704712416efea06db93bfb7cd5f29d36315ea2a"
  },
  "homepage": "https://github.com/danieljohnmorris/ilo-lang"
}
```

Notes on this entry:

- Repo URL `danieljohnmorris/ilo-lang.git` redirects (HTTP 301) to
  `ilo-lang/ilo`, so the install would still resolve. But it's the wrong
  canonical URL.
- The pinned `sha` is `4704712`. That commit exists in our history; it's
  `perf: add OP_ADD_SS string concat opcode...` from 2026-03-15. The
  `marketplace.json` at that SHA declares `"version": "0.8.0"`. So users who
  install `ilo@claude-community` today get v0.8.0.
- The most recent sync commit on the community marketplace
  (`7a773c6b`, 2026-05-01) did not touch the `ilo` entry, confirming the
  pipeline doesn't auto-refresh source SHAs - it only re-vendors when
  something on Anthropic's side triggers a re-pull (e.g. a fresh
  submission).

### Submission flow

Per [code.claude.com/docs/en/plugins#submit-your-plugin-to-the-official-marketplace](https://code.claude.com/docs/en/plugins):

> To submit a plugin to the official Anthropic marketplace, use one of the
> in-app submission forms:
> - Claude.ai: claude.ai/settings/plugins/submit
> - Console: platform.claude.com/plugins/submit

Both URLs are gated (the bare URL returns 403 without a session). The
`claude-plugins-community` README states that every listed plugin "has been
submitted via clau.de/plugin-directory-submission, passed automated security
scanning, and been approved for distribution". So the flow is: submit form ->
automated security scan -> nightly sync into the public mirror.

There's no public bumping endpoint - re-submitting the form is the only known
way to refresh an existing entry's pinned SHA / metadata.

## Community marketplaces

| Marketplace | URL | Has `ilo`? | Submission |
|-------------|-----|------------|------------|
| `alirezarezvani/claude-skills` | [github.com/alirezarezvani/claude-skills](https://github.com/alirezarezvani/claude-skills) | No - confirmed by grepping the live `marketplace.json`. The catalog name is `claude-code-skills` and lists 232+ skills, none of them `ilo`. | Repo description doesn't list a public submission flow; would likely require opening a PR or contacting the maintainer. |
| `claudemarketplaces.com` | [claudemarketplaces.com](https://claudemarketplaces.com) | Not found. This is an aggregator/directory ranked by install count and GitHub stars; not a `marketplace.json`-based source. | Could not verify a submission form. |
| `buildwithclaude.com` | [buildwithclaude.com](https://buildwithclaude.com) | Could not verify - 403 from the page. | Could not verify. |
| `aitmpl.com/plugins` | [aitmpl.com/plugins](https://www.aitmpl.com/plugins/) | Not checked end-to-end. | Not checked. |

So the only confirmed community listing is the official Anthropic community
mirror, and there it's stale.

## User-side experience for ilo today

Two install paths work right now:

```
# Path A - direct from the canonical repo, gets v0.10.1
/plugin marketplace add ilo-lang/ilo
/plugin install ilo@ilo-lang

# Path B - via Anthropic's community marketplace, currently gets v0.8.0
/plugin marketplace add anthropics/claude-plugins-community
/plugin install ilo@claude-community
```

To upgrade an existing install:

```
/plugin marketplace update ilo-lang     # refresh the catalog
/plugin update ilo@ilo-lang             # pull the new version
/reload-plugins                          # apply without restart
```

Because `plugin.json` carries an explicit `"version": "0.10.1"`, every release
must bump that field for users to receive it. New commits on `main` without a
version bump are invisible to clients (per the `Warning` in
[plugins-reference#version-management](https://code.claude.com/docs/en/plugins-reference#version-management)).

## Action items

If the goal is **v0.10.1 should be installable everywhere `ilo` is currently
listed**, only one thing is outstanding:

1. **Re-submit `ilo` to the Anthropic plugin directory** at
   [clau.de/plugin-directory-submission](https://clau.de/plugin-directory-submission)
   (or one of the in-app forms) so the next nightly sync of
   `claude-plugins-community` re-vendors with:
   - URL: `https://github.com/ilo-lang/ilo.git` (the canonical org URL, replacing
     the personal `danieljohnmorris/ilo-lang.git`)
   - SHA: a commit at or after the `v0.10.1` tag

   No code changes needed; this is a form submission only.

If the goal is **broader distribution**, optional follow-ups:

2. Open a PR or contact the maintainer of `alirezarezvani/claude-skills` to
   request inclusion (no documented submission flow found - assume PR-based).
3. Submit the canonical marketplace URL (`ilo-lang/ilo`) to aggregator sites
   like `claudemarketplaces.com` and `buildwithclaude.com` if they offer a
   submission form (not verified during this research).

Anything beyond that requires no further action - GitHub-hosted self-distribution
already works for any user who knows to add `ilo-lang/ilo` as a marketplace.

## References

All sources accessed 2026-05-02.

- [code.claude.com/docs/en/discover-plugins](https://code.claude.com/docs/en/discover-plugins) - how users add marketplaces and install plugins; confirms `claude-plugins-official` is auto-loaded and `claude-plugins-community` is opt-in.
- [code.claude.com/docs/en/plugin-marketplaces](https://code.claude.com/docs/en/plugin-marketplaces) - marketplace authoring, `source.sha` pinning semantics.
- [code.claude.com/docs/en/plugins](https://code.claude.com/docs/en/plugins) - plugin authoring + the "Submit your plugin to the official marketplace" section pointing at the in-app forms.
- [code.claude.com/docs/en/plugins-reference](https://code.claude.com/docs/en/plugins-reference) - `version` field semantics; `plugin.json.version` overrides marketplace version, falls back to commit SHA.
- [github.com/anthropics/claude-plugins-official](https://github.com/anthropics/claude-plugins-official) - confirmed Anthropic-curated, no public PR submission path.
- [github.com/anthropics/claude-plugins-community](https://github.com/anthropics/claude-plugins-community) - read-only mirror; submission flow via `clau.de/plugin-directory-submission`; PRs auto-closed.
- [github.com/anthropics/skills](https://github.com/anthropics/skills) - first-party skills repo, marketplace `anthropic-agent-skills`, 3 plugins, not a community target.
- [github.com/alirezarezvani/claude-skills](https://github.com/alirezarezvani/claude-skills) - 232+ skill aggregator marketplace; `ilo` not listed (verified by grep on live `marketplace.json`).
- [agentskills.io/clients](https://agentskills.io/clients) - lists Claude Code, Claude.ai, Cursor, Codex and ~30 other clients supporting the Agent Skills format. No plugin index.
- [claudemarketplaces.com](https://claudemarketplaces.com) - third-party directory; `ilo` not surfaced in search.
- Live JSON: `https://raw.githubusercontent.com/anthropics/claude-plugins-community/main/.claude-plugin/marketplace.json` - the stale `ilo` entry quoted above.
- Live JSON: `https://raw.githubusercontent.com/anthropics/claude-plugins-official/main/.claude-plugin/marketplace.json` - confirmed `ilo` is not present.
