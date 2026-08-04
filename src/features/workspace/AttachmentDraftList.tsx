import { X } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { ResourceSummary } from "../../bindings";
import {
  AttachmentMetadata,
  AttachmentThumbnail,
  useAttachmentThumbnailSource,
} from "./AttachmentChips";

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
  const [preview, setPreview] = useState<{
    resource: ResourceSummary;
    trigger: HTMLButtonElement;
  } | null>(null);
  const visible = resources.filter(
    (resource) => resource.status === "failed" || selectedIds.includes(resource.id),
  );
  if (visible.length === 0) return null;

  return (
    <div className="attachment-drafts">
      {visible.map((resource) => (
        <div
          className={`attachment-chip attachment-chip--${resource.status}`}
          key={resource.id}
          role={resource.status === "failed" ? "alert" : undefined}
          title={resource.originalName}
        >
          {resource.status === "ready" && resource.mediaType.startsWith("image/") ? (
            <button
              aria-label={t("attachments.preview", { name: resource.originalName })}
              className="attachment-chip__preview-trigger"
              onClick={(event) => setPreview({ resource, trigger: event.currentTarget })}
              type="button"
            >
              <AttachmentThumbnail resource={resource} />
            </button>
          ) : (
            <AttachmentThumbnail resource={resource} />
          )}
          <AttachmentMetadata resource={resource} />
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
      {preview && (
        <AttachmentImagePreview
          resource={preview.resource}
          returnFocus={preview.trigger}
          onClose={() => setPreview(null)}
        />
      )}
    </div>
  );
}

function AttachmentImagePreview({
  resource,
  returnFocus,
  onClose,
}: {
  resource: ResourceSummary;
  returnFocus: HTMLButtonElement;
  onClose(): void;
}) {
  const { t } = useTranslation();
  const titleId = useId();
  const closeRef = useRef<HTMLButtonElement>(null);
  const source = useAttachmentThumbnailSource(resource);

  useEffect(() => {
    closeRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
      if (event.key === "Tab") {
        event.preventDefault();
        closeRef.current?.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      returnFocus.focus();
    };
  }, [onClose, returnFocus]);

  return (
    <div
      className="attachment-preview"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section
        aria-labelledby={titleId}
        aria-modal="true"
        className="attachment-preview__dialog"
        role="dialog"
      >
        <header className="attachment-preview__header">
          <h2 id={titleId}>{resource.originalName}</h2>
          <button
            aria-label={t("common.close")}
            className="attachment-preview__close"
            onClick={onClose}
            ref={closeRef}
            type="button"
          >
            <X aria-hidden="true" size={17} />
          </button>
        </header>
        <div className="attachment-preview__canvas">
          {source ? (
            <img alt={resource.originalName} src={source} />
          ) : (
            <p role="status">{t("attachments.previewLoading")}</p>
          )}
        </div>
      </section>
    </div>
  );
}
