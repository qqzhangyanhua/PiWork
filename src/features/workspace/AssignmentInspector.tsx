import type { WorkEventEnvelope } from "../../bindings";

export type AssignmentRow = {
  id: string;
  status: string;
  detail: string;
};

/**
 * Aggregates Assignment-scoped collaboration events into one row per
 * Assignment, keeping the latest event for each Assignment id.
 */
export function summarizeAssignments(
  events: WorkEventEnvelope[],
): AssignmentRow[] {
  const latest = new Map<string, AssignmentRow>();
  for (const event of events) {
    const payload = event.payload;
    if (!("assignmentId" in payload) || !payload.assignmentId) {
      continue;
    }
    const id = payload.assignmentId;
    latest.set(id, {
      id,
      status: payload.type,
      detail: assignmentDetail(payload.type, payload),
    });
  }
  return [...latest.values()];
}

function assignmentDetail(
  type: string,
  payload: WorkEventEnvelope["payload"],
): string {
  switch (type) {
    case "assignmentQueued":
      return "title" in payload && typeof payload.title === "string"
        ? payload.title
        : "Queued";
    case "assignmentCompleted":
      return "resultSummary" in payload && typeof payload.resultSummary === "string"
        ? payload.resultSummary
        : "Completed";
    case "assignmentFailed":
      return "error" in payload && typeof payload.error === "string"
        ? payload.error
        : "Failed";
    case "assignmentCancelled":
    case "assignmentInterrupted":
      return "reason" in payload && typeof payload.reason === "string"
        ? payload.reason
        : type;
    case "assignmentDeadLettered":
      return "error" in payload && typeof payload.error === "string"
        ? payload.error
        : "Dead lettered";
    case "assignmentDelegated":
      return "title" in payload && typeof payload.title === "string"
        ? `Delegated: ${payload.title}`
        : "Delegated";
    case "assignmentResultSubmitted":
      return "summary" in payload && typeof payload.summary === "string"
        ? payload.summary
        : "Result submitted";
    case "assignmentResultRejected":
      return "reason" in payload && typeof payload.reason === "string"
        ? `Rejected: ${payload.reason}`
        : "Result rejected";
    case "leadResumed":
      return "Lead resumed";
    default:
      return type;
  }
}

export function AssignmentInspector({ events }: { events: WorkEventEnvelope[] }) {
  const assignments = summarizeAssignments(events);
  if (!assignments.length) {
    return <p className="inspector-empty">No assignments recorded yet.</p>;
  }
  return (
    <ul className="inspector-list">
      {assignments.map(({ id, status, detail }) => (
        <li key={id}>
          <span className="inspector-assignment-status">{status}</span>{" "}
          {detail}
        </li>
      ))}
    </ul>
  );
}
