import { useEffect, useState } from "react";
import {
  AlertCircle,
  CheckCircle2,
  Cloud,
  Eye,
  EyeOff,
  FolderGit2,
  KeyRound,
  RefreshCw,
  Save,
  ShieldAlert,
  Trash2,
} from "lucide-react";
import { useTranslation } from "react-i18next";

import type {
  MemoryConnectionTestResult,
  MemorySettingsSummary,
  PiWorkClient,
  WorkspaceMemoryBindingSummary,
} from "../../app/tauriClient";
import { credentialVaultLabel } from "../../i18n/credentialVault";

const defaultSettings: MemorySettingsSummary = {
  enabled: false,
  hubEndpoint: "http://124.221.254.61",
  endpoint: "http://124.221.254.61/mem",
  authMode: "basic",
  authUsername: "tdai",
  allowInsecureHttp: false,
  serviceId: "default",
  teamId: "team-eb16plgnne",
  userId: "usr-1faz3ley78",
  requestTimeoutMs: 5000,
  recallTimeoutMs: 1500,
  maxRecallItems: 8,
  maxRecallChars: 6000,
  captureEnabled: true,
  recallEnabled: true,
  apiKeyConfigured: false,
  userKeyConfigured: false,
};

type SaveState = "idle" | "saving" | "saved" | "error";
type TestState = "idle" | "testing" | "success" | "error";

function Toggle({
  checked,
  label,
  onChange,
}: {
  checked: boolean;
  label: string;
  onChange(checked: boolean): void;
}) {
  return (
    <label className="settings-switch settings-switch--compact">
      <input checked={checked} onChange={(event) => onChange(event.target.checked)} type="checkbox" />
      <span aria-hidden="true" />
      <strong>{label}</strong>
    </label>
  );
}

function workspaceName(rootPath: string) {
  return rootPath.split(/[\\/]/u).filter(Boolean).at(-1) ?? rootPath;
}

