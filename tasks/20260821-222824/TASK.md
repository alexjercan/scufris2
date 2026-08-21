# Prune stale Scufris job records

- STATUS: CLOSED
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

## Completion record

- Added private executable `scripts/scufris-jobs-prune`. Preview is the default. `--apply` is the only deletion mode. Retention accepts only `0..36500` whole days and defaults to 30.
- Valid records require the retention boundary and an exact missing or dead recorded pane. Apply repeats exact pane validation and filesystem snapshot validation before deletion. Identity mismatch and live panes are refused.
- Malformed records use job-directory mtime. They require both the selected retention period and a one-hour minimum grace, including with zero days.
- Scans are bounded and sorted. Only exact 12-character, same-filesystem, non-symlink directories with exactly the four known regular non-symlink single-link files can become candidates. Diagnostics escape invalid names and never read prompt or report content.
- Deletion uses no-follow descriptors and directory-relative operations rooted at the opened jobs directory. Root, directory, file, record, and liveness changes fail closed. The helper does not invoke Git or mutating tmux operations.
- Added isolated integration tests for default write-free preview, deterministic output, valid dead and live jobs, exact identity mismatch, missing panes, old and recent malformed jobs, zero-day and age/grace boundaries, invalid names, missing and unknown files, directory and file symlinks, jobs-root escape attempts, TOCTOU changes, bounded arguments, retained tmux windows, and unchanged Git branches and worktrees.
- Nix resources include the executable helper under `share/scufris/scripts`. Checks assert that neither the resource package nor launcher exposes a `bin/scufris-jobs-prune` PATH command.
- Verification passed before synchronization: `npm run check` (12 TypeScript tests); `python3 -m unittest -v tests/test_scufris_jobs_prune.py` (5 focused tests); `python3 -m unittest discover -s tests -p 'test_*.py'` (22 Python tests); Ruff lint and format checks for the helper and focused tests; `nix flake check`; `git diff --check`.
- Nix verification ran on x86_64-linux. Flake checking omitted incompatible configured systems. No live deletion against user state was performed; isolated state, tmux, and Git fixtures covered mutation boundaries.
- Implementation revision: `976d60f430a8696b3c28beca4059c5dbd97eff15`.
- `sprout sync job-state-pruning` merged landing revision `78a5702f498ee76c3e498ed0b2074d1f3df48e49`; synchronized feature revision before this evidence update: `fa740c243caed8e4eafe749afdd46a725de40629`.
- Post-sync verification passed: `npm run check` (12 TypeScript tests); 5 focused and 22 total Python tests; Ruff lint and format checks; `nix flake check`; `git diff --check`.
