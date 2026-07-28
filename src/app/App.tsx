import { useTranslation } from "react-i18next";
import { ContinuousLoopLogo } from "../components/brand/ContinuousLoopLogo";

export function App() {
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
