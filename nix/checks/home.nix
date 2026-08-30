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
          SCUFRIS_TEST_AGENT_PACKAGE = toString config.programs.scufris.agent.package;
        };
      })
    ];
  };
  legacyHome = mkHome {
    configureAgent = false;
    settings = {
      piPackage = pkgs.hello;
      voice.enable = true;
      service = {
        agentPackage = pkgs.hello;
        agent.projectRoots = ["/legacy/projects"];
      };
      desktop = {
        aiToolsApi.manage = false;
        hotkey = "Super+H";
        cancelKey = "Super+Escape";
        stopKey = "Super+Delete";
        chatCommand = pkgs.hello;
        todayCommand = pkgs.hello;
        denPath = "/legacy/den";
        macrosDatabase = "/legacy/macros.csv";
      };
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
    assert legacyConfig.agent.piPackage == pkgs.hello;
    assert legacyConfig.agent.projectRoots == ["/legacy/projects"];
    assert legacyConfig.agent.package == pkgs.hello;
    assert legacyConfig.finalPackage == legacyConfig.agent.package;
    assert legacyConfig.desktop.speech.enable;
    assert !legacyConfig.aiToolsApi.enable;
    assert legacyConfig.desktop.popupKey == "Super+H";
    assert legacyConfig.desktop.backgroundKey == "Super+Escape";
    assert legacyConfig.desktop.abortKey == "Super+Delete";
    assert legacyConfig.desktop.terminalCommand == pkgs.hello;
    assert legacyConfig.desktop.widgets.todayCommand == pkgs.hello;
    assert legacyConfig.desktop.widgets.denPath == "/legacy/den";
    assert legacyConfig.desktop.widgets.macrosDatabase == "/legacy/macros.csv";
    assert speechConfig.programs.scufris.desktop.speech.enable;
    assert !(scufrisOptions.agent.package.internal or false);
    assert !(scufrisOptions.desktop.transcription ? endpoint);
    assert scufrisOptions.aiToolsApi.enable.default == false;
    assert speechConfig.programs.scufris.aiToolsApi.baseUrl == "http://127.0.0.1:10300";
    assert speechConfig.programs.scufris.desktop.aiToolsApi.baseUrl == speechConfig.programs.scufris.aiToolsApi.baseUrl;
    assert scufrisOptions.desktop.popupKey.default == "Super+D";
    assert scufrisOptions.desktop.speech.model.default == "piper-1";
    assert scufrisOptions.desktop.speech.voice.default == "en_US-lessac-medium";
    assert lib.elem speechConfig.programs.scufris.agent.package speechConfig.home.packages;
    assert consumerEnvironment.SCUFRIS_TEST_AGENT_PACKAGE == toString speechConfig.programs.scufris.agent.package;
    # Speech changes nothing about the agent. It asserts the platform and hands
    # the companion a speak command, and the launcher is the same either way -
    # which is what this asserts, on the speech-enabled home: no part of it is a
    # terminal, a synthesiser, or a window manager.
      pkgs.runCommand "scufris-home-interface-check" {} ''
        launcher=${lib.getExe speechConfig.programs.scufris.agent.package}
        ! grep -Ei 'whisper|PI_STT|i3|kitty|piper|pw-play' "$launcher"
        touch "$out"
      '';
  }
