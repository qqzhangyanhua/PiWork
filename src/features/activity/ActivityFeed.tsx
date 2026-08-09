import {
  Check,
  ChevronDown,
  CircleAlert,
  Clock3,
  LoaderCircle,
  RefreshCw,
  ShieldAlert,
} from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import type { PermissionOutcome } from "../../bindings";
import {
  buildActivityDisplayGroups,
  type ActivityDisplayBlock,
} from "./activityGrouping";
import type { ActivityItem, ToolStatus } from "./activityTypes";

export type ActivityFeedProps = {
  items: ActivityItem[];
  permissionRequestRole?: "alert" | "status";
};

type ThoughtOrPlan = Extract<ActivityItem, { type: "thought" | "plan" }>;
type ToolItem = Extract<ActivityItem, { type: "tool" }>;
type LifecycleItem = Extract<ActivityItem, { type: "lifecycle" }>;

const toolStatusKey: Record<ToolStatus, string> = {
  pending: "activity.toolPending",
  executing: "activity.toolRunning",
  completed: "activity.toolCompleted",
  failed: "activity.toolFailed",
};

const permissionOutcomeKey: Record<PermissionOutcome, string> = {
  allowed_once: "activity.permissionOutcomes.allowed_once",
  allowed_for_run: "activity.permissionOutcomes.allowed_for_run",
  denied: "activity.permissionOutcomes.denied",
  cancelled: "activity.permissionOutcomes.cancelled",
};

