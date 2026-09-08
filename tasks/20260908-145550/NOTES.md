# Notes

## What was built

A briefing source can now be declared for the machine instead of for a
project. It is an ordinary source: same entry shape, same reader, same
envelope, same deadlines, same repair, same page.

- **The reader** (`tools/jobs/scufris-jobs`). `user_briefing_sources()` reads
  `$XDG_CONFIG_HOME/scufris/config.toml` and parses
  `[briefings.<profile>.<name>]`. It reuses `briefing_entry()`, which now takes
  an `extra` key set so a user entry's `root` is validated by the same code
  that validates a project entry. `briefing_sources()` returns the machine's
  sources first, then the projects'. The `briefings` request takes an optional
  `config`.
- **Slugs are the reader's.** Every source carries `slug`, and `stamp()` reads
  it instead of deriving one. `project_root` is now `root`.
- **The consumer** (`tools/briefing/briefing.py`). `declared_sources` takes the
  config path, `ask()` runs in `source["root"]`, and `collect` passes the path
  through. `cli.py` gains `--config` on every subcommand.
- **The watermark.** `previous_finished(date, profile)` reads back over the
  kept runs for the newest finished run of that profile, and every source's
  prompt carries it under a `## Since` heading. Computed once per run, before
  the run writes its own manifest over the last one.
- **The Home Manager option.**
  `programs.scufris.agent.briefing.sources.<profile>.<name>`, a sibling of
  `profiles`, rendered with `pkgs.formats.toml` into
  `xdg.configFile."scufris/config.toml"`.
- **The archived-job listing.** `history` in the jobs helper, over both
  `jobs_root()` and `archive_root()`, records since a timestamp with their
  newest receipt. `scufris-jobs history --since <moment>` surfaces it.

## Decisions the task and plan did not settle

- **`@` is what makes the collision inexpressible.** A machine source is
  named `@<name>` and that is also its slug. `PROJECT_ID` is
  `[A-Za-z0-9_.-]` and slashes, so no project ID can start with `@` and no
  project slug can be one. A section named `projects-the-den` therefore cannot
  take `projects/the-den`'s contribution file, which a plain `user-` prefix
  would not have prevented (a project at `user/the-den` slugs to
  `user-the-den`). `briefing.py` also holds the reader's slug to one path
  component before it names a file, so the file write cannot escape the
  contributions directory even if the reader changes.
- **The identity key stays `project`.** The manifest, the page, and the
  extension all read `project`. Renaming it would have rippled into the page
  renderer and the run records for no gain, so a machine source's `project` is
  `@jobs` and the page names it that way.
- **The user file is read with `read_bytes()`, not the hardened artifact
  reader.** Home Manager writes it as a symlink into the store. A reader that
  refused a symlink would refuse the ordinary way of having one.
- **`SCUFRIS_CONFIG` is resolved in the jobs helper, not in `briefing.py`.**
  The reader owns the file, so it owns where the file comes from. `--config`
  wins because the CLI passes it as an explicit `config` in the request and
  the helper prefers the request over the variable.
- **A malformed user file costs the whole file**, one diagnostic naming it,
  exactly as specified. See the concerns below.
- **`root` is type-checked but not stat'd.** The reader stays a parser. A root
  that does not exist becomes a failed contribution naming the harness error,
  not a config diagnostic that would blank every other machine source.
- **`history` tolerates an unreadable record.** `all_job_records()` raises on
  one; a briefing source is reading a morning, not repairing state, so
  `every_job_record()` skips what it cannot load. It still refuses an unsafe
  job directory. A record whose own timestamps cannot be parsed is listed
  rather than hidden, and a `since` that is not a moment is refused rather
  than ignored.
