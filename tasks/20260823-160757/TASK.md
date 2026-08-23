# Add complete Scufris job inspection commands

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: jobs

## Goal

Add explicit complete-list and direct job-inspection forms to the private
read-only `scripts/scufris-jobs` CLI without breaking its existing no-argument
or JSON list behavior.

## Decisions

- Use `all` as the command and `--all` as its consistent flag alias. Retain no
  argument as a backward-compatible list alias.
- Accept 1 to 12 lowercase hexadecimal ID characters. Resolve a unique prefix
  to its exact durable ID. Reject missing and ambiguous prefixes before
  inspection; never select the first ambiguous match.
- Keep `{"jobs": [...]}` as the JSON list shape. Emit one canonical inspection
  object for JSON detail and `{"error": ...}` for lookup failures.
- Keep plain list output stable and independent of terminal detection. Use
  fixed-width, bounded columns and an explicit legend for ID, state, pane
  liveness, project, workspace, worker, and summary.
- Include all canonical useful detail for one job: durable metadata, exact tmux
  IDs and liveness, bounded events, report, pinned project context, and prompt.
  Continue to use the owning helper for validation and bounded artifact reads.
- Ignore state-root entries that are not exact job-directory names. Render an
  exact malformed job record as `invalid` in complete lists, but fail direct
  inspection with a clear error.
- Keep the command read-only. Accept no state-root, path, tmux target, command,
  or other arbitrary operational input.

## Conflict note

Active task `20260823-153419` changes linked report behavior and currently
modifies `tools/jobs/scufris-jobs`, `tests/test_scufris_jobs.py`, and
`docs/src/workflow.md`. This task also touches those files. Landing after that
work can require a semantic merge, especially around the helper's `inspect`
result and report assertions. The standalone CLI design does not depend on the
report storage format because it consumes the canonical inspection result.

## Implementation

- Reworked `scripts/scufris-jobs` around explicit list and detail paths while
  retaining the existing no-argument list and `--json` behavior.
- Extended the canonical helper inspection result with creation, ownership,
  workspace path, landing branch, and exact tmux metadata needed by full direct
  diagnostics.
- Added focused integration coverage for empty lists, command and flag aliases,
  stable headers, JSON compatibility, exact and unique-prefix detail, missing
  and ambiguous IDs, invalid IDs, invalid flag composition, events, report,
  prompt, and durable metadata.
- Updated the durable workflow documentation and CLI help.

## Verification

- `python3 -m unittest -v tests.test_scufris_jobs` - 6 tests passed.
- `ruff check scripts/scufris-jobs tools/jobs/scufris-jobs tests/test_scufris_jobs.py`
  - passed.
- `ruff format --check scripts/scufris-jobs tools/jobs/scufris-jobs tests/test_scufris_jobs.py`
  - passed.
- `npm run check` - passed: TypeScript typecheck, 51 tests, and Prettier.
- `git diff --check` - passed.
- Direct active-state smoke checks:
  - `./scripts/scufris-jobs all` rendered the fixed legend, header, separator,
    and every current job.
  - `./scripts/scufris-jobs 7c1d6f` resolved this job and rendered metadata,
    events, report, project context, and prompt.
- The fresh Sprout initially lacked ignored Node dependencies. `npm ci` restored
  the lockfile dependencies before the successful final `npm run check`.

## Independent review follow-up

Resolved all review findings:

- Human output now escapes C0, C1, format, bidirectional, and other Unicode
  control characters as visible ASCII escapes. Newlines remain structural only
  in labeled multiline blocks. JSON values are unchanged.
- Table clipping and padding now count standard-library Unicode combining and
  East Asian display widths. No runtime dependency was added.
- Job inspection opens `job.json`, `status`, `report.md`,
  `project-context.md`, and `prompt.md` by descriptor with `O_NOFOLLOW` and
  `O_NONBLOCK`, verifies a regular file with `fstat`, and performs bounded
  reads. Incremental event reads and Quick Review artifact reads use the same
  primitives.
