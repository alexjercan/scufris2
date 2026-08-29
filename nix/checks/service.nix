# The headless half. These assert that the service and its client build with
# no graphical dependency at all, and that the module gives the service a unit
# of its own that does not wait for a screen.
{
  pkgs,
  scufris,
  homes,
  ...
}: let
  inherit (pkgs) lib;
  inherit (scufris) service ctl;
  inherit (homes) mkHome;
  testAgent = pkgs.writeShellScriptBin "scufris" ''
    printf 'scufris %s\n' "$@"
  '';
  serviceHome = mkHome {
    settings.service = {
      enable = true;
      package = service;
      agent.package = testAgent;
    };
  };
  serviceConfig = serviceHome.config.programs.scufris.service;
  serviceUnit = serviceHome.config.systemd.user.services.${serviceConfig.serviceName};
  serviceClosure = pkgs.closureInfo {rootPaths = [service];};
  ctlClosure = pkgs.closureInfo {rootPaths = [ctl];};
in
  lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    service-closure = pkgs.runCommand "scufris-service-closure-check" {} ''
      # The reason the service is its own package: a machine with no display
      # runs the conversation, and a terminal over ssh reaches it.
      for closure in ${serviceClosure} ${ctlClosure}; do
        ! grep -Fx ${lib.escapeShellArg (toString pkgs.webkitgtk_4_1)} "$closure/store-paths"
        ! grep -Fx ${lib.escapeShellArg (toString pkgs.gtk3)} "$closure/store-paths"
        ! grep -Fx ${lib.escapeShellArg (toString scufris.desktop)} "$closure/store-paths"
      done

      ${service}/bin/scufris-service --help | grep -F 'Usage: scufris-service'
      # The agent and the session directory are options as well as variables,
      # so one run can be pointed somewhere else from a terminal.
      ${service}/bin/scufris-service --help | grep -F 'SCUFRIS_SERVICE_AGENT'
      ${service}/bin/scufris-service --help | grep -F 'SCUFRIS_SERVICE_SESSION_DIR'
      ! ${service}/bin/scufris-service --nonsense

      # Protocol v4 control is intentionally diagnostic-only.
      ${ctl}/bin/scufris-ctl --help | grep -F 'Usage: scufris-ctl [COMMAND]'
      for verb in state open hud show hide; do
        ${ctl}/bin/scufris-ctl --help | grep -qE "^  $verb "
      done
      for removed in send watch abort debug; do
        ! ${ctl}/bin/scufris-ctl --help | grep -qE "^  $removed "
      done
      ! ${ctl}/bin/scufris-ctl nonsense
      touch "$out"
    '';

    service-home = serviceHome.activationPackage;

    service-interface = assert !(mkHome {}).config.programs.scufris.service.enable;
    assert serviceConfig.serviceName == "scufris-service";
    # The conversation is not part of the graphical session. That is the whole
    # claim of this package, and the unit is where it is either true or not.
    assert serviceUnit.Install.WantedBy == ["default.target"];
    assert !(lib.elem "graphical-session.target" (serviceUnit.Unit.After or []));
    assert serviceUnit.Service.ExecStart == [(lib.getExe service)];
    assert serviceUnit.Service.Restart == "on-failure";
    assert serviceUnit.Service.RuntimeDirectory == "scufris-service";
    assert lib.elem "SCUFRIS_SERVICE_AGENT=${lib.getExe testAgent}" serviceUnit.Service.Environment;
    assert lib.elem "SCUFRIS_SERVICE_SESSION_DIR=/home/scufris-test/.local/share/scufris/sessions" serviceUnit.Service.Environment;
    # The client belongs to whoever enabled a half of Scufris, and it is one
    # package so enabling both halves does not collide.
    assert lib.elem ctl serviceHome.config.home.packages;
    # Speech is not the agent's decision and never reaches this unit. The
    # answer is prose whatever is listening, and the speaker is the
    # companion's, so no variable here turns anything on.
    assert !(lib.any (lib.hasPrefix "SCUFRIS_SPEECH=") serviceUnit.Service.Environment);
      pkgs.runCommand "scufris-service-interface-check" {} ''touch "$out"'';
  }
