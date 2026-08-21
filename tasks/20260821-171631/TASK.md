# Define Scufris orchestration and worker lifecycle

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: agents, orchestration, pair

## Goal

Make Scufris a persistent foreground orchestrator that pairs by default, delegates only well-defined work, gives workers a complete role and protocol, mediates decisions and blockers, opens local review, and lands only exact approved revisions.

## Accepted design

### Scufris identity and Pair

- Add one canonical `prompts/pair.md`, maximum 500 ASCII bytes.
- Inject its complete contents into the Scufris system prompt on every model turn so identity and Pair behavior survive compaction.
- Pair is automatic. User does not invoke `/pair`.
- Keep the prompt in this repository for fast iteration. It can later be packaged by nix.dotfiles as the general Pair skill.
- Before proposing work, inspect the smallest relevant project context: instructions, conventions, docs, task artifacts, relevant code and tests, Git state, project history, and user style.
- Stop only at real decisions. Give recommendations and consequences. Options are not exhaustive; accept combinations, modifications, annotations, and other proposals.

Canonical prompt:

```text
You are Scufris, the foreground orchestrator. Pair by default: inspect project instructions, docs, code, tests, history, and user style before proposing work. Stop only at real decisions. Give recommendations and consequences; options are not exhaustive. Record durable decisions. Full-send only bounded, well-defined work. Give workers complete context, scope, artifacts, checks, and protocol. Mediate decisions, blockers, review, and landing. Workers are bounded executors, not user sessions.
```

### Readiness and artifacts

- Readiness is a prose judgment by Scufris, not a deterministic router.
- Full-send small work only when it has one bounded outcome, no unresolved product decision, no durable architectural decision, known constraints and checks, and a self-contained prompt.
- Use a Tatr task folder for durable, consequential, multi-stage, multi-agent, or compaction-sensitive work.
- `TASK.md` is normally enough. Use `NOTES.md`, `DESIGN.md`, HTML, PNG, demos, diagrams, or other artifacts when useful.
- Retain distilled decisions, not raw chat transcripts.
- Commit accepted design artifacts before implementation delegation so the worker receives them in its Sprout.
- Scufris can delegate bounded research, prototype, diagram, or demo artifacts while Pair continues. Workers never spawn workers.

### Worker handoff and role

Every generated worker prompt explains upfront:

- It is a bounded delegated worker, not a normal user session.
- Exact problem, outcome, scope, accepted design, artifacts, constraints, non-goals, checks, and completion conditions.
- It works only in its assigned worktree, does not land or push, and does not spawn workers.
- It commits implementation, synchronizes its selected Sprout feature, and records verification evidence.
- Complete status protocol and how Scufris responds.

Status semantics:

- `working`: sparse progress; UI only; never wakes the foreground model.
- `needs-decision`: detailed decision context in report, then wait; wakes Scufris.
- `blocked`: detailed blocker context in report, then wait; wakes Scufris.
- `review-ready`: committed, synchronized, checked, and documented; wakes Scufris once.
- `done`: final non-review result or review approval acknowledged.
- `failed`: terminal failure.

### Review and landing

- On `review-ready`, verify clean worktree, synchronization, landing and feature revisions, and ancestry.
- Open Plannotator through its public Pi event API with `since-base`.
- Feedback returns to the same worker. It resumes work, commits, synchronizes, checks, and emits another `review-ready`.
- Run an independent review worker only when requested.
- Structured Plannotator approval is bound to exact revisions.
- After approval, send exactly one worker instruction to avoid repository changes, finalize `report.md`, append `done: review approved with no changes requested`, and wait.
- Missing `done` is a protocol bug and blocks automated landing. Do not normalize it as an accepted fallback.
- After `done`, reverify exact approved revisions, run Sprout land dry-run, land locally, and stop the worker.
- GitHub issues, forge PRs, stacked PRs, and Tatr-to-issue synchronization remain out of scope.

### Presentation

- Keep `working` telemetry out of model context.
- Preserve compact UI notification and inspection history.
- A dashboardd job widget is a possible later presentation layer, not part of this task.

## Definition of done

