import { describe, expect, it } from "vitest";

import {
  appErrorMessageKey,
  appErrorMessageValues,
  formatAppErrorDiagnostics,
} from "./appError";

describe("referenced file errors", () => {
  it("uses a localized message with the safe relative path", () => {
    const error = {
      code: "referenced_file_error",
      message: "internal filesystem detail",
      details: { path: "src/context.ts" },
    };

    expect(appErrorMessageKey(error)).toBe("errors.referencedFile");
    expect(appErrorMessageValues(error)).toEqual({ path: "src/context.ts" });
  });
});

describe("resource errors", () => {
  it("maps resource failures without exposing backend diagnostics", () => {
    const error = {
      code: "resource_import",
      message: "resource import failed",
      details: { reason: "unsupported_image", sourcePath: "C:/private/file.png" },
    };

    expect(appErrorMessageKey(error)).toBe("errors.resourceImport");
    expect(appErrorMessageValues(error)).toBeUndefined();
  });
});

describe("formatAppErrorDiagnostics", () => {
  it("shows the sanitized engine startup reason while redacting unknown details", () => {
    const diagnostics = formatAppErrorDiagnostics(
      {
        code: "engine_start_failed",
        message: "Engine failed to start",
        details: {
          reason: "Pi RPC exited before accepting the Run (exit code 1): safe stderr",
          secret: "must not be displayed",
          workId: "work-1",
        },
      },
      "unavailable",
    );

    expect(diagnostics).toContain(
      "Pi RPC exited before accepting the Run (exit code 1): safe stderr",
    );
    expect(diagnostics).toContain('"secret": "[redacted]"');
    expect(diagnostics).not.toContain("must not be displayed");
  });
});
