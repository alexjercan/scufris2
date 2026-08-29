# AGENTS.md

Global `~/AGENTS.md` applies. This file defines project-specific instructions.

## Project

- `scufris2` is the package. Scufris is the user-facing Pi assistant.
- Pi owns the foreground conversation. The desktop companion owns windows.
- Keep orchestration narrow. Prefer product skills and small deterministic
  helpers over extension complexity.

## Workflow

- Work directly on `master` unless the user requests an isolated worktree.
- Use Tatr for requested tracked work. Keep one task for one request and its
  follow-up work.
- Keep decisions and verification evidence under `tasks/<id>/`.
- Use Sprout only when the user requests an isolated worktree.
- Inspect installed Pi documentation and local source before network research.

## Conventions

- Keep Pi lifecycle, native tools, session state, and polling in
  `agent/extensions/scufris/`.
- Keep distributed model-facing workflows in `agent/skills/`. Keep development
  skills in `.agents/skills/`.
- Keep deterministic process and filesystem work in small Bash or Python
  scripts.
- Add files with their first tested behavior. Do not add empty placeholders.
- Use strict TypeScript and Prettier.
- Put Pi APIs in `peerDependencies` and other runtime libraries in
  `dependencies`.
- Use Python 3 and the standard library unless a concrete need justifies a
  package. Use type hints for public and non-obvious interfaces.
- Quote Bash expansions, use command arrays, and preserve exit codes.
- Keep `README.md` to the description and Quickstart. Put durable documentation
  in the mdBook under `docs/`.
- Prefer focused integration tests and small end-to-end examples.
- Run the cheapest relevant check. Use `npm run check` for TypeScript behavior,
  `python3 -m unittest discover -s tests -p 'test_*.py'` for the Python
  helpers, and `nix flake check` for broad package integration.
