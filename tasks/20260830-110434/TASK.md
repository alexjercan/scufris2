# Improve README speech and quickstart

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: docs

## Goal

Make the top-level description and Quickstart accurately explain conversation
ownership, desktop responsibilities, shared inference, staging modes, and Home
Manager provider selection.

## Decisions

- Keep the README limited to the product description and Quickstart.
- Show external staging as the default and managed staging as an explicit
  alternative.
- Show multi-surface staging because it is a core protocol-v4 capability.
- Explain upstream provider reuse, pinned fallback management, and explicit
  external Home Manager configuration without duplicating the option manual.

## Verification

- `npx prettier --check README.md tasks/20260830-110434/TASK.md`
- `nix build .#checks.x86_64-linux.docs --no-link`
- Link targets and all documented commands were checked against the current
  staging script and Home Manager module.
