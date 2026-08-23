use std::{collections::BTreeSet, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    transport::smtp::{Error as SmtpError, authentication::Credentials},
};
use mail_parser::MessageParser;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use tauri::{AppHandle, Emitter};
use tokio::{net::TcpStream, time::timeout};
use tokio_native_tls::TlsConnector;
use uuid::Uuid;

use crate::{error::AppError, secret};

pub mod commands;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_TICK: Duration = Duration::from_secs(30);
const MAX_POLL_MESSAGES: usize = 200;
const INBOX: &str = "INBOX";
const EMAIL_PERMISSION_METADATA: &str = "metadata";
const EMAIL_PERMISSION_READ_BODY: &str = "read_body";
const EMAIL_PERMISSION_SEND: &str = "send";

pub const TOOL_LIST_EMAIL_ACCOUNTS: &str = "list_email_accounts";
pub const TOOL_SEARCH_EMAIL_METADATA: &str = "search_email_metadata";
pub const TOOL_REQUEST_EMAIL_BODY: &str = "request_email_body";
pub const TOOL_REQUEST_SEND_EMAIL: &str = "request_send_email";

type ImapSession = async_imap::Session<tokio_native_tls::TlsStream<TcpStream>>;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct EmailConnectorSummary {
    pub id: String,
    pub display_name: String,
    pub email_address: String,
    pub username: String,
    pub preset: String,
    pub imap_host: String,
    pub imap_port: i64,
    pub smtp_host: String,
    pub smtp_port: i64,
    pub enabled: bool,
    pub poll_interval_minutes: i64,
    pub health_status: String,
    pub last_error_code: Option<String>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub last_polled_at: Option<DateTime<Utc>>,
    pub credential_configured: bool,
    pub granted_work_ids: Vec<String>,
    pub work_grants: Vec<ConnectorWorkGrantSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorWorkGrantSummary {
    pub work_id: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveEmailConnectorInput {
    pub id: Option<String>,
    pub display_name: String,
    pub email_address: String,
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    pub preset: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub poll_interval_minutes: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorTestResult {
    pub imap_ok: bool,
    pub smtp_ok: bool,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct EmailMetadataSummary {
    pub connection_id: String,
    pub folder: String,
    pub uid: i64,
    pub message_id: Option<String>,
    pub sender_name: Option<String>,
    pub sender_address: String,
    pub subject: String,
    pub sent_at: Option<String>,
    pub received_at: Option<String>,
    pub flags_json: String,
    pub attachment_count: i64,
    pub size_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AppNotificationSummary {
    pub id: String,
    pub category: String,
    pub connection_id: Option<String>,
    pub work_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub action: Value,
    pub read_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PendingConnectorActionSummary {
    pub id: String,
    pub connection_id: String,
    pub work_id: String,
    pub run_id: String,
    pub action_type: String,
    pub preview: Value,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetConnectorWorkGrantInput {
    pub connection_id: String,
    pub work_id: String,
    pub enabled: bool,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, FromRow)]
struct EmailConnectionRow {
    id: String,
    display_name: String,
    email_address: String,
    username: String,
    preset: String,
    imap_host: String,
    imap_port: i64,
    smtp_host: String,
    smtp_port: i64,
    enabled: bool,
    poll_interval_minutes: i64,
    health_status: String,
    last_error_code: Option<String>,
    last_checked_at: Option<DateTime<Utc>>,
    inbox_uid_validity: Option<i64>,
    inbox_last_uid: Option<i64>,
    last_polled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct NotificationRow {
    id: String,
    category: String,
    connection_id: Option<String>,
    work_id: Option<String>,
    title: String,
    summary: String,
    action_json: String,
    read_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct PendingActionRow {
    id: String,
    connection_id: String,
    work_id: String,
    run_id: String,
    action_type: String,
    payload_json: String,
    preview_json: String,
    status: String,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

#[derive(Debug)]
struct FetchedMetadata {
    uid: u32,
    message_id: Option<String>,
    sender_name: Option<String>,
    sender_address: String,
    subject: String,
    sent_at: Option<String>,
    received_at: Option<String>,
    flags: Vec<String>,
    size_bytes: Option<u32>,
}

#[derive(Clone)]
pub struct ConnectorService {
    pool: SqlitePool,
    app: Option<AppHandle>,
}

impl ConnectorService {
    pub fn new(pool: SqlitePool, app: Option<AppHandle>) -> Self {
        Self { pool, app }
    }

    pub fn start_poller(self: &Arc<Self>) {
        let service = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let mut ticker = tokio::time::interval(POLL_TICK);
            loop {
                ticker.tick().await;
                let _ = service.poll_due_connections().await;
            }
        });
    }

    pub async fn list_connections(&self) -> Result<Vec<EmailConnectorSummary>, AppError> {
        let rows = self.connection_rows().await?;
        let grants = sqlx::query_as::<_, (String, String, String)>(
            "SELECT connection_id, work_id, permissions_json FROM connector_work_grants ORDER BY work_id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| EmailConnectorSummary {
                credential_configured: secret::load(&credential_target(&row.id)).is_ok(),
                granted_work_ids: grants
                    .iter()
                    .filter(|(connection_id, _, _)| connection_id == &row.id)
                    .map(|(_, work_id, _)| work_id.clone())
                    .collect(),
                work_grants: grants
                    .iter()
                    .filter(|(connection_id, _, _)| connection_id == &row.id)
                    .map(|(_, work_id, permissions)| ConnectorWorkGrantSummary {
                        work_id: work_id.clone(),
                        permissions: serde_json::from_str(permissions).unwrap_or_default(),
                    })
                    .collect(),
                id: row.id,
                display_name: row.display_name,
                email_address: row.email_address,
                username: row.username,
                preset: row.preset,
                imap_host: row.imap_host,
                imap_port: row.imap_port,
                smtp_host: row.smtp_host,
                smtp_port: row.smtp_port,
                enabled: row.enabled,
                poll_interval_minutes: row.poll_interval_minutes,
                health_status: row.health_status,
                last_error_code: row.last_error_code,
                last_checked_at: row.last_checked_at,
                last_polled_at: row.last_polled_at,
            })
            .collect())
    }

    pub async fn save_connection(
        &self,
        input: SaveEmailConnectorInput,
    ) -> Result<EmailConnectorSummary, AppError> {
        validate_input(&input)?;
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        if input.id.is_some() && self.connection(&id).await?.is_none() {
            return Err(AppError::invalid_input("id", "connector was not found"));
        }
        let password = input
            .password
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if password.is_none() && secret::load(&credential_target(&id)).is_err() {
            return Err(AppError::invalid_input(
                "password",
                "email password is required",
            ));
        }
        if let Some(password) = password {
            secret::store(&credential_target(&id), &input.username, &password).map_err(|_| {
                AppError::Credential {
                    message: "email credential could not be stored".into(),
                }
            })?;
        }
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO connector_connections (
                id, connector_kind, preset, display_name, email_address, username,
                imap_host, imap_port, smtp_host, smtp_port, tls_mode, enabled,
                poll_interval_minutes, health_status, created_at, updated_at
             ) VALUES (?, 'email', ?, ?, ?, ?, ?, ?, ?, ?, 'tls', 0, ?, 'untested', ?, ?)
             ON CONFLICT(id) DO UPDATE SET preset = excluded.preset,
                display_name = excluded.display_name, email_address = excluded.email_address,
                username = excluded.username, imap_host = excluded.imap_host,
                imap_port = excluded.imap_port, smtp_host = excluded.smtp_host,
                smtp_port = excluded.smtp_port,
                poll_interval_minutes = excluded.poll_interval_minutes,
                health_status = 'untested', last_error_code = NULL,
                last_checked_at = NULL, updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(input.preset.trim())
        .bind(input.display_name.trim())
        .bind(input.email_address.trim())
        .bind(input.username.trim())
        .bind(input.imap_host.trim())
        .bind(i64::from(input.imap_port))
        .bind(input.smtp_host.trim())
        .bind(i64::from(input.smtp_port))
        .bind(i64::from(input.poll_interval_minutes))
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        self.connection_summary(&id).await
    }

    pub async fn test_connection(
        &self,
        input: SaveEmailConnectorInput,
    ) -> Result<ConnectorTestResult, AppError> {
        validate_input(&input)?;
        let password = resolve_input_password(&input)?;
        let row = input_row(&input);
        let mut session = match connect_imap(&row, &password).await {
            Ok(session) => session,
            Err(error_code) => return Ok(connection_test_failure(false, error_code)),
        };
        let _ = session.logout().await;
        match test_smtp(&row, &password).await {
            Ok(()) => Ok(ConnectorTestResult {
                imap_ok: true,
                smtp_ok: true,
                error_code: None,
            }),
            Err(error_code) => Ok(connection_test_failure(true, error_code)),
        }
    }

    pub async fn set_enabled(
        &self,
        connection_id: &str,
        enabled: bool,
    ) -> Result<EmailConnectorSummary, AppError> {
        let row = self
            .connection(connection_id)
            .await?
            .ok_or_else(|| AppError::invalid_input("connectionId", "connector was not found"))?;
        if enabled {
            let password = load_password(connection_id)?;
            let mut session = connect_imap(&row, &password)
                .await
                .map_err(|_| AppError::invalid_input("connection", "IMAP TLS connection failed"))?;
            test_smtp(&row, &password)
                .await
                .map_err(|_| AppError::invalid_input("connection", "SMTP TLS connection failed"))?;
            let mailbox = session.select(INBOX).await.map_err(|_| {
                AppError::invalid_input("connection", "inbox could not be selected")
            })?;
            let baseline = i64::from(mailbox.uid_next.unwrap_or(1).saturating_sub(1));
            let uid_validity = mailbox.uid_validity.map(i64::from);
            let _ = session.logout().await;
            sqlx::query(
                "UPDATE connector_connections SET enabled = 1, health_status = 'healthy',
                 last_error_code = NULL, last_checked_at = ?, inbox_uid_validity = ?,
                 inbox_last_uid = COALESCE(inbox_last_uid, ?), last_polled_at = ?, updated_at = ?
                 WHERE id = ?",
            )
            .bind(Utc::now())
            .bind(uid_validity)
            .bind(baseline)
            .bind(Utc::now())
            .bind(Utc::now())
            .bind(connection_id)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query(
                "UPDATE connector_connections SET enabled = 0, updated_at = ? WHERE id = ?",
            )
            .bind(Utc::now())
            .bind(connection_id)
            .execute(&self.pool)
            .await?;
        }
        self.connection_summary(connection_id).await
    }

    pub async fn delete_connection(&self, connection_id: &str) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM connector_connections WHERE id = ?")
            .bind(connection_id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::invalid_input(
                "connectionId",
                "connector was not found",
            ));
        }
        if secret::load(&credential_target(connection_id)).is_ok() {
            secret::delete(&credential_target(connection_id)).map_err(|_| {
                AppError::Credential {
                    message: "email credential could not be removed".into(),
                }
            })?;
        }
        Ok(())
    }

    pub async fn set_work_grant(
        &self,
        input: SetConnectorWorkGrantInput,
    ) -> Result<EmailConnectorSummary, AppError> {
        let allowed = normalize_permissions(input.permissions)?;
        let connection = self
            .connection(&input.connection_id)
            .await?
            .ok_or_else(|| AppError::invalid_input("connectionId", "connector was not found"))?;
        if !connection.enabled && input.enabled {
            return Err(AppError::invalid_input(
                "connectionId",
                "enable the connector before granting it to a Work",
            ));
        }
        let work_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM works WHERE id = ?)")
                .bind(&input.work_id)
                .fetch_one(&self.pool)
                .await?;
        if !work_exists {
            return Err(AppError::work_not_found(input.work_id));
        }
        let workspace_id =
            sqlx::query_scalar::<_, String>("SELECT workspace_id FROM works WHERE id = ?")
                .bind(&input.work_id)
                .fetch_one(&self.pool)
                .await?;
        if input.enabled {
            let permissions_json = serde_json::to_string(&allowed).unwrap_or_else(|_| "[]".into());
            sqlx::query(
                "INSERT INTO connector_work_grants
                 (connection_id, work_id, permissions_json, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT(connection_id, work_id) DO UPDATE SET
                 permissions_json = excluded.permissions_json, updated_at = excluded.updated_at",
            )
            .bind(&input.connection_id)
            .bind(&input.work_id)
            .bind(&permissions_json)
            .bind(Utc::now())
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;
            sqlx::query("INSERT INTO connector_workspace_grants (connection_id, workspace_id, permissions_json, has_conflict, created_at, updated_at) VALUES (?, ?, ?, 0, ?, ?) ON CONFLICT(connection_id, workspace_id) DO UPDATE SET permissions_json = excluded.permissions_json, has_conflict = 0, updated_at = excluded.updated_at")
                .bind(&input.connection_id).bind(&workspace_id).bind(&permissions_json).bind(Utc::now()).bind(Utc::now())
                .execute(&self.pool).await?;
        } else {
            sqlx::query(
                "DELETE FROM connector_work_grants WHERE connection_id = ? AND work_id = ?",
            )
            .bind(&input.connection_id)
            .bind(&input.work_id)
            .execute(&self.pool)
            .await?;
            sqlx::query("DELETE FROM connector_workspace_grants WHERE connection_id = ? AND workspace_id = ?")
                .bind(&input.connection_id).bind(&workspace_id).execute(&self.pool).await?;
        }
        self.connection_summary(&input.connection_id).await
    }

    pub async fn list_metadata(
        &self,
        connection_id: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<EmailMetadataSummary>, AppError> {
        let pattern = format!("%{}%", query.trim());
        let limit = i64::from(limit.clamp(1, 100));
        Ok(sqlx::query_as::<_, EmailMetadataSummary>(
            "SELECT connection_id, folder, uid, message_id, sender_name, sender_address,
             subject, sent_at, received_at, flags_json, attachment_count, size_bytes
             FROM email_metadata WHERE connection_id = ? AND
             (? = '' OR subject LIKE ? OR sender_name LIKE ? OR sender_address LIKE ?)
             ORDER BY COALESCE(received_at, sent_at) DESC, uid DESC LIMIT ?",
        )
        .bind(connection_id)
        .bind(query.trim())
        .bind(&pattern)
        .bind(&pattern)
        .bind(&pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_notifications(
        &self,
        limit: u32,
    ) -> Result<Vec<AppNotificationSummary>, AppError> {
        let rows = sqlx::query_as::<_, NotificationRow>(
            "SELECT id, category, connection_id, work_id, title, summary, action_json,
             read_at, expires_at, created_at FROM app_notifications
             WHERE cleared_at IS NULL AND (expires_at IS NULL OR expires_at > ?)
             ORDER BY created_at DESC LIMIT ?",
        )
        .bind(Utc::now())
        .bind(i64::from(limit.clamp(1, 100)))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(notification_summary).collect())
    }

    pub async fn mark_notification_read(&self, id: &str) -> Result<(), AppError> {
        sqlx::query("UPDATE app_notifications SET read_at = COALESCE(read_at, ?) WHERE id = ?")
            .bind(Utc::now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn clear_notification(&self, id: &str) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE app_notifications SET cleared_at = COALESCE(cleared_at, ?) WHERE id = ?",
        )
        .bind(Utc::now())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_pending_actions(
        &self,
        work_id: Option<&str>,
    ) -> Result<Vec<PendingConnectorActionSummary>, AppError> {
        self.expire_actions().await?;
        let rows = sqlx::query_as::<_, PendingActionRow>(
            "SELECT id, connection_id, work_id, run_id, action_type, payload_json,
             preview_json, status, expires_at, created_at FROM connector_pending_actions
             WHERE status = 'pending' AND (? IS NULL OR work_id = ?)
             ORDER BY created_at DESC",
        )
        .bind(work_id)
        .bind(work_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(pending_summary).collect())
    }

    pub async fn resolve_pending_action(
        &self,
        action_id: &str,
        approve: bool,
    ) -> Result<PendingConnectorActionSummary, AppError> {
        self.expire_actions().await?;
        let status = if approve { "approved" } else { "denied" };
        let updated = sqlx::query(
            "UPDATE connector_pending_actions SET status = ?, resolved_at = ?, updated_at = ?
             WHERE id = ? AND status = 'pending' AND expires_at > ?",
        )
        .bind(status)
        .bind(Utc::now())
        .bind(Utc::now())
        .bind(action_id)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(AppError::invalid_input(
                "actionId",
                "approval is no longer pending",
            ));
        }
        let row = self.pending_action(action_id).await?;
        self.audit(
            Some(&row.connection_id),
            Some(&row.work_id),
            Some(&row.run_id),
            "approval_resolved",
            status,
            json!({ "actionId": action_id, "actionType": row.action_type }),
        )
        .await?;
        Ok(pending_summary(row))
    }

    pub async fn dispatch_host_tool(
        &self,
        tool: &str,
        context: &crate::collaboration::tool_bridge::AuthorizedRunContext,
        arguments: Value,
    ) -> Result<Value, AppError> {
        match tool {
            TOOL_LIST_EMAIL_ACCOUNTS => {
                let accounts = self.enabled_accounts().await?;
                Ok(json!(accounts))
            }
            TOOL_SEARCH_EMAIL_METADATA => {
                let connection_id = required_string(&arguments, "connectionId")?;
                self.authorize(&context.work_id, connection_id, EMAIL_PERMISSION_METADATA)
                    .await?;
                let query = arguments.get("query").and_then(Value::as_str).unwrap_or("");
                Ok(json!(self.list_metadata(connection_id, query, 50).await?))
            }
            TOOL_REQUEST_EMAIL_BODY => self.request_email_body(context, arguments).await,
            TOOL_REQUEST_SEND_EMAIL => self.request_send_email(context, arguments).await,
            _ => Err(AppError::invalid_input("tool", "unknown connector tool")),
        }
    }

    async fn request_email_body(
        &self,
        context: &crate::collaboration::tool_bridge::AuthorizedRunContext,
        arguments: Value,
    ) -> Result<Value, AppError> {
        let connection_id = required_string(&arguments, "connectionId")?;
        self.authorize(&context.work_id, connection_id, EMAIL_PERMISSION_READ_BODY)
            .await?;
        let uid = arguments
            .get("uid")
            .and_then(Value::as_u64)
            .and_then(|uid| u32::try_from(uid).ok())
            .ok_or_else(|| AppError::invalid_input("uid", "valid message UID is required"))?;
        let folder = arguments
            .get("folder")
            .and_then(Value::as_str)
            .unwrap_or(INBOX);
        if !folder.eq_ignore_ascii_case(INBOX) {
            return Err(AppError::invalid_input("folder", "only INBOX is supported"));
        }
        let payload = json!({ "connectionId": connection_id, "folder": INBOX, "uid": uid });
        let preview = self.email_preview(connection_id, uid).await?;
        let action = self
            .get_or_create_action(context, connection_id, "read_email_body", payload, preview)
            .await?;
        match action.status.as_str() {
            "approved" | "executed" => {
                let body = self.fetch_body(connection_id, uid).await?;
                sqlx::query(
                    "UPDATE connector_pending_actions SET status = 'executed', updated_at = ?
                     WHERE id = ? AND status = 'approved'",
                )
                .bind(Utc::now())
                .bind(&action.id)
                .execute(&self.pool)
                .await?;
                self.audit(
                    Some(connection_id),
                    Some(&context.work_id),
                    Some(&context.run_id),
                    "email_body_read",
                    "executed",
                    json!({ "uid": uid, "folder": INBOX }),
                )
                .await?;
                Ok(json!({ "status": "executed", "body": body }))
            }
            "pending" => Ok(
                json!({ "status": "pending", "approvalId": action.id, "expiresAt": action.expires_at }),
            ),
            other => Ok(json!({ "status": other, "approvalId": action.id })),
        }
    }

    async fn request_send_email(
        &self,
        context: &crate::collaboration::tool_bridge::AuthorizedRunContext,
        arguments: Value,
    ) -> Result<Value, AppError> {
        let connection_id = required_string(&arguments, "connectionId")?;
        self.authorize(&context.work_id, connection_id, EMAIL_PERMISSION_SEND)
            .await?;
        let to = required_string(&arguments, "to")?;
        let subject = required_string(&arguments, "subject")?;
        let body = required_string(&arguments, "body")?;
        if body.len() > 200_000 {
            return Err(AppError::invalid_input("body", "email body is too large"));
        }
        let payload =
            json!({ "connectionId": connection_id, "to": to, "subject": subject, "body": body });
        let preview = json!({ "to": to, "subject": subject, "bodyPreview": truncate(body, 240) });
        let action = self
            .get_or_create_action(
                context,
                connection_id,
                "send_email",
                payload.clone(),
                preview,
            )
            .await?;
        match action.status.as_str() {
            "approved" => {
                self.send_email(connection_id, to, subject, body).await?;
                sqlx::query(
                    "UPDATE connector_pending_actions SET status = 'executed', updated_at = ?
                     WHERE id = ? AND status = 'approved'",
                )
                .bind(Utc::now())
                .bind(&action.id)
                .execute(&self.pool)
                .await?;
                self.audit(
                    Some(connection_id),
                    Some(&context.work_id),
                    Some(&context.run_id),
                    "email_sent",
                    "executed",
                    json!({ "to": to, "subject": subject }),
                )
                .await?;
                Ok(json!({ "status": "executed", "actionId": action.id }))
            }
            "pending" => Ok(
                json!({ "status": "pending", "approvalId": action.id, "expiresAt": action.expires_at }),
            ),
            other => Ok(json!({ "status": other, "approvalId": action.id })),
        }
    }

    async fn poll_due_connections(&self) -> Result<(), AppError> {
        let rows = self.connection_rows().await?;
        for row in rows.into_iter().filter(|row| {
            row.enabled
                && row.last_polled_at.is_none_or(|last| {
                    Utc::now() - last >= chrono::Duration::minutes(row.poll_interval_minutes)
                })
        }) {
            if let Err(code) = self.poll_connection(&row).await {
                let was_error = row.health_status == "error";
                sqlx::query(
                    "UPDATE connector_connections SET health_status = 'error', last_error_code = ?,
                     last_checked_at = ?, last_polled_at = ?, updated_at = ? WHERE id = ?",
                )
                .bind(&code)
                .bind(Utc::now())
                .bind(Utc::now())
                .bind(Utc::now())
                .bind(&row.id)
                .execute(&self.pool)
                .await?;
                if !was_error {
                    let notification = self
                        .insert_notification(
                            "connector",
                            Some(&row.id),
                            None,
                            "邮箱连接需要处理",
                            &format!("{} 暂时无法同步，请检查连接设置。", row.display_name),
                            json!({ "view": "connectors", "connectionId": row.id }),
                            None,
                        )
                        .await?;
                    self.emit_notification(&notification);
                }
            }
        }
        Ok(())
    }

    async fn poll_connection(&self, row: &EmailConnectionRow) -> Result<(), String> {
        let password = load_password(&row.id).map_err(|_| "credential_unavailable".to_owned())?;
        let mut session = connect_imap(row, &password).await?;
        let mailbox = timeout(CONNECT_TIMEOUT, session.select(INBOX))
            .await
            .map_err(|_| "imap_timeout".to_owned())?
            .map_err(|_| "imap_protocol".to_owned())?;
        let uid_validity = mailbox.uid_validity.map(i64::from);
        let baseline = i64::from(mailbox.uid_next.unwrap_or(1).saturating_sub(1));
        if row.inbox_uid_validity != uid_validity || row.inbox_last_uid.is_none() {
            sqlx::query(
                "UPDATE connector_connections SET inbox_uid_validity = ?, inbox_last_uid = ?,
                 health_status = 'healthy', last_error_code = NULL, last_checked_at = ?,
                 last_polled_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(uid_validity)
            .bind(baseline)
            .bind(Utc::now())
            .bind(Utc::now())
            .bind(Utc::now())
            .bind(&row.id)
            .execute(&self.pool)
            .await
            .map_err(|_| "storage_failed".to_owned())?;
            let _ = session.logout().await;
            return Ok(());
        }
        let last_uid = row.inbox_last_uid.unwrap_or(0).max(0) as u32;
        let mut uids = timeout(
            CONNECT_TIMEOUT,
            session.uid_search(format!("UID {}:*", last_uid.saturating_add(1))),
        )
        .await
        .map_err(|_| "imap_timeout".to_owned())?
        .map_err(|_| "imap_protocol".to_owned())?
        .into_iter()
        .filter(|uid| *uid > last_uid)
        .collect::<Vec<_>>();
        uids.sort_unstable();
        if uids.len() > MAX_POLL_MESSAGES {
            uids = uids.split_off(uids.len() - MAX_POLL_MESSAGES);
        }
        let fetched = if uids.is_empty() {
            Vec::new()
        } else {
            let sequence = uids
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let stream = timeout(
                CONNECT_TIMEOUT,
                session.uid_fetch(sequence, "(UID FLAGS INTERNALDATE RFC822.SIZE ENVELOPE)"),
            )
            .await
            .map_err(|_| "imap_timeout".to_owned())?
            .map_err(|_| "imap_protocol".to_owned())?;
            timeout(CONNECT_TIMEOUT, stream.try_collect::<Vec<_>>())
                .await
                .map_err(|_| "imap_timeout".to_owned())?
                .map_err(|_| "imap_protocol".to_owned())?
                .into_iter()
                .filter_map(fetch_metadata)
                .collect::<Vec<_>>()
        };
        let _ = session.logout().await;
        self.persist_poll(row, uid_validity, baseline, &fetched)
            .await
            .map_err(|_| "storage_failed".to_owned())?;
        Ok(())
    }

    async fn persist_poll(
        &self,
        row: &EmailConnectionRow,
        uid_validity: Option<i64>,
        baseline: i64,
        fetched: &[FetchedMetadata],
    ) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await?;
        for message in fetched {
            sqlx::query(
                "INSERT INTO email_metadata (
                    connection_id, folder, uid, message_id, sender_name, sender_address,
                    subject, sent_at, received_at, flags_json, attachment_count,
                    size_bytes, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)
                 ON CONFLICT(connection_id, folder, uid) DO UPDATE SET
                    message_id = excluded.message_id, sender_name = excluded.sender_name,
                    sender_address = excluded.sender_address, subject = excluded.subject,
                    sent_at = excluded.sent_at, received_at = excluded.received_at,
                    flags_json = excluded.flags_json, size_bytes = excluded.size_bytes,
                    updated_at = excluded.updated_at",
            )
            .bind(&row.id)
            .bind(INBOX)
            .bind(i64::from(message.uid))
            .bind(&message.message_id)
            .bind(&message.sender_name)
            .bind(&message.sender_address)
            .bind(&message.subject)
            .bind(&message.sent_at)
            .bind(&message.received_at)
            .bind(serde_json::to_string(&message.flags).unwrap_or_else(|_| "[]".into()))
            .bind(message.size_bytes.map(i64::from))
            .bind(Utc::now())
            .execute(&mut *tx)
            .await?;
        }
        let last_uid = fetched
            .iter()
            .map(|message| i64::from(message.uid))
            .max()
            .unwrap_or_else(|| row.inbox_last_uid.unwrap_or(baseline));
        sqlx::query(
            "UPDATE connector_connections SET inbox_uid_validity = ?, inbox_last_uid = ?,
             health_status = 'healthy', last_error_code = NULL, last_checked_at = ?,
             last_polled_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(uid_validity)
        .bind(last_uid)
        .bind(Utc::now())
        .bind(Utc::now())
        .bind(Utc::now())
        .bind(&row.id)
        .execute(&mut *tx)
        .await?;
        let notifications = insert_mail_notifications(&mut tx, row, fetched).await?;
        tx.commit().await?;
        for notification in notifications {
            self.emit_notification(&notification);
        }
        Ok(())
    }

    async fn fetch_body(&self, connection_id: &str, uid: u32) -> Result<String, AppError> {
        let row = self
            .connection(connection_id)
            .await?
            .ok_or_else(|| AppError::invalid_input("connectionId", "connector was not found"))?;
        let password = load_password(connection_id)?;
        let mut session = connect_imap(&row, &password)
            .await
            .map_err(|_| AppError::engine("email body could not be fetched"))?;
        session
            .select(INBOX)
            .await
            .map_err(|_| AppError::engine("email body could not be fetched"))?;
        let stream = session
            .uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")
            .await
            .map_err(|_| AppError::engine("email body could not be fetched"))?;
        let messages = stream
            .try_collect::<Vec<_>>()
            .await
            .map_err(|_| AppError::engine("email body could not be fetched"))?;
        let raw = messages
            .first()
            .and_then(|message| message.body())
            .ok_or_else(|| AppError::engine("email body could not be fetched"))?;
        let parsed = MessageParser::default()
            .parse(raw)
            .ok_or_else(|| AppError::engine("email body could not be parsed"))?;
        let body = parsed
            .body_text(0)
            .or_else(|| parsed.body_html(0))
            .map(|value| value.into_owned())
            .unwrap_or_default();
        let _ = session.logout().await;
        Ok(truncate(&body, 250_000))
    }

    async fn send_email(
        &self,
        connection_id: &str,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), AppError> {
        let row = self
            .connection(connection_id)
            .await?
            .ok_or_else(|| AppError::invalid_input("connectionId", "connector was not found"))?;
        let password = load_password(connection_id)?;
        let message = Message::builder()
            .from(row.email_address.parse().map_err(|_| {
                AppError::invalid_input("emailAddress", "sender email address is invalid")
            })?)
            .to(to
                .parse()
                .map_err(|_| AppError::invalid_input("to", "recipient email address is invalid"))?)
            .subject(subject)
            .body(body.to_owned())
            .map_err(|_| AppError::invalid_input("body", "email could not be composed"))?;
        let transport = smtp_transport(&row, &password)?;
        timeout(CONNECT_TIMEOUT, transport.send(message))
            .await
            .map_err(|_| AppError::engine("email send timed out"))?
            .map_err(|_| AppError::engine("email could not be sent"))?;
        Ok(())
    }

    async fn authorize(
        &self,
        work_id: &str,
        connection_id: &str,
        permission: &str,
    ) -> Result<(), AppError> {
        if !matches!(
            permission,
            EMAIL_PERMISSION_METADATA | EMAIL_PERMISSION_READ_BODY | EMAIL_PERMISSION_SEND
        ) {
            return Err(AppError::invalid_input(
                "permission",
                "unknown connector permission",
            ));
        }
        let enabled =
            sqlx::query_scalar::<_, bool>("SELECT enabled FROM connector_connections WHERE id = ?")
                .bind(connection_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| {
                    AppError::invalid_input("connectionId", "connector was not found")
                })?;
        if !enabled {
            Err(AppError::invalid_input(
                "connectionId",
                "connector is disabled",
            ))
        } else {
            let grant = sqlx::query_as::<_, (String, bool)>(
                "SELECT grants.permissions_json, grants.has_conflict FROM connector_workspace_grants grants INNER JOIN works ON works.workspace_id = grants.workspace_id WHERE grants.connection_id = ? AND works.id = ?",
            )
            .bind(connection_id)
            .bind(work_id)
            .fetch_optional(&self.pool)
            .await?;
            let Some((permissions_json, false)) = grant else {
                return Err(AppError::invalid_input(
                    "permission",
                    "connector is not granted to this Workspace",
                ));
            };
            let permissions =
                serde_json::from_str::<Vec<String>>(&permissions_json).unwrap_or_default();
            if permissions.iter().any(|granted| granted == permission) {
                Ok(())
            } else {
                Err(AppError::invalid_input(
                    "permission",
                    "connector permission is not granted to this Workspace",
                ))
            }
        }
    }

    async fn enabled_accounts(&self) -> Result<Vec<EmailConnectorSummary>, AppError> {
        Ok(self
            .list_connections()
            .await?
            .into_iter()
            .filter(|connection| connection.enabled)
            .collect())
    }

    async fn get_or_create_action(
        &self,
        context: &crate::collaboration::tool_bridge::AuthorizedRunContext,
        connection_id: &str,
        action_type: &str,
        payload: Value,
        preview: Value,
    ) -> Result<PendingActionRow, AppError> {
        let approval_required = email_action_requires_approval(action_type);
        self.expire_actions().await?;
        let payload_json = serde_json::to_string(&payload)
            .map_err(|_| AppError::invalid_input("payload", "action payload is invalid"))?;
        let payload_hash = hex_digest(payload_json.as_bytes());
        let idempotency_key = hex_digest(
            format!(
                "{}:{}:{}:{}:{}",
                context.run_id, context.work_id, connection_id, action_type, payload_hash
            )
            .as_bytes(),
        );
        if let Some(row) = sqlx::query_as::<_, PendingActionRow>(
            "SELECT id, connection_id, work_id, run_id, action_type, payload_json,
             preview_json, status, expires_at, created_at FROM connector_pending_actions
             WHERE idempotency_key = ?",
        )
        .bind(&idempotency_key)
        .fetch_optional(&self.pool)
        .await?
        {
            if approval_required || matches!(row.status.as_str(), "approved" | "executed") {
                return Ok(row);
            }
            sqlx::query(
                "UPDATE connector_pending_actions SET status = 'approved', resolved_at = ?,
                 updated_at = ? WHERE id = ?",
            )
            .bind(Utc::now())
            .bind(Utc::now())
            .bind(&row.id)
            .execute(&self.pool)
            .await?;
            return self.pending_action(&row.id).await;
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::hours(24);
        let status = if approval_required {
            "pending"
        } else {
            "approved"
        };
        let resolved_at = (!approval_required).then_some(now);
        sqlx::query(
            "INSERT INTO connector_pending_actions (
                id, connection_id, work_id, run_id, action_type, payload_json,
                payload_hash, preview_json, idempotency_key, status, expires_at,
                resolved_at, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(connection_id)
        .bind(&context.work_id)
        .bind(&context.run_id)
        .bind(action_type)
        .bind(&payload_json)
        .bind(&payload_hash)
        .bind(serde_json::to_string(&preview).unwrap_or_else(|_| "{}".into()))
        .bind(&idempotency_key)
        .bind(status)
        .bind(expires_at)
        .bind(resolved_at)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        if approval_required {
            let notification = self
                .insert_notification(
                    "approval",
                    Some(connection_id),
                    Some(&context.work_id),
                    "需要确认邮箱操作",
                    "智能体请求执行删除类邮箱操作。",
                    json!({ "approvalId": id, "view": "connectors" }),
                    Some(expires_at),
                )
                .await?;
            self.emit_notification(&notification);
            self.audit(
                Some(connection_id),
                Some(&context.work_id),
                Some(&context.run_id),
                "approval_requested",
                "pending",
                json!({ "actionId": id, "actionType": action_type }),
            )
            .await?;
        }
        self.pending_action(&id).await
    }

    async fn email_preview(&self, connection_id: &str, uid: u32) -> Result<Value, AppError> {
        let row = sqlx::query_as::<_, EmailMetadataSummary>(
            "SELECT connection_id, folder, uid, message_id, sender_name, sender_address,
             subject, sent_at, received_at, flags_json, attachment_count, size_bytes
             FROM email_metadata WHERE connection_id = ? AND folder = ? AND uid = ?",
        )
        .bind(connection_id)
        .bind(INBOX)
        .bind(i64::from(uid))
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::invalid_input("uid", "email metadata was not found"))?;
        Ok(json!({
            "uid": row.uid,
            "senderName": row.sender_name,
            "senderAddress": row.sender_address,
            "subject": row.subject,
            "receivedAt": row.received_at
        }))
    }

    async fn pending_action(&self, id: &str) -> Result<PendingActionRow, AppError> {
        sqlx::query_as::<_, PendingActionRow>(
            "SELECT id, connection_id, work_id, run_id, action_type, payload_json,
             preview_json, status, expires_at, created_at FROM connector_pending_actions WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::invalid_input("actionId", "approval was not found"))
    }

    async fn expire_actions(&self) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE connector_pending_actions SET status = 'expired', updated_at = ?
             WHERE status IN ('pending', 'approved') AND expires_at <= ?",
        )
        .bind(Utc::now())
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn insert_notification(
        &self,
        category: &str,
        connection_id: Option<&str>,
        work_id: Option<&str>,
        title: &str,
        summary: &str,
        action: Value,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<AppNotificationSummary, AppError> {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now();
        sqlx::query(
            "INSERT INTO app_notifications (
                id, category, connection_id, work_id, title, summary,
                action_json, expires_at, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(category)
        .bind(connection_id)
        .bind(work_id)
        .bind(title)
        .bind(summary)
        .bind(serde_json::to_string(&action).unwrap_or_else(|_| "{}".into()))
        .bind(expires_at)
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        Ok(AppNotificationSummary {
            id,
            category: category.into(),
            connection_id: connection_id.map(str::to_owned),
            work_id: work_id.map(str::to_owned),
            title: title.into(),
            summary: summary.into(),
            action,
            read_at: None,
            expires_at,
            created_at,
        })
    }

    fn emit_notification(&self, notification: &AppNotificationSummary) {
        if let Some(app) = &self.app {
            let _ = app.emit("piwork://notification", notification);
        }
    }

    async fn audit(
        &self,
        connection_id: Option<&str>,
        work_id: Option<&str>,
        run_id: Option<&str>,
        event_kind: &str,
        outcome: &str,
        details: Value,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO connector_audit_log (
                id, connection_id, work_id, run_id, event_kind, outcome,
                details_json, created_at, expires_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(connection_id)
        .bind(work_id)
        .bind(run_id)
        .bind(event_kind)
        .bind(outcome)
        .bind(serde_json::to_string(&details).unwrap_or_else(|_| "{}".into()))
        .bind(Utc::now())
        .bind(Utc::now() + chrono::Duration::days(30))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn connection_summary(&self, id: &str) -> Result<EmailConnectorSummary, AppError> {
        self.list_connections()
            .await?
            .into_iter()
            .find(|connection| connection.id == id)
            .ok_or_else(|| AppError::invalid_input("connectionId", "connector was not found"))
    }

    async fn connection_rows(&self) -> Result<Vec<EmailConnectionRow>, AppError> {
        Ok(sqlx::query_as::<_, EmailConnectionRow>(CONNECTION_SELECT)
            .fetch_all(&self.pool)
            .await?)
    }

    async fn connection(&self, id: &str) -> Result<Option<EmailConnectionRow>, AppError> {
        Ok(
            sqlx::query_as::<_, EmailConnectionRow>(&format!("{CONNECTION_SELECT} WHERE id = ?"))
                .bind(id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }
}

const CONNECTION_SELECT: &str = "SELECT id, display_name, email_address, username, preset,
    imap_host, imap_port, smtp_host, smtp_port, enabled, poll_interval_minutes,
    health_status, last_error_code, last_checked_at, inbox_uid_validity,
    inbox_last_uid, last_polled_at FROM connector_connections";

fn validate_input(input: &SaveEmailConnectorInput) -> Result<(), AppError> {
    for (field, value) in [
        ("displayName", input.display_name.as_str()),
        ("emailAddress", input.email_address.as_str()),
        ("username", input.username.as_str()),
        ("imapHost", input.imap_host.as_str()),
        ("smtpHost", input.smtp_host.as_str()),
    ] {
        if value.trim().is_empty() || value.len() > 255 {
            return Err(AppError::invalid_input(field, "value is required"));
        }
    }
    if !input.email_address.contains('@') {
        return Err(AppError::invalid_input(
            "emailAddress",
            "valid email address is required",
        ));
    }
    if input.imap_host.chars().any(char::is_whitespace)
        || input.smtp_host.chars().any(char::is_whitespace)
    {
        return Err(AppError::invalid_input("host", "mail host is invalid"));
    }
    if !matches!(input.poll_interval_minutes, 1 | 2 | 5 | 15) {
        return Err(AppError::invalid_input(
            "pollIntervalMinutes",
            "poll interval must be 1, 2, 5, or 15 minutes",
        ));
    }
    Ok(())
}

fn normalize_permissions(permissions: Vec<String>) -> Result<Vec<String>, AppError> {
    let allowed = [
        EMAIL_PERMISSION_METADATA,
        EMAIL_PERMISSION_READ_BODY,
        EMAIL_PERMISSION_SEND,
    ];
    let normalized = permissions
        .into_iter()
        .map(|permission| permission.trim().to_owned())
        .filter(|permission| !permission.is_empty())
        .collect::<BTreeSet<_>>();
    if normalized
        .iter()
        .any(|item| !allowed.contains(&item.as_str()))
    {
        return Err(AppError::invalid_input(
            "permissions",
            "email connector permission is invalid",
        ));
    }
    Ok(normalized.into_iter().collect())
}

fn input_row(input: &SaveEmailConnectorInput) -> EmailConnectionRow {
    EmailConnectionRow {
        id: input.id.clone().unwrap_or_default(),
        display_name: input.display_name.clone(),
        email_address: input.email_address.clone(),
        username: input.username.clone(),
        preset: input.preset.clone(),
        imap_host: input.imap_host.clone(),
        imap_port: i64::from(input.imap_port),
        smtp_host: input.smtp_host.clone(),
        smtp_port: i64::from(input.smtp_port),
        enabled: false,
        poll_interval_minutes: i64::from(input.poll_interval_minutes),
        health_status: "untested".into(),
        last_error_code: None,
        last_checked_at: None,
        inbox_uid_validity: None,
        inbox_last_uid: None,
        last_polled_at: None,
    }
}

fn resolve_input_password(input: &SaveEmailConnectorInput) -> Result<String, AppError> {
    if let Some(password) = input
        .password
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(password.to_owned());
    }
    let id = input
        .id
        .as_deref()
        .ok_or_else(|| AppError::invalid_input("password", "email password is required"))?;
    load_password(id)
}

fn load_password(connection_id: &str) -> Result<String, AppError> {
    secret::load(&credential_target(connection_id)).map_err(|_| AppError::Credential {
        message: "email credential is unavailable".into(),
    })
}

fn credential_target(connection_id: &str) -> String {
    format!("PiWork/connector/email/{connection_id}")
}

async fn connect_imap(row: &EmailConnectionRow, password: &str) -> Result<ImapSession, String> {
    let port = u16::try_from(row.imap_port).map_err(|_| "imap_configuration".to_owned())?;
    let tcp = timeout(CONNECT_TIMEOUT, TcpStream::connect((&*row.imap_host, port)))
        .await
        .map_err(|_| "imap_timeout".to_owned())?
        .map_err(|_| "imap_network".to_owned())?;
    let native = native_tls::TlsConnector::builder()
        .build()
        .map_err(|_| "imap_tls".to_owned())?;
    let tls = timeout(
        CONNECT_TIMEOUT,
        TlsConnector::from(native).connect(&row.imap_host, tcp),
    )
    .await
    .map_err(|_| "imap_timeout".to_owned())?
    .map_err(|_| "imap_tls".to_owned())?;
    let mut client = async_imap::Client::new(tls);
    timeout(CONNECT_TIMEOUT, client.read_response())
        .await
        .map_err(|_| "imap_timeout".to_owned())?
        .map_err(|_| "imap_protocol".to_owned())?
        .ok_or_else(|| "imap_protocol".to_owned())?;
    timeout(CONNECT_TIMEOUT, client.login(&row.username, password))
        .await
        .map_err(|_| "imap_timeout".to_owned())?
        .map_err(|_| "imap_authentication".to_owned())
}

fn smtp_transport(
    row: &EmailConnectionRow,
    password: &str,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, AppError> {
    let port = u16::try_from(row.smtp_port)
        .map_err(|_| AppError::invalid_input("smtpPort", "SMTP port is invalid"))?;
    Ok(AsyncSmtpTransport::<Tokio1Executor>::relay(&row.smtp_host)
        .map_err(|_| AppError::invalid_input("smtpHost", "SMTP TLS host is invalid"))?
        .port(port)
        .credentials(Credentials::new(row.username.clone(), password.to_owned()))
        .build())
}

async fn test_smtp(row: &EmailConnectionRow, password: &str) -> Result<(), String> {
    let transport = smtp_transport(row, password).map_err(|_| "smtp_configuration".to_owned())?;
    let connected = timeout(CONNECT_TIMEOUT, transport.test_connection())
        .await
        .map_err(|_| "smtp_timeout".to_owned())?
        .map_err(|error| smtp_error_code(&error).to_owned())?;
    if connected {
        Ok(())
    } else {
        Err("smtp_connection".into())
    }
}

fn smtp_error_code(error: &SmtpError) -> &'static str {
    if error.is_timeout() {
        "smtp_timeout"
    } else if error.is_tls() {
        "smtp_tls"
    } else if error.is_permanent() {
        "smtp_authentication"
    } else {
        "smtp_connection"
    }
}

fn connection_test_failure(imap_ok: bool, error_code: String) -> ConnectorTestResult {
    ConnectorTestResult {
        imap_ok,
        smtp_ok: false,
        error_code: Some(error_code),
    }
}

fn fetch_metadata(fetch: async_imap::types::Fetch) -> Option<FetchedMetadata> {
    let uid = fetch.uid?;
    let envelope = fetch.envelope()?;
    let sender = envelope.from.as_ref()?.first()?;
    let mailbox = sender.mailbox.as_deref().map(lossy).unwrap_or_default();
    let host = sender.host.as_deref().map(lossy).unwrap_or_default();
    let sender_address = if mailbox.is_empty() || host.is_empty() {
        String::new()
    } else {
        format!("{mailbox}@{host}")
    };
    let received_at = fetch.internal_date().map(|date| date.to_rfc3339());
    Some(FetchedMetadata {
        uid,
        message_id: envelope.message_id.as_deref().map(lossy),
        sender_name: sender
            .name
            .as_deref()
            .map(lossy)
            .filter(|name| !name.is_empty()),
        sender_address,
        subject: envelope
            .subject
            .as_deref()
            .map(lossy)
            .filter(|subject| !subject.is_empty())
            .unwrap_or_else(|| "(无主题)".into()),
        sent_at: envelope.date.as_deref().map(lossy),
        received_at,
        flags: fetch.flags().map(imap_flag_name).collect(),
        size_bytes: fetch.size,
    })
}

fn imap_flag_name(flag: async_imap::types::Flag<'_>) -> String {
    use async_imap::types::Flag;

    match flag {
        Flag::Seen => "\\Seen".into(),
        Flag::Answered => "\\Answered".into(),
        Flag::Flagged => "\\Flagged".into(),
        Flag::Deleted => "\\Deleted".into(),
        Flag::Draft => "\\Draft".into(),
        Flag::Recent => "\\Recent".into(),
        Flag::MayCreate => "\\*".into(),
        Flag::Custom(value) => value.into_owned(),
    }
}

async fn insert_mail_notifications(
    tx: &mut Transaction<'_, Sqlite>,
    row: &EmailConnectionRow,
    fetched: &[FetchedMetadata],
) -> Result<Vec<AppNotificationSummary>, AppError> {
    if fetched.is_empty() {
        return Ok(Vec::new());
    }
    let created_at = Utc::now();
    let mut notifications = Vec::new();
    let entries = if fetched.len() > 5 {
        vec![(
            format!("{} 封新邮件", fetched.len()),
            format!("{} 收到了 {} 封新邮件。", row.display_name, fetched.len()),
            json!({ "view": "connectors", "connectionId": row.id }),
        )]
    } else {
        fetched
            .iter()
            .map(|message| {
                (
                    message.subject.clone(),
                    if message.sender_address.is_empty() {
                        "收到一封新邮件".to_owned()
                    } else {
                        format!("来自 {}", message.sender_address)
                    },
                    json!({ "view": "connectors", "connectionId": row.id, "uid": message.uid }),
                )
            })
            .collect()
    };
    for (title, summary, action) in entries {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO app_notifications (
                id, category, connection_id, title, summary, action_json, created_at
             ) VALUES (?, 'mail', ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&row.id)
        .bind(&title)
        .bind(&summary)
        .bind(serde_json::to_string(&action).unwrap_or_else(|_| "{}".into()))
        .bind(created_at)
        .execute(&mut **tx)
        .await?;
        notifications.push(AppNotificationSummary {
            id,
            category: "mail".into(),
            connection_id: Some(row.id.clone()),
            work_id: None,
            title,
            summary,
            action,
            read_at: None,
            expires_at: None,
            created_at,
        });
    }
    Ok(notifications)
}

fn notification_summary(row: NotificationRow) -> AppNotificationSummary {
    AppNotificationSummary {
        id: row.id,
        category: row.category,
        connection_id: row.connection_id,
        work_id: row.work_id,
        title: row.title,
        summary: row.summary,
        action: serde_json::from_str(&row.action_json).unwrap_or_else(|_| json!({})),
        read_at: row.read_at,
        expires_at: row.expires_at,
        created_at: row.created_at,
    }
}

fn pending_summary(row: PendingActionRow) -> PendingConnectorActionSummary {
    let _ = row.payload_json;
    PendingConnectorActionSummary {
        id: row.id,
        connection_id: row.connection_id,
        work_id: row.work_id,
        run_id: row.run_id,
        action_type: row.action_type,
        preview: serde_json::from_str(&row.preview_json).unwrap_or_else(|_| json!({})),
        status: row.status,
        expires_at: row.expires_at,
        created_at: row.created_at,
    }
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, AppError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::invalid_input(field, "value is required"))
}

fn lossy(value: &[u8]) -> String {
    String::from_utf8_lossy(value).trim().to_owned()
}

fn hex_digest(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn truncate(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &value[..end])
}

fn email_action_requires_approval(action_type: &str) -> bool {
    action_type.starts_with("delete_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{collaboration::tool_bridge::AuthorizedRunContext, storage::sqlite::Database};

    async fn test_service() -> (Database, ConnectorService) {
        let database = Database::open_in_memory().await.unwrap();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO works (
                id, title, goal, root_path, permission_mode, status, created_at, updated_at
             ) VALUES ('work-1', 'Email work', 'Handle email', '', 'balanced', 'idle', ?, ?)",
        )
        .bind(now)
        .bind(now)
        .execute(database.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO runs (
                id, work_id, engine_kind, model_label, status, created_at, updated_at
             ) VALUES ('run-1', 'work-1', 'pi', 'test', 'running', ?, ?)",
        )
        .bind(now)
        .bind(now)
        .execute(database.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO connector_connections (
                id, connector_kind, preset, display_name, email_address, username,
                imap_host, imap_port, smtp_host, smtp_port, tls_mode, enabled,
                poll_interval_minutes, health_status, created_at, updated_at
             ) VALUES (
                'connector-1', 'email', 'aliyun-enterprise', 'Company mail',
                'owner@example.com', 'owner@example.com', 'imap.qiye.aliyun.com', 993,
                'smtp.qiye.aliyun.com', 465, 'tls', 1, 2, 'healthy', ?, ?
             )",
        )
        .bind(now)
        .bind(now)
        .execute(database.pool())
        .await
        .unwrap();
        let workspace_id: String =
            sqlx::query_scalar("SELECT workspace_id FROM works WHERE id = 'work-1'")
                .fetch_one(database.pool())
                .await
                .unwrap();
        sqlx::query("INSERT INTO connector_workspace_grants (connection_id, workspace_id, permissions_json, has_conflict, created_at, updated_at) VALUES ('connector-1', ?, '[\"metadata\",\"read_body\",\"send\"]', 0, ?, ?)")
            .bind(workspace_id).bind(now).bind(now).execute(database.pool()).await.unwrap();
        let service = ConnectorService::new(database.pool().clone(), None);
        (database, service)
    }

    fn run_context() -> AuthorizedRunContext {
        AuthorizedRunContext {
            capability_snapshot_id: None,
            run_id: "run-1".into(),
            work_id: "work-1".into(),
            assignment_id: "assignment-1".into(),
            agent_instance_id: "agent-1".into(),
            runtime_owner: "test-owner".into(),
            allowed_tools: vec![TOOL_REQUEST_SEND_EMAIL.into()],
        }
    }

    #[test]
    fn permissions_are_restricted_to_the_email_contract() {
        assert_eq!(
            normalize_permissions(vec!["send".into(), "metadata".into(), "send".into()]).unwrap(),
            vec!["metadata", "send"]
        );
        assert!(normalize_permissions(vec!["filesystem".into()]).is_err());
    }

    #[test]
    fn body_preview_truncation_preserves_utf8_boundaries() {
        assert_eq!(truncate("企业邮箱", 4), "企...");
    }

    #[test]
    fn only_destructive_email_actions_require_approval() {
        assert!(!email_action_requires_approval("read_email_body"));
        assert!(!email_action_requires_approval("send_email"));
        assert!(email_action_requires_approval("delete_email"));
    }

    #[test]
    fn connection_test_failure_preserves_the_transport_error_code() {
        let imap = connection_test_failure(false, "imap_authentication".into());
        assert!(!imap.imap_ok);
        assert_eq!(imap.error_code.as_deref(), Some("imap_authentication"));

        let smtp = connection_test_failure(true, "smtp_tls".into());
        assert!(smtp.imap_ok);
        assert_eq!(smtp.error_code.as_deref(), Some("smtp_tls"));
    }

    #[tokio::test]
    async fn enabled_connectors_require_an_explicit_workspace_grant() {
        let (database, service) = test_service().await;
        for permission in [
            EMAIL_PERMISSION_METADATA,
            EMAIL_PERMISSION_READ_BODY,
            EMAIL_PERMISSION_SEND,
        ] {
            service
                .authorize("work-1", "connector-1", permission)
                .await
                .unwrap();
        }
        assert!(
            service
                .authorize("future-work", "connector-1", EMAIL_PERMISSION_METADATA)
                .await
                .is_err()
        );

        sqlx::query("UPDATE connector_connections SET enabled = 0 WHERE id = 'connector-1'")
            .execute(database.pool())
            .await
            .unwrap();
        assert!(
            service
                .authorize("work-1", "connector-1", EMAIL_PERMISSION_METADATA)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn non_destructive_actions_are_auto_authorized_and_delete_actions_require_approval() {
        let (database, service) = test_service().await;
        let payload = json!({
            "connectionId": "connector-1",
            "to": "customer@example.com",
            "subject": "Contract",
            "body": "Please review the contract"
        });
        let preview = json!({
            "to": "customer@example.com",
            "subject": "Contract",
            "bodyPreview": "Please review the contract"
        });

        let first = service
            .get_or_create_action(
                &run_context(),
                "connector-1",
                "send_email",
                payload.clone(),
                preview.clone(),
            )
            .await
            .unwrap();
        let second = service
            .get_or_create_action(
                &run_context(),
                "connector-1",
                "send_email",
                payload,
                preview,
            )
            .await
            .unwrap();
        assert_eq!(first.status, "approved");
        assert_eq!(first.id, second.id);

        let delete_action = service
            .get_or_create_action(
                &run_context(),
                "connector-1",
                "delete_email",
                json!({ "connectionId": "connector-1", "uid": 42 }),
                json!({ "subject": "Old message" }),
            )
            .await
            .unwrap();
        assert_eq!(delete_action.status, "pending");
        let notification_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM app_notifications WHERE category = 'approval'",
        )
        .fetch_one(database.pool())
        .await
        .unwrap();
        assert_eq!(notification_count, 1);

        service
            .resolve_pending_action(&delete_action.id, true)
            .await
            .unwrap();
        assert!(
            service
                .resolve_pending_action(&delete_action.id, false)
                .await
                .is_err()
        );
        sqlx::query("UPDATE connector_pending_actions SET expires_at = ? WHERE id = ?")
            .bind(Utc::now() - chrono::Duration::seconds(1))
            .bind(&delete_action.id)
            .execute(database.pool())
            .await
            .unwrap();

        service.list_pending_actions(None).await.unwrap();
        let status: String =
            sqlx::query_scalar("SELECT status FROM connector_pending_actions WHERE id = ?")
                .bind(&delete_action.id)
                .fetch_one(database.pool())
                .await
                .unwrap();
        assert_eq!(status, "expired");
    }

    #[tokio::test]
    async fn more_than_five_new_messages_create_one_aggregate_notification() {
        let (database, service) = test_service().await;
        let row = service.connection("connector-1").await.unwrap().unwrap();
        let fetched = (1..=6)
            .map(|uid| FetchedMetadata {
                uid,
                message_id: None,
                sender_name: None,
                sender_address: format!("sender-{uid}@example.com"),
                subject: format!("Message {uid}"),
                sent_at: None,
                received_at: None,
                flags: Vec::new(),
                size_bytes: None,
            })
            .collect::<Vec<_>>();
        let mut tx = database.pool().begin().await.unwrap();
        let notifications = insert_mail_notifications(&mut tx, &row, &fetched)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].title, "6 封新邮件");
        assert_eq!(service.list_notifications(10).await.unwrap().len(), 1);
    }
}
