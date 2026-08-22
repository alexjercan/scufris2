# Make Scufris a prose-only delegated orchestrator

- STATUS: OPEN
- PRIORITY: 100
- TAGS: orchestration, speech, artifacts, plannotator

## Goal

Make foreground Scufris a minimal spoken conversational router. Display and speak one short prose paragraph per response, store all normal Markdown detail in private session artifacts, and delegate project work instead of performing it in the foreground session.

## Foreground policy

- Answer ordinary conversation, clarification, and product decisions directly.
- Keep every visible assistant response to one short natural prose paragraph that passes the existing Piper safety contract.
- Delegate filesystem inspection, local or external research, implementation, checks, task maintenance, diagnostics, releases, deployment, and other project work to an independent worker.
- Do not inspect a project before delegation. Give the worker the user request and require it to inspect applicable instructions, context, code, history, and checks.
- Keep built-in tools and skills loaded. Enforce the foreground work boundary through the canonical Scufris system prompt rather than removing capabilities.
- Keep native delegation tools available for spawn, mediation, review, landing, diagnostics, and exact cleanup.
- Keep native widget tools available as foreground presentation controls.
- Preserve Pair decision depth. Put evidence and detailed comparisons in artifacts while the spoken paragraph states the decision or question.

## Structured response contract

- Add one narrow Scufris final-response tool with separate `spoken` and optional `detail` fields.
- Require foreground Scufris to use it for every final response.
- Validate `spoken` with the existing complete plain-prose and Piper byte-safety rules.
- Store the complete `detail` value as Markdown. Never render detailed Markdown as assistant chat text.
- Terminate the model turn after the final-response tool succeeds so no extra assistant response follows.
- Render only the spoken paragraph and, when detail exists, one separate `/detail <artifact_id>` command line.
- Piper speaks only the spoken paragraph. It never speaks the detail command or artifact content automatically.
- When no detail exists, render only the spoken paragraph.
- Keep the detail command visually compact and copyable.

## Fail-safe response handling

- Add a fallback for direct assistant text when the model does not call the structured response tool.
- Hide direct assistant streaming content until it is finalized and classified.
- If the direct response starts with a valid spoken paragraph, display that paragraph and store all remaining content as a Markdown artifact.
- If it has no valid spoken paragraph, store the complete response as an artifact and display one safe fallback sentence plus its detail command.
- Prevent malformed output from producing raw Markdown, paths, URLs, code, tool calls, or reasoning in the visible transcript or Piper input.
- Keep the detailed artifact out of the final visible assistant message while preserving required model and session continuity.

## Session artifacts

- Store artifacts in a private sidecar owned by the current Pi session and adjacent to its durable session storage.
- Use mode 0700 directories and mode 0600 regular files. Reject symlinks, replacements, malformed files, and ownership mismatches.
- Address artifacts through generated opaque session-owned IDs. Never accept arbitrary filesystem paths from the model or `/detail` command.
- Keep artifacts for the lifetime of the owning session. Provide bounded pruning for artifact sidecars whose session no longer exists.
- Bound artifact count and Markdown size. Fail closed without losing the spoken response when artifact persistence fails; report the failure concisely.
- Record enough private metadata to associate Plannotator feedback with the exact artifact without placing raw paths in model-facing output.

## Detail review

- Add `/detail <artifact_id>` as an extension command. It resolves only an artifact owned by the active session and opens no model turn by itself.
- Use Plannotator's public supported interface to open the Markdown artifact directly.
- Verify the installed Plannotator arbitrary-Markdown contract before selecting the exact integration. Do not import private internals or automate a browser.
- If the public interface cannot support this accepted flow, stop for a `needs-decision` report instead of adding an unrelated viewer or unsafe subprocess contract.
- Feed actionable Plannotator feedback back to foreground Scufris as structured context.
- Render Plannotator results as one compact custom row such as approval, closure, or annotation count. Never render raw annotation JSON or Markdown, including after session resume or when Calm is off.
- Store full structured review feedback with the owning artifact.
- Actionable feedback triggers a foreground mediation turn. Approval or closure without feedback does not require a model turn.

## System prompt inspection

- Add a read-only `scufris-prompt` helper and focused tests for prompt composition.
- Show the exact assembled prompt and ordered provenance for the Pi base prompt, context files, active tool descriptions and guidelines, loaded skills, canonical Scufris orchestration policy, speech policy, and any final-response policy.
- Do not make a provider request to inspect the prompt.
- Treat prompt snapshots as private because they can contain local instructions and paths.
- Keep the canonical Scufris policy small, explicit, ASCII, and independently testable.

## Integration

- Replace the current foreground Pair instruction that requires direct project inspection with the accepted delegation-only work policy.
- Preserve worker behavior: delegated workers remain bounded executors with their existing tools, project inspection, reports, visible tmux sessions, preflight review, Plannotator gate, and landing protocol.
- Do not apply the prose-only response tool or foreground work restriction to implementation workers or independent preflight reviewers.
- Keep the existing popup, Calm mode, speech modes, session resume, and future voice HUD compatible with the new visible-response and artifact contract.
- Update the manual, architecture, protocol, delegation skill, prompt documentation, and both relevant task records.

## Verification

- Test valid spoken-only, spoken-plus-detail, maximum-size, invalid prose, malformed direct output, artifact write failure, and fallback behavior.
- Test that detailed Markdown never renders or reaches Piper and that restored sessions remain minimal.
- Test final-response termination and prevention of an extra assistant message.
- Test private artifact creation, opaque lookup, ownership, symlink rejection, bounds, resume, and orphan pruning.
- Test `/detail` success and refusal paths using the public Plannotator contract.
- Test compact Plannotator approval, closure, feedback, annotation, resume, and Calm-off rendering.
- Test that actionable detail feedback enters model context without raw transcript rendering.
- Test exact prompt rendering and provenance without a provider call.
- Test foreground prompt policy, worker exclusion, and retention of loaded tools and skills.
- Run repository TypeScript and Python integration checks, formatting, ShellCheck, Nix formatting, diff checks, documentation build, and full Nix checks.
- Complete live Piper, session resume, detail review, annotation, and transcript-minimality playtesting.

## Completion criteria

- Every foreground Scufris answer is a speakable prose paragraph with at most one compact detail command.
- Full Markdown detail is durable, private, session-owned, and reviewable through Plannotator.
- Plannotator feedback remains useful to Scufris without creating transcript walls.
- Foreground Scufris delegates project work and remains focused on conversation, decisions, and orchestration.
- Users can inspect the exact effective system prompt and its provenance without contacting a model provider.
