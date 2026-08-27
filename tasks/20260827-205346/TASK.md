# Bring the documentation up to the inverted tree

- STATUS: CLOSED
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

## Outcome (2026-08-27)

Landed as two commits: the five findings, then a vocabulary sweep the
third of them turned out to be one instance of.

### M7, m10, m11, m12, m14

All five as written. Beyond them:

- `architecture.md` also described a resources derivation with a normal
  and a voice variant. There is one, and it removes the development
  launcher and `tools/voice`. It also said the companion is absent from
  "the default and voice launcher closures"; there is one launcher.
- The `orchestrator` role no longer activates a speech module.
- m11's second half was the more interesting one. `nix/checks/home.nix`
  said "Voice changes which resources the agent is handed and nothing
  else", and it changes nothing about the agent at all: it asserts the
  platform and appends `SCUFRIS_DESKTOP_SPEAK_COMMAND` to the desktop
  unit. The check below the comment is still worth having - it proves
  the voice-enabled launcher carries no synthesiser - so the check
  stands and the reason is rewritten.
- The chat hook opened "the full popup chat" in three comments. The
  popup is gone; it opens a terminal, usually around
  `scufris-ctl debug`.

### What reading the rendered book found

The proof said to read the render rather than trust exit status, and it
paid:

- `installation.md` pinned `v0.3.0` in two places, one release behind.
- Its package list omitted `scufris-service` and `scufris-ctl`, which
  are the halves this range is about, and called the companion the
  "voice pill and tray companion".
- "The workflow, response, Calm, and desktop extensions are always
  present" named the deleted `desktop` extension and left out three.

### The sweep

m12's fourth bullet was `config.rs` calling the service socket the
"Daemon control socket". Fixing it showed the word fifty-odd more times
across the companion. The daemon was version 2's popup Pi process, so a
reader looking for it finds nothing.

Replaced by what each site actually means: mostly the service, the agent
where the comment is about naming a widget or typing a tool from the
catalog, and Scufris where the contrast is with the person acting on
their own desktop. `scufris-control` keeps the word where it describes
version 2 as history. Two log lines and one test name changed with it.

### Proof

- `nix flake check`: all checks passed.
- `nix build .#docs`: builds, and the three changed chapters were read
  as rendered text rather than as source.
- `npm run check`: typecheck, 79 tests, format, clean.
- `cargo test --workspace`: 336 passed. `cargo clippy`: clean.
