# Expose durable agent diagnostics tool

- STATUS: CLOSED
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

## Implementation evidence

- Registered `scufris_agent_diagnostics` without changing the existing list or inspect tool implementations.
- The tool invokes the packaged sibling `scripts/scufris-jobs` executable with fixed argument arrays and `shell: false`. It accepts no helper path, state root, worktree, URL, tmux target, command, or desktop input.
- Empty input uses `--json`; historical input uses `--all --json`; exact input uses `<job-id> --json`; report input adds only `--report`.
- Strict native and helper-result schemas reject unknown fields, invalid combinations, malformed UTF-8 or JSON, semantic scope mismatches, process failures, oversized output, and timeouts.
- Model-facing output reconstructs a whitelist. It omits helper, worktree, Git path, tmux identity, prompt, transcript, and environment fields. It bounds reports, events, diagnostics, job count, and text fields and redacts absolute paths, URLs, environment assignments, and credential patterns.
- Ownership is observed through the existing current-session map. Diagnostics never inserts into that map. Existing inspect, send, retry-review, and stop checks remain restricted to map-owned jobs.
- Resource packaging already copied `scripts/` and asserted that `scufris-jobs` is executable. The Nix resource check verifies the exact helper used by the extension.
- Focused TypeScript tests cover strict schema and invocation, actual packaged-helper empty-state behavior, historical and malformed records, exact report detail, ownership, report bounds, sanitization, option rejection, malformed output, helper failure, timeout, output size, no adoption, restricted controls, and unchanged list and inspect results.
- Existing Python integration tests continue to cover helper list, historical, malformed, stale, exact detail, report, bounds, symlinks, and invalid forms.
- Changed files: `extensions/scufris/agents.ts`, `extensions/scufris/diagnostics.ts`, `tests/agent_diagnostics.test.ts`, and this task record.
- Pre-sync checks:
  - `npm run check` - passed: typecheck, 21 TypeScript tests, and Prettier.
  - `python3 -m unittest discover -v -s tests -p 'test_*.py'` - 17 passed.
  - `nix develop -c ruff check scripts tests` and `nix develop -c ruff format --check scripts tests` - passed.
  - `nix flake check` - passed for x86_64-linux, including packaged resource checks. Configured incompatible systems were omitted.
  - `git diff --check` - passed.
- No durable protocol page needed. The accepted task contract defines this read-only native projection; the private helper protocol remains unchanged.
- Limitation: no live foreground model playtest. Native registration tests and the actual packaged-helper test cover the extension boundary without changing durable user state.
- `sprout sync agent-diagnostics-tool` merged landing revision `78a5702` into the feature without conflicts. Synchronized feature revision before this evidence update: `f46386a`.
- Post-sync checks:
  - `npm run check` - passed: typecheck, 21 TypeScript tests, and Prettier.
  - `python3 -m unittest discover -v -s tests -p 'test_*.py'` - 17 passed.
  - Ruff lint and format checks - passed.
  - `nix flake check` - passed for x86_64-linux, including packaged resource checks. Configured incompatible systems were omitted.
  - `git diff --check` - passed.
