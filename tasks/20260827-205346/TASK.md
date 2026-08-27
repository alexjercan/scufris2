# Bring the documentation up to the inverted tree

- STATUS: OPEN
- PRIORITY: 65
- TAGS: documentation

## Source

Review round 1 of `20260827-081702`, range `185034a..a13cb38`. Findings
M7, m10, m11, m12 and m14. Full record: `tasks/20260827-081702/REVIEW.md`.

## M7. The two orientation chapters describe the pre-inversion tree

These are the first two pages a reader lands on.

- `overview.md:9` tells Linux users to select a separate voice-capable
  package. `flake.nix` exposes no such attribute and `nix/scufris.nix`
  builds one launcher on purpose.
- `overview.md:15` says five extensions and names `voice`. `package.json`
  lists six, `extensions/scufris/voice/` is gone, and `conversation` is
  missing. `dev/extensions.md:3` already says six, so the two chapters
  disagree with each other.
- `architecture.md:7` lists `voice/` and omits `shared/`, `response.ts`
  and `conversation.ts`. `architecture.md:41` names a speech module
  deleted in `8813aa4`.

## m10. The environment table claims to be complete and is not

`installation.md:207` says "Every value comes from the environment" and
then omits `SCUFRIS_DESKTOP_COMMAND_SOCKET`, which `config.rs:93` reads,
`config.rs:171` prints, `nix/checks/desktop.nix:129` diffs, and
`dev/desktop.md:616` documents.

## m11. Two places still say `voice.enable` changes the agent

`README.md:58` says it "lets the agent decide what is worth saying
aloud". `nix/checks/home.nix:39` says "Voice changes which resources the
agent is handed and nothing else", and the check below it proves the
launcher is unchanged. In `nix/home-manager.nix` the option does two
things: a platform assertion, and appending
`SCUFRIS_DESKTOP_SPEAK_COMMAND` to the desktop unit. `response.ts` shapes
the spoken paragraph with no gate at all.

## m12. Comments that point at things this range deleted

- `shared/spoken.ts:23` names the speech mode, contradicting its own file
  header eight lines above.
- `service/protocol.ts:8` sends the reader to `desktop/protocol.ts`.
- `shared/assistant-state.ts:6` defers to an increment that has landed.
- `scufris-desktop/src/config.rs:40` still calls the service socket the
  "Daemon control socket", and eight of its tests use `daemon.sock`
  paths.

## m14. The README doubled and took on documentation the mdBook carries

32 lines to 64. It now explains the debug lease and what each module
option means; `dev/service.md:126` carries the first almost verbatim.
AGENTS.md: keep the README to the description and Quickstart. Line 3
also still describes the product without the service.

## Proof

- `TMPDIR=/tmp npm run check` and `TMPDIR=/tmp nix flake check`.
- Read the rendered mdBook rather than trusting exit status. Every claim
  changed here should be checked against the tree it describes, not
  against another document.
