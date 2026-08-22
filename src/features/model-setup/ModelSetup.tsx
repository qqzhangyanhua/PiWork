import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";

import type {
  AvailableModel,
  ModelConfigurationSummary,
  ModelProvider,
  PiWorkClient,
} from "../../app/tauriClient";
import { CoDoLogo } from "../../components/brand/CoDoLogo";

const providers: Array<{ id: ModelProvider; label: string; baseUrl: string }> = [
  { id: "openai", label: "OpenAI", baseUrl: "https://api.openai.com/v1" },
  { id: "anthropic", label: "Anthropic", baseUrl: "https://api.anthropic.com/v1" },
  { id: "google", label: "Google Gemini", baseUrl: "https://generativelanguage.googleapis.com/v1beta" },
  { id: "openrouter", label: "OpenRouter", baseUrl: "https://openrouter.ai/api/v1" },
  { id: "deepseek", label: "DeepSeek", baseUrl: "https://api.deepseek.com" },
  { id: "custom", label: "OpenAI-compatible", baseUrl: "" },
];

function providerBaseUrl(provider: ModelProvider) {
  return providers.find(({ id }) => id === provider)?.baseUrl ?? "";
}

export function ModelConfigurationForm({
  client,
  initialConfiguration,
  mode,
  onConfigured,
}: {
  client: PiWorkClient;
  initialConfiguration?: ModelConfigurationSummary | null;
  mode: "setup" | "settings";
  onConfigured(configuration: ModelConfigurationSummary): void;
}) {
  const { t } = useTranslation();
  const initialProvider = initialConfiguration?.provider ?? "openai";
  const [provider, setProvider] = useState<ModelProvider>(initialProvider);
  const [baseUrl, setBaseUrl] = useState(providerBaseUrl(initialProvider));
  const [apiKey, setApiKey] = useState("");
  const [models, setModels] = useState<AvailableModel[]>([]);
  const [modelId, setModelId] = useState("");
  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [connectionState, setConnectionState] = useState<"idle" | "success" | "empty" | "error">("idle");
  const [saveError, setSaveError] = useState(false);
  const [saved, setSaved] = useState(false);

  const resetVerification = () => {
    setModels([]);
    setModelId("");
    setConnectionState("idle");
    setSaveError(false);
    setSaved(false);
  };
  const changeProvider = (next: ModelProvider) => {
    setProvider(next);
    setBaseUrl(providerBaseUrl(next));
    resetVerification();
  };
  const testConnection = async () => {
    if (!apiKey.trim() || !baseUrl.trim() || testing) return;
    setTesting(true);
    setConnectionState("idle");
    try {
      const result = await client.testModelConnection({
        provider,
        apiKey: apiKey.trim(),
        baseUrl: baseUrl.trim(),
      });
      setModels(result.models);
      setModelId(result.models[0]?.id ?? "");
      setConnectionState(result.models.length > 0 ? "success" : "empty");
    } catch {
      setModels([]);
      setModelId("");
      setConnectionState("error");
    } finally {
      setTesting(false);
    }
  };
  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (!modelId || saving) return;
    setSaving(true);
    setSaveError(false);
    try {
      const configuration = await client.saveModelConfiguration({
        provider,
        apiKey: apiKey.trim(),
        baseUrl: baseUrl.trim(),
        modelId,
      });
      setSaving(false);
      setConnectionState("idle");
      setSaved(true);
      onConfigured(configuration);
    } catch {
      setSaveError(true);
      setSaving(false);
    }
  };

  return (
    <form className="model-setup__form" onSubmit={save} aria-busy={testing || saving}>
      <label htmlFor={`model-provider-${mode}`}>{t("model.provider")}</label>
      <select id={`model-provider-${mode}`} value={provider} onChange={(event) => changeProvider(event.target.value as ModelProvider)} disabled={testing || saving}>
        {providers.map(({ id, label }) => <option key={id} value={id}>{label}</option>)}
      </select>
      <label htmlFor={`model-api-key-${mode}`}>{t("model.apiKey")}</label>
      <input id={`model-api-key-${mode}`} type="password" value={apiKey} autoComplete="new-password" onChange={(event) => { setApiKey(event.target.value); resetVerification(); }} disabled={testing || saving} />
      <p className="model-setup__credential-note">{t("model.credentialNote")}</p>
      {provider === "custom" && <><label htmlFor={`model-base-url-${mode}`}>{t("model.baseUrl")}</label><input id={`model-base-url-${mode}`} type="url" value={baseUrl} onChange={(event) => { setBaseUrl(event.target.value); resetVerification(); }} disabled={testing || saving} /></>}
      <button className="button" type="button" onClick={() => void testConnection()} disabled={!apiKey.trim() || !baseUrl.trim() || testing || saving}>{testing ? t("model.testing") : t("model.test")}</button>
      {testing && <p className="model-setup__connection-status" role="status">{t("model.testingStatus")}</p>}
      {connectionState === "success" && <p className="model-setup__connection-status model-setup__connection-status--success" role="status">{t("model.connectionSuccess")}</p>}
      {connectionState === "empty" && <p className="model-setup__connection-status" role="status">{t("model.noModels")}</p>}
      {models.length > 0 && <><label htmlFor={`model-default-${mode}`}>{t("model.defaultModel")}</label><select id={`model-default-${mode}`} value={modelId} onChange={(event) => setModelId(event.target.value)} disabled={saving}>{models.map((model) => <option key={model.id} value={model.id}>{model.label}</option>)}</select></>}
      {connectionState === "error" && <p className="field-error" role="alert">{t("model.connectionError")}</p>}
      {saveError && <p className="field-error" role="alert">{t("model.saveError")}</p>}
      {mode === "settings" && saved && <p className="model-setup__saved" role="status">{t("settings.saved")}</p>}
      <button className="button button--primary" type="submit" disabled={!modelId || saving}>{saving ? t("model.saving") : t(mode === "settings" ? "model.saveChanges" : "model.save")}</button>
    </form>
  );
}

export function ModelSetup({
  client,
  onConfigured,
}: {
  client: PiWorkClient;
  onConfigured(configuration: ModelConfigurationSummary): void;
}) {
  const { t } = useTranslation();
  return (
    <main className="model-setup">
      <CoDoLogo showWordmark />
      <section className="model-setup__card">
        <p className="model-setup__eyebrow">{t("model.required")}</p>
        <h1>{t("model.connectTitle")}</h1>
        <p>{t("model.connectBody")}</p>
        <ModelConfigurationForm client={client} mode="setup" onConfigured={onConfigured} />
      </section>
    </main>
  );
}
