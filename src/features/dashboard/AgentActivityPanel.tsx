import { CircleCheck, ClipboardCheck, FlaskConical, Hammer, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";

const ACTIVITY_ITEMS = [
  { key: "codeReviewDone", icon: CircleCheck },
  { key: "buildRun", icon: Hammer },
  { key: "testsGenerated", icon: FlaskConical },
  { key: "depsUpdated", icon: RefreshCw },
  { key: "requirementsAnalyzed", icon: ClipboardCheck },
] as const;

export function AgentActivityPanel() {
  const { t } = useTranslation();
  return (
    <section aria-labelledby="agent-activity-heading" className="dashboard-activity">
      <header className="dashboard-panel__header">
        <h2 id="agent-activity-heading">{t("dashboard.activity.title")}</h2>
        <button
          aria-label={t("dashboard.comingSoon", { feature: t("dashboard.activity.filterAll") })}
          className="dashboard-activity__filter"
          disabled
          type="button"
        >
          {t("dashboard.activity.filterAll")}
        </button>
      </header>
      <ul className="dashboard-activity__list">
        {ACTIVITY_ITEMS.map(({ key, icon: Icon }) => (
          <li key={key}>
            <span aria-hidden="true" className="dashboard-activity__icon"><Icon size={15} /></span>
            <span className="dashboard-activity__body">
              <span className="dashboard-activity__title-row">
                <strong>{t(`dashboard.activity.items.${key}.title`)}</strong>
                <time>{t(`dashboard.activity.items.${key}.time`)}</time>
              </span>
              <small className="dashboard-activity__path">{t(`dashboard.activity.items.${key}.path`)}</small>
              <small className="dashboard-activity__desc">{t(`dashboard.activity.items.${key}.body`)}</small>
            </span>
          </li>
        ))}
      </ul>
      <button
        aria-label={t("dashboard.comingSoon", { feature: t("dashboard.activity.viewAll") })}
        className="text-button dashboard-activity__view-all"
        disabled
        type="button"
      >
        {t("dashboard.activity.viewAll")}
      </button>
    </section>
  );
}
