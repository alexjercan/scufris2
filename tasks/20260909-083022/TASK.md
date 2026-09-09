# Render briefing pages as safe Markdown

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: briefing, markdown, security, ui, bug

## Request

Render all generated and archived Scufris briefing content through one safe Markdown pipeline. Support CommonMark prose plus GitHub-style tables, preserve briefing routing and records, add responsive Gruber-styled tables, and test realistic morning and non-morning runs plus unsafe HTML and links.

## Constraints

- Work only in the `briefing-markdown` Sprout based on current `master`.
- Treat contribution and synthesized Markdown as untrusted.
- Do not land, push, release, deploy, switch Home Manager, or restart services.
- Inspect a rendered fixture at desktop and narrow widths.

## Decisions

- The defect was not a CSS failure. `tools/briefing/page.py` used a hand-written
  regex and line parser whose contract explicitly returned tables as pipe-text
  paragraphs. Collection, publish, manual render, archive render, and every
  profile already call the same `render_page`, so the shared renderer is the
  only implementation boundary.
- Replace the Markdown subset with markdown-it-py's CommonMark parser and its
  GFM table rule. Use that one configured parser for synthesized prose,
  contribution bodies, headlines, and offers. Keep the existing two-level
  heading offset so briefing Markdown stays below the page and card headings.
- Disable raw HTML. Parse link destinations, but render an anchor only for an
  HTTP or HTTPS URL with a host, no credentials, no control characters, and a
  bounded length. Unsafe links retain their labels. Image syntax retains its
  alt text and never fetches a resource.
- Wrap each semantic table in its own labelled scroll region. Keep the Gruber
  palette and add bordered cells, contrasting headers, source-specified column
  alignment, wrapping, dark scrollbars, and a mobile cell width. A 32 rem table
  floor makes wide data scroll locally on narrow screens while ordinary tables
  fit and wrap at desktop width.
- Package markdown-it-py in the briefing runtime, helper-test environment, and
  development shell. Do not add another rendering path or change run records,
  source facts, offers, diagnostics, profile/date routing, or prose storage.

## Verification

- Focused briefing library and CLI tests: 115 passed.
- `npm run check` in the standard Nix development environment: 105 Node tests,
  strict TypeScript, versions, and Prettier passed. `PI_PACKAGE_DIR` was unset
  because it is a worker-harness variable, not part of the project check.
- Full Python suite: 372 passed. The Nix helper gate repeated all 372, with its
  three expected host-dependent skips.
- Ruff check and format: 248 files passed. Alejandra checked 26 Nix files.
  `git diff --check` passed.
- Cargo format and Clippy with warnings denied passed. Workspace tests passed:
  19 control, 330 desktop, 48 service, and 9 surface-gateway tests.
- `nix flake check -L`: all 48 compatible-system checks passed. The packaged
  `/nix/store/66s3yig4q7zilywk17762ia6ijjjh2lj-scufris-briefing` starts and
  prints its command help with the packaged Markdown dependency.
- Chromium rendered the realistic morning fixture at 1280x1800 and 360x1800.
  `briefing-desktop.png` shows both prose and contribution tables fitting and
  wrapping with visible borders and aligned cells. `briefing-mobile.png` shows
  the page staying at viewport width while each wide table gets its own dark
  horizontal scroller. Both screenshots were inspected from the generated
  standalone page; the page itself still has no scripts or fetched resources.
- The Sprout began at master `936c7cec50fa2dbcc82019065aeeea10c5fd30d6`.
  Concurrent owner work advanced master while checks ran. The staged change was
  saved as a binary full-index patch, the Sprout was fast-forwarded with
  `sprout sync` to `6bbccd7ff6766f2c6aec62df05fd7632f776d9d7`, and the patch
  reapplied cleanly before the final focused, Node, Python, and Nix checks. The
  main checkout was never modified by this task.
