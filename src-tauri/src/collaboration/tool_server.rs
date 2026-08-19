//! Minimal loopback HTTP transport for the Host Tool Bridge.
//!
//! Binds only to 127.0.0.1 on an ephemeral port and accepts exactly one JSON
//! POST shape from the Pi extension: `{ "runId", "token", "tool", "arguments" }`.
//! The token is authenticated against the `HostToolRegistry` (constant-time),
//! and the authorized tool call is forwarded to a caller-provided dispatcher.

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use serde_json::Value;

use crate::{
    collaboration::tool_bridge::{AuthorizedRunContext, HostToolRegistry},
    error::AppError,
};

/// Dispatches an authenticated tool call. Implementations bridge to the async
/// Lead/Member tool services (typically via `tokio::runtime::Handle::block_on`).
pub type ToolDispatch =
    dyn Fn(&str, &AuthorizedRunContext, Value) -> Result<Value, AppError> + Send + Sync;

const MAX_REQUEST_BYTES: usize = 256 * 1024;

pub struct HostToolServer {
    endpoint: String,
    shutdown: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl HostToolServer {
    /// Binds a loopback server and serves until dropped.
    pub fn bind(
        registry: Arc<HostToolRegistry>,
        dispatch: Arc<ToolDispatch>,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let endpoint = format!("http://{}/tool", listener.local_addr()?);
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let join = std::thread::spawn(move || {
            serve_loop(listener, registry, dispatch, thread_shutdown);
        });
        Ok(Self {
            endpoint,
            shutdown,
            join: Some(join),
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl Drop for HostToolServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn serve_loop(
    listener: TcpListener,
    registry: Arc<HostToolRegistry>,
    dispatch: Arc<ToolDispatch>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _addr)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                let _ = serve_one(stream, &registry, &dispatch);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn serve_one(
    mut stream: TcpStream,
    registry: &Arc<HostToolRegistry>,
    dispatch: &Arc<ToolDispatch>,
) -> std::io::Result<()> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_REQUEST_BYTES {
            break;
        }
        if let Some(body) = complete_body(&buffer) {
            let response = handle_body(body, registry, dispatch);
            stream.write_all(response.as_bytes())?;
            return Ok(());
        }
    }
    stream.write_all(respond(400, json_error("malformed request")).as_bytes())?;
    Ok(())
}

fn complete_body(buffer: &[u8]) -> Option<&[u8]> {
    let header_end = find_bytes(buffer, b"\r\n\r\n")?;
    let headers = std::str::from_utf8(&buffer[..header_end]).ok()?;
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let body_start = header_end + 4;
    (buffer.len() >= body_start + content_length).then(|| &buffer[body_start..body_start + content_length])
}

fn handle_body(
    body: &[u8],
    registry: &Arc<HostToolRegistry>,
    dispatch: &Arc<ToolDispatch>,
) -> String {
    let Ok(request) = serde_json::from_slice::<Value>(body) else {
        return respond(400, json_error("invalid json body"));
    };
    let run_id = request.get("runId").and_then(Value::as_str);
    let token = request.get("token").and_then(Value::as_str);
    let tool = request.get("tool").and_then(Value::as_str);
    let arguments = request.get("arguments").cloned().unwrap_or(Value::Null);

    let (Some(run_id), Some(token), Some(tool)) = (run_id, token, tool) else {
        return respond(400, json_error("runId, token and tool are required"));
    };
    let Some(token_bytes) = decode_hex(token) else {
        return respond(401, json_error("invalid token encoding"));
    };
    if !registry.authenticate(run_id, &token_bytes) {
        return respond(401, json_error("unauthorized"));
    }
    let Some(context) = registry.context(run_id) else {
        return respond(401, json_error("run context not found"));
    };
    if !context.allowed_tools.iter().any(|allowed| allowed == tool) {
        return respond(403, json_error("tool not authorized for this run"));
    }
    match dispatch(tool, &context, arguments) {
        Ok(value) => respond(200, &serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_owned())),
        Err(error) => respond(500, &serde_json::to_string(&json_error(&error.to_string())).unwrap()),
    }
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).ok())
        .collect()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn json_error(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

fn respond(status: u16, body: impl AsRef<str>) -> String {
    let body = body.as_ref();
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        _ => "Internal Server Error",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::collaboration::tool_bridge::{AuthorizedRunContext, HostToolRegistry};

    #[test]
    fn loopback_server_authenticates_and_dispatches() {
        let registry = Arc::new(HostToolRegistry::new());
        let lease = registry.issue(
            AuthorizedRunContext {
                run_id: "run-1".into(),
                work_id: "work-1".into(),
                assignment_id: "a1".into(),
                agent_instance_id: "agent-1".into(),
                runtime_owner: "owner-1".into(),
                allowed_tools: vec!["delegate_assignment".into()],
            },
            "http://127.0.0.1:0/tool".into(),
        );

        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_clone = Arc::clone(&seen);
        let dispatch: Arc<ToolDispatch> = Arc::new(move |tool, _context, _args| {
            seen_clone.lock().unwrap().push(tool.to_owned());
            Ok(serde_json::json!({ "ok": true }))
        });

        let server = HostToolServer::bind(Arc::clone(&registry), dispatch).unwrap();
        let endpoint = server.endpoint().to_owned();

        // Send a valid request over a raw TCP connection.
        let body = serde_json::json!({
            "runId": "run-1",
            "token": lease.token.to_hex(),
            "tool": "delegate_assignment",
            "arguments": { "title": "T" }
        });
        let request = format!(
            "POST /tool HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.to_string().len(),
            body
        );
        let addr = endpoint.strip_prefix("http://").unwrap().strip_suffix("/tool").unwrap();
        let mut stream = std::net::TcpStream::connect(addr).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains("200 OK"));
        assert!(response.contains("\"ok\":true"));

        assert_eq!(seen.lock().unwrap().as_slice(), ["delegate_assignment"]);
        drop(server);
    }

    #[test]
    fn loopback_server_rejects_a_forged_token() {
        let registry = Arc::new(HostToolRegistry::new());
        registry.issue(
            AuthorizedRunContext {
                run_id: "run-1".into(),
                work_id: "work-1".into(),
                assignment_id: "a1".into(),
                agent_instance_id: "agent-1".into(),
                runtime_owner: "owner-1".into(),
                allowed_tools: vec!["delegate_assignment".into()],
            },
            "http://127.0.0.1:0/tool".into(),
        );
        let dispatch: Arc<ToolDispatch> = Arc::new(|_tool, _context, _args| {
            Ok(serde_json::json!({ "ok": true }))
        });
        let server = HostToolServer::bind(registry, dispatch).unwrap();
        let endpoint = server.endpoint().to_owned();

        let body = serde_json::json!({
            "runId": "run-1",
            "token": "00".repeat(32),
            "tool": "delegate_assignment",
            "arguments": {}
        });
        let request = format!(
            "POST /tool HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.to_string().len(),
            body
        );
        let addr = endpoint.strip_prefix("http://").unwrap().strip_suffix("/tool").unwrap();
        let mut stream = std::net::TcpStream::connect(addr).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains("401 Unauthorized"));
        drop(server);
    }
}
