# The resources carry every distributed file the agent runs, and neither the
# synthesiser nor the development launcher is one of them.
{
  pkgs,
  scufris,
  ...
}: let
  inherit (scufris) resources;
in {
  resources = pkgs.runCommand "scufris-resources-check" {} ''
    test -f ${resources}/share/scufris/extensions/scufris/workflow/index.ts
    test -f ${resources}/share/scufris/extensions/scufris/workflow/identity.ts
    test -f ${resources}/share/scufris/extensions/scufris/workflow/orchestration.ts
    test -f ${resources}/share/scufris/extensions/scufris/workflow/worker-report.ts
    test -f ${resources}/share/scufris/extensions/scufris/response.ts
    test -f ${resources}/share/scufris/extensions/scufris/calm.ts
    test -f ${resources}/share/scufris/extensions/scufris/service/index.ts
    test -f ${resources}/share/scufris/extensions/scufris/service/protocol.ts
    test -f ${resources}/share/scufris/extensions/scufris/service/client.ts
    test -f ${resources}/share/scufris/extensions/scufris/widgets/index.ts
    test -f ${resources}/share/scufris/extensions/scufris/conversation.ts
    # Nothing in this process tree makes sound. There is no speech module to
    # ship and no variant that ships one, and the synthesiser the companion
    # runs is not on the agent's side of the machine.
    test ! -e ${resources}/share/scufris/extensions/scufris/voice
    test ! -e ${resources}/share/scufris/tools/voice
    test ! -e ${resources}/share/scufris/scripts/scufris-dev
    test ! -e ${resources}/share/scufris/scripts/scufris-agent
    test ! -e ${resources}/share/scufris/scripts/scufris-staging
    test ! -e ${resources}/share/scufris/prompts
    test -x ${resources}/share/scufris/tools/jobs/scufris-jobs
    test -x ${resources}/share/scufris/tools/jobs/scufris-report
    test -x ${resources}/share/scufris/tools/quick-review-agent/scufris-quick-review-agent
    test -x ${resources}/share/scufris/scripts/scufris-jobs
    test -x ${resources}/share/scufris/scripts/scufris-artifacts-prune
    test -f ${resources}/share/scufris/skills/workflow/SKILL.md
    test -f ${resources}/share/scufris/skills/widgets/SKILL.md
    touch "$out"
  '';
}
