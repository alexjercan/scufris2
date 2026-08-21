# Add descriptive delegated feature names

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: agents, orchestration, sprout

## Goal

Add an optional descriptive feature slug to delegated spawn while retaining opaque job IDs for ownership.

## Design

- Accept ASCII lowercase alphanumeric segments separated by single hyphens, up to 48 characters.
- Use the selected slug exactly for Sprout and its compatible tmux session name.
- Fall back to `scufris-<job_id>` only when omitted.
- Reject existing branches, worktrees, and matching tmux sessions instead of reusing them.
- Keep exact tmux IDs and `job_id` as ownership identities.

## Completion record

- Added the optional `feature` field to the native spawn schema and helper request. Both boundaries validate lowercase alphanumeric segments with single hyphen separators and a 48-character limit.
- Preserved `scufris-<job_id>` when the field is omitted. Spawn returns and records the exact selected feature.
- Added an atomic branch reservation before `sprout new`. This prevents Sprout from reusing an existing branch. Existing worktrees and compatible tmux sessions are also rejected.
- Replaced spawn rollback through `sprout rm` with exact Git worktree and branch cleanup. This prevents a late tmux name collision from making rollback remove an unowned session.
- Updated generated worker prompts to synchronize the selected feature.
- Kept ownership on the opaque job ID and recorded exact tmux IDs. Feature and session names remain display and Sprout identities.
- Updated delegation guidance, architecture, protocol, and parent task records.
- Added type tests for slug boundaries and integration coverage for fallback behavior, exact descriptive worktree and session naming, prompt synchronization, invalid values, active-worktree collisions, existing-branch collisions, and existing-session preservation.
- Synchronization passed: `sprout sync scufris-d40c350d6ec8` reported already up to date.
- Verification passed after synchronization: `npm run check`; 12 Python integration tests; Ruff lint and format checks for tests and the extensionless helper; `nix flake check`; `git diff --check`.
- Limitation: `nix flake check` checked x86_64-linux and omitted incompatible configured systems. Fake harnesses cover process integration; no live provider spawn was needed for this protocol change.
- Tradeoff: a stale matching tmux session blocks spawn. This is safer than placing a new job under a display name that can imply another feature owner.
