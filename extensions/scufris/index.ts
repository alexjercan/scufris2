import { randomBytes } from "node:crypto";
import { spawn as spawnProcess } from "node:child_process";
import { fileURLToPath } from "node:url";
import { StringEnum, Type, type TSchema } from "@earendil-works/pi-ai";
import {
  defineTool,
  type ExtensionAPI,
  type ExtensionContext,
} from "@earendil-works/pi-coding-agent";

const jobHelperPath = fileURLToPath(
  new URL("../../scripts/scufris-job", import.meta.url),
);
const dashboardHelperPath = fileURLToPath(
  new URL("../../scripts/scufris-dashboard", import.meta.url),
);
const maxHelperOutput = 2 * 1024 * 1024;

interface HelperEnvelope<T> {
  ok: boolean;
  result?: T;
  error?: string;
  error_code?: string;
}

class DashboardError extends Error {
  constructor(
    message: string,
    readonly code?: string,
  ) {
    super(message);
  }
}

interface OwnedJob {
  job_id: string;
  harness: "pi" | "claude";
  feature: string;
  state: string;
  summary: string;
  offset: number;
  tail: string;
  inode: number | null;
  window_alive: boolean;
  protocolErrors: Set<string>;
  exitReported: boolean;
}

interface SpawnResult {
  job_id: string;
  state: string;
  harness: "pi" | "claude";
  feature: string;
  model: string;
  thinking: string;
  message: string;
}

interface PollResult {
  jobs: Array<{
    job_id: string;
    events: string[];
    errors: string[];
    offset: number;
    tail: string;
    inode: number | null;
    window_alive: boolean;
  }>;
}

interface InspectResult {
  job_id: string;
  harness: string;
  feature: string;
  state: string;
  summary: string;
  window_alive: boolean;
  events: string[];
  report?: string;
}

interface WidgetChoice {
  name: string;
  value: string;
}

interface WidgetOption {
  id: string;
  name: string;
  description: string;
  type: "boolean" | "integer" | "select" | "text";
  default?: unknown;
  minimum?: number;
  maximum?: number;
  choices?: WidgetChoice[];
  variants: string[];
}

interface WidgetInput {
  id: string;
  name: string;
  type: string;
  required: boolean;
  variants: string[];
}

interface WidgetContract {
  id: string;
  name: string;
  description: string;
  variants: Array<{ id: string; name: string }>;
  options: WidgetOption[];
  inputs: WidgetInput[];
}

interface WidgetCatalog {
  widgets: WidgetContract[];
}

interface OwnedSurface {
  surface_id: string;
  widget_id: string;
  variant_id: string;
}

interface WidgetOpenParams {
  widget_id: string;
  variant_id: string;
  options?: Record<string, unknown>;
  inputs?: Record<string, { type: string; value: unknown }>;
  presentation?: "focus" | "tile";
}

interface WidgetSurface {
  surface_id: string;
  widget_id: string;
  variant_id: string;
  presentation: string;
  title?: string;
  health?: string;
}

function toolResult(value: unknown, isError = false, text?: string) {
  return {
    content: [
      {
        type: "text" as const,
        text: text ?? JSON.stringify(value, null, 2),
      },
    ],
    details: value,
    ...(isError ? { isError: true } : {}),
  };
}

