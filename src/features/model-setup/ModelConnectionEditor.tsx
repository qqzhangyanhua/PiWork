import { CheckCircle2, CircleHelp, Eye, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import type {
  AvailableModel,
  ModelConfigurationSummary,
  ModelProvider,
  PiWorkClient,
} from "../../app/tauriClient";
import { BrandSelect, type BrandSelectOption } from "./BrandSelect";

const providers: Array<{ id: ModelProvider; label: string; baseUrl: string }> = [
  { id: "deepseek", label: "DeepSeek", baseUrl: "https://api.deepseek.com" },
  { id: "openai", label: "OpenAI", baseUrl: "https://api.openai.com/v1" },
  { id: "anthropic", label: "Anthropic", baseUrl: "https://api.anthropic.com/v1" },
  { id: "google", label: "Google Gemini", baseUrl: "https://generativelanguage.googleapis.com/v1beta" },
  { id: "openrouter", label: "OpenRouter", baseUrl: "https://openrouter.ai/api/v1" },
  { id: "custom", label: "OpenAI-compatible", baseUrl: "" },
];

const providerOptions = (recommended: string): BrandSelectOption[] => providers.map((provider) => ({
  value: provider.id,
  label: provider.label,
  provider: provider.id,
  badge: provider.id === "deepseek" ? recommended : undefined,
}));

export function providerBaseUrl(provider: ModelProvider) {
  return providers.find((entry) => entry.id === provider)?.baseUrl ?? "";
}

export function ModelConnectionEditor({
  client,
  configuration,
  onSaved,
  onActivated,
}: {
  client: PiWorkClient;
  configuration: ModelConfigurationSummary | null;
  onSaved(configuration: ModelConfigurationSummary): void;
  onActivated(configuration: ModelConfigurationSummary): void;
}) {
  const { t } = useTranslation();
  const [provider, setProvider] = useState<ModelProvider>(configuration?.provider ?? "deepseek");
  const [baseUrl, setBaseUrl] = useState(configuration?.baseUrl ?? providerBaseUrl(configuration?.provider ?? "deepseek"));
  const [apiKey, setApiKey] = useState("");
  const [usingStoredCredential, setUsingStoredCredential] = useState(Boolean(configuration?.credentialConfigured));
  const [models, setModels] = useState<AvailableModel[]>(configuration ? [{ id: configuration.modelId, label: configuration.modelId }] : []);
  const [modelId, setModelId] = useState(configuration?.modelId ?? "");
  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [activating, setActivating] = useState(false);
  const [connectionState, setConnectionState] = useState<"idle" | "success" | "error">("idle");
  const [saveError, setSaveError] = useState(false);
  const [activationError, setActivationError] = useState(false);

  useEffect(() => {
    const nextProvider = configuration?.provider ?? "deepseek";
    setProvider(nextProvider);
    setBaseUrl(configuration?.baseUrl ?? providerBaseUrl(nextProvider));
    setApiKey("");
    setUsingStoredCredential(Boolean(configuration?.credentialConfigured));
    setModels(configuration ? [{ id: configuration.modelId, label: configuration.modelId }] : []);
    setModelId(configuration?.modelId ?? "");
    setConnectionState("idle");
    setSaveError(false);
    setActivationError(false);
  }, [configuration]);

  const clearVerification = () => {
    setModels([]);
    setModelId("");
    setConnectionState("idle");
    setSaveError(false);
  };

  const changeProvider = (nextProvider: ModelProvider) => {
    setProvider(nextProvider);
    setBaseUrl(providerBaseUrl(nextProvider));
    setApiKey("");
    setUsingStoredCredential(false);
    clearVerification();
  };

  const resetCredential = () => {
    setApiKey("");
    setUsingStoredCredential(false);
    clearVerification();
  };

  const testConnection = async () => {
    if (testing || (!usingStoredCredential && (!apiKey.trim() || !baseUrl.trim()))) return;
    setTesting(true);
    setConnectionState("idle");
    try {
      const result = usingStoredCredential && configuration
        ? await client.testSavedModelConfiguration(configuration.id)
        : await client.testModelConnection({ provider, apiKey: apiKey.trim(), baseUrl: baseUrl.trim() });
      setModels(result.models);
      setModelId(result.models.some((model) => model.id === configuration?.modelId)
        ? configuration?.modelId ?? ""
        : result.models[0]?.id ?? "");
      setConnectionState("success");
    } catch {
      setConnectionState("error");
    } finally {
      setTesting(false);
    }
  };

  const save = async () => {
    if (!modelId || saving || (!usingStoredCredential && connectionState !== "success")) return;
    setSaving(true);
    setSaveError(false);
    try {
      let saved: ModelConfigurationSummary;
      if (configuration && usingStoredCredential) {
        saved = modelId === configuration.modelId
          ? configuration
          : await client.selectModelForConfiguration({ configurationId: configuration.id, modelId });
      } else {
        saved = await client.saveModelConfiguration({
          ...(configuration ? { id: configuration.id } : {}),
          provider,
          apiKey: apiKey.trim(),
          baseUrl: baseUrl.trim(),
          modelId,
        });
      }
      onSaved(saved);
      if (saved.active) onActivated(saved);
    } catch {
      setSaveError(true);
    } finally {
      setSaving(false);
    }
  };

  const activate = async () => {
    if (!configuration || activating) return;
    setActivating(true);
    setActivationError(false);
    try {
      const activated = await client.activateModelConfiguration(configuration.id);
      onActivated(activated);
    } catch {
      setActivationError(true);
    } finally {
      setActivating(false);
    }
  };

  const modelOptions: BrandSelectOption[] = models.map((model, index) => ({
    value: model.id,
    label: model.label || model.id,
    provider,
    badge: index === 0 ? t("settings.latest") : undefined,
  }));

  return (
    <div className="model-connection-editor">
      <section className="model-provider-card">
        <div className="model-provider-card__heading">
          <div>
            <h3>{t("settings.providerCardTitle")}</h3>
            <p>{t("settings.providerCardDescription")}</p>
          </div>
          <button className="model-provider-card__help" type="button">
            {t("settings.providerHelp")}<CircleHelp aria-hidden="true" size={15} />
          </button>
        </div>

        <div className="model-provider-card__fields">
          <BrandSelect
            id={`provider-${configuration?.id ?? "new"}`}
            label={t("model.provider")}
            onChange={(value) => changeProvider(value as ModelProvider)}
            options={providerOptions(t("settings.recommended"))}
            value={provider}
          />

          <div className="model-credential-field">
            <span className="model-credential-field__label">{t("model.apiKey")}</span>
            {usingStoredCredential ? (
              <div className="model-credential-field__stored">
                <span>{t("settings.storedCredential")}</span>
                <Eye aria-hidden="true" size={16} />
                <button className="button button--quiet" onClick={resetCredential} type="button">{t("settings.resetApiKey")}</button>
              </div>
            ) : (
              <input
                aria-label={t("model.apiKey")}
                autoComplete="new-password"
                onChange={(event) => {
                  setApiKey(event.target.value);
                  clearVerification();
                }}
                type="password"
                value={apiKey}
              />
            )}
            <p>{t("model.credentialNote")}</p>
          </div>

          {provider === "custom" && (
            <label className="model-base-url-field">
              <span>{t("model.baseUrl")}</span>
              <input
                onChange={(event) => {
                  setBaseUrl(event.target.value);
                  clearVerification();
                }}
                type="url"
                value={baseUrl}
              />
            </label>
          )}

          <BrandSelect
            disabled={models.length === 0}
            id={`model-${configuration?.id ?? "new"}`}
            label={t("settings.modelField")}
            onChange={setModelId}
            options={modelOptions}
            value={modelId}
          />

          <div className="model-provider-card__actions">
            {!configuration?.active && (
              <button className="button button--quiet" disabled={activating} onClick={() => void activate()} type="button">
                {activating ? t("settings.activatingConnection") : t("settings.setActiveConnection")}
              </button>
            )}
            <button className="button" disabled={!modelId || saving || (!usingStoredCredential && connectionState !== "success")} onClick={() => void save()} type="button">
              {saving ? t("settings.savingConfiguration") : t("settings.saveConfiguration")}
            </button>
          </div>
          {saveError && <p className="field-error" role="alert">{t("settings.saveConfigurationError")}</p>}
          {activationError && <p className="field-error" role="alert">{t("settings.activationError")}</p>}
        </div>

        <div className="model-local-priority">
          <ShieldCheck aria-hidden="true" size={18} />
          <div><strong>{t("settings.localPriorityTitle")}</strong><p>{t("settings.localPriorityBody")}</p></div>
          <span><i />{t("settings.localPriorityEnabled")}</span>
        </div>
      </section>

      <section className="model-connection-test">
        <div>
          <h3>{t("settings.connectionTestTitle")}</h3>
          <p>{t("settings.connectionTestDescription")}</p>
        </div>
        {connectionState === "success" && (
          <span className="model-connection-test__success" role="status"><CheckCircle2 aria-hidden="true" size={14} />{t("settings.connectionTestSuccess")}</span>
        )}
        <button className="button" disabled={testing || (!usingStoredCredential && (!apiKey.trim() || !baseUrl.trim()))} onClick={() => void testConnection()} type="button">
          {testing ? t("settings.testingConnection") : t("settings.testConnection")}
        </button>
      </section>
      {connectionState === "error" && <p className="model-connection-test__error" role="alert">{t("settings.connectionTestError")}</p>}
    </div>
  );
}
