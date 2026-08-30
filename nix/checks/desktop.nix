# The companion is its own Linux-only package. These assert that it stays out
# of the launcher closures, resolves its configuration exactly, and that the
# module wires the companion to a shared or managed ai-tools-api deployment.
{
  inputs,
  pkgs,
  scufris,
  homes,
  ...
}: let
  inherit (pkgs) lib;
  inherit (scufris) launcher desktop;
  inherit (homes) mkHome;
  # Stand-ins keep module checks from realising inference packages and models.
  testApi = pkgs.writeShellScriptBin "ai-tools-api" ''
    printf 'ai-tools-api %s\n' "$@"
  '';
  testWhisper = pkgs.writeShellScriptBin "whisper-server" ''
    printf 'whisper-server %s\n' "$@"
  '';
  testPiper = pkgs.writeShellScriptBin "piper" ''
    printf 'piper %s\n' "$@"
  '';
  testWhisperModel = pkgs.writeText "ggml-test.bin" "model";
  testPiperModel = pkgs.writeText "voice.onnx" "model";
  testPiperConfig = pkgs.writeText "voice.onnx.json" "config";
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
    agent.package = testAgent;
    service = {
      enable = true;
      package = scufris.service;
    };
  };
  desktopHome = mkHome {
    modules = [
      # A deployment may already import and configure the shared module. The
      # Scufris module imports the same followed input, so this is one option
      # and one pair of units rather than a second inference stack.
      inputs.ai-tools-api.homeModules.default
      {
        services.ai-tools-api = {
          enable = true;
          package = testApi;
          whisperPackage = testWhisper;
          whisperModel = testWhisperModel;
          piperPackage = testPiper;
          piperModel = testPiperModel;
          piperConfig = testPiperConfig;
        };
      }
    ];
    settings =
      serviceSettings
      // {
        desktop = {
          enable = true;
          package = desktop;
          terminalCommand = testChat;
          widgets = {
            todayCommand = testToday;
            denPath = "/home/tester/the-den";
            macrosDatabase = "/home/tester/macros.csv";
          };
          aiToolsApi.baseUrl = "http://127.0.0.1:10300";
          speech = {
            enable = true;
            model = "custom-piper";
            voice = "custom-voice";
          };
          transcription = {
            model = "custom-whisper";
            language = "en";
          };
        };
      };
  };
  fallbackHome = mkHome {
    settings.aiToolsApi.enable = true;
  };
  configuredRequestHome = mkHome {
    settings =
      serviceSettings
      // {
        desktop = {
          enable = true;
          package = desktop;
          backgroundKey = "Control+Alt+Q";
          abortKey = "none";
          aiToolsApi.baseUrl = "http://127.0.0.1:10400";
          transcription = {
            model = "another-whisper";
            language = "ro";
          };
        };
      };
  };
  desktopWithoutServiceHome = mkHome {
    settings.desktop = {
      enable = true;
      package = desktop;
    };
  };
  desktopConfig = desktopHome.config.programs.scufris.desktop;
  desktopUnit = desktopHome.config.systemd.user.services.${desktopConfig.serviceName};
  speakerCommand = lib.removePrefix "SCUFRIS_DESKTOP_SPEAK_COMMAND=" (lib.findFirst
    (lib.hasPrefix "SCUFRIS_DESKTOP_SPEAK_COMMAND=")
    (throw "speech-enabled desktop has no speaker")
    desktopUnit.Service.Environment);
  configuredDesktop = configuredRequestHome.config;
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
      export SCUFRIS_DESKTOP_SOCKET=/run/user/1000/scufris/surface.sock
      export HOME=/home/scufris-test
      unset XDG_STATE_HOME
      # A build sandbox has no session runtime directory, which is exactly the
      # case the command socket must not be fatal for.
      unset XDG_RUNTIME_DIR
      ${desktop}/bin/scufris-desktop --print-config > defaults
      cat > expected-defaults <<'EOF'
      socket=/run/user/1000/scufris/surface.sock
      command_socket=none
      state_file=/home/scufris-test/.local/state/scufris-desktop/pending.json
      stt_endpoint=http://127.0.0.1:10300/v1/audio/transcriptions
      stt_model=whisper-1
      stt_language=auto
      popup_key=Super+D
      background_key=derived
      abort_key=derived
      terminal_command=none
      restart_command=none
      speak_command=none
      EOF
      diff -u expected-defaults defaults

      XDG_STATE_HOME=/home/scufris-test/.state \
        ${desktop}/bin/scufris-desktop --print-config | grep -Fx \
        'state_file=/home/scufris-test/.state/scufris-desktop/pending.json'

      SCUFRIS_STT_ENDPOINT=http://127.0.0.1:10400/v1/audio/transcriptions \
        SCUFRIS_STT_MODEL=another-whisper \
        SCUFRIS_STT_LANGUAGE=ro \
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
      socket=/run/user/1000/scufris/surface.sock
      command_socket=/run/user/1000/scufris/desktop.sock
      state_file=/run/user/1000/scufris-desktop/pending.json
      stt_endpoint=http://127.0.0.1:10400/v1/audio/transcriptions
      stt_model=another-whisper
      stt_language=ro
      popup_key=Super+G
      background_key=Control+Alt+Q
      abort_key=none
      terminal_command=/nix/store/fake/bin/scufris-chat
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
      grep -Fx 'socket=/run/user/1000/scufris-staging/surface.sock' staged
      grep -Fx 'command_socket=/run/user/1000/scufris-staging/desktop.sock' staged
      # A path named outright still outranks the directory.
      SCUFRIS_RUNTIME_DIR=/run/user/1000/scufris-staging \
        ${desktop}/bin/scufris-desktop --print-config | grep -Fx \
        'socket=/run/user/1000/scufris/surface.sock'

      # A relative hook would let the working directory choose the executable.
      ! SCUFRIS_DESKTOP_CHAT_COMMAND=scufris-chat \
        ${desktop}/bin/scufris-desktop --print-config
      ! SCUFRIS_STT_ENDPOINT=file:///etc/passwd \
        ${desktop}/bin/scufris-desktop --print-config
      ! SCUFRIS_STT_MODEL='not a model' \
        ${desktop}/bin/scufris-desktop --print-config
      touch "$out"
    '';

    # rustfmt needs no dependencies, so the whole Rust workspace is checked
    # here. Clippy needs the whole dependency tree and stays a release step.
    native-format =
      pkgs.runCommand "scufris-native-format-check" {
        nativeBuildInputs = [pkgs.cargo pkgs.rustfmt];
      } ''
        cp -R ${scufris.rustSource} source
        chmod -R u+w source
        cd source
        cargo fmt --all --check
        touch "$out"
      '';

    desktop-home = desktopHome.activationPackage;

    desktop-interface = assert !(mkHome {}).config.programs.scufris.desktop.enable;
    assert !desktopWithoutServiceEvaluation.success;
    assert desktopConfig.serviceName == "scufris-desktop";
    assert desktopUnit.Install.WantedBy == ["graphical-session.target"];
    assert desktopUnit.Service.Restart == "on-failure";
    assert desktopUnit.Service.StateDirectory == "scufris-desktop";
    assert desktopUnit.Service.ExecStart == [(lib.getExe desktop)];
    assert lib.elem scufris.ctl desktopHome.config.home.packages;
    assert lib.elem "SCUFRIS_STT_ENDPOINT=http://127.0.0.1:10300/v1/audio/transcriptions" desktopUnit.Service.Environment;
    assert lib.elem "SCUFRIS_STT_MODEL=custom-whisper" desktopUnit.Service.Environment;
    assert lib.elem "SCUFRIS_STT_LANGUAGE=en" desktopUnit.Service.Environment;
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
    assert desktopHome.config.services.ai-tools-api.enable;
    assert !desktopHome.config.programs.scufris.aiToolsApi.enable;
    assert builtins.hasAttr "ai-tools-api" desktopHome.config.systemd.user.services;
    assert builtins.hasAttr "ai-tools-api-whisper" desktopHome.config.systemd.user.services;
    assert !(builtins.hasAttr "scufris-ai-tools-api" desktopHome.config.systemd.user.services);
    assert fallbackHome.config.programs.scufris.aiToolsApi.enable;
    assert builtins.hasAttr "scufris-ai-tools-api" fallbackHome.config.systemd.user.services;
    assert fallbackHome.config.systemd.user.services.scufris-ai-tools-api.Service.ExecStart == [(lib.getExe scufris.aiToolsApi)];
    assert fallbackHome.config.systemd.user.services.scufris-ai-tools-api.Service.ProtectSystem == "strict";
    assert fallbackHome.config.systemd.user.services.scufris-ai-tools-api.Service.ProtectHome == "tmpfs";
    assert !(builtins.hasAttr "scufris-whisper" desktopHome.config.systemd.user.services);
    assert configuredDesktop.programs.scufris.desktop.aiToolsApi.baseUrl == "http://127.0.0.1:10400";
    assert configuredDesktop.programs.scufris.desktop.transcription.model == "another-whisper";
    assert configuredDesktop.programs.scufris.desktop.transcription.language == "ro";
    assert !configuredDesktop.programs.scufris.aiToolsApi.enable;
    assert !(builtins.hasAttr "ai-tools-api" configuredDesktop.systemd.user.services);
    assert !(builtins.hasAttr "ai-tools-api-whisper" configuredDesktop.systemd.user.services);
      pkgs.runCommand "scufris-desktop-interface-check" {} ''
        restart=${lib.getExe desktopConfig.restartCommand}
        speaker=${speakerCommand}
        grep -F 'custom-piper' "$speaker"
        grep -F 'custom-voice' "$speaker"
        # The companion may only restart the backend service this module owns.
        grep -F 'systemctl --user restart scufris-service.service' "$restart"
        ! grep -Ei 'whisper|kitty' "$restart"
        touch "$out"
      '';
  }