async function runPrivateHelper<T>(
  helperPath: string,
  command: string,
  request: unknown,
  signal?: AbortSignal,
  timeoutMs = 30_000,
): Promise<HelperEnvelope<T>> {
  return await new Promise<HelperEnvelope<T>>((resolve, reject) => {
    const child = spawnProcess(helperPath, [command], {
      env: process.env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    let outputBytes = 0;
    let settled = false;

    const finish = (error?: Error, value?: HelperEnvelope<T>) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      signal?.removeEventListener("abort", abort);
      if (error) reject(error);
      else resolve(value as HelperEnvelope<T>);
    };
    const abort = () => {
      child.kill("SIGTERM");
      finish(new Error("Scufris helper request aborted"));
    };
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      finish(new Error(`Scufris helper timed out during ${command}`));
    }, timeoutMs);

    signal?.addEventListener("abort", abort, { once: true });
    child.stdout.on("data", (chunk: Buffer) => {
      outputBytes += chunk.length;
      if (outputBytes > maxHelperOutput) {
        child.kill("SIGTERM");
        finish(new Error("Scufris helper output exceeded 2 MiB"));
        return;
      }
      stdout.push(chunk);
    });
    child.stderr.on("data", (chunk: Buffer) => {
      outputBytes += chunk.length;
      if (outputBytes > maxHelperOutput) {
        child.kill("SIGTERM");
        finish(new Error("Scufris helper output exceeded 2 MiB"));
        return;
      }
      stderr.push(chunk);
    });
    child.on("error", (error) => finish(error));
    child.on("close", () => {
      if (settled) return;
      const text = Buffer.concat(stdout).toString("utf8");
      let envelope: HelperEnvelope<T>;
      try {
        envelope = JSON.parse(text) as HelperEnvelope<T>;
      } catch {
        const diagnostic = Buffer.concat(stderr).toString("utf8").trim();
        finish(
          new Error(
            `Invalid Scufris helper response${diagnostic ? `: ${diagnostic}` : ""}`,
          ),
        );
        return;
      }
      finish(undefined, envelope);
    });
    child.stdin.end(JSON.stringify(request));
  });
}

async function runHelper<T>(
  command: string,
  request: unknown,
  signal?: AbortSignal,
  timeoutMs = 30_000,
): Promise<T> {
  const envelope = await runPrivateHelper<T>(
    jobHelperPath,
    command,
    request,
    signal,
    timeoutMs,
  );
  if (!envelope.ok || envelope.result === undefined) {
    throw new Error(envelope.error ?? "Scufris helper failed");
  }
  return envelope.result;
}

async function runDashboard<T>(
  command: string,
  request: unknown,
  signal?: AbortSignal,
): Promise<T> {
  const envelope = await runPrivateHelper<T>(
    dashboardHelperPath,
    command,
    request,
    signal,
    7_000,
  );
  if (!envelope.ok || envelope.result === undefined) {
    throw new DashboardError(
      envelope.error ?? "Scufris dashboard helper failed",
      envelope.error_code,
    );
  }
  return envelope.result;
}

function generatedJobId(): string {
  return randomBytes(6).toString("hex");
}

function parseEvent(line: string): { state: string; summary: string } {
  const separator = line.indexOf(": ");
  return {
    state: line.slice(0, separator),
    summary: line.slice(separator + 2),
  };
}

function objectValue(value: unknown, name: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`Invalid dashboard catalog: ${name} must be an object`);
  }
  return value as Record<string, unknown>;
}

function stringValue(value: unknown, name: string): string {
  if (typeof value !== "string" || value.length === 0 || value.length > 256) {
    throw new Error(`Invalid dashboard catalog: ${name} must be a string`);
  }
  return value;
}

function stringArray(value: unknown, name: string): string[] {
  if (!Array.isArray(value)) {
    throw new Error(`Invalid dashboard catalog: ${name} must be an array`);
  }
  return value.map((item, index) => stringValue(item, `${name}[${index}]`));
}

