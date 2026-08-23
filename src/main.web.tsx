// Browser demo entry for PiWork's React UI.
//
// The production app is a Tauri desktop app and its frontend gates on
// `isTauri()`; this entry exists purely for viewing the UI in a plain browser
// (`pnpm dev` → http://127.0.0.1:1420/web-demo.html). It injects a self-contained
// in-memory client with a seeded multi-agent Work so the Team / Assignments /
// Plan / Memory Inspector tabs and the timeline are populated without a
// backend. Nothing here is used by the desktop build.

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./i18n";
import { App } from "./app/App";
import type { MemorySettingsSummary, PiWorkClient } from "./app/tauriClient";
import type {
  AgentInstanceSummary,
  AssignmentSummary,
  CapabilityPackSummary,
  MessageSummary,
  ResourceSummary,
  RunSummary,
  StartWorkOutput,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
  WorkTeamSummary,
} from "./bindings";
import "./styles/globals.css";

const now = (offset: number) =>
  new Date(Date.UTC(2026, 6, 28, 8, 0, offset)).toISOString();

const leadId = "agent-instance:piwork-lead";
const researcherId = "agent-instance:piwork-researcher";
const leadAssignmentId = "assignment-lead-1";
const childAssignmentId = "assignment-research-1";
const leadRun1 = "run-lead-1";
const childRun = "run-research-1";
const leadRun2 = "run-lead-2";

const pack = (id: string, name: string): CapabilityPackSummary => ({
  id,
  catalogCapabilityId: null,
  name,
  description: `${name} executable capability`,
  instructions: `Act as ${name}`,
  inputSchema: {},
  outputSchema: {},
  procedure: {},
  validationRubric: {},
  requiredTools: ["read", "grep", "find", "ls"],
  defaultPermissionScope: "read_only",
  compatibleRoleTemplateIds: [],
  requiredEngineCapabilities: [],
  conflictsWithCapabilityPackIds: [],
  version: 1,
  status: "executable",
});

const instance = (
  id: string,
  roleKind: "lead" | "researcher",
  slug: string,
  packId: string,
  packName: string,
): AgentInstanceSummary => ({
  id,
  definition: {
    id: `agent-definition:${slug}:v1`,
    roleTemplateId: `role-template:${slug}:v1`,
    roleKind,
    slug,
    name: slug,
    description: `PiWork builtin ${slug}`,
    instructions: `Act as the ${slug}`,
    responsibilities: [],
    nonResponsibilities: [],
    inputContract: {},
    resultContract: {},
    qualityRubric: {},
    defaultEngineKind: "pi",
    defaultModelConfigurationId: null,
    defaultPermissionPolicy: roleKind === "lead" ? "inherit_work" : "read_only",
    defaultParallelism: 1,
    memoryPolicy: "confirmed_only",
    capabilityPacks: [pack(packId, packName)],
    builtin: true,
    active: true,
    version: 1,
    createdAt: now(0),
    updatedAt: now(0),
  },
  displayName: slug,
  engineOverride: null,
  modelConfigurationOverride: null,
  permissionPolicyOverride: null,
  parallelismOverride: null,
  builtin: true,
  status: "active",
  createdAt: now(0),
  updatedAt: now(0),
});

const lead = instance(leadId, "lead", "lead", "capability-pack:lead-coordination:v1", "lead coordination");
const researcher = instance(researcherId, "researcher", "researcher", "capability-pack:source-research:v1", "source research");

const team = (): WorkTeamSummary => ({
  workId: "work-demo",
  lead: {
    workId: "work-demo",
    instance: lead,
    roleKind: "lead",
    status: "joined",
    permissionPolicy: "inherit_work",
    joinedAt: now(1),
    updatedAt: now(1),
  },
  members: [
    {
      workId: "work-demo",
      instance: researcher,
      roleKind: "researcher",
      status: "joined",
      permissionPolicy: "read_only",
      joinedAt: now(2),
      updatedAt: now(2),
    },
  ],
});

const demoWorkSummary: WorkSummary = {
  id: "work-demo",
  workspaceId: "workspace-demo",
  title: "营收看板",
  goal: "构建营收看板",
  rootPath: "D:/workspace/revenue",
  permissionMode: "balanced",
  status: "completed",
  createdAt: now(1),
  updatedAt: now(240),
};

