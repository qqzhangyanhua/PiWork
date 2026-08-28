export type CredentialVaultKind = "macos" | "windows" | "other";

export function credentialVaultKind(
  userAgent = typeof navigator === "undefined" ? "" : navigator.userAgent,
): CredentialVaultKind {
  if (/mac|darwin/i.test(userAgent)) {
    return "macos";
  }
  if (/win/i.test(userAgent)) {
    return "windows";
  }
  return "other";
}

export function credentialVaultLabel(translate: (key: string) => string): string {
  return translate(`vault.${credentialVaultKind()}`);
}
