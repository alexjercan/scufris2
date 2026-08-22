# Add the Scufris mdBook manual

- STATUS: OPEN
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
