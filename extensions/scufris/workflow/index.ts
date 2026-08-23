import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import identity from "./identity.ts";
import orchestration from "./orchestration.ts";

export default function workflow(pi: ExtensionAPI): void {
  identity(pi);
  orchestration(pi);
}
