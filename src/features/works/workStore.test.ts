import { describe, expect, it, vi } from "vitest";

import type { PiWorkClient } from "../../app/tauriClient";
import type {
  CreateWorkInput,
  MessageSummary,
  ResourceSummary,
  RunSummary,
  StartWorkOutput,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
} from "../../bindings";
import { isWorkEventTimelineItem } from "../../domain/work";
import {
  assignmentSummary,
  createMockTauriClient,
} from "../../test/mockTauriClient";
import {
  createWorkStore,
  didPersistStartInstruction,
} from "./workStore";

const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, reject, resolve };
};

const work: WorkSummary = {
  id: "w1",
  workspaceId: "workspace-w1",
  title: "Build the store",
  goal: "Replay events",
  rootPath: "D:/dev/PiWork",
  permissionMode: "balanced",
  status: "draft",
  createdAt: "2026-07-28T09:00:00.000Z",
  updatedAt: "2026-07-28T09:00:00.000Z",
};

const event = (
  sequence: number,
  payload: WorkEventEnvelope["payload"],
): WorkEventEnvelope => ({
  version: 2,
  eventId: `event-r1-${sequence}`,
  workId: "w1",
  runId: "r1",
  turnId: "r1",
  correlationId: "r1",
  sequence,
  occurredAt: `2026-07-28T09:00:0${sequence}.000Z`,
  payload,
});

const legacyEvent = (
  sequence: number,
  payload: WorkEventEnvelope["payload"],
  overrides: Partial<WorkEventEnvelope> = {},
): WorkEventEnvelope => {
  const {
    eventId: _eventId,
    turnId: _turnId,
    correlationId: _correlationId,
    ...legacy
  } = event(sequence, payload);
  return { ...legacy, version: 1, ...overrides };
};

const unusedClient: PiWorkClient = {
  getModelConfigurationStatus: async () => ({ configured: true, configuration: { id: "openai-default", provider: "openai", baseUrl: "https://api.openai.com/v1", modelId: "gpt-5.2", active: true, credentialConfigured: true } }),
  listModelConfigurations: async () => [],
  testModelConnection: async () => ({ models: [] }),
  testSavedModelConfiguration: async () => ({ models: [] }),
  saveModelConfiguration: async (input) => ({ id: input.id ?? "model", provider: input.provider, baseUrl: input.baseUrl, modelId: input.modelId, active: true, credentialConfigured: true }),
  activateModelConfiguration: async () => { throw new Error("unused"); },
  selectModelForConfiguration: async () => { throw new Error("unused"); },
  createWork: async () => {
    throw new Error("unused");
  },
  listWorks: async () => [],
  getWork: async () => {
    throw new Error("unused");
  },
  listAgentInstances: vi.fn(async () => { throw new Error("unused"); }),
  listCapabilityPacks: vi.fn(async () => { throw new Error("unused"); }),
  getWorkTeam: vi.fn(async () => { throw new Error("unused"); }),
  validateAgentAssembly: vi.fn(async () => { throw new Error("unused"); }),
  saveAgentCopy: vi.fn(async () => { throw new Error("unused"); }),
  addWorkMember: vi.fn(async () => { throw new Error("unused"); }),
  listProjectFiles: async () => [],
  importResources: async () => [],
  listWorkResources: async () => [],
  getResourceThumbnail: async () => ({ mediaType: "image/png", dataBase64: "" }),
  detachDraftResource: async () => undefined,
  startWork: async () => {
    throw new Error("unused");
  },
  stopWork: async () => {
    throw new Error("unused");
  },
  archiveWork: async () => {
    throw new Error("unused");
  },
  restoreWork: async () => {
    throw new Error("unused");
  },
  drainAssignmentEventOutbox: async () => undefined,
  listWorkAssignments: async () => [],
  queueWorkInput: async () => {
    throw new Error("unused");
  },
  confirmAssignmentRecovery: async () => {
    throw new Error("unused");
  },
  interruptAndReplace: async () => {
    throw new Error("unused");
  },
  listMemoryCandidates: async () => {
    throw new Error("unused");
  },
  resolveMemoryCandidate: async () => {
    throw new Error("unused");
  },
  listenToWorkEvents: async () => () => undefined,
};

const run: RunSummary = {
  id: "r1",
  workId: "w1",
  assignmentId: "assignment-1",
  agentInstanceId: "agent-1",
  engineKind: "codex",
  engineSessionId: null,
  modelLabel: "gpt-5",
  status: "running",
  createdAt: "2026-07-28T09:00:01.000Z",
  startedAt: "2026-07-28T09:00:01.000Z",
  completedAt: null,
};

const userMessage = (
  overrides: Partial<MessageSummary> = {},
): MessageSummary => ({
  id: "m1",
  workId: "w1",
  runId: "r1",
  role: "user",
  content: "Ship it",
  resourceIds: [],
  createdAt: "2026-07-28T09:00:01.000Z",
  ...overrides,
});

const resource = (
  overrides: Partial<ResourceSummary> = {},
): ResourceSummary => ({
  id: "resource-1",
  originalName: "chart.png",
  mediaType: "image/png",
  size: 68n,
  origin: "user_upload",
  status: "ready",
  failureCode: null,
  createdAt: "2026-07-28T09:00:00.000Z",
  ...overrides,
});

const workDetail = (): WorkDetail => ({
  summary: work,
  runs: [],
  messages: [],
  events: [],
});

