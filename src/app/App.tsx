import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { isTauri } from "@tauri-apps/api/core";

import { ContinuousLoopLogo } from "../components/brand/ContinuousLoopLogo";
import { ModelSetup } from "../features/model-setup/ModelSetup";
import { WorkSurface } from "../features/workspace/WorkSurface";
import {
  tauriClient,
  type ModelConfigurationStatus,
  type PiWorkClient,
} from "./tauriClient";

export type AppProps = {
  client?: PiWorkClient;
};

export function App({ client }: AppProps) {
  const { t } = useTranslation();
  const resolvedClient = client ?? tauriClient;
  const desktopRuntimeAvailable = client !== undefined || isTauri();
  const [status, setStatus] = useState<ModelConfigurationStatus | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [loadRequest, setLoadRequest] = useState(0);

  useEffect(() => {
    if (!desktopRuntimeAvailable) return;
    let active = true;
    setStatus(null);
    setLoadError(false);
    void resolvedClient.getModelConfigurationStatus()
      .then((nextStatus) => {
        if (active) setStatus(nextStatus);
      })
      .catch(() => {
        if (active) setLoadError(true);
      });
    return () => { active = false; };
  }, [desktopRuntimeAvailable, loadRequest, resolvedClient]);

  if (!desktopRuntimeAvailable) {
    return (
      <main className="model-setup model-setup--loading">
        <section className="model-setup__card" role="alert">
          <h1>{t("runtime.desktopRequiredTitle")}</h1>
          <p>{t("runtime.desktopRequiredBody")}</p>
        </section>
      </main>
    );
  }

  if (loadError) {
    return (
      <main className="model-setup model-setup--loading">
        <section className="model-setup__card" role="alert">
          <h1>{t("model.loadError")}</h1>
          <button className="button button--primary" type="button" onClick={() => setLoadRequest((request) => request + 1)}>{t("common.retry")}</button>
        </section>
      </main>
    );
  }

  if (!status) {
    return (
      <main className="model-setup model-setup--loading" role="status" aria-label={t("model.loading")}>
        <ContinuousLoopLogo showWordmark />
      </main>
    );
  }

  if (!status.configured) {
    return <ModelSetup client={resolvedClient} onConfigured={(configuration) => setStatus({ configured: true, configuration })} />;
  }

  return <WorkSurface client={resolvedClient} modelLabel={status.configuration?.modelId ?? "Pi"} />;
}
