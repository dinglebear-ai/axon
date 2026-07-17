use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct JobCursor {
    pub updated_at: String,
    pub job_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct EventCursor {
    pub sequence: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct WatchCursor {
    pub created_at: i64,
    pub watch_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct WatchHistoryCursor {
    pub created_at: i64,
    pub run_id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

pub fn encode_job_cursor(cursor: &JobCursor) -> String {
    encode(cursor)
}

pub fn decode_job_cursor(cursor: &str) -> Result<JobCursor, String> {
    decode(cursor)
}

pub fn encode_event_cursor(cursor: &EventCursor) -> String {
    encode(cursor)
}

pub fn decode_event_cursor(cursor: &str) -> Result<EventCursor, String> {
    decode(cursor)
}

pub fn encode_watch_cursor(cursor: &WatchCursor) -> String {
    encode(cursor)
}

pub fn decode_watch_cursor(cursor: &str) -> Result<WatchCursor, String> {
    decode(cursor)
}

pub fn encode_watch_history_cursor(cursor: &WatchHistoryCursor) -> String {
    encode(cursor)
}

pub fn decode_watch_history_cursor(cursor: &str) -> Result<WatchHistoryCursor, String> {
    decode(cursor)
}

fn encode<T: serde::Serialize>(cursor: &T) -> String {
    let json = serde_json::to_vec(cursor).expect("cursor serialization is infallible");
    URL_SAFE_NO_PAD.encode(json)
}

fn decode<T: serde::de::DeserializeOwned>(cursor: &str) -> Result<T, String> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|error| format!("invalid cursor encoding: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid cursor payload: {error}"))
}
