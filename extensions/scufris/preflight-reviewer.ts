import { writeFileSync } from "node:fs";
import { StringEnum, Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";

const MAX_RESULT_BYTES = 64 * 1024;
const MAX_FEEDBACK_BYTES = 16 * 1024;
const CONTROL_CHARACTER = /[\x00-\x1f\x7f]/;
const SEVERITIES = new Set(["BLOCKER", "MAJOR", "MINOR"]);

const findingSchema = Type.Object(
  {
    severity: StringEnum(["BLOCKER", "MAJOR", "MINOR"] as const),
    path: Type.String({ minLength: 1, maxLength: 512 }),
    line: Type.Integer({ minimum: 1, maximum: 2 ** 31 - 1 }),
    reason: Type.String({ minLength: 1, maxLength: 2048 }),
    change: Type.String({ minLength: 1, maxLength: 2048 }),
  },
  { additionalProperties: false },
);

export const preflightResultSchema = Type.Union([
  Type.Object(
    {
      verdict: StringEnum(["approve"] as const),
      findings: Type.Array(findingSchema, { maxItems: 0 }),
    },
    { additionalProperties: false },
  ),
  Type.Object(
    {
      verdict: StringEnum(["request_changes"] as const),
      findings: Type.Array(findingSchema, { minItems: 1, maxItems: 50 }),
    },
    { additionalProperties: false },
  ),
]);

type Finding = {
  severity: "BLOCKER" | "MAJOR" | "MINOR";
  path: string;
  line: number;
  reason: string;
  change: string;
};

type PreflightResult = {
  verdict: "approve" | "request_changes";
  findings: Finding[];
};

function hasExactKeys(
  value: Record<string, unknown>,
  expected: string[],
): boolean {
  const actual = Object.keys(value).sort();
  return (
    actual.length === expected.length &&
    actual.every((key, index) => key === expected[index])
  );
}

function asciiJson(value: unknown): string {
  return JSON.stringify(value).replace(/[\u0080-\u{10ffff}]/gu, (character) => {
    const codePoint = character.codePointAt(0)!;
    if (codePoint <= 0xffff)
      return `\\u${codePoint.toString(16).padStart(4, "0")}`;
    const offset = codePoint - 0x10000;
    const high = 0xd800 + (offset >> 10);
    const low = 0xdc00 + (offset & 0x3ff);
    return `\\u${high.toString(16)}\\u${low.toString(16)}`;
  });
}

function hasValidUnicode(value: string): boolean {
  for (let index = 0; index < value.length; index++) {
    const code = value.charCodeAt(index);
    if (code >= 0xd800 && code <= 0xdbff) {
      if (index + 1 >= value.length) return false;
      const low = value.charCodeAt(index + 1);
      if (low < 0xdc00 || low > 0xdfff) return false;
      index++;
    } else if (code >= 0xdc00 && code <= 0xdfff) {
      return false;
    }
  }
  return true;
}

function validateText(value: unknown, maximum: number): value is string {
  return (
    typeof value === "string" &&
    value.trim().length > 0 &&
    hasValidUnicode(value) &&
    !CONTROL_CHARACTER.test(value) &&
    Buffer.byteLength(value, "utf8") <= maximum
  );
}

function validateFinding(value: unknown): value is Finding {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const finding = value as Record<string, unknown>;
  if (!hasExactKeys(finding, ["change", "line", "path", "reason", "severity"]))
    return false;
  const path = finding.path;
  return (
    typeof finding.severity === "string" &&
    SEVERITIES.has(finding.severity) &&
    typeof path === "string" &&
    path.length > 0 &&
    hasValidUnicode(path) &&
    Buffer.byteLength(path, "utf8") <= 512 &&
    !CONTROL_CHARACTER.test(path) &&
    !path.startsWith("/") &&
    !path.includes("\\") &&
    path
      .split("/")
      .every((part) => part.length > 0 && part !== "." && part !== "..") &&
    Number.isSafeInteger(finding.line) &&
    Number(finding.line) >= 1 &&
    Number(finding.line) <= 2 ** 31 - 1 &&
    validateText(finding.reason, 2048) &&
    validateText(finding.change, 2048)
  );
}

export function validatePreflightResult(value: unknown): PreflightResult {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Scufris review result is invalid");
  }
  const result = value as Record<string, unknown>;
  if (
    !hasExactKeys(result, ["findings", "verdict"]) ||
    !Array.isArray(result.findings)
  ) {
    throw new Error("Scufris review result is invalid");
  }
  if (result.findings.length > 50 || !result.findings.every(validateFinding)) {
    throw new Error("Scufris review findings are invalid");
  }
  if (
    (result.verdict === "approve" && result.findings.length !== 0) ||
    (result.verdict === "request_changes" && result.findings.length === 0) ||
    (result.verdict !== "approve" && result.verdict !== "request_changes")
  ) {
    throw new Error("Scufris review verdict and findings are inconsistent");
  }
  const validated: PreflightResult = {
    verdict: result.verdict,
    findings: result.findings,
  };
  const feedback = `Independent preflight requested changes: ${asciiJson({ findings: validated.findings })}`;
  if (Buffer.byteLength(feedback, "utf8") > MAX_FEEDBACK_BYTES) {
    throw new Error("Scufris review findings exceed the feedback limit");
  }
  return validated;
}

export const submitPreflightTool = defineTool({
  name: "submit_preflight",
  label: "Submit preflight review",
  description:
    "Submit the final independent preflight verdict. Use this exactly once as the final review action.",
  parameters: preflightResultSchema,
  async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
    const result = validatePreflightResult(params);
    const resultPath = process.env.SCUFRIS_REVIEW_RESULT;
    if (!resultPath)
      throw new Error("Scufris review result path is unavailable");
    const encoded = `${JSON.stringify(result)}\n`;
    if (Buffer.byteLength(encoded, "utf8") > MAX_RESULT_BYTES) {
      throw new Error("Scufris review result exceeds 64 KiB");
    }
    writeFileSync(resultPath, encoded, {
      encoding: "utf8",
      flag: "wx",
      mode: 0o600,
    });
    ctx.shutdown();
    return {
      content: [{ type: "text" as const, text: encoded.trimEnd() }],
      details: result,
      terminate: true,
    };
  },
});

export default function preflightReviewer(pi: ExtensionAPI): void {
  pi.registerTool(submitPreflightTool);
}
