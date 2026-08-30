# Speech remains Linux-only and frontend-local. Inference belongs to the shared
# ai-tools-api package; the Scufris helper contains only HTTP adaptation and
# PipeWire playback.
{
  pkgs,
  scufris,
  ...
}: let
  inherit (pkgs) lib;
  inherit (scufris) launcher speak devShell aiToolsApi;
  launcherClosure = pkgs.closureInfo {rootPaths = [launcher];};
  speakClosure = pkgs.closureInfo {rootPaths = [speak];};
  devShellClosure = pkgs.closureInfo {rootPaths = [devShell];};
in
  lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    closures = pkgs.runCommand "scufris-voice-closures-check" {} ''
      launcher=${launcherClosure}/store-paths
      speaker=${speakClosure}/store-paths

      ! grep -Fx ${lib.escapeShellArg (toString pkgs.piper-tts)} "$launcher"
      ! grep -Fx ${lib.escapeShellArg (toString pkgs.pipewire)} "$launcher"
      ! grep -Fx ${lib.escapeShellArg (toString aiToolsApi)} "$launcher"

      grep -Fx ${lib.escapeShellArg (toString pkgs.pipewire)} "$speaker"
      grep -Fx ${lib.escapeShellArg (toString pkgs.python3)} "$speaker"
      ! grep -Fx ${lib.escapeShellArg (toString pkgs.piper-tts)} "$speaker"
      ! grep -Fx ${lib.escapeShellArg (toString aiToolsApi)} "$speaker"
      touch "$out"
    '';

    synthesiser = pkgs.runCommand "scufris-synthesiser-check" {} ''
      helper=${lib.getExe speak}
      grep -F 'SCUFRIS_TTS_ENDPOINT' "$helper"
      grep -F 'SCUFRIS_TTS_MODEL' "$helper"
      grep -F 'SCUFRIS_TTS_VOICE' "$helper"
      grep -F 'http://127.0.0.1:10300/v1/audio/speech' "$helper"
      grep -F scufris-speak "$helper"
      grep -F '"piper-1"' ${../../tools/voice/scufris-speak}
      grep -F '"en_US-lessac-medium"' ${../../tools/voice/scufris-speak}
      ! grep -F 'SCUFRIS_PIPER_' "$helper"
      touch "$out"
    '';

    development-shell = pkgs.runCommand "scufris-development-shell-check" {} ''
      closure=${devShellClosure}/store-paths
      grep -Fx ${lib.escapeShellArg (toString pkgs.pipewire)} "$closure"
      ! grep -Fx ${lib.escapeShellArg (toString pkgs.piper-tts)} "$closure"
      ! grep -Fx ${lib.escapeShellArg (toString aiToolsApi)} "$closure"
      touch "$out"
    '';
  }
