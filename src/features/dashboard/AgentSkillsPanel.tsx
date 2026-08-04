import { Blocks, ClipboardList, FileText, FlaskConical } from "lucide-react";
import { useTranslation } from "react-i18next";

const SKILLS = [
  { key: "requirementsAnalyst", icon: ClipboardList },
  { key: "architect", icon: Blocks },
  { key: "qaEngineer", icon: FlaskConical },
  { key: "docsAssistant", icon: FileText },
] as const;

export function AgentSkillsPanel() {
  const { t } = useTranslation();
  return (
    <section aria-labelledby="agent-skills-heading" className="dashboard-panel dashboard-panel--agent-skills">
      <header className="dashboard-panel__header">
        <h2 id="agent-skills-heading">{t("dashboard.agentSkills.title")}</h2>
        <button
          aria-label={t("dashboard.comingSoon", { feature: t("dashboard.agentSkills.moreSkills") })}
          className="text-button"
          disabled
          type="button"
        >
          {t("dashboard.agentSkills.moreSkills")}
        </button>
      </header>
      <ul className="dashboard-skill-list">
        {SKILLS.map(({ key, icon: Icon }) => (
          <li key={key}>
            <span className="dashboard-skill-item__icon" aria-hidden="true"><Icon size={17} /></span>
            <span className="dashboard-skill-item__body">
              <strong>{t(`dashboard.agentSkills.${key}.title`)}</strong>
              <small>{t(`dashboard.agentSkills.${key}.body`)}</small>
            </span>
            <button
              aria-label={t("dashboard.comingSoon", { feature: t(`dashboard.agentSkills.${key}.title`) })}
              className="dashboard-skill-item__use"
              disabled
              type="button"
            >
              {t("dashboard.agentSkills.use")}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
