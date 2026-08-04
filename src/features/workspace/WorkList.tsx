import {
  CircleAlert,
  CircleCheck,
  CircleDashed,
  Clock3,
  LoaderCircle,
} from "lucide-react";
import { useTranslation } from "react-i18next";

import type { WorkSummary } from "../../bindings";
import { formatRelativeTime } from "./relativeTime";

export const visibleWorks = (works: WorkSummary[]) =>
  works
    .filter((work) => work.status !== "archived")
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));

export const projectName = (rootPath: string) =>
  rootPath.split(/[\\/]/).filter(Boolean).at(-1) ?? rootPath;

export type ProjectGroup = {
  rootPath: string;
  name: string;
  conversations: WorkSummary[];
  updatedAt: string;
};

export const projectGroups = (works: WorkSummary[]): ProjectGroup[] => {
  const groups = new Map<string, WorkSummary[]>();
  for (const work of visibleWorks(works)) {
    groups.set(work.rootPath, [...(groups.get(work.rootPath) ?? []), work]);
  }
  return [...groups.entries()]
    .map(([rootPath, conversations]) => ({
      rootPath,
      name: projectName(rootPath),
      conversations,
      updatedAt: conversations[0]?.updatedAt ?? "",
    }))
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
};

export const workGroup = (status: WorkSummary["status"]) => {
  if (status === "running" || status === "queued" || status === "waiting") return "active";
  if (status === "failed" || status === "interrupted" || status === "stopped") return "attention";
  if (status === "completed") return "completed";
  return "open";
};

function WorkStatusIcon({ status }: { status: WorkSummary["status"] }) {
  if (status === "running" || status === "queued") return <LoaderCircle aria-hidden="true" />;
  if (status === "completed" || status === "idle") return <CircleCheck aria-hidden="true" />;
  if (status === "failed" || status === "interrupted" || status === "stopped") return <CircleAlert aria-hidden="true" />;
  if (status === "waiting") return <Clock3 aria-hidden="true" />;
  return <CircleDashed aria-hidden="true" />;
}

export function WorkListRow({
  work,
  compact = false,
  selected = false,
  onSelect,
}: {
  work: WorkSummary;
  compact?: boolean;
  selected?: boolean;
  onSelect(work: WorkSummary): void;
}) {
  const { i18n, t } = useTranslation();
  const relative = formatRelativeTime(work.updatedAt, i18n.language);
  const statusLabel = t(`status.${work.status}`);
  const project = projectName(work.rootPath);

  return (
    <button
      aria-current={selected ? "page" : undefined}
      aria-label={compact ? work.title : `${work.title}, ${statusLabel}, ${project}`}
      className={`work-list-row${compact ? " work-list-row--compact work-sidebar__item" : ""}`}
      onClick={() => onSelect(work)}
      title={work.rootPath}
      type="button"
    >
      <span className={`work-list-row__status work-list-row__status--${work.status}`}>
        <WorkStatusIcon status={work.status} />
        <span className={compact ? "work-sidebar__status" : undefined}>{statusLabel}</span>
      </span>
      <span className={`work-list-row__title${compact ? " work-sidebar__title" : ""}`}>{work.title}</span>
      <span className={`work-list-row__project${compact ? " work-sidebar__project" : ""}`}>{project}</span>
      <time className={`work-list-row__time${compact ? " work-sidebar__time" : ""}`} dateTime={work.updatedAt}>{relative}</time>
    </button>
  );
}
