# Extract Quick Review into a standalone Pi extension

- STATUS: OPEN
- PRIORITY: 60
- TAGS: review, extension, extraction

## Objective

Turn Quick Review into its own repository: a shippable Pi extension any
session can use, with Scufris as one consumer. The current in-repo
pipeline (`extensions/scufris/workflow/walkthrough.ts`,
`walkthrough-reviewer.ts`, `tools/quick-review/`, the
`scufris_job_quick_review` tool) is the proof of concept - same effect,
different shape. Do not port it as-is; redesign against the concept
below and keep only what earned its place.

## Concept

- A `/quick-review` command in a plain Pi session. The session's own
  agent does the work - no spawned generator, no sub-agent: it builds
  the walkthrough itself and opens the local review page.
- Customizable range, not hardcoded: base and target are parameters
  (`/quick-review [--base <ref>] [--target <ref>]`) with sensible
  defaults (merge-base with the default branch; HEAD). No Sprout or
  workspace assumptions; any git repo works.
- The review page keeps the PoC's interaction set: one section per
  change with diff and review prompt; mark viewed and reopen; explain;
  ask a free question; exact-revision context; change requests.
  Questions flow back to the same session's agent.
- Keep the PoC's proven safety properties: exact-revision validation
  of the walkthrough artifact, revision recheck around every action,
  bounded artifact size, loopback page with a random path token,
  invalidation after a change request.

## Scufris integration (after extraction)

- Quick Review gets its own `.scufris.toml` entry. Review becomes a
  pluggable slot: quick-review, Plannotator, and the independent
  reviewer are selectable agents, not built-ins.
- Scufris runs review as a separate review agent - a Pi session with
  this extension - so the foreground never hosts the bridge or the
  state machine again. The foreground keeps only job bookkeeping and
  the completion follow-up.
- Remove the in-repo pipeline from scufris2 once parity is verified.

## Stages

1. Contract and concept doc in the new repository: inputs (repo path,
   base ref, target ref), outputs (versioned walkthrough artifact,
   review page, completion event), compatibility policy.
2. Extension implementation: the `/quick-review` command, in-session
   generation, page server, bridge.
3. Nix packaging, checks, and a release.
4. scufris2 consumes it: `.scufris.toml` entry, review-agent spawn,
   in-repo pipeline removed.
5. nix.dotfiles pin and wiring.

## Completion criteria

- `/quick-review` works in a plain Pi session on an arbitrary git
  repository with chosen base and target refs.
- Page interaction parity with the PoC, including invalidation on
  change requests, verified in live use.
- The artifact and completion-event contract is versioned from day
  one.
- The scufris2 foreground no longer contains the bridge, the state
  machine, or the walkthrough tools; review selection happens through
  `.scufris.toml`.
- Repository checks and Nix checks pass in both repos; released and
  pinned through the normal gate.
