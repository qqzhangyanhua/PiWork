import { useEffect, useRef, useState, type CSSProperties, type PointerEvent } from "react";
import { useTranslation } from "react-i18next";

import type { PiWorkClient } from "../../app/tauriClient";
import {
  pickAttachments as openAttachments,
  type PickAttachments,
} from "../../app/attachmentPicker";
import { pickProjectDirectory as openProjectDirectory, type PickProjectDirectory } from "../../app/projectDirectory";
import { appErrorMessageKey, formatAppErrorDiagnostics } from "../../domain/appError";
import { WorkSidebar } from "../works/WorkSidebar";
import { useWorkEvents } from "../works/useWorkEvents";
import { WorkStoreProvider, useWorkStore } from "../works/WorkStoreProvider";
import { WorkComposer } from "./WorkComposer";
import { WorkHeader } from "./WorkHeader";
import { WorkInspector, type InspectorTab } from "./WorkInspector";
import { NewWorkStart } from "./NewWorkStart";
import {
  DEFAULT_INSPECTOR_PERCENT,
  clampInspectorPercent,
  persistInspectorPercent,
  readInspectorPercent,
} from "./inspectorLayout";
import { WorkTimeline } from "./WorkTimeline";
import "../../styles/workspace.css";

function SurfaceContent({ modelLabel, pickProjectDirectory, pickAttachments }: { modelLabel: string; pickProjectDirectory: PickProjectDirectory; pickAttachments: PickAttachments }) {
  const { t } = useTranslation();
  const hydrate = useWorkStore((state) => state.hydrate);
  const works = useWorkStore((state) => state.works);
  const selectedWorkId = useWorkStore((state) => state.selectedWorkId);
  const timelines = useWorkStore((state) => state.timelines);
  const latestRuns = useWorkStore((state) => state.latestRuns);
  const resources = useWorkStore((state) => state.resources);
  const loading = useWorkStore((state) => state.loading);
  const error = useWorkStore((state) => state.error);
  const hydrationError = useWorkStore((state) => state.hydrationError);
  const [creating, setCreating] = useState(false);
  const composerRef = useRef<HTMLDivElement>(null);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>("preview");
  const [inspectorPercent, setInspectorPercent] = useState(readInspectorPercent);
  const workspaceRef = useRef<HTMLElement>(null);
  const inspectorToggleRef = useRef<HTMLButtonElement>(null);
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
  const hasWorks = Object.keys(works).length > 0;
  const pageError = hydrationError;
  useEffect(() => setDiagnosticsOpen(false), [pageError]);
  useEffect(() => {
    if (creating || !selectedWorkId) return;
    queueMicrotask(() => composerRef.current?.focus());
  }, [creating, selectedWorkId]);
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
  return (
    <main className="work-surface">
      <WorkSidebar
        creating={creating || !selectedWork}
        onCreateRequest={() => setCreating(true)}
        onWorkSelected={() => setCreating(false)}
      />
      {!hydrated && loading ? (
        <section className="surface-state surface-state--loading" role="status" aria-label={t("state.loading")}><div className="loading-line" /><div className="loading-line loading-line--short" /></section>
      ) : pageError ? (
        <section className="surface-state" role="alert">
          <h1>{t("state.errorTitle")}</h1><p>{t(appErrorMessageKey(pageError))}</p>
          <div className="surface-state__actions"><button className="button button--primary" type="button" onClick={() => void hydrate()}>{t("common.retry")}</button><button className="button" type="button" onClick={() => setDiagnosticsOpen((open) => !open)}>{t("diagnostics.open")}</button></div>
          {diagnosticsOpen && <pre className="diagnostics">{formatAppErrorDiagnostics(pageError, t("diagnostics.unavailable"))}</pre>}
        </section>
      ) : creating || !selectedWork ? (
        <NewWorkStart modelLabel={modelLabel} works={Object.values(works)} onStarted={() => setCreating(false)} pickProjectDirectory={pickProjectDirectory} pickAttachments={pickAttachments} />
      ) : (
        <section
          className="workspace-main"
          data-inspector-open={inspectorOpen}
          ref={workspaceRef}
          style={{ "--inspector-width": `${inspectorPercent}%` } as CSSProperties}
        >
          <WorkHeader work={selectedWork} inspectorOpen={inspectorOpen} inspectorToggleRef={inspectorToggleRef} onInspectorToggle={() => inspectorOpen ? closeInspector() : setInspectorOpen(true)} />
          <div className="workspace-center"><WorkTimeline timeline={timeline} resources={selectedResources} error={error} onOpenDiagnostics={openDiagnostics} /><WorkComposer modelLabel={latestRuns[selectedWork.id]?.modelLabel ?? modelLabel} pickAttachments={pickAttachments} promptRef={composerRef} resources={selectedResources} work={selectedWork} /></div>
          <WorkInspector
            active={inspectorTab}
            error={error}
            resources={selectedResources}
            timeline={timeline}
            open={inspectorOpen}
            widthPercent={inspectorPercent}
            onClose={closeInspector}
            onActiveChange={setInspectorTab}
            onResizeReset={resetInspector}
            onResizeStart={resizeInspector}
          />
          {inspectorOpen && <button className="inspector-scrim" aria-label={t("inspector.close")} type="button" onClick={closeInspector} />}
        </section>
      )}
    </main>
  );
}

export function WorkSurface({ client, modelLabel = "Pi", pickProjectDirectory = openProjectDirectory, pickAttachments = openAttachments }: { client?: PiWorkClient; modelLabel?: string; pickProjectDirectory?: PickProjectDirectory; pickAttachments?: PickAttachments }) {
  return <WorkStoreProvider client={client}><SurfaceContent modelLabel={modelLabel} pickProjectDirectory={pickProjectDirectory} pickAttachments={pickAttachments} /></WorkStoreProvider>;
}
