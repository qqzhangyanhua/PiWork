import { createStore } from "zustand/vanilla";

import { tauriClient, type PiWorkClient } from "../../app/tauriClient";
import type {
  CreateWorkInput,
  RunSummary,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
} from "../../bindings";
import {
  normalizeAppError,
  type AppError,
  type TimelineItem,
} from "../../domain/work";

export type WorkState = {
  works: Record<string, WorkSummary>;
  selectedWorkId: string | null;
  timelines: Record<string, TimelineItem[]>;
  queuedInstructions: Record<string, string[]>;
  latestRuns: Record<string, RunSummary>;
  lastSequenceByRun: Record<string, number>;
  loading: boolean;
  error: AppError | null;
  hydrate(): Promise<void>;
  createWork(input: CreateWorkInput): Promise<WorkDetail>;
  startWork(workId: string, prompt: string): Promise<RunSummary>;
  queueInstruction(workId: string, prompt: string): void;
  selectWork(workId: string): void;
  upsertWork(work: WorkSummary): void;
  applyEvent(event: WorkEventEnvelope): void;
};

type InternalWorkState = WorkState & {
  setSubscriptionError(error: AppError | null): void;
};

const statusForEvent = (
  currentStatus: WorkSummary["status"],
  event: WorkEventEnvelope,
): WorkSummary["status"] => {
  if (event.payload.type === "runStarted") {
    return "running";
  }
  if (event.payload.type === "runCompleted") {
    return "completed";
  }
  if (event.payload.type === "runFailed") {
    return "failed";
  }
  return currentStatus;
};

type WorkMutation =
  | { type: "upsert"; work: WorkSummary }
  | { type: "liveEvent"; event: WorkEventEnvelope }
  | { type: "startResponse"; run: RunSummary }
  | { type: "detail"; detail: WorkDetail };

const isTerminalStatus = (status: WorkSummary["status"]) =>
  status === "completed" ||
  status === "failed" ||
  status === "stopped" ||
  status === "interrupted";

