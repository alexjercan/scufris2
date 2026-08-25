# The resource variants carry every distributed file, and the normal variant
# carries no speech module and no voice tool.
{
  pkgs,
  scufris,
  ...
}: let
  inherit (scufris) resources voiceResources;
in {
  resources = pkgs.runCommand "scufris-resources-check" {} ''
    test -f ${resources}/share/scufris/extensions/scufris/workflow/index.ts
    test -f ${resources}/share/scufris/extensions/scufris/workflow/identity.ts
    test -f ${resources}/share/scufris/extensions/scufris/workflow/orchestration.ts
    test -f ${resources}/share/scufris/extensions/scufris/workflow/worker-report.ts
    test -f ${resources}/share/scufris/extensions/scufris/voice/index.ts
    test -f ${resources}/share/scufris/extensions/scufris/voice/response.ts
    test ! -e ${resources}/share/scufris/extensions/scufris/voice/speech.ts
    test ! -e ${resources}/share/scufris/tools/voice
    test -f ${resources}/share/scufris/extensions/scufris/calm.ts
    test -f ${resources}/share/scufris/extensions/scufris/dashboard/index.ts
    test -f ${resources}/share/scufris/extensions/scufris/desktop/index.ts
    test -f ${resources}/share/scufris/extensions/scufris/desktop/protocol.ts
    test -f ${resources}/share/scufris/extensions/scufris/desktop/server.ts
    test -f ${voiceResources}/share/scufris/extensions/scufris/voice/speech.ts
    test -x ${voiceResources}/share/scufris/tools/voice/scufris-speak
    test ! -e ${resources}/share/scufris/scripts/scufris-dev
    test ! -e ${voiceResources}/share/scufris/scripts/scufris-dev
    test ! -e ${resources}/share/scufris/prompts
    test -x ${resources}/share/scufris/tools/jobs/scufris-jobs
    test -x ${resources}/share/scufris/tools/jobs/scufris-report
    test -x ${resources}/share/scufris/tools/quick-review-agent/scufris-quick-review-agent
    test -x ${resources}/share/scufris/tools/dashboard/scufris-dashboard
    test -x ${resources}/share/scufris/tools/desktop/scufris-socket-lock
    test -x ${resources}/share/scufris/scripts/scufris-jobs
    test -x ${resources}/share/scufris/scripts/scufris-artifacts-prune
    test -f ${resources}/share/scufris/skills/workflow/SKILL.md
    test -f ${resources}/share/scufris/skills/dashboard/SKILL.md
    touch "$out"
  '';
}
