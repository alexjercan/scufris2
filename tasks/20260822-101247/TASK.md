# Add the Scufris mdBook manual

- STATUS: CLOSED
- PRIORITY: 90
- TAGS: docs, nix, mdbook

## Goal

Add a durable mdBook manual for Scufris with the conventional Nix flake, Home Manager, local build, check, and GitHub Pages integration.

## Accepted design

- Keep `README.md` within repository conventions: title, one concise product sentence, and short Quickstart commands. Move durable ownership and development explanation into the book.
- Use `docs/book.toml` with source pages under `docs/src/`.
- Start with accurate user-facing pages for overview and quickstart, flake outputs and consumption, Home Manager configuration, voice and external ownership, development and verification, release workflow, and troubleshooting.
- Explain both direct `nix run` use and use as a tagged flake input.
- Document normal versus voice-capable outputs and the opt-in popup without implying that Scufris owns STT, Whisper, or i3 policy.
- Generate the `programs.scufris` option reference from the evaluated Home Manager module during the Nix documentation build. Do not hand-maintain option defaults or descriptions in prose.
- Add mdBook to the development shell.
- Expose a `docs` flake package that builds the complete book reproducibly, including generated option reference.
- Add the documentation build to `nix flake check`.
- Add a focused GitHub Pages workflow that builds `.#docs` on relevant pull requests and master changes, uploads only on non-pull-request runs, and deploys through the official Pages actions with least permissions.
- Keep generated HTML out of Git. The Nix output is the build artifact.
- Use plain mdBook unless a concrete page requires a preprocessor. Do not add Mermaid speculatively.

## Documentation scope

- Product overview and boundaries.
- Quickstart and prerequisites.
- Flake apps, packages, module output, and tagged-input examples.
- Home Manager examples for normal, voice, and popup configurations.
- Voice architecture, trusted runtime, and external STT and desktop responsibilities.
- Development commands and complete check contract.
- Release automation and manual release checklist relationship.
- Common configuration and runtime failures.
- Generated Home Manager option reference.

## Non-goals

- Product behavior changes.
- New Home Manager options.
- Native RPC frontend.
- Voice UX changes.
- General Pi documentation.
- Generated API documentation for internal TypeScript or Python helpers.
- Marketing site or custom theme.

## Verification

- Build the documentation flake package.
- Confirm the generated option reference includes the public Scufris options and excludes unrelated Home Manager options.
- Run the documentation check and full `nix flake check`.
- Run repository formatting and diff whitespace checks.
- Validate the Pages workflow syntax, event paths, permissions, artifact source, and pull-request no-deploy behavior.
- Record implementation and verification evidence here.

## Completion criteria

- A new user can run Scufris or consume its Home Manager module from the book without inspecting source.
- Voice ownership and opt-in behavior are unambiguous.
- Option documentation cannot drift from module declarations.
- Local and CI documentation builds use the same Nix output.
- Master documentation changes publish the book through GitHub Pages.

## Implementation

- Kept `README.md` to the project title, one product sentence, Quickstart commands, and the primary copy-paste Home Manager configuration. Durable ownership, development, release, and failure guidance now lives in `docs/`.
- Added a plain mdBook with overview, quickstart, flake interface, Home Manager, voice ownership, development and checks, release, troubleshooting, and generated option-reference chapters.
- Added `nix/docs.nix`. It evaluates `homeModules.default` through Home Manager, selects only the `programs.scufris` option subtree, generates CommonMark with `nixosOptionsDoc`, and inserts it into a writable source copy before mdBook runs.
- The docs derivation asserts that every generated option starts with `programs.scufris` and that core normal and popup options exist. The generated Markdown and HTML remain outside Git.
- Exposed `packages.docs`, reused the identical derivation as `checks.docs`, added mdBook to the development shell, and ignored local `docs/book/` output.
- Added a focused Pages workflow for documentation-relevant pull requests, `master` pushes, and manual runs. Every event builds `.#docs`; artifact preparation, official Pages upload, and deployment are guarded outside pull requests.
- Linked the release chapter to the root release checklist owned by release automation. It does not duplicate release mechanics.

