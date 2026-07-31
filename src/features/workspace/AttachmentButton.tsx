import { Check, Plus, Upload } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { ResourceSummary } from "../../bindings";
import type { PickAttachments } from "../../app/attachmentPicker";
import { useWorkStore } from "../works/WorkStoreProvider";

type AttachmentButtonProps = {
  available: ResourceSummary[];
  disabled?: boolean;
  draftId: string | null;
  pickAttachments: PickAttachments;
  selectedIds: string[];
  workId: string | null;
  onImported(resources: ResourceSummary[]): void;
  onSelectedIdsChange(resourceIds: string[]): void;
};

export function AttachmentButton({
  available,
  disabled,
  draftId,
  pickAttachments,
  selectedIds,
  workId,
  onImported,
  onSelectedIdsChange,
}: AttachmentButtonProps) {
  const { t } = useTranslation();
  const importResources = useWorkStore((state) => state.importResources);
  const [open, setOpen] = useState(false);
  const [importing, setImporting] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  const close = (restoreFocus = false) => {
    setOpen(false);
    if (restoreFocus) queueMicrotask(() => triggerRef.current?.focus());
  };

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (event.target instanceof Node && !rootRef.current?.contains(event.target)) close();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        close(true);
      }
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const upload = async () => {
    if (importing) return;
    const sourcePaths = await pickAttachments();
    if (sourcePaths.length === 0) return;
    setImporting(true);
    try {
      const imported = await importResources({ sourcePaths, draftId, workId });
      onImported(imported);
      const readyIds = imported
        .filter(({ status }) => status === "ready")
        .map(({ id }) => id);
      onSelectedIdsChange([...new Set([...selectedIds, ...readyIds])]);
      close(true);
    } finally {
      setImporting(false);
    }
  };

  const ready = available.filter(({ status }) => status === "ready");
  return (
    <div className="attachment-button" ref={rootRef}>
      <button
        aria-expanded={open}
        aria-label={t("attachments.add")}
        className="attachment-button__trigger"
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
        ref={triggerRef}
        type="button"
      >
        <Plus aria-hidden="true" size={16} />
      </button>
      {open && (
        <div className="attachment-popover">
          {ready.length > 0 && (
            <div className="attachment-popover__available">
              <span>{t("attachments.available")}</span>
              {ready.map((resource) => {
                const selected = selectedIds.includes(resource.id);
                return (
                  <button
                    aria-pressed={selected}
                    key={resource.id}
                    onClick={() =>
                      onSelectedIdsChange(
                        selected
                          ? selectedIds.filter((id) => id !== resource.id)
                          : [...selectedIds, resource.id],
                      )
                    }
                    title={resource.originalName}
                    type="button"
                  >
                    <span>{resource.originalName}</span>
                    {selected && <Check aria-hidden="true" size={14} />}
                  </button>
                );
              })}
            </div>
          )}
          <button
            className="attachment-popover__upload"
            disabled={importing}
            onClick={() => void upload()}
            type="button"
          >
            <Upload aria-hidden="true" size={15} />
            {importing ? t("attachments.importing") : t("attachments.uploadFiles")}
          </button>
        </div>
      )}
    </div>
  );
}
