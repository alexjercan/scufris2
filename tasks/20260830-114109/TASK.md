# Refine Home Manager architecture options

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: home-manager, api

## Goal

Make the Home Manager interface match product ownership: a top-level agent,
optional top-level API management, desktop API consumption, configurable
speech and transcription requests, clear key names, and grouped widget data
sources.

## Decisions

- Keep immutable v0.6.0 unchanged. Land this redesign under `Unreleased`.
- Move `service.agent` to `agent`; the foreground `scufris` launcher and the
  background service consume the same configured agent package.
- Move API process ownership to top-level `aiToolsApi.enable`; keep the desktop
  base URL with the desktop consumer.
- Remove the transcription endpoint override. Always derive the route from the
  desktop API base URL and expose model and language instead.
- Expose speech model and voice while keeping WAV fixed because playback
  requires validated WAV.
- Rename desktop keys to `popupKey`, `backgroundKey`, and `abortKey`.
- Rename `chatCommand` to `terminalCommand`; move journal and food settings
  under `desktop.widgets`.
- Provide one-release renamed-option aliases where the old value maps exactly.
  Do not retain the removed transcription endpoint override.

## Verification

- `npm run check` passed all 67 Node tests and formatting.
- Python discovery passed all 92 tests; Ruff check and format passed.
- `cargo test --workspace` passed 351 tests. Clippy passed with warnings denied.
- Focused speech helper tests proved custom model and voice request fields and
  malformed setting rejection.
- Focused desktop tests proved custom transcription model and language fields,
  bounded settings, and the renamed printed configuration.
- Home Manager checks proved the top-level agent and API ownership, derived
  routes, custom speech and transcription values, renamed keys, widget group,
  external provider reuse, and API management without a desktop.
- The documentation build passed. `nix flake show --all-systems` evaluated all
  Linux and Darwin outputs. Final `nix flake check` passed all compatible
  checks.
