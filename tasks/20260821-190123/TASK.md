# Add read-only Scufris job diagnostics

- STATUS: OPEN
- PRIORITY: 100
- TAGS: agents, diagnostics

## Goal

Add a standalone private helper that lets the user inspect live and historical Scufris jobs from durable state without changing the native Scufris tools.

## Accepted design

- Add executable `scripts/scufris-jobs`.
- Keep `scufris_agent_list` and `scufris_agent_inspect` unchanged. The extension does not call the new helper initially.
- The script is private: callable directly from the repository, available to packaged resources when appropriate, but not installed as a stable PATH command.
- No positional argument lists live jobs across all Scufris foreground sessions.
- `--all` includes historical jobs.
- One exact 12-character job ID shows detailed durable state for that job.
- `--report` includes bounded report content for one job only.
- `--json` emits machine-readable output suitable for later Dashboardd integration. Human-readable output is the default.
- Accept no repository path, tmux target, URL, command, or arbitrary state root from CLI input.
- Read only validated immutable job records, bounded valid status events, exact recorded tmux identity, and bounded Git metadata from the trusted record.
- Never mutate, adopt, steer, stop, review, land, attach, focus, or capture pane contents.

## List fields

- Job ID, project, feature, harness, model, worker state, latest summary, creation time, elapsed time, tmux session, exact pane liveness, and cleanup policy when recorded.

## Detailed fields

- Sanitized immutable metadata.
- Bounded valid status history and protocol errors.
- Report size and optional bounded report content.
- Worktree existence, branch, revision, clean or dirty state, and recorded landing revision.
- Exact tmux identity and liveness.
- Never include pane transcripts, prompts, environment, or credentials.

## Definition of done

- `./scripts/scufris-jobs` gives a concise human-readable list of live jobs.
- `./scripts/scufris-jobs --all` includes stopped and completed records.
- `./scripts/scufris-jobs <job-id>` gives detailed diagnostics.
- `--report` and `--json` compose correctly and reject invalid forms.
- Dead panes, malformed or oversized records, malformed status lines, missing worktrees, and stale Git metadata fail closed or render explicit diagnostics without mutation.
- Output and ordering are deterministic and bounded.
- Automated tests use isolated state, Git, and tmux resources and preserve helper exit codes.
- Script help and task evidence document the private, read-only contract.

## Verification

- Focused Python integration tests.
- Ruff lint and format checks.
- `npm run check`.
- `nix flake check`.
- `git diff --check`.
