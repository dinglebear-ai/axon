//! Supervised JSONL transport for the dedicated Codex control process.

use crate::control::ControlConfig;
use crate::events::{EventKind, EventRecorder, RecordedEvent};
use crate::protocol::{
    ClientRequest, IncomingMessage, PendingRequests, ProtocolError, RpcError, RuntimeEpoch,
    parse_frame,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle;

const EVENT_CAPACITY: usize = 256;

pub struct ControlTransport {
    epoch: RuntimeEpoch,
    pending: PendingRequests,
    stdin: Arc<Mutex<ChildStdin>>,
    child: Arc<Mutex<Child>>,
    events: broadcast::Sender<RecordedEvent>,
    event_recorder: EventRecorder,
    reader_task: JoinHandle<()>,
    timeout: Duration,
    alive: Arc<AtomicBool>,
    server_requests: Arc<StdMutex<HashMap<u64, PendingServerRequest>>>,
}

impl ControlTransport {
    pub async fn start(config: &ControlConfig, epoch: RuntimeEpoch) -> Result<Self, String> {
        crate::control::validate_config(config)?;
        if !config.enabled {
            return Err("codex control runtime is disabled".to_string());
        }
        let mut command = tokio::process::Command::new(&config.codex_binary);
        command
            .arg("app-server")
            .env_clear()
            .env("HOME", &config.control_home)
            .env("CODEX_HOME", &config.control_home)
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|error| format!("failed to start Codex control app-server: {error}"))?;
        let stdin = Arc::new(Mutex::new(
            child
                .stdin
                .take()
                .ok_or("Codex control stdin unavailable")?,
        ));
        let stdout = child
            .stdout
            .take()
            .ok_or("Codex control stdout unavailable")?;
        let child = Arc::new(Mutex::new(child));
        let pending = PendingRequests::new(epoch);
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        let event_recorder = EventRecorder::new(epoch);
        let alive = Arc::new(AtomicBool::new(true));
        let server_requests = Arc::new(StdMutex::new(HashMap::new()));
        let reader_task = spawn_reader(
            stdout,
            epoch,
            pending.clone(),
            events.clone(),
            event_recorder.clone(),
            Arc::clone(&alive),
            Arc::clone(&server_requests),
        );
        let transport = Self {
            epoch,
            pending,
            stdin,
            child,
            events,
            event_recorder,
            reader_task,
            timeout: config.request_timeout,
            alive,
            server_requests,
        };
        transport.initialize().await?;
        Ok(transport)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RecordedEvent> {
        self.events.subscribe()
    }

    pub fn events_after(
        &self,
        cursor: Option<crate::events::EventCursor>,
        limit: usize,
    ) -> Result<Vec<RecordedEvent>, String> {
        self.event_recorder.after(cursor, limit)
    }

    pub fn epoch(&self) -> RuntimeEpoch {
        self.epoch
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    pub async fn request<P: Serialize>(
        &self,
        method: &'static str,
        params: P,
    ) -> Result<Value, String> {
        let pending = self.pending.register().map_err(protocol_error)?;
        let id = pending.id;
        let request = ClientRequest::new(id, method, params);
        if let Err(error) = self.write_json(&request).await {
            let _ = pending.cancel();
            return Err(error);
        }
        match pending.wait(self.timeout).await.map_err(protocol_error)? {
            Ok(value) => Ok(value),
            Err(error) => Err(rpc_error(error)),
        }
    }

    pub async fn respond_to_server_request(
        &self,
        boot_id: u64,
        request_id: u64,
        approved: bool,
    ) -> Result<(), String> {
        if boot_id != self.epoch.0 {
            return Err("server request belongs to a previous runtime".to_string());
        }
        {
            let mut registry = self
                .server_requests
                .lock()
                .map_err(|_| "server request registry lock poisoned".to_string())?;
            let pending_request = registry
                .get(&request_id)
                .cloned()
                .ok_or("server request is unknown or already answered")?;
            if pending_request.expires_at <= Instant::now() {
                registry.remove(&request_id);
                return Err("server request approval expired".to_string());
            }
            if approved && !approval_decision_supported(&pending_request.method) {
                return Err(format!(
                    "server request {} requires a typed response and cannot be generically approved",
                    pending_request.method
                ));
            }
            registry.remove(&request_id);
        }
        let response = if approved {
            json!({"jsonrpc":"2.0","id":request_id,"result":{"decision":"accept"}})
        } else {
            json!({"jsonrpc":"2.0","id":request_id,"result":{"decision":"decline"}})
        };
        self.write_json(&response).await
    }

    pub async fn stop(self) -> Result<(), String> {
        self.reader_task.abort();
        let mut child = self.child.lock().await;
        child
            .kill()
            .await
            .map_err(|error| format!("failed to stop Codex control app-server: {error}"))?;
        child
            .wait()
            .await
            .map_err(|error| format!("failed to reap Codex control app-server: {error}"))?;
        Ok(())
    }

    async fn initialize(&self) -> Result<(), String> {
        self.request("initialize", json!({
            "clientInfo": { "name": "axon", "title": "Axon Palette", "version": env!("CARGO_PKG_VERSION") },
            "capabilities": { "experimentalApi": true }
        })).await?;
        self.write_json(&json!({"method": "initialized", "params": {}}))
            .await
    }

    async fn write_json<T: Serialize>(&self, value: &T) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(value)
            .map_err(|error| format!("failed to encode Codex request: {error}"))?;
        bytes.push(b'\n');
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(&bytes)
            .await
            .map_err(|error| format!("failed to write Codex request: {error}"))?;
        stdin
            .flush()
            .await
            .map_err(|error| format!("failed to flush Codex request: {error}"))
    }
}

fn spawn_reader(
    stdout: tokio::process::ChildStdout,
    epoch: RuntimeEpoch,
    pending: PendingRequests,
    events: broadcast::Sender<RecordedEvent>,
    recorder: EventRecorder,
    alive: Arc<AtomicBool>,
    server_requests: Arc<StdMutex<HashMap<u64, PendingServerRequest>>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = Vec::new();
            let read = (&mut reader)
                .take((crate::protocol::MAX_FRAME_BYTES + 1) as u64)
                .read_until(b'\n', &mut line)
                .await;
            match read {
                Ok(0) => {
                    let _ = events.send(recorder.record(EventKind::Exited));
                    break;
                }
                Ok(_) if line.len() > crate::protocol::MAX_FRAME_BYTES => {
                    let _ = events.send(recorder.record(EventKind::ProtocolFailure {
                        detail: "Codex control frame exceeded maximum size".to_string(),
                    }));
                    break;
                }
                Ok(_) if line == b"\n" => continue,
                Ok(_) => {
                    if line.last() == Some(&b'\n') {
                        line.pop();
                    }
                    match parse_frame(epoch, &line) {
                        Ok(IncomingMessage::Response { id, result }) => {
                            let _ = pending.resolve(id, result);
                        }
                        Ok(IncomingMessage::Notification { method, params }) => {
                            let _ = events
                                .send(recorder.record(EventKind::Notification { method, params }));
                        }
                        Ok(IncomingMessage::ServerRequest { id, method, params }) => {
                            let mut registry = server_requests
                                .lock()
                                .unwrap_or_else(|value| value.into_inner());
                            registry.insert(
                                id.sequence,
                                PendingServerRequest {
                                    method: method.clone(),
                                    expires_at: Instant::now() + Duration::from_secs(300),
                                },
                            );
                            let _ = events.send(recorder.record(EventKind::ServerRequest {
                                request_id: id.sequence,
                                method,
                                params,
                            }));
                        }
                        Ok(IncomingMessage::Unknown(_)) => {}
                        Err(error) => {
                            let _ = events.send(recorder.record(EventKind::ProtocolFailure {
                                detail: error.to_string(),
                            }));
                        }
                    }
                }
                Err(error) => {
                    let _ = events.send(recorder.record(EventKind::ProtocolFailure {
                        detail: error.to_string(),
                    }));
                    break;
                }
            }
        }
        alive.store(false, Ordering::Release);
        let _ = pending.restart(RuntimeEpoch(epoch.0.saturating_add(1)));
        server_requests
            .lock()
            .unwrap_or_else(|value| value.into_inner())
            .clear();
    })
}

fn approval_decision_supported(method: &str) -> bool {
    method == "applyPatchApproval"
        || method == "execCommandApproval"
        || method.ends_with("requestApproval")
}

#[derive(Clone)]
struct PendingServerRequest {
    method: String,
    expires_at: Instant,
}

fn protocol_error(error: ProtocolError) -> String {
    format!("Codex control protocol error: {error}")
}
fn rpc_error(error: RpcError) -> String {
    format!(
        "Codex control request failed ({}): {}",
        error.code, error.message
    )
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
