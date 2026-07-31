import { ArrowUp, Check, Folder, FolderOpen, Plus, Search } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { appErrorMessageKey, appErrorMessageValues } from "../../domain/appError";
import type { ResourceSummary, WorkSummary } from "../../bindings";
import type { PickAttachments } from "../../app/attachmentPicker";
import type { PickProjectDirectory } from "../../app/projectDirectory";
import { didPersistStartInstruction } from "../works/workStore";
import { useWorkStore, useWorkStoreContext } from "../works/WorkStoreProvider";
import { stripFileMentions } from "./fileMentions";
import { ComposerModelIndicator } from "./ComposerModelIndicator";
import { ProjectPromptEditor } from "./ProjectPromptEditor";
import { AttachmentButton } from "./AttachmentButton";
import { AttachmentDraftList } from "./AttachmentDraftList";

const workTitle = (prompt: string) => {
  const firstLine = prompt.trim().split(/\r?\n/, 1)[0] ?? prompt.trim();
  return firstLine.slice(0, 40);
};

export function NewWorkStart({
  modelLabel,
  works,
  onStarted,
  pickProjectDirectory,
  pickAttachments,
}: {
  modelLabel: string;
  works: WorkSummary[];
  onStarted(): void;
  pickProjectDirectory: PickProjectDirectory;
  pickAttachments: PickAttachments;
}) {
  const { t } = useTranslation();
  const createWork = useWorkStore((state) => state.createWork);
  const startWork = useWorkStore((state) => state.startWork);
  const { client } = useWorkStoreContext();
  const error = useWorkStore((state) => state.error);
  const [prompt, setPrompt] = useState("");
  const [referencedFiles, setReferencedFiles] = useState<string[]>([]);
  const draftIdRef = useRef<string>(crypto.randomUUID());
  const draftId = draftIdRef.current;
  const [attachmentResults, setAttachmentResults] = useState<ResourceSummary[]>([]);
  const [selectedResourceIds, setSelectedResourceIds] = useState<string[]>([]);
  const [detachError, setDetachError] = useState(false);
  const [rootPath, setRootPath] = useState("");
  const [projectQuery, setProjectQuery] = useState("");
  const [projectOpen, setProjectOpen] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const submittingRef = useRef(false);
  const projectTriggerRef = useRef<HTMLButtonElement>(null);
  const projectPickerRef = useRef<HTMLDivElement>(null);
  const promptRef = useRef<HTMLDivElement>(null);
  const recentPaths = Array.from(new Set(works.map((work) => work.rootPath))).slice(0, 20);
  const filteredPaths = recentPaths.filter((path) =>
    path.toLocaleLowerCase().includes(projectQuery.trim().toLocaleLowerCase()),
  );
  useEffect(() => {
    if (error && !submitting) promptRef.current?.focus();
  }, [error, submitting]);

  useEffect(() => {
    if (!projectOpen) return;

    const closeOnOutsidePointer = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (projectTriggerRef.current?.contains(target) || projectPickerRef.current?.contains(target)) return;
      setProjectOpen(false);
    };

    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [projectOpen]);

  const usePath = (path: string) => {
    const next = path.trim();
    if (!next) return;
    if (rootPath && rootPath !== next) {
      setPrompt((current) => stripFileMentions(current));
      setReferencedFiles([]);
    }
    setRootPath(next);
    setProjectQuery("");
    setProjectOpen(false);
  };

  const submit = async () => {
    const instruction = prompt.trim();
    const readyResources = attachmentResults.filter(
      (resource) =>
        resource.status === "ready" && selectedResourceIds.includes(resource.id),
    );
    const firstAttachment = readyResources[0];
    if ((!instruction && !firstAttachment) || !rootPath || submittingRef.current) return;
    submittingRef.current = true;
    setSubmitting(true);
    try {
      const detail = await createWork({
        title: instruction
          ? workTitle(instruction)
          : (firstAttachment?.originalName ?? "Attachment").slice(0, 40),
        goal: instruction || `Review ${firstAttachment?.originalName ?? "attachment"}`,
        rootPath,
        permissionMode: "balanced",
        resourceDraftId: draftId,
      });
      try {
        await startWork(
          detail.summary.id,
          instruction,
          referencedFiles,
          readyResources.map(({ id }) => id),
        );
      } catch (startError) {
        if (!didPersistStartInstruction(startError)) throw startError;
      }
      onStarted();
    } catch {
      // The store exposes a localized product-safe error below the composer.
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  return (
    <section className="new-work-start" aria-labelledby="new-work-heading">
      <div className="new-work-start__intro">
        <span className="new-work-start__pi" aria-hidden="true">π</span>
        <h1 id="new-work-heading">{t("newWork.heading")}</h1>
        <p>{t("newWork.body")}</p>
      </div>
      <div className="new-work-start__composer">
        <ProjectPromptEditor
          autoFocus
          disabled={submitting}
          editorRef={promptRef}
          id="first-work-prompt"
          label={t("newWork.promptLabel")}
          onDraftChange={(nextPrompt, nextReferences) => {
            setPrompt(nextPrompt);
            setReferencedFiles(nextReferences);
          }}
          onSubmit={() => void submit()}
          placeholder={t("newWork.promptPlaceholder")}
          rootPath={rootPath}
          value={prompt}
        />
        <AttachmentDraftList
          resources={attachmentResults}
          selectedIds={selectedResourceIds}
          onRemove={(resource) => {
            const previousResults = attachmentResults;
            const previousSelection = selectedResourceIds;
            setDetachError(false);
            setAttachmentResults((current) =>
              current.filter(({ id }) => id !== resource.id),
            );
            setSelectedResourceIds((current) =>
              current.filter((id) => id !== resource.id),
            );
            void client.detachDraftResource(draftId, resource.id).catch(() => {
              setAttachmentResults(previousResults);
              setSelectedResourceIds(previousSelection);
              setDetachError(true);
            });
          }}
        />
        <div className="new-work-start__actions">
          <button
            aria-expanded={projectOpen}
            className={`project-chip${rootPath ? " project-chip--selected" : ""}`}
            disabled={submitting}
            onClick={() => setProjectOpen((open) => !open)}
            ref={projectTriggerRef}
            title={rootPath || undefined}
            type="button"
          >
            <Folder aria-hidden="true" size={15} />
            <span title={rootPath}>{rootPath ? rootPath.split(/[\\/]/).filter(Boolean).at(-1) : t("newWork.selectProject")}</span>
          </button>
          <AttachmentButton
            available={attachmentResults}
            disabled={submitting}
            draftId={draftId}
            pickAttachments={pickAttachments}
            selectedIds={selectedResourceIds}
            workId={null}
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
            aria-label={t("newWork.start")}
            className="new-work-start__send"
            disabled={
              (!prompt.trim() &&
                !attachmentResults.some(
                  (resource) =>
                    resource.status === "ready" &&
                    selectedResourceIds.includes(resource.id),
                )) ||
              !rootPath ||
              submitting
            }
            onClick={() => void submit()}
            type="button"
          >
            <ArrowUp aria-hidden="true" size={17} />
          </button>
        </div>
        {projectOpen && (
          <div className="project-picker" ref={projectPickerRef}>
            <label className="project-picker__search">
              <Search aria-hidden="true" size={15} />
              <input
                aria-label={t("newWork.searchProjects")}
                autoFocus
                onChange={(event) => setProjectQuery(event.target.value)}
                placeholder={t("newWork.searchProjects")}
                type="search"
                value={projectQuery}
              />
            </label>
            {recentPaths.length > 0 && (
              <div className="project-picker__recent">
                <span>{t("newWork.recentProjects")}</span>
                {filteredPaths.map((path) => (
                  <button key={path} onClick={() => usePath(path)} title={path} type="button">
                    <FolderOpen aria-hidden="true" size={14} />
                    <span>{path.split(/[\\/]/).filter(Boolean).at(-1)}</span>
                    {path === rootPath && <Check aria-hidden="true" className="project-picker__check" size={14} />}
                  </button>
                ))}
              </div>
            )}
            {recentPaths.length === 0 && <p className="project-picker__empty">{t("newWork.noRecentProjects")}</p>}
            <button
              className="project-picker__browse"
              disabled={submitting}
              onClick={async () => {
                const selected = await pickProjectDirectory();
                if (selected) usePath(selected);
              }}
              type="button"
            >
              <Plus aria-hidden="true" size={14} />
              {t("newWork.browse")}
            </button>
          </div>
        )}
      </div>
      {detachError && (
        <p className="new-work-start__error" role="alert">
          {t("errors.resourceStorage")}
        </p>
      )}
      {error && <p className="new-work-start__error" role="alert">{t(appErrorMessageKey(error), appErrorMessageValues(error))}</p>}
      <p className="new-work-start__hint">{t("newWork.hint")}</p>
    </section>
  );
}
