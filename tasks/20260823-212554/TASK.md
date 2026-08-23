# Rewrite Scufris documentation

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: docs

Replace the old docs with a concise user guide and complete developer
documentation for architecture, extensions, jobs, messaging, tmux, operation,
and maintenance.

## Decisions

- Structure the mdBook in three parts. A concise user guide (`overview.md`,
  `guide/installation.md`, `guide/using.md`), a developer guide with one page
  per requested topic (`dev/architecture.md`, `dev/extensions.md`,
  `dev/jobs.md`, `dev/messaging.md`, `dev/tmux.md`, `dev/operation.md`,
  `dev/maintenance.md`), and the generated reference at
  `reference/options.md`, which `nix/docs.nix` still creates at build time.
- Write every page from the current source, not from the removed pages. The
  old manual was stale in two load-bearing places: workers now share the
  default tmux server with socket selection forbidden (no
  `SCUFRIS_TMUX_SOCKET`), and `.scufris.toml` preferences use
  `keywords`/`guidance`, not `name`/`options`.
- Keep accurate prior prose where it still matched the code: flake interface,
  Home Manager composition, voice ownership, popup service policy, and the
  troubleshooting items, folded into the installation, operation, and
  extensions pages.
- Split responsibilities by topic instead of by module: `jobs.md` covers the
  durable helper lifecycle, `messaging.md` covers event delivery, the
  acknowledgment gate, response shaping, and the Quick Review bridge,
  `tmux.md` covers execution ownership and exact termination.
- Keep `README.md` unchanged at description plus Quickstart.

## Independent review fixes

All five findings were reinspected against the implementation and corrected:

- Worker environment: the docs claimed a full scrub. `worker_environment()`
  removes only `SCUFRIS_ROLE`, `SCUFRIS_SPEECH`, `SCUFRIS_CALM`, the Piper
  paths, `SCUFRIS_REPORT_CAPABILITY`, and the `PI_*` session variables;
  `SCUFRIS_PROJECT_ROOTS` and `SCUFRIS_VOICE_AVAILABLE` pass through. The
  architecture and operation pages now list the exact removed and inherited
  variables and the values the wrapper sets per execution.
- Launch capability: `prepare_execution` writes the raw one-use value into a
  read-only `.launch-capability` file so the pane command and creation
  recovery can present it; `launch` validates it against the stored hash,
  clears the hash, installs the report capability hash, and deletes the raw
  file. The jobs page now documents this lifecycle instead of claiming only
  hashes are stored.
- Event delivery: only waking events are persisted as `scufris-job-event`
  messages before acknowledgement. Quiet `working` events surface as a
  transient notification and are acknowledged directly with no replay
  guarantee. The messaging page now distinguishes the two paths.
- Extension inventory: the Nix launcher includes the dashboard extension and
  skill only when dashboard control is enabled. The extensions page now
  states the conditional composition instead of an unconditional list.
- Test isolation: lifecycle tests relocate the default tmux server per
  fixture with `TMUX_TMPDIR`; they do not use explicit sockets. The
  maintenance page wording was corrected.

## Verification

- `mdbook build` on a copy with a stubbed options page: clean build, exactly
  the 11 expected chapters, no auto-created files.
- `nix build .#docs`: passed. The generated `reference/options.html` contains
  the evaluated `programs.scufris` options.
- `npm run check`: TypeScript passed, 61 tests passed, Prettier passed
  (including the new docs pages and this file).
- After the review fixes: `mdbook build` on the stubbed copy passed with the
  same 11 chapters, and `npm run check` passed again.
