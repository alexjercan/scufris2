# Run preflight reviewers in interactive Pi TUI

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: agents, review, tmux, interface

Run the owned preflight reviewer in Pi interactive TUI mode. Keep the reviewer input-capable by user acceptance while preserving independent automated ownership, structured result transport, exact revision checks, correction reuse, cancellation, cleanup, and guarded landing. Add integration coverage and update current policy and durable lifecycle documentation.

## Decision

- Run Pi directly on the owned tmux pane terminal in regular interactive mode. Do not proxy or replay a print-mode process.
- Pass the bounded review prompt as an `@file` argument. This avoids operating-system argument limits for the 512 KiB patch while Pi displays the complete initial user prompt.
- Load one explicit private reviewer extension while extension discovery remains disabled. Its terminating `submit_preflight` tool writes one bounded exclusive result file and requests graceful Pi shutdown.
- Keep the existing strict controller parser, separate result channel, paired ownership evidence, exact child readiness, deadlines, correction session reuse, and lifecycle transitions.
- Require pane input to remain enabled in ownership diagnostics and stale pruning. User input is technically possible by accepted policy. Scufris remains the only automated review owner and result consumer. Foreground tools expose no reviewer steering operation.

## Implementation

- Replaced `pi -p` and piped standard I/O with Pi's regular interactive TUI on the pane PTY.
- Added the private structured-result extension and packaged-resource assertion.
- Changed reviewer identity validation and diagnostics from input-disabled to input-capable.
- Preserved the same reviewer window and Pi session for correction verification. Human invalidation, cancellation, shutdown, retain, remove, and guarded landing paths are unchanged.
- Updated the foreground delegation policy, user manual, architecture, protocol, prior task context, and durable implementation notes.

## Regression evidence

- Real tmux integration now verifies an input-capable pane, TTY-backed reviewer standard input and output, absence of `-p`, regular TUI mode, visible prompt, visible tool activity, visible final JSON, correction reuse, exact removal, and unrelated-window preservation.
- Existing integration cases still cover malformed and oversized results, nonzero exit, mutation, revision drift, exact deadline, cancellation, child cleanup, retain, remove, and guarded landing.
- A TypeScript test verifies exclusive bounded result writing, visible result content, terminating tool behavior, and graceful shutdown request.

## Pre-synchronization verification

- Focused Python reviewer lifecycle integration - pass, 3 tests.
- Focused TypeScript reviewer, diagnostics, and lifecycle tests - pass, 26 tests.
- `python3 -m unittest discover -s tests -p 'test_*.py'` - pass, 29 tests.
- `npm run check` - pass, including 52 TypeScript tests.
- `ruff check .` and `ruff format --check .` - pass.
- `git diff --check` - pass.
- `nix flake check -L` - pass, 25 checks on x86_64-linux after staging the new extension so the Git-backed flake source included it.

## Investigation notes

- Pi's default CLI mode is the input-capable interactive TUI. `-p` is explicitly non-interactive and cannot provide the requested live tool UI.
- Pi extensions can request graceful shutdown after `agent_settled`. A terminating result tool gives the controller structured output without parsing terminal rendering.
- The first Nix check attempt could not find the new packaged extension because untracked files are absent from a Git-backed flake source. Staging the file fixed the check input; no product change was required.

## Post-synchronization verification

- `sprout sync interactive-preflight-review` - pass after both commits; already up to date with landing revision `44d0b54b51b7605e6d4b0d007470678789ff6da1`.
- `npm run check` - pass, including 52 TypeScript tests.
- `python3 -m unittest discover -s tests -p 'test_*.py'` - pass, 29 Python integration tests.
- `ruff check .`, `ruff format --check .`, and `git diff --check` - pass.
- `nix flake check -L` - pass on x86_64-linux. Nix reported the existing deprecated platform accessors, unknown custom outputs, and incompatible-system omissions.

## Independent preflight correction

The initial `submit_preflight` schema accepted verdict and finding combinations that the controller rejected. It also applied character limits without the controller's stricter UTF-8 byte, relative path, control character, text, and feedback-size rules. An invalid tool call could therefore create the exclusive result file and request shutdown before the reviewer could retry.

The corrected tool uses a discriminated approve or request-changes schema and validates the complete controller contract before file creation or shutdown:

- Exact result and finding fields.
- Verdict and finding consistency.
- Severity and integer range.
- Relative POSIX path shape, backslash refusal, and traversal refusal.
- Valid Unicode, control-character refusal, non-whitespace text, and UTF-8 byte limits.
- Python-compatible ASCII JSON feedback sizing and the 16 KiB feedback limit.
- Final 64 KiB result limit.

Regression coverage submits inconsistent verdicts, absolute and traversal paths, backslashes, controls, whitespace-only text, oversized UTF-8, invalid Unicode, and oversized feedback. Every invalid call leaves the result path absent and does not request shutdown. A valid retry then persists and terminates normally.

Correction verification:

- Focused reviewer result tests - pass, 2 tests.
- `sprout sync interactive-preflight-review` - pass after the correction commit; already up to date.
- `npm run check` - pass after synchronization, including 53 TypeScript tests.
- `python3 -m unittest discover -s tests -p 'test_*.py'` - pass after synchronization, 29 Python integration tests.
- Ruff, Python formatting, Prettier, and `git diff --check` - pass.
- `nix flake check -L` - pass after synchronization on x86_64-linux with the existing warnings and incompatible-system omissions.

## Revisions

- Starting and landing revision: `44d0b54b51b7605e6d4b0d007470678789ff6da1`.
- Initial implementation revision: `61e7ef9cfaa563123060b313cd49363075f84466`.
- Initial evidence revision: `3f2d811a40f0d9d599cc105696eb8df50d62a1ec`.
- Review correction revision: `4922267a50f27d6df10b6fb6f20c654afc7a59cc`.
- Final evidence: this task record's closing commit.

## Limitations

- Automated coverage uses a fake Pi executable inside real isolated tmux sessions. It verifies PTY mode, arguments, presentation output, ownership, and lifecycle mechanics. It does not call the external review model.
