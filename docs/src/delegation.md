# Delegated work and review

Scufris can delegate one bounded outcome to an independent Pi or Claude worker in a Sprout worktree. The foreground session owns the job, mediates decisions and blockers, and keeps landing local.

Every job declares one review policy at spawn:

- `code`: implementation correctness and maintainability.
- `consumer`: documentation, setup, and user outcomes.
- `operations`: deployment, reliability, diagnostics, and rollback.
- `interface`: APIs, schemas, protocols, and caller contracts.
- `none`: a non-landable result.

A landable policy includes a concise accepted-outcome and audience brief. A `none` job finishes with `done` and cannot enter review-ready.

## Landable review sequence

1. The worker commits, synchronizes, checks, and reports `review-ready`.
2. Scufris verifies the exact clean base and feature revisions.
3. A fresh headless Pi reviewer inspects the exact patch with read-only tools. It has a separate session and does not receive the worker transcript, report, reasoning, or claims.
4. Fix-worthy BLOCKER, MAJOR, or MINOR findings return once to the same worker. The same reviewer session verifies correction commits. A third change request stops for Pair mediation.
5. Exact preflight approval opens the Plannotator since-base review.
6. Plannotator feedback invalidates preflight approval. The next worker revision starts a fresh preflight reviewer session.
7. Human Plannotator approval remains the only landing approval. Scufris lands only the exact approved revisions after the worker's required acknowledgment.

One reviewer run has an exact 1800-second execution deadline. When the exact reviewer child starts, the helper sends a private readiness signal that resets the enclosing process deadline to 1810 seconds. This gives a fixed 10-second margin to stop the child and return the fail-closed result. Review failure, timeout, malformed output, repository mutation, or revision drift fails closed. Scufris stops only reviewer and worker processes that the foreground session owns.
