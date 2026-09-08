# The schedule is systemd's. Each profile gets its own timer and its own run,
# and a schedule that is not one fails the build.
{
  pkgs,
  homes,
  ...
}: let
  inherit (pkgs) lib;
  inherit (homes) mkHome;
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
in
  lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
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
      done
      grep -Fx 'OnCalendar=07:30' ${units}/scufris-briefing-morning.timer
      grep -Fx 'OnCalendar=Mon *-*-* 09:00' ${units}/scufris-briefing-weekly.timer
      grep -Fx 'Persistent=true' ${units}/scufris-briefing-morning.timer
      grep -Fx 'Persistent=false' ${units}/scufris-briefing-weekly.timer
      grep -Fx 'WantedBy=timers.target' ${units}/scufris-briefing-morning.timer
      grep -Fx 'Type=oneshot' ${units}/scufris-briefing-morning.service
      # The unit outlives the run deadline the collection holds itself to.
      grep -Fx 'TimeoutStartSec=2100' ${units}/scufris-briefing-morning.service
      grep -Fx 'TimeoutStartSec=3900' ${units}/scufris-briefing-weekly.service
      # Each profile collects into its own run and delivers it itself.
      runner="$(sed -n 's/^ExecStart=//p' ${units}/scufris-briefing-weekly.service)"
      grep -F -- "--profile weekly" "$runner"
      grep -F -- "collect --profile weekly" "$runner"
      grep -F -- "wake --profile weekly" "$runner"
      ! grep -F -- "--profile morning" "$runner"
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
