import { createStore } from "zustand/vanilla";

import { tauriClient, type PiWorkClient } from "../../app/tauriClient";
import type {
  CreateWorkInput,
  ImportResourcesInput,
  MessageSummary,
  ResourceSummary,
  RunSummary,
  StartWorkOutput,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
} from "../../bindings";
import {
  normalizeAppError,
  isWorkEventTimelineItem,
  timelineItemKey,
  workEventSequenceKey,
  type AppError,
  type TimelineItem,
} from "../../domain/work";

export type WorkState = {
  works: Record<string, WorkSummary>;
  resources: Record<string, ResourceSummary[]>;
  selectedWorkId: string | null;
  timelines: Record<string, TimelineItem[]>;
  queuedInstructions: Record<string, QueuedInstruction[]>;
  latestRuns: Record<string, RunSummary>;
  lastSequenceByRun: Record<string, number>;
  loading: boolean;
  error: AppError | null;
  hydrationError: AppError | null;
  hydrate(): Promise<void>;
  createWork(input: CreateWorkInput): Promise<WorkDetail>;
  importResources(input: ImportResourcesInput): Promise<ResourceSummary[]>;
  startWork(
    workId: string,
    prompt: string,
    referencedFiles?: string[],
    resourceIds?: string[],
  ): Promise<StartWorkOutput>;
  stopWork(workId: string): Promise<WorkDetail>;
  archiveWork(workId: string): Promise<WorkDetail>;
  restoreWork(workId: string): Promise<WorkDetail>;
  interruptWork(
    workId: string,
    prompt: string,
    referencedFiles?: string[],
    resourceIds?: string[],
  ): Promise<StartWorkOutput>;
  queueInstruction(
    workId: string,
    prompt: string,
    referencedFiles?: string[],
    resourceIds?: string[],
  ): void;
  selectWork(workId: string): void;
  upsertWork(work: WorkSummary): void;
  applyEvent(event: WorkEventEnvelope): void;
};

export type QueuedInstruction = {
  prompt: string;
  referencedFiles: string[];
  resourceIds: string[];
};

type InternalWorkState = WorkState & {
  setSubscriptionError(error: AppError | null): void;
};

const persistedStartInstruction = Symbol("persistedStartInstruction");

class PersistedStartInstructionError extends Error {
  readonly [persistedStartInstruction] = true;

  constructor(readonly cause: unknown) {
    super("Start failed after the instruction was persisted");
    this.name = "PersistedStartInstructionError";
  }
}

export const didPersistStartInstruction = (
  error: unknown,
): boolean => error instanceof PersistedStartInstructionError;

const statusForEvent = (
  currentStatus: WorkSummary["status"],
  event: WorkEventEnvelope,
): WorkSummary["status"] => {
  if (event.payload.type === "runStarted") {
    return "running";
  }
  if (event.payload.type === "waiting") {
    return "waiting";
  }
  if (event.payload.type === "runCompleted") {
    return "idle";
  }
  if (event.payload.type === "workDeliveryCompleted") {
    return "completed";
  }
  if (event.payload.type === "runFailed") {
    return "failed";
  }
  if (
    currentStatus === "waiting" &&
    (
      event.payload.type === "assistantDelta" ||
      event.payload.type === "thoughtDelta" ||
      event.payload.type === "planChanged" ||
      event.payload.type === "toolPending" ||
      event.payload.type === "toolStarted" ||
      event.payload.type === "toolProgress" ||
      event.payload.type === "toolFinished"
    )
  ) {
    return "running";
  }
  return currentStatus;
};

type WorkMutation =
  | { type: "upsert"; work: WorkSummary }
  | { type: "liveEvent"; event: WorkEventEnvelope }
  | { type: "startResponse"; output: StartWorkOutput }
  | { type: "detail"; detail: WorkDetail };

