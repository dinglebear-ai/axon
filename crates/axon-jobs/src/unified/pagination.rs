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
