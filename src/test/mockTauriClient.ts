import { vi, type Mock } from "vitest";

import type { PiWorkClient } from "../app/tauriClient";
import type {
  AgentInstanceSummary,
  AssignmentSummary,
  AssemblyDiagnostic,
  CapabilityPackSummary,
  CreateWorkInput,
  MessageSummary,
  ResourceSummary,
  ResourceThumbnail,
  RunSummary,
  SaveAgentAssemblyInput,
  StartWorkOutput,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
  WorkTeamSummary,
} from "../bindings";

export const assignmentSummary = (
  overrides: Partial<AssignmentSummary> = {},
): AssignmentSummary => ({
  id: "assignment-1",
  workId: "work-1",
  parentAssignmentId: null,
  createdByAgentId: null,
  assignedAgentId: "00000000-0000-0000-0000-000000000001",
  capabilityPackId: null,
  kind: "lead",
  sideEffect: "unknown",
  title: "Run work",
  instruction: "go",
  contextManifest: {},
  expectedResultSchema: {},
  acceptanceCriteria: [],
  permissionScope: {},
  priority: 0,
  status: "running",
  attemptCount: 1,
  maxAttempts: 1,
  notBefore: null,
  resultSummary: null,
  lastError: null,
  nextAttemptAt: null,
  recoveryReason: null,
  createdAt: "2026-07-28T08:00:10.000Z",
  claimedAt: "2026-07-28T08:00:10.000Z",
  startedAt: "2026-07-28T08:00:10.000Z",
  completedAt: null,
  updatedAt: "2026-07-28T08:00:10.000Z",
  ...overrides,
});

