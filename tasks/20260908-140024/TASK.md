# Run briefing profiles on systemd timers with profile-keyed run directories

- STATUS: OPEN
- PRIORITY: 95
- TAGS: workflow

## Goal

More than one briefing profile runs on its own schedule, each keyed by its own
run directory, and the schedule is owned by systemd rather than by arithmetic
in the extension. A profile missed because the machine was off is caught up
once, by `Persistent=true`, not by a hand-written rule.

Origin: task 20260908-103403, split. Depends on task 20260908-135852 for the
wake.

## Facts

- One env var, one timer, one profile. `SCUFRIS_BRIEFING_TIME` is `HH:MM` or
  `off` (`agent/extensions/scufris/briefing/schedule.ts:31-39`),
  `nix/launcher.nix:44-48` exports it, and nothing ever sets
  `SCUFRIS_BRIEFING_PROFILE`, so `profile()` is always `morning`
  (`briefing.ts:108`).
- Run directories are `briefings/<date>/` with no profile
  (`tools/briefing/briefing.py:122-123`). `--profile` reaches only `collect`;
  `state`, `show`, `publish`, `render`, `open`, and `path` key on the date
  alone (`tools/briefing/cli.py:56-75`). Two profiles on one date collide.
- `cron` does not fire a missed job. A systemd timer with `Persistent=true`
  does: `man 5 systemd.timer` says the unit "is triggered immediately if it
  would have been triggered at least once during the time when the timer was
  inactive".
- systemd does not accept crontab syntax. `systemd-analyze calendar
"0 7 * * *"` fails; `OnCalendar` takes `07:30`, `Mon *-*-* 09:00`,
  `Mon..Fri 23:00`, and `systemd-analyze calendar` validates a spec.
- Home Manager already renders this shape on this host: `nix-gc.timer` is a
  user timer with `OnCalendar=weekly` and `Persistent=true`, stamped at
  `~/.local/share/systemd/timers/stamp-nix-gc.timer`.
- `loginctl show-user alex` reports `Linger=no`, so the user manager runs from
  login to logout. Catch-up fires at login, which is where Scufris lives.
- The module already declares user services
  (`nix/home-manager.nix:379,422,477,519`), so a timer per profile is the same
  pattern.

## Direction

- Replace `briefing.time` with `briefing.profiles`: an attribute set of
  profile name to a schedule and its options, for example
  `morning = { schedule = "07:30"; }` and
  `weekly = { schedule = "Mon 09:00"; }`. Validate each spec with
  `systemd-analyze calendar` at build time, so a bad schedule fails the build
  rather than the morning. `briefing.time` is removed with a message, not
  renamed: the type changes.
- Render one `scufris-briefing@.timer` and `scufris-briefing@.service` pair
  per profile, `Persistent=true`, `Type=oneshot`, with a start timeout above
  the run deadline. The unit runs `scufris-briefing collect --profile <name>`
  and then wakes the foreground with the ingress from task 20260908-135852.
- Key run directories by date and profile: `briefings/<date>/<profile>/`.
  Every subcommand takes `--profile`. No migration: `prune()` sweeps by date,
  so single-profile runs age out on their own.
- Cut `schedule.ts` down to what systemd does not do. What stays is one
  session-start read: is there a `collected` run with no prose, for any
  profile? That case is real, because the agent can be down when a collection
  finishes. It is a file read, not a timer, and not polling.
- The wake names its profile, and the `show`, `publish`, and `open` tools take
  a profile. Without one, the helper resolves to the newest collected and
  undelivered run for the date, so a wake can never publish into the wrong
  profile's run.
- A collection whose wake is refused leaves the run `collected`. The
  session-start read is the fallback, so a briefing gathered while the agent
  was down is still written up.

## Verification

- Test: two profiles on one date produce two run directories and two
  deliveries.
- Test: publish without a profile resolves to the collected, undelivered run
  and never to a delivered one.
- Test: a session that starts with a `collected` run and no prose wakes once
  for it, and a session that starts with a `delivered` run wakes for nothing.
- Test: an invalid `OnCalendar` spec fails the Nix build.
- One staging run with a second profile on the same date.
- `npm run check`, Python unit tests, and `nix flake check`.
