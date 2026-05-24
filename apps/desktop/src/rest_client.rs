use std::{fs, path::PathBuf, time::Duration};

use reqwest::blocking::Client;
use serde_json::{Value, json};

use crate::actions::{ArgMode, CommandAction};

#[cfg(test)]
#[path = "rest_client_tests.rs"]
mod tests;

const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8001";
const SERVER_URL_ENV: &str = "AXON_SERVER_URL";
const TOKEN_ENV: &str = "AXON_MCP_HTTP_TOKEN";
const REQUEST_TIMEOUT_SECS: u64 = 300;

pub(crate) struct RestClient {
    base_url: String,
    token: Option<String>,
    client: Client,
}

pub(crate) struct RestRequest {
    pub(crate) method: &'static str,
    pub(crate) path: String,
    pub(crate) body: Option<Value>,
    pub(crate) label: String,
}

pub(crate) struct RestOutput {
    pub(crate) ok: bool,
    pub(crate) status: u16,
    pub(crate) stdout: Option<String>,
    pub(crate) stderr: Option<String>,
}

impl RestClient {
    pub(crate) fn from_env() -> Result<Self, String> {
        let env_entries = read_default_env_entries();
        let base_url = env_value(SERVER_URL_ENV, &env_entries)
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string());
        let client = Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|err| format!("build REST client: {err}"))?;
        let token = env_value(TOKEN_ENV, &env_entries)
            .map(|token| token.trim().to_string())
            .filter(|token| !token.is_empty());
        Ok(Self {
            base_url,
            token,
            client,
        })
    }

    pub(crate) fn execute(&self, request: &RestRequest) -> Result<RestOutput, String> {
        let url = format!("{}{}", self.base_url, request.path);
        let mut builder = match request.method {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            method => return Err(format!("unsupported REST method: {method}")),
        };
        if let Some(token) = &self.token {
            builder = builder.bearer_auth(token);
        }
        if let Some(body) = &request.body {
            builder = builder.json(body);
        }

        let response = builder
            .send()
            .map_err(|err| format!("connect to {url}: {err}"))?;
        let status = response.status();
        let text = response
            .text()
            .map_err(|err| format!("read response from {url}: {err}"))?;
        let pretty = pretty_json_or_text(&text);
        if status.is_success() {
            Ok(RestOutput {
                ok: true,
                status: status.as_u16(),
                stdout: Some(pretty),
                stderr: None,
            })
        } else {
            Ok(RestOutput {
                ok: false,
                status: status.as_u16(),
                stdout: None,
                stderr: Some(pretty),
            })
        }
    }
}

fn env_value(key: &str, file_entries: &[(String, String)]) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            file_entries
                .iter()
                .find(|(entry_key, _)| entry_key == key)
                .map(|(_, value)| value.clone())
        })
}

fn read_default_env_entries() -> Vec<(String, String)> {
    let Some(path) = default_env_path() else {
        return Vec::new();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_env_entries(&contents)
}

fn default_env_path() -> Option<PathBuf> {
    std::env::var_os("AXON_ENV_PATH")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".axon/.env")))
}

fn parse_env_entries(contents: &str) -> Vec<(String, String)> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            Some((key.to_string(), trim_env_value(value)))
        })
        .collect()
}

fn trim_env_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

