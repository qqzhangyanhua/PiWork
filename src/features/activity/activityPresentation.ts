/*
 * Adapted from block/buzz at 5bf78671f45178f8de02ba18d3d321cbbf19cd1f,
 * Apache-2.0. Original classifier:
 * desktop/src/features/agents/ui/agentSessionToolClassifier.ts.
 * Descriptor/render-class types are influenced by:
 * desktop/src/features/agents/ui/agentSessionTypes.ts.
 * PiWork changes: reduces Buzz/Relay/MCP-specific providers to typed, pure
 * read/write/shell semantics over Pi tool names and summary JSON.
 */
import type {
  ActivityAction,
  ActivityDescriptor,
  ActivityRenderClass,
  ActivityTone,
} from "./activityTypes";

const parseSummary = (summary: string): Record<string, unknown> => {
  try {
    const value: unknown = JSON.parse(summary);
    return value && typeof value === "object" && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : {};
  } catch {
    return {};
  }
};

const stringField = (
  fields: Record<string, unknown>,
  ...keys: string[]
): string | null => {
  for (const key of keys) {
    const value = fields[key];
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return null;
};

const descriptor = (
  renderClass: ActivityRenderClass,
  action: ActivityAction,
  object: string | null,
  tone: ActivityTone,
  groupKey: string,
): ActivityDescriptor => ({
  renderClass,
  action,
  object,
  preview: null,
  tone,
  groupKey,
});

export function describeTool(
  toolName: string,
  summary: string,
): ActivityDescriptor {
  const normalized = toolName.trim().toLowerCase();
  const fields = parseSummary(summary);
  const path = stringField(fields, "path", "file_path");
  const command = stringField(fields, "command");

  if (/^(read|grep|find|ls)(?:_|$)/u.test(normalized)) {
    return descriptor(
      "file-read",
      "read",
      path ?? summary,
      "read",
      `read:${normalized}`,
    );
  }
  if (/^(edit|write)(?:_|$)/u.test(normalized)) {
    return descriptor(
      "file-edit",
      "write",
      path ?? summary,
      "write",
      `write:${normalized}`,
    );
  }
  if (normalized === "bash") {
    return descriptor(
      "shell",
      "execute",
      command ?? summary,
      "admin",
      "shell",
    );
  }
  return descriptor(
    "generic",
    "invoke",
    toolName,
    "neutral",
    `tool:${normalized}`,
  );
}
