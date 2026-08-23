use std::{collections::BTreeSet, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    domain::{agent::RoleKind, work::PermissionMode},
    error::AppError,
};

pub const RUN_CAPABILITY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct RunCapabilityRequest {
    pub run_id: String,
    pub work_id: String,
    pub assignment_id: String,
    pub agent_instance_id: String,
    pub role_kind: RoleKind,
    pub permission_mode: PermissionMode,
    pub workspace_root: PathBuf,
    pub expert_pack_ids: Vec<String>,
    pub host_tool_ids: Vec<String>,
    pub extension_tool_ids: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCapabilitySnapshot {
    pub id: String,
    pub schema_version: u32,
    pub run_id: String,
    pub work_id: String,
    pub assignment_id: String,
    pub agent_instance_id: String,
    pub role_kind: RoleKind,
    pub permission_mode: PermissionMode,
    pub workspace_root: PathBuf,
    pub expert_pack_ids: Vec<String>,
    pub host_tool_ids: Vec<String>,
    pub extension_tool_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl RunCapabilitySnapshot {
    pub(crate) fn compile(request: RunCapabilityRequest) -> Result<Self, AppError> {
        validate_identity("runId", &request.run_id)?;
        validate_identity("workId", &request.work_id)?;
        validate_identity("assignmentId", &request.assignment_id)?;
        validate_identity("agentInstanceId", &request.agent_instance_id)?;

        let workspace_root = dunce::canonicalize(&request.workspace_root).map_err(|source| {
            AppError::WorkspacePathResolution {
                path: request.workspace_root.clone(),
                source,
            }
        })?;
        if !workspace_root.is_dir() {
            return Err(AppError::invalid_input(
                "workspaceRoot",
                "must be an existing directory",
            ));
        }

        let now = Utc::now();
        if request
            .expires_at
            .is_some_and(|expires_at| expires_at <= now)
        {
            return Err(AppError::invalid_input(
                "expiresAt",
                "must be later than snapshot creation",
            ));
        }

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            schema_version: RUN_CAPABILITY_SCHEMA_VERSION,
            run_id: request.run_id,
            work_id: request.work_id,
            assignment_id: request.assignment_id,
            agent_instance_id: request.agent_instance_id,
            role_kind: request.role_kind,
            permission_mode: request.permission_mode,
            workspace_root,
            expert_pack_ids: normalized_ids(request.expert_pack_ids, "expertPackIds")?,
            host_tool_ids: normalized_ids(request.host_tool_ids, "hostToolIds")?,
            extension_tool_ids: normalized_ids(request.extension_tool_ids, "extensionToolIds")?,
            created_at: now,
            expires_at: request.expires_at,
            revoked_at: None,
        })
    }

    pub fn expert_pack_ids(&self) -> &[String] {
        &self.expert_pack_ids
    }

    pub fn host_tool_ids(&self) -> &[String] {
        &self.host_tool_ids
    }

    pub fn extension_tool_ids(&self) -> &[String] {
        &self.extension_tool_ids
    }

    pub fn executable_tool_ids(&self) -> Vec<String> {
        self.host_tool_ids
            .iter()
            .chain(&self.extension_tool_ids)
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

fn normalized_ids(values: Vec<String>, field: &str) -> Result<Vec<String>, AppError> {
    values
        .into_iter()
        .map(|value| {
            let value = value.trim().to_owned();
            validate_identity(field, &value)?;
            Ok(value)
        })
        .collect::<Result<BTreeSet<_>, _>>()
        .map(|values| values.into_iter().collect())
}

fn validate_identity(field: &str, value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.len() > 512 {
        return Err(AppError::invalid_input(
            field,
            "must contain between 1 and 512 characters",
        ));
    }
    Ok(())
}
