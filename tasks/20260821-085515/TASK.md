# Implement the Scufris fake-worker job loop

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: agents,orchestration,python,tmux,pi

## Goal

Implement Milestone 1 from `tasks/20260818-222337/PLAN.md`: a tested non-blocking job loop with a private Python CLI and narrow Pi tools.

## Accepted design

- Use one extension-owned executable Python script: `scripts/scufris-job`.
- Use Python's standard library only.
- Exchange one JSON request on stdin and one JSON result on stdout for extension operations.
- Keep subcommands narrow: spawn, inspect, send, stop, poll, and orphan discovery. The fixed tmux command can call one internal launch subcommand with a generated job ID.
- Keep job listing and event mediation in the TypeScript extension from owned in-memory state.
- Accept generated job IDs and owned-job context only. Do not accept model-provided commands, executables, paths, URLs, or desktop operations.
- Resolve the current trusted Git repository in the extension. The CLI creates its worktree through sprout.
- Use `pi` and `claude` names resolved from `PATH`. Integration tests put deterministic fake executables first in a temporary `PATH`.
- Use a dedicated tmux server socket. Never create, kill, or test jobs on the user's default tmux server. Tests use a unique dedicated socket and target only their own session.
- Implement built-in model defaults and per-spawn model/thinking overrides. This slice tests adapters with fake executables; live provider smoke tests remain in their owning milestones.
- Keep the fixed one-second poll loop, Pi lifecycle, notifications, and custom messages in `extensions/scufris/index.ts`.
- Add the delegation skill with its first tested workflow. Do not add placeholders.
- Split the CLI only if concrete implementation complexity requires it.

## Scope

- Immutable launch record and prompt creation.
- Append-only status and bounded report handling.
- Exact tmux session/window lifecycle.
- Sprout worktree creation and rollback.
- Spawn, list, inspect, send, and stop native tools.
- Partial-line and malformed-status handling.
- Process-exit failure mediation.
- Session shutdown cleanup.
- Startup orphan reporting without adoption.
- Integration tests with temporary Git repositories, real tmux, real sprout, and fake harness executables.

## Non-goals

- Live Pi or Claude provider smoke tests.
- Plannotator review and landing automation.
- Dashboardd widget tools.
- Recovery, adoption, restart, or transcript parsing.

## Verification

- `npm run check`
- `python3 -m unittest discover -s tests -p 'test_*.py'`
- `ruff check tests` and an stdin-filename Ruff check for extensionless `scripts/scufris-job`
- `nix flake check`
- Pi extension load smoke test

## Completion record

- Initial delegated implementation worker exited after resetting the default tmux server during research. No implementation changes were produced. The design now requires a dedicated tmux server socket for all Scufris runtime and test operations.
- Root cause control: every tmux invocation selects the dedicated socket. Every tmux and sprout subprocess removes inherited `TMUX` and uses a Scufris-owned `TMUX_TMPDIR`. Integration tests record the default server PID and require it to remain unchanged.
- Added `scripts/scufris-job`: one standard-library Python CLI for job creation, sprout transactions, fixed harness launch, exact tmux steering and stop, bounded status polling, inspection, and orphan discovery.
- Added five native Pi agent tools and session lifecycle mediation in `extensions/scufris/index.ts`.
- Added `skills/delegation/SKILL.md` with the first model-facing delegation workflow.
- Added real sprout and dedicated-tmux integration tests with fake Pi and Claude executables. Tests cover immutable file modes, defaults, Claude permission activation, exact stop targeting, literal steering, partial lines, malformed UTF-8, CRLF, unknown states, oversized lines and files, orphan discovery, idempotent stop, request rejection, and default-server preservation.
- Added direct `@earendil-works/pi-ai` peer and development dependencies for Pi-provided TypeBox and `StringEnum` APIs.
- Updated the parent architecture and protocol after the tmux isolation defect. Steering is one line; larger follow-ups remain out of scope.
- Verification passed: `ruff format --check tests`; Ruff format check for `scripts/scufris-job` through stdin; `ruff check tests`; Ruff check for the extensionless CLI through stdin; `python3 -m unittest discover -s tests -p 'test_*.py'` (3 integration tests); `npm run check`; `nix flake check`; Pi 0.84.2 offline extension load and model listing; `git diff --check`.
- Limitation: this slice uses fake harness executables. Live Pi and Claude provider smoke tests remain Milestones 2 and 3. Pi lifecycle and tool registration are typechecked and load-tested but do not yet have an interactive foreground playtest.
- Tradeoff: the single CLI is larger than several scripts, but it keeps transaction rollback, validation, and tmux isolation in one private boundary. Split only after concrete maintenance pressure.
- Plannotator approved initial revision `401b9783ac0e08ef19e8ceb7938f07f17a83c439` against landing revision `f5657a419c7e047ec0974baa4bedf241c86d738b` with no requested changes.
