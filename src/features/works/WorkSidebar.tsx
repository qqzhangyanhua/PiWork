import { Clock3, Plus, X } from "lucide-react";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";

import { ContinuousLoopLogo } from "../../components/brand/ContinuousLoopLogo";
import { useWorkStore } from "./WorkStoreProvider";

type WorkSidebarProps = {
  createOpen: boolean;
  onCreateOpenChange(open: boolean): void;
};

const relativeTime = (value: string, locale: string) => {
  const elapsedMinutes = Math.round((Date.now() - Date.parse(value)) / 60_000);
  if (!Number.isFinite(elapsedMinutes)) return value;
  return new Intl.RelativeTimeFormat(locale, { numeric: "auto" }).format(
    -Math.max(0, elapsedMinutes),
    "minute",
  );
};

export function WorkSidebar({ createOpen, onCreateOpenChange }: WorkSidebarProps) {
  const { i18n, t } = useTranslation();
  const works = useWorkStore((state) => state.works);
  const selectedWorkId = useWorkStore((state) => state.selectedWorkId);
  const selectWork = useWorkStore((state) => state.selectWork);
  const createWork = useWorkStore((state) => state.createWork);
  const error = useWorkStore((state) => state.error);
  const [goal, setGoal] = useState("");
  const [rootPath, setRootPath] = useState("");
  const [submitted, setSubmitted] = useState(false);

  const sortedWorks = Object.values(works).sort((left, right) =>
    right.updatedAt.localeCompare(left.updatedAt),
  );
  const close = () => {
    onCreateOpenChange(false);
    setSubmitted(false);
  };
  const submit = async (submitEvent: FormEvent) => {
    submitEvent.preventDefault();
    setSubmitted(true);
    const trimmedGoal = goal.trim();
    const trimmedPath = rootPath.trim();
    if (!trimmedGoal || !trimmedPath) return;
    const firstLine = trimmedGoal.split(/\r?\n/, 1)[0] ?? trimmedGoal;
    try {
      await createWork({
        title: firstLine.slice(0, 40),
        goal: trimmedGoal,
        rootPath: trimmedPath,
        permissionMode: "balanced",
      });
      setGoal("");
      setRootPath("");
      close();
    } catch {
      // The store exposes the normalized error next to the form.
    }
  };

  return (
    <aside className="work-sidebar" aria-label={t("sidebar.label")}>
      <div className="work-sidebar__brand">
        <ContinuousLoopLogo size={28} />
        <strong>PiWork</strong>
      </div>
      <button className="button button--primary work-sidebar__new" type="button" onClick={() => onCreateOpenChange(true)}>
        <Plus aria-hidden="true" size={16} />
        {t("work.new")}
      </button>
      <nav className="work-sidebar__nav" aria-label={t("sidebar.works")}>
        {sortedWorks.map((work) => (
          <button
            className="work-sidebar__item"
            aria-current={selectedWorkId === work.id ? "page" : undefined}
            key={work.id}
            onClick={() => selectWork(work.id)}
            type="button"
          >
            <span className={`status-dot status-dot--${work.status}`} aria-hidden="true" />
            <span className="work-sidebar__copy">
              <span className="work-sidebar__title">{work.title}</span>
              <span className="work-sidebar__time">
                <Clock3 aria-hidden="true" size={12} />
                {relativeTime(work.updatedAt, i18n.language)}
              </span>
            </span>
          </button>
        ))}
      </nav>
      {createOpen && (
        <div className="dialog-backdrop">
          <section className="create-dialog" role="dialog" aria-modal="true" aria-labelledby="create-work-title">
            <header className="create-dialog__header">
              <h2 id="create-work-title">{t("work.new")}</h2>
              <button className="icon-button" type="button" aria-label={t("common.close")} onClick={close}>
                <X aria-hidden="true" size={18} />
              </button>
            </header>
            <form onSubmit={submit} noValidate>
              <label htmlFor="work-goal">{t("create.goal")}</label>
              <textarea id="work-goal" value={goal} onChange={(event) => setGoal(event.target.value)} />
              {submitted && !goal.trim() && <p className="field-error">{t("create.goalRequired")}</p>}
              <label htmlFor="work-root">{t("create.rootPath")}</label>
              <input id="work-root" value={rootPath} onChange={(event) => setRootPath(event.target.value)} />
              {submitted && !rootPath.trim() && <p className="field-error">{t("create.rootRequired")}</p>}
              {error && <p className="field-error" role="alert">{error.message}</p>}
              <footer className="create-dialog__actions">
                <button className="button" type="button" onClick={close}>{t("common.cancel")}</button>
                <button className="button button--primary" type="submit">{t("common.create")}</button>
              </footer>
            </form>
          </section>
        </div>
      )}
    </aside>
  );
}
