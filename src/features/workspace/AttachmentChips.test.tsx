import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import type { ResourceSummary } from "../../bindings";
import { i18n } from "../../i18n";
import { AttachmentChips } from "./AttachmentChips";

const resource = (overrides: Partial<ResourceSummary>): ResourceSummary => ({
  id: "resource-1",
  originalName: "brief.pdf",
  mediaType: "application/pdf",
  size: 1536n,
  origin: "user_upload",
  status: "ready",
  failureCode: null,
  createdAt: "2026-08-01T00:00:00.000Z",
  ...overrides,
});

describe("AttachmentChips", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("shows file type, formatted size, and every managed resource state", () => {
    render(
      <AttachmentChips resources={[
        resource({ id: "staging", originalName: "queued.pdf", status: "staging" }),
        resource({ id: "processing", originalName: "scan.pdf", status: "processing" }),
        resource({ id: "ready", originalName: "brief.pdf", status: "ready" }),
        resource({ id: "failed", originalName: "broken.bmp", mediaType: "image/bmp", status: "failed", failureCode: "unsupported_image" }),
      ]} />,
    );

    expect(screen.getByText("准备导入")).toBeInTheDocument();
    expect(screen.getByText("正在处理")).toBeInTheDocument();
    expect(screen.getByText("可用")).toBeInTheDocument();
    expect(screen.getAllByText(/PDF · 1\.5 KB/u).length).toBeGreaterThanOrEqual(3);
    expect(screen.getByText("不支持此图片格式")).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("unsupported_image");
  });

  it("maps every import limit and unsupported-document failure to safe copy", () => {
    render(
      <AttachmentChips resources={[
        resource({ id: "image-limit", originalName: "large.png", mediaType: "image/png", status: "failed", failureCode: "image_too_large" }),
        resource({ id: "document-limit", originalName: "large.pdf", status: "failed", failureCode: "document_too_large" }),
        resource({ id: "unsupported-document", originalName: "legacy.doc", mediaType: "application/msword", status: "failed", failureCode: "unsupported_document" }),
      ]} />,
    );

    expect(screen.getByText("图片超过 10 MB 限制")).toBeInTheDocument();
    expect(screen.getByText("文档超过 50 MB 限制")).toBeInTheDocument();
    expect(screen.getByText("不支持此文档格式")).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent(/image_too_large|document_too_large|unsupported_document/u);
  });
});
