# Improve README speech and quickstart

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: docs

## Goal

Keep the existing description and staging Quickstart. Make only the Home
Manager example directly usable with the machine's existing shared
`ai-tools-api` deployment.

## Decisions

- Revert the expanded product and staging text from the first revision.
- Show every writable behavior option in one copy-pasteable Home Manager
  example for the current machine.
- Set `desktop.aiToolsApi.manage = false` and show the loopback base URL so the
  example consumes port 10300 without starting a competing inference service.
- Show package override options as comments because pinned defaults are correct
  and the agent launcher must remain module-rendered for the selected Pi and
  project roots.
- Name the generated read-only options below the example rather than pretending
  users can configure them.

## Verification

- `npx prettier --check README.md tasks/20260830-110434/TASK.md`
- The example was checked against the current Home Manager option defaults.
