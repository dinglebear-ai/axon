//! Bounded, runtime-generation-aware JSON-RPC primitives for Codex app-server.

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::oneshot;

/// Maximum accepted JSON-RPC frame size before parsing.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
/// Maximum nesting accepted in a JSON-RPC frame.
pub const MAX_JSON_DEPTH: usize = 64;
/// Maximum number of simultaneously pending critical requests.
pub const MAX_PENDING_REQUESTS: usize = 256;

/// Identifies one initialized app-server process generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuntimeEpoch(pub u64);

/// Correlation identity that cannot be reused across app-server restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId {
    /// Runtime generation that issued the request.
    pub epoch: RuntimeEpoch,
    /// JSON-RPC numeric request id within the runtime generation.
    pub sequence: u64,
}

/// A typed outgoing JSON-RPC request.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClientRequest<P> {
    jsonrpc: &'static str,
    /// Numeric wire id. The runtime epoch remains an internal correlation key.
    pub id: u64,
    /// Codex app-server method name.
    pub method: &'static str,
    /// Method-specific parameters.
    pub params: P,
}

impl<P> ClientRequest<P> {
    /// Creates a request for a registered correlation identity.
    pub fn new(id: RequestId, method: &'static str, params: P) -> Self {
        Self {
            jsonrpc: "2.0",
            id: id.sequence,
            method,
            params,
        }
    }
}

/// Parsed inbound JSON-RPC message. Unknown methods remain fully represented.
#[derive(Debug, Clone, PartialEq)]
pub enum IncomingMessage {
    /// Response to a request issued in this runtime generation.
    Response {
        id: RequestId,
        result: Result<Value, RpcError>,
    },
    /// Server-initiated request that requires a client response.
    ServerRequest {
        id: RequestId,
        method: String,
        params: Value,
    },
    /// Advisory server notification.
    Notification { method: String, params: Value },
    /// Valid JSON that is not a recognized JSON-RPC envelope.
    Unknown(Value),
}

/// Transport-safe JSON-RPC error payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Protocol boundary failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    FrameTooLarge {
        actual: usize,
        maximum: usize,
    },
    JsonTooDeep {
        actual: usize,
        maximum: usize,
    },
    MalformedJson(String),
    PendingCapacity {
        maximum: usize,
    },
    UnknownRequest(RequestId),
    Timeout(RequestId),
    Cancelled(RequestId),
    RuntimeInterrupted {
        previous: RuntimeEpoch,
        next: RuntimeEpoch,
    },
    LockPoisoned,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ProtocolError {}

type PendingResult = Result<Result<Value, RpcError>, ProtocolError>;
type PendingSender = oneshot::Sender<PendingResult>;

/// O(1) request correlation scoped to one app-server runtime generation.
#[derive(Debug, Clone)]
pub struct PendingRequests {
    inner: Arc<PendingInner>,
}

#[derive(Debug)]
struct PendingInner {
    epoch: AtomicU64,
    next_sequence: AtomicU64,
    senders: Mutex<HashMap<RequestId, PendingSender>>,
}

