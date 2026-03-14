use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Deserialize;
use std::io::{Read, Write};
use tokio::sync::mpsc;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ShellClientMsg {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
}

pub async fn handle_shell_ws(socket: WebSocket) {
    let (ws_tx, ws_rx) = socket.split();
    if let Err(e) = run_shell(ws_tx, ws_rx).await {
        tracing::warn!("shell session error: {e}");
    }
}

async fn run_shell(
    ws_tx: impl SinkExt<Message, Error = axum::Error> + Send + Unpin + 'static,
    mut ws_rx: impl StreamExt<Item = Result<Message, axum::Error>> + Send + Unpin,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    pair.slave.spawn_command(cmd)?;
    drop(pair.slave); // child keeps slave open; drop our reference

    let master = pair.master;
    let reader = master.try_clone_reader()?;
    let writer = master.take_writer()?;

    let (pty_out_tx, mut pty_out_rx) = mpsc::channel::<String>(256);
    let (pty_in_tx, pty_in_rx) = mpsc::channel::<Vec<u8>>(256);

    // Blocking task: reads PTY output → sends JSON to channel
    let reader_task = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 4096];
        let mut reader = reader;
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let json = serde_json::json!({"type": "output", "data": s}).to_string();
                    if pty_out_tx.blocking_send(json).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Blocking task: drains input channel → writes to PTY stdin
    let writer_task = tokio::task::spawn_blocking(move || {
        let mut writer = writer;
        let mut rx: mpsc::Receiver<Vec<u8>> = pty_in_rx;
        loop {
            match rx.blocking_recv() {
                None => break,
                Some(bytes) => {
                    let _ = writer.write_all(&bytes);
                }
            }
        }
    });

    // Async task: drains pty_out channel → sends to WS client
    use std::sync::Arc;
    use tokio::sync::Mutex;
    let ws_tx = Arc::new(Mutex::new(ws_tx));
    let ws_tx_clone = ws_tx.clone();
    let sender_task = tokio::spawn(async move {
        while let Some(msg) = pty_out_rx.recv().await {
            let mut tx = ws_tx_clone.lock().await;
            if tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // WS receive loop: dispatches input + resize to PTY
    let mut unacked_pings = 0;
    loop {
        match tokio::time::timeout(std::time::Duration::from_secs(30), ws_rx.next()).await {
            Ok(Some(Ok(msg))) => {
                unacked_pings = 0; // Reset on any message received
                match msg {
                    Message::Text(text) => match serde_json::from_str::<ShellClientMsg>(&text) {
                        Ok(ShellClientMsg::Input { data }) => {
                            // Guard against unbounded PTY stdin writes (CWE-400).
                            // 64 KiB is generous for any legitimate terminal input.
                            const MAX_SHELL_INPUT_BYTES: usize = 65_536;
                            if data.len() > MAX_SHELL_INPUT_BYTES {
                                tracing::warn!(
                                    "[shell ws] input message too large ({} bytes), dropping",
                                    data.len()
                                );
                                continue;
                            }
                            let _ = pty_in_tx.send(data.into_bytes()).await;
                        }
                        Ok(ShellClientMsg::Resize { cols, rows }) => {
                            let _ = master.resize(PtySize {
                                rows,
                                cols,
                                pixel_width: 0,
                                pixel_height: 0,
                            });
                        }
                        Err(_) => {}
                    },
                    Message::Close(_) => break,
                    Message::Pong(_) => {} // Keepalive response
                    _ => {}
                }
            }
            Ok(Some(Err(_))) | Ok(None) => break,
            Err(_) => {
                // Timeout elapsed
                if unacked_pings >= 2 {
                    tracing::debug!("Closing shell WS due to keepalive timeout");
                    break;
                }
                let mut tx = ws_tx.lock().await;
                if tx.send(Message::Ping(vec![].into())).await.is_err() {
                    break;
                }
                unacked_pings += 1;
            }
        }
    }

    reader_task.abort();
    writer_task.abort();
    sender_task.abort();
    Ok(())
}
