import { Boxes, Search, SlidersHorizontal, Sparkles } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { CapabilityCard } from "./CapabilityCard";
import { CapabilityDetailDrawer } from "./CapabilityDetailDrawer";
import {
  AGENT_CAPABILITIES,
  AGENT_CAPABILITY_DOMAINS,
  AGENT_CAPABILITY_PATHS,
  filterCapabilities,
  type AgentCapability,
  type AgentCapabilityDomainId,
  type CapabilityFilters,
  type CapabilityPriority,
} from "./agentCapabilities";

type AgentCenterView = "recommended" | "all";

const domainName = (id: AgentCapabilityDomainId) =>
  AGENT_CAPABILITY_DOMAINS.find((domain) => domain.id === id)?.name ?? id;

export function AgentCenterPage({
  onStartCapability,
}: {
  onStartCapability(capability: AgentCapability): void;
}) {
  const { t } = useTranslation();
  const [activeView, setActiveView] = useState<AgentCenterView>("recommended");
  const [selectedPath, setSelectedPath] = useState<(typeof AGENT_CAPABILITY_PATHS)[number]["id"]>(
    AGENT_CAPABILITY_PATHS[0].id,
  );
  const [filters, setFilters] = useState<CapabilityFilters>({
    query: "",
    domainId: "all",
    priority: "all",
  });
  const [selectedCapability, setSelectedCapability] = useState<AgentCapability | null>(null);
  const [returnFocusTo, setReturnFocusTo] = useState<HTMLButtonElement | null>(null);

  const recommended = useMemo(() => {
    const path = AGENT_CAPABILITY_PATHS.find((item) => item.id === selectedPath) ?? AGENT_CAPABILITY_PATHS[0];
    return AGENT_CAPABILITIES.filter((capability) => path.capabilityIds.includes(capability.id as never));
  }, [selectedPath]);
  const filtered = useMemo(() => filterCapabilities(AGENT_CAPABILITIES, filters), [filters]);
  const capabilities = activeView === "recommended" ? recommended : filtered;

  const openCapability = (capability: AgentCapability, trigger: HTMLButtonElement) => {
    setReturnFocusTo(trigger);
    setSelectedCapability(capability);
  };
  const clearFilters = () => setFilters({ query: "", domainId: "all", priority: "all" });

  return (
    <section aria-label={t("agentCenter.title")} className="agent-center">
      <div className="agent-center__scroll">
        <div className="agent-center__content">
          <header className="agent-center__header">
            <span className="agent-center__identity"><Boxes aria-hidden="true" size={16} />Magic Factory</span>
            <span>{t("agentCenter.headerKicker")}</span>
          </header>

          <section className="agent-center-hero">
            <div className="agent-center-hero__copy">
              <span className="agent-center-hero__eyebrow"><Sparkles aria-hidden="true" size={14} />{t("agentCenter.eyebrow")}</span>
              <h1>{t("agentCenter.title")}</h1>
              <p>{t("agentCenter.description")}</p>
            </div>
            <div className="agent-center-hero__stats" aria-label={t("agentCenter.stats.label")}>
              <span className="agent-center-stat"><strong className="agent-center-stat__value">9</strong><small>{t("agentCenter.stats.domains")}</small></span>
              <span className="agent-center-stat"><strong className="agent-center-stat__value">96</strong><small>{t("agentCenter.stats.capabilities")}</small></span>
              <span className="agent-center-stat"><strong className="agent-center-stat__value">23</strong><small>{t("agentCenter.stats.p0")}</small></span>
            </div>
          </section>

          <div aria-label={t("agentCenter.views.label")} className="agent-center-tabs" role="tablist">
            {(["recommended", "all"] as const).map((view) => (
              <button
                aria-selected={activeView === view}
                className="agent-center-tabs__button"
                key={view}
                onClick={() => setActiveView(view)}
                role="tab"
                type="button"
              >
                {t(`agentCenter.views.${view}`)}
              </button>
            ))}
          </div>

          <div className="agent-center__catalog" role="tabpanel">
            {activeView === "recommended" ? (
              <>
                <section className="agent-center-section">
                  <div className="agent-center-section__heading">
                    <div><span>{t("agentCenter.paths.kicker")}</span><h2>{t("agentCenter.paths.title")}</h2></div>
                    <p>{t("agentCenter.paths.description")}</p>
                  </div>
                  <div className="capability-paths">
                    {AGENT_CAPABILITY_PATHS.map((path, index) => (
                      <button
                        aria-pressed={selectedPath === path.id}
                        className="capability-path"
                        key={path.id}
                        onClick={() => setSelectedPath(path.id)}
                        type="button"
                      >
                        <span>{String(index + 1).padStart(2, "0")}</span>
                        <strong>{path.title}</strong>
                        <small>{path.description}</small>
                      </button>
                    ))}
                  </div>
                </section>
                <section className="agent-center-section">
                  <div className="agent-center-section__heading agent-center-section__heading--compact">
                    <div><span>{t("agentCenter.recommended.kicker")}</span><h2>{AGENT_CAPABILITY_PATHS.find((path) => path.id === selectedPath)?.title}</h2></div>
                    <p>{t("agentCenter.recommended.count", { count: capabilities.length })}</p>
                  </div>
                  <div className="capability-grid">
                    {capabilities.map((capability) => (
                      <CapabilityCard capability={capability} domainName={domainName(capability.domainId)} key={capability.id} onOpen={(trigger) => openCapability(capability, trigger)} />
                    ))}
                  </div>
                </section>
              </>
            ) : (
              <section className="agent-center-section agent-center-section--all">
                <div className="agent-center-section__heading">
                  <div><span>{t("agentCenter.catalog.kicker")}</span><h2>{t("agentCenter.catalog.title")}</h2></div>
                  <p>{t("agentCenter.catalog.description")}</p>
                </div>
                <div className="capability-filters">
                  <label className="capability-search">
                    <Search aria-hidden="true" size={16} />
                    <span className="sr-only">{t("agentCenter.filters.search")}</span>
                    <input aria-label={t("agentCenter.filters.search")} onChange={(event) => setFilters((current) => ({ ...current, query: event.target.value }))} placeholder={t("agentCenter.filters.searchPlaceholder")} type="search" value={filters.query} />
                  </label>
                  <label><span>{t("agentCenter.filters.domain")}</span><select onChange={(event) => setFilters((current) => ({ ...current, domainId: event.target.value as CapabilityFilters["domainId"] }))} value={filters.domainId}><option value="all">{t("agentCenter.filters.allDomains")}</option>{AGENT_CAPABILITY_DOMAINS.map((domain) => <option key={domain.id} value={domain.id}>{domain.name} · {domain.count}</option>)}</select></label>
                  <label><span>{t("agentCenter.filters.priority")}</span><select onChange={(event) => setFilters((current) => ({ ...current, priority: event.target.value as CapabilityPriority | "all" }))} value={filters.priority}><option value="all">{t("agentCenter.filters.allPriorities")}</option>{(["P0", "P1", "P2"] as const).map((priority) => <option key={priority} value={priority}>{priority}</option>)}</select></label>
                </div>
                <div className="capability-results__header"><span><SlidersHorizontal aria-hidden="true" size={14} />{t("agentCenter.results", { count: filtered.length })}</span></div>
                {filtered.length ? (
                  <div className="capability-grid">
                    {filtered.map((capability) => (
                      <CapabilityCard capability={capability} domainName={domainName(capability.domainId)} key={capability.id} onOpen={(trigger) => openCapability(capability, trigger)} />
                    ))}
                  </div>
                ) : (
                  <div className="capability-empty">
                    <Search aria-hidden="true" size={22} />
                    <h2>{t("agentCenter.empty.title")}</h2>
                    <p>{t("agentCenter.empty.body")}</p>
                    <button onClick={clearFilters} type="button">{t("agentCenter.empty.clear")}</button>
                  </div>
                )}
              </section>
            )}
          </div>
        </div>
      </div>

      {selectedCapability && (
        <CapabilityDetailDrawer
          capability={selectedCapability}
          domainName={domainName(selectedCapability.domainId)}
          onClose={() => setSelectedCapability(null)}
          onStart={() => onStartCapability(selectedCapability)}
          returnFocusTo={returnFocusTo}
        />
      )}
    </section>
  );
}
