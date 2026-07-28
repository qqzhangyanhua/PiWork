import { CornerDownLeft } from "lucide-react";
import { type Ref, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { WorkSummary } from "../../bindings";
import { didPersistStartInstruction } from "../works/workStore";
import { useWorkStore } from "../works/WorkStoreProvider";

const queueStatuses: WorkSummary["status"][] = ["queued", "running", "waiting"];
const continueStatuses: WorkSummary["status"][] = [
  "completed", "failed", "stopped", "interrupted", "idle",
];

type WorkComposerProps = {
  promptRef?: Ref<HTMLTextAreaElement>;
  work: WorkSummary;
};

export function WorkComposer({ promptRef, work }: WorkComposerProps) {
  const { t } = useTranslation();
  const [prompt, setPrompt] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const submittingRef = useRef(false);
  const startWork = useWorkStore((state) => state.startWork);
  const queueInstruction = useWorkStore((state) => state.queueInstruction);
  const queuedInstructions = useWorkStore((state) => state.queuedInstructions);
  const queued = queuedInstructions[work.id] ?? [];
  const loading = useWorkStore((state) => state.loading);
  const shouldQueue = queueStatuses.includes(work.status);
  const actionLabel = continueStatuses.includes(work.status)
      ? t("composer.continue")
      : t("composer.send");

  const submit = async () => {
    const instruction = prompt.trim();
    if (!instruction) return;
    if (shouldQueue) {
      queueInstruction(work.id, instruction);
      setPrompt("");
      return;
    }
    if (submittingRef.current) return;
    submittingRef.current = true;
    setSubmitting(true);
    try {
      await startWork(work.id, instruction);
      setPrompt("");
    } catch (error) {
      if (didPersistStartInstruction(error)) {
        setPrompt("");
      }
      // The store normalizes and exposes the error in the product UI.
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  return (
    <footer className="work-composer">
      {queued.length > 0 && <p className="queue-count">{t("composer.queued", { count: queued.length })}</p>}
      <div className="work-composer__box">
        <label className="sr-only" htmlFor="work-prompt">{t("composer.label")}</label>
        <textarea
          ref={promptRef}
          id="work-prompt"
          placeholder={t("composer.placeholder")}
          value={prompt}
          onChange={(event) => setPrompt(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              void submit();
            }
          }}
        />
        <button className="button button--primary" type="button" disabled={!prompt.trim() || submitting || (loading && !shouldQueue)} onClick={() => void submit()}>
          <CornerDownLeft aria-hidden="true" size={15} />
          {actionLabel}
        </button>
      </div>
    </footer>
  );
}
