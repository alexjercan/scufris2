# The schedule is systemd's. Each profile gets its own timer and its own run,
# and a schedule that is not one fails the build. The machine's own sources are
# generated from a typed option, so a malformed entry fails the build too.
{
  pkgs,
  scufris,
  homes,
  ...
}: let
  inherit (pkgs) lib;
  inherit (homes) mkHome;
  declared = mkHome {
    settings.agent.briefing.sources = {
      morning.jobs = {
        description = "What Scufris did overnight.";
        keywords = {
          harness = "pi";
          thinking = "medium";
        };
        guidance = "Read what the helper measured.";
      };
      morning.den = {
        description = "Report the journal.";
        guidance = "Read the journal.";
        root = "/home/scufris-test/personal/the-den";
      };
      weekly.jobs = {
        description = "Report the week.";
        guidance = "Read the week.";
      };
    };
  };
  configured = "${declared.activationPackage}/home-files/.config/scufris/config.toml";
  bare = "${(mkHome {}).activationPackage}/home-files/.config";
  # Whether one set of sources can be rendered at all. The reason to generate
  # the file is that a mistake in it fails the build rather than the morning,
  # so a malformed entry must be caught here and not by the reader at dawn.
  renders = sources:
    (builtins.tryEval (builtins.seq
      (mkHome {settings.agent.briefing.sources = sources;})
      .config.xdg.configFile."scufris/config.toml".source
      true))
    .success;
  scheduled = mkHome {
    settings.agent.briefing.profiles = {
      morning.schedule = "07:30";
      weekly = {
        schedule = "Mon *-*-* 09:00";
        persistent = false;
        deadline = 3600;
      };
    };
  };
  quiet = mkHome {settings.agent.briefing.profiles = {};};
  units = "${scheduled.activationPackage}/home-files/.config/systemd/user";
  none = "${quiet.activationPackage}/home-files/.config/systemd/user";
  bounds = "${scheduled.activationPackage}/home-files/.config/scufris/briefing-profiles.json";
