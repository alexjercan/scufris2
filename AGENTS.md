# AGENTS.md

Repository guidance. Global `~/AGENTS.md` applies.

## Project

- `scufris2` is the project and package name. Scufris is the user-facing assistant.
- Pi is the foreground conversation harness. Dashboardd is an external widget service.
- Keep orchestration in one narrow extension. Prefer skills and small Bash or Python helpers for workflows.

## Agent workflow

- Tracker/epics: use the tatr skill and CLI for task records under `tasks/`; statuses are only `OPEN`, `IN_PROGRESS`, and `CLOSED`.
- Examples/retention: keep runnable examples with the owning skill or task.
- Domain docs: use `docs/` as an mdBook when needed; keep design evidence with its task.
- Research/network: inspect installed Pi docs and local source first; record external evidence in the owning task.
- Checks/records: run `npm run check` and `nix flake check`; keep decisions and verification evidence with the task.

## Rules

- Follow `CONVENTIONS.md` for repository structure and language rules.
- Put Pi-provided APIs in `peerDependencies`. Put other runtime libraries in `dependencies`.
- Use Python's standard library unless a concrete requirement justifies a package.
- Keep native tool schemas narrow and harness-neutral.
- Never expose unrestricted shell commands, filesystem paths, URLs, or desktop operations to the model.
- Start session resources from `session_start` or on demand. Stop them idempotently during `session_shutdown`.