- **The Nix option declares four keys over a freeform type.** `description`,
  `guidance`, `keywords` (flat scalars or lists of them, matching the
  reader's rule) and `root`. Nulls and empty tables are stripped before
  rendering, because TOML has no null and an absent key is what the reader
  reads as "not set".

## The jobs source, as guidance

Build step 6 asks for the jobs source written as guidance rather than code.
This is the text to paste. It is not written into
`~/.config/scufris/config.toml`, because Home Manager generates that file from
your own dotfiles repository, which is outside this checkout.

As a Home Manager option:

```nix
programs.scufris.agent.briefing.sources.morning.jobs = {
  description = "What Scufris did overnight.";
  keywords = {
    harness = "pi";
    model = "openai-codex/gpt-5.6-sol";
    thinking = "medium";
  };
  guidance = ''
    Report what Scufris did since the moment named above. Read only; change
    nothing, and start no job.

    - Run `~/personal/scufris2/scripts/scufris-jobs history --since <that
      moment> --json`. It lists archived jobs as well as live ones, so it is
      the only listing that can answer what landed. With no previous run
      named, drop `--since` and report the whole listing.
    - Put the jobs in a Markdown table: job ID, project, workspace, state, and
      what the receipt says. One row for each job, newest last.

    The receipt is measured and you are not. Quote its fields verbatim and do
    not judge whether any work is done:

    - Copy each string in `receipt.sentences` into the row exactly as it is
      written. Say "not landed" in those words, and say "claimed, not
      verified" in those words.
    - A `facts` value of `false` was measured and is false. A value of `null`
      was not measured; say it is unknown and give the reason from
      `unavailable`. Never report an unmeasured fact as a no.
    - A job with no receipt has none. Say so rather than inferring one from
      its state or summary.

    Facts are counts: jobs in the window, jobs that landed, jobs with an
    unverified claim. Set `status` to `attention` when any receipt carries a
    sentence, because an unverified claim is the owner's to settle. A window
    with no job at all is `status = "ok"` and one short line.
  '';
};
```

As the TOML that renders to, for a machine without Home Manager:

```toml
[briefings.morning.jobs]
description = "What Scufris did overnight."
guidance = """
...the same prose...
"""

[briefings.morning.jobs.keywords]
harness = "pi"
model = "openai-codex/gpt-5.6-sol"
thinking = "medium"
```

The path in the first bullet is the one real gap; see the concerns below.

## What is wrong with the design as specified

Reported rather than worked around.

1. **`scufris-jobs history` is not callable by name.** Build step 5 says the
   listing is "surfaced in `scripts/scufris-jobs` so a source can call it", and
   it is - but that script is a resource file
   (`$resources/share/scufris/scripts/scufris-jobs`), not an installed binary.
   Nothing puts it on `PATH`: the launcher's `runtimeInputs` are `python3`,
   `tmux`, `scufris-den` and `scufris-briefing`, and `nix/briefing-unit.nix`
   builds the timer's `PATH` from the same list plus `git` and `scufris-ctl`.
   So the guidance above has to name a checkout path. That works on your
   machine and nowhere else, and it breaks if the checkout moves. The fix is a
   `scufris-jobs` package on the unit's `PATH`, which is a real addition and
   was not in the plan, so I did not make it. Worth a follow-up before the
   jobs source is relied on.

2. **The watermark competes with guidance that names its own window.** Every
   source you have written today hard-codes one: the-den's `--days 3` and
   `--date <yesterday>`, nova-protocol's and content-machine's
   `git log --oneline -12` "for what landed yesterday", nova-protocol's
   `gh run list --limit 8`. If the watermark were phrased as an instruction,
   a machine that was off for a week would contradict all of them, and the
   guidance is what names the commands. I phrased it as a fact with an
   explicit precedence rule - "Where the guidance below asks what changed and
   names no window of its own, that is the moment to measure from. Where it
   names its own window, keep it." - so nothing in your files reads oddly now.
   But the tension is real and will bite the first source that both says
   "since the last briefing" and passes a fixed `--days`. Nothing in the task
   named it.

3. **The window is the previous run's `finished`, so the collection itself is
   a gap.** A job that finished while yesterday's run was collecting falls
   between that run's `started` and `finished` and is reported by neither
   morning. At the default 1800 second deadline that gap can be half an hour.
   Using `started` would double-report instead, which is the safer error for a
   briefing. The task and the plan both say `finished`, so `finished` is what
   I built.

4. **The watermark depends on the 30-run prune.** `previous_finished` reads
   the runs that are still on disk. Thirty kept dates is about a month of
   mornings, so a `monthly` profile's previous run is often already swept and
   the source is told there was none. A profile-scoped watermark file would be
   independent of the prune. Fine at a morning and weekly cadence; not fine
   beyond it.

5. **A malformed user file costs every machine source, not just the bad
   entry.** That is what "one diagnostic naming the file" means and it is what
   I built, but the asymmetry with projects will show: a project's malformed
   briefing costs that project only, while a typo in one of five machine
   sources loses all five. It is the right trade at one source and the wrong
   one at five.

6. **`history` reads every record on disk before filtering.** `since` is
   applied after `load_job` and `stored_receipts`, so an early-morning source
   pays for the whole archive. Bounded only on output (`MAX_HISTORY = 200`).
   Fine at the current archive size; O(all jobs) per call by construction.

## Verification

`python3 -m unittest discover -s tests -p 'test_*.py'`

```
Ran 317 tests in 37.203s

OK
```

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

`nix flake check`. 26 checks, six of them the briefing's:
`briefing-no-schedule`, `briefing-no-sources`, `briefing-schedule-is-a-calendar`,
`briefing-sources-are-typed`, `briefing-sources-file`, `briefing-timers`. The
run that first built the three new ones:

```
running 7 flake checks...
building '/nix/store/9fjyffhw5f33dr6kpk3gw248vfc816kn-scufris-briefing-source-types-check.drv'...
building '/nix/store/i0kq54bqhg8hqr8mb1xbg5l1ky9c3rd5-scufris-briefing-no-sources-check.drv'...
building '/nix/store/dg36kvww3f5m9qsi0j8l7zlvz3qzaz6l-scufris-config.toml.drv'...
building '/nix/store/i08iyd0gq83yqll2ww25g2gplwkx8yll-scufris-helper-tests.drv'...
building '/nix/store/jqkal7iiwrd4cnixancw1p8hls4r51f2-home-manager-files.drv'...
building '/nix/store/67f4rrig0mij42y9mh5siyz6w0s51m17-home-manager-generation.drv'...
building '/nix/store/q0gs14b5baj72a9xr7wgq0yf93bq8im9-scufris-briefing-sources-check.drv'...
all checks passed!
warning: The check omitted these incompatible systems: aarch64-darwin, aarch64-linux
```

and the last one, with everything already built:

```
running 0 flake checks...
all checks passed!
warning: The check omitted these incompatible systems: aarch64-darwin, aarch64-linux
```

`alejandra --check nix/ flake.nix`

```
Checking style in 25 files using 16 threads.
Congratulations! Your code complies with the Alejandra style.
```

`cargo test` was not run: no Rust changed.

### The generated file

`programs.scufris.agent.briefing.sources` with two morning sources and one
weekly, built through the Home Manager module:

```toml
[briefings.morning.den]
description = "Report the journal."
guidance = "Read it."
root = "/home/t/personal/the-den"

[briefings.morning.jobs]
description = "What Scufris did overnight."
guidance = """
Read what the helper measured.
"""

[briefings.morning.jobs.keywords]
harness = "pi"
thinking = "medium"

[briefings.weekly.jobs]
description = "The week."
guidance = "Read the week."
```

The helper reads that exact file back into two morning sources, `@den` at its
declared root and `@jobs` at the home directory.

### A malformed entry fails the build

`briefing-sources-are-typed` renders each of these and asserts which ones can
be built at all:

| entry                             | renders |
| --------------------------------- | ------- |
| description and guidance          | yes     |
| description, no guidance          | no      |
| `keywords.model = { nested = 1 }` | no      |
| `root = 12`                       | no      |

### One real morning, end to end

A temporary state directory, one project declaring `[briefings.morning]`, a
user file declaring `jobs` and a second section named `projects-the-den`
(exactly the project's slug), two job records - one live, one archived with a
receipt saying the work was not landed and the push was claimed, not verified -
and a harness that really runs `scufris-jobs history` with the moment from its
own prompt:

```
== first morning: no previous run ==
2026-09-08 morning collected
  [attention] @jobs: 2 jobs since the last briefing.
  [ok] @projects-the-den: Nothing to report.
  [ok] projects/the-den: Nothing to report.

== contribution files, first morning ==
  @jobs.json
  @projects-the-den.json
  projects-the-den.json

  prompt says: No earlier morning briefing was kept on this machine, so there
  is no previous run to measure against. Report where things stand now, and do
  not invent a period you cannot measure.

== second morning ==
  prompt says: The last morning briefing finished at 2026-09-08T15:29:17+03:00.
  Where the guidance below asks what changed and names no window of its own,
  that is the moment to measure from. Where it names its own window, keep it.

== what the jobs source reported ==
2 jobs since the last briefing.
| Job | State | Receipt |
| --- | --- | --- |
| bb0000000002 | done | not landed; pushed: claimed, not verified |
| aa0000000001 | working | nothing to flag |

== the page carries the receipt's own words ==
  'claimed, not verified' on the page: True
  'not landed' on the page: True
  @jobs named on the page: True
```

Three sources, three distinct contribution files, and no slug taken from
another. The archived job `bb0000000002` is one the live listing cannot see:
`scufris-jobs all --json` returned only `aa0000000001`.

## What is left

- Nothing here writes `~/.config/scufris/config.toml`. Add
  `programs.scufris.agent.briefing.sources.morning.jobs` to your own dotfiles
  with the guidance above, then `home-manager switch`.
- The `scufris-jobs` path in that guidance is a checkout path. See concern 1.
- `SCUFRIS_PROJECT_ROOTS` and the briefing deadlines still live outside this
  file. That was named out of scope in the plan and is the natural follow-on.