const draftWorkSummary: WorkSummary = {
  id: "work-draft",
  workspaceId: "workspace-docs",
  title: "文档翻译",
  goal: "翻译 README 到中文",
  rootPath: "D:/workspace/docs",
  permissionMode: "balanced",
  status: "draft",
  createdAt: now(2),
  updatedAt: now(2),
};

const runs: RunSummary[] = [
  {
    id: leadRun1,
    workId: "work-demo",
    assignmentId: leadAssignmentId,
    agentInstanceId: leadId,
    engineKind: "pi",
    engineSessionId: "session-lead-1",
    modelLabel: "GPT-5.2",
    status: "completed",
    createdAt: now(10),
    startedAt: now(10),
    completedAt: now(90),
  },
  {
    id: childRun,
    workId: "work-demo",
    assignmentId: childAssignmentId,
    agentInstanceId: researcherId,
    engineKind: "pi",
    engineSessionId: "session-research-1",
    modelLabel: "GPT-5.2",
    status: "completed",
    createdAt: now(100),
    startedAt: now(100),
    completedAt: now(170),
  },
  {
    id: leadRun2,
    workId: "work-demo",
    assignmentId: leadAssignmentId,
    agentInstanceId: leadId,
    engineKind: "pi",
    engineSessionId: "session-lead-1",
    modelLabel: "GPT-5.2",
    status: "completed",
    createdAt: now(180),
    startedAt: now(180),
    completedAt: now(240),
  },
];

const messages: MessageSummary[] = [
  {
    id: "message-1",
    workId: "work-demo",
    runId: leadRun1,
    role: "user",
    content: "构建营收看板",
    resourceIds: [],
    createdAt: now(10),
  },
  {
    id: "message-2",
    workId: "work-demo",
    runId: leadRun2,
    role: "assistant",
    content: "已根据研究员的核验结果完成营收看板交付。",
    resourceIds: [],
    createdAt: now(230),
  },
];

const event = (
  sequence: number,
  runId: string | null,
  payload: WorkEventEnvelope["payload"],
  overrides: Partial<WorkEventEnvelope> = {},
): WorkEventEnvelope => ({
  version: 2,
  eventId: `event-demo-${sequence}`,
  workId: "work-demo",
  runId,
  turnId: runId ?? undefined,
  correlationId:
    runId ?? ("assignmentId" in payload ? payload.assignmentId : undefined),
  sequence,
  occurredAt: now(5 + sequence),
  payload,
  ...overrides,
});

