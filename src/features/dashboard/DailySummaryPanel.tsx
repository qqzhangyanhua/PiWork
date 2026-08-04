import { Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";

const CHART_POINTS = "0,34 16,28 32,30 48,18 64,22 80,10 96,14 112,4 128,8";

export function DailySummaryPanel() {
  const { t } = useTranslation();
  return (
    <section aria-labelledby="daily-summary-heading" className="dashboard-summary">
      <header className="dashboard-summary__header">
        <Sparkles aria-hidden="true" size={15} />
        <h2 id="daily-summary-heading">{t("dashboard.summary.title")}</h2>
      </header>
      <dl className="dashboard-summary__stats">
        <div>
          <dt>{t("dashboard.summary.tasksCompleted")}</dt>
          <dd>12</dd>
        </div>
        <div>
          <dt>{t("dashboard.summary.timeSaved")}</dt>
          <dd>3.6h</dd>
        </div>
        <div>
          <dt>{t("dashboard.summary.codeChanges")}</dt>
          <dd>+512 -37</dd>
        </div>
      </dl>
      <p className="dashboard-summary__body">{t("dashboard.summary.body")}</p>
      <svg aria-hidden="true" className="dashboard-summary__chart" preserveAspectRatio="none" viewBox="0 0 128 40">
        <polyline fill="none" points={CHART_POINTS} strokeWidth="2" />
      </svg>
    </section>
  );
}
