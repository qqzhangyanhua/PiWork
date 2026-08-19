import { useState } from "react";

import type { WorkEventEnvelope } from "../../bindings";
import { useOptionalWorkStoreContext } from "../works/WorkStoreProvider";

export type MemoryCandidateView = {
  id: string;
  content: string;
  status: "proposed" | "confirmed" | "rejected";
};

/**
 * Derives Memory candidates and their resolution status from events. A
 * candidate is proposed by `memoryCandidateProposed` and resolved by
 * `memoryCandidateResolved`.
 */
export function summarizeMemoryCandidates(
  events: WorkEventEnvelope[],
): MemoryCandidateView[] {
  const candidates = new Map<string, MemoryCandidateView>();
  for (const event of events) {
    const payload = event.payload;
    if (payload.type === "memoryCandidateProposed") {
      candidates.set(payload.candidateId, {
        id: payload.candidateId,
        content: payload.content,
        status: "proposed",
      });
    } else if (payload.type === "memoryCandidateResolved") {
      const current = candidates.get(payload.candidateId);
      if (current) {
        current.status = payload.status;
      }
    }
  }
  return [...candidates.values()];
}

export function MemoryInspector({ events }: { events: WorkEventEnvelope[] }) {
  const context = useOptionalWorkStoreContext();
  const [resolvingId, setResolvingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const candidates = summarizeMemoryCandidates(events);
  if (!candidates.length) {
    return <p className="inspector-empty">No memory candidates recorded yet.</p>;
  }

  const resolve = async (candidateId: string, confirm: boolean) => {
    if (!context) return;
    setResolvingId(candidateId);
    setError(null);
    try {
      await context.client.resolveMemoryCandidate(candidateId, confirm);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setResolvingId(null);
    }
  };

  return (
    <>
      {error && <p className="inspector-error">{error}</p>}
      <ul className="inspector-list">
        {candidates.map(({ id, content, status }) => (
          <li key={id} data-status={status}>
            <span className="inspector-assignment-status">{status}</span> {content}
            {status === "proposed" && context && (
              <span className="inspector-memory-actions">
                <button
                  type="button"
                  disabled={resolvingId === id}
                  onClick={() => void resolve(id, true)}
                >
                  Confirm
                </button>
                <button
                  type="button"
                  disabled={resolvingId === id}
                  onClick={() => void resolve(id, false)}
                >
                  Reject
                </button>
              </span>
            )}
          </li>
        ))}
      </ul>
    </>
  );
}