pub(crate) fn build_rest_request(action: CommandAction, arg: &str) -> Result<RestRequest, String> {
    let words = match action.arg_mode {
        ArgMode::None => Vec::new(),
        ArgMode::OptionalSingle => {
            let arg = arg.trim();
            if arg.is_empty() {
                Vec::new()
            } else {
                vec![arg.to_string()]
            }
        }
        ArgMode::Single => vec![arg.trim().to_string()],
        ArgMode::Split => split_shell_words(arg)?,
    };

    match action.subcommand {
        "doctor" => Ok(get("/v1/doctor", "GET /v1/doctor")),
        "status" => Ok(get("/v1/status", "GET /v1/status")),
        "sources" => Ok(get("/v1/sources?limit=100", "GET /v1/sources")),
        "domains" => Ok(get("/v1/domains?limit=100", "GET /v1/domains")),
        "stats" => Ok(get("/v1/stats", "GET /v1/stats")),
        "scrape" => {
            let url = first_arg(&words, "url")?;
            Ok(post("/v1/scrape", json!({ "url": url }), "POST /v1/scrape"))
        }
        "crawl" => {
            let urls = require_args(&words, "urls")?;
            Ok(post("/v1/crawl", json!({ "urls": urls }), "POST /v1/crawl"))
        }
        "map" => {
            let url = first_arg(&words, "url")?;
            Ok(post(
                "/v1/map",
                json!({ "url": url, "limit": 100 }),
                "POST /v1/map",
            ))
        }
        "summarize" => {
            let urls = require_args(&words, "urls")?;
            Ok(post(
                "/v1/summarize",
                json!({ "urls": urls }),
                "POST /v1/summarize",
            ))
        }
        "ask" => {
            let query = first_arg(&words, "query")?;
            Ok(post(
                "/v1/ask",
                json!({ "query": query, "explain": false, "diagnostics": false }),
                "POST /v1/ask",
            ))
        }
        "query" => {
            let query = first_arg(&words, "query")?;
            Ok(post(
                "/v1/query",
                json!({ "query": query, "limit": 10 }),
                "POST /v1/query",
            ))
        }
        "retrieve" => {
            let url = first_arg(&words, "url")?;
            Ok(post(
                "/v1/retrieve",
                json!({ "url": url, "token_budget": 6000 }),
                "POST /v1/retrieve",
            ))
        }
        "suggest" => {
            let body = words
                .first()
                .map(|focus| json!({ "focus": focus }))
                .unwrap_or_else(|| json!({}));
            Ok(post("/v1/suggest", body, "POST /v1/suggest"))
        }
        "evaluate" => {
            let question = first_arg(&words, "question")?;
            Ok(post(
                "/v1/evaluate",
                json!({ "question": question }),
                "POST /v1/evaluate",
            ))
        }
        "search" => {
            let query = first_arg(&words, "query")?;
            Ok(post(
                "/v1/search",
                json!({ "query": query, "limit": 10 }),
                "POST /v1/search",
            ))
        }
        "research" => {
            let query = first_arg(&words, "query")?;
            Ok(post(
                "/v1/research",
                json!({ "query": query, "limit": 10 }),
                "POST /v1/research",
            ))
        }
        "embed" => {
            let input = first_arg(&words, "input")?;
            Ok(post(
                "/v1/embed",
                json!({ "input": input }),
                "POST /v1/embed",
            ))
        }
        "extract" => {
            let urls = require_args(&words, "urls")?;
            Ok(post(
                "/v1/extract",
                json!({ "urls": urls }),
                "POST /v1/extract",
            ))
        }
        "ingest" => {
            let target = first_arg(&words, "target")?;
            Ok(post("/v1/ingest", ingest_body(&target), "POST /v1/ingest"))
        }
        other => Err(format!("REST route is not wired for action: {other}")),
    }
}

fn get(path: &'static str, label: &'static str) -> RestRequest {
    RestRequest {
        method: "GET",
        path: path.to_string(),
        body: None,
        label: label.to_string(),
    }
}

fn post(path: &'static str, body: Value, label: &'static str) -> RestRequest {
    RestRequest {
        method: "POST",
        path: path.to_string(),
        body: Some(body),
        label: label.to_string(),
    }
}

fn first_arg(words: &[String], field: &'static str) -> Result<String, String> {
    require_args(words, field).map(|args| args[0].clone())
}

fn require_args(words: &[String], field: &'static str) -> Result<Vec<String>, String> {
    let args: Vec<_> = words
        .iter()
        .map(|word| word.trim().to_string())
        .filter(|word| !word.is_empty())
        .collect();
    if args.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(args)
    }
}

fn ingest_body(target: &str) -> Value {
    let lower = target.to_ascii_lowercase();
    if lower.contains("youtube.com/") || lower.contains("youtu.be/") {
        json!({ "source_type": "youtube", "target": target })
    } else if lower.contains("reddit.com/") || lower.starts_with("/r/") || lower.starts_with("r/") {
        json!({ "source_type": "reddit", "target": target })
    } else {
        json!({ "source_type": "github", "target": target, "include_source": true })
    }
}

pub(crate) fn display_rest_request(request: &RestRequest) -> String {
    match &request.body {
        Some(body) => format!("axon rest {} {}", request.label, body),
        None => format!("axon rest {}", request.label),
    }
}

fn pretty_json_or_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    serde_json::from_str::<Value>(trimmed)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| trimmed.to_string())
}

fn split_shell_words(input: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars();
    let mut quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        match ch {
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            '\'' | '"' if quote == Some(ch) => quote = None,
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ch if ch.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            ch => current.push(ch),
        }
    }

    if let Some(quote) = quote {
        return Err(format!("unterminated {quote} quote"));
    }
    if !current.is_empty() {
        words.push(current);
    }
    Ok(words)
}
