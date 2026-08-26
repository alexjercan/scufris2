# The companion is its own Linux-only package. These assert that it stays out
# of the launcher closures, resolves its configuration exactly, and that the
# module wires the companion and the bundled whisper-server it may own.
{
  pkgs,
  scufris,
  fixtures,
  homes,
  ...
}: let
  inherit (pkgs) lib;
  inherit (scufris) launcher voiceLauncher desktop;
  inherit (fixtures) testTerminal;
  inherit (homes) mkHome;
  # Stand-ins keep the checks from realising the pinned whisper model and the
  # real whisper.cpp build; only the wiring is under test here.
  testWhisper = pkgs.writeShellScriptBin "whisper-server" ''
    printf 'whisper-server %s\n' "$@"
  '';
  testWhisperModel = pkgs.runCommand "ggml-test.bin" {} ''printf model > "$out"'';
  testChat = pkgs.writeShellScriptBin "scufris-chat" ''
    printf 'open the chat\n'
  '';
  popupSettings = {
    enable = true;
    popup = {
      enable = true;
      terminalPackage = testTerminal;
      sessionDirectory = "/build/scufris-desktop-sessions";
    };
  };
  desktopHome = mkHome {
    settings = {
      voice = popupSettings;
      desktop = {
        enable = true;
        package = desktop;
        chatCommand = testChat;
        stt.whisper = {
          package = testWhisper;
          model = testWhisperModel;
        };
      };
    };
  };
  configuredEndpointHome = mkHome {
    settings = {
      voice = popupSettings;
      desktop = {
        enable = true;
        package = desktop;
        stt.endpoint = "http://127.0.0.1:10301/inference";
      };
    };
  };
  desktopWithoutPopupHome = mkHome {
    settings.desktop = {
      enable = true;
      package = desktop;
      stt.endpoint = "http://127.0.0.1:10301/inference";
    };
  };
  desktopConfig = desktopHome.config.programs.scufris.desktop;
  desktopUnit = desktopHome.config.systemd.user.services.${desktopConfig.serviceName};
  whisperUnit = desktopHome.config.systemd.user.services.${desktopConfig.stt.whisper.serviceName};
  configuredDesktop = configuredEndpointHome.config;
  desktopWithoutPopupEvaluation = builtins.tryEval (builtins.deepSeq desktopWithoutPopupHome.activationPackage true);
  normalClosure = pkgs.closureInfo {rootPaths = [launcher];};
  voiceClosure = pkgs.closureInfo {rootPaths = [voiceLauncher];};
  desktopClosure = pkgs.closureInfo {rootPaths = [desktop];};
