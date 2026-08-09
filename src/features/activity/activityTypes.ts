/*
 * Adapted from block/buzz at 5bf78671f45178f8de02ba18d3d321cbbf19cd1f,
 * Apache-2.0. Original: desktop/src/features/agents/ui/agentSessionTypes.ts.
 * PiWork changes: removes Relay/Nostr/ACP identities and adds Work/Run activity.
 */
import type { PermissionOutcome } from "../../bindings";

export type ActivityRenderClass =
  | "message"
  | "file-edit"
  | "file-read"
  | "shell"
  | "status"
  | "thought"
  | "plan"
  | "permission"
  | "error"
  | "generic"
  | "raw-rail"
  | "suppressed";

export type ActivityTone = "read" | "write" | "admin" | "neutral";
export type ActivityAction = "read" | "write" | "execute" | "invoke";
export type ToolStatus = "pending" | "executing" | "completed" | "failed";
export type ActivitySessionTransition = "created" | "resumed" | "rotated";
export type ActivityLifecycleKind =
  | "runStarted"
  | "waiting"
  | "liveness"
  | "sessionChanged"
  | "artifactProduced"
  | "validationProduced"
  | "runCompleted"
  | "runFailed";

export type ActivityIdentity = {
  workId: string;
  runId: string;
  turnId: string | null;
  sessionId: string | null;
  agentId: string | null;
  assignmentId: string | null;
};

export type ActivityDescriptor = {
  renderClass: ActivityRenderClass;
  action: ActivityAction;
  object: string | null;
  preview: string | null;
  tone: ActivityTone;
  groupKey: string | null;
};

type ActivityBase = ActivityIdentity & {
  id: string;
  timestamp: string;
};

export type ActivityItem = ActivityBase &
  (
    | { type: "message"; renderClass: "message"; text: string }
    | { type: "thought"; renderClass: "thought"; text: string }
    | {
        type: "plan";
        renderClass: "plan";
        planId: string;
        revision: number;
        text: string;
      }
    | {
        type: "tool";
        renderClass: ActivityRenderClass;
        toolCallId: string;
        toolName: string;
        status: ToolStatus;
        input: string;
        result: string;
        isError: boolean;
        descriptor: ActivityDescriptor;
      }
    | {
        type: "permission";
        renderClass: "permission";
        requestId: string;
        toolCallId: string | null;
        title: string;
        detail: string;
        status: "requested" | "resolved";
        outcome: PermissionOutcome | null;
      }
    | ({
        type: "lifecycle";
        renderClass: "status" | "error" | "suppressed";
      } & (
        | {
            activityKind: "sessionChanged";
            transition: ActivitySessionTransition;
            reason: string | null;
          }
        | {
            activityKind: Exclude<ActivityLifecycleKind, "sessionChanged">;
            detail: string | null;
          }
      ))
    | {
        type: "usage";
        renderClass: "suppressed";
        inputTokens: number;
        outputTokens: number;
        cacheReadTokens: number;
        cacheWriteTokens: number;
        totalTokens: number;
      }
    | {
        type: "raw";
        renderClass: "raw-rail" | "suppressed";
        kind: string;
        payloadJson: string;
      }
  );
