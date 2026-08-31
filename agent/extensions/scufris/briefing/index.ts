import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import briefing from "./briefing.ts";

export default function briefings(pi: ExtensionAPI): void {
  briefing(pi);
}
