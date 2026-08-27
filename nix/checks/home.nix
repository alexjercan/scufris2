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
  voiceHome = mkHome {
    settings.voice.enable = true;
    # Read the consumer-facing options back out of an unrelated place in the
    # configuration, the way a desktop configuration consumes them.
    modules = [
      ({config, ...}: {
        home.sessionVariables = {
          SCUFRIS_TEST_FINAL_PACKAGE = toString config.programs.scufris.finalPackage;
        };
      })
    ];
  };
  voiceConfig = voiceHome.config;
  scufrisOptions = voiceHome.options.programs.scufris;
  consumerEnvironment = voiceConfig.home.sessionVariables;
in
  {
    home-module = normalHome.activationPackage;
  }
  // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    home-voice = voiceHome.activationPackage;

    home-interface = assert !normalHome.config.programs.scufris.voice.enable;
    assert voiceConfig.programs.scufris.voice.enable;
    assert !(scufrisOptions.finalPackage.internal or false);
    assert lib.elem voiceConfig.programs.scufris.finalPackage voiceConfig.home.packages;
    assert consumerEnvironment.SCUFRIS_TEST_FINAL_PACKAGE == toString voiceConfig.programs.scufris.finalPackage;
    # Voice changes nothing about the agent. It asserts the platform and hands
    # the companion a speak command, and the launcher is the same either way -
    # which is what this asserts, on the voice-enabled home: no part of it is a
    # terminal, a synthesiser, or a window manager.
      pkgs.runCommand "scufris-home-interface-check" {} ''
        launcher=${lib.getExe voiceConfig.programs.scufris.finalPackage}
        ! grep -Ei 'whisper|PI_STT|i3|kitty|piper|pw-play' "$launcher"
        touch "$out"
      '';
  }
