import { Boxes, Search, SlidersHorizontal, Sparkles, UsersRound } from "lucide-react";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";

import type { PiWorkClient } from "../../app/tauriClient";
import type {
  AgentInstanceSummary,
  CapabilityPackSummary,
  WorkTeamSummary,
} from "../../bindings";
import { appErrorMessageKey, appErrorMessageValues } from "../../domain/appError";
import { normalizeAppError, type AppError } from "../../domain/work";
import { CapabilityCard } from "./CapabilityCard";
import { CapabilityDetailDrawer } from "./CapabilityDetailDrawer";
import { MemberDetailDrawer } from "./MemberDetailDrawer";
import { TeamMemberCard } from "./TeamMemberCard";
import {
  AGENT_CAPABILITIES,
  AGENT_CAPABILITY_DOMAINS,
  AGENT_CAPABILITY_PATHS,
  filterCapabilities,
  type AgentCapability,
  type AgentCapabilityDomainId,
  type CapabilityFilters,
  type CapabilityLibraryItem,
  type CapabilityPriority,
} from "./agentCapabilities";
import {
  buildCapabilityLibrary,
  validateCapabilityPackInventory,
} from "./agentCenterModel";

type PrimaryView = "team" | "library";
type LibraryView = "recommended" | "all";

const domainName = (id: AgentCapabilityDomainId) =>
  AGENT_CAPABILITY_DOMAINS.find((domain) => domain.id === id)?.name ?? id;

