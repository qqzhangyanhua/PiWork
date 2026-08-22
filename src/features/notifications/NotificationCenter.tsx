import { Bell, Check, CheckCheck, CircleAlert, Mail, ShieldAlert, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type {
  AppNotificationSummary,
  PendingConnectorActionSummary,
  PiWorkClient,
} from "../../app/tauriClient";

const previewText = (preview: Record<string, unknown>, field: string) => {
  const value = preview[field];
  return typeof value === "string" ? value : null;
};

export function NotificationCenter({
  client,
  onOpenConnectors,
}: {
  client: PiWorkClient;
  onOpenConnectors(): void;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [notifications, setNotifications] = useState<AppNotificationSummary[]>([]);
  const [approvals, setApprovals] = useState<PendingConnectorActionSummary[]>([]);
  const [toast, setToast] = useState<AppNotificationSummary | null>(null);
  const [resolving, setResolving] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const loadSequenceRef = useRef(0);

  const load = async () => {
    const sequence = ++loadSequenceRef.current;
    const [nextNotifications, nextApprovals] = await Promise.all([
      client.listAppNotifications?.(50) ?? Promise.resolve([]),
      client.listPendingConnectorActions?.(null) ?? Promise.resolve([]),
    ]);
    if (sequence !== loadSequenceRef.current) return;
    setNotifications(nextNotifications);
    setApprovals(nextApprovals);
  };

  useEffect(() => { void load(); }, [client]);
  useEffect(() => {
    if (!client.listenToAppNotifications) return;
    let unlisten: (() => void) | undefined;
    void client.listenToAppNotifications((notification) => {
      setNotifications((current) => [notification, ...current.filter(({ id }) => id !== notification.id)]);
      setToast(notification);
      if (notification.category === "approval") void load();
    }).then((stop) => { unlisten = stop; });
    return () => unlisten?.();
  }, [client]);
  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), 6000);
    return () => window.clearTimeout(timer);
  }, [toast]);
  useEffect(() => {
    if (!open) return;
    void load();
    const close = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);

  const unread = notifications.filter(({ readAt }) => readAt === null).length;
  const approvalMap = useMemo(() => new Map(approvals.map((item) => [item.id, item])), [approvals]);

  const openNotification = async (notification: AppNotificationSummary) => {
    if (!notification.readAt && client.markAppNotificationRead) {
      await client.markAppNotificationRead(notification.id);
      setNotifications((current) => current.map((item) => item.id === notification.id
        ? { ...item, readAt: new Date().toISOString() }
        : item));
    }
    if (notification.connectionId || notification.action.view === "connectors") {
      onOpenConnectors();
      setOpen(false);
    }
  };

  const clear = async (notification: AppNotificationSummary) => {
    await client.clearAppNotification?.(notification.id);
    setNotifications((current) => current.filter(({ id }) => id !== notification.id));
  };

  const resolve = async (notification: AppNotificationSummary, approve: boolean) => {
    const approvalId = typeof notification.action.approvalId === "string"
      ? notification.action.approvalId
      : null;
    if (!approvalId || !client.resolvePendingConnectorAction) return;
    setResolving(approvalId);
    try {
      await client.resolvePendingConnectorAction(approvalId, approve);
      await client.markAppNotificationRead?.(notification.id);
      await load();
    } finally {
      setResolving(null);
    }
  };

  return (
    <div className="notification-center" ref={rootRef}>
      <button
        aria-expanded={open}
        aria-label={t("notifications.title")}
        className="notification-center__trigger"
        onClick={() => setOpen((current) => !current)}
        title={t("notifications.title")}
        type="button"
      >
        <Bell aria-hidden="true" size={16} />
        {unread > 0 && <span>{unread > 99 ? "99+" : unread}</span>}
      </button>

      {toast && !open && (
        <button className="notification-toast" onClick={() => void openNotification(toast)} type="button">
          <span>{toast.category === "mail" ? <Mail aria-hidden="true" size={15} /> : <CircleAlert aria-hidden="true" size={15} />}</span>
          <span><strong>{toast.title}</strong><small>{toast.summary}</small></span>
          <X aria-hidden="true" onClick={(event) => { event.stopPropagation(); setToast(null); }} size={14} />
        </button>
      )}

      {open && (
        <section aria-label={t("notifications.title")} className="notification-popover">
          <header><div><h2>{t("notifications.title")}</h2><span>{t("notifications.unread", { count: unread })}</span></div><button aria-label={t("common.close")} className="icon-button" onClick={() => setOpen(false)} title={t("common.close")} type="button"><X aria-hidden="true" size={15} /></button></header>
          <div className="notification-popover__list">
            {notifications.length === 0 ? (
              <div className="notification-popover__empty"><CheckCheck aria-hidden="true" size={20} /><span>{t("notifications.empty")}</span></div>
            ) : notifications.map((notification) => {
              const approvalId = typeof notification.action.approvalId === "string" ? notification.action.approvalId : null;
              const approval = approvalId ? approvalMap.get(approvalId) : null;
              return (
                <article className="notification-item" data-read={notification.readAt !== null} key={notification.id}>
                  <button className="notification-item__body" onClick={() => void openNotification(notification)} type="button">
                    <span className={`notification-item__icon notification-item__icon--${notification.category}`}>{notification.category === "mail" ? <Mail aria-hidden="true" size={15} /> : notification.category === "approval" ? <ShieldAlert aria-hidden="true" size={15} /> : <CircleAlert aria-hidden="true" size={15} />}</span>
                    <span><strong>{notification.title}</strong><small>{notification.summary}</small><time>{new Date(notification.createdAt).toLocaleString()}</time></span>
                    {!notification.readAt && <i />}
                  </button>
                  {approval && (
                    <div className="notification-approval">
                      <dl>
                        {previewText(approval.preview, "to") && <><dt>{t("notifications.to")}</dt><dd>{previewText(approval.preview, "to")}</dd></>}
                        {previewText(approval.preview, "subject") && <><dt>{t("notifications.subject")}</dt><dd>{previewText(approval.preview, "subject")}</dd></>}
                        {previewText(approval.preview, "senderAddress") && <><dt>{t("notifications.sender")}</dt><dd>{previewText(approval.preview, "senderAddress")}</dd></>}
                        {previewText(approval.preview, "bodyPreview") && <><dt>{t("notifications.preview")}</dt><dd>{previewText(approval.preview, "bodyPreview")}</dd></>}
                      </dl>
                      <div><button className="button" disabled={resolving === approval.id} onClick={() => void resolve(notification, false)} type="button">{t("notifications.deny")}</button><button className="button button--primary" disabled={resolving === approval.id} onClick={() => void resolve(notification, true)} type="button"><Check aria-hidden="true" size={13} />{t("notifications.approve")}</button></div>
                    </div>
                  )}
                  <button aria-label={t("notifications.clear")} className="notification-item__clear" onClick={() => void clear(notification)} title={t("notifications.clear")} type="button"><Trash2 aria-hidden="true" size={13} /></button>
                </article>
              );
            })}
          </div>
        </section>
      )}
    </div>
  );
}
