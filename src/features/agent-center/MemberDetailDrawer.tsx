import {
  Activity,
  Bot,
  Box,
  BrainCircuit,
  CheckCircle2,
  Cpu,
  Database,
  KeyRound,
  Plus,
  Shield,
  X,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { PiWorkClient } from "../../app/tauriClient";
import type { AgentInstanceSummary, CapabilityPackSummary } from "../../bindings";
import { appErrorMessageKey, appErrorMessageValues } from "../../domain/appError";
import { normalizeAppError, type AppError } from "../../domain/work";
import { MemberAssembler } from "./MemberAssembler";

const FOCUSABLE = "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])";

export function MemberDetailDrawer({
  capabilityPacks,
  client,
  currentWorkId,
  isInCurrentWork,
  member,
  onAddToWork,
  onClose,
  onSaved,
  requestedCapabilityPackId,
  returnFocusTo,
}: {
  capabilityPacks: readonly CapabilityPackSummary[];
  client: PiWorkClient;
  currentWorkId?: string;
  isInCurrentWork: boolean;
  member: AgentInstanceSummary;
  onAddToWork(member: AgentInstanceSummary): Promise<AppError | null>;
  onClose(): void;
  onSaved(member: AgentInstanceSummary): void;
  requestedCapabilityPackId?: string;
  returnFocusTo: HTMLButtonElement | null;
}) {
  const { t } = useTranslation();
  const drawerRef = useRef<HTMLElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const onCloseRef = useRef(onClose);
  const addContext = `${currentWorkId ?? ""}:${member.id}`;
  const activeAddContextRef = useRef(addContext);
  const addGenerationRef = useRef(0);
  const [adding, setAdding] = useState(false);
  const [addError, setAddError] = useState<AppError | null>(null);
  const [addWarning, setAddWarning] = useState<AppError | null>(null);
  onCloseRef.current = onClose;
  activeAddContextRef.current = addContext;

  useEffect(() => {
    addGenerationRef.current += 1;
    setAdding(false);
    setAddError(null);
    setAddWarning(null);
  }, [addContext]);

  useEffect(() => {
    closeRef.current?.focus();
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab" || !drawerRef.current) return;
      const focusable = Array.from(drawerRef.current.querySelectorAll<HTMLElement>(FOCUSABLE));
      const first = focusable[0];
      const last = focusable.at(-1);
      if (!first || !last) return;
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      queueMicrotask(() => returnFocusTo?.focus());
    };
  }, [returnFocusTo]);

  const definition = member.definition;
  const loadedPacks = definition.capabilityPacks;
  const tools = [...new Set(loadedPacks.flatMap(({ requiredTools }) => requiredTools))];
  const engine = member.engineOverride ?? definition.defaultEngineKind;
  const model = member.modelConfigurationOverride
    ?? definition.defaultModelConfigurationId
    ?? t("agentCenter.member.modelDefault");
  const permission = member.permissionPolicyOverride ?? definition.defaultPermissionPolicy;

  const addToWork = async () => {
    const context = addContext;
    const generation = addGenerationRef.current + 1;
    addGenerationRef.current = generation;
    setAdding(true);
    setAddError(null);
    setAddWarning(null);
    try {
      const warning = await onAddToWork(member);
      if (
        activeAddContextRef.current !== context
        || addGenerationRef.current !== generation
      ) return;
      setAddWarning(warning);
    } catch (error) {
      if (
        activeAddContextRef.current !== context
        || addGenerationRef.current !== generation
      ) return;
      setAddError(normalizeAppError(error));
    } finally {
      if (
        activeAddContextRef.current === context
        && addGenerationRef.current === generation
      ) setAdding(false);
    }
  };

  return (
    <div className="capability-drawer-layer member-drawer-layer">
      <button
        aria-hidden="true"
        className="capability-drawer-scrim"
        onClick={onClose}
        tabIndex={-1}
        type="button"
      />
      <aside
        aria-label={member.displayName}
        aria-modal="true"
        className="capability-drawer member-drawer"
        ref={drawerRef}
        role="dialog"
      >
        <header className="capability-drawer__header member-drawer__header">
          <div>
            <span className="capability-drawer__eyebrow">
              {t(`agentCenter.member.roles.${definition.roleKind}`)} · {member.builtin
                ? t("agentCenter.member.builtin")
                : t("agentCenter.member.local")}
            </span>
            <h2>{member.displayName}</h2>
            <p>{definition.description}</p>
          </div>
          <button
            aria-label={t("agentCenter.memberDrawer.close")}
            className="capability-drawer__close"
            onClick={onClose}
            ref={closeRef}
            type="button"
          >
            <X aria-hidden="true" size={18} />
          </button>
        </header>

        <div className="capability-drawer__meta member-drawer__meta">
          <span className={`team-member-card__status team-member-card__status--${member.status}`}>
            <CheckCircle2 aria-hidden="true" size={11} />
            {t(`agentCenter.member.status.${member.status}`)}
          </span>
          <span>{t("agentCenter.member.version", { version: definition.version })}</span>
          <span>{t("agentCenter.member.identityStable")}</span>
        </div>

        <div className="capability-drawer__content member-drawer__content">
          <section className="member-drawer__split">
            <div>
              <h3><Activity aria-hidden="true" size={13} />{t("agentCenter.memberDrawer.responsibilities")}</h3>
              <ul>{definition.responsibilities.map((item) => <li key={item}>{item}</li>)}</ul>
            </div>
            <div>
              <h3><Shield aria-hidden="true" size={13} />{t("agentCenter.memberDrawer.boundaries")}</h3>
              <ul>{definition.nonResponsibilities.map((item) => <li key={item}>{item}</li>)}</ul>
            </div>
          </section>

          <section>
            <h3><BrainCircuit aria-hidden="true" size={13} />{t("agentCenter.memberDrawer.loadedPacks")}</h3>
            <div className="member-drawer__pack-list">
              {loadedPacks.map((pack) => (
                <span key={pack.id}><Box aria-hidden="true" size={13} /><span><strong>{pack.name}</strong><small>{pack.description}</small></span></span>
              ))}
            </div>
          </section>

          <section className="member-drawer__facts">
            <div><Cpu aria-hidden="true" size={14} /><span><small>{t("agentCenter.memberDrawer.engineModel")}</small><strong>{engine} · {model}</strong></span></div>
            <div><KeyRound aria-hidden="true" size={14} /><span><small>{t("agentCenter.member.permission")}</small><strong>{t(`agentCenter.member.permissions.${permission}`)}</strong></span></div>
            <div><Database aria-hidden="true" size={14} /><span><small>{t("agentCenter.memberDrawer.memory")}</small><strong>{t(`agentCenter.memberDrawer.memoryPolicies.${definition.memoryPolicy}`)}</strong></span></div>
            <div><Bot aria-hidden="true" size={14} /><span><small>{t("agentCenter.memberDrawer.tools")}</small><strong>{tools.length > 0 ? tools.join(" · ") : t("agentCenter.memberDrawer.noTools")}</strong></span></div>
          </section>

          <section>
            <h3>{t("agentCenter.memberDrawer.participatingWorks")}</h3>
            {isInCurrentWork && currentWorkId ? (
              <p className="member-drawer__work"><CheckCircle2 aria-hidden="true" size={13} />{currentWorkId}</p>
            ) : (
              <p className="member-drawer__empty">{t("agentCenter.memberDrawer.noParticipatingWorks")}</p>
            )}
          </section>

          <MemberAssembler
            capabilityPacks={capabilityPacks}
            client={client}
            onSaved={onSaved}
            requestedCapabilityPackId={requestedCapabilityPackId}
            source={member}
          />
        </div>

        <footer className="capability-drawer__footer member-drawer__footer">
          {addError && (
            <p role="alert">
              {t(appErrorMessageKey(addError), appErrorMessageValues(addError))}
            </p>
          )}
          {addWarning && (
            <p role="status">
              {t("agentCenter.memberDrawer.refreshWarning")}
            </p>
          )}
          {currentWorkId ? (
            <button
              className="member-drawer__add"
              disabled={isInCurrentWork || adding}
              onClick={() => void addToWork()}
              type="button"
            >
              {isInCurrentWork ? <CheckCircle2 aria-hidden="true" size={15} /> : <Plus aria-hidden="true" size={15} />}
              {isInCurrentWork
                ? t("agentCenter.member.inCurrentWork")
                : adding
                  ? t("agentCenter.memberDrawer.adding")
                  : t("agentCenter.memberDrawer.addToWork")}
            </button>
          ) : (
            <p className="member-drawer__no-work">{t("agentCenter.memberDrawer.noCurrentWork")}</p>
          )}
        </footer>
      </aside>
    </div>
  );
}
