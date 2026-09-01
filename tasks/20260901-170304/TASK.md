# Fix missing scufris_report in pi workers

- STATUS: OPEN
- PRIORITY: 100
- TAGS: subagent

## Cause

The jobs helper resolved `worker-report.ts` only as
`<resource-root>/agent/extensions/...`. That path exists in the source tree,
but packaged resources place it at `<resource-root>/extensions/...`. The
packaged helper therefore exited before Pi started.

## Decision

Resolve the packaged layout first and retain the source-tree layout as a
fallback. Keep the validation in the trusted launch helper, before Pi starts.

## Verification

- `python3 -m unittest discover -s tests -p 'test_scufris_jobs.py'`: 34 passed.
- `nix build --no-link .#checks.x86_64-linux.resources`: passed.
- Loaded the built resources helper and confirmed that
  `worker_report_extension()` resolves the packaged `worker-report.ts`.
- `npm run typecheck`: passed.
- `npm run check`: 85 TypeScript tests passed and one unrelated Calm test
  failed because its installed Pi store path lacks `dist/modes/interactive/theme/dark.json`.
- `npm run format:check` also reports the pre-existing untracked
  `tasks/20260901-103246/TASK.md`.