export type MockTauriClient = PiWorkClient & {
  getRuntimeStatus: Mock<Required<PiWorkClient>["getRuntimeStatus"]>;
  getModelConfigurationStatus: Mock<PiWorkClient["getModelConfigurationStatus"]>;
  listModelConfigurations: Mock<PiWorkClient["listModelConfigurations"]>;
  testModelConnection: Mock<PiWorkClient["testModelConnection"]>;
  testSavedModelConfiguration: Mock<PiWorkClient["testSavedModelConfiguration"]>;
  saveModelConfiguration: Mock<PiWorkClient["saveModelConfiguration"]>;
  activateModelConfiguration: Mock<PiWorkClient["activateModelConfiguration"]>;
  selectModelForConfiguration: Mock<PiWorkClient["selectModelForConfiguration"]>;
  createWork: Mock<PiWorkClient["createWork"]>;
  listWorks: Mock<PiWorkClient["listWorks"]>;
  getWork: Mock<PiWorkClient["getWork"]>;
  listAgentInstances: Mock<PiWorkClient["listAgentInstances"]>;
  listCapabilityPacks: Mock<PiWorkClient["listCapabilityPacks"]>;
  getWorkTeam: Mock<PiWorkClient["getWorkTeam"]>;
  validateAgentAssembly: Mock<PiWorkClient["validateAgentAssembly"]>;
  saveAgentCopy: Mock<PiWorkClient["saveAgentCopy"]>;
  addWorkMember: Mock<PiWorkClient["addWorkMember"]>;
  listProjectFiles: Mock<PiWorkClient["listProjectFiles"]>;
  importResources: Mock<PiWorkClient["importResources"]>;
  listWorkResources: Mock<PiWorkClient["listWorkResources"]>;
  getResourceThumbnail: Mock<PiWorkClient["getResourceThumbnail"]>;
  detachDraftResource: Mock<PiWorkClient["detachDraftResource"]>;
  startWork: Mock<PiWorkClient["startWork"]>;
  stopWork: Mock<PiWorkClient["stopWork"]>;
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
): WorkEventEnvelope => {
  const runId = overrides.runId ?? "run-1";
  const sequence = overrides.sequence ?? 2;
  return {
    version: 2,
    eventId: `event-${runId}-${sequence}`,
    workId: "work-1",
    runId,
    turnId: runId,
    correlationId: runId,
    sequence,
    occurredAt: now(20),
    payload: {
      type: "runCompleted",
      summary: "任务已完成",
      artifacts: ["dist/report.html"],
      validation: ["Dashboard checks passed"],
      limitations: ["Uses mock revenue data"],
    },
    ...overrides,
  };
};

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
  const cloneDto = <T,>(value: T): T => structuredClone(value);
  const roleKinds = ["lead", "researcher", "engineer", "reviewer"] as const;
  const executablePackIds = {
    lead: "capability-pack:lead-coordination:v1",
    researcher: "capability-pack:source-research:v1",
    engineer: "capability-pack:engineering-execution:v1",
    reviewer: "capability-pack:independent-review:v1",
  } as const;
  const executablePacks: CapabilityPackSummary[] = roleKinds.map((roleKind) => ({
    id: executablePackIds[roleKind],
    catalogCapabilityId: null,
    name: `${roleKind} capability`,
    description: `${roleKind} executable capability`,
    instructions: `Act as the ${roleKind}`,
    inputSchema: {},
    outputSchema: {},
    procedure: {},
    validationRubric: {},
    requiredTools: roleKind === "engineer"
      ? ["read", "grep", "find", "ls", "edit", "write", "bash"]
      : ["read", "grep", "find", "ls"],
    defaultPermissionScope:
      roleKind === "lead" || roleKind === "engineer" ? "inherit_work" : "read_only",
    compatibleRoleTemplateIds: [`role-template:${roleKind}:v1`],
    requiredEngineCapabilities: [],
    conflictsWithCapabilityPackIds: [],
    version: 1,
    status: "executable",
  }));
  const catalogPacks: CapabilityPackSummary[] = Array.from({ length: 96 }, (_, index) => {
    const sequence = String(index + 1).padStart(3, "0");
    return {
      id: `catalog-capability:${sequence}`,
      catalogCapabilityId: `catalog-capability:${sequence}`,
      name: `Catalog capability ${sequence}`,
      description: "",
      instructions: "",
      inputSchema: {},
      outputSchema: {},
      procedure: {},
      validationRubric: {},
      requiredTools: [],
      defaultPermissionScope: "read_only",
      compatibleRoleTemplateIds: [],
      requiredEngineCapabilities: [],
      conflictsWithCapabilityPackIds: [],
      version: 1,
      status: "catalog_only",
    };
  });
  const agentInstances: AgentInstanceSummary[] = roleKinds.map((roleKind, index) => {
    const permission = roleKind === "engineer" || roleKind === "lead"
      ? "inherit_work" as const
      : "read_only" as const;
    return {
      id: `agent-instance:piwork-${roleKind}`,
      definition: {
        id: `agent-definition:piwork-${roleKind}:v1`,
        roleTemplateId: `role-template:${roleKind}:v1`,
        roleKind,
        slug: `piwork-${roleKind}`,
        name: roleKind,
        description: `PiWork builtin ${roleKind}`,
        instructions: `Act as the ${roleKind}`,
        responsibilities: [],
        nonResponsibilities: [],
        inputContract: {},
        resultContract: {},
        qualityRubric: {},
        defaultEngineKind: "pi",
        defaultModelConfigurationId: null,
        defaultPermissionPolicy: permission,
        defaultParallelism: 1,
        memoryPolicy: "confirmed_only",
        capabilityPacks: [executablePacks[index]!],
        builtin: true,
        active: true,
        version: 1,
        createdAt: now(0),
        updatedAt: now(0),
      },
      displayName: roleKind,
      engineOverride: null,
      modelConfigurationOverride: null,
      permissionPolicyOverride: null,
      parallelismOverride: null,
      builtin: true,
      status: "active",
      createdAt: now(0),
      updatedAt: now(0),
    };
  });
  const capabilityPacks = [...executablePacks, ...catalogPacks];
  const permissionRank = (
    permission: AgentInstanceSummary["definition"]["defaultPermissionPolicy"],
  ) => {
    switch (permission) {
      case "read_only":
        return 0;
      case "inherit_work":
        return 1;
      case "work_write":
        return 2;
    }
  };
  const leastPermission = (
    left: AgentInstanceSummary["definition"]["defaultPermissionPolicy"],
    right: AgentInstanceSummary["definition"]["defaultPermissionPolicy"],
  ) => permissionRank(left) <= permissionRank(right) ? left : right;
  const validateAssemblyInput = (input: SaveAgentAssemblyInput): AssemblyDiagnostic[] => {
    if (input.displayName.trim().length === 0) {
      throw new Error("Display name must not be empty");
    }
    if (
      input.parallelismOverride !== null
      && (!Number.isInteger(input.parallelismOverride)
        || input.parallelismOverride < 1
        || input.parallelismOverride > 8)
    ) {
      throw new Error("Parallelism must be between 1 and 8");
    }

    const source = agentInstances.find(({ id }) => id === input.sourceInstanceId);
    if (!source) throw new Error("Agent instance does not exist");

    const seenPackIds = new Set<string>();
    const packs = input.capabilityPackIds.map((packId) => {
      if (seenPackIds.has(packId)) {
        throw new Error("Capability pack ids must be unique");
      }
      seenPackIds.add(packId);
      const pack = capabilityPacks.find(({ id }) => id === packId);
      if (!pack) throw new Error("Capability pack does not exist");
      return pack;
    });
    const diagnostics: AssemblyDiagnostic[] = [];
    const addDiagnostic = (
      code: AssemblyDiagnostic["code"],
      capabilityPackId: string | null,
      message: string,
    ) => diagnostics.push({ code, capabilityPackId, message });

    for (const pack of packs) {
      if (pack.status !== "executable") {
        addDiagnostic(
          "not_executable",
          pack.id,
          `Capability pack '${pack.name}' is not executable`,
        );
      }
      if (!pack.compatibleRoleTemplateIds.includes(source.definition.roleTemplateId)) {
        addDiagnostic(
          "incompatible_role",
          pack.id,
          `Capability pack '${pack.name}' is incompatible with role '${source.definition.roleKind}'`,
        );
      }
    }

    const definitionPermission = source.definition.defaultPermissionPolicy;
    const sourcePermission = source.permissionPolicyOverride === null
      ? definitionPermission
      : leastPermission(definitionPermission, source.permissionPolicyOverride);
    const requestedPermission = input.permissionPolicyOverride ?? sourcePermission;
    const effectivePermission = leastPermission(sourcePermission, requestedPermission);
    if (permissionRank(requestedPermission) > permissionRank(sourcePermission)) {
      addDiagnostic(
        "permission_escalation",
        null,
        "Requested permission would expand the source Agent's authority",
      );
    }
    for (const pack of packs) {
      if (permissionRank(pack.defaultPermissionScope) > permissionRank(effectivePermission)) {
        addDiagnostic(
          "permission_escalation",
          pack.id,
          `Capability pack '${pack.name}' requires a broader permission`,
        );
      }
    }

    return diagnostics;
  };
  const teamMembersByWork = new Map<string, AgentInstanceSummary[]>();
  const workTeam = (workId: string): WorkTeamSummary => {
    const instances = [agentInstances[0]!, ...(teamMembersByWork.get(workId) ?? [])]
      .filter((instance, index, all) => all.findIndex(({ id }) => id === instance.id) === index);
    const members = instances.map((instance) => ({
      workId,
      instance,
      roleKind: instance.definition.roleKind,
      status: "joined" as const,
      permissionPolicy:
        instance.permissionPolicyOverride ?? instance.definition.defaultPermissionPolicy,
      joinedAt: now(0),
      updatedAt: now(0),
    }));
    return { workId, lead: members[0]!, members };
  };
  const listAgentInstances: Mock<PiWorkClient["listAgentInstances"]> = vi.fn(
    async () => cloneDto(agentInstances),
  );
  const listCapabilityPacks: Mock<PiWorkClient["listCapabilityPacks"]> = vi.fn(
    async () => cloneDto(capabilityPacks),
  );
  const getWorkTeam: Mock<PiWorkClient["getWorkTeam"]> = vi.fn(
    async (workId) => {
      if (!details.has(workId)) throw new Error(`Work not found: ${workId}`);
      return cloneDto(workTeam(workId));
    },
  );
  const validateAgentAssembly: Mock<PiWorkClient["validateAgentAssembly"]> = vi.fn(
    async (input: SaveAgentAssemblyInput) => cloneDto(validateAssemblyInput(input)),
  );
  const saveAgentCopy: Mock<PiWorkClient["saveAgentCopy"]> = vi.fn(async (input) => {
    const diagnostics = validateAssemblyInput(input);
    if (diagnostics.length > 0) {
      throw new Error(diagnostics.map(({ message }) => message).join("; "));
    }
    const source = agentInstances.find(({ id }) => id === input.sourceInstanceId);
    if (!source) throw new Error("Agent instance does not exist");
    const selectedPacks = input.capabilityPackIds.map((packId) =>
      capabilityPacks.find(({ id }) => id === packId)!,
    );
    const sourcePermission = source.permissionPolicyOverride === null
      ? source.definition.defaultPermissionPolicy
      : leastPermission(
        source.definition.defaultPermissionPolicy,
        source.permissionPolicyOverride,
      );
    const effectivePermission = leastPermission(
      sourcePermission,
      input.permissionPolicyOverride ?? sourcePermission,
    );
    const copy: AgentInstanceSummary = {
      ...source,
      id: `agent-instance:local:${agentInstances.length + 1}`,
      definition: {
        ...source.definition,
        id: `agent-definition:local:${agentInstances.length + 1}:v1`,
        name: input.displayName,
        defaultPermissionPolicy: effectivePermission,
        capabilityPacks: selectedPacks,
        builtin: false,
      },
      displayName: input.displayName,
      engineOverride: input.engineOverride,
      modelConfigurationOverride: input.modelConfigurationOverride,
      permissionPolicyOverride: input.permissionPolicyOverride,
      parallelismOverride: input.parallelismOverride,
      builtin: false,
    };
    agentInstances.push(cloneDto(copy));
    return cloneDto(copy);
  });
  const addWorkMember: Mock<PiWorkClient["addWorkMember"]> = vi.fn(
    async (workId, agentInstanceId) => {
      if (!details.has(workId)) throw new Error(`Work not found: ${workId}`);
      const instance = agentInstances.find(({ id }) => id === agentInstanceId);
      if (!instance) throw new Error(`Agent instance not found: ${agentInstanceId}`);
      const members = teamMembersByWork.get(workId) ?? [];
      if (instance.id !== agentInstances[0]!.id && !members.some(({ id }) => id === instance.id)) {
        teamMembersByWork.set(workId, [...members, instance]);
      }
      return cloneDto(workTeam(workId));
    },
  );
  const getRuntimeStatus: Mock<Required<PiWorkClient>["getRuntimeStatus"]> = vi.fn(async () => ({
    python: { available: true, version: "3.11.6" },
    node: { available: true, version: "18.20.2" },
    git: { available: true, version: null },
  }));
  const getModelConfigurationStatus: Mock<PiWorkClient["getModelConfigurationStatus"]> = vi.fn(async () => ({
    configured: true,
    configuration: {
      id: "openai-default",
      provider: "openai" as const,
      baseUrl: "https://api.openai.com/v1",
      modelId: "gpt-5.2",
      active: true,
      credentialConfigured: true,
    },
  }));
  const listModelConfigurations: Mock<PiWorkClient["listModelConfigurations"]> = vi.fn(async () => {
    const configuration = (await getModelConfigurationStatus()).configuration;
    return configuration ? [configuration] : [];
  });
  const testModelConnection: Mock<PiWorkClient["testModelConnection"]> = vi.fn(async (_input) => ({
    models: [{ id: "gpt-5.2", label: "GPT-5.2" }],
  }));
  const testSavedModelConfiguration: Mock<PiWorkClient["testSavedModelConfiguration"]> = vi.fn(async (_configurationId) => ({
    models: [{ id: "gpt-5.2", label: "GPT-5.2" }],
  }));
  const saveModelConfiguration: Mock<PiWorkClient["saveModelConfiguration"]> = vi.fn(async (input) => ({
    id: input.id ?? "model-new",
    provider: input.provider,
    baseUrl: input.baseUrl,
    modelId: input.modelId,
    active: false,
    credentialConfigured: true,
  }));
  const activateModelConfiguration: Mock<PiWorkClient["activateModelConfiguration"]> = vi.fn(async (configurationId) => ({
    id: configurationId,
    provider: "openai" as const,
    baseUrl: "https://api.openai.com/v1",
    modelId: "gpt-5.2",
    active: true,
    credentialConfigured: true,
  }));
  const selectModelForConfiguration: Mock<PiWorkClient["selectModelForConfiguration"]> = vi.fn(async (input) => ({
    id: input.configurationId,
    provider: "openai" as const,
    baseUrl: "https://api.openai.com/v1",
    modelId: input.modelId,
    active: true,
    credentialConfigured: true,
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
    const legacyAssignmentId = `legacy-run:${id}`;
    const run: RunSummary = {
      id,
      workId,
      assignmentId: null,
      agentInstanceId: null,
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
      return {
        assignment: assignmentSummary({
          id: legacyAssignmentId,
          workId,
          instruction: prompt.trim(),
          contextManifest: null,
          expectedResultSchema: null,
          acceptanceCriteria: null,
          permissionScope: null,
          createdAt: run.createdAt,
          claimedAt: null,
          startedAt: run.createdAt,
          updatedAt: run.createdAt,
        }),
        run,
        userMessage: { ...userMessage, assignmentId: legacyAssignmentId },
      } satisfies StartWorkOutput;
    },
  );
  const stopWork: Mock<PiWorkClient["stopWork"]> = vi.fn(async (workId) => {
    const detail = details.get(workId);
    if (!detail) throw new Error(`Work not found: ${workId}`);
    const activeRun = [...detail.runs]
      .reverse()
      .find(({ status }) => status === "queued" || status === "running" || status === "waiting");
    if (!activeRun) throw new Error(`Work is not running: ${workId}`);
    activeRun.status = "stopped";
    activeRun.completedAt = now(50 + runSequence);
    detail.summary.status = "stopped";
    detail.summary.updatedAt = activeRun.completedAt;
    return detail;
  });
  const listenToWorkEvents = vi.fn(
    async (handler: (event: WorkEventEnvelope) => void) => {
      handlers.add(handler);
      return unlisten;
    },
  );

  const client: MockTauriClient = {
    getRuntimeStatus,
    getModelConfigurationStatus,
    listModelConfigurations,
    testModelConnection,
    testSavedModelConfiguration,
    saveModelConfiguration,
    activateModelConfiguration,
    selectModelForConfiguration,
    createWork,
    listWorks,
    getWork,
    listAgentInstances,
    listCapabilityPacks,
    getWorkTeam,
    validateAgentAssembly,
    saveAgentCopy,
    addWorkMember,
    listProjectFiles,
    importResources,
    listWorkResources,
    getResourceThumbnail,
    detachDraftResource,
    startWork,
    stopWork,
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
