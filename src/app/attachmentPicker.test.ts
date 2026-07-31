import { describe, expect, it } from "vitest";

import { ATTACHMENT_EXTENSIONS } from "./attachmentPicker";

describe("attachment picker", () => {
  it("offers every image and first-pack document extension", () => {
    expect(ATTACHMENT_EXTENSIONS).toEqual([
      "png",
      "jpg",
      "jpeg",
      "gif",
      "webp",
      "pdf",
      "docx",
      "xls",
      "xlsx",
      "xlsm",
      "xlsb",
      "csv",
    ]);
  });
});
