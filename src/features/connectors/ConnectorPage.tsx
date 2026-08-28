import {
  AtSign,
  Bot,
  Cable,
  Check,
  ChevronDown,
  CircleAlert,
  Inbox,
  KeyRound,
  Layers3,
  LoaderCircle,
  Mail,
  Plus,
  RefreshCw,
  Save,
  Search,
  Server,
  ShieldCheck,
  Trash2,
  Unplug,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  siAftership,
  siAirtable,
  siBigcommerce,
  siGithub,
  siGmail,
  siGoogledrive,
  siNotion,
  type SimpleIcon,
} from "simple-icons";

import type { AgentInstanceSummary } from "../../bindings";
import type {
  ConnectorPermission,
  EmailConnectorSummary,
  EmailMetadataSummary,
  PiWorkClient,
  SaveEmailConnectorInput,
} from "../../app/tauriClient";
import openConnectorCatalog from "./openConnectorCatalog.generated.json";

type ConnectorDraft = SaveEmailConnectorInput & { password: string };
type CatalogView = "all" | "connected" | "agents";
type CatalogProvider = (typeof openConnectorCatalog.providers)[number];

const NATIVE_EMAIL_SERVICE = "generic_imap";
const PAGE_SIZE = 60;
const EMAIL_PERMISSIONS: ConnectorPermission[] = ["metadata", "read_body", "send"];
const FEATURED_SERVICES = [
  "generic_imap",
  "gmail",
  "github",
  "slack",
  "notion",
  "google_drive",
  "airtable",
];
const BRAND_ICONS: Partial<Record<string, SimpleIcon>> = {
  aftership: siAftership,
  airtable: siAirtable,
  big_commerce: siBigcommerce,
  github: siGithub,
  gmail: siGmail,
  google_drive: siGoogledrive,
  notion: siNotion,
};
const ICON_LOAD_CONCURRENCY = 4;
const iconRequestCache = new WeakMap<PiWorkClient, Map<string, Promise<string | null>>>();
const iconLoadQueue: Array<() => void> = [];
let activeIconLoads = 0;

const ALIYUN_DEFAULTS = {
  imapHost: "imap.qiye.aliyun.com",
  imapPort: 993,
  smtpHost: "smtp.qiye.aliyun.com",
  smtpPort: 465,
} as const;

const TEST_ERROR_KEYS: Record<string, string> = {
  imap_authentication: "connectors.testErrors.imapAuthentication",
  imap_configuration: "connectors.testErrors.imapConfiguration",
  imap_connection_failed: "connectors.testErrors.imapConnection",
  imap_network: "connectors.testErrors.imapNetwork",
  imap_protocol: "connectors.testErrors.imapProtocol",
  imap_timeout: "connectors.testErrors.imapTimeout",
  imap_tls: "connectors.testErrors.imapTls",
  smtp_authentication: "connectors.testErrors.smtpAuthentication",
  smtp_configuration: "connectors.testErrors.smtpConfiguration",
  smtp_connection: "connectors.testErrors.smtpConnection",
  smtp_connection_failed: "connectors.testErrors.smtpConnection",
  smtp_timeout: "connectors.testErrors.smtpTimeout",
  smtp_tls: "connectors.testErrors.smtpTls",
};

const emptyDraft = (): ConnectorDraft => ({
  id: null,
  displayName: "",
  emailAddress: "",
  username: "",
  password: "",
  preset: "aliyun-enterprise",
  ...ALIYUN_DEFAULTS,
  pollIntervalMinutes: 2,
});

const toDraft = (connection: EmailConnectorSummary): ConnectorDraft => ({
  id: connection.id,
  displayName: connection.displayName,
  emailAddress: connection.emailAddress,
  username: connection.username,
  password: "",
  preset: connection.preset,
  imapHost: connection.imapHost,
  imapPort: connection.imapPort,
  smtpHost: connection.smtpHost,
  smtpPort: connection.smtpPort,
  pollIntervalMinutes: connection.pollIntervalMinutes,
});

