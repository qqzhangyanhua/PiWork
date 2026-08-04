import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import type { WorkSummary } from "../../bindings";
import type { PickAttachments } from "../../app/attachmentPicker";
import type { PickProjectDirectory } from "../../app/projectDirectory";
import { AgentActivityPanel } from "../dashboard/AgentActivityPanel";
import { AgentSkillsPanel } from "../dashboard/AgentSkillsPanel";
import { DailySummaryPanel } from "../dashboard/DailySummaryPanel";
import { DashboardGreeting } from "../dashboard/DashboardGreeting";
import { DashboardHeader } from "../dashboard/DashboardHeader";
import { EnvironmentStatusPanel } from "../dashboard/EnvironmentStatusPanel";
import { RecentProjectsPanel } from "../dashboard/RecentProjectsPanel";
import { useWorkStoreContext } from "../works/WorkStoreProvider";
import { NewWorkStart } from "./NewWorkStart";
import { projectGroups } from "./WorkList";

export function WorkHome({
  draftRevision,
  initialPrompt,
  initialRootPath,
  modelLabel,
  works,
  pickProjectDirectory,
  pickAttachments,
  onStarted,
  onAgentsRequest,
  onWorkSelected,
  onAllWorks,
}: {
  draftRevision?: number;
  initialPrompt?: string;
  initialRootPath?: string;
  modelLabel: string;
  works: WorkSummary[];
  pickProjectDirectory: PickProjectDirectory;
  pickAttachments: PickAttachments;
  onStarted(): void;
  onAgentsRequest(): void;
  onWorkSelected(work: WorkSummary): void;
  onAllWorks(): void;
}) {
  const { t } = useTranslation();
  const { client } = useWorkStoreContext();
  const [dashboardRootPath, setDashboardRootPath] = useState<string | undefined>(initialRootPath);
  const groups = useMemo(() => projectGroups(works), [works]);

  useEffect(() => {
    setDashboardRootPath(initialRootPath);
  }, [draftRevision, initialRootPath]);

  const handleImportProject = async () => {
    const selected = await pickProjectDirectory();
    if (selected) setDashboardRootPath(selected);
  };

  const handleSelectProject = (rootPath: string) => {
    const group = groups.find((item) => item.rootPath === rootPath);
    const work = group?.conversations[0];
    if (work) onWorkSelected(work);
  };

  return (
    <div aria-label={t("workspace.home")} className="work-home" role="region">
      <div className="work-home__main">
        <DashboardHeader onAgentsRequest={onAgentsRequest} onImportProject={() => void handleImportProject()} />
        <div className="work-home__scroll">
          <div className="work-home__dashboard-content">
            <DashboardGreeting />
            <NewWorkStart
              dashboardExtras={
                <div className="dashboard-panels-row" data-testid="dashboard-panels-row">
                  <RecentProjectsPanel groups={groups} onSelectProject={handleSelectProject} onViewAll={onAllWorks} />
                  <AgentSkillsPanel onAgentsRequest={onAgentsRequest} />
                  <EnvironmentStatusPanel client={client} />
                </div>
              }
              initialPrompt={initialPrompt}
              initialRootPath={dashboardRootPath}
              key={draftRevision}
              modelLabel={modelLabel}
              onStarted={onStarted}
              pickAttachments={pickAttachments}
              pickProjectDirectory={pickProjectDirectory}
              variant="dashboard"
              works={works}
            />
          </div>
        </div>
      </div>
      <aside aria-label={t("dashboard.activity.title")} className="work-home__activity-rail">
        <AgentActivityPanel />
        <DailySummaryPanel />
      </aside>
    </div>
  );
}
