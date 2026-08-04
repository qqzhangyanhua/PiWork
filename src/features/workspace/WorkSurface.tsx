import { useEffect, useRef, useState, type PointerEvent } from "react";
import { useTranslation } from "react-i18next";

import { tauriClient, type ModelConfigurationSummary, type PiWorkClient } from "../../app/tauriClient";
import {
  pickAttachments as openAttachments,
  type PickAttachments,
} from "../../app/attachmentPicker";
import { pickProjectDirectory as openProjectDirectory, type PickProjectDirectory } from "../../app/projectDirectory";
import { appErrorMessageKey, formatAppErrorDiagnostics } from "../../domain/appError";
import { AnimatedSurfaceState } from "../../components/motion/AnimatedSurfaceState";
import { WorkSidebar, type WorkspaceView } from "../works/WorkSidebar";
import { AgentCenterPage } from "../agent-center/AgentCenterPage";
import { buildCapabilityPrompt, type AgentCapability } from "../agent-center/agentCapabilities";
import { useWorkEvents } from "../works/useWorkEvents";
import { WorkStoreProvider, useWorkStore } from "../works/WorkStoreProvider";
import { SettingsPage } from "../settings/SettingsPage";
import { type InspectorTab } from "./WorkInspector";
import {
  DEFAULT_INSPECTOR_PERCENT,
  clampInspectorPercent,
  persistInspectorPercent,
  readInspectorPercent,
} from "./inspectorLayout";
import { AllWorks } from "./AllWorks";
import { WorkHome } from "./WorkHome";
import { WorkDetail } from "./WorkDetail";
import {
  persistDetailExperience,
  readDetailExperience,
  type DetailExperience,
} from "./detailExperience";
import "../../styles/workspace.css";
import "../../styles/linear-fidelity.css";
import "../../styles/dashboard.css";
import "../../styles/agent-center.css";