const isTerminalStatus = (status: WorkSummary["status"]) =>
  status === "completed" ||
  status === "failed" ||
  status === "stopped" ||
  status === "interrupted" ||
  status === "idle";

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
  const timelineTime = (item: TimelineItem) =>
    isWorkEventTimelineItem(item) ? item.occurredAt : item.createdAt;
  const sortTimeline = (items: TimelineItem[]): TimelineItem[] =>
    [...items].sort(
      (left, right) =>
        timelineTime(left).localeCompare(timelineTime(right)) ||
        timelineItemKey(left).localeCompare(timelineItemKey(right)),
    );
  const mergeTimeline = (...groups: TimelineItem[][]) => {
    const byIdentity = new Map<string, TimelineItem>();
    for (const group of groups) {
      for (const item of group) {
        byIdentity.set(timelineItemKey(item), item);
      }
    }
    return sortTimeline([...byIdentity.values()]);
  };
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
      const { assignment, run, userMessage } = mutation.output;
      const timelineRunId = userMessage.runId ?? `assignment:${userMessage.assignmentId ?? assignment.id}`;
      if (!timelineRunId) {
        return state;
      }
      const timelineMessage: MessageSummary = {
        id: userMessage.id,
        workId: userMessage.workId,
        runId: timelineRunId,
        role: userMessage.role,
        content: userMessage.content,
        resourceIds: userMessage.resourceIds,
        createdAt: userMessage.createdAt,
      };
      registerRun(
        timelineMessage.workId,
        timelineMessage.runId,
        timelineMessage.createdAt,
      );
      const timelines = {
        ...state.timelines,
        [timelineMessage.workId]: mergeTimeline(
          state.timelines[timelineMessage.workId] ?? [],
          [timelineMessage],
        ),
      };
      if (!run) {
        return { timelines };
      }
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
        return { latestRuns, timelines };
      }
      return {
        latestRuns,
        timelines,
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
      const sequenceKey = workEventSequenceKey(event);
      if (event.sequence <= (state.lastSequenceByRun[sequenceKey] ?? 0)) {
        return state;
      }
      if (event.runId) {
        registerRun(event.workId, event.runId, event.occurredAt);
      }
      const previousCurrentRun = currentRunByWork.get(event.workId);
      if (
        event.runId &&
        (!previousCurrentRun ||
          (event.payload.type === "runStarted" &&
            compareRuns(event.workId, event.runId, previousCurrentRun) > 0))
      ) {
        currentRunByWork.set(event.workId, event.runId);
      }
      const currentRun = currentRunByWork.get(event.workId);
      const currentWork = state.works[event.workId];
      const canUpdateSummary =
        currentWork &&
        event.runId !== null &&
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
          [event.workId]: mergeTimeline(
            state.timelines[event.workId] ?? [],
            [event],
          ),
        },
        lastSequenceByRun: {
          ...state.lastSequenceByRun,
          [sequenceKey]: event.sequence,
        },
      };
    }

    const { detail } = mutation;
    for (const run of detail.runs) {
      registerRun(detail.summary.id, run.id, run.createdAt);
    }
    for (const message of detail.messages) {
      registerRun(message.workId, message.runId, message.createdAt);
    }
    for (const persistedEvent of detail.events) {
      if (persistedEvent.runId) {
        registerRun(
          persistedEvent.workId,
          persistedEvent.runId,
          persistedEvent.occurredAt,
        );
      }
    }
    for (const liveItem of state.timelines[detail.summary.id] ?? []) {
      if (liveItem.runId) {
        registerRun(
          liveItem.workId,
          liveItem.runId,
          isWorkEventTimelineItem(liveItem)
            ? liveItem.occurredAt
            : liveItem.createdAt,
        );
      }
    }
    const mergedTimeline = mergeTimeline(
      detail.messages,
      detail.events,
      state.timelines[detail.summary.id] ?? [],
    );
    const mergedEvents = mergedTimeline.filter(isWorkEventTimelineItem);
    const candidateCurrentRun = newestRun(
      detail.summary.id,
      new Set([
        ...detail.runs.map((run) => run.id),
        ...mergedTimeline.flatMap((item) => (item.runId ? [item.runId] : [])),
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
      const sequenceKey = workEventSequenceKey(event);
      mergedMaximum.set(
        sequenceKey,
        Math.max(mergedMaximum.get(sequenceKey) ?? 0, event.sequence),
      );
      if (
        event.runId !== null &&
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
        [detail.summary.id]: mergedTimeline,
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
      resources: {},
      selectedWorkId: null,
      timelines: {},
      queuedInstructions: {},
      latestRuns: {},
      lastSequenceByRun: {},
      loading: false,
      error: null,
      hydrationError: null,
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
              if (operation === errorOwner) set({ hydrationError: null });
              succeedOperation(operation);
              return;
            }

            const intent = latestSelectionIntent;
            const request = beginSelectionRequest();
            const resourcesPromise = client.listWorkResources(selectedWorkId);
            const detail = await client.getWork(selectedWorkId);
            if (
              intent !== latestSelectionIntent ||
              request !== latestSelectionRequest ||
              get().selectedWorkId !== selectedWorkId
            ) {
              return;
            }
            set((state) => reduceWork(state, { type: "detail", detail }));
            void resourcesPromise.then(
              (resources) => {
                if (
                  intent === latestSelectionIntent &&
                  request === latestSelectionRequest &&
                  get().selectedWorkId === selectedWorkId
                ) {
                  set((state) => ({
                    resources: {
                      ...state.resources,
                      [selectedWorkId]: resources,
                    },
                  }));
                }
              },
              () => undefined,
            );
            if (operation === errorOwner) set({ hydrationError: null });
            succeedOperation(operation);
          } catch (error) {
            failOperation(operation, error);
            if (operation === errorOwner) {
              set({ hydrationError: normalizeAppError(error) });
            }
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
          const resources = await client.listWorkResources(detail.summary.id);
          set((state) => ({
            ...reduceWork(state, { type: "detail", detail }),
            resources: { ...state.resources, [detail.summary.id]: resources },
          }));
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
      importResources: async (input) => {
        const operation = beginOperation();
        try {
          const imported = await client.importResources(input);
          if (input.workId) {
            set((state) => {
              const merged = new Map(
                (state.resources[input.workId!] ?? []).map((resource) => [
                  resource.id,
                  resource,
                ]),
              );
              for (const resource of imported) merged.set(resource.id, resource);
              return {
                resources: {
                  ...state.resources,
                  [input.workId!]: [...merged.values()],
                },
              };
            });
          }
          succeedOperation(operation);
          return imported;
        } catch (error) {
          failOperation(operation, error);
          throw error;
        } finally {
          endOperation();
        }
      },
      startWork: async (workId, prompt, referencedFiles = [], resourceIds = []) => {
        const operation = beginOperation();
        const knownUserMessageIds = new Set(
          (get().timelines[workId] ?? [])
            .filter(
              (item): item is MessageSummary =>
                !isWorkEventTimelineItem(item) && item.role === "user",
            )
            .map(({ id }) => id),
        );
        try {
          const output = await client.startWork(
            workId,
            prompt,
            referencedFiles,
            resourceIds,
          );
          set((state) => reduceWork(state, { type: "startResponse", output }));
          succeedOperation(operation);
          return output;
        } catch (error) {
          failOperation(operation, error);
          let instructionPersisted = false;
          try {
            const detail = await client.getWork(workId);
            instructionPersisted = detail.messages.some(
              (message) =>
                message.role === "user" &&
                !knownUserMessageIds.has(message.id) &&
                message.resourceIds.length === resourceIds.length &&
                message.resourceIds.every((id, index) => id === resourceIds[index]),
            );
            set((state) => reduceWork(state, { type: "detail", detail }));
          } catch {
            // The original start failure remains the product error and retry signal.
          }
          if (instructionPersisted) {
            throw new PersistedStartInstructionError(error);
          }
          throw error;
        } finally {
          endOperation();
        }
      },
      stopWork: async (workId) => {
        const operation = beginOperation();
        try {
          const detail = await client.stopWork(workId);
          set((state) => reduceWork(state, { type: "detail", detail }));
          succeedOperation(operation);
          return detail;
        } catch (error) {
          failOperation(operation, error);
          throw error;
        } finally {
          endOperation();
        }
      },
      archiveWork: async (workId) => {
        const operation = beginOperation();
        try {
          const detail = await client.archiveWork(workId);
          set((state) => reduceWork(state, { type: "detail", detail }));
          succeedOperation(operation);
          return detail;
        } catch (error) {
          failOperation(operation, error);
          throw error;
        } finally {
          endOperation();
        }
      },
      restoreWork: async (workId) => {
        const operation = beginOperation();
        try {
          const detail = await client.restoreWork(workId);
          set((state) => reduceWork(state, { type: "detail", detail }));
          succeedOperation(operation);
          return detail;
        } catch (error) {
          failOperation(operation, error);
          throw error;
        } finally {
          endOperation();
        }
      },
      interruptWork: async (
        workId,
        prompt,
        referencedFiles = [],
        resourceIds = [],
      ) => {
        const operation = beginOperation();
        try {
          const output = await client.interruptAndReplace(workId, {
            assignmentId: "",
            replacement: {
              instruction: prompt,
              referencedFiles,
              resourceIds,
            },
          });
          set((state) => reduceWork(state, { type: "startResponse", output }));
          succeedOperation(operation);
          return output;
        } catch (error) {
          failOperation(operation, error);
          throw error;
        } finally {
          endOperation();
        }
      },
      queueInstruction: (
        workId,
        prompt,
        referencedFiles = [],
        resourceIds = [],
      ) => {
        const instruction = prompt.trim();
        if (!instruction && resourceIds.length === 0) return;
        set((state) => ({
          queuedInstructions: {
            ...state.queuedInstructions,
            [workId]: [
              ...(state.queuedInstructions[workId] ?? []),
              {
                prompt: instruction,
                referencedFiles: [...referencedFiles],
                resourceIds: [...resourceIds],
              },
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
            const resourcesPromise = client.listWorkResources(workId);
            const detail = await client.getWork(workId);
            if (
              intent !== latestSelectionIntent ||
              request !== latestSelectionRequest ||
              get().selectedWorkId !== workId
            ) {
              return;
            }
            set((state) => reduceWork(state, { type: "detail", detail }));
            void resourcesPromise.then(
              (resources) => {
                if (
                  intent === latestSelectionIntent &&
                  request === latestSelectionRequest &&
                  get().selectedWorkId === workId
                ) {
                  set((state) => ({
                    resources: { ...state.resources, [workId]: resources },
                  }));
                }
              },
              () => undefined,
            );
            if (operation === errorOwner) set({ hydrationError: null });
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
