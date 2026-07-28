import { describe, expect, it } from "vitest";

import type { PiWorkClient } from "../../app/tauriClient";
import type {
  CreateWorkInput,
  MessageSummary,
  RunSummary,
  StartWorkOutput,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
} from "../../bindings";
import { isWorkEventTimelineItem } from "../../domain/work";
import { createWorkStore } from "./workStore";

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

const userMessage = (
  overrides: Partial<MessageSummary> = {},
): MessageSummary => ({
  id: "m1",
  workId: "w1",
  runId: "r1",
  role: "user",
  content: "Ship it",
  createdAt: "2026-07-28T09:00:01.000Z",
  ...overrides,
});

describe("createWorkStore", () => {
  it("queues trimmed instructions per Work without creating events", () => {
    const store = createWorkStore(unusedClient);

    store.getState().queueInstruction("w1", "  Follow up  ");
    store.getState().queueInstruction("w1", "   ");
    store.getState().queueInstruction("w2", "Second Work");

    expect(store.getState().queuedInstructions).toEqual({
      w1: ["Follow up"],
      w2: ["Second Work"],
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
    detailResult.resolve({ summary: staleSummary, runs: [run], messages: [], events: [] });
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
      messages: [],
      events: [
        event(1, { type: "runStarted", modelLabel: "gpt-5" }),
        event(2, { type: "runFailed", message: "persisted failure" }),
      ],
    });
    await hydration;

    expect(
      store.getState().timelines.w1?.filter(isWorkEventTimelineItem).map(({ sequence }) => sequence),
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
      messages: [],
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

  it("deduplicates the same authoritative message across live start and hydration", async () => {
    const output: StartWorkOutput = { run, userMessage: userMessage() };
    const client: PiWorkClient = {
      ...unusedClient,
      startWork: async () => output,
      listWorks: async () => [work],
      getWork: async () => ({
        summary: work,
        runs: [run],
        messages: [output.userMessage],
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
            runId: "r2",
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
    startResult.resolve({ run, userMessage: userMessage({ content: "go" }) });
    await starting;

    expect(store.getState().loading).toBe(true);

    selectionResult.resolve({ summary: work, runs: [], messages: [], events: [] });
    await Promise.resolve();
    expect(store.getState().loading).toBe(false);
  });

  it("does not let a late start response overwrite a terminal event", async () => {
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
    startResult.resolve({ run, userMessage: userMessage({ content: "go" }) });
    await starting;

    expect(store.getState().works.w1).toMatchObject({
      status: "completed",
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
      runId: "r2",
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
      runId: "r2",
      occurredAt: "2026-07-28T09:00:03.000Z",
    });
    store.getState().applyEvent({
      ...event(2, { type: "runFailed", message: "old run was late" }),
      occurredAt: "2026-07-28T09:00:04.000Z",
    });

    expect(store.getState().works.w1).toMatchObject({
      status: "completed",
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
      runId: "r2",
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
