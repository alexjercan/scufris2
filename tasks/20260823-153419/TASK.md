# Append status-linked report entries

- STATUS: CLOSED
- PRIORITY: 70
- TAGS: workflow

## Goal

Turn `report.md` into an append-only, status-linked conversation between a
worker and foreground Scufris.

## Format

Each report operation appends one entry whose heading is the exact status line
and whose body is the detailed Markdown evidence:

```markdown
# working: implementing filesystem notifications

Updated the watcher lifecycle and added focused tests.

# blocked: shutdown ownership is unclear

The current shutdown contract conflicts with...
```

## Scope

- Append report entries instead of replacing the complete file.
- Write each status heading and report body through one bounded reporting
  operation so evidence is unambiguously linked to its event.
- Preserve ordering: durable report entry first, then the matching status
  notification.
- Include runtime-generated failure entries through the same internal format.
- Define bounded file and entry behavior without losing the newest linked
  evidence.
- Make inspection return the chronological report without reconstructing links
  from separate files.
- Update the Pi report tool, Claude adapter, helper validation, prompts,
  documentation, and integration tests.

## Acceptance

- Multiple worker updates remain visible in chronological order.
- Every status event has one matching report heading and body.
- Status text in the heading exactly matches the appended event line.
- Partial writes cannot expose an event without its durable report evidence.
- Bounds, file modes, symlink refusal, and owned-job validation remain intact.

## Dependencies

Run after `20260823-153415`, and therefore after the status simplification.

## Decisions

- Serialize report operations with a dedicated private regular `report.lock`.
  Build the bounded next report in a private temporary file, flush it, replace
  `report.md` atomically, flush the job directory, and only then append and
  flush the matching status event.
- Render entries as `# <event>: <summary>`, a blank line, and the stripped
  evidence body. Use the same internal writer for worker and trusted runtime
  failures.
- Keep report details at the existing 512 KiB API bound. Bound each complete
  entry to the detail plus the validated 4 KiB event line, and bound report
  history to 2 MiB. When one append would cross the history bound, retain the
  newest complete entry and discard older history instead of cutting an entry.
- Keep `failed` unavailable through the worker tool and Claude adapter. Give
  each worker a random job-bound report capability and give foreground
  orchestration a separate random trusted-failure capability. Protect the
  harness failure path with a one-use launch capability invalidated before the
  worker starts. Persist only SHA-256 capability hashes in a private
  authorization artifact.
- Enforce the 512 KiB limit on encoded UTF-8 evidence in the owning helper and
  reject oversized or invalid UTF-8 standard input in the Claude adapter.
- Return `report.md` directly in chronological order during inspection. Open
  report inputs for inspection and Quick Review with bounded `O_NOFOLLOW`
  regular-file validation. Keep the newest report suffix in bounded Quick
  Review handoff context.

## Verification

- `python3 -m unittest tests.test_scufris_jobs` - 6 integration tests passed.
- `ruff check tools/jobs/scufris-jobs tools/jobs/scufris-report tests/test_scufris_jobs.py` - passed.
- `ruff format --check tools/jobs/scufris-jobs tools/jobs/scufris-report tests/test_scufris_jobs.py` - passed after formatting one test expression.
- `npm run check` - type checking, 51 TypeScript tests, and Prettier checks passed.
- `git diff --check` - passed.
- Independent-review follow-up: `python3 -m unittest tests.test_scufris_jobs` -
  8 integration tests passed, including cross-job capability forgery, one-use
  launch and trusted channel separation, UTF-8 byte bounds, adapter overflow,
  inspection and Quick Review symlinks, and rollover replace/status fault
  injection.
- Independent-review follow-up: Ruff check and format check passed for the jobs
  helper, Claude adapter, and integration tests.
- Independent-review follow-up: `npm run check` - type checking, all 51
  TypeScript tests, and Prettier passed.
- Independent-review follow-up: `git diff --check` - passed.

## Integration note

Task `20260823-153415` changed worker event delivery and the same error loop in
`extensions/scufris/workflow/orchestration.ts`, plus adjacent workflow
documentation. The master integration below resolves that overlap. No helper or
report-format implementation overlapped.

## Master integration

- Synchronized the Sprout with master at `7e481ee` after task
  `20260823-153415` landed.
- Preserved `/wake minimal|all`, session restoration, quiet minimal-mode
  progress, and Wake-mode delivery for status events.
- Resolved malformed status records by publishing the linked `failed` entry and
  event through the job's trusted failure capability. The event reader marks a
  reread immediately, then routes the durable generated event through normal
  Wake-mode delivery instead of sending an unauthenticated or duplicate wake.
- Preserved both sets of durable workflow documentation, including Wake and
  Calm commands plus capability-authenticated atomic report publication.

## Integration verification

- `python3 -m unittest tests.test_scufris_jobs` - 8 integration tests passed.
- `ruff check tools/jobs/scufris-jobs tools/jobs/scufris-report tests/test_scufris_jobs.py` - passed.
- `ruff format --check tools/jobs/scufris-jobs tools/jobs/scufris-report tests/test_scufris_jobs.py` - passed.
- `node --experimental-strip-types --test --test-concurrency=1 tests/agents.test.ts tests/calm.test.ts` - 11 focused Wake, Calm, and orchestration tests passed.
- `npm run check` - type checking, all 55 TypeScript tests, and Prettier passed.
- `git diff --check` - passed.
