import { createStore } from "zustand/vanilla";

import { tauriClient, type PiWorkClient } from "../../app/tauriClient";
import type {
  CreateWorkInput,
  RunSummary,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
} from "../../bindings";
import type { AppError, TimelineItem } from "../../domain/work";

export type WorkState = {
  works: Record<string, WorkSummary>;
  selectedWorkId: string | null;
  timelines: Record<string, TimelineItem[]>;
  lastSequenceByRun: Record<string, number>;
  loading: boolean;
  error: AppError | null;
  hydrate(): Promise<void>;
  createWork(input: CreateWorkInput): Promise<WorkDetail>;
  startWork(workId: string, prompt: string): Promise<RunSummary>;
  selectWork(workId: string): void;
  upsertWork(work: WorkSummary): void;
  applyEvent(event: WorkEventEnvelope): void;
};

const normalizeError = (error: unknown): AppError => {
  if (error instanceof Error) {
    return { code: "unknown", message: error.message };
  }

  return { code: "unknown", message: String(error) };
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

const mergeWorkDetail = (state: WorkState, detail: WorkDetail) => {
  const currentSummary = state.works[detail.summary.id];
  let mergedSummary =
    currentSummary && currentSummary.updatedAt >= detail.summary.updatedAt
      ? currentSummary
      : detail.summary;
  const eventBySequence = new Map<string, WorkEventEnvelope>();

  for (const persistedEvent of detail.events) {
    eventBySequence.set(
      `${persistedEvent.runId}\u0000${persistedEvent.sequence}`,
      persistedEvent,
    );
  }
  for (const liveEvent of state.timelines[detail.summary.id] ?? []) {
    eventBySequence.set(
      `${liveEvent.runId}\u0000${liveEvent.sequence}`,
      liveEvent,
    );
  }

  const mergedEvents = [...eventBySequence.values()];
  const knownRunIds = new Set(detail.runs.map((run) => run.id));
  const runTime = new Map(
    detail.runs.map((run) => [run.id, run.createdAt] as const),
  );
  for (const event of mergedEvents) {
    const currentTime = runTime.get(event.runId);
    if (
      !knownRunIds.has(event.runId) &&
      (!currentTime || event.occurredAt < currentTime)
    ) {
      runTime.set(event.runId, event.occurredAt);
    }
  }
  mergedEvents.sort(
    (left, right) =>
      (runTime.get(left.runId) ?? left.occurredAt).localeCompare(
        runTime.get(right.runId) ?? right.occurredAt,
      ) ||
      left.runId.localeCompare(right.runId) ||
      left.sequence - right.sequence ||
      left.occurredAt.localeCompare(right.occurredAt) ||
      left.version - right.version,
  );

  const lastSequenceByRun = { ...state.lastSequenceByRun };
  for (const event of mergedEvents) {
    lastSequenceByRun[event.runId] = Math.max(
      lastSequenceByRun[event.runId] ?? 0,
      event.sequence,
    );
    if (event.occurredAt >= mergedSummary.updatedAt) {
      mergedSummary = {
        ...mergedSummary,
        status: statusForEvent(mergedSummary.status, event),
        updatedAt: event.occurredAt,
      };
    }
  }

  return {
    works: { ...state.works, [detail.summary.id]: mergedSummary },
    timelines: {
      ...state.timelines,
      [detail.summary.id]: mergedEvents,
    },
    lastSequenceByRun,
  };
};

export const createWorkStore = (client: PiWorkClient = tauriClient) => {
  let hydration: Promise<void> | null = null;
  let selectionRequest = 0;
  let activeSelectionRequest: number | null = null;

  const store = createStore<WorkState>((set, get) => ({
    works: {},
    selectedWorkId: null,
    timelines: {},
    lastSequenceByRun: {},
    loading: false,
    error: null,
    hydrate: async () => {
      if (hydration) {
        return hydration;
      }

      hydration = (async () => {
        set({ loading: true, error: null });
        try {
          const listedWorks = await client.listWorks();
          const sortedWorks = [...listedWorks].sort((left, right) =>
            right.updatedAt.localeCompare(left.updatedAt),
          );
          for (const listedWork of sortedWorks) {
            get().upsertWork(listedWork);
          }

          const selectedWorkId = get().selectedWorkId ?? sortedWorks[0]?.id ?? null;
          set({ selectedWorkId });
          if (!selectedWorkId) {
            return;
          }

          const request = selectionRequest;
          const detail = await client.getWork(selectedWorkId);
          if (
            request !== selectionRequest ||
            get().selectedWorkId !== selectedWorkId
          ) {
            return;
          }
          set((state) => mergeWorkDetail(state, detail));
        } catch (error) {
          set({ error: normalizeError(error) });
        } finally {
          if (activeSelectionRequest === null) {
            set({ loading: false });
          }
          hydration = null;
        }
      })();

      return hydration;
    },
    createWork: async (input) => {
      set({ loading: true, error: null });
      try {
        const detail = await client.createWork(input);
        selectionRequest += 1;
        set((state) => mergeWorkDetail(state, detail));
        set({ selectedWorkId: detail.summary.id });
        return detail;
      } catch (error) {
        set({ error: normalizeError(error) });
        throw error;
      } finally {
        set({ loading: false });
      }
    },
    startWork: async (workId, prompt) => {
      set({ loading: true, error: null });
      try {
        const run = await client.startWork(workId, prompt);
        set((state) => {
          const currentWork = state.works[workId];
          return currentWork
            ? {
                works: {
                  ...state.works,
                  [workId]: {
                    ...currentWork,
                    status: "running",
                    updatedAt: run.createdAt,
                  },
                },
              }
            : state;
        });
        return run;
      } catch (error) {
        set({ error: normalizeError(error) });
        throw error;
      } finally {
        set({ loading: false });
      }
    },
    selectWork: (workId) => {
      const request = ++selectionRequest;
      activeSelectionRequest = request;
      set({ selectedWorkId: workId, loading: true, error: null });
      void (async () => {
        try {
          const detail = await client.getWork(workId);
          if (
            request !== selectionRequest ||
            get().selectedWorkId !== workId
          ) {
            return;
          }
          set((state) => mergeWorkDetail(state, detail));
        } catch (error) {
          if (request === selectionRequest) {
            set({ error: normalizeError(error) });
          }
        } finally {
          if (request === selectionRequest) {
            activeSelectionRequest = null;
            set({ loading: false });
          }
        }
      })();
    },
    upsertWork: (work) =>
      set((state) => {
        const current = state.works[work.id];
        if (current && current.updatedAt > work.updatedAt) {
          return state;
        }
        return { works: { ...state.works, [work.id]: work } };
      }),
    applyEvent: (event) => {
      if (event.sequence <= (get().lastSequenceByRun[event.runId] ?? 0)) {
        return;
      }

      set((state) => {
        const currentWork = state.works[event.workId];
        const updatedWork = currentWork
          ? {
              ...currentWork,
              status: statusForEvent(currentWork.status, event),
              updatedAt: event.occurredAt,
            }
          : undefined;

        return {
          works: updatedWork
            ? { ...state.works, [event.workId]: updatedWork }
            : state.works,
          timelines: {
            ...state.timelines,
            [event.workId]: [...(state.timelines[event.workId] ?? []), event],
          },
          lastSequenceByRun: {
            ...state.lastSequenceByRun,
            [event.runId]: event.sequence,
          },
        };
      });
    },
  }));

  return store;
};

export type WorkStore = ReturnType<typeof createWorkStore>;
