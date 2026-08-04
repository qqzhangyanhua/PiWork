import { useEffect, useState, type ReactNode } from "react";
import {
  Archive,
  Boxes,
  Command,
  Cpu,
  Database,
  Info,
  Plus,
  RefreshCw,
  Settings,
  ShieldCheck,
} from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ModelConfigurationSummary, ModelProvider, PiWorkClient } from "../../app/tauriClient";
import { ModelConnectionEditor } from "../model-setup/ModelConnectionEditor";
import { ProviderLogo } from "../model-setup/ProviderLogo";

const providerLabels: Record<ModelProvider, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
  google: "Google Gemini",
  openrouter: "OpenRouter",
  deepseek: "DeepSeek",
  custom: "OpenAI-compatible",
};

function NavigationItem({ icon, label, current = false }: { icon: ReactNode; label: string; current?: boolean }) {
  return (
    <button aria-current={current ? "page" : undefined} className="settings-navigation__item" disabled={!current} type="button">
      {icon}<span>{label}</span>
    </button>
  );
}

function SettingsNavigation() {
  const { t } = useTranslation();
  return (
    <nav aria-label={t("settings.settingsNavigation")} className="settings-navigation">
      <section>
        <h2>{t("settings.generalGroup")}</h2>
        <NavigationItem icon={<Settings aria-hidden="true" size={16} />} label={t("settings.generalSettings")} />
        <NavigationItem icon={<Command aria-hidden="true" size={16} />} label={t("settings.shortcuts")} />
      </section>
      <section>
        <h2>{t("settings.modelRuntimeGroup")}</h2>
        <NavigationItem current icon={<Cpu aria-hidden="true" size={16} />} label={t("settings.modelRuntimeNav")} />
        <NavigationItem icon={<Boxes aria-hidden="true" size={16} />} label={t("settings.localResources")} />
      </section>
      <section>
        <h2>{t("settings.dataSecurityGroup")}</h2>
        <NavigationItem icon={<Database aria-hidden="true" size={16} />} label={t("settings.dataStorage")} />
        <NavigationItem icon={<ShieldCheck aria-hidden="true" size={16} />} label={t("settings.privacySecurity")} />
        <NavigationItem icon={<Archive aria-hidden="true" size={16} />} label={t("settings.exportBackup")} />
      </section>
      <section>
        <h2>{t("settings.aboutGroup")}</h2>
        <NavigationItem icon={<Info aria-hidden="true" size={16} />} label={t("settings.aboutPiWork")} />
        <NavigationItem icon={<RefreshCw aria-hidden="true" size={16} />} label={t("settings.checkUpdates")} />
      </section>
    </nav>
  );
}

function ConnectionTabs({
  configurations,
  adding,
  selectedId,
  onAdd,
  onSelect,
}: {
  configurations: ModelConfigurationSummary[];
  adding: boolean;
  selectedId: string | null;
  onAdd(): void;
  onSelect(id: string): void;
}) {
  const { t } = useTranslation();
  return (
    <div className="model-connections-bar">
      <div aria-label={t("settings.connections")} className="model-connection-tabs" role="tablist">
        {configurations.map((configuration) => (
          <button
            aria-selected={!adding && selectedId === configuration.id}
            className="model-connection-tab"
            key={configuration.id}
            onClick={() => onSelect(configuration.id)}
            role="tab"
            type="button"
          >
            <ProviderLogo provider={configuration.provider} size={17} />
            <span>{providerLabels[configuration.provider]}</span>
            <small>{configuration.modelId}</small>
            {configuration.active && <em>{t("settings.currentConnection")}</em>}
          </button>
        ))}
      </div>
      <button aria-pressed={adding} className="model-connection-add" onClick={onAdd} type="button">
        <Plus aria-hidden="true" size={15} />{t("settings.addConnection")}
      </button>
    </div>
  );
}

export function SettingsPage({
  client,
  configuration,
  onModelConfigured,
}: {
  client: PiWorkClient;
  configuration: ModelConfigurationSummary | null;
  onModelConfigured(configuration: ModelConfigurationSummary): void;
}) {
  const { t } = useTranslation();
  const [configurations, setConfigurations] = useState<ModelConfigurationSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState(false);

  const load = async () => {
    setLoading(true);
    setLoadError(false);
    try {
      const loaded = await client.listModelConfigurations();
      setConfigurations(loaded);
      const selected = loaded.find((item) => item.id === configuration?.id)
        ?? loaded.find((item) => item.active)
        ?? loaded[0]
        ?? null;
      setSelectedId(selected?.id ?? null);
      setAdding(loaded.length === 0);
    } catch {
      setLoadError(true);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { void load(); }, [client]);

  const selectedConfiguration = adding
    ? null
    : configurations.find((item) => item.id === selectedId) ?? null;

  const saved = (next: ModelConfigurationSummary) => {
    setConfigurations((current) => {
      const normalized = next.active ? current.map((item) => ({ ...item, active: false })) : current;
      return normalized.some((item) => item.id === next.id)
        ? normalized.map((item) => item.id === next.id ? next : item)
        : [...normalized, next];
    });
    setSelectedId(next.id);
    setAdding(false);
  };

  const activated = (next: ModelConfigurationSummary) => {
    setConfigurations((current) => {
      const exists = current.some((item) => item.id === next.id);
      const updated = current.map((item) => ({ ...item, active: item.id === next.id }));
      return exists ? updated.map((item) => item.id === next.id ? { ...next, active: true } : item) : [...updated, { ...next, active: true }];
    });
    setSelectedId(next.id);
    setAdding(false);
    onModelConfigured(next);
  };

  return (
    <section aria-labelledby="settings-page-title" className="settings-page">
      <header className="settings-page__header">
        <h1 id="settings-page-title">{t("settings.title")}</h1>
        <p>{t("settings.description")}</p>
      </header>

      <div className="settings-page__layout">
        <SettingsNavigation />
        <section aria-labelledby="settings-model-runtime-title" className="model-settings-panel">
          <header className="model-settings-panel__header">
            <div>
              <h2 id="settings-model-runtime-title">{t("settings.modelRuntimeTitle")}</h2>
              <p>{t("settings.modelRuntimeDescription")}</p>
            </div>
          </header>

          {loading ? (
            <p className="model-settings-panel__state" role="status">{t("settings.loadingModels")}</p>
          ) : loadError ? (
            <div className="model-settings-panel__state model-settings-panel__state--error" role="alert">
              <span>{t("settings.loadModelsError")}</span>
              <button className="button" onClick={() => void load()} type="button">{t("common.retry")}</button>
            </div>
          ) : (
            <>
              <ConnectionTabs
                adding={adding}
                configurations={configurations}
                onAdd={() => setAdding(true)}
                onSelect={(id) => { setSelectedId(id); setAdding(false); }}
                selectedId={selectedId}
              />
              <ModelConnectionEditor
                key={selectedConfiguration?.id ?? "new-connection"}
                client={client}
                configuration={selectedConfiguration}
                onActivated={activated}
                onSaved={saved}
              />
              <details className="model-settings-advanced">
                <summary>
                  <span><strong>{t("settings.advancedOptions")}</strong><small>{t("settings.advancedOptionsDescription")}</small></span>
                </summary>
              </details>
            </>
          )}
        </section>
      </div>
    </section>
  );
}
