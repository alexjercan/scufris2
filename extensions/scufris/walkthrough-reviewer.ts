import { writeFileSync } from "node:fs";
import { Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { parseWalkthrough } from "./walkthrough.ts";

const completionSchema = Type.Object(
  {
    revision: Type.String({ pattern: "^[0-9a-f]{40}$" }),
    markdown: Type.String({ minLength: 1, maxLength: 262_144 }),
    sectionCount: Type.Integer({ minimum: 1, maximum: 40 }),
  },
  { additionalProperties: false },
);

export const submitWalkthroughTool = defineTool({
  name: "submit_walkthrough",
  label: "Submit walkthrough",
  description:
    "Submit the exact-revision Markdown walkthrough once as the final action.",
  parameters: completionSchema,
  async execute(_id, params, _signal, _update, ctx) {
    const expected = process.env.SCUFRIS_WALKTHROUGH_REVISION;
    const resultPath = process.env.SCUFRIS_WALKTHROUGH_RESULT;
    if (!expected || !resultPath || params.revision !== expected)
      throw new Error("walkthrough revision does not match");
    const document = parseWalkthrough(params.markdown);
    if (
      document.revision !== expected ||
      document.sections.length !== params.sectionCount
    )
      throw new Error("walkthrough completion does not match the artifact");
    const encoded = `${JSON.stringify(params)}\n`;
    if (Buffer.byteLength(encoded) > 300 * 1024)
      throw new Error("walkthrough result exceeds 300 KiB");
    writeFileSync(resultPath, encoded, {
      encoding: "utf8",
      flag: "wx",
      mode: 0o600,
    });
    ctx.shutdown();
    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify({
            revision: params.revision,
            sectionCount: params.sectionCount,
          }),
        },
      ],
      details: { revision: params.revision, sectionCount: params.sectionCount },
      terminate: true,
    };
  },
});

export default function walkthroughReviewer(pi: ExtensionAPI): void {
  pi.registerTool(submitWalkthroughTool);
}
