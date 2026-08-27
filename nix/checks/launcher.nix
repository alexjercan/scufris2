# The launcher renders an exact Pi command line and falls back to the pinned
# Pi only when no system Pi is on PATH.
{
  pkgs,
  scufris,
  fixtures,
  ...
}: let
  inherit (pkgs) lib;
  inherit (scufris) resources voiceResources launcher voiceLauncher piPackage;
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
        ${resources}/share/scufris/extensions/scufris/service/index.ts
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
    # The voice launcher differs only in which resources it hands Pi. Every
    # speech variable and both speech programs stay empty here, because the
    # agent decides what is worth saying and the companion is what says it.
    launcher-voice =
      pkgs.runCommand "scufris-launcher-voice-check" {
        nativeBuildInputs = [voiceLauncher systemPi];
      } ''
        scufris user-argument > actual
        cat > expected <<'EOF'
        ["~/personal","~/work","~/third-party"]
        orchestrator






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
        ${voiceResources}/share/scufris/extensions/scufris/service/index.ts
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
