{
  pkgs,
  voice ? false,
}: let
  inherit (pkgs) lib;
in
  pkgs.runCommand "scufris-${lib.optionalString voice "voice-"}resources" {} ''
    mkdir -p "$out/share/scufris"
    cp -R ${../extensions} "$out/share/scufris/extensions"
    cp -R ${../scripts} "$out/share/scufris/scripts"
    cp -R ${../skills} "$out/share/scufris/skills"
    cp -R ${../tools} "$out/share/scufris/tools"
    chmod -R u+w "$out/share/scufris"
    rm "$out/share/scufris/scripts/scufris-dev"
    ${lib.optionalString (!voice) ''
      rm "$out/share/scufris/extensions/scufris/voice/speech.ts"
      rm -R "$out/share/scufris/tools/voice"
    ''}
  ''
