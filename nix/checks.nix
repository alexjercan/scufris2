{
  inputs,
  self,
  pkgs,
  system,
  resources,
  voiceResources,
  launcher,
  voiceLauncher,
  voice,
  devShell,
}: let
  inherit (pkgs) lib;
  systemPi = pkgs.writeShellScriptBin "pi" ''
    printf '%s\n' \
      "''${SCUFRIS_PROJECT_ROOTS-}" \
      "''${SCUFRIS_ROLE-}" \
      "''${SCUFRIS_SPEECH-}" \
      "''${SCUFRIS_CALM-}" \
      "''${SCUFRIS_PIPER_MODEL-}" \
      "''${SCUFRIS_PIPER_CONFIG-}" \
      "$(type -P piper || true)" \
      "$(type -P pw-play || true)" \
      system-pi \
      "$@"
  '';
  testTerminal = pkgs.writeShellScriptBin "kitty" ''
    printf '%s\n' \
      "speech=''${SCUFRIS_SPEECH-}" \
      "calm=''${SCUFRIS_CALM-}" \
      "model=''${SCUFRIS_PIPER_MODEL-}" \
      "config=''${SCUFRIS_PIPER_CONFIG-}" \
      "stt=''${PI_STT_CONFIG-}"
    printf 'arg=%s\n' "$@"
  '';
  normalHome = inputs.home-manager.lib.homeManagerConfiguration {
    inherit pkgs;
    modules = [
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
          widgets.enable = false;
        };
      }
    ];
  };
  invalidPopupHome = inputs.home-manager.lib.homeManagerConfiguration {
    inherit pkgs;
    modules = [
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
          widgets.enable = false;
          voice.popup.enable = true;
        };
      }
    ];
  };
  invalidPopupEvaluation = builtins.tryEval (builtins.deepSeq invalidPopupHome.activationPackage true);
  voiceHome = inputs.home-manager.lib.homeManagerConfiguration {
    inherit pkgs;
    modules = [
      inputs.pi.homeModules.default
      self.homeModules.default
      ({config, ...}: {
        home = {
          username = "scufris-test";
          homeDirectory = "/home/scufris-test";
          stateVersion = "25.05";
        };
        programs.pi.coding-agent = {
          enable = true;
          package = systemPi;
        };
        programs.scufris = {
          enable = true;
          piPackage = config.programs.pi.coding-agent.finalPackage;
          delegation.enable = false;
          widgets.enable = false;
          voice = {
            enable = true;
            popup = {
              enable = true;
              terminalPackage = testTerminal;
              sessionDirectory = "/build/scufris-popup-sessions";
            };
          };
        };
      })
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
  normalClosure = pkgs.closureInfo {rootPaths = [launcher];};
  voiceClosure = pkgs.closureInfo {rootPaths = [voiceLauncher];};
  devShellClosure = pkgs.closureInfo {rootPaths = [devShell];};
  fakePlayer = pkgs.writeShellScriptBin "pw-play" ''
    test "$#" -eq 1
    test "$1" = -
    cat > "$SCUFRIS_FIXTURE_WAV"
  '';
in
  {
    launcher-normal =
      pkgs.runCommand "scufris-launcher-normal-check" {
        nativeBuildInputs = [launcher systemPi];
      } ''
        scufris user-argument > actual
        cat > expected <<'EOF'
        ["~/personal","~/work","~/third-party"]
        orchestrator






        system-pi
        --extension
        ${resources}/share/scufris/extensions/scufris/identity.ts
        --extension
        ${resources}/share/scufris/extensions/scufris/response.ts
        --extension
        ${resources}/share/scufris/extensions/scufris/calm.ts
        --extension
        ${resources}/share/scufris/extensions/scufris/agents.ts
        --skill
        ${resources}/share/scufris/skills/delegation
        --extension
        ${resources}/share/scufris/extensions/scufris/widgets.ts
        --skill
        ${resources}/share/scufris/skills/widgets
        user-argument
        EOF
        diff -u expected actual
        touch "$out"
      '';

    launcher-fallback-pi = pkgs.runCommand "scufris-launcher-fallback-pi-check" {} ''
      export HOME="$TMPDIR/home"
      mkdir -p "$HOME"
      expected="$(${inputs.pi.packages.${system}.default}/bin/pi --version)"
      actual="$(PATH=/nonexistent ${launcher}/bin/scufris --version)"
      test "$actual" = "$expected"
      touch "$out"
    '';

    resources = pkgs.runCommand "scufris-resources-check" {} ''
      test -f ${resources}/share/scufris/extensions/scufris/identity.ts
      test -f ${resources}/share/scufris/extensions/scufris/response.ts
      test -f ${resources}/share/scufris/extensions/scufris/calm.ts
      test ! -e ${resources}/share/scufris/extensions/scufris/speech.ts
      test ! -e ${resources}/share/scufris/scripts/scufris-speak
      test ! -e ${resources}/share/scufris/scripts/scufris-dev
      test ! -e ${voiceResources}/share/scufris/scripts/scufris-dev
      test -f ${voiceResources}/share/scufris/extensions/scufris/speech.ts
      test -x ${voiceResources}/share/scufris/scripts/scufris-speak
      test ! -e ${resources}/share/scufris/prompts
      test -f ${resources}/share/scufris/extensions/scufris/agents.ts
      test -f ${resources}/share/scufris/extensions/scufris/widgets.ts
      test -x ${resources}/share/scufris/tools/jobs/scufris-jobs
      test -x ${resources}/share/scufris/scripts/scufris-artifacts-prune
      test -x ${resources}/share/scufris/scripts/scufris-dashboard
      test -f ${resources}/share/scufris/skills/delegation/SKILL.md
      test -f ${resources}/share/scufris/skills/widgets/SKILL.md
      touch "$out"
    '';

    home-module = normalHome.activationPackage;
  }
  // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    launcher-voice =
      pkgs.runCommand "scufris-launcher-voice-check" {
        nativeBuildInputs = [voiceLauncher systemPi];
      } ''
        scufris user-argument > actual
        cat > expected <<'EOF'
        ["~/personal","~/work","~/third-party"]
        orchestrator


        ${voice.model}
        ${voice.config}
        ${voice.piperPackage}/bin/piper
        ${pkgs.pipewire}/bin/pw-play
        system-pi
        --extension
        ${voiceResources}/share/scufris/extensions/scufris/identity.ts
        --extension
        ${voiceResources}/share/scufris/extensions/scufris/response.ts
        --extension
        ${voiceResources}/share/scufris/extensions/scufris/calm.ts
        --extension
        ${voiceResources}/share/scufris/extensions/scufris/speech.ts
        --extension
        ${voiceResources}/share/scufris/extensions/scufris/agents.ts
        --skill
        ${voiceResources}/share/scufris/skills/delegation
        --extension
        ${voiceResources}/share/scufris/extensions/scufris/widgets.ts
        --skill
        ${voiceResources}/share/scufris/skills/widgets
        user-argument
        EOF
        diff -u expected actual
        touch "$out"
      '';

    closures = pkgs.runCommand "scufris-voice-closures-check" {} ''
      normal=${normalClosure}/store-paths
      voice=${voiceClosure}/store-paths

      ! grep -Fx ${lib.escapeShellArg (toString voice.piperPackage)} "$normal"
      ! grep -Fx ${lib.escapeShellArg (toString pkgs.piper-tts)} "$normal"
      ! grep -Fx ${lib.escapeShellArg (toString pkgs.pipewire)} "$normal"
      ! grep -Fx ${lib.escapeShellArg (toString voice.assets)} "$normal"
      grep -Fx ${lib.escapeShellArg (toString voice.piperPackage)} "$voice"
      grep -Fx ${lib.escapeShellArg (toString pkgs.pipewire)} "$voice"
      grep -Fx ${lib.escapeShellArg (toString voice.assets)} "$voice"
      ! grep -Fx ${lib.escapeShellArg (toString pkgs.piper-tts)} "$voice"
      touch "$out"
    '';

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

    development-shell = assert devShell.SCUFRIS_DEV_VOICE == "1";
    assert devShell.SCUFRIS_PIPER_MODEL == voice.model;
    assert devShell.SCUFRIS_PIPER_CONFIG == voice.config;
      pkgs.runCommand "scufris-development-shell-check" {} ''
        closure=${devShellClosure}/store-paths
        grep -Fx ${lib.escapeShellArg (toString voice.piperPackage)} "$closure"
        grep -Fx ${lib.escapeShellArg (toString pkgs.pipewire)} "$closure"
        grep -Fx ${lib.escapeShellArg (toString voice.assets)} "$closure"
        ! grep -Fx ${lib.escapeShellArg (toString pkgs.piper-tts)} "$closure"
        touch "$out"
      '';

    piper-real-fixture = assert voice.piperPackage.version == "1.4.2";
    assert voice.piperPackage != pkgs.piper-tts;
      pkgs.runCommand "scufris-piper-real-fixture-check" {
        nativeBuildInputs = [voice.piperPackage fakePlayer pkgs.python3];
      } ''
        export SCUFRIS_PIPER_MODEL=${lib.escapeShellArg voice.model}
        export SCUFRIS_PIPER_CONFIG=${lib.escapeShellArg voice.config}
        export SCUFRIS_FIXTURE_WAV="$PWD/fixture.wav"
        printf %s 'The real Piper fixture is complete.' | \
          python3 ${voiceResources}/share/scufris/scripts/scufris-speak
        test -s fixture.wav
        test "$(head -c 4 fixture.wav)" = RIFF
        python3 - <<'PY'
        import wave

        with wave.open("fixture.wav", "rb") as fixture:
            assert fixture.getnchannels() > 0
            assert fixture.getframerate() > 0
            assert fixture.getnframes() > 0
        PY
        cp fixture.wav "$out"
      '';
  }
