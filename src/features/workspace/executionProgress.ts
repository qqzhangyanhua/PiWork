import type { WorkEventEnvelope } from "../../bindings";

export type ExecutionPhaseId =
  | "prepare"
  | "analyze"
  | "execute"
  | "validate"
  | "deliver";

export type ExecutionPhaseStatus =
  | "pending"
  | "active"
  | "completed"
  | "skipped"
  | "failed";

export type ExecutionPhase = {
  id: ExecutionPhaseId;
  status: ExecutionPhaseStatus;
  toolCount: number;
  failedToolCount: number;
};

export type ExecutionProgressModel = {
  status: "preparing" | "running" | "completed" | "failed";
  currentPhase: ExecutionPhaseId;
  phases: ExecutionPhase[];
  toolCount: number;
  failedToolCount: number;
  failureMessage: string | null;
};

type ToolPhase = Exclude<ExecutionPhaseId, "prepare" | "deliver">;

type ToolRecord = {
  phase: ToolPhase;
  failed: boolean;
};

const phaseOrder: ExecutionPhaseId[] = [
  "prepare",
  "analyze",
  "execute",
  "validate",
  "deliver",
];

const toolPhaseOrder: ToolPhase[] = ["analyze", "execute", "validate"];

const validationCommand = /\b(?:test|check|lint|build)\b/iu;

const classifyTool = (toolName: string, inputSummary: string): ToolPhase => {
  const normalized = toolName.trim().toLocaleLowerCase();
  if (/(?:^|[_-])(?:read|grep|find|ls)(?:[_-]|$)/u.test(normalized)) {
    return "analyze";
  }
  if (normalized === "bash") {
    return validationCommand.test(inputSummary) ? "validate" : "execute";
  }
  if (/(?:^|[_-])(?:edit|write)(?:[_-]|$)/u.test(normalized)) {
    return "execute";
  }
  return "execute";
};

export function buildExecutionProgress(
  events: WorkEventEnvelope[],
): ExecutionProgressModel {
  const sortedEvents = [...events].sort(
    (left, right) => left.sequence - right.sequence,
  );
  const tools = new Map<string, ToolRecord>();
  let terminal: Extract<
    WorkEventEnvelope["payload"],
    { type: "runCompleted" | "runFailed" }
  > | null = null;

  for (const { payload } of sortedEvents) {
    if (payload.type === "toolStarted") {
      const previous = tools.get(payload.toolCallId);
      tools.set(payload.toolCallId, {
        phase:
          previous?.phase ??
          classifyTool(payload.toolName, payload.inputSummary),
        failed: previous?.failed ?? false,
      });
    } else if (payload.type === "toolFinished") {
      const previous = tools.get(payload.toolCallId);
      tools.set(payload.toolCallId, {
        phase:
          previous?.phase ??
          classifyTool(payload.toolName, payload.outputSummary),
        failed: (previous?.failed ?? false) || !payload.success,
      });
    } else if (
      payload.type === "runCompleted" ||
      payload.type === "runFailed"
    ) {
      terminal = payload;
    }
  }

  const phaseTools = new Map<ToolPhase, ToolRecord[]>(
    toolPhaseOrder.map((id) => [id, []]),
  );
  for (const tool of tools.values()) {
    phaseTools.get(tool.phase)?.push(tool);
  }

  const visited = toolPhaseOrder.filter(
    (id) => (phaseTools.get(id)?.length ?? 0) > 0,
  );
  const highestVisited = visited.reduce<ToolPhase | null>((highest, id) => {
    if (!highest) return id;
    return toolPhaseOrder.indexOf(id) > toolPhaseOrder.indexOf(highest)
      ? id
      : highest;
  }, null);
  const terminalFailed = terminal?.type === "runFailed";
  const status: ExecutionProgressModel["status"] = terminal
    ? terminalFailed
      ? "failed"
      : "completed"
    : tools.size === 0
      ? "preparing"
      : "running";
  const currentPhase: ExecutionPhaseId = terminal
    ? "deliver"
    : highestVisited ?? "prepare";

  const phases = phaseOrder.map<ExecutionPhase>((id) => {
    if (id === "prepare") {
      return {
        id,
        status: tools.size === 0 && !terminal ? "active" : "completed",
        toolCount: 0,
        failedToolCount: 0,
      };
    }
    if (id === "deliver") {
      return {
        id,
        status: terminal ? (terminalFailed ? "failed" : "completed") : "pending",
        toolCount: 0,
        failedToolCount: 0,
      };
    }

    const records = phaseTools.get(id) ?? [];
    const failedToolCount = records.filter(({ failed }) => failed).length;
    let phaseStatus: ExecutionPhaseStatus;
    if (terminal) {
      if (records.length === 0) {
        phaseStatus = "skipped";
      } else if (terminalFailed && id === highestVisited) {
        phaseStatus = "failed";
      } else {
        phaseStatus = "completed";
      }
    } else if (!highestVisited) {
      phaseStatus = "pending";
    } else {
      const phaseIndex = toolPhaseOrder.indexOf(id);
      const currentIndex = toolPhaseOrder.indexOf(highestVisited);
      if (phaseIndex < currentIndex) {
        phaseStatus = records.length > 0 ? "completed" : "skipped";
      } else if (phaseIndex === currentIndex) {
        phaseStatus = "active";
      } else {
        phaseStatus = "pending";
      }
    }

    return {
      id,
      status: phaseStatus,
      toolCount: records.length,
      failedToolCount,
    };
  });

  const failedToolCount = [...tools.values()].filter(
    ({ failed }) => failed,
  ).length;

  return {
    status,
    currentPhase,
    phases,
    toolCount: tools.size,
    failedToolCount,
    failureMessage: terminal?.type === "runFailed" ? terminal.message : null,
  };
}
