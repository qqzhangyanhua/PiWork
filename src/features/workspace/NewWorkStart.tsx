import {
  ArrowUp,
  ArrowUpRight,
  Bot,
  BookOpen,
  Bug,
  Check,
  FileText,
  Folder,
  FolderOpen,
  GitPullRequest,
  Globe,
  Hammer,
  Laptop,
  Plus,
  Search,
  X,
} from "lucide-react";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
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
import { ContinuousLoopLogo } from "../../components/brand/ContinuousLoopLogo";

const workTitle = (prompt: string) => {
  const firstLine = prompt.trim().split(/\r?\n/, 1)[0] ?? prompt.trim();
  return firstLine.slice(0, 40);
};

export function NewWorkStart({
  initialPrompt,
  initialRootPath,
  modelLabel,
  works,
  onStarted,
  pickProjectDirectory,
  pickAttachments,
  variant = "standalone",
  dashboardExtras,
}: {
  initialPrompt?: string;
  initialRootPath?: string;
  modelLabel: string;
  works: WorkSummary[];
  onStarted(): void;
  pickProjectDirectory: PickProjectDirectory;
  pickAttachments: PickAttachments;
  variant?: "standalone" | "dashboard";
  dashboardExtras?: ReactNode;
}) {
  const { t } = useTranslation();
  const createWork = useWorkStore((state) => state.createWork);
  const startWork = useWorkStore((state) => state.startWork);
  const { client } = useWorkStoreContext();
  const error = useWorkStore((state) => state.error);
  const [prompt, setPrompt] = useState(initialPrompt ?? "");
  const [referencedFiles, setReferencedFiles] = useState<string[]>([]);
  const draftIdRef = useRef<string>(crypto.randomUUID());
  const draftId = draftIdRef.current;
  const [attachmentResults, setAttachmentResults] = useState<ResourceSummary[]>([]);
  const [selectedResourceIds, setSelectedResourceIds] = useState<string[]>([]);
  const [detachError, setDetachError] = useState(false);
  const [rootPath, setRootPath] = useState(initialRootPath?.trim() ?? "");
  const [defaultRootPath, setDefaultRootPath] = useState("");
  const [projectQuery, setProjectQuery] = useState("");
  const [projectOpen, setProjectOpen] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const submittingRef = useRef(false);
  const projectTriggerRef = useRef<HTMLButtonElement>(null);
  const projectPickerRef = useRef<HTMLDivElement>(null);
  const promptRef = useRef<HTMLDivElement>(null);
  const recentPaths = useMemo(
    () => Array.from(new Set(works.map((work) => work.rootPath))).slice(0, 20),
    [works],
  );
  const filteredPaths = recentPaths.filter((path) =>
    path.toLocaleLowerCase().includes(projectQuery.trim().toLocaleLowerCase()),
  );
  const effectiveRootPath = rootPath || defaultRootPath;
  const suggestions = [
    { key: "explore", icon: Search },
    { key: "build", icon: Hammer },
    { key: "review", icon: GitPullRequest },
    { key: "fix", icon: Bug },
    { key: "docs", icon: FileText },
  ] as const;
  useEffect(() => {
    if (error && !submitting) promptRef.current?.focus();
  }, [error, submitting]);

  useEffect(() => {
    const next = initialRootPath?.trim();
    if (!next || next === rootPath) return;
    setRootPath(next);
    setPrompt((current) => stripFileMentions(current));
    setReferencedFiles([]);
    // The explicit project comes from a sidebar action; manual picker changes
    // must remain local until another sidebar action changes this prop.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialRootPath]);

  useEffect(() => {
    if (!client.getDefaultProjectDirectory) return;
    let active = true;
    void client.getDefaultProjectDirectory()
      .then((path) => {
        if (active && path.trim()) setDefaultRootPath(path.trim());
      })
      .catch(() => {
        // The user can still choose an explicit project from the picker.
      });
    return () => { active = false; };
  }, [client]);

  useEffect(() => {
    if (!projectOpen) return;

    const closeOnOutsidePointer = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (projectTriggerRef.current?.contains(target) || projectPickerRef.current?.contains(target)) return;
      setProjectOpen(false);
    };

    const closeOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      setProjectOpen(false);
      queueMicrotask(() => projectTriggerRef.current?.focus());
    };

    document.addEventListener("pointerdown", closeOnOutsidePointer);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutsidePointer);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [projectOpen]);

  const projectName = (path: string) => path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
  const projectParent = (path: string) => {
    const normalized = path.replace(/[\\/]+$/u, "");
    const boundary = Math.max(normalized.lastIndexOf("\\"), normalized.lastIndexOf("/"));
    return boundary > 0 ? normalized.slice(0, boundary) : normalized;
  };

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

  const clearProject = () => {
    setRootPath("");
    setPrompt((current) => stripFileMentions(current));
    setReferencedFiles([]);
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
    if ((!instruction && !firstAttachment) || !effectiveRootPath || submittingRef.current) return;
    submittingRef.current = true;
    setSubmitting(true);
    try {
      const detail = await createWork({
        title: instruction
          ? workTitle(instruction)
          : (firstAttachment?.originalName ?? "Attachment").slice(0, 40),
        goal: instruction || `Review ${firstAttachment?.originalName ?? "attachment"}`,
        rootPath: effectiveRootPath,
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

  const attachmentButton = (
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
  );
  const sendButton = (
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
        !effectiveRootPath ||
        submitting
      }
      onClick={() => void submit()}
      type="button"
    >
      <ArrowUp aria-hidden="true" size={17} />
      {variant === "dashboard" && (
        <span className="new-work-start__send-hint" aria-hidden="true">{t("dashboard.composer.sendHint")}</span>
      )}
    </button>
  );

  return (
    <section
      className={`new-work-start${variant === "dashboard" ? " new-work-start--dashboard" : ""}`}
      aria-labelledby={variant === "dashboard" ? undefined : "new-work-heading"}
    >
      {variant === "standalone" && (
        <div className="new-work-start__intro">
          <span className="new-work-start__mark" aria-hidden="true">
            <ContinuousLoopLogo size={28} />
          </span>
          <h1 id="new-work-heading">{t("newWork.heading")}</h1>
        </div>
      )}
      <div aria-label={t("newWork.suggestions.label")} className="new-work-suggestions">
        {suggestions.map(({ key, icon: Icon }) => (
          <button
            aria-label={t(`newWork.suggestions.${key}.title`)}
            className="new-work-suggestion"
            disabled={submitting}
            key={key}
            onClick={() => {
              setPrompt(t(`newWork.suggestions.${key}.prompt`));
              setReferencedFiles([]);
              queueMicrotask(() => promptRef.current?.focus());
            }}
            type="button"
          >
            <span className="new-work-suggestion__icon"><Icon aria-hidden="true" size={17} /></span>
            <strong>{t(`newWork.suggestions.${key}.title`)}</strong>
            <small>{t(`newWork.suggestions.${key}.body`)}</small>
            <ArrowUpRight aria-hidden="true" className="new-work-suggestion__arrow" size={14} />
          </button>
        ))}
      </div>
      {dashboardExtras}
      <div
        className="new-work-start__composer"
        data-testid={variant === "dashboard" ? "dashboard-composer" : undefined}
      >
        <div
          className="new-work-start__contextbar"
          data-testid={variant === "dashboard" ? "dashboard-composer-context" : undefined}
        >
          <div className={`project-chip-control${rootPath ? " project-chip-control--selected" : ""}`}>
            <button
              aria-label={rootPath
                ? t("newWork.changeProject", { project: projectName(rootPath) })
                : t("newWork.selectProject")}
              aria-expanded={projectOpen}
              aria-haspopup="dialog"
              className={`project-chip${rootPath ? " project-chip--selected" : ""}`}
              disabled={submitting}
              onClick={() => setProjectOpen((open) => !open)}
              ref={projectTriggerRef}
              title={rootPath || undefined}
              type="button"
            >
              <Folder aria-hidden="true" size={15} />
              <span title={rootPath}>{rootPath ? projectName(rootPath) : t("newWork.defaultProject")}</span>
            </button>
            {rootPath && (
              <button
                aria-label={t("newWork.clearProject")}
                className="project-chip__clear"
                disabled={submitting}
                onClick={clearProject}
                type="button"
              >
                <X aria-hidden="true" size={14} />
              </button>
            )}
            {projectOpen && (
              <div aria-label={t("newWork.projectPicker")} className="project-picker" ref={projectPickerRef} role="dialog">
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
                        <span>{projectName(path)}</span>
                        <small className="project-picker__path-context">{projectParent(path)}</small>
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
          <span className="new-work-start__local-context">
            <Laptop aria-hidden="true" size={14} />
            {t("newWork.localExecution")}
          </span>
        </div>
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
          placeholder={variant === "dashboard" ? t("dashboard.composer.placeholder") : t("newWork.promptPlaceholder")}
          rootPath={effectiveRootPath}
          value={prompt}
        />
        <div className="new-work-start__actions">
          {variant === "dashboard" ? (
            <>
              <div className="new-work-start__tool-group" data-testid="dashboard-composer-tools">
                {attachmentButton}
                <button
                  aria-label={t("dashboard.comingSoon", { feature: t("dashboard.composer.agentButton") })}
                  className="new-work-start__extra-action"
                  disabled
                  type="button"
                >
                  <Bot aria-hidden="true" size={16} />
                </button>
                <button
                  aria-label={t("dashboard.comingSoon", { feature: t("dashboard.composer.knowledgeButton") })}
                  className="new-work-start__extra-action"
                  disabled
                  type="button"
                >
                  <BookOpen aria-hidden="true" size={16} />
                </button>
                <button
                  aria-label={t("dashboard.comingSoon", { feature: t("dashboard.composer.webSearchButton") })}
                  className="new-work-start__extra-action"
                  disabled
                  type="button"
                >
                  <Globe aria-hidden="true" size={16} />
                </button>
              </div>
              <div className="new-work-start__submit-group" data-testid="dashboard-composer-submit">
                <ComposerModelIndicator modelLabel={modelLabel} />
                {sendButton}
              </div>
            </>
          ) : (
            <>
              {attachmentButton}
              <ComposerModelIndicator modelLabel={modelLabel} />
              {sendButton}
            </>
          )}
        </div>
      </div>
      {detachError && (
        <p className="new-work-start__error" role="alert">
          {t("errors.resourceStorage")}
        </p>
      )}
      {error && <p className="new-work-start__error" role="alert">{t(appErrorMessageKey(error), appErrorMessageValues(error))}</p>}
    </section>
  );
}
