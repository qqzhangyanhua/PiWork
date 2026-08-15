import {
  Bot,
  BrainCircuit,
  CheckCircle2,
  Code2,
  Crown,
  Search,
  ShieldCheck,
} from "lucide-react";
import { forwardRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import type { AgentInstanceSummary, RoleKind } from "../../bindings";

const ROLE_ICONS: Record<RoleKind, ReactNode> = {
  lead: <Crown aria-hidden="true" size={18} />,
  researcher: <Search aria-hidden="true" size={18} />,
  engineer: <Code2 aria-hidden="true" size={18} />,
  reviewer: <ShieldCheck aria-hidden="true" size={18} />,
};

const effectivePermission = (member: AgentInstanceSummary) =>
  member.permissionPolicyOverride ?? member.definition.defaultPermissionPolicy;

export const TeamMemberCard = forwardRef<HTMLButtonElement, {
  isInCurrentWork: boolean;
  isLead: boolean;
  member: AgentInstanceSummary;
  onOpen(trigger: HTMLButtonElement): void;
}>(function TeamMemberCard({ isInCurrentWork, isLead, member, onOpen }, ref) {
  const { t } = useTranslation();
  const definition = member.definition;
  const systemPacks = definition.capabilityPacks.filter(
    ({ catalogCapabilityId }) => catalogCapabilityId === null,
  );
  const engine = member.engineOverride ?? definition.defaultEngineKind;
  const model = member.modelConfigurationOverride
    ?? definition.defaultModelConfigurationId
    ?? t("agentCenter.member.modelDefault");
  const responsibilitySummary = definition.responsibilities.length > 0
    ? definition.responsibilities.slice(0, 2).join(" · ")
    : definition.description;

  return (
    <button
      aria-label={t("agentCenter.member.open", { name: member.displayName })}
      className="team-member-card"
      data-role={definition.roleKind}
      onClick={(event) => onOpen(event.currentTarget)}
      ref={ref}
      type="button"
    >
      <span className="team-member-card__topline">
        <span className="team-member-card__avatar">{ROLE_ICONS[definition.roleKind] ?? <Bot aria-hidden="true" size={18} />}</span>
        <span className="team-member-card__badges">
          {isLead && <span className="team-member-card__lead">{t("agentCenter.member.lead")}</span>}
          <span className={`team-member-card__status team-member-card__status--${member.status}`}>
            <CheckCircle2 aria-hidden="true" size={11} />
            {t(`agentCenter.member.status.${member.status}`)}
          </span>
        </span>
      </span>

      <span className="team-member-card__identity">
        <strong>{member.displayName}</strong>
        <small>{t(`agentCenter.member.roles.${definition.roleKind}`)}</small>
      </span>
      <span className="team-member-card__description">{responsibilitySummary}</span>

      <span className="team-member-card__packs" aria-label={t("agentCenter.member.systemPacks")}>
        {systemPacks.map((pack) => (
          <span key={pack.id}><BrainCircuit aria-hidden="true" size={11} />{pack.name}</span>
        ))}
      </span>

      <span className="team-member-card__facts">
        <span><small>{t("agentCenter.member.permission")}</small><strong>{t(`agentCenter.member.permissions.${effectivePermission(member)}`)}</strong></span>
        <span><small>{t("agentCenter.member.runtime")}</small><strong>{engine} · {model}</strong></span>
      </span>

      <span className="team-member-card__footer">
        <span>{t("agentCenter.member.version", { version: definition.version })}</span>
        <span className={isInCurrentWork ? "is-joined" : undefined}>
          {isInCurrentWork
            ? t("agentCenter.member.inCurrentWork")
            : t("agentCenter.member.longLived")}
        </span>
      </span>
    </button>
  );
});
