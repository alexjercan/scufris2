import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  approveWalkthrough,
  changeRequestInstruction,
  encodeBridgeInit,
  encodeBridgeResult,
  initialWalkthroughState,
  parseBridgeAction,
  parseWalkthrough,
  saveWalkthroughState,
  startWalkthroughServer,
  type WalkthroughState,
  validateWalkthroughState,
} from "../extensions/scufris/walkthrough.ts";
import { approvalInstruction } from "../extensions/scufris/agents.ts";
import { submitWalkthroughTool } from "../extensions/scufris/walkthrough-reviewer.ts";

const base = "1".repeat(40);
const revision = "2".repeat(40);
const valid = `# Safe <script>alert(1)</script>

What was built with **ordinary Markdown**.

:::walkthrough
status: ready
revision: ${revision}
baseRevision: ${base}
files: 1
added: 2
removed: 1
preflight: passed
:::

## Runtime filtering

:::change
id: runtime-filter
importance: critical
file: src/actions.ts
lines: 42-61
:::

Code-confirmed fact: filtering occurs before return.

\`\`\`diff
-return all;
+return safe;
\`\`\`

:::review
Verify AND semantics.
:::
`;

function postAction(url: string, body: string | URLSearchParams) {
  const values =
    body instanceof URLSearchParams ? body : new URLSearchParams(body);
  return fetch(new URL("action", url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(Object.fromEntries(values)),
  });
}

test("quick review bridge accepts only bounded typed actions", () => {
  assert.deepEqual(
    parseBridgeAction(
      JSON.stringify({
        type: "action",
        id: "a".repeat(24),
        action: "mark-viewed",
        section: "runtime-filter",
      }),
    ),
    {
      type: "action",
      id: "a".repeat(24),
      action: "mark-viewed",
      section: "runtime-filter",
    },
  );
  assert.throws(() => parseBridgeAction("not json"));
  assert.throws(
    () =>
      parseBridgeAction(
        JSON.stringify({ type: "action", id: "bad", action: "approve" }),
      ),
    /schema/,
  );
  assert.throws(() => parseBridgeAction("x".repeat(512 * 1024 + 1)), /exceeds/);
});

test("escape-heavy parser-valid walkthrough fits the explicit init wire contract", async () => {
  const document = parseWalkthrough(
    valid.replace("-return all;\n+return safe;", `+${"\x01".repeat(100_000)}`),
  );
  const state = initialWalkthroughState(document);
  const encoded = encodeBridgeInit(document, state);
  assert.ok(Buffer.byteLength(encoded) > 512 * 1024);
  assert.ok(Buffer.byteLength(encoded) < 4 * 1024 * 1024);
  assert.throws(
    () =>
      encodeBridgeInit(
        {
          ...document,
          sections: [
            { ...document.sections[0]!, diff: "\x01".repeat(700_000) },
          ],
        },
        state,
      ),
    /exceeds 4 MiB/,
  );
  const server = await startWalkthroughServer(
    document,
    state,
    {
      verify: async () => undefined,
      persist: () => undefined,
      explain: async () => "answer",
      requestChanges: async () => undefined,
      fullDiff: async () => undefined,
      approved: async () => undefined,
      context: () => "context",
    },
    { openBrowser: false },
  );
  await server.close();
});

test("large schema-valid question history fits the result wire contract", async () => {
  const document = parseWalkthrough(valid);
  const state = initialWalkthroughState(document);
  const boundedText = '\\\"'.repeat(8192);
  const server = await startWalkthroughServer(
    document,
    state,
    {
      verify: async () => undefined,
      persist: () => undefined,
      explain: async () => "answer",
      requestChanges: async () => undefined,
      fullDiff: async () => undefined,
      approved: async () => undefined,
      context: () => "context",
    },
    { openBrowser: false },
  );
  try {
    state.questions = Array.from({ length: 100 }, () => ({
      sectionId: "runtime-filter",
      question: boundedText,
      answer: boundedText,
    }));
    validateWalkthroughState(document, state);
    const encoded = encodeBridgeResult({
      type: "result",
      id: "a".repeat(24),
      ok: true,
      state,
      message: "Review updated.",
    });
    assert.ok(Buffer.byteLength(encoded) > 512 * 1024);
    assert.ok(Buffer.byteLength(encoded) < 32 * 1024 * 1024);
    assert.throws(
      () => encodeBridgeResult({ message: "\x01".repeat(6 * 1024 * 1024) }),
      /exceeds 32 MiB/,
    );
    const response = await postAction(
      server.url,
      "action=mark-viewed&section=runtime-filter",
    );
    assert.equal(response.status, 200);
    const result = (await response.json()) as {
      ok: boolean;
      state: WalkthroughState;
    };
    assert.equal(result.ok, true);
    assert.equal(result.state.questions.length, 100);
  } finally {
    await server.close();
  }
});

