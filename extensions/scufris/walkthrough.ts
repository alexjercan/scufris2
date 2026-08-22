import { createHash, randomBytes } from "node:crypto";
import {
  createServer,
  type IncomingMessage,
  type ServerResponse,
} from "node:http";
import {
  closeSync,
  constants,
  fstatSync,
  openSync,
  readFileSync,
  writeFileSync,
} from "node:fs";

const MAX_MARKDOWN_BYTES = 256 * 1024;
const MAX_SECTIONS = 40;
const SHA = /^[0-9a-f]{40}$/;
const ID = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const SAFE_PATH =
  /^(?!\/)(?!.*(?:^|\/)\.\.?(?:\/|$))(?!.*\\)[^\x00-\x1f\x7f]+$/;
const LINES = /^(?:[1-9][0-9]*)(?:-[1-9][0-9]*)?$/;

export type SectionState =
  | "not-reviewed"
  | "looks-good"
  | "needs-explanation"
  | "change-requested";
export interface WalkthroughSection {
  id: string;
  importance: "critical" | "important" | "supporting";
  file: string;
  lines: string;
  markdown: string;
  diff: string;
  prompt: string;
}
export interface WalkthroughDocument {
  title: string;
  revision: string;
  baseRevision: string;
  files: number;
  added: number;
  removed: number;
  preflight: "passed";
  sections: WalkthroughSection[];
  warnings: string[];
  source: string;
  identity: string;
}
export interface WalkthroughState {
  version: 1;
  identity: string;
  revision: string;
  sections: Record<string, SectionState>;
  questions: Array<{ sectionId: string; question: string; answer?: string }>;
  changeRequests: Array<{ sectionId: string; feedback: string }>;
  approved: boolean;
}

function fields(text: string): Record<string, string> | undefined {
  const result: Record<string, string> = {};
  for (const line of text.split("\n")) {
    const match = /^([a-zA-Z][a-zA-Z0-9]*): ([^\r\n]+)$/.exec(line);
    if (!match) return undefined;
    const key = match[1]!;
    if (key in result) return undefined;
    result[key] = match[2]!;
  }
  return result;
}
function exact(value: Record<string, string>, names: string[]): boolean {
  return Object.keys(value).sort().join("\0") === [...names].sort().join("\0");
}
function uint(value: string): number | undefined {
  return /^(?:0|[1-9][0-9]{0,8})$/.test(value) ? Number(value) : undefined;
}

