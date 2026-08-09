/*
 * Regression invariants adapted from block/buzz at
 * 5bf78671f45178f8de02ba18d3d321cbbf19cd1f, Apache-2.0.
 * Original implementation:
 * desktop/src/features/agents/ui/agentSessionTranscriptGrouping.ts.
 * Test basis:
 * desktop/src/features/agents/ui/agentSessionTranscriptGrouping.test.mjs.
 * PiWork changes: fixtures cover protocol-derived Session/Turn keys and
 * semantic Pi tool bursts without Channel or Relay transport framing.
 */
import { describe, expect, it } from "vitest";

import type {
  ActivityDescriptor,
  ActivityItem,
  ActivityRenderClass,
} from "./activityTypes";
import { buildActivityDisplayGroups } from "./activityGrouping";

type ToolItem = Extract<ActivityItem, { type: "tool" }>;

const tool = (
  id: string,
  sessionId: string | null = "s1",
  turnId: string | null = "t1",
  options: {
    renderClass?: ActivityRenderClass;
    descriptorGroupKey?: string | null;
    status?: ToolItem["status"];
    isError?: boolean;
    runId?: string;
  } = {},
): ToolItem => {
  const renderClass = options.renderClass ?? "file-read";
  const descriptor: ActivityDescriptor = {
    renderClass,
    action: renderClass === "file-read" ? "read" : "invoke",
    object: "src/app.tsx",
    preview: "done",
    tone: renderClass === "file-read" ? "read" : "neutral",
    groupKey:
      options.descriptorGroupKey === undefined
        ? "read:read"
        : options.descriptorGroupKey,
  };

  return {
    id: `tool:${options.runId ?? "run-1"}:${id}`,
    type: "tool",
    workId: "work-1",
    runId: options.runId ?? "run-1",
    turnId,
    sessionId,
    agentId: null,
    assignmentId: null,
    timestamp: "2026-08-09T00:00:00.000Z",
    renderClass,
    toolCallId: id,
    toolName: "read",
    status: options.status ?? "completed",
    input: "src/app.tsx",
    result: "done",
    isError: options.isError ?? false,
    descriptor,
  };
};

const interruption = (
  type: "permission" | "plan" | "thought" | "raw",
  id: string = type,
): ActivityItem => {
  const base = {
    id,
    workId: "work-1",
    runId: "run-1",
    turnId: "t1",
    sessionId: "s1",
    agentId: null,
    assignmentId: null,
    timestamp: "2026-08-09T00:00:00.000Z",
  };

  if (type === "permission") {
    return {
      ...base,
      type,
      renderClass: "permission",
      requestId: id,
      toolCallId: null,
      title: "Allow read",
      detail: "src/app.tsx",
      status: "requested",
      outcome: null,
    };
  }
  if (type === "plan") {
    return {
      ...base,
      type,
      renderClass: "plan",
      planId: id,
      revision: 1,
      text: "Read files",
    };
  }
  if (type === "thought") {
    return { ...base, type, renderClass: "thought", text: "Thinking" };
  }
  return {
    ...base,
    type,
    renderClass: "raw-rail",
    kind: "raw",
    payloadJson: "{}",
  };
};

