import { ShieldCheck, UsersRound, Wrench } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import type { PiWorkClient } from "../../app/tauriClient";
import type {
  AgentInstanceSummary,
  CapabilityPackSummary,
  WorkTeamSummary,
} from "../../bindings";
import { appErrorMessageKey, appErrorMessageValues } from "../../domain/appError";
import { normalizeAppError, type AppError } from "../../domain/work";
import { MemberDetailDrawer } from "./MemberDetailDrawer";
import { TeamMemberCard } from "./TeamMemberCard";

export function AgentCenterPage({
  client,
  currentWorkId,
}: {
  client: PiWorkClient;
  currentWorkId?: string;
}) {
  const { t } = useTranslation();
  const [members, setMembers] = useState<AgentInstanceSummary[]>([]);
  const [packs, setPacks] = useState<CapabilityPackSummary[]>([]);
  const [workTeam, setWorkTeam] = useState<WorkTeamSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<AppError | null>(null);
  const [selectedMemberId, setSelectedMemberId] = useState<string | null>(null);
  const [returnFocusTo, setReturnFocusTo] = useState<HTMLButtonElement | null>(null);
  const currentWorkIdRef = useRef(currentWorkId);
  currentWorkIdRef.current = currentWorkId;

  useEffect(() => {
    let current = true;
    setWorkTeam(null);
    setSelectedMemberId(null);
    setLoading(true);
    setLoadError(null);
    void Promise.all([
      client.listAgentInstances(),
      client.listCapabilityPacks(),
      currentWorkId ? client.getWorkTeam(currentWorkId) : Promise.resolve(null),
    ])
      .then(([nextMembers, nextPacks, nextTeam]) => {
        if (!current) return;
        setMembers(nextMembers);
        setPacks(nextPacks.filter(({ status }) => status === "executable"));
        setWorkTeam(nextTeam);
      })
      .catch((error: unknown) => {
        if (current) setLoadError(normalizeAppError(error));
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [client, currentWorkId]);

  const selectedMember = members.find(({ id }) => id === selectedMemberId) ?? null;
  const currentWorkMemberIds = useMemo(
    () => new Set(workTeam?.members.map(({ instance }) => instance.id) ?? []),
    [workTeam],
  );
  const leadId = workTeam?.lead.instance.id
    ?? members.find((member) => member.builtin && member.definition.roleKind === "lead")?.id
    ?? members.find((member) => member.definition.roleKind === "lead")?.id;
  const activeMembers = members.filter(({ status }) => status === "active").length;
  const declaredTools = new Set(
    packs.flatMap(({ requiredTools }) => requiredTools),
  ).size;

  const openMember = (member: AgentInstanceSummary, trigger: HTMLButtonElement) => {
    setReturnFocusTo(trigger);
    setSelectedMemberId(member.id);
  };
  const addMemberToWork = async (member: AgentInstanceSummary): Promise<AppError | null> => {
    if (!currentWorkId) return null;
    const workId = currentWorkId;
    try {
      const mutationTeam = await client.addWorkMember(workId, member.id);
      if (currentWorkIdRef.current !== workId) return null;
      setWorkTeam(mutationTeam);
    } catch (error) {
      if (currentWorkIdRef.current === workId) throw error;
      return null;
    }
    try {
      const refreshedTeam = await client.getWorkTeam(workId);
      if (currentWorkIdRef.current === workId) setWorkTeam(refreshedTeam);
      return null;
    } catch (error) {
      return currentWorkIdRef.current === workId ? normalizeAppError(error) : null;
    }
  };
  const savedMember = (member: AgentInstanceSummary) => {
    setMembers((current) => [
      ...current.filter(({ id }) => id !== member.id),
      member,
    ]);
    setSelectedMemberId(member.id);
  };
  const renderState = (content: ReactNode) => loading ? (
    <div className="agent-center-state" role="status">{t("agentCenter.loading")}</div>
  ) : loadError ? (
    <div className="agent-center-state is-error" role="alert">
      <strong>{t("agentCenter.loadError")}</strong>
      <span>{t(appErrorMessageKey(loadError), appErrorMessageValues(loadError))}</span>
    </div>
  ) : content;

  return (
    <section aria-label={t("agentCenter.title")} className="agent-center agent-center--team-only">
      <div className="agent-center__scroll">
        <div className="agent-center__content">
          <header className="agent-center__header">
            <span className="agent-center__identity">
              <UsersRound aria-hidden="true" size={16} />
              {t("agentCenter.team.title")}
            </span>
            <span>{t("agentCenter.headerKicker")}</span>
          </header>

          <section className="agent-center-hero agent-center-hero--team">
            <div className="agent-center-hero__copy">
              <span className="agent-center-hero__eyebrow">
                <ShieldCheck aria-hidden="true" size={14} />
                {t("agentCenter.eyebrow")}
              </span>
              <h1>{t("agentCenter.team.title")}</h1>
              <p>{t("agentCenter.descriptions.team")}</p>
            </div>
            <div className="agent-center-hero__stats" aria-label={t("agentCenter.stats.teamLabel")}>
              <span className="agent-center-stat">
                <strong className="agent-center-stat__value">{activeMembers}</strong>
                <small>{t("agentCenter.stats.members")}</small>
              </span>
              <span className="agent-center-stat">
                <strong className="agent-center-stat__value">{leadId ? 1 : 0}</strong>
                <small>{t("agentCenter.stats.leads")}</small>
              </span>
              <span className="agent-center-stat">
                <strong className="agent-center-stat__value">{declaredTools}</strong>
                <small>{t("agentCenter.stats.tools")}</small>
              </span>
            </div>
          </section>

          {renderState(
            <section className="agent-center-section agent-team">
              <div className="agent-center-section__heading">
                <div>
                  <span>{t("agentCenter.team.kicker")}</span>
                  <h2>{t("agentCenter.team.membersTitle")}</h2>
                </div>
                <p>
                  {currentWorkId
                    ? t("agentCenter.team.currentWorkHint")
                    : t("agentCenter.team.description")}
                </p>
              </div>
              <div className="team-member-grid">
                {members.map((member) => (
                  <TeamMemberCard
                    isInCurrentWork={currentWorkMemberIds.has(member.id)}
                    isLead={member.id === leadId}
                    key={member.id}
                    member={member}
                    onOpen={(trigger) => openMember(member, trigger)}
                  />
                ))}
              </div>
              <div className="agent-team__runtime-note">
                <Wrench aria-hidden="true" size={14} />
                {t("agentCenter.team.runtimeNote")}
              </div>
            </section>,
          )}
        </div>
      </div>

      {selectedMember && (
        <MemberDetailDrawer
          capabilityPacks={packs}
          client={client}
          currentWorkId={currentWorkId}
          isInCurrentWork={currentWorkMemberIds.has(selectedMember.id)}
          member={selectedMember}
          onAddToWork={addMemberToWork}
          onClose={() => setSelectedMemberId(null)}
          onSaved={savedMember}
          returnFocusTo={returnFocusTo}
        />
      )}
    </section>
  );
}