function validateWidgetCatalog(value: unknown): WidgetCatalog {
  const root = objectValue(value, "result");
  if (!Array.isArray(root.widgets) || root.widgets.length > 128) {
    throw new Error(
      "Invalid dashboard catalog: widgets must be a bounded array",
    );
  }
  const widgetIds = new Set<string>();
  const widgets = root.widgets.map((rawWidget, widgetIndex) => {
    const widget = objectValue(rawWidget, `widgets[${widgetIndex}]`);
    const id = stringValue(widget.id, `widgets[${widgetIndex}].id`);
    if (widgetIds.has(id)) {
      throw new Error(`Invalid dashboard catalog: duplicate widget ${id}`);
    }
    widgetIds.add(id);
    if (!Array.isArray(widget.variants) || widget.variants.length === 0) {
      throw new Error(`Invalid dashboard catalog: ${id} has no variants`);
    }
    const variantIds = new Set<string>();
    const variants = widget.variants.map((rawVariant, variantIndex) => {
      const variant = objectValue(
        rawVariant,
        `widgets[${widgetIndex}].variants[${variantIndex}]`,
      );
      const variantId = stringValue(variant.id, `${id} variant id`);
      if (variantIds.has(variantId)) {
        throw new Error(
          `Invalid dashboard catalog: duplicate ${id} variant ${variantId}`,
        );
      }
      variantIds.add(variantId);
      return {
        id: variantId,
        name: stringValue(variant.name, `${id}.${variantId} name`),
      };
    });
    const options = Array.isArray(widget.options)
      ? widget.options.map((rawOption, optionIndex): WidgetOption => {
          const option = objectValue(
            rawOption,
            `widgets[${widgetIndex}].options[${optionIndex}]`,
          );
          const type = stringValue(option.type, `${id} option type`);
          if (!["boolean", "integer", "select", "text"].includes(type)) {
            throw new Error(
              `Invalid dashboard catalog: unsupported option type ${type}`,
            );
          }
          const choices = Array.isArray(option.choices)
            ? option.choices.map((rawChoice, choiceIndex) => {
                const choice = objectValue(
                  rawChoice,
                  `${id} option choice ${choiceIndex}`,
                );
                return {
                  name: stringValue(choice.name, `${id} choice name`),
                  value: stringValue(choice.value, `${id} choice value`),
                };
              })
            : undefined;
          return {
            id: stringValue(option.id, `${id} option id`),
            name: stringValue(option.name, `${id} option name`),
            description: stringValue(
              option.description,
              `${id} option description`,
            ),
            type: type as WidgetOption["type"],
            ...(option.default === undefined
              ? {}
              : { default: option.default }),
            ...(typeof option.minimum === "number"
              ? { minimum: option.minimum }
              : {}),
            ...(typeof option.maximum === "number"
              ? { maximum: option.maximum }
              : {}),
            ...(choices === undefined ? {} : { choices }),
            variants: stringArray(option.variants, `${id} option variants`),
          };
        })
      : [];
    const optionIds = new Set<string>();
    for (const option of options) {
      if (optionIds.has(option.id)) {
        throw new Error(
          `Invalid dashboard catalog: duplicate ${id} option ${option.id}`,
        );
      }
      optionIds.add(option.id);
      if (option.variants.some((variantId) => !variantIds.has(variantId))) {
        throw new Error(
          `Invalid dashboard catalog: ${id} option ${option.id} names an unknown variant`,
        );
      }
    }
    const inputs = Array.isArray(widget.inputs)
      ? widget.inputs.map((rawInput, inputIndex): WidgetInput => {
          const input = objectValue(
            rawInput,
            `widgets[${widgetIndex}].inputs[${inputIndex}]`,
          );
          if (typeof input.required !== "boolean") {
            throw new Error(
              `Invalid dashboard catalog: ${id} input required must be boolean`,
            );
          }
          return {
            id: stringValue(input.id, `${id} input id`),
            name: stringValue(input.name, `${id} input name`),
            type: stringValue(input.type, `${id} input type`),
            required: input.required,
            variants: stringArray(input.variants, `${id} input variants`),
          };
        })
      : [];
    const inputIds = new Set<string>();
    for (const input of inputs) {
      if (inputIds.has(input.id)) {
        throw new Error(
          `Invalid dashboard catalog: duplicate ${id} input ${input.id}`,
        );
      }
      inputIds.add(input.id);
      if (input.variants.some((variantId) => !variantIds.has(variantId))) {
        throw new Error(
          `Invalid dashboard catalog: ${id} input ${input.id} names an unknown variant`,
        );
      }
    }
    return {
      id,
      name: stringValue(widget.name, `${id} name`),
      description: stringValue(widget.description, `${id} description`),
      variants,
      options,
      inputs,
    };
  });
  return { widgets };
}

