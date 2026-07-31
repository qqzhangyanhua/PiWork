import type { AppError } from "./work";

const fieldOf = (error: AppError) => {
  const field = error.details?.field;
  return typeof field === "string" ? field : undefined;
};

export const appErrorMessageKey = (error: AppError): string => {
  const field = fieldOf(error);
  if (error.code === "path_resolution_error" && field === "rootPath") {
    return "errors.workspacePath";
  }
  if (error.code === "invalid_input") {
    if (field === "title") return "errors.invalidInput.title";
    if (field === "goal") return "errors.invalidInput.goal";
    if (field === "rootPath") return "errors.invalidInput.rootPath";
    if (field === "prompt") return "errors.invalidInput.prompt";
  }

  const byCode: Record<string, string> = {
    concurrent_modification: "errors.concurrentModification",
    database_error: "errors.database",
    engine_error: "errors.engine",
    engine_faulted: "errors.engine",
    engine_start_failed: "errors.engineStart",
    event_publish_error: "errors.eventPublish",
    invalid_work_state: "errors.invalidWorkState",
    io_error: "errors.io",
    migration_error: "errors.database",
    not_found: "errors.notFound",
    path_resolution_error: "errors.pathResolution",
    referenced_file_error: "errors.referencedFile",
    resource_import: "errors.resourceImport",
    resource_not_found: "errors.resourceNotFound",
    resource_storage: "errors.resourceStorage",
    work_already_running: "errors.workAlreadyRunning",
  };
  return byCode[error.code] ?? "errors.generic";
};

export const appErrorMessageValues = (error: AppError): Record<string, string> | undefined => {
  const path = error.details?.path;
  if (
    error.code === "referenced_file_error" &&
    typeof path === "string" &&
    path.length > 0 &&
    !path.startsWith("/") &&
    !/^[a-z]:/iu.test(path) &&
    !path.split(/[\\/]/u).includes("..")
  ) {
    return { path };
  }
  return undefined;
};

const diagnosticFields = new Set(["field", "from", "reason", "runId", "to", "workId"]);

export const formatAppErrorDiagnostics = (error: AppError, fallback: string) => {
  if (!error.details) return `${error.code}\n${error.message}`;
  try {
    // Preserve the existing fail-safe for cyclic objects and throwing getters.
    JSON.stringify(error.details);
    const details = Object.fromEntries(
      Object.entries(error.details).map(([key, value]) => [
        key,
        diagnosticFields.has(key) &&
        (typeof value === "string" || typeof value === "number" || typeof value === "boolean")
          ? value
          : "[redacted]",
      ]),
    );
    return `${error.code}\n${error.message}\n${JSON.stringify(details, null, 2)}`;
  } catch {
    return `${error.code}\n${error.message}\n${fallback}`;
  }
};
