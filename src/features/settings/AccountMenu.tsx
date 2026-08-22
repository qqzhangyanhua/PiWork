import { Settings, UserRound } from "lucide-react";
import { useEffect, useRef, useState, type RefObject } from "react";
import { useTranslation } from "react-i18next";

type AccountMenuProps = {
  active: boolean;
  onSettingsRequest(): void;
  triggerRef: RefObject<HTMLButtonElement | null>;
};

export function AccountMenu({ active, onSettingsRequest, triggerRef }: AccountMenuProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const settingsRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    queueMicrotask(() => settingsRef.current?.focus());
    const handlePointerDown = (event: globalThis.PointerEvent) => {
      if (!containerRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      setOpen(false);
      queueMicrotask(() => triggerRef.current?.focus());
    };
    document.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [open, triggerRef]);

  const accessibleName = `${t("account.localUser")}, ${t("account.deviceOnly")}`;

  return (
    <div className="account-menu" ref={containerRef}>
      {open && (
        <div aria-label={t("account.menu")} className="account-menu__popover" role="menu">
          <div className="account-menu__identity" aria-hidden="true">
            <span className="account-avatar"><UserRound aria-hidden="true" size={16} /></span>
            <span className="account-identity">
              <strong>{t("account.localUser")}</strong>
              <small>{t("account.deviceOnly")}</small>
            </span>
          </div>
          <div className="account-menu__separator" />
          <button
            aria-label={t("settings.title")}
            className="account-menu__item"
            onClick={() => {
              setOpen(false);
              onSettingsRequest();
            }}
            ref={settingsRef}
            role="menuitem"
            type="button"
          >
            <Settings aria-hidden="true" size={16} />
            <span>{t("settings.title")}</span>
            <kbd>{t("settings.shortcut")}</kbd>
          </button>
        </div>
      )}
      <button
        aria-current={active ? "page" : undefined}
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label={accessibleName}
        className="account-menu__trigger"
        onClick={() => setOpen((current) => !current)}
        ref={triggerRef}
        title={accessibleName}
        type="button"
      >
        <span className="account-avatar" aria-hidden="true"><UserRound size={16} /></span>
        <span className="account-identity">
          <strong>{t("account.localUser")}</strong>
          <small>{t("account.deviceOnly")}</small>
        </span>
      </button>
    </div>
  );
}
