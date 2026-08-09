/*
 * Adapted from block/buzz at 5bf78671f45178f8de02ba18d3d321cbbf19cd1f,
 * Apache-2.0. Original: desktop/src/features/agents/ui/agentSessionTranscriptGrouping.ts.
 * PiWork changes: replaces Channel prompt/setup framing with stable
 * Session/Turn groups and consecutive semantic tool bursts.
 */
import type { ActivityItem, ActivityRenderClass } from "./activityTypes";

type ToolItem = Extract<ActivityItem, { type: "tool" }>;

export type ActivityDisplayBlock =
  | { kind: "item"; key: string; item: ActivityItem }
  | {
      kind: "toolBurst";
      key: string;
      renderClass: ActivityRenderClass;
      count: number;
      items: ActivityItem[];
    };

export type ActivityDisplayGroup = {
  key: string;
  sessionId: string | null;
  turnId: string | null;
  blocks: ActivityDisplayBlock[];
};

const escapeIdentityPart = (value: string): string =>
  encodeURIComponent(value);

const sessionIdentityPart = (sessionId: string | null): string => {
  if (sessionId === null) {
    return "legacy";
  }
  return sessionId === "legacy"
    ? "%6Cegacy"
    : escapeIdentityPart(sessionId);
};

const identityBase = (item: ActivityItem): string =>
  `session:${sessionIdentityPart(item.sessionId)}:turn:${escapeIdentityPart(item.turnId ?? item.runId)}`;

const isBurstableTool = (item: ActivityItem): item is ToolItem =>
  item.type === "tool" &&
  item.status === "completed" &&
  !item.isError &&
  item.renderClass !== "error" &&
  item.descriptor.renderClass !== "error" &&
  item.descriptor.groupKey !== null;

const itemBlock = (item: ActivityItem): ActivityDisplayBlock => ({
  kind: "item",
  key: item.id,
  item,
});

export function buildActivityDisplayGroups(
  items: ActivityItem[],
): ActivityDisplayGroup[] {
  const groups: ActivityDisplayGroup[] = [];
  let currentGroup: ActivityDisplayGroup | null = null;
  let currentIdentityBase: string | null = null;
  let previousItem: ActivityItem | null = null;

  for (const item of items) {
    const nextIdentityBase = identityBase(item);
    if (currentGroup === null || nextIdentityBase !== currentIdentityBase) {
      currentGroup = {
        key: `${nextIdentityBase}:segment:${item.id}`,
        sessionId: item.sessionId,
        turnId: item.turnId,
        blocks: [],
      };
      currentIdentityBase = nextIdentityBase;
      previousItem = null;
      groups.push(currentGroup);
    }

    const previousTool =
      previousItem && isBurstableTool(previousItem) ? previousItem : null;
    const canJoinPrevious =
      previousTool !== null &&
      isBurstableTool(item) &&
      previousTool.descriptor.groupKey === item.descriptor.groupKey;

    if (!canJoinPrevious) {
      currentGroup.blocks.push(itemBlock(item));
      previousItem = item;
      continue;
    }

    const lastBlock = currentGroup.blocks.at(-1);
    if (
      lastBlock?.kind === "item" &&
      lastBlock.item.id === previousTool.id
    ) {
      currentGroup.blocks[currentGroup.blocks.length - 1] = {
        kind: "toolBurst",
        key: `tool-burst:${previousTool.id}`,
        renderClass: previousTool.renderClass,
        count: 2,
        items: [previousTool, item],
      };
    } else if (
      lastBlock?.kind === "toolBurst" &&
      lastBlock.items.at(-1)?.id === previousTool.id
    ) {
      currentGroup.blocks[currentGroup.blocks.length - 1] = {
        ...lastBlock,
        count: lastBlock.count + 1,
        items: [...lastBlock.items, item],
      };
    } else {
      currentGroup.blocks.push(itemBlock(item));
    }

    previousItem = item;
  }

  return groups;
}