## Design tradeoffs

- Used `homeManagerConfiguration` instead of a partial module evaluation. This generates documentation from the real Home Manager composition and its evaluated package defaults.
- Filtered the option tree before documentation generation instead of filtering rendered Markdown. This makes unrelated Home Manager options structurally unavailable to the generator.
- Kept the package source at `docs/` and copied it inside the sandbox. The build can create the SUMMARY-referenced option page without a generated source file in Git.
- Used mdBook and the Nixpkgs option generator only. No preprocessor, Mermaid, custom theme, or additional runtime dependency was added.
- Kept Pages permissions at job scope. The build job receives only contents read and Pages write for `configure-pages`; only the deploy job receives an OIDC token.
- Consumer documentation describes integration actions and responsible components positively. Detailed lists of absent or out-of-scope features stay in task and architecture records, where they prevent scope drift without confusing users.

## Verification evidence

Before synchronization:

- `nix build .#docs -L` - passed. The result contains the complete mdBook and generated option HTML.
- Generated reference inspection - passed: 19 unique `programs.scufris` option names, including `programs.scufris.enable` and `programs.scufris.voice.popup.enable`; no `home.username`, `programs.git`, or `systemd.user` options.
- `npm ci && npm run check` - passed: 33 TypeScript tests, strict typecheck, and Prettier including Markdown and workflow YAML.
- Python tests - passed: 22 tests. Ruff lint and format, ShellCheck, Nix formatting, and diff whitespace checks passed.
- Full `nix flake check -L` - passed for `x86_64-linux`, including `checks.docs`. Configured incompatible systems were omitted.
- Workflow contract inspection - passed: relevant event paths, `master` branch filter, job-scoped permissions, `result` copied to `_site`, official Pages upload/deploy actions, and four non-pull-request guards including the deploy job.
- The first docs build found that current mdBook rejects the former `multilingual` key. Removing the obsolete key fixed the build without changing output scope.
- The first JavaScript check found missing worktree dependencies. `npm ci` installed the lockfile dependencies, after which the exact check passed.
- A Ruby YAML parser attempt was unavailable. Prettier parsed the workflow successfully, and a focused Python contract check validated its required GitHub semantics.

After synchronization:

- Initial implementation commit: `6a236d2` on `mdbook-manual`.
- `sprout sync mdbook-manual` - passed. It merged release automation from `master` at `b8f4748` without conflicts. The manual release chapter now links to the synchronized final `RELEASE.md` checklist.
- Rebuilt `.#docs` and repeated generated-scope assertions - passed with the same 19 Scufris-only option names.
- Repeated `npm run check` - passed with 33 tests, typecheck, and formatting. Repeated 22 Python tests, Ruff, ShellCheck, Nix formatting, and diff whitespace checks - passed.
- Repeated workflow and release-link contract validation - passed. Pull requests build only; `_site` preparation, Pages configuration, official artifact upload, and deploy remain non-pull-request operations.
- Repeated full `nix flake check -L` - passed for `x86_64-linux`, including the shared docs package/check output. Incompatible configured systems were omitted.
- Final review revision: `mdbook-manual` HEAD containing this evidence update. Worktree clean.

Review feedback:

- Restored the primary Home Manager configuration in `README.md` as a concise copy-paste Quickstart.
- Replaced consumer-facing "does not own" lists with positive integration guidance. Optional speech-to-text input points to Pi configuration; popup startup and presentation point to desktop configuration.
- Mitigation for future documentation: document available interfaces, required actions, and responsible components in the manual. Keep exhaustive exclusions only in design records when they guard implementation scope.
- Review-fix commit: `8335ece`. `sprout sync mdbook-manual` - passed; already up to date with `master`.
- `npm run check` - passed after synchronization: 33 tests, typecheck, and formatting.
- `nix build .#docs -L` and generated-reference scope checks - passed with the same 19 Scufris-only options.
- `nix fmt -- --check .`, consumer-documentation negation search, and `git diff --check` - passed.
- Full `nix flake check -L` - passed for `x86_64-linux`; incompatible configured systems were omitted.
