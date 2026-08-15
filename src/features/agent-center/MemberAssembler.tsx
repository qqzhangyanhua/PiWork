import { AlertTriangle, Check, Copy, LockKeyhole, Save } from "lucide-react";
import { useEffect, useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import type { PiWorkClient } from "../../app/tauriClient";
import type {
  AgentInstanceSummary,
  AssemblyDiagnostic,
  CapabilityPackSummary,
  PermissionPolicy,
  SaveAgentAssemblyInput,
} from "../../bindings";
import { appErrorMessageKey, appErrorMessageValues } from "../../domain/appError";
import { normalizeAppError, type AppError } from "../../domain/work";

const PERMISSION_OPTIONS: PermissionPolicy[] = ["read_only", "inherit_work", "work_write"];

const diagnosticRepairKey = (code: AssemblyDiagnostic["code"]) =>
  `agentCenter.assembler.diagnostics.${code}` as const;

export function MemberAssembler({
  capabilityPacks,
  client,
  onSaved,
  requestedCapabilityPackId,
  source,
}: {
  capabilityPacks: readonly CapabilityPackSummary[];
  client: PiWorkClient;
  onSaved(instance: AgentInstanceSummary): void;
  requestedCapabilityPackId?: string;
  source: AgentInstanceSummary;
}) {
  const { t } = useTranslation();
  const validationId = useId();
  const [isEditing, setIsEditing] = useState(!source.builtin);
  const [displayName, setDisplayName] = useState(source.displayName);
  const [engineOverride, setEngineOverride] = useState(source.engineOverride ?? "");
  const [modelOverride, setModelOverride] = useState(source.modelConfigurationOverride ?? "");
  const [permissionOverride, setPermissionOverride] = useState<PermissionPolicy | "">(
    source.permissionPolicyOverride ?? "",
  );
  const [parallelismOverride, setParallelismOverride] = useState(
    source.parallelismOverride?.toString() ?? "",
  );
  const [selectedPackIds, setSelectedPackIds] = useState<Set<string>>(
    () => {
      const next = new Set(source.definition.capabilityPacks.map(({ id }) => id));
      if (
        requestedCapabilityPackId
        && capabilityPacks.some(({ id, status }) =>
          id === requestedCapabilityPackId && status === "executable"
        )
      ) {
        next.add(requestedCapabilityPackId);
      }
      return next;
    },
  );
  const [diagnostics, setDiagnostics] = useState<AssemblyDiagnostic[]>([]);
  const [validationPending, setValidationPending] = useState(false);
  const [validationError, setValidationError] = useState<AppError | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<AppError | null>(null);

  useEffect(() => {
    setIsEditing(!source.builtin);
    setDisplayName(source.displayName);
    setEngineOverride(source.engineOverride ?? "");
    setModelOverride(source.modelConfigurationOverride ?? "");
    setPermissionOverride(source.permissionPolicyOverride ?? "");
    setParallelismOverride(source.parallelismOverride?.toString() ?? "");
    const nextPackIds = new Set(source.definition.capabilityPacks.map(({ id }) => id));
    if (
      requestedCapabilityPackId
      && capabilityPacks.some(({ id, status }) =>
        id === requestedCapabilityPackId && status === "executable"
      )
    ) {
      nextPackIds.add(requestedCapabilityPackId);
    }
    setSelectedPackIds(nextPackIds);
    setDiagnostics([]);
    setValidationError(null);
    setSaveError(null);
  }, [capabilityPacks, requestedCapabilityPackId, source]);

  const fixedSystemPackIds = useMemo(() => new Set(
    source.definition.capabilityPacks
      .filter(({ catalogCapabilityId }) => catalogCapabilityId === null)
      .map(({ id }) => id),
  ), [source]);
  const executableBusinessPacks = useMemo(() => capabilityPacks.filter(
    (pack) => pack.catalogCapabilityId !== null && pack.status === "executable",
  ), [capabilityPacks]);
  const catalogOnlyCount = useMemo(() => capabilityPacks.filter(
    ({ status }) => status === "catalog_only",
  ).length, [capabilityPacks]);
  const inheritedSystemPacks = useMemo(() => capabilityPacks.filter(
    ({ id }) => fixedSystemPackIds.has(id),
  ), [capabilityPacks, fixedSystemPackIds]);

  const input = useMemo<SaveAgentAssemblyInput>(() => ({
    sourceInstanceId: source.id,
    displayName,
    capabilityPackIds: [...selectedPackIds].sort(),
    engineOverride: engineOverride.trim() || null,
    modelConfigurationOverride: modelOverride.trim() || null,
    permissionPolicyOverride: permissionOverride || null,
    parallelismOverride: parallelismOverride === ""
      ? null
      : Number(parallelismOverride),
  }), [
    displayName,
    engineOverride,
    modelOverride,
    parallelismOverride,
    permissionOverride,
    selectedPackIds,
    source.id,
  ]);

  useEffect(() => {
    if (!isEditing || displayName.trim() === "") {
      setValidationPending(false);
      setDiagnostics([]);
      setValidationError(null);
      return;
    }
    let current = true;
    setValidationPending(true);
    setValidationError(null);
    void client.validateAgentAssembly(input)
      .then((nextDiagnostics) => {
        if (!current) return;
        setDiagnostics(nextDiagnostics);
      })
      .catch((error: unknown) => {
        if (!current) return;
        setDiagnostics([]);
        setValidationError(normalizeAppError(error));
      })
      .finally(() => {
        if (current) setValidationPending(false);
      });
    return () => {
      current = false;
    };
  }, [client, displayName, input, isEditing]);

  const beginCopy = () => {
    setDisplayName(`${source.displayName}${t("agentCenter.assembler.copySuffix")}`);
    setIsEditing(true);
  };
  const togglePack = (packId: string) => {
    setSelectedPackIds((current) => {
      const next = new Set(current);
      if (next.has(packId)) next.delete(packId);
      else next.add(packId);
      return next;
    });
  };
  const save = async () => {
    setSaving(true);
    setSaveError(null);
    try {
      const saved = await client.saveAgentCopy(input);
      onSaved(saved);
    } catch (error) {
      setSaveError(normalizeAppError(error));
    } finally {
      setSaving(false);
    }
  };
  const saveDisabled = validationPending
    || saving
    || diagnostics.length > 0
    || validationError !== null
    || displayName.trim() === "";

  return (
    <section className="member-assembler" aria-label={t("agentCenter.assembler.title")}>
      <div className="member-assembler__heading">
        <div>
          <span>{t("agentCenter.assembler.kicker")}</span>
          <h3>{t("agentCenter.assembler.title")}</h3>
        </div>
        <span className="member-assembler__source">
          <LockKeyhole aria-hidden="true" size={12} />
          {source.builtin
            ? t("agentCenter.assembler.builtinSource")
            : t("agentCenter.assembler.localSource")}
        </span>
      </div>

      {source.builtin && !isEditing && (
        <div className="member-assembler__readonly-note">
          <p>{t("agentCenter.assembler.readonlyNote")}</p>
          <button className="member-assembler__copy" onClick={beginCopy} type="button">
            <Copy aria-hidden="true" size={14} />{t("agentCenter.assembler.customize")}
          </button>
        </div>
      )}

      <div className="member-assembler__fields">
        <label>
          <span>{t("agentCenter.assembler.displayName")}</span>
          <input
            aria-describedby={isEditing ? validationId : undefined}
            aria-label={t("agentCenter.assembler.displayName")}
            disabled={!isEditing}
            onChange={(event) => setDisplayName(event.target.value)}
            value={displayName}
          />
        </label>
        <label>
          <span>{t("agentCenter.assembler.engine")}</span>
          <input
            aria-describedby={isEditing ? validationId : undefined}
            aria-label={t("agentCenter.assembler.engine")}
            disabled={!isEditing}
            onChange={(event) => setEngineOverride(event.target.value)}
            placeholder={source.definition.defaultEngineKind}
            value={engineOverride}
          />
        </label>
        <label>
          <span>{t("agentCenter.assembler.model")}</span>
          <input
            aria-describedby={isEditing ? validationId : undefined}
            aria-label={t("agentCenter.assembler.model")}
            disabled={!isEditing}
            onChange={(event) => setModelOverride(event.target.value)}
            placeholder={source.definition.defaultModelConfigurationId ?? t("agentCenter.member.modelDefault")}
            value={modelOverride}
          />
        </label>
        <label>
          <span>{t("agentCenter.assembler.permission")}</span>
          <select
            aria-describedby={isEditing ? validationId : undefined}
            aria-label={t("agentCenter.assembler.permission")}
            disabled={!isEditing}
            onChange={(event) => setPermissionOverride(event.target.value as PermissionPolicy | "")}
            value={permissionOverride}
          >
            <option value="">{t("agentCenter.assembler.inheritDefinition")}</option>
            {PERMISSION_OPTIONS.map((permission) => (
              <option key={permission} value={permission}>
                {t(`agentCenter.member.permissions.${permission}`)}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>{t("agentCenter.assembler.parallelism")}</span>
          <input
            aria-describedby={isEditing ? validationId : undefined}
            aria-label={t("agentCenter.assembler.parallelism")}
            disabled={!isEditing}
            inputMode="numeric"
            max={8}
            min={1}
            onChange={(event) => setParallelismOverride(event.target.value)}
            placeholder={source.definition.defaultParallelism.toString()}
            type="number"
            value={parallelismOverride}
          />
        </label>
      </div>

      <section className="member-assembler__packs">
        <div className="member-assembler__section-title">
          <h4>{t("agentCenter.assembler.systemPacks")}</h4>
          <span>{t("agentCenter.assembler.fixedInherited")}</span>
        </div>
        <div className="member-assembler__pack-list">
          {inheritedSystemPacks.map((pack) => (
            <label className="member-pack-option is-fixed" key={pack.id}>
              <input checked disabled readOnly type="checkbox" />
              <span><strong>{pack.name}</strong><small>{pack.description}</small></span>
              <LockKeyhole aria-hidden="true" size={13} />
            </label>
          ))}
        </div>
      </section>

      <section className="member-assembler__packs">
        <div className="member-assembler__section-title">
          <h4>{t("agentCenter.assembler.businessPacks")}</h4>
          <span>{t("agentCenter.assembler.executableCount", { count: executableBusinessPacks.length })}</span>
        </div>
        {executableBusinessPacks.length > 0 && (
          <div className="member-assembler__pack-list">
            {executableBusinessPacks.map((pack) => (
              <label className="member-pack-option" key={pack.id}>
                <input
                  aria-describedby={isEditing ? validationId : undefined}
                  checked={selectedPackIds.has(pack.id)}
                  disabled={!isEditing}
                  onChange={() => togglePack(pack.id)}
                  type="checkbox"
                />
                <span><strong>{pack.name}</strong><small>{pack.description}</small></span>
              </label>
            ))}
          </div>
        )}
        <div className="member-assembler__catalog-note">
          <AlertTriangle aria-hidden="true" size={14} />
          <span>
            <strong>{t("agentCenter.assembler.catalogOnlyCount", { count: catalogOnlyCount })}</strong>
            <small>{t("agentCenter.assembler.catalogOnlyReason")}</small>
          </span>
        </div>
      </section>

      {isEditing && (
        <section
          aria-busy={validationPending}
          aria-label={t("agentCenter.assembler.validation")}
          aria-live="polite"
          className="member-assembler__validation"
          id={validationId}
        >
          {displayName.trim() === "" ? (
            <p>{t("agentCenter.assembler.displayNameRequired")}</p>
          ) : validationPending ? (
            <p>{t("agentCenter.assembler.validating")}</p>
          ) : !validationError && diagnostics.length === 0 && (
            <p className="is-valid"><Check aria-hidden="true" size={13} />{t("agentCenter.assembler.valid")}</p>
          )}
          {displayName.trim() !== "" && validationError && (
            <p role="alert">
              {t("agentCenter.assembler.validationFailed")}: {t(
                appErrorMessageKey(validationError),
                appErrorMessageValues(validationError),
              )}
            </p>
          )}
          {diagnostics.length > 0 && (
            <ul className="member-assembler__diagnostics">
              {diagnostics.map((diagnostic, index) => (
                <li key={`${diagnostic.code}:${diagnostic.capabilityPackId ?? "assembly"}:${index}`}>
                  <code>{diagnostic.code}</code>
                  <p>{diagnostic.message}</p>
                  <span><strong>{t("agentCenter.assembler.repair")}</strong>{t(diagnosticRepairKey(diagnostic.code))}</span>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      {saveError && (
        <p className="member-assembler__save-error" role="alert">
          {t(appErrorMessageKey(saveError), appErrorMessageValues(saveError))}
        </p>
      )}
      {isEditing && (
        <button
          className="member-assembler__save"
          aria-describedby={validationId}
          disabled={saveDisabled}
          onClick={() => void save()}
          type="button"
        >
          <Save aria-hidden="true" size={14} />
          {saving ? t("agentCenter.assembler.saving") : t("agentCenter.assembler.save")}
        </button>
      )}
    </section>
  );
}
