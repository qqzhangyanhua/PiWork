import { X } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ResourceSummary } from "../../bindings";
import { AttachmentThumbnail } from "./AttachmentChips";

export function AttachmentDraftList({
  resources,
  selectedIds,
  onRemove,
}: {
  resources: ResourceSummary[];
  selectedIds: string[];
  onRemove(resource: ResourceSummary): void;
}) {
  const { t } = useTranslation();
  const visible = resources.filter(
    (resource) => resource.status === "failed" || selectedIds.includes(resource.id),
  );
  if (visible.length === 0) return null;

  return (
    <div className="attachment-drafts">
      {visible.map((resource) => (
        <div
          className={`attachment-chip${resource.status === "failed" ? " attachment-chip--failed" : ""}`}
          key={resource.id}
          role={resource.status === "failed" ? "alert" : undefined}
          title={resource.originalName}
        >
          <AttachmentThumbnail resource={resource} />
          <span className="attachment-chip__name">{resource.originalName}</span>
          {resource.status === "failed" && (
            <span className="attachment-chip__status">{t("attachments.failed")}</span>
          )}
          <button
            aria-label={t("attachments.remove", { name: resource.originalName })}
            className="attachment-chip__remove"
            onClick={() => onRemove(resource)}
            type="button"
          >
            <X aria-hidden="true" size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}
