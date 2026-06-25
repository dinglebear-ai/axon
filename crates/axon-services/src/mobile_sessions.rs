use axon_core::paths::{axon_data_base_dir, ensure_private_dir_async};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::sync::LazyLock;
use tokio::sync::Mutex;
use utoipa::ToSchema;

static STORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
const STORE_KEY_SEPARATOR: char = '\u{1f}';

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
    InvalidSession(String),
    IdMismatch,
    NotFound,
    StaleUpdate,
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
            Self::InvalidSession(message) => write!(f, "invalid mobile session: {message}"),
            Self::IdMismatch => write!(f, "path session id does not match request body session id"),
            Self::NotFound => write!(f, "mobile session not found"),
            Self::StaleUpdate => write!(f, "mobile session update is older than stored state"),
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

pub async fn list_sessions(owner: &str) -> Result<MobileSessionListResponse> {
    let _guard = STORE_LOCK.lock().await;
    let mut store = read_store().await?;
    if migrate_legacy_entries(&mut store, owner) {
        write_store(&store).await?;
    }
    let mut sessions: Vec<_> = store
        .iter()
        .filter(|(key, _session)| store_key_owner_matches(key, owner))
        .map(|(_key, session)| summary(session))
        .collect();
    sort_summaries(&mut sessions);
    Ok(MobileSessionListResponse { sessions })
}

pub async fn get_session(owner: &str, id: &str) -> Result<MobileSessionDetailResponse> {
    validate_id(id)?;
    let _guard = STORE_LOCK.lock().await;
    let mut store = read_store().await?;
    if migrate_legacy_entries(&mut store, owner) {
        write_store(&store).await?;
    }
    let session = store
        .get(&store_key(owner, id))
        .cloned()
        .ok_or(MobileSessionError::NotFound)?;
    Ok(MobileSessionDetailResponse { session })
}

pub async fn upsert_session(
    owner: &str,
    path_id: &str,
    request: UpsertMobileSessionRequest,
) -> Result<UpsertMobileSessionResponse> {
    validate_id(path_id)?;
    validate_id(&request.session.id)?;
    if path_id != request.session.id {
        return Err(MobileSessionError::IdMismatch);
    }
    let _guard = STORE_LOCK.lock().await;
    let mut store = read_store().await?;
    migrate_legacy_entries(&mut store, owner);
    let session = request.session.clone();
    upsert_into_store(&mut store, owner, path_id, request.session)?;
    write_store(&store).await?;
    Ok(UpsertMobileSessionResponse { ok: true, session })
}

