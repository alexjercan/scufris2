# The companion is its own Linux-only package. These assert that it stays out
# of the launcher closures, resolves its configuration exactly, and that the
# module wires the companion and the bundled whisper-server it may own.
{
  pkgs,
  scufris,
  homes,
  ...
}: let
  inherit (pkgs) lib;
  inherit (scufris) launcher desktop;
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
  testToday = pkgs.writeShellScriptBin "today" ''
    printf 'today %s\n' "$@"
  '';
  testAgent = pkgs.writeShellScriptBin "scufris" ''
    printf 'scufris %s\n' "$@"
  '';
  # The companion is a client, so every desktop configuration under test also
  # runs the service that owns the conversation.
  serviceSettings = {
    enable = true;
    package = scufris.service;
    agentPackage = testAgent;
  };
  desktopHome = mkHome {
    settings = {
      voice.enable = true;
      service = serviceSettings;
      desktop = {
        enable = true;
        package = desktop;
        chatCommand = testChat;
        todayCommand = testToday;
        denPath = "/home/tester/the-den";
        macrosDatabase = "/home/tester/macros.csv";
        stt.whisper = {
          package = testWhisper;
          model = testWhisperModel;
        };
      };
    };
  };
  configuredEndpointHome = mkHome {
    settings = {
      service = serviceSettings;
      desktop = {
        enable = true;
        package = desktop;
        cancelKey = "Control+Alt+Q";
        stopKey = "none";
        stt.endpoint = "http://127.0.0.1:10301/inference";
      };
    };
  };
  desktopWithoutServiceHome = mkHome {
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
  desktopWithoutServiceEvaluation = builtins.tryEval (builtins.deepSeq desktopWithoutServiceHome.activationPackage true);
  normalClosure = pkgs.closureInfo {rootPaths = [launcher];};
  desktopClosure = pkgs.closureInfo {rootPaths = [desktop];};
in
  lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    desktop-closure = pkgs.runCommand "scufris-desktop-closure-check" {} ''
      normal=${normalClosure}/store-paths
      desktop=${desktopClosure}/store-paths

      # The companion is its own package output: consumers who never enable it
      # never build Tauri.
      ! grep -Fx ${lib.escapeShellArg (toString desktop)} "$normal"
      ! grep -Fx ${lib.escapeShellArg (toString pkgs.webkitgtk_4_1)} "$normal"
      grep -Fx ${lib.escapeShellArg (toString pkgs.webkitgtk_4_1)} "$desktop"
      ! grep -Fx ${lib.escapeShellArg (toString launcher)} "$desktop"

      # Widget backends are Python 3 programs the companion spawns, so the
      # interpreter is part of the package rather than of the person's PATH.
      grep -Fx ${lib.escapeShellArg (toString pkgs.python3)} "$desktop"
      grep -F ${lib.escapeShellArg (toString pkgs.python3)} \
        "$(readlink -f ${desktop}/bin/scufris-desktop)"
      test -f ${desktop}/share/applications/scufris-desktop.desktop
      test -f ${desktop}/share/icons/hicolor/scalable/apps/scufris.svg

      # The command client is its own package now, built without any of this.
      ! test -e ${desktop}/bin/scufris-ctl
      touch "$out"
    '';

    desktop-configuration = pkgs.runCommand "scufris-desktop-configuration-check" {} ''
      export SCUFRIS_DESKTOP_SOCKET=/run/user/1000/scufris/service.sock
      export HOME=/home/scufris-test
      unset XDG_STATE_HOME
      # A build sandbox has no session runtime directory, which is exactly the
      # case the command socket must not be fatal for.
      unset XDG_RUNTIME_DIR
      ${desktop}/bin/scufris-desktop --print-config > defaults
      cat > expected-defaults <<'EOF'
      socket=/run/user/1000/scufris/service.sock
      command_socket=none
      state_file=/home/scufris-test/.local/state/scufris-desktop/pending.json
      stt_endpoint=http://127.0.0.1:10301/inference
      hotkey=Super+D
      cancel_key=derived
      stop_key=derived
      chat_command=none
      restart_command=none
      speak_command=none
      EOF
      diff -u expected-defaults defaults

      XDG_STATE_HOME=/home/scufris-test/.state \
        ${desktop}/bin/scufris-desktop --print-config | grep -Fx \
        'state_file=/home/scufris-test/.state/scufris-desktop/pending.json'

      SCUFRIS_STT_ENDPOINT=http://127.0.0.1:10302/inference \
        SCUFRIS_DESKTOP_HOTKEY=Super+G \
        SCUFRIS_DESKTOP_CANCEL_KEY=Control+Alt+Q \
        SCUFRIS_DESKTOP_STOP_KEY=none \
        SCUFRIS_DESKTOP_COMMAND_SOCKET=/run/user/1000/scufris/desktop.sock \
        SCUFRIS_DESKTOP_STATE_FILE=/run/user/1000/scufris-desktop/pending.json \
        SCUFRIS_DESKTOP_CHAT_COMMAND=/nix/store/fake/bin/scufris-chat \
        SCUFRIS_DESKTOP_RESTART_COMMAND=/nix/store/fake/bin/scufris-restart-backend \
        SCUFRIS_DESKTOP_SPEAK_COMMAND=/nix/store/fake/bin/scufris-speak \
        ${desktop}/bin/scufris-desktop --print-config > overridden
      cat > expected-overridden <<'EOF'
      socket=/run/user/1000/scufris/service.sock
      command_socket=/run/user/1000/scufris/desktop.sock
      state_file=/run/user/1000/scufris-desktop/pending.json
      stt_endpoint=http://127.0.0.1:10302/inference
      hotkey=Super+G
      cancel_key=Control+Alt+Q
      stop_key=none
      chat_command=/nix/store/fake/bin/scufris-chat
      restart_command=/nix/store/fake/bin/scufris-restart-backend
      speak_command=/nix/store/fake/bin/scufris-speak
      EOF
      diff -u expected-overridden overridden

      # One knob moves both sockets, which is what puts a staging Scufris
      # beside the deployed one. The directory is used as named, with no
      # `scufris` below it, and `SCUFRIS_DESKTOP_SOCKET` still outranks it.
      # `SCUFRIS_DESKTOP_SOCKET` is exported above and would hide the first
      # half of this, and `XDG_RUNTIME_DIR` is unset, so the override is
      # answering on its own.
      env -u SCUFRIS_DESKTOP_SOCKET SCUFRIS_RUNTIME_DIR=/run/user/1000/scufris-staging \
        ${desktop}/bin/scufris-desktop --print-config > staged
      grep -Fx 'socket=/run/user/1000/scufris-staging/service.sock' staged
      grep -Fx 'command_socket=/run/user/1000/scufris-staging/desktop.sock' staged
      # A path named outright still outranks the directory.
      SCUFRIS_RUNTIME_DIR=/run/user/1000/scufris-staging \
        ${desktop}/bin/scufris-desktop --print-config | grep -Fx \
        'socket=/run/user/1000/scufris/service.sock'

      # A relative hook would let the working directory choose the executable.
      ! SCUFRIS_DESKTOP_CHAT_COMMAND=scufris-chat \
        ${desktop}/bin/scufris-desktop --print-config
      ! SCUFRIS_STT_ENDPOINT=file:///etc/passwd \
        ${desktop}/bin/scufris-desktop --print-config
      touch "$out"
    '';

    # rustfmt needs no dependencies, so the whole Rust workspace is checked
    # here. Clippy needs the whole dependency tree and stays a release step.
    native-format =
      pkgs.runCommand "scufris-native-format-check" {
        nativeBuildInputs = [pkgs.cargo pkgs.rustfmt];
      } ''
        cp -R ${../../native} native
        chmod -R u+w native
        cd native
        cargo fmt --all --check
        touch "$out"
      '';

    desktop-home = desktopHome.activationPackage;

    desktop-interface = assert !(mkHome {}).config.programs.scufris.desktop.enable;
    assert !desktopWithoutServiceEvaluation.success;
    assert desktopConfig.endpoint == "http://127.0.0.1:10302/inference";
    assert desktopConfig.stt.whisper.enable;
    assert desktopConfig.serviceName == "scufris-desktop";
    assert desktopUnit.Install.WantedBy == ["graphical-session.target"];
    assert desktopUnit.Service.Restart == "on-failure";
    assert desktopUnit.Service.StateDirectory == "scufris-desktop";
    assert desktopUnit.Service.ExecStart == [(lib.getExe desktop)];
    assert lib.elem scufris.ctl desktopHome.config.home.packages;
    assert lib.elem "SCUFRIS_STT_ENDPOINT=http://127.0.0.1:10302/inference" desktopUnit.Service.Environment;
    assert lib.elem "SCUFRIS_DESKTOP_HOTKEY=Super+D" desktopUnit.Service.Environment;
    assert lib.elem "SCUFRIS_DESKTOP_CHAT_COMMAND=${lib.getExe testChat}" desktopUnit.Service.Environment;
    # The journal is personal data, so the deployment names the command that
    # reads it and where it lives. A unit does not inherit a login shell, so a
    # den somewhere other than the default has to be written here.
    assert lib.elem "SCUFRIS_TODAY_COMMAND=${lib.getExe testToday}" desktopUnit.Service.Environment;
    assert lib.elem "DEN_PATH=/home/tester/the-den" desktopUnit.Service.Environment;
    # Logging a food turns a name and an amount into a row, and the database is
    # what does that. It follows the den for the same reason.
    assert lib.elem "MACROS_DATABASE=/home/tester/macros.csv" desktopUnit.Service.Environment;
    assert !(lib.any (lib.hasPrefix "SCUFRIS_TODAY_COMMAND=") configuredDesktop.systemd.user.services.scufris-desktop.Service.Environment);
    assert !(lib.any (lib.hasPrefix "DEN_PATH=") configuredDesktop.systemd.user.services.scufris-desktop.Service.Environment);
    assert !(lib.any (lib.hasPrefix "MACROS_DATABASE=") configuredDesktop.systemd.user.services.scufris-desktop.Service.Environment);
    # The two keys beside the hotkey are the deployment's to name. A module
    # that named neither writes neither, because absent means derived and the
    # companion is the one that derives them.
    assert !(lib.any (lib.hasPrefix "SCUFRIS_DESKTOP_CANCEL_KEY=") desktopUnit.Service.Environment);
    assert !(lib.any (lib.hasPrefix "SCUFRIS_DESKTOP_STOP_KEY=") desktopUnit.Service.Environment);
    assert lib.elem "SCUFRIS_DESKTOP_CANCEL_KEY=Control+Alt+Q" configuredDesktop.systemd.user.services.scufris-desktop.Service.Environment;
    assert lib.elem "SCUFRIS_DESKTOP_STOP_KEY=none" configuredDesktop.systemd.user.services.scufris-desktop.Service.Environment;
    # The speaker is the companion's. Voice hands it a synthesiser, and a
    # deployment without voice hands it nothing and it stays silent.
    assert lib.any (lib.hasPrefix "SCUFRIS_DESKTOP_SPEAK_COMMAND=") desktopUnit.Service.Environment;
    assert !(lib.any (lib.hasPrefix "SCUFRIS_DESKTOP_SPEAK_COMMAND=") configuredDesktop.systemd.user.services.scufris-desktop.Service.Environment);
    assert lib.hasInfix "--port 10302" (builtins.head whisperUnit.Service.ExecStart);
    assert lib.hasInfix "--host 127.0.0.1" (builtins.head whisperUnit.Service.ExecStart);
    assert lib.hasInfix "--inference-path /inference" (builtins.head whisperUnit.Service.ExecStart);
    assert configuredDesktop.programs.scufris.desktop.endpoint == "http://127.0.0.1:10301/inference";
    assert !configuredDesktop.programs.scufris.desktop.stt.whisper.enable;
    assert !(builtins.hasAttr "scufris-whisper" configuredDesktop.systemd.user.services);
      pkgs.runCommand "scufris-desktop-interface-check" {} ''
        restart=${lib.getExe desktopConfig.restartCommand}
        # The companion may only restart the backend service this module owns.
        grep -F 'systemctl --user restart scufris-service.service' "$restart"
        ! grep -Ei 'whisper|kitty' "$restart"
        touch "$out"
      '';
  }
