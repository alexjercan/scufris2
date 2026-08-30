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
- Set `desktop.aiToolsApi.manage = false` in the main Home Manager example.
- Rely on the default loopback base URL so the example consumes port 10300
  without starting a competing inference service.

## Verification

- `npx prettier --check README.md tasks/20260830-110434/TASK.md`
- The example was checked against the current Home Manager option defaults.
