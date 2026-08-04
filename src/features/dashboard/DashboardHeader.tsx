import { Bell, Bot, Search } from "lucide-react";
import { useTranslation } from "react-i18next";

import { ContinuousLoopLogo } from "../../components/brand/ContinuousLoopLogo";

export function DashboardHeader({ onImportProject }: { onImportProject(): void }) {
  const { t } = useTranslation();
  return (
    <header className="dashboard-header">
      <span className="dashboard-header__identity">
        <ContinuousLoopLogo size={13} />
        <span>PiWork</span>
      </span>
      <div className="dashboard-header__actions">
        <button className="dashboard-header__action" onClick={onImportProject} type="button">
          <Search aria-hidden="true" size={15} />
          <span>{t("dashboard.header.importProject")}</span>
        </button>
        <button
          aria-label={t("dashboard.comingSoon", { feature: t("dashboard.header.exploreAgents") })}
          className="dashboard-header__action"
          disabled
          type="button"
        >
          <Bot aria-hidden="true" size={15} />
          <span>{t("dashboard.header.exploreAgents")}</span>
        </button>
        <button
          aria-label={t("dashboard.comingSoon", { feature: t("dashboard.header.notifications") })}
          className="dashboard-header__bell"
          disabled
          type="button"
        >
          <Bell aria-hidden="true" size={16} />
        </button>
      </div>
    </header>
  );
}