function CapabilityLibrary({
  capabilities,
  onOpen,
}: {
  capabilities: readonly CapabilityLibraryItem[];
  onOpen(capability: CapabilityLibraryItem, trigger: HTMLButtonElement): void;
}) {
  const { t } = useTranslation();
  const [activeView, setActiveView] = useState<LibraryView>("recommended");
  const [selectedPath, setSelectedPath] = useState<(typeof AGENT_CAPABILITY_PATHS)[number]["id"]>(
    AGENT_CAPABILITY_PATHS[0].id,
  );
  const [filters, setFilters] = useState<CapabilityFilters>({
    query: "",
    domainId: "all",
    priority: "all",
  });
  const recommended = useMemo(() => {
    const path = AGENT_CAPABILITY_PATHS.find(({ id }) => id === selectedPath)
      ?? AGENT_CAPABILITY_PATHS[0];
    return capabilities.filter((capability) => path.capabilityIds.includes(capability.id as never));
  }, [capabilities, selectedPath]);
  const filtered = useMemo(
    () => filterCapabilities(capabilities, filters),
    [capabilities, filters],
  );
  const visibleCapabilities = activeView === "recommended" ? recommended : filtered;
  const clearFilters = () => setFilters({ query: "", domainId: "all", priority: "all" });

  return (
    <div className="agent-center__catalog">
      <div className="agent-center-library-switch" aria-label={t("agentCenter.libraryViews.label")}>
        {(["recommended", "all"] as const).map((view) => (
          <button
            aria-pressed={activeView === view}
            key={view}
            onClick={() => setActiveView(view)}
            type="button"
          >
            {t(`agentCenter.libraryViews.${view}`)}
          </button>
        ))}
      </div>

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
              <div><span>{t("agentCenter.recommended.kicker")}</span><h2>{AGENT_CAPABILITY_PATHS.find(({ id }) => id === selectedPath)?.title}</h2></div>
              <p>{t("agentCenter.recommended.count", { count: visibleCapabilities.length })}</p>
            </div>
            <div className="capability-grid">
              {visibleCapabilities.map((capability) => (
                <CapabilityCard
                  capability={capability}
                  domainName={domainName(capability.domainId)}
                  key={capability.id}
                  onOpen={(trigger) => onOpen(capability, trigger)}
                />
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
          {filtered.length > 0 ? (
            <div className="capability-grid">
              {filtered.map((capability) => (
                <CapabilityCard capability={capability} domainName={domainName(capability.domainId)} key={capability.id} onOpen={(trigger) => onOpen(capability, trigger)} />
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
  );
}

export function AgentCenterPage({
  client,
  currentWorkId,
  onStartCatalogCapability,
}: {
  client: PiWorkClient;
  currentWorkId?: string;
  onStartCatalogCapability(capability: AgentCapability): void;
}) {
  const { t } = useTranslation();
  const [activeView, setActiveView] = useState<PrimaryView>("team");
  const [members, setMembers] = useState<AgentInstanceSummary[]>([]);
  const [packs, setPacks] = useState<CapabilityPackSummary[]>([]);
  const [library, setLibrary] = useState<ReadonlyArray<CapabilityLibraryItem>>([]);
  const [workTeam, setWorkTeam] = useState<WorkTeamSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<AppError | null>(null);
  const [selectedMemberId, setSelectedMemberId] = useState<string | null>(null);
  const [selectedCapability, setSelectedCapability] = useState<CapabilityLibraryItem | null>(null);
  const [requestedCapabilityPackId, setRequestedCapabilityPackId] = useState<string>();
  const [returnFocusTo, setReturnFocusTo] = useState<HTMLButtonElement | null>(null);
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const currentWorkIdRef = useRef(currentWorkId);
  currentWorkIdRef.current = currentWorkId;

  useEffect(() => {
    let current = true;
    setLoading(true);
    setLoadError(null);
    void Promise.all([
      client.listAgentInstances(),
      client.listCapabilityPacks(),
      currentWorkId ? client.getWorkTeam(currentWorkId) : Promise.resolve(null),
    ])
      .then(([nextMembers, nextPacks, nextTeam]) => {
        if (!current) return;
        const { catalog } = validateCapabilityPackInventory(nextPacks);
        setMembers(nextMembers);
        setPacks(nextPacks);
        setLibrary(buildCapabilityLibrary(AGENT_CAPABILITIES, catalog));
        setWorkTeam(nextTeam);
      })
      .catch((error: unknown) => {
        if (!current) return;
        setLoadError(normalizeAppError(error));
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [client, currentWorkId]);

  const selectedMember = members.find(({ id }) => id === selectedMemberId) ?? null;
  const currentWorkMemberIds = useMemo(() => new Set(
    workTeam?.members.map(({ instance }) => instance.id) ?? [],
  ), [workTeam]);
  const leadId = workTeam?.lead.instance.id
    ?? members.find((member) => member.builtin && member.definition.roleKind === "lead")?.id
    ?? members.find((member) => member.definition.roleKind === "lead")?.id;
  const systemPackCount = packs.filter(({ catalogCapabilityId }) => catalogCapabilityId === null).length;
  const heroStats = activeView === "team"
    ? [
      [members.length, t("agentCenter.stats.members")],
      [leadId ? 1 : 0, t("agentCenter.stats.leads")],
      [systemPackCount, t("agentCenter.stats.systemPacks")],
    ]
    : [
      [9, t("agentCenter.stats.domains")],
      [library.length, t("agentCenter.stats.capabilities")],
      [23, t("agentCenter.stats.p0")],
    ];

  const selectPrimaryView = (view: PrimaryView, focus = false) => {
    const index = view === "team" ? 0 : 1;
    setActiveView(view);
    if (focus) tabRefs.current[index]?.focus();
  };
  const handleTabKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>, index: number) => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const nextIndex = event.key === "Home"
      ? 0
      : event.key === "End"
        ? 1
        : (index + (event.key === "ArrowRight" ? 1 : -1) + 2) % 2;
    selectPrimaryView(nextIndex === 0 ? "team" : "library", true);
  };
  const openMember = (member: AgentInstanceSummary, trigger: HTMLButtonElement) => {
    setReturnFocusTo(trigger);
    setSelectedMemberId(member.id);
  };
  const openCapability = (capability: CapabilityLibraryItem, trigger: HTMLButtonElement) => {
    setReturnFocusTo(trigger);
    setSelectedCapability(capability);
  };
  const addMemberToWork = async (member: AgentInstanceSummary) => {
    if (!currentWorkId) return;
    const workId = currentWorkId;
    try {
      await client.addWorkMember(workId, member.id);
      if (currentWorkIdRef.current !== workId) return;
      const nextTeam = await client.getWorkTeam(workId);
      if (currentWorkIdRef.current === workId) setWorkTeam(nextTeam);
    } catch (error) {
      if (currentWorkIdRef.current === workId) throw error;
    }
  };
  const savedMember = (member: AgentInstanceSummary) => {
    setMembers((current) => {
      const withoutSaved = current.filter(({ id }) => id !== member.id);
      return [...withoutSaved, member];
    });
    setSelectedMemberId(member.id);
    setRequestedCapabilityPackId(undefined);
  };
  const assembleExecutable = () => {
    if (selectedCapability?.status !== "executable") return;
    setRequestedCapabilityPackId(selectedCapability.capabilityPackId);
    setSelectedCapability(null);
    setReturnFocusTo(null);
    selectPrimaryView("team", true);
  };
  const renderPanelState = (content: ReactNode) => loading ? (
    <div className="agent-center-state" role="status">{t("agentCenter.loading")}</div>
  ) : loadError ? (
    <div className="agent-center-state is-error" role="alert">
      <strong>{t("agentCenter.loadError")}</strong>
      <span>{t(appErrorMessageKey(loadError), appErrorMessageValues(loadError))}</span>
    </div>
  ) : content;

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
              <p>{t(`agentCenter.descriptions.${activeView}`)}</p>
            </div>
            <div className="agent-center-hero__stats" aria-label={t(`agentCenter.stats.${activeView}Label`)}>
              {heroStats.map(([value, label]) => (
                <span className="agent-center-stat" key={String(label)}><strong className="agent-center-stat__value">{value}</strong><small>{label}</small></span>
              ))}
            </div>
          </section>

          <div aria-label={t("agentCenter.views.label")} className="agent-center-tabs" role="tablist">
            {(["team", "library"] as const).map((view, index) => (
              <button
                aria-controls={`agent-center-panel-${view}`}
                aria-selected={activeView === view}
                className="agent-center-tabs__button"
                id={`agent-center-tab-${view}`}
                key={view}
                onClick={() => selectPrimaryView(view)}
                onKeyDown={(event) => handleTabKeyDown(event, index)}
                ref={(element) => { tabRefs.current[index] = element; }}
                role="tab"
                tabIndex={activeView === view ? 0 : -1}
                type="button"
              >
                {t(`agentCenter.views.${view}`)}
              </button>
            ))}
          </div>

          <div
            aria-labelledby="agent-center-tab-team"
            hidden={activeView !== "team"}
            id="agent-center-panel-team"
            role="tabpanel"
          >
            {renderPanelState(
              <section className="agent-center-section agent-team">
                <div className="agent-center-section__heading">
                  <div><span>{t("agentCenter.team.kicker")}</span><h2>{t("agentCenter.team.title")}</h2></div>
                  <p>{currentWorkId ? t("agentCenter.team.currentWorkHint") : t("agentCenter.team.description")}</p>
                </div>
                <div className="team-member-grid">
                  {members.map((member) => (
                    <TeamMemberCard
                      isInCurrentWork={currentWorkMemberIds.has(member.id)}
                      isLead={member.id === leadId}
                      key={member.id}
                      member={member}
                      onOpen={(trigger) => openMember(member, trigger)}
                    />
                  ))}
                </div>
                <div className="agent-team__runtime-note"><UsersRound aria-hidden="true" size={14} />{t("agentCenter.team.runtimeNote")}</div>
              </section>,
            )}
          </div>
          <div
            aria-labelledby="agent-center-tab-library"
            hidden={activeView !== "library"}
            id="agent-center-panel-library"
            role="tabpanel"
          >
            {renderPanelState(<CapabilityLibrary capabilities={library} onOpen={openCapability} />)}
          </div>
        </div>
      </div>

      {selectedMember && (
        <MemberDetailDrawer
          capabilityPacks={packs}
          client={client}
          currentWorkId={currentWorkId}
          isInCurrentWork={currentWorkMemberIds.has(selectedMember.id)}
          member={selectedMember}
          onAddToWork={addMemberToWork}
          onClose={() => setSelectedMemberId(null)}
          onSaved={savedMember}
          requestedCapabilityPackId={requestedCapabilityPackId}
          returnFocusTo={returnFocusTo}
        />
      )}
      {selectedCapability && (
        <CapabilityDetailDrawer
          capability={selectedCapability}
          domainName={domainName(selectedCapability.domainId)}
          onAssemble={assembleExecutable}
          onClose={() => setSelectedCapability(null)}
          onStartCatalog={() => onStartCatalogCapability(selectedCapability)}
          returnFocusTo={returnFocusTo}
        />
      )}
    </section>
  );
}
