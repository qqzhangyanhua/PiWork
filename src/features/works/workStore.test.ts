import { describe, expect, it } from "vitest";

import type { PiWorkClient } from "../../app/tauriClient";
import type {
  CreateWorkInput,
  RunSummary,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
} from "../../bindings";
import { createWorkStore } from "./workStore";

const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
};

const work: WorkSummary = {
  id: "w1",
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
  version: 1,
  workId: "w1",
  runId: "r1",
  sequence,
  occurredAt: `2026-07-28T09:00:0${sequence}.000Z`,
  payload,
});

const unusedClient: PiWorkClient = {
  createWork: async () => {
    throw new Error("unused");
  },
  listWorks: async () => [],
  getWork: async () => {
    throw new Error("unused");
  },
  startWork: async () => {
    throw new Error("unused");
  },
  listenToWorkEvents: async () => () => undefined,
};

const run: RunSummary = {
  id: "r1",
  workId: "w1",
  engineKind: "codex",
  engineSessionId: null,
  modelLabel: "gpt-5",
  status: "running",
  createdAt: "2026-07-28T09:00:01.000Z",
  startedAt: "2026-07-28T09:00:01.000Z",
  completedAt: null,
};

describe("createWorkStore", () => {
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
      store.getState().timelines.w1?.map(({ sequence }) => sequence),
    ).toEqual([1, 2]);
  });

  it("does not let a stale hydrate detail overwrite a newer live event", async () => {
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
    detailResult.resolve({ summary: staleSummary, runs: [run], events: [] });
    await hydration;

    expect(store.getState().works.w1).toMatchObject({
      status: "completed",
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
      events: [
        event(1, { type: "runStarted", modelLabel: "gpt-5" }),
        event(2, { type: "runFailed", message: "persisted failure" }),
      ],
    });
    await hydration;

    expect(
      store.getState().timelines.w1?.map(({ sequence }) => sequence),
    ).toEqual([1, 2, 3]);
    expect(store.getState().lastSequenceByRun.r1).toBe(3);
    expect(store.getState().works.w1).toMatchObject({
      status: "completed",
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
      events: [
        {
          ...event(1, { type: "runStarted", modelLabel: "gpt-5" }),
          workId: "w2",
          runId: "r2",
        },
      ],
    });
    await Promise.resolve();
    firstResult.resolve({
      summary: work,
      runs: [run],
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
      store.getState().timelines.w1?.map(({ sequence }) => sequence),
    ).toEqual([1, 2, 3]);
    expect(store.getState().lastSequenceByRun.r1).toBe(3);
    expect(store.getState().works.w1).toMatchObject({
      status: "completed",
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
      events: [event(1, { type: "runStarted", modelLabel: "gpt-5" })],
    });
    await hydration;

    expect(store.getState().selectedWorkId).toBe("w2");
    expect(store.getState().timelines.w1).toBeUndefined();
    expect(store.getState().loading).toBe(true);

    selectionResult.resolve({ summary: secondWork, runs: [], events: [] });
    await Promise.resolve();
    expect(store.getState().loading).toBe(false);
  });

  it("creates, selects, and replays the returned work detail", async () => {
    const input: CreateWorkInput = {
      title: work.title,
      goal: work.goal,
      rootPath: work.rootPath,
      permissionMode: work.permissionMode,
    };
    const detail: WorkDetail = {
      summary: work,
      runs: [run],
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

  it("starts a work without inventing a timeline event", async () => {
    const calls: Array<{ workId: string; prompt: string }> = [];
    const queuedRun: RunSummary = {
      ...run,
      status: "queued",
      startedAt: null,
    };
    const client: PiWorkClient = {
      ...unusedClient,
      startWork: async (workId, prompt) => {
        calls.push({ workId, prompt });
        return queuedRun;
      },
    };
    const store = createWorkStore(client);
    store.getState().upsertWork(work);

    const result = await store.getState().startWork("w1", "Ship it");

    expect(calls).toEqual([{ workId: "w1", prompt: "Ship it" }]);
    expect(result).toBe(queuedRun);
    expect(store.getState().works.w1).toMatchObject({
      status: "running",
      updatedAt: queuedRun.createdAt,
    });
    expect(store.getState().timelines.w1).toBeUndefined();
  });
});
