import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { submitPreflightTool } from "../extensions/scufris/preflight-reviewer.ts";

const finding = {
  severity: "MAJOR" as const,
  path: "src/review.ts",
  line: 12,
  reason: "The result can be lost.",
  change: "Persist it before shutdown.",
};

async function submit(value: unknown, shutdown: () => void) {
  return submitPreflightTool.execute(
    "call",
    value as never,
    undefined,
    undefined,
    { shutdown } as never,
  );
}

test("invalid reviewer results leave the result channel available for retry", async () => {
  const root = await mkdtemp(join(tmpdir(), "scufris-preflight-invalid-"));
  const resultPath = join(root, "result.json");
  const previous = process.env.SCUFRIS_REVIEW_RESULT;
  let shutdowns = 0;
  const invalid = [
    { verdict: "approve", findings: [finding] },
    { verdict: "request_changes", findings: [] },
    {
      verdict: "request_changes",
      findings: [{ ...finding, path: "/src/review.ts" }],
    },
    {
      verdict: "request_changes",
      findings: [{ ...finding, path: "src/../review.ts" }],
    },
    {
      verdict: "request_changes",
      findings: [{ ...finding, path: "src\\review.ts" }],
    },
    {
      verdict: "request_changes",
      findings: [{ ...finding, reason: "line one\nline two" }],
    },
    {
      verdict: "request_changes",
      findings: [{ ...finding, change: "   " }],
    },
    {
      verdict: "request_changes",
      findings: [{ ...finding, reason: "é".repeat(1025) }],
    },
    {
      verdict: "request_changes",
      findings: [{ ...finding, reason: "invalid \ud800 unicode" }],
    },
    {
      verdict: "request_changes",
      findings: [{ ...finding, change: "\ud800" }],
    },
    {
      verdict: "request_changes",
      findings: Array.from({ length: 4 }, (_, index) => ({
        ...finding,
        path: `src/review-${index}.ts`,
        reason: "r".repeat(2048),
        change: "c".repeat(2048),
      })),
    },
  ];
  try {
    process.env.SCUFRIS_REVIEW_RESULT = resultPath;
    for (const value of invalid) {
      await assert.rejects(
        submit(value, () => void shutdowns++),
        /Scufris review/,
      );
      await assert.rejects(readFile(resultPath), /ENOENT/);
      assert.equal(shutdowns, 0);
    }

    const valid = { verdict: "request_changes", findings: [finding] };
    const output = await submit(valid, () => void shutdowns++);
    assert.equal(shutdowns, 1);
    assert.equal(output.terminate, true);
    assert.equal(
      await readFile(resultPath, "utf8"),
      `${JSON.stringify(valid)}\n`,
    );
  } finally {
    if (previous === undefined) delete process.env.SCUFRIS_REVIEW_RESULT;
    else process.env.SCUFRIS_REVIEW_RESULT = previous;
    await rm(root, { recursive: true, force: true });
  }
});

test("interactive reviewer tool writes one bounded result and requests shutdown", async () => {
  const root = await mkdtemp(join(tmpdir(), "scufris-preflight-result-"));
  const resultPath = join(root, "result.json");
  const previous = process.env.SCUFRIS_REVIEW_RESULT;
  let shutdown = false;
  try {
    process.env.SCUFRIS_REVIEW_RESULT = resultPath;
    const params = { verdict: "request_changes", findings: [finding] };
    const output = await submit(params, () => void (shutdown = true));

    assert.equal(shutdown, true);
    assert.equal(output.terminate, true);
    assert.equal(
      await readFile(resultPath, "utf8"),
      `${JSON.stringify(params)}\n`,
    );
    assert.equal(output.content[0]?.type, "text");
    assert.equal(
      output.content[0]?.type === "text" ? output.content[0].text : undefined,
      JSON.stringify(params),
    );
    await assert.rejects(
      submit(params, () => undefined),
      /EEXIST/,
    );
  } finally {
    if (previous === undefined) delete process.env.SCUFRIS_REVIEW_RESULT;
    else process.env.SCUFRIS_REVIEW_RESULT = previous;
    await rm(root, { recursive: true, force: true });
  }
});
