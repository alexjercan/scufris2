# AGENTS.md

Global `~/AGENTS.md` applies.

## Project

- `scufris2` is the project and package name. Scufris is the user-facing
  assistant.
- Pi is the foreground conversation harness. Dashboardd is an external widget
  service.
- Keep orchestration in one narrow extension. Prefer skills and small Bash or
  Python helpers for workflows.

## Agent workflow

- Work directly on `master` unless the user requests an isolated worktree.
- Use tatr for tracked work. Create a task only when the user requests one.
- Use one task for one user request and its follow-up work. Create dependent
  tasks only when the user requests decomposition.
- Store task records under `tasks/`. Keep decisions and verification evidence
  with the task.
- Keep runnable examples with the owning skill or task.
- Use `docs/` as an mdBook when durable documentation is needed.
- Inspect installed Pi documentation and local source before network research.

## Conventions

- Keep Pi lifecycle events, native tools, in-memory job state, and polling in
  `extensions/scufris/`.
- Keep model-facing workflows in `skills/`.
- Keep deterministic process and filesystem operations in small, owning Bash or
  Python scripts.
- Add files with their first tested behavior. Do not add empty placeholders.
- Never expose unrestricted shell commands, filesystem paths, URLs, or desktop
  operations to the model.
- Start session resources from `session_start` or on demand. Stop them
  idempotently during `session_shutdown`.

### TypeScript

- Use strict TypeScript and Prettier.
- Keep the Pi extension narrow. Move deterministic mechanics to scripts.
- Put Pi-provided APIs in `peerDependencies`. Put other runtime libraries in
  `dependencies`.
- Keep native tool schemas narrow and harness-neutral.

### Python

- Use Python 3 and the standard library unless a concrete requirement justifies
  a package.
- Use type hints for public functions and non-obvious data structures.
- Use `snake_case` for modules, functions, and variables. Use `PascalCase` for
  classes.
- Check Python with Ruff.

### Bash

- Use Bash for small process adapters and command composition.
- Quote expansions and use arrays for commands.
- Preserve command exit codes.
- Stop helper processes by recorded PID. Never use pattern-based process
  killing.
- Check scripts with ShellCheck.

### Documentation

- Keep `README.md` to the project description and Quickstart.
- Put durable documentation in `docs/` when the first durable page is needed.
- Keep design evidence with its task until it becomes durable documentation.

## Verification

- Prefer integration tests and small end-to-end examples when practical.
- Run the relevant checks:

```bash
npm run check
nix flake check
```

- Open rendered or generated output when the change affects it.
