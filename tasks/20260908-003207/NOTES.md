# Research notes: delegate-and-verify

Task `20260908-003207`. Market research plus an internal audit. No code was
changed. Every claim below is either a cited URL or a `file:line` in this
checkout. "(vendor)" is a product page or company blog. "(user)" is a report by
someone who used the tool. "(snippet)" means the page was not fetched and only
the search excerpt was read.

## Summary

- Every serious tool has converged on the same proof-of-done: a branch or PR
  URL, a check state the system read from git or CI, and artifacts. The agent's
  own summary is shown, but it is labeled as what the agent says.
- The false "pushed" and "merged" claims in Scufris are structural. Nothing in
  the stack runs `git push`, `git ls-remote`, `gh run`, or checks a tag. The
  worker's `done` prose is the only source, and the foreground repeats it.
- The branch-without-merge deletion has two open paths: a worker with a full
  shell and `sprout rm`, and `scufris_job_stop` with `remove_workspace`.
- Surfaces today get one aggregate state (`blocked`, `failed`, `clear`). No job
  list, no elapsed time, no git facts reach the desktop or the phone.
- A nightly profile can be collected by hand today. It cannot be scheduled: one
  timer, one time, one profile, one run directory per date.
- Named routines exist per project as `[agents.<name>]`. There is no one-word
  alias across projects and the prompt is re-derived by the model each time.
- Build landing receipts first. They close the exact incident, cost no UI, and
  give the jobs widget its tier-2 columns.

## Trust tiers

