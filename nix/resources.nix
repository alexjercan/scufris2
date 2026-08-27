{pkgs}:
# What the agent is handed: extensions, scripts, skills, and the tools it runs
# itself. The synthesiser is not among them. Nothing in this process tree makes
# sound, so `tools/voice` belongs to `scufris-speak`, which takes it from the
# source tree.
pkgs.runCommand "scufris-resources" {} ''
  mkdir -p "$out/share/scufris"
  cp -R ${../extensions} "$out/share/scufris/extensions"
  cp -R ${../scripts} "$out/share/scufris/scripts"
  cp -R ${../skills} "$out/share/scufris/skills"
  cp -R ${../tools} "$out/share/scufris/tools"
  chmod -R u+w "$out/share/scufris"
  # The development launchers. What a deployment runs is built from the store,
  # so a copy of the working tree's launcher here would be a second answer to
  # which Scufris this is.
  rm "$out/share/scufris/scripts/scufris-dev"
  rm "$out/share/scufris/scripts/scufris-agent"
  rm "$out/share/scufris/scripts/scufris-staging"
  rm -R "$out/share/scufris/tools/voice"
''
