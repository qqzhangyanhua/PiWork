import { Plus, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { ContinuousLoopLogo } from "../../components/brand/ContinuousLoopLogo";
import { useWorkStore } from "./WorkStoreProvider";

type WorkSidebarProps = {
  creating: boolean;
  onCreateRequest(): void;
  onWorkSelected(): void;
};

export function WorkSidebar({ creating, onCreateRequest, onWorkSelected }: WorkSidebarProps) {
  const { t } = useTranslation();
  const works = useWorkStore((state) => state.works);
  const selectedWorkId = useWorkStore((state) => state.selectedWorkId);
  const selectWork = useWorkStore((state) => state.selectWork);
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState("");
  const sortedWorks = useMemo(() => Object.values(works)
    .filter((work) => work.status !== "archived")
    .filter((work) => work.title.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt)), [query, works]);

  return (
    <aside className="work-sidebar" aria-label={t("sidebar.label")}>
      <div className="work-sidebar__brand">
        <ContinuousLoopLogo size={28} />
        <strong>PiWork</strong>
      </div>
      <div className="work-sidebar__primary-actions">
      <button className="work-sidebar__new" aria-current={creating ? "page" : undefined} type="button" onClick={onCreateRequest}>
        <Plus aria-hidden="true" size={16} />
        {t("work.new")}
      </button>
      <button className="work-sidebar__search-toggle" type="button" aria-label={t("sidebar.search")} onClick={() => setSearchOpen((open) => !open)}>
        <Search aria-hidden="true" size={15} />
      </button>
      </div>
      {searchOpen && <label className="work-sidebar__search"><span className="sr-only">{t("sidebar.search")}</span><input autoFocus value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t("sidebar.searchPlaceholder")} /></label>}
      <p className="work-sidebar__section-label">{t("sidebar.recent")}</p>
      <nav className="work-sidebar__nav" aria-label={t("sidebar.works")}>
        {sortedWorks.map((work) => (
          <button
            className="work-sidebar__item"
            aria-current={selectedWorkId === work.id ? "page" : undefined}
            key={work.id}
            onClick={() => {
              onWorkSelected();
              selectWork(work.id);
            }}
            type="button"
          >
            <span className={`status-dot status-dot--${work.status}`} aria-hidden="true" />
            <span className="work-sidebar__title">{work.title}</span>
          </button>
        ))}
      </nav>
      <div className="work-sidebar__footer"><span className="status-dot status-dot--completed" />{t("sidebar.localConnected")}</div>
    </aside>
  );
}
