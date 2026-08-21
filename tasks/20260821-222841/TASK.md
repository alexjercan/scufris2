# Expose durable agent diagnostics tool

- STATUS: IN_PROGRESS
- PRIORITY: 100
- TAGS: agents, diagnostics, tools

## Goal

Expose the private durable job diagnostics through a narrow read-only native tool without changing existing agent list or inspect behavior.

## Accepted design

- Register `scufris_agent_diagnostics`.
- Keep `scufris_agent_list` and `scufris_agent_inspect` unchanged.
- Invoke the packaged `scripts/scufris-jobs --json` helper without a shell.
- Optional exact `job_id` selects detailed diagnostics.
- Optional `include_finished` lists historical and malformed records when no job ID is supplied.
- Optional `include_report` includes bounded report content for an exact job only.
- Reject invalid option combinations and unknown fields.
- Add current foreground ownership as observation only. Discovery never adopts a job or grants send, stop, review, retry, or landing authority.
- Return bounded sanitized structures. Do not expose helper paths, worktree paths, arbitrary filesystem paths, prompts, pane transcripts, environment, credentials, URLs, or desktop operations.
- Preserve helper diagnostics needed to understand malformed, stale, and dead records.
- Add timeout, output-size, JSON-shape, and helper-failure guards.
- No mutation and no compatibility alias.

## Definition of done

- Empty input lists durable live jobs across Scufris sessions.
- `include_finished` includes valid historical and malformed records.
- Exact job ID returns bounded detail and ownership.
- `include_report` composes only with exact job detail.
- Existing list and inspect outputs and behavior remain unchanged.
- Unowned results cannot be used by ownership-restricted tools.
- Malformed helper output and process failure fail closed with concise errors.
- Resource packaging includes the helper used by the extension.
- Focused tests cover schema, invocation, sanitization, ownership, unowned records, malformed records, failures, and unchanged existing tools.

## Verification

- `npm run check`.
- Focused Python tests where needed.
- `nix flake check`.
- `git diff --check`.
