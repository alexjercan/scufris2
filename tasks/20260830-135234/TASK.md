# Reshape the manual as a visual, sequential book

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: docs, architecture, nix, surfaces

Rewrite the mdBook as a top-to-bottom book with simple language, diagrams, user
flows, complete configuration references, platform test paths, and a guide for
new surfaces and machine-specific widgets.

## Decisions

- Put the architecture before installation. A reader first learns the three
  ownership rules, then chooses a deployment shape.
- Use Mermaid for the main architecture, sequence, choice, lifecycle, and user
  flow diagrams. Vendor the renderer through `mdbook-mermaid` so the built book
  needs no network access. Keep small path maps and checklists as text where
  source readability is more useful than a rendered graph.
- Make the first seven chapters the required reading path: stack, installation,
  configuration, use, surfaces, widgets, and testing. Keep deep runtime pages
  after that path.
- Add explicit Previous/Next links. Later pages point back to the ownership or
  configuration chapter they depend on.
- Keep the evaluated Nix option page as the source of truth. Add a guided option
  tree and task table before it rather than copying generated type details.
- Put active, internal, staging, test-only, ambient, and retired environment
  names on one reference page so a search has one complete destination.
- Document both surface transports from protocol v4 source. New clients must
  implement replay-before-ready, submission acknowledgement, bounds, stable
  identity, and no automatic retry after an uncertain submission.

## Verification

- `mdbook-mermaid install docs` with the pinned Nixpkgs package: installed the
  preprocessor configuration and vendored offline renderer.
- `mdbook build docs` with `mdbook-mermaid` on `PATH`: passed. The generated
  overview contains Mermaid markup.
- `nix build "path:$PWD#docs" --no-link -L`: passed. This evaluated the current
  Home Manager module, generated the full option reference, ran Mermaid, and
  built the book with the vendored JavaScript.
- The generated options include
  `programs.scufris.service.remoteSurface.tailscaleServiceName`.
- Local Markdown link scan: every relative target exists.
- `git diff --check`: passed.
