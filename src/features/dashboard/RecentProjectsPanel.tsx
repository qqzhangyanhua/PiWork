import { ArrowRight, FolderClosed } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ProjectGroup } from "../workspace/WorkList";
import { formatRelativeTime } from "../workspace/relativeTime";

export function RecentProjectsPanel({
  groups,
  onSelectProject,
  onViewAll,
}: {
  groups: ProjectGroup[];
  onSelectProject(rootPath: string): void;
  onViewAll(): void;
}) {
  const { i18n, t } = useTranslation();
  const recent = groups.slice(0, 4);

  return (
    <section aria-labelledby="recent-projects-heading" className="dashboard-panel dashboard-panel--recent-projects">
      <header className="dashboard-panel__header">
        <h2 id="recent-projects-heading">{t("dashboard.recentProjects.title")}</h2>
        {recent.length > 0 && (
          <button className="text-button" onClick={onViewAll} type="button">
            {t("workspace.viewAll")}
            <ArrowRight aria-hidden="true" size={13} />
          </button>
        )}
      </header>
      {recent.length > 0 ? (
        <ul className="dashboard-project-list">
          {recent.map((group) => {
            const relativeTime = formatRelativeTime(group.updatedAt, i18n.language);
            return (
              <li key={group.rootPath}>
                <button
                  aria-label={t("dashboard.recentProjects.itemLabel", { project: group.name, time: relativeTime })}
                  className="dashboard-project-item"
                  onClick={() => onSelectProject(group.rootPath)}
                  title={group.rootPath}
                  type="button"
                >
                  <span className="dashboard-project-item__icon" aria-hidden="true"><FolderClosed size={16} /></span>
                  <span className="dashboard-project-item__body">
                    <strong>{group.name}</strong>
                    <small title={group.rootPath}>{group.rootPath}</small>
                  </span>
                  <time className="dashboard-project-item__time" dateTime={group.updatedAt}>{relativeTime}</time>
                </button>
              </li>
            );
          })}
        </ul>
      ) : (
        <p className="dashboard-panel__empty">{t("dashboard.recentProjects.empty")}</p>
      )}
    </section>
  );
}
