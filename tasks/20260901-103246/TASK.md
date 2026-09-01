# Keep routine morning backlog clear

- STATUS: CLOSED
- PRIORITY: 60
- TAGS: briefing

## Goal

Keep routine planned backlog clear in the Scufris morning briefing. Reserve
`attention` for a current problem or work that requires owner or agent
intervention.

## Decision

Open task count, task age, and absence from recent commits do not by themselves
require attention. A green master and clean checkout with open planned tasks is
`ok`. Broken or blocked work, overnight changes needing disposition, a release
decision, and a missing manual prerequisite can require attention. The briefing
checks recent CI so it can distinguish repository health from routine backlog.

## Verification

Completed on 2026-09-01:

- Parsed `.scufris.toml` with Python `tomllib` and asserted that its CI check
  and explicit clear and attention policy are present.
- The configured `tatr ls` query completed successfully.
- `git diff --check` passed.