**Tier 1, fire and forget.** Alex says the word and never checks. Examples: the
morning briefing, "do the videos for seedzero". Scufris must guarantee that the
work is bounded, that it changes nothing outside what the project's own file
allows, and that the answer is short. Failure is acceptable when it is named.
Silence is acceptable when nothing happened (OpenClaw's `NO_REPLY`). The lever
is more entries in `.scufris.toml` and a way to say them in one word.

**Tier 2, delegate but verify.** Alex delegates and reads the result before
trusting it. Examples: a Scufris feature, a nightly review. Scufris must
guarantee that every completion claim about git, remotes, CI, and tags is a
fact the helper measured, not a sentence the worker wrote. It must show which
agents are running and what state they are in without tmux. It must never
delete a branch that was not landed, and never say "pushed" or "released"
without a remote or a run to point at. Anything the worker claims that the
helper did not measure is reported as "claimed, not verified".

**Tier 3, keep for yourself.** Flow work: the nova-protocol story, pair
programming, brainstorming. Scufris must stay out of the way. The only
guarantee is absence: no wake, no unprompted widget, no delegation nudges.
Not a gap. Some tier 3 work (nightly bug hunting) moves to tier 2 once tier 2
is trustworthy.

## Market findings

### Claude Code (routines, mobile, agent view, best practices)

Routines are saved prompts run as cloud sessions on a schedule, an API call,
or a GitHub event (vendor:
https://code.claude.com/docs/en/routines). Each run is a session in the list
where you "see what Claude did, review changes, create a pull request". Pushes
go to `claude/`-prefixed branches; a push elsewhere is rejected if the branch is
protected, has someone else's open PR, or carries others' commits. The
important sentence: "A green status in the run list means the session started
and exited without an infrastructure error. It does not mean the task in your
prompt succeeded. Open the run to read the transcript and confirm what Claude
actually did." The CLI can answer "/schedule why did my nightly review do
nothing this morning?" by reading the run log.

Agent view (`claude agents`) is the closest thing to an agent board (vendor:
https://code.claude.com/docs/en/agent-view). Rows: state icon (Working, Needs
input, Idle, Completed, Failed, Stopped), a one-line summary, age, and a PR
label whose colour comes from GitHub (yellow waiting or failed checks, green
passed, purple merged). The doc separates the two explicitly: "A working row
shows what the session says it's doing". Groups: Needs input, Working, Ready
for review, Completed. Limits: sessions are local; worktrees die with the
session.

Mobile: the Code tab lists sessions; push notifications fire "when a
long-running task finishes or when it needs a decision from you" (vendor:
https://code.claude.com/docs/en/mobile,
https://code.claude.com/docs/en/whats-new/2026-w16).

Verification guidance (vendor: https://code.claude.com/docs/en/best-practices):
"Have Claude show evidence rather than asserting success: the test output, the
command it ran and what it returned, or a screenshot." Stop hooks are named as
the deterministic gate. "The trust-then-verify gap ... If you can't verify it,
don't ship it."

A user running production routines (user:
https://makerkit.dev/blog/tutorials/claude-code-routines-guide): "propose, do
not perform"; "the trust model is not 'watch it closely'. You cannot. The trust
model is 'build boundaries it cannot cross.'"; "Fail loud".

### OpenAI Codex cloud

Tasks sit in a sidebar per environment with timestamps and a merge state on
the row ("Merged +24 -8", "Closed +7 -4"). A finished task delivers a diff, a
summary, test logs and terminal output with citations, and a PR on request.
"Inspect the summary and diff, request a follow-up, or open a pull request when
the result is ready." (vendor: https://learn.chatgpt.com/docs/cloud). A user
describes the loop from Slack dispatch to merge: "Codex reacts with eyes whilst
processing and replies with a task link upon completion"; `codex cloud status`,
`codex cloud list`, `codex cloud diff`, `codex apply` (user:
https://codex.danielvaughan.com/2026/04/08/codex-cloud-task-application/).
The deliverable is the diff. Status during the run is minimal.

### Cursor cloud agents

Agents work on their own branch and push for handoff. The run keeps
"screenshots, videos, and logs so you can see exactly what changed and how the
agent verified its work" and the run URL is shareable (vendor:
https://cursor.com/docs/background-agent). A control panel lists agents and
their status (snippet). Thin on status detail; strong on artifacts as proof.

### Devin

2026 release notes (vendor: https://docs.devin.ai/release-notes/2026): sidebar
notifications with persistent labels such as "PR created" and "Awaiting
instructions" plus unread dots (Aug 19); "CI failed" rows open the PR tab with
checks expanded; an Agents tab for child sessions (Mar 27); a Test Recording
Viewer with pass/fail cards (Mar 27); a popover that names missing write
permission "preventing silent failures" (Jul 1).

After 18 months (vendor: https://cognition.com/blog/devin-annual-performance-review-2025):
teams delegate "clear, upfront requirements and verifiable outcomes that would
take a junior engineer 4-8 hrs"; "67% of its PRs are now merged vs 34% last
year"; humans keep ambiguous work, design, mid-task scope changes, and final
verification ("code owners will check to see if all logic has been tested").

One team's report (user: https://zackproser.com/blog/devin-has-come-a-long-way):
a 2024 trial ended with "zero completed sessions"; in 2026 "Messages land,
tasks execute, PRs appear. No mysterious failures." and "The PRs that come back
are reviewable." They "review it after lunch". Adoption came because "it kept
working", not from a mandate. A secondary review (user:
https://easyclaw.com/blog/knowledge/devin-ai-review): "Treat every
Devin-generated PR as you would any junior engineer's work: read the diff, run
the tests locally, check for edge cases."

### GitHub Copilot coding agent

Every agent commit carries an `Agent-Logs-Url` trailer, a permanent link from
the commit to the session log (vendor:
https://github.blog/changelog/2026-03-20-trace-any-copilot-coding-agent-commit-to-its-session-logs/).
Session logs "show Copilot's internal reasoning and the tools it used to
understand your repository, make changes, and validate its work"; the sessions
page steers, stops, and archives (vendor:
https://docs.github.com/en/copilot/how-tos/copilot-on-github/use-copilot-agents/manage-and-track-agents).
Setup steps and subagent activity were added to the log (vendor:
https://github.blog/changelog/2026-03-19-more-visibility-into-copilot-coding-agent-sessions/).
The PR opens at the start with a checklist that is ticked as commits land
(snippet, GitHub blog).

### Google Jules

Queue model: describe a task, walk away, a PR arrives. Changelog (vendor:
https://jules.google/docs/changelog/): pause, resume, delete tasks (Jul 2025);
web app testing with screenshots (Aug 2025); open a PR from the UI (Aug 2025);
responds to PR comments (Sep 2025); Scheduled Tasks (Dec 2025) with edit,
pause, resume (Jan 2026). No daily digest was found.

### Local boards: Vibe Kanban, Conductor, Claude agent view

Vibe Kanban: columns To Do, In Progress, In Review, Done; per task the task
branch, target branch, and commits ahead and behind; Rebase, Merge, Create PR.
"When your PR is merged on GitHub, your task automatically moves to Done."
(vendor: https://vibekanban.com/docs/core-features/completing-a-task). A
reviewer valued it because "Managing this workflow using only terminal tools
becomes complicated ... tmux ... the complexity increases quickly", and noted
agents run with permission-skipping flags by default (user:
https://elite-ai-assisted-coding.dev/p/vibe-kanban-tool-review). An open issue
reports worktrees left behind after merge (snippet:
https://github.com/BloopAI/vibe-kanban/issues/1764).

Conductor: "Each task gets its own workspace, branch, files, terminal, diff,
and review path" and it "helps you review the diff, open a pull request, merge,
and archive the workspace" (vendor: https://conductor.build/docs). The docs
have a Checks page. Claude agent view (above) is the same category with the PR
label read from GitHub.

### OpenHands (Aider, Warp: light evidence)

The verification stack (vendor:
https://www.openhands.dev/blog/20260506-the-verification-stack): a critic model
scores the run before push and can retry; a review agent with a 10-scenario
checklist; a QA agent that "actually runs the software"; the iterate skill
"opens a draft PR (so auto-merge and deploy don't fire early), runs whatever
verification layers the repo has, fixes what they flag, and pushes again. It
repeats until everything passes" and only then marks the PR ready. High-risk
PRs go to a human. Aider iterates against text output and Warp has a review
panel (snippet only). Not studied further.

### Personal assistants and briefings

OpenClaw heartbeat (vendor: https://docs.openclaw.ai/gateway/heartbeat): a
system-owned periodic turn, 30 minutes by default; delivered to the owner DM,
a channel, or nowhere; the agent answers `NO_REPLY` when nothing needs
attention; flood guard and active hours. A one-month user report (user:
https://dev.to/cypriantinasheaarons/i-replaced-my-entire-productivity-stack-with-an-ai-agent-running-247-4f6p):
06:00 morning briefing (weather, calendar, emails, priorities), 21:00 evening
wrap-up ("what got done, metrics, tomorrow's plan"), delivered to Discord;
"3 cron jobs failed due to a misconfigured delivery channel. The heartbeat
caught it". Review time on the automated task fell from 45-60 minutes to
"10 min (review only)".

Home Assistant's daily summary is calendar plus weather, sent to a messenger
(vendor: https://www.home-assistant.io/voice_control/assist_daily_summary/).
Community builds add garbage day, school lunch, and RSS headlines, once a day,
often read aloud on entering the kitchen (snippet,
https://community.home-assistant.io/t/morning-briefing/417246).

### Community signal

Why developers do not use background agents (user, HN
https://news.ycombinator.com/item?id=46615737): "easily intervene, correct and
live-check" (lompad); "they need too much hand holding still imho" (Zekio);
"I do not trust an agent to give it unsupervised access to my systems"
(thesuperbigfrog); "not following along and making it interactive adds
compounding interest to the cost of editing" and "most of my usage is walking
through a pre-determined set of steps" (speakingmoistly).

Parallel agent lifestyle (user, HN https://news.ycombinator.com/item?id=45489884):
"review is hands down the biggest bottleneck" (Areibman); "the code is rarely
good enough to accept blindly, but the response is quick enough that it feels
like progress" (joshvm); "I turn all edit permissions off and manually approve
each change" (extr); "how does everyone visually organize the multiple terminal
tabs open for these numerous agents?" (aantix); simonw sends an agent to
implement a feature "with no intention of actually using their code".

Six months exclusively with agents (user, HN
https://news.ycombinator.com/item?id=49465119): delegated after the novelty:
writing, boilerplate, reviews by several model families, triage; kept: problem
definition, design, final review, permission enforcement. "I was EXHAUSTED at
the end of the day" (alexpotato, on parallel agents); "The agents tend to be a
bit overeager, so I have to do a lot of work up front" (mrothroc);
deterministic gates run before any human reads the diff.

Why we built our own background agent (user, HN
https://news.ycombinator.com/item?id=46589842): the closed feedback loop and
sandbox output piped to the UI are what made it credible; "you basically
implemented ralph wiggum in the cloud" is the skeptic's line.

Fake verification is the common hallucination (user:
https://claudefolio.com/blog/how-to-tell-when-claude-code-is-hallucinating):
"The agent says 'All tests pass' when it never ran the tests"; "outcome claims
with no evidence attached: no command output, no test results, no screenshot";
rule: "no receipts, no belief"; also "narrative-diff mismatch". A r/ClaudeAI
post "Claude has been lying to me instead of generating code" reports the model
admitting "not testing implementations" and "lying about completeness"
(snippet, archive returned 403).

### Patterns

**Proof-of-done has converged.** Branch or PR URL plus a check state the
system read, plus artifacts. Claude agent view colours the PR label from
GitHub; Codex shows "Merged +24 -8" on the row; Vibe Kanban moves Done only on
the merge event; Devin labels "PR created" and opens CI failures; Cursor and
Jules keep screenshots and logs; Copilot links every commit to its log.

**Agent says versus system verified is drawn as a line.** Claude routines:
green means "exited without an infrastructure error", not success. Agent view:
the row text is "what the session says it's doing", the PR label is not.
OpenHands gates the push with a critic and keeps the PR a draft until checks
pass. Best practices: evidence, not assertion; hooks, not instructions.

**What people keep 1:1 after the novelty.** Delegated: well-specified,
verifiable, 4-8 hour junior tasks (Cognition), ticket-to-PR, migrations,
regressions with a failing test (easyclaw), boilerplate and first-pass review
(HN 49465119). Kept: design, ambiguity, mid-task changes, taste (zackproser),
and anything where "intervene, correct and live-check" is the point (HN
46615737). Boundaries replace supervision (makerkit).

**Briefings that stick.** Same time daily, short, a few measured values, one
recommendation, and silence when there is nothing (OpenClaw `NO_REPLY`, the
scufris2 briefing guidance's "a green master with a clean tree is a good
morning"). A morning and an evening pair is the one recurring shape (dev.to
06:00 and 21:00; Home Assistant once a day). The evening one is "what got
done".

**Live state on a phone or a widget.** Claude Code: session list in the app
plus push when finished or blocked. Devin: sidebar labels with unread dots.
Codex: task sidebar grouped by day. Agent view: a desktop table. No tool was
found with a desktop widget; the nearest is a tray-style notification. Scufris
already has the widget framework the others lack.

## Internal audit

### 1. Where a false "pushed" or "merged" claim comes from

The path is prose end to end.

- The worker writes the claim. Pi workers call `scufris_report`
  (`agent/extensions/scufris/workflow/worker-report.ts:33-78`), Claude workers
  run `scufris-report` (`tools/jobs/scufris-report:12-63`). Both send
  `{event, summary, report}` to the helper's `report` command
  (`tools/jobs/scufris-jobs:2111`). The helper checks the capability, the
  event keyword (`:2128`), byte bounds, and generation. It never reads git.
- The contract the worker is given asks for "one-line status summary" and
  "detailed Markdown evidence" (`tools/jobs/scufris-jobs:731-800`). No required
  fields, no command output, no revision.
- The `done` event wakes the foreground with "Inspect the pinned job context,
  prompt, report, and state, then tell the user what happened"
  (`agent/extensions/scufris/workflow/orchestration.ts:229-245`). The policy
  text says the same (`orchestration.ts:66-70`).
- `scufris_job_inspect` returns the record fields, the last 100 events, and
  the report text (`tools/jobs/scufris-jobs:2368-2440`). No branch, no
  revision, no clean or dirty state. `scripts/scufris-jobs` prints the same
  record fields, events, and report (`scripts/scufris-jobs:250,267`); its only
  subprocess is the helper (`:54`). The git facts promised in task
  `20260821-190123` ("worktree existence, branch, revision, clean or dirty
  state") are not in the current CLI.
- `scufris_final_response` validates plain-prose shape only
  (`agent/extensions/scufris/response.ts:12,131-176`).
- Nothing in `tools/`, `scripts/`, `agent/`, or `.claude/skills/` runs
  `git push`, `git ls-remote`, `gh run`, `gh pr`, `gh release`, or `git tag`
  (grep over those trees is empty). A "pushed" or "released" claim can only
  come from a worker's own shell and reaches Alex as the worker's sentence,
  repeated by the foreground.

What the helper does verify: the event is one of three keywords, the
capability matches the generation, evidence is durable before the event is
visible (`docs/src/dev/jobs.md`, Reporting). That is the whole verified set.

### 2. The landing path and where a branch can go without a merge

The tool path is safe. `scufris_job_land` (`orchestration.ts:1113-1145`)
calls helper `land` (`tools/jobs/scufris-jobs:2689-2760`): record a durable
intent with `HEAD` (`:2703-2712`), stop executions, check already landed by
`git merge-base --is-ancestor` (`:2725-2733`) or tree equality (`:2734-2741`),
then `sprout land --dry-run` and `sprout land` (`:2744-2745`), record the
landed revision (`:2746-2752`), then `cleanup_workflow` (`:2754`), which
removes the Sprout only after that (`:2660-2661`, `:2592-2617`, `sprout rm` at
`:2617`). `sprout land` itself refuses to run inside the worktree, refuses a
target mismatch, refuses a dirty main checkout, requires the feature to contain
the target tip, squashes with rollback, and prints `landed <hash> <subject>`
(`sprout` script, `/nix/store/...-sprout/bin/sprout:379-390,432-462,472-488`).
`remove_workspace` defaults to true on land (`:2695`,
`orchestration.ts:1124`), so a landed branch is deleted by design.

Paths that delete without a merge:

- `scufris_job_stop` with `remove_workspace: true`
  (`orchestration.ts:1246-1270`, helper `stop` at
  `tools/jobs/scufris-jobs:2763-2790`) runs the same cleanup and therefore
  `sprout rm`, which is `git worktree remove` then `git branch -D`
  (`sprout:500,516`) with no merge check. Correct for "abandon"; wrong when the
  model calls it after reading a worker's "landed" sentence.
- The worker. It has a full shell in its worktree (`--approve` for Pi, the
  normal adapter for Claude, `docs/src/dev/jobs.md` Spawn), `sprout` on PATH,
  and the project root in its record. `sprout land --remove` or `sprout rm`
  from the main checkout, or `git branch -D` after checking out master, all
  work. The only rule against it is prose in a development skill, "Remove
  resources only with user approval" (`.claude/skills/sprout/SKILL.md:18-20`),
  which is not in the worker prompt. `sprout rm` (`sprout:492-530`) has no
  merged check and no role check.
- "Pushed" has no check anywhere. A land is a local squash onto local
  `master`. Pushing master and the tag is a manual step (`RELEASE.md:23-25`), and
  the release workflow runs only on a tag push
  (`.github/workflows/release.yml:3-5`). So "landed", "pushed", and "released"
  are three different facts and Scufris holds only the first.

Deterministic checks that would close it, all cheap and already available:

- Landed: `git merge-base --is-ancestor <landed_revision> refs/heads/<base>`
  (exists at `:2725-2733`); branch absent:
  `git rev-parse --verify refs/heads/<feature>` fails; worktree absent.
- Pushed: `git fetch origin <base>` then
  `git merge-base --is-ancestor <landed_revision> refs/remotes/origin/<base>`,
  or `git ls-remote --heads origin <base>` compared with the local tip;
  ahead/behind via `git rev-list --left-right --count`.
- Released: `git ls-remote --tags origin vX.Y.Z` and
  `gh run list --workflow release.yml --commit <sha> --json status,conclusion,url`;
  `tools/release/check_versions.py` for the version match.
- CI on master: `gh run list --branch master --commit <sha> --json
conclusion,url`. The scufris2 morning briefing already runs this shape of
  command (`.scufris.toml`, `briefings.morning`), so the pattern is proven.

### 3. What exists toward an agent board

- Durable state: `job.json`, append-only `status`, `report.md`, generations,
  exact tmux identity (`docs/src/dev/jobs.md`). `scripts/scufris-jobs all
--json` lists ID, STATE, LIVE, PROJECT, WORKSPACE, WORKER, SUMMARY
  (`scripts/scufris-jobs:20-28`), read-only and fail-closed (tasks
  `20260821-190123`, `20260823-160757`). `scufris_job_list` gives the model
  state, summary, and `window_alive` (`orchestration.ts:1150-1178`). The
  `scufris_agent_diagnostics` tool from task `20260821-222841` no longer exists
  in `agent/` (grep empty); list and inspect replaced it.
- What surfaces receive: one aggregate. The workflow extension emits a per-job
  notice (`orchestration.ts:229`); the service extension folds the set into
  `failed`, `blocked`, or `clear` with one detail string
  (`agent/extensions/scufris/service/index.ts:50-75`); the service stores it
  (`host/service/src/service.rs:452-456`) and merges it into `ScufrisState`
  (`service.rs:84-95`, enum at `shared/control/src/service.rs:118-124`). The
  desktop paints the tray (`surfaces/desktop/src/tray.rs:55,104`); iOS maps
  `blocked` to an attention notice
  (`surfaces/ios/Sources/ContentView.swift:1134-1135`). Task `20260827-212938`
  decided "Notices are tray-only" and CHANGELOG 0.5.0 records it
  (`CHANGELOG.md:302-306`). Neither surface knows how many jobs, which project,
  elapsed time, `working` versus `done`, or any git fact.
- Widgets: seven shipped (`surfaces/desktop/widgets/`: agenda, claude, codex,
  cpu, macros, notes, timer) and six backends (`surfaces/desktop/backends/`:
  claude, codex, den, system, timer, today). No jobs widget, no jobs backend.
  A widget is `widget.toml` plus `widget.ts`, fed by a deterministic backend
  process writing JSON lines (`docs/src/dev/widgets.md`, Backend rule;
  `surfaces/desktop/src/widgets/catalog.rs:1-30`). Widgets are presentation
  only, opened by a model widget call or the tray, pinnable
  (`docs/src/guide/using.md`, Widgets). iOS has no widgets
  (`docs/src/dev/architecture.md`, "iOS: WSS, text UI").
- Task `20260825-153801` (closed, wontdo) wanted an escalation ladder: "a line
  in the pill, then exhibits beside it, then the session surface raised". The
  conversation window took the third rung; the first two are still open ideas.

A jobs widget needs: a `jobs` backend that wraps `scripts/scufris-jobs all
--json` on a cadence of a few seconds; a widget with one row per live job (id,
project, agent, state, live pane, elapsed, last summary); receipt fields
(branch present, landed revision, pushed, CI) once they exist in durable state;
a tray summon; and for the phone either a protocol frame carrying the job list
replayed on connect (the notice set already does this for one state) or a
rendered page like the briefing page.

### 4. Briefings: can a nightly profile run today

Collect by hand: yes. Any profile name is accepted (`tools/briefing/briefing.py:9`,
`validated_profile` at `:107-109`; `briefing_sources(profile)` at
`tools/jobs/scufris-jobs:623-660`). `scufris-briefing collect --profile
nightly` works (`tools/briefing/cli.py:44-46,89-90`) and `scufris_briefing_run`
takes `profile` (`agent/extensions/scufris/briefing/briefing.ts:245-278`).

Schedule: no. One timer (`briefing.ts:108-115,152-160`). The scheduled profile
is `SCUFRIS_BRIEFING_PROFILE || "morning"` (`briefing.ts:121`) and nothing sets
that variable: the launcher exports only `SCUFRIS_BRIEFING_TIME`
(`nix/launcher.nix:44-48`) and the module exposes only `briefing.time`
(`nix/home-manager.nix:107-118`). `decide()` knows one time per day
(`agent/extensions/scufris/briefing/schedule.ts:86-95`). The run directory is
`briefings/<date>/` with no profile (`briefing.py:123-124`), so a second
profile on the same date finds the morning's `delivered` state and waits
(`schedule.ts:88-94`). The docs say so: "Only `morning` is scheduled today"
(`docs/src/dev/briefings.md`).

Fit for a nightly project review: a source is one bounded read-only headless
run in the project root, 900 s per source, 1800 s per run, one JSON envelope
with a Markdown body, rendered to a page (`docs/src/dev/briefings.md`). "Read
today's commits in nova-protocol, list suspicious spots and refactor
candidates" fits exactly and lands on a page Alex already reads. "Fix bugs
overnight" does not fit: that needs a worktree, commits, and landing, which is a
job, and jobs have no scheduler; only the foreground model spawns them. To
schedule a profile: a profile-to-time map instead of one time, run directories
keyed by date and profile, per-profile state, `decide()` per profile. The wake
message already carries the profile (`briefing.ts:103,147`). Effort M.

### 5. Named routines today

Per project, yes. `[agents.<name>]` with `description`, `keywords`, `guidance`
(`tools/jobs/scufris-jobs:513-540`) is a named agent with a fixed harness,
model, and thinking. seedzero declares `[agents.produce]` "Produce the next
Seed Zero short end to end and publish it after QA."
(`~/personal/seedzero/.scufris.toml`). The foreground starts "only the agents
this request names" and delegates to an unknown name "with that entry's
keywords" (`orchestration.ts:50-60`), so "produce for seedzero" resolves.

Missing: no alias across projects; the worker prompt is composed by the model
each time (`agent/skills/workflow/SKILL.md`, steps 1-5) so the wording drifts;
`scufris_project_context` is single-use and is "a menu, not a workflow"
(`orchestration.ts:743-759`, `tools/jobs/scufris-jobs:513-517`); no schedule;
no prompt-template directory is wired into the launcher
(`nix/launcher.nix:10-24` passes extensions and skills only).

### 6. The open tasks by tier

`tatr ls --filter ':status eq OPEN'` lists five; four besides this one.

- `20260825-153756` Look at this: verb, window identity, selection. Tier 3.
  Desk-side, synchronous, makes 1:1 work smoother.
- `20260825-153806` Wake word. Input path. Tier 1 at most (say it without a
  key). Does not touch tier 2.
- `20260828-220328` Look at this: ask the program. Tier 3.
- `20260828-224226` Look at this: the picture. Tier 3.

None of the four serves tier 2.

## Feature candidates

Ranked. Effort: S under a day, M a few days, L a week or more.

1. **Landing receipts.** Tier 2. Alex reads "landed 3f2a1c on master, branch
   removed, origin/master 1 behind, no release run" instead of a sentence, and
   stops opening tmux to check. Verification: a helper `receipt` command
   measures ancestry, branch and worktree existence, remote ahead/behind after
   a fetch, tag presence local and remote, and `gh run list --commit`; writes
   `receipt.json` beside the job; `land`, `stop`, and `inspect` return it. The
   foreground policy: quote receipt fields verbatim; any worker claim of push,
   merge, or release with no matching receipt field is said as "claimed, not
   verified". Exists: `already_landed` (`scufris-jobs:2725-2741`), the landed
   revision in the cleanup intent, the `sprout land` guards. Effort S-M.
   Draws on: Claude routines "green is not success", agent view's PR label
   from GitHub, Vibe Kanban Done on the merge event, Copilot commit trailers.

2. **Worker boundary for branch removal and push.** Tier 2. Alex never finds a
   branch gone without a landing. Verification: `sprout rm` and `sprout land`
   refuse when `SCUFRIS_ROLE=worker`, and `sprout rm` refuses an unmerged
   branch without `--force`; the worker prompt says landing and removal are
   the foreground's; `scufris_job_stop` with `remove_workspace` requires a
   receipt that says landed or an explicit "abandon" in the request. Exists:
   the role variable, the inside-worktree guard on land (`sprout:379-390`).
   Effort S. Draws on: Claude routines' branch-push checks, makerkit "build
   boundaries it cannot cross", OpenHands draft PR until checks pass.

3. **Jobs widget, desktop then phone.** Tier 2. Alex glances at a pinned panel
   instead of attaching to tmux; the phone shows the same rows. Verification:
   the backend reads durable state only through `scripts/scufris-jobs --json`;
   every column is a helper fact; the worker's summary is shown in a
   "says" column, the receipt in a "verified" column. Exists: the CLI, the
   widget and backend framework, the notice channel. Missing: the backend, the
   widget, receipt fields, an iOS list frame. Effort M desktop, M-L phone.
   Draws on: agent view rows, Devin sidebar labels, Codex task sidebar, Claude
   mobile session list.

4. **Nightly project review as a scheduled profile.** Tier 2, and it moves
   nightly bug hunting out of tier 3. Alex wakes to a page per project with
   yesterday's commits, suspicious spots, and refactor candidates, linked from
   the morning briefing. Verification: the source is read-only and bounded;
   facts are counts; nothing is claimed fixed; the page is code-rendered from
   the run. Exists: profiles, the collector, the page, the wake. Missing: a
   profile schedule map, run directories keyed by profile, per-profile state.
   Effort M. Draws on: Claude routines' nightly review example, Jules
   scheduled tasks, the 21:00 wrap-up in the OpenClaw user report.

5. **Named routines.** Tier 1. "videos" runs seedzero's `produce` with a fixed
   prompt every time. Verification: `[routines.<name>]` in `.scufris.toml`
   names an agent and a verbatim prompt; the context renders it as a routine
   the foreground may start by that one word without composing a prompt; the
   acknowledgment names the routine and the job. Exists: `[agents.<name>]`,
   the menu renderer. Effort S-M. Draws on: Claude routines' saved prompt,
   Jules scheduled tasks, skills with `disable-model-invocation`.

6. **Per-project hands-off or hands-on flag.** Tier 1 versus 2. Alex declares
   `conventions.keywords.trust = "fire-and-forget"` for seedzero and `"verify"`
   for scufris2 and nova-protocol. Verification: with `verify`, a `done` wake
   must carry the receipt and the foreground never calls stop with removal
   without one; with `fire-and-forget`, the wake is one line and no receipt is
   demanded. Landing stays explicit in both; "Scufris never lands implicitly"
   is settled. Exists: conventions keywords are free scalars. Effort S. Draws
   on: Cognition's "verifiable outcomes" scoping, per-category autonomy
   (vibe-eval snippet, weak), makerkit boundaries.

7. **Evidence-bearing done reports.** Tier 2. The helper stamps `HEAD`,
   `git status --porcelain`, and the branch into the status line at report
   time, so the report is anchored to a revision the worker cannot misstate.
   Folds into candidate 1. Effort S. Draws on: best practices "show evidence
   rather than asserting success", ClaudeFolio "no receipts, no belief".

8. **Yesterday's jobs in the morning briefing.** Tier 1. A built-in `jobs`
   source lists what landed, pushed, or failed with receipt facts. Effort S
   once receipts exist. Draws on: the evening "what got done" pattern.

Rejected:

- A kanban board application. The evidence says boards pay off with many
  parallel agents and a review bottleneck (HN 45489884, Berger). Alex runs a
  few jobs and already has the conversation window and widgets. The widget
  covers it with less surface.
- A critic model or self-review gate (OpenHands). The gap here is facts, not
  another model opinion. Review agents and Quick Review exist, and the review
  skill already notes adjudication is expensive
  (`.claude/skills/scufris-review/SKILL.md`, Adjudicate).
- Moving workers to the cloud. Workers are local by design (tmux, sockets,
  Nix), and the vendor docs say a green cloud run is not a success either.
- Auto-landing on green. Contradicts the settled "never lands implicitly", and
  the market keeps the PR a draft until checks pass.
- Push notifications as the first phone feature. The iOS app already shows
  unprompted responses when opened; the missing piece is rows, not a ping.

## Recommendation

Build landing receipts first. It is the exact incident: "pushed" with no push,
"released" with no run, "merged" with a branch gone. The helper already holds
half the facts (`already_landed`, the landed revision) and none of the remote
ones. Adding a `receipt` command, a `receipt.json`, and a foreground rule to
quote it verbatim costs S-M, needs no UI, and every tool studied converged on
this same shape: a state the system read, shown beside what the agent said.
Without receipts, a jobs widget would just show the worker's prose in a nicer
font.

Second, the worker boundary on `sprout rm`, `sprout land`, and removal on
stop. One day of work closes the branch-deletion path with a role check and a
merged check, the same "boundaries, not supervision" that routines use.

Third, the jobs widget on the desktop, reading the receipts, with the iOS list
frame after it. Then the nightly profile, because once tier 2 is trustworthy
the nightly review stops being tier 3 work.

## Sources

Vendor:

- https://code.claude.com/docs/en/routines - Claude Code routines (vendor)
- https://code.claude.com/docs/en/agent-view - Claude Code agent view (vendor)
- https://code.claude.com/docs/en/mobile - Claude Code on mobile (vendor)
- https://code.claude.com/docs/en/best-practices - Claude Code best practices (vendor)
- https://code.claude.com/docs/en/whats-new/2026-w16 - Claude Code week 16 digest (vendor)
- https://learn.chatgpt.com/docs/cloud - Codex cloud docs, redirect from developers.openai.com/codex/cloud (vendor)
- https://cursor.com/docs/background-agent - Cursor cloud agents (vendor)
- https://docs.devin.ai/release-notes/2026 - Devin 2026 release notes (vendor)
- https://cognition.com/blog/devin-annual-performance-review-2025 - Devin 18-month review (vendor)
- https://github.blog/changelog/2026-03-19-more-visibility-into-copilot-coding-agent-sessions/ - Copilot session visibility (vendor)
- https://github.blog/changelog/2026-03-20-trace-any-copilot-coding-agent-commit-to-its-session-logs/ - Copilot commit to log trailer (vendor)
- https://docs.github.com/en/copilot/how-tos/copilot-on-github/use-copilot-agents/manage-and-track-agents - Copilot sessions page (vendor)
- https://jules.google/docs/changelog/ - Jules changelog (vendor)
- https://vibekanban.com/docs/core-features/completing-a-task - Vibe Kanban completing a task (vendor)
- https://conductor.build/docs - Conductor docs introduction (vendor)
- https://www.openhands.dev/blog/20260506-the-verification-stack - OpenHands verification stack (vendor)
- https://docs.openclaw.ai/gateway/heartbeat - OpenClaw heartbeat (vendor)
- https://www.home-assistant.io/voice_control/assist_daily_summary/ - Home Assistant daily summary (vendor)

User:

- https://makerkit.dev/blog/tutorials/claude-code-routines-guide - production routines guide (user)
- https://codex.danielvaughan.com/2026/04/08/codex-cloud-task-application/ - Codex dispatch to merge (user)
- https://zackproser.com/blog/devin-has-come-a-long-way - one team's Devin adoption (user)
- https://easyclaw.com/blog/knowledge/devin-ai-review - Devin review (user, secondary)
- https://elite-ai-assisted-coding.dev/p/vibe-kanban-tool-review - Vibe Kanban review (user)
- https://claudefolio.com/blog/how-to-tell-when-claude-code-is-hallucinating - fake verification patterns (user)
- https://news.ycombinator.com/item?id=46615737 - Ask HN, why not background agents (user; read via hn.algolia.com API)
- https://news.ycombinator.com/item?id=45489884 - parallel coding agent lifestyle (user)
- https://news.ycombinator.com/item?id=49465119 - six months exclusively with agents (user; via API)
- https://news.ycombinator.com/item?id=46589842 - why we built our own background agent (user; via API)
- https://dev.to/cypriantinasheaarons/i-replaced-my-entire-productivity-stack-with-an-ai-agent-running-247-4f6p - OpenClaw one-month report (user)

Snippet only, not fetched:

- https://github.com/BloopAI/vibe-kanban/issues/1764 - worktree left after merge (user)
- https://community.home-assistant.io/t/morning-briefing/417246 - community briefing thread (user)
- https://digitalscholarship.library.jhu.edu/s/aivoices/item/360 - archived r/ClaudeAI "lying about completeness" post (user; 403 on fetch)
- https://vibe-eval.com/agentic-coding-security/devin-security-practices/ - per-category autonomy (secondary)

## Round 2: Alex's comments on the page (2026-09-08)

Nine page comments and seven annotations on the follow-up message. What
changed, keyed by the passage commented on.

### Nightly review as a scheduled profile

Alex: extend the morning briefing into a cron-like system, then nightly
review is easy; likes this most. Agreed. It moves to second place. Needs a
profile-to-time map instead of one timer (`briefing.ts:108-121`), run
directories keyed by date and profile (`briefing.py:123-124`), per-profile
delivered state (`schedule.ts:86-95`).

### Jobs widget, and why widgets feel clunky

Alex: likes seeing jobs without running the script, but the widget system
feels clunky and gimmicky; prefers keyboard, tmux, nvim; sometimes wants to
show `pi` instead of the HUD. Likely causes: widgets open after the turn, so
there is latency and unpredictability; they are mouse-first windows in an
i3 workflow with no keyboard path; they are presentation only. Decision: no
jobs widget first. Show live jobs as a row or side panel in the conversation
window. Correction from Alex: the tray and the pill are reserved for Scufris'
own state, so jobs do not go there. `scripts/scufris-jobs all` stays the
keyboard path.

### Named routines

Alex: too messy; prefer Scufris reading `.scufris.toml` and suggesting, so
the briefing says "seedzero could produce" and "yes" starts it. Dropped.
Replaced by offers: one optional offer per source in the briefing envelope
(agent name plus one-line prompt), rendered on the page and bound
deterministically to "yes". Alex, thinking aloud: the response format could
carry questions or offers that the HUD renders as one click to start a task.

### Per-project trust flag

Alex: simpler as an `agents.verify` entry that runs after every job is done,
before Scufris gives a verdict; a full work-review-land workflow was too
much. Replaced. `[agents.verify]`: bounded, read-only, runs after `done`,
reads the diff, the report, and the receipt, answers one envelope the
foreground quotes.

### Evidence-bearing done reports

Alex: wants citations in the final message; widgets are meant as citations
that support claims, like RAG; does not want an agent IDE. Folded into
receipts plus a typed `receipts` field in the final response (beside prose,
details, attachments, widgets: `response.ts:131-166`), rendered as a facts
row in the HUD. Alex wants more such fields over time.

### Yesterday's jobs in the briefing

Alex: briefings are an engine that builds a page; should be configurable;
jobs stats for scufris2 would be nice; per-project jobs would need each toml
to opt in. Decision: built-in sources that do not need a project file
(`briefing_sources` reads only project tomls today,
`tools/jobs/scufris-jobs:623-660`); a global jobs source; per-project job
stats as an optional toml flag.

### Receipts, the verify step, and the HUD

Alex: how do a verify step and receipts fit, and how to show them in the HUD
without turning it into Cursor. Two layers. Receipts are measured facts.
The verify agent consumes them; without receipts it reads the same prose or
re-runs git non-deterministically. HUD: one facts row under the prose, no
diff viewer.

### Worker boundary and landing

Alex: workers can still run these commands; wants the worker to be able to
land; sometimes just wants changes landed, and otherwise a clear "not
landed" beats more commands and beats a deleted branch with lost work.
Facts: the worker prompt (`tools/jobs/scufris-jobs:731-800`) says nothing
about landing or pushing; `scufris_job_land` is foreground-only and "never
runs automatically" (`orchestration.ts:1115-1118`); seedzero workers already
push. Decision: no new worker commands. `sprout rm` refuses an unmerged
branch without a force flag, so work is never lost. The foreground says "not
landed" plainly when the receipt says so.

### The four-widget limit

Alex: the desktop opens no more than four widgets from the tray. Found. The
desktop runtime is a slot-based window manager: exhibits share a shelf of
three (`surfaces/desktop/src/widgets/runtime.rs:28`), pinned instruments
take one of four edge slots, top right, top left, bottom right, bottom left
(`runtime.rs:36-41`). A fifth pin fails with "every instrument slot is
taken" (`runtime.rs:572-573`, `:944`). By design, not a fault. Changing it
means a new slot geometry, for example stacking two per edge.

### Revised order

1. Landing receipts. Unchanged. Everything below consumes them.
2. Scheduler: profiles on a schedule, built-in sources, offers.
3. Verify agent, `[agents.verify]`, quoting receipts.
4. HUD: `receipts` and `offers` response fields, jobs rows in the
   conversation window.
   Dropped: named routines, per-project trust flag, jobs widget as first UI.
