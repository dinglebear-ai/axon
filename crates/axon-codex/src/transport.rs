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
const MAX_PENDING_SERVER_REQUESTS: usize = 128;
const STDERR_CHUNK_BYTES: usize = 4096;

#[path = "transport/diagnostics.rs"]
mod diagnostics;
#[cfg(test)]
use diagnostics::redact_stderr;
use diagnostics::{emit_protocol_failure, spawn_stderr_reader};
#[path = "transport/server_requests.rs"]
mod server_requests;
use server_requests::{
    PendingServerRequest, claim_server_request, finish_server_request, server_request_result,
};

pub struct ControlTransport {
    epoch: RuntimeEpoch,
    pending: PendingRequests,
    stdin: Arc<Mutex<ChildStdin>>,
    child: Arc<Mutex<Child>>,
    events: broadcast::Sender<RecordedEvent>,
    event_recorder: EventRecorder,
    reader_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
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
            .stderr(Stdio::piped())
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
        let stderr = child
            .stderr
            .take()
            .ok_or("Codex control stderr unavailable")?;
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
            Arc::clone(&stdin),
        );
        let stderr_task = spawn_stderr_reader(stderr, events.clone(), event_recorder.clone());
        let transport = Self {
            epoch,
            pending,
            stdin,
            child,
            events,
            event_recorder,
            reader_task,
            stderr_task,
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

    pub async fn pending_server_requests(&self) -> Result<Vec<RecordedEvent>, String> {
        self.reject_expired_server_requests().await?;
        let registry = self
            .server_requests
            .lock()
            .map_err(|_| "server request registry lock poisoned".to_string())?;
        let mut pending = registry
            .values()
            .map(|request| request.event.clone())
            .collect::<Vec<_>>();
        pending.sort_by_key(|event| event.cursor.sequence);
        Ok(pending)
    }

    async fn reject_expired_server_requests(&self) -> Result<(), String> {
        let expired = {
            let mut registry = self
                .server_requests
                .lock()
                .map_err(|_| "server request registry lock poisoned".to_string())?;
            let now = Instant::now();
            let expired = registry
                .iter()
                .filter(|(_, request)| request.expires_at <= now && !request.claimed)
                .map(|(id, request)| (*id, request.method.clone()))
                .collect::<Vec<_>>();
            for (id, _) in &expired {
                registry.remove(id);
            }
            expired
        };
        for (id, method) in expired {
            let result = server_request_result(&method, false, None)?;
            if let Err(error) = write_json_to(
                &self.stdin,
                &json!({"jsonrpc":"2.0","id":id,"result":result}),
            )
            .await
            {
                self.alive.store(false, Ordering::Release);
                return Err(error);
            }
        }
        Ok(())
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
            self.alive.store(false, Ordering::Release);
            return Err(error);
        }
        let response = pending.wait(self.timeout).await.map_err(|error| {
            self.alive.store(false, Ordering::Release);
            protocol_error(error)
        })?;
        match response {
            Ok(value) => Ok(value),
            Err(error) => Err(rpc_error(error)),
        }
    }

    pub async fn respond_to_server_request(
        &self,
        boot_id: u64,
        request_id: u64,
        approved: bool,
        typed_response: Option<Value>,
    ) -> Result<(), String> {
        if boot_id != self.epoch.0 {
            return Err("server request belongs to a previous runtime".to_string());
        }
        let method = self.server_request_method(request_id)?;
        let result = server_request_result(&method, approved, typed_response)?;
        {
            let mut registry = self
                .server_requests
                .lock()
                .map_err(|_| "server request registry lock poisoned".to_string())?;
            claim_server_request(&mut registry, request_id)?;
        }
        let response = json!({"jsonrpc":"2.0","id":request_id,"result":result});
        let write_result = self.write_json(&response).await;
        let mut registry = self
            .server_requests
            .lock()
            .map_err(|_| "server request registry lock poisoned".to_string())?;
        finish_server_request(&mut registry, request_id, write_result.is_ok());
        write_result
    }

    fn server_request_method(&self, request_id: u64) -> Result<String, String> {
        self.server_requests
            .lock()
            .map_err(|_| "server request registry lock poisoned".to_string())?
            .get(&request_id)
            .map(|request| request.method.clone())
            .ok_or_else(|| "server request is unknown or already answered".to_string())
    }

    pub async fn stop(self) -> Result<(), String> {
        self.reader_task.abort();
        self.stderr_task.abort();
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
        write_json_to(&self.stdin, value).await
    }
}