export const createWorkStore = (client: PiWorkClient = tauriClient) => {
  let hydration: Promise<void> | null = null;
  let selectionIntentSequence = 0;
  let latestSelectionIntent = 0;
  let selectionRequestSequence = 0;
  let latestSelectionRequest = 0;
  let pendingOperations = 0;
  let operationSequence = 0;
  let errorOwner = 0;
  let operationError: AppError | null = null;
  let subscriptionError: AppError | null = null;
  const currentRunByWork = new Map<string, string>();
  const runSortKey = new Map<string, { at: string; id: string }>();

  const beginSelectionIntent = () => {
    latestSelectionIntent = ++selectionIntentSequence;
    latestSelectionRequest = ++selectionRequestSequence;
    return latestSelectionIntent;
  };
  const beginSelectionRequest = () => {
    latestSelectionRequest = ++selectionRequestSequence;
    return latestSelectionRequest;
  };

  const metadataKey = (workId: string, runId: string) =>
    `${workId}\u0000${runId}`;
  const registerRun = (workId: string, runId: string, at: string) => {
    const key = metadataKey(workId, runId);
    if (!runSortKey.has(key)) {
      runSortKey.set(key, { at, id: runId });
    }
  };
  const compareRuns = (workId: string, leftRunId: string, rightRunId: string) => {
    const left = runSortKey.get(metadataKey(workId, leftRunId));
    const right = runSortKey.get(metadataKey(workId, rightRunId));
    return (
      (left?.at ?? "").localeCompare(right?.at ?? "") ||
      (left?.id ?? leftRunId).localeCompare(right?.id ?? rightRunId)
    );
  };
  const sortTimeline = (
    workId: string,
    events: WorkEventEnvelope[],
  ): WorkEventEnvelope[] =>
    [...events].sort(
      (left, right) =>
        compareRuns(workId, left.runId, right.runId) ||
        left.sequence - right.sequence ||
        left.occurredAt.localeCompare(right.occurredAt) ||
        left.version - right.version,
    );
  const newestRun = (workId: string, runIds: Iterable<string>) => {
    let newest: string | undefined;
    for (const runId of runIds) {
      if (!newest || compareRuns(workId, runId, newest) > 0) {
        newest = runId;
      }
    }
    return newest;
  };
  const reduceWork = (
    state: WorkState,
    mutation: WorkMutation,
  ): Partial<WorkState> | WorkState => {
    if (mutation.type === "upsert") {
      const current = state.works[mutation.work.id];
      if (current && current.updatedAt >= mutation.work.updatedAt) {
        return state;
      }
      return {
        works: { ...state.works, [mutation.work.id]: mutation.work },
      };
    }

    if (mutation.type === "startResponse") {
      const { run } = mutation;
      registerRun(run.workId, run.id, run.createdAt);
      const previousLatestRun = state.latestRuns[run.workId];
      const latestRun =
        !previousLatestRun ||
        run.createdAt.localeCompare(previousLatestRun.createdAt) > 0 ||
        (run.createdAt === previousLatestRun.createdAt &&
          run.id.localeCompare(previousLatestRun.id) > 0)
          ? run
          : previousLatestRun;
      const latestRuns = {
        ...state.latestRuns,
        [run.workId]: latestRun,
      };
      const previousCurrentRun = currentRunByWork.get(run.workId);
      const runIsNotOlder =
        !previousCurrentRun ||
        compareRuns(run.workId, run.id, previousCurrentRun) >= 0;
      if (
        !previousCurrentRun ||
        compareRuns(run.workId, run.id, previousCurrentRun) > 0
      ) {
        currentRunByWork.set(run.workId, run.id);
      }

      const currentWork = state.works[run.workId];
      const protectsSameRunTerminal =
        previousCurrentRun === run.id &&
        currentWork &&
        isTerminalStatus(currentWork.status);
      if (
        !currentWork ||
        !runIsNotOlder ||
        run.createdAt < currentWork.updatedAt ||
        protectsSameRunTerminal
      ) {
        return latestRun === previousLatestRun ? state : { latestRuns };
      }
      return {
        latestRuns,
        works: {
          ...state.works,
          [run.workId]: {
            ...currentWork,
            status: "running",
            updatedAt: run.createdAt,
          },
        },
      };
    }

    if (mutation.type === "liveEvent") {
      const { event } = mutation;
      if (event.sequence <= (state.lastSequenceByRun[event.runId] ?? 0)) {
        return state;
      }
      registerRun(event.workId, event.runId, event.occurredAt);
      const previousCurrentRun = currentRunByWork.get(event.workId);
      if (
        !previousCurrentRun ||
        (event.payload.type === "runStarted" &&
          compareRuns(event.workId, event.runId, previousCurrentRun) > 0)
      ) {
        currentRunByWork.set(event.workId, event.runId);
      }
      const currentRun = currentRunByWork.get(event.workId);
      const currentWork = state.works[event.workId];
      const canUpdateSummary =
        currentWork &&
        currentRun === event.runId &&
        event.occurredAt >= currentWork.updatedAt;
      const updatedWork = canUpdateSummary
        ? {
            ...currentWork,
            status: statusForEvent(currentWork.status, event),
            updatedAt: event.occurredAt,
          }
        : currentWork;

      return {
        works: updatedWork
          ? { ...state.works, [event.workId]: updatedWork }
          : state.works,
        timelines: {
          ...state.timelines,
          [event.workId]: sortTimeline(event.workId, [
            ...(state.timelines[event.workId] ?? []),
            event,
          ]),
        },
        lastSequenceByRun: {
          ...state.lastSequenceByRun,
          [event.runId]: event.sequence,
        },
      };
    }

    const { detail } = mutation;
    for (const run of detail.runs) {
      registerRun(detail.summary.id, run.id, run.createdAt);
    }
    const eventBySequence = new Map<string, WorkEventEnvelope>();
    for (const persistedEvent of detail.events) {
      registerRun(
        persistedEvent.workId,
        persistedEvent.runId,
        persistedEvent.occurredAt,
      );
      eventBySequence.set(
        `${persistedEvent.runId}\u0000${persistedEvent.sequence}`,
        persistedEvent,
      );
    }
    for (const liveEvent of state.timelines[detail.summary.id] ?? []) {
      registerRun(liveEvent.workId, liveEvent.runId, liveEvent.occurredAt);
      eventBySequence.set(
        `${liveEvent.runId}\u0000${liveEvent.sequence}`,
        liveEvent,
      );
    }
    const mergedEvents = sortTimeline(
      detail.summary.id,
      [...eventBySequence.values()],
    );
    const candidateCurrentRun = newestRun(
      detail.summary.id,
      new Set([
        ...detail.runs.map((run) => run.id),
        ...mergedEvents.map((event) => event.runId),
      ]),
    );
    const latestRun = detail.runs.reduce<RunSummary | undefined>(
      (latest, candidate) =>
        !latest ||
        candidate.createdAt.localeCompare(latest.createdAt) > 0 ||
        (candidate.createdAt === latest.createdAt &&
          candidate.id.localeCompare(latest.id) > 0)
          ? candidate
          : latest,
      state.latestRuns[detail.summary.id],
    );
    const previousCurrentRun = currentRunByWork.get(detail.summary.id);
    if (
      candidateCurrentRun &&
      (!previousCurrentRun ||
        compareRuns(
          detail.summary.id,
          candidateCurrentRun,
          previousCurrentRun,
        ) > 0)
    ) {
      currentRunByWork.set(detail.summary.id, candidateCurrentRun);
    }

    const currentSummary = state.works[detail.summary.id];
    let mergedSummary =
      currentSummary && currentSummary.updatedAt >= detail.summary.updatedAt
        ? currentSummary
        : detail.summary;
    const currentRun = currentRunByWork.get(detail.summary.id);
    const lastSequenceByRun = { ...state.lastSequenceByRun };
    const mergedMaximum = new Map<string, number>();
    for (const event of mergedEvents) {
      mergedMaximum.set(
        event.runId,
        Math.max(mergedMaximum.get(event.runId) ?? 0, event.sequence),
      );
      if (
        event.runId === currentRun &&
        event.occurredAt >= mergedSummary.updatedAt
      ) {
        mergedSummary = {
          ...mergedSummary,
          status: statusForEvent(mergedSummary.status, event),
          updatedAt: event.occurredAt,
        };
      }
    }
    for (const [runId, maximum] of mergedMaximum) {
      lastSequenceByRun[runId] = maximum;
    }
    return {
      latestRuns: latestRun
        ? { ...state.latestRuns, [detail.summary.id]: latestRun }
        : state.latestRuns,
      works: { ...state.works, [detail.summary.id]: mergedSummary },
      timelines: {
        ...state.timelines,
        [detail.summary.id]: mergedEvents,
      },
      lastSequenceByRun,
    };
  };

  const store = createStore<InternalWorkState>((set, get) => {
    const publishError = () => {
      set({ error: subscriptionError ?? operationError });
    };
    const beginOperation = () => {
      const operation = ++operationSequence;
      errorOwner = operation;
      pendingOperations += 1;
      operationError = null;
      set({ loading: true, error: subscriptionError });
      return operation;
    };
    const succeedOperation = (operation: number) => {
      if (operation === errorOwner) {
        operationError = null;
        publishError();
      }
    };
    const failOperation = (operation: number, error: unknown) => {
      if (operation === errorOwner) {
        operationError = normalizeAppError(error);
        publishError();
      }
    };
    const endOperation = () => {
      pendingOperations -= 1;
      set({ loading: pendingOperations > 0 });
    };

    return {
      works: {},
      selectedWorkId: null,
      timelines: {},
      queuedInstructions: {},
      latestRuns: {},
      lastSequenceByRun: {},
      loading: false,
      error: null,
      hydrate: async () => {
        if (hydration) {
          return hydration;
        }

        const operation = beginOperation();
        const initialSelectionIntent = latestSelectionIntent;
        hydration = (async () => {
          try {
            const listedWorks = await client.listWorks();
            const sortedWorks = [...listedWorks].sort((left, right) =>
              right.updatedAt.localeCompare(left.updatedAt),
            );
            for (const listedWork of sortedWorks) {
              get().upsertWork(listedWork);
            }

            if (latestSelectionIntent !== initialSelectionIntent) {
              return;
            }

            let selectedWorkId = get().selectedWorkId;
            if (
              !selectedWorkId &&
              latestSelectionIntent === initialSelectionIntent
            ) {
              selectedWorkId = sortedWorks[0]?.id ?? null;
              if (selectedWorkId) {
                beginSelectionIntent();
                set({ selectedWorkId });
              }
            }
            if (!selectedWorkId) {
              succeedOperation(operation);
              return;
            }

            const intent = latestSelectionIntent;
            const request = beginSelectionRequest();
            const detail = await client.getWork(selectedWorkId);
            if (
              intent !== latestSelectionIntent ||
              request !== latestSelectionRequest ||
              get().selectedWorkId !== selectedWorkId
            ) {
              return;
            }
            set((state) => reduceWork(state, { type: "detail", detail }));
            succeedOperation(operation);
          } catch (error) {
            failOperation(operation, error);
          } finally {
            endOperation();
            hydration = null;
          }
        })();

        return hydration;
      },
      createWork: async (input) => {
        const intent = beginSelectionIntent();
        const operation = beginOperation();
        try {
          const detail = await client.createWork(input);
          set((state) => reduceWork(state, { type: "detail", detail }));
          if (intent === latestSelectionIntent) {
            set({ selectedWorkId: detail.summary.id });
          }
          succeedOperation(operation);
          return detail;
        } catch (error) {
          failOperation(operation, error);
          throw error;
        } finally {
          endOperation();
        }
      },
      startWork: async (workId, prompt) => {
        const operation = beginOperation();
        try {
          const run = await client.startWork(workId, prompt);
          set((state) => reduceWork(state, { type: "startResponse", run }));
          succeedOperation(operation);
          return run;
        } catch (error) {
          failOperation(operation, error);
          throw error;
        } finally {
          endOperation();
        }
      },
      queueInstruction: (workId, prompt) => {
        const instruction = prompt.trim();
        if (!instruction) return;
        set((state) => ({
          queuedInstructions: {
            ...state.queuedInstructions,
            [workId]: [
              ...(state.queuedInstructions[workId] ?? []),
              instruction,
            ],
          },
        }));
      },
      selectWork: (workId) => {
        const intent = beginSelectionIntent();
        const request = beginSelectionRequest();
        const operation = beginOperation();
        set({ selectedWorkId: workId });
        void (async () => {
          try {
            const detail = await client.getWork(workId);
            if (
              intent !== latestSelectionIntent ||
              request !== latestSelectionRequest ||
              get().selectedWorkId !== workId
            ) {
              return;
            }
            set((state) => reduceWork(state, { type: "detail", detail }));
            succeedOperation(operation);
          } catch (error) {
            if (
              intent === latestSelectionIntent &&
              request === latestSelectionRequest
            ) {
              failOperation(operation, error);
            }
          } finally {
            endOperation();
          }
        })();
      },
      upsertWork: (work) =>
        set((state) => reduceWork(state, { type: "upsert", work })),
      applyEvent: (event) =>
        set((state) => reduceWork(state, { type: "liveEvent", event })),
      setSubscriptionError: (error) => {
        subscriptionError = error;
        publishError();
      },
    };
  });

  return store;
};

export type WorkStore = ReturnType<typeof createWorkStore>;
