# Maintenance

## Development environment

Enter the reproducible shell and install the locked JavaScript dependencies:

```bash
nix develop
npm ci
```

The shell includes Node.js, Python, Ruff, ShellCheck, Alejandra, mdBook, Git,
and tmux. On Linux it also includes the private Piper runtime, PipeWire, and
trusted model paths, and it exports the voice development variables.

Run working-tree Scufris with system Pi and dedicated resumable sessions:

```bash
npm run dev
npm run dev:voice   # requires the repository development shell
```

`scufris-dev` strips the repository `node_modules/.bin` from `PATH` so the
system Pi runs, sets the orchestrator role and default project roots, and
passes the working-tree extensions and skills.

Preview the Quick Review page without a real job:

```bash
python3 tools/quick-review/preview.py
```

The preview serves the production page against a deterministic in-process
bridge fixture and validates every response with the production validator.

## Checks

Run the cheapest relevant check first: `npm run check` for TypeScript
behavior, the focused `unittest` module for a Python helper change, and
`nix flake check` only when packaging scope warrants it.

The complete local contract:

```bash
npm run check
python3 -m unittest discover -s tests -p 'test_*.py'
ruff check .
ruff format --check .
shellcheck scripts/scufris-dev
nix fmt -- --check .
nix flake check -L
git diff --check
```

`npm run check` runs strict TypeScript, the Node test suite, and Prettier.
`nix flake check` builds the launchers, resources, Home Manager
configurations, closure separation, the real Piper fixture, and this manual.

Test ownership:

- `tests/*.test.ts`: extension behavior in Node with the Pi APIs stubbed:
  orchestration, walkthrough parsing and state, response shaping, speech,
  Calm, identity, and repository structure.
- `tests/test_scufris_jobs.py`: the jobs helper and inspection CLI. Lifecycle
  tests create real tmux sessions on the default server, relocated per test
  fixture with `TMUX_TMPDIR` into an isolated server directory, and prove
  unrelated sessions survive.
- `tests/test_quick_review.py` and `tests/test_quick_review_preview.py`: the
  review page server, bridge validation, and preview harness.
- `tests/test_scufris_dashboard.py` and
  `tests/test_scufris_artifacts_prune.py`: the dashboard adapter and sidecar
  pruning.

## Documentation

This manual is an mdBook under `docs/`. Build it with the same output CI
uses:

```bash
nix build .#docs
```

The build evaluates the Home Manager module and generates
`reference/options.md` before mdBook runs; generated Markdown and HTML are
not tracked. Flake source comes from the Git worktree, so stage new files
before a Nix build:

```bash
git add docs
nix build .#docs
```

Durable documentation belongs here. `README.md` stays at the description and
Quickstart.

## Continuous integration

- `check.yml` runs `npm run check` and `nix flake check -L` on `master`
  pushes and pull requests, and is reused by the release workflow.
- `docs.yml` builds `packages.docs` for documentation-affecting changes and
  deploys `master` builds to GitHub Pages.
- `release.yml` runs on stable `v*` tags: it reuses the check job, verifies
  the tag matches the package version, and publishes a source-only GitHub
  Release.

## Release

Follow the repository
[release checklist](https://github.com/alexjercan/scufris2/blob/master/RELEASE.md)
for preparation, versioning, verification, tagging, and publication. Release
tags are immutable; consumers build from the tagged source flake, and no
binary assets are published.
