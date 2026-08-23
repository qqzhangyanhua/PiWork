use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

use super::{
    DocumentRequest, DocumentResult, DocumentRuntime, DocumentRuntimeError, DocumentRuntimeResponse,
};

const DOCUMENT_RUNTIME_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_PROTOCOL_BYTES: usize = 64 * 1024;
const MAX_STDERR_BYTES: usize = 8 * 1024;

#[derive(Clone)]
pub struct ProcessDocumentRuntime {
    derivative_root: Arc<PathBuf>,
    executable: Arc<PathBuf>,
}

impl ProcessDocumentRuntime {
    pub fn new(derivative_root: PathBuf) -> Self {
        let executable = document_runtime_executable();
        Self {
            derivative_root: Arc::new(derivative_root),
            executable: Arc::new(executable),
        }
    }

    pub fn with_executable(derivative_root: PathBuf, executable: PathBuf) -> Self {
        Self {
            derivative_root: Arc::new(derivative_root),
            executable: Arc::new(executable),
        }
    }
}

#[async_trait]
impl DocumentRuntime for ProcessDocumentRuntime {
    async fn extract(
        &self,
        request: DocumentRequest,
    ) -> Result<DocumentResult, DocumentRuntimeError> {
        let mut command = Command::new(self.executable.as_ref());
        command
            .arg(self.derivative_root.as_ref())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command
            .spawn()
            .map_err(|_| DocumentRuntimeError::Unavailable)?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or(DocumentRuntimeError::Unavailable)?;
        let stdout = child
            .stdout
            .take()
            .ok_or(DocumentRuntimeError::Unavailable)?;
        let stderr = child
            .stderr
            .take()
            .ok_or(DocumentRuntimeError::Unavailable)?;
        let request =
            serde_json::to_vec(&request).map_err(|_| DocumentRuntimeError::InvalidRequest)?;
        stdin
            .write_all(&request)
            .await
            .map_err(|_| DocumentRuntimeError::Unavailable)?;
        drop(stdin);
        let stdout_task = tokio::spawn(read_bounded(stdout, MAX_PROTOCOL_BYTES));
        let stderr_task = tokio::spawn(read_bounded(stderr, MAX_STDERR_BYTES));
        let status = match tokio::time::timeout(DOCUMENT_RUNTIME_TIMEOUT, child.wait()).await {
            Ok(Ok(status)) => status,
            Ok(Err(_)) => return Err(DocumentRuntimeError::Unavailable),
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(DocumentRuntimeError::Timeout);
            }
        };
        let stdout = stdout_task
            .await
            .map_err(|_| DocumentRuntimeError::Unavailable)??;
        let _stderr = stderr_task.await;
        let response: DocumentRuntimeResponse =
            serde_json::from_slice(&stdout).map_err(|_| DocumentRuntimeError::Unavailable)?;
        if !status.success() && matches!(response, DocumentRuntimeResponse::Success { .. }) {
            return Err(DocumentRuntimeError::Unavailable);
        }
        match response {
            DocumentRuntimeResponse::Success { result } => Ok(result),
            DocumentRuntimeResponse::Failure { code } => Err(code.into()),
        }
    }
}

fn document_runtime_executable() -> PathBuf {
    let name = if cfg!(windows) {
        "piwork-document-runtime.exe"
    } else {
        "piwork-document-runtime"
    };
    std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(PathBuf::from))
        .map_or_else(
            || PathBuf::from(name),
            |parent| parent.join("document-runtime").join(name),
        )
}

async fn read_bounded(
    stream: impl AsyncRead + Unpin,
    limit: usize,
) -> Result<Vec<u8>, DocumentRuntimeError> {
    let mut bytes = Vec::new();
    stream
        .take(u64::try_from(limit + 1).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| DocumentRuntimeError::Unavailable)?;
    if bytes.len() > limit {
        return Err(DocumentRuntimeError::Unavailable);
    }
    Ok(bytes)
}