function optionSchema(option: WidgetOption): TSchema {
  const metadata = {
    description: option.description,
    ...(option.default === undefined ? {} : { default: option.default }),
  };
  if (option.type === "boolean") return Type.Boolean(metadata);
  if (option.type === "integer") {
    return Type.Integer({
      ...metadata,
      ...(option.minimum === undefined ? {} : { minimum: option.minimum }),
      ...(option.maximum === undefined ? {} : { maximum: option.maximum }),
    });
  }
  if (option.type === "select") {
    const values = option.choices?.map((choice) => choice.value) ?? [];
    if (values.length === 0) {
      throw new Error(`Invalid dashboard catalog: ${option.id} has no choices`);
    }
    return Type.Union(
      values.map((value) => Type.Literal(value)),
      metadata,
    );
  }
  return Type.String({ ...metadata, maxLength: 16_384 });
}

function appliesToVariant(item: { variants: string[] }, variantId: string) {
  return item.variants.length === 0 || item.variants.includes(variantId);
}

function widgetOpenSchema(catalog: WidgetCatalog): TSchema {
  const branches: TSchema[] = [];
  for (const widget of catalog.widgets) {
    for (const variant of widget.variants) {
      const optionProperties: Record<string, TSchema> = {};
      for (const option of widget.options.filter((item) =>
        appliesToVariant(item, variant.id),
      )) {
        optionProperties[option.id] = Type.Optional(optionSchema(option));
      }
      const inputProperties: Record<string, TSchema> = {};
      for (const input of widget.inputs.filter((item) =>
        appliesToVariant(item, variant.id),
      )) {
        const schema = Type.Object(
          {
            type: Type.Literal(input.type),
            value: Type.Unknown(),
          },
          { additionalProperties: false, description: input.name },
        );
        inputProperties[input.id] = input.required
          ? schema
          : Type.Optional(schema);
      }
      const inputsSchema = Type.Object(inputProperties, {
        additionalProperties: false,
      });
      const hasRequiredInput = widget.inputs.some(
        (input) => input.required && appliesToVariant(input, variant.id),
      );
      branches.push(
        Type.Object(
          {
            widget_id: Type.Literal(widget.id),
            variant_id: Type.Literal(variant.id),
            options: Type.Optional(
              Type.Object(optionProperties, { additionalProperties: false }),
            ),
            inputs: hasRequiredInput
              ? inputsSchema
              : Type.Optional(inputsSchema),
            presentation: Type.Optional(StringEnum(["focus", "tile"] as const)),
          },
          {
            additionalProperties: false,
            description: `${widget.name} - ${variant.name}: ${widget.description}`,
          },
        ),
      );
    }
  }
  if (branches.length === 0) {
    throw new Error("Invalid dashboard catalog: no widget variants");
  }
  return Type.Union(branches);
}

function dashboardFailure(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  const details = {
    error: message,
    ...(error instanceof DashboardError && error.code
      ? { error_code: error.code }
      : {}),
  };
  const prefix = "error_code" in details ? `${details.error_code}: ` : "";
  return toolResult(details, true, `Dashboard error: ${prefix}${message}`);
}