function Disclosure({
  children,
  className,
  collapseKey,
  expandKey,
}: {
  children: ReactNode;
  className: string;
  collapseKey: string;
  expandKey: string;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  return (
    <details
      className={className}
      onToggle={(event) => setOpen(event.currentTarget.open)}
      open={open}
    >
      <summary aria-expanded={open} role="button">
        <span>{t(open ? collapseKey : expandKey)}</span>
        <ChevronDown aria-hidden="true" />
      </summary>
      <div className="activity-feed__disclosure-body">{children}</div>
    </details>
  );
}

function ThoughtOrPlanRow({ item }: { item: ThoughtOrPlan }) {
  const isThought = item.type === "thought";
  return (
    <li
      className={`activity-feed__entry activity-feed__entry--${item.renderClass}`}
    >
      <Disclosure
        className={`activity-feed__disclosure activity-feed__disclosure--${item.renderClass}`}
        collapseKey={
          isThought ? "activity.collapseThought" : "activity.collapsePlan"
        }
        expandKey={
          isThought ? "activity.expandThought" : "activity.expandPlan"
        }
      >
        <p>{item.text}</p>
      </Disclosure>
    </li>
  );
}

function ToolStatusMarker({ status }: { status: ToolStatus }) {
  const { t } = useTranslation();
  const Icon =
    status === "completed"
      ? Check
      : status === "failed"
        ? CircleAlert
        : status === "executing"
          ? LoaderCircle
          : Clock3;

  return (
    <span className="activity-feed__status" data-status={status}>
      <Icon aria-hidden="true" />
      <span>{t(toolStatusKey[status])}</span>
    </span>
  );
}

function ToolRow({ item }: { item: ToolItem }) {
  const { t } = useTranslation();
  const object = item.descriptor.object ?? item.toolName;
  const preview = item.descriptor.preview;

  return (
    <li
      className={`activity-feed__entry activity-feed__entry--${item.renderClass}`}
    >
      <article
        className="activity-feed__tool"
        data-render-class={item.renderClass}
        data-tone={item.descriptor.tone}
      >
        <div className="activity-feed__tool-copy">
          <strong>
            <span className="activity-feed__action">
              {t(`activity.actions.${item.descriptor.action}`)}
            </span>{" "}
            <span className="activity-feed__object">{object}</span>
          </strong>
          {preview && preview !== object ? <small>{preview}</small> : null}
        </div>
        <ToolStatusMarker status={item.status} />
      </article>
    </li>
  );
}

function PermissionRow({
  item,
  requestRole,
}: {
  item: Extract<ActivityItem, { type: "permission" }>;
  requestRole: "alert" | "status";
}) {
  const { t } = useTranslation();
  const status =
    item.status === "requested"
      ? t("activity.permissionRequired")
      : item.outcome
        ? t(permissionOutcomeKey[item.outcome])
        : t("activity.permissionResolved");

  return (
    <li className="activity-feed__entry activity-feed__entry--permission">
      <article
        className="activity-feed__permission"
        data-status={item.status}
        role={item.status === "requested" ? requestRole : "status"}
      >
        <ShieldAlert aria-hidden="true" />
        <div>
          <span className="activity-feed__permission-status">{status}</span>
          <strong>{item.title}</strong>
          {item.detail ? <p>{item.detail}</p> : null}
        </div>
      </article>
    </li>
  );
}

function LifecycleRow({ item }: { item: LifecycleItem }) {
  const { t } = useTranslation();
  if (
    item.activityKind !== "waiting" &&
    item.activityKind !== "sessionChanged" &&
    item.activityKind !== "runFailed"
  ) {
    return null;
  }

  const isFailure = item.activityKind === "runFailed";
  const Icon = isFailure
    ? CircleAlert
    : item.activityKind === "sessionChanged"
      ? RefreshCw
      : Clock3;
  const label = t(`activity.lifecycle.${item.activityKind}`);
  let detail: string | null = null;
  if (item.activityKind === "sessionChanged") {
    const transition = t(
      `activity.lifecycle.sessionTransitions.${item.transition}`,
    );
    detail = item.reason
      ? t("activity.lifecycle.sessionWithReason", {
          reason: item.reason,
          transition,
        })
      : transition;
  } else if (!isFailure) {
    detail = item.detail;
  }

  return (
    <li
      className={`activity-feed__entry activity-feed__entry--${isFailure ? "error" : "status"}`}
    >
      <article
        className="activity-feed__lifecycle"
        data-kind={item.activityKind}
        role={isFailure ? undefined : "status"}
      >
        <Icon aria-hidden="true" />
        <div>
          <strong>{label}</strong>
          {detail ? <p>{detail}</p> : null}
        </div>
      </article>
    </li>
  );
}

export const isActivityFeedItem = (item: ActivityItem): boolean => {
  if (
    item.type === "message" ||
    item.type === "usage" ||
    item.type === "raw" ||
    item.renderClass === "suppressed"
  ) {
    return false;
  }
  return (
    item.type !== "lifecycle" ||
    item.activityKind === "waiting" ||
    item.activityKind === "sessionChanged" ||
    item.activityKind === "runFailed"
  );
};

function ActivityItemRow({
  item,
  permissionRequestRole = "alert",
}: {
  item: ActivityItem;
  permissionRequestRole?: "alert" | "status";
}) {
  if (!isActivityFeedItem(item)) return null;
  if (item.type === "thought" || item.type === "plan") {
    return <ThoughtOrPlanRow item={item} />;
  }
  if (item.type === "tool") return <ToolRow item={item} />;
  if (item.type === "permission") {
    return <PermissionRow item={item} requestRole={permissionRequestRole} />;
  }
  if (item.type === "lifecycle") return <LifecycleRow item={item} />;
  return null;
}

function ToolBurst({
  block,
}: {
  block: Extract<ActivityDisplayBlock, { kind: "toolBurst" }>;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const first = block.items[0];
  if (!first || first.type !== "tool") return null;
  const summary = t("activity.toolBurst", {
    action: t(`activity.actions.${first.descriptor.action}`),
    count: block.count,
  });

  return (
    <li
      className={`activity-feed__entry activity-feed__entry--${block.renderClass}`}
    >
      <details
        className="activity-feed__burst"
        onToggle={(event) => setOpen(event.currentTarget.open)}
        open={open}
      >
        <summary aria-expanded={open} role="button">
          <span>{summary}</span>
          <ChevronDown aria-hidden="true" />
        </summary>
        <ol className="activity-feed__burst-items">
          {block.items.map((item) => (
            <ActivityItemRow item={item} key={item.id} />
          ))}
        </ol>
      </details>
    </li>
  );
}

const visibleBlock = (block: ActivityDisplayBlock): boolean => {
  if (block.kind === "toolBurst") return true;
  return isActivityFeedItem(block.item);
};

export function ActivityFeed({
  items,
  permissionRequestRole = "alert",
}: ActivityFeedProps) {
  const { t } = useTranslation();
  const groups = buildActivityDisplayGroups(items);
  const visibleGroups = groups
    .map((group) => ({
      ...group,
      blocks: group.blocks.filter(visibleBlock),
    }))
    .filter((group) => group.blocks.length > 0);

  if (visibleGroups.length === 0) return null;

  return (
    <ol aria-label={t("activity.label")} className="activity-feed">
      {visibleGroups.flatMap((group) =>
        group.blocks.map((block) =>
          block.kind === "toolBurst" ? (
            <ToolBurst block={block} key={`${group.key}:${block.key}`} />
          ) : (
            <ActivityItemRow
              item={block.item}
              key={`${group.key}:${block.key}`}
              permissionRequestRole={permissionRequestRole}
            />
          ),
        ),
      )}
    </ol>
  );
}
