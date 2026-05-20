Models keep getting bigger and hungrier for tokens. Usage is moving from single LLMs to swarms of agents working in teams and workflows, which multiplies token spend further.

The community has been working on this from several angles: "caveman" agent skills that make outputs terse, codebase indexing (vector DB, embeddings, RLM) to cut filesystem search, MCP to share tools, prompt caching, smaller models for cheap steps. All real solutions to real costs.

ilo bets on a different angle: the language itself. As agents write more of the code, and more of that code talks to other agents, the source representation matters. Python is 3x heavier than dense ilo across a five-task suite. Zero is roughly Python-equivalent. The wire formats agents read every day (JSON, OpenAPI, file contents) are heavy in tokens too. Most "tokens spent reasoning about code" turns out to be tokens spent re-reading the same code.

The question I'm trying to answer here: what does a source language designed for an agent to write in, and another agent to read, look like? ilo is one bet. The manifesto sets out six principles. The implementation is on GitHub.

## Use Discussions for

- Design proposals (sigils, builtins, syntax shapes)
- Programs you've written, agent workflows, surprising output
- Questions, including basic ones
- Meta: how ilo fits alongside other token-reduction angles. Where it should and shouldn't go.

## Use Issues for

- Bugs with a clear repro
- Concrete feature requests with a proposed shape
- Anything blocking your work

If a thread here turns into a concrete bug or feature, I'll convert it.

## Resources

- [Manifesto](https://github.com/ilo-lang/ilo/blob/main/MANIFESTO.md): six principles behind every design call
- [SPEC.md](https://github.com/ilo-lang/ilo/blob/main/SPEC.md): language reference
- [examples/](https://github.com/ilo-lang/ilo/tree/main/examples): 229 small programs
- [ilo-lang.ai](https://ilo-lang.ai): install, docs, blog
- [Zero and ilo: Two Layers of the Same Agent Stack](https://danieljohnmorris.com/writing/zero-and-ilo-two-layer-agent-stack) for the strategic framing

## What I find hardest

Writing the manifesto. The audience is agents rather than humans, and that changes every design call. Sigil readability for someone glancing at the file versus single-token compressibility for a context window. Usually I land on the latter, but the argument is the interesting part. I'd value input from anyone working the same problem from another angle.
