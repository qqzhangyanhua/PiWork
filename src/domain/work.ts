import type { WorkEventEnvelope } from "../bindings";

export type AppError = {
  code: string;
  message: string;
  details?: Record<string, unknown>;
};

export type TimelineItem = WorkEventEnvelope;
