import { fileURLToPath } from "node:url";
import { StringEnum, Type, type TSchema } from "@earendil-works/pi-ai";
import {
  defineTool,
  type ExtensionAPI,
  type ExtensionContext,
} from "@earendil-works/pi-coding-agent";
import { runPrivateHelper, toolResult } from "../shared/runtime.ts";

const dashboardHelperPath = fileURLToPath(
  new URL("../../../tools/dashboard/scufris-dashboard", import.meta.url),
);

class DashboardError extends Error {
  constructor(
    message: string,
    readonly code?: string,
  ) {
    super(message);
  }
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
  const surfaces = new Map<string, OwnedSurface>();
  let timer: ReturnType<typeof setInterval> | undefined;
  let pollRunning = false;
  let pollError: string | undefined;
  let shuttingDown = false;
  let widgetToolsRegistered = false;
  let context: ExtensionContext | undefined;

  const poll = async () => {
    if (pollRunning || shuttingDown || !context || surfaces.size === 0) return;
    pollRunning = true;
    try {
      const listed = await runDashboard<{ surfaces: WidgetSurface[] }>(
        "list",
        {},
      );
      if (shuttingDown) return;
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
    if (!shuttingDown) timer = setInterval(() => void poll(), 1_000);
  });

  pi.on("session_shutdown", async () => {
    shuttingDown = true;
    if (timer) clearInterval(timer);
    timer = undefined;
    while (pollRunning) {
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    surfaces.clear();
    context = undefined;
  });
}