- Scufris system prompt contains the exact canonical Pair prompt on every turn and after compaction.
- Normal Pi sessions and delegated Pi workers do not receive the Scufris system prompt.
- Generated worker prompts contain the complete role, capability, escalation, review, and completion contract.
- `working` never triggers a model turn. All actionable states trigger once.
- `needs-decision` and `blocked` details can be inspected and answered through the same worker.
- `review-ready` drives structured Plannotator review through the public event API.
- Feedback and approval transitions are covered by focused tests.
- Approval requires worker `done` before exact guarded local landing.
- Existing cross-project selection, descriptive features, normal-server tmux sessions, and exact resource ownership remain intact.
- Architecture, protocol, skills or prompts, and task evidence match implementation.

## Verification

- Focused TypeScript tests for system-prompt, event, review, and lifecycle policy.
- Python integration tests for generated worker prompt and job protocol.
- `npm run check`.
- Python unittest and Ruff checks.
- Focused Nix launcher and Home Manager checks.
- Live playtest: small full-send job, blocked or decision mediation, review feedback or approval, worker done, and local landing.

## Completion record

- Added the exact 495-byte canonical `prompts/pair.md`. The foreground-only identity extension appends it from `before_agent_start` on every agent run, including post-compaction runs. The Scufris launchers set a private foreground marker. Ambient delegated harnesses remove it, and normal Pi package loading stays unchanged.
- Replaced opt-in delegation guidance with automatic Pair readiness. Scufris now inspects the smallest relevant project context, retains durable decisions, full-sends only bounded self-contained work, and uses task artifacts for consequential or compaction-sensitive work.
- Expanded generated worker prompts with the complete bounded-worker role, capability and scope limits, status semantics, detailed decision and blocker reports, same-session mediation, review feedback loop, exact approval acknowledgment, synchronization and check evidence, and no-land, no-push, no-worker-spawn rules.
- Added trusted review snapshots and guarded landing helper verbs. They verify the selected main checkout and feature, exact branches and revisions, clean tracked state, ancestry, commit subject, Sprout worktree location, exact tmux identities, and absence of unowned windows. Landing rechecks the approved snapshot before and after `sprout land --dry-run`.
- Added automatic structured Plannotator review through `plannotator:request` with `code-review` and `since-base`. Structured feedback takes precedence over approval and returns as one bounded literal message to the same worker. Closed, malformed, unavailable, oversized, or inconsistent review outcomes fail closed as actionable lifecycle blockers.
- Bound approval to the exact landing SHA, feature SHA, subject, and review request. Scufris sends one final no-change instruction, requires exact `done: review approved with no changes requested`, reverifies, lands locally through Sprout, and stops the exact worker. Missing or incorrect acknowledgment blocks landing.
- Preserved quiet `working` telemetry, once-per-line actionable wakeups, detailed report inspection, cross-project IDs, descriptive feature names, normal-server detached tmux sessions, ambient normal worker Pi, collision rejection, and exact resource ownership.
- Updated architecture, protocol, delegation guidance, launcher resources, and focused TypeScript and Python tests.
- Verification passed: `npm run check` (8 TypeScript tests); `python3 -m unittest discover -s tests -p 'test_*.py'` (13 integration tests); Ruff lint and format checks for tests and the extensionless helper; `nix flake check`; Pi offline extension load and model listing; `git diff --check`.
- `nix flake check` built launcher, resource, and Home Manager checks for x86_64-linux. It omitted incompatible configured systems.
- Integration tests use real Git, Sprout, and isolated normal-server tmux with fake ambient Pi and Claude executables. They exercise complete prompt generation, foreground-marker removal, review snapshots, revision drift rejection, unowned-window rejection, dry-run, local landing, and exact cleanup.
- Limitation: no interactive provider and browser playtest was run in this non-foreground worker. Structured public-event response policy and process integration are covered separately. A foreground Scufris session must complete the listed live feedback or approval playtest before release.
- Tradeoff: trusted project and worktree paths exist only in extension-owned memory and private helper calls. They are not added to model tool schemas or immutable job records. This keeps cross-project review possible without exposing generic filesystem paths.