in
  {
    # `[briefings.<profile>.<name>]` is what the helper reads, and it reads a
    # path: Home Manager is one way to produce this file and hand-writing it is
    # another.
    briefing-sources-file = pkgs.runCommand "scufris-briefing-sources-check" {} ''
      grep -Fx '[briefings.morning.jobs]' ${configured}
      grep -Fx '[briefings.morning.den]' ${configured}
      grep -Fx '[briefings.weekly.jobs]' ${configured}
      grep -Fx 'description = "What Scufris did overnight."' ${configured}
      grep -Fx 'root = "/home/scufris-test/personal/the-den"' ${configured}
      grep -Fx '[briefings.morning.jobs.keywords]' ${configured}
      grep -Fx 'harness = "pi"' ${configured}
      # A source that named no root says nothing about one. TOML has no null,
      # and the reader answers an absent root with the home directory.
      ! grep -F 'root =' ${configured} | grep -Fv 'the-den'
      # Briefings and nothing else. Agents and conventions stay in the project
      # that declares them.
      ! grep -F '[agents' ${configured}
      touch "$out"
    '';

    # Declaring none writes no file, and the helper reads a machine with none
    # as a machine that declared none rather than as a mistake.
    briefing-no-sources = pkgs.runCommand "scufris-briefing-no-sources-check" {} ''
      ! test -e ${bare}/scufris/config.toml
      touch "$out"
    '';

    # A deployed Home Manager file is one final link into the immutable store.
    # This is the only configuration link the reader follows.
    briefing-generated-symlink =
      pkgs.runCommand "scufris-briefing-generated-symlink-check" {
        nativeBuildInputs = [pkgs.jq];
      } ''
        mkdir -p home config/scufris
        target="$(readlink -f ${configured})"
        case "$target" in /nix/store/*) ;; *) exit 1 ;; esac
        ln -s "$target" config/scufris/config.toml
        HOME="$PWD/home" XDG_CONFIG_HOME="$PWD/config" SCUFRIS_PROJECT_ROOTS='[]' \
          ${lib.getExe scufris.briefing} sources --profile morning --json > answer.json
        jq -e '.sources | any(.project == "@jobs")' answer.json > /dev/null
        touch "$out"
      '';

    briefing-sources-are-typed = assert renders {
      morning.jobs = {
        description = "Report it.";
        guidance = "Read it.";
      };
    };
    # Guidance is what a source is asked, so a source without one is not one.
    assert !(renders {morning.jobs.description = "Report it.";});
    # A keyword is given back to the source verbatim, so it stays flat.
    assert !(renders {
      morning.jobs = {
        description = "Report it.";
        guidance = "Read it.";
        keywords.model = {nested = 1;};
      };
    });
    # A root is one path.
    assert !(renders {
      morning.jobs = {
        description = "Report it.";
        guidance = "Read it.";
        root = 12;
      };
    });
      pkgs.runCommand "scufris-briefing-source-types-check" {} ''touch "$out"'';
  }
  // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    # A schedule is validated with the same command the build uses. systemd
    # accepts none of crontab's syntax, which is the mistake a person carrying
    # a cron job over makes first.
    briefing-schedule-is-a-calendar =
      pkgs.runCommand "scufris-briefing-schedule-check" {
        nativeBuildInputs = [pkgs.systemd];
      } ''
        for good in "07:30" "Mon *-*-* 09:00" "Mon..Fri 23:00" "weekly"; do
          systemd-analyze calendar "$good" > /dev/null
        done
        for bad in "0 7 * * *" "@daily" "every morning" "25:00"; do
          if systemd-analyze calendar "$bad" > /dev/null 2>&1; then
            echo "systemd-analyze accepted $bad, so a bad schedule would reach the timer" >&2
            exit 1
          fi
        done
        touch "$out"
      '';

    # Two profiles are two timers and two services, each collecting its own
    # briefing. One date holding two profiles is the case that used to
    # collide.
    briefing-timers = pkgs.runCommand "scufris-briefing-timers-check" {} ''
      for name in morning weekly; do
        test -f ${units}/scufris-briefing-$name.timer
        test -f ${units}/scufris-briefing-$name.service
        test -f ${units}/scufris-briefing-$name-failure.service
        grep -Fx "OnFailure=scufris-briefing-$name-failure.service" ${units}/scufris-briefing-$name.service
      done
      test -f ${units}/scufris-briefing-reconcile.service
      test -f ${units}/scufris-briefing-reconcile.timer
      grep -Fx 'OnBootSec=2m' ${units}/scufris-briefing-reconcile.timer
      grep -Fx 'OnUnitActiveSec=1m' ${units}/scufris-briefing-reconcile.timer
      grep -Fx 'Persistent=true' ${units}/scufris-briefing-reconcile.timer
      grep -Fx 'OnCalendar=07:30' ${units}/scufris-briefing-morning.timer
      grep -Fx 'OnCalendar=Mon *-*-* 09:00' ${units}/scufris-briefing-weekly.timer
      grep -Fx 'Persistent=true' ${units}/scufris-briefing-morning.timer
      grep -Fx 'Persistent=false' ${units}/scufris-briefing-weekly.timer
      grep -Fx 'WantedBy=timers.target' ${units}/scufris-briefing-morning.timer
      grep -Fx 'Type=oneshot' ${units}/scufris-briefing-morning.service
      for unit in \
        scufris-briefing-morning.service \
        scufris-briefing-weekly.service \
        scufris-briefing-morning-failure.service \
        scufris-briefing-weekly-failure.service \
        scufris-briefing-reconcile.service; do
        grep -Fx 'MemoryHigh=3G' "${units}/$unit"
        grep -Fx 'MemoryMax=4G' "${units}/$unit"
      done
      # The unit outlives the run deadline the collection holds itself to.
      grep -Fx 'TimeoutStartSec=2100' ${units}/scufris-briefing-morning.service
      grep -Fx 'TimeoutStartSec=3900' ${units}/scufris-briefing-weekly.service
      # Each profile records a generation before collection. Its external
      # failure unit passes that exact ID back to the finalizer.
      runner="$(sed -n 's/^ExecStart=//p' ${units}/scufris-briefing-weekly.service)"
      grep -F -- "--profile weekly" "$runner"
      grep -F -- "collect --profile weekly" "$runner"
      grep -F -- '--generation "$generation"' "$runner"
      grep -F -- 'briefing-weekly.generation' "$runner"
      grep -F -- "wake --profile weekly" "$runner"
      ! grep -F -- "--profile morning" "$runner"
      finalizer="$(sed -n 's/^ExecStart=//p' ${units}/scufris-briefing-weekly-failure.service)"
      grep -F -- 'briefing-weekly.generation' "$finalizer"
      grep -F -- '--generation "$generation"' "$finalizer"
      grep -F -- '--property=Result' "$finalizer"
      reconciler="$(sed -n 's/^ExecStart=//p' ${units}/scufris-briefing-reconcile.service)"
      grep -F -- 'SCUFRIS_BRIEFING_KEEP_DAYS=30' "$reconciler"
      grep -F -- 'briefing reconcile --json' "$reconciler"

      # Machine sources report on jobs and the den, so both readers are
      # programs on the run's own PATH. A user manager inherits no login shell,
      # and guidance that relied on a profile would work in the foreground but
      # not from the schedule.
      grep -E '^export PATH=' "$runner" | grep -F ${scufris.jobs}/bin
      grep -E '^export PATH=' "$runner" | grep -F ${scufris.den}/bin
      touch "$out"
    '';

    # The same numbers, where a run started any other way can read them. Only
    # the timer's environment carried them before, so a briefing asked for by
    # hand was held to the built-in defaults and never said so.
    briefing-profile-bounds =
      pkgs.runCommand "scufris-briefing-bounds-check" {
        nativeBuildInputs = [pkgs.jq];
      } ''
        test "$(jq -r '.morning.deadline' ${bounds})" = 1800
        test "$(jq -r '.weekly.deadline' ${bounds})" = 3600
        test "$(jq -r '.morning.source_deadline' ${bounds})" = 900
        test "$(jq -r '.morning.parallel' ${bounds})" = null
        test "$(jq -r '.morning.max_offers' ${bounds})" = 3
        test "$(jq -r '.morning.max_body' ${bounds})" = 16384
        test "$(jq -r '.morning.keep_days' ${bounds})" = 30
        # Every profile that has a timer has its numbers here, and nothing else
        # does.
        test "$(jq -r 'keys | join(",")' ${bounds})" = "morning,weekly"
        # A machine that schedules nothing writes no file, and the helper reads
        # that as a machine held to its defaults.
        ! test -e ${quiet.activationPackage}/home-files/.config/scufris/briefing-profiles.json
        touch "$out"
      '';

    # No profile is no timer, and the tools still work. The schedule costs a
    # deployment that wants none exactly nothing.
    briefing-no-schedule = pkgs.runCommand "scufris-briefing-no-schedule-check" {} ''
      ! test -e ${none}/scufris-briefing-morning.timer
      ! ls ${none} 2>/dev/null | grep -F scufris-briefing
      touch "$out"
    '';
  }
