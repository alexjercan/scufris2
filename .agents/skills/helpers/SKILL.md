---
name: helpers
description: Change Scufris deterministic Bash or Python process and filesystem helpers and their focused tests.
---

# Helpers

Keep deterministic process and filesystem mechanics in small owning scripts.
Keep extension code focused on orchestration.

## Python

- Use Python 3 and the standard library unless a concrete need justifies a
  package.
- Use type hints for public functions and non-obvious data structures.
- Use `snake_case` for modules, functions, and variables and `PascalCase` for
  classes.
- Run the focused `unittest`, then Ruff checks relevant to the change.

## Bash

- Use Bash for small process adapters and command composition.
- Quote expansions, use arrays for commands, and preserve exit codes.
- Stop only owned helpers by recorded PID. Never use pattern-based process
  killing.
- Run the focused integration test and ShellCheck for changed scripts.

Prefer a small end-to-end test over isolated implementation tests when practical.
