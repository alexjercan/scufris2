# Voice is Linux only. Speech runtime and assets must reach the voice closures
# and no other, and the private Piper must really synthesise.
{
  pkgs,
  scufris,
  fixtures,
  ...
}: let
  inherit (pkgs) lib;
  inherit (scufris) voice voiceResources launcher voiceLauncher devShell;
  inherit (fixtures) fakePlayer;
  normalClosure = pkgs.closureInfo {rootPaths = [launcher];};
  voiceClosure = pkgs.closureInfo {rootPaths = [voiceLauncher];};
  devShellClosure = pkgs.closureInfo {rootPaths = [devShell];};
in
  lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
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
          python3 ${voiceResources}/share/scufris/tools/voice/scufris-speak
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
