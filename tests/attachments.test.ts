import assert from "node:assert/strict";
import {
  createServer,
  type IncomingMessage,
  type ServerResponse,
} from "node:http";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  STORE_ATTACHMENT_TOOL,
  attachmentMediaType,
  importAttachment,
  registerAttachmentTool,
  resolveContentSocketPath,
} from "../agent/extensions/scufris/service/attachments.ts";

const descriptor = {
  id: "att_0123456789",
  name: "report.pdf",
  media_type: "application/pdf",
  size: 9,
};

async function listen(
  handler: (request: IncomingMessage, response: ServerResponse) => void,
): Promise<{ root: string; socketPath: string; close: () => Promise<void> }> {
  const root = await mkdtemp(join(tmpdir(), "scufris-content-tool-"));
  const socketPath = join(root, "content.sock");
  const server = createServer(handler);
  await new Promise<void>((resolve) => server.listen(socketPath, resolve));
  return {
    root,
    socketPath,
    close: async () => {
      await new Promise<void>((resolve) => server.close(() => resolve()));
      await rm(root, { recursive: true, force: true });
    },
  };
}

test("content socket resolution is isolated from the agent channel", () => {
  assert.equal(
    resolveContentSocketPath({ SCUFRIS_RUNTIME_DIR: "/run/scufris-test" }),
    "/run/scufris-test/content.sock",
  );
  assert.equal(
    resolveContentSocketPath({ XDG_RUNTIME_DIR: "/run/user/1000" }),
    "/run/user/1000/scufris/content.sock",
  );
  assert.equal(
    resolveContentSocketPath({
      SCUFRIS_CONTENT_SOCKET: "/tmp/exact-content.sock",
    }),
    "/tmp/exact-content.sock",
  );
});

test("attachment media types are deterministic and bounded to known suffixes", () => {
  assert.equal(attachmentMediaType("REPORT.PDF"), "application/pdf");
  assert.equal(attachmentMediaType("photo.jpeg"), "image/jpeg");
  assert.equal(
    attachmentMediaType("archive.unknown"),
    "application/octet-stream",
  );
});

test("the content client sends only the trusted local import request", async () => {
  let requestBody = "";
  const server = await listen((request, response) => {
    assert.equal(request.method, "POST");
    assert.equal(request.url, "/attachments/import");
    assert.equal(request.headers["content-type"], "application/json");
    request.setEncoding("utf8");
    request.on("data", (chunk) => {
      requestBody += chunk;
    });
    request.on("end", () => {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify(descriptor));
    });
  });
  try {
    assert.deepEqual(
      await importAttachment(server.socketPath, "/tmp/report.pdf"),
      descriptor,
    );
    assert.deepEqual(JSON.parse(requestBody), {
      path: "/tmp/report.pdf",
      media_type: "application/pdf",
    });
  } finally {
    await server.close();
  }
});

test("store_attachment resolves relative paths and returns only the managed ID", async () => {
  let importedPath = "";
  const server = await listen((request, response) => {
    let body = "";
    request.setEncoding("utf8");
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      importedPath = (JSON.parse(body) as { path: string }).path;
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify(descriptor));
    });
  });
  let tool: any;
  const pi = {
    registerTool(value: unknown) {
      tool = value;
    },
  };
  try {
    registerAttachmentTool(pi as never, {
      SCUFRIS_CONTENT_SOCKET: server.socketPath,
    });
    assert.equal(tool.name, STORE_ATTACHMENT_TOOL);
    const result = await tool.execute(
      "call-1",
      { path: "@output/report.pdf" },
      undefined,
      undefined,
      { cwd: "/work/project" },
    );
    assert.equal(importedPath, "/work/project/output/report.pdf");
    assert.deepEqual(result.content, [{ type: "text", text: descriptor.id }]);
    assert.deepEqual(result.details, descriptor);
  } finally {
    await server.close();
  }
});

test("the content client rejects service errors and invalid descriptors safely", async () => {
  const invalid = await listen((_request, response) => {
    response.writeHead(200, { "content-type": "application/json" });
    response.end('{"id":"../bad"}');
  });
  try {
    await assert.rejects(
      importAttachment(invalid.socketPath, "/tmp/report.pdf"),
      /Attachment storage is unavailable/,
    );
  } finally {
    await invalid.close();
  }

  const tooLarge = await listen((_request, response) => {
    response.writeHead(413, { "content-type": "application/json" });
    response.end(
      JSON.stringify({
        error: { code: "attachment_too_large", message: "safe" },
      }),
    );
  });
  try {
    await assert.rejects(
      importAttachment(tooLarge.socketPath, "/tmp/report.pdf"),
      /larger than 16 MiB/,
    );
  } finally {
    await tooLarge.close();
  }
});
