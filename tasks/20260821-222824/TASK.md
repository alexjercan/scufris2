# Prune stale Scufris job records

- STATUS: IN_PROGRESS
- PRIORITY: 100
- TAGS: agents, diagnostics, cleanup

## Goal

Provide a safe manual helper to preview and delete stale durable Scufris job metadata, including obsolete malformed records, without changing worker, tmux, Git, or worktree resources.

## Accepted design

- Add executable private helper `scripts/scufris-jobs-prune`.
- Keep `scripts/scufris-jobs` strictly read-only.
- No arguments previews eligible records using a 30-day default.
- `--apply` performs exactly the eligible deletions represented by the same policy.
- `--older-than-days N` accepts a bounded nonnegative integer. Zero makes all eligible dead records candidates.
- Valid records are eligible only when the exact recorded pane is not alive and record age meets the threshold.
- Malformed records use job-directory modification time and require both the threshold and a minimum one-hour grace period, including with zero days.
- Never repair, migrate, quarantine, or automatically prune at session start.
- Never kill tmux, remove a worktree or branch, attach, focus, capture panes, or change Git.
- Accept no state root, path, tmux target, URL, command, or arbitrary job name.
- Scan only exact 12-character job directories below the conventional bounded state root.
- Delete only non-symlink directories containing exactly known regular non-symlink job files: `job.json`, `prompt.md`, `report.md`, and `status`.
- Refuse unknown entries, invalid names, symlinks, live valid jobs, recent records, races, and boundary violations with explicit diagnostics.
- Preserve deterministic ordering and exit codes. Preview and apply summaries identify candidates and refusals without printing prompt or report contents.
- Private resource only. Do not install a PATH command or expose a native mutation tool.

## Definition of done

- `./scripts/scufris-jobs-prune` is write-free and previews the default 30-day policy.
- `--apply` deletes only preview-eligible metadata directories.
- `--older-than-days 0 --apply` deletes eligible dead records but protects live and recent malformed jobs.
- Valid dead, valid live, old malformed, recent malformed, invalid names, symlinks, unknown files, and TOCTOU changes have isolated integration coverage.
- Deletion cannot escape the jobs root and does not affect tmux, Git branches, or worktrees.
- Script help and task evidence explain retention and security behavior.
- Packaged resources include the helper without adding it to PATH.

## Verification

- Focused Python integration tests with isolated state and tmux.
- Ruff lint and format checks.
- `npm run check`.
- `nix flake check`.
- `git diff --check`.
