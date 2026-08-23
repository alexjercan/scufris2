import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import response from "./response.ts";

export default async function voice(pi: ExtensionAPI): Promise<void> {
  response(pi);
  if (process.env.SCUFRIS_VOICE_AVAILABLE !== "1") return;

  const { default: speech } = await import("./speech.ts");
  speech(pi);
}