const connectionTone = (connection: EmailConnectorSummary) => {
  if (!connection.enabled) return "disabled";
  return connection.healthStatus;
};

const monogram = (provider: CatalogProvider) => provider.displayName
  .split(/\s+/u)
  .slice(0, 2)
  .map((part) => part[0])
  .join("")
  .toUpperCase();

const drainIconLoadQueue = () => {
  while (activeIconLoads < ICON_LOAD_CONCURRENCY && iconLoadQueue.length > 0) {
    iconLoadQueue.shift()?.();
  }
};

const scheduleIconLoad = <T,>(operation: () => Promise<T>): Promise<T> => new Promise((resolve, reject) => {
  iconLoadQueue.push(() => {
    activeIconLoads += 1;
    void operation()
      .then(resolve, reject)
      .finally(() => {
        activeIconLoads -= 1;
        drainIconLoadQueue();
      });
  });
  drainIconLoadQueue();
});

const requestProviderIcon = (client: PiWorkClient, provider: CatalogProvider) => {
  if (!client.resolveConnectorIcon) return Promise.resolve(null);
  let clientCache = iconRequestCache.get(client);
  if (!clientCache) {
    clientCache = new Map();
    iconRequestCache.set(client, clientCache);
  }
  const cached = clientCache.get(provider.service);
  if (cached) return cached;
  const request = scheduleIconLoad(() => client.resolveConnectorIcon!(
    provider.service,
    provider.homepageUrl,
    provider.iconUrl,
  )).catch(() => null);
  clientCache.set(provider.service, request);
  return request;
};

function ConnectorLogo({
  client,
  large = false,
  provider,
}: {
  client: PiWorkClient;
  large?: boolean;
  provider: CatalogProvider;
}) {
  const markRef = useRef<HTMLSpanElement>(null);
  const [remoteSource, setRemoteSource] = useState<string | null>(null);
  const [remoteFailed, setRemoteFailed] = useState(false);
  const brandIcon = BRAND_ICONS[provider.service];

  useEffect(() => {
    setRemoteSource(null);
    setRemoteFailed(false);
    if (brandIcon || provider.service === NATIVE_EMAIL_SERVICE || !client.resolveConnectorIcon) return;
    let cancelled = false;
    let observer: IntersectionObserver | null = null;
    const loadIcon = () => {
      observer?.disconnect();
      void requestProviderIcon(client, provider).then((source) => {
        if (!cancelled) setRemoteSource(source);
      });
    };
    if (typeof IntersectionObserver === "undefined" || !markRef.current) {
      loadIcon();
    } else {
      observer = new IntersectionObserver((entries) => {
        if (entries.some(({ isIntersecting }) => isIntersecting)) loadIcon();
      }, { rootMargin: "160px" });
      observer.observe(markRef.current);
    }
    return () => {
      cancelled = true;
      observer?.disconnect();
    };
  }, [brandIcon, client, provider]);

  return (
    <span
      aria-hidden="true"
      className={`connector-provider-mark${large ? " connector-provider-mark--large" : ""}`}
      ref={markRef}
    >
      {brandIcon ? (
        <svg data-icon-source="brand" focusable="false" viewBox="0 0 24 24">
          <path d={brandIcon.path} fill={`#${brandIcon.hex}`} />
        </svg>
      ) : provider.service === NATIVE_EMAIL_SERVICE ? (
        <Mail data-icon-source="native" size={large ? 22 : 18} strokeWidth={1.8} />
      ) : remoteSource && !remoteFailed ? (
        <img
          alt=""
          data-icon-source="cached"
          decoding="async"
          loading="lazy"
          onError={() => setRemoteFailed(true)}
          src={remoteSource}
        />
      ) : monogram(provider)}
    </span>
  );
}

