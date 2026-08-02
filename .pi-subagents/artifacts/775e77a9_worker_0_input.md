# Task for worker

You are a delegated subagent running from a fork of the parent session. Treat the inherited conversation as reference-only context, not a live thread to continue. Do not continue or answer prior messages as if they are waiting for a reply. Your sole job is to execute the task below and return a focused result for that task using your tools.

Task:
Integrate the 6 roadmap feature branches into a local `release/26.8` branch. Work directly in this main checkout (NOT a worktree). The release stays LOCAL — do not push.\n\n## The 6 branches (all off main, all additive)\n1. `feat/pipe-shortcircuit` (ILO-510) — Result-aware `>>`\n2. `feat/shadow-tests` (ILO-511) — `test` blocks, ILO-T050/W020\n3. `feat/constrained-decoding` (ILO-512) — `--constrain`/`--logit-masks`/`--completions`\n4. `feat/effect-sigils` (ILO-513) — `/http /fs /io /net /ml /time /rand`, ILO-W051\n5. `feat/tool-policies` (ILO-514) — `policy{domain,tokens,rate}`\n6. `feat/optional-contracts` (ILO-515) — `req`/`ens`, ILO-W030\n\nEach branch has 2 commits: the feature + its doc-sync commit.\n\n## Steps\n1. Create `release/26.8` off `main`: `git checkout main && git checkout -b release/26.8`\n2. Merge the 6 branches ONE AT A TIME in this order (most isolated first):\n   constrained-decoding → pipe-shortcircuit → shadow-tests → tool-policies → effect-sigils → contracts\n3. For EACH merge:\n   - `git merge --no-ff <branch>`\n   - If conflicts: ALL these changes are ADDITIVE (new enum variants, new match arms, new struct fields, new diagnostic codes). Resolve by KEEPING BOTH SIDES. For a struct field conflict, include all fields from both. For a match-arm conflict, include all arms. For a diagnostic-registry conflict, include all entries. NEVER delete one side's addition.\n   - After resolving: `cargo check` MUST pass before continuing. If it fails, fix the resolution (likely a field/arm still missing) until it compiles.\n   - Commit the merge.\n4. After ALL 6 merged: `cargo test --lib` must pass (expect ~3560+ tests). Fix any breakage.\n5. Bump version: in `Cargo.toml` change `version = \"26.5.0\"` → `version = \"26.8.0\"`. Also bump any other version refs (grep for 26.5.0).\n6. Update `CHANGELOG.md`: add a new top section `## 26.8 — 2026-08 (local, unreleased)` with one bullet per feature (copy the key sentence from each feature's work). Keep the existing `## Unreleased` section above or below as appropriate.\n7. Commit: `chore(release): prepare 26.8 local cut — 6 agent-authoring features`\n8. Do NOT push. Do NOT tag.\n\n## Report to /tmp/release-26.8.md\n- final `git log --oneline main..release/26.8` (commit count)\n- conflicts encountered per merge + how resolved\n- final `cargo test --lib` result line\n- confirmation version is 26.8.0 and CHANGELOG updated\n- any residual issues\n\nIf at any point you cannot resolve a conflict cleanly, STOP and report the blocker rather than forcing a broken state."

---
**Output:**
Write your findings to exactly this path: /tmp/release-26.8.md
This path is authoritative for this run.
Ignore any other output filename or output path mentioned elsewhere, including output destinations in the base agent prompt, system prompt, or task instructions.

## Acceptance Contract
Acceptance level: checked
Completion is not accepted from prose alone. End with a structured acceptance report.

Criteria:
- criterion-1: Implement the requested change without widening scope
- criterion-2: Return evidence sufficient for an independent acceptance review

Required evidence: changed-files, tests-added, commands-run, residual-risks, no-staged-files

Review gate: required by reviewer.

Finish with a fenced JSON block tagged `acceptance-report` in this shape:
Use empty arrays when no items apply; array fields contain strings unless object entries are shown.
`criteriaSatisfied[].status` must be exactly one of: satisfied, not-satisfied, not-applicable.
`commandsRun[].result` must be exactly one of: passed, failed, not-run.
`manualNotes` and `notes` are optional strings; an empty string means no note and does not satisfy `manual-notes` evidence.
```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "specific proof"
    },
    {
      "id": "criterion-2",
      "status": "satisfied",
      "evidence": "specific proof"
    }
  ],
  "changedFiles": [
    "src/file.ts"
  ],
  "testsAddedOrUpdated": [
    "test/file.test.ts"
  ],
  "commandsRun": [
    {
      "command": "command",
      "result": "passed",
      "summary": "short result"
    }
  ],
  "validationOutput": [
    "validation output or concise summary"
  ],
  "residualRisks": [
    "none"
  ],
  "noStagedFiles": true,
  "diffSummary": "short description of the diff",
  "reviewFindings": [
    "blocker: file.ts:12 - issue found, or no blockers"
  ],
  "manualNotes": "anything else the parent should know"
}
```