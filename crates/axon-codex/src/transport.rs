//! Supervised JSONL transport for the dedicated Codex control process.

use crate::control::ControlConfig;
use crate::protocol::{
    ClientRequest, IncomingMessage, PendingRequests, ProtocolError, RpcError, RuntimeEpoch,
    parse_frame,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle;

const EVENT_CAPACITY: usize = 256;

#[derive(Debug, Clone)]
pub enum ControlEvent {
    Notification {
        method: String,
        params: Value,
    },
    ServerRequest {
        id: u64,
        method: String,
        params: Value,
    },
    ProtocolFailure(String),
    Exited,
}

pub struct ControlTransport {
    epoch: RuntimeEpoch,
    pending: PendingRequests,
    stdin: Arc<Mutex<ChildStdin>>,
    child: Arc<Mutex<Child>>,
    events: broadcast::Sender<ControlEvent>,
    reader_task: JoinHandle<()>,
    timeout: std::time::Duration,
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
            .env("CODEX_HOME", &config.control_home)
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
        let reader_task = spawn_reader(stdout, epoch, pending.clone(), events.clone());
        let transport = Self {
            epoch,
            pending,
            stdin,
            child,
            events,
            reader_task,
            timeout: config.request_timeout,
        };
        transport.initialize().await?;
        Ok(transport)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ControlEvent> {
        self.events.subscribe()
    }

    pub fn epoch(&self) -> RuntimeEpoch {
        self.epoch
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
    events: broadcast::Sender<ControlEvent>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).split(b'\n');
        loop {
            match lines.next_segment().await {
                Ok(Some(line)) if line.is_empty() => continue,
                Ok(Some(line)) => match parse_frame(epoch, &line) {
                    Ok(IncomingMessage::Response { id, result }) => {
                        let _ = pending.resolve(id, result);
                    }
                    Ok(IncomingMessage::Notification { method, params }) => {
                        let _ = events.send(ControlEvent::Notification { method, params });
                    }
                    Ok(IncomingMessage::ServerRequest { id, method, params }) => {
                        let _ = events.send(ControlEvent::ServerRequest {
                            id: id.sequence,
                            method,
                            params,
                        });
                    }
                    Ok(IncomingMessage::Unknown(_)) => {}
                    Err(error) => {
                        let _ = events.send(ControlEvent::ProtocolFailure(error.to_string()));
                    }
                },
                Ok(None) => {
                    let _ = events.send(ControlEvent::Exited);
                    break;
                }
                Err(error) => {
                    let _ = events.send(ControlEvent::ProtocolFailure(error.to_string()));
                    break;
                }
            }
        }
    })
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
