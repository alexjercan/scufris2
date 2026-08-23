import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import {
  closeSync,
  constants,
  fstatSync,
  openSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

const MAX_MARKDOWN_BYTES = 256 * 1024;
const MAX_INIT_LINE_BYTES = 4 * 1024 * 1024;
const MAX_RESULT_LINE_BYTES = 32 * 1024 * 1024;
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
  summary: string;
  revision: string;
  baseRevision: string;
  files: number;
  added: number;
  removed: number;
  sections: WalkthroughSection[];
  warnings: string[];
  source: string;
  identity: string;
}
export interface ReviewComment {
  id: string;
  sectionId: string;
  file: string;
  lines: string;
  body: string;
}
export interface BlockingChangeRequest {
  sectionId: string;
  feedback: string;
}
export const APPROVAL_INSTRUCTION =
  "Quick Review approved this exact revision. Foreground Scufris decides what follows.";
const MAX_APPROVAL_INSTRUCTION_BYTES = 16 * 1024;

export function approvalInstruction(comments: ReviewComment[]): string {
  if (comments.length === 0) return APPROVAL_INSTRUCTION;
  const notes = comments.map(({ sectionId, file, lines, body }) => ({
    sectionId,
    anchor: `${file}:${lines}`,
    body,
  }));
  const message = `${APPROVAL_INSTRUCTION}\n\nNon-blocking review notes to include in the approval: ${JSON.stringify({ notes })}`;
  if (Buffer.byteLength(message, "utf8") > MAX_APPROVAL_INSTRUCTION_BYTES)
    throw new Error("approval notes exceed the steering limit");
  return message;
}

const CHANGE_REQUEST_INSTRUCTION = "Walkthrough review requested changes.";
const MAX_CHANGE_REQUEST_INSTRUCTION_BYTES = 16 * 1024;

export function changeRequestInstruction(
  changeRequests: BlockingChangeRequest[],
): string {
  if (changeRequests.length === 0) return CHANGE_REQUEST_INSTRUCTION;
  const message = `${CHANGE_REQUEST_INSTRUCTION.slice(0, -1)}: ${JSON.stringify({ changeRequests })}`;
  if (Buffer.byteLength(message, "utf8") > MAX_CHANGE_REQUEST_INSTRUCTION_BYTES)
    throw new Error("blocking feedback exceeds the steering limit");
  return message;
}