export function parseWalkthrough(source: string): WalkthroughDocument {
  if (Buffer.byteLength(source, "utf8") > MAX_MARKDOWN_BYTES)
    throw new Error("walkthrough exceeds 256 KiB");
  const warnings: string[] = [];
  const top =
    /^# ([^\r\n]{1,200})\n[\s\S]*?^:::walkthrough\n([\s\S]*?)\n:::\s*$/m.exec(
      source,
    );
  if (!top) throw new Error("walkthrough metadata is missing");
  const metadata = fields(top[2]!);
  const expected = [
    "status",
    "revision",
    "baseRevision",
    "files",
    "added",
    "removed",
    "preflight",
  ];
  if (!metadata || !exact(metadata, expected))
    throw new Error("walkthrough metadata schema is invalid");
  const status = metadata.status!;
  const preflight = metadata.preflight!;
  const revision = metadata.revision!;
  const baseRevision = metadata.baseRevision!;
  const files = uint(metadata.files!),
    added = uint(metadata.added!),
    removed = uint(metadata.removed!);
  if (
    status !== "ready" ||
    preflight !== "passed" ||
    !SHA.test(revision) ||
    !SHA.test(baseRevision) ||
    files === undefined ||
    added === undefined ||
    removed === undefined
  )
    throw new Error("walkthrough metadata values are invalid");

  const blocks = [
    ...source.matchAll(/^:::([a-zA-Z][a-zA-Z0-9-]*)\n([\s\S]*?)\n:::\s*$/gm),
  ];
  for (const block of blocks)
    if (!["walkthrough", "change", "review"].includes(block[1]!))
      warnings.push(`Unsupported directive: ${block[1]}`);
  const sections: WalkthroughSection[] = [];
  const changes = [...source.matchAll(/^:::change\n([\s\S]*?)\n:::\s*$/gm)];
  for (const [index, change] of changes.entries()) {
    if (sections.length >= MAX_SECTIONS)
      throw new Error("walkthrough has more than 40 changes");
    const value = fields(change[1]!);
    if (
      !value ||
      !exact(value, ["id", "importance", "file", "lines"]) ||
      !ID.test(value.id ?? "") ||
      !["critical", "important", "supporting"].includes(
        value.importance ?? "",
      ) ||
      !SAFE_PATH.test(value.file ?? "") ||
      Buffer.byteLength(value.file ?? "", "utf8") > 512 ||
      !LINES.test(value.lines ?? "")
    ) {
      warnings.push(`Malformed change directive ${index + 1}`);
      continue;
    }
    if (sections.some((section) => section.id === value.id)) {
      warnings.push(`Duplicate change id: ${value.id}`);
      continue;
    }
    const start = (change.index ?? 0) + change[0].length;
    const end = changes[index + 1]?.index ?? source.length;
    const body = source.slice(start, end);
    const diffMatches = [...body.matchAll(/```diff\n([\s\S]*?)\n```/g)];
    const reviewMatches = [
      ...body.matchAll(/^:::review\n([\s\S]*?)\n:::\s*$/gm),
    ];
    const diff = diffMatches[0]?.[1];
    const prompt = reviewMatches[0]?.[1];
    if (
      diffMatches.length !== 1 ||
      reviewMatches.length !== 1 ||
      !diff ||
      !prompt?.trim() ||
      Buffer.byteLength(prompt, "utf8") > 4096
    ) {
      warnings.push(`Change ${value.id} needs one diff and one review prompt`);
      continue;
    }
    sections.push({
      id: value.id!,
      importance: value.importance as WalkthroughSection["importance"],
      file: value.file!,
      lines: value.lines!,
      markdown: body
        .replace(/```diff\n[\s\S]*?\n```/g, "")
        .replace(/^:::review\n[\s\S]*?\n:::\s*$/gm, "")
        .trim(),
      diff,
      prompt: prompt.trim(),
    });
  }
  if (sections.length === 0)
    throw new Error("walkthrough has no valid changes");
  const identity = createHash("sha256").update(source).digest("hex");
  return {
    title: top[1]!,
    revision,
    baseRevision,
    files,
    added,
    removed,
    preflight: "passed",
    sections,
    warnings,
    source,
    identity,
  };
}

