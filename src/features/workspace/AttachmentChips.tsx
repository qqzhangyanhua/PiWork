import {
  File,
  FileSpreadsheet,
  FileText,
  Image as ImageIcon,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import type { ResourceSummary } from "../../bindings";
import { useOptionalWorkStoreContext } from "../works/WorkStoreProvider";

export function AttachmentThumbnail({ resource }: { resource: ResourceSummary }) {
  const context = useOptionalWorkStoreContext();
  const client = context?.client;
  const [source, setSource] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    if (
      resource.status !== "ready" ||
      !resource.mediaType.startsWith("image/") ||
      !client
    )
      return () => undefined;
    void client.getResourceThumbnail(resource.id).then(
      (thumbnail) => {
        if (active && thumbnail.dataBase64) {
          setSource(`data:${thumbnail.mediaType};base64,${thumbnail.dataBase64}`);
        }
      },
      () => undefined,
    );
    return () => {
      active = false;
    };
  }, [client, resource.id, resource.status]);

  if (!resource.mediaType.startsWith("image/")) {
    const Icon = documentIcon(resource.mediaType);
    return (
      <span className="attachment-chip__thumbnail attachment-chip__thumbnail--document">
        <Icon aria-hidden="true" size={16} />
      </span>
    );
  }

  return source ? (
    <img
      alt=""
      className="attachment-chip__thumbnail"
      loading="lazy"
      src={source}
    />
  ) : (
    <span className="attachment-chip__thumbnail attachment-chip__thumbnail--fallback">
      <ImageIcon aria-hidden="true" size={15} />
    </span>
  );
}

const documentIcon = (mediaType: string) => {
  if (
    mediaType === "text/csv" ||
    mediaType.includes("excel") ||
    mediaType.includes("spreadsheet")
  )
    return FileSpreadsheet;
  if (mediaType === "application/pdf" || mediaType.includes("wordprocessingml"))
    return FileText;
  return File;
};

export function AttachmentChips({ resources }: { resources: ResourceSummary[] }) {
  const { t } = useTranslation();
  return (
    <div className="attachment-chips">
      {resources.map((resource) => (
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
        </div>
      ))}
    </div>
  );
}
