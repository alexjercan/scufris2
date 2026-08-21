import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const pairPromptPath = fileURLToPath(
  new URL("../../prompts/pair.md", import.meta.url),
);

export const pairPrompt = readFileSync(pairPromptPath, "utf8");

if (
  Buffer.byteLength(pairPrompt, "ascii") > 500 ||
  !pairPrompt.endsWith("\n") ||
  !/^[\x00-\x7f]+$/.test(pairPrompt)
) {
  throw new Error(
    "prompts/pair.md must be LF-terminated ASCII at most 500 bytes",
  );
}

export function appendPairPrompt(systemPrompt: string): string {
  return `${systemPrompt}\n\n${pairPrompt.trimEnd()}`;
}

export default function identity(pi: ExtensionAPI): void {
  if (process.env.SCUFRIS_ROLE !== "orchestrator") return;

  pi.on("before_agent_start", (event) => ({
    systemPrompt: appendPairPrompt(event.systemPrompt),
  }));
}
