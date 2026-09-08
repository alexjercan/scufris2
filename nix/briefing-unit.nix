# One profile's briefing, as its systemd timer starts it.
#
# Nix owns when a briefing happens; each project's own `.scufris.toml` owns
# what is in it. This is the whole of "when": a schedule that has to be a
# schedule, one collection, and one wake carried to the foreground.
{
  pkgs,
  name,
  profile,
  briefing,
  jobs,
  ctl,
  pi,
  projectRoots,
}: let
  inherit (pkgs) lib;
  quoted = lib.escapeShellArg name;
  # A user manager does not inherit the login shell, so everything the run
  # reaches for by name is put here: the helper, the job reader a machine
  # source reports from, the control client the wake is carried by, the harness
  # a source runs under, and `git` and `python3`, which the project reader
  # needs and a source's own guidance often does.
  # The user profile and the system path come last, so a deployment that pins
  # its own `pi` or ships `claude` keeps working without this knowing.
  binaries = lib.makeBinPath [briefing jobs ctl pi pkgs.git pkgs.python3];
in
  pkgs.runCommand "scufris-briefing-${name}" {
    nativeBuildInputs = [pkgs.systemd];
    inherit (profile) schedule;
    passAsFile = ["script"];
    script = ''
      #!${pkgs.runtimeShell}
      set -euo pipefail
      export PATH=${binaries}:"$HOME/.nix-profile/bin":/run/current-system/sw/bin:"''${PATH-}"
      if [[ -z "''${SCUFRIS_PROJECT_ROOTS+x}" ]]; then
        export SCUFRIS_PROJECT_ROOTS=${lib.escapeShellArg (builtins.toJSON projectRoots)}
      fi
      if [[ -z "''${SCUFRIS_BRIEFING_DEADLINE+x}" ]]; then
        export SCUFRIS_BRIEFING_DEADLINE=${toString profile.deadline}
      fi
      # The run on disk is the durable half. It is written before anything is
      # said, so a wake nobody is there to take costs the delivery and not the
      # briefing: the run stays gathered and the next session that opens reads
      # it and asks for the writing.
      scufris-briefing collect --profile ${quoted} --json > /dev/null
      exec scufris-briefing wake --profile ${quoted}
    '';
    meta = {
      description = "Collect the Scufris ${name} briefing and deliver it";
      mainProgram = "scufris-briefing-${name}";
    };
  } ''
    # A schedule nobody can act on fails the build rather than the morning.
    # systemd reads none of crontab's syntax, so `0 7 * * *` is caught here
    # and not at seven in the morning.
    if ! systemd-analyze calendar "$schedule" > /dev/null; then
      echo "programs.scufris.agent.briefing.profiles.${name}.schedule is not a systemd OnCalendar specification: $schedule" >&2
      exit 1
    fi
    install -Dm755 "$scriptPath" "$out/bin/scufris-briefing-${name}"
  ''
