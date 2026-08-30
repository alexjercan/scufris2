import { request as httpRequest } from "node:http";
import { extname, isAbsolute, join, resolve } from "node:path";
import { Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";
import {
  CONTENT_FILE_NAME,
  decodeAttachmentDescriptor,
  SOCKET_DIRECTORY_NAME,
  type AttachmentDescriptor,
} from "./protocol.ts";

export const STORE_ATTACHMENT_TOOL = "store_attachment";
const MAX_PATH_BYTES = 4 * 1024;
const MAX_RESPONSE_BYTES = 16 * 1024;
const IMPORT_TIMEOUT_MS = 30_000;
const STORE_ATTACHMENT_POLICY =
  "Use store_attachment only when a file must be delivered to a Scufris surface. Pass a regular file path, then put the returned attachment ID in scufris_final_response attachments.";

const MEDIA_TYPES: Readonly<Record<string, string>> = {
  ".avif": "image/avif",
  ".bmp": "image/bmp",
  ".csv": "text/csv",
  ".gif": "image/gif",
  ".heic": "image/heic",
  ".html": "text/html",
  ".jpeg": "image/jpeg",
  ".jpg": "image/jpeg",
  ".json": "application/json",
  ".md": "text/markdown",
  ".pdf": "application/pdf",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".txt": "text/plain",
  ".webp": "image/webp",
};

export function resolveContentSocketPath(
  environment: NodeJS.ProcessEnv = process.env,
): string | undefined {
  if (environment.SCUFRIS_CONTENT_SOCKET)
    return environment.SCUFRIS_CONTENT_SOCKET;
  if (environment.SCUFRIS_RUNTIME_DIR)
    return join(environment.SCUFRIS_RUNTIME_DIR, CONTENT_FILE_NAME);
  if (environment.XDG_RUNTIME_DIR)
    return join(
      environment.XDG_RUNTIME_DIR,
      SOCKET_DIRECTORY_NAME,
      CONTENT_FILE_NAME,
    );
  return undefined;
}

export function attachmentMediaType(path: string): string {
  return MEDIA_TYPES[extname(path).toLowerCase()] ?? "application/octet-stream";
}

interface ErrorEnvelope {
  error?: { code?: unknown };
}

function safeFailure(status: number | undefined, body: Buffer): Error {
  let code: unknown;
  try {
    code = (JSON.parse(body.toString("utf8")) as ErrorEnvelope).error?.code;
  } catch {
    // The service boundary is strict. An invalid error body is unavailable.
  }
  if (status === 413 || code === "attachment_too_large")
    return new Error("The attachment is larger than 16 MiB.");
  if (status === 422 || code === "invalid_attachment")
    return new Error(
      "The attachment must be a readable regular file with a valid display name.",
    );
  if (status === 507 || code === "attachment_quota")
    return new Error("Attachment storage is full.");
  return new Error("Attachment storage is unavailable.");
}

export function importAttachment(
  socketPath: string,
  path: string,
  signal?: AbortSignal,
): Promise<AttachmentDescriptor> {
  const encoded = Buffer.from(
    JSON.stringify({ path, media_type: attachmentMediaType(path) }),
    "utf8",
  );
  return new Promise((resolvePromise, rejectPromise) => {
    let finished = false;
    const finish = (
      outcome: { descriptor: AttachmentDescriptor } | { error: Error },
    ) => {
      if (finished) return;
      finished = true;
      signal?.removeEventListener("abort", abort);
      if ("error" in outcome) rejectPromise(outcome.error);
      else resolvePromise(outcome.descriptor);
    };
    const request = httpRequest(
      {
        socketPath,
        path: "/attachments/import",
        method: "POST",
        headers: {
          "content-type": "application/json",
          "content-length": encoded.length,
        },
      },
      (response) => {
        const chunks: Buffer[] = [];
        let bytes = 0;
        response.on("data", (chunk: Buffer) => {
          bytes += chunk.length;
          if (bytes > MAX_RESPONSE_BYTES) {
            request.destroy();
            finish({ error: new Error("Attachment storage is unavailable.") });
            return;
          }
          chunks.push(chunk);
        });
        response.on("end", () => {
          if (finished) return;
          const body = Buffer.concat(chunks, bytes);
          if (response.statusCode !== 200) {
            finish({ error: safeFailure(response.statusCode, body) });
            return;
          }
          try {
            finish({
              descriptor: decodeAttachmentDescriptor(
                JSON.parse(body.toString("utf8")),
              ),
            });
          } catch {
            finish({ error: new Error("Attachment storage is unavailable.") });
          }
        });
        response.on("error", () =>
          finish({ error: new Error("Attachment storage is unavailable.") }),
        );
      },
    );
    const abort = () => {
      request.destroy();
      finish({ error: new Error("Attachment storage was cancelled.") });
    };
    request.setTimeout(IMPORT_TIMEOUT_MS, () => {
      request.destroy();
      finish({ error: new Error("Attachment storage is unavailable.") });
    });
    request.on("error", () =>
      finish({ error: new Error("Attachment storage is unavailable.") }),
    );
    signal?.addEventListener("abort", abort, { once: true });
    if (signal?.aborted) abort();
    else request.end(encoded);
  });
}

export function registerAttachmentTool(
  pi: ExtensionAPI,
  environment: NodeJS.ProcessEnv = process.env,
): void {
  const socketPath = resolveContentSocketPath(environment);
  pi.registerTool(
    defineTool({
      name: STORE_ATTACHMENT_TOOL,
      label: "Store attachment",
      description:
        "Import one readable regular file up to 16 MiB into managed Scufris storage and return its attachment ID.",
      promptSnippet: "Store a generated file for delivery to a Scufris surface",
      promptGuidelines: [STORE_ATTACHMENT_POLICY],
      executionMode: "sequential",
      parameters: Type.Object(
        {
          path: Type.String({ minLength: 1, maxLength: MAX_PATH_BYTES }),
        },
        { additionalProperties: false },
      ),
      async execute(_toolCallId, params, signal, _onUpdate, context) {
        if (!socketPath) throw new Error("Attachment storage is unavailable.");
        const supplied = params.path.startsWith("@")
          ? params.path.slice(1)
          : params.path;
        if (Buffer.byteLength(supplied, "utf8") > MAX_PATH_BYTES)
          throw new Error("The attachment path is too long.");
        const absolute = isAbsolute(supplied)
          ? supplied
          : resolve(context.cwd, supplied);
        const descriptor = await importAttachment(socketPath, absolute, signal);
        return {
          content: [{ type: "text", text: descriptor.id }],
          details: descriptor,
        };
      },
    }),
  );
}
