import { ArrowUpRight, Bot } from "lucide-react";
import { forwardRef } from "react";
import { useTranslation } from "react-i18next";

import type { AgentCapability } from "./agentCapabilities";

export const CapabilityCard = forwardRef<HTMLButtonElement, {
  capability: AgentCapability;
  domainName: string;
  onOpen(trigger: HTMLButtonElement): void;
}>(function CapabilityCard({ capability, domainName, onOpen }, ref) {
  const { t } = useTranslation();

  return (
    <button
      aria-label={t("agentCenter.capability.open", { name: capability.name })}
      className="capability-card"
      data-domain={capability.domainId}
      onClick={(event) => onOpen(event.currentTarget)}
      ref={ref}
      type="button"
    >
      <span className="capability-card__topline">
        <span className="capability-card__icon" aria-hidden="true"><Bot size={17} /></span>
        <span className={`capability-priority capability-priority--${capability.priority.toLocaleLowerCase()}`}>
          {capability.priority}
        </span>
      </span>
      <span className="capability-card__body">
        <strong>{capability.name}</strong>
        <small>{domainName}</small>
        <span>{capability.coreCapability}</span>
      </span>
      <span className="capability-card__action" aria-hidden="true">
        {t("agentCenter.capability.viewDetails")}<ArrowUpRight size={14} />
      </span>
    </button>
  );
});
