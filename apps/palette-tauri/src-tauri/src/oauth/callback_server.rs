//! Loopback HTTP listener that captures the OAuth `?code&state` redirect.
//! RFC 8252 §7.3 native-app pattern: bind loopback on an ephemeral port,
//! register that port as a loopback `redirect_uri`, then accept browser requests
//! until one carries the matching state. A non-matching request (favicon, a
//! racing local process with a wrong state) is answered and ignored — only a
//! state-matching code/error ends the loop — so a hostile local request cannot
//! abort a legitimate login.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const MAX_REQUEST_BYTES: usize = 8192;

const SUCCESS_PAGE: &str = "<!doctype html><html><body style=\"font-family:sans-serif;background:#07131c;color:#e6f4fb;\
     text-align:center;padding-top:4rem\"><h2>Signed in to Axon</h2>\
     <p>You can close this tab and return to the palette.</p></body></html>";

const ERROR_PAGE: &str = "<!doctype html><html><body style=\"font-family:sans-serif;background:#07131c;color:#e6f4fb;\
     text-align:center;padding-top:4rem\"><h2>Sign-in failed</h2>\
     <p>Authorization was denied or could not complete. Return to the palette and try again.</p></body></html>";

pub(crate) struct CallbackListener {
    listener: TcpListener,
    pub redirect_uri: String,
}

pub(crate) struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// Bind a loopback listener on an ephemeral port. The `redirect_uri` string is
/// fixed here and must be reused verbatim for `/register`, `/authorize`, and
/// `/token`.
pub(crate) async fn bind() -> Result<CallbackListener, String> {
    let listener = TcpListener::bind(("localhost", 0))
        .await
        .map_err(|err| format!("failed to bind loopback callback listener: {err}"))?;
    let port = listener.local_addr().map_err(|err| err.to_string())?.port();
    // Chrome HTTPS upgrade modes can attempt TLS for IP-literal loopback URLs
    // (`https://127.0.0.1:...`), which fails against this intentionally-plain
    // native-app HTTP listener. `localhost` remains a loopback redirect URI but
    // is treated as a trustworthy local origin by browsers.
    let redirect_uri = format!("http://localhost:{port}/callback");
    Ok(CallbackListener {
        listener,
        redirect_uri,
    })
}

impl CallbackListener {
    /// Accept connections until one carries the OAuth redirect with the matching
    /// `state`, returning the authorization `code`. Times out after `timeout`.
    pub(crate) async fn await_code(
        &self,
        expected_state: &str,
        timeout: Duration,
    ) -> Result<String, String> {
        tokio::time::timeout(timeout, self.accept_loop(expected_state))
            .await
            .map_err(|_| "timed out waiting for the OAuth redirect".to_string())?
    }

    async fn accept_loop(&self, expected_state: &str) -> Result<String, String> {
        loop {
            let (mut socket, _) = self
                .listener
                .accept()
                .await
                .map_err(|err| err.to_string())?;
            let Some(target) = read_request_target(&mut socket).await else {
                respond(&mut socket, "400 Bad Request", "Bad Request").await;
                continue;
            };
            // Only requests to the registered callback path bearing OUR state
            // are the real callback. Anything else (favicon, a racing process
            // with a wrong/absent state) is answered and ignored so it cannot
            // abort the flow.
            let path = target.split('?').next().unwrap_or(&target);
            if path != "/callback" {
                respond(&mut socket, "404 Not Found", "Not Found").await;
                continue;
            }
            let params = parse_callback_params(&target);
            if params.state.as_deref() != Some(expected_state) {
                respond(&mut socket, "404 Not Found", "Not Found").await;
                continue;
            }
            if let Some(error) = params.error {
                respond(&mut socket, "400 Bad Request", ERROR_PAGE).await;
                return Err(format!("authorization was denied ({error})"));
            }
            if let Some(code) = params.code {
                respond(&mut socket, "200 OK", SUCCESS_PAGE).await;
                return Ok(code);
            }
            respond(&mut socket, "400 Bad Request", "Missing code").await;
        }
    }
}

async fn read_request_target(socket: &mut TcpStream) -> Option<String> {
    let mut buf = vec![0u8; MAX_REQUEST_BYTES];
    let n = socket.read(&mut buf).await.ok()?;
    let head = String::from_utf8_lossy(&buf[..n]);
    let request_line = head.lines().next()?;
    parse_request_target(request_line).map(str::to_string)
}

async fn respond(socket: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = socket.write_all(response.as_bytes()).await;
    let _ = socket.flush().await;
    let _ = socket.shutdown().await;
}

/// Extract the request target (path + query) from an HTTP request line.
pub(crate) fn parse_request_target(request_line: &str) -> Option<&str> {
    let mut parts = request_line.split_whitespace();
    let _method = parts.next()?;
    let target = parts.next()?;
    target.starts_with('/').then_some(target)
}

/// Parse `code`/`state`/`error` from a `/callback?...` target.
pub(crate) fn parse_callback_params(target: &str) -> CallbackParams {
    let mut params = CallbackParams {
        code: None,
        state: None,
        error: None,
    };
    if let Ok(url) = url::Url::parse(&format!("http://127.0.0.1{target}")) {
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "code" => params.code = Some(value.into_owned()),
                "state" => params.state = Some(value.into_owned()),
                "error" => params.error = Some(value.into_owned()),
                _ => {}
            }
        }
    }
    params
}

#[cfg(test)]
#[path = "callback_server_tests.rs"]
mod tests;