export function MemorySettingsPanel({ client }: { client: PiWorkClient }) {
  const { t } = useTranslation();
  const [settings, setSettings] = useState(defaultSettings);
  const [bindings, setBindings] = useState<WorkspaceMemoryBindingSummary[]>([]);
  const [apiKey, setApiKey] = useState("");
  const [userKey, setUserKey] = useState("");
  const [showApiKey, setShowApiKey] = useState(false);
  const [showUserKey, setShowUserKey] = useState(false);
  const [clearApiKey, setClearApiKey] = useState(false);
  const [clearUserKey, setClearUserKey] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState(false);
  const [saveState, setSaveState] = useState<SaveState>("idle");
  const [testState, setTestState] = useState<TestState>("idle");
  const [testFailureCode, setTestFailureCode] = useState<
    MemoryConnectionTestResult["failureCode"]
  >(null);
  const [latency, setLatency] = useState<number | null>(null);
  const isRemoteHttp = settings.endpoint.trim().toLowerCase().startsWith("http://")
    && !/^http:\/\/(localhost|127\.0\.0\.1|\[::1\])(?::|\/|$)/iu.test(settings.endpoint.trim());
  const authSecretLabel = settings.authMode === "basic"
    ? t("memory.basicPassword")
    : t("memory.gatewayApiKey");

  const markConfigurationChanged = () => {
    setSaveState("idle");
    setTestState("idle");
    setTestFailureCode(null);
    setLatency(null);
  };

  const updateSettings = (patch: Partial<MemorySettingsSummary>) => {
    setSettings((current) => ({ ...current, ...patch }));
    markConfigurationChanged();
  };

  const load = async () => {
    if (!client.getMemorySettings || !client.listWorkspaceMemoryBindings) {
      setLoadError(true);
      setLoading(false);
      return;
    }
    setLoading(true);
    setLoadError(false);
    try {
      const [nextSettings, nextBindings] = await Promise.all([
        client.getMemorySettings(),
        client.listWorkspaceMemoryBindings(),
      ]);
      setSettings(nextSettings);
      setBindings(nextBindings);
    } catch {
      setLoadError(true);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { void load(); }, [client]);

  const persist = async () => {
    if (!client.saveMemorySettings || !client.saveWorkspaceMemoryBinding) return null;
    setSaveState("saving");
    setTestState("idle");
    setTestFailureCode(null);
    try {
      const savedSettings = await client.saveMemorySettings({
        ...settings,
        apiKey: apiKey || null,
        userKey: userKey || null,
        clearApiKey,
        clearUserKey,
      });
      const savedBindings = await Promise.all(bindings.map((binding) =>
        client.saveWorkspaceMemoryBinding!({
          rootPath: binding.rootPath,
          enabled: binding.enabled,
          captureEnabled: binding.captureEnabled,
          recallEnabled: binding.recallEnabled,
        })));
      setSettings(savedSettings);
      setBindings(savedBindings);
      setApiKey("");
      setUserKey("");
      setClearApiKey(false);
      setClearUserKey(false);
      setSaveState("saved");
      return savedSettings;
    } catch {
      setSaveState("error");
      return null;
    }
  };

  const testConnection = async () => {
    if (!client.testMemoryConnection) return;
    setTestState("testing");
    setTestFailureCode(null);
    setLatency(null);
    const savedSettings = await persist();
    if (!savedSettings) {
      setTestState("error");
      return;
    }
    setTestState("testing");
    try {
      const result = await client.testMemoryConnection();
      if (
        result.resolvedUserId
        && result.resolvedUserId !== savedSettings.userId
        && client.saveMemorySettings
      ) {
        try {
          const correctedSettings = await client.saveMemorySettings({
            ...savedSettings,
            userId: result.resolvedUserId,
            apiKey: null,
            userKey: null,
            clearApiKey: false,
            clearUserKey: false,
          });
          setSettings(correctedSettings);
        } catch {
          setSettings({ ...savedSettings, userId: result.resolvedUserId });
          setSaveState("error");
        }
      }
      setLatency(result.latencyMs);
      setTestFailureCode(result.failureCode);
      setTestState(result.healthy && result.authenticated ? "success" : "error");
    } catch {
      setTestFailureCode(null);
      setTestState("error");
    }
  };

  const updateBinding = (
    rootPath: string,
    patch: Partial<WorkspaceMemoryBindingSummary>,
  ) => {
    setBindings((current) => current.map((binding) =>
      binding.rootPath === rootPath ? { ...binding, ...patch } : binding));
    setSaveState("idle");
    setTestState("idle");
    setTestFailureCode(null);
    setLatency(null);
  };

  if (loading) {
    return <div className="memory-settings-state" role="status">{t("memory.loading")}</div>;
  }
  if (loadError) {
    return (
      <div className="memory-settings-state memory-settings-state--error" role="alert">
        <AlertCircle aria-hidden="true" size={16} />
        <span>{t("memory.loadError")}</span>
        <button className="button" onClick={() => void load()} type="button">{t("common.retry")}</button>
      </div>
    );
  }

  return (
    <section aria-labelledby="memory-settings-title" className="memory-settings">
      <header className="model-settings-panel__header memory-settings__header">
        <div>
          <h2 id="memory-settings-title">{t("memory.title")}</h2>
          <p>{t("memory.description")}</p>
        </div>
        <Toggle
          checked={settings.enabled}
          label={settings.enabled ? t("common.enabled") : t("common.disabled")}
          onChange={(enabled) => updateSettings({ enabled })}
        />
      </header>

      <div className="memory-settings__content">
        <section aria-labelledby="memory-connection-title" className="memory-settings__section">
          <div className="settings-section-heading">
            <div><Cloud aria-hidden="true" size={15} /><h3 id="memory-connection-title">{t("memory.connectionTitle")}</h3></div>
            <p>{t("memory.connectionDescription", { vault: credentialVaultLabel(t) })}</p>
          </div>
          <div className="memory-connection-grid">
            <label className="memory-field memory-field--wide">
              <span>{t("memory.hubEndpoint")}</span>
              <input
                onChange={(event) => updateSettings({ hubEndpoint: event.target.value })}
                placeholder="https://memory.example.com"
                spellCheck={false}
                value={settings.hubEndpoint}
              />
            </label>
            <label className="memory-field memory-field--wide">
              <span>{t("memory.endpoint")}</span>
              <input
                onChange={(event) => updateSettings({ endpoint: event.target.value })}
                placeholder="https://memory.example.com"
                spellCheck={false}
                value={settings.endpoint}
              />
            </label>
            <fieldset className="memory-field memory-field--wide memory-auth-field">
              <legend>{t("memory.authMode")}</legend>
              <div aria-label={t("memory.authMode")} className="memory-auth-mode" role="radiogroup">
                {(["basic", "gatewayBearer"] as const).map((authMode) => (
                  <button
                    aria-checked={settings.authMode === authMode}
                    className={settings.authMode === authMode ? "is-selected" : undefined}
                    key={authMode}
                    onClick={() => updateSettings({ authMode })}
                    role="radio"
                    type="button"
                  >
                    {t(`memory.authModes.${authMode}`)}
                  </button>
                ))}
              </div>
            </fieldset>
            {settings.authMode === "basic" && (
              <label className="memory-field">
                <span>{t("memory.basicUsername")}</span>
                <input
                  onChange={(event) => updateSettings({ authUsername: event.target.value })}
                  spellCheck={false}
                  value={settings.authUsername}
                />
              </label>
            )}
            <label className="memory-field memory-field--credential">
              <span><KeyRound aria-hidden="true" size={13} />{authSecretLabel}</span>
              <span className="memory-secret-input">
                <input
                  aria-label={authSecretLabel}
                  onChange={(event) => {
                    setApiKey(event.target.value);
                    setClearApiKey(false);
                    markConfigurationChanged();
                  }}
                  placeholder={settings.apiKeyConfigured && !clearApiKey ? t("memory.credentialStored") : t("memory.authSecretRequired")}
                  type={showApiKey ? "text" : "password"}
                  value={apiKey}
                />
                <button aria-label={showApiKey ? t("memory.hideKey") : t("memory.showKey")} onClick={() => setShowApiKey((current) => !current)} title={showApiKey ? t("memory.hideKey") : t("memory.showKey")} type="button">
                  {showApiKey ? <EyeOff aria-hidden="true" size={14} /> : <Eye aria-hidden="true" size={14} />}
                </button>
                {settings.apiKeyConfigured && <button aria-label={t("memory.clearAuthSecret")} className={clearApiKey ? "is-pending" : undefined} onClick={() => { setClearApiKey((current) => !current); setApiKey(""); markConfigurationChanged(); }} title={t("memory.clearAuthSecret")} type="button"><Trash2 aria-hidden="true" size={14} /></button>}
              </span>
            </label>
            <label className="memory-field">
              <span>{t("memory.serviceId")}</span>
              <input onChange={(event) => updateSettings({ serviceId: event.target.value })} spellCheck={false} value={settings.serviceId} />
            </label>
            <label className="memory-field">
              <span>{t("memory.teamId")}</span>
              <input onChange={(event) => updateSettings({ teamId: event.target.value })} spellCheck={false} value={settings.teamId} />
            </label>
            <label className="memory-field">
              <span>{t("memory.userId")}</span>
              <input onChange={(event) => updateSettings({ userId: event.target.value })} spellCheck={false} value={settings.userId} />
            </label>
            <label className="memory-field memory-field--credential">
              <span><KeyRound aria-hidden="true" size={13} />{t("memory.userKey")}</span>
              <span className="memory-secret-input">
                <input
                  aria-label={t("memory.userKey")}
                  onChange={(event) => {
                    setUserKey(event.target.value);
                    setClearUserKey(false);
                    markConfigurationChanged();
                  }}
                  placeholder={settings.userKeyConfigured && !clearUserKey ? t("memory.credentialStored") : t("memory.credentialOptional")}
                  type={showUserKey ? "text" : "password"}
                  value={userKey}
                />
                <button aria-label={showUserKey ? t("memory.hideKey") : t("memory.showKey")} onClick={() => setShowUserKey((current) => !current)} title={showUserKey ? t("memory.hideKey") : t("memory.showKey")} type="button">
                  {showUserKey ? <EyeOff aria-hidden="true" size={14} /> : <Eye aria-hidden="true" size={14} />}
                </button>
                {settings.userKeyConfigured && <button aria-label={t("memory.clearUserKey")} className={clearUserKey ? "is-pending" : undefined} onClick={() => { setClearUserKey((current) => !current); setUserKey(""); markConfigurationChanged(); }} title={t("memory.clearUserKey")} type="button"><Trash2 aria-hidden="true" size={14} /></button>}
              </span>
            </label>
          </div>
          {isRemoteHttp && (
            <div className="memory-security-warning" role="note">
              <ShieldAlert aria-hidden="true" size={17} />
              <span><strong>{t("memory.insecureHttpTitle")}</strong><small>{t("memory.insecureHttpDescription")}</small></span>
              <Toggle
                checked={settings.allowInsecureHttp}
                label={t("memory.allowInsecureHttp")}
                onChange={(allowInsecureHttp) => updateSettings({ allowInsecureHttp })}
              />
            </div>
          )}
          <div className="memory-runtime-controls">
            <Toggle checked={settings.recallEnabled} label={t("memory.recall")} onChange={(recallEnabled) => updateSettings({ recallEnabled })} />
            <Toggle checked={settings.captureEnabled} label={t("memory.capture")} onChange={(captureEnabled) => updateSettings({ captureEnabled })} />
            <label><span>{t("memory.recallTimeout")}</span><input min={100} max={10000} onChange={(event) => updateSettings({ recallTimeoutMs: Number(event.target.value) })} step={100} type="number" value={settings.recallTimeoutMs} /></label>
            <label><span>{t("memory.requestTimeout")}</span><input min={100} max={30000} onChange={(event) => updateSettings({ requestTimeoutMs: Number(event.target.value) })} step={100} type="number" value={settings.requestTimeoutMs} /></label>
            <label><span>{t("memory.recallItems")}</span><input min={1} max={50} onChange={(event) => updateSettings({ maxRecallItems: Number(event.target.value) })} type="number" value={settings.maxRecallItems} /></label>
            <label><span>{t("memory.recallChars")}</span><input min={256} max={32000} onChange={(event) => updateSettings({ maxRecallChars: Number(event.target.value) })} step={256} type="number" value={settings.maxRecallChars} /></label>
          </div>
        </section>

        <section aria-labelledby="memory-workspaces-title" className="memory-settings__section memory-settings__section--workspaces">
          <div className="settings-section-heading">
            <div><FolderGit2 aria-hidden="true" size={15} /><h3 id="memory-workspaces-title">{t("memory.workspacesTitle")}</h3></div>
            <p>{t("memory.workspacesDescription")}</p>
          </div>
          {bindings.length === 0 ? <p className="memory-workspaces-empty">{t("memory.noWorkspaces")}</p> : (
            <div className="memory-workspace-list">
              {bindings.map((binding) => (
                <div className="memory-workspace-row" data-enabled={binding.enabled} key={binding.rootPath}>
                  <div className="memory-workspace-row__identity">
                    <FolderGit2 aria-hidden="true" size={16} />
                    <span><strong>{workspaceName(binding.rootPath)}</strong><small title={binding.rootPath}>{binding.rootPath}</small></span>
                  </div>
                  <div className="memory-workspace-row__scope">
                    <span>{t("memory.taskId")}</span>
                    <code title={binding.taskId}>{binding.taskId || t("memory.taskIdPending")}</code>
                  </div>
                  <div className="memory-workspace-row__toggles">
                    <Toggle checked={binding.enabled} label={binding.enabled ? t("common.enabled") : t("common.disabled")} onChange={(enabled) => updateBinding(binding.rootPath, { enabled })} />
                    <Toggle checked={binding.recallEnabled} label={t("memory.recall")} onChange={(recallEnabled) => updateBinding(binding.rootPath, { recallEnabled })} />
                    <Toggle checked={binding.captureEnabled} label={t("memory.capture")} onChange={(captureEnabled) => updateBinding(binding.rootPath, { captureEnabled })} />
                  </div>
                  <span className="memory-workspace-row__pending">{t("memory.pending", { count: binding.pendingCaptureCount })}</span>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>

      <footer className="memory-settings__footer">
        <span>
          {saveState === "saved" && <span className="is-saved"><CheckCircle2 aria-hidden="true" size={14} />{t("memory.saved")}</span>}
          {saveState === "error" && <span className="is-error"><AlertCircle aria-hidden="true" size={14} />{t("memory.saveError")}</span>}
          {testState === "success" && <span className="is-saved"><CheckCircle2 aria-hidden="true" size={14} />{t("memory.testSuccess", { latency })}</span>}
          {testState === "error" && (
            <span className="is-error">
              <AlertCircle aria-hidden="true" size={14} />
              {t(testFailureCode ? `memory.testErrors.${testFailureCode}` : "memory.testError")}
            </span>
          )}
        </span>
        <div>
          <button className="button" disabled={saveState === "saving" || testState === "testing"} onClick={() => void testConnection()} type="button"><RefreshCw aria-hidden="true" size={14} />{testState === "testing" ? t("memory.testing") : t("memory.test")}</button>
          <button className="button button--primary" disabled={saveState === "saving" || testState === "testing"} onClick={() => void persist()} type="button"><Save aria-hidden="true" size={14} />{saveState === "saving" ? t("memory.saving") : t("common.save")}</button>
        </div>
      </footer>
    </section>
  );
}