describe("createWorkStore", () => {
  it("clears the running sidebar state when a Run completes", () => {
    const store = createWorkStore(createMockTauriClient());
    store.getState().upsertWork({
      ...work,
      status: "running",
      updatedAt: "2026-07-28T09:00:01.000Z",
    });
    store.getState().applyEvent(event(1, { type: "runStarted", modelLabel: "gpt-5" }));

    store.getState().applyEvent(event(2, {
      type: "runCompleted",
      summary: "本轮对话已结束。",
      artifacts: [],
      validation: [],
      limitations: [],
    }));

    expect(store.getState().works.w1?.status).toBe("idle");
  });

  it("clears the running sidebar state when Work delivery completes", () => {
    const store = createWorkStore(createMockTauriClient());
    store.getState().upsertWork({
      ...work,
      status: "running",
      updatedAt: "2026-07-28T09:00:01.000Z",
    });
    store.getState().applyEvent(event(1, { type: "runStarted", modelLabel: "gpt-5" }));

    store.getState().applyEvent(event(2, {
      type: "workDeliveryCompleted",
      summary: "已完成并交付。",
      artifacts: [],
      validation: [],
      limitations: [],
    }));

    expect(store.getState().works.w1?.status).toBe("completed");
  });

  it("projects a Run waiting signal into a stable Work status", () => {
    const store = createWorkStore(createMockTauriClient());
    store.getState().upsertWork({
      ...work,
      status: "running",
      updatedAt: "2026-07-28T09:00:01.000Z",
    });
    store.getState().applyEvent(event(1, { type: "runStarted", modelLabel: "gpt-5" }));

    store.getState().applyEvent(event(2, {
      type: "waiting",
      reason: "waiting_on_assignments",
    }));

    expect(store.getState().works.w1?.status).toBe("waiting");
  });

  it("keeps mock legacy start output non-durable and opaque", async () => {
    const client = createMockTauriClient();
    client.seed(workDetail());

    const output = await client.startWork("w1", "Investigate");

    expect(output.assignment).toMatchObject({
      id: `legacy-run:${output.run?.id}`,
      workId: "w1",
      status: "running",
      contextManifest: null,
      expectedResultSchema: null,
      acceptanceCriteria: null,
      permissionScope: null,
    });
    expect(output.run).toMatchObject({
      assignmentId: null,
      agentInstanceId: null,
    });
    expect((await client.getWork("w1")).events).toEqual([]);
  });

  it("imports Work resources and keeps failed files isolated", async () => {
    const client = createMockTauriClient();
    client.seed(workDetail());
    client.importResources.mockResolvedValueOnce([
      resource({ id: "resource-ready", originalName: "ready.png" }),
      resource({
        id: "resource-failed",
        originalName: "broken.png",
        status: "failed",
        failureCode: "unsupported_image",
      }),
    ]);
    const store = createWorkStore(client);

    const imported = await store.getState().importResources({
      sourcePaths: ["C:/private/ready.png", "C:/private/broken.png"],
      workId: "w1",
      draftId: null,
    });

    expect(imported).toHaveLength(2);
    expect(store.getState().resources.w1).toEqual(imported);
    expect(store.getState().error).toBeNull();
  });

  it("copies resource ids when an instruction is queued", () => {
    const store = createWorkStore(createMockTauriClient());
    const resourceIds = ["resource-1"];

    store.getState().queueInstruction("w1", "Compare", [], resourceIds);
    resourceIds.push("resource-2");

    expect(store.getState().queuedInstructions.w1).toEqual([
      {
        prompt: "Compare",
        referencedFiles: [],
        resourceIds: ["resource-1"],
      },
    ]);
  });

  it("hydrates Work resources beside the Work detail", async () => {
    const client = createMockTauriClient();
    client.seed(workDetail());
    client.listWorkResources.mockResolvedValueOnce([
      resource({ id: "resource-1", originalName: "chart.png" }),
    ]);
    const store = createWorkStore(client);

    await store.getState().hydrate();

    expect(client.listWorkResources).toHaveBeenCalledWith("w1");
    expect(store.getState().resources.w1?.[0]?.id).toBe("resource-1");
  });

  it("queues trimmed instructions per Work without creating events", () => {
    const store = createWorkStore(unusedClient);

    store.getState().queueInstruction("w1", "  Follow up  ", ["src/context.ts"]);
    store.getState().queueInstruction("w1", "   ");
    store.getState().queueInstruction("w2", "Second Work");

    expect(store.getState().queuedInstructions).toEqual({
      w1: [
        {
          prompt: "Follow up",
          referencedFiles: ["src/context.ts"],
          resourceIds: [],
        },
      ],
      w2: [{ prompt: "Second Work", referencedFiles: [], resourceIds: [] }],
    });
    expect(store.getState().timelines).toEqual({});
  });

  it("ignores a duplicate sequence for the same run", () => {
    const store = createWorkStore(unusedClient);

    store.getState().upsertWork(work);
    store
      .getState()
      .applyEvent(event(1, { type: "runStarted", modelLabel: "gpt-5" }));
    store
      .getState()
      .applyEvent(event(2, { type: "assistantDelta", text: "hello" }));
    store
      .getState()
      .applyEvent(event(2, { type: "assistantDelta", text: "duplicate" }));

    expect(store.getState().timelines.w1).toHaveLength(2);
    expect(store.getState().lastSequenceByRun.r1).toBe(2);
  });

  it("keeps pre-Run events for distinct assignments without marking the Work running", () => {
    const store = createWorkStore(unusedClient);
    const assignmentEvent = (assignmentId: string): WorkEventEnvelope => ({
      version: 2,
      workId: "w1",
      runId: null,
      assignmentId,
      sequence: 1,
      occurredAt: "2026-08-15T04:00:00.000Z",
      payload: {
        type: "assignmentQueued",
        assignmentId,
        assignedAgentId: "agent-1",
        title: "Investigate",
        priority: 10,
      },
    });

    store.getState().upsertWork({ ...work, status: "draft" });
    store.getState().applyEvent(assignmentEvent("assignment-1"));
    store.getState().applyEvent(assignmentEvent("assignment-2"));

    expect(store.getState().timelines.w1).toHaveLength(2);
    expect(store.getState().works.w1?.status).toBe("draft");
  });

  it("keeps a pre-Run segment and every retry attempt on independent cursors", () => {
    const store = createWorkStore(unusedClient);
    const attemptEvent = (
      runId: string | null,
      sequence: number,
      occurredSecond: number,
      payload: WorkEventEnvelope["payload"],
    ): WorkEventEnvelope => ({
      version: 2,
      eventId: `event-${runId ?? "queued"}-${sequence}`,
      workId: "w1",
      runId,
      assignmentId: "assignment-1",
      turnId: runId ?? undefined,
      sequence,
      occurredAt: `2026-08-15T04:00:0${occurredSecond}.000Z`,
      payload,
    });
    const queued = attemptEvent(null, 1, 0, {
      type: "assignmentQueued",
      assignmentId: "assignment-1",
      assignedAgentId: "agent-1",
      title: "Investigate",
      priority: 10,
    });
    const runOneStarted = attemptEvent("r1", 1, 1, {
      type: "runStarted",
      modelLabel: "gpt-5",
    });
    const runOneOutput = attemptEvent("r1", 2, 2, {
      type: "assistantDelta",
      text: "first attempt",
    });
    const runTwoStarted = attemptEvent("r2", 1, 3, {
      type: "runStarted",
      modelLabel: "gpt-5",
    });
    const runTwoOutput = attemptEvent("r2", 2, 4, {
      type: "assistantDelta",
      text: "retry attempt",
    });

    store.getState().upsertWork({ ...work, status: "draft" });
    store.getState().applyEvent(queued);
    store.getState().applyEvent(runOneStarted);
    expect.soft(store.getState().works.w1?.status).toBe("running");
    store.getState().applyEvent(runOneOutput);
    store.getState().applyEvent(runTwoStarted);
    store.getState().applyEvent(runTwoOutput);

    const retained = store.getState().timelines.w1?.filter(isWorkEventTimelineItem);
    expect.soft(retained).toHaveLength(5);
    expect.soft(retained?.map(({ runId, sequence }) => [runId, sequence])).toEqual([
      [null, 1],
      ["r1", 1],
      ["r1", 2],
      ["r2", 1],
      ["r2", 2],
    ]);
    expect(store.getState().lastSequenceByRun).toMatchObject({
      "assignment:assignment-1": 1,
      r1: 2,
      r2: 2,
    });
  });

  it("keeps the live envelope when an older detail hydrates the same event", async () => {
    const detailResult = deferred<WorkDetail>();
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [work],
      getWork: () => detailResult.promise,
    };
    const store = createWorkStore(client);
    const hydration = store.getState().hydrate();
    await Promise.resolve();
    const earlierEvent = {
      ...event(1, { type: "runStarted", modelLabel: "gpt-5" }),
      eventId: "event-earlier",
    };
    const liveEvent = {
      ...event(2, { type: "assistantDelta", text: "live committed envelope" }),
      eventId: "event-shared",
    };
    const persistedSnapshot = {
      ...liveEvent,
      payload: {
        type: "assistantDelta" as const,
        text: "older persisted snapshot",
      },
    };

    store.getState().applyEvent(liveEvent);
    detailResult.resolve({
      summary: work,
      runs: [run],
      messages: [],
      events: [persistedSnapshot, earlierEvent],
    });
    await hydration;

    expect(store.getState().timelines.w1).toEqual([earlierEvent, liveEvent]);
    expect(store.getState().timelines.w1?.[1]).toMatchObject({
      eventId: "event-shared",
      payload: { type: "assistantDelta", text: "live committed envelope" },
    });
  });

  it("keeps the hydrated envelope when the same live frame arrives later", async () => {
    const persistedEvent = {
      ...event(1, {
        type: "assistantDelta",
        text: "persisted authoritative envelope",
      }),
      eventId: "event-shared",
    };
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [work],
      getWork: async () => ({
        summary: work,
        runs: [run],
        messages: [],
        events: [persistedEvent],
      }),
    };
    const store = createWorkStore(client);

    await store.getState().hydrate();
    store.getState().applyEvent({
      ...persistedEvent,
      payload: { type: "assistantDelta", text: "duplicate live envelope" },
    });

    expect(store.getState().timelines.w1).toEqual([persistedEvent]);
    expect(store.getState().timelines.w1?.[0]).toMatchObject({
      eventId: "event-shared",
      payload: {
        type: "assistantDelta",
        text: "persisted authoritative envelope",
      },
    });
    expect(store.getState().lastSequenceByRun.r1).toBe(1);
  });

  it("uses a hydration-only watermark to reject an older distinct live event", async () => {
    const persistedEvent = {
      ...event(3, { type: "assistantDelta", text: "persisted sequence 3" }),
      eventId: "event-persisted-3",
    };
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [work],
      getWork: async () => ({
        summary: work,
        runs: [run],
        messages: [],
        events: [persistedEvent],
      }),
    };
    const store = createWorkStore(client);

    await store.getState().hydrate();
    store.getState().applyEvent({
      ...event(2, { type: "assistantDelta", text: "stale sequence 2" }),
      eventId: "event-stale-2",
    });

    expect(store.getState().timelines.w1).toEqual([persistedEvent]);
    expect(store.getState().lastSequenceByRun.r1).toBe(3);
  });

  it("keeps distinct stable events even when their timestamps and sequences match", async () => {
    const first = {
      ...event(1, { type: "assistantDelta", text: "first" }),
      eventId: "event-first",
    };
    const second = {
      ...event(1, { type: "assistantDelta", text: "second" }),
      eventId: "event-second",
    };
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [work],
      getWork: async () => ({
        summary: work,
        runs: [run],
        messages: [],
        events: [second, first],
      }),
    };
    const store = createWorkStore(client);

    await store.getState().hydrate();

    expect(
      store
        .getState()
        .timelines.w1?.filter(isWorkEventTimelineItem)
        .map(({ eventId }) => eventId),
    ).toEqual(["event-first", "event-second"]);
  });

  it("deduplicates legacy run sequences without colliding across runs", async () => {
    const firstRun = legacyEvent(1, {
      type: "assistantDelta",
      text: "first run",
    });
    const duplicate = legacyEvent(1, {
      type: "assistantDelta",
      text: "duplicate first run",
    });
    const secondRun = legacyEvent(
      1,
      { type: "assistantDelta", text: "second run" },
      {
        runId: "r2",
        occurredAt: "2026-07-28T09:00:02.000Z",
      },
    );
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [work],
      getWork: async () => ({
        summary: work,
        runs: [run, { ...run, id: "r2" }],
        messages: [],
        events: [firstRun, duplicate, secondRun],
      }),
    };
    const store = createWorkStore(client);

    await store.getState().hydrate();

    expect(
      store
        .getState()
        .timelines.w1?.filter(isWorkEventTimelineItem)
        .map(({ runId, sequence }) => `${runId}:${sequence}`),
    ).toEqual(["r1:1", "r2:1"]);
  });

  it("hydrates the newest work and replays its persisted events in order", async () => {
    const older: WorkSummary = {
      ...work,
      id: "older",
      updatedAt: "2026-07-28T08:00:00.000Z",
    };
    const newest: WorkSummary = {
      ...work,
      updatedAt: "2026-07-28T10:00:00.000Z",
    };
    const detail: WorkDetail = {
      summary: newest,
      runs: [run],
      messages: [],
      events: [
        event(2, { type: "assistantDelta", text: "second" }),
        event(1, { type: "runStarted", modelLabel: "gpt-5" }),
      ],
    };
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [older, newest],
      getWork: async () => detail,
    };
    const store = createWorkStore(client);

    await store.getState().hydrate();

    expect(store.getState().selectedWorkId).toBe("w1");
    expect(
      store.getState().timelines.w1?.filter(isWorkEventTimelineItem).map(({ sequence }) => sequence),
    ).toEqual([1, 2]);
  });

  it("retains the latest Run summary for workspace metadata", async () => {
    const newerRun: RunSummary = {
      ...run,
      id: "r2",
      modelLabel: "new-model",
      createdAt: "2026-07-28T09:00:02.000Z",
    };
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [work],
      getWork: async () => ({
        summary: work,
        runs: [newerRun, run],
        messages: [],
        events: [],
      }),
    };
    const store = createWorkStore(client);

    await store.getState().hydrate();

    expect(store.getState().latestRuns.w1).toEqual(newerRun);
  });

  it("does not let Run completion claim Work completion over persisted state", async () => {
    const staleSummary: WorkSummary = {
      ...work,
      status: "running",
      updatedAt: "2026-07-28T10:00:00.000Z",
    };
    const detailResult = deferred<WorkDetail>();
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [staleSummary],
      getWork: () => detailResult.promise,
    };
    const store = createWorkStore(client);
    const hydration = store.getState().hydrate();
    await Promise.resolve();

    store.getState().applyEvent({
      ...event(1, {
        type: "runCompleted",
        summary: "done",
        artifacts: [],
        validation: [],
        limitations: [],
      }),
      occurredAt: "2026-07-28T11:00:00.000Z",
    });
    detailResult.resolve({ summary: staleSummary, runs: [run], messages: [], events: [] });
    await hydration;

    expect(store.getState().works.w1).toMatchObject({
      status: "idle",
      updatedAt: "2026-07-28T11:00:00.000Z",
    });
  });

  it("hydrates persisted history that arrived before a live run watermark", async () => {
    const detailResult = deferred<WorkDetail>();
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [work],
      getWork: () => detailResult.promise,
    };
    const store = createWorkStore(client);
    const hydration = store.getState().hydrate();
    await Promise.resolve();

    store.getState().applyEvent(
      event(3, {
        type: "runCompleted",
        summary: "live completion",
        artifacts: [],
        validation: [],
        limitations: [],
      }),
    );
    detailResult.resolve({
      summary: work,
      runs: [run],
      messages: [],
      events: [
        event(1, { type: "runStarted", modelLabel: "gpt-5" }),
        event(2, { type: "runFailed", message: "persisted failure" }),
      ],
    });
    await hydration;

    store.getState().applyEvent(
      event(2, { type: "assistantDelta", text: "stale live frame" }),
    );

    expect(
      store.getState().timelines.w1?.filter(isWorkEventTimelineItem).map(({ sequence }) => sequence),
    ).toEqual([1, 2, 3]);
    expect(
      store
        .getState()
        .timelines.w1?.filter(isWorkEventTimelineItem)
        .some(
          ({ payload }) =>
            payload.type === "assistantDelta" &&
            payload.text === "stale live frame",
        ),
    ).toBe(false);
    expect(store.getState().lastSequenceByRun.r1).toBe(3);
    expect(store.getState().works.w1).toMatchObject({
      status: "idle",
      updatedAt: "2026-07-28T09:00:03.000Z",
    });
  });

  it("ignores a stale detail response after selecting another work", async () => {
    const firstResult = deferred<WorkDetail>();
    const secondResult = deferred<WorkDetail>();
    const requestedIds: string[] = [];
    const secondWork: WorkSummary = { ...work, id: "w2" };
    const client: PiWorkClient = {
      ...unusedClient,
      getWork: (workId) => {
        requestedIds.push(workId);
        return workId === "w1" ? firstResult.promise : secondResult.promise;
      },
    };
    const store = createWorkStore(client);
    store.getState().upsertWork(work);
    store.getState().upsertWork(secondWork);

    store.getState().selectWork("w1");
    store.getState().selectWork("w2");
    secondResult.resolve({
      summary: secondWork,
      runs: [{ ...run, id: "r2", workId: "w2" }],
      messages: [],
      events: [
        {
          ...event(1, { type: "runStarted", modelLabel: "gpt-5" }),
          eventId: "event-r2-1",
          workId: "w2",
          runId: "r2",
          turnId: "r2",
          correlationId: "r2",
        },
      ],
    });
    await Promise.resolve();
    firstResult.resolve({
      summary: work,
      runs: [run],
      messages: [],
      events: [event(1, { type: "runStarted", modelLabel: "gpt-5" })],
    });
    await Promise.resolve();

    expect(requestedIds).toEqual(["w1", "w2"]);
    expect(store.getState().selectedWorkId).toBe("w2");
    expect(store.getState().timelines.w2).toHaveLength(1);
    expect(store.getState().timelines.w1).toBeUndefined();
    expect(store.getState().loading).toBe(false);
  });

  it("selects a work without losing persisted history behind a live watermark", async () => {
    const detailResult = deferred<WorkDetail>();
    const client: PiWorkClient = {
      ...unusedClient,
      getWork: () => detailResult.promise,
    };
    const store = createWorkStore(client);
    store.getState().upsertWork(work);

    store.getState().selectWork("w1");
    store.getState().applyEvent(
      event(3, {
        type: "runCompleted",
        summary: "live completion",
        artifacts: [],
        validation: [],
        limitations: [],
      }),
    );
    detailResult.resolve({
      summary: work,
      runs: [run],
      messages: [],
      events: [
        event(1, { type: "runStarted", modelLabel: "gpt-5" }),
        event(2, { type: "runFailed", message: "persisted failure" }),
        event(3, {
          type: "runCompleted",
          summary: "live completion",
          artifacts: [],
          validation: [],
          limitations: [],
        }),
      ],
    });
    await Promise.resolve();

    expect(
      store.getState().timelines.w1?.filter(isWorkEventTimelineItem).map(({ sequence }) => sequence),
    ).toEqual([1, 2, 3]);
    expect(store.getState().lastSequenceByRun.r1).toBe(3);
    expect(store.getState().works.w1).toMatchObject({
      status: "idle",
      updatedAt: "2026-07-28T09:00:03.000Z",
    });
  });

  it("keeps the current selection loading when an older hydrate resolves", async () => {
    const hydrateResult = deferred<WorkDetail>();
    const selectionResult = deferred<WorkDetail>();
    const secondWork: WorkSummary = { ...work, id: "w2" };
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [work, secondWork],
      getWork: (workId) =>
        workId === "w1" ? hydrateResult.promise : selectionResult.promise,
    };
    const store = createWorkStore(client);
    const hydration = store.getState().hydrate();
    await Promise.resolve();

    store.getState().selectWork("w2");
    hydrateResult.resolve({
      summary: work,
      runs: [run],
      messages: [],
      events: [event(1, { type: "runStarted", modelLabel: "gpt-5" })],
    });
    await hydration;

    expect(store.getState().selectedWorkId).toBe("w2");
    expect(store.getState().timelines.w1).toBeUndefined();
    expect(store.getState().loading).toBe(true);

    selectionResult.resolve({ summary: secondWork, runs: [], messages: [], events: [] });
    await Promise.resolve();
    expect(store.getState().loading).toBe(false);
  });

  it("creates, selects, and replays the returned work detail", async () => {
    const input: CreateWorkInput = {
      title: work.title,
      goal: work.goal,
      rootPath: work.rootPath,
      permissionMode: work.permissionMode,
      resourceDraftId: null,
    };
    const detail: WorkDetail = {
      summary: work,
      runs: [run],
      messages: [],
      events: [event(1, { type: "runStarted", modelLabel: "gpt-5" })],
    };
    const received: CreateWorkInput[] = [];
    const client: PiWorkClient = {
      ...unusedClient,
      createWork: async (createInput) => {
        received.push(createInput);
        return detail;
      },
    };
    const store = createWorkStore(client);

    const result = await store.getState().createWork(input);

    expect(received).toEqual([input]);
    expect(result).toBe(detail);
    expect(store.getState().selectedWorkId).toBe("w1");
    expect(store.getState().works.w1).toEqual({
      ...work,
      status: "running",
      updatedAt: "2026-07-28T09:00:01.000Z",
    });
    expect(store.getState().timelines.w1).toHaveLength(1);
  });

  it("starts a work and immediately adds the authoritative user message", async () => {
    const calls: Array<{ workId: string; prompt: string }> = [];
    const queuedRun: RunSummary = {
      ...run,
      status: "queued",
      startedAt: null,
    };
    const output: StartWorkOutput = {
      assignment: assignmentSummary({ id: "assignment-1", workId: "w1" }),
      run: queuedRun,
      userMessage: userMessage(),
    };
    const client: PiWorkClient = {
      ...unusedClient,
      startWork: async (workId, prompt) => {
        calls.push({ workId, prompt });
        return output;
      },
    };
    const store = createWorkStore(client);
    store.getState().upsertWork(work);

    const result = await store.getState().startWork("w1", "Ship it");

    expect(calls).toEqual([{ workId: "w1", prompt: "Ship it" }]);
    expect(result).toBe(output);
    expect(store.getState().works.w1).toMatchObject({
      status: "running",
      updatedAt: queuedRun.createdAt,
    });
    expect(store.getState().timelines.w1).toEqual([output.userMessage]);
  });

  it("shows a queued assignment message before the Scheduler creates its Run", async () => {
    const output: StartWorkOutput = {
      assignment: assignmentSummary({
        id: "assignment-queued",
        workId: "w1",
        status: "queued",
        startedAt: null,
      }),
      run: null,
      userMessage: {
        id: "message-queued",
        workId: "w1",
        assignmentId: "assignment-queued",
        role: "user",
        content: "Queue it",
        resourceIds: [],
        createdAt: "2026-07-28T09:00:01.000Z",
      },
    };
    const client: PiWorkClient = {
      ...unusedClient,
      startWork: async () => output,
    };
    const store = createWorkStore(client);
    store.getState().upsertWork(work);

    await store.getState().startWork("w1", "Queue it");

    expect(store.getState().works.w1).toEqual(work);
    expect(store.getState().latestRuns.w1).toBeUndefined();
    expect(store.getState().timelines.w1).toEqual([{
      id: "message-queued",
      workId: "w1",
      runId: "assignment:assignment-queued",
      role: "user",
      content: "Queue it",
      resourceIds: [],
      createdAt: "2026-07-28T09:00:01.000Z",
    }]);
  });

  it("deduplicates the same authoritative message across live start and hydration", async () => {
    const output: StartWorkOutput = {
      assignment: assignmentSummary({ id: "assignment-1", workId: "w1" }),
      run,
      userMessage: userMessage(),
    };
    const client: PiWorkClient = {
      ...unusedClient,
      startWork: async () => output,
      listWorks: async () => [work],
      getWork: async () => ({
        summary: work,
        runs: [run],
        messages: [userMessage()],
        events: [event(1, { type: "runStarted", modelLabel: "gpt-5" })],
      }),
    };
    const store = createWorkStore(client);
    store.getState().upsertWork(work);

    await store.getState().startWork("w1", "Ship it");
    await store.getState().hydrate();

    const timeline = store.getState().timelines.w1 ?? [];
    expect(timeline.filter((item) => "role" in item)).toEqual([
      output.userMessage,
    ]);
    expect(timeline).toHaveLength(2);
  });

  it("reconciles a persisted failed start by new message ID and keeps the original error", async () => {
    const originalError = {
      code: "engine_start_failed",
      message: "Raw engine startup failure",
      details: { workId: "w1" },
    };
    const previousRun: RunSummary = {
      ...run,
      id: "r0",
      status: "failed",
      completedAt: "2026-07-28T09:00:00.500Z",
    };
    const previousMessage = userMessage({
      id: "m0",
      runId: "r0",
      content: "Repeatable prompt",
      createdAt: previousRun.createdAt,
    });
    const failedRun: RunSummary = {
      ...run,
      status: "failed",
      completedAt: "2026-07-28T09:00:02.000Z",
    };
    const failedMessage = userMessage({ content: "Repeatable prompt" });
    const initialDetail: WorkDetail = {
      summary: { ...work, status: "failed" },
      runs: [previousRun],
      messages: [previousMessage],
      events: [],
    };
    let detail = initialDetail;
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [detail.summary],
      getWork: async () => detail,
      startWork: async () => {
        detail = {
          summary: {
            ...work,
            status: "failed",
            updatedAt: failedRun.completedAt!,
          },
          runs: [previousRun, failedRun],
          messages: [previousMessage, failedMessage],
          events: [],
        };
        throw originalError;
      },
    };
    const store = createWorkStore(client);
    await store.getState().hydrate();

    let rejected: unknown;
    try {
      await store.getState().startWork("w1", "Repeatable prompt");
    } catch (error) {
      rejected = error;
    }

    expect(didPersistStartInstruction(rejected)).toBe(true);
    expect(store.getState().error).toEqual(originalError);
    expect(
      store.getState().timelines.w1?.filter((item) => "role" in item).map(({ id }) => id),
    ).toEqual(["m0", "m1"]);

    await store.getState().hydrate();
    store.getState().applyEvent(event(1, { type: "runStarted", modelLabel: "gpt-5" }));
    expect(
      store.getState().timelines.w1?.filter((item) => "role" in item).map(({ id }) => id),
    ).toEqual(["m0", "m1"]);
  });

  it("does not let a late failed-start refresh change a newer selection intent", async () => {
    const refresh = deferred<WorkDetail>();
    const secondWork: WorkSummary = { ...work, id: "w2", title: "Second" };
    const secondDetail: WorkDetail = {
      summary: secondWork,
      runs: [],
      messages: [],
      events: [],
    };
    const failedDetail: WorkDetail = {
      summary: { ...work, status: "failed", updatedAt: run.createdAt },
      runs: [{ ...run, status: "failed", completedAt: run.createdAt }],
      messages: [userMessage({ content: "Persisted before failure" })],
      events: [],
    };
    let firstWorkReads = 0;
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [work],
      getWork: (workId) => {
        if (workId === "w2") return Promise.resolve(secondDetail);
        firstWorkReads += 1;
        return firstWorkReads === 1
          ? Promise.resolve({ summary: work, runs: [], messages: [], events: [] })
          : refresh.promise;
      },
      startWork: async () => {
        throw { code: "engine_start_failed", message: "original start error" };
      },
    };
    const store = createWorkStore(client);
    await store.getState().hydrate();
    store.getState().upsertWork(secondWork);

    const starting = store.getState().startWork("w1", "Persisted before failure");
    await Promise.resolve();
    store.getState().selectWork("w2");
    await Promise.resolve();
    refresh.resolve(failedDetail);
    await expect(starting).rejects.toSatisfy(didPersistStartInstruction);

    expect(store.getState().selectedWorkId).toBe("w2");
    expect(store.getState().timelines.w1).toContainEqual(failedDetail.messages[0]);
  });

  it("keeps the original start error when failed-start refresh also fails", async () => {
    const originalError = {
      code: "engine_start_failed",
      message: "original start error",
    };
    let refreshCalls = 0;
    const client: PiWorkClient = {
      ...unusedClient,
      getWork: async () => {
        refreshCalls += 1;
        throw { code: "database_error", message: "refresh error" };
      },
      startWork: async () => {
        throw originalError;
      },
    };
    const store = createWorkStore(client);
    store.getState().upsertWork(work);

    await expect(store.getState().startWork("w1", "Try again")).rejects.toBe(
      originalError,
    );

    expect(store.getState().error).toEqual(originalError);
    expect(store.getState().timelines.w1).toBeUndefined();
    expect(refreshCalls).toBe(1);
  });

  it("hydrates two Run prompts in chronological order without engine prompt echoes", async () => {
    const first = userMessage({
      id: "m-first",
      content: "First persisted prompt",
      createdAt: "2026-07-28T09:00:01.000Z",
    });
    const second = userMessage({
      id: "m-second",
      runId: "r2",
      content: "Second persisted prompt",
      createdAt: "2026-07-28T09:00:03.000Z",
    });
    const secondRun: RunSummary = {
      ...run,
      id: "r2",
      createdAt: "2026-07-28T09:00:03.000Z",
      startedAt: "2026-07-28T09:00:03.000Z",
    };
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [work],
      getWork: async () => ({
        summary: work,
        runs: [run, secondRun],
        messages: [second, first],
        events: [
          event(1, { type: "runStarted", modelLabel: "gpt-5" }),
          {
            ...event(1, { type: "runStarted", modelLabel: "gpt-5" }),
            eventId: "event-r2-1",
            runId: "r2",
            turnId: "r2",
            correlationId: "r2",
            occurredAt: "2026-07-28T09:00:04.000Z",
          },
        ],
      }),
    };
    const store = createWorkStore(client);

    await store.getState().hydrate();

    expect(
      store
        .getState()
        .timelines.w1?.filter((item) => "role" in item)
        .map((item) => ("content" in item ? item.content : null)),
    ).toEqual(["First persisted prompt", "Second persisted prompt"]);
  });

  it("does not leak loading after create invalidates a deferred selection", async () => {
    const selectionResult = deferred<WorkDetail>();
    const createdWork: WorkSummary = { ...work, id: "w2" };
    const createdDetail: WorkDetail = {
      summary: createdWork,
      runs: [],
      messages: [],
      events: [],
    };
    let detailCalls = 0;
    const client: PiWorkClient = {
      ...unusedClient,
      createWork: async () => createdDetail,
      listWorks: async () => [createdWork],
      getWork: () => {
        detailCalls += 1;
        return detailCalls === 1
          ? selectionResult.promise
          : Promise.resolve(createdDetail);
      },
    };
    const store = createWorkStore(client);
    store.getState().upsertWork(work);

    store.getState().selectWork("w1");
    await store.getState().createWork({
      title: createdWork.title,
      goal: createdWork.goal,
      rootPath: createdWork.rootPath,
      permissionMode: createdWork.permissionMode,
      resourceDraftId: null,
    });
    expect(store.getState().loading).toBe(true);

    selectionResult.resolve({ summary: work, runs: [], messages: [], events: [] });
    await Promise.resolve();
    await store.getState().hydrate();

    expect(store.getState().selectedWorkId).toBe("w2");
    expect(store.getState().loading).toBe(false);
  });

  it("ignores an old hydrate error after a newer selection succeeds", async () => {
    const hydrateResult = deferred<WorkDetail>();
    const secondWork: WorkSummary = { ...work, id: "w2" };
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [work],
      getWork: (workId) =>
        workId === "w1"
          ? hydrateResult.promise
          : Promise.resolve({ summary: secondWork, runs: [], messages: [], events: [] }),
    };
    const store = createWorkStore(client);
    const hydration = store.getState().hydrate();
    await Promise.resolve();

    store.getState().selectWork("w2");
    await Promise.resolve();
    hydrateResult.reject({ code: "stale", message: "stale hydrate" });
    await hydration;

    expect(store.getState().selectedWorkId).toBe("w2");
    expect(store.getState().error).toBeNull();
    expect(store.getState().loading).toBe(false);
  });

  it("keeps loading until concurrent selection and start operations finish", async () => {
    const selectionResult = deferred<WorkDetail>();
    const startResult = deferred<StartWorkOutput>();
    const client: PiWorkClient = {
      ...unusedClient,
      getWork: () => selectionResult.promise,
      startWork: () => startResult.promise,
    };
    const store = createWorkStore(client);
    store.getState().upsertWork(work);

    store.getState().selectWork("w1");
    const starting = store.getState().startWork("w1", "go");
    startResult.resolve({
      assignment: assignmentSummary({ id: "assignment-1", workId: "w1" }),
      run,
      userMessage: userMessage({ content: "go" }),
    });
    await starting;

    expect(store.getState().loading).toBe(true);

    selectionResult.resolve({ summary: work, runs: [], messages: [], events: [] });
    await Promise.resolve();
    expect(store.getState().loading).toBe(false);
  });

  it("does not let a late start response revive a completed Run as running", async () => {
    const startResult = deferred<StartWorkOutput>();
    const client: PiWorkClient = {
      ...unusedClient,
      startWork: () => startResult.promise,
    };
    const store = createWorkStore(client);
    store.getState().upsertWork(work);

    const starting = store.getState().startWork("w1", "go");
    store.getState().applyEvent(
      event(1, {
        type: "runCompleted",
        summary: "finished first",
        artifacts: [],
        validation: [],
        limitations: [],
      }),
    );
    startResult.resolve({
      assignment: assignmentSummary({ id: "assignment-1", workId: "w1" }),
      run,
      userMessage: userMessage({ content: "go" }),
    });
    await starting;

    expect(store.getState().works.w1).toMatchObject({
      status: "idle",
      updatedAt: "2026-07-28T09:00:01.000Z",
    });
  });

  it("does not let a late event from an old run change the current run", () => {
    const store = createWorkStore(unusedClient);
    store.getState().upsertWork(work);

    store
      .getState()
      .applyEvent(event(1, { type: "runStarted", modelLabel: "old" }));
    store.getState().applyEvent({
      ...event(1, { type: "runStarted", modelLabel: "new" }),
      eventId: "event-r2-1",
      runId: "r2",
      turnId: "r2",
      correlationId: "r2",
      occurredAt: "2026-07-28T09:00:02.000Z",
    });
    store.getState().applyEvent({
      ...event(2, {
        type: "runCompleted",
        summary: "new run done",
        artifacts: [],
        validation: [],
        limitations: [],
      }),
      eventId: "event-r2-2",
      runId: "r2",
      turnId: "r2",
      correlationId: "r2",
      occurredAt: "2026-07-28T09:00:03.000Z",
    });
    store.getState().applyEvent({
      ...event(2, { type: "runFailed", message: "old run was late" }),
      occurredAt: "2026-07-28T09:00:04.000Z",
    });

    expect(store.getState().works.w1).toMatchObject({
      status: "idle",
      updatedAt: "2026-07-28T09:00:03.000Z",
    });
  });

  it("keeps live run order stable before and after detail hydration", async () => {
    const detailResult = deferred<WorkDetail>();
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: async () => [work],
      getWork: () => detailResult.promise,
    };
    const store = createWorkStore(client);
    const hydration = store.getState().hydrate();
    await Promise.resolve();

    store.getState().applyEvent({
      ...event(1, { type: "runStarted", modelLabel: "second" }),
      eventId: "event-r2-1",
      runId: "r2",
      turnId: "r2",
      correlationId: "r2",
      occurredAt: "2026-07-28T09:00:02.000Z",
    });
    store
      .getState()
      .applyEvent(event(1, { type: "runStarted", modelLabel: "first" }));
    const beforeHydration = store
      .getState()
      .timelines.w1?.map(({ runId }) => runId);
    expect(beforeHydration).toEqual(["r1", "r2"]);

    detailResult.resolve({
      summary: work,
      runs: [
        { ...run, createdAt: "2026-07-28T09:00:03.000Z" },
        {
          ...run,
          id: "r2",
          createdAt: "2026-07-28T09:00:00.000Z",
        },
      ],
      messages: [],
      events: [],
    });
    await hydration;

    expect(store.getState().timelines.w1?.map(({ runId }) => runId)).toEqual(
      beforeHydration,
    );
  });

  it("keeps a user selection made after create was invoked", async () => {
    const createResult = deferred<WorkDetail>();
    const createdWork: WorkSummary = { ...work, id: "created" };
    const client: PiWorkClient = {
      ...unusedClient,
      createWork: () => createResult.promise,
      getWork: async () => ({ summary: work, runs: [], messages: [], events: [] }),
    };
    const store = createWorkStore(client);
    const creating = store.getState().createWork({
      title: createdWork.title,
      goal: createdWork.goal,
      rootPath: createdWork.rootPath,
      permissionMode: createdWork.permissionMode,
      resourceDraftId: null,
    });

    store.getState().selectWork("w1");
    await Promise.resolve();
    createResult.resolve({ summary: createdWork, runs: [], messages: [], events: [] });
    await creating;

    expect(store.getState().selectedWorkId).toBe("w1");
    expect(store.getState().works.created).toEqual(createdWork);
  });

  it("selects the later-invoked create when creates resolve in reverse", async () => {
    const firstResult = deferred<WorkDetail>();
    const secondResult = deferred<WorkDetail>();
    const firstWork: WorkSummary = { ...work, id: "first" };
    const secondWork: WorkSummary = { ...work, id: "second" };
    let createCalls = 0;
    const client: PiWorkClient = {
      ...unusedClient,
      createWork: () => {
        createCalls += 1;
        return createCalls === 1 ? firstResult.promise : secondResult.promise;
      },
    };
    const store = createWorkStore(client);
    const input: CreateWorkInput = {
      title: work.title,
      goal: work.goal,
      rootPath: work.rootPath,
      permissionMode: work.permissionMode,
      resourceDraftId: null,
    };

    const firstCreate = store.getState().createWork(input);
    const secondCreate = store.getState().createWork(input);
    secondResult.resolve({ summary: secondWork, runs: [], messages: [], events: [] });
    await secondCreate;
    firstResult.resolve({ summary: firstWork, runs: [], messages: [], events: [] });
    await firstCreate;

    expect(store.getState().selectedWorkId).toBe("second");
    expect(store.getState().works).toMatchObject({
      first: firstWork,
      second: secondWork,
    });
  });

  it("lets a newer create invalidate a pending selection detail", async () => {
    const selectionResult = deferred<WorkDetail>();
    const createdWork: WorkSummary = { ...work, id: "created" };
    const client: PiWorkClient = {
      ...unusedClient,
      createWork: async () => ({
        summary: createdWork,
        runs: [],
        messages: [],
        events: [],
      }),
      getWork: () => selectionResult.promise,
    };
    const store = createWorkStore(client);
    store.getState().upsertWork(work);

    store.getState().selectWork("w1");
    await store.getState().createWork({
      title: createdWork.title,
      goal: createdWork.goal,
      rootPath: createdWork.rootPath,
      permissionMode: createdWork.permissionMode,
      resourceDraftId: null,
    });
    selectionResult.resolve({
      summary: work,
      runs: [run],
      messages: [],
      events: [event(1, { type: "runStarted", modelLabel: "stale" })],
    });
    await Promise.resolve();

    expect(store.getState().selectedWorkId).toBe("created");
    expect(store.getState().timelines.w1).toBeUndefined();
  });

  it("does not auto-select during hydrate after a create intent", async () => {
    const listResult = deferred<WorkSummary[]>();
    const createResult = deferred<WorkDetail>();
    const listedWork: WorkSummary = { ...work, id: "listed" };
    const createdWork: WorkSummary = { ...work, id: "created" };
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: () => listResult.promise,
      createWork: () => createResult.promise,
      getWork: async () => ({ summary: listedWork, runs: [], messages: [], events: [] }),
    };
    const store = createWorkStore(client);
    const hydration = store.getState().hydrate();
    const creating = store.getState().createWork({
      title: createdWork.title,
      goal: createdWork.goal,
      rootPath: createdWork.rootPath,
      permissionMode: createdWork.permissionMode,
      resourceDraftId: null,
    });

    listResult.resolve([listedWork]);
    await hydration;
    expect(store.getState().selectedWorkId).toBeNull();

    createResult.resolve({ summary: createdWork, runs: [], messages: [], events: [] });
    await creating;
    expect(store.getState().selectedWorkId).toBe("created");
  });

  it("does not auto-select over an explicit select during hydrate", async () => {
    const listResult = deferred<WorkSummary[]>();
    const listedWork: WorkSummary = { ...work, id: "listed" };
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: () => listResult.promise,
      getWork: async (workId) => ({
        summary: workId === "w1" ? work : listedWork,
        runs: [],
        messages: [],
        events: [],
      }),
    };
    const store = createWorkStore(client);
    const hydration = store.getState().hydrate();

    store.getState().selectWork("w1");
    listResult.resolve([listedWork]);
    await hydration;

    expect(store.getState().selectedWorkId).toBe("w1");
  });

  it("yields hydrate detail loading to a newer successful selection", async () => {
    const listResult = deferred<WorkSummary[]>();
    const selectionResult = deferred<WorkDetail>();
    let detailCalls = 0;
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: () => listResult.promise,
      getWork: () => {
        detailCalls += 1;
        return selectionResult.promise;
      },
    };
    const store = createWorkStore(client);
    const hydration = store.getState().hydrate();

    store.getState().selectWork("w1");
    listResult.resolve([work]);
    await Promise.resolve();
    selectionResult.resolve({
      summary: work,
      runs: [run],
      messages: [],
      events: [event(1, { type: "runStarted", modelLabel: "selected" })],
    });
    await hydration;
    await Promise.resolve();

    expect(detailCalls).toBe(1);
    expect(store.getState().timelines.w1).toHaveLength(1);
    expect(store.getState().selectedWorkId).toBe("w1");
    expect(store.getState().loading).toBe(false);
  });

  it("does not hide a newer selection error behind hydrate", async () => {
    const listResult = deferred<WorkSummary[]>();
    const selectionResult = deferred<WorkDetail>();
    let detailCalls = 0;
    const client: PiWorkClient = {
      ...unusedClient,
      listWorks: () => listResult.promise,
      getWork: () => {
        detailCalls += 1;
        return selectionResult.promise;
      },
    };
    const store = createWorkStore(client);
    const hydration = store.getState().hydrate();

    store.getState().selectWork("w1");
    listResult.resolve([work]);
    await Promise.resolve();
    selectionResult.reject(new Error("selection detail failed"));
    await hydration;
    await Promise.resolve();

    expect(detailCalls).toBe(1);
    expect(store.getState().error).toEqual({
      code: "unknown",
      message: "selection detail failed",
    });
    expect(store.getState().loading).toBe(false);
  });
});
