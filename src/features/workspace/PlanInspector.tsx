import type { WorkEventEnvelope } from "../../bindings";

export type PlanView = {
  plans: string[];
  decisions: string[];
};

/**
 * Derives the Work Ledger's plan steps and decisions from collaboration events.
 * Later plan revisions replace earlier text for the same plan id.
 */
export function summarizePlan(events: WorkEventEnvelope[]): PlanView {
  const plans = new Map<string, { revision: number; text: string }>();
  const decisions = new Map<string, { version: number; summary: string }>();

  for (const event of events) {
    const payload = event.payload;
    if (payload.type === "workPlanUpdated") {
      const current = plans.get(payload.planId);
      if (!current || payload.revision >= current.revision) {
        plans.set(payload.planId, { revision: payload.revision, text: payload.text });
      }
    } else if (payload.type === "workDecisionRecorded") {
      const current = decisions.get(payload.decisionId);
      if (!current || payload.version >= current.version) {
        decisions.set(payload.decisionId, {
          version: payload.version,
          summary: payload.summary,
        });
      }
    }
  }

  return {
    plans: [...plans.values()].map((plan) => plan.text),
    decisions: [...decisions.values()].map((decision) => decision.summary),
  };
}

export function PlanInspector({ events }: { events: WorkEventEnvelope[] }) {
  const { plans, decisions } = summarizePlan(events);
  if (!plans.length && !decisions.length) {
    return <p className="inspector-empty">No plan or decisions recorded yet.</p>;
  }
  return (
    <div className="inspector-plan">
      {plans.length > 0 && (
        <section>
          <strong>Plan</strong>
          <ul className="inspector-list">
            {plans.map((plan, index) => (
              <li key={index}>{plan}</li>
            ))}
          </ul>
        </section>
      )}
      {decisions.length > 0 && (
        <section>
          <strong>Decisions</strong>
          <ul className="inspector-list">
            {decisions.map((decision, index) => (
              <li key={index}>{decision}</li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
