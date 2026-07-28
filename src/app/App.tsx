import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { ContinuousLoopLogo } from "../components/brand/ContinuousLoopLogo";
import {
  WorkStoreProvider,
  useWorkStore,
} from "../features/works/WorkStoreProvider";
import { useWorkEvents } from "../features/works/useWorkEvents";
import type { PiWorkClient } from "./tauriClient";

function WorkBootstrap() {
  const hydrate = useWorkStore((state) => state.hydrate);

  useWorkEvents();
  useEffect(() => {
    void hydrate();
  }, [hydrate]);

  return <ProductShell />;
}

function ProductShell() {
  const { t } = useTranslation();

  return (
    <main className="app-shell">
      <header className="app-shell__header">
        <div className="app-shell__brand">
          <ContinuousLoopLogo size={34} />
          <h1 className="app-shell__title">{t("app.name")}</h1>
        </div>
        <button className="app-shell__action" type="button">
          {t("work.new")}
        </button>
      </header>
      <div aria-hidden="true" className="app-shell__workspace" />
    </main>
  );
}

export type AppProps = {
  client?: PiWorkClient;
};

export function App({ client }: AppProps) {
  return (
    <WorkStoreProvider client={client}>
      <WorkBootstrap />
    </WorkStoreProvider>
  );
}