- Durable records now require the exact field set and validate field types,
  identifier and enum domains, bounded printable text, timestamp syntax,
  absolute working directories, exact tmux IDs, and project/workspace/feature/
  review consistency. Complete lists render invalid records explicitly; direct
  lookup fails closed.
- Focused fixtures cover unknown fields, wrong types, invalid domains,
  inconsistent workspaces, malformed and oversized records, symlinks, FIFOs,
  oversized status/report/context/prompt files, terminal escape content, exact
  JSON retention, and wide plus combining Unicode table cells.

### Linked-report compatibility

Inspected active task `20260823-153419` at commit `72de350`. This follow-up uses
its 2 MiB complete linked-report inspection bound, 512 KiB prompt/context detail
bound, and newest 512 KiB Quick Review handoff. Inspection consumes the
canonical chronological report string and does not depend on replacement or
append write semantics. When landing both tasks, retain this task's strict read
and validation primitives around that task's `MAX_REPORT_FILE`,
`MAX_REPORT_DETAIL`, `open_job_artifact`, and linked write behavior.

## Follow-up verification

- `python3 -m unittest -v tests.test_scufris_jobs` - 9 tests passed,
  including all adversarial fixtures.
- `ruff check scripts/scufris-jobs tools/jobs/scufris-jobs tests/test_scufris_jobs.py`
  - passed.
- `ruff format --check scripts/scufris-jobs tools/jobs/scufris-jobs tests/test_scufris_jobs.py`
  - passed.
- `npm run check` - passed: TypeScript typecheck, 51 tests, and Prettier.
- `git diff --check` - passed.
- Direct `all` and JSON unique-prefix detail smoke checks passed against current
  durable state with strict record validation enabled.

## Master integration

Integrated current `master` after tasks `20260823-153415` and
`20260823-153419` landed. The semantic conflict resolution retains:

- Per-job worker and trusted failure capabilities, one-use launch capability,
  and private authorization records.
- Locked temporary report construction, atomic `report.md` replacement,
  directory flush, and status publication only after durable linked evidence.
- Complete linked-report rollover bounds and newest Quick Review handoff.
- Descriptor-based bounded `O_NOFOLLOW|O_NONBLOCK` regular-file reads, strict
  durable-record validation, and fail-closed direct inspection.
- Complete list aliases, direct exact or unique-prefix lookup, exact JSON,
  terminal-safe human output, and Unicode display-cell table alignment.
- Both linked-report fault/capability tests and CLI security/format tests. The
  direct CLI assertion now verifies the post-rollover chronological linked
  report instead of the obsolete replacement report.
- Combined durable documentation for linked reporting, Wake and Calm behavior,
  and private job inspection.

## Integration verification

- `sprout sync scufris-jobs-inspection --dry-run` identified the two expected
  helper and test conflicts. `sprout sync scufris-jobs-inspection` merged
  current master, and both conflicts were resolved semantically.
- `python3 -m unittest -v tests.test_scufris_jobs` - 12 tests passed, including
  atomic linked-report fault injection, capability isolation, rollover bounds,
  complete CLI behavior, invalid artifacts, terminal escaping, and Unicode
  alignment.
- `ruff check scripts/scufris-jobs tools/jobs/scufris-jobs tests/test_scufris_jobs.py`
  - passed.
- `ruff format --check scripts/scufris-jobs tools/jobs/scufris-jobs tests/test_scufris_jobs.py`
  - passed.
- `npm run check` - passed: TypeScript typecheck, all 55 tests, and Prettier.
- `git diff --check` - passed.
- Direct complete-list and JSON unique-prefix inspection passed against current
  durable state after integration.

## Concerns

No implementation concern is known. The previous linked-report overlap is now
resolved in this Sprout. This Sprout has not been landed.
