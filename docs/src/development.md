# Development and checks

Enter the reproducible shell and install locked JavaScript dependencies:

```bash
nix develop
npm ci
```

The shell includes Node.js, Python, Ruff, ShellCheck, Alejandra, mdBook, Git, and tmux. On Linux it also includes the private Piper runtime, PipeWire, and trusted model paths.

Repository ownership follows the runtime architecture:

- `extensions/scufris/{workflow,voice,dashboard}` contain capability modules;
  `calm.ts` remains a small independent extension.
- `tools/` contains deterministic executables called by extensions.
- `scripts/` contains commands called directly by people or development tasks.
- `skills/` contains only broader model-facing workflow and dashboard policy.

## Run working-tree Scufris

```bash
npm run dev
npm run dev:voice
```

Both commands use system Pi, working-tree resources, and dedicated resumable development sessions. `dev:voice` must run in the repository development shell. It enables speech and Calm but inherits STT configuration unchanged.

## Repository checks

Run the complete local contract:

```bash
npm run check
python3 -m unittest discover -s tests -p 'test_*.py'
ruff check .
ruff format --check .
shellcheck scripts/scufris-dev
nix fmt -- --check .
nix flake check
git diff --check
```

`npm run check` runs strict TypeScript checking, TypeScript tests, and Prettier. `nix flake check` builds the package, Home Manager, closure, real Piper fixture, and documentation checks supported by the current system.

Build only the manual with the same output used by CI:

```bash
nix build .#docs
```

Open `result/index.html`. The build evaluates the Home Manager module and creates the Scufris-only option reference before mdBook runs. Generated Markdown and HTML are not tracked.
