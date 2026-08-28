import {
  Check,
  CircleAlert,
  Eye,
  EyeOff,
  Globe2,
  KeyRound,
  Link2,
  Save,
  Search,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import type {
  PiWorkClient,
  SaveWebAccessSettingsInput,
  WebAccessSettingsSummary,
} from "../../app/tauriClient";
import { credentialVaultLabel } from "../../i18n/credentialVault";

const PROVIDER_LABELS: Record<string, string> = {
  exa: "Exa",
  brave: "Brave Search",
  tavily: "Tavily",
  bocha: "Bocha",
  jina: "Jina AI",
  firecrawl: "Firecrawl",
  openai: "OpenAI Search",
  searxng: "SearXNG",
};

type DraftProvider = WebAccessSettingsSummary["providers"][number] & {
  apiKey: string;
  clearCredential: boolean;
  revealKey: boolean;
};

type Draft = Omit<WebAccessSettingsSummary, "providers"> & { providers: DraftProvider[] };

const toDraft = (settings: WebAccessSettingsSummary): Draft => ({
  ...settings,
  providers: settings.providers.map((provider) => ({
    ...provider,
    apiKey: "",
    clearCredential: false,
    revealKey: false,
  })),
});

export function WebAccessSettingsPanel({ client }: { client: PiWorkClient }) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState<Draft | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const load = async () => {
    setLoading(true);
    setLoadError(false);
    try {
      const settings = await client.getWebAccessSettings?.();
      if (!settings) throw new Error("web access settings command is unavailable");
      setDraft(toDraft(settings));
    } catch {
      setLoadError(true);
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => { void load(); }, [client]);

  const enabledProviders = useMemo(
    () => draft?.providers.filter(({ enabled }) => enabled) ?? [],
    [draft],
  );
  const updateProvider = (providerId: string, update: Partial<DraftProvider>) => {
    setSaved(false);
    setDraft((current) => current ? ({
      ...current,
      providers: current.providers.map((provider) =>
        provider.providerId === providerId ? { ...provider, ...update } : provider),
    }) : current);
  };
  const toggleProvider = (providerId: string, enabled: boolean) => {
    setDraft((current) => {
      if (!current) return current;
      const providers = current.providers.map((provider) =>
        provider.providerId === providerId ? { ...provider, enabled } : provider);
      const enabledIds = new Set(providers.filter((provider) => provider.enabled).map((provider) => provider.providerId));
      const defaultProvider = current.defaultProvider && enabledIds.has(current.defaultProvider)
        ? current.defaultProvider
        : providers.find((provider) => provider.enabled)?.providerId ?? null;
      const fallbackProvider = current.fallbackProvider
        && enabledIds.has(current.fallbackProvider)
        && current.fallbackProvider !== defaultProvider
        ? current.fallbackProvider
        : null;
      return { ...current, providers, defaultProvider, fallbackProvider };
    });
    setSaved(false);
  };
  const save = async () => {
    if (!draft || !client.saveWebAccessSettings) return;
    setSaving(true);
    setSaveError(null);
    setSaved(false);
    const input: SaveWebAccessSettingsInput = {
      enabled: draft.enabled,
      urlFetchEnabled: draft.urlFetchEnabled,
      defaultProvider: draft.defaultProvider,
      fallbackProvider: draft.fallbackProvider,
      providers: draft.providers.map((provider) => ({
        providerId: provider.providerId,
        enabled: provider.enabled,
        endpoint: provider.endpoint,
        apiKey: provider.apiKey || null,
        clearCredential: provider.clearCredential,
      })),
    };
    try {
      const settings = await client.saveWebAccessSettings(input);
      setDraft(toDraft(settings));
      setSaved(true);
    } catch (error) {
      const message = error && typeof error === "object" && "message" in error
        ? String(error.message)
        : t("webAccess.saveError");
      setSaveError(message);
    } finally {
      setSaving(false);
    }
  };

  if (loading) return <div className="web-access-state" role="status">{t("webAccess.loading")}</div>;
  if (loadError || !draft) {
    return (
      <div className="web-access-state web-access-state--error" role="alert">
        <CircleAlert aria-hidden="true" size={18} />
        <span>{t("webAccess.loadError")}</span>
        <button className="button" onClick={() => void load()} type="button">{t("common.retry")}</button>
      </div>
    );
  }

  return (
    <section aria-labelledby="web-access-title" className="web-access-settings">
      <header className="model-settings-panel__header web-access-settings__header">
        <div>
          <h2 id="web-access-title">{t("webAccess.title")}</h2>
          <p>{t("webAccess.description")}</p>
        </div>
        <label className="settings-switch">
          <input
            checked={draft.enabled}
            onChange={(event) => {
              setDraft({ ...draft, enabled: event.target.checked });
              setSaved(false);
            }}
            type="checkbox"
          />
          <span aria-hidden="true" />
          <strong>{draft.enabled ? t("common.enabled") : t("common.disabled")}</strong>
        </label>
      </header>

      <div className="web-access-settings__content">
        <section className="web-access-routing" aria-labelledby="web-access-routing-title">
          <div className="settings-section-heading">
            <div><Search aria-hidden="true" size={15} /><h3 id="web-access-routing-title">{t("webAccess.routingTitle")}</h3></div>
            <p>{t("webAccess.routingDescription")}</p>
          </div>
          <div className="web-access-routing__controls">
            <label>
              <span>{t("webAccess.defaultProvider")}</span>
              <select
                disabled={enabledProviders.length === 0}
                onChange={(event) => {
                  const value = event.target.value || null;
                  setDraft({
                    ...draft,
                    defaultProvider: value,
                    fallbackProvider: draft.fallbackProvider === value ? null : draft.fallbackProvider,
                  });
                  setSaved(false);
                }}
                value={draft.defaultProvider ?? ""}
              >
                {enabledProviders.length === 0 && <option value="">{t("webAccess.noProvider")}</option>}
                {enabledProviders.map((provider) => (
                  <option key={provider.providerId} value={provider.providerId}>{PROVIDER_LABELS[provider.providerId]}</option>
                ))}
              </select>
            </label>
            <label>
              <span>{t("webAccess.fallbackProvider")}</span>
              <select
                disabled={enabledProviders.length < 2}
                onChange={(event) => {
                  setDraft({ ...draft, fallbackProvider: event.target.value || null });
                  setSaved(false);
                }}
                value={draft.fallbackProvider ?? ""}
              >
                <option value="">{t("webAccess.noFallback")}</option>
                {enabledProviders
                  .filter(({ providerId }) => providerId !== draft.defaultProvider)
                  .map((provider) => (
                    <option key={provider.providerId} value={provider.providerId}>{PROVIDER_LABELS[provider.providerId]}</option>
                  ))}
              </select>
            </label>
            <label className="web-access-fetch-toggle">
              <input
                checked={draft.urlFetchEnabled}
                onChange={(event) => {
                  setDraft({ ...draft, urlFetchEnabled: event.target.checked });
                  setSaved(false);
                }}
                type="checkbox"
              />
              <span><Link2 aria-hidden="true" size={14} /><strong>{t("webAccess.urlFetch")}</strong><small>{t("webAccess.urlFetchDescription")}</small></span>
            </label>
          </div>
        </section>

        <section className="web-provider-section" aria-labelledby="web-provider-title">
          <div className="settings-section-heading">
            <div><Globe2 aria-hidden="true" size={15} /><h3 id="web-provider-title">{t("webAccess.providersTitle")}</h3></div>
            <p>{t("webAccess.providersDescription", { vault: credentialVaultLabel(t) })}</p>
          </div>
          <div className="web-provider-list">
            {draft.providers.map((provider) => {
              const needsEndpoint = provider.providerId === "searxng";
              const optionalEndpoint = provider.providerId === "openai" || provider.providerId === "firecrawl";
              return (
                <article className="web-provider-row" data-enabled={provider.enabled} key={provider.providerId}>
                  <div className="web-provider-row__identity">
                    <span>{provider.providerId === "searxng" ? <ShieldCheck aria-hidden="true" size={15} /> : <Globe2 aria-hidden="true" size={15} />}</span>
                    <div><strong>{PROVIDER_LABELS[provider.providerId]}</strong><small>{t(`webAccess.providerDescriptions.${provider.providerId}`)}</small></div>
                  </div>
                  <label className="settings-switch settings-switch--compact">
                    <input checked={provider.enabled} onChange={(event) => toggleProvider(provider.providerId, event.target.checked)} type="checkbox" />
                    <span aria-hidden="true" />
                    <strong>{provider.enabled ? t("common.enabled") : t("common.disabled")}</strong>
                  </label>
                  <div className="web-provider-row__configuration">
                    {provider.providerId !== "searxng" && (
                      <label className="web-provider-key">
                        <KeyRound aria-hidden="true" size={14} />
                        <span className="sr-only">{t("webAccess.apiKey", { provider: PROVIDER_LABELS[provider.providerId] })}</span>
                        <input
                          autoComplete="off"
                          disabled={provider.clearCredential}
                          onChange={(event) => updateProvider(provider.providerId, { apiKey: event.target.value })}
                          placeholder={provider.credentialConfigured ? t("webAccess.keyConfigured") : t("webAccess.keyPlaceholder")}
                          type={provider.revealKey ? "text" : "password"}
                          value={provider.apiKey}
                        />
                        <button
                          aria-label={provider.revealKey ? t("webAccess.hideKey") : t("webAccess.showKey")}
                          onClick={() => updateProvider(provider.providerId, { revealKey: !provider.revealKey })}
                          title={provider.revealKey ? t("webAccess.hideKey") : t("webAccess.showKey")}
                          type="button"
                        >
                          {provider.revealKey ? <EyeOff aria-hidden="true" size={14} /> : <Eye aria-hidden="true" size={14} />}
                        </button>
                        {provider.credentialConfigured && (
                          <button
                            aria-label={t("webAccess.clearKey")}
                            className={provider.clearCredential ? "is-pending" : undefined}
                            onClick={() => updateProvider(provider.providerId, {
                              apiKey: "",
                              clearCredential: !provider.clearCredential,
                            })}
                            title={t("webAccess.clearKey")}
                            type="button"
                          >
                            <Trash2 aria-hidden="true" size={14} />
                          </button>
                        )}
                      </label>
                    )}
                    {(needsEndpoint || optionalEndpoint) && (
                      <label className="web-provider-endpoint">
                        <Link2 aria-hidden="true" size={14} />
                        <span className="sr-only">{t("webAccess.endpoint")}</span>
                        <input
                          onChange={(event) => updateProvider(provider.providerId, { endpoint: event.target.value || null })}
                          placeholder={needsEndpoint ? "https://search.example.com" : t("webAccess.endpointOptional")}
                          type="url"
                          value={provider.endpoint ?? ""}
                        />
                      </label>
                    )}
                  </div>
                  <span className="web-provider-row__credential">
                    {provider.providerId === "searxng" || provider.credentialConfigured
                      ? <><Check aria-hidden="true" size={13} />{t("webAccess.ready")}</>
                      : <>{t("webAccess.keyRequired")}</>}
                  </span>
                </article>
              );
            })}
          </div>
        </section>
      </div>

      <footer className="web-access-settings__footer">
        <div aria-live="polite">
          {saveError && <span className="is-error" role="alert"><CircleAlert aria-hidden="true" size={14} />{saveError}</span>}
          {saved && <span className="is-saved"><Check aria-hidden="true" size={14} />{t("webAccess.saved")}</span>}
        </div>
        <button className="button button--primary" disabled={saving} onClick={() => void save()} type="button">
          <Save aria-hidden="true" size={14} />{saving ? t("webAccess.saving") : t("common.save")}
        </button>
      </footer>
    </section>
  );
}
