use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{
    domain::{
        event::{WorkEventEnvelope, WorkEventPayload},
        work::{
            CreateWorkInput, PermissionMode, RunStatus, RunSummary, WorkDetail, WorkStatus,
            WorkSummary,
        },
    },
    error::AppError,
};

#[derive(Clone)]
pub struct WorkRepository {
    pool: SqlitePool,
}

impl WorkRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, input: CreateWorkInput) -> Result<WorkDetail, AppError> {
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
        .execute(&self.pool)
        .await?;

        Ok(WorkDetail {
            summary,
            runs: Vec::new(),
            events: Vec::new(),
        })
    }

    pub async fn get(&self, id: &str) -> Result<Option<WorkDetail>, AppError> {
        let row = sqlx::query_as::<_, WorkRow>(
            "SELECT id, title, goal, root_path, permission_mode, status, created_at, updated_at \
             FROM works WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let runs = sqlx::query_as::<_, RunRow>(
            "SELECT id, work_id, model_label, status, created_at, started_at, completed_at \
             FROM runs WHERE work_id = ? ORDER BY created_at ASC, id ASC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(RunSummary::from)
        .collect();
        let events = sqlx::query_as::<_, EventRow>(
            "SELECT events.work_id, events.run_id, events.sequence, events.version, \
                    events.occurred_at, events.payload \
             FROM events \
             INNER JOIN runs ON runs.id = events.run_id AND runs.work_id = events.work_id \
             WHERE events.work_id = ? \
             ORDER BY runs.created_at ASC, runs.id ASC, events.sequence ASC, events.id ASC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(WorkEventEnvelope::try_from)
        .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(WorkDetail {
            summary: row.into(),
            runs,
            events,
        }))
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

    pub async fn insert_run(
        &self,
        work_id: &str,
        model_label: &str,
    ) -> Result<RunSummary, AppError> {
        let now = Utc::now();
        let run = RunSummary {
            id: Uuid::new_v4().to_string(),
            work_id: work_id.to_owned(),
            model_label: model_label.to_owned(),
            status: RunStatus::Queued,
            created_at: now,
            started_at: None,
            completed_at: None,
        };
        let mut transaction = self.pool.begin().await?;
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
             (id, work_id, model_label, status, created_at, updated_at, started_at, completed_at) \
             VALUES (?, ?, ?, ?, ?, ?, NULL, NULL)",
        )
        .bind(&run.id)
        .bind(&run.work_id)
        .bind(&run.model_label)
        .bind(run.status)
        .bind(run.created_at)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;

        Ok(run)
    }

    pub async fn set_work_status(&self, work_id: &str, status: WorkStatus) -> Result<(), AppError> {
        let result = sqlx::query("UPDATE works SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(Utc::now())
            .bind(work_id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::work_not_found(work_id));
        }

        Ok(())
    }

    pub async fn set_run_status(&self, run_id: &str, status: RunStatus) -> Result<(), AppError> {
        let now = Utc::now();
        let result = match status {
            RunStatus::Running => {
                sqlx::query(
                    "UPDATE runs \
                     SET status = ?, updated_at = ?, started_at = COALESCE(started_at, ?), \
                         completed_at = NULL \
                     WHERE id = ?",
                )
                .bind(status)
                .bind(now)
                .bind(now)
                .bind(run_id)
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
                     WHERE id = ?",
                )
                .bind(status)
                .bind(now)
                .bind(now)
                .bind(run_id)
                .execute(&self.pool)
                .await?
            }
            RunStatus::Queued | RunStatus::Waiting => {
                sqlx::query("UPDATE runs SET status = ?, updated_at = ? WHERE id = ?")
                    .bind(status)
                    .bind(now)
                    .bind(run_id)
                    .execute(&self.pool)
                    .await?
            }
        };
        if result.rows_affected() == 0 {
            return Err(AppError::run_not_found(run_id));
        }

        Ok(())
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
    model_label: String,
    status: RunStatus,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
}

impl From<RunRow> for RunSummary {
    fn from(row: RunRow) -> Self {
        Self {
            id: row.id,
            work_id: row.work_id,
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
    work_id: String,
    run_id: String,
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
            work_id: row.work_id,
            run_id: row.run_id,
            sequence,
            occurred_at: row.occurred_at,
            payload,
        })
    }
}
