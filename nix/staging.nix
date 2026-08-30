# `nix run .#staging`: one combined stack or a split backend and named
# frontends from this source tree, beside whatever is deployed.
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
  aiToolsApi,
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
    # The packaged frontend helper calls the shared API and plays its WAV.
    # Staging may consume a deployed API or own this pinned complete package.
    export SCUFRIS_STAGING_SPEAK=${pkgs.lib.getExe' speak "scufris-speak"}
    export SCUFRIS_STAGING_AI_TOOLS_API_PACKAGE=${pkgs.lib.getExe' aiToolsApi "ai-tools-api"}
    exec ${self}/scripts/scufris-staging "$@"
  '';
  meta = {
    description = "Run a combined or multi-frontend staging Scufris";
    mainProgram = "scufris-staging";
  };
}
