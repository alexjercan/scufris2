# Reviewer contract

Every lane obeys this. Read it with your lane brief.

## Rules

- You review. You never edit, stage, commit, or fix. Findings only.
- Work from the bundle paths you were given. Do not re-derive the
  range.
- Read the tree when the diff is not enough. Read a whole file rather
  than guess from a hunk.
- Ground every finding: `file:line`, and a concrete failure scenario -
  the inputs or state, then the wrong result. Drop what you cannot
  ground. A plausible smell is not a finding.
- Read `AGENTS.md` at the repository root. It is the house authority.
- Run Cargo through the Nix development shell:
  `nix develop --command cargo ...`. The desktop crate's suite is
  cheap; `cargo test -p scufris-desktop` is allowed and encouraged.
- Run the TypeScript suite as `TMPDIR=/tmp npm test`. A nested
  nix-shell TMPDIR exceeds the 108-byte Unix socket path cap and fakes
  about 48 failures; without the override the result is a lie.
- Never run `nix flake check`. It is the slow integration gate and CI
  owns it.
- Never edit `ui/orb-engine.js` and never run Prettier on it. It is
  vendored byte-identical and `.prettierignore` protects it.
- Do not bring up an X display (Xvfb, i3, a harness) unless your brief
  grants the display slot.
- Stop a helper process by its recorded PID. Never match processes by
  name. Never kill a tmux server.
- The documents under `tasks/` are append-only records. Read them as
  evidence; never flag or fix their history.
- Say what you did not check. A skip is not a pass.

## Bounded probes

A probe must not be able to cost more than the finding is worth.

- Never point product code at `/dev/zero`, `/dev/random`, a device node, a
  FIFO, a socket, or a symlink to any of them.
- Never create an unbounded stream, file, or pipe, and never create a file
  that grows without a ceiling.
- Fixtures are bounded regular files. Keep them under 1 MiB.
- To show an unbounded read or allocation, use `os.lstat`/`os.stat` metadata
  and the code itself. Do not execute the unbounded read to prove it.
- Prefer static reasoning wherever it reaches the same conclusion.

This is written from an incident. A red-team lane made a config file a symlink
to `/dev/zero` and called the reader, which had no bound. The process reached
29 GB, systemd stopped the whole control group, and the night lost its
collector, every sibling source, and four completed lane reports. The defect
was real and the lane was right to look for it. Reading `os.lstat` and the
reader's own source would have found it for nothing.

## Severity

- `BLOCKER`: a defect that ships, a broken lockstep pair, a lost
  keyboard or focus path, or a build path that fails.
- `MAJOR`: wrong behavior at an edge, a stale contract or document
  claim, a verdict that can lie.
- `MINOR`: worth folding in; does not block.

## Report

Return findings only, strongest first. For each:

- `<SEVERITY> - <file:line> - <one-line claim>`
- The failure scenario.
- The actionable change.
- Why it is not higher, when the severity is arguable.

Close with `Checked:` and `Not checked:`. Return nothing else: your
text is the review, not a message to a person.
