import {
  Bot,
  Blocks,
  Cable,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  Clock3,
  Folder,
  LoaderCircle,
  MessageSquare,
  Plus,
  Settings,
  SquarePen,
} from "lucide-react";
import { useMemo, useState, type RefObject } from "react";
import { useTranslation } from "react-i18next";

import type { WorkSummary } from "../../bindings";
import { CoDoLogo } from "../../components/brand/CoDoLogo";
import { AccountMenu } from "../settings/AccountMenu";
import { projectGroups } from "../workspace/WorkList";
import { useWorkStore } from "./WorkStoreProvider";

export type WorkspaceView =
  | "home"
  | "new"
  | "all"
  | "detail"
  | "agents"
  | "plugins"
  | "connectors"
  | "settings";

type WorkSidebarProps = {
  activeView: WorkspaceView;
  onAgentsRequest(): void;
  onConnectorsRequest(): void;
  onCreateRequest(rootPath?: string): void;
  onPluginsRequest(): void;
  onSettingsRequest(): void;
  onWorkSelected(): void;
  newWorkTriggerRef: RefObject<HTMLButtonElement | null>;
  settingsTriggerRef: RefObject<HTMLButtonElement | null>;
};

function ConversationIcon({ status }: { status: WorkSummary["status"] }) {
  if (status === "running" || status === "queued") {
    return <LoaderCircle aria-hidden="true" className="project-conversation__spinner" size={14} />;
  }
  if (status === "waiting") {
    return <Clock3 aria-hidden="true" className="project-conversation__waiting" size={14} />;
  }
  if (status === "failed" || status === "interrupted" || status === "stopped") {
    return <CircleAlert aria-hidden="true" size={14} />;
  }
  return <MessageSquare aria-hidden="true" size={14} />;
}

export function WorkSidebar({
  activeView,
  onAgentsRequest,
  onConnectorsRequest,
  onCreateRequest,
  onPluginsRequest,
  onSettingsRequest,
  onWorkSelected,
  newWorkTriggerRef,
  settingsTriggerRef,
}: WorkSidebarProps) {
  const { t } = useTranslation();
  const works = useWorkStore((state) => state.works);
  const selectedWorkId = useWorkStore((state) => state.selectedWorkId);
  const selectWork = useWorkStore((state) => state.selectWork);
  const groups = useMemo(() => projectGroups(Object.values(works)), [works]);
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const selectedRootPath = selectedWorkId ? works[selectedWorkId]?.rootPath : undefined;
  const defaultRootPath = selectedRootPath ?? groups[0]?.rootPath;

  const toggleProject = (rootPath: string) => {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(rootPath)) next.delete(rootPath);
      else next.add(rootPath);
      return next;
    });
  };

  return (
    <aside className="work-sidebar" aria-label={t("sidebar.label")}>
      <div className="work-sidebar__topbar">
        <div className="work-sidebar__brand">
          <span className="work-sidebar__brand-mark"><CoDoLogo showWordmark size={28} /></span>
        </div>
      </div>

      <button
        aria-current={activeView === "home" ? "page" : undefined}
        aria-label={t("conversation.new")}
        className="work-sidebar__new-conversation"
        onClick={() => onCreateRequest(defaultRootPath)}
        ref={newWorkTriggerRef}
        type="button"
      >
        <SquarePen aria-hidden="true" size={16} />
        <span>{t("conversation.new")}</span>
        <kbd aria-hidden="true" className="work-sidebar__shortcut">Ctrl N</kbd>
      </button>

      <nav className="work-sidebar__primary-nav" aria-label={t("sidebar.primary")}>
        <button
          aria-current={activeView === "agents" ? "page" : undefined}
          className="work-sidebar__nav-item"
          onClick={onAgentsRequest}
          type="button"
        >
          <Bot aria-hidden="true" size={16} />
          <span>{t("sidebar.nav.agents")}</span>
        </button>
        <button
          aria-current={activeView === "plugins" ? "page" : undefined}
          className="work-sidebar__nav-item"
          onClick={onPluginsRequest}
          type="button"
        >
          <Blocks aria-hidden="true" size={16} />
          <span>{t("sidebar.nav.plugins")}</span>
        </button>
        <button
          aria-current={activeView === "connectors" ? "page" : undefined}
          className="work-sidebar__nav-item"
          onClick={onConnectorsRequest}
          type="button"
        >
          <Cable aria-hidden="true" size={16} />
          <span>{t("sidebar.nav.connectors")}</span>
        </button>
        <button
          aria-current={activeView === "settings" ? "page" : undefined}
          className="work-sidebar__nav-item"
          onClick={onSettingsRequest}
          type="button"
        >
          <Settings aria-hidden="true" size={16} />
          <span>{t("sidebar.nav.settings")}</span>
        </button>
      </nav>

      <div className="work-sidebar__section-header">
        <p className="work-sidebar__section-label">{t("project.section")}</p>
      </div>
      <nav className="project-groups" aria-label={t("sidebar.projects")}>
        {groups.map((group) => {
          const isCollapsed = collapsed.has(group.rootPath);
          return (
            <section className="project-group" key={group.rootPath}>
              <div className="project-group__header">
                <button
                  aria-expanded={!isCollapsed}
                  aria-label={t("project.groupLabel", { name: group.name })}
                  className="project-group__toggle"
                  onClick={() => toggleProject(group.rootPath)}
                  title={group.rootPath}
                  type="button"
                >
                  {isCollapsed ? <ChevronRight aria-hidden="true" size={13} /> : <ChevronDown aria-hidden="true" size={13} />}
                  <Folder aria-hidden="true" size={15} />
                  <span>{group.name}</span>
                </button>
                <button
                  aria-label={t("conversation.newInProject", { project: group.name })}
                  className="project-group__new"
                  onClick={() => onCreateRequest(group.rootPath)}
                  title={t("conversation.newInProject", { project: group.name })}
                  type="button"
                >
                  <Plus aria-hidden="true" size={14} />
                </button>
              </div>
              {!isCollapsed && (
                <div className="project-group__conversations">
                  {group.conversations.map((work) => (
                    <button
                      aria-current={activeView === "detail" && selectedWorkId === work.id ? "page" : undefined}
                      aria-label={`${work.title}, ${t(`status.${work.status}`)}`}
                      className="project-conversation"
                      key={work.id}
                      onClick={() => {
                        selectWork(work.id);
                        onWorkSelected();
                      }}
                      title={work.title}
                      type="button"
                    >
                      <ConversationIcon status={work.status} />
                      <span>{work.title}</span>
                    </button>
                  ))}
                </div>
              )}
            </section>
          );
        })}
      </nav>

      <AccountMenu
        active={activeView === "settings"}
        onSettingsRequest={onSettingsRequest}
        triggerRef={settingsTriggerRef}
      />
    </aside>
  );
}
