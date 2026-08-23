# Restructure Scufris extensions and tools

- STATUS: IN_PROGRESS
- PRIORITY: 100
- TAGS: architecture

## Scope

Replace the flat extension and helper layout with capability-owned modules.
Move executables called by extensions from `scripts/` to `tools/`. Keep human
and development helper commands in `scripts/`. Update package, Nix, tests,
skills, and documentation.

## Architecture decision

Use four loaded extensions:

- `workflow`: Scufris identity, project workflow resolution, delegated-agent
  lifecycle, optional reviews, and landing.
- `voice`: response shaping in every package plus optional speech playback in
  the voice-capable package.
- `calm`: one independent UI decluttering mode.
- `dashboard`: Dashboardd widget ownership and control.

Keep agent mechanics inside `workflow`, not as a separately loaded extension.
A workflow owns agent state, polling, review, and shutdown, so separate loaded
extensions would need an unnecessary service protocol and load-order contract.
Keep raw agent controls as narrow native tools inside the workflow extension.

Use `tools/jobs`, `tools/quick-review`, `tools/dashboard`, and `tools/voice` for
executables invoked by extensions. Keep `scufris-dev`, `scufris-jobs`, and
`scufris-artifacts-prune` in `scripts` because people invoke them directly.

Keep workflow and dashboard skills because they contain distributed
model-facing policy that is broader than one native tool schema. Rename them to
match their owning extensions. Do not use skills for deterministic mechanics.

## Implemented

- Replaced the flat package manifest with four loaded extension entry points.
- Combined identity and delegated-agent orchestration under `workflow`.
- Combined response shaping and optional speech under `voice`.
- Renamed widget ownership to `dashboard`; retained Calm independently.
- Moved the Dashboardd and Piper executables from `scripts/` to their owning
  `tools/` directories. Kept only direct human and development helpers in
  `scripts/`.
- Renamed model-facing skills and workflow documentation by capability.
- Removed nonstandard per-extension and per-skill flake outputs. The Pi package
  manifest and launcher now own composition.
- Renamed the Home Manager widget option to `dashboard` and made workflow core
  rather than optional delegation composition.
- Added Python and tmux to the normal launcher because the core workflow calls
  the Python job tool and owns tmux workers.

## Verification evidence

- `npm run check` passes 45 TypeScript tests.
- Python unittest discovery passes 19 tests.
- Ruff check and format checks pass.
- ShellCheck passes for the Bash development helper.
- Alejandra check and `git diff --check` pass.
- `nix flake check` passes all supported-system checks.
- Both normal and voice launchers load their packaged extensions with
  `--list-models`; normal resources omit speech code and the Piper tool.
