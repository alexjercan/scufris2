# `nix run .#staging`: the whole stack from this source tree, beside whatever
# is deployed.
#
# Everything here is the flake's own source, which for a dirty tree is the
# working tree as Nix snapshots it. The two binaries are built rather than
# compiled by the script, so the command works with no dev shell and no warm
# `cargo` target directory; the agent is the snapshot's `scufris-agent`, which
# is the same code the deployed launcher runs with the extensions taken from
# here instead of from the resources package.
{
  pkgs,
  self,
  service,
  desktop,
  speak,
}:
pkgs.writeShellApplication {
  name = "scufris-staging";
  # `flock` for the one-stack-at-a-time lock and `git` for the seeded project.
  # `pi` is not among them: the agent finds the managed one on PATH, the same
  # way `scufris-dev` does.
  runtimeInputs = [pkgs.util-linux pkgs.git];
  text = ''
    export SCUFRIS_STAGING_SERVICE=${pkgs.lib.getExe' service "scufris-service"}
    export SCUFRIS_STAGING_DESKTOP=${pkgs.lib.getExe' desktop "scufris-desktop"}
    # The synthesiser is the packaged one, which binds Piper, the model, and
    # the configuration itself. Staging hears the deployed voice rather than
    # one assembled here, and the script still lets an environment override
    # name another.
    export SCUFRIS_STAGING_SPEAK=${pkgs.lib.getExe' speak "scufris-speak"}
    exec ${self}/scripts/scufris-staging "$@"
  '';
  meta = {
    description = "Run the working tree's Scufris beside the deployed one";
    mainProgram = "scufris-staging";
  };
}
