# Conventions

## Structure

- Keep Pi lifecycle events, native tools, in-memory job state, and polling in `extensions/scufris/`.
- Keep model-facing workflows in `skills/`.
- Keep deterministic process and filesystem operations in small skill-owned or extension-owned Bash or Python scripts.
- Keep design evidence, decisions, and verification records in `tasks/`.
- Add files with their first tested behavior. Do not add empty placeholders.

## TypeScript

- Use strict TypeScript and Prettier.
- Keep the Pi extension narrow. Move deterministic mechanics to scripts.
- Put Pi-provided APIs in `peerDependencies`.
- Put other Node runtime libraries in `dependencies`.
- Keep native tool schemas narrow and harness-neutral.

## Python

- Use Python 3 and the standard library unless a concrete requirement justifies a package.
- Use type hints for public functions and non-obvious data structures.
- Use `snake_case` for modules, functions, and variables. Use `PascalCase` for classes.
- Check Python with Ruff.

## Bash

- Use Bash for small process adapters and command composition.
- Quote expansions and use arrays for commands.
- Preserve command exit codes.
- Record helper process IDs and stop those exact processes. Do not use pattern-based process killing.
- Check scripts with ShellCheck.

## Tests and checks

- Prefer integration tests and small end-to-end examples when practical.
- Run `npm run check` and `nix flake check` before irreversible repository operations.
- Re-read edited files after checks.

## Documentation

- Keep `README.md` limited to the project title and Quickstart commands.
- Put durable documentation in `docs/` as an mdBook when the first durable page is needed.
- Add the mdBook scaffold and dependency with that first page, not before it.
- Keep design evidence and work records with the owning task until they become durable documentation.
