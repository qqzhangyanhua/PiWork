import { ArrowUp } from "lucide-react";
import { type Ref, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { ResourceSummary, WorkSummary } from "../../bindings";
import type { PickAttachments } from "../../app/attachmentPicker";
import { didPersistStartInstruction } from "../works/workStore";
import { useWorkStore } from "../works/WorkStoreProvider";
import { ComposerModelIndicator } from "./ComposerModelIndicator";
import { AttachmentButton } from "./AttachmentButton";
import { AttachmentDraftList } from "./AttachmentDraftList";
import { ProjectPromptEditor } from "./ProjectPromptEditor";

const queueStatuses: WorkSummary["status"][] = ["queued", "running", "waiting"];
const continueStatuses: WorkSummary["status"][] = [
  "completed", "failed", "stopped", "interrupted", "idle",
];

type WorkComposerProps = {
  modelLabel: string;
  pickAttachments: PickAttachments;
  promptRef?: Ref<HTMLDivElement>;
  resources: ResourceSummary[];
  work: WorkSummary;
};

export function WorkComposer({
  modelLabel,
  pickAttachments,
  promptRef,
  resources,
  work,
}: WorkComposerProps) {
  const { t } = useTranslation();
  const [prompt, setPrompt] = useState("");
  const [referencedFiles, setReferencedFiles] = useState<string[]>([]);
  const [attachmentResults, setAttachmentResults] = useState<ResourceSummary[]>([]);
  const [selectedResourceIds, setSelectedResourceIds] = useState<string[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const submittingRef = useRef(false);
  const startWork = useWorkStore((state) => state.startWork);
  const loading = useWorkStore((state) => state.loading);
  const runActive = queueStatuses.includes(work.status);
  const actionLabel = continueStatuses.includes(work.status)
      ? t("composer.continue")
      : t("composer.send");
  const availableResources = [...resources, ...attachmentResults].filter(
    (resource, index, all) => all.findIndex(({ id }) => id === resource.id) === index,
  );
  const readyResourceIds = selectedResourceIds.filter((id) =>
    availableResources.some(
      (resource) => resource.id === id && resource.status === "ready",
    ),
  );
  const canSubmit = Boolean(prompt.trim()) || readyResourceIds.length > 0;

  const clearDraft = () => {
    setPrompt("");
    setReferencedFiles([]);
    setAttachmentResults([]);
    setSelectedResourceIds([]);
  };

  const submit = async () => {
    const instruction = prompt.trim();
    if (!instruction && readyResourceIds.length === 0) return;
    if (submittingRef.current) return;
    submittingRef.current = true;
    setSubmitting(true);
    try {
      await startWork(work.id, instruction, referencedFiles, readyResourceIds);
      clearDraft();
    } catch (error) {
      if (didPersistStartInstruction(error)) {
        clearDraft();
      }
      // The store normalizes and exposes the error in the product UI.
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  return (
    <footer className="work-composer">
      {runActive && <p className="composer-status" role="status">{t("composer.runActive")}</p>}
      <div className="work-composer__box">
        <AttachmentDraftList
          resources={availableResources}
          selectedIds={selectedResourceIds}
          onRemove={(resource) => {
            setSelectedResourceIds((current) => current.filter((id) => id !== resource.id));
            setAttachmentResults((current) => current.filter(({ id }) => id !== resource.id));
          }}
        />
        <ProjectPromptEditor
          editorRef={promptRef}
          id="work-prompt"
          label={t("composer.label")}
          placeholder={t("composer.placeholder")}
          rootPath={work.rootPath}
          value={prompt}
          onDraftChange={(nextPrompt, nextReferences) => {
            setPrompt(nextPrompt);
            setReferencedFiles(nextReferences);
          }}
          onSubmit={() => void submit()}
        />
        <div className="work-composer__actions">
          <AttachmentButton
            available={availableResources}
            disabled={submitting}
            draftId={null}
            pickAttachments={pickAttachments}
            selectedIds={selectedResourceIds}
            workId={work.id}
            onImported={(imported) =>
              setAttachmentResults((current) => {
                const merged = new Map(current.map((resource) => [resource.id, resource]));
                for (const resource of imported) merged.set(resource.id, resource);
                return [...merged.values()];
              })
            }
            onSelectedIdsChange={setSelectedResourceIds}
          />
          <ComposerModelIndicator modelLabel={modelLabel} />
          <button
            aria-label={runActive ? t("composer.queueNext") : actionLabel}
            className={`button button--primary composer-submit${runActive ? " composer-submit--queue" : ""}`}
            type="button"
            disabled={!canSubmit || submitting || loading}
            onClick={() => void submit()}
          >
            <ArrowUp aria-hidden="true" size={16} />
          </button>
        </div>
      </div>
    </footer>
  );
}
