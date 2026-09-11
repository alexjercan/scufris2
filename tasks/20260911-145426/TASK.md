# Terminal handoff production path

- STATUS: CLOSED
- PRIORITY: 90
- TAGS: pi, orchestration, protocol

Implement the production-ready terminal handoff described in
`native-pi-scufris-clone/tasks/20260911-120039/DESIGN.md` without changing Pi
or its package. A normal interactive Pi in scufris2 temporarily becomes the
sole leased Scufris foreground agent: canonical HUD conversation both ways,
protocol turn ids and response correlation, session-lineage fork and catch-up,
lease fencing with heartbeat recovery, movable foreground job ownership,
durable job and briefing visibility from terminal and phone, proactive wakes,
TTS policy, command and compaction policy, lifecycle recovery, security,
configuration, packaging, documentation, and migration/rollback. Images and
rich widgets stay out of scope.

Use the proof-of-concept commits as source material and reconcile against
current master. Preserve the untracked surface study from
`terminal-pi-surface-study/tasks/20260911-102043/`. Measure and record the
Phase 0 Pi behaviour gates before code, and use the documented fallback when a
gate fails. Convert debug naming to production naming, off by default.

## Completion

Implemented, verified through the recorded Phase 0 and acceptance gates, and
landed on `master` as `ee84f869a7c604695e689c243feb9b38d22eda7d`.
