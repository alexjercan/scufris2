# Render final-response details as safe Markdown on desktop and iOS

- STATUS: CLOSED
- PRIORITY: 80
- TAGS: desktop, ios, markdown, security

## Goal

Render the optional `details` field of a final response as readable Markdown in
the desktop and iPhone conversations. Preserve required `text` as short literal
plain prose, canonical replay, selection, wrapping, scrolling, and follow-latest
behavior.

## Decisions

- `text` is never Markdown. The two surfaces preserve every Markdown delimiter
  literally and add links only around safe bare HTTP or HTTPS URLs.
- `details` supports paragraphs, emphasis, strong text, inline and fenced code,
  headings, ordered and unordered lists, block quotes, thematic rules, Markdown
  links, and bare URL autolinks.
- Content is untrusted. Neither surface uses HTML injection or a web view for
  Markdown. Raw HTML stays inert text. Only credential-free HTTP and HTTPS URLs
  with a host are actionable. Desktop URL opening repeats validation in Rust;
  iOS uses native attributed links after sanitizing parsed and detected URLs.
- Desktop uses a small DOM renderer that creates semantic elements and text
  nodes. iOS uses native `AttributedString` inline Markdown and link detection,
  with a small native block model for hierarchy. This avoids a cross-platform
  framework while retaining platform-native selection, links, and accessibility.
- Existing Gruber colors supply all hierarchy, code, quote, rule, and link
  styling. No new theme is introduced.

## Implementation

- Strengthened the final-response policy and tool descriptions so agents keep
  Markdown out of mandatory `text` and put formatted explanation in `details`.
- Added a dependency-free desktop renderer that builds semantic DOM only from
  text nodes and a fixed element set. A new Tauri command repeats URL validation
  before it starts packaged `xdg-open` without a shell.
- Added native iPhone block presentation around Foundation inline Markdown.
  Markdown links and detected bare URLs are sanitized before SwiftUI can open
  them. Headings expose header traits, code remains selectable and horizontally
  scrollable, and details stay behind the existing disclosure control.
- Kept protocol v5 and `ConversationMessage` unchanged. Added local iPhone
  validation that matches the existing canonical 32 KiB details bound. Extended
  protocol and durable-replay tests with representative Markdown.
- Documented the rendering and trust boundary. Captured the focused desktop
  visual review in `desktop-markdown.png`.

## Verification

- Focused response and desktop rendering tests: 7 passed.
- Focused canonical details boundary test: 1 passed.
- Desktop native link tests: 2 passed.
- `npm run check`: passed, including TypeScript, desktop UI tests, versions, and
  Prettier.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace --no-fail-fast`: passed, including 321 desktop, 36
  service, and 9 surface-gateway tests.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 281 passed.
- Ruff and repository shell checks: passed.
- `nix fmt -- --check .`: passed.
- `nix flake check -L`: passed, including desktop and service packages and
  closure/configuration checks. The desktop closure contains `xdg-open`.
- `git diff --check`: passed.
- iOS source and focused Swift tests were reviewed statically. This Linux host
  has no `xcodegen`, `xcodebuild`, or `swift`, so the native simulator build and
  tests remain for the existing macOS iOS workflow.

## Corrective recovery verification

The original job left this complete implementation only in the removed
worktree's index. Its cleanup record therefore named the unchanged `7dca61e`
base revision. On 2026-09-06, the corrective Sprout started at that same clean
master revision and recovered the intended staged file set from loose Git blobs.
Object timestamps, content, and nearest-path comparisons identified all 26
paths, including this task and the 760x560 visual evidence. Master was not
modified.

The corrective run repeated the required checks:

- Five focused Node tests passed for literal `text`, safe autolinks, semantic
  Markdown, malformed details, and final-response guidance.
- The focused canonical details-bound test, five durable replay tests, and two
  native desktop URL tests passed.
- `npm run check` passed with 103 tests. Strict TypeScript, version alignment,
  and Prettier passed.
- All 281 Python tests passed. Ruff check and format passed for 228 files.
- Cargo Clippy passed with warnings denied. Workspace tests passed: 17 control,
  321 desktop, 36 service, and 9 surface-gateway tests.
- ShellCheck passed for the three Bash launchers. `nix fmt -- --check .` passed
  for 23 Nix files. `git diff --check` passed.
- The first flake check correctly excluded the recovered new files while they
  were still untracked, so native formatting could not resolve `external.rs`.
  After all intended paths were staged for the Git-backed flake source,
  `nix flake check -L` passed all compatible-system checks and builds.
- The Linux host still has no `xcodegen`, `xcodebuild`, or `swift`. Therefore,
  the unsigned iOS simulator build and Swift tests could not run here and remain
  covered by the existing macOS iOS workflow.