export default function scufris(pi: ExtensionAPI): void {
  const jobs = new Map<string, OwnedJob>();
  const surfaces = new Map<string, OwnedSurface>();
  let timer: ReturnType<typeof setInterval> | undefined;
  let pollRunning = false;
  let pollError: string | undefined;
  let shuttingDown = false;
  let widgetToolsRegistered = false;
  let context: ExtensionContext | undefined;

  const sendJobEvent = (job: OwnedJob, event: string, triggerTurn: boolean) => {
    pi.sendMessage(
      {
        customType: "scufris-job-event",
        content: `Scufris job ${job.job_id}: ${event}`,
        display: true,
        details: { job_id: job.job_id, event },
      },
      { deliverAs: "followUp", triggerTurn },
    );
  };

  const poll = async () => {
    if (pollRunning || shuttingDown || !context) return;
    const active = [...jobs.values()].filter(
      (job) =>
        job.state !== "stopped" &&
        job.state !== "done" &&
        job.state !== "failed",
    );
    if (active.length === 0 && surfaces.size === 0) {
      context.ui.setStatus("scufris", undefined);
      return;
    }
    pollRunning = true;
    try {
      const result = await runHelper<PollResult>("poll", {
        jobs: active.map((job) => ({
          job_id: job.job_id,
          offset: job.offset,
          tail: job.tail,
          inode: job.inode,
        })),
      });
      if (shuttingDown) return;
      const routine: string[] = [];
      for (const update of result.jobs) {
        const job = jobs.get(update.job_id);
        if (!job) continue;
        job.offset = update.offset;
        job.tail = update.tail;
        job.inode = update.inode;
        job.window_alive = update.window_alive;

        for (const error of update.errors) {
          const errorIdentity = `${update.offset}:${error}`;
          if (job.protocolErrors.has(errorIdentity)) continue;
          job.protocolErrors.add(errorIdentity);
          context.ui.notify(`Job ${job.job_id}: ${error}`, "error");
          sendJobEvent(job, `protocol-error: ${error}`, true);
        }
        for (const line of update.events) {
          const event = parseEvent(line);
          job.state = event.state;
          job.summary = event.summary;
          if (event.state === "working" || event.state === "review-ready") {
            routine.push(`${job.job_id}: ${line}`);
          } else {
            sendJobEvent(job, line, true);
          }
        }
        if (
          !job.window_alive &&
          !job.exitReported &&
          job.state !== "done" &&
          job.state !== "failed" &&
          job.state !== "stopped"
        ) {
          job.exitReported = true;
          job.state = "failed";
          job.summary = "worker exited without terminal status";
          context.ui.notify(`Job ${job.job_id} exited`, "error");
          sendJobEvent(
            job,
            "failed: worker exited without terminal status",
            true,
          );
        }
      }
      if (surfaces.size > 0) {
        const listed = await runDashboard<{ surfaces: WidgetSurface[] }>(
          "list",
          {},
        );
        if (!Array.isArray(listed.surfaces)) {
          throw new Error("Invalid dashboard list result");
        }
        const present = new Set(
          listed.surfaces
            .map((surface) => surface.surface_id)
            .filter(
              (surfaceId): surfaceId is string => typeof surfaceId === "string",
            ),
        );
        for (const [surfaceId] of surfaces) {
          if (present.has(surfaceId)) continue;
          surfaces.delete(surfaceId);
          const content = `Widget surface ${surfaceId} was closed outside Scufris.`;
          context.ui.notify(content, "info");
          pi.sendMessage(
            {
              customType: "scufris-widget-event",
              content,
              display: true,
              details: { surface_id: surfaceId, event: "closed" },
            },
            { deliverAs: "followUp", triggerTurn: false },
          );
        }
      }
      if (routine.length > 0) context.ui.notify(routine.join("\n"), "info");
      const running = active.filter((job) => job.window_alive).length;
      context.ui.setStatus(
        "scufris",
        running > 0
          ? `${running} delegated job${running === 1 ? "" : "s"}`
          : undefined,
      );
      pollError = undefined;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (!shuttingDown && message !== pollError) {
        context.ui.notify(message, "error");
      }
      pollError = message;
    } finally {
      pollRunning = false;
    }
  };

  const spawnTool = defineTool({
    name: "scufris_agent_spawn",
    label: "Spawn delegated agent",
    description:
      "Start one independent Pi or Claude coding worker in an isolated worktree.",
    parameters: Type.Object(
      {
        harness: StringEnum(["pi", "claude"] as const),
        instructions: Type.String({ minLength: 1, maxLength: 262_144 }),
        model: Type.Optional(Type.String({ minLength: 1, maxLength: 200 })),
        thinking: Type.Optional(
          StringEnum([
            "off",
            "minimal",
            "low",
            "medium",
            "high",
            "xhigh",
            "max",
          ] as const),
        ),
      },
      { additionalProperties: false },
    ),
    async execute(_id, params, signal, _update, ctx) {
      try {
        const result = await runHelper<SpawnResult>(
          "spawn",
          {
            job_id: generatedJobId(),
            harness: params.harness,
            instructions: params.instructions,
            project_root: ctx.cwd,
            ...(params.model === undefined ? {} : { model: params.model }),
            ...(params.thinking === undefined
              ? {}
              : { thinking: params.thinking }),
          },
          signal,
          120_000,
        );
        if (shuttingDown) {
          await runHelper("stop", { job_id: result.job_id }, undefined, 15_000);
          return toolResult({ error: "Pi session is shutting down" }, true);
        }
        jobs.set(result.job_id, {
          job_id: result.job_id,
          harness: result.harness,
          feature: result.feature,
          state: result.state,
          summary: "worker starting",
          offset: 0,
          tail: "",
          inode: null,
          window_alive: true,
          protocolErrors: new Set(),
          exitReported: false,
        });
        void poll();
        return toolResult(result);
      } catch (error) {
        return toolResult(
          { error: error instanceof Error ? error.message : String(error) },
          true,
        );
      }
    },
  });

  const listTool = defineTool({
    name: "scufris_agent_list",
    label: "List delegated agents",
    description: "List jobs owned by this Pi session.",
    parameters: Type.Object({}, { additionalProperties: false }),
    async execute() {
      return toolResult({
        jobs: [...jobs.values()].map((job) => ({
          job_id: job.job_id,
          harness: job.harness,
          state: job.state,
          summary: job.summary,
          feature: job.feature,
          window_alive: job.window_alive,
        })),
      });
    },
  });

  const inspectTool = defineTool({
    name: "scufris_agent_inspect",
    label: "Inspect delegated agent",
    description:
      "Inspect one owned job and optionally include its bounded report.",
    parameters: Type.Object(
      {
        job_id: Type.String({ pattern: "^[a-z0-9]{12}$" }),
        include_report: Type.Optional(Type.Boolean({ default: false })),
      },
      { additionalProperties: false },
    ),
    async execute(_id, params, signal) {
      if (!jobs.has(params.job_id)) {
        return toolResult(
          { error: "job is not owned by this Pi session" },
          true,
        );
      }
      try {
        return toolResult(
          await runHelper<InspectResult>(
            "inspect",
            {
              job_id: params.job_id,
              include_report: params.include_report ?? false,
            },
            signal,
          ),
        );
      } catch (error) {
        return toolResult(
          { error: error instanceof Error ? error.message : String(error) },
          true,
        );
      }
    },
  });

  const sendTool = defineTool({
    name: "scufris_agent_send",
    label: "Steer delegated agent",
    description: "Submit one literal steering message to an owned worker.",
    parameters: Type.Object(
      {
        job_id: Type.String({ pattern: "^[a-z0-9]{12}$" }),
        message: Type.String({
          minLength: 1,
          maxLength: 16_384,
          pattern: "^[^\\r\\n]+$",
        }),
      },
      { additionalProperties: false },
    ),
    async execute(_id, params, signal) {
      if (!jobs.has(params.job_id)) {
        return toolResult(
          { error: "job is not owned by this Pi session" },
          true,
        );
      }
      try {
        return toolResult(await runHelper("send", params, signal, 20_000));
      } catch (error) {
        return toolResult(
          { error: error instanceof Error ? error.message : String(error) },
          true,
        );
      }
    },
  });

  const stopTool = defineTool({
    name: "scufris_agent_stop",
    label: "Stop delegated agent",
    description:
      "Stop one owned worker without deleting its worktree or evidence.",
    parameters: Type.Object(
      { job_id: Type.String({ pattern: "^[a-z0-9]{12}$" }) },
      { additionalProperties: false },
    ),
    async execute(_id, params, signal) {
      const job = jobs.get(params.job_id);
      if (!job) {
        return toolResult(
          { error: "job is not owned by this Pi session" },
          true,
        );
      }
      try {
        const result = await runHelper<{ job_id: string; state: string }>(
          "stop",
          params,
          signal,
          15_000,
        );
        job.state = "stopped";
        job.summary = "stopped by Scufris";
        job.window_alive = false;
        return toolResult(result);
      } catch (error) {
        return toolResult(
          { error: error instanceof Error ? error.message : String(error) },
          true,
        );
      }
    },
  });

  const registerWidgetTools = (catalog: WidgetCatalog) => {
    const catalogSummary = catalog.widgets
      .map(
        (widget) =>
          `${widget.name} (${widget.id}): ${widget.variants.map((variant) => variant.id).join(", ")}`,
      )
      .join("; ");
    const surfaceIdSchema = Type.String({
      pattern: "^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$",
    });
    const inputMapSchema = Type.Record(
      Type.String({ pattern: "^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$" }),
      Type.Object(
        {
          type: Type.String({ minLength: 1, maxLength: 256 }),
          value: Type.Unknown(),
        },
        { additionalProperties: false },
      ),
    );

    pi.registerTool(
      defineTool({
        name: "scufris_widget_open",
        label: "Open dashboard widget",
        description: `Open one discovered native dashboard widget. Available: ${catalogSummary}`,
        parameters: widgetOpenSchema(catalog),
        async execute(_id, rawParams, signal) {
          const params = rawParams as WidgetOpenParams;
          try {
            const result = await runDashboard<Record<string, unknown>>(
              "open",
              {
                widget_id: params.widget_id,
                variant_id: params.variant_id,
                options: params.options ?? {},
                inputs: params.inputs ?? {},
                presentation: params.presentation ?? "focus",
              },
              signal,
            );
            const surfaceId = result.surface_id;
            if (
              typeof surfaceId !== "string" ||
              !/^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$/.test(surfaceId)
            ) {
              throw new Error("Invalid dashboard open result: surface_id");
            }
            surfaces.set(surfaceId, {
              surface_id: surfaceId,
              widget_id: params.widget_id,
              variant_id: params.variant_id,
            });
            const details = {
              widget_id: params.widget_id,
              variant_id: params.variant_id,
              surface_id: surfaceId,
              state: "open",
            };
            return toolResult(
              details,
              false,
              `Opened ${params.widget_id} ${params.variant_id} view as ${surfaceId}.`,
            );
          } catch (error) {
            return dashboardFailure(error);
          }
        },
      }),
    );

    pi.registerTool(
      defineTool({
        name: "scufris_widget_update",
        label: "Update dashboard widget",
        description:
          "Replace inputs or presentation on one surface opened by Scufris.",
        parameters: Type.Object(
          {
            surface_id: surfaceIdSchema,
            inputs: Type.Optional(inputMapSchema),
            presentation: Type.Optional(StringEnum(["focus", "tile"] as const)),
          },
          { additionalProperties: false },
        ),
        async execute(_id, params, signal) {
          if (!surfaces.has(params.surface_id)) {
            return toolResult(
              { error: "surface is not owned by this Pi session" },
              true,
            );
          }
          if (
            params.inputs === undefined &&
            params.presentation === undefined
          ) {
            return toolResult(
              { error: "update requires inputs or presentation" },
              true,
            );
          }
          try {
            await runDashboard(
              "update",
              {
                surface_id: params.surface_id,
                ...(params.inputs === undefined
                  ? {}
                  : { inputs: params.inputs }),
                ...(params.presentation === undefined
                  ? {}
                  : { presentation: params.presentation }),
              },
              signal,
            );
            return toolResult(
              { surface_id: params.surface_id, state: "updated" },
              false,
              `Updated widget surface ${params.surface_id}.`,
            );
          } catch (error) {
            if (
              error instanceof DashboardError &&
              error.code === "surface_not_found"
            ) {
              surfaces.delete(params.surface_id);
            }
            return dashboardFailure(error);
          }
        },
      }),
    );

    pi.registerTool(
      defineTool({
        name: "scufris_widget_list",
        label: "List dashboard widgets",
        description:
          "List native dashboard surfaces and identify those owned by this Pi session.",
        parameters: Type.Object({}, { additionalProperties: false }),
        async execute(_id, _params, signal) {
          try {
            const result = await runDashboard<{ surfaces: WidgetSurface[] }>(
              "list",
              {},
              signal,
            );
            if (!Array.isArray(result.surfaces)) {
              throw new Error("Invalid dashboard list result");
            }
            const listed = result.surfaces.map((surface) => ({
              ...surface,
              owned: surfaces.has(surface.surface_id),
            }));
            const ownedCount = listed.filter((surface) => surface.owned).length;
            return toolResult(
              { surfaces: listed },
              false,
              `${listed.length} dashboard surface${listed.length === 1 ? "" : "s"} open; ${ownedCount} owned by this Pi session.`,
            );
          } catch (error) {
            return dashboardFailure(error);
          }
        },
      }),
    );

    pi.registerTool(
      defineTool({
        name: "scufris_widget_focus",
        label: "Focus dashboard widget",
        description: "Focus one surface opened by Scufris.",
        parameters: Type.Object(
          { surface_id: surfaceIdSchema },
          { additionalProperties: false },
        ),
        async execute(_id, params, signal) {
          if (!surfaces.has(params.surface_id)) {
            return toolResult(
              { error: "surface is not owned by this Pi session" },
              true,
            );
          }
          try {
            await runDashboard("focus", params, signal);
            return toolResult(
              { surface_id: params.surface_id, state: "focused" },
              false,
              `Focused widget surface ${params.surface_id}.`,
            );
          } catch (error) {
            if (
              error instanceof DashboardError &&
              error.code === "surface_not_found"
            ) {
              surfaces.delete(params.surface_id);
            }
            return dashboardFailure(error);
          }
        },
      }),
    );

    pi.registerTool(
      defineTool({
        name: "scufris_widget_close",
        label: "Close dashboard widget",
        description: "Close one surface opened by Scufris.",
        parameters: Type.Object(
          { surface_id: surfaceIdSchema },
          { additionalProperties: false },
        ),
        async execute(_id, params, signal) {
          if (!surfaces.has(params.surface_id)) {
            return toolResult(
              { error: "surface is not owned by this Pi session" },
              true,
            );
          }
          try {
            await runDashboard("close", params, signal);
            surfaces.delete(params.surface_id);
            return toolResult(
              { surface_id: params.surface_id, state: "closed" },
              false,
              `Closed widget surface ${params.surface_id}.`,
            );
          } catch (error) {
            if (
              error instanceof DashboardError &&
              error.code === "surface_not_found"
            ) {
              surfaces.delete(params.surface_id);
            }
            return dashboardFailure(error);
          }
        },
      }),
    );
  };

  pi.registerTool(spawnTool);
  pi.registerTool(listTool);
  pi.registerTool(inspectTool);
  pi.registerTool(sendTool);
  pi.registerTool(stopTool);

  pi.on("session_start", async (_event, ctx) => {
    context = ctx;
    shuttingDown = false;
    if (!widgetToolsRegistered) {
      try {
        const catalog = validateWidgetCatalog(
          await runDashboard<unknown>("discover", {}),
        );
        registerWidgetTools(catalog);
        widgetToolsRegistered = true;
      } catch (error) {
        ctx.ui.notify(
          `Scufris widget discovery failed: ${error instanceof Error ? error.message : String(error)}`,
          "error",
        );
      }
    }
    try {
      const result = await runHelper<{ job_ids: string[] }>("orphans", {});
      if (result.job_ids.length > 0) {
        const content = `Possible Scufris orphan jobs: ${result.job_ids.join(", ")}. They were not adopted.`;
        ctx.ui.notify(content, "warning");
        pi.sendMessage(
          {
            customType: "scufris-job-event",
            content,
            display: true,
            details: { event: "orphans", job_ids: result.job_ids },
          },
          { deliverAs: "followUp", triggerTurn: true },
        );
      }
    } catch (error) {
      ctx.ui.notify(
        `Scufris orphan scan failed: ${error instanceof Error ? error.message : String(error)}`,
        "error",
      );
    }
    if (!shuttingDown) timer = setInterval(() => void poll(), 1_000);
  });

  pi.on("session_shutdown", async () => {
    shuttingDown = true;
    if (timer) clearInterval(timer);
    timer = undefined;
    while (pollRunning) {
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    const owned = [...jobs.values()].filter((job) => job.window_alive);
    await Promise.allSettled(
      owned.map((job) =>
        runHelper("stop", { job_id: job.job_id }, undefined, 15_000),
      ),
    );
    context?.ui.setStatus("scufris", undefined);
    jobs.clear();
    surfaces.clear();
    context = undefined;
  });
}
