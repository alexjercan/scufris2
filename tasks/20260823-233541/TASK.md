# Research the Jarvis vision: Scufris over the-den with widget answers

- STATUS: IN_PROGRESS
- PRIORITY: 80
- TAGS: research

## Objective

Turn the settled vision in [NOTES.md](NOTES.md) into a concrete, staged
direction: I control the machine with my voice, and the machine talks back
with what it knows. Scufris answers from local data in the-den through
domain CLI contracts, shows references and interactive views as Dashboardd
widgets, and can eventually start the conversation itself.

The vision, the rejected directions, the first vertical slice (Scufris x
today as native Pi tools), and the library direction (capture before
retrieval; Scufris fetches, a deterministic CLI persists blobs and
manifests) are decided. Research fills in the how: the native-tool
surface, the library's first increment, reference presentation per
content type, multimodal ingestion, and the design of proactive contact.
The HUD companion task `20260822-132001` is the interaction layer and
proceeds in parallel; treat its daemon control channel as the front door
this work feeds.

## Deliverables

- `INVENTORY.md` - verified current behavior of scufris2, dashboardd,
  today, the-den, and nix.dotfiles, with file paths, plus the HUD task
  interface.
- `RESEARCH.md` - reusable existing tools and patterns judged against
  real workflows: web capture and archival, multimodal ingestion (video
  transcripts and keyframes), local lexical and embedding indexes,
  widget webview options, `.ics` calendar tooling.
- `ARCHITECTURE.md` - at least two credible options each for the
  Scufris x today native-tool integration, the library (manifest format,
  storage layout, CLI surface, capture flow), and the reference widget
  path, with recommendations covering ownership, interfaces, trust,
  failure, and NixOS packaging.
- `MARKET.md` - market survey of similar projects and products, from
  GitHub and other sources: what exists, what to reuse, what failed and
  why. Raw sweep reports live under `research/`.
- `UX.md` - end-to-end conversational and widget flows, including HUD
  states, for the scenarios listed in NOTES.md.
- `designs/` - self-contained HTML concepts where visual exploration
  helps.
- `ROADMAP.md` - staged work from the first slice onward; each stage
  states value, dependencies, risks, and a cheap verification.
- `IDEAS.md` - organized future ideas within the constraints.
- A final recommendation answering the seven questions in NOTES.md.

## Completion criteria

- The outputs answer the final recommendation questions in NOTES.md.
- The Scufris x today slice has a concrete, verifiable implementation
  plan as the roadmap's first stage.
- Rejected directions stay out: no database canon, no custom calendar or
  workout CLIs, no den restructuring, no widget window manager, no
  Scufris-specific hooks inside dashboardd.
- Observation through CLIs stays separate from widget presentation.
- The library write path is deterministic and local-only: Scufris
  fetches, the CLI persists blobs and writes manifests; tracked
  manifests, ignored blobs, backup stated.
- The course builder appears as a UX flow and designs/ concept, not as
  proposed implementation work.
- Proactive contact has a design with trigger sources, attention path,
  quiet rules, and audit, explicitly deferred to a later stage.
- Every proposed component fits the NixOS lifecycle: prototype, test,
  release, pin in nix.dotfiles, rebuild.
- Recommendations reuse existing tools where they fit and state what
  stays unbuilt.
