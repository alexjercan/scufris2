# Voice is Linux only. The speaker is the companion's: the speech runtime and
# the assets reach the synthesiser and the development shell and no launcher at
# all, and the private Piper must really synthesise.
{
  pkgs,
  scufris,
  fixtures,
  ...
}: let
  inherit (pkgs) lib;
  inherit (scufris) voice voiceResources launcher voiceLauncher speak devShell;
  inherit (fixtures) fakePlayer;
  normalClosure = pkgs.closureInfo {rootPaths = [launcher];};
  voiceClosure = pkgs.closureInfo {rootPaths = [voiceLauncher];};
  speakClosure = pkgs.closureInfo {rootPaths = [speak];};
  devShellClosure = pkgs.closureInfo {rootPaths = [devShell];};
in
  lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    closures = pkgs.runCommand "scufris-voice-closures-check" {} ''
      normal=${normalClosure}/store-paths
      voice=${voiceClosure}/store-paths
      speak=${speakClosure}/store-paths

      # Nothing in the agent's process tree makes sound, whichever resources it
      # was handed. The voice launcher differs only in what it hands Pi.
      for launcher in "$normal" "$voice"; do
        ! grep -Fx ${lib.escapeShellArg (toString voice.piperPackage)} "$launcher"
        ! grep -Fx ${lib.escapeShellArg (toString pkgs.piper-tts)} "$launcher"
        ! grep -Fx ${lib.escapeShellArg (toString pkgs.pipewire)} "$launcher"
        ! grep -Fx ${lib.escapeShellArg (toString voice.assets)} "$launcher"
      done

      grep -Fx ${lib.escapeShellArg (toString voice.piperPackage)} "$speak"
      grep -Fx ${lib.escapeShellArg (toString pkgs.pipewire)} "$speak"
      grep -Fx ${lib.escapeShellArg (toString voice.assets)} "$speak"
      ! grep -Fx ${lib.escapeShellArg (toString pkgs.piper-tts)} "$speak"
      touch "$out"
    '';

    # The companion is handed one program and no settings: the voice is pinned
    # by the package rather than chosen at run time or by the environment.
    synthesiser = pkgs.runCommand "scufris-synthesiser-check" {} ''
      helper=${lib.getExe speak}
      grep -F ${lib.escapeShellArg voice.model} "$helper"
      grep -F ${lib.escapeShellArg voice.config} "$helper"
      grep -F ${lib.escapeShellArg "${voiceResources}/share/scufris/tools/voice/scufris-speak"} "$helper" \
        || grep -F scufris-speak "$helper"
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
