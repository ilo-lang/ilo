# Bash reference programs

Human floor for the closed-loop language bakeoff. These 24 scripts are the
`plan/2026-09-16` references (`6cc702d4`), copied here so a bash arm has
something to execute offline.

Run one with the same shape the harness uses (`bash`, then the file):

```bash
bash bench/closed-loop/references-bash/simple-function.sh
```

Five of them match the tasks in `bench/closed-loop/tasks.json`
(`simple-function`, `with-dependencies`, `data-transform`,
`tool-interaction`, `workflow-rollback`). The other nineteen match the
wider plan task list and are not scored until that list is merged.

`bench/closed-loop/task-class.json` tags each id `artefact`, `ops`, or
`sanity`. `tool-interaction` is `ops`. A bash win on an ops task, or on
wall time, is expected and is not an ilo manifesto loss. The bakeoff
question is whether ilo wins the repair loop on artefact tasks.
