import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  approveWalkthrough,
  initialWalkthroughState,
  parseWalkthrough,
  renderWalkthrough,
  saveWalkthroughState,
  startWalkthroughServer,
  validateWalkthroughState,
} from "../extensions/scufris/walkthrough.ts";
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

test("strict walkthrough parsing binds revisions and preserves literal diff", () => {
  const document = parseWalkthrough(valid);
  assert.equal(document.revision, revision);
  assert.equal(document.sections[0]?.diff, "-return all;\n+return safe;");
  assert.deepEqual(document.warnings, []);
  const state = initialWalkthroughState(document);
  assert.throws(() => approveWalkthrough(state), /all sections/);
  state.sections["runtime-filter"] = "looks-good";
  approveWalkthrough(state);
  assert.equal(validateWalkthroughState(document, state).approved, true);
});

test("malformed and unsupported directives warn without HTML injection", () => {
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
  const html = renderWalkthrough(
    document,
    initialWalkthroughState(document),
    "token",
  );
  assert.doesNotMatch(html, /<script>alert/);
  assert.match(html, /&lt;script&gt;alert/);
  assert.doesNotMatch(html, /javascript:alert/);
  assert.match(html, /default-src 'none'/);
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
  const server = await startWalkthroughServer(document, state, {
    verify: async () => undefined,
    persist: () => undefined,
    explain: async (_section, question) => `Answer to ${question}`,
    requestChanges: async () => undefined,
    fullDiff: async () => undefined,
    approved: async () => {
      if (approvalFailure) throw new Error("finalization failed");
      approvals++;
    },
    context: () => "<script>unsafe()</script>",
  });
  try {
    const action = async (body: string) =>
      fetch(new URL("action", server.url), {
        method: "POST",
        redirect: "manual",
        headers: { "content-type": "application/x-www-form-urlencoded" },
        body,
      });
    assert.equal((await action("action=approve")).status, 400);
    assert.equal(
      (
        await action(
          "action=ask&section=runtime-filter&comment=%3Cimg%20onerror%3Dx%3E",
        )
      ).status,
      303,
    );
    const page = await (await fetch(server.url)).text();
    assert.match(page, /&lt;img onerror=x&gt;/);
    assert.doesNotMatch(page, /<img onerror/);
    assert.equal(
      (await action("action=looks-good&section=runtime-filter")).status,
      303,
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

test("in-flight explanation rechecks stale ownership before persistence", async () => {
  const document = parseWalkthrough(valid);
  const state = initialWalkthroughState(document);
  let active = true;
  let persisted = 0;
  const server = await startWalkthroughServer(document, state, {
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
  });
  try {
    const response = await fetch(new URL("action", server.url), {
      method: "POST",
      redirect: "manual",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: "action=explain&section=runtime-filter",
    });
    assert.equal(response.status, 400);
    assert.equal(persisted, 0);
    assert.equal(state.questions.length, 0);
  } finally {
    await server.close();
  }
});

test("invalid terminal input releases the review for a corrected action", async () => {
  const document = parseWalkthrough(valid);
  const state = initialWalkthroughState(document);
  let changeRequests = 0;
  const server = await startWalkthroughServer(document, state, {
    verify: async () => undefined,
    persist: () => undefined,
    explain: async () => "answer",
    requestChanges: async () => void changeRequests++,
    fullDiff: async () => undefined,
    approved: async () => undefined,
    context: () => "context",
  });
  try {
    const post = (comment: string) =>
      fetch(new URL("action", server.url), {
        method: "POST",
        redirect: "manual",
        headers: { "content-type": "application/x-www-form-urlencoded" },
        body: new URLSearchParams({
          action: "request-change",
          section: "runtime-filter",
          comment,
        }),
      });
    assert.equal((await post(`${"a".repeat(4095)}é`)).status, 400);
    assert.equal(changeRequests, 0);
    assert.equal((await post("Use OR semantics.")).status, 303);
    assert.equal(changeRequests, 1);
  } finally {
    await server.close();
  }
});

test("approval persistence completes before routing and can retry on local failure", async () => {
  const document = parseWalkthrough(valid);
  const state = initialWalkthroughState(document);
  state.sections["runtime-filter"] = "looks-good";
  let persistenceFails = true;
  const sequence: string[] = [];
  const server = await startWalkthroughServer(document, state, {
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
  });
  try {
    const approve = () =>
      fetch(new URL("action", server.url), {
        method: "POST",
        redirect: "manual",
        headers: { "content-type": "application/x-www-form-urlencoded" },
        body: "action=approve",
      });
    assert.equal((await approve()).status, 400);
    assert.deepEqual(sequence, ["persist"]);
    assert.equal(state.approved, false);
    persistenceFails = false;
    assert.equal((await approve()).status, 303);
    assert.deepEqual(sequence, ["persist", "persist", "approved"]);
  } finally {
    await server.close();
  }
});

test("local review server serializes one terminal action", async () => {
  const document = parseWalkthrough(valid);
  const state = initialWalkthroughState(document);
  state.sections["runtime-filter"] = "looks-good";
  let releaseApproval!: () => void;
  let approvalStarted!: () => void;
  const started = new Promise<void>((resolve) => (approvalStarted = resolve));
  const gate = new Promise<void>((resolve) => (releaseApproval = resolve));
  let changeRequests = 0;
  const server = await startWalkthroughServer(document, state, {
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
  });
  try {
    const post = (body: string) =>
      fetch(new URL("action", server.url), {
        method: "POST",
        redirect: "manual",
        headers: { "content-type": "application/x-www-form-urlencoded" },
        body,
      });
    const approval = post("action=approve");
    await started;
    const change = post("action=request-changes");
    releaseApproval();
    assert.equal((await approval).status, 303);
    assert.equal((await change).status, 400);
    assert.equal(changeRequests, 0);
  } finally {
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
