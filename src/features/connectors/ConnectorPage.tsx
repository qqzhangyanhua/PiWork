import {
  AtSign,
  Cable,
  Check,
  CircleAlert,
  Inbox,
  KeyRound,
  LoaderCircle,
  Mail,
  Plus,
  RefreshCw,
  Save,
  Server,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import type {
  EmailConnectorSummary,
  EmailMetadataSummary,
  PiWorkClient,
  SaveEmailConnectorInput,
} from "../../app/tauriClient";

type ConnectorDraft = SaveEmailConnectorInput & { password: string };

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

export function ConnectorPage({ client }: { client: PiWorkClient }) {
  const { t } = useTranslation();
  const [connections, setConnections] = useState<EmailConnectorSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<ConnectorDraft>(emptyDraft);
  const [creating, setCreating] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState(false);
  const [busy, setBusy] = useState<"save" | "test" | "enable" | "delete" | null>(null);
  const [message, setMessage] = useState<{ tone: "success" | "error"; text: string } | null>(null);
  const [recentMail, setRecentMail] = useState<EmailMetadataSummary[]>([]);
  const selected = connections.find(({ id }) => id === selectedId) ?? null;

  const load = async () => {
    setLoading(true);
    setLoadError(false);
    try {
      const loaded = await (client.listEmailConnectors?.() ?? Promise.resolve([]));
      setConnections(loaded);
      setSelectedId((current) => current && loaded.some(({ id }) => id === current)
        ? current
        : loaded[0]?.id ?? null);
      if (loaded.length === 0) {
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

  const replaceConnection = (next: EmailConnectorSummary) => {
    setConnections((current) => current.some(({ id }) => id === next.id)
      ? current.map((item) => item.id === next.id ? next : item)
      : [...current, next]);
    setSelectedId(next.id);
    setCreating(false);
    setDraft(toDraft(next));
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
        : {
            tone: "error",
            text: t(TEST_ERROR_KEYS[result.errorCode ?? ""] ?? "connectors.testError"),
          });
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
      setMessage({
        tone: "success",
        text: t(updated.enabled ? "connectors.enabledMessage" : "connectors.disabledMessage"),
      });
    } catch {
      setMessage({ tone: "error", text: t("connectors.enableError") });
    } finally {
      setBusy(null);
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
        <span>{t("connectors.subtitle")}</span>
      </header>

      <div className="connector-page__layout">
        <aside aria-label={t("connectors.accounts")} className="connector-list">
          <div className="connector-list__header">
            <strong>{t("connectors.accounts")}</strong>
            <button
              aria-label={t("connectors.add")}
              className="icon-button"
              onClick={() => { setCreating(true); setSelectedId(null); setDraft(emptyDraft()); setMessage(null); }}
              title={t("connectors.add")}
              type="button"
            ><Plus aria-hidden="true" size={15} /></button>
          </div>
          {loading ? (
            <div className="connector-list__state" role="status"><LoaderCircle aria-hidden="true" className="spin" size={16} />{t("connectors.loading")}</div>
          ) : loadError ? (
            <div className="connector-list__state is-error" role="alert"><CircleAlert aria-hidden="true" size={16} />{t("connectors.loadError")}</div>
          ) : connections.length === 0 ? (
            <div className="connector-list__empty"><Mail aria-hidden="true" size={18} /><span>{t("connectors.empty")}</span></div>
          ) : connections.map((connection) => (
            <button
              aria-current={!creating && selectedId === connection.id ? "page" : undefined}
              className="connector-list__item"
              key={connection.id}
              onClick={() => { setCreating(false); setSelectedId(connection.id); }}
              type="button"
            >
              <span className="connector-list__icon"><Mail aria-hidden="true" size={15} /></span>
              <span><strong>{connection.displayName}</strong><small>{connection.emailAddress}</small></span>
              <i className={`connector-health connector-health--${connectionTone(connection)}`} title={t(`connectors.health.${connectionTone(connection)}`)} />
            </button>
          ))}
        </aside>

        <div className="connector-editor">
          <header className="connector-editor__header">
            <div>
              <span>{creating ? t("connectors.newAccount") : t("connectors.accountSettings")}</span>
              <h2>{creating ? t("connectors.addTitle") : selected?.displayName}</h2>
            </div>
            {!creating && selected && (
              <div className="connector-editor__actions">
                <span className={`connector-status connector-status--${connectionTone(selected)}`}>
                  {selected.enabled ? <Check aria-hidden="true" size={13} /> : <CircleAlert aria-hidden="true" size={13} />}
                  {t(`connectors.health.${connectionTone(selected)}`)}
                </span>
                <button className="button" disabled={busy !== null} onClick={() => void toggleEnabled()} type="button">
                  {busy === "enable" && <LoaderCircle aria-hidden="true" className="spin" size={14} />}
                  {t(selected.enabled ? "common.disable" : "common.enable")}
                </button>
              </div>
            )}
          </header>

          <div className="connector-editor__scroll">
            <section aria-labelledby="connector-identity-heading" className="connector-section">
              <div className="connector-section__heading">
                <AtSign aria-hidden="true" size={15} />
                <div><h3 id="connector-identity-heading">{t("connectors.identity")}</h3><p>{t("connectors.identityDescription")}</p></div>
              </div>
              <div className="connector-field-grid">
                <label><span>{t("connectors.displayName")}</span><input onChange={(event) => setDraft({ ...draft, displayName: event.target.value })} placeholder={t("connectors.displayNamePlaceholder")} value={draft.displayName} /></label>
                <label><span>{t("connectors.emailAddress")}</span><input onChange={(event) => {
                  const emailAddress = event.target.value;
                  setDraft({
                    ...draft,
                    emailAddress,
                    username: !draft.username || draft.username === draft.emailAddress
                      ? emailAddress
                      : draft.username,
                  });
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
                  setDraft(preset === "aliyun-enterprise"
                    ? { ...draft, preset, ...ALIYUN_DEFAULTS }
                    : { ...draft, preset });
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
              <section aria-labelledby="connector-work-heading" className="connector-section connector-work-access">
                <div className="connector-section__heading">
                  <ShieldCheck aria-hidden="true" size={15} />
                  <div><h3 id="connector-work-heading">{t("connectors.workAccess")}</h3><p>{t("connectors.workAccessDescription")}</p></div>
                </div>
                <div className="connector-global-access">
                  <Check aria-hidden="true" size={14} />
                  <span>{t("connectors.allAgentsAccess")}</span>
                </div>
                <div aria-label={t("connectors.permissions")} className="connector-global-permissions">
                  {(["metadata", "read_body", "send"] as const).map((permission) => (
                    <span key={permission}><Check aria-hidden="true" size={12} />{t(`connectors.permission.${permission}`)}</span>
                  ))}
                </div>
              </section>
            )}

            {!creating && selected?.enabled && recentMail.length > 0 && (
              <section aria-labelledby="connector-recent-mail" className="connector-section connector-recent-mail">
                <div className="connector-section__heading"><Inbox aria-hidden="true" size={15} /><div><h3 id="connector-recent-mail">{t("connectors.recentMail")}</h3><p>{t("connectors.metadataOnly")}</p></div></div>
                <div>{recentMail.map((mail) => <div className="connector-mail-row" key={mail.uid}><span><strong>{mail.subject}</strong><small>{mail.senderName || mail.senderAddress}</small></span><time>{mail.receivedAt ? new Date(mail.receivedAt).toLocaleString() : ""}</time></div>)}</div>
              </section>
            )}
          </div>

          <footer className="connector-editor__footer">
            <div aria-live="polite">{message && <span className={`connector-message is-${message.tone}`} role={message.tone === "error" ? "alert" : "status"}>{message.tone === "success" ? <Check aria-hidden="true" size={14} /> : <CircleAlert aria-hidden="true" size={14} />}{message.text}</span>}</div>
            {!creating && selected && <button aria-label={t("connectors.delete")} className="icon-button connector-delete" disabled={busy !== null} onClick={() => void remove()} title={t("connectors.delete")} type="button"><Trash2 aria-hidden="true" size={15} /></button>}
            <button className="button" disabled={busy !== null} onClick={() => void test()} type="button">{busy === "test" ? <LoaderCircle aria-hidden="true" className="spin" size={14} /> : <RefreshCw aria-hidden="true" size={14} />}{t("connectors.test")}</button>
            <button className="button button--primary" disabled={busy !== null} onClick={() => void save()} type="button">{busy === "save" ? <LoaderCircle aria-hidden="true" className="spin" size={14} /> : <Save aria-hidden="true" size={14} />}{t("common.save")}</button>
          </footer>
        </div>
      </div>
    </section>
  );
}
