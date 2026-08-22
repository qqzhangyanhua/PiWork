import {
  Blocks,
  Box,
  Check,
  CircleAlert,
  Download,
  Globe2,
  HardDrive,
  LockKeyhole,
  Network,
  PackageCheck,
  Search,
  ShieldCheck,
  TerminalSquare,
  UsersRound,
} from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";

import type {
  CommunityExtensionSummary,
  ExtensionSummary,
  PiWorkClient,
} from "../../app/tauriClient";

type MarketplaceView = "catalog" | "installed" | "community";

const trustIcon = (tier: ExtensionSummary["trustTier"]) => {
  if (tier === "builtin") return <LockKeyhole aria-hidden="true" size={14} />;
  if (tier === "verified") return <ShieldCheck aria-hidden="true" size={14} />;
  return <Box aria-hidden="true" size={14} />;
};

export function ExtensionMarketplacePage({ client }: { client: PiWorkClient }) {
  const { t } = useTranslation();
  const [view, setView] = useState<MarketplaceView>("catalog");
  const [extensions, setExtensions] = useState<ExtensionSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState(false);
  const [expandedPackage, setExpandedPackage] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [community, setCommunity] = useState<CommunityExtensionSummary[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState(false);
  const load = async () => {
    setLoading(true);
    setLoadError(false);
    try {
      const nextExtensions = await (client.listExtensions?.() ?? Promise.resolve([]));
      setExtensions(nextExtensions);
      setExpandedPackage((current) => current ?? nextExtensions[0]?.packageId ?? null);
    } catch {
      setLoadError(true);
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => { void load(); }, [client]);

  const visibleExtensions = useMemo(() => view === "installed"
    ? extensions.filter(({ installedVersion }) => installedVersion !== null)
    : extensions,
  [extensions, view]);

  const searchCommunity = async (event: FormEvent) => {
    event.preventDefault();
    setSearching(true);
    setSearchError(false);
    try {
      setCommunity(await (client.searchCommunityExtensions?.(query) ?? Promise.resolve([])));
    } catch {
      setSearchError(true);
    } finally {
      setSearching(false);
    }
  };
  return (
    <section aria-labelledby="extension-marketplace-title" className="extension-marketplace product-page">
      <header className="product-page__header">
        <div>
          <Blocks aria-hidden="true" size={17} />
          <h1 id="extension-marketplace-title">{t("extensions.title")}</h1>
        </div>
        <span>{t("extensions.subtitle")}</span>
      </header>

      <div className="product-page__toolbar">
        <div aria-label={t("extensions.viewsLabel")} className="product-segmented" role="tablist">
          {(["catalog", "installed", "community"] as const).map((item) => (
            <button
              aria-selected={view === item}
              key={item}
              onClick={() => setView(item)}
              role="tab"
              type="button"
            >
              {t(`extensions.views.${item}`)}
            </button>
          ))}
        </div>
        <div className="extension-marketplace__legend">
          <span><LockKeyhole aria-hidden="true" size={13} />{t("extensions.tiers.builtin")}</span>
          <span><ShieldCheck aria-hidden="true" size={13} />{t("extensions.tiers.verified")}</span>
          <span><Box aria-hidden="true" size={13} />{t("extensions.tiers.community")}</span>
        </div>
      </div>

      <div className="product-page__body">
        {view === "community" ? (
          <section className="community-search" aria-labelledby="community-search-title">
            <div className="community-search__intro">
              <div>
                <h2 id="community-search-title">{t("extensions.community.title")}</h2>
                <p>{t("extensions.community.description")}</p>
              </div>
              <span><CircleAlert aria-hidden="true" size={14} />{t("extensions.community.readOnly")}</span>
            </div>
            <form className="community-search__form" onSubmit={searchCommunity}>
              <label>
                <Search aria-hidden="true" size={15} />
                <span className="sr-only">{t("extensions.community.search")}</span>
                <input
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder={t("extensions.community.placeholder")}
                  type="search"
                  value={query}
                />
              </label>
              <button className="button button--primary" disabled={searching} type="submit">
                <Search aria-hidden="true" size={14} />
                {searching ? t("extensions.community.searching") : t("extensions.community.search")}
              </button>
            </form>
            {searchError ? (
              <div className="product-state product-state--error" role="alert">
                <CircleAlert aria-hidden="true" size={18} />
                <span>{t("extensions.community.error")}</span>
              </div>
            ) : community.length > 0 ? (
              <div className="extension-list extension-list--community">
                {community.map((item) => (
                  <article className="community-extension-row" key={item.packageId}>
                    <span className="extension-row__mark"><Box aria-hidden="true" size={16} /></span>
                    <div className="extension-row__copy">
                      <div><strong>{item.packageId}</strong><code>{item.version}</code></div>
                      <p>{item.description || t("extensions.noDescription")}</p>
                      <small>{item.publisher || t("extensions.unknownPublisher")}</small>
                    </div>
                    <button className="button" disabled type="button">
                      <Download aria-hidden="true" size={14} />{t("extensions.community.reviewRequired")}
                    </button>
                  </article>
                ))}
              </div>
            ) : (
              <div className="product-state">
                <Globe2 aria-hidden="true" size={20} />
                <span>{t("extensions.community.empty")}</span>
              </div>
            )}
          </section>
        ) : loading ? (
          <div className="product-state" role="status">{t("extensions.loading")}</div>
        ) : loadError ? (
          <div className="product-state product-state--error" role="alert">
            <CircleAlert aria-hidden="true" size={18} />
            <span>{t("extensions.loadError")}</span>
            <button className="button" onClick={() => void load()} type="button">{t("common.retry")}</button>
          </div>
        ) : visibleExtensions.length === 0 ? (
          <div className="product-state">
            <PackageCheck aria-hidden="true" size={20} />
            <span>{t("extensions.empty")}</span>
          </div>
        ) : (
          <div className="extension-list">
            {visibleExtensions.map((extension) => {
              const expanded = expandedPackage === extension.packageId;
              const tools = Array.isArray(extension.manifest.tools)
                ? extension.manifest.tools.filter((tool): tool is string => typeof tool === "string")
                : [];
              return (
                <article className="extension-row" data-expanded={expanded} key={extension.packageId}>
                  <button
                    aria-expanded={expanded}
                    className="extension-row__summary"
                    onClick={() => setExpandedPackage(expanded ? null : extension.packageId)}
                    type="button"
                  >
                    <span className="extension-row__mark"><Network aria-hidden="true" size={16} /></span>
                    <span className="extension-row__copy">
                      <span className="extension-row__title">
                        <strong>{extension.displayName}</strong>
                        <code>{extension.installedVersion ?? extension.latestVersion}</code>
                      </span>
                      <span>{extension.description}</span>
                    </span>
                    <span className={`extension-tier extension-tier--${extension.trustTier}`}>
                      {trustIcon(extension.trustTier)}
                      {t(`extensions.tiers.${extension.trustTier}`)}
                    </span>
                    <span className="extension-row__status">
                      <Check aria-hidden="true" size={13} />
                      {t(`extensions.status.${extension.lifecycleStatus}`)}
                    </span>
                  </button>
                  {expanded && (
                    <div className="extension-row__details">
                      <section>
                        <h3><TerminalSquare aria-hidden="true" size={14} />{t("extensions.tools")}</h3>
                        <div className="extension-tool-list">
                          {tools.map((tool) => <code key={tool}>{tool}</code>)}
                        </div>
                      </section>
                      <section>
                        <h3><ShieldCheck aria-hidden="true" size={14} />{t("extensions.permissions")}</h3>
                        <div className="extension-permission-list">
                          {extension.permissions.network === true && <span><Network aria-hidden="true" size={13} />{t("extensions.permissionNetwork")}</span>}
                          {Boolean(extension.permissions.filesystem) && <span><HardDrive aria-hidden="true" size={13} />{t("extensions.permissionFiles")}</span>}
                          {extension.permissions.subprocess === true && <span><TerminalSquare aria-hidden="true" size={13} />{t("extensions.permissionProcess")}</span>}
                        </div>
                        <p>{t("extensions.trustedExecutionNotice")}</p>
                      </section>
                      <section className="extension-agent-access">
                        <h3><UsersRound aria-hidden="true" size={14} />{t("extensions.agentAccess")}</h3>
                        <p className="extension-agent-access__global"><Check aria-hidden="true" size={13} />{t("extensions.allAgents")}</p>
                      </section>
                    </div>
                  )}
                </article>
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
}
