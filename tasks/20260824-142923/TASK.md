# Honor configured harnesses for independent review jobs

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: orchestration

## Reproduction and root cause

A focused helper reproduction created a Pi implementation job and requested a
Claude Opus xhigh `review_of` child. Spawn failed with `independent review jobs
require the read-only Pi harness`. The generic adapter resolver and native tool
schema accept Pi and Claude, but `spawn` has a later Pi-only review guard. The
configuration renderer also treats all adapter keyword values as opaque, so it
can advertise a review adapter tuple that runtime rejects.

## Decisions

- Keep the exact source workspace path, device, and inode binding unchanged.
- Define review safety per harness as an enforced model-tool allowlist, not an
  operating-system read-only filesystem sandbox. The role prompt is only
  defense in depth and must not be described as enforcement.
- Pi review jobs retain only `read`, `grep`, `find`, `ls`, and the authenticated
  report tool.
- Claude review jobs run non-interactively with only Read, Glob, and Grep. They
  load no user/project/local settings, request-supplied MCP servers, slash
  commands, shell, edit, write, web, or subagent tools. The trusted wrapper
  captures the final Markdown response into the job report. Normal Claude
  implementation jobs keep their existing adapter behavior.
- Treat Claude administrator-managed hooks and plugin hooks as part of the
  trusted host boundary. Claude preserves managed policy outside ordinary
  setting-source controls, so Scufris must disclose it rather than overclaim
  that the built-in tool allowlist controls those processes.
- Validate an explicit `harness` plus optional `model` and `thinking` keyword
  tuple in project preferences with the same adapter resolver used at spawn.
  Invalid configured tuples degrade with an actionable context diagnostic.

## Implementation

- Removed the late Pi-only `review_of` rejection.
- Preserved source owner, project, canonical path, device, and inode checks.
- Added explicit review isolation metadata to spawn, inspect, recovery, and the
  foreground job list.
- Kept Pi reviewers on `read,grep,find,ls,scufris_report`.
- Added a Claude review launch path with `--print`, `dontAsk`,
  `Read,Glob,Grep`, explicit mutation/shell/web/subagent denial, no
  user/project/local settings sources, no slash commands, and an empty strict
  MCP configuration. It does
  not receive the raw report capability. The trusted wrapper bounds and records
  the final Markdown response.
- Updated workflow guidance and user/developer documentation.

## Independent review corrections

Independent job `950e511dd257` found one high, two medium, and one low issue in
commit `9eaba8a`.

- Bound harness completion publication to the expected generation under the
  global workflow lock and recheck it under the report lock. Terminal events
  suppress publication only when they belong to that same generation. Claude
  steering now proves generation 2 uses `--resume` and gets its own terminal
  report; stale generation 1 output is ignored.
- Removed the review workspace check/recompute split. Initial creation,
  precreated completion, restart, and recovery validate the recorded path,
  device, and inode. The launch wrapper validates the actual inherited current
  directory by inode before starting the harness, so pathname replacement
  between validation and tmux `chdir` fails closed.
- Added native Claude review creation-crash recovery coverage and replacement
  inode tests for initial, precreated, recovery, and wrapper launch paths.
- Probed Claude Code 2.1.220 with a real project `PreToolUse` Read hook and the
  exact review arguments. The Read completed and the hook marker stayed absent,
  proving project settings are excluded. No managed settings file existed at
  the tested host locations. Managed hooks cannot be disabled by ordinary
  settings, so runtime metadata and docs now name `managed-claude-policy` as a
  trusted boundary instead of claiming control over it.
- Corrected tmux documentation: direct-report adapters receive the report
  capability; captured Claude reviews intentionally do not.

## Verification evidence

- Original reproduction before the fix: Claude Opus xhigh `review_of` spawn
  returned `independent review jobs require the read-only Pi harness`.
- Focused Python integration: Pi and Claude review jobs, adapter diagnostics,
  exact workspace identity, tool mutation probes, captured Claude reporting,
  and capability exclusion pass.
- Full Python suite after independent-review corrections:
  `python3 -m unittest discover -s tests -p 'test_*.py'` - 55 tests pass.
- TypeScript/package suite: `npm run check` - 61 tests pass, with typecheck and
  Prettier checks passing.
- Python quality: Ruff lint and format checks pass for the helper and tests.
- Packaging: `nix flake check` - all supported x86_64-linux checks pass. Nix
  reports only existing deprecated platform warnings and incompatible-system
  omissions.
- Real Claude CLI proof: Claude Code 2.1.220 accepted the exact Opus xhigh
  review isolation arguments, read `package.json`, returned a sentence naming
  `scufris2`, and left the Git status SHA-256 unchanged at
  `c798111e60c9c683ff54a0eff9384eb47182dc430a4adfde313873b6beddd3d1`.
- Real hook probe: a project `PreToolUse` Read hook was present, Claude read the
  requested file, the hook marker remained absent, and Git status stayed at
  `7fa2bb3dde7855b4cb62b6faecb110389ab4597abe8c1c2ccace81c129abb4f4`.
  No managed settings file was present at the checked Linux host locations.
- `git diff --check` passes.

## Remaining risk

The safety boundary restricts built-in model tools. It is not an OS filesystem
sandbox and does not defend against a malicious or compromised harness
executable. Claude administrator-managed hooks and plugin hooks are also trusted
host policy and can run outside this allowlist. Documentation and runtime
metadata state these limits.
