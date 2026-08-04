import { ArrowRight, X } from "lucide-react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";

import type { AgentCapability } from "./agentCapabilities";

const FOCUSABLE = "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])";

export function CapabilityDetailDrawer({
  capability,
  domainName,
  returnFocusTo,
  onClose,
  onStart,
}: {
  capability: AgentCapability;
  domainName: string;
  returnFocusTo: HTMLButtonElement | null;
  onClose(): void;
  onStart(): void;
}) {
  const { t } = useTranslation();
  const drawerRef = useRef<HTMLElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    closeRef.current?.focus();
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab" || !drawerRef.current) return;
      const focusable = Array.from(drawerRef.current.querySelectorAll<HTMLElement>(FOCUSABLE));
      const first = focusable[0];
      const last = focusable.at(-1);
      if (!first || !last) return;
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      queueMicrotask(() => returnFocusTo?.focus());
    };
  }, [onClose, returnFocusTo]);

  return (
    <div className="capability-drawer-layer">
      <button
        aria-hidden="true"
        className="capability-drawer-scrim"
        onClick={onClose}
        tabIndex={-1}
        type="button"
      />
      <aside
        aria-label={capability.name}
        aria-modal="true"
        className="capability-drawer"
        ref={drawerRef}
        role="dialog"
      >
        <header className="capability-drawer__header">
          <div>
            <span className="capability-drawer__eyebrow">{domainName}</span>
            <h2>{capability.name}</h2>
          </div>
          <button
            aria-label={t("agentCenter.drawer.close")}
            className="capability-drawer__close"
            onClick={onClose}
            ref={closeRef}
            type="button"
          >
            <X aria-hidden="true" size={18} />
          </button>
        </header>

        <div className="capability-drawer__meta">
          <span className={`capability-priority capability-priority--${capability.priority.toLocaleLowerCase()}`}>
            {capability.priority}
          </span>
          <span>{t("agentCenter.drawer.catalogId", { id: capability.id })}</span>
        </div>

        <div className="capability-drawer__content">
          <section>
            <h3>{t("agentCenter.drawer.audiences")}</h3>
            <p>{capability.audiences.join("、")}</p>
          </section>
          <section>
            <h3>{t("agentCenter.drawer.coreCapability")}</h3>
            <p>{capability.coreCapability}</p>
          </section>
          <section>
            <h3>{t("agentCenter.drawer.outputs")}</h3>
            <ul>{capability.outputs.map((output) => <li key={output}>{output}</li>)}</ul>
          </section>
          <section>
            <h3>{t("agentCenter.drawer.implementation")}</h3>
            <p>{capability.implementation}</p>
          </section>
          <section className="capability-drawer__preparation">
            <h3>{t("agentCenter.drawer.preparation")}</h3>
            <ul>{capability.suggestedInputs.map((input) => <li key={input}>{input}</li>)}</ul>
          </section>
        </div>

        <footer className="capability-drawer__footer">
          <p>{t("agentCenter.drawer.startHint")}</p>
          <button className="capability-drawer__start" onClick={onStart} type="button">
            {t("agentCenter.drawer.start")}<ArrowRight aria-hidden="true" size={16} />
          </button>
        </footer>
      </aside>
    </div>
  );
}