export function initialWalkthroughState(
  document: WalkthroughDocument,
): WalkthroughState {
  return {
    version: 1,
    identity: document.identity,
    revision: document.revision,
    sections: Object.fromEntries(
      document.sections.map((section) => [section.id, "not-reviewed"]),
    ),
    questions: [],
    changeRequests: [],
    approved: false,
  };
}
export function validateWalkthroughState(
  document: WalkthroughDocument,
  value: unknown,
): WalkthroughState {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error("review state is invalid");
  const state = value as WalkthroughState;
  if (
    Object.keys(state).sort().join("\0") !==
      [
        "approved",
        "changeRequests",
        "identity",
        "questions",
        "revision",
        "sections",
        "version",
      ].join("\0") ||
    state.version !== 1 ||
    state.identity !== document.identity ||
    state.revision !== document.revision ||
    typeof state.approved !== "boolean" ||
    !state.sections ||
    typeof state.sections !== "object" ||
    !Array.isArray(state.questions) ||
    !Array.isArray(state.changeRequests)
  )
    throw new Error("review state does not match the artifact");
  const expected = document.sections.map((section) => section.id).sort();
  if (
    Object.keys(state.sections).sort().join("\0") !== expected.join("\0") ||
    Object.values(state.sections).some(
      (item) =>
        ![
          "not-reviewed",
          "looks-good",
          "needs-explanation",
          "change-requested",
        ].includes(item),
    )
  )
    throw new Error("review section state is invalid");
  const validText = (item: unknown) =>
    typeof item === "string" &&
    item.trim().length > 0 &&
    Buffer.byteLength(item, "utf8") <= 16 * 1024 &&
    !/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/.test(item);
  if (
    state.questions.length > 100 ||
    state.questions.some(
      (item) =>
        !item ||
        typeof item !== "object" ||
        !["answer", "question", "sectionId"].every(
          (key) => key in item || key === "answer",
        ) ||
        Object.keys(item).some(
          (key) => !["answer", "question", "sectionId"].includes(key),
        ) ||
        !(item.sectionId in state.sections) ||
        !validText(item.question) ||
        (item.answer !== undefined && !validText(item.answer)),
    ) ||
    state.changeRequests.length > MAX_SECTIONS ||
    state.changeRequests.some(
      (item) =>
        !item ||
        typeof item !== "object" ||
        Object.keys(item).sort().join("\0") !== "feedback\0sectionId" ||
        !(item.sectionId in state.sections) ||
        !validText(item.feedback),
    )
  )
    throw new Error("review questions or change requests are invalid");
  if (
    state.approved &&
    Object.values(state.sections).some((item) => item !== "looks-good")
  )
    throw new Error("approval has unresolved sections");
  return state;
}
export function applySectionAction(
  state: WalkthroughState,
  sectionId: string,
  action: "looks-good" | "explain" | "request-change",
): void {
  if (!(sectionId in state.sections)) throw new Error("unknown review section");
  state.approved = false;
  state.sections[sectionId] =
    action === "looks-good"
      ? "looks-good"
      : action === "explain"
        ? "needs-explanation"
        : "change-requested";
}
export function approveWalkthrough(state: WalkthroughState): void {
  if (Object.values(state.sections).some((item) => item !== "looks-good"))
    throw new Error("all sections must be marked Looks good before approval");
  state.approved = true;
}

function escapeHtml(value: string): string {
  return value.replace(
    /[&<>"']/g,
    (char) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        char
      ]!,
  );
}
function readableMarkdown(value: string): string {
  return value
    .split(/\n\n+/)
    .map((part) => `<p>${escapeHtml(part).replace(/\n/g, "<br>")}</p>`)
    .join("");
}
export function renderWalkthrough(
  document: WalkthroughDocument,
  state: WalkthroughState,
  token: string,
): string {
  const reviewed = Object.values(state.sections).filter(
    (value) => value === "looks-good",
  ).length;
  const cards = document.sections
    .map((section) => {
      const answers = state.questions
        .filter((item) => item.sectionId === section.id && item.answer)
        .map(
          (item) =>
            `<aside><strong>${escapeHtml(item.question)}</strong>${readableMarkdown(item.answer!)}</aside>`,
        )
        .join("");
      return `<article><h2>${escapeHtml(section.id)} <small>${section.importance}</small></h2><p><code>${escapeHtml(section.file)}:${escapeHtml(section.lines)}</code></p>${readableMarkdown(section.markdown)}<pre><code>${escapeHtml(section.diff)}</code></pre><blockquote>${escapeHtml(section.prompt)}</blockquote>${answers}<p>State: <strong>${escapeHtml(state.sections[section.id]!)}</strong></p><form method="post" action="/${token}/action"><input type="hidden" name="section" value="${escapeHtml(section.id)}"><button name="action" value="looks-good">Looks good</button><button name="action" value="explain">Explain</button><button name="action" value="request-change">Request change</button><button name="action" value="context">Show context / open file</button><label>Question or change details <input name="comment" maxlength="4096"></label><button name="action" value="ask">Ask reviewer</button></form></article>`;
    })
    .join("");
  return `<!doctype html><html><head><meta charset="utf-8"><meta name="referrer" content="no-referrer"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; base-uri 'none'"><title>${escapeHtml(document.title)}</title><style>body{font:16px system-ui;max-width:1000px;margin:auto;padding:2rem;background:#fafafa;color:#171717}article{background:white;border:1px solid #ddd;border-radius:8px;padding:1rem;margin:1rem 0}pre{overflow:auto;background:#111;color:#eee;padding:1rem}button{margin:.25rem;padding:.5rem}small{color:#666}.warning{color:#8a4b00}</style></head><body><h1>${escapeHtml(document.title)}</h1><p>${document.files} files, +${document.added}, -${document.removed}. Revision <code>${document.revision}</code>. Preflight passed.</p>${document.warnings.map((warning) => `<p class="warning">Warning: ${escapeHtml(warning)}</p>`).join("")} ${cards}<footer><h2>Final review</h2><p>${reviewed}/${document.sections.length} sections look good.</p><form method="post" action="/${token}/action"><button name="action" value="approve">Approve</button><button name="action" value="request-changes">Request changes</button><button name="action" value="full-diff">View full diff</button></form></footer></body></html>`;
}

