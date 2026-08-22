import { useTranslation } from "react-i18next";

import type { WorkSummary } from "../../bindings";
import type { PickAttachments } from "../../app/attachmentPicker";
import type { PickProjectDirectory } from "../../app/projectDirectory";
import { NewWorkStart } from "./NewWorkStart";

export function WorkHome({
  draftRevision,
  initialPrompt,
  initialRootPath,
  modelLabel,
  works,
  pickProjectDirectory,
  pickAttachments,
  onStarted,
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

  return (
    <div aria-label={t("workspace.home")} className="work-home" role="region">
      <NewWorkStart
        initialPrompt={initialPrompt}
        initialRootPath={initialRootPath}
        key={draftRevision}
        modelLabel={modelLabel}
        onStarted={onStarted}
        pickAttachments={pickAttachments}
        pickProjectDirectory={pickProjectDirectory}
        variant="dashboard"
        works={works}
      />
    </div>
  );
}
