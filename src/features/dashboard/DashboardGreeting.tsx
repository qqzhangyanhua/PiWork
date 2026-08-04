import type { CSSProperties } from "react";
import { Trans, useTranslation } from "react-i18next";

import referenceUrl from "../../assets/piwork-home-reference.png";

export function DashboardGreeting() {
  const { t } = useTranslation();
  return (
    <section className="dashboard-greeting">
      <div className="dashboard-greeting__text">
        <p className="dashboard-greeting__hello">
          <Trans
            components={{ pi: <em className="dashboard-greeting__pi" /> }}
            i18nKey="dashboard.greeting.hello"
          />
        </p>
        <h1>
          <Trans
            components={{ pi: <em className="dashboard-greeting__pi" /> }}
            i18nKey="dashboard.greeting.title"
          />
        </h1>
        <p className="dashboard-greeting__body">{t("dashboard.greeting.body")}</p>
      </div>
      <div
        aria-hidden="true"
        className="dashboard-greeting__art"
        data-testid="dashboard-orbit-art"
        style={{ "--dashboard-reference": `url(${referenceUrl})` } as CSSProperties}
      />
    </section>
  );
}
