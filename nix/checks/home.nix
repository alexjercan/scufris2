# The Home Manager module activates and keeps a stable read-only interface for
# desktop consumers.
{
  pkgs,
  homes,
  ...
}: let
  inherit (pkgs) lib;
  inherit (homes) mkHome;
  normalHome = mkHome {};
  speechHome = mkHome {
    settings.desktop.speech.enable = true;
    # Read the consumer-facing options back out of an unrelated place in the
    # configuration, the way a desktop configuration consumes them.
    modules = [
      ({config, ...}: {
        home.sessionVariables = {
          SCUFRIS_TEST_AGENT_PACKAGE = toString config.programs.scufris.service.agent.package;
        };
      })
    ];
  };
  legacyHome = mkHome {
    configureAgent = false;
    settings = {
      piPackage = pkgs.hello;
      projectRoots = ["/legacy/projects"];
      voice.enable = true;
      service.agentPackage = pkgs.hello;
      desktop.stt.endpoint = "http://127.0.0.1:10300/v1/audio/transcriptions";
    };
  };
  speechConfig = speechHome.config;
  legacyConfig = legacyHome.config.programs.scufris;
  scufrisOptions = speechHome.options.programs.scufris;
  consumerEnvironment = speechConfig.home.sessionVariables;
in
  {
    home-module = normalHome.activationPackage;
  }
  // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    home-voice = speechHome.activationPackage;
    home-legacy-options = legacyHome.activationPackage;

    home-interface = assert !normalHome.config.programs.scufris.desktop.speech.enable;
    assert legacyConfig.service.agent.piPackage == pkgs.hello;
    assert legacyConfig.service.agent.projectRoots == ["/legacy/projects"];
    assert legacyConfig.service.agent.package == pkgs.hello;
    assert legacyConfig.finalPackage == legacyConfig.service.agent.package;
    assert legacyConfig.desktop.speech.enable;
    assert legacyConfig.desktop.transcription.endpoint == "http://127.0.0.1:10300/v1/audio/transcriptions";
    assert speechConfig.programs.scufris.desktop.speech.enable;
    assert !(scufrisOptions.service.agent.package.internal or false);
    assert lib.elem speechConfig.programs.scufris.service.agent.package speechConfig.home.packages;
    assert consumerEnvironment.SCUFRIS_TEST_AGENT_PACKAGE == toString speechConfig.programs.scufris.service.agent.package;
    # Speech changes nothing about the agent. It asserts the platform and hands
    # the companion a speak command, and the launcher is the same either way -
    # which is what this asserts, on the speech-enabled home: no part of it is a
    # terminal, a synthesiser, or a window manager.
      pkgs.runCommand "scufris-home-interface-check" {} ''
        launcher=${lib.getExe speechConfig.programs.scufris.service.agent.package}
        ! grep -Ei 'whisper|PI_STT|i3|kitty|piper|pw-play' "$launcher"
        touch "$out"
      '';
  }