const events: WorkEventEnvelope[] = [
  // Lead Assignment starts the work.
  event(1, null, {
    type: "assignmentQueued",
    assignmentId: leadAssignmentId,
    assignedAgentId: leadId,
    title: "构建营收看板",
    priority: 0,
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(2, null, {
    type: "assignmentClaimed",
    assignmentId: leadAssignmentId,
    agentInstanceId: leadId,
    agentSessionId: "session-lead-1",
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(3, leadRun1, {
    type: "assignmentStarted",
    assignmentId: leadAssignmentId,
    agentInstanceId: leadId,
    agentSessionId: "session-lead-1",
    runId: leadRun1,
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(4, leadRun1, { type: "runStarted", modelLabel: "GPT-5.2" }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(5, leadRun1, { type: "assistantDelta", text: "我先梳理团队，营收口径需要核验，我会委派研究员。" }, { agentId: leadId, assignmentId: leadAssignmentId }),
  // Lead delegates to the researcher.
  event(6, leadRun1, {
    type: "toolStarted",
    toolCallId: "tool-1",
    toolName: "delegate_assignment",
    inputSummary: "委派研究员核验营收数据口径",
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(7, null, {
    type: "assignmentDelegated",
    assignmentId: childAssignmentId,
    parentAssignmentId: leadAssignmentId,
    assignedAgentId: researcherId,
    title: "核验营收数据口径",
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(8, leadRun1, {
    type: "toolFinished",
    toolCallId: "tool-1",
    toolName: "delegate_assignment",
    outputSummary: "已接受子任务，等待执行",
    success: true,
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(9, leadRun1, {
    type: "assignmentWaiting",
    assignmentId: leadAssignmentId,
    agentInstanceId: leadId,
    agentSessionId: "session-lead-1",
    reason: "waiting_on_assignments",
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  // Researcher executes the child Assignment.
  event(10, null, {
    type: "assignmentQueued",
    assignmentId: childAssignmentId,
    assignedAgentId: researcherId,
    title: "核验营收数据口径",
    priority: 0,
  }, { agentId: researcherId, assignmentId: childAssignmentId }),
  event(11, null, {
    type: "assignmentClaimed",
    assignmentId: childAssignmentId,
    agentInstanceId: researcherId,
    agentSessionId: "session-research-1",
  }, { agentId: researcherId, assignmentId: childAssignmentId }),
  event(12, childRun, {
    type: "assignmentStarted",
    assignmentId: childAssignmentId,
    agentInstanceId: researcherId,
    agentSessionId: "session-research-1",
    runId: childRun,
  }, { agentId: researcherId, assignmentId: childAssignmentId }),
  event(13, childRun, { type: "runStarted", modelLabel: "GPT-5.2" }, { agentId: researcherId, assignmentId: childAssignmentId }),
  event(14, childRun, { type: "assistantDelta", text: "我核对了财务月报与销售系统的口径差异。" }, { agentId: researcherId, assignmentId: childAssignmentId }),
  event(15, childRun, {
    type: "toolStarted",
    toolCallId: "tool-2",
    toolName: "submit_assignment_result",
    inputSummary: "提交核验结果",
  }, { agentId: researcherId, assignmentId: childAssignmentId }),
  event(16, childRun, {
    type: "toolFinished",
    toolCallId: "tool-2",
    toolName: "submit_assignment_result",
    outputSummary: "结果已受理",
    success: true,
  }, { agentId: researcherId, assignmentId: childAssignmentId }),
  event(17, null, {
    type: "assignmentResultSubmitted",
    assignmentId: childAssignmentId,
    agentInstanceId: researcherId,
    status: "completed",
    summary: "数据口径核验通过：以财务月报为准",
  }, { agentId: researcherId, assignmentId: childAssignmentId }),
  event(18, null, {
    type: "memoryCandidateProposed",
    candidateId: "candidate-1",
    authorAgentId: researcherId,
    content: "营收数据以财务月报口径为准，销售系统含预收款差异",
  }, { agentId: researcherId, assignmentId: childAssignmentId }),
  // Lead resumes with the child's result and delivers.
  event(19, null, {
    type: "leadResumed",
    assignmentId: leadAssignmentId,
    dependencyGeneration: 1,
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(20, null, {
    type: "assignmentQueued",
    assignmentId: leadAssignmentId,
    assignedAgentId: leadId,
    title: "综合成员结果并交付",
    priority: 0,
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(21, null, {
    type: "assignmentClaimed",
    assignmentId: leadAssignmentId,
    agentInstanceId: leadId,
    agentSessionId: "session-lead-1",
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(22, leadRun2, {
    type: "assignmentStarted",
    assignmentId: leadAssignmentId,
    agentInstanceId: leadId,
    agentSessionId: "session-lead-1",
    runId: leadRun2,
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(23, leadRun2, { type: "runStarted", modelLabel: "GPT-5.2" }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(24, leadRun2, { type: "assistantDelta", text: "研究员已确认口径，我据此构建看板并记录计划。" }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(25, leadRun2, {
    type: "workPlanUpdated",
    planId: "plan-1",
    revision: 1,
    text: "[{\"id\":\"step-1\",\"title\":\"核验营收口径\",\"status\":\"completed\"},{\"id\":\"step-2\",\"title\":\"构建看板\",\"status\":\"completed\"}]",
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(26, leadRun2, {
    type: "workDecisionRecorded",
    decisionId: "decision-1",
    summary: "营收口径以财务月报为准",
    version: 1,
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(27, leadRun2, {
    type: "toolStarted",
    toolCallId: "tool-3",
    toolName: "complete_work_delivery",
    inputSummary: "提交最终交付",
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(28, leadRun2, {
    type: "toolFinished",
    toolCallId: "tool-3",
    toolName: "complete_work_delivery",
    outputSummary: "交付已受理",
    success: true,
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(29, null, {
    type: "workDeliveryCompleted",
    summary: "营收看板已交付：含月度、季度与年度视图",
    artifacts: ["dist/revenue-dashboard.html", "docs/README.md"],
    validation: ["Dashboard checks passed"],
    limitations: ["数据源为财务月报导出"],
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
  event(30, leadRun2, {
    type: "runCompleted",
    summary: "营收看板已交付：含月度、季度与年度视图",
    artifacts: ["dist/revenue-dashboard.html", "docs/README.md"],
    validation: ["Dashboard checks passed"],
    limitations: ["数据源为财务月报导出"],
  }, { agentId: leadId, assignmentId: leadAssignmentId }),
];

const demoDetail: WorkDetail = {
  summary: demoWorkSummary,
  runs,
  messages,
  events,
};

const draftDetail: WorkDetail = {
  summary: draftWorkSummary,
  runs: [],
  messages: [],
  events: [],
};

const demoAssignments: AssignmentSummary[] = [
  {
    id: leadAssignmentId,
    workId: "work-demo",
    parentAssignmentId: null,
    createdByAgentId: null,
    assignedAgentId: leadId,
    capabilityPackId: "capability-pack:lead-coordination:v1",
    kind: "lead",
    sideEffect: "unknown",
    title: "构建营收看板",
    instruction: "构建营收看板",
    contextManifest: {},
    expectedResultSchema: {},
    acceptanceCriteria: [],
    permissionScope: {},
    priority: 0,
    status: "completed",
    attemptCount: 1,
    maxAttempts: 3,
    notBefore: null,
    resultSummary: "营收看板已交付：含月度、季度与年度视图",
    lastError: null,
    nextAttemptAt: null,
    recoveryReason: null,
    createdAt: now(10),
    claimedAt: now(11),
    startedAt: now(12),
    completedAt: now(240),
    updatedAt: now(240),
  },
  {
    id: childAssignmentId,
    workId: "work-demo",
    parentAssignmentId: leadAssignmentId,
    createdByAgentId: leadId,
    assignedAgentId: researcherId,
    capabilityPackId: "capability-pack:source-research:v1",
    kind: "member",
    sideEffect: "read_only",
    title: "核验营收数据口径",
    instruction: "核验营收数据口径并提交结果",
    contextManifest: {},
    expectedResultSchema: {},
    acceptanceCriteria: [],
    permissionScope: {},
    priority: 0,
    status: "completed",
    attemptCount: 1,
    maxAttempts: 3,
    notBefore: null,
    resultSummary: "数据口径核验通过：以财务月报为准",
    lastError: null,
    nextAttemptAt: null,
    recoveryReason: null,
    createdAt: now(100),
    claimedAt: now(101),
    startedAt: now(102),
    completedAt: now(170),
    updatedAt: now(170),
  },
];

const memoryCandidates = [
  {
    id: "candidate-1",
    sourceWorkId: "work-demo",
    sourceEventId: "event-demo-18",
    authorAgentId: researcherId,
    content: "营收数据以财务月报口径为准，销售系统含预收款差异",
    reason: "研究员核验结论",
    version: 1,
    status: "proposed" as const,
    createdAt: now(118),
  },
];

let demoMemorySettings: MemorySettingsSummary = {
  enabled: true,
  hubEndpoint: "http://124.221.254.61",
  endpoint: "http://124.221.254.61/mem",
  authMode: "basic",
  authUsername: "tdai",
  allowInsecureHttp: false,
  serviceId: "default",
  teamId: "team-eb16plgnne",
  userId: "usr-1faz3ley78",
  requestTimeoutMs: 5000,
  recallTimeoutMs: 1500,
  maxRecallItems: 8,
  maxRecallChars: 6000,
  captureEnabled: true,
  recallEnabled: true,
  apiKeyConfigured: false,
  userKeyConfigured: false,
};

let demoMemoryBindings = [
  { rootPath: demoWorkSummary.rootPath, taskId: "task-revenue", enabled: true, captureEnabled: true, recallEnabled: true, pendingCaptureCount: 0 },
  { rootPath: draftWorkSummary.rootPath, taskId: "task-draft", enabled: false, captureEnabled: true, recallEnabled: true, pendingCaptureCount: 0 },
];

const demoClient: PiWorkClient = {
  getDefaultProjectDirectory: async () => "D:/workspace",
  getRuntimeStatus: async () => ({
    python: { available: true, version: "3.11.6" },
    node: { available: true, version: "18.20.2" },
    git: { available: true, version: null },
  }),
  getModelConfigurationStatus: async () => ({
    configured: true,
    configuration: {
      id: "openai-default",
      provider: "openai",
      baseUrl: "https://api.openai.com/v1",
      modelId: "gpt-5.2",
      active: true,
      credentialConfigured: true,
    },
  }),
  listModelConfigurations: async () => [],
  testModelConnection: async () => ({ models: [] }),
  testSavedModelConfiguration: async () => ({ models: [] }),
  saveModelConfiguration: async (input) => ({
    id: input.id ?? "model-new",
    provider: input.provider,
    baseUrl: input.baseUrl,
    modelId: input.modelId,
    active: false,
    credentialConfigured: true,
  }),
  activateModelConfiguration: async (id) => ({
    id,
    provider: "openai",
    baseUrl: "https://api.openai.com/v1",
    modelId: "gpt-5.2",
    active: true,
    credentialConfigured: true,
  }),
  selectModelForConfiguration: async (input) => ({
    id: input.configurationId,
    provider: "openai",
    baseUrl: "https://api.openai.com/v1",
    modelId: input.modelId,
    active: true,
    credentialConfigured: true,
  }),
  getMemorySettings: async () => structuredClone(demoMemorySettings),
  saveMemorySettings: async (input) => {
    demoMemorySettings = {
      ...input,
      apiKeyConfigured: Boolean(input.apiKey) || demoMemorySettings.apiKeyConfigured,
      userKeyConfigured: Boolean(input.userKey) || demoMemorySettings.userKeyConfigured,
    };
    return structuredClone(demoMemorySettings);
  },
  testMemoryConnection: async () => ({
    healthy: true,
    authenticated: true,
    latencyMs: 84,
    failureCode: null,
    resolvedUserId: demoMemorySettings.userId,
  }),
  listWorkspaceMemoryBindings: async () => structuredClone(demoMemoryBindings),
  saveWorkspaceMemoryBinding: async (input) => {
    const existing = demoMemoryBindings.find(({ rootPath }) => rootPath === input.rootPath);
    const saved = { ...input, taskId: existing?.taskId ?? "task-auto", pendingCaptureCount: 0 };
    demoMemoryBindings = demoMemoryBindings.some(({ rootPath }) => rootPath === input.rootPath)
      ? demoMemoryBindings.map((binding) => binding.rootPath === input.rootPath ? saved : binding)
      : [...demoMemoryBindings, saved];
    return structuredClone(saved);
  },
  drainMemoryCaptureOutbox: async () => 0,
  createWork: async (input) => ({
    summary: {
      id: `work-${Date.now()}`,
      workspaceId: `workspace-${input.rootPath}`,
      title: input.title,
      goal: input.goal,
      rootPath: input.rootPath,
      permissionMode: input.permissionMode,
      status: "draft",
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    },
    runs: [],
    messages: [],
    events: [],
  }),
  listWorks: async () => [demoWorkSummary, draftWorkSummary],
  getWork: async (workId) => {
    if (workId === "work-demo") return structuredClone(demoDetail);
    if (workId === "work-draft") return structuredClone(draftDetail);
    throw new Error(`Work not found: ${workId}`);
  },
  listAgentInstances: async () => [lead, researcher],
  listCapabilityPacks: async () => [
    pack("capability-pack:lead-coordination:v1", "lead coordination"),
    pack("capability-pack:source-research:v1", "source research"),
  ],
  getWorkTeam: async () => team(),
  validateAgentAssembly: async () => [],
  saveAgentCopy: async (input) => ({
    ...lead,
    id: `agent-instance:local:${Date.now()}`,
    displayName: input.displayName,
  }),
  addWorkMember: async () => team(),
  listProjectFiles: async () => [
    { relativePath: "src/main.ts" },
    { relativePath: "src/revenue.ts" },
    { relativePath: "README.md" },
  ],
  importResources: async (input) =>
    input.sourcePaths.map((sourcePath) => ({
      id: `resource-${Date.now()}-${sourcePath.length}`,
      originalName: sourcePath.split(/[\\/]/u).at(-1) || "attachment",
      mediaType: "image/png",
      size: 68n,
      origin: "user_upload" as const,
      status: "ready" as const,
      failureCode: null,
      createdAt: new Date().toISOString(),
    })),
  listWorkResources: async () => [],
  getResourceThumbnail: async () => ({ mediaType: "image/png", dataBase64: "" }),
  detachDraftResource: async () => undefined,
  startWork: async (workId, prompt) => {
    const detail = workId === "work-demo" ? demoDetail : draftDetail;
    const runId = `run-${Date.now()}`;
    const run: RunSummary = {
      id: runId,
      workId,
      assignmentId: null,
      agentInstanceId: null,
      engineKind: "fake",
      engineSessionId: `session-${runId}`,
      modelLabel: "Fake model",
      status: "running",
      createdAt: new Date().toISOString(),
      startedAt: new Date().toISOString(),
      completedAt: null,
    };
    detail.runs.push(run);
    detail.messages.push({
      id: `message-${Date.now()}`,
      workId,
      runId,
      role: "user",
      content: prompt.trim(),
      resourceIds: [],
      createdAt: run.createdAt,
    });
    detail.summary.status = "running";
    detail.summary.updatedAt = run.createdAt;
    return {
      assignment: {
        id: `assignment-${Date.now()}`,
        workId,
        parentAssignmentId: null,
        createdByAgentId: null,
        assignedAgentId: leadId,
        capabilityPackId: null,
        kind: "lead",
        sideEffect: "unknown",
        title: prompt.trim(),
        instruction: prompt.trim(),
        contextManifest: {},
        expectedResultSchema: {},
        acceptanceCriteria: [],
        permissionScope: {},
        priority: 0,
        status: "queued",
        attemptCount: 0,
        maxAttempts: 3,
        notBefore: null,
        resultSummary: null,
        lastError: null,
        nextAttemptAt: null,
        recoveryReason: null,
        createdAt: run.createdAt,
        claimedAt: null,
        startedAt: null,
        completedAt: null,
        updatedAt: run.createdAt,
      },
      run,
      userMessage: {
        id: `message-${Date.now()}`,
        workId,
        runId,
        role: "user",
        content: prompt.trim(),
        resourceIds: [],
        createdAt: run.createdAt,
      },
    } as StartWorkOutput;
  },
  stopWork: async (workId) => {
    const detail = workId === "work-demo" ? demoDetail : draftDetail;
    detail.summary.status = "stopped";
    detail.summary.updatedAt = new Date().toISOString();
    return structuredClone(detail);
  },
  archiveWork: async (workId) => {
    const detail = workId === "work-demo" ? demoDetail : draftDetail;
    detail.summary.status = "archived";
    detail.summary.updatedAt = new Date().toISOString();
    return structuredClone(detail);
  },
  restoreWork: async (workId) => {
    const detail = workId === "work-demo" ? demoDetail : draftDetail;
    detail.summary.status = "idle";
    detail.summary.updatedAt = new Date().toISOString();
    return structuredClone(detail);
  },
  drainAssignmentEventOutbox: async () => undefined,
  listWorkAssignments: async (workId) =>
    workId === "work-demo" ? structuredClone(demoAssignments) : [],
  queueWorkInput: async (workId, input) =>
    demoClient.startWork(workId, input.instruction, input.referencedFiles, input.resourceIds),
  confirmAssignmentRecovery: async (assignmentId, resume) => {
    const found = demoAssignments.find(({ id }) => id === assignmentId);
    if (!found) throw new Error(`Assignment not found: ${assignmentId}`);
    found.status = resume ? "queued" : "cancelled";
    return structuredClone(found);
  },
  interruptAndReplace: async (workId, input) =>
    demoClient.startWork(workId, input.replacement.instruction, input.replacement.referencedFiles, input.replacement.resourceIds),
  listMemoryCandidates: async (workId) =>
    workId === "work-demo" ? structuredClone(memoryCandidates) : [],
  resolveMemoryCandidate: async (candidateId, confirm) => {
    const found = memoryCandidates.find(({ id }) => id === candidateId);
    if (!found) throw new Error(`Candidate not found: ${candidateId}`);
    const resolvedAt = new Date().toISOString();
    if (confirm) {
      Object.assign(found, { status: "confirmed" as const, resolvedAt, resolvedBy: "user" });
    } else {
      Object.assign(found, { status: "rejected" as const, resolvedAt, resolvedBy: "user" });
    }
    return structuredClone(found);
  },
  listenToWorkEvents: async () => () => undefined,
};

const rootElement = document.getElementById("root");

if (!rootElement) {
  throw new Error("Missing #root mount point");
}

createRoot(rootElement).render(
  <StrictMode>
    <App client={demoClient} />
  </StrictMode>,
);
