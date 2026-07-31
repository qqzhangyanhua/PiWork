import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";

import type {
  AvailableModel,
  ModelConfigurationSummary,
  ModelProvider,
  PiWorkClient,
} from "../../app/tauriClient";
import { ContinuousLoopLogo } from "../../components/brand/ContinuousLoopLogo";

const providers: Array<{ id: ModelProvider; label: string; baseUrl: string }> = [
  { id: "openai", label: "OpenAI", baseUrl: "https://api.openai.com/v1" },
  { id: "anthropic", label: "Anthropic", baseUrl: "https://api.anthropic.com/v1" },
  { id: "google", label: "Google Gemini", baseUrl: "https://generativelanguage.googleapis.com/v1beta" },
  { id: "openrouter", label: "OpenRouter", baseUrl: "https://openrouter.ai/api/v1" },
  { id: "deepseek", label: "DeepSeek", baseUrl: "https://api.deepseek.com" },
  { id: "custom", label: "OpenAI-compatible", baseUrl: "" },
];

export function ModelSetup({
  client,
  onConfigured,
}: {
  client: PiWorkClient;
  onConfigured(configuration: ModelConfigurationSummary): void;
}) {
  const { t } = useTranslation();
  const [provider, setProvider] = useState<ModelProvider>("openai");
  const [baseUrl, setBaseUrl] = useState(providers[0]!.baseUrl);
  const [apiKey, setApiKey] = useState("");
  const [models, setModels] = useState<AvailableModel[]>([]);
  const [modelId, setModelId] = useState("");
  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(false);

  const resetVerification = () => {
    setModels([]);
    setModelId("");
    setError(false);
  };
  const changeProvider = (next: ModelProvider) => {
    setProvider(next);
    setBaseUrl(providers.find(({ id }) => id === next)?.baseUrl ?? "");
    resetVerification();
  };
  const testConnection = async () => {
    if (!apiKey.trim() || !baseUrl.trim() || testing) return;
    setTesting(true);
    setError(false);
    try {
      const result = await client.testModelConnection({
        provider,
        apiKey: apiKey.trim(),
        baseUrl: baseUrl.trim(),
      });
      setModels(result.models);
      setModelId(result.models[0]?.id ?? "");
      if (result.models.length === 0) setError(true);
    } catch {
      setModels([]);
      setModelId("");
      setError(true);
    } finally {
      setTesting(false);
    }
  };
  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (!modelId || saving) return;
    setSaving(true);
    setError(false);
    try {
      const configuration = await client.saveModelConfiguration({
        provider,
        apiKey: apiKey.trim(),
        baseUrl: baseUrl.trim(),
        modelId,
      });
      onConfigured(configuration);
    } catch {
      setError(true);
      setSaving(false);
    }
  };

  return (
    <main className="model-setup">
      <ContinuousLoopLogo showWordmark />
      <section className="model-setup__card">
        <p className="model-setup__eyebrow">{t("model.required")}</p>
        <h1>{t("model.connectTitle")}</h1>
        <p>{t("model.connectBody")}</p>
        <form className="model-setup__form" onSubmit={save}>
          <label htmlFor="model-provider">{t("model.provider")}</label>
          <select id="model-provider" value={provider} onChange={(event) => changeProvider(event.target.value as ModelProvider)} disabled={testing || saving}>
            {providers.map(({ id, label }) => <option key={id} value={id}>{label}</option>)}
          </select>
          <label htmlFor="model-api-key">{t("model.apiKey")}</label>
          <input id="model-api-key" type="password" value={apiKey} autoComplete="off" onChange={(event) => { setApiKey(event.target.value); resetVerification(); }} disabled={testing || saving} />
          {provider === "custom" && <><label htmlFor="model-base-url">{t("model.baseUrl")}</label><input id="model-base-url" type="url" value={baseUrl} onChange={(event) => { setBaseUrl(event.target.value); resetVerification(); }} disabled={testing || saving} /></>}
          <button className="button" type="button" onClick={() => void testConnection()} disabled={!apiKey.trim() || !baseUrl.trim() || testing || saving}>{testing ? t("model.testing") : t("model.test")}</button>
          {models.length > 0 && <><label htmlFor="model-default">{t("model.defaultModel")}</label><select id="model-default" value={modelId} onChange={(event) => setModelId(event.target.value)} disabled={saving}>{models.map((model) => <option key={model.id} value={model.id}>{model.label}</option>)}</select></>}
          {error && <p className="field-error" role="alert">{t("model.connectionError")}</p>}
          <button className="button button--primary" type="submit" disabled={!modelId || saving}>{saving ? t("model.saving") : t("model.save")}</button>
        </form>
      </section>
    </main>
  );
}
