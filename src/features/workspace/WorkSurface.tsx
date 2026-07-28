import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import type { PiWorkClient } from "../../app/tauriClient";
import { WorkSidebar } from "../works/WorkSidebar";
import { useWorkEvents } from "../works/useWorkEvents";
import { WorkStoreProvider, useWorkStore } from "../works/WorkStoreProvider";
import { WorkComposer } from "./WorkComposer";
import { WorkHeader } from "./WorkHeader";
import { WorkInspector } from "./WorkInspector";
import { WorkTimeline } from "./WorkTimeline";
import "../../styles/workspace.css";

const formatDiagnostics = (
  error: { code: string; message: string; details?: Record<string, unknown> },
  fallback: string,
) => {
  if (!error.details) return `${error.code}\n${error.message}`;
  try {
    return `${error.code}\n${error.message}\n${JSON.stringify(error.details, null, 2)}`;
  } catch {
    return `${error.code}\n${error.message}\n${fallback}`;
  }
};

function SurfaceContent() {
  const { t } = useTranslation();
  const hydrate = useWorkStore((state) => state.hydrate);
  const works = useWorkStore((state) => state.works);
  const selectedWorkId = useWorkStore((state) => state.selectedWorkId);
  const timelines = useWorkStore((state) => state.timelines);
  const latestRuns = useWorkStore((state) => state.latestRuns);
  const loading = useWorkStore((state) => state.loading);
  const error = useWorkStore((state) => state.error);
  const [createOpen, setCreateOpen] = useState(false);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  useWorkEvents();
  useEffect(() => { void hydrate(); }, [hydrate]);

  const selectedWork = selectedWorkId ? works[selectedWorkId] : undefined;
  const timeline = selectedWork ? timelines[selectedWork.id] ?? [] : [];
  const hasWorks = Object.keys(works).length > 0;

  return (
    <main className="work-surface">
      <WorkSidebar createOpen={createOpen} onCreateOpenChange={setCreateOpen} />
      {!hasWorks && loading ? (
        <section className="surface-state surface-state--loading" role="status" aria-label={t("state.loading")}><div className="loading-line" /><div className="loading-line loading-line--short" /></section>
      ) : !hasWorks && error ? (
        <section className="surface-state" role="alert">
          <h1>{t("state.errorTitle")}</h1><p>{error.message}</p>
          <div className="surface-state__actions"><button className="button button--primary" type="button" onClick={() => void hydrate()}>{t("common.retry")}</button><button className="button" type="button" onClick={() => setDiagnosticsOpen((open) => !open)}>{t("diagnostics.open")}</button></div>
          {diagnosticsOpen && <pre className="diagnostics">{formatDiagnostics(error, t("diagnostics.unavailable"))}</pre>}
        </section>
      ) : !selectedWork ? (
        <section className="surface-state surface-state--empty"><h1>{t("work.empty")}</h1><p>{t("state.emptyBody")}</p><button className="button button--primary" type="button" onClick={() => setCreateOpen(true)}>{t("work.new")}</button></section>
      ) : (
        <section className="workspace-main">
          <WorkHeader work={selectedWork} timeline={timeline} error={error} latestRun={latestRuns[selectedWork.id]} inspectorOpen={inspectorOpen} onInspectorToggle={() => setInspectorOpen((open) => !open)} />
          <div className="workspace-center"><WorkTimeline timeline={timeline} /><WorkComposer work={selectedWork} /></div>
          <WorkInspector timeline={timeline} open={inspectorOpen} />
          {inspectorOpen && <button className="inspector-scrim" aria-label={t("inspector.close")} type="button" onClick={() => setInspectorOpen(false)} />}
        </section>
      )}
    </main>
  );
}

export function WorkSurface({ client }: { client?: PiWorkClient }) {
  return <WorkStoreProvider client={client}><SurfaceContent /></WorkStoreProvider>;
}
