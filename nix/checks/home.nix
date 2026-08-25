# The Home Manager module activates, rejects an unsupported popup, and keeps a
# stable read-only interface for desktop consumers.
{
  inputs,
  self,
  pkgs,
  fixtures,
  ...
}: let
  inherit (pkgs) lib;
  inherit (fixtures) systemPi testTerminal;
  mkHome = {
    settings ? {},
    modules ? [],
  }:
    inputs.home-manager.lib.homeManagerConfiguration {
      inherit pkgs;
      modules =
        [
          self.homeModules.default
          {
            home = {
              username = "scufris-test";
              homeDirectory = "/home/scufris-test";
              stateVersion = "25.05";
            };
            programs.scufris = {
              enable = true;
              piPackage = systemPi;
              dashboard.enable = false;
            };
          }
          {programs.scufris = settings;}
        ]
        ++ modules;
    };
  normalHome = mkHome {};
  invalidPopupHome = mkHome {settings.voice.popup.enable = true;};
  invalidPopupEvaluation = builtins.tryEval (builtins.deepSeq invalidPopupHome.activationPackage true);
  voiceHome = mkHome {
    settings.voice = {
      enable = true;
      popup = {
        enable = true;
        terminalPackage = testTerminal;
        sessionDirectory = "/build/scufris-popup-sessions";
      };
    };
    # Read the consumer-facing options back out of an unrelated place in the
    # configuration, the way a desktop configuration consumes them.
    modules = [
      ({config, ...}: {
        home.sessionVariables = {
          SCUFRIS_TEST_FINAL_PACKAGE = toString config.programs.scufris.finalPackage;
          SCUFRIS_TEST_POPUP_SERVICE = config.programs.scufris.voice.popup.serviceName;
          SCUFRIS_TEST_POPUP_LAUNCHER = toString config.programs.scufris.voice.popup.finalLauncher;
        };
      })
    ];
  };
  voiceConfig = voiceHome.config;
  scufrisOptions = voiceHome.options.programs.scufris;
  popupConfig = voiceConfig.programs.scufris.voice.popup;
  popupUnit = voiceConfig.systemd.user.services.${popupConfig.serviceName};
  consumerEnvironment = voiceConfig.home.sessionVariables;
in
  {
    home-module = normalHome.activationPackage;
  }
  // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    home-voice = voiceHome.activationPackage;

    home-interface = assert !normalHome.config.programs.scufris.voice.enable;
    assert !normalHome.config.programs.scufris.voice.popup.enable;
    assert normalHome.config.programs.scufris.voice.popup.sessionDirectory == "/home/scufris-test/.local/share/scufris-popup/sessions";
    assert !invalidPopupEvaluation.success;
    assert voiceConfig.programs.scufris.voice.enable;
    assert voiceConfig.programs.scufris.voice.popup.enable;
    assert !(scufrisOptions.finalPackage.internal or false);
    assert !(scufrisOptions.voice.popup.serviceName.internal or false);
    assert !(scufrisOptions.voice.popup.finalLauncher.internal or false);
    assert lib.elem voiceConfig.programs.scufris.finalPackage voiceConfig.home.packages;
    assert popupConfig.serviceName == "scufris-popup";
    assert !(popupUnit ? Install);
    assert popupUnit.Service.ExecStart == [(lib.getExe popupConfig.finalLauncher)];
    assert consumerEnvironment.SCUFRIS_TEST_FINAL_PACKAGE == toString voiceConfig.programs.scufris.finalPackage;
    assert consumerEnvironment.SCUFRIS_TEST_POPUP_SERVICE == popupConfig.serviceName;
    assert consumerEnvironment.SCUFRIS_TEST_POPUP_LAUNCHER == toString popupConfig.finalLauncher;
      pkgs.runCommand "scufris-home-interface-check" {} ''
        popup=${lib.getExe popupConfig.finalLauncher}
        PI_STT_CONFIG=/trusted/pi-voice-stt.json "$popup" > actual
        cat > expected <<'EOF'
        speech=1
        calm=1
        model=${voiceConfig.programs.scufris.voice.piper.model}
        config=${voiceConfig.programs.scufris.voice.piper.config}
        stt=/trusted/pi-voice-stt.json
        arg=--class
        arg=Scufris
        arg=--name
        arg=scufris-popup
        arg=--title
        arg=Scufris
        arg=${lib.getExe voiceConfig.programs.scufris.finalPackage}
        arg=--session-dir
        arg=/build/scufris-popup-sessions
        arg=--continue
        EOF
        diff -u expected actual
        test -d /build/scufris-popup-sessions
        ! grep -Ei 'whisper|PI_STT|i3|tmux' "$popup"
        touch "$out"
      '';
  }