export interface WalkthroughActions {
  verify(): Promise<void>;
  persist(state: WalkthroughState): void;
  explain(section: string, question: string): Promise<string>;
  requestChanges(feedback: string): Promise<void>;
  fullDiff(): Promise<void>;
  approved(): Promise<void>;
  context(section: WalkthroughSection): string;
}
export function startWalkthroughServer(
  document: WalkthroughDocument,
  state: WalkthroughState,
  actions: WalkthroughActions,
): Promise<{ url: string; close(): Promise<void> }> {
  const token = randomBytes(24).toString("hex");
  let actionTail = Promise.resolve();
  let terminal: "open" | "pending" | "committed" = "open";
  const server = createServer(
    async (request: IncomingMessage, response: ServerResponse) => {
      const send = (
        status: number,
        body: string,
        type = "text/html; charset=utf-8",
      ) => {
        response.writeHead(status, {
          "content-type": type,
          "cache-control": "no-store",
          "x-content-type-options": "nosniff",
          "content-security-policy":
            "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; base-uri 'none'",
        });
        response.end(body);
      };
      if (request.url === `/${token}/` && request.method === "GET")
        return send(200, renderWalkthrough(document, state, token));
      if (request.url !== `/${token}/action` || request.method !== "POST")
        return send(404, "Not found", "text/plain");
      let body = "";
      for await (const chunk of request) {
        body += chunk;
        if (Buffer.byteLength(body) > 16 * 1024) {
          request.destroy();
          return;
        }
      }
      const previousAction = actionTail;
      let releaseAction!: () => void;
      actionTail = new Promise<void>((resolve) => {
        releaseAction = resolve;
      });
      await previousAction;
      let rollbackPending: (() => void) | undefined;
      try {
        await actions.verify();
        const form = new URLSearchParams(body);
        const action = form.get("action") ?? "";
        const sectionId = form.get("section") ?? "";
        const section = document.sections.find((item) => item.id === sectionId);
        const sectionAction = [
          "looks-good",
          "explain",
          "request-change",
          "context",
          "ask",
        ].includes(action);
        const supportedAction =
          sectionAction ||
          ["approve", "request-changes", "full-diff"].includes(action);
        if (!supportedAction) throw new Error("unknown action");
        if (sectionAction && !section) throw new Error("unknown section");
        if (terminal !== "open")
          throw new Error("review already has a terminal action");
        const comment = (form.get("comment") ?? "").trim();
        if (Buffer.byteLength(comment, "utf8") > 4096)
          throw new Error("review comment exceeds 4 KiB");
        const feedback =
          action === "request-change" ? comment || section!.prompt : undefined;
        const finalFeedback =
          action === "request-changes"
            ? state.changeRequests
                .map((item) => `${item.sectionId}: ${item.feedback}`)
                .join("; ") || "Walkthrough review requested changes."
            : undefined;
        const terminalAction = [
          "request-change",
          "request-changes",
          "approve",
        ].includes(action);
        if (action === "approve") {
          approveWalkthrough(state);
          rollbackPending = () => void (state.approved = false);
        } else if (action === "request-change") {
          const previousSectionState = state.sections[sectionId]!;
          const previousRequestCount = state.changeRequests.length;
          rollbackPending = () => {
            state.sections[sectionId] = previousSectionState;
            state.changeRequests.length = previousRequestCount;
          };
        }
        if (terminalAction) {
          try {
            await actions.verify();
          } catch (error) {
            if (action === "approve") state.approved = false;
            throw error;
          }
          terminal = "pending";
        }
        if (["looks-good", "explain", "request-change"].includes(action)) {
          if (!section) throw new Error("unknown section");
          applySectionAction(
            state,
            sectionId,
            action as "looks-good" | "explain" | "request-change",
          );
          if (action === "explain") {
            const question = comment || section.prompt;
            const answer = await actions.explain(sectionId, question);
            await actions.verify();
            state.questions.push({ sectionId, question, answer });
          }
          if (action === "request-change") {
            state.changeRequests.push({ sectionId, feedback: feedback! });
            actions.persist(state);
            terminal = "committed";
            await actions.requestChanges(`${sectionId}: ${feedback}`);
          } else actions.persist(state);
        } else if (action === "ask") {
          if (!section) throw new Error("unknown section");
          if (!comment) throw new Error("Ask reviewer requires a question");
          const answer = await actions.explain(sectionId, comment);
          await actions.verify();
          state.questions.push({ sectionId, question: comment, answer });
          actions.persist(state);
        } else if (action === "context") {
          if (!section) throw new Error("unknown section");
          return send(
            200,
            `<pre>${escapeHtml(actions.context(section))}</pre><p><a href="/${token}/">Back</a></p>`,
          );
        } else if (action === "full-diff") await actions.fullDiff();
        else if (action === "request-changes") {
          terminal = "committed";
          await actions.requestChanges(finalFeedback!);
        } else if (action === "approve") {
          try {
            actions.persist(state);
          } catch (error) {
            state.approved = false;
            throw error;
          }
          terminal = "committed";
          try {
            await actions.approved();
          } catch (error) {
            state.approved = false;
            throw error;
          }
        }
        response.writeHead(303, {
          location: `/${token}/`,
          "cache-control": "no-store",
        });
        response.end();
      } catch (error) {
        if (terminal === "pending") {
          rollbackPending?.();
          terminal = "open";
        }
        send(
          400,
          escapeHtml(error instanceof Error ? error.message : String(error)),
          "text/plain; charset=utf-8",
        );
      } finally {
        releaseAction();
      }
    },
  );
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string")
        return reject(new Error("review server has no local address"));
      resolve({
        url: `http://127.0.0.1:${address.port}/${token}/`,
        close: () =>
          new Promise<void>((done, fail) =>
            server.close((error) => (error ? fail(error) : done())),
          ),
      });
    });
  });
}

export function saveWalkthroughState(
  path: string,
  state: WalkthroughState,
): void {
  const descriptor = openSync(
    path,
    constants.O_WRONLY |
      constants.O_CREAT |
      constants.O_TRUNC |
      constants.O_NOFOLLOW,
    0o600,
  );
  try {
    if (!fstatSync(descriptor).isFile())
      throw new Error("review state is not a regular file");
    writeFileSync(descriptor, `${JSON.stringify(state, null, 2)}\n`, "utf8");
  } finally {
    closeSync(descriptor);
  }
}
export function loadWalkthroughState(
  path: string,
  document: WalkthroughDocument,
): WalkthroughState {
  const descriptor = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const info = fstatSync(descriptor);
    if (!info.isFile() || info.size > 256 * 1024)
      throw new Error("review state file is invalid");
    return validateWalkthroughState(
      document,
      JSON.parse(readFileSync(descriptor, "utf8")),
    );
  } finally {
    closeSync(descriptor);
  }
}
