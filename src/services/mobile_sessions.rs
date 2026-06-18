use crate::core::paths::{axon_data_base_dir, ensure_private_dir_async};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MobileChatItem {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub payload: serde_json::Value,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MobileSession {
    pub id: String,
    pub title: String,
    pub first_message_preview: String,
    pub turn_count: u32,
    pub injected_op_count: u32,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_at: Option<i64>,
    #[serde(default)]
    pub items: Vec<MobileChatItem>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MobileSessionSummary {
    pub id: String,
    pub title: String,
    pub first_message_preview: String,
    pub turn_count: u32,
    pub injected_op_count: u32,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MobileSessionListResponse {
    pub sessions: Vec<MobileSessionSummary>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MobileSessionDetailResponse {
    pub session: MobileSession,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpsertMobileSessionRequest {
    pub session: MobileSession,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UpsertMobileSessionResponse {
    pub ok: bool,
    pub session: MobileSession,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DeleteMobileSessionResponse {
    pub ok: bool,
}

#[derive(Debug)]
pub enum MobileSessionError {
    InvalidId,
    IdMismatch,
    NotFound,
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl Display for MobileSessionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidId => write!(
                f,
                "session id must be 1-128 ASCII letters, numbers, '-' or '_'"
            ),
            Self::IdMismatch => write!(f, "path session id does not match request body session id"),
            Self::NotFound => write!(f, "mobile session not found"),
            Self::Io(err) => write!(f, "{err}"),
            Self::Json(err) => write!(f, "{err}"),
        }
    }
}

impl Error for MobileSessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Json(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for MobileSessionError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for MobileSessionError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

type Result<T> = std::result::Result<T, MobileSessionError>;

pub async fn list_sessions() -> Result<MobileSessionListResponse> {
    let store = read_store().await?;
    let mut sessions: Vec<_> = store.values().map(summary).collect();
    sort_summaries(&mut sessions);
    Ok(MobileSessionListResponse { sessions })
}

pub async fn get_session(id: &str) -> Result<MobileSessionDetailResponse> {
    validate_id(id)?;
    let store = read_store().await?;
    let session = store.get(id).cloned().ok_or(MobileSessionError::NotFound)?;
    Ok(MobileSessionDetailResponse { session })
}

pub async fn upsert_session(
    path_id: &str,
    request: UpsertMobileSessionRequest,
) -> Result<UpsertMobileSessionResponse> {
    validate_id(path_id)?;
    validate_id(&request.session.id)?;
    if path_id != request.session.id {
        return Err(MobileSessionError::IdMismatch);
    }
    let mut store = read_store().await?;
    store.insert(path_id.to_string(), request.session.clone());
    write_store(&store).await?;
    Ok(UpsertMobileSessionResponse {
        ok: true,
        session: request.session,
    })
}

pub async fn delete_session(id: &str) -> Result<DeleteMobileSessionResponse> {
    validate_id(id)?;
    let mut store = read_store().await?;
    store.remove(id);
    write_store(&store).await?;
    Ok(DeleteMobileSessionResponse { ok: true })
}

fn summary(session: &MobileSession) -> MobileSessionSummary {
    MobileSessionSummary {
        id: session.id.clone(),
        title: session.title.clone(),
        first_message_preview: session.first_message_preview.clone(),
        turn_count: session.turn_count,
        injected_op_count: session.injected_op_count,
        created_at: session.created_at,
        updated_at: session.updated_at,
        pinned_at: session.pinned_at,
    }
}

fn sort_summaries(sessions: &mut [MobileSessionSummary]) {
    sessions.sort_by(|a, b| {
        let a_pin = a.pinned_at.unwrap_or(0);
        let b_pin = b.pinned_at.unwrap_or(0);
        b_pin
            .cmp(&a_pin)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.title.cmp(&b.title))
    });
}

fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(MobileSessionError::InvalidId);
    }
    Ok(())
}

async fn read_store() -> Result<BTreeMap<String, MobileSession>> {
    let path = store_path();
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            if bytes.is_empty() {
                Ok(BTreeMap::new())
            } else {
                Ok(serde_json::from_slice(&bytes)?)
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(err) => Err(err.into()),
    }
}

async fn write_store(store: &BTreeMap<String, MobileSession>) -> Result<()> {
    let dir = store_dir();
    ensure_private_dir_async(dir.clone()).await?;
    let path = dir.join("sessions.json");
    let tmp = dir.join("sessions.json.tmp");
    let bytes = serde_json::to_vec_pretty(store)?;
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(tmp, path).await?;
    Ok(())
}

fn store_dir() -> PathBuf {
    axon_data_base_dir().join("mobile-sessions")
}

fn store_path() -> PathBuf {
    store_dir().join("sessions.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_like_ids() {
        assert!(validate_id("../nope").is_err());
        assert!(validate_id("abc/def").is_err());
        assert!(validate_id("ok-123_ABC").is_ok());
    }

    #[test]
    fn pinned_sessions_sort_first_then_recent() {
        let mut sessions = vec![
            MobileSessionSummary {
                id: "a".into(),
                title: "A".into(),
                first_message_preview: String::new(),
                turn_count: 0,
                injected_op_count: 0,
                created_at: 1,
                updated_at: 10,
                pinned_at: None,
            },
            MobileSessionSummary {
                id: "b".into(),
                title: "B".into(),
                first_message_preview: String::new(),
                turn_count: 0,
                injected_op_count: 0,
                created_at: 1,
                updated_at: 5,
                pinned_at: Some(20),
            },
        ];
        sort_summaries(&mut sessions);
        assert_eq!(sessions[0].id, "b");
    }
}
