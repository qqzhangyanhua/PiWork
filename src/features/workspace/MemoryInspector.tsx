import type { WorkEventEnvelope } from "../../bindings";

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
  const candidates = summarizeMemoryCandidates(events);
  if (!candidates.length) {
    return <p className="inspector-empty">No memory candidates recorded yet.</p>;
  }
  return (
    <ul className="inspector-list">
      {candidates.map(({ id, content, status }) => (
        <li key={id} data-status={status}>
          <span className="inspector-assignment-status">{status}</span> {content}
        </li>
      ))}
    </ul>
  );
}
