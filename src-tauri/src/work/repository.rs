use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqliteConnection, SqlitePool};
use uuid::Uuid;

use crate::{
    domain::{
        assignment::{
            AssignmentKind, AssignmentSideEffect, AssignmentStatus, AssignmentSummary,
            UserMessageSummary,
        },
        event::{WorkEventEnvelope, WorkEventPayload},
        work::{
            CreateWorkInput, MessageRole, MessageSummary, PermissionMode, RunStatus, RunSummary,
            StartWorkOutput, WorkDetail, WorkStatus, WorkSummary,
        },
    },
    error::AppError,
};

use super::state_machine::{RunAction, WorkAction, transition, transition_run};
use crate::agent::repository::DEFAULT_LEAD_INSTANCE_ID;

#[derive(Clone)]
pub struct WorkRepository {
    pool: SqlitePool,
}

impl WorkRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, input: CreateWorkInput) -> Result<WorkDetail, AppError> {
        let resource_draft_id = input.resource_draft_id.clone();
        let title = input.title.trim();
        if title.is_empty() {
            return Err(AppError::invalid_input("title", "title must not be empty"));
        }

        let goal = input.goal.trim();
        if goal.is_empty() {
            return Err(AppError::invalid_input("goal", "goal must not be empty"));
        }

        let canonical_path = dunce::canonicalize(&input.root_path).map_err(|source| {
            AppError::WorkspacePathResolution {
                path: input.root_path.clone().into(),
                source,
            }
        })?;
        let metadata = std::fs::metadata(&canonical_path).map_err(|source| {
            AppError::WorkspacePathResolution {
                path: input.root_path.clone().into(),
                source,
            }
        })?;
        if !metadata.is_dir() {
            return Err(AppError::invalid_input(
                "rootPath",
                "rootPath must be a directory",
            ));
        }
        let now = Utc::now();
        let summary = WorkSummary {
            id: Uuid::new_v4().to_string(),
            title: title.to_owned(),
            goal: goal.to_owned(),
            root_path: canonical_path.to_string_lossy().into_owned(),
            permission_mode: input.permission_mode,
            status: WorkStatus::Draft,
            created_at: now,
            updated_at: now,
        };

        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query(
            "INSERT INTO works \
             (id, title, goal, root_path, permission_mode, status, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&summary.id)
        .bind(&summary.title)
        .bind(&summary.goal)
        .bind(&summary.root_path)
        .bind(summary.permission_mode)
        .bind(summary.status)
        .bind(summary.created_at)
        .bind(summary.updated_at)
        .execute(&mut *transaction)
        .await?;

        if let Some(draft_id) = resource_draft_id {
            sqlx::query(
                "UPDATE resource_links SET work_id = ?, draft_id = NULL \
                 WHERE draft_id = ? AND work_id IS NULL",
            )
            .bind(&summary.id)
            .bind(draft_id)
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query(
            "INSERT INTO work_agents \
             (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) \
             VALUES (?, ?, 'lead', 'joined', 'inherit_work', ?, ?)",
        )
        .bind(&summary.id)
        .bind(DEFAULT_LEAD_INSTANCE_ID)
        .bind(summary.created_at)
        .bind(summary.created_at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES (?, ?, ?)",
        )
        .bind(&summary.id)
        .bind(DEFAULT_LEAD_INSTANCE_ID)
        .bind(summary.created_at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;

        Ok(WorkDetail {
            summary,
            runs: Vec::new(),
            messages: Vec::new(),
            events: Vec::new(),
        })
    }

    pub async fn get(&self, id: &str) -> Result<Option<WorkDetail>, AppError> {
        let mut transaction = self.pool.begin().await?;
        let row = Self::load_work(&mut transaction, id).await?;
        let detail = match row {
            Some(row) => Some(Self::load_detail(&mut transaction, id, row).await?),
            None => None,
        };
        transaction.commit().await?;
        Ok(detail)
    }

    #[cfg(test)]
    async fn get_after_work_loaded<F>(
        &self,
        id: &str,
        after_work_loaded: F,
    ) -> Result<Option<WorkDetail>, AppError>
    where
        F: FnOnce(),
    {
        let mut transaction = self.pool.begin().await?;
        let row = Self::load_work(&mut transaction, id).await?;
        let detail = match row {
            Some(row) => {
                after_work_loaded();
                Some(Self::load_detail(&mut transaction, id, row).await?)
            }
            None => None,
        };
        transaction.commit().await?;
        Ok(detail)
    }

    async fn load_work(
        connection: &mut SqliteConnection,
        id: &str,
    ) -> Result<Option<WorkRow>, AppError> {
        Ok(sqlx::query_as::<_, WorkRow>(
            "SELECT id, title, goal, root_path, permission_mode, status, created_at, updated_at \
             FROM works WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&mut *connection)
        .await?)
    }

    async fn insert_event(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        event_id: &str,
        envelope: &WorkEventEnvelope,
        sequence: i64,
    ) -> Result<(), AppError> {
        let payload = serde_json::to_string(&envelope.payload)
            .map_err(|error| AppError::Database(sqlx::Error::Encode(Box::new(error))))?;
        sqlx::query(
            "INSERT INTO events \
             (id, work_id, run_id, turn_id, session_id, agent_id, assignment_id, causation_id, \
              correlation_id, sequence, version, occurred_at, payload) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(event_id)
        .bind(&envelope.work_id)
        .bind(&envelope.run_id)
        .bind(&envelope.turn_id)
        .bind(&envelope.session_id)
        .bind(&envelope.agent_id)
        .bind(&envelope.assignment_id)
        .bind(&envelope.causation_id)
        .bind(&envelope.correlation_id)
        .bind(sequence)
        .bind(i64::from(envelope.version))
        .bind(envelope.occurred_at)
        .bind(payload)
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }

    async fn load_detail(
        connection: &mut SqliteConnection,
        id: &str,
        row: WorkRow,
    ) -> Result<WorkDetail, AppError> {
        let runs = sqlx::query_as::<_, RunRow>(
            "SELECT id, work_id, engine_kind, engine_session_id, model_label, status, \
                    created_at, started_at, completed_at \
             FROM runs WHERE work_id = ? ORDER BY created_at ASC, id ASC",
        )
        .bind(id)
        .fetch_all(&mut *connection)
        .await?
        .into_iter()
        .map(RunSummary::from)
        .collect();
        let events = sqlx::query_as::<_, EventRow>(
            "SELECT events.id, events.work_id, events.run_id, events.turn_id, \
                    events.session_id, events.agent_id, events.assignment_id, \
                    events.causation_id, events.correlation_id, events.sequence, \
                    events.version, events.occurred_at, events.payload \
             FROM events \
             INNER JOIN runs ON runs.id = events.run_id AND runs.work_id = events.work_id \
             WHERE events.work_id = ? \
             ORDER BY runs.created_at ASC, runs.id ASC, events.sequence ASC, events.id ASC",
        )
        .bind(id)
        .fetch_all(&mut *connection)
        .await?
        .into_iter()
        .map(WorkEventEnvelope::try_from)
        .collect::<Result<Vec<_>, _>>()?;
        let mut messages = sqlx::query_as::<_, MessageRow>(
            "SELECT messages.id, messages.work_id, messages.run_id, messages.role, \
                    messages.content, messages.created_at \
             FROM messages \
             INNER JOIN runs \
                ON runs.id = messages.run_id AND runs.work_id = messages.work_id \
             WHERE messages.work_id = ? \
             ORDER BY messages.created_at ASC, messages.id ASC",
        )
        .bind(id)
        .fetch_all(&mut *connection)
        .await?
        .into_iter()
        .map(MessageSummary::from)
        .collect::<Vec<_>>();
        let mut resources_by_message = HashMap::<String, Vec<String>>::new();
        for (message_id, resource_id) in sqlx::query_as::<_, (String, String)>(
            "SELECT resource_links.message_id, resource_links.resource_id \
             FROM resource_links \
             INNER JOIN messages \
                ON messages.id = resource_links.message_id \
               AND messages.work_id = resource_links.work_id \
             WHERE resource_links.work_id = ? AND resource_links.message_id IS NOT NULL \
             ORDER BY resource_links.created_at ASC, resource_links.id ASC",
        )
        .bind(id)
        .fetch_all(&mut *connection)
        .await?
        {
            resources_by_message
                .entry(message_id)
                .or_default()
                .push(resource_id);
        }
        for message in &mut messages {
            message.resource_ids = resources_by_message.remove(&message.id).unwrap_or_default();
        }

        Ok(WorkDetail {
            summary: row.into(),
            runs,
            messages,
            events,
        })
    }

    pub async fn list(&self) -> Result<Vec<WorkSummary>, AppError> {
        let rows = sqlx::query_as::<_, WorkRow>(
            "SELECT id, title, goal, root_path, permission_mode, status, created_at, updated_at \
             FROM works ORDER BY updated_at DESC, created_at DESC, id ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(WorkSummary::from).collect())
    }

    pub async fn recover_interrupted_runs(&self) -> Result<u64, AppError> {
        let now = Utc::now();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query(
            "UPDATE works SET status = ?, updated_at = ? \
             WHERE id IN (\
                 SELECT DISTINCT work_id FROM runs \
                 WHERE status IN (?, ?, ?)\
             )",
        )
        .bind(WorkStatus::Interrupted)
        .bind(now)
        .bind(RunStatus::Running)
        .bind(RunStatus::Waiting)
        .bind(RunStatus::Queued)
        .execute(&mut *transaction)
        .await?;
        let recovered = sqlx::query(
            "UPDATE runs \
             SET status = ?, updated_at = ?, completed_at = COALESCE(completed_at, ?) \
             WHERE status IN (?, ?, ?)",
        )
        .bind(RunStatus::Interrupted)
        .bind(now)
        .bind(now)
        .bind(RunStatus::Running)
        .bind(RunStatus::Waiting)
        .bind(RunStatus::Queued)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        transaction.commit().await?;

        Ok(recovered)
    }

    pub async fn insert_run(
        &self,
        work_id: &str,
        model_label: &str,
    ) -> Result<RunSummary, AppError> {
        let now = Utc::now();
        let run = RunSummary {
            id: Uuid::new_v4().to_string(),
            work_id: work_id.to_owned(),
            assignment_id: None,
            agent_instance_id: None,
            engine_kind: "manual".into(),
            engine_session_id: None,
            model_label: model_label.to_owned(),
            status: RunStatus::Queued,
            created_at: now,
            started_at: None,
            completed_at: None,
        };
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let updated = sqlx::query("UPDATE works SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(work_id)
            .execute(&mut *transaction)
            .await?;
        if updated.rows_affected() == 0 {
            return Err(AppError::work_not_found(work_id));
        }

        sqlx::query(
            "INSERT INTO runs \
             (id, work_id, engine_kind, engine_session_id, model_label, status, created_at, \
              updated_at, started_at, completed_at) \
             VALUES (?, ?, ?, NULL, ?, ?, ?, ?, NULL, NULL)",
        )
        .bind(&run.id)
        .bind(&run.work_id)
        .bind(&run.engine_kind)
        .bind(&run.model_label)
        .bind(run.status)
        .bind(run.created_at)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;

        Ok(run)
    }

    pub async fn begin_run(
        &self,
        work_id: &str,
        prompt: &str,
        resource_ids: &[String],
        engine_kind: &str,
        model_label: &str,
    ) -> Result<StartWorkOutput, AppError> {
        let prompt = prompt.trim();
        let mut seen = HashSet::new();
        let resource_ids = resource_ids
            .iter()
            .filter(|resource_id| seen.insert((*resource_id).clone()))
            .cloned()
            .collect::<Vec<_>>();
        if prompt.is_empty() && resource_ids.is_empty() {
            return Err(AppError::invalid_input(
                "prompt",
                "prompt or attachment must not be empty",
            ));
        }

        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let work = Self::load_work(&mut transaction, work_id)
            .await?
            .ok_or_else(|| AppError::work_not_found(work_id))?;
        if matches!(
            work.status,
            WorkStatus::Queued | WorkStatus::Running | WorkStatus::Waiting
        ) {
            return Err(AppError::work_already_running(work_id));
        }
        if !resource_ids.is_empty() {
            let resource_ids_json = serde_json::to_string(&resource_ids)
                .map_err(|error| AppError::Database(sqlx::Error::Encode(Box::new(error))))?;
            let ready_count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(DISTINCT managed_resources.id) \
                 FROM managed_resources \
                 INNER JOIN resource_links \
                    ON resource_links.resource_id = managed_resources.id \
                 WHERE managed_resources.id IN (SELECT value FROM json_each(?)) \
                   AND managed_resources.status = 'ready' \
                   AND resource_links.work_id = ? \
                   AND resource_links.message_id IS NULL \
                   AND resource_links.role IN ('attached', 'pinned')",
            )
            .bind(resource_ids_json)
            .bind(work_id)
            .fetch_one(&mut *transaction)
            .await?;
            if usize::try_from(ready_count).ok() != Some(resource_ids.len()) {
                return Err(AppError::resource_import("resource_not_ready_or_unlinked"));
            }
        }
        let queued = transition(work.status, WorkAction::Queue)
            .map_err(|_| AppError::invalid_work_state(work_id, work.status, WorkStatus::Queued))?;
        let running = transition(queued, WorkAction::Start)
            .map_err(|_| AppError::invalid_work_state(work_id, queued, WorkStatus::Running))?;
        let assigned_agent_id = sqlx::query_scalar::<_, String>(
            "SELECT agent_instance_id FROM work_agents \
             WHERE work_id = ? AND role_kind = 'lead' AND status = 'joined'",
        )
        .bind(work_id)
        .fetch_one(&mut *transaction)
        .await?;
        let now = Utc::now();
        let run_id = Uuid::new_v4().to_string();
        let assignment =
            legacy_compatibility_assignment(&work, prompt, &assigned_agent_id, &run_id, now);
        let run = RunSummary {
            id: run_id,
            work_id: work_id.to_owned(),
            assignment_id: None,
            agent_instance_id: None,
            engine_kind: engine_kind.to_owned(),
            engine_session_id: None,
            model_label: model_label.to_owned(),
            status: RunStatus::Running,
            created_at: now,
            started_at: Some(now),
            completed_at: None,
        };
        let user_message = UserMessageSummary {
            id: Uuid::new_v4().to_string(),
            work_id: work_id.to_owned(),
            assignment_id: Some(assignment.id.clone()),
            run_id: Some(run.id.clone()),
            role: MessageRole::User,
            content: prompt.to_owned(),
            resource_ids: resource_ids.clone(),
            created_at: now,
        };

        let updated =
            sqlx::query("UPDATE works SET status = ?, updated_at = ? WHERE id = ? AND status = ?")
                .bind(running)
                .bind(now)
                .bind(work_id)
                .bind(work.status)
                .execute(&mut *transaction)
                .await?;
        if updated.rows_affected() == 0 {
            return Err(AppError::concurrent_work_modification(work_id));
        }
        sqlx::query(
            "INSERT INTO runs \
             (id, work_id, engine_kind, engine_session_id, model_label, status, created_at, \
              updated_at, started_at, completed_at) \
             VALUES (?, ?, ?, NULL, ?, ?, ?, ?, ?, NULL)",
        )
        .bind(&run.id)
        .bind(&run.work_id)
        .bind(&run.engine_kind)
        .bind(&run.model_label)
        .bind(run.status)
        .bind(run.created_at)
        .bind(now)
        .bind(run.started_at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO messages (id, work_id, run_id, role, content, created_at) \
             VALUES (?, ?, ?, 'user', ?, ?)",
        )
        .bind(&user_message.id)
        .bind(&user_message.work_id)
        .bind(user_message.run_id.as_deref().expect("new run has an id"))
        .bind(&user_message.content)
        .bind(user_message.created_at)
        .execute(&mut *transaction)
        .await?;
        for resource_id in &resource_ids {
            sqlx::query(
                "INSERT INTO resource_links \
                 (id, resource_id, work_id, draft_id, message_id, run_id, role, created_at) \
                 VALUES (?, ?, ?, NULL, ?, ?, 'attached', ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(resource_id)
            .bind(work_id)
            .bind(&user_message.id)
            .bind(&run.id)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;

        Ok(StartWorkOutput {
            assignment,
            run: Some(run),
            user_message,
        })
    }

    pub async fn append_event_and_transition(
        &self,
        envelope: &WorkEventEnvelope,
    ) -> Result<(), AppError> {
        let event_id = envelope.event_id.as_deref().ok_or_else(|| {
            AppError::invalid_input("eventId", "new Work events require an event id")
        })?;
        if envelope.sequence == 0 {
            return Err(AppError::invalid_input(
                "sequence",
                "event sequence must be at least 1",
            ));
        }

        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let run = sqlx::query_as::<_, RunStateRow>("SELECT work_id, status FROM runs WHERE id = ?")
            .bind(&envelope.run_id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| AppError::run_not_found(&envelope.run_id))?;
        if run.work_id != envelope.work_id {
            return Err(AppError::run_not_found(&envelope.run_id));
        }
        let work_status =
            sqlx::query_scalar::<_, WorkStatus>("SELECT status FROM works WHERE id = ?")
                .bind(&envelope.work_id)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or_else(|| AppError::work_not_found(&envelope.work_id))?;
        if !matches!(run.status, RunStatus::Running | RunStatus::Waiting) {
            return Err(AppError::invalid_run_state(
                &envelope.run_id,
                run.status,
                RunStatus::Running,
            ));
        }
        if !matches!(work_status, WorkStatus::Running | WorkStatus::Waiting) {
            return Err(AppError::invalid_work_state(
                &envelope.work_id,
                work_status,
                WorkStatus::Running,
            ));
        }
        let current_sequence = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(sequence) FROM events WHERE run_id = ?",
        )
        .bind(&envelope.run_id)
        .fetch_one(&mut *transaction)
        .await?
        .unwrap_or(0);
        let next_sequence = current_sequence
            .checked_add(1)
            .ok_or_else(|| AppError::invalid_input("sequence", "event sequence limit exceeded"))?;
        if next_sequence > i64::from(u32::MAX) || i64::from(envelope.sequence) != next_sequence {
            return Err(AppError::invalid_input(
                "sequence",
                "event sequence is not the next sequence for this Run",
            ));
        }

        Self::insert_event(
            &mut transaction,
            event_id,
            envelope,
            i64::from(envelope.sequence),
        )
        .await?;

        let terminal = match envelope.payload {
            WorkEventPayload::RunCompleted { .. } => {
                Some((RunAction::Complete, WorkAction::Complete))
            }
            WorkEventPayload::RunFailed { .. } => Some((RunAction::Fail, WorkAction::Fail)),
            _ => None,
        };
        if let Some((run_action, work_action)) = terminal {
            let next_run = transition_run(run.status, run_action).map_err(|_| {
                AppError::invalid_run_state(
                    &envelope.run_id,
                    run.status,
                    match run_action {
                        RunAction::Complete => RunStatus::Completed,
                        RunAction::Fail => RunStatus::Failed,
                        _ => unreachable!(),
                    },
                )
            })?;
            let next_work = transition(work_status, work_action).map_err(|_| {
                AppError::invalid_work_state(
                    &envelope.work_id,
                    work_status,
                    match work_action {
                        WorkAction::Complete => WorkStatus::Completed,
                        WorkAction::Fail => WorkStatus::Failed,
                        _ => unreachable!(),
                    },
                )
            })?;
            let now = Utc::now();
            let run_update = sqlx::query(
                "UPDATE runs SET status = ?, updated_at = ?, completed_at = ? \
                 WHERE id = ? AND status = ?",
            )
            .bind(next_run)
            .bind(now)
            .bind(now)
            .bind(&envelope.run_id)
            .bind(run.status)
            .execute(&mut *transaction)
            .await?;
            if run_update.rows_affected() == 0 {
                return Err(AppError::concurrent_run_modification(&envelope.run_id));
            }
            let work_update = sqlx::query(
                "UPDATE works SET status = ?, updated_at = ? WHERE id = ? AND status = ?",
            )
            .bind(next_work)
            .bind(now)
            .bind(&envelope.work_id)
            .bind(work_status)
            .execute(&mut *transaction)
            .await?;
            if work_update.rows_affected() == 0 {
                return Err(AppError::concurrent_work_modification(&envelope.work_id));
            }
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn events_for_run(&self, run_id: &str) -> Result<Vec<WorkEventEnvelope>, AppError> {
        sqlx::query_as::<_, EventRow>(
            "SELECT events.id, events.work_id, events.run_id, events.turn_id, \
                    events.session_id, events.agent_id, events.assignment_id, \
                    events.causation_id, events.correlation_id, events.sequence, \
                    events.version, events.occurred_at, events.payload \
             FROM events WHERE run_id = ? ORDER BY sequence ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(WorkEventEnvelope::try_from)
        .collect()
    }

    pub async fn attach_engine_session(
        &self,
        run_id: &str,
        engine_kind: &str,
        session_id: &str,
    ) -> Result<RunSummary, AppError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut run = sqlx::query_as::<_, RunRow>(
            "SELECT id, work_id, engine_kind, engine_session_id, model_label, status, \
                    created_at, started_at, completed_at FROM runs WHERE id = ?",
        )
        .bind(run_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| AppError::run_not_found(run_id))?;
        if !matches!(run.status, RunStatus::Running | RunStatus::Waiting) {
            return Err(AppError::invalid_run_state(
                run_id,
                run.status,
                RunStatus::Running,
            ));
        }
        if run.engine_kind != engine_kind {
            return Err(AppError::invalid_input(
                "engineKind",
                "engine session kind does not match the Run",
            ));
        }
        let updated = sqlx::query(
            "UPDATE runs SET engine_session_id = ?, updated_at = ? \
             WHERE id = ? AND status IN ('running', 'waiting') AND engine_session_id IS NULL",
        )
        .bind(session_id)
        .bind(Utc::now())
        .bind(run_id)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(AppError::concurrent_run_modification(run_id));
        }
        transaction.commit().await?;
        run.engine_session_id = Some(session_id.to_owned());
        Ok(run.into())
    }

    pub async fn finalize_run_failure(
        &self,
        run_id: &str,
        work_id: &str,
        reason: &str,
    ) -> Result<WorkEventEnvelope, AppError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let run = sqlx::query_as::<_, FinalizeRunStateRow>(
            "SELECT work_id, status, engine_session_id FROM runs WHERE id = ?",
        )
        .bind(run_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| AppError::run_not_found(run_id))?;
        if run.work_id != work_id {
            return Err(AppError::run_not_found(run_id));
        }
        let work_status =
            sqlx::query_scalar::<_, WorkStatus>("SELECT status FROM works WHERE id = ?")
                .bind(work_id)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or_else(|| AppError::work_not_found(work_id))?;
        if !matches!(run.status, RunStatus::Running | RunStatus::Waiting) {
            return Err(AppError::invalid_run_state(
                run_id,
                run.status,
                RunStatus::Failed,
            ));
        }
        if !matches!(work_status, WorkStatus::Running | WorkStatus::Waiting) {
            return Err(AppError::invalid_work_state(
                work_id,
                work_status,
                WorkStatus::Failed,
            ));
        }
        let next_run = transition_run(run.status, RunAction::Fail)
            .map_err(|_| AppError::invalid_run_state(run_id, run.status, RunStatus::Failed))?;
        let next_work = transition(work_status, WorkAction::Fail)
            .map_err(|_| AppError::invalid_work_state(work_id, work_status, WorkStatus::Failed))?;
        let last_event = sqlx::query_as::<_, (String, i64)>(
            "SELECT id, sequence FROM events \
             WHERE run_id = ? ORDER BY sequence DESC LIMIT 1",
        )
        .bind(run_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let (causation_id, current_sequence) = match last_event {
            Some((event_id, sequence)) => (Some(event_id), sequence),
            None => (None, 0),
        };
        let next_sequence = current_sequence
            .checked_add(1)
            .filter(|sequence| *sequence <= i64::from(u32::MAX))
            .ok_or_else(|| AppError::invalid_input("sequence", "event sequence limit exceeded"))?;
        let event_id = Uuid::new_v4().to_string();
        let envelope = WorkEventEnvelope {
            version: 2,
            event_id: Some(event_id.clone()),
            work_id: work_id.to_owned(),
            run_id: run_id.to_owned(),
            turn_id: Some(run_id.to_owned()),
            session_id: run.engine_session_id,
            agent_id: None,
            assignment_id: None,
            causation_id,
            correlation_id: Some(run_id.to_owned()),
            sequence: u32::try_from(next_sequence)
                .map_err(|error| AppError::Database(sqlx::Error::Decode(Box::new(error))))?,
            occurred_at: Utc::now(),
            payload: WorkEventPayload::RunFailed {
                message: reason.to_owned(),
            },
        };
        Self::insert_event(&mut transaction, &event_id, &envelope, next_sequence).await?;
        let now = Utc::now();
        let run_update = sqlx::query(
            "UPDATE runs SET status = ?, updated_at = ?, completed_at = ? \
             WHERE id = ? AND status = ?",
        )
        .bind(next_run)
        .bind(now)
        .bind(now)
        .bind(run_id)
        .bind(run.status)
        .execute(&mut *transaction)
        .await?;
        if run_update.rows_affected() == 0 {
            return Err(AppError::concurrent_run_modification(run_id));
        }
        let work_update =
            sqlx::query("UPDATE works SET status = ?, updated_at = ? WHERE id = ? AND status = ?")
                .bind(next_work)
                .bind(now)
                .bind(work_id)
                .bind(work_status)
                .execute(&mut *transaction)
                .await?;
        if work_update.rows_affected() == 0 {
            return Err(AppError::concurrent_work_modification(work_id));
        }
        transaction.commit().await?;
        Ok(envelope)
    }

    pub async fn stop_run(&self, run_id: &str, work_id: &str) -> Result<(), AppError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let run = sqlx::query_as::<_, RunStateRow>("SELECT work_id, status FROM runs WHERE id = ?")
            .bind(run_id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| AppError::run_not_found(run_id))?;
        if run.work_id != work_id {
            return Err(AppError::run_not_found(run_id));
        }
        let work_status =
            sqlx::query_scalar::<_, WorkStatus>("SELECT status FROM works WHERE id = ?")
                .bind(work_id)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or_else(|| AppError::work_not_found(work_id))?;
        let next_run = transition_run(run.status, RunAction::Stop)
            .map_err(|_| AppError::invalid_run_state(run_id, run.status, RunStatus::Stopped))?;
        let next_work = transition(work_status, WorkAction::Stop)
            .map_err(|_| AppError::invalid_work_state(work_id, work_status, WorkStatus::Stopped))?;
        let now = Utc::now();
        let run_update = sqlx::query(
            "UPDATE runs SET status = ?, updated_at = ?, completed_at = ? \
             WHERE id = ? AND status = ?",
        )
        .bind(next_run)
        .bind(now)
        .bind(now)
        .bind(run_id)
        .bind(run.status)
        .execute(&mut *transaction)
        .await?;
        if run_update.rows_affected() == 0 {
            return Err(AppError::concurrent_run_modification(run_id));
        }
        let work_update =
            sqlx::query("UPDATE works SET status = ?, updated_at = ? WHERE id = ? AND status = ?")
                .bind(next_work)
                .bind(now)
                .bind(work_id)
                .bind(work_status)
                .execute(&mut *transaction)
                .await?;
        if work_update.rows_affected() == 0 {
            return Err(AppError::concurrent_work_modification(work_id));
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn set_work_status(&self, work_id: &str, status: WorkStatus) -> Result<(), AppError> {
        let current = self.validate_work_transition(work_id, status).await?;
        self.update_work_status(work_id, current, status).await
    }

    #[cfg(test)]
    async fn set_work_status_after_read<F>(
        &self,
        work_id: &str,
        status: WorkStatus,
        after_read: F,
    ) -> Result<(), AppError>
    where
        F: FnOnce(),
    {
        let current = self.validate_work_transition(work_id, status).await?;
        after_read();
        self.update_work_status(work_id, current, status).await
    }

    async fn validate_work_transition(
        &self,
        work_id: &str,
        status: WorkStatus,
    ) -> Result<WorkStatus, AppError> {
        let current = sqlx::query_scalar::<_, WorkStatus>("SELECT status FROM works WHERE id = ?")
            .bind(work_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::work_not_found(work_id))?;
        let transition_result = work_action_for_target(status)
            .and_then(|action| transition(current, action).ok())
            .filter(|next| *next == status);
        if transition_result.is_none() {
            return Err(AppError::invalid_work_state(work_id, current, status));
        }

        Ok(current)
    }

    async fn update_work_status(
        &self,
        work_id: &str,
        current: WorkStatus,
        status: WorkStatus,
    ) -> Result<(), AppError> {
        let result =
            sqlx::query("UPDATE works SET status = ?, updated_at = ? WHERE id = ? AND status = ?")
                .bind(status)
                .bind(Utc::now())
                .bind(work_id)
                .bind(current)
                .execute(&self.pool)
                .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::concurrent_work_modification(work_id));
        }

        Ok(())
    }

    pub async fn set_run_status(&self, run_id: &str, status: RunStatus) -> Result<(), AppError> {
        let current = self.validate_run_transition(run_id, status).await?;
        self.update_run_status(run_id, current, status).await
    }

    #[cfg(test)]
    async fn set_run_status_after_read<F>(
        &self,
        run_id: &str,
        status: RunStatus,
        after_read: F,
    ) -> Result<(), AppError>
    where
        F: FnOnce(),
    {
        let current = self.validate_run_transition(run_id, status).await?;
        after_read();
        self.update_run_status(run_id, current, status).await
    }

    async fn validate_run_transition(
        &self,
        run_id: &str,
        status: RunStatus,
    ) -> Result<RunStatus, AppError> {
        let current = sqlx::query_scalar::<_, RunStatus>("SELECT status FROM runs WHERE id = ?")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::run_not_found(run_id))?;
        let transition_result = run_action_for_target(status)
            .and_then(|action| transition_run(current, action).ok())
            .filter(|next| *next == status);
        if transition_result.is_none() {
            return Err(AppError::invalid_run_state(run_id, current, status));
        }

        Ok(current)
    }

    async fn update_run_status(
        &self,
        run_id: &str,
        current: RunStatus,
        status: RunStatus,
    ) -> Result<(), AppError> {
        let now = Utc::now();
        let result = match status {
            RunStatus::Running => {
                sqlx::query(
                    "UPDATE runs \
                     SET status = ?, updated_at = ?, started_at = COALESCE(started_at, ?), \
                         completed_at = NULL \
                     WHERE id = ? AND status = ?",
                )
                .bind(status)
                .bind(now)
                .bind(now)
                .bind(run_id)
                .bind(current)
                .execute(&self.pool)
                .await?
            }
            RunStatus::Completed
            | RunStatus::Failed
            | RunStatus::Stopped
            | RunStatus::Interrupted => {
                sqlx::query(
                    "UPDATE runs \
                     SET status = ?, updated_at = ?, completed_at = COALESCE(completed_at, ?) \
                     WHERE id = ? AND status = ?",
                )
                .bind(status)
                .bind(now)
                .bind(now)
                .bind(run_id)
                .bind(current)
                .execute(&self.pool)
                .await?
            }
            RunStatus::Queued | RunStatus::Waiting => {
                sqlx::query(
                    "UPDATE runs SET status = ?, updated_at = ? WHERE id = ? AND status = ?",
                )
                .bind(status)
                .bind(now)
                .bind(run_id)
                .bind(current)
                .execute(&self.pool)
                .await?
            }
        };
        if result.rows_affected() == 0 {
            return Err(AppError::concurrent_run_modification(run_id));
        }

        Ok(())
    }
}

// Delete this adapter when C9 switches start acceptance to persisted Assignments.
// Its namespace and empty execution metadata prevent legacy output from being
// mistaken for a persisted, schedulable, or recoverable Assignment.
fn legacy_compatibility_assignment(
    work: &WorkRow,
    instruction: &str,
    assigned_agent_id: &str,
    run_id: &str,
    now: DateTime<Utc>,
) -> AssignmentSummary {
    AssignmentSummary {
        id: format!("legacy-run:{run_id}"),
        work_id: work.id.clone(),
        parent_assignment_id: None,
        created_by_agent_id: None,
        assigned_agent_id: assigned_agent_id.to_owned(),
        capability_pack_id: None,
        kind: AssignmentKind::Lead,
        side_effect: AssignmentSideEffect::Unknown,
        title: work.title.clone(),
        instruction: instruction.to_owned(),
        context_manifest: serde_json::Value::Null,
        expected_result_schema: serde_json::Value::Null,
        acceptance_criteria: serde_json::Value::Null,
        permission_scope: serde_json::Value::Null,
        priority: 0,
        status: AssignmentStatus::Running,
        attempt_count: 1,
        max_attempts: 1,
        not_before: None,
        result_summary: None,
        last_error: None,
        next_attempt_at: None,
        recovery_reason: None,
        created_at: now,
        claimed_at: None,
        started_at: Some(now),
        completed_at: None,
        updated_at: now,
    }
}

fn work_action_for_target(status: WorkStatus) -> Option<WorkAction> {
    match status {
        WorkStatus::Draft => None,
        WorkStatus::Queued => Some(WorkAction::Queue),
        WorkStatus::Running => Some(WorkAction::Start),
        WorkStatus::Waiting => Some(WorkAction::Wait),
        WorkStatus::Idle => Some(WorkAction::Idle),
        WorkStatus::Completed => Some(WorkAction::Complete),
        WorkStatus::Failed => Some(WorkAction::Fail),
        WorkStatus::Stopped => Some(WorkAction::Stop),
        WorkStatus::Interrupted => Some(WorkAction::Interrupt),
        WorkStatus::Archived => Some(WorkAction::Archive),
    }
}

fn run_action_for_target(status: RunStatus) -> Option<RunAction> {
    match status {
        RunStatus::Queued => None,
        RunStatus::Running => Some(RunAction::Start),
        RunStatus::Waiting => Some(RunAction::Wait),
        RunStatus::Completed => Some(RunAction::Complete),
        RunStatus::Failed => Some(RunAction::Fail),
        RunStatus::Stopped => Some(RunAction::Stop),
        RunStatus::Interrupted => Some(RunAction::Interrupt),
    }
}

#[derive(FromRow)]
struct WorkRow {
    id: String,
    title: String,
    goal: String,
    root_path: String,
    permission_mode: PermissionMode,
    status: WorkStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<WorkRow> for WorkSummary {
    fn from(row: WorkRow) -> Self {
        Self {
            id: row.id,
            title: row.title,
            goal: row.goal,
            root_path: row.root_path,
            permission_mode: row.permission_mode,
            status: row.status,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(FromRow)]
struct RunRow {
    id: String,
    work_id: String,
    engine_kind: String,
    engine_session_id: Option<String>,
    model_label: String,
    status: RunStatus,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct RunStateRow {
    work_id: String,
    status: RunStatus,
}

#[derive(FromRow)]
struct FinalizeRunStateRow {
    work_id: String,
    status: RunStatus,
    engine_session_id: Option<String>,
}

#[derive(FromRow)]
struct MessageRow {
    id: String,
    work_id: String,
    run_id: String,
    role: MessageRole,
    content: String,
    created_at: DateTime<Utc>,
}

impl From<MessageRow> for MessageSummary {
    fn from(row: MessageRow) -> Self {
        Self {
            id: row.id,
            work_id: row.work_id,
            run_id: row.run_id,
            role: row.role,
            content: row.content,
            resource_ids: Vec::new(),
            created_at: row.created_at,
        }
    }
}

impl From<RunRow> for RunSummary {
    fn from(row: RunRow) -> Self {
        Self {
            id: row.id,
            work_id: row.work_id,
            assignment_id: None,
            agent_instance_id: None,
            engine_kind: row.engine_kind,
            engine_session_id: row.engine_session_id,
            model_label: row.model_label,
            status: row.status,
            created_at: row.created_at,
            started_at: row.started_at,
            completed_at: row.completed_at,
        }
    }
}

#[derive(FromRow)]
struct EventRow {
    id: String,
    work_id: String,
    run_id: String,
    turn_id: Option<String>,
    session_id: Option<String>,
    agent_id: Option<String>,
    assignment_id: Option<String>,
    causation_id: Option<String>,
    correlation_id: Option<String>,
    sequence: i64,
    version: i64,
    occurred_at: DateTime<Utc>,
    payload: String,
}

impl TryFrom<EventRow> for WorkEventEnvelope {
    type Error = AppError;

    fn try_from(row: EventRow) -> Result<Self, Self::Error> {
        let sequence = u32::try_from(row.sequence)
            .map_err(|error| AppError::Database(sqlx::Error::Decode(Box::new(error))))?;
        let version = u32::try_from(row.version)
            .map_err(|error| AppError::Database(sqlx::Error::Decode(Box::new(error))))?;
        let payload: WorkEventPayload = serde_json::from_str(&row.payload)
            .map_err(|error| AppError::Database(sqlx::Error::Decode(Box::new(error))))?;

        Ok(Self {
            version,
            event_id: Some(row.id),
            work_id: row.work_id,
            run_id: row.run_id,
            turn_id: row.turn_id,
            session_id: row.session_id,
            agent_id: row.agent_id,
            assignment_id: row.assignment_id,
            causation_id: row.causation_id,
            correlation_id: row.correlation_id,
            sequence,
            occurred_at: row.occurred_at,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Barrier, mpsc},
        time::Duration,
    };

    use chrono::Utc;
    use tokio::sync::oneshot;
    use uuid::Uuid;

    use crate::{
        domain::{
            event::{WorkEventEnvelope, WorkEventPayload},
            work::{CreateWorkInput, PermissionMode, RunStatus, WorkStatus},
        },
        storage::sqlite::Database,
    };

    use super::WorkRepository;

    #[tokio::test]
    async fn event_append_waits_for_a_concurrent_session_attachment() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let database_path = temporary_directory
            .path()
            .join("event-session-race.sqlite3");
        let workspace_path = temporary_directory.path().join("workspace");
        std::fs::create_dir(&workspace_path).unwrap();
        let database = Database::open(&database_path).await.unwrap();
        let repository = WorkRepository::new(database.pool().clone());
        let work = repository
            .create(CreateWorkInput {
                title: "Concurrent session".into(),
                goal: "Journal the first event".into(),
                root_path: workspace_path.to_string_lossy().into_owned(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap();
        let started = repository
            .begin_run(&work.summary.id, "Start", &[], "test-engine", "test-model")
            .await
            .unwrap();

        let mut session_attachment = database.pool().begin_with("BEGIN IMMEDIATE").await.unwrap();
        sqlx::query("UPDATE runs SET engine_session_id = ? WHERE id = ?")
            .bind("session-1")
            .bind(&started.run.as_ref().expect("immediate run").id)
            .execute(&mut *session_attachment)
            .await
            .unwrap();

        let append_repository = repository.clone();
        let envelope = WorkEventEnvelope {
            version: 1,
            event_id: Some(Uuid::new_v4().to_string()),
            work_id: work.summary.id.clone(),
            run_id: started.run.as_ref().expect("immediate run").id.clone(),
            turn_id: None,
            session_id: None,
            agent_id: None,
            assignment_id: None,
            causation_id: None,
            correlation_id: None,
            sequence: 1,
            occurred_at: Utc::now(),
            payload: WorkEventPayload::RunStarted {
                model_label: "test-model".into(),
            },
        };
        let expected = envelope.clone();
        let append = tokio::spawn(async move {
            append_repository
                .append_event_and_transition(&envelope)
                .await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        session_attachment.commit().await.unwrap();

        tokio::time::timeout(Duration::from_secs(1), append)
            .await
            .expect("event append remained blocked after session attachment")
            .unwrap()
            .expect("event append failed after concurrent session attachment");

        let detail = repository.get(&work.summary.id).await.unwrap().unwrap();
        assert_eq!(detail.events, vec![expected]);
    }

    #[tokio::test]
    async fn activity_context_round_trips_through_the_journal() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let database_path = temporary_directory.path().join("activity-context.sqlite3");
        let workspace_path = temporary_directory.path().join("workspace");
        std::fs::create_dir(&workspace_path).unwrap();
        let database = Database::open(&database_path).await.unwrap();
        let repository = WorkRepository::new(database.pool().clone());
        let work = repository
            .create(CreateWorkInput {
                title: "Activity context".into(),
                goal: "Round-trip the complete event envelope".into(),
                root_path: workspace_path.to_string_lossy().into_owned(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap();
        let started = repository
            .begin_run(&work.summary.id, "Start", &[], "test-engine", "test-model")
            .await
            .unwrap();
        let envelope = WorkEventEnvelope {
            version: 2,
            event_id: Some("event-1".into()),
            work_id: work.summary.id.clone(),
            run_id: started.run.as_ref().expect("immediate run").id.clone(),
            turn_id: Some("turn-1".into()),
            session_id: Some("session-1".into()),
            agent_id: Some("agent-1".into()),
            assignment_id: Some("assignment-1".into()),
            causation_id: Some("event-0".into()),
            correlation_id: Some("correlation-1".into()),
            sequence: 1,
            occurred_at: Utc::now(),
            payload: WorkEventPayload::ToolProgress {
                tool_call_id: "tool-1".into(),
                tool_name: "read".into(),
                output_summary: "halfway".into(),
            },
        };

        repository
            .append_event_and_transition(&envelope)
            .await
            .unwrap();

        let loaded = repository
            .events_for_run(&started.run.as_ref().expect("immediate run").id)
            .await
            .unwrap();
        assert_eq!(loaded, vec![envelope.clone()]);
        let detail = repository.get(&work.summary.id).await.unwrap().unwrap();
        assert_eq!(detail.events, vec![envelope]);
    }

    #[tokio::test]
    async fn finalize_run_failure_without_prior_events_uses_v2_run_context() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let database_path = temporary_directory.path().join("finalize-failure.sqlite3");
        let workspace_path = temporary_directory.path().join("workspace");
        std::fs::create_dir(&workspace_path).unwrap();
        let database = Database::open(&database_path).await.unwrap();
        let repository = WorkRepository::new(database.pool().clone());
        let work = repository
            .create(CreateWorkInput {
                title: "Finalize failure".into(),
                goal: "Persist the returned event identity".into(),
                root_path: workspace_path.to_string_lossy().into_owned(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap();
        let started = repository
            .begin_run(&work.summary.id, "Start", &[], "test-engine", "test-model")
            .await
            .unwrap();

        let failed = repository
            .finalize_run_failure(
                &started.run.as_ref().expect("immediate run").id,
                &work.summary.id,
                "engine failed",
            )
            .await
            .unwrap();
        let loaded = repository
            .events_for_run(&started.run.as_ref().expect("immediate run").id)
            .await
            .unwrap();

        assert!(failed.event_id.is_some());
        assert_eq!(failed.event_id, loaded[0].event_id);
        assert_eq!(failed.version, 2);
        assert_eq!(
            failed.turn_id.as_deref(),
            Some(started.run.as_ref().expect("immediate run").id.as_str())
        );
        assert_eq!(
            failed.correlation_id.as_deref(),
            Some(started.run.as_ref().expect("immediate run").id.as_str())
        );
        assert_eq!(failed.session_id, None);
        assert_eq!(failed.causation_id, None);
    }

    #[tokio::test]
    async fn finalize_run_failure_without_prior_events_uses_the_attached_run_session() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let database_path = temporary_directory
            .path()
            .join("attached-session-failure.sqlite3");
        let workspace_path = temporary_directory.path().join("workspace");
        std::fs::create_dir(&workspace_path).unwrap();
        let database = Database::open(&database_path).await.unwrap();
        let repository = WorkRepository::new(database.pool().clone());
        let work = repository
            .create(CreateWorkInput {
                title: "Attached failure".into(),
                goal: "Retain the authoritative engine session".into(),
                root_path: workspace_path.to_string_lossy().into_owned(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap();
        let started = repository
            .begin_run(&work.summary.id, "Start", &[], "test-engine", "test-model")
            .await
            .unwrap();
        repository
            .attach_engine_session(
                &started.run.as_ref().expect("immediate run").id,
                "test-engine",
                "test-session",
            )
            .await
            .unwrap();

        let failed = repository
            .finalize_run_failure(
                &started.run.as_ref().expect("immediate run").id,
                &work.summary.id,
                "engine failed",
            )
            .await
            .unwrap();

        assert_eq!(failed.version, 2);
        assert_eq!(failed.session_id.as_deref(), Some("test-session"));
        assert_eq!(failed.causation_id, None);
    }

    #[tokio::test]
    async fn finalize_run_failure_uses_run_session_and_last_committed_event_as_causation() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let database_path = temporary_directory.path().join("causal-failure.sqlite3");
        let workspace_path = temporary_directory.path().join("workspace");
        std::fs::create_dir(&workspace_path).unwrap();
        let database = Database::open(&database_path).await.unwrap();
        let repository = WorkRepository::new(database.pool().clone());
        let work = repository
            .create(CreateWorkInput {
                title: "Causal failure".into(),
                goal: "Link failure to committed activity".into(),
                root_path: workspace_path.to_string_lossy().into_owned(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap();
        let started = repository
            .begin_run(&work.summary.id, "Start", &[], "test-engine", "test-model")
            .await
            .unwrap();
        repository
            .attach_engine_session(
                &started.run.as_ref().expect("immediate run").id,
                "test-engine",
                "test-session",
            )
            .await
            .unwrap();
        let committed_id = Uuid::new_v4().to_string();
        let committed = WorkEventEnvelope {
            version: 1,
            event_id: Some(committed_id.clone()),
            work_id: work.summary.id.clone(),
            run_id: started.run.as_ref().expect("immediate run").id.clone(),
            turn_id: None,
            session_id: None,
            agent_id: None,
            assignment_id: None,
            causation_id: None,
            correlation_id: None,
            sequence: 1,
            occurred_at: Utc::now(),
            payload: WorkEventPayload::RunStarted {
                model_label: "test-model".into(),
            },
        };
        repository
            .append_event_and_transition(&committed)
            .await
            .unwrap();
        let rejected_id = Uuid::new_v4().to_string();
        let mut rejected = committed.clone();
        rejected.event_id = Some(rejected_id.clone());
        rejected.sequence = 3;
        rejected.payload = WorkEventPayload::AssistantDelta {
            text: "not committed".into(),
        };
        repository
            .append_event_and_transition(&rejected)
            .await
            .unwrap_err();

        let failed = repository
            .finalize_run_failure(
                &started.run.as_ref().expect("immediate run").id,
                &work.summary.id,
                "engine failed",
            )
            .await
            .unwrap();
        let loaded = repository
            .events_for_run(&started.run.as_ref().expect("immediate run").id)
            .await
            .unwrap();

        assert_eq!(failed.version, 2);
        assert_eq!(failed.sequence, 2);
        assert_eq!(failed.session_id.as_deref(), Some("test-session"));
        assert_eq!(failed.causation_id.as_deref(), Some(committed_id.as_str()));
        assert_ne!(failed.causation_id.as_deref(), Some(rejected_id.as_str()));
        assert_eq!(loaded, vec![committed, failed]);
    }

    #[tokio::test]
    async fn new_event_without_an_id_is_rejected() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let database_path = temporary_directory.path().join("missing-event-id.sqlite3");
        let workspace_path = temporary_directory.path().join("workspace");
        std::fs::create_dir(&workspace_path).unwrap();
        let database = Database::open(&database_path).await.unwrap();
        let repository = WorkRepository::new(database.pool().clone());
        let work = repository
            .create(CreateWorkInput {
                title: "Missing event id".into(),
                goal: "Reject incomplete event identity".into(),
                root_path: workspace_path.to_string_lossy().into_owned(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap();
        let started = repository
            .begin_run(&work.summary.id, "Start", &[], "test-engine", "test-model")
            .await
            .unwrap();
        let envelope = WorkEventEnvelope {
            version: 2,
            event_id: None,
            work_id: work.summary.id,
            run_id: started.run.as_ref().expect("immediate run").id.clone(),
            turn_id: None,
            session_id: None,
            agent_id: None,
            assignment_id: None,
            causation_id: None,
            correlation_id: None,
            sequence: 1,
            occurred_at: Utc::now(),
            payload: WorkEventPayload::RunStarted {
                model_label: "test-model".into(),
            },
        };

        let error = repository
            .append_event_and_transition(&envelope)
            .await
            .unwrap_err();

        assert_eq!(
            serde_json::to_value(error).unwrap(),
            serde_json::json!({
                "code": "invalid_input",
                "message": "new Work events require an event id",
                "details": { "field": "eventId" }
            })
        );
    }

    #[tokio::test]
    async fn session_attachment_waits_for_a_concurrent_event_append() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let database_path = temporary_directory
            .path()
            .join("session-event-race.sqlite3");
        let workspace_path = temporary_directory.path().join("workspace");
        std::fs::create_dir(&workspace_path).unwrap();
        let database = Database::open(&database_path).await.unwrap();
        let repository = WorkRepository::new(database.pool().clone());
        let work = repository
            .create(CreateWorkInput {
                title: "Concurrent event".into(),
                goal: "Attach the engine session".into(),
                root_path: workspace_path.to_string_lossy().into_owned(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap();
        let started = repository
            .begin_run(&work.summary.id, "Start", &[], "test-engine", "test-model")
            .await
            .unwrap();

        let mut event_append = database.pool().begin_with("BEGIN IMMEDIATE").await.unwrap();
        sqlx::query(
            "INSERT INTO events \
             (id, work_id, run_id, sequence, version, occurred_at, payload) \
             VALUES (?, ?, ?, 1, 1, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&work.summary.id)
        .bind(&started.run.as_ref().expect("immediate run").id)
        .bind(Utc::now())
        .bind(r#"{"type":"runStarted","modelLabel":"test-model"}"#)
        .execute(&mut *event_append)
        .await
        .unwrap();

        let attach_repository = repository.clone();
        let run_id = started.run.as_ref().expect("immediate run").id.clone();
        let attachment = tokio::spawn(async move {
            attach_repository
                .attach_engine_session(&run_id, "test-engine", "session-1")
                .await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        event_append.commit().await.unwrap();

        let attached = tokio::time::timeout(Duration::from_secs(1), attachment)
            .await
            .expect("session attachment remained blocked after event append")
            .unwrap()
            .expect("session attachment failed after concurrent event append");

        assert_eq!(attached.engine_session_id.as_deref(), Some("session-1"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn detail_read_keeps_one_snapshot_during_concurrent_run_and_event_inserts() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let database_path = temporary_directory.path().join("snapshot.sqlite3");
        let workspace_path = temporary_directory.path().join("workspace");
        std::fs::create_dir(&workspace_path).unwrap();
        let database = Database::open(&database_path).await.unwrap();
        let repository = WorkRepository::new(database.pool().clone());
        let work = repository
            .create(CreateWorkInput {
                title: "Snapshot".into(),
                goal: "Read consistently".into(),
                root_path: workspace_path.to_string_lossy().into_owned(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap();

        let writer = repository.clone();
        let work_id = work.summary.id.clone();
        let (start_writer, writer_started) = oneshot::channel();
        let (writer_finished, wait_for_writer) = mpsc::sync_channel(0);
        let writer_task = tokio::spawn(async move {
            writer_started.await.unwrap();
            let run = writer
                .insert_run(&work_id, "concurrent-model")
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO events \
                 (id, work_id, run_id, sequence, version, occurred_at, payload) \
                 VALUES (?, ?, ?, 1, 1, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&work_id)
            .bind(&run.id)
            .bind(Utc::now())
            .bind(r#"{"type":"assistantDelta","text":"concurrent"}"#)
            .execute(&writer.pool)
            .await
            .unwrap();
            writer_finished.send(()).unwrap();
            run
        });

        let detail = repository
            .get_after_work_loaded(&work.summary.id, move || {
                start_writer.send(()).unwrap();
                wait_for_writer
                    .recv_timeout(Duration::from_secs(5))
                    .expect("concurrent writer did not finish");
            })
            .await
            .unwrap()
            .unwrap();
        let inserted_run = writer_task.await.unwrap();

        assert!(detail.runs.is_empty());
        assert!(detail.events.is_empty());
        let refreshed = repository.get(&work.summary.id).await.unwrap().unwrap();
        assert_eq!(refreshed.runs, vec![inserted_run]);
        assert_eq!(refreshed.events.len(), 1);
        assert_eq!(refreshed.events[0].run_id, refreshed.runs[0].id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_run_transitions_use_compare_and_swap() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let database_path = temporary_directory.path().join("status-cas.sqlite3");
        let workspace_path = temporary_directory.path().join("workspace");
        std::fs::create_dir(&workspace_path).unwrap();
        let database = Database::open(&database_path).await.unwrap();
        let repository = WorkRepository::new(database.pool().clone());
        let work = repository
            .create(CreateWorkInput {
                title: "CAS".into(),
                goal: "Allow one transition".into(),
                root_path: workspace_path.to_string_lossy().into_owned(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap();
        let run = repository
            .insert_run(&work.summary.id, "model")
            .await
            .unwrap();
        let barrier = Arc::new(Barrier::new(2));

        let running_repository = repository.clone();
        let running_id = run.id.clone();
        let running_barrier = Arc::clone(&barrier);
        let running = tokio::spawn(async move {
            running_repository
                .set_run_status_after_read(&running_id, RunStatus::Running, move || {
                    running_barrier.wait();
                })
                .await
        });
        let stopped_repository = repository.clone();
        let stopped_id = run.id.clone();
        let stopped_barrier = Arc::clone(&barrier);
        let stopped = tokio::spawn(async move {
            stopped_repository
                .set_run_status_after_read(&stopped_id, RunStatus::Stopped, move || {
                    stopped_barrier.wait();
                })
                .await
        });

        let running_result = running.await.unwrap();
        let stopped_result = stopped.await.unwrap();
        let running_succeeded = running_result.is_ok();
        let stopped_succeeded = stopped_result.is_ok();
        assert_eq!(
            usize::from(running_succeeded) + usize::from(stopped_succeeded),
            1
        );
        let error = running_result
            .err()
            .or_else(|| stopped_result.err())
            .unwrap();
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            serde_json::json!({
                "code": "concurrent_modification",
                "message": "Run was modified concurrently",
                "details": { "runId": run.id }
            })
        );

        let persisted = repository
            .get(&work.summary.id)
            .await
            .unwrap()
            .unwrap()
            .runs[0]
            .clone();
        let expected = if running_succeeded {
            RunStatus::Running
        } else {
            RunStatus::Stopped
        };
        assert_eq!(persisted.status, expected);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_work_transitions_use_compare_and_swap() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let database_path = temporary_directory.path().join("work-status-cas.sqlite3");
        let workspace_path = temporary_directory.path().join("workspace");
        std::fs::create_dir(&workspace_path).unwrap();
        let database = Database::open(&database_path).await.unwrap();
        let repository = WorkRepository::new(database.pool().clone());
        let work = repository
            .create(CreateWorkInput {
                title: "Work CAS".into(),
                goal: "Allow one transition".into(),
                root_path: workspace_path.to_string_lossy().into_owned(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap();
        let barrier = Arc::new(Barrier::new(2));

        let queued_repository = repository.clone();
        let queued_id = work.summary.id.clone();
        let queued_barrier = Arc::clone(&barrier);
        let queued = tokio::spawn(async move {
            queued_repository
                .set_work_status_after_read(&queued_id, WorkStatus::Queued, move || {
                    queued_barrier.wait();
                })
                .await
        });
        let archived_repository = repository.clone();
        let archived_id = work.summary.id.clone();
        let archived_barrier = Arc::clone(&barrier);
        let archived = tokio::spawn(async move {
            archived_repository
                .set_work_status_after_read(&archived_id, WorkStatus::Archived, move || {
                    archived_barrier.wait();
                })
                .await
        });

        let queued_result = queued.await.unwrap();
        let archived_result = archived.await.unwrap();
        let queued_succeeded = queued_result.is_ok();
        let archived_succeeded = archived_result.is_ok();
        assert_eq!(
            usize::from(queued_succeeded) + usize::from(archived_succeeded),
            1
        );
        let error = queued_result
            .err()
            .or_else(|| archived_result.err())
            .unwrap();
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            serde_json::json!({
                "code": "concurrent_modification",
                "message": "Work was modified concurrently",
                "details": { "workId": work.summary.id }
            })
        );

        let persisted = repository.get(&work.summary.id).await.unwrap().unwrap();
        let expected = if queued_succeeded {
            WorkStatus::Queued
        } else {
            WorkStatus::Archived
        };
        assert_eq!(persisted.summary.status, expected);
    }
}
