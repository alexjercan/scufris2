# The launcher renders an exact Pi command line and falls back to the pinned
# Pi only when no system Pi is on PATH.
{
  pkgs,
  scufris,
  fixtures,
  ...
}: let
  inherit (pkgs) lib;
  inherit (scufris) resources voiceResources launcher voiceLauncher voice piPackage;
  inherit (fixtures) systemPi;
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
        ${resources}/share/scufris/extensions/scufris/workflow/index.ts
        --skill
        ${resources}/share/scufris/skills/workflow
        --extension
        ${resources}/share/scufris/extensions/scufris/voice/index.ts
        --extension
        ${resources}/share/scufris/extensions/scufris/calm.ts
        --extension
        ${resources}/share/scufris/extensions/scufris/desktop/index.ts
        --extension
        ${resources}/share/scufris/extensions/scufris/widgets/index.ts
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
      expected="$(${piPackage}/bin/pi --version)"
      actual="$(PATH=/nonexistent ${launcher}/bin/scufris --version)"
      test "$actual" = "$expected"
      touch "$out"
    '';
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
        ${voiceResources}/share/scufris/extensions/scufris/workflow/index.ts
        --skill
        ${voiceResources}/share/scufris/skills/workflow
        --extension
        ${voiceResources}/share/scufris/extensions/scufris/voice/index.ts
        --extension
        ${voiceResources}/share/scufris/extensions/scufris/calm.ts
        --extension
        ${voiceResources}/share/scufris/extensions/scufris/desktop/index.ts
        --extension
        ${voiceResources}/share/scufris/extensions/scufris/widgets/index.ts
        --skill
        ${voiceResources}/share/scufris/skills/widgets
        user-argument
        EOF
        diff -u expected actual
        touch "$out"
      '';
  }
