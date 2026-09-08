# Notes

## What was built

Nix owns when a briefing happens; each project's `.scufris.toml` owns what is
in it. `programs.scufris.agent.briefing.profiles` is an attribute set of
profile name to `{ schedule, persistent, deadline }`. Nothing in it describes
briefing content.

- `nix/briefing-unit.nix` builds one runner per profile. Its build runs
  `systemd-analyze calendar` on the profile's schedule, so the runner cannot
  exist unless the schedule is one. The script collects that profile and then
  runs `scufris-briefing wake --profile <name>`.
- `nix/home-manager.nix` renders `scufris-briefing-<profile>.timer`
  (`OnCalendar`, `Persistent`) and `.service` (`Type=oneshot`,
  `TimeoutStartSec = deadline + 300`) for each profile.
  `agent.briefing.time` is removed with a message. `nix/launcher.nix` no
  longer exports `SCUFRIS_BRIEFING_TIME`.
- Run directories are `briefings/<date>/<profile>/`. Every reader and writer
  takes a profile: `run_dir`, `read_manifest`, `write_manifest`, `run_state`,
  `read_run`, `publish`, `render`, `delivered`, and the contribution and page
  paths under them. `prune` still sweeps by date, so a date takes its profiles
  with it and old single-profile runs age out unmigrated.
- `agent/extensions/scufris/schedule.ts` is now `run.ts` and holds `localDate`
  and `RunState`. `parseSchedule`, `untilTomorrow` and `decide` are gone with
  the timer. The extension holds no clock: `session_start` reads
  `scufris-briefing pending` once and asks for the writing of each run that is
  waiting. There is no interval.

## Decisions the task did not settle

- **Concrete units, not a template.** The task asked for
  `scufris-briefing@.timer`. Every profile carries its own `OnCalendar`, which
  a template timer cannot express without a per-instance drop-in, and Home
  Manager has no way to enable an instance of a template it declares. One
  concrete `scufris-briefing-<profile>` pair per profile is the same shape
  with none of that.
- **The wake message moved to Python.** The timer reaches the conversation
  through `scufris-ctl` and a tool call reaches it in process. Both must ask
  for the writing in the same words, so `briefing.wake_message` is the one
  owner and the extension reads it back from `pending`. The TypeScript
  `wakeMessage` is gone.
- **Two new helper verbs.** `wake` carries a gathered run to the conversation
  and reports rather than fails when nothing is listening; `pending` lists the
  runs for a day that still need their prose, with the words to ask for each.
  `pending` is the session-start read and the run tool's wake, so the two
  paths cannot disagree about what is waiting.
- **Naming no profile refuses rather than guesses.** The first staging run
  found the real hazard: `finished` has second resolution, two profiles
  collected in the same minute tie, and "the newest waiting run" published the
  evening's prose onto the morning's page. `resolve` now answers only when
  there is exactly one obvious run and otherwise refuses by name. `publish`
  still resolves only to a run that was gathered and never written up.
- **`SCUFRIS_BRIEFING_PROFILE` is removed.** Nothing set it and the unit names
  the profile on its command line.
- **The unit's PATH is explicit.** A user manager does not inherit the login
  shell, so the runner puts the helper, `scufris-ctl`, the pinned `pi`, `git`
  and `python3` on `PATH`, then the user profile and the system path.
- **Timers are Linux-only.** The block is gated on `isLinux`, because
  `profiles` has a non-empty default and a platform assertion would break
  every darwin user who only set `enable = true`.

## Verification

`npm run check`

```
> scufris2@2.1.7 version:check
product 2.1.7; surface protocol 6
> scufris2@2.1.7 typecheck
> tsc --noEmit
> scufris2@2.1.7 test
ℹ tests 100
ℹ pass 100
ℹ fail 0
> scufris2@2.1.7 format:check
Checking formatting...
All matched files use Prettier code style!
```

`python3 -m unittest discover -s tests -p 'test_*.py'`

```
Ran 303 tests in 48.369s

OK
```

`nix flake check`

```
running 43 flake checks...
all checks passed!
warning: The check omitted these incompatible systems: aarch64-darwin, aarch64-linux
```

`cargo test` was not run: no Rust changed.

### A schedule that is not one fails the build

```
$ nix build ... agent.briefing.profiles.morning.schedule = "0 7 * * *"
error: Cannot build '/nix/store/...-scufris-briefing-morning.drv'.
       Reason: builder failed with exit code 1.
$ nix log /nix/store/...-scufris-briefing-morning.drv
Failed to parse calendar specification '0 7 * * *': Invalid argument
programs.scufris.agent.briefing.profiles.morning.schedule is not a systemd
OnCalendar specification: 0 7 * * *
```

### The rendered units

```
[Timer]
OnCalendar=07:30
Persistent=true

[Service]
ExecStart=/nix/store/...-scufris-briefing-weekly/bin/scufris-briefing-weekly
TimeoutStartSec=3900
Type=oneshot
```

### One run with two profiles on one date

Against the built `scufris-briefing`, a temporary state directory, one project
declaring `[briefings.morning]` and `[briefings.evening]`, and a scripted
harness and control client:

```
== two profiles collected on one date ==
  2026-09-08 morning collected
    [ok] projects/the-den: The morning is clear.
  2026-09-08 evening collected
    [ok] projects/the-den: The evening is clear.
  2026-09-08/evening/briefing.html
  2026-09-08/evening/contributions
  2026-09-08/evening/manifest.json
  2026-09-08/morning/briefing.html
  2026-09-08/morning/contributions
  2026-09-08/morning/manifest.json
== both are waiting, and an unnamed publish refuses ==
  evening 1
  morning 1
  scufris-briefing: name one of the briefings for 2026-09-08 with --profile: evening, morning
== a refused wake leaves the run gathered ==
  nothing was woken: scufris-ctl: agent_unavailable: no agent is connected
  state: collected
== a wake that lands names its own profile ==
  woke the conversation for the evening briefing
  argv: wake --custom-type scufris-briefing --details
  details: {"date": "2026-09-08", "profile": "evening", "sources": 1}
  text: The evening briefing for 2026-09-08 is collected: 1 source answered. Read it with scufris_briefi
== named publishes keep their own pages ==
  morning page: The morning was quiet.
  evening page: The evening was quiet.
  no prose crossed between the two runs
== nothing is left waiting, and an unnamed publish says so ==
  {"date":"2026-09-08","runs":[]}
  scufris-briefing: no gathered briefing for 2026-09-08 is waiting to be written
```

## What is left

- Nothing here starts the timers on this host. `home-manager switch` does that,
  and the first `Persistent` catch-up fires at the next login.
- The `deadline` option sets `SCUFRIS_BRIEFING_DEADLINE` for the run and the
  unit's start timeout. `SCUFRIS_BRIEFING_SOURCE_DEADLINE` is still only an
  environment variable; no profile option covers it, because one profile
  bounding its own sources has not been asked for.