impl PendingRequests {
    /// Creates an empty correlation registry for `epoch`.
    pub fn new(epoch: RuntimeEpoch) -> Self {
        Self {
            inner: Arc::new(PendingInner {
                epoch: AtomicU64::new(epoch.0),
                next_sequence: AtomicU64::new(1),
                senders: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Current runtime epoch.
    pub fn epoch(&self) -> RuntimeEpoch {
        RuntimeEpoch(self.inner.epoch.load(Ordering::Acquire))
    }

    /// Registers a critical request, enforcing the pending-request budget.
    pub fn register(&self) -> Result<PendingRequest, ProtocolError> {
        let mut senders = self
            .inner
            .senders
            .lock()
            .map_err(|_| ProtocolError::LockPoisoned)?;
        if senders.len() >= MAX_PENDING_REQUESTS {
            return Err(ProtocolError::PendingCapacity {
                maximum: MAX_PENDING_REQUESTS,
            });
        }
        let id = RequestId {
            epoch: self.epoch(),
            sequence: self.inner.next_sequence.fetch_add(1, Ordering::Relaxed),
        };
        let (sender, receiver) = oneshot::channel();
        senders.insert(id, sender);
        Ok(PendingRequest {
            id,
            receiver,
            registry: self.clone(),
        })
    }

    /// Resolves a response. Late or cross-generation responses are rejected.
    pub fn resolve(
        &self,
        id: RequestId,
        result: Result<Value, RpcError>,
    ) -> Result<(), ProtocolError> {
        if id.epoch != self.epoch() {
            return Err(ProtocolError::UnknownRequest(id));
        }
        let sender = self
            .inner
            .senders
            .lock()
            .map_err(|_| ProtocolError::LockPoisoned)?
            .remove(&id)
            .ok_or(ProtocolError::UnknownRequest(id))?;
        let _ = sender.send(Ok(result));
        Ok(())
    }

    /// Interrupts all pending requests and advances to a fresh runtime epoch.
    pub fn restart(&self, next: RuntimeEpoch) -> Result<(), ProtocolError> {
        let previous = self.epoch();
        let mut senders = self
            .inner
            .senders
            .lock()
            .map_err(|_| ProtocolError::LockPoisoned)?;
        self.inner.epoch.store(next.0, Ordering::Release);
        self.inner.next_sequence.store(1, Ordering::Release);
        for (_, sender) in senders.drain() {
            let _ = sender.send(Err(ProtocolError::RuntimeInterrupted { previous, next }));
        }
        Ok(())
    }

    fn cancel(&self, id: RequestId) -> Result<(), ProtocolError> {
        let sender = self
            .inner
            .senders
            .lock()
            .map_err(|_| ProtocolError::LockPoisoned)?
            .remove(&id)
            .ok_or(ProtocolError::UnknownRequest(id))?;
        let _ = sender.send(Err(ProtocolError::Cancelled(id)));
        Ok(())
    }
}

/// Registered request handle with timeout and cancellation cleanup.
#[derive(Debug)]
pub struct PendingRequest {
    pub id: RequestId,
    receiver: oneshot::Receiver<PendingResult>,
    registry: PendingRequests,
}

impl PendingRequest {
    /// Waits for a response and unregisters the request on timeout.
    pub async fn wait(self, timeout: Duration) -> PendingResult {
        let id = self.id;
        match tokio::time::timeout(timeout, self.receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(ProtocolError::RuntimeInterrupted {
                previous: id.epoch,
                next: self.registry.epoch(),
            }),
            Err(_) => {
                let _ = self.registry.cancel(id);
                Err(ProtocolError::Timeout(id))
            }
        }
    }

    /// Cancels the pending request exactly once.
    pub fn cancel(self) -> Result<(), ProtocolError> {
        self.registry.cancel(self.id)
    }
}

/// Parses one bounded JSON-RPC frame for the supplied runtime generation.
pub fn parse_frame(epoch: RuntimeEpoch, bytes: &[u8]) -> Result<IncomingMessage, ProtocolError> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            actual: bytes.len(),
            maximum: MAX_FRAME_BYTES,
        });
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| ProtocolError::MalformedJson(error.to_string()))?;
    let depth = json_depth(&value);
    if depth > MAX_JSON_DEPTH {
        return Err(ProtocolError::JsonTooDeep {
            actual: depth,
            maximum: MAX_JSON_DEPTH,
        });
    }
    Ok(classify(epoch, value))
}

fn classify(epoch: RuntimeEpoch, value: Value) -> IncomingMessage {
    let Some(object) = value.as_object() else {
        return IncomingMessage::Unknown(value);
    };
    let id = object
        .get("id")
        .and_then(Value::as_u64)
        .map(|sequence| RequestId { epoch, sequence });
    if let Some(method) = object.get("method").and_then(Value::as_str) {
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        return match id {
            Some(id) => IncomingMessage::ServerRequest {
                id,
                method: method.to_owned(),
                params,
            },
            None => IncomingMessage::Notification {
                method: method.to_owned(),
                params,
            },
        };
    }
    if let Some(id) = id {
        if let Some(error) = object.get("error") {
            let parsed = serde_json::from_value::<RpcError>(error.clone()).unwrap_or(RpcError {
                code: -32603,
                message: "malformed JSON-RPC error".to_owned(),
                data: None,
            });
            return IncomingMessage::Response {
                id,
                result: Err(parsed),
            };
        }
        if let Some(result) = object.get("result") {
            return IncomingMessage::Response {
                id,
                result: Ok(result.clone()),
            };
        }
    }
    IncomingMessage::Unknown(value)
}

fn json_depth(value: &Value) -> usize {
    match value {
        Value::Array(values) => 1 + values.iter().map(json_depth).max().unwrap_or(0),
        Value::Object(values) => 1 + values.values().map(json_depth).max().unwrap_or(0),
        _ => 1,
    }
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