export interface WalkthroughState {
  version: 1;
  identity: string;
  revision: string;
  sections: Record<string, SectionState>;
  viewed: Record<string, boolean>;
  questions: Array<{ sectionId: string; question: string; answer?: string }>;
  comments: ReviewComment[];
  changeRequests: BlockingChangeRequest[];
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
  ];
  if (!metadata || !exact(metadata, expected))
    throw new Error("walkthrough metadata schema is invalid");
  const status = metadata.status!;
  const revision = metadata.revision!;
  const baseRevision = metadata.baseRevision!;
  const files = uint(metadata.files!),
    added = uint(metadata.added!),
    removed = uint(metadata.removed!);
  if (
    status !== "ready" ||
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
  const headingEnd = source.indexOf("\n") + 1;
  const metadataStart = source.indexOf(":::walkthrough", headingEnd);
  const summary = source.slice(headingEnd, metadataStart).trim();
  return {
    title: top[1]!,
    summary: summary || "Review the important implementation changes below.",
    revision,
    baseRevision,
    files,
    added,
    removed,
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
    viewed: Object.fromEntries(
      document.sections.map((section) => [section.id, false]),
    ),
    questions: [],
    comments: [],
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
        "comments",
        "identity",
        "questions",
        "revision",
        "sections",
        "version",
        "viewed",
      ]
        .sort()
        .join("\0") ||
    state.version !== 1 ||
    state.identity !== document.identity ||
    state.revision !== document.revision ||
    typeof state.approved !== "boolean" ||
    !state.sections ||
    typeof state.sections !== "object" ||
    !state.viewed ||
    typeof state.viewed !== "object" ||
    !Array.isArray(state.questions) ||
    !Array.isArray(state.comments) ||
    !Array.isArray(state.changeRequests)
  )
    throw new Error("review state does not match the artifact");
  const expected = document.sections.map((section) => section.id).sort();
  if (
    Object.keys(state.sections).sort().join("\0") !== expected.join("\0") ||
    Object.keys(state.viewed).sort().join("\0") !== expected.join("\0") ||
    Object.values(state.viewed).some((item) => typeof item !== "boolean") ||
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
    state.comments.length > MAX_SECTIONS ||
    new Set(state.comments.map((item) => item?.id)).size !==
      state.comments.length ||
    state.comments.some(
      (item) =>
        !item ||
        typeof item !== "object" ||
        Object.keys(item).sort().join("\0") !==
          "body\0file\0id\0lines\0sectionId" ||
        !(item.sectionId in state.sections) ||
        !/^[0-9a-f]{24}$/.test(item.id) ||
        document.sections.find((section) => section.id === item.sectionId)
          ?.file !== item.file ||
        document.sections.find((section) => section.id === item.sectionId)
          ?.lines !== item.lines ||
        !validText(item.body),
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
  approvalInstruction(state.comments);
  changeRequestInstruction(state.changeRequests);
  if (
    state.approved &&
    (Object.values(state.viewed).some((item) => !item) ||
      state.changeRequests.length > 0)
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
  if (Object.values(state.viewed).some((item) => !item))
    throw new Error("all sections must be marked viewed before approval");
  if (state.changeRequests.length > 0)
    throw new Error("blocking feedback must be sent as requested changes");
  state.approved = true;
}

export interface WalkthroughActions {
  verify(): Promise<void>;
  persist(state: WalkthroughState): void;
  explain(section: string, question: string): Promise<string>;
  requestChanges(feedback: string): Promise<void>;
  fullDiff(): Promise<void>;
  approved(comments: ReviewComment[]): Promise<void>;
  context(section: WalkthroughSection): string;
}
type BridgeAction = {
  type: "action";
  id: string;
  action: string;
  section?: string;
  comment?: string;
};

export interface WalkthroughServerOptions {
  openBrowser?: boolean;
  python?: string;
  toolPath?: string;
}

const quickReviewPath = fileURLToPath(
  new URL("../../../tools/quick-review/quick_review.py", import.meta.url),
);

function bridgeDocument(document: WalkthroughDocument) {
  const { source: _source, identity: _identity, ...descriptor } = document;
  return descriptor;
}

export function encodeBridgeInit(
  document: WalkthroughDocument,
  state: WalkthroughState,
): string {
  const line = `${JSON.stringify({ type: "init", version: 1, document: bridgeDocument(document), state })}\n`;
  if (Buffer.byteLength(line, "utf8") > MAX_INIT_LINE_BYTES)
    throw new Error("quick review init message exceeds 4 MiB");
  return line;
}

export function encodeBridgeResult(value: object): string {
  const line = `${JSON.stringify(value)}\n`;
  if (Buffer.byteLength(line, "utf8") > MAX_RESULT_LINE_BYTES)
    throw new Error("quick review result message exceeds 32 MiB");
  return line;
}

export function parseBridgeAction(line: string): BridgeAction {
  if (Buffer.byteLength(line, "utf8") > 512 * 1024)
    throw new Error("quick review bridge message exceeds 512 KiB");
  const value: unknown = JSON.parse(line);
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error("quick review bridge message is invalid");
  const action = value as Partial<BridgeAction>;
  if (
    action.type !== "action" ||
    typeof action.id !== "string" ||
    !/^[0-9a-f]{24}$/.test(action.id) ||
    typeof action.action !== "string" ||
    (action.section !== undefined && typeof action.section !== "string") ||
    (action.comment !== undefined && typeof action.comment !== "string") ||
    Object.keys(action).some(
      (key) => !["type", "id", "action", "section", "comment"].includes(key),
    )
  )
    throw new Error("quick review action schema is invalid");
  return action as BridgeAction;
}

export function startWalkthroughServer(
  document: WalkthroughDocument,
  state: WalkthroughState,
  actions: WalkthroughActions,
  options: WalkthroughServerOptions = {},
): Promise<{ url: string; open(): void; close(): Promise<void> }> {
  let actionTail = Promise.resolve();
  let terminal: "open" | "pending" | "committed" = "open";
  const initLine = encodeBridgeInit(document, state);
  const child: ChildProcessWithoutNullStreams = spawn(
    options.python ?? "python3",
    [
      options.toolPath ?? quickReviewPath,
      ...(options.openBrowser === false ? ["--no-open"] : []),
    ],
    { stdio: ["pipe", "pipe", "pipe"] },
  );
  const lines = createInterface({ input: child.stdout, crlfDelay: Infinity });
  let stderr = "";
  child.stdin.on("error", () => undefined);
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk: string) => {
    stderr = (stderr + chunk).slice(-4096);
  });
  child.stdin.write(initLine);

  const execute = async (
    request: BridgeAction,
  ): Promise<{ message: string; context?: string }> => {
    let rollbackPending: (() => void) | undefined;
    try {
      await actions.verify();
      const action = request.action;
      const sectionId = request.section ?? "";
      const section = document.sections.find((item) => item.id === sectionId);
      const sectionAction = [
        "mark-viewed",
        "reopen",
        "add-comment",
        "explain",
        "request-change",
        "context",
        "ask",
      ].includes(action);
      if (
        !sectionAction &&
        ![
          "approve",
          "approve-with-comments",
          "request-changes",
          "full-diff",
        ].includes(action)
      )
        throw new Error("unknown action");
      if (sectionAction && !section) throw new Error("unknown section");
      if (terminal !== "open")
        throw new Error("review already has a terminal action");
      const comment = (request.comment ?? "").trim();
      if (Buffer.byteLength(comment, "utf8") > 4096)
        throw new Error("review comment exceeds 4 KiB");
      const feedback =
        action === "request-change" ? comment || section!.prompt : undefined;
      const finalFeedback =
        action === "request-changes"
          ? changeRequestInstruction(state.changeRequests)
          : undefined;
      const terminalAction = [
        "request-changes",
        "approve",
        "approve-with-comments",
      ].includes(action);
      if (["approve", "approve-with-comments"].includes(action)) {
        if (action === "approve" && state.comments.length > 0)
          throw new Error("non-blocking notes require Approve with comments");
        if (action === "approve-with-comments" && state.comments.length === 0)
          throw new Error("there are no review notes to include");
        approveWalkthrough(state);
        rollbackPending = () => void (state.approved = false);
      }
      if (terminalAction) {
        try {
          await actions.verify();
        } catch (error) {
          if (["approve", "approve-with-comments"].includes(action))
            state.approved = false;
          throw error;
        }
        terminal = "pending";
      }
      if (action === "mark-viewed") {
        state.approved = false;
        state.viewed[sectionId] = true;
        if (state.sections[sectionId] !== "change-requested")
          state.sections[sectionId] = "looks-good";
        actions.persist(state);
      } else if (action === "reopen") {
        state.approved = false;
        state.viewed[sectionId] = false;
        if (state.sections[sectionId] === "looks-good")
          state.sections[sectionId] = "not-reviewed";
        actions.persist(state);
      } else if (action === "add-comment") {
        if (!comment) throw new Error("Add comment requires a review note");
        if (state.comments.length >= MAX_SECTIONS)
          throw new Error("review notes exceed the bounded review limit");
        const nextComment = {
          id: randomBytes(12).toString("hex"),
          sectionId,
          file: section!.file,
          lines: section!.lines,
          body: comment,
        };
        approvalInstruction([...state.comments, nextComment]);
        state.comments.push(nextComment);
        actions.persist(state);
      } else if (["explain", "request-change"].includes(action)) {
        if (action === "explain") {
          applySectionAction(state, sectionId, "explain");
          const question = comment || section!.prompt;
          const answer = await actions.explain(sectionId, question);
          await actions.verify();
          state.questions.push({ sectionId, question, answer });
        } else {
          const nextChangeRequest = { sectionId, feedback: feedback! };
          changeRequestInstruction([
            ...state.changeRequests,
            nextChangeRequest,
          ]);
          applySectionAction(state, sectionId, "request-change");
          state.changeRequests.push(nextChangeRequest);
        }
        actions.persist(state);
      } else if (action === "ask") {
        if (!comment) throw new Error("Ask reviewer requires a question");
        const answer = await actions.explain(sectionId, comment);
        await actions.verify();
        state.questions.push({ sectionId, question: comment, answer });
        actions.persist(state);
      } else if (action === "context") {
        const context = actions.context(section!);
        if (Buffer.byteLength(context, "utf8") > 256 * 1024)
          throw new Error("exact-revision context exceeds 256 KiB");
        return {
          message: "Exact-revision context loaded.",
          context,
        };
      } else if (action === "full-diff") await actions.fullDiff();
      else if (action === "request-changes") {
        terminal = "committed";
        await actions.requestChanges(finalFeedback!);
      } else if (["approve", "approve-with-comments"].includes(action)) {
        try {
          actions.persist(state);
        } catch (error) {
          state.approved = false;
          throw error;
        }
        terminal = "committed";
        try {
          await actions.approved([...state.comments]);
        } catch (error) {
          state.approved = false;
          throw error;
        }
      }
      return {
        message:
          action === "full-diff"
            ? "Opened the exact full diff."
            : "Review updated.",
      };
    } catch (error) {
      if (terminal === "pending") {
        rollbackPending?.();
        terminal = "open";
      }
      throw error;
    }
  };

  return new Promise((resolve, reject) => {
    let ready = false;
    let settled = false;
    let pendingActions = 0;
    let closeRequested = false;
    let shutdownSent = false;
    const exited = new Promise<void>((done) =>
      child.once("exit", () => done()),
    );
    const sendShutdownIfReady = () => {
      if (
        closeRequested &&
        pendingActions === 0 &&
        !shutdownSent &&
        child.exitCode === null &&
        !child.stdin.destroyed
      ) {
        shutdownSent = true;
        child.stdin.write(`${JSON.stringify({ type: "shutdown" })}\n`);
      }
    };
    const close = () => {
      closeRequested = true;
      sendShutdownIfReady();
      return exited;
    };
    const fail = (error: Error) => {
      if (settled) return;
      settled = true;
      child.kill();
      reject(error);
    };
    child.once("error", fail);
    child.once("exit", (code) => {
      if (!ready)
        fail(
          new Error(
            `quick review exited before ready (${code}): ${stderr.trim()}`,
          ),
        );
    });
    lines.on("line", (line) => {
      if (!ready) {
        try {
          if (Buffer.byteLength(line, "utf8") > 4096)
            throw new Error("quick review ready message is oversized");
          const message: unknown = JSON.parse(line);
          if (!message || typeof message !== "object" || Array.isArray(message))
            throw new Error("quick review ready message is invalid");
          const value = message as { type?: unknown; url?: unknown };
          if (
            value.type !== "ready" ||
            typeof value.url !== "string" ||
            !/^http:\/\/127\.0\.0\.1:[0-9]+\/[A-Za-z0-9_-]+\/$/.test(value.url)
          )
            throw new Error("quick review returned an unsafe URL");
          ready = true;
          settled = true;
          resolve({
            url: value.url,
            open: () =>
              child.stdin.write(`${JSON.stringify({ type: "activate" })}\n`),
            close,
          });
        } catch (error) {
          fail(error instanceof Error ? error : new Error(String(error)));
        }
        return;
      }
      let request: BridgeAction;
      try {
        request = parseBridgeAction(line);
      } catch {
        child.kill();
        return;
      }
      pendingActions++;
      const previous = actionTail;
      let release!: () => void;
      actionTail = new Promise<void>((done) => (release = done));
      void previous.then(async () => {
        let payload: object;
        try {
          const result = await execute(request);
          payload = {
            type: "result",
            id: request.id,
            ok: true,
            state,
            ...result,
          };
        } catch (error) {
          payload = {
            type: "result",
            id: request.id,
            ok: false,
            state,
            error: error instanceof Error ? error.message : String(error),
          };
        }
        let result: string;
        try {
          result = encodeBridgeResult(payload);
        } catch (error) {
          result = encodeBridgeResult({
            type: "result",
            id: request.id,
            ok: false,
            state,
            error: error instanceof Error ? error.message : String(error),
          });
        }
        child.stdin.write(result, () => {
          pendingActions--;
          release();
          sendShutdownIfReady();
        });
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
