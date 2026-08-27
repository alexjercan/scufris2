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
      agentPackage = testAgent;
    };
  };
  voiceServiceHome = mkHome {
    settings = {
      voice.enable = true;
      service = {
        enable = true;
        package = service;
        agentPackage = testAgent;
      };
    };
  };
  voiceServiceUnit = voiceServiceHome.config.systemd.user.services.scufris-service;
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

      # A window manager binding and a terminal both run the client by name.
      ${ctl}/bin/scufris-ctl --help | grep -F 'Usage: scufris-ctl <COMMAND>'
      for verb in send state watch abort debug open; do
        ${ctl}/bin/scufris-ctl --help | grep -qE "^  $verb "
      done
      ! ${ctl}/bin/scufris-ctl nonsense
      # A verb with nothing to say is a wrong run, not an empty submission.
      ! ${ctl}/bin/scufris-ctl send
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
    # Speech is the agent's decision and the frontend's job. The variable that
    # lets the agent decide is inherited here; nothing in this unit makes sound.
    assert !(lib.any (lib.hasPrefix "SCUFRIS_SPEECH=") serviceUnit.Service.Environment);
    assert lib.elem "SCUFRIS_SPEECH=1" voiceServiceUnit.Service.Environment;
      pkgs.runCommand "scufris-service-interface-check" {} ''touch "$out"'';
  }
