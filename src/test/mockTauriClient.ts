import { vi, type Mock } from "vitest";

import type { PiWorkClient } from "../app/tauriClient";
import type {
  CreateWorkInput,
  MessageSummary,
  ResourceSummary,
  ResourceThumbnail,
  RunSummary,
  StartWorkOutput,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
} from "../bindings";

export type MockTauriClient = PiWorkClient & {
  getModelConfigurationStatus: Mock<PiWorkClient["getModelConfigurationStatus"]>;
  testModelConnection: Mock<PiWorkClient["testModelConnection"]>;
  saveModelConfiguration: Mock<PiWorkClient["saveModelConfiguration"]>;
  createWork: Mock<PiWorkClient["createWork"]>;
  listWorks: Mock<PiWorkClient["listWorks"]>;
  getWork: Mock<PiWorkClient["getWork"]>;
  listProjectFiles: Mock<PiWorkClient["listProjectFiles"]>;
  importResources: Mock<PiWorkClient["importResources"]>;
  listWorkResources: Mock<PiWorkClient["listWorkResources"]>;
  getResourceThumbnail: Mock<PiWorkClient["getResourceThumbnail"]>;
  detachDraftResource: Mock<PiWorkClient["detachDraftResource"]>;
  startWork: Mock<PiWorkClient["startWork"]>;
  listenToWorkEvents: Mock<PiWorkClient["listenToWorkEvents"]>;
  emit(event: WorkEventEnvelope): void;
  seed(detail: WorkDetail): void;
  seedResource(workId: string, resource: ResourceSummary): void;
  unlisten: Mock<() => void>;
};

const now = (offset: number) =>
  new Date(Date.UTC(2026, 6, 28, 8, 0, offset)).toISOString();

export const runCompletedEvent = (
  overrides: Partial<WorkEventEnvelope> = {},
): WorkEventEnvelope => ({
  version: 1,
  workId: "work-1",
  runId: "run-1",
  sequence: 2,
  occurredAt: now(20),
  payload: {
    type: "runCompleted",
    summary: "Work 已完成",
    artifacts: ["dist/report.html"],
    validation: ["Dashboard checks passed"],
    limitations: ["Uses mock revenue data"],
  },
  ...overrides,
});