describe("buildActivityDisplayGroups", () => {
  it("keeps stable session and turn keys across prepended history", () => {
    const current = buildActivityDisplayGroups([tool("a", "s2", "t2")]);
    const withHistory = buildActivityDisplayGroups([
      tool("old", "s1", "t1"),
      tool("a", "s2", "t2"),
    ]);

    expect(current[0]?.key).toBe(
      "session:s2:turn:t2:segment:tool:run-1:a",
    );
    expect(withHistory.at(-1)?.key).toBe(current[0]?.key);
  });

  it("uses the legacy session and run fallback exactly", () => {
    const groups = buildActivityDisplayGroups([
      tool("legacy", null, null, { runId: "run-legacy" }),
    ]);

    expect(groups).toMatchObject([
      {
        key: "session:legacy:turn:run-legacy:segment:tool:run-legacy:legacy",
        sessionId: null,
        turnId: null,
      },
    ]);
  });

  it("preserves interleaved repeated identities as contiguous ordered groups", () => {
    const groups = buildActivityDisplayGroups([
      tool("a", "s1", "t1"),
      tool("b", "s2", "t2"),
      tool("c", "s1", "t1"),
    ]);

    expect(groups.map(({ key }) => key)).toEqual([
      "session:s1:turn:t1:segment:tool:run-1:a",
      "session:s2:turn:t2:segment:tool:run-1:b",
      "session:s1:turn:t1:segment:tool:run-1:c",
    ]);
    expect(
      groups.flatMap(({ blocks }) =>
        blocks.flatMap((block) =>
          block.kind === "item"
            ? [block.item.id]
            : block.items.map(({ id }) => id),
        ),
      ),
    ).toEqual([
      "tool:run-1:a",
      "tool:run-1:b",
      "tool:run-1:c",
    ]);
    expect(new Set(groups.map(({ key }) => key)).size).toBe(3);
  });

  it("escapes reserved identity values without colliding with legacy groups", () => {
    const groups = buildActivityDisplayGroups([
      tool("null-session", null, "t1"),
      tool("literal-legacy", "legacy", "t1"),
      tool("delimited-a", "a:turn:b", "c"),
      tool("delimited-b", "a", "b:turn:c"),
    ]);

    expect(groups.map(({ key }) => key)).toEqual([
      "session:legacy:turn:t1:segment:tool:run-1:null-session",
      "session:%6Cegacy:turn:t1:segment:tool:run-1:literal-legacy",
      "session:a%3Aturn%3Ab:turn:c:segment:tool:run-1:delimited-a",
      "session:a:turn:b%3Aturn%3Ac:segment:tool:run-1:delimited-b",
    ]);
  });

  it("bursts consecutive successful tools with the same descriptor group key", () => {
    const groups = buildActivityDisplayGroups([
      tool("read-1"),
      tool("read-2"),
    ]);

    expect(groups[0]?.blocks).toMatchObject([
      {
        kind: "toolBurst",
        renderClass: "file-read",
        count: 2,
        items: [{ id: "tool:run-1:read-1" }, { id: "tool:run-1:read-2" }],
      },
    ]);
  });

  it.each([
    ["different session", tool("read-2", "s2", "t1")],
    ["different turn", tool("read-2", "s1", "t2")],
  ])("does not burst across a %s boundary", (_label, second) => {
    const groups = buildActivityDisplayGroups([tool("read-1"), second]);

    expect(groups).toHaveLength(2);
    expect(groups.flatMap(({ blocks }) => blocks)).toMatchObject([
      { kind: "item", item: { id: "tool:run-1:read-1" } },
      { kind: "item", item: { id: "tool:run-1:read-2" } },
    ]);
  });

  it("does not burst different descriptor group keys with the same render class", () => {
    const groups = buildActivityDisplayGroups([
      tool("read-1", "s1", "t1", { descriptorGroupKey: "read:read" }),
      tool("grep-1", "s1", "t1", { descriptorGroupKey: "read:grep" }),
    ]);

    expect(groups[0]?.blocks).toMatchObject([
      { kind: "item", item: { id: "tool:run-1:read-1" } },
      { kind: "item", item: { id: "tool:run-1:grep-1" } },
    ]);
  });

  it("does not burst tools without a descriptor group key", () => {
    const groups = buildActivityDisplayGroups([
      tool("generic-1", "s1", "t1", { descriptorGroupKey: null }),
      tool("generic-2", "s1", "t1", { descriptorGroupKey: null }),
    ]);

    expect(groups[0]?.blocks).toMatchObject([
      { kind: "item", item: { id: "tool:run-1:generic-1" } },
      { kind: "item", item: { id: "tool:run-1:generic-2" } },
    ]);
  });

  it.each(["permission", "plan", "thought", "raw"] as const)(
    "keeps %s interruptions outside tool bursts",
    (type) => {
      const groups = buildActivityDisplayGroups([
        tool("read-1"),
        interruption(type),
        tool("read-2"),
      ]);

      expect(groups[0]?.blocks).toMatchObject([
        { kind: "item", item: { id: "tool:run-1:read-1" } },
        { kind: "item", item: { type } },
        { kind: "item", item: { id: "tool:run-1:read-2" } },
      ]);
    },
  );

  it.each([
    ["failed status", { status: "failed" as const }],
    ["error flag", { isError: true }],
    ["error render class", { renderClass: "error" as const }],
  ])("never includes a tool with %s in a burst", (_label, options) => {
    const groups = buildActivityDisplayGroups([
      tool("read-1"),
      tool("failed", "s1", "t1", options),
      tool("read-2"),
    ]);

    expect(groups[0]?.blocks).toHaveLength(3);
    expect(groups[0]?.blocks.every(({ kind }) => kind === "item")).toBe(true);
  });

  it("keeps pending, executing, and single completed tools as items", () => {
    const groups = buildActivityDisplayGroups([
      tool("pending", "s1", "t1", { status: "pending" }),
      tool("executing", "s1", "t1", { status: "executing" }),
      tool("completed"),
    ]);

    expect(groups[0]?.blocks).toHaveLength(3);
    expect(groups[0]?.blocks.every(({ kind }) => kind === "item")).toBe(true);
  });

  it("keeps item and burst block keys stable when history is prepended", () => {
    const items = [
      tool("read-1", "s2", "t2"),
      tool("read-2", "s2", "t2"),
      interruption("plan", "plan-current"),
    ].map((item) =>
      item.id === "plan-current"
        ? { ...item, sessionId: "s2", turnId: "t2" }
        : item,
    ) as ActivityItem[];
    const current = buildActivityDisplayGroups(items)[0];
    const withHistory = buildActivityDisplayGroups([
      tool("old", "s1", "t1"),
      ...items,
    ]).at(-1);

    expect(current?.blocks.map(({ key }) => key)).toEqual([
      "tool-burst:tool:run-1:read-1",
      "plan-current",
    ]);
    expect(withHistory?.blocks.map(({ key }) => key)).toEqual(
      current?.blocks.map(({ key }) => key),
    );
  });
});
