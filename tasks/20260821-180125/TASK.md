# Allow user tmux windows and retry blocked reviews

- STATUS: REVIEW_READY
- PRIORITY: 100
- TAGS: agents, tmux, review

## Goal

Let Scufris review and land an exact owned worker even when the reusable worktree tmux session also contains user-owned windows, and provide an explicit retry after a transient lifecycle precondition blocks review.

## Evidence

The `readme-gauntlet` live playtest reached `review-ready` with exact worker window `@31` and pane `%31`. The user had manually opened a separate `fish` window `@32` in the same visible worktree session. `validated_review_snapshot` required the session window list to equal only the owned worker window, so automatic Plannotator review failed with `worker tmux session contains an unowned window`.

The extra window was valid under the accepted reusable-session design. Scufris left it untouched. Review and landing completed manually. The lifecycle remained blocked because no explicit review retry transition exists.

## Accepted direction

- Validate the recorded worker session, window, pane, process liveness, worktree, Git revisions, cleanliness, and ancestry.
- Ignore unrelated windows in the same session. Never inspect their content, target them, or remove them.
- Remove the session-singleton guard from review and landing.
- Add one narrow, explicit retry for a lifecycle blocked by review preconditions.
- Retry must rerun all current preconditions and must not reuse approval or a stale review snapshot.
- Keep exact owned-window stop and shutdown behavior unchanged.

## Definition of done

- A user-created window in the worker session does not prevent review or landing.
- Scufris targets and stops only the recorded worker window and pane.
- A failed review precondition can be retried after mediation without a new implementation commit.
- Retry is rejected unless the job is owned and lifecycle state is retryable.
- Approval remains bound to a fresh exact review snapshot.
- Integration tests reproduce the live extra-window case and retry transition.
- Architecture, protocol, and task evidence match implementation.

## Implementation record

- Review and landing validate the exact recorded pane, its session and window IDs, and worker liveness. They do not list other windows.
- Guarded landing keeps the Sprout dry-run, then performs Git-only squash and cleanup. This avoids Sprout's whole-session cleanup. Stop still targets only the recorded worker window.
- Added `scufris_agent_retry_review`. Only an owned lifecycle blocked by an initial review-snapshot precondition can use it. The transition rejects active or non-retryable states and any approval, consumes stale request state, and takes a fresh snapshot before opening a new `since-base` request.
- Added Python integration coverage with a live extra tmux window through snapshot, landing, and worker stop. The unrelated window remains alive. Added TypeScript lifecycle coverage for accepted and rejected retry states and stale request cleanup.
- Updated architecture, protocol, and delegation guidance.

## Verification

- `sprout sync shared-session-review`: passed, already up to date.
- Verified landing revision: `3a7e4f71c2d2eee2fefc549569205cc3dde1bb9f`.
- Verified implementation revision before this evidence-only update: `e34489dc5e43ade1d5cc28bcdbd4c69ffba21f76`.
- `npm run check`: passed, including typecheck, 9 TypeScript tests, and formatting.
- `python3 -m unittest tests/test_scufris_job.py`: passed, 6 integration tests with real Git, Sprout, and isolated normal-server tmux.
- `nix flake check`: passed all 9 x86_64-linux checks. Incompatible configured systems were omitted.
- `git diff --check`: passed.
- Ruff and ShellCheck were unavailable in the initial environment. Python byte-compilation passed. Nix package checks provide the repository lint and packaging path.