pub async fn delete_session(owner: &str, id: &str) -> Result<DeleteMobileSessionResponse> {
    validate_id(id)?;
    let _guard = STORE_LOCK.lock().await;
    let mut store = read_store().await?;
    migrate_legacy_entries(&mut store, owner);
    store.remove(&store_key(owner, id));
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

fn upsert_into_store(
    store: &mut BTreeMap<String, MobileSession>,
    owner: &str,
    path_id: &str,
    session: MobileSession,
) -> Result<()> {
    validate_session(&session)?;
    let key = store_key(owner, path_id);
    if store
        .get(&key)
        .is_some_and(|existing| existing.updated_at > session.updated_at)
    {
        return Err(MobileSessionError::StaleUpdate);
    }
    store.insert(key, session);
    Ok(())
}

fn validate_session(session: &MobileSession) -> Result<()> {
    if session.title.len() > 256 {
        return Err(MobileSessionError::InvalidSession(
            "title must be at most 256 bytes".to_string(),
        ));
    }
    if session.first_message_preview.len() > 512 {
        return Err(MobileSessionError::InvalidSession(
            "first message preview must be at most 512 bytes".to_string(),
        ));
    }
    if session.created_at < 0 || session.updated_at < 0 {
        return Err(MobileSessionError::InvalidSession(
            "timestamps must be non-negative".to_string(),
        ));
    }
    if session.updated_at < session.created_at {
        return Err(MobileSessionError::InvalidSession(
            "updated_at must be greater than or equal to created_at".to_string(),
        ));
    }
    if session.pinned_at.is_some_and(|timestamp| timestamp < 0) {
        return Err(MobileSessionError::InvalidSession(
            "pinned_at must be non-negative".to_string(),
        ));
    }

    let turn_count = usize::try_from(session.turn_count)
        .map_err(|_| MobileSessionError::InvalidSession("turn_count is too large".to_string()))?;
    let injected_op_count = usize::try_from(session.injected_op_count).map_err(|_| {
        MobileSessionError::InvalidSession("injected_op_count is too large".to_string())
    })?;
    let actual_turn_count = session
        .items
        .iter()
        .filter(|item| item.kind == "user")
        .count();
    let actual_injected_op_count = session
        .items
        .iter()
        .filter(|item| item.kind == "injection" || item.kind == "action_result")
        .count();
    if turn_count != actual_turn_count {
        return Err(MobileSessionError::InvalidSession(format!(
            "turn_count {turn_count} does not match {actual_turn_count} user items"
        )));
    }
    if injected_op_count != actual_injected_op_count {
        return Err(MobileSessionError::InvalidSession(format!(
            "injected_op_count {injected_op_count} does not match {actual_injected_op_count} operation items"
        )));
    }
    Ok(())
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

fn store_key(owner: &str, id: &str) -> String {
    format!("{owner}{STORE_KEY_SEPARATOR}{id}")
}

fn store_key_owner_matches(key: &str, owner: &str) -> bool {
    key.strip_prefix(owner)
        .is_some_and(|rest| rest.starts_with(STORE_KEY_SEPARATOR))
}

fn migrate_legacy_entries(store: &mut BTreeMap<String, MobileSession>, owner: &str) -> bool {
    let legacy_keys: Vec<String> = store
        .iter()
        .filter(|(key, session)| {
            !key.contains(STORE_KEY_SEPARATOR) && key.as_str() == session.id.as_str()
        })
        .map(|(key, _session)| key.clone())
        .collect();
    let mut migrated = false;
    for key in legacy_keys {
        let Some(session) = store.remove(&key) else {
            continue;
        };
        store
            .entry(store_key(owner, &session.id))
            .or_insert(session);
        migrated = true;
    }
    migrated
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

    #[test]
    fn sessions_are_owner_scoped() {
        let mut store = BTreeMap::new();

        upsert_into_store(&mut store, "owner-a", "shared", test_session("shared", 100))
            .expect("owner a insert");

        upsert_into_store(&mut store, "owner-b", "shared", test_session("shared", 200))
            .expect("owner b insert");

        assert_eq!(
            store
                .get(&store_key("owner-b", "shared"))
                .unwrap()
                .updated_at,
            200
        );
        assert!(store_key_owner_matches(
            &store_key("owner-a", "shared"),
            "owner-a"
        ));
        assert!(!store_key_owner_matches(
            &store_key("owner-a", "shared"),
            "owner-c"
        ));
    }

    #[test]
    fn rejects_stale_updates() {
        let mut store = BTreeMap::new();

        upsert_into_store(&mut store, "owner", "shared", test_session("shared", 200))
            .expect("initial insert");

        let stale = upsert_into_store(&mut store, "owner", "shared", test_session("shared", 150));
        assert!(matches!(stale, Err(MobileSessionError::StaleUpdate)));
        assert_eq!(
            store.get(&store_key("owner", "shared")).unwrap().updated_at,
            200
        );
    }

    #[test]
    fn rejects_inconsistent_denormalized_counts() {
        let mut store = BTreeMap::new();
        let mut session = test_session("shared", 200);
        session.turn_count = 1;

        let result = upsert_into_store(&mut store, "owner", "shared", session);

        assert!(matches!(result, Err(MobileSessionError::InvalidSession(_))));
    }

    #[test]
    fn migrates_legacy_unscoped_sessions_to_current_owner() {
        let mut store = BTreeMap::new();
        store.insert("shared".to_string(), test_session("shared", 200));

        assert!(migrate_legacy_entries(&mut store, "owner"));

        assert!(!store.contains_key("shared"));
        assert!(store.contains_key(&store_key("owner", "shared")));
    }

    #[test]
    fn legacy_migration_does_not_overwrite_owner_session() {
        let mut store = BTreeMap::new();
        store.insert("shared".to_string(), test_session("shared", 100));
        store.insert(store_key("owner", "shared"), test_session("shared", 200));

        assert!(migrate_legacy_entries(&mut store, "owner"));

        assert_eq!(
            store.get(&store_key("owner", "shared")).unwrap().updated_at,
            200
        );
    }

    fn test_session(id: &str, updated_at: i64) -> MobileSession {
        MobileSession {
            id: id.to_string(),
            title: "Test".into(),
            first_message_preview: String::new(),
            turn_count: 0,
            injected_op_count: 0,
            created_at: 1,
            updated_at,
            pinned_at: None,
            items: Vec::new(),
        }
    }
}