export function ConnectorPage({ client }: { client: PiWorkClient }) {
  const { t } = useTranslation();
  const [connections, setConnections] = useState<EmailConnectorSummary[]>([]);
  const [agents, setAgents] = useState<AgentInstanceSummary[]>([]);
  const [selectedService, setSelectedService] = useState(NATIVE_EMAIL_SERVICE);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<ConnectorDraft>(emptyDraft);
  const [creating, setCreating] = useState(true);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState(false);
  const [busy, setBusy] = useState<"save" | "test" | "enable" | "delete" | null>(null);
  const [bindingBusy, setBindingBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ tone: "success" | "error"; text: string } | null>(null);
  const [recentMail, setRecentMail] = useState<EmailMetadataSummary[]>([]);
  const [query, setQuery] = useState("");
  const [catalogView, setCatalogView] = useState<CatalogView>("all");
  const [scenario, setScenario] = useState("all");
  const [visibleCount, setVisibleCount] = useState(PAGE_SIZE);
  const selected = connections.find(({ id }) => id === selectedId) ?? null;
  const selectedProvider = openConnectorCatalog.providers.find(
    ({ service }) => service === selectedService,
  ) ?? openConnectorCatalog.providers[0]!;

  const load = async () => {
    setLoading(true);
    setLoadError(false);
    try {
      const [loadedConnections, loadedAgents] = await Promise.all([
        client.listEmailConnectors?.() ?? Promise.resolve([]),
        client.listAgentInstances().catch(() => []),
      ]);
      setConnections(loadedConnections);
      setAgents(loadedAgents.filter(({ status }) => status === "active"));
      setSelectedId((current) => current && loadedConnections.some(({ id }) => id === current)
        ? current
        : loadedConnections[0]?.id ?? null);
      if (loadedConnections.length === 0) {
        setCreating(true);
        setDraft(emptyDraft());
      }
    } catch {
      setLoadError(true);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { void load(); }, [client]);
  useEffect(() => {
    if (creating || !selected) return;
    setDraft(toDraft(selected));
    setMessage(null);
    if (selected.enabled && client.listEmailMetadata) {
      void client.listEmailMetadata(selected.id, "", 8).then(setRecentMail).catch(() => setRecentMail([]));
    } else {
      setRecentMail([]);
    }
  }, [client, creating, selectedId, selected?.lastPolledAt, selected?.enabled]);
  useEffect(() => { setVisibleCount(PAGE_SIZE); }, [catalogView, query, scenario]);

  const scenarioCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const provider of openConnectorCatalog.providers) {
      counts.set(provider.scenario, (counts.get(provider.scenario) ?? 0) + 1);
    }
    return [...counts.entries()].sort((left, right) => right[1] - left[1]);
  }, []);

  const connectedCount = connections.filter(({ enabled }) => enabled).length;
  const agentEnabledCount = connections.filter(({ agentGrants }) => agentGrants.length > 0).length;
  const filteredProviders = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    const providers = openConnectorCatalog.providers.filter((provider) => {
      if (scenario !== "all" && provider.scenario !== scenario) return false;
      if (catalogView === "connected" && (provider.service !== NATIVE_EMAIL_SERVICE || connectedCount === 0)) return false;
      if (catalogView === "agents" && (provider.service !== NATIVE_EMAIL_SERVICE || agentEnabledCount === 0)) return false;
      if (!normalizedQuery) return true;
      return [provider.displayName, provider.service, provider.scenario, ...provider.categories, ...provider.authTypes]
        .some((value) => value.toLocaleLowerCase().includes(normalizedQuery));
    });
    return [...providers].sort((left, right) => {
      const leftConnected = left.service === NATIVE_EMAIL_SERVICE && connections.length > 0 ? 1 : 0;
      const rightConnected = right.service === NATIVE_EMAIL_SERVICE && connections.length > 0 ? 1 : 0;
      if (leftConnected !== rightConnected) return rightConnected - leftConnected;
      const leftFeatured = FEATURED_SERVICES.indexOf(left.service);
      const rightFeatured = FEATURED_SERVICES.indexOf(right.service);
      if (leftFeatured !== -1 || rightFeatured !== -1) {
        if (leftFeatured === -1) return 1;
        if (rightFeatured === -1) return -1;
        return leftFeatured - rightFeatured;
      }
      return left.displayName.localeCompare(right.displayName);
    });
  }, [agentEnabledCount, catalogView, connectedCount, connections.length, query, scenario]);

  const replaceConnection = (next: EmailConnectorSummary) => {
    setConnections((current) => current.some(({ id }) => id === next.id)
      ? current.map((item) => item.id === next.id ? next : item)
      : [...current, next]);
    setSelectedId(next.id);
    setCreating(false);
    setDraft(toDraft(next));
  };

  const selectProvider = (provider: CatalogProvider) => {
    setSelectedService(provider.service);
    setMessage(null);
    if (provider.service === NATIVE_EMAIL_SERVICE) {
      const connection = selected ?? connections[0] ?? null;
      setCreating(!connection);
      setSelectedId(connection?.id ?? null);
      setDraft(connection ? toDraft(connection) : emptyDraft());
    }
  };

  const startNewEmailConnection = () => {
    setSelectedService(NATIVE_EMAIL_SERVICE);
    setCreating(true);
    setSelectedId(null);
    setDraft(emptyDraft());
    setMessage(null);
  };

  const save = async () => {
    if (!client.saveEmailConnector) return;
    setBusy("save");
    setMessage(null);
    try {
      const saved = await client.saveEmailConnector({ ...draft, password: draft.password || null });
      replaceConnection(saved);
      setMessage({ tone: "success", text: t("connectors.saved") });
    } catch {
      setMessage({ tone: "error", text: t("connectors.saveError") });
    } finally {
      setBusy(null);
    }
  };

  const test = async () => {
    if (!client.testEmailConnector) return;
    setBusy("test");
    setMessage(null);
    try {
      const result = await client.testEmailConnector({ ...draft, password: draft.password || null });
      setMessage(result.imapOk && result.smtpOk
        ? { tone: "success", text: t("connectors.testSuccess") }
        : { tone: "error", text: t(TEST_ERROR_KEYS[result.errorCode ?? ""] ?? "connectors.testError") });
    } catch {
      setMessage({ tone: "error", text: t("connectors.testError") });
    } finally {
      setBusy(null);
    }
  };

  const toggleEnabled = async () => {
    if (!selected || !client.setEmailConnectorEnabled) return;
    setBusy("enable");
    setMessage(null);
    try {
      const updated = await client.setEmailConnectorEnabled(selected.id, !selected.enabled);
      replaceConnection(updated);
      setMessage({ tone: "success", text: t(updated.enabled ? "connectors.enabledMessage" : "connectors.disabledMessage") });
    } catch {
      setMessage({ tone: "error", text: t("connectors.enableError") });
    } finally {
      setBusy(null);
    }
  };

  const toggleAgent = async (agentInstanceId: string) => {
    if (!selected || !client.setConnectorAgentGrant) return;
    const enabled = !selected.agentGrants.some((grant) => grant.agentInstanceId === agentInstanceId);
    setBindingBusy(agentInstanceId);
    setMessage(null);
    try {
      const updated = await client.setConnectorAgentGrant(selected.id, agentInstanceId, enabled, EMAIL_PERMISSIONS);
      replaceConnection(updated);
      setMessage({ tone: "success", text: t("connectors.agentGrantSaved") });
    } catch {
      setMessage({ tone: "error", text: t("connectors.agentGrantError") });
    } finally {
      setBindingBusy(null);
    }
  };

  const remove = async () => {
    if (!selected || !client.deleteEmailConnector) return;
    if (!window.confirm(t("connectors.deleteConfirm", { name: selected.displayName }))) return;
    setBusy("delete");
    try {
      await client.deleteEmailConnector(selected.id);
      const next = connections.filter(({ id }) => id !== selected.id);
      setConnections(next);
      setSelectedId(next[0]?.id ?? null);
      setCreating(next.length === 0);
      setDraft(next[0] ? toDraft(next[0]) : emptyDraft());
    } finally {
      setBusy(null);
    }
  };

  return (
    <section aria-labelledby="connectors-title" className="connector-page product-page">
      <header className="product-page__header">
        <div><Cable aria-hidden="true" size={17} /><h1 id="connectors-title">{t("connectors.title")}</h1></div>
        <span>{t("connectors.subtitle", { count: openConnectorCatalog.providers.length })}</span>
      </header>

      <div className="connector-toolbar">
        <div aria-label={t("connectors.viewsLabel")} className="connector-view-switcher" role="group">
          {(["all", "connected", "agents"] as const).map((view) => (
            <button aria-pressed={catalogView === view} key={view} onClick={() => setCatalogView(view)} type="button">
              {t(`connectors.views.${view}`)}
              <span>{view === "all" ? openConnectorCatalog.providers.length : view === "connected" ? connectedCount : agentEnabledCount}</span>
            </button>
          ))}
        </div>
        <label className="connector-search">
          <Search aria-hidden="true" size={15} />
          <span className="sr-only">{t("connectors.search")}</span>
          <input onChange={(event) => setQuery(event.target.value)} placeholder={t("connectors.searchPlaceholder")} type="search" value={query} />
        </label>
      </div>

      <div className="connector-page__layout">
        <aside aria-label={t("connectors.scenariosLabel")} className="connector-filters">
          <div className="connector-filters__heading"><Layers3 aria-hidden="true" size={14} /><strong>{t("connectors.scenariosLabel")}</strong></div>
          <button aria-current={scenario === "all" ? "true" : undefined} onClick={() => setScenario("all")} type="button">
            <span>{t("connectors.scenarios.all")}</span><small>{openConnectorCatalog.providers.length}</small>
          </button>
          {scenarioCounts.map(([item, count]) => (
            <button aria-current={scenario === item ? "true" : undefined} key={item} onClick={() => setScenario(item)} type="button">
              <span>{t(`connectors.scenarios.${item}`)}</span><small>{count}</small>
            </button>
          ))}
          <div className="connector-catalog-source">
            <strong>OpenConnector</strong>
            <span>{t("connectors.catalogRevision", { revision: openConnectorCatalog.revision.slice(0, 7) })}</span>
          </div>
        </aside>

        <section aria-label={t("connectors.catalog")} className="connector-catalog">
          <header className="connector-catalog__header">
            <div><strong>{t("connectors.catalog")}</strong><span>{t("connectors.results", { count: filteredProviders.length })}</span></div>
            <span>{t("connectors.metadataOnlyCatalog")}</span>
          </header>
          {loading ? (
            <div className="connector-catalog__state" role="status"><LoaderCircle aria-hidden="true" className="spin" size={17} />{t("connectors.loading")}</div>
          ) : loadError ? (
            <div className="connector-catalog__state is-error" role="alert"><CircleAlert aria-hidden="true" size={17} />{t("connectors.loadError")}</div>
          ) : filteredProviders.length === 0 ? (
            <div className="connector-catalog__state"><Search aria-hidden="true" size={17} />{t("connectors.noResults")}</div>
          ) : (
            <div className="connector-catalog__scroll">
              <div className="connector-catalog-grid">
                {filteredProviders.slice(0, visibleCount).map((provider) => {
                  const nativeConnections = provider.service === NATIVE_EMAIL_SERVICE ? connections : [];
                  const connected = nativeConnections.some(({ enabled }) => enabled);
                  const bound = nativeConnections.some(({ agentGrants }) => agentGrants.length > 0);
                  return (
                    <button aria-current={selectedService === provider.service ? "page" : undefined} className="connector-catalog-card" key={provider.service} onClick={() => selectProvider(provider)} type="button">
                      <ConnectorLogo client={client} provider={provider} />
                      <span className="connector-catalog-card__copy"><strong>{provider.displayName}</strong><small>{provider.categories.slice(0, 2).join(" · ") || provider.service}</small></span>
                      <span className="connector-catalog-card__meta">
                        {connected ? <i className="is-connected">{t("connectors.status.connected")}</i> : bound ? <i>{t("connectors.status.configured")}</i> : <i>{t("connectors.status.available")}</i>}
                        <small>{t("connectors.actionCount", { count: provider.actionCount })}</small>
                      </span>
                    </button>
                  );
                })}
              </div>
              {visibleCount < filteredProviders.length && (
                <button className="connector-load-more" onClick={() => setVisibleCount((count) => count + PAGE_SIZE)} type="button">
                  {t("connectors.loadMore", { count: Math.min(PAGE_SIZE, filteredProviders.length - visibleCount) })}<ChevronDown aria-hidden="true" size={14} />
                </button>
              )}
            </div>
          )}
        </section>

        <section aria-label={t("connectors.details")} className="connector-detail">
          <header className="connector-detail__header">
            <ConnectorLogo client={client} large provider={selectedProvider} />
            <div><span>{selectedProvider.service}</span><h2>{selectedProvider.displayName}</h2><p>{selectedProvider.categories.join(" · ")}</p></div>
            <span className={`connector-status ${selectedService === NATIVE_EMAIL_SERVICE && selected?.enabled ? "connector-status--healthy" : ""}`}>
              {selectedService === NATIVE_EMAIL_SERVICE && selected?.enabled ? <Check aria-hidden="true" size={13} /> : <Unplug aria-hidden="true" size={13} />}
              {selectedService === NATIVE_EMAIL_SERVICE && selected?.enabled ? t("connectors.status.connected") : t("connectors.status.notConnected")}
            </span>
          </header>

          {selectedService !== NATIVE_EMAIL_SERVICE ? (
            <div className="connector-detail__scroll connector-provider-overview">
              <section className="connector-overview-card">
                <div><strong>{t("connectors.actions")}</strong><span>{selectedProvider.actionCount}</span></div>
                <div><strong>{t("connectors.authentication")}</strong><span>{selectedProvider.authTypes.map((auth) => t(`connectors.auth.${auth}`)).join(" / ")}</span></div>
                <div><strong>{t("connectors.connectionInstances")}</strong><span>0</span></div>
              </section>
              <section className="connector-runtime-notice">
                <ShieldCheck aria-hidden="true" size={18} />
                <div><h3>{t("connectors.catalogOnlyTitle")}</h3><p>{t("connectors.catalogOnlyDescription")}</p></div>
              </section>
              <section className="connector-lifecycle">
                <h3>{t("connectors.lifecycleTitle")}</h3>
                <ol>
                  <li className="is-active"><span>1</span><div><strong>{t("connectors.lifecycle.discovered")}</strong><small>{t("connectors.lifecycle.discoveredDescription")}</small></div></li>
                  <li><span>2</span><div><strong>{t("connectors.lifecycle.configure")}</strong><small>{t("connectors.lifecycle.configureDescription")}</small></div></li>
                  <li><span>3</span><div><strong>{t("connectors.lifecycle.bind")}</strong><small>{t("connectors.lifecycle.bindDescription")}</small></div></li>
                </ol>
              </section>
            </div>
          ) : (
            <>
              <div className="connector-email-tabs">
                {connections.map((connection) => (
                  <button aria-current={!creating && connection.id === selectedId ? "true" : undefined} key={connection.id} onClick={() => { setCreating(false); setSelectedId(connection.id); }} type="button">
                    <Mail aria-hidden="true" size={13} /><span>{connection.displayName}</span><i className={`connector-health connector-health--${connectionTone(connection)}`} />
                  </button>
                ))}
                <button aria-current={creating ? "true" : undefined} onClick={startNewEmailConnection} type="button"><Plus aria-hidden="true" size={13} />{t("connectors.newConnection")}</button>
              </div>

              <div className="connector-detail__scroll">
                <section aria-labelledby="connector-identity-heading" className="connector-section">
                  <div className="connector-section__heading">
                    <AtSign aria-hidden="true" size={15} />
                    <div><h3 id="connector-identity-heading">{creating ? t("connectors.addTitle") : t("connectors.identity")}</h3><p>{t("connectors.identityDescription")}</p></div>
                  </div>
                  <div className="connector-field-grid">
                    <label><span>{t("connectors.displayName")}</span><input onChange={(event) => setDraft({ ...draft, displayName: event.target.value })} placeholder={t("connectors.displayNamePlaceholder")} value={draft.displayName} /></label>
                    <label><span>{t("connectors.emailAddress")}</span><input onChange={(event) => {
                      const emailAddress = event.target.value;
                      setDraft({ ...draft, emailAddress, username: !draft.username || draft.username === draft.emailAddress ? emailAddress : draft.username });
                    }} placeholder="name@company.com" type="email" value={draft.emailAddress} /></label>
                    <label><span>{t("connectors.username")}</span><input onChange={(event) => setDraft({ ...draft, username: event.target.value })} value={draft.username} /></label>
                    <label><span>{t("connectors.password")}</span><div className="connector-secret-field"><KeyRound aria-hidden="true" size={14} /><input autoComplete="new-password" onChange={(event) => setDraft({ ...draft, password: event.target.value })} placeholder={!creating && selected?.credentialConfigured ? t("connectors.passwordStored") : t("connectors.passwordPlaceholder")} type="password" value={draft.password} /></div></label>
                  </div>
                </section>

                <section aria-labelledby="connector-server-heading" className="connector-section">
                  <div className="connector-section__heading">
                    <Server aria-hidden="true" size={15} />
                    <div><h3 id="connector-server-heading">{t("connectors.servers")}</h3><p>{t("connectors.serversDescription")}</p></div>
                  </div>
                  <div className="connector-preset-row">
                    <label><span>{t("connectors.preset")}</span><select onChange={(event) => {
                      const preset = event.target.value;
                      setDraft(preset === "aliyun-enterprise" ? { ...draft, preset, ...ALIYUN_DEFAULTS } : { ...draft, preset });
                    }} value={draft.preset}><option value="aliyun-enterprise">{t("connectors.presets.aliyun")}</option><option value="custom">{t("connectors.presets.custom")}</option></select></label>
                    <label><span>{t("connectors.pollInterval")}</span><select onChange={(event) => setDraft({ ...draft, pollIntervalMinutes: Number(event.target.value) as 1 | 2 | 5 | 15 })} value={draft.pollIntervalMinutes}>{[1, 2, 5, 15].map((value) => <option key={value} value={value}>{t("connectors.minutes", { count: value })}</option>)}</select></label>
                  </div>
                  <div className="connector-server-grid">
                    <label><span>IMAP</span><input onChange={(event) => setDraft({ ...draft, imapHost: event.target.value })} value={draft.imapHost} /></label>
                    <label><span>{t("connectors.port")}</span><input max={65535} min={1} onChange={(event) => setDraft({ ...draft, imapPort: Number(event.target.value) })} type="number" value={draft.imapPort} /></label>
                    <label><span>SMTP</span><input onChange={(event) => setDraft({ ...draft, smtpHost: event.target.value })} value={draft.smtpHost} /></label>
                    <label><span>{t("connectors.port")}</span><input max={65535} min={1} onChange={(event) => setDraft({ ...draft, smtpPort: Number(event.target.value) })} type="number" value={draft.smtpPort} /></label>
                  </div>
                  <div className="connector-tls-note"><ShieldCheck aria-hidden="true" size={14} /><span>{t("connectors.tlsOnly")}</span></div>
                </section>

                {!creating && selected?.enabled && (
                  <section aria-labelledby="connector-agent-heading" className="connector-section connector-agent-access">
                    <div className="connector-section__heading">
                      <Bot aria-hidden="true" size={15} />
                      <div><h3 id="connector-agent-heading">{t("connectors.agentAccess")}</h3><p>{t("connectors.agentAccessDescription")}</p></div>
                    </div>
                    {agents.length === 0 ? <p className="connector-section__empty">{t("connectors.noAgents")}</p> : (
                      <div className="connector-agent-list">
                        {agents.map((agent) => {
                          const granted = selected.agentGrants.some(({ agentInstanceId }) => agentInstanceId === agent.id);
                          return (
                            <div className="connector-agent-row" key={agent.id}>
                              <span className="connector-agent-row__identity"><Bot aria-hidden="true" size={14} /><span><strong>{agent.displayName}</strong><small>{agent.definition.roleKind}</small></span></span>
                              <span className="connector-agent-row__permissions">{granted ? EMAIL_PERMISSIONS.map((permission) => <i key={permission}>{t(`connectors.permission.${permission}`)}</i>) : t("connectors.notAvailableToAgent")}</span>
                              <button aria-checked={granted} aria-label={t(granted ? "connectors.removeFromAgent" : "connectors.addToAgent", { connector: selected.displayName, agent: agent.displayName })} className="connector-agent-toggle" disabled={bindingBusy !== null} onClick={() => void toggleAgent(agent.id)} role="switch" type="button">
                                {bindingBusy === agent.id ? <LoaderCircle aria-hidden="true" className="spin" size={12} /> : <span />}
                              </button>
                            </div>
                          );
                        })}
                      </div>
                    )}
                  </section>
                )}

                {!creating && selected?.enabled && recentMail.length > 0 && (
                  <section aria-labelledby="connector-recent-mail" className="connector-section connector-recent-mail">
                    <div className="connector-section__heading"><Inbox aria-hidden="true" size={15} /><div><h3 id="connector-recent-mail">{t("connectors.recentMail")}</h3><p>{t("connectors.metadataOnly")}</p></div></div>
                    <div>{recentMail.map((mail) => <div className="connector-mail-row" key={mail.uid}><span><strong>{mail.subject}</strong><small>{mail.senderName || mail.senderAddress}</small></span><time>{mail.receivedAt ? new Date(mail.receivedAt).toLocaleString() : ""}</time></div>)}</div>
                  </section>
                )}
              </div>

              <footer className="connector-detail__footer">
                <div aria-live="polite">{message && <span className={`connector-message is-${message.tone}`} role={message.tone === "error" ? "alert" : "status"}>{message.tone === "success" ? <Check aria-hidden="true" size={14} /> : <CircleAlert aria-hidden="true" size={14} />}{message.text}</span>}</div>
                {!creating && selected && <button aria-label={t("connectors.delete")} className="icon-button connector-delete" disabled={busy !== null} onClick={() => void remove()} title={t("connectors.delete")} type="button"><Trash2 aria-hidden="true" size={15} /></button>}
                {!creating && selected && <button className="button" disabled={busy !== null} onClick={() => void toggleEnabled()} type="button">{busy === "enable" && <LoaderCircle aria-hidden="true" className="spin" size={14} />}{t(selected.enabled ? "common.disable" : "common.enable")}</button>}
                <button className="button" disabled={busy !== null} onClick={() => void test()} type="button">{busy === "test" ? <LoaderCircle aria-hidden="true" className="spin" size={14} /> : <RefreshCw aria-hidden="true" size={14} />}{t("connectors.test")}</button>
                <button className="button button--primary" disabled={busy !== null} onClick={() => void save()} type="button">{busy === "save" ? <LoaderCircle aria-hidden="true" className="spin" size={14} /> : <Save aria-hidden="true" size={14} />}{t("common.save")}</button>
              </footer>
            </>
          )}
        </section>
      </div>
    </section>
  );
}
