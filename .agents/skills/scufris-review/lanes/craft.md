# Lane: craft

Judge whether the change is the simplest correct shape for this
repository, and whether it obeys the house rules.

Read `AGENTS.md` first. This brief only says where to look.

## Look for

- Extension complexity where a product skill or a small deterministic
  helper would do. Orchestration stays narrow: Pi lifecycle, native
  tools, session state, and polling belong in `agent/extensions/scufris/`;
  model-facing workflows in `agent/skills/`; process and filesystem work in
  small Bash or Python scripts.
- TypeScript that loosens: a widened type, an `any`, an assertion
  where a narrow type would hold, a check `strict` and
  `noUncheckedIndexedAccess` would have forced.
- Python beyond the standard library without a concrete need, or a
  public or non-obvious interface with no type hints.
- Bash: an unquoted expansion, a command built as a string instead of
  an array, an exit code swallowed by a pipeline or a subshell.
- A runtime library in `dependencies` that is a Pi API, or the
  reverse. Pi APIs belong in `peerDependencies`.
- The same logic in two places. A helper that abstracts exactly one
  caller. An option that exists to avoid changing a caller.
- Comments that narrate the code or its history instead of stating
  ownership and constraints. A comment an edit left stale.
- A file added with no tested behavior, or an empty placeholder.
- `README.md` growing past the description and Quickstart. Durable
  documentation belongs in the mdBook under `docs/`.
- Prose with non-ASCII punctuation or emojis, and a commit trailer or
  AI attribution. Authorship is preserved exactly.
