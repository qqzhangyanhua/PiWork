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

export function useAttachmentThumbnailSource(resource: ResourceSummary) {
  const context = useOptionalWorkStoreContext();
  const client = context?.client;
  const [source, setSource] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    setSource(null);
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

  return source;
}

export function AttachmentThumbnail({ resource }: { resource: ResourceSummary }) {
  const source = useAttachmentThumbnailSource(resource);

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

const formatSize = (size: bigint) => {
  const bytes = Number(size);
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10 * 1024 ? 1 : 0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(bytes < 10 * 1024 * 1024 ? 1 : 0)} MB`;
};

const fileType = (resource: ResourceSummary) => {
  const extension = resource.originalName.split(".").at(-1)?.toLocaleUpperCase();
  if (resource.mediaType === "application/pdf") return "PDF";
  if (resource.mediaType === "text/csv") return "CSV";
  return extension && extension.length <= 8 ? extension : resource.mediaType.split("/").at(-1)?.toLocaleUpperCase() ?? "FILE";
};

const failureMessageKey = (failureCode: string | null) => {
  const keys: Record<string, string> = {
    unsupported_image: "attachments.failure.unsupportedImage",
    image_too_large: "attachments.failure.imageTooLarge",
    image_decode_failed: "attachments.failure.imageDecode",
    thumbnail_failed: "attachments.failure.thumbnail",
    unsupported_document: "attachments.failure.unsupportedDocument",
    document_too_large: "attachments.failure.documentTooLargeUpload",
    document_parse_failed: "attachments.failure.documentParse",
    document_output_too_large: "attachments.failure.documentTooLarge",
    document_ocr_failed: "attachments.failure.ocr",
    document_runtime_unavailable: "attachments.failure.runtime",
    document_runtime_timeout: "attachments.failure.timeout",
    import_interrupted: "attachments.failure.interrupted",
  };
  return failureCode ? keys[failureCode] ?? "attachments.failure.generic" : "attachments.failure.generic";
};

export function AttachmentMetadata({ resource }: { resource: ResourceSummary }) {
  const { t } = useTranslation();
  const status = resource.status === "failed"
    ? t(failureMessageKey(resource.failureCode))
    : t(`attachments.status.${resource.status}`);
  return (
    <span className="attachment-chip__meta">
      <span className="attachment-chip__name">{resource.originalName}</span>
      <span className="attachment-chip__details">{fileType(resource)} · {formatSize(resource.size)}</span>
      <span className="attachment-chip__status">{status}</span>
    </span>
  );
}

export function AttachmentChips({ resources }: { resources: ResourceSummary[] }) {
  return (
    <div className="attachment-chips">
      {resources.map((resource) => (
        <div
          className={`attachment-chip attachment-chip--${resource.status}`}
          key={resource.id}
          role={resource.status === "failed" ? "alert" : undefined}
          title={resource.originalName}
        >
          <AttachmentThumbnail resource={resource} />
          <AttachmentMetadata resource={resource} />
        </div>
      ))}
    </div>
  );
}
