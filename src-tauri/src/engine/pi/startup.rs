//! Pi startup protocol gate.
//!
//! This module only reads through prompt acceptance and buffers a bounded set
//! of pre-acceptance events. It never publishes to the caller's bounded sink;
//! lifecycle acknowledgement and ordered publication are separate concerns.

use serde_json::Value;

use super::{
    EngineError, EngineEvent, MAX_PRE_ACCEPTANCE_EVENTS, PI_STARTUP_EXITED_DIAGNOSTIC,
    PI_STARTUP_REJECTED_DIAGNOSTIC, PI_STARTUP_TIMEOUT_DIAGNOSTIC, PiRpcRecordReader,
    RPC_START_TIMEOUT, RpcEventTranslator, startup_rpc_record_error,
};

pub(super) async fn await_prompt_acceptance(
    stdout: &mut PiRpcRecordReader,
    request_id: &str,
    translator: &mut RpcEventTranslator,
) -> Result<Vec<EngineEvent>, EngineError> {
    tokio::time::timeout(RPC_START_TIMEOUT, async {
        let mut buffered_events = Vec::new();
        loop {
            let record = stdout
                .next_record()
                .await
                .map_err(startup_rpc_record_error)?;
            let Some(record) = record else {
                return Err(EngineError::Start(PI_STARTUP_EXITED_DIAGNOSTIC.into()));
            };
            let message: Value = serde_json::from_str(&record)
                .map_err(|_| EngineError::Start("Pi RPC returned malformed JSON".into()))?;
            if message.get("type").and_then(Value::as_str) == Some("response")
                && message.get("id").and_then(Value::as_str) == Some(request_id)
            {
                if message.get("success").and_then(Value::as_bool) == Some(true) {
                    return Ok(buffered_events);
                }
                return Err(EngineError::Start(PI_STARTUP_REJECTED_DIAGNOSTIC.into()));
            }
            if let Some(event) = translator.translate(message) {
                if event.is_terminal() {
                    return Err(EngineError::Start(
                        "Pi failed before accepting the Run".into(),
                    ));
                }
                if buffered_events.len() == MAX_PRE_ACCEPTANCE_EVENTS {
                    return Err(EngineError::Start(
                        "Pi emitted too many events before accepting the Run".into(),
                    ));
                }
                buffered_events.push(event);
            }
        }
    })
    .await
    .map_err(|_| EngineError::Start(PI_STARTUP_TIMEOUT_DIAGNOSTIC.into()))?
}
