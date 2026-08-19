import type { WorkEventEnvelope } from "../../bindings";

/**
 * Derives the distinct participating Agents from Work events. Assignment events
 * carry the assigned agent, and every Run event carries the emitting agent.
 */
export function summarizeTeam(events: WorkEventEnvelope[]): string[] {
  const agents = new Set<string>();
  for (const event of events) {
    if (event.agentId) {
      agents.add(event.agentId);
    }
    const payload = event.payload;
    if ("assignedAgentId" in payload && payload.assignedAgentId) {
      agents.add(payload.assignedAgentId);
    }
    if ("agentInstanceId" in payload && payload.agentInstanceId) {
      agents.add(payload.agentInstanceId);
    }
  }
  return [...agents].sort();
}

export function TeamInspector({ events }: { events: WorkEventEnvelope[] }) {
  const agents = summarizeTeam(events);
  if (!agents.length) {
    return <p className="inspector-empty">No team members recorded yet.</p>;
  }
  return (
    <ul className="inspector-list">
      {agents.map((agentId) => (
        <li key={agentId}>{agentId}</li>
      ))}
    </ul>
  );
}
