# Fix missing scufris_report in pi workers

- STATUS: CLOSED
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
- `npm run format:check` initially reported the pre-existing
  `tasks/20260901-103246/TASK.md`; its trailing blank line was removed before
  release checks.

## Follow-up audit and release

No equivalent packaged/source mismatch remains. The extension helper resolver
already tests both layouts. The jobs inspection script resolves from the
`scripts` directory in both layouts. Briefing and report helpers resolve
siblings under `tools`, and the Quick Review adapter uses the shared resolver.

Prepared patch release 2.1.3 and verified:

- Clean `npm ci` and `npm run check`: 87 tests passed.
- Python unit suite: 277 tests passed.
- `shellcheck scripts/scufris-dev`: passed.
- Cargo clippy and tests: 374 tests passed.
- `nix fmt -- --check .`: passed.
- `nix flake check -L`: passed.
- `git diff --check`: passed.

The checklist's standalone Ruff commands remain stale against the existing
2.1.2 tree: Ruff reports the den backend's build-time prelude names and would
reformat 13 existing Python files. This release does not change those files or
weaken the passing package checks.