function SurfaceContent({ client, initialView, modelConfiguration, modelLabel, onModelConfigured, pickProjectDirectory, pickAttachments }: { client: PiWorkClient; initialView: WorkspaceView; modelConfiguration: ModelConfigurationSummary | null; modelLabel: string; onModelConfigured?(configuration: ModelConfigurationSummary): void; pickProjectDirectory: PickProjectDirectory; pickAttachments: PickAttachments }) {
  const { t } = useTranslation();
  const hydrate = useWorkStore((state) => state.hydrate);
  const works = useWorkStore((state) => state.works);
  const selectedWorkId = useWorkStore((state) => state.selectedWorkId);
  const selectWork = useWorkStore((state) => state.selectWork);
  const timelines = useWorkStore((state) => state.timelines);
  const latestRuns = useWorkStore((state) => state.latestRuns);
  const resources = useWorkStore((state) => state.resources);
  const loading = useWorkStore((state) => state.loading);
  const error = useWorkStore((state) => state.error);
  const hydrationError = useWorkStore((state) => state.hydrationError);
  const [activeView, setActiveView] = useState<WorkspaceView>(initialView === "new" ? "home" : initialView);
  const [homeDraft, setHomeDraft] = useState<{ revision: number; rootPath?: string; prompt?: string }>({
    revision: 0,
  });
  const composerRef = useRef<HTMLDivElement>(null);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [detailExperience, setDetailExperience] = useState(readDetailExperience);
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>("delivery");
  const [inspectorPercent, setInspectorPercent] = useState(readInspectorPercent);
  const workspaceRef = useRef<HTMLElement>(null);
  const inspectorToggleRef = useRef<HTMLButtonElement>(null);
  const newWorkTriggerRef = useRef<HTMLButtonElement>(null);
  const settingsTriggerRef = useRef<HTMLButtonElement>(null);
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const [hydrated, setHydrated] = useState(false);
  useWorkEvents();
  useEffect(() => {
    let active = true;
    void hydrate().finally(() => {
      if (active) setHydrated(true);
    });
    return () => { active = false; };
  }, [hydrate]);
  const selectedWork = selectedWorkId ? works[selectedWorkId] : undefined;
  const timeline = selectedWork ? timelines[selectedWork.id] ?? [] : [];
  const selectedResources = selectedWork ? resources[selectedWork.id] ?? [] : [];
  const pageError = hydrationError;
  useEffect(() => setDiagnosticsOpen(false), [pageError]);
  useEffect(() => {
    if (activeView === "detail" && selectedWorkId) {
      queueMicrotask(() => composerRef.current?.focus());
      return;
    }
    if (activeView === "home") {
      queueMicrotask(() => document.getElementById("first-work-prompt")?.focus());
    }
  }, [activeView, selectedWorkId]);
  const closeInspector = () => {
    setInspectorOpen(false);
    queueMicrotask(() => inspectorToggleRef.current?.focus());
  };
  const resizeInspector = (event: PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const workspace = workspaceRef.current;
    if (!workspace) return;
    const update = (clientX: number) => {
      const bounds = workspace.getBoundingClientRect();
      if (!bounds.width) return;
      const next = clampInspectorPercent(((bounds.right - clientX) / bounds.width) * 100);
      setInspectorPercent(next);
    };
    const move = (moveEvent: globalThis.PointerEvent) => update(moveEvent.clientX);
    const finish = (upEvent: globalThis.PointerEvent) => {
      update(upEvent.clientX);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      setInspectorPercent((current) => persistInspectorPercent(current));
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish);
  };
  const resetInspector = () => {
    setInspectorPercent(persistInspectorPercent(DEFAULT_INSPECTOR_PERCENT));
  };
  const openDiagnostics = () => {
    setInspectorTab("logs");
    setInspectorOpen(true);
  };
  const changeDetailExperience = (value: DetailExperience) => {
    setDetailExperience(persistDetailExperience(value));
  };
  const openWork = (workId: string) => {
    selectWork(workId);
    setActiveView("detail");
  };
  const openHomeDraft = (rootPath?: string) => {
    setHomeDraft(({ revision }) => ({ revision: revision + 1, rootPath, prompt: undefined }));
    setActiveView("home");
  };
  const startCapability = (capability: AgentCapability) => {
    setHomeDraft(({ revision, rootPath }) => ({
      revision: revision + 1,
      rootPath: selectedWork?.rootPath ?? rootPath,
      prompt: buildCapabilityPrompt(capability),
    }));
    setActiveView("home");
  };
  const defaultRootPath = selectedWork?.rootPath ?? Object.values(works)[0]?.rootPath;
  useEffect(() => {
    const openSettings = (event: KeyboardEvent) => {
      if (event.isComposing || !(event.ctrlKey || event.metaKey) || event.key !== ",") return;
      event.preventDefault();
      setActiveView("settings");
    };
    window.addEventListener("keydown", openSettings);
    return () => window.removeEventListener("keydown", openSettings);
  }, []);
  useEffect(() => {
    const openNewConversation = (event: KeyboardEvent) => {
      if (
        event.isComposing ||
        !(event.ctrlKey || event.metaKey) ||
        event.key.toLocaleLowerCase() !== "n"
      ) return;
      event.preventDefault();
      openHomeDraft(defaultRootPath);
    };
    window.addEventListener("keydown", openNewConversation);
    return () => window.removeEventListener("keydown", openNewConversation);
  }, [defaultRootPath]);
  return (
    <main className="work-surface">
      <WorkSidebar
        activeView={activeView}
        newWorkTriggerRef={newWorkTriggerRef}
        onAgentsRequest={() => setActiveView("agents")}
        onCreateRequest={openHomeDraft}
        onSettingsRequest={() => setActiveView("settings")}
        onWorkSelected={() => {
          setActiveView("detail");
        }}
        settingsTriggerRef={settingsTriggerRef}
      />
      {!hydrated && loading ? (
        <AnimatedSurfaceState
          aria-label={t("state.loading")}
          className="surface-state surface-state--loading"
          role="status"
          variant="loading"
        >
          <div className="loading-line" data-motion-line />
          <div className="loading-line loading-line--short" data-motion-line />
        </AnimatedSurfaceState>
      ) : pageError ? (
        <AnimatedSurfaceState
          className="surface-state"
          role="alert"
          variant="error"
        >
          <h1>{t("state.errorTitle")}</h1><p>{t(appErrorMessageKey(pageError))}</p>
          <div className="surface-state__actions"><button className="button button--primary" type="button" onClick={() => void hydrate()}>{t("common.retry")}</button><button className="button" type="button" onClick={() => setDiagnosticsOpen((open) => !open)}>{t("diagnostics.open")}</button></div>
          {diagnosticsOpen && <pre className="diagnostics">{formatAppErrorDiagnostics(pageError, t("diagnostics.unavailable"))}</pre>}
        </AnimatedSurfaceState>
      ) : activeView === "agents" ? (
        <AgentCenterPage onStartCapability={startCapability} />
      ) : activeView === "settings" ? (
        <SettingsPage
          client={client}
          configuration={modelConfiguration}
          onModelConfigured={(configuration) => onModelConfigured?.(configuration)}
        />
      ) : activeView === "all" ? (
        <AllWorks works={Object.values(works)} onWorkSelected={(work) => openWork(work.id)} />
      ) : activeView === "home" || !selectedWork ? (
        <WorkHome
          draftRevision={homeDraft.revision}
          initialPrompt={homeDraft.prompt}
          initialRootPath={homeDraft.rootPath}
          modelLabel={modelLabel}
          onAgentsRequest={() => setActiveView("agents")}
          onAllWorks={() => setActiveView("all")}
          onStarted={() => setActiveView("detail")}
          onWorkSelected={(work) => openWork(work.id)}
          pickAttachments={pickAttachments}
          pickProjectDirectory={pickProjectDirectory}
          works={Object.values(works)}
        />
      ) : (
        <WorkDetail
          composerRef={composerRef}
          error={error}
          experience={detailExperience}
          inspectorOpen={inspectorOpen}
          inspectorPercent={inspectorPercent}
          inspectorTab={inspectorTab}
          inspectorToggleRef={inspectorToggleRef}
          modelLabel={latestRuns[selectedWork.id]?.modelLabel ?? modelLabel}
          pickAttachments={pickAttachments}
          resources={selectedResources}
          timeline={timeline}
          work={selectedWork}
          workspaceRef={workspaceRef}
          onExperienceChange={changeDetailExperience}
          onInspectorClose={closeInspector}
          onInspectorOpenDiagnostics={openDiagnostics}
          onInspectorResizeReset={resetInspector}
          onInspectorResizeStart={resizeInspector}
          onInspectorTabChange={setInspectorTab}
          onInspectorToggle={() => inspectorOpen ? closeInspector() : setInspectorOpen(true)}
        />
      )}
    </main>
  );
}

export function WorkSurface({ client, initialView = "detail", modelConfiguration = null, modelLabel = "Pi", onModelConfigured, pickProjectDirectory = openProjectDirectory, pickAttachments = openAttachments }: { client?: PiWorkClient; initialView?: WorkspaceView; modelConfiguration?: ModelConfigurationSummary | null; modelLabel?: string; onModelConfigured?(configuration: ModelConfigurationSummary): void; pickProjectDirectory?: PickProjectDirectory; pickAttachments?: PickAttachments }) {
  const resolvedClient = client ?? tauriClient;
  return <WorkStoreProvider client={resolvedClient}><SurfaceContent client={resolvedClient} initialView={initialView} modelConfiguration={modelConfiguration} modelLabel={modelLabel} onModelConfigured={onModelConfigured} pickProjectDirectory={pickProjectDirectory} pickAttachments={pickAttachments} /></WorkStoreProvider>;
}
