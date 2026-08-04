import { CircleCheck, CircleDashed } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import type { PiWorkClient } from "../../app/tauriClient";
import type { RuntimeStatus } from "../../bindings";

type LoadState = "loading" | "loaded" | "failed";

const ROWS = [
  { key: "python", labelKey: "pythonLabel", showVersion: true },
  { key: "node", labelKey: "nodeLabel", showVersion: true },
  { key: "git", labelKey: "gitLabel", showVersion: false },
] as const;

export function EnvironmentStatusPanel({ client }: { client: PiWorkClient }) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<RuntimeStatus | null>(null);
  const [state, setState] = useState<LoadState>("loading");

  useEffect(() => {
    if (!client.getRuntimeStatus) {
      setState("failed");
      return;
    }
    let active = true;
    client
      .getRuntimeStatus()
      .then((result) => {
        if (active) {
          setStatus(result);
          setState("loaded");
        }
      })
      .catch(() => {
        if (active) setState("failed");
      });
    return () => {
      active = false;
    };
  }, [client]);

  return (
    <section aria-labelledby="environment-status-heading" className="dashboard-panel dashboard-panel--environment">
      <header className="dashboard-panel__header">
        <h2 id="environment-status-heading">{t("dashboard.environment.title")}</h2>
        <button
          aria-label={t("dashboard.comingSoon", { feature: t("dashboard.environment.configure") })}
          className="text-button"
          disabled
          type="button"
        >
          {t("dashboard.environment.configure")}
        </button>
      </header>
      <p className="dashboard-environment__runtime">
        <span aria-hidden="true" className="dashboard-environment__dot" />
        {t("dashboard.environment.localRuntime")}
        <span className="dashboard-environment__running">{t("dashboard.environment.running")}</span>
      </p>
      <ul className="dashboard-environment__list">
        {ROWS.map(({ key, labelKey, showVersion }) => {
          const check = status?.[key];
          const available = state === "loaded" && check?.available;
          const text =
            state === "loading"
              ? t("dashboard.environment.checking")
              : available
                ? showVersion && check?.version
                  ? check.version
                  : t("dashboard.environment.ready")
                : t("dashboard.environment.notDetected");
          return (
            <li key={key}>
              <span aria-hidden="true" className={`dashboard-environment__icon${available ? " dashboard-environment__icon--ready" : ""}`}>
                {available ? <CircleCheck size={14} /> : <CircleDashed size={14} />}
              </span>
              <span>{t(`dashboard.environment.${labelKey}`)}</span>
              <span className="dashboard-environment__status">{text}</span>
            </li>
          );
        })}
      </ul>
      <button
        aria-label={t("dashboard.comingSoon", { feature: t("dashboard.environment.viewDetails") })}
        className="text-button dashboard-environment__details"
        disabled
        type="button"
      >
        {t("dashboard.environment.viewDetails")}
      </button>
    </section>
  );
}