async fn write_json_to<T: Serialize>(
    stdin: &Arc<Mutex<ChildStdin>>,
    value: &T,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode Codex request: {error}"))?;
    bytes.push(b'\n');
    let mut stdin = stdin.lock().await;
    stdin
        .write_all(&bytes)
        .await
        .map_err(|error| format!("failed to write Codex request: {error}"))?;
    stdin
        .flush()
        .await
        .map_err(|error| format!("failed to flush Codex request: {error}"))
}

#[allow(clippy::too_many_arguments)]
fn spawn_reader(
    stdout: tokio::process::ChildStdout,
    epoch: RuntimeEpoch,
    pending: PendingRequests,
    events: broadcast::Sender<RecordedEvent>,
    recorder: EventRecorder,
    alive: Arc<AtomicBool>,
    server_requests: Arc<StdMutex<HashMap<u64, PendingServerRequest>>>,
    stdin: Arc<Mutex<ChildStdin>>,
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
                            if let Err(error) = pending.resolve(id, result) {
                                emit_protocol_failure(
                                    &events,
                                    &recorder,
                                    format!("failed to correlate Codex response: {error}"),
                                );
                            }
                        }
                        Ok(IncomingMessage::Notification { method, params }) => {
                            let _ = events
                                .send(recorder.record(EventKind::Notification { method, params }));
                        }
                        Ok(IncomingMessage::ServerRequest { id, method, params }) => {
                            let event = recorder.record(EventKind::ServerRequest {
                                request_id: id.sequence,
                                method: method.clone(),
                                params,
                            });
                            let mut registry = server_requests
                                .lock()
                                .unwrap_or_else(|value| value.into_inner());
                            let now = Instant::now();
                            registry.retain(|_, request| request.expires_at > now);
                            if registry.len() >= MAX_PENDING_SERVER_REQUESTS {
                                let _ = events.send(recorder.record(EventKind::ProtocolFailure {
                                    detail: "Codex server request capacity exceeded".to_string(),
                                }));
                                drop(registry);
                                let rejection_stdin = Arc::clone(&stdin);
                                let rejection_alive = Arc::clone(&alive);
                                tokio::spawn(async move {
                                    let result = server_request_result(&method, false, None)
                                        .unwrap_or_else(|_| json!({"decision":"decline"}));
                                    if write_json_to(
                                        &rejection_stdin,
                                        &json!({"jsonrpc":"2.0","id":id.sequence,"result":result}),
                                    )
                                    .await
                                    .is_err()
                                    {
                                        rejection_alive.store(false, Ordering::Release);
                                    }
                                });
                                continue;
                            }
                            registry.insert(
                                id.sequence,
                                PendingServerRequest {
                                    method,
                                    expires_at: Instant::now() + Duration::from_secs(300),
                                    claimed: false,
                                    event: event.clone(),
                                },
                            );
                            let _ = events.send(event);
                        }
                        Ok(IncomingMessage::Unknown(_)) => {
                            emit_protocol_failure(
                                &events,
                                &recorder,
                                "Codex app-server sent an unrecognized JSON-RPC frame".to_string(),
                            );
                        }
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
        finish_reader(epoch, &pending, &alive, &server_requests);
    })
}

fn finish_reader(
    epoch: RuntimeEpoch,
    pending: &PendingRequests,
    alive: &AtomicBool,
    server_requests: &StdMutex<HashMap<u64, PendingServerRequest>>,
) {
    alive.store(false, Ordering::Release);
    let _ = pending.restart(RuntimeEpoch(epoch.0.saturating_add(1)));
    server_requests
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .clear();
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