export const createMockTauriClient = (): MockTauriClient => {
  const details = new Map<string, WorkDetail>();
  const handlers = new Set<(event: WorkEventEnvelope) => void>();
  let workSequence = 0;
  let runSequence = 0;
  let resourceSequence = 0;
  const resourcesByWork = new Map<string, ResourceSummary[]>();
  const resourcesByDraft = new Map<string, ResourceSummary[]>();
  const thumbnailByResource = new Map<string, ResourceThumbnail>();
  const unlisten = vi.fn(() => handlers.clear());
  const getModelConfigurationStatus: Mock<PiWorkClient["getModelConfigurationStatus"]> = vi.fn(async () => ({
    configured: true,
    configuration: {
      provider: "openai" as const,
      modelId: "gpt-5.2",
    },
  }));
  const testModelConnection: Mock<PiWorkClient["testModelConnection"]> = vi.fn(async (_input) => ({
    models: [{ id: "gpt-5.2", label: "GPT-5.2" }],
  }));
  const saveModelConfiguration: Mock<PiWorkClient["saveModelConfiguration"]> = vi.fn(async (input) => ({
    provider: input.provider,
    modelId: input.modelId,
  }));

  const createWork = vi.fn(async (input: CreateWorkInput) => {
    const id = `work-${++workSequence}`;
    const summary: WorkSummary = {
      id,
      title: input.title,
      goal: input.goal,
      rootPath: input.rootPath,
      permissionMode: input.permissionMode,
      status: "draft",
      createdAt: now(workSequence),
      updatedAt: now(workSequence),
    };
    const detail: WorkDetail = { summary, runs: [], messages: [], events: [] };
    details.set(id, detail);
    if (input.resourceDraftId) {
      const drafted = resourcesByDraft.get(input.resourceDraftId) ?? [];
      resourcesByWork.set(id, [...drafted]);
      resourcesByDraft.delete(input.resourceDraftId);
    }
    return detail;
  });
  const listWorks = vi.fn(async () =>
    [...details.values()].map(({ summary }) => summary),
  );
  const getWork = vi.fn(async (workId: string) => {
    const detail = details.get(workId);
    if (!detail) throw new Error(`Work not found: ${workId}`);
    return detail;
  });
  const listProjectFiles: Mock<PiWorkClient["listProjectFiles"]> = vi.fn(
    async (_rootPath: string) => [],
  );
  const importResources: Mock<PiWorkClient["importResources"]> = vi.fn(
    async (input) => {
      const imported = input.sourcePaths.map((sourcePath) => {
        const originalName = sourcePath.split(/[\\/]/u).at(-1) || "attachment";
        const id = `resource-${++resourceSequence}`;
        const summary: ResourceSummary = {
          id,
          originalName,
          mediaType: "image/png",
          size: 68n,
          origin: "user_upload",
          status: "ready",
          failureCode: null,
          createdAt: now(30 + resourceSequence),
        };
        thumbnailByResource.set(id, {
          mediaType: "image/png",
          dataBase64: "iVBORw0KGgo=",
        });
        return summary;
      });
      if (input.workId) {
        resourcesByWork.set(input.workId, [
          ...(resourcesByWork.get(input.workId) ?? []),
          ...imported,
        ]);
      } else if (input.draftId) {
        resourcesByDraft.set(input.draftId, [
          ...(resourcesByDraft.get(input.draftId) ?? []),
          ...imported,
        ]);
      }
      return imported;
    },
  );
  const listWorkResources: Mock<PiWorkClient["listWorkResources"]> = vi.fn(
    async (workId) => [...(resourcesByWork.get(workId) ?? [])],
  );
  const getResourceThumbnail: Mock<PiWorkClient["getResourceThumbnail"]> = vi.fn(
    async (resourceId) =>
      thumbnailByResource.get(resourceId) ?? {
        mediaType: "image/png",
        dataBase64: "",
      },
  );
  const detachDraftResource: Mock<PiWorkClient["detachDraftResource"]> = vi.fn(
    async (draftId, resourceId) => {
      resourcesByDraft.set(
        draftId,
        (resourcesByDraft.get(draftId) ?? []).filter(({ id }) => id !== resourceId),
      );
    },
  );
  const startWork = vi.fn(
    async (
      workId: string,
      prompt: string,
      _referencedFiles: string[] = [],
      resourceIds: string[] = [],
    ) => {
    const detail = details.get(workId);
    if (!detail) throw new Error(`Work not found: ${workId}`);
    const id = `run-${++runSequence}`;
    const run: RunSummary = {
      id,
      workId,
      engineKind: "fake",
      engineSessionId: `session-${runSequence}`,
      modelLabel: "Fake model",
      status: "running",
      createdAt: now(10 + runSequence),
      startedAt: now(10 + runSequence),
      completedAt: null,
    };
    const userMessage: MessageSummary = {
      id: `message-${runSequence}`,
      workId,
      runId: id,
      role: "user",
      content: prompt.trim(),
      resourceIds: [...resourceIds],
      createdAt: run.createdAt,
    };
    detail.runs.push(run);
    detail.messages.push(userMessage);
    detail.summary.status = "running";
    detail.summary.updatedAt = run.createdAt;
      return { run, userMessage } satisfies StartWorkOutput;
    },
  );
  const listenToWorkEvents = vi.fn(
    async (handler: (event: WorkEventEnvelope) => void) => {
      handlers.add(handler);
      return unlisten;
    },
  );

  const client: MockTauriClient = {
    getModelConfigurationStatus,
    testModelConnection,
    saveModelConfiguration,
    createWork,
    listWorks,
    getWork,
    listProjectFiles,
    importResources,
    listWorkResources,
    getResourceThumbnail,
    detachDraftResource,
    startWork,
    listenToWorkEvents,
    unlisten,
    seed(detail) {
      details.set(detail.summary.id, detail);
      const numericId = Number(detail.summary.id.split("-").at(-1));
      if (Number.isFinite(numericId)) workSequence = Math.max(workSequence, numericId);
    },
    seedResource(workId, resource) {
      const existing = resourcesByWork.get(workId) ?? [];
      resourcesByWork.set(workId, [
        ...existing.filter(({ id }) => id !== resource.id),
        resource,
      ]);
      resourceSequence = Math.max(
        resourceSequence,
        Number(resource.id.split("-").at(-1)) || 0,
      );
    },
    emit(event) {
      const detail = details.get(event.workId);
      if (detail) {
        detail.events.push(event);
        detail.summary.updatedAt = event.occurredAt;
        if (event.payload.type === "runStarted") detail.summary.status = "running";
        if (event.payload.type === "runCompleted") detail.summary.status = "completed";
        if (event.payload.type === "runFailed") detail.summary.status = "failed";
        const run = detail.runs.find(({ id }) => id === event.runId);
        if (run && event.payload.type === "runCompleted") {
          run.status = "completed";
          run.completedAt = event.occurredAt;
        }
        if (run && event.payload.type === "runFailed") {
          run.status = "failed";
          run.completedAt = event.occurredAt;
        }
      }
      handlers.forEach((handler) => handler(event));
    },
  };
  return client;
};
