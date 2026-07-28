import { vi, type Mock } from "vitest";

import type { PiWorkClient } from "../app/tauriClient";
import type {
  CreateWorkInput,
  RunSummary,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
} from "../bindings";

export type MockTauriClient = PiWorkClient & {
  createWork: Mock<PiWorkClient["createWork"]>;
  listWorks: Mock<PiWorkClient["listWorks"]>;
  getWork: Mock<PiWorkClient["getWork"]>;
  startWork: Mock<PiWorkClient["startWork"]>;
  listenToWorkEvents: Mock<PiWorkClient["listenToWorkEvents"]>;
  emit(event: WorkEventEnvelope): void;
  seed(detail: WorkDetail): void;
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
  const unlisten = vi.fn(() => handlers.clear());

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
    const detail: WorkDetail = { summary, runs: [], events: [] };
    details.set(id, detail);
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
  const startWork = vi.fn(async (workId: string, _prompt: string) => {
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
    detail.runs.push(run);
    detail.summary.status = "running";
    detail.summary.updatedAt = run.createdAt;
    return run;
  });
  const listenToWorkEvents = vi.fn(
    async (handler: (event: WorkEventEnvelope) => void) => {
      handlers.add(handler);
      return unlisten;
    },
  );

  const client: MockTauriClient = {
    createWork,
    listWorks,
    getWork,
    startWork,
    listenToWorkEvents,
    unlisten,
    seed(detail) {
      details.set(detail.summary.id, detail);
      const numericId = Number(detail.summary.id.split("-").at(-1));
      if (Number.isFinite(numericId)) workSequence = Math.max(workSequence, numericId);
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
