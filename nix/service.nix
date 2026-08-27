{
  pkgs,
  source,
  lockFile,
  version,
}: let
  inherit (pkgs) lib;
  # No GTK, no WebKit, no pkg-config. The service is the headless half of
  # Scufris, and the whole point of it being its own package is that a machine
  # with no display builds and runs it.
  unwrapped = pkgs.rustPlatform.buildRustPackage {
    pname = "scufris-service-unwrapped";
    inherit version;
    src = source;
    cargoLock = {inherit lockFile;};
    cargoBuildFlags = ["-p" "scufris-service"];
    cargoTestFlags = ["-p" "scufris-service" "-p" "scufris-control"];
    meta = {
      description = "Scufris background service and its control client";
      platforms = lib.platforms.linux;
    };
  };
  # Two outputs from one build. The client is installed on its own because a
  # window manager binding and a terminal both want it, and neither of them
  # wants the service or the companion dragged in beside it.
  pick = name: description:
    pkgs.runCommand "${name}-${version}" {
      meta =
        unwrapped.meta
        // {
          inherit description;
          mainProgram = name;
        };
      passthru = {inherit unwrapped;};
    } ''
      mkdir -p "$out/bin"
      ln -s ${unwrapped}/bin/${name} "$out/bin/${name}"
    '';
in {
  service = pick "scufris-service" "Scufris background service that owns the Pi agent and the session";
  ctl = pick "scufris-ctl" "Talk to Scufris from a terminal";
}
