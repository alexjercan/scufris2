# Market: what exists, what to reuse, what failed and why

Synthesis of two sweeps: GitHub projects (`research/market-github.md`,
16 projects with live activity data) and products, postmortems, and
practitioner writing (`research/market-web.md`). Citations live in the
raw files.

## The competitive picture

Nobody ships the combination Scufris targets: local voice, a plain-file
personal vault, deterministic domain CLIs, and assistant-driven native
widget windows. The closest analogs each miss a piece:

- Leon: the closest architectural analog (skill and tool split) but no
  owned data model.
- Khoj: wanted the same outcome, drifted from file-centric to
  server-owns-truth with a Postgres dependency.
- OpenVoiceOS: solid local voice, no knowledge layer.
- Open Interpreter: started general, diluted into one more coding
  agent; its 01 hardware device was discontinued.
- Obsidian Smart Connections and org-ai: thrive precisely by staying
  thin layers over a host they do not own - structurally the same bet
  as Scufris being a Pi package.

The den is the moat. Mass-market voice assistants plateaued at timers
and music because they never owned a personal knowledge base.

## Reusable now

1. ArchiveBox's per-item directory and manifest schema - the strongest
   direct precedent for the library manifest format.
2. The MCP filesystem server's contract shape - allowlist roots,
   dry-run mode, introspection - a template for library CLI safety.
3. Karakeep's triad: CLI, SDK, and MCP server over one store - the
   contract-first pattern the library CLI should follow (CLI first;
   the rest only if needed).
4. The Wyoming protocol lesson: Rhasspy the app is archived, but its
   narrow protocol won by absorption into Home Assistant. Narrow
   protocols outlive applications - applies to the today JSON
   contract, the library manifest format, and the HUD control channel.
5. Onyx/Danswer's connector-per-source concept - the right mental
   model for future ingest sources, without adopting its stack.
6. For the course pillar: Nicky Case's explorable-explanation pattern
   (hook, ladder of abstraction, sandbox) and Execute Program's
   dependency-ordered, auto-graded spaced repetition.

## Mistakes to avoid, ranked

1. Canonical data in someone else's store. Omnivore, a well-loved app,
   died in 15 days after an acquihire because user data lived in its
   Postgres. Test every design with: does it still work after the
   acquihire? The den passes today; keep it that way.
2. DB-as-canonical regression. Homarr's YAML-to-database rewrite hurt
   retention; Khoj's index-owns-truth drift did the same. The derived
   index must stay deletable.
3. Generality dilution. Open Interpreter tried to be everything and
   became indistinct. Scufris stays a narrow orchestrator over owned
   contracts.
4. Unmanaged local resource cost. Rewind's always-on capture ate disks
   and CPUs, then the product pivoted away. Ingestion and indexing
   need budgets (GPU minutes, disk, watch scope) from day one.
5. Demo-ware. Rabbit and Humane destroyed trust by shipping promises.
   The NixOS release gate - small verified slices - is the antidote
   and should be kept even when it feels slow.
6. Hardware before the software is proven. The 01 and the Pin both
   died there. Relevant to the "give Scufris a speaker and a body"
   ambition: collect the research in the library now, build much
   later.
7. Good storage without a contract dies. Reor had the right local
   model but no CLI or API, and is archived. The library CLI is not
   optional plumbing; it is the survival trait.
8. Company and license fragility. Mycroft was killed by a patent
   troll; the boring, community-run fork (OVOS) survived. Prefer
   boring, maintained, standard components.

## Durable lessons for the design

- Grep-first retrieval over personal corpora is the practitioner
  consensus; embeddings are a measured upgrade, not a default. Matches
  the staged retrieval plan.
- Curation beats hoarding. Multiple "I deleted my second brain"
  postmortems agree: capture without triage becomes a write-only
  archive. The library needs a lightweight triage moment (inbox to
  kept), not just an ingest pipe.
- Proactivity: offer, do not act. Ambient-first escalation, graduated
  controls (not one kill switch), and the assistant must be able to
  explain why it spoke. Notification fatigue is the default failure.
- Local voice is competitive only with tuning; keep the exposed tool
  surface small (~30-entity ceiling degrades tool selection) and keep
  latency budgets explicit.
- Ship the boring version first. Every surviving project in the sweep
  is the boring fork, the thin layer, or the narrow protocol.