test("strict walkthrough parsing binds revisions and preserves literal diff", () => {
  const document = parseWalkthrough(valid);
  assert.equal(document.revision, revision);
  assert.equal(document.sections[0]?.diff, "-return all;\n+return safe;");
  assert.deepEqual(document.warnings, []);
  const state = initialWalkthroughState(document);
  assert.throws(() => approveWalkthrough(state), /all sections/);
  state.sections["runtime-filter"] = "looks-good";
  state.viewed["runtime-filter"] = true;
  approveWalkthrough(state);
  assert.equal(validateWalkthroughState(document, state).approved, true);
});

test("malformed and unsupported directives warn", () => {
  const malformed = valid
    .replace("lines: 42-61", "lines: ../bad")
    .replace(
      "## Runtime filtering",
      ":::widget\nurl: javascript:alert(1)\n:::\n\n## Runtime filtering",
    );
  assert.throws(() => parseWalkthrough(malformed), /no valid changes/);
  const document = parseWalkthrough(
    valid.replace(
      "## Runtime filtering",
      ":::widget\nurl: javascript:alert(1)\n:::\n\n## Runtime filtering",
    ),
  );
  assert.deepEqual(document.warnings, ["Unsupported directive: widget"]);
});

test("persisted review state is separate and stale identities fail closed", async () => {
  const root = await mkdtemp(join(tmpdir(), "scufris-walkthrough-"));
  try {
    const document = parseWalkthrough(valid);
    const state = initialWalkthroughState(document);
    const path = join(root, "state.json");
    saveWalkthroughState(path, state);
    assert.equal(
      JSON.parse(await readFile(path, "utf8")).identity,
      document.identity,
    );
    assert.throws(
      () => validateWalkthroughState(document, { ...state, revision: base }),
      /does not match/,
    );
    const target = join(root, "target.json");
    const link = join(root, "linked-state.json");
    await writeFile(target, "unchanged");
    await symlink(target, link);
    assert.throws(() => saveWalkthroughState(link, state), /ELOOP/);
    assert.equal(await readFile(target, "utf8"), "unchanged");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("local review server routes bounded actions and approval guard", async () => {
  const document = parseWalkthrough(valid);
  const state = initialWalkthroughState(document);
  let approvals = 0;
  let approvalFailure = true;
  const server = await startWalkthroughServer(
    document,
    state,
    {
      verify: async () => undefined,
      persist: () => undefined,
      explain: async (_section, question) => `Answer to ${question}`,
      requestChanges: async () => undefined,
      fullDiff: async () => undefined,
      approved: async () => {
        if (approvalFailure) throw new Error("finalization failed");
        approvals++;
      },
      context: () =>
        "function example(): void {\n    const indented = '<unsafe>';\n}\n",
    },
    { openBrowser: false },
  );
  try {
    const action = (body: string) => postAction(server.url, body);
    assert.equal((await action("action=approve")).status, 400);
    assert.equal(
      (
        await action(
          "action=ask&section=runtime-filter&comment=%3Cimg%20onerror%3Dx%3E",
        )
      ).status,
      200,
    );
    const pageResponse = await fetch(server.url);
    const page = await pageResponse.text();
    assert.match(page, /&lt;script&gt;alert/);
    assert.doesNotMatch(page, /<script>alert/);
    assert.match(page, /diff-del/);
    assert.match(page, /diff-add/);
    assert.match(page, /<pre class="context-view"[^>]*><code><\/code><\/pre>/);
    const script = await (await fetch(new URL("app.js", server.url))).text();
    assert.match(script, /textContent=result\.context/);
    const contextResponse = await action(
      "action=context&section=runtime-filter",
    );
    assert.equal(contextResponse.status, 200);
    const contextResult = (await contextResponse.json()) as {
      message: string;
      context: string;
    };
    assert.equal(contextResult.message, "Exact-revision context loaded.");
    assert.equal(
      contextResult.context,
      "function example(): void {\n    const indented = '<unsafe>';\n}\n",
    );
    assert.match(
      pageResponse.headers.get("content-security-policy") ?? "",
      /default-src 'none'/,
    );
    assert.equal(
      (await action("action=mark-viewed&section=runtime-filter")).status,
      200,
    );
    assert.equal((await action("action=approve")).status, 400);
    assert.equal(state.approved, false);
    approvalFailure = false;
    assert.equal((await action("action=approve")).status, 400);
    assert.equal(approvals, 0);
  } finally {
    await server.close();
  }
});

test("viewed sections collapse, reopen, and approve with anchored comments", async () => {
  const document = parseWalkthrough(valid);
  const state = initialWalkthroughState(document);
  const approvals: string[] = [];
  let changeRequests = 0;
  const server = await startWalkthroughServer(
    document,
    state,
    {
      verify: async () => undefined,
      persist: (next) => validateWalkthroughState(document, next),
      explain: async () => "answer",
      requestChanges: async () => {
        changeRequests++;
      },
      fullDiff: async () => undefined,
      approved: async (comments) => {
        approvals.push(approvalInstruction(comments));
      },
      context: () => "context",
    },
    { openBrowser: false },
  );
  try {
    const action = (body: string | URLSearchParams) =>
      postAction(server.url, body);
    const note = new URLSearchParams({
      action: "add-comment",
      section: "runtime-filter",
      comment: "Keep this behavior documented. <script>alert(1)</script>",
    });
    assert.equal((await action(note)).status, 200);
    assert.equal(state.comments.length, 1);
    assert.deepEqual(
      {
        sectionId: state.comments[0]!.sectionId,
        file: state.comments[0]!.file,
        lines: state.comments[0]!.lines,
      },
      {
        sectionId: "runtime-filter",
        file: "src/actions.ts",
        lines: "42-61",
      },
    );
    assert.match(state.comments[0]!.id, /^[0-9a-f]{24}$/);
    assert.equal(
      (await action("action=mark-viewed&section=runtime-filter")).status,
      200,
    );
    let page = await (await fetch(server.url)).text();
    assert.match(page, /card viewed/);
    assert.match(page, /data-action="reopen"/);
    assert.match(page, /Approve with comments/);
    assert.doesNotMatch(page, /Create follow-up task/);
    assert.doesNotMatch(page, /<script>alert\(1\)<\/script>/);
    assert.equal(
      (await action("action=reopen&section=runtime-filter")).status,
      200,
    );
    assert.equal(state.sections["runtime-filter"], "not-reviewed");
    assert.equal((await action("action=approve-with-comments")).status, 400);
    await action("action=mark-viewed&section=runtime-filter");
    assert.equal((await action("action=approve")).status, 400);
    assert.equal((await action("action=approve-with-comments")).status, 200);
    assert.equal(approvals.length, 1);
    assert.equal(changeRequests, 0);
    assert.match(approvals[0]!, /Keep this behavior documented/);
    assert.match(approvals[0]!, /src\/actions\.ts:42-61/);
  } finally {
    await server.close();
  }
});

test("comment admission budgets the exact escaped approval instruction", async () => {
  const document = parseWalkthrough(valid);
  const state = initialWalkthroughState(document);
  let persisted = 0;
  const server = await startWalkthroughServer(
    document,
    state,
    {
      verify: async () => undefined,
      persist: () => void persisted++,
      explain: async () => "answer",
      requestChanges: async () => undefined,
      fullDiff: async () => undefined,
      approved: async () => undefined,
      context: () => "context",
    },
    { openBrowser: false },
  );
  try {
    const escapeHeavy = '\\"'.repeat(2048);
    const add = () =>
      postAction(
        server.url,
        new URLSearchParams({
          action: "add-comment",
          section: "runtime-filter",
          comment: escapeHeavy,
        }),
      );
    assert.equal((await add()).status, 200);
    assert.equal(persisted, 1);
    assert.equal(state.comments.length, 1);
    assert.equal((await add()).status, 400);
    assert.equal(persisted, 1);
    assert.equal(state.comments.length, 1);
    const overflow = {
      ...state,
      comments: [
        ...state.comments,
        { ...state.comments[0]!, id: "f".repeat(24) },
      ],
    };
    assert.throws(
      () => validateWalkthroughState(document, overflow),
      /approval notes exceed/,
    );
  } finally {
    await server.close();
  }
});

test("in-flight explanation rechecks stale ownership before persistence", async () => {
  const document = parseWalkthrough(valid);
  const state = initialWalkthroughState(document);
  let active = true;
  let persisted = 0;
  const server = await startWalkthroughServer(
    document,
    state,
    {
      verify: async () => {
        if (!active) throw new Error("walkthrough is no longer active");
      },
      persist: () => void persisted++,
      explain: async () => {
        active = false;
        return "stale answer";
      },
      requestChanges: async () => undefined,
      fullDiff: async () => undefined,
      approved: async () => undefined,
      context: () => "context",
    },
    { openBrowser: false },
  );
  try {
    const response = await postAction(
      server.url,
      "action=explain&section=runtime-filter",
    );
    assert.equal(response.status, 400);
    assert.equal(persisted, 0);
    assert.equal(state.questions.length, 0);
  } finally {
    await server.close();
  }
});

test("blocking feedback is serialized and budgeted before persistence", async () => {
  const document = parseWalkthrough(valid);
  const state = initialWalkthroughState(document);
  let persisted = 0;
  let routed = "";
  const server = await startWalkthroughServer(
    document,
    state,
    {
      verify: async () => undefined,
      persist: () => void persisted++,
      explain: async () => "answer",
      requestChanges: async (message) => void (routed = message),
      fullDiff: async () => undefined,
      approved: async () => undefined,
      context: () => "context",
    },
    { openBrowser: false },
  );
  try {
    const post = (comment: string) =>
      postAction(
        server.url,
        new URLSearchParams({
          action: "request-change",
          section: "runtime-filter",
          comment,
        }),
      );
    assert.equal((await post(`${"a".repeat(4095)}é`)).status, 400);
    assert.equal(persisted, 0);
    assert.equal((await post("line one\nline two")).status, 200);
    for (let index = 0; index < 3; index++)
      assert.equal((await post("x".repeat(4090))).status, 200);
    assert.equal(persisted, 4);
    assert.equal(state.changeRequests.length, 4);
    assert.equal((await post("x".repeat(4090))).status, 400);
    assert.equal(persisted, 4);
    assert.equal(state.changeRequests.length, 4);
    assert.equal(
      (await postAction(server.url, "action=request-changes")).status,
      200,
    );
    assert.equal(routed, changeRequestInstruction(state.changeRequests));
    assert.doesNotMatch(routed, /[\r\n]/);
    assert.match(routed, /line one\\nline two/);
  } finally {
    await server.close();
  }

  const overflow = initialWalkthroughState(document);
  overflow.changeRequests = Array.from({ length: 4 }, () => ({
    sectionId: "runtime-filter",
    feedback: "x".repeat(4090),
  }));
  assert.throws(
    () => validateWalkthroughState(document, overflow),
    /blocking feedback exceeds/,
  );
});

test("approval persistence completes before routing and can retry on local failure", async () => {
  const document = parseWalkthrough(valid);
  const state = initialWalkthroughState(document);
  state.sections["runtime-filter"] = "looks-good";
  state.viewed["runtime-filter"] = true;
  let persistenceFails = true;
  const sequence: string[] = [];
  const server = await startWalkthroughServer(
    document,
    state,
    {
      verify: async () => undefined,
      persist: () => {
        sequence.push("persist");
        if (persistenceFails) throw new Error("disk full");
      },
      explain: async () => "answer",
      requestChanges: async () => undefined,
      fullDiff: async () => undefined,
      approved: async () => void sequence.push("approved"),
      context: () => "context",
    },
    { openBrowser: false },
  );
  try {
    const approve = () => postAction(server.url, "action=approve");
    assert.equal((await approve()).status, 400);
    assert.deepEqual(sequence, ["persist"]);
    assert.equal(state.approved, false);
    persistenceFails = false;
    assert.equal((await approve()).status, 200);
    assert.deepEqual(sequence, ["persist", "persist", "approved"]);
  } finally {
    await server.close();
  }
});

test("local review server serializes one terminal action", async () => {
  const document = parseWalkthrough(valid);
  const state = initialWalkthroughState(document);
  state.sections["runtime-filter"] = "looks-good";
  state.viewed["runtime-filter"] = true;
  let releaseApproval!: () => void;
  let approvalStarted!: () => void;
  const started = new Promise<void>((resolve) => (approvalStarted = resolve));
  const gate = new Promise<void>((resolve) => (releaseApproval = resolve));
  let changeRequests = 0;
  const server = await startWalkthroughServer(
    document,
    state,
    {
      verify: async () => undefined,
      persist: () => undefined,
      explain: async () => "answer",
      requestChanges: async () => void changeRequests++,
      fullDiff: async () => undefined,
      approved: async () => {
        approvalStarted();
        await gate;
      },
      context: () => "context",
    },
    { openBrowser: false },
  );
  try {
    const post = (body: string) => postAction(server.url, body);
    const approval = post("action=approve");
    await started;
    const change = post("action=request-changes");
    releaseApproval();
    assert.equal((await approval).status, 200);
    assert.equal((await change).status, 400);
    assert.equal(changeRequests, 0);
  } finally {
    await server.close();
  }
});

test("terminal callbacks can close the production server after correlated results", async () => {
  for (const terminalAction of ["approve", "request-changes"] as const) {
    const document = parseWalkthrough(valid);
    const state = initialWalkthroughState(document);
    if (terminalAction === "approve") {
      state.sections["runtime-filter"] = "looks-good";
      state.viewed["runtime-filter"] = true;
    } else {
      state.sections["runtime-filter"] = "change-requested";
      state.changeRequests.push({
        sectionId: "runtime-filter",
        feedback: "Change this",
      });
    }
    let server!: Awaited<ReturnType<typeof startWalkthroughServer>>;
    server = await startWalkthroughServer(
      document,
      state,
      {
        verify: async () => undefined,
        persist: () => undefined,
        explain: async () => "answer",
        requestChanges: async () => void server.close(),
        fullDiff: async () => undefined,
        approved: async () => void server.close(),
        context: () => "context",
      },
      { openBrowser: false },
    );
    const response = await postAction(
      server.url,
      terminalAction === "approve"
        ? "action=approve"
        : "action=request-changes",
    );
    assert.equal(response.status, 200);
    const result = (await response.json()) as {
      ok: boolean;
      state: WalkthroughState;
    };
    assert.equal(result.ok, true);
    assert.equal(
      terminalAction === "approve"
        ? result.state.approved
        : result.state.sections["runtime-filter"] === "change-requested",
      true,
    );
    await server.close();
  }
});

test("walkthrough reviewer completion validates before writing once", async () => {
  const root = await mkdtemp(join(tmpdir(), "scufris-walkthrough-tool-"));
  const path = join(root, "result.json");
  const oldPath = process.env.SCUFRIS_WALKTHROUGH_RESULT;
  const oldRevision = process.env.SCUFRIS_WALKTHROUGH_REVISION;
  let shutdowns = 0;
  try {
    process.env.SCUFRIS_WALKTHROUGH_RESULT = path;
    process.env.SCUFRIS_WALKTHROUGH_REVISION = revision;
    const params = { revision, markdown: valid, sectionCount: 1 };
    await submitWalkthroughTool.execute("call", params, undefined, undefined, {
      shutdown: () => shutdowns++,
    } as never);
    assert.equal(shutdowns, 1);
    assert.equal(JSON.parse(await readFile(path, "utf8")).revision, revision);
    await assert.rejects(
      submitWalkthroughTool.execute("call", params, undefined, undefined, {
        shutdown: () => undefined,
      } as never),
      /EEXIST/,
    );
  } finally {
    if (oldPath === undefined) delete process.env.SCUFRIS_WALKTHROUGH_RESULT;
    else process.env.SCUFRIS_WALKTHROUGH_RESULT = oldPath;
    if (oldRevision === undefined)
      delete process.env.SCUFRIS_WALKTHROUGH_REVISION;
    else process.env.SCUFRIS_WALKTHROUGH_REVISION = oldRevision;
    await rm(root, { recursive: true, force: true });
  }
});