in
  lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    desktop-closure = pkgs.runCommand "scufris-desktop-closure-check" {} ''
      normal=${normalClosure}/store-paths
      voice=${voiceClosure}/store-paths
      desktop=${desktopClosure}/store-paths

      # The companion is its own package output: consumers who never enable it
      # never build Tauri.
      ! grep -Fx ${lib.escapeShellArg (toString desktop)} "$normal"
      ! grep -Fx ${lib.escapeShellArg (toString desktop)} "$voice"
      ! grep -Fx ${lib.escapeShellArg (toString pkgs.webkitgtk_4_1)} "$normal"
      ! grep -Fx ${lib.escapeShellArg (toString pkgs.webkitgtk_4_1)} "$voice"
      grep -Fx ${lib.escapeShellArg (toString pkgs.webkitgtk_4_1)} "$desktop"
      ! grep -Fx ${lib.escapeShellArg (toString launcher)} "$desktop"

      # Widget backends are Python 3 programs the companion spawns, so the
      # interpreter is part of the package rather than of the person's PATH.
      grep -Fx ${lib.escapeShellArg (toString pkgs.python3)} "$desktop"
      grep -F ${lib.escapeShellArg (toString pkgs.python3)} \
        "$(readlink -f ${desktop}/bin/scufris-desktop)"
      test -f ${desktop}/share/applications/scufris-desktop.desktop
      test -f ${desktop}/share/icons/hicolor/scalable/apps/scufris.svg
      touch "$out"
    '';

    desktop-configuration = pkgs.runCommand "scufris-desktop-configuration-check" {} ''
      export SCUFRIS_DESKTOP_SOCKET=/run/user/1000/scufris/daemon.sock
      export HOME=/home/scufris-test
      unset XDG_STATE_HOME
      ${desktop}/bin/scufris-desktop --print-config > defaults
      cat > expected-defaults <<'EOF'
      socket=/run/user/1000/scufris/daemon.sock
      state_file=/home/scufris-test/.local/state/scufris-desktop/pending.json
      stt_endpoint=http://127.0.0.1:10301/inference
      hotkey=Super+D
      chat_command=none
      restart_command=none
      EOF
      diff -u expected-defaults defaults

      XDG_STATE_HOME=/home/scufris-test/.state \
        ${desktop}/bin/scufris-desktop --print-config | grep -Fx \
        'state_file=/home/scufris-test/.state/scufris-desktop/pending.json'

      SCUFRIS_STT_ENDPOINT=http://127.0.0.1:10302/inference \
        SCUFRIS_DESKTOP_HOTKEY=Super+G \
        SCUFRIS_DESKTOP_STATE_FILE=/run/user/1000/scufris-desktop/pending.json \
        SCUFRIS_DESKTOP_CHAT_COMMAND=/nix/store/fake/bin/scufris-chat \
        SCUFRIS_DESKTOP_RESTART_COMMAND=/nix/store/fake/bin/scufris-restart-backend \
        ${desktop}/bin/scufris-desktop --print-config > overridden
      cat > expected-overridden <<'EOF'
      socket=/run/user/1000/scufris/daemon.sock
      state_file=/run/user/1000/scufris-desktop/pending.json
      stt_endpoint=http://127.0.0.1:10302/inference
      hotkey=Super+G
      chat_command=/nix/store/fake/bin/scufris-chat
      restart_command=/nix/store/fake/bin/scufris-restart-backend
      EOF
      diff -u expected-overridden overridden

      # A relative hook would let the working directory choose the executable.
      ! SCUFRIS_DESKTOP_CHAT_COMMAND=scufris-chat \
        ${desktop}/bin/scufris-desktop --print-config
      ! SCUFRIS_STT_ENDPOINT=file:///etc/passwd \
        ${desktop}/bin/scufris-desktop --print-config
      touch "$out"
    '';

    # rustfmt needs no dependencies, so the companion's formatting is checked
    # here. Clippy needs the whole dependency tree and stays a release step.
    desktop-format =
      pkgs.runCommand "scufris-desktop-format-check" {
        nativeBuildInputs = [pkgs.cargo pkgs.rustfmt];
      } ''
        cp -R ${../../desktop} desktop
        chmod -R u+w desktop
        cd desktop
        cargo fmt --all --check
        touch "$out"
      '';

    desktop-home = desktopHome.activationPackage;

    desktop-interface = assert !(mkHome {}).config.programs.scufris.desktop.enable;
    assert !desktopWithoutPopupEvaluation.success;
    assert desktopConfig.endpoint == "http://127.0.0.1:10302/inference";
    assert desktopConfig.stt.whisper.enable;
    assert desktopConfig.serviceName == "scufris-desktop";
    assert desktopUnit.Install.WantedBy == ["graphical-session.target"];
    assert desktopUnit.Service.Restart == "on-failure";
    assert desktopUnit.Service.StateDirectory == "scufris-desktop";
    assert desktopUnit.Service.ExecStart == [(lib.getExe desktop)];
    assert lib.elem "SCUFRIS_STT_ENDPOINT=http://127.0.0.1:10302/inference" desktopUnit.Service.Environment;
    assert lib.elem "SCUFRIS_DESKTOP_HOTKEY=Super+D" desktopUnit.Service.Environment;
    assert lib.elem "SCUFRIS_DESKTOP_CHAT_COMMAND=${lib.getExe testChat}" desktopUnit.Service.Environment;
    assert lib.hasInfix "--port 10302" (builtins.head whisperUnit.Service.ExecStart);
    assert lib.hasInfix "--host 127.0.0.1" (builtins.head whisperUnit.Service.ExecStart);
    assert lib.hasInfix "--inference-path /inference" (builtins.head whisperUnit.Service.ExecStart);
    assert configuredDesktop.programs.scufris.desktop.endpoint == "http://127.0.0.1:10301/inference";
    assert !configuredDesktop.programs.scufris.desktop.stt.whisper.enable;
    assert !(builtins.hasAttr "scufris-whisper" configuredDesktop.systemd.user.services);
      pkgs.runCommand "scufris-desktop-interface-check" {} ''
        restart=${lib.getExe desktopConfig.restartCommand}
        # The companion may only restart the backend service this module owns.
        grep -F 'systemctl --user restart scufris-popup.service' "$restart"
        ! grep -Ei 'whisper|kitty' "$restart"
        touch "$out"
      '';
  }
