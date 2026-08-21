# Allow user tmux windows and retry blocked reviews

- STATUS: IN_PROGRESS
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
