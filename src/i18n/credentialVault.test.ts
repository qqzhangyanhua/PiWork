import { describe, expect, it } from "vitest";

import { credentialVaultKind } from "./credentialVault";

describe("credentialVaultKind", () => {
  it("recognizes macOS Tauri and jsdom user agents", () => {
    expect(
      credentialVaultKind(
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)",
      ),
    ).toBe("macos");
    expect(credentialVaultKind("Mozilla/5.0 (darwin) AppleWebKit/537.36 jsdom/26.0.0")).toBe(
      "macos",
    );
  });

  it("recognizes Windows user agents", () => {
    expect(
      credentialVaultKind(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko)",
      ),
    ).toBe("windows");
    expect(credentialVaultKind("Mozilla/5.0 (win32) AppleWebKit/537.36 jsdom/26.0.0")).toBe(
      "windows",
    );
  });

  it("does not treat an unknown agent as Windows Credential Manager", () => {
    expect(credentialVaultKind("Mozilla/5.0 (X11; Linux x86_64)")).toBe("other");
  });
});
